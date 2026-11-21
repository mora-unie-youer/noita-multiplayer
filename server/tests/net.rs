//! Fake clients against a real server over loopback TCP.

mod common;

use std::collections::HashMap;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::thread;
use std::time::Duration;

use common::*;
use noita_coop_agent::{Link, LinkError, LinkEvent};
use noita_coop_protocol::frame::{read_message, write_message};
use noita_coop_protocol::{
    ClientMsg, CloseReason, CommandBody, CommandResult, EventBody, ObjectKind, RejectReason,
    ServerMsg, Welcome,
};
use noita_coop_server::engine::Engine;
use noita_coop_server::net::{ServeOptions, Server};

const WAIT: Duration = Duration::from_secs(10);

fn start(engine: Engine, addr: &str, backup_dir: Option<&Path>) -> Server {
    let listener = TcpListener::bind(addr).unwrap();
    Server::start(
        engine,
        listener,
        ServeOptions {
            backup_dir: backup_dir.map(Path::to_owned),
            backup_interval: Duration::from_secs(3600),
        },
    )
    .unwrap()
}

fn connect(addr: SocketAddr, token: &str) -> (Link, Welcome) {
    Link::connect(addr, hello(token)).unwrap()
}

/// Receive until `pred` accepts a server message; panics on silence or disconnect.
fn expect(link: &mut Link, mut pred: impl FnMut(&ServerMsg) -> bool) -> ServerMsg {
    loop {
        match link.recv(WAIT) {
            Some(LinkEvent::Server(msg)) if pred(&msg) => return *msg,
            Some(LinkEvent::Server(_)) => {}
            Some(LinkEvent::Disconnected) => panic!("disconnected while waiting"),
            None => panic!("timed out"),
        }
    }
}

fn result_of(link: &mut Link, seq: u64) -> CommandResult {
    match expect(
        link,
        |m| matches!(m, ServerMsg::CommandResult { seq: s, .. } if *s == seq),
    ) {
        ServerMsg::CommandResult { result, .. } => result,
        _ => unreachable!(),
    }
}

fn journal(link: &mut Link) -> Vec<noita_coop_protocol::JournalEvent> {
    link.request_journal(0, 1000);
    match expect(link, |m| matches!(m, ServerMsg::Journal { .. })) {
        ServerMsg::Journal { events, .. } => events,
        _ => unreachable!(),
    }
}

#[test]
fn result_comes_back_and_everyone_hears_about_the_event_and_their_new_balance() {
    let (_dir, _path, engine, tokens) = create_run();
    let server = start(engine, "127.0.0.1:0", None);
    let (mut a, wa) = connect(server.local_addr(), &tokens[0].token);
    let (mut b, wb) = connect(server.local_addr(), &tokens[1].token);
    assert_eq!((wa.slot, wb.slot), (0, 1));
    assert_eq!(wa.run.seed, SEED);

    a.submit(CommandBody::RegisterObject {
        object_id: 1,
        kind: nugget(10),
    });
    let seq = a.submit(CommandBody::ClaimPickup { object_id: 1 });
    assert!(matches!(
        result_of(&mut a, seq),
        CommandResult::Applied { event_seq: Some(2) }
    ));

    let seen = expect(&mut b, |m| matches!(m, ServerMsg::Wallet { .. }));
    assert_eq!(
        seen,
        ServerMsg::Wallet {
            balance: 10,
            as_of_seq: 2
        }
    );
    expect(
        &mut b,
        |m| matches!(m, ServerMsg::Event(e) if matches!(e.body, EventBody::GoldIncome { base_amount: 10, claimed_by: 0, .. })),
    );
    expect(&mut a, |m| {
        matches!(m, ServerMsg::Wallet { balance: 10, .. })
    });
    server.shutdown().unwrap();
}

#[test]
fn many_simultaneous_claims_have_exactly_one_winner_each() {
    const OBJECTS: u64 = 60;
    let (_dir, _path, engine, tokens) = create_run();
    let server = start(engine, "127.0.0.1:0", None);
    let addr = server.local_addr();
    let (mut reg, _) = connect(addr, &tokens[0].token);
    for id in 1..=OBJECTS {
        reg.submit(CommandBody::RegisterObject {
            object_id: id,
            kind: nugget(7),
        });
    }
    while reg.unacked() > 0 {
        expect(&mut reg, |m| matches!(m, ServerMsg::CommandResult { .. }));
    }

    // Both players race for every object, from their own threads.
    let race = |token: String, reverse: bool| {
        thread::spawn(move || {
            let (mut link, welcome) = Link::connect(addr, hello(&token)).unwrap();
            let mut order: Vec<u64> = (1..=OBJECTS).collect();
            if reverse {
                order.reverse();
            }
            let mut seqs = HashMap::new();
            for id in order {
                seqs.insert(link.submit(CommandBody::ClaimPickup { object_id: id }), id);
            }
            let mut won = Vec::new();
            while link.unacked() > 0 {
                if let Some(LinkEvent::Server(msg)) = link.recv(WAIT)
                    && let ServerMsg::CommandResult { seq, result } = *msg
                    && matches!(result, CommandResult::Applied { .. })
                {
                    won.push(seqs[&seq]);
                }
            }
            (welcome.slot, won)
        })
    };
    let ta = race(tokens[0].token.clone(), false);
    let tb = race(tokens[1].token.clone(), true);
    let (_, mut won_a) = ta.join().unwrap();
    let (_, won_b) = tb.join().unwrap();

    assert_eq!(
        won_a.len() + won_b.len(),
        OBJECTS as usize,
        "every object claimed once"
    );
    won_a.extend(won_b);
    won_a.sort_unstable();
    assert_eq!(
        won_a,
        (1..=OBJECTS).collect::<Vec<_>>(),
        "and by exactly one player"
    );

    let (_, w) = connect(addr, &tokens[1].token);
    assert_eq!(w.wallet, 7 * OBJECTS as i64, "income paid once per object");
    server.shutdown().unwrap();
}

#[test]
fn command_whose_connection_died_before_the_answer_is_applied_exactly_once() {
    let (_dir, _path, engine, tokens) = create_run();
    let server = start(engine, "127.0.0.1:0", None);
    let addr = server.local_addr();
    let (mut a, _) = connect(addr, &tokens[0].token);

    for round in 1..=15u64 {
        a.submit(CommandBody::RegisterObject {
            object_id: round,
            kind: nugget(3),
        });
        a.submit(CommandBody::ClaimPickup { object_id: round });
        // Cut the line without reading any answer: the server may or may not have applied
        // these before noticing. Either way the outcome must be the same.
        a.disconnect();
        a.reconnect().unwrap();
        // Whether or not the first copy reached the server, the client must hear "applied"
        // for both commands, never a rejection caused by the resend itself.
        let mut answered = 0;
        while answered < 2 {
            if let ServerMsg::CommandResult { result, .. } =
                expect(&mut a, |m| matches!(m, ServerMsg::CommandResult { .. }))
            {
                assert!(
                    matches!(result, CommandResult::Applied { .. }),
                    "{result:?}"
                );
                answered += 1;
            }
        }
    }

    let events = journal(&mut a);
    let registered = events
        .iter()
        .filter(|e| matches!(e.body, EventBody::ObjectRegistered { .. }))
        .count();
    let income = events
        .iter()
        .filter(|e| matches!(e.body, EventBody::GoldIncome { .. }))
        .count();
    assert_eq!((registered, income), (15, 15));
    let (_, w) = connect(addr, &tokens[1].token);
    assert_eq!(w.wallet, 45);
    server.shutdown().unwrap();
}

#[test]
fn commands_issued_while_offline_are_queued_and_sent_after_the_reconnect() {
    let (_dir, _path, engine, tokens) = create_run();
    let server = start(engine, "127.0.0.1:0", None);
    let (mut a, _) = connect(server.local_addr(), &tokens[0].token);
    a.disconnect();
    assert!(!a.is_connected());
    a.submit(CommandBody::RegisterObject {
        object_id: 1,
        kind: ObjectKind::Heart,
    });
    let seq = a.submit(CommandBody::ClaimPickup { object_id: 1 });
    assert_eq!(a.unacked(), 2);

    a.reconnect().unwrap();
    assert!(matches!(
        result_of(&mut a, seq),
        CommandResult::Applied { .. }
    ));
    server.shutdown().unwrap();
}

#[test]
fn newest_connection_of_a_slot_replaces_a_stale_one() {
    let (_dir, _path, engine, tokens) = create_run();
    let server = start(engine, "127.0.0.1:0", None);
    let (mut old, _) = connect(server.local_addr(), &tokens[0].token);
    let (mut new, _) = connect(server.local_addr(), &tokens[0].token);

    expect(&mut old, |m| {
        matches!(m, ServerMsg::Closed(CloseReason::Superseded))
    });
    new.submit(CommandBody::RegisterObject {
        object_id: 1,
        kind: ObjectKind::Heart,
    });
    expect(&mut new, |m| matches!(m, ServerMsg::CommandResult { .. }));
    server.shutdown().unwrap();
}

#[test]
fn refused_handshakes_and_protocol_violations_are_reported() {
    let (_dir, _path, engine, tokens) = create_run();
    let server = start(engine, "127.0.0.1:0", None);
    let addr = server.local_addr();

    let mut bad = hello("not-a-token");
    assert!(matches!(
        Link::connect(addr, bad.clone()),
        Err(LinkError::Rejected(RejectReason::BadToken))
    ));
    bad = hello(&tokens[0].token);
    bad.game_hash = "other-build".into();
    assert!(matches!(
        Link::connect(addr, bad),
        Err(LinkError::Rejected(RejectReason::GameHash { .. }))
    ));

    // A command before Hello.
    let mut raw = TcpStream::connect(addr).unwrap();
    raw.set_read_timeout(Some(WAIT)).unwrap();
    write_message(&mut raw, &ClientMsg::Ping(1)).unwrap();
    let answer: ServerMsg = read_message(&mut raw).unwrap();
    assert_eq!(answer, ServerMsg::Closed(CloseReason::ProtocolViolation));

    // Second Hello on an established connection.
    let mut raw = TcpStream::connect(addr).unwrap();
    raw.set_read_timeout(Some(WAIT)).unwrap();
    write_message(&mut raw, &ClientMsg::Hello(hello(&tokens[1].token))).unwrap();
    assert!(matches!(
        read_message::<_, ServerMsg>(&mut raw).unwrap(),
        ServerMsg::Welcome(_)
    ));
    write_message(&mut raw, &ClientMsg::Hello(hello(&tokens[1].token))).unwrap();
    assert_eq!(
        read_message::<_, ServerMsg>(&mut raw).unwrap(),
        ServerMsg::Closed(CloseReason::ProtocolViolation)
    );
    server.shutdown().unwrap();
}

#[test]
fn server_restart_loses_nothing_and_the_client_continues_where_it_was() {
    let (_dir, path, engine, tokens) = create_run();
    let server = start(engine, "127.0.0.1:0", None);
    let addr = server.local_addr();
    let (mut a, _) = connect(addr, &tokens[0].token);
    a.submit(CommandBody::RegisterObject {
        object_id: 1,
        kind: nugget(10),
    });
    let seq = a.submit(CommandBody::ClaimPickup { object_id: 1 });
    result_of(&mut a, seq);
    server.shutdown().unwrap();
    expect_disconnect(&mut a);

    let server = start(Engine::resume(&path).unwrap(), &addr.to_string(), None);
    let welcome = a.reconnect().unwrap();
    assert_eq!(
        (welcome.wallet, welcome.last_command_seq, welcome.head_seq),
        (10, 2, 2)
    );
    let seq = a.submit(CommandBody::Spend { amount: 4 });
    assert_eq!(seq, 3);
    assert!(matches!(
        result_of(&mut a, seq),
        CommandResult::Applied { .. }
    ));
    assert_eq!(connect(addr, &tokens[0].token).1.wallet, 6);
    server.shutdown().unwrap();
}

fn expect_disconnect(link: &mut Link) {
    loop {
        match link.recv(WAIT) {
            Some(LinkEvent::Disconnected) => return,
            Some(_) => {}
            None => panic!("connection was not closed"),
        }
    }
}

#[test]
fn graceful_shutdown_writes_a_final_backup_and_tells_clients() {
    let (dir, _path, engine, tokens) = create_run();
    let backups = dir.path().join("backups");
    let server = start(engine, "127.0.0.1:0", Some(&backups));
    let (mut a, _) = connect(server.local_addr(), &tokens[0].token);
    let seq = a.submit(CommandBody::RegisterObject {
        object_id: 1,
        kind: ObjectKind::Heart,
    });
    result_of(&mut a, seq);

    server.shutdown().unwrap();
    expect(&mut a, |m| {
        matches!(m, ServerMsg::Closed(CloseReason::ServerShutdown))
    });
    let files: Vec<_> = std::fs::read_dir(&backups).unwrap().collect();
    assert_eq!(files.len(), 1, "one backup of the changed state");
}
