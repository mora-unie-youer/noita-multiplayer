//! Authoritative run server (D3). See `docs/ARCHITECTURE.md` and `docs/SYNC_CONTRACTS.md`.
//!
//! * [`storage`]: SQLite schema, separate *create* and *resume* paths, locking, backup copy.
//! * [`engine`]: commands, arbitration, journal, wallets; no I/O except the database.
//! * [`backup`]: backup file names and rotation (D40).
//! * [`net`]: provisional TCP transport around the engine.

pub mod backup;
pub mod engine;
pub mod error;
pub mod net;
pub mod storage;
