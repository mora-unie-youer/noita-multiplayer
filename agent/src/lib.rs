//! Client side of the run-server link.
//!
//! The agent is the process between Noita and the server (ARCHITECTURE §2). This crate is its
//! network half: handshake, reconnect, and **resend of commands the server has not acknowledged**.
//! The game-facing half (Lua ↔ agent frames) is stage 3 and builds on [`Link`].
//!
//! Exactly-once effect comes from two cooperating rules:
//!
//! * the link numbers every command and keeps it until the server's result arrives;
//! * after a reconnect it resends everything still unacknowledged under the **same** numbers,
//!   and the server answers a number it already applied with the stored result instead of
//!   applying it again (SYNC_CONTRACTS §3).
//!
//! So a command that was applied but whose acknowledgement got lost is neither lost nor doubled.
//! Commands issued while disconnected are queued and sent after the reconnect.

use std::collections::BTreeMap;
use std::io;
use std::net::{SocketAddr, TcpStream};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use noita_coop_protocol::frame::{FrameError, read_message, write_message};
use noita_coop_protocol::{
    ClientMsg, CommandBody, CommandSeq, EventSeq, Hello, RejectReason, ServerMsg, Welcome,
};

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, thiserror::Error)]
pub enum LinkError {
    #[error("i/o error: {0}")]
    Io(#[from] io::Error),
    #[error("protocol error: {0}")]
    Frame(#[from] FrameError),
    #[error("server refused the handshake: {0:?}")]
    Rejected(RejectReason),
    #[error("unexpected answer to the handshake: {0}")]
    Unexpected(String),
}

/// What [`Link::recv`] yields.
#[derive(Debug)]
pub enum LinkEvent {
    /// Any server message. `CommandResult`s have already been removed from the resend queue.
    Server(Box<ServerMsg>),
    /// The connection is gone. Call [`Link::reconnect`]; queued commands are kept.
    Disconnected,
}

pub struct Link {
    addr: SocketAddr,
    hello: Hello,
    stream: Option<TcpStream>,
    incoming: Receiver<LinkEvent>,
    next_seq: CommandSeq,
    /// Sent but not yet acknowledged, in order.
    unacked: BTreeMap<CommandSeq, CommandBody>,
}

impl Link {
    /// Connect and perform the handshake.
    pub fn connect(addr: SocketAddr, hello: Hello) -> Result<(Link, Welcome), LinkError> {
        let (stream, incoming, welcome) = open(addr, &hello)?;
        let link = Link {
            addr,
            hello,
            stream: Some(stream),
            incoming,
            next_seq: welcome.last_command_seq + 1,
            unacked: BTreeMap::new(),
        };
        Ok((link, welcome))
    }

    /// Number of commands waiting for the server's result.
    pub fn unacked(&self) -> usize {
        self.unacked.len()
    }

    pub fn is_connected(&self) -> bool {
        self.stream.is_some()
    }

    /// Queue a command and send it if connected. Returns its sequence number.
    ///
    /// A failed send is not an error for the caller: the command stays queued and goes out
    /// after [`Link::reconnect`].
    pub fn submit(&mut self, body: CommandBody) -> CommandSeq {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.unacked.insert(seq, body.clone());
        self.send(&ClientMsg::Command { seq, body });
        seq
    }

    /// Ask for journal events after `after_seq`; the answer arrives as `ServerMsg::Journal`.
    pub fn request_journal(&mut self, after_seq: EventSeq, limit: u32) {
        self.send(&ClientMsg::GetJournal { after_seq, limit });
    }

    pub fn ping(&mut self, token: u64) {
        self.send(&ClientMsg::Ping(token));
    }

    fn send(&mut self, msg: &ClientMsg) {
        if let Some(stream) = &mut self.stream
            && write_message(stream, msg).is_err()
        {
            self.stream = None;
        }
    }

    /// Next event, or `None` if nothing arrived within `timeout`.
    pub fn recv(&mut self, timeout: Duration) -> Option<LinkEvent> {
        let event = match self.incoming.recv_timeout(timeout) {
            Ok(event) => event,
            Err(RecvTimeoutError::Timeout) => return None,
            Err(RecvTimeoutError::Disconnected) => LinkEvent::Disconnected,
        };
        match &event {
            LinkEvent::Server(msg) => {
                if let ServerMsg::CommandResult { seq, .. } = msg.as_ref() {
                    self.unacked.remove(seq);
                }
            }
            LinkEvent::Disconnected => self.stream = None,
        }
        Some(event)
    }

    /// Close the connection on purpose. Queued commands stay queued.
    pub fn disconnect(&mut self) {
        if let Some(stream) = self.stream.take() {
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
    }

    /// Open a new connection, repeat the handshake and resend every unacknowledged command
    /// under its original number.
    pub fn reconnect(&mut self) -> Result<Welcome, LinkError> {
        self.disconnect();
        let (stream, incoming, welcome) = open(self.addr, &self.hello)?;
        self.stream = Some(stream);
        self.incoming = incoming;
        // The server may know more than we do (we lost our counter): never reuse a number.
        self.next_seq = self.next_seq.max(welcome.last_command_seq + 1);
        let pending: Vec<_> = self.unacked.iter().map(|(s, b)| (*s, b.clone())).collect();
        for (seq, body) in pending {
            self.send(&ClientMsg::Command { seq, body });
        }
        Ok(welcome)
    }
}

fn open(
    addr: SocketAddr,
    hello: &Hello,
) -> Result<(TcpStream, Receiver<LinkEvent>, Welcome), LinkError> {
    let mut stream = TcpStream::connect_timeout(&addr, HANDSHAKE_TIMEOUT)?;
    stream.set_nodelay(true)?;
    stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT))?;
    write_message(&mut stream, &ClientMsg::Hello(hello.clone()))?;
    let welcome = match read_message::<_, ServerMsg>(&mut stream)? {
        ServerMsg::Welcome(w) => w,
        ServerMsg::Rejected(reason) => return Err(LinkError::Rejected(reason)),
        other => return Err(LinkError::Unexpected(format!("{other:?}"))),
    };
    stream.set_read_timeout(None)?;

    let (tx, rx) = mpsc::channel();
    let mut reader = stream.try_clone()?;
    thread::Builder::new()
        .name("link-reader".into())
        .spawn(move || {
            loop {
                match read_message::<_, ServerMsg>(&mut reader) {
                    Ok(msg) => {
                        if tx.send(LinkEvent::Server(Box::new(msg))).is_err() {
                            return;
                        }
                    }
                    Err(_) => {
                        let _ = tx.send(LinkEvent::Disconnected);
                        return;
                    }
                }
            }
        })?;
    Ok((stream, rx, welcome))
}
