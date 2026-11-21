use std::io;
use std::path::PathBuf;

use noita_coop_protocol::RunId;

/// Failure of the durable layer. The server treats every one of these as fatal for the running
/// session: it stops accepting commands instead of continuing with state it cannot save
/// (SYNC_CONTRACTS §12).
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("i/o error: {0}")]
    Io(#[from] io::Error),
    #[error("stored data is inconsistent: {0}")]
    Corrupt(String),
    #[error("cannot encode or decode stored value: {0}")]
    Codec(#[from] postcard::Error),
}

/// Errors of the "new run" path. Creating never touches an existing run (R6.4).
#[derive(Debug, thiserror::Error)]
pub enum CreateError {
    #[error("{0} already exists; refusing to overwrite a run")]
    AlreadyExists(PathBuf),
    #[error("invalid run parameters: {0}")]
    InvalidParams(String),
    #[error("{0} is locked by another server process")]
    Locked(PathBuf),
    #[error(transparent)]
    Storage(#[from] StorageError),
}

/// Errors of the "continue run" path. Resuming never creates a run (R6.4).
#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    #[error("{0} does not exist; refusing to start a new run implicitly")]
    NotFound(PathBuf),
    #[error("{0} is locked by another server process")]
    Locked(PathBuf),
    #[error("not a run database (or damaged beyond reading)")]
    NotARunDatabase,
    #[error("database schema version {found} is not supported (this build supports {supported})")]
    SchemaMismatch { found: i32, supported: i32 },
    #[error("run {run_id} ended at unix time {ended_at}; its id is closed")]
    RunEnded { run_id: RunId, ended_at: i64 },
    #[error(transparent)]
    Storage(#[from] StorageError),
}
