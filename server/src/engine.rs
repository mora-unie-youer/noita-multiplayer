//! Rules of the run: handshake checks, command arbitration, journal, wallets.
//!
//! The engine knows nothing about sockets. It is a plain state machine over [`Store`], driven
//! from one thread, so two commands are always applied one after the other and "who claimed
//! first" is decided by the order they reach [`Engine::apply`].
//!
//! Every command runs in a single transaction that also records its result. The transaction
//! is committed before `apply` returns, and the transport sends the acknowledgement only after
//! that (SYNC_CONTRACTS §2).

use std::path::Path;

use noita_coop_protocol::{
    CommandBody, CommandResult, CommandSeq, EventBody, EventSeq, Hello, JournalEvent,
    MAX_JOURNAL_PAGE, MAX_NUGGET_AMOUNT, MAX_SNAPSHOT_LEN, ObjectId, ObjectKind, PROTOCOL_VERSION,
    RejectReason, Rejection, RunInfo, Slot, StoredSnapshot, Welcome,
};
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};

use crate::error::{CreateError, OpenError, StorageError};
use crate::storage::{IssuedToken, NewRun, Store, from_db, get_u64, hash_token, to_db, unix_now};

pub enum Auth {
    Accepted(Slot),
    Rejected(RejectReason),
}

/// Outcome of [`Engine::apply`].
#[derive(Debug)]
pub struct Applied {
    pub result: CommandResult,
    /// Event to broadcast. `None` for rejections, snapshots and replays of an old command.
    pub event: Option<JournalEvent>,
    /// New balances caused by the event, for the players whose wallet changed.
    pub wallets: Vec<(Slot, i64)>,
}

impl Applied {
    fn plain(result: CommandResult) -> Self {
        Self {
            result,
            event: None,
            wallets: Vec::new(),
        }
    }
    fn rejected(why: Rejection) -> Self {
        Self::plain(CommandResult::Rejected(why))
    }
}

pub struct Engine {
    store: Store,
    info: RunInfo,
    ended: bool,
    dirty: bool,
}

impl Engine {
    /// New run. See [`Store::create`].
    pub fn create(path: &Path, new: &NewRun) -> Result<(Engine, Vec<IssuedToken>), CreateError> {
        let (store, info, tokens) = Store::create(path, new)?;
        Ok((
            Engine {
                store,
                info,
                ended: false,
                dirty: true,
            },
            tokens,
        ))
    }

    /// Continue an existing run. See [`Store::open`]. Verifies the consistency invariants and
    /// refuses to start on a violation instead of serving damaged state.
    pub fn resume(path: &Path) -> Result<Engine, OpenError> {
        let store = Store::open(path)?;
        let info = store.run_info()?;
        let engine = Engine {
            store,
            info,
            ended: false,
            dirty: false,
        };
        let problems = engine.check_consistency()?;
        if !problems.is_empty() {
            return Err(StorageError::Corrupt(problems.join("; ")).into());
        }
        Ok(engine)
    }

    pub fn info(&self) -> &RunInfo {
        &self.info
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    /// True if something was committed since the last call (a backup is due).
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    /// Request a backup at the next opportunity (used after a failed backup).
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    // -----------------------------------------------------------------------------------------
    // Handshake
    // -----------------------------------------------------------------------------------------

    pub fn authenticate(&self, hello: &Hello) -> Result<Auth, StorageError> {
        use Auth::Rejected;
        if hello.protocol_version != PROTOCOL_VERSION {
            return Ok(Rejected(RejectReason::ProtocolVersion {
                server: PROTOCOL_VERSION,
            }));
        }
        if self.ended {
            return Ok(Rejected(RejectReason::RunEnded));
        }
        let slot: Option<u8> = self
            .store
            .conn
            .query_row(
                "SELECT slot FROM players WHERE token_hash = ?1",
                [hash_token(&hello.token).as_slice()],
                |r| r.get(0),
            )
            .optional()?;
        let Some(slot) = slot else {
            return Ok(Rejected(RejectReason::BadToken));
        };
        if hello.game_hash != self.info.game_hash {
            return Ok(Rejected(RejectReason::GameHash {
                expected: self.info.game_hash.clone(),
            }));
        }
        if hello.seed != self.info.seed {
            return Ok(Rejected(RejectReason::SeedMismatch));
        }
        let mut mods = hello.mods.clone();
        mods.sort();
        if mods != self.info.mods {
            return Ok(Rejected(RejectReason::ModsMismatch {
                expected: self.info.mods.clone(),
            }));
        }
        Ok(Auth::Accepted(slot))
    }

    pub fn welcome(&self, slot: Slot) -> Result<Welcome, StorageError> {
        let (wallet, last_command_seq, snapshot_seq, applied, crc, blob) =
            self.store.conn.query_row(
                "SELECT wallet, last_command_seq, snapshot_seq, snapshot_applied_seq,
                        snapshot_crc, snapshot
                 FROM players WHERE slot = ?1",
                [slot],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        get_u64(r, 1)?,
                        get_u64(r, 2)?,
                        get_u64(r, 3)?,
                        r.get::<_, Option<u32>>(4)?,
                        r.get::<_, Option<Vec<u8>>>(5)?,
                    ))
                },
            )?;
        let snapshot = match (blob, crc) {
            (Some(blob), Some(crc)) => {
                if crc32fast::hash(&blob) != crc {
                    return Err(StorageError::Corrupt(format!(
                        "player snapshot of slot {slot} fails its checksum"
                    )));
                }
                Some(StoredSnapshot {
                    snapshot_seq,
                    applied_event_seq: applied,
                    blob,
                })
            }
            (None, None) => None,
            _ => {
                return Err(StorageError::Corrupt(format!(
                    "snapshot and checksum of slot {slot} disagree"
                )));
            }
        };
        Ok(Welcome {
            run: self.info.clone(),
            slot,
            wallet,
            snapshot,
            head_seq: head_seq(&self.store.conn)?,
            last_command_seq,
        })
    }

    pub fn journal_after(
        &self,
        after: EventSeq,
        limit: u32,
    ) -> Result<(Vec<JournalEvent>, EventSeq), StorageError> {
        let limit = limit.min(MAX_JOURNAL_PAGE);
        let mut stmt = self.store.conn.prepare_cached(
            "SELECT seq, origin, time, body FROM journal WHERE seq > ?1 ORDER BY seq LIMIT ?2",
        )?;
        let events = stmt
            .query_map(params![to_db(after), limit], read_event)?
            .collect::<Result<Vec<_>, _>>()?;
        let events = events.into_iter().collect::<Result<Vec<_>, _>>()?;
        Ok((events, head_seq(&self.store.conn)?))
    }

    // -----------------------------------------------------------------------------------------
    // Commands
    // -----------------------------------------------------------------------------------------

    pub fn apply(
        &mut self,
        slot: Slot,
        seq: CommandSeq,
        body: CommandBody,
    ) -> Result<Applied, StorageError> {
        let tx = self
            .store
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        let last: u64 = tx.query_row(
            "SELECT last_command_seq FROM players WHERE slot = ?1",
            [slot],
            |r| get_u64(r, 0),
        )?;
        if seq <= last {
            // Replay of a command we already applied: same answer, no second effect.
            let stored: Vec<u8> = tx
                .query_row(
                    "SELECT result FROM commands WHERE slot = ?1 AND seq = ?2",
                    params![slot, to_db(seq)],
                    |r| r.get(0),
                )
                .optional()?
                .ok_or_else(|| {
                    StorageError::Corrupt(format!("no stored result for slot {slot} command {seq}"))
                })?;
            return Ok(Applied::plain(postcard::from_bytes(&stored)?));
        }
        if seq != last + 1 {
            return Ok(Applied::rejected(Rejection::OutOfOrder {
                expected: last + 1,
            }));
        }

        let applied = execute(&tx, slot, body)?;
        tx.execute(
            "INSERT INTO commands (slot, seq, result) VALUES (?1, ?2, ?3)",
            params![slot, to_db(seq), postcard::to_stdvec(&applied.result)?],
        )?;
        tx.execute(
            "UPDATE players SET last_command_seq = ?2 WHERE slot = ?1",
            params![slot, to_db(seq)],
        )?;
        tx.commit()?;
        self.dirty = true;
        Ok(applied)
    }

    // -----------------------------------------------------------------------------------------
    // End of the run (D12)
    // -----------------------------------------------------------------------------------------

    /// Close the run for good: write the terminal marker, then delete the active state.
    ///
    /// One transaction, so a crash leaves either the full run or the marker with nothing
    /// behind it. The marker closes the run id: [`Store::open`] refuses it afterwards, and an
    /// old copy of the database held by a client cannot revive it because clients never hold
    /// authoritative state. Called by the game-over logic (stage 8), only when no player is
    /// alive, not when none is connected.
    pub fn end_run(&mut self, reason: &str) -> Result<(), StorageError> {
        let tx = self
            .store
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "INSERT OR IGNORE INTO terminal (run_id, ended_at, reason) VALUES (?1, ?2, ?3)",
            params![self.info.run_id.0.as_slice(), unix_now(), reason],
        )?;
        tx.execute_batch(
            "DELETE FROM commands; DELETE FROM journal; DELETE FROM objects; DELETE FROM players;",
        )?;
        tx.commit()?;
        self.ended = true;
        self.store.compact()?;
        Ok(())
    }

    pub fn is_ended(&self) -> bool {
        self.ended
    }

    // -----------------------------------------------------------------------------------------
    // Invariants
    // -----------------------------------------------------------------------------------------

    /// Cross-check the derived state against the journal. Returns the violations found.
    ///
    /// `wallet_i = sum(shared income) - sum(spent_i)` (SYNC_CONTRACTS §5), every claimed object
    /// has exactly one matching event, and the command counter equals the stored results.
    pub fn check_consistency(&self) -> Result<Vec<String>, StorageError> {
        let conn = &self.store.conn;
        let mut problems = Vec::new();

        let mut income: i64 = 0;
        let mut spent: std::collections::HashMap<Slot, i64> = Default::default();
        let mut claims: std::collections::HashMap<ObjectId, (EventSeq, Slot)> = Default::default();
        let mut events = Vec::new();
        let mut after = 0;
        loop {
            let (page, _) = self.journal_after(after, MAX_JOURNAL_PAGE)?;
            let Some(last) = page.last() else { break };
            after = last.seq;
            events.extend(page);
        }
        for ev in &events {
            match &ev.body {
                EventBody::GoldIncome {
                    object_id,
                    base_amount,
                    claimed_by,
                } => {
                    income += base_amount;
                    claims.insert(*object_id, (ev.seq, *claimed_by));
                }
                EventBody::OrbTaken {
                    object_id,
                    claimed_by,
                    ..
                }
                | EventBody::HeartTaken {
                    object_id,
                    claimed_by,
                } => {
                    claims.insert(*object_id, (ev.seq, *claimed_by));
                }
                EventBody::Spent { slot, amount } => {
                    *spent.entry(*slot).or_default() += amount;
                }
                EventBody::ObjectRegistered { .. } => {}
            }
        }

        let mut stmt = conn.prepare("SELECT slot, wallet, last_command_seq FROM players")?;
        let players = stmt
            .query_map([], |r| {
                Ok((r.get::<_, Slot>(0)?, r.get::<_, i64>(1)?, get_u64(r, 2)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (slot, wallet, last_command_seq) in players {
            let expected = income - spent.get(&slot).copied().unwrap_or(0);
            if wallet != expected {
                problems.push(format!(
                    "slot {slot}: wallet {wallet} != journal-derived {expected}"
                ));
            }
            let stored: u64 = conn.query_row(
                "SELECT COALESCE(MAX(seq), 0) FROM commands WHERE slot = ?1",
                [slot],
                |r| get_u64(r, 0),
            )?;
            if stored != last_command_seq {
                problems.push(format!(
                    "slot {slot}: last_command_seq {last_command_seq} != stored results {stored}"
                ));
            }
        }

        let mut stmt = conn.prepare(
            "SELECT object_id, taken_by, taken_event_seq FROM objects WHERE taken_by IS NOT NULL",
        )?;
        let taken = stmt
            .query_map([], |r| {
                Ok((from_db(r.get(0)?), r.get::<_, Slot>(1)?, get_u64(r, 2)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        if taken.len() != claims.len() {
            problems.push(format!(
                "{} objects marked taken but {} claim events in the journal",
                taken.len(),
                claims.len()
            ));
        }
        for (object_id, by, event_seq) in taken {
            if claims.get(&object_id) != Some(&(event_seq, by)) {
                problems.push(format!(
                    "object {object_id}: taken state and journal disagree"
                ));
            }
        }
        Ok(problems)
    }
}

// ---------------------------------------------------------------------------------------------
// Command execution (inside the transaction)
// ---------------------------------------------------------------------------------------------

fn head_seq(conn: &rusqlite::Connection) -> Result<EventSeq, rusqlite::Error> {
    conn.query_row("SELECT COALESCE(MAX(seq), 0) FROM journal", [], |r| {
        get_u64(r, 0)
    })
}

fn read_event(r: &rusqlite::Row<'_>) -> rusqlite::Result<Result<JournalEvent, StorageError>> {
    let body: Vec<u8> = r.get(3)?;
    let (seq, origin, time) = (get_u64(r, 0)?, r.get(1)?, r.get(2)?);
    Ok(postcard::from_bytes(&body)
        .map(|body| JournalEvent {
            seq,
            origin,
            time,
            body,
        })
        .map_err(StorageError::from))
}

fn insert_event(
    tx: &Transaction<'_>,
    origin: Slot,
    body: EventBody,
) -> Result<JournalEvent, StorageError> {
    let time = unix_now();
    tx.execute(
        "INSERT INTO journal (origin, time, body) VALUES (?1, ?2, ?3)",
        params![origin, time, postcard::to_stdvec(&body)?],
    )?;
    Ok(JournalEvent {
        seq: tx.last_insert_rowid() as EventSeq,
        origin,
        time,
        body,
    })
}

fn applied_with_event(event: JournalEvent, wallets: Vec<(Slot, i64)>) -> Applied {
    Applied {
        result: CommandResult::Applied {
            event_seq: Some(event.seq),
        },
        event: Some(event),
        wallets,
    }
}

fn execute(tx: &Transaction<'_>, slot: Slot, body: CommandBody) -> Result<Applied, StorageError> {
    match body {
        CommandBody::RegisterObject { object_id, kind } => register(tx, slot, object_id, kind),
        CommandBody::ClaimPickup { object_id } => claim(tx, slot, object_id),
        CommandBody::Spend { amount } => spend(tx, slot, amount),
        CommandBody::UploadPlayerSnapshot {
            snapshot_seq,
            applied_event_seq,
            blob,
        } => upload_snapshot(tx, slot, snapshot_seq, applied_event_seq, &blob),
    }
}

fn register(
    tx: &Transaction<'_>,
    slot: Slot,
    object_id: ObjectId,
    kind: ObjectKind,
) -> Result<Applied, StorageError> {
    if let ObjectKind::GoldNugget { amount } = kind
        && !(1..=MAX_NUGGET_AMOUNT).contains(&amount)
    {
        return Ok(Applied::rejected(Rejection::InvalidAmount));
    }
    let inserted = tx.execute(
        "INSERT OR IGNORE INTO objects (object_id, kind) VALUES (?1, ?2)",
        params![to_db(object_id), postcard::to_stdvec(&kind)?],
    )?;
    if inserted == 0 {
        return Ok(Applied::rejected(Rejection::ObjectAlreadyRegistered));
    }
    let event = insert_event(tx, slot, EventBody::ObjectRegistered { object_id, kind })?;
    Ok(applied_with_event(event, Vec::new()))
}

fn claim(tx: &Transaction<'_>, slot: Slot, object_id: ObjectId) -> Result<Applied, StorageError> {
    let row: Option<(Vec<u8>, Option<Slot>)> = tx
        .query_row(
            "SELECT kind, taken_by FROM objects WHERE object_id = ?1",
            [to_db(object_id)],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((kind, taken_by)) = row else {
        return Ok(Applied::rejected(Rejection::UnknownObject));
    };
    if let Some(by) = taken_by {
        return Ok(Applied::rejected(Rejection::AlreadyTaken { by }));
    }
    let kind: ObjectKind = postcard::from_bytes(&kind)?;

    // Nothing is written before the last check that can refuse: a rejected command must leave
    // no partial effect behind.
    let mut wallets = Vec::new();
    let body = match kind {
        ObjectKind::GoldNugget { amount } => {
            // D4: one physical nugget, income for every player (also offline and dead ones).
            let richest: i64 = tx.query_row("SELECT MAX(wallet) FROM players", [], |r| r.get(0))?;
            if richest.checked_add(amount).is_none() {
                return Ok(Applied::rejected(Rejection::WalletOverflow));
            }
            tx.execute("UPDATE players SET wallet = wallet + ?1", [amount])?;
            let mut stmt = tx.prepare("SELECT slot, wallet FROM players ORDER BY slot")?;
            wallets = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            EventBody::GoldIncome {
                object_id,
                base_amount: amount,
                claimed_by: slot,
            }
        }
        ObjectKind::Orb { orb_id } => EventBody::OrbTaken {
            object_id,
            orb_id,
            claimed_by: slot,
        },
        ObjectKind::Heart => EventBody::HeartTaken {
            object_id,
            claimed_by: slot,
        },
    };
    let event = insert_event(tx, slot, body)?;
    tx.execute(
        "UPDATE objects SET taken_by = ?2, taken_event_seq = ?3 WHERE object_id = ?1",
        params![to_db(object_id), slot, to_db(event.seq)],
    )?;
    Ok(applied_with_event(event, wallets))
}

fn spend(tx: &Transaction<'_>, slot: Slot, amount: i64) -> Result<Applied, StorageError> {
    if amount <= 0 {
        // A negative spend must never turn into income.
        return Ok(Applied::rejected(Rejection::InvalidAmount));
    }
    let balance: i64 = tx.query_row("SELECT wallet FROM players WHERE slot = ?1", [slot], |r| {
        r.get(0)
    })?;
    if balance < amount {
        return Ok(Applied::rejected(Rejection::InsufficientFunds { balance }));
    }
    tx.execute(
        "UPDATE players SET wallet = wallet - ?2 WHERE slot = ?1",
        params![slot, amount],
    )?;
    let event = insert_event(tx, slot, EventBody::Spent { slot, amount })?;
    Ok(applied_with_event(event, vec![(slot, balance - amount)]))
}

fn upload_snapshot(
    tx: &Transaction<'_>,
    slot: Slot,
    snapshot_seq: u64,
    applied_event_seq: EventSeq,
    blob: &[u8],
) -> Result<Applied, StorageError> {
    if blob.len() > MAX_SNAPSHOT_LEN {
        return Ok(Applied::rejected(Rejection::SnapshotTooLarge));
    }
    let head = head_seq(tx)?;
    if applied_event_seq > head {
        return Ok(Applied::rejected(Rejection::CursorAhead { head }));
    }
    let stored: u64 = tx.query_row(
        "SELECT snapshot_seq FROM players WHERE slot = ?1",
        [slot],
        |r| get_u64(r, 0),
    )?;
    if snapshot_seq <= stored {
        return Ok(Applied::rejected(Rejection::StaleSnapshot { stored }));
    }
    // The wallet column is deliberately not touched: a snapshot can never set a balance.
    tx.execute(
        "UPDATE players SET snapshot_seq = ?2, snapshot_applied_seq = ?3,
                            snapshot_crc = ?4, snapshot = ?5
         WHERE slot = ?1",
        params![
            slot,
            to_db(snapshot_seq),
            to_db(applied_event_seq),
            crc32fast::hash(blob),
            blob
        ],
    )?;
    Ok(Applied::plain(CommandResult::Applied { event_seq: None }))
}
