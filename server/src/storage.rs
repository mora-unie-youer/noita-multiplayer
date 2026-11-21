//! SQLite storage of one run: schema, the separate create/open paths, locking and backups.
//!
//! Durability: WAL with `synchronous=FULL`, so a committed transaction survives `kill -9` and a
//! power cut. The engine acknowledges a command only after its transaction committed.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::time::Duration;

use noita_coop_protocol::{MAX_PLAYERS, MIN_PLAYERS, PlayerInfo, RunId, RunInfo, RunSettings};
use rusqlite::{Connection, OptionalExtension, backup::Backup, params};
use sha2::{Digest, Sha256};

use crate::error::{CreateError, OpenError, StorageError};

/// Bump together with a migration. An unsupported version is refused, never guessed at.
pub const SCHEMA_VERSION: i32 = 1;

const SCHEMA: &str = "
CREATE TABLE run (
    id           INTEGER PRIMARY KEY CHECK (id = 1),
    run_id       BLOB    NOT NULL,
    seed         INTEGER NOT NULL,   -- u64 stored as its i64 bit pattern
    ng_count     INTEGER NOT NULL,
    game_hash    TEXT    NOT NULL,
    mod_version  TEXT    NOT NULL,
    mods         TEXT    NOT NULL,   -- sorted names joined by newline
    settings     BLOB    NOT NULL,   -- postcard RunSettings
    created_at   INTEGER NOT NULL
);

CREATE TABLE players (
    slot                 INTEGER PRIMARY KEY,
    name                 TEXT    NOT NULL UNIQUE,
    token_hash           BLOB    NOT NULL UNIQUE,   -- sha256 of the token, never the token
    wallet               INTEGER NOT NULL DEFAULT 0,
    last_command_seq     INTEGER NOT NULL DEFAULT 0,
    snapshot_seq         INTEGER NOT NULL DEFAULT 0,
    snapshot_applied_seq INTEGER NOT NULL DEFAULT 0,
    snapshot_crc         INTEGER,
    snapshot             BLOB
);

CREATE TABLE objects (
    object_id       INTEGER PRIMARY KEY,   -- u64 stored as its i64 bit pattern
    kind            BLOB    NOT NULL,      -- postcard ObjectKind
    taken_by        INTEGER REFERENCES players(slot),
    taken_event_seq INTEGER
);

CREATE TABLE journal (
    seq    INTEGER PRIMARY KEY AUTOINCREMENT,
    origin INTEGER NOT NULL,
    time   INTEGER NOT NULL,
    body   BLOB    NOT NULL                -- postcard EventBody
);

-- Result of every applied command; a repeated (slot, seq) returns it unchanged.
CREATE TABLE commands (
    slot   INTEGER NOT NULL,
    seq    INTEGER NOT NULL,
    result BLOB    NOT NULL,               -- postcard CommandResult
    PRIMARY KEY (slot, seq)
) WITHOUT ROWID;

-- Present only after the run ended (D12). Closes the run id forever.
CREATE TABLE terminal (
    run_id   BLOB    PRIMARY KEY,
    ended_at INTEGER NOT NULL,
    reason   TEXT    NOT NULL
);
";

/// Parameters of a new run.
#[derive(Clone, Debug)]
pub struct NewRun {
    pub seed: u64,
    pub ng_count: u32,
    pub game_hash: String,
    pub mod_version: String,
    pub mods: Vec<String>,
    pub player_names: Vec<String>,
    pub settings: RunSettings,
}

/// Secret issued to a player when the run is created. Shown once; only its hash is stored.
#[derive(Clone, Debug)]
pub struct IssuedToken {
    pub slot: u8,
    pub name: String,
    pub token: String,
}

pub struct Store {
    pub(crate) conn: Connection,
    path: PathBuf,
    /// Held for the lifetime of the store: two servers on one database would corrupt the run.
    _lock: File,
}

pub fn hash_token(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

pub fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

// SQLite integers are signed 64-bit. Ids are u64; the bit pattern is stored and restored, so
// ordering of the column is meaningless but equality and lookup are exact.
pub(crate) fn to_db(v: u64) -> i64 {
    i64::from_ne_bytes(v.to_ne_bytes())
}
pub(crate) fn from_db(v: i64) -> u64 {
    u64::from_ne_bytes(v.to_ne_bytes())
}

fn lock_path(db: &Path) -> PathBuf {
    let mut s = db.as_os_str().to_owned();
    s.push(".lock");
    PathBuf::from(s)
}

fn take_lock(db: &Path) -> Result<Option<File>, std::io::Error> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(lock_path(db))?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(e)) => Err(e),
    }
}

fn configure(conn: &Connection) -> rusqlite::Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "FULL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    Ok(())
}

fn fsync_dir(dir: &Path) -> std::io::Result<()> {
    File::open(dir)?.sync_all()
}

fn random_bytes<const N: usize>() -> Result<[u8; N], StorageError> {
    let mut buf = [0u8; N];
    getrandom::fill(&mut buf)
        .map_err(|e| StorageError::Io(std::io::Error::other(e.to_string())))?;
    Ok(buf)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn validate(new: &NewRun) -> Result<(), CreateError> {
    let bad = |m: &str| Err(CreateError::InvalidParams(m.to_owned()));
    if !(MIN_PLAYERS..=MAX_PLAYERS).contains(&new.player_names.len()) {
        return bad("a run needs 2 to 4 players (D1)");
    }
    for (i, name) in new.player_names.iter().enumerate() {
        if name.trim().is_empty() {
            return bad("player names must not be empty");
        }
        if new.player_names[..i].contains(name) {
            return bad("player names must be unique");
        }
    }
    if new.game_hash.trim().is_empty() {
        return bad("game hash must not be empty");
    }
    if new.mods.iter().any(|m| m.is_empty() || m.contains('\n')) {
        return bad("mod names must be non-empty single lines");
    }
    Ok(())
}

impl Store {
    /// The **new run** path. Fails if anything exists at `path`; never opens an existing run.
    ///
    /// The database is built under a temporary name and renamed only once complete, so a crash
    /// during creation leaves no half-initialised run behind.
    pub fn create(
        path: &Path,
        new: &NewRun,
    ) -> Result<(Store, RunInfo, Vec<IssuedToken>), CreateError> {
        validate(new)?;
        let lock = take_lock(path)
            .map_err(StorageError::Io)?
            .ok_or_else(|| CreateError::Locked(path.to_owned()))?;
        if path.exists() {
            return Err(CreateError::AlreadyExists(path.to_owned()));
        }

        let mut mods = new.mods.clone();
        mods.sort();
        mods.dedup();
        let run_id = RunId(random_bytes::<16>()?);

        let tmp = {
            let mut s = path.as_os_str().to_owned();
            s.push(".creating");
            PathBuf::from(s)
        };
        remove_db_files(&tmp);
        let mut issued = Vec::new();
        {
            let mut conn = Connection::open(&tmp).map_err(StorageError::from)?;
            configure(&conn).map_err(StorageError::from)?;
            let tx = conn.transaction().map_err(StorageError::from)?;
            tx.execute_batch(SCHEMA).map_err(StorageError::from)?;
            tx.execute(
                "INSERT INTO run (id, run_id, seed, ng_count, game_hash, mod_version, mods, settings, created_at)
                 VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    run_id.0.as_slice(),
                    to_db(new.seed),
                    new.ng_count,
                    new.game_hash,
                    new.mod_version,
                    mods.join("\n"),
                    postcard::to_stdvec(&new.settings).map_err(StorageError::from)?,
                    unix_now(),
                ],
            )
            .map_err(StorageError::from)?;
            for (slot, name) in new.player_names.iter().enumerate() {
                let token = hex(&random_bytes::<32>()?);
                tx.execute(
                    "INSERT INTO players (slot, name, token_hash) VALUES (?1, ?2, ?3)",
                    params![slot as i64, name, hash_token(&token).as_slice()],
                )
                .map_err(StorageError::from)?;
                issued.push(IssuedToken {
                    slot: slot as u8,
                    name: name.clone(),
                    token,
                });
            }
            tx.pragma_update(None, "user_version", SCHEMA_VERSION)
                .map_err(StorageError::from)?;
            tx.commit().map_err(StorageError::from)?;
            conn.close().map_err(|(_, e)| StorageError::from(e))?;
        }
        File::open(&tmp)
            .and_then(|f| f.sync_all())
            .map_err(StorageError::from)?;
        fs::rename(&tmp, path).map_err(StorageError::from)?;
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            fsync_dir(dir).map_err(StorageError::from)?;
        }

        let store = Self::attach(path, lock)?;
        let info = store.run_info()?;
        Ok((store, info, issued))
    }

    /// The **continue run** path. Fails if there is no run; never creates one.
    pub fn open(path: &Path) -> Result<Store, OpenError> {
        if !path.exists() {
            return Err(OpenError::NotFound(path.to_owned()));
        }
        let lock = take_lock(path)
            .map_err(StorageError::Io)?
            .ok_or_else(|| OpenError::Locked(path.to_owned()))?;
        let conn = Connection::open(path).map_err(StorageError::from)?;
        let not_a_db = |e: &rusqlite::Error| {
            matches!(
                e,
                rusqlite::Error::SqliteFailure(f, _)
                    if f.code == rusqlite::ErrorCode::NotADatabase
            )
        };
        let version: i32 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .map_err(|e| {
                if not_a_db(&e) {
                    OpenError::NotARunDatabase
                } else {
                    StorageError::from(e).into()
                }
            })?;
        if version == 0 {
            return Err(OpenError::NotARunDatabase);
        }
        if version != SCHEMA_VERSION {
            return Err(OpenError::SchemaMismatch {
                found: version,
                supported: SCHEMA_VERSION,
            });
        }
        drop(conn);

        let store = Self::attach(path, lock)?;
        let check: String = store
            .conn
            .pragma_query_value(None, "quick_check", |r| r.get(0))
            .map_err(StorageError::from)?;
        if check != "ok" {
            return Err(StorageError::Corrupt(format!("quick_check: {check}")).into());
        }
        if let Some((run_id, ended_at)) = store.terminal()? {
            return Err(OpenError::RunEnded { run_id, ended_at });
        }
        Ok(store)
    }

    fn attach(path: &Path, lock: File) -> Result<Store, StorageError> {
        let conn = Connection::open(path)?;
        configure(&conn)?;
        Ok(Store {
            conn,
            path: path.to_owned(),
            _lock: lock,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn run_info(&self) -> Result<RunInfo, StorageError> {
        let (run_id, seed, ng_count, game_hash, mod_version, mods, settings) = self.conn.query_row(
            "SELECT run_id, seed, ng_count, game_hash, mod_version, mods, settings FROM run WHERE id = 1",
            [],
            |r| {
                Ok((
                    r.get::<_, Vec<u8>>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, u32>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, Vec<u8>>(6)?,
                ))
            },
        )?;
        let run_id = RunId(
            run_id
                .try_into()
                .map_err(|_| StorageError::Corrupt("run_id is not 16 bytes".into()))?,
        );
        let mut stmt = self
            .conn
            .prepare("SELECT slot, name FROM players ORDER BY slot")?;
        let players = stmt
            .query_map([], |r| {
                Ok(PlayerInfo {
                    slot: r.get::<_, u8>(0)?,
                    name: r.get(1)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(RunInfo {
            run_id,
            seed: from_db(seed),
            ng_count,
            game_hash,
            mod_version,
            mods: if mods.is_empty() {
                Vec::new()
            } else {
                mods.split('\n').map(str::to_owned).collect()
            },
            players,
            settings: postcard::from_bytes(&settings)?,
        })
    }

    /// `(run_id, ended_at)` once the run ended.
    pub fn terminal(&self) -> Result<Option<(RunId, i64)>, StorageError> {
        let row: Option<(Vec<u8>, i64)> = self
            .conn
            .query_row("SELECT run_id, ended_at FROM terminal", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()?;
        row.map(|(id, at)| {
            let id =
                RunId(id.try_into().map_err(|_| {
                    StorageError::Corrupt("terminal run_id is not 16 bytes".into())
                })?);
            Ok((id, at))
        })
        .transpose()
    }

    /// Consistent copy of the whole database via the SQLite online backup API.
    ///
    /// Written under a temporary name, synced, then renamed: a crash never leaves a truncated
    /// file under a real backup name.
    pub fn backup_to(&self, dest: &Path) -> Result<(), StorageError> {
        let tmp = {
            let mut s = dest.as_os_str().to_owned();
            s.push(".partial");
            PathBuf::from(s)
        };
        remove_db_files(&tmp);
        {
            let mut out = Connection::open(&tmp)?;
            Backup::new(&self.conn, &mut out)?.run_to_completion(256, Duration::ZERO, None)?;
            out.close().map_err(|(_, e)| e)?;
        }
        File::open(&tmp)?.sync_all()?;
        fs::rename(&tmp, dest)?;
        if let Some(dir) = dest.parent().filter(|d| !d.as_os_str().is_empty()) {
            fsync_dir(dir)?;
        }
        Ok(())
    }

    /// Fold the WAL into the main file and drop freed pages (used after the run ended).
    pub(crate) fn compact(&self) -> Result<(), StorageError> {
        self.conn.execute_batch("VACUUM")?;
        self.conn
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
        Ok(())
    }
}

fn remove_db_files(db: &Path) {
    for suffix in ["", "-wal", "-shm"] {
        let mut s = db.as_os_str().to_owned();
        s.push(suffix);
        let _ = fs::remove_file(PathBuf::from(s));
    }
}

/// Read a `u64` stored with [`to_db`].
pub(crate) fn get_u64(row: &rusqlite::Row<'_>, idx: usize) -> rusqlite::Result<u64> {
    row.get::<_, i64>(idx).map(from_db)
}
