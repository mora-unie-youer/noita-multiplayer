use std::fmt;

use serde::{Deserialize, Serialize};

/// Bumped on every incompatible change of any type in this crate.
pub const PROTOCOL_VERSION: u32 = 1;

/// D1: 2–4 players, fixed when the run is created.
pub const MIN_PLAYERS: usize = 2;
pub const MAX_PLAYERS: usize = 4;

/// Named player slot of a run (`player_id` in SYNC_CONTRACTS §3). Not a network identity.
pub type Slot = u8;
/// Id of a registered world object (nugget, orb, heart, ...). Assigned by the registering side.
pub type ObjectId = u64;
/// Per-player command counter. Starts at 1; the server demands `last + 1` for a new command.
pub type CommandSeq = u64;
/// Position in the server journal. Starts at 1; 0 means "before the first event".
pub type EventSeq = u64;

/// Largest nugget the server accepts. Keeps sums of many claims far from `i64::MAX`.
pub const MAX_NUGGET_AMOUNT: i64 = 1_000_000_000_000;
/// Largest player snapshot blob the server stores.
pub const MAX_SNAPSHOT_LEN: usize = 4 * 1024 * 1024;
/// Most journal events returned by one [`ClientMsg::GetJournal`].
pub const MAX_JOURNAL_PAGE: u32 = 1000;

/// Random identity of a run. A finished run id is closed forever (D12, terminal marker).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RunId(pub [u8; 16]);

impl fmt::Display for RunId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|b| write!(f, "{b:02x}"))
    }
}

impl fmt::Debug for RunId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RunId({self})")
    }
}

/// Run settings chosen by the host (D10, D25, D30). The defaults are the decided ones.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct RunSettings {
    /// Friendly fire, damage (D30). Off by default.
    pub friendly_fire_damage: bool,
    /// Friendly fire, harmful statuses: polymorph, charm, teleport, blindness (D30). Off.
    pub friendly_fire_harmful_status: bool,
    /// Helping allies: healing, shields, haste (D30). On.
    pub friendly_fire_helpful: bool,
    /// Enemy HP multiplier (D25).
    pub enemy_hp_mult: f32,
}

impl Default for RunSettings {
    fn default() -> Self {
        Self {
            friendly_fire_damage: false,
            friendly_fire_harmful_status: false,
            friendly_fire_helpful: true,
            enemy_hp_mult: 1.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerInfo {
    pub slot: Slot,
    pub name: String,
}

/// Immutable facts about a run plus its settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RunInfo {
    pub run_id: RunId,
    pub seed: u64,
    pub ng_count: u32,
    /// Build of the game the run is pinned to (D19). Compared with `Hello::game_hash`.
    pub game_hash: String,
    /// Mod build that created the run. Informational: updating the mod mid-run is allowed.
    pub mod_version: String,
    /// Mod whitelist of the run (D23), sorted. Compared with `Hello::mods`.
    pub mods: Vec<String>,
    pub players: Vec<PlayerInfo>,
    pub settings: RunSettings,
}

// ---------------------------------------------------------------------------------------------
// Client → server
// ---------------------------------------------------------------------------------------------

/// First message of every connection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hello {
    pub protocol_version: u32,
    /// Hex token issued for the slot when the run was created. Identifies the player.
    pub token: String,
    pub game_hash: String,
    pub mod_version: String,
    /// Seed the game was started with. Must equal the run seed.
    pub seed: u64,
    /// Enabled mods, sorted. Must equal the run whitelist.
    pub mods: Vec<String>,
}

/// What a world object is. Decides what a successful claim does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObjectKind {
    /// D4/D32: one physical nugget; the claim credits every player `amount` (shared income).
    GoldNugget { amount: i64 },
    /// Shared orb collection. `orb_id` identifies the orb across the main and parallel worlds.
    Orb { orb_id: u32 },
    /// D6: one heart; the claim records the shared effect.
    Heart,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandBody {
    /// A chunk authority announces an object that must be claimable exactly once.
    RegisterObject {
        object_id: ObjectId,
        kind: ObjectKind,
    },
    /// "I picked this up." The server arbitrates: the first claim wins.
    ClaimPickup { object_id: ObjectId },
    /// Individual expense (purchase, reroll). Atomic check-and-debit; never creates income.
    Spend { amount: i64 },
    /// Full player snapshot for reconnects. It never carries or changes the wallet.
    UploadPlayerSnapshot {
        /// Monotonic per slot; an older or equal number is rejected.
        snapshot_seq: u64,
        /// Journal position already reflected in the snapshot (SYNC_CONTRACTS §6).
        applied_event_seq: EventSeq,
        blob: Vec<u8>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClientMsg {
    Hello(Hello),
    Command {
        seq: CommandSeq,
        body: CommandBody,
    },
    /// Page of the journal after `after_seq`. Used after the welcome to catch up.
    GetJournal {
        after_seq: EventSeq,
        limit: u32,
    },
    Ping(u64),
}

// ---------------------------------------------------------------------------------------------
// Server → client
// ---------------------------------------------------------------------------------------------

/// Why the handshake was refused. The connection is closed afterwards.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RejectReason {
    ProtocolVersion {
        server: u32,
    },
    BadToken,
    GameHash {
        expected: String,
    },
    SeedMismatch,
    ModsMismatch {
        expected: Vec<String>,
    },
    /// The run is over (D12); its id is closed forever.
    RunEnded,
}

/// Why the server closes an established connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CloseReason {
    /// The same slot connected again; the newest connection wins.
    Superseded,
    ServerShutdown,
    /// The server could not write to disk and stopped instead of accepting unsaved state.
    StorageFailure,
    /// First message was not `Hello`, a second `Hello` arrived, or similar.
    ProtocolViolation,
}

/// Result of a command. Stored by the server and returned unchanged on a repeated command.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandResult {
    Applied {
        /// Journal position of the resulting event; `None` for commands without one.
        event_seq: Option<EventSeq>,
    },
    Rejected(Rejection),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Rejection {
    UnknownObject,
    ObjectAlreadyRegistered,
    /// Lost the race: someone claimed it first.
    AlreadyTaken {
        by: Slot,
    },
    InvalidAmount,
    InsufficientFunds {
        balance: i64,
    },
    WalletOverflow,
    SnapshotTooLarge,
    /// `applied_event_seq` is beyond the journal head.
    CursorAhead {
        head: EventSeq,
    },
    /// A newer snapshot is already stored.
    StaleSnapshot {
        stored: u64,
    },
    /// New commands must carry `last + 1`. Not stored; the sequence number is not consumed.
    OutOfOrder {
        expected: CommandSeq,
    },
}

/// A durable, ordered fact about the shared state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalEvent {
    pub seq: EventSeq,
    pub origin: Slot,
    /// Unix seconds on the server.
    pub time: i64,
    pub body: EventBody,
}

/// The events are a log. Balances travel in [`ServerMsg::Wallet`], not here, so a replayed tail
/// can never be applied to a wallet a second time.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventBody {
    ObjectRegistered {
        object_id: ObjectId,
        kind: ObjectKind,
    },
    GoldIncome {
        object_id: ObjectId,
        base_amount: i64,
        claimed_by: Slot,
    },
    OrbTaken {
        object_id: ObjectId,
        orb_id: u32,
        claimed_by: Slot,
    },
    HeartTaken {
        object_id: ObjectId,
        claimed_by: Slot,
    },
    Spent {
        slot: Slot,
        amount: i64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredSnapshot {
    pub snapshot_seq: u64,
    pub applied_event_seq: EventSeq,
    pub blob: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Welcome {
    pub run: RunInfo,
    pub slot: Slot,
    /// Balance as of `head_seq`.
    pub wallet: i64,
    pub snapshot: Option<StoredSnapshot>,
    pub head_seq: EventSeq,
    /// Last command the server applied for this slot; the next one must be this + 1.
    pub last_command_seq: CommandSeq,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ServerMsg {
    Welcome(Welcome),
    Rejected(RejectReason),
    CommandResult {
        seq: CommandSeq,
        result: CommandResult,
    },
    /// Live copy of a freshly committed journal event, sent to every connected player.
    Event(JournalEvent),
    /// New authoritative balance of the receiving player. Newer `as_of_seq` replaces older.
    Wallet {
        balance: i64,
        as_of_seq: EventSeq,
    },
    Journal {
        events: Vec<JournalEvent>,
        head_seq: EventSeq,
    },
    Pong(u64),
    Closed(CloseReason),
}
