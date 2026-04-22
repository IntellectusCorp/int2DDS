#![allow(dead_code)]
#![allow(unused_variables)]

use std::env;
use std::io::{self, ErrorKind};
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use log::{debug, info, warn};
use socket2::{Domain, Protocol, SockAddr, Socket as Socket2, Type};

use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::transport::error::{transport_io_error, TransportError, TransportErrorCode};
use crate::rtps::transport::port_manager::PortManager;
use crate::rtps::transport::tcp::framing::{read_framed_message, write_framed_message};
use crate::rtps::transport::tcp::protocol::{
    encode_locator, ControlMsg, MSG_KEEPALIVE_ACK, MSG_PEER_HELLO_ACK, MSG_PORT_BIND_ACK,
};
use crate::rtps::transport::tcp::stream_wrapper::{
    connect_tls, wrap_stream, TcpStreamWrapper,
};
use crate::rtps::transport::tcp::tls::TlsConfig;

/// Connection key: (physical address, logical_port)
type ConnectionKey = (SocketAddr, u16);

/// Logical port 0 = control connection
const CONTROL_LOGICAL_PORT: u16 = 0;

/// Cached peer info from PEER_HELLO handshake
#[derive(Debug, Clone)]
struct PeerInfo {
    /// Physical address of the control connection
    control_addr: SocketAddr,
}

/// TCP sender with 3-step handshake: PEER_HELLO → PORT_RESERVE → PORT_BIND
///
/// Connections are stored as `Box<dyn TcpStreamWrapper>` so that both
/// plain TCP and TLS connections can be held uniformly. A per-connection
/// TLS config on the sender decides which variant is used during
/// `tcp_connect()`.
#[derive(Clone)]
pub(crate) struct TcpSender {
    working_ip: String,
    connections: Arc<DashMap<ConnectionKey, Box<dyn TcpStreamWrapper>>>,
    peer_info: Arc<DashMap<SocketAddr, PeerInfo>>,
    /// Missed keepalive count per peer (physical addr)
    keepalive_missed: Arc<DashMap<SocketAddr, u32>>,
    connect_timeout: Duration,
    local_guid_prefix: GuidPrefix,
    domain_id: u32,
    participant_id: u32,
    listener_port: u16,
    /// TLS configuration for outbound connections. When `Some`, every
    /// newly-opened TCP socket is wrapped with TLS via `connect_tls`.
    tls_config: Option<Arc<TlsConfig>>,
    /// Set of peers for which an asymmetric host has already dialed a
    /// reverse channel, so the plugin does not repeatedly open them for
    /// the same peer on every SPDP tick. Managed by the plugin via
    /// `reverse_channels_dialed()`.
    reverse_channels_dialed: Arc<DashMap<SocketAddr, ()>>,
    /// Set of (peer, logical_port) pairs for which an asymmetric host
    /// has already dialed a reverse DATA channel. Used by the mux
    /// listener to avoid spawning duplicate dial threads when the
    /// reachable peer repeats PORT_RESERVE while a dial is in flight.
    reverse_data_dialed: Arc<DashMap<(SocketAddr, u16), ()>>,
    /// Channel to notify the RTPS layer (PeerMonitor) when a peer has
    /// been disconnected by any send-path failure, not just keepalive.
    /// Critical for asymmetric peers: once `disconnect_peer` clears the
    /// port-0 control entry, `ensure_control` short-circuits all future
    /// sends, so the keepalive loop has no connection left to probe —
    /// without this explicit signal the RTPS proxy state leaks forever.
    dead_peer_tx: std::sync::OnceLock<crossbeam_channel::Sender<SocketAddr>>,
}

// `Debug` by hand — `Box<dyn TcpStreamWrapper>` doesn't derive Debug.
impl std::fmt::Debug for TcpSender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TcpSender")
            .field("working_ip", &self.working_ip)
            .field("connection_count", &self.connections.len())
            .field("peer_count", &self.peer_info.len())
            .field("domain_id", &self.domain_id)
            .field("participant_id", &self.participant_id)
            .field("listener_port", &self.listener_port)
            .field("tls", &self.tls_config.is_some())
            .finish()
    }
}

impl TcpSender {
    const DEFAULT_CONNECT_TIMEOUT_MS: u64 = 5000;
    const DEFAULT_WRITE_TIMEOUT_MS: u64 = 10000;
    const DEFAULT_NODELAY: bool = true;

    pub(crate) fn new(
        working_ip: String,
        local_guid_prefix: GuidPrefix,
        domain_id: u32,
        participant_id: u32,
        listener_port: u16,
        keepalive_missed: Arc<DashMap<SocketAddr, u32>>,
    ) -> io::Result<Self> {
        Self::new_with_tls(
            working_ip,
            local_guid_prefix,
            domain_id,
            participant_id,
            listener_port,
            keepalive_missed,
            None,
        )
    }

    /// Like `new`, but with an optional TLS config for outbound connections.
    pub(crate) fn new_with_tls(
        working_ip: String,
        local_guid_prefix: GuidPrefix,
        domain_id: u32,
        participant_id: u32,
        listener_port: u16,
        keepalive_missed: Arc<DashMap<SocketAddr, u32>>,
        tls_config: Option<Arc<TlsConfig>>,
    ) -> io::Result<Self> {
        let connect_timeout_ms = env::var("INT2DDS_TCP_CONNECT_TIMEOUT")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(Self::DEFAULT_CONNECT_TIMEOUT_MS);

        debug!(
            "TcpSender: Created (domain={}, pid={}, port={}, tls={})",
            domain_id,
            participant_id,
            listener_port,
            tls_config.is_some()
        );

        Ok(Self {
            working_ip,
            connections: Arc::new(DashMap::new()),
            peer_info: Arc::new(DashMap::new()),
            keepalive_missed,
            connect_timeout: Duration::from_millis(connect_timeout_ms),
            local_guid_prefix,
            domain_id,
            participant_id,
            listener_port,
            tls_config,
            reverse_channels_dialed: Arc::new(DashMap::new()),
            reverse_data_dialed: Arc::new(DashMap::new()),
            dead_peer_tx: std::sync::OnceLock::new(),
        })
    }

    /// Returns true if a reverse channel was already dialed for this
    /// peer (so the plugin avoids redundant dials on SPDP retries).
    /// The caller inserts into the set via `mark_reverse_channel_dialed`.
    pub(crate) fn is_reverse_channel_dialed(&self, addr: &SocketAddr) -> bool {
        self.reverse_channels_dialed.contains_key(addr)
    }

    pub(crate) fn mark_reverse_channel_dialed(&self, addr: SocketAddr) {
        self.reverse_channels_dialed.insert(addr, ());
    }

    pub(crate) fn clear_reverse_channel_dialed(&self, addr: &SocketAddr) {
        self.reverse_channels_dialed.remove(addr);
    }

    /// True when a reverse DATA dial is already in flight or has
    /// succeeded for `(peer, logical_port)`. Used by the mux listener's
    /// reverse-data dispatcher to debounce repeated PORT_RESERVE frames.
    pub(crate) fn is_reverse_data_dialed(&self, peer: SocketAddr, logical_port: u16) -> bool {
        self.reverse_data_dialed.contains_key(&(peer, logical_port))
    }

    pub(crate) fn mark_reverse_data_dialed(&self, peer: SocketAddr, logical_port: u16) {
        self.reverse_data_dialed.insert((peer, logical_port), ());
    }

    pub(crate) fn clear_reverse_data_dialed(&self, peer: SocketAddr, logical_port: u16) {
        self.reverse_data_dialed.remove(&(peer, logical_port));
    }

    pub(crate) fn listener_port(&self) -> u16 {
        self.listener_port
    }

    /// Attach the dead-peer notification channel. Called once by the
    /// plugin after it constructs both the sender and the channel.
    /// Safe to call from any thread; subsequent calls are no-ops.
    pub(crate) fn set_dead_peer_tx(&self, tx: crossbeam_channel::Sender<SocketAddr>) {
        let _ = self.dead_peer_tx.set(tx);
    }

    fn get_write_timeout() -> Duration {
        Duration::from_millis(crate::common::env::get_tcp_write_timeout_ms())
    }

    fn get_nodelay() -> bool {
        crate::common::env::get_tcp_nodelay()
    }

    pub(crate) fn port(&self) -> u16 {
        self.listener_port
    }

    // ========================================================================
    // Incoming connection re-use (Connection Reversal for asymmetric peers)
    // ========================================================================

    /// Register a stream accepted by our mux listener as the control
    /// connection for `peer_listener_addr`.
    ///
    /// Used when the remote peer is in asymmetric NAT mode and cannot be
    /// dialed. The incoming control stream accepted from the asymmetric
    /// peer is re-purposed as this sender's outbound path to that peer:
    /// subsequent `ensure_control` calls for `peer_listener_addr` find
    /// the registered stream in cache and skip the outbound `tcp_connect`
    /// (which would fail since the peer has no listener).
    ///
    /// The caller is responsible for passing a cloned stream — the
    /// original should remain owned by the listener's read thread so that
    /// inbound framing continues to be consumed.
    pub(crate) fn register_incoming_control(
        &self,
        peer_listener_addr: SocketAddr,
        stream: Box<dyn TcpStreamWrapper>,
    ) {
        let key = (peer_listener_addr, CONTROL_LOGICAL_PORT);
        if self.connections.insert(key, stream).is_some() {
            debug!(
                "TcpSender: replaced inbound control stream for asymmetric peer {:?}",
                peer_listener_addr
            );
        } else {
            info!(
                "TcpSender: registered inbound control stream for asymmetric peer {:?}",
                peer_listener_addr
            );
        }
        // Also record peer_info so features like keepalive can find this peer.
        self.peer_info
            .insert(peer_listener_addr, PeerInfo { control_addr: peer_listener_addr });
    }

    /// Register a data-channel stream accepted by our mux listener for an
    /// asymmetric peer. `logical_port` is the port the peer expects us to
    /// use for this stream (set via PORT_BIND).
    pub(crate) fn register_incoming_data(
        &self,
        peer_listener_addr: SocketAddr,
        logical_port: u16,
        stream: Box<dyn TcpStreamWrapper>,
    ) {
        debug_assert_ne!(logical_port, CONTROL_LOGICAL_PORT);
        let key = (peer_listener_addr, logical_port);
        if self.connections.insert(key, stream).is_some() {
            debug!(
                "TcpSender: replaced inbound data stream for asymmetric peer {:?}:{}",
                peer_listener_addr, logical_port
            );
        } else {
            info!(
                "TcpSender: registered inbound data stream for asymmetric peer {:?}:{}",
                peer_listener_addr, logical_port
            );
        }
        // Clear the reverse-data dial guard: future losses of this
        // (peer, port) stream should be allowed to trigger a fresh dial.
        self.clear_reverse_data_dialed(peer_listener_addr, logical_port);
    }

    /// Open an additional outbound TCP connection to `physical_addr` and
    /// hand it over to the remote side as its reverse send channel.
    ///
    /// Used by asymmetric (NAT-behind) hosts to implement connection
    /// reversal: they open two physical TCP connections per peer — one
    /// for their own sending (via `ensure_control`) and one as a
    /// "reverse" channel the reachable peer uses for its sending. The
    /// stream returned here belongs to the listener side (this host's
    /// mux listener must adopt it and run a read loop, since the remote
    /// peer will write to it).
    ///
    /// This method only performs the dial + PEER_HELLO_REVERSE exchange;
    /// the caller is responsible for handing the returned stream to the
    /// local mux listener (see `TcpMuxListener::adopt_inbound_stream`).
    pub(crate) fn dial_reverse_channel(
        &self,
        physical_addr: &SocketAddr,
        marker_locator_port: u16,
    ) -> io::Result<Box<dyn TcpStreamWrapper>> {
        let mut stream = self.tcp_connect(physical_addr)?;

        let local_ip: std::net::Ipv4Addr =
            self.working_ip.parse().unwrap_or(std::net::Ipv4Addr::UNSPECIFIED);
        let locator = encode_locator(local_ip, marker_locator_port);
        let hello = ControlMsg::PeerHelloReverse { locator };
        write_framed_message(&mut stream, &hello.to_bytes()).map_err(|e| {
            Self::wrap_raw_io_error(
                e,
                TransportErrorCode::TcpHandshakeHelloFailed,
                physical_addr,
            )
        })?;

        let resp = self.read_control_response(&mut stream).map_err(|e| {
            Self::wrap_raw_io_error(
                e,
                TransportErrorCode::TcpHandshakeHelloFailed,
                physical_addr,
            )
        })?;
        if resp.to_bytes()[0] != MSG_PEER_HELLO_ACK {
            return Err(transport_io_error(
                TransportErrorCode::TcpHandshakeHelloFailed,
                format!(
                    "Expected PEER_HELLO_ACK after PEER_HELLO_REVERSE, got {}",
                    resp.type_name()
                ),
            ));
        }

        info!(
            "TcpSender: reverse channel dialed to {:?} (marker_port={})",
            physical_addr, marker_locator_port
        );
        Ok(stream)
    }

    /// Open an additional outbound TCP connection to `physical_addr` and
    /// hand it over to the remote side as its reverse user-data send
    /// channel for `logical_port`. Counterpart to `dial_reverse_channel`
    /// but at the PORT_BIND stage — used after the asymmetric host sees
    /// a PORT_RESERVE arrive on an already-established reverse control
    /// stream. The caller is responsible for adopting the returned
    /// stream on the local mux listener as an Active data connection.
    pub(crate) fn dial_reverse_data_channel(
        &self,
        physical_addr: &SocketAddr,
        marker_locator_port: u16,
        logical_port: u16,
    ) -> io::Result<Box<dyn TcpStreamWrapper>> {
        let mut stream = self.tcp_connect(physical_addr)?;

        let local_ip: std::net::Ipv4Addr =
            self.working_ip.parse().unwrap_or(std::net::Ipv4Addr::UNSPECIFIED);
        let locator = encode_locator(local_ip, marker_locator_port);
        let bind = ControlMsg::PortBindReverse { locator, logical_port };
        write_framed_message(&mut stream, &bind.to_bytes()).map_err(|e| {
            Self::wrap_raw_io_error(
                e,
                TransportErrorCode::TcpHandshakeBindFailed,
                physical_addr,
            )
        })?;

        let resp = self.read_control_response(&mut stream).map_err(|e| {
            Self::wrap_raw_io_error(
                e,
                TransportErrorCode::TcpHandshakeBindFailed,
                physical_addr,
            )
        })?;
        if resp.to_bytes()[0] != MSG_PORT_BIND_ACK {
            return Err(transport_io_error(
                TransportErrorCode::TcpHandshakeBindFailed,
                format!(
                    "Expected PORT_BIND_ACK after PORT_BIND_REVERSE, got {}",
                    resp.type_name()
                ),
            ));
        }

        info!(
            "TcpSender: reverse data channel dialed to {:?} (port={}, marker_port={})",
            physical_addr, logical_port, marker_locator_port
        );
        Ok(stream)
    }

    // ========================================================================
    // 3-Step Handshake
    // ========================================================================

    /// Step 1: PEER_HELLO — establish control connection, exchange locators.
    fn ensure_control(&self, physical_addr: &SocketAddr) -> io::Result<()> {
        let key = (*physical_addr, CONTROL_LOGICAL_PORT);
        if self.connections.contains_key(&key) {
            return Ok(());
        }
        // Asymmetric peer marker: a locator with port 0 means the peer has
        // no listener and we must wait for them to dial us. Never attempt
        // an outbound `tcp_connect` to port 0 — it would just fail.
        if physical_addr.port() == 0 {
            return Err(transport_io_error(
                TransportErrorCode::TcpConnectionRefused,
                format!(
                    "Asymmetric peer {:?} not yet connected (no inbound stream cached)",
                    physical_addr
                ),
            ));
        }
        self.ensure_control_inner(physical_addr, key).map_err(|e| {
            Self::wrap_raw_io_error(e, TransportErrorCode::TcpHandshakeHelloFailed, physical_addr)
        })
    }

    fn ensure_control_inner(
        &self,
        physical_addr: &SocketAddr,
        key: ConnectionKey,
    ) -> io::Result<()> {
        let mut stream = self.tcp_connect(physical_addr)?;

        let local_ip: std::net::Ipv4Addr =
            self.working_ip.parse().unwrap_or(std::net::Ipv4Addr::UNSPECIFIED);
        let locator = encode_locator(local_ip, self.listener_port);

        // Send PEER_HELLO
        let hello = ControlMsg::PeerHello { locator };
        write_framed_message(&mut stream, &hello.to_bytes())?;

        // Read PEER_HELLO_ACK
        let resp = self.read_control_response(&mut stream)?;
        if resp.to_bytes()[0] != MSG_PEER_HELLO_ACK {
            return Err(transport_io_error(
                TransportErrorCode::TcpHandshakeHelloFailed,
                format!("Expected PEER_HELLO_ACK, got {}", resp.type_name()),
            ));
        }

        debug!("TcpSender: PEER_HELLO complete to {:?}", physical_addr);

        self.connections.insert(key, stream);
        self.peer_info.insert(*physical_addr, PeerInfo { control_addr: *physical_addr });

        Ok(())
    }

    /// Steps 2+3: PORT_RESERVE on control connection, then PORT_BIND on new connection.
    fn ensure_data(&self, physical_addr: &SocketAddr, logical_port: u16) -> io::Result<()> {
        // Ensure control connection is healthy first. If it was stale,
        // disconnect_peer (called by the retry in send_to_logical_port)
        // already removed all connections for this address, so the
        // contains_key check below sees a clean slate.
        self.ensure_control(physical_addr)?;

        let key = (*physical_addr, logical_port);
        if self.connections.contains_key(&key) {
            return Ok(());
        }

        // Asymmetric-peer path: the peer has no listener, so we cannot
        // dial a PORT_BIND ourselves. Send PORT_RESERVE on the adopted
        // reverse control (fire-and-forget — the asymmetric peer reads
        // it, recognises the reverse-control state, and dials the data
        // channel back to us tagged with PORT_BIND_REVERSE). Return Err
        // so the upper layer's next send cycle retries; by then the
        // reverse data stream should be registered in the cache.
        if physical_addr.port() == 0 {
            self.request_reverse_data(physical_addr, logical_port);
            return Err(transport_io_error(
                TransportErrorCode::TcpReverseChannelPending,
                format!(
                    "Reverse data channel pending for asymmetric peer {:?}:{}",
                    physical_addr, logical_port
                ),
            ));
        }

        self.ensure_data_inner(physical_addr, logical_port, key)
    }

    /// Send PORT_RESERVE on the existing reverse control stream for this
    /// asymmetric peer. Fire-and-forget — no PORT_RESERVE_ACK is read
    /// because the asymmetric peer will not issue a cookie; instead it
    /// dials a PORT_BIND_REVERSE connection in response.
    fn request_reverse_data(&self, physical_addr: &SocketAddr, logical_port: u16) {
        let control_key = (*physical_addr, CONTROL_LOGICAL_PORT);
        let mut control_stream = match self
            .connections
            .get(&control_key)
            .and_then(|s| s.try_clone_box().ok())
        {
            Some(s) => s,
            None => {
                debug!(
                    "TcpSender: reverse PORT_RESERVE skipped — no control stream for {:?}",
                    physical_addr
                );
                return;
            }
        };

        let reserve = ControlMsg::PortReserve { logical_port };
        if let Err(e) = write_framed_message(&mut control_stream, &reserve.to_bytes()) {
            warn!(
                "TcpSender: reverse PORT_RESERVE write failed for {:?}:{} — {:?}",
                physical_addr, logical_port, e
            );
            // Drop the broken control so the next SPDP tick can rebuild.
            self.disconnect_peer(physical_addr);
        } else {
            debug!(
                "TcpSender: reverse PORT_RESERVE sent to {:?} (port={})",
                physical_addr, logical_port
            );
        }
    }

    fn ensure_data_inner(
        &self,
        physical_addr: &SocketAddr,
        logical_port: u16,
        key: ConnectionKey,
    ) -> io::Result<()> {
        // Step 2: PORT_RESERVE on the control connection
        let control_key = (*physical_addr, CONTROL_LOGICAL_PORT);
        let cookie = {
            let mut control_stream = self
                .connections
                .get(&control_key)
                .ok_or_else(|| {
                    transport_io_error(
                        TransportErrorCode::TcpHandshakeReserveFailed,
                        "Control connection lost before PORT_RESERVE",
                    )
                })?
                .value()
                .try_clone_box()
                .map_err(|e| {
                    Self::wrap_raw_io_error(
                        e,
                        TransportErrorCode::TcpHandshakeReserveFailed,
                        physical_addr,
                    )
                })?;

            let reserve = ControlMsg::PortReserve { logical_port };
            write_framed_message(&mut control_stream, &reserve.to_bytes()).map_err(|e| {
                Self::wrap_raw_io_error(
                    e,
                    TransportErrorCode::TcpHandshakeReserveFailed,
                    physical_addr,
                )
            })?;

            let resp = self.read_control_response(&mut control_stream).map_err(|e| {
                Self::wrap_raw_io_error(
                    e,
                    TransportErrorCode::TcpHandshakeReserveFailed,
                    physical_addr,
                )
            })?;
            match resp {
                ControlMsg::PortReserveAck { cookie } => cookie,
                ControlMsg::Error { operation, code, message } => {
                    return Err(transport_io_error(
                        TransportErrorCode::TcpHandshakeReserveFailed,
                        format!(
                            "PORT_RESERVE rejected (op=0x{:02x}, code={}): {}",
                            operation, code, message
                        ),
                    ));
                }
                other => {
                    return Err(transport_io_error(
                        TransportErrorCode::TcpHandshakeReserveFailed,
                        format!("Expected PORT_RESERVE_ACK, got {}", other.type_name()),
                    ));
                }
            }
        };

        debug!(
            "TcpSender: PORT_RESERVE complete (port={}, cookie=0x{:02x})",
            logical_port, cookie[0]
        );

        // Step 3: PORT_BIND on a NEW TCP connection
        let mut data_stream = self.tcp_connect(physical_addr)?;

        let bind = ControlMsg::PortBind { cookie };
        write_framed_message(&mut data_stream, &bind.to_bytes()).map_err(|e| {
            Self::wrap_raw_io_error(e, TransportErrorCode::TcpHandshakeBindFailed, physical_addr)
        })?;

        let resp = self.read_control_response(&mut data_stream).map_err(|e| {
            Self::wrap_raw_io_error(e, TransportErrorCode::TcpHandshakeBindFailed, physical_addr)
        })?;
        if resp.to_bytes()[0] != MSG_PORT_BIND_ACK {
            return Err(transport_io_error(
                TransportErrorCode::TcpHandshakeBindFailed,
                format!("Expected PORT_BIND_ACK, got {}", resp.type_name()),
            ));
        }

        debug!("TcpSender: PORT_BIND complete (port={}, cookie=0x{:02x})", logical_port, cookie[0]);

        self.connections.insert(key, data_stream);
        Ok(())
    }

    // ========================================================================
    // Sending
    // ========================================================================

    /// Send RTPS data to a logical port. Performs handshake if needed.
    ///
    /// If the write fails with BrokenPipe / ConnectionReset (peer went
    /// away), the dead connection is purged and **one automatic retry** is
    /// attempted. The retry re-runs the full ensure_control → ensure_data
    /// → write sequence, which opens a fresh TCP connection to the same
    /// address. This lets a restarted remote peer (new GUID, same
    /// address) be discovered on the very next SPDP send cycle instead of
    /// having to wait for the keepalive timeout to expire.
    pub(crate) fn send_to_logical_port(
        &self,
        addr: &SocketAddr,
        logical_port: u16,
        data: &[u8],
    ) -> io::Result<usize> {
        match self.send_to_logical_port_once(addr, logical_port, data) {
            Ok(n) => Ok(n),
            Err(e)
                if e.kind() == ErrorKind::BrokenPipe
                    || e.kind() == ErrorKind::ConnectionReset
                    || e.kind() == ErrorKind::ConnectionAborted =>
            {
                debug!(
                    "TcpSender: send to {:?}:{} failed ({}), reconnecting and retrying",
                    addr, logical_port, e
                );
                self.disconnect_peer(addr);
                // One retry — if this also fails, propagate the error.
                self.send_to_logical_port_once(addr, logical_port, data)
            }
            Err(e) => Err(e),
        }
    }

    fn send_to_logical_port_once(
        &self,
        addr: &SocketAddr,
        logical_port: u16,
        data: &[u8],
    ) -> io::Result<usize> {
        self.ensure_data(addr, logical_port)?;

        let key = (*addr, logical_port);
        let mut stream = self
            .connections
            .get(&key)
            .ok_or_else(|| {
                transport_io_error(
                    TransportErrorCode::TcpConnectionRefused,
                    format!("Connection not found for {:?}", key),
                )
            })?
            .value()
            .try_clone_box()?;

        match write_framed_message(&mut stream, data) {
            Ok(()) => {
                debug!("TcpSender: Sent {} bytes to {:?}", data.len(), key);
                Ok(data.len())
            }
            Err(e) => {
                if e.kind() == ErrorKind::BrokenPipe
                    || e.kind() == ErrorKind::ConnectionReset
                    || e.kind() == ErrorKind::ConnectionAborted
                {
                    debug!("TcpSender: Peer {:?} disconnected, cleaning up", addr);
                    self.disconnect_peer(addr);
                } else {
                    self.connections.remove(&key);
                }
                Err(e)
            }
        }
    }

    /// Send to discovery channel.
    pub(crate) fn send_to_discovery(&self, addr: &SocketAddr, data: &[u8]) -> io::Result<usize> {
        let port =
            PortManager::get_discovery_traffic_unicast_port(self.domain_id, self.participant_id);
        self.send_to_logical_port(addr, port, data)
    }

    /// Send to user data channel.
    pub(crate) fn send_to_user_data(&self, addr: &SocketAddr, data: &[u8]) -> io::Result<usize> {
        let port = PortManager::get_user_traffic_unicast_port(self.domain_id, self.participant_id);
        self.send_to_logical_port(addr, port, data)
    }

    pub(crate) fn get_peer_discovery_port(&self, addr: &SocketAddr) -> io::Result<u16> {
        Ok(PortManager::get_discovery_traffic_unicast_port(self.domain_id, self.participant_id))
    }

    pub(crate) fn get_peer_user_port(&self, addr: &SocketAddr) -> io::Result<u16> {
        Ok(PortManager::get_user_traffic_unicast_port(self.domain_id, self.participant_id))
    }

    pub(crate) fn disconnect_peer(&self, addr: &SocketAddr) {
        let had_entry = self
            .connections
            .iter()
            .any(|e| e.key().0 == *addr)
            || self.peer_info.contains_key(addr);

        self.connections.retain(|key, _| key.0 != *addr);
        self.peer_info.remove(addr);
        self.keepalive_missed.remove(addr);
        // Also clear reverse-channel guards so a future SPDP re-discovery
        // can re-establish the 2-dial flow from scratch.
        self.reverse_channels_dialed.remove(addr);
        self.reverse_data_dialed.retain(|key, _| key.0 != *addr);

        debug!("TcpSender: Disconnected peer {:?}", addr);

        // Signal the RTPS layer so PeerMonitor can unmatch RTPS proxies.
        // We only emit if there was actually something to clean up, to
        // avoid spurious events on idempotent disconnect_peer calls.
        if had_entry {
            if let Some(tx) = self.dead_peer_tx.get() {
                let _ = tx.try_send(*addr);
            }
        }
    }

    /// Send keepalive on each outgoing control connection.
    /// ACK waiting is offloaded to a spawned thread so the mux loop is never blocked.
    /// Returns list of peer addresses that have exceeded max missed keepalives.
    pub(crate) fn send_keepalives(&self) -> Vec<SocketAddr> {
        let max_missed: u32 = crate::common::env::get_tcp_keepalive_max_misses();

        let ack_timeout = Duration::from_millis(crate::common::env::get_tcp_keepalive_timeout_ms());

        let mut dead_peers = Vec::new();
        let control_peers: Vec<SocketAddr> = self
            .connections
            .iter()
            .filter(|e| e.key().1 == CONTROL_LOGICAL_PORT)
            .map(|e| e.key().0)
            .collect();

        for peer_addr in control_peers {
            let missed = self.keepalive_missed.get(&peer_addr).map(|v| *v).unwrap_or(0);

            if missed >= max_missed {
                warn!("TcpSender: Peer {:?} missed {} keepalives", peer_addr, missed);
                dead_peers.push(peer_addr);
                continue;
            }

            let key = (peer_addr, CONTROL_LOGICAL_PORT);
            let mut stream =
                match self.connections.get(&key).and_then(|s| s.try_clone_box().ok()) {
                    Some(s) => s,
                    None => continue,
                };

            if let Err(e) = write_framed_message(&mut stream, &ControlMsg::Keepalive.to_bytes()) {
                warn!("TcpSender: Keepalive send failed to {:?}: {:?}", peer_addr, e);
                self.disconnect_peer(&peer_addr);
                dead_peers.push(peer_addr);
                continue;
            }

            // Spawn a thread to wait for ACK — prevents blocking the mux loop.
            // This avoids deadlock in self-connection where the mux thread must
            // both send the ACK (via on_readable) and receive it (via read here).
            let keepalive_missed = self.keepalive_missed.clone();
            std::thread::Builder::new()
                .name(format!("keepalive_ack_{}", peer_addr))
                .spawn(move || {
                    stream.set_read_timeout(Some(ack_timeout)).ok();
                    match read_framed_message(&mut stream) {
                        Ok(payload) if payload.first() == Some(&MSG_KEEPALIVE_ACK) => {
                            keepalive_missed.insert(peer_addr, 0);
                            debug!("TcpSender: KeepaliveAck from {:?}", peer_addr);
                        }
                        Ok(_) | Err(_) => {
                            let prev = keepalive_missed.get(&peer_addr).map(|v| *v).unwrap_or(0);
                            let new_missed = prev + 1;
                            warn!(
                                "TcpSender: No KeepaliveAck from {:?} (missed={}/{})",
                                peer_addr, new_missed, max_missed
                            );
                            keepalive_missed.insert(peer_addr, new_missed);
                        }
                    }
                })
                .ok();
        }

        dead_peers
    }

    /// Prune outgoing data connections whose control connection is missing.
    ///
    /// This mirrors the listener-side orphan pruning in `TcpMuxListener`.
    /// A data connection becomes orphaned when its control connection was
    /// removed (e.g. write failure) but the data entry was not cleaned up
    /// at the same time. Returns the number of pruned connections.
    pub(crate) fn prune_orphan_connections(&self) -> usize {
        // Collect peer addrs that have at least one data connection
        let peers_with_data: Vec<SocketAddr> = self
            .connections
            .iter()
            .filter(|e| e.key().1 != CONTROL_LOGICAL_PORT)
            .map(|e| e.key().0)
            .collect();

        let mut pruned = 0;
        for peer_addr in peers_with_data {
            let has_control = self.connections.contains_key(&(peer_addr, CONTROL_LOGICAL_PORT));

            if !has_control {
                let before = self.connections.len();
                self.connections.retain(|key, _| key.0 != peer_addr);
                let removed = before - self.connections.len();
                self.peer_info.remove(&peer_addr);
                self.keepalive_missed.remove(&peer_addr);
                if removed > 0 {
                    warn!(
                        "TcpSender [{}]: Pruned {} orphan outgoing connection(s) for {:?}",
                        TransportErrorCode::TcpOrphanPruned,
                        removed,
                        peer_addr
                    );
                    pruned += removed;
                }
            }
        }
        pruned
    }

    /// Test only: remove the control connection entry without touching data connections.
    /// This creates an orphan state where data connections exist without their control.
    #[cfg(test)]
    pub(crate) fn drop_control_only(&self, addr: &SocketAddr) {
        self.connections.remove(&(*addr, CONTROL_LOGICAL_PORT));
        debug!("TcpSender: [test] dropped control-only for {:?}", addr);
    }

    /// Test only: remove a specific data connection entry without touching control.
    /// The next send attempt to this logical port will trigger re-reserve + re-bind.
    #[cfg(test)]
    pub(crate) fn drop_data_connection(&self, addr: &SocketAddr, logical_port: u16) {
        assert_ne!(logical_port, CONTROL_LOGICAL_PORT, "use drop_control_only for control");
        self.connections.remove(&(*addr, logical_port));
        debug!("TcpSender: [test] dropped data connection {:?} port={}", addr, logical_port);
    }

    pub(crate) fn connection_count(&self) -> usize {
        self.connections.len()
    }

    pub(crate) fn peer_count(&self) -> usize {
        self.peer_info.len()
    }

    pub(crate) fn close_all(self) {
        self.connections.clear();
        self.peer_info.clear();
        debug!("TcpSender: All connections closed");
    }

    // ========================================================================
    // Internal helpers
    // ========================================================================

    /// Wrap a raw OS `io::Error` with a `TransportError` if it isn't one already.
    fn wrap_raw_io_error(e: io::Error, code: TransportErrorCode, addr: &SocketAddr) -> io::Error {
        if e.get_ref().and_then(|s| s.downcast_ref::<TransportError>()).is_some() {
            return e;
        }
        transport_io_error(code, format!("{} (peer {:?})", e, addr))
    }

    fn tcp_connect(&self, addr: &SocketAddr) -> io::Result<Box<dyn TcpStreamWrapper>> {
        self.tcp_connect_inner(addr).map_err(|e| {
            // Already a TransportError — pass through as-is.
            if e.get_ref().and_then(|s| s.downcast_ref::<TransportError>()).is_some() {
                return e;
            }
            // Wrap raw OS errors with a TransportErrorCode based on ErrorKind.
            let code = match e.kind() {
                ErrorKind::TimedOut => TransportErrorCode::TcpConnectionTimeout,
                ErrorKind::ConnectionRefused => TransportErrorCode::TcpConnectionRefused,
                ErrorKind::AddrNotAvailable | ErrorKind::AddrInUse => {
                    TransportErrorCode::TcpBindFailed
                }
                _ => TransportErrorCode::TcpConnectionRefused,
            };
            transport_io_error(code, format!("{} (to {:?})", e, addr))
        })
    }

    fn tcp_connect_inner(&self, addr: &SocketAddr) -> io::Result<Box<dyn TcpStreamWrapper>> {
        let socket2 = Socket2::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))?;

        // Apply optional buffer-size overrides BEFORE connect so they take
        // effect on the initial handshake's window negotiation.
        if let Some(sz) = crate::common::env::get_tcp_so_rcvbuf() {
            let _ = socket2.set_recv_buffer_size(sz);
        }
        if let Some(sz) = crate::common::env::get_tcp_so_sndbuf() {
            let _ = socket2.set_send_buffer_size(sz);
        }

        let local_ip: IpAddr = self.working_ip.parse().map_err(|e| {
            io::Error::new(ErrorKind::InvalidInput, format!("Invalid working_ip: {}", e))
        })?;
        socket2.bind(&SockAddr::from(SocketAddr::new(local_ip, 0)))?;
        socket2.set_nonblocking(true)?;

        match socket2.connect(&SockAddr::from(*addr)) {
            Ok(_) => {}
            Err(e)
                if e.raw_os_error() == Some(10035)
                    || e.raw_os_error() == Some(115)
                    || e.kind() == ErrorKind::WouldBlock =>
            {
                let start = std::time::Instant::now();
                loop {
                    if start.elapsed() >= self.connect_timeout {
                        return Err(transport_io_error(
                            TransportErrorCode::TcpConnectionTimeout,
                            format!("Connection timeout to {:?}", addr),
                        ));
                    }
                    match socket2.take_error() {
                        Ok(Some(err)) => {
                            return Err(transport_io_error(
                                TransportErrorCode::TcpConnectionRefused,
                                format!("Connection failed to {:?}: {:?}", addr, err),
                            ));
                        }
                        Ok(None) if socket2.peer_addr().is_ok() => break,
                        Ok(None) => {}
                        Err(e) => return Err(e),
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
            Err(e) => return Err(e),
        }

        socket2.set_nonblocking(false)?;
        let stream: TcpStream = socket2.into();
        let _ = stream.set_nodelay(Self::get_nodelay());
        let _ = stream.set_write_timeout(Some(Self::get_write_timeout()));

        // If TLS is configured, wrap the connected TCP socket with a TLS
        // session. The handshake runs synchronously on this thread.
        match &self.tls_config {
            Some(cfg) => {
                let client_cfg = cfg.build_client_config()?;
                connect_tls(stream, client_cfg, &cfg.server_name)
            }
            None => Ok(wrap_stream(stream)),
        }
    }

    fn read_control_response(
        &self,
        stream: &mut Box<dyn TcpStreamWrapper>,
    ) -> io::Result<ControlMsg> {
        let payload = read_framed_message(stream)?;
        ControlMsg::from_bytes(&payload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtps::transport::tcp::tcp_mux_listener::TcpMuxListener;
    use crossbeam_channel::bounded;

    /// Allocate a non-overlapping domain so concurrent tests don't collide.
    fn next_test_domain() -> u32 {
        use std::sync::atomic::{AtomicU32, Ordering};
        static NEXT: AtomicU32 = AtomicU32::new(800);
        NEXT.fetch_add(1, Ordering::SeqCst)
    }

    fn create_test_sender() -> TcpSender {
        let keepalive_missed = Arc::new(DashMap::new());
        TcpSender::new("127.0.0.1".to_string(), [0x01; 12], 0, 0, 7400, keepalive_missed).unwrap()
    }

    /// Create a MuxListener on an ephemeral port + a TcpSender targeting it.
    /// Returns (listener, sender, listener_addr, discovery_port, user_port).
    fn create_listener_and_sender(
        domain_id: u32,
    ) -> (TcpMuxListener, TcpSender, SocketAddr, u16, u16) {
        let (disc_tx, _disc_rx) = bounded(64);
        let (user_tx, _user_rx) = bounded(64);
        let listener = TcpMuxListener::new(
            0, // port 0 → OS-assigned ephemeral port
            domain_id, 0, [0x02; 12], disc_tx, user_tx,
        )
        .expect("listener creation");
        let port = listener.port();
        let addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();

        let disc_port =
            crate::rtps::transport::port_manager::PortManager::get_discovery_traffic_unicast_port(
                domain_id, 0,
            );
        let user_port =
            crate::rtps::transport::port_manager::PortManager::get_user_traffic_unicast_port(
                domain_id, 0,
            );

        let keepalive_missed = Arc::new(DashMap::new());
        let sender = TcpSender::new(
            "127.0.0.1".to_string(),
            [0x01; 12],
            domain_id,
            0,
            port,
            keepalive_missed,
        )
        .expect("sender creation");

        (listener, sender, addr, disc_port, user_port)
    }

    /// Pump the listener's accept + on_readable in a background thread until
    /// the stop flag is set. Returns a JoinHandle.
    fn spawn_listener_pump(
        mut listener: TcpMuxListener,
        stop: Arc<std::sync::atomic::AtomicBool>,
    ) -> std::thread::JoinHandle<()> {
        use crate::rtps::transport::tcp::stream_wrapper::wrap_stream;

        std::thread::Builder::new()
            .name("test_listener_pump".to_string())
            .spawn(move || {
                // TcpMuxListener already sets SO_RCVTIMEO = 100ms on the socket,
                // so accept() returns WouldBlock periodically for stop-flag checks.
                let raw_listener = match listener.take_listener() {
                    Some(l) => l,
                    None => return,
                };

                let terminated = stop.clone();
                let idle_timeout = Duration::from_secs(30);

                while !stop.load(std::sync::atomic::Ordering::SeqCst) {
                    match raw_listener.accept() {
                        Ok((tcp, addr)) => {
                            let stream = wrap_stream(tcp);
                            listener.accept_connection(
                                stream,
                                addr,
                                terminated.clone(),
                                idle_timeout,
                            );
                        }
                        Err(ref e)
                            if e.kind() == std::io::ErrorKind::WouldBlock
                                || e.kind() == std::io::ErrorKind::TimedOut =>
                        {
                            // Timeout — check stop flag on next iteration.
                        }
                        Err(_) => break,
                    }
                }

                listener.close();
            })
            .unwrap()
    }

    #[test]
    fn test_tcp_sender_creation() {
        let sender = create_test_sender();
        assert_eq!(sender.port(), 7400);
        assert_eq!(sender.connection_count(), 0);
        assert_eq!(sender.peer_count(), 0);
    }

    #[test]
    fn test_disconnect_nonexistent_peer() {
        let sender = create_test_sender();
        let addr: SocketAddr = "192.168.1.10:7400".parse().unwrap();
        sender.disconnect_peer(&addr);
        assert_eq!(sender.connection_count(), 0);
    }

    #[test]
    fn control_drop_orphan_prune() {
        let domain = next_test_domain();
        let (listener, sender, addr, disc_port, _user_port) = create_listener_and_sender(domain);

        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let handle = spawn_listener_pump(listener, stop.clone());

        // Allow listener thread to start
        std::thread::sleep(Duration::from_millis(100));

        // Establish control + discovery data connection
        sender.ensure_data(&addr, disc_port).expect("ensure_data");
        assert_eq!(sender.connection_count(), 2, "control + data");

        // Drop control only → data becomes orphan
        sender.drop_control_only(&addr);
        assert_eq!(sender.connection_count(), 1, "orphan data remains");

        // Prune orphans
        let pruned = sender.prune_orphan_connections();
        assert!(pruned > 0, "should have pruned orphan data");
        assert_eq!(sender.connection_count(), 0, "all cleaned up");

        stop.store(true, std::sync::atomic::Ordering::SeqCst);
        handle.join().unwrap();
    }

    #[test]
    fn discovery_data_drop_recovery() {
        let domain = next_test_domain();
        let (listener, sender, addr, disc_port, _user_port) = create_listener_and_sender(domain);

        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let handle = spawn_listener_pump(listener, stop.clone());

        std::thread::sleep(Duration::from_millis(100));

        // Establish control + discovery data
        sender.ensure_data(&addr, disc_port).expect("ensure_data");
        assert_eq!(sender.connection_count(), 2);

        // Drop discovery data only → control survives
        sender.drop_data_connection(&addr, disc_port);
        assert_eq!(sender.connection_count(), 1, "only control remains");

        // Next send triggers re-reserve + re-bind via ensure_data
        let dummy_rtps = b"RTPS test payload for 12_8";
        let result = sender.send_to_discovery(&addr, dummy_rtps);
        assert!(
            result.is_ok(),
            "send_to_discovery should succeed after re-reserve: {:?}",
            result.err()
        );
        assert_eq!(sender.connection_count(), 2, "control + new data restored");

        stop.store(true, std::sync::atomic::Ordering::SeqCst);
        handle.join().unwrap();
    }

    #[test]
    fn user_data_drop_recovery() {
        let domain = next_test_domain();
        let (listener, sender, addr, disc_port, user_port) = create_listener_and_sender(domain);

        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let handle = spawn_listener_pump(listener, stop.clone());

        std::thread::sleep(Duration::from_millis(100));

        // Establish control + discovery + user data
        sender.ensure_data(&addr, disc_port).expect("ensure disc");
        sender.ensure_data(&addr, user_port).expect("ensure user");
        assert_eq!(sender.connection_count(), 3, "control + disc + user");

        // Drop user data only → control + discovery survive
        sender.drop_data_connection(&addr, user_port);
        assert_eq!(sender.connection_count(), 2, "control + disc remain");

        // Next send triggers re-reserve + re-bind for user port
        let dummy_rtps = b"RTPS test payload for 12_9";
        let result = sender.send_to_user_data(&addr, dummy_rtps);
        assert!(
            result.is_ok(),
            "send_to_user_data should succeed after re-reserve: {:?}",
            result.err()
        );
        assert_eq!(sender.connection_count(), 3, "all three connections restored");

        stop.store(true, std::sync::atomic::Ordering::SeqCst);
        handle.join().unwrap();
    }
}
