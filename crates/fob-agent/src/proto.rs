//! SSH agent protocol wire format (draft-miller-ssh-agent), the de facto
//! standard OpenSSH implements. Messages on the wire are:
//!
//!   uint32 length   (of everything that follows)
//!   byte   type
//!   ...    type-specific fields
//!
//! Fields inside a message are plain SSH wire-format primitives: `uint32`
//! and length-prefixed `string` (byte string, not necessarily UTF-8).
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub const SSH_AGENT_FAILURE: u8 = 5;
pub const SSH_AGENT_SUCCESS: u8 = 6;
pub const SSH_AGENTC_REQUEST_IDENTITIES: u8 = 11;
pub const SSH_AGENT_IDENTITIES_ANSWER: u8 = 12;
pub const SSH_AGENTC_SIGN_REQUEST: u8 = 13;
pub const SSH_AGENT_SIGN_RESPONSE: u8 = 14;
pub const SSH_AGENTC_REMOVE_ALL_IDENTITIES: u8 = 19;

/// Refuse to allocate more than this for a single incoming message — a
/// generous bound for key lists and sign requests, small enough to bound a
/// malformed or hostile peer's ability to make us allocate.
const MAX_MESSAGE_LEN: usize = 256 * 1024;

/// Read one length-prefixed message body (the 4-byte length header is
/// consumed but not included in the returned bytes).
pub async fn read_message(stream: &mut (impl AsyncRead + Unpin)) -> std::io::Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_MESSAGE_LEN {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "message too large",
        ));
    }
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).await?;
    Ok(buf)
}

/// Write one length-prefixed message body.
pub async fn write_message(
    stream: &mut (impl AsyncWrite + Unpin),
    body: &[u8],
) -> std::io::Result<()> {
    stream.write_all(&(body.len() as u32).to_be_bytes()).await?;
    stream.write_all(body).await?;
    stream.flush().await
}

/// Sequential reader for SSH wire-format fields within a message body.
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn read_u8(&mut self) -> Option<u8> {
        let b = *self.data.get(self.pos)?;
        self.pos += 1;
        Some(b)
    }

    /// Read (and discard) a `uint32` field — used for flags we don't act on.
    pub fn read_u32(&mut self) -> Option<u32> {
        let bytes = self.data.get(self.pos..self.pos + 4)?;
        self.pos += 4;
        Some(u32::from_be_bytes(bytes.try_into().unwrap()))
    }

    pub fn read_string(&mut self) -> Option<&'a [u8]> {
        let len = self.read_u32()? as usize;
        let bytes = self.data.get(self.pos..self.pos + len)?;
        self.pos += len;
        Some(bytes)
    }
}

pub fn write_string(out: &mut Vec<u8>, s: &[u8]) {
    out.extend_from_slice(&(s.len() as u32).to_be_bytes());
    out.extend_from_slice(s);
}

pub fn write_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_be_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reader_round_trips_string_and_u32() {
        let mut body = Vec::new();
        write_u32(&mut body, 3);
        write_string(&mut body, b"abc");
        write_string(&mut body, b"");

        let mut r = Reader::new(&body);
        assert_eq!(r.read_u32(), Some(3));
        assert_eq!(r.read_string(), Some(&b"abc"[..]));
        assert_eq!(r.read_string(), Some(&b""[..]));
        assert_eq!(r.read_string(), None);
    }

    #[test]
    fn reader_rejects_truncated_string_length() {
        let mut body = Vec::new();
        write_u32(&mut body, 10); // claims 10 bytes follow
        body.extend_from_slice(b"abc"); // only 3 actually present
        let mut r = Reader::new(&body);
        assert_eq!(r.read_string(), None);
    }

    #[tokio::test]
    async fn message_round_trips_over_a_duplex_stream() {
        let (mut a, mut b) = tokio::io::duplex(4096);
        write_message(&mut a, b"hello").await.unwrap();
        let got = read_message(&mut b).await.unwrap();
        assert_eq!(got, b"hello");
    }

    #[tokio::test]
    async fn oversized_length_is_rejected() {
        let (mut a, mut b) = tokio::io::duplex(64);
        a.write_all(&(MAX_MESSAGE_LEN as u32 + 1).to_be_bytes())
            .await
            .unwrap();
        let err = read_message(&mut b).await.unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }
}
