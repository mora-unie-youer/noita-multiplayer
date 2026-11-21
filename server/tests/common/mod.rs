#![allow(dead_code)]

use std::path::{Path, PathBuf};

use noita_coop_protocol::{
    CommandBody, CommandResult, Hello, ObjectKind, PROTOCOL_VERSION, RunInfo, RunSettings, Slot,
};
use noita_coop_server::engine::{Applied, Engine};
use noita_coop_server::storage::{IssuedToken, NewRun};

pub const GAME_HASH: &str = "deadbeefcafe";
pub const SEED: u64 = 0xFEED_FACE_0123_4567;

pub fn new_run() -> NewRun {
    NewRun {
        seed: SEED,
        ng_count: 0,
        game_hash: GAME_HASH.into(),
        mod_version: "test".into(),
        mods: vec!["noita_cartographer".into()],
        player_names: vec!["Alice".into(), "Bob".into()],
        settings: RunSettings::default(),
    }
}

/// A fresh run in a temp directory. Returns the directory guard too.
pub fn create_run() -> (tempfile::TempDir, PathBuf, Engine, Vec<IssuedToken>) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("run.sqlite");
    let (engine, tokens) = Engine::create(&path, &new_run()).unwrap();
    (dir, path, engine, tokens)
}

pub fn hello(token: &str) -> Hello {
    Hello {
        protocol_version: PROTOCOL_VERSION,
        token: token.to_owned(),
        game_hash: GAME_HASH.into(),
        mod_version: "test".into(),
        seed: SEED,
        mods: vec!["noita_cartographer".into()],
    }
}

pub fn nugget(amount: i64) -> ObjectKind {
    ObjectKind::GoldNugget { amount }
}

/// Test driver that numbers commands per slot like a well-behaved agent.
pub struct Driver {
    pub engine: Engine,
    next: [u64; 4],
}

impl Driver {
    pub fn new(engine: Engine) -> Self {
        Self {
            engine,
            next: [1; 4],
        }
    }

    pub fn run(&mut self, slot: Slot, body: CommandBody) -> Applied {
        let seq = self.next[slot as usize];
        self.next[slot as usize] += 1;
        self.engine.apply(slot, seq, body).unwrap()
    }

    pub fn register(&mut self, slot: Slot, object_id: u64, kind: ObjectKind) {
        let a = self.run(slot, CommandBody::RegisterObject { object_id, kind });
        assert!(
            matches!(a.result, CommandResult::Applied { .. }),
            "{:?}",
            a.result
        );
    }

    pub fn claim(&mut self, slot: Slot, object_id: u64) -> Applied {
        self.run(slot, CommandBody::ClaimPickup { object_id })
    }

    pub fn wallet(&self, slot: Slot) -> i64 {
        self.engine.welcome(slot).unwrap().wallet
    }
}

pub fn info(engine: &Engine) -> RunInfo {
    engine.info().clone()
}

pub fn assert_consistent(engine: &Engine) {
    let problems = engine.check_consistency().unwrap();
    assert!(problems.is_empty(), "{problems:?}");
}

pub fn sqlite_path(dir: &Path) -> PathBuf {
    dir.join("run.sqlite")
}
