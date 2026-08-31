use std::collections::{HashMap, VecDeque};
use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use log::warn;
use mio::Token;

use crate::rtps::transport::tcp::connection_registry::TcpSocketTuning;
use crate::rtps::transport::tcp::framing::{
    TcpBufferPool, TcpFrame, TcpFrameKind, TcpFrameReadState, TcpReadOutcome,
};
use crate::rtps::transport::tcp::tls::TlsConfig;
use crate::rtps::transport::tokens::ListenerToken;

const FIRST_CONNECTION_TOKEN: usize = 100_000;

pub(crate) struct IncomingTcpMessage {
    pub(crate) data: bytes::Bytes,
    pub(crate) source: SocketAddr,
    pub(crate) kind: TcpFrameKind,
}

struct TcpReadConnection {
    stream: mio::net::TcpStream,
    source: SocketAddr,
    frame_state: TcpFrameReadState,
    tls: Option<rustls::ServerConnection>,
    tls_handshake_deadline: Option<Instant>,
    first_frame_deadline: Option<Instant>,
}

pub(crate) struct TcpListener {
    port: u16,
    socket: mio::net::TcpListener,
    /// What the kernel actually granted the listening socket, which every
    /// accepted stream inherits. Read back rather than assumed: the request is
    /// doubled and then clamped to the system maximum.
    recv_buffer_size: Option<usize>,
    connections: HashMap<Token, TcpReadConnection>,
    next_connection_token: usize,
    buffer_pool: Arc<TcpBufferPool>,
    ready_frames: Vec<TcpFrame>,
    ready_messages: VecDeque<IncomingTcpMessage>,
    tuning: TcpSocketTuning,
    tls_server_config: Option<Arc<rustls::ServerConfig>>,
    tls_handshake_timeout: Duration,
    first_frame_timeout: Duration,
}

impl TcpListener {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        port: u16,
        tuning: TcpSocketTuning,
        tls_config: Option<Arc<TlsConfig>>,
        tls_handshake_timeout: Duration,
        first_frame_timeout: Duration,
    ) -> io::Result<Self> {
        let tls_server_config =
            tls_config.as_ref().map(|config| config.build_server_config()).transpose()?;
        let address = SocketAddr::from((Ipv4Addr::UNSPECIFIED, port));
        let socket = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::STREAM, None)?;
        #[cfg(unix)]
        socket.set_reuse_address(true)?;
        if let Some(size) = tuning.so_rcvbuf {
            socket.set_recv_buffer_size(size)?;
        }
        socket.set_nonblocking(true)?;
        socket.bind(&address.into())?;
        socket.listen(128)?;

        let recv_buffer_size = socket.recv_buffer_size().ok();

        let std_listener = std::net::TcpListener::from(socket);
        let actual_port = std_listener.local_addr()?.port();
        let socket = mio::net::TcpListener::from_std(std_listener);

        Ok(Self {
            port: actual_port,
            socket,
            recv_buffer_size,
            connections: HashMap::new(),
            next_connection_token: FIRST_CONNECTION_TOKEN,
            buffer_pool: TcpBufferPool::new(),
            ready_frames: Vec::new(),
            ready_messages: VecDeque::new(),
            tuning,
            tls_server_config,
            tls_handshake_timeout,
            first_frame_timeout,
        })
    }

    pub(crate) fn port(&self) -> u16 {
        self.port
    }

    pub(crate) fn recv_buffer_size(&self) -> Option<usize> {
        self.recv_buffer_size
    }

    pub(crate) fn connection_count(&self) -> usize {
        self.connections.len()
    }

    pub(crate) fn register(&mut self, registry: &mio::Registry) -> io::Result<Token> {
        let token = ListenerToken::Tcp(self.port).to_mio();
        registry.register(&mut self.socket, token, mio::Interest::READABLE)?;
        Ok(token)
    }

    pub(crate) fn accept_ready(&mut self, registry: &mio::Registry) -> io::Result<usize> {
        let mut accepted_count = 0;
        loop {
            let (mut stream, source) = match self.socket.accept() {
                Ok(accepted) => accepted,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    return Ok(accepted_count);
                }
                Err(error) if error.kind() == io::ErrorKind::ConnectionAborted => {
                    warn!("TCP accept dropped a pending connection: {error}");
                    continue;
                }
                Err(error) => {
                    warn!("TCP accept failed: {error}");
                    return Ok(accepted_count);
                }
            };

            let _ = stream.set_nodelay(self.tuning.nodelay);
            if let Some(size) = self.tuning.so_rcvbuf {
                let _ = socket2::SockRef::from(&stream).set_recv_buffer_size(size);
            }
            if let Some(size) = self.tuning.so_sndbuf {
                let _ = socket2::SockRef::from(&stream).set_send_buffer_size(size);
            }
            if let Some(timeout) = self.tuning.unacked_timeout {
                #[cfg(any(target_os = "linux", target_os = "android"))]
                {
                    let _ = socket2::SockRef::from(&stream).set_tcp_user_timeout(Some(timeout));
                }
                #[cfg(target_os = "macos")]
                {
                    use std::os::unix::io::AsRawFd;

                    const TCP_RXT_CONNDROPTIME: libc::c_int = 0x80;
                    let seconds = timeout.as_secs().max(1) as libc::c_int;
                    unsafe {
                        libc::setsockopt(
                            stream.as_raw_fd(),
                            libc::IPPROTO_TCP,
                            TCP_RXT_CONNDROPTIME,
                            &seconds as *const _ as *const libc::c_void,
                            std::mem::size_of::<libc::c_int>() as libc::socklen_t,
                        );
                    }
                }
                #[cfg(not(any(target_os = "linux", target_os = "android", target_os = "macos")))]
                {
                    let _ = timeout;
                }
            }
            if let Some(keepalive) = self.tuning.keepalive {
                #[cfg(any(
                    target_os = "linux",
                    target_os = "android",
                    target_os = "macos",
                    target_os = "windows"
                ))]
                {
                    let mut params = socket2::TcpKeepalive::new()
                        .with_time(keepalive.time)
                        .with_interval(keepalive.interval);
                    #[cfg(any(target_os = "linux", target_os = "android", target_os = "macos"))]
                    {
                        params = params.with_retries(keepalive.retries);
                    }
                    let _ = socket2::SockRef::from(&stream).set_tcp_keepalive(&params);
                }
                #[cfg(not(any(
                    target_os = "linux",
                    target_os = "android",
                    target_os = "macos",
                    target_os = "windows"
                )))]
                {
                    let _ = keepalive;
                }
            }

            let now = Instant::now();
            let tls = match &self.tls_server_config {
                Some(config) => match rustls::ServerConnection::new(Arc::clone(config)) {
                    Ok(session) => Some(session),
                    Err(error) => {
                        warn!("TCP TLS session setup for {source} failed: {error}");
                        continue;
                    }
                },
                None => None,
            };
            let interest = if tls.is_some() {
                mio::Interest::READABLE.add(mio::Interest::WRITABLE)
            } else {
                mio::Interest::READABLE
            };
            let token = Token(self.next_connection_token);
            self.next_connection_token += 1;
            if let Err(error) = registry.register(&mut stream, token, interest) {
                warn!("TCP connection from {source} could not be registered: {error}");
                continue;
            }

            let tls_handshake_deadline = tls.as_ref().map(|_| now + self.tls_handshake_timeout);
            let first_frame_deadline = tls.is_none().then_some(now + self.first_frame_timeout);
            self.connections.insert(
                token,
                TcpReadConnection {
                    stream,
                    source,
                    frame_state: TcpFrameReadState::default(),
                    tls,
                    tls_handshake_deadline,
                    first_frame_deadline,
                },
            );
            accepted_count += 1;
        }
    }

    pub(crate) fn get_message(
        &mut self,
        token: Token,
        registry: &mio::Registry,
    ) -> io::Result<Option<IncomingTcpMessage>> {
        if let Some(message) = self.ready_messages.pop_front() {
            return Ok(Some(message));
        }

        self.ready_frames.clear();
        let now = Instant::now();
        let pool = Arc::clone(&self.buffer_pool);
        let first_frame_timeout = self.first_frame_timeout;
        let Some(connection) = self.connections.get_mut(&token) else {
            return Ok(None);
        };
        let source = connection.source;

        let read_result =
            if connection.tls_handshake_deadline.is_some_and(|deadline| deadline <= now) {
                Err(io::Error::new(io::ErrorKind::TimedOut, "TLS handshake timed out"))
            } else if connection.first_frame_deadline.is_some_and(|deadline| deadline <= now) {
                Err(io::Error::new(io::ErrorKind::TimedOut, "first TCP frame timed out"))
            } else {
                match connection.tls.as_mut() {
                    Some(tls) => {
                        let was_handshaking = tls.is_handshaking();
                        let result = {
                            let mut stream = rustls::Stream::new(tls, &mut connection.stream);
                            connection.frame_state.read_available(
                                &mut stream,
                                &pool,
                                &mut self.ready_frames,
                            )
                        };
                        if was_handshaking && !tls.is_handshaking() {
                            connection.tls_handshake_deadline = None;
                            connection.first_frame_deadline = Some(now + first_frame_timeout);
                            let interest = if tls.wants_write() {
                                mio::Interest::READABLE.add(mio::Interest::WRITABLE)
                            } else {
                                mio::Interest::READABLE
                            };
                            match registry.reregister(&mut connection.stream, token, interest) {
                                Ok(()) => result,
                                Err(error) => Err(error),
                            }
                        } else {
                            result
                        }
                    }
                    None => connection.frame_state.read_available(
                        &mut connection.stream,
                        &pool,
                        &mut self.ready_frames,
                    ),
                }
            };

        if !self.ready_frames.is_empty() {
            connection.first_frame_deadline = None;
        }
        let terminal = matches!(read_result, Ok(TcpReadOutcome::Closed) | Err(_));
        for frame in self.ready_frames.drain(..) {
            self.ready_messages.push_back(IncomingTcpMessage {
                data: frame.payload,
                source,
                kind: frame.kind,
            });
        }
        if terminal {
            if let Some(mut connection) = self.connections.remove(&token) {
                let _ = registry.deregister(&mut connection.stream);
            }
        }

        if let Some(message) = self.ready_messages.pop_front() {
            return Ok(Some(message));
        }
        match read_result {
            Ok(TcpReadOutcome::WouldBlock | TcpReadOutcome::Closed) => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub(crate) fn expire_connections(&mut self, registry: &mio::Registry, now: Instant) -> usize {
        let expired_tokens: Vec<_> = self
            .connections
            .iter()
            .filter_map(|(token, connection)| {
                let expired =
                    connection.tls_handshake_deadline.is_some_and(|deadline| deadline <= now)
                        || connection.first_frame_deadline.is_some_and(|deadline| deadline <= now);
                expired.then_some(*token)
            })
            .collect();

        for token in &expired_tokens {
            if let Some(mut connection) = self.connections.remove(token) {
                let _ = registry.deregister(&mut connection.stream);
            }
        }
        expired_tokens.len()
    }

    pub(crate) fn deregister_all(&mut self, registry: &mio::Registry) -> io::Result<()> {
        let mut first_error = None;
        for (_, mut connection) in self.connections.drain() {
            if let Err(error) = registry.deregister(&mut connection.stream) {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
        if let Err(error) = registry.deregister(&mut self.socket) {
            if first_error.is_none() {
                first_error = Some(error);
            }
        }
        self.ready_frames.clear();
        self.ready_messages.clear();

        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::io::Write;
    use std::net::Shutdown;

    use mio::{Events, Poll};

    use crate::rtps::transport::tcp::framing::{test_framed, test_message};
    use crate::rtps::transport::tcp::sync_connection::{OutboundConnection, SendOutcome};

    /// The advertised window is built from this, so it must be what the kernel
    /// granted rather than what was asked for. An accepted stream inherits the
    /// listening socket's buffer, so the listener speaks for every connection.
    #[test]
    fn recv_buffer_size_reports_the_kernel_grant_not_the_request() {
        const REQUESTED: usize = 512 * 1024;
        let tuning = TcpSocketTuning { so_rcvbuf: Some(REQUESTED), ..TcpSocketTuning::default() };
        let listener =
            TcpListener::new(0, tuning, None, Duration::from_secs(1), Duration::from_secs(1))
                .unwrap();
        let granted = listener.recv_buffer_size().expect("the kernel reported a value");

        // Independent oracle: the same request hand-rolled on a throwaway socket, so a broken
        // capture cannot also break the comparison. Where the kernel clamps to a system maximum
        // both land on the clamp, and where it does not both land on the same grant.
        let oracle = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::STREAM, None)
            .and_then(|socket| {
                socket.set_recv_buffer_size(REQUESTED)?;
                socket.recv_buffer_size()
            })
            .expect("oracle socket");
        assert_eq!(granted, oracle);

        drop(listener);
    }

    /// A default-tuned listener still has to report something, or every peer it
    /// talks to falls back to the floor.
    #[test]
    fn recv_buffer_size_is_reported_without_any_tuning() {
        let listener = TcpListener::new(
            0,
            TcpSocketTuning::default(),
            None,
            Duration::from_secs(1),
            Duration::from_secs(1),
        )
        .unwrap();
        assert!(listener.recv_buffer_size().is_some_and(|size| size > 0));

        drop(listener);
    }

    #[test]
    fn bind_creates_a_pollable_nonblocking_listener() {
        let mut listener = TcpListener::new(
            0,
            TcpSocketTuning::default(),
            None,
            Duration::from_secs(1),
            Duration::from_secs(1),
        )
        .unwrap();
        assert_ne!(listener.port, 0);
        assert!(listener.connections.is_empty());
        assert!(listener.ready_frames.is_empty());
        assert!(listener.ready_messages.is_empty());

        let mut poll = Poll::new().unwrap();
        let token = listener.register(poll.registry()).unwrap();

        let client =
            std::net::TcpStream::connect(SocketAddr::from((Ipv4Addr::LOCALHOST, listener.port)))
                .unwrap();
        let mut events = Events::with_capacity(4);
        poll.poll(&mut events, Some(Duration::from_secs(1))).unwrap();
        assert!(events.iter().any(|event| event.token() == token && event.is_readable()));

        let (accepted, source) = listener.socket.accept().unwrap();
        assert_eq!(source.ip(), Ipv4Addr::LOCALHOST);

        listener.deregister_all(poll.registry()).unwrap();
        let _ = client.shutdown(Shutdown::Both);
        drop(accepted);
        drop(client);
        drop(listener);
        drop(poll);
    }

    #[test]
    fn accept_ready_drains_and_registers_each_connection() {
        let mut listener = TcpListener::new(
            0,
            TcpSocketTuning::default(),
            None,
            Duration::from_secs(1),
            Duration::from_secs(1),
        )
        .unwrap();
        let mut poll = Poll::new().unwrap();
        let listener_token = listener.register(poll.registry()).unwrap();

        let address = SocketAddr::from((Ipv4Addr::LOCALHOST, listener.port));
        let mut clients: Vec<_> =
            (0..3).map(|_| std::net::TcpStream::connect(address).unwrap()).collect();
        let mut events = Events::with_capacity(8);
        poll.poll(&mut events, Some(Duration::from_secs(1))).unwrap();
        assert!(events.iter().any(|event| event.token() == listener_token));

        assert_eq!(listener.accept_ready(poll.registry()).unwrap(), 3);
        assert_eq!(listener.accept_ready(poll.registry()).unwrap(), 0);
        assert_eq!(listener.connections.len(), 3);
        assert_eq!(listener.next_connection_token, FIRST_CONNECTION_TOKEN + 3);

        let expected_tokens: HashSet<_> =
            (FIRST_CONNECTION_TOKEN..FIRST_CONNECTION_TOKEN + 3).map(Token).collect();
        let actual_tokens: HashSet<_> = listener.connections.keys().copied().collect();
        assert_eq!(actual_tokens, expected_tokens);
        for connection in listener.connections.values() {
            assert_eq!(connection.source.ip(), Ipv4Addr::LOCALHOST);
            assert!(connection.tls.is_none());
            assert!(connection.tls_handshake_deadline.is_none());
            assert!(connection.first_frame_deadline.is_some());
            assert!(connection.stream.nodelay().unwrap());
        }

        for client in &mut clients {
            client.write_all(b"x").unwrap();
        }
        let deadline = Instant::now() + Duration::from_secs(1);
        let mut readable_tokens = HashSet::new();
        while readable_tokens != expected_tokens {
            assert!(Instant::now() < deadline, "timed out waiting for connection readiness");
            events.clear();
            poll.poll(&mut events, Some(Duration::from_millis(10))).unwrap();
            for event in &events {
                if event.is_readable() && expected_tokens.contains(&event.token()) {
                    readable_tokens.insert(event.token());
                }
            }
        }

        listener.deregister_all(poll.registry()).unwrap();
        for client in clients {
            let _ = client.shutdown(Shutdown::Both);
        }
        drop(listener);
        drop(poll);
    }

    #[test]
    fn get_message_resumes_a_partial_frame_and_drains_complete_frames() {
        const BUILTIN_WRITER: u8 = 0xC2;
        const USER_WRITER: u8 = 0x02;

        let mut listener = TcpListener::new(
            0,
            TcpSocketTuning::default(),
            None,
            Duration::from_secs(1),
            Duration::from_secs(1),
        )
        .unwrap();
        let mut poll = Poll::new().unwrap();
        let listener_token = listener.register(poll.registry()).unwrap();

        let address = SocketAddr::from((Ipv4Addr::LOCALHOST, listener.port));
        let mut client = std::net::TcpStream::connect(address).unwrap();
        let source = client.local_addr().unwrap();
        let mut events = Events::with_capacity(4);
        poll.poll(&mut events, Some(Duration::from_secs(1))).unwrap();
        assert_eq!(listener.accept_ready(poll.registry()).unwrap(), 1);
        let connection_token = Token(FIRST_CONNECTION_TOKEN);

        let discovery = test_framed(&test_message(BUILTIN_WRITER, b"discovery"));
        let user = test_framed(&test_message(USER_WRITER, b"user"));
        client.write_all(&discovery[..10]).unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            assert!(Instant::now() < deadline, "timed out waiting for partial frame");
            events.clear();
            poll.poll(&mut events, Some(Duration::from_millis(10))).unwrap();
            if events.iter().any(|event| event.token() == connection_token && event.is_readable()) {
                break;
            }
        }
        assert!(listener.get_message(connection_token, poll.registry()).unwrap().is_none());

        client.write_all(&discovery[10..]).unwrap();
        client.write_all(&user).unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        let first = loop {
            assert!(Instant::now() < deadline, "timed out waiting for complete frames");
            events.clear();
            poll.poll(&mut events, Some(Duration::from_millis(10))).unwrap();
            if events.iter().any(|event| event.token() == connection_token && event.is_readable()) {
                if let Some(message) =
                    listener.get_message(connection_token, poll.registry()).unwrap()
                {
                    break message;
                }
            }
        };
        let second = listener.get_message(connection_token, poll.registry()).unwrap().unwrap();

        assert_eq!(first.source, source);
        assert_eq!(first.kind, TcpFrameKind::Discovery);
        assert_eq!(first.data.as_ref(), discovery.as_slice());
        assert_eq!(second.source, source);
        assert_eq!(second.kind, TcpFrameKind::UserData);
        assert_eq!(second.data.as_ref(), user.as_slice());
        assert!(listener.get_message(connection_token, poll.registry()).unwrap().is_none());

        listener.deregister_all(poll.registry()).unwrap();
        let _ = client.shutdown(Shutdown::Both);
        drop(client);
        drop(listener);
        drop(poll);
    }

    #[test]
    fn expire_connections_removes_a_silent_connection() {
        let mut listener = TcpListener::new(
            0,
            TcpSocketTuning::default(),
            None,
            Duration::from_secs(1),
            Duration::ZERO,
        )
        .unwrap();
        let mut poll = Poll::new().unwrap();
        let listener_token = listener.register(poll.registry()).unwrap();

        let address = SocketAddr::from((Ipv4Addr::LOCALHOST, listener.port));
        let client = std::net::TcpStream::connect(address).unwrap();
        let mut events = Events::with_capacity(4);
        poll.poll(&mut events, Some(Duration::from_secs(1))).unwrap();
        assert!(events.iter().any(|event| event.token() == listener_token));
        assert_eq!(listener.accept_ready(poll.registry()).unwrap(), 1);
        assert_eq!(listener.connections.len(), 1);

        assert_eq!(listener.expire_connections(poll.registry(), Instant::now()), 1);
        assert!(listener.connections.is_empty());
        assert_eq!(listener.expire_connections(poll.registry(), Instant::now()), 0);

        listener.deregister_all(poll.registry()).unwrap();
        let _ = client.shutdown(Shutdown::Both);
        drop(client);
        drop(listener);
        drop(poll);
    }

    #[test]
    fn tls_roundtrip_from_a_sync_outbound_connection() {
        const BUILTIN_WRITER: u8 = 0xC2;

        let pem_dir = tempfile::tempdir().unwrap();
        let tls_config = Arc::new(self_signed_tls_config(pem_dir.path()));

        let mut listener = TcpListener::new(
            0,
            TcpSocketTuning::default(),
            Some(Arc::clone(&tls_config)),
            Duration::from_secs(5),
            Duration::from_secs(5),
        )
        .unwrap();
        let mut poll = Poll::new().unwrap();
        let listener_token = listener.register(poll.registry()).unwrap();
        let address = SocketAddr::from((Ipv4Addr::LOCALHOST, listener.port));

        let frame = test_framed(&test_message(BUILTIN_WRITER, b"tls-discovery"));
        let client_frame = frame.clone();
        let client_config = Arc::clone(&tls_config);
        // The server half of the handshake only advances while the poll loop
        // below runs, so the client cannot share this thread.
        let client = std::thread::spawn(move || {
            let connection = OutboundConnection::connect(
                address,
                &TcpSocketTuning::default(),
                Some(client_config.as_ref()),
                Duration::from_secs(5),
                Duration::from_secs(5),
            )
            .unwrap();
            assert_eq!(connection.send(&client_frame).unwrap(), SendOutcome::Sent);
            connection
        });

        let mut events = Events::with_capacity(8);
        let deadline = Instant::now() + Duration::from_secs(10);
        let received = loop {
            assert!(Instant::now() < deadline, "timed out waiting for the TLS frame");
            events.clear();
            poll.poll(&mut events, Some(Duration::from_millis(10))).unwrap();
            let mut message = None;
            for event in &events {
                if event.token() == listener_token {
                    listener.accept_ready(poll.registry()).unwrap();
                } else if let Some(ready) =
                    listener.get_message(event.token(), poll.registry()).unwrap()
                {
                    message = Some(ready);
                }
            }
            if let Some(message) = message {
                break message;
            }
        };

        assert_eq!(listener.connection_count(), 1);
        assert_eq!(received.kind, TcpFrameKind::Discovery);
        assert_eq!(received.data.as_ref(), frame.as_slice());

        let connection = client.join().unwrap();
        assert!(!connection.is_failed());
        listener.deregister_all(poll.registry()).unwrap();
        drop(connection);
        drop(listener);
        drop(poll);
    }

    /// A self-signed identity written to `dir`, trusting only itself.
    fn self_signed_tls_config(dir: &std::path::Path) -> TlsConfig {
        let cert_file = dir.join("cert.pem");
        let key_file = dir.join("key.pem");
        let generated = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
        std::fs::write(&cert_file, generated.cert.pem()).unwrap();
        std::fs::write(&key_file, generated.key_pair.serialize_pem()).unwrap();
        TlsConfig {
            ca_file: cert_file.clone(),
            cert_file,
            key_file,
            server_name: "localhost".to_string(),
            verify_peer: false,
        }
    }

    #[test]
    fn tls_connect_rejects_a_server_it_does_not_trust() {
        let server_dir = tempfile::tempdir().unwrap();
        let client_dir = tempfile::tempdir().unwrap();
        let server_config = Arc::new(self_signed_tls_config(server_dir.path()));
        // A different self-signed identity, so the server's certificate chains
        // to nothing this client trusts.
        let client_config = Arc::new(self_signed_tls_config(client_dir.path()));

        let mut listener = TcpListener::new(
            0,
            TcpSocketTuning::default(),
            Some(server_config),
            Duration::from_secs(5),
            Duration::from_secs(5),
        )
        .unwrap();
        let mut poll = Poll::new().unwrap();
        let listener_token = listener.register(poll.registry()).unwrap();
        let address = SocketAddr::from((Ipv4Addr::LOCALHOST, listener.port));

        let client = std::thread::spawn(move || {
            OutboundConnection::connect(
                address,
                &TcpSocketTuning::default(),
                Some(client_config.as_ref()),
                Duration::from_secs(5),
                Duration::from_secs(5),
            )
            .err()
        });

        let mut events = Events::with_capacity(8);
        let deadline = Instant::now() + Duration::from_secs(10);
        while !client.is_finished() {
            assert!(Instant::now() < deadline, "timed out waiting for the handshake to fail");
            events.clear();
            poll.poll(&mut events, Some(Duration::from_millis(10))).unwrap();
            for event in &events {
                if event.token() == listener_token {
                    listener.accept_ready(poll.registry()).unwrap();
                } else {
                    // The rejected handshake surfaces here as a read error.
                    let _ = listener.get_message(event.token(), poll.registry());
                }
            }
        }

        let error = client.join().unwrap().expect("client must reject an untrusted server");
        assert_eq!(error.kind(), io::ErrorKind::ConnectionAborted);

        listener.deregister_all(poll.registry()).unwrap();
        drop(listener);
        drop(poll);
    }
}
