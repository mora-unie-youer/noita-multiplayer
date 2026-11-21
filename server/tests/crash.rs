//! Stage-2 result: the server survives `kill -9` at any moment without losing confirmed events.
//!
//! The real server binary runs as a child process under load and is killed with SIGKILL several
//! times. After every kill the database is reopened in-process (which also runs the integrity
//! and journal-consistency checks) and compared with what the client had been told.

mod common;

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::net::{SocketAddr, TcpListener};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

use common::*;
use noita_coop_agent::{Link, LinkEvent};
use noita_coop_protocol::{CommandBody, CommandResult, EventBody, ServerMsg};
use noita_coop_server::engine::Engine;
use rusqlite::Connection;

const NUGGET: i64 = 5;

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn spawn_server(db: &Path, backups: &Path, port: u16) -> Child {
    let mut child = Command::new(env!("CARGO_BIN_EXE_noita-coop-server"))
        .arg("serve")
        .arg("--db")
        .arg(db)
        .arg("--listen")
        .arg(format!("127.0.0.1:{port}"))
        .arg("--backup-dir")
        .arg(backups)
        .args(["--backup-interval-secs", "1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.as_mut().unwrap())
        .read_line(&mut line)
        .unwrap();
    assert!(line.starts_with("listening "), "server said {line:?}");
    child
}

/// Pipeline register+claim pairs until the connection dies. Returns the link and every result
/// the client was told, by command number.
fn hammer(mut link: Link, first_object: u64) -> (Link, HashMap<u64, CommandResult>, u64) {
    let mut told = HashMap::new();
    let mut object = first_object;
    loop {
        while link.unacked() < 40 && link.is_connected() {
            link.submit(CommandBody::RegisterObject {
                object_id: object,
                kind: nugget(NUGGET),
            });
            link.submit(CommandBody::ClaimPickup { object_id: object });
            object += 1;
        }
        match link.recv(Duration::from_millis(20)) {
            Some(LinkEvent::Server(msg)) => {
                if let ServerMsg::CommandResult { seq, result } = *msg {
                    told.insert(seq, result);
                }
            }
            Some(LinkEvent::Disconnected) => return (link, told, object),
            None => {}
        }
    }
}

fn stored_results(db: &Path) -> HashMap<u64, CommandResult> {
    let conn = Connection::open(db).unwrap();
    let mut stmt = conn
        .prepare("SELECT seq, result FROM commands WHERE slot = 0")
        .unwrap();
    stmt.query_map([], |r| {
        Ok((r.get::<_, i64>(0)? as u64, r.get::<_, Vec<u8>>(1)?))
    })
    .unwrap()
    .map(|row| {
        let (seq, blob) = row.unwrap();
        (seq, postcard::from_bytes(&blob).unwrap())
    })
    .collect()
}

#[test]
fn kill_dash_nine_under_load_loses_no_confirmed_event() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("run.sqlite");
    let backups = dir.path().join("backups");
    let (engine, tokens) = Engine::create(&db, &new_run()).unwrap();
    drop(engine);

    let port = free_port();
    let addr: SocketAddr = ([127, 0, 0, 1], port).into();
    let mut child = spawn_server(&db, &backups, port);
    let (mut link, _) = Link::connect(addr, hello(&tokens[0].token)).unwrap();

    let delays_ms = [120, 330, 60, 1300, 210, 700];
    let mut everything_told: HashMap<u64, CommandResult> = HashMap::new();
    let mut next_object = 1;

    for (round, delay) in delays_ms.into_iter().enumerate() {
        let worker = {
            let first = next_object;
            thread::spawn(move || hammer(link, first))
        };
        thread::sleep(Duration::from_millis(delay));
        child.kill().unwrap(); // SIGKILL: no shutdown path runs
        child.wait().unwrap();
        let (l, told, object) = worker.join().unwrap();
        link = l;
        next_object = object;
        assert!(
            !told.is_empty(),
            "round {round}: nothing was confirmed before the kill"
        );

        // Reopen: runs quick_check and the journal-vs-wallet consistency check.
        let engine = Engine::resume(&db)
            .unwrap_or_else(|e| panic!("round {round}: run does not reopen after kill -9: {e}"));
        drop(engine);

        // Every result the client was told is durable and identical on disk.
        let stored = stored_results(&db);
        for (seq, result) in &told {
            assert_eq!(
                stored.get(seq),
                Some(result),
                "round {round}: command {seq} was confirmed but is not on disk"
            );
        }
        everything_told.extend(told);

        // Backups written under fire are complete and consistent.
        if let Some(newest) = newest_backup(&backups) {
            let copy = dir.path().join(format!("restored-{round}.sqlite"));
            std::fs::copy(&newest, &copy).unwrap();
            Engine::resume(&copy).unwrap_or_else(|e| {
                panic!("round {round}: backup {} unusable: {e}", newest.display())
            });
        }

        child = spawn_server(&db, &backups, port);
        // Reconnect resends everything the dead server never answered, under the same numbers.
        link.reconnect()
            .unwrap_or_else(|e| panic!("round {round}: reconnect failed: {e}"));
    }

    // Final round without a kill: let the queue drain completely.
    while link.unacked() > 0 {
        match link.recv(Duration::from_secs(10)) {
            Some(LinkEvent::Server(msg)) => {
                if let ServerMsg::CommandResult { seq, result } = *msg {
                    everything_told.insert(seq, result);
                }
            }
            other => panic!("queue did not drain: {other:?}"),
        }
    }
    child.kill().unwrap();
    child.wait().unwrap();

    // Despite six kills and resends: every command applied once, none rejected.
    let rejected: Vec<_> = everything_told
        .iter()
        .filter(|(_, r)| !matches!(r, CommandResult::Applied { .. }))
        .collect();
    assert!(
        rejected.is_empty(),
        "resends must not be refused: {rejected:?}"
    );

    let engine = Engine::resume(&db).unwrap();
    let mut registered = 0;
    let mut claimed = 0;
    let mut after = 0;
    loop {
        let (page, _) = engine.journal_after(after, 1000).unwrap();
        let Some(last) = page.last() else { break };
        after = last.seq;
        for ev in page {
            match ev.body {
                EventBody::ObjectRegistered { .. } => registered += 1,
                EventBody::GoldIncome { .. } => claimed += 1,
                other => panic!("unexpected event {other:?}"),
            }
        }
    }
    assert_eq!(
        registered, claimed,
        "every registered nugget claimed exactly once"
    );
    assert_eq!(engine.welcome(0).unwrap().wallet, NUGGET * claimed);
    assert_eq!(engine.welcome(1).unwrap().wallet, NUGGET * claimed);
    assert!(
        claimed > 20,
        "the test must have moved real traffic ({claimed})"
    );
}

fn newest_backup(dir: &Path) -> Option<std::path::PathBuf> {
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "sqlite"))
        .collect();
    files.sort();
    files.pop()
}
