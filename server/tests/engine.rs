//! Behaviour of the run state machine without any network.

mod common;

use common::*;
use noita_coop_protocol::{
    CommandBody, CommandResult, EventBody, ObjectKind, PROTOCOL_VERSION, RejectReason, Rejection,
};
use noita_coop_server::backup;
use noita_coop_server::engine::{Auth, Engine};
use noita_coop_server::error::{CreateError, OpenError};
use noita_coop_server::storage::Store;
use rusqlite::Connection;

fn rejected(a: &noita_coop_server::engine::Applied) -> &Rejection {
    match &a.result {
        CommandResult::Rejected(r) => r,
        other => panic!("expected a rejection, got {other:?}"),
    }
}

// --- handshake -------------------------------------------------------------------------------

#[test]
fn handshake_checks_version_token_build_seed_and_mods() {
    let (_dir, _path, engine, tokens) = create_run();
    let good = hello(&tokens[0].token);

    let reason = |h| match engine.authenticate(&h).unwrap() {
        Auth::Rejected(r) => r,
        Auth::Accepted(s) => panic!("accepted slot {s}"),
    };

    assert!(matches!(
        engine.authenticate(&good).unwrap(),
        Auth::Accepted(0)
    ));
    assert!(matches!(
        engine.authenticate(&hello(&tokens[1].token)).unwrap(),
        Auth::Accepted(1)
    ));

    let mut h = good.clone();
    h.protocol_version = PROTOCOL_VERSION + 1;
    assert_eq!(
        reason(h),
        RejectReason::ProtocolVersion {
            server: PROTOCOL_VERSION
        }
    );

    assert_eq!(reason(hello("00")), RejectReason::BadToken);

    let mut h = good.clone();
    h.game_hash = "other".into();
    assert_eq!(
        reason(h),
        RejectReason::GameHash {
            expected: GAME_HASH.into()
        }
    );

    let mut h = good.clone();
    h.seed += 1;
    assert_eq!(reason(h), RejectReason::SeedMismatch);

    let mut h = good.clone();
    h.mods = vec![];
    assert!(matches!(reason(h), RejectReason::ModsMismatch { .. }));

    // Mod order is irrelevant.
    let mut h = good;
    h.mods = vec!["noita_cartographer".into()];
    assert!(matches!(
        engine.authenticate(&h).unwrap(),
        Auth::Accepted(0)
    ));
}

// --- gold: D4 -------------------------------------------------------------------------------

#[test]
fn shared_income_and_individual_spending_follow_the_documented_example() {
    // SYNC_CONTRACTS §5: A and B at 0; A picks up 10 → 10/10; A spends 6 → 4/10;
    // B picks up 20 → 24/30.
    let (_dir, _path, engine, _t) = create_run();
    let mut d = Driver::new(engine);
    d.register(0, 1, nugget(10));
    d.register(1, 2, nugget(20));

    let a = d.claim(0, 1);
    assert_eq!(a.wallets, vec![(0, 10), (1, 10)]);
    assert_eq!((d.wallet(0), d.wallet(1)), (10, 10));

    d.run(0, CommandBody::Spend { amount: 6 });
    assert_eq!((d.wallet(0), d.wallet(1)), (4, 10));

    d.claim(1, 2);
    assert_eq!((d.wallet(0), d.wallet(1)), (24, 30));
    assert_consistent(&d.engine);
}

#[test]
fn second_claim_of_one_nugget_pays_nobody_and_names_the_winner() {
    let (_dir, _path, engine, _t) = create_run();
    let mut d = Driver::new(engine);
    d.register(0, 7, nugget(50));

    let first = d.claim(1, 7);
    assert!(matches!(
        first.result,
        CommandResult::Applied { event_seq: Some(_) }
    ));
    let second = d.claim(0, 7);
    assert_eq!(rejected(&second), &Rejection::AlreadyTaken { by: 1 });
    assert!(second.event.is_none() && second.wallets.is_empty());

    assert_eq!((d.wallet(0), d.wallet(1)), (50, 50)); // once, not twice
    let (events, _) = d.engine.journal_after(0, 100).unwrap();
    let incomes = events
        .iter()
        .filter(|e| matches!(e.body, EventBody::GoldIncome { .. }))
        .count();
    assert_eq!(incomes, 1);
    assert_consistent(&d.engine);
}

#[test]
fn offline_player_still_receives_income() {
    // Nobody ever connected: the wallet lives in the database, not in a session.
    let (_dir, _path, engine, _t) = create_run();
    let mut d = Driver::new(engine);
    d.register(0, 1, nugget(33));
    d.claim(0, 1);
    assert_eq!(d.wallet(1), 33);
}

#[test]
fn spend_is_checked_and_can_never_create_income() {
    let (_dir, _path, engine, _t) = create_run();
    let mut d = Driver::new(engine);
    d.register(0, 1, nugget(10));
    d.claim(0, 1);

    let a = d.run(0, CommandBody::Spend { amount: 11 });
    assert_eq!(rejected(&a), &Rejection::InsufficientFunds { balance: 10 });
    let a = d.run(0, CommandBody::Spend { amount: -5 });
    assert_eq!(rejected(&a), &Rejection::InvalidAmount);
    let a = d.run(0, CommandBody::Spend { amount: 0 });
    assert_eq!(rejected(&a), &Rejection::InvalidAmount);

    assert_eq!((d.wallet(0), d.wallet(1)), (10, 10));
    let a = d.run(0, CommandBody::Spend { amount: 10 });
    assert_eq!(a.wallets, vec![(0, 0)]);
    assert_eq!((d.wallet(0), d.wallet(1)), (0, 10));
    assert_consistent(&d.engine);
}

#[test]
fn invalid_nuggets_and_double_registration_are_refused() {
    let (_dir, _path, engine, _t) = create_run();
    let mut d = Driver::new(engine);
    for bad in [0, -1, noita_coop_protocol::MAX_NUGGET_AMOUNT + 1] {
        let a = d.run(
            0,
            CommandBody::RegisterObject {
                object_id: 1,
                kind: nugget(bad),
            },
        );
        assert_eq!(rejected(&a), &Rejection::InvalidAmount, "amount {bad}");
    }
    d.register(0, 1, nugget(5));
    let a = d.run(
        1,
        CommandBody::RegisterObject {
            object_id: 1,
            kind: nugget(999),
        },
    );
    assert_eq!(rejected(&a), &Rejection::ObjectAlreadyRegistered);
    let a = d.claim(0, 404);
    assert_eq!(rejected(&a), &Rejection::UnknownObject);
}

#[test]
fn orbs_and_hearts_are_claimed_once_without_touching_wallets() {
    let (_dir, _path, engine, _t) = create_run();
    let mut d = Driver::new(engine);
    d.register(0, 1, ObjectKind::Orb { orb_id: 11 });
    d.register(0, 2, ObjectKind::Heart);
    for (slot, object) in [(0, 1), (1, 2)] {
        let a = d.claim(slot, object);
        assert!(a.event.is_some() && a.wallets.is_empty());
        assert_eq!(
            rejected(&d.claim(1 - slot, object)),
            &Rejection::AlreadyTaken { by: slot }
        );
    }
    assert_eq!((d.wallet(0), d.wallet(1)), (0, 0));
    assert_consistent(&d.engine);
}

#[test]
fn wallet_overflow_is_refused_without_partial_effect() {
    let (dir, path, engine, _t) = create_run();
    let mut d = Driver::new(engine);
    d.register(0, 1, nugget(10));

    // Push a wallet next to the limit behind the engine's back (a second connection is fine
    // with WAL), then claim.
    let raw = Connection::open(&path).unwrap();
    raw.execute(
        "UPDATE players SET wallet = ?1 WHERE slot = 1",
        [i64::MAX - 5],
    )
    .unwrap();

    let a = d.claim(0, 1);
    assert_eq!(rejected(&a), &Rejection::WalletOverflow);
    let (events, _) = d.engine.journal_after(0, 100).unwrap();
    assert_eq!(events.len(), 1); // only the registration
    assert_eq!(d.wallet(0), 0);
    drop(dir);
}

// --- idempotence and ordering ---------------------------------------------------------------

#[test]
fn repeating_a_command_returns_the_stored_result_and_has_no_second_effect() {
    let (_dir, _path, mut engine, _t) = create_run();
    let reg = CommandBody::RegisterObject {
        object_id: 1,
        kind: nugget(10),
    };
    engine.apply(0, 1, reg).unwrap();
    let claim = CommandBody::ClaimPickup { object_id: 1 };
    let first = engine.apply(0, 2, claim.clone()).unwrap();
    let replay = engine.apply(0, 2, claim).unwrap();
    assert_eq!(first.result, replay.result);
    assert!(replay.event.is_none() && replay.wallets.is_empty());

    // Even replaying under a different body returns the stored answer: the number decides.
    let other = engine
        .apply(0, 2, CommandBody::Spend { amount: 1 })
        .unwrap();
    assert_eq!(other.result, first.result);

    assert_eq!(engine.welcome(1).unwrap().wallet, 10);
    assert_eq!(engine.journal_after(0, 100).unwrap().0.len(), 2);
}

#[test]
fn a_gap_in_command_numbers_is_refused_and_does_not_consume_a_number() {
    let (_dir, _path, mut engine, _t) = create_run();
    let reg = |id| CommandBody::RegisterObject {
        object_id: id,
        kind: nugget(1),
    };
    let a = engine.apply(0, 3, reg(1)).unwrap();
    assert_eq!(rejected(&a), &Rejection::OutOfOrder { expected: 1 });
    let a = engine.apply(0, 1, reg(1)).unwrap();
    assert!(matches!(a.result, CommandResult::Applied { .. }));
}

// --- snapshots ------------------------------------------------------------------------------

#[test]
fn snapshot_is_stored_validated_and_never_touches_the_wallet() {
    let (_dir, _path, engine, _t) = create_run();
    let mut d = Driver::new(engine);
    d.register(0, 1, nugget(10));
    d.claim(0, 1); // head = 2

    let snap = |seq, applied, blob: &[u8]| CommandBody::UploadPlayerSnapshot {
        snapshot_seq: seq,
        applied_event_seq: applied,
        blob: blob.to_vec(),
    };
    assert!(matches!(
        d.run(0, snap(1, 2, b"inventory-v1")).result,
        CommandResult::Applied { .. }
    ));
    assert_eq!(
        rejected(&d.run(0, snap(1, 2, b"again"))),
        &Rejection::StaleSnapshot { stored: 1 }
    );
    assert_eq!(
        rejected(&d.run(0, snap(2, 99, b"x"))),
        &Rejection::CursorAhead { head: 2 }
    );
    let huge = vec![0u8; noita_coop_protocol::MAX_SNAPSHOT_LEN + 1];
    assert_eq!(
        rejected(&d.run(0, snap(3, 2, &huge))),
        &Rejection::SnapshotTooLarge
    );

    let w = d.engine.welcome(0).unwrap();
    let stored = w.snapshot.unwrap();
    assert_eq!((stored.snapshot_seq, stored.applied_event_seq), (1, 2));
    assert_eq!(stored.blob, b"inventory-v1");
    assert_eq!(w.wallet, 10);
    assert!(d.engine.welcome(1).unwrap().snapshot.is_none());
}

#[test]
fn damaged_snapshot_is_detected_not_served() {
    let (_dir, path, engine, _t) = create_run();
    let mut d = Driver::new(engine);
    d.run(
        0,
        CommandBody::UploadPlayerSnapshot {
            snapshot_seq: 1,
            applied_event_seq: 0,
            blob: vec![1, 2, 3],
        },
    );
    Connection::open(&path)
        .unwrap()
        .execute(
            "UPDATE players SET snapshot = x'010203ff' WHERE slot = 0",
            [],
        )
        .unwrap();
    assert!(d.engine.welcome(0).is_err());
}

// --- durability and the two start paths -------------------------------------------------------

#[test]
fn everything_confirmed_survives_a_restart() {
    let (_dir, path, engine, tokens) = create_run();
    let mut d = Driver::new(engine);
    d.register(0, 1, nugget(10));
    d.claim(0, 1);
    d.run(0, CommandBody::Spend { amount: 3 });
    d.run(
        1,
        CommandBody::UploadPlayerSnapshot {
            snapshot_seq: 5,
            applied_event_seq: 3,
            blob: b"p".to_vec(),
        },
    );
    drop(d);

    let engine = Engine::resume(&path).unwrap();
    let w0 = engine.welcome(0).unwrap();
    let w1 = engine.welcome(1).unwrap();
    assert_eq!((w0.wallet, w0.last_command_seq, w0.head_seq), (7, 3, 3));
    assert_eq!((w1.wallet, w1.last_command_seq), (10, 1));
    assert_eq!(w1.snapshot.unwrap().blob, b"p");
    // Tokens still authenticate; they were not lost with the process.
    assert!(matches!(
        engine.authenticate(&hello(&tokens[1].token)).unwrap(),
        Auth::Accepted(1)
    ));
    assert_consistent(&engine);
}

#[test]
fn create_never_overwrites_and_resume_never_creates() {
    let (dir, path, engine, _t) = create_run();
    drop(engine);
    assert!(matches!(
        Engine::create(&path, &new_run()),
        Err(CreateError::AlreadyExists(_))
    ));

    let missing = dir.path().join("nope.sqlite");
    assert!(matches!(
        Engine::resume(&missing),
        Err(OpenError::NotFound(_))
    ));
    assert!(!missing.exists());

    let junk = dir.path().join("junk.sqlite");
    std::fs::write(
        &junk,
        b"this is not a database at all, just text padding.....",
    )
    .unwrap();
    assert!(matches!(
        Engine::resume(&junk),
        Err(OpenError::NotARunDatabase)
    ));

    let empty_db = dir.path().join("empty.sqlite");
    Connection::open(&empty_db)
        .unwrap()
        .execute_batch("CREATE TABLE t(x)")
        .unwrap();
    assert!(matches!(
        Engine::resume(&empty_db),
        Err(OpenError::NotARunDatabase)
    ));
}

#[test]
fn unknown_schema_version_is_refused() {
    let (_dir, path, engine, _t) = create_run();
    drop(engine);
    Connection::open(&path)
        .unwrap()
        .pragma_update(None, "user_version", 99)
        .unwrap();
    assert!(matches!(
        Engine::resume(&path),
        Err(OpenError::SchemaMismatch { found: 99, .. })
    ));
}

#[test]
fn a_second_server_on_the_same_run_is_refused() {
    let (_dir, path, _engine, _t) = create_run();
    assert!(matches!(Engine::resume(&path), Err(OpenError::Locked(_))));
}

#[test]
fn run_parameters_are_validated() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("r.sqlite");
    let mut bad = new_run();
    bad.player_names = vec!["Solo".into()];
    assert!(matches!(
        Engine::create(&path, &bad),
        Err(CreateError::InvalidParams(_))
    ));
    bad.player_names = vec!["A".into(), "A".into()];
    assert!(matches!(
        Engine::create(&path, &bad),
        Err(CreateError::InvalidParams(_))
    ));
    bad.player_names = vec!["A".into(), "B".into(), "C".into(), "D".into(), "E".into()];
    assert!(matches!(
        Engine::create(&path, &bad),
        Err(CreateError::InvalidParams(_))
    ));
    assert!(!path.exists(), "a refused creation leaves nothing behind");
}

#[test]
fn consistency_check_catches_a_tampered_wallet() {
    let (_dir, path, engine, _t) = create_run();
    let mut d = Driver::new(engine);
    d.register(0, 1, nugget(10));
    d.claim(0, 1);
    assert_consistent(&d.engine);
    Connection::open(&path)
        .unwrap()
        .execute("UPDATE players SET wallet = wallet + 1 WHERE slot = 0", [])
        .unwrap();
    assert!(!d.engine.check_consistency().unwrap().is_empty());
    drop(d);
    assert!(matches!(Engine::resume(&path), Err(OpenError::Storage(_))));
}

// --- end of the run (D12) ---------------------------------------------------------------------

#[test]
fn ended_run_is_closed_forever_and_its_data_is_gone() {
    let (_dir, path, engine, tokens) = create_run();
    let mut d = Driver::new(engine);
    d.register(0, 1, nugget(10));
    d.claim(0, 1);
    d.run(
        0,
        CommandBody::UploadPlayerSnapshot {
            snapshot_seq: 1,
            applied_event_seq: 2,
            blob: b"secret".to_vec(),
        },
    );
    let run_id = d.engine.info().run_id;

    d.engine.end_run("all players dead").unwrap();
    assert!(matches!(
        d.engine.authenticate(&hello(&tokens[0].token)).unwrap(),
        Auth::Rejected(RejectReason::RunEnded)
    ));
    drop(d);

    match Engine::resume(&path) {
        Err(OpenError::RunEnded { run_id: id, .. }) => assert_eq!(id, run_id),
        other => panic!("{:?}", other.err()),
    }
    assert!(matches!(
        Engine::create(&path, &new_run()),
        Err(CreateError::AlreadyExists(_))
    ));

    let raw = Connection::open(&path).unwrap();
    for table in ["players", "objects", "journal", "commands"] {
        let n: i64 = raw
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0, "{table} must be empty after the run ended");
    }
    let bytes = std::fs::read(&path).unwrap();
    assert!(
        !bytes.windows(6).any(|w| w == b"secret"),
        "snapshot data must not linger in the file"
    );
}

#[test]
fn a_backup_taken_before_the_end_can_be_restored_by_the_host() {
    // D12/D40: the last backup is the host's way out of a false game over.
    let (dir, path, engine, _t) = create_run();
    let mut d = Driver::new(engine);
    d.register(0, 1, nugget(10));
    d.claim(0, 1);
    let backups = dir.path().join("backups");
    let file = backup::write(d.engine.store(), &backups, 1_000).unwrap();

    d.engine.end_run("bug").unwrap();
    drop(d);
    assert!(matches!(
        Engine::resume(&path),
        Err(OpenError::RunEnded { .. })
    ));

    // Host action: put the backup in place of the database.
    std::fs::remove_file(&path).unwrap();
    let _ = std::fs::remove_file(format!("{}-wal", path.display()));
    let _ = std::fs::remove_file(format!("{}-shm", path.display()));
    std::fs::copy(&file, &path).unwrap();
    let engine = Engine::resume(&path).unwrap();
    assert_eq!(engine.welcome(1).unwrap().wallet, 10);
    assert_consistent(&engine);
}

#[test]
fn backup_is_a_complete_openable_copy() {
    let (dir, _path, engine, _t) = create_run();
    let mut d = Driver::new(engine);
    d.register(0, 1, nugget(10));
    d.claim(1, 1);
    let file = backup::write(d.engine.store(), &dir.path().join("b"), 5).unwrap();
    assert!(file.ends_with("run-000000000005.sqlite"));

    let copy = Store::open(&file).unwrap();
    assert_eq!(copy.run_info().unwrap(), d.engine.info().clone());
}
