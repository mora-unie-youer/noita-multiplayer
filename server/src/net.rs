//! TCP front of the engine: one acceptor, a reader and a writer thread per connection, and a
//! single engine thread that owns all state.
//!
//! **Provisional transport.** Plain TCP without encryption or authentication of the server.
//! D9 requires both, and ARCHITECTURE §7 proposes QUIC with a pinned certificate; that is chosen
//! after the stage-1 spikes. The engine does not depend on this module, so replacing it later
//! changes no state logic. Until then the binary refuses non-loopback addresses unless told
//! otherwise explicitly.
//!
//! All engine calls happen on the engine thread. Messages from every connection pass through one
//! channel, so the arrival order at that channel decides every race (two claims of one orb).

use std::collections::HashMap;
use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use log::{debug, error, info, warn};
use noita_coop_protocol::frame::{read_message, write_message};
use noita_coop_protocol::{ClientMsg, CloseReason, ServerMsg, Slot};

use crate::backup;
use crate::engine::{Auth, Engine};
use crate::error::StorageError;
use crate::storage::unix_now;

/// A fresh connection must send its `Hello` within this time.
const HELLO_TIMEOUT: Duration = Duration::from_secs(10);
/// A peer that stops reading must not stall the thread forever.
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);
const ACCEPT_POLL: Duration = Duration::from_millis(20);

type ConnId = u64;

enum Inbox {
    Connected {
        id: ConnId,
        out: Sender<ServerMsg>,
        writer: JoinHandle<()>,
    },
    Message {
        id: ConnId,
        msg: ClientMsg,
    },
    Gone {
        id: ConnId,
    },
    Shutdown,
}

pub struct ServeOptions {
    /// Where to write backups; `None` disables them.
    pub backup_dir: Option<PathBuf>,
    pub backup_interval: Duration,
}

/// Cheap handle that asks a running server to stop (used by the signal handler).
#[derive(Clone)]
pub struct ShutdownHandle {
    stop: Arc<AtomicBool>,
    inbox: Sender<Inbox>,
}

impl ShutdownHandle {
    pub fn shutdown(&self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = self.inbox.send(Inbox::Shutdown);
    }
}

pub struct Server {
    local_addr: SocketAddr,
    handle: ShutdownHandle,
    engine: JoinHandle<Result<(), StorageError>>,
    acceptor: JoinHandle<()>,
}

impl Server {
    pub fn start(engine: Engine, listener: TcpListener, opts: ServeOptions) -> io::Result<Server> {
        listener.set_nonblocking(true)?;
        let local_addr = listener.local_addr()?;
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let handle = ShutdownHandle {
            stop: stop.clone(),
            inbox: tx.clone(),
        };

        let engine = thread::Builder::new()
            .name("engine".into())
            .spawn(move || engine_loop(engine, rx, opts))?;
        let acceptor = thread::Builder::new()
            .name("accept".into())
            .spawn(move || accept_loop(listener, tx, stop))?;
        Ok(Server {
            local_addr,
            handle,
            engine,
            acceptor,
        })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    pub fn shutdown_handle(&self) -> ShutdownHandle {
        self.handle.clone()
    }

    /// Block until the server stops: after a shutdown request, or after a storage failure.
    pub fn wait(self) -> Result<(), StorageError> {
        let result = self.engine.join().expect("engine thread panicked");
        self.handle.stop.store(true, Ordering::SeqCst);
        let _ = self.acceptor.join();
        result
    }

    pub fn shutdown(self) -> Result<(), StorageError> {
        self.handle.shutdown();
        self.wait()
    }
}

// ---------------------------------------------------------------------------------------------
// Connections
// ---------------------------------------------------------------------------------------------

fn accept_loop(listener: TcpListener, inbox: Sender<Inbox>, stop: Arc<AtomicBool>) {
    let next_id = AtomicU64::new(1);
    while !stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, peer)) => {
                let id = next_id.fetch_add(1, Ordering::Relaxed);
                if let Err(e) = spawn_connection(id, stream, &inbox) {
                    warn!("connection from {peer} dropped during setup: {e}");
                } else {
                    debug!("connection {id} from {peer}");
                }
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => thread::sleep(ACCEPT_POLL),
            Err(e) => {
                warn!("accept failed: {e}");
                thread::sleep(ACCEPT_POLL);
            }
        }
    }
}

fn spawn_connection(id: ConnId, stream: TcpStream, inbox: &Sender<Inbox>) -> io::Result<()> {
    stream.set_nonblocking(false)?;
    stream.set_nodelay(true)?;
    stream.set_write_timeout(Some(WRITE_TIMEOUT))?;
    let read_half = stream.try_clone()?;

    let (out, out_rx) = mpsc::channel::<ServerMsg>();
    let writer = thread::Builder::new()
        .name(format!("conn{id}-w"))
        .spawn(move || writer_loop(stream, out_rx))?;
    // Connected must reach the engine before any Message of this connection: both are sent on
    // the same channel, and the reader is started only afterwards.
    if inbox.send(Inbox::Connected { id, out, writer }).is_err() {
        return Ok(());
    }
    let inbox = inbox.clone();
    thread::Builder::new()
        .name(format!("conn{id}-r"))
        .spawn(move || reader_loop(id, read_half, inbox))?;
    Ok(())
}

/// Writes until the engine drops the sender, then closes the socket. Dropping the sender is
/// therefore the way to close a connection: queued messages are still flushed first.
fn writer_loop(mut stream: TcpStream, rx: mpsc::Receiver<ServerMsg>) {
    for msg in rx {
        if let Err(e) = write_message(&mut stream, &msg) {
            debug!("write failed: {e}");
            break;
        }
    }
    let _ = stream.shutdown(std::net::Shutdown::Both);
}

fn reader_loop(id: ConnId, mut stream: TcpStream, inbox: Sender<Inbox>) {
    let _ = stream.set_read_timeout(Some(HELLO_TIMEOUT));
    let mut first = true;
    loop {
        match read_message::<_, ClientMsg>(&mut stream) {
            Ok(msg) => {
                if first {
                    first = false;
                    let _ = stream.set_read_timeout(None);
                }
                if inbox.send(Inbox::Message { id, msg }).is_err() {
                    return;
                }
            }
            Err(e) => {
                if !e.is_disconnect() {
                    debug!("connection {id} closed: {e}");
                }
                break;
            }
        }
    }
    let _ = inbox.send(Inbox::Gone { id });
}

// ---------------------------------------------------------------------------------------------
// Engine thread
// ---------------------------------------------------------------------------------------------

struct Session {
    out: Sender<ServerMsg>,
    writer: JoinHandle<()>,
    slot: Option<Slot>,
}

struct Hub {
    engine: Engine,
    sessions: HashMap<ConnId, Session>,
    online: HashMap<Slot, ConnId>,
}

fn engine_loop(
    engine: Engine,
    rx: mpsc::Receiver<Inbox>,
    opts: ServeOptions,
) -> Result<(), StorageError> {
    let mut hub = Hub {
        engine,
        sessions: HashMap::new(),
        online: HashMap::new(),
    };
    let mut next_backup = Instant::now() + opts.backup_interval;

    let outcome = loop {
        match rx.recv_timeout(next_backup.saturating_duration_since(Instant::now())) {
            Ok(Inbox::Shutdown) => {
                info!("shutting down");
                backup_if_due(&mut hub.engine, &opts);
                break Ok(CloseReason::ServerShutdown);
            }
            Ok(msg) => {
                if let Err(e) = hub.handle(msg) {
                    error!("storage failure, stopping the server: {e}");
                    break Err(e);
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                backup_if_due(&mut hub.engine, &opts);
                next_backup = Instant::now() + opts.backup_interval;
            }
            Err(RecvTimeoutError::Disconnected) => break Ok(CloseReason::ServerShutdown),
        }
    };

    let reason = match &outcome {
        Ok(reason) => *reason,
        Err(_) => CloseReason::StorageFailure,
    };
    for session in hub.sessions.values() {
        let _ = session.out.send(ServerMsg::Closed(reason));
    }
    // Dropping the senders lets each writer flush its queue and close the socket.
    let writers: Vec<_> = hub.sessions.drain().map(|(_, s)| s.writer).collect();
    for writer in writers {
        let _ = writer.join();
    }
    outcome.map(|_| ())
}

fn backup_if_due(engine: &mut Engine, opts: &ServeOptions) {
    let Some(dir) = &opts.backup_dir else { return };
    if !engine.take_dirty() {
        return;
    }
    let now = unix_now();
    let started = Instant::now();
    match backup::write(engine.store(), dir, now) {
        Ok(path) => {
            debug!("backup {} in {:?}", path.display(), started.elapsed());
            if let Err(e) = backup::prune(dir, now, None) {
                warn!("backup rotation failed: {e}");
            }
        }
        Err(e) => {
            error!("backup failed (will retry): {e}");
            engine.mark_dirty();
        }
    }
}

impl Hub {
    fn send(&self, id: ConnId, msg: ServerMsg) {
        if let Some(s) = self.sessions.get(&id) {
            let _ = s.out.send(msg);
        }
    }

    /// Close `id` after flushing what is queued for it.
    fn close(&mut self, id: ConnId, reason: Option<CloseReason>) {
        if let Some(session) = self.sessions.remove(&id) {
            if let Some(reason) = reason {
                let _ = session.out.send(ServerMsg::Closed(reason));
            }
            if let Some(slot) = session.slot
                && self.online.get(&slot) == Some(&id)
            {
                self.online.remove(&slot);
            }
            // `session` drops here: the writer drains its queue and closes the socket.
        }
    }

    fn handle(&mut self, msg: Inbox) -> Result<(), StorageError> {
        match msg {
            Inbox::Shutdown => unreachable!("handled by the loop"),
            Inbox::Connected { id, out, writer } => {
                self.sessions.insert(
                    id,
                    Session {
                        out,
                        writer,
                        slot: None,
                    },
                );
            }
            Inbox::Gone { id } => self.close(id, None),
            Inbox::Message { id, msg } => {
                // Messages of a connection we already closed are dropped.
                let Some(slot) = self.sessions.get(&id).map(|s| s.slot) else {
                    return Ok(());
                };
                match (slot, msg) {
                    (None, ClientMsg::Hello(hello)) => self.hello(id, &hello)?,
                    (None, _) | (Some(_), ClientMsg::Hello(_)) => {
                        self.close(id, Some(CloseReason::ProtocolViolation));
                    }
                    (Some(slot), ClientMsg::Command { seq, body }) => {
                        let applied = self.engine.apply(slot, seq, body)?;
                        self.send(
                            id,
                            ServerMsg::CommandResult {
                                seq,
                                result: applied.result,
                            },
                        );
                        if let Some(event) = applied.event {
                            for (slot, balance) in applied.wallets {
                                if let Some(&conn) = self.online.get(&slot) {
                                    self.send(
                                        conn,
                                        ServerMsg::Wallet {
                                            balance,
                                            as_of_seq: event.seq,
                                        },
                                    );
                                }
                            }
                            for &conn in self.online.values() {
                                self.send(conn, ServerMsg::Event(event.clone()));
                            }
                        }
                    }
                    (Some(_), ClientMsg::GetJournal { after_seq, limit }) => {
                        let (events, head_seq) = self.engine.journal_after(after_seq, limit)?;
                        self.send(id, ServerMsg::Journal { events, head_seq });
                    }
                    (Some(_), ClientMsg::Ping(n)) => self.send(id, ServerMsg::Pong(n)),
                }
            }
        }
        Ok(())
    }

    fn hello(
        &mut self,
        id: ConnId,
        hello: &noita_coop_protocol::Hello,
    ) -> Result<(), StorageError> {
        match self.engine.authenticate(hello)? {
            Auth::Rejected(reason) => {
                info!("handshake refused: {reason:?}");
                self.send(id, ServerMsg::Rejected(reason));
                self.close(id, None);
            }
            Auth::Accepted(slot) => {
                // The newest connection of a slot wins: a stale half-open connection of a
                // crashed client must not lock the player out.
                if let Some(old) = self.online.remove(&slot) {
                    info!("slot {slot}: connection {old} superseded by {id}");
                    if let Some(s) = self.sessions.get_mut(&old) {
                        s.slot = None;
                    }
                    self.close(old, Some(CloseReason::Superseded));
                }
                let welcome = self.engine.welcome(slot)?;
                self.online.insert(slot, id);
                if let Some(s) = self.sessions.get_mut(&id) {
                    s.slot = Some(slot);
                }
                info!(
                    "slot {slot} ({}) connected",
                    self.engine.info().players[slot as usize].name
                );
                self.send(id, ServerMsg::Welcome(welcome));
            }
        }
        Ok(())
    }
}
