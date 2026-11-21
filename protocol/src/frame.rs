//! Length-prefixed framing: `u32` little-endian payload length, then the `postcard` payload.

use std::io::{self, Read, Write};

use serde::{Serialize, de::DeserializeOwned};

/// Largest accepted payload. Checked before allocating, so a bogus length cannot exhaust memory.
pub const MAX_FRAME_LEN: u32 = 8 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error("i/o error: {0}")]
    Io(#[from] io::Error),
    #[error("frame of {0} bytes exceeds the {MAX_FRAME_LEN} byte limit")]
    TooLarge(u64),
    #[error("malformed message: {0}")]
    Codec(#[from] postcard::Error),
}

impl FrameError {
    /// True when the peer simply went away (clean EOF, reset, broken pipe).
    pub fn is_disconnect(&self) -> bool {
        use io::ErrorKind::*;
        matches!(
            self,
            FrameError::Io(e) if matches!(
                e.kind(),
                UnexpectedEof | ConnectionReset | ConnectionAborted | BrokenPipe | NotConnected
            )
        )
    }
}

pub fn write_message<W: Write, T: Serialize>(w: &mut W, msg: &T) -> Result<(), FrameError> {
    let payload = postcard::to_stdvec(msg)?;
    let len = u32::try_from(payload.len())
        .ok()
        .filter(|len| *len <= MAX_FRAME_LEN)
        .ok_or(FrameError::TooLarge(payload.len() as u64))?;
    // One write call so concurrent writers on a shared socket cannot interleave a header and
    // a body.
    let mut buf = Vec::with_capacity(4 + payload.len());
    buf.extend_from_slice(&len.to_le_bytes());
    buf.extend_from_slice(&payload);
    w.write_all(&buf)?;
    w.flush()?;
    Ok(())
}

pub fn read_message<R: Read, T: DeserializeOwned>(r: &mut R) -> Result<T, FrameError> {
    let mut header = [0u8; 4];
    r.read_exact(&mut header)?;
    let len = u32::from_le_bytes(header);
    if len > MAX_FRAME_LEN {
        return Err(FrameError::TooLarge(u64::from(len)));
    }
    let mut payload = vec![0u8; len as usize];
    r.read_exact(&mut payload)?;
    Ok(postcard::from_bytes(&payload)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ClientMsg, CommandBody, ObjectKind};

    #[test]
    fn roundtrip_preserves_message() {
        let msg = ClientMsg::Command {
            seq: u64::MAX - 1,
            body: CommandBody::RegisterObject {
                object_id: u64::MAX,
                kind: ObjectKind::GoldNugget { amount: 200 },
            },
        };
        let mut wire = Vec::new();
        write_message(&mut wire, &msg).unwrap();
        let back: ClientMsg = read_message(&mut wire.as_slice()).unwrap();
        assert_eq!(back, msg);
    }

    #[test]
    fn oversized_length_is_refused_before_allocation() {
        let wire = (MAX_FRAME_LEN + 1).to_le_bytes();
        let err = read_message::<_, ClientMsg>(&mut wire.as_slice()).unwrap_err();
        assert!(matches!(err, FrameError::TooLarge(_)));
    }

    #[test]
    fn truncated_frame_is_a_disconnect() {
        let mut wire = Vec::new();
        write_message(&mut wire, &ClientMsg::Ping(7)).unwrap();
        wire.pop();
        let err = read_message::<_, ClientMsg>(&mut wire.as_slice()).unwrap_err();
        assert!(err.is_disconnect());
    }
}
