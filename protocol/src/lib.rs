//! Wire protocol between the agent and the run server.
//!
//! One source of truth for both sides (ARCHITECTURE §2, §7). The messages are plain Rust types
//! serialised with `postcard`; [`frame`] adds the length prefix. The Lua side does not speak this
//! protocol: the future game-facing part of the agent translates (stage 3).
//!
//! # Rules that hold for every message
//!
//! * Every discrete change of shared state is a [`CommandBody`] with a per-player
//!   [`CommandSeq`]. The server applies it exactly once and remembers the result; a repeated
//!   `(slot, seq)` returns the remembered result (SYNC_CONTRACTS §3).
//! * A [`ServerMsg::CommandResult`] is sent only after the change is durable on disk
//!   (SYNC_CONTRACTS §2).
//! * 64-bit ids are `u64` here. A Lua double loses precision above 2^53, so the game-facing
//!   side must carry them as pairs of `u32` or as strings (SYNC_CONTRACTS §3).
//! * Wallet balances are owned by the server. The client displays [`ServerMsg::Wallet`] and
//!   never assigns a balance itself (SYNC_CONTRACTS §5).

pub mod frame;
pub mod messages;

pub use messages::*;
