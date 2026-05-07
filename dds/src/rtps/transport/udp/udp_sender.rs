#![allow(dead_code)]
#![allow(unused_variables)]

use crate::rtps::transport::{Transport, TransportConfig, TransportType};
use crate::rtps::{common::locator::MULTICAST_IP, transport::port_manager::PortManager};
use log::debug;
use socket2::{Domain, Protocol, SockAddr, Socket as Socket2, Type};
use std::env;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Mutex;

#[derive(Debug)]
pub(crate) struct UdpSender {
    socket: Mutex<Option<Socket2>>,
}

impl UdpSender {
    // INT2DDS_UDP_SOCKET_BUFFER check
    fn get_socket_buffer_size() -> Option<usize> {
        env::var("INT2DDS_UDP_SOCKET_BUFFER").ok().and_then(|val| val.parse().ok())
    }

    pub(crate) fn port(&self) -> u16 {
        self.socket
            .lock()
            .ok()
            .and_then(|guard| {
                guard
                    .as_ref()
                    .and_then(|s| s.local_addr().ok())
                    .and_then(|addr| addr.as_socket_ipv4().map(|v4| v4.port()))
            })
            .unwrap_or(0)
    }

    pub(crate) fn new(
        bind_ip: String,
        multicast_if_ip: String,
        transport_config: TransportConfig,
    ) -> std::io::Result<Self> {
        let socket = Socket2::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;

        let mc_addr: Ipv4Addr = multicast_if_ip.parse().unwrap();
        socket.set_multicast_if_v4(&mc_addr)?;
        socket.set_multicast_ttl_v4(transport_config.multicast_ttl as u32)?;

        let bind_addr: Ipv4Addr = bind_ip.parse().unwrap();
        let sock_addr = SockAddr::from(SocketAddr::new(IpAddr::V4(bind_addr), 0));
        socket.bind(&sock_addr)?;
        if let Some(size) = Self::get_socket_buffer_size() {
            socket.set_send_buffer_size(size)?;
        }

        crate::common::int2dds_feature_ffi::init_extended_discovery(&socket)?;

        Ok(Self { socket: Mutex::new(Some(socket)) })
    }

    /// Force close the socket even when there are other Arc references.
    /// This can be called with &self, unlike close() which requires ownership.
    pub(crate) fn force_close(&self) {
        if let Ok(mut guard) = self.socket.lock() {
            if let Some(socket) = guard.take() {
                log::info!("[UdpSender] Socket force closed");
                drop(socket);
            }
        }
    }
}

/// Implementation of Transport trait for UdpSender
///
/// This allows UdpSender to be used through the generic Transport interface,
/// enabling transport-agnostic code throughout the DDS stack.
impl Transport for UdpSender {
    fn send(&self, addr: &SocketAddr, data: &[u8]) -> io::Result<usize> {
        let guard = self.socket.lock().map_err(|_| io::Error::other("Mutex poisoned"))?;
        let socket = guard
            .as_ref()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotConnected, "Socket is closed"))?;
        let sock_addr = SockAddr::from(*addr);
        let result = socket.send_to(data, &sock_addr);
        debug!("UDP send to {:?}: {:?}", addr, result);
        result
    }

    fn send_multicast(&self, domain_id: u32, data: &[u8]) -> io::Result<usize> {
        let guard = self.socket.lock().map_err(|_| io::Error::other("Mutex poisoned"))?;
        let socket = guard
            .as_ref()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotConnected, "Socket is closed"))?;
        let port = PortManager::get_discovery_traffic_multicast_port(domain_id);
        let multicast_target: SocketAddr = SocketAddr::new(IpAddr::V4(MULTICAST_IP), port);
        let sock_addr = SockAddr::from(multicast_target);

        let result = socket.send_to(data, &sock_addr);
        debug!("UDP multicast send (domain {}): {:?}", domain_id, result);

        if let Err(e) = crate::common::int2dds_feature_ffi::send_extended_discovery(
            socket, port, data, domain_id,
        ) {
            log::warn!("[udp_sender] Extended discovery send failed: {}", e);
        }

        result
    }

    fn port(&self) -> u16 {
        self.socket
            .lock()
            .ok()
            .and_then(|guard| {
                guard
                    .as_ref()
                    .and_then(|s| s.local_addr().ok())
                    .and_then(|addr| addr.as_socket_ipv4().map(|v4| v4.port()))
            })
            .unwrap_or(0)
    }

    fn transport_type(&self) -> TransportType {
        TransportType::UDP
    }

    fn close(self) {
        if let Ok(mut guard) = self.socket.lock() {
            if let Some(socket) = guard.take() {
                log::info!("[UdpSender] Socket closed, port released");
                drop(socket);
            }
        }
    }
}
