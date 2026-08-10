//! The link between two gateways.
//!
//! The gateway only ever sees frames: a payload plus the channel that says
//! which of the two LAN ports the datagram entered by, which is the same port
//! the peer gateway must deliver it to on the other side. How a frame reaches
//! the peer is the transport's business, so the length prefix below lives with
//! the TCP transport that needs it — a datagram loses its own boundaries inside
//! a byte stream, and a transport that preserves boundaries would not carry it.

use std::{
    io::{self, BufReader, Read, Write},
    net::{Shutdown, SocketAddr, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

use super::config::LinkRole;

const RETRY_INTERVAL: Duration = Duration::from_millis(100);
const DIAL_TIMEOUT: Duration = Duration::from_millis(200);

/// Which of the configured peer gateways a link stands for. Links are numbered
/// by their place in the configuration, which is stable for the run and needs
/// no identity handshake with the peer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct LinkId(pub(crate) usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Channel {
    Metatraffic,
    UserData,
}

impl Channel {
    fn tag(self) -> u8 {
        match self {
            Channel::Metatraffic => 0,
            Channel::UserData => 1,
        }
    }

    fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            0 => Some(Channel::Metatraffic),
            1 => Some(Channel::UserData),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Frame {
    pub(crate) channel: Channel,
    pub(crate) payload: Vec<u8>,
}

/// The end of the link this gateway owns. It outlives every connection made
/// through it, because a peer that goes away has to be able to come back.
pub(crate) trait LinkEndpoint: Send {
    /// Blocks until the peer gateway is reachable, retrying until then.
    /// `None` once shutdown is set.
    fn connect(&self, shutdown: &AtomicBool) -> Option<Arc<dyn LinkConnection>>;
}

/// An established link. The reader and the writer thread hold it at the same
/// time, so every method takes `&self` and closing is safe from either.
pub(crate) trait LinkConnection: Send + Sync {
    fn send(&self, channel: Channel, payload: &[u8]) -> io::Result<()>;
    fn recv(&self) -> io::Result<Frame>;
    /// Tears the connection down under whichever thread is blocked on it.
    fn close(&self);
}

/// Picks the transport that serves the configured role.
pub(crate) fn open_endpoint(role: &LinkRole) -> io::Result<Box<dyn LinkEndpoint>> {
    match role {
        LinkRole::Listen(address) => {
            let listener = TcpListener::bind(address)?;
            listener.set_nonblocking(true)?;
            Ok(Box::new(TcpEndpoint::Listener(listener)))
        }
        LinkRole::Connect(address) => Ok(Box::new(TcpEndpoint::Dialer(*address))),
    }
}

enum TcpEndpoint {
    Listener(TcpListener),
    Dialer(SocketAddr),
}

impl LinkEndpoint for TcpEndpoint {
    fn connect(&self, shutdown: &AtomicBool) -> Option<Arc<dyn LinkConnection>> {
        while !shutdown.load(Ordering::Relaxed) {
            let attempt = match self {
                TcpEndpoint::Listener(listener) => listener.accept().map(|(stream, _)| stream),
                TcpEndpoint::Dialer(address) => TcpStream::connect_timeout(address, DIAL_TIMEOUT),
            };
            match attempt.and_then(TcpLink::new) {
                Ok(link) => return Some(Arc::new(link)),
                Err(_) => thread::sleep(RETRY_INTERVAL),
            }
        }
        None
    }
}

/// Three handles on one connection: reads and writes are independent, and
/// closing must work while both of them are blocked.
struct TcpLink {
    writer: Mutex<TcpStream>,
    reader: Mutex<BufReader<TcpStream>>,
    control: TcpStream,
}

impl TcpLink {
    fn new(stream: TcpStream) -> io::Result<Self> {
        stream.set_nonblocking(false)?;
        let _ = stream.set_nodelay(true);
        let reader = stream.try_clone()?;
        let control = stream.try_clone()?;
        Ok(Self { writer: Mutex::new(stream), reader: Mutex::new(BufReader::new(reader)), control })
    }
}

impl LinkConnection for TcpLink {
    fn send(&self, channel: Channel, payload: &[u8]) -> io::Result<()> {
        write_frame(&mut *lock(&self.writer), channel, payload)
    }

    fn recv(&self) -> io::Result<Frame> {
        read_frame(&mut *lock(&self.reader))
    }

    fn close(&self) {
        let _ = self.control.shutdown(Shutdown::Both);
    }
}

/// Wide enough for a fragmented RTPS datagram, narrow enough that a corrupted
/// length cannot make the relay allocate without bound.
const MAX_FRAME_LEN: usize = 64 * 1024;

fn write_frame<W: Write>(writer: &mut W, channel: Channel, payload: &[u8]) -> io::Result<()> {
    if payload.len() + 1 > MAX_FRAME_LEN {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "frame too large"));
    }
    let length = (payload.len() + 1) as u32;
    writer.write_all(&length.to_be_bytes())?;
    writer.write_all(&[channel.tag()])?;
    writer.write_all(payload)?;
    writer.flush()
}

fn read_frame<R: Read>(reader: &mut R) -> io::Result<Frame> {
    let mut length_bytes = [0u8; 4];
    reader.read_exact(&mut length_bytes)?;
    let length = u32::from_be_bytes(length_bytes) as usize;
    if length == 0 || length > MAX_FRAME_LEN {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid frame length"));
    }

    let mut tag = [0u8; 1];
    reader.read_exact(&mut tag)?;
    let channel = Channel::from_tag(tag[0])
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "unknown frame channel"))?;

    let mut payload = vec![0u8; length - 1];
    reader.read_exact(&mut payload)?;

    Ok(Frame { channel, payload })
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_survive_a_round_trip() {
        let mut stream = Vec::new();
        write_frame(&mut stream, Channel::Metatraffic, b"discovery").unwrap();
        write_frame(&mut stream, Channel::UserData, b"sample").unwrap();

        let mut cursor = stream.as_slice();
        assert_eq!(
            read_frame(&mut cursor).unwrap(),
            Frame { channel: Channel::Metatraffic, payload: b"discovery".to_vec() }
        );
        assert_eq!(
            read_frame(&mut cursor).unwrap(),
            Frame { channel: Channel::UserData, payload: b"sample".to_vec() }
        );
        assert!(read_frame(&mut cursor).is_err());
    }

    #[test]
    fn oversized_length_is_rejected_without_allocating() {
        let mut stream = Vec::new();
        stream.extend_from_slice(&u32::MAX.to_be_bytes());
        stream.push(0);

        let err = read_frame(&mut stream.as_slice()).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn unknown_channel_tag_is_rejected() {
        let mut stream = Vec::new();
        stream.extend_from_slice(&2u32.to_be_bytes());
        stream.extend_from_slice(&[9, 0]);

        let err = read_frame(&mut stream.as_slice()).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }
}
