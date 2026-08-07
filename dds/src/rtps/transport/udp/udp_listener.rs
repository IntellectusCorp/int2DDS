#![allow(dead_code)]
#![allow(unused_variables)]

use bytes::Bytes;
use core::net::{Ipv4Addr, SocketAddr};
use log::{debug, error, warn};
use network_interface::{Addr, NetworkInterface, NetworkInterfaceConfig};
use socket2::{Domain, Protocol, SockAddr, Socket as Socket2, Type};
use std::env;
use std::net::UdpSocket as StdUdpSocket;

use crate::rtps::common::locator::{Locator, MULTICAST_IP};
use crate::rtps::transport::udp::recv_arena::{
    RecvArena, DEFAULT_RECV_ARENA_CHUNK_BYTES, MAX_UDP_PACKET_BYTES,
};

// One arena per listener; reused across every incoming datagram on this socket.
fn new_recv_arena() -> RecvArena {
    RecvArena::new(DEFAULT_RECV_ARENA_CHUNK_BYTES, MAX_UDP_PACKET_BYTES)
}

#[derive(Debug)]
pub(crate) struct UdpListener {
    port: u16,
    socket: Option<mio::net::UdpSocket>,
    recv_arena: RecvArena,
    multicast_group_handle: Option<Socket2>,
}

impl Drop for UdpListener {
    fn drop(&mut self) {
        self.close();
    }
}

impl UdpListener {
    // INT2DDS_UDP_SOCKET_BUFFER check
    fn get_socket_buffer_size() -> Option<usize> {
        env::var("INT2DDS_UDP_SOCKET_BUFFER").ok().and_then(|val| val.parse().ok())
    }

    // Applies the configured receive buffer, or doubles the OS default when unset.
    //
    // Linux silently caps SO_RCVBUF at net.core.rmem_max, and getsockopt reports back
    // twice the granted size. Losing one DATA_FRAG loses the whole fragmented sample,
    // so a buffer that is smaller than asked for must not pass unnoticed: a deployment
    // otherwise believes it has headroom it does not have.
    fn apply_recv_buffer_size(socket: &Socket2) {
        let Some(requested) = Self::get_socket_buffer_size() else {
            if let Ok(current) = socket.recv_buffer_size() {
                let _ = socket.set_recv_buffer_size(current.saturating_mul(2));
            }
            return;
        };

        let _ = socket.set_recv_buffer_size(requested);
        if let Ok(granted) = socket.recv_buffer_size() {
            if granted < requested {
                warn!(
                    "UDP recv buffer capped by OS: requested {} bytes, granted {}. \
                     Large samples may be dropped; raise net.core.rmem_max.",
                    requested, granted
                );
            }
        }
    }

    pub(crate) fn new(port: u16) -> std::io::Result<Self> {
        let socket = {
            let saddr: SocketAddr = SocketAddr::new("0.0.0.0".parse().unwrap(), port);

            let socket2 = Socket2::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
            Self::apply_recv_buffer_size(&socket2);

            // println!("[socket2] new_listener recv_buffer_size: {:?}", socket2.recv_buffer_size());

            let sock_addr = SockAddr::from(saddr);
            socket2.bind(&sock_addr)?;
            socket2.set_nonblocking(true)?;

            let std_socket: StdUdpSocket = socket2.into();
            std_socket.set_nonblocking(true)?;

            mio::net::UdpSocket::from_std(std_socket)
        };

        Ok(Self {
            socket: Some(socket),
            port,
            recv_arena: new_recv_arena(),
            multicast_group_handle: None,
        })
    }

    pub(crate) fn port(&self) -> u16 {
        self.port
    }

    fn get_interface_address(port: u16) -> Vec<Locator> {
        let interface_address_list = NetworkInterface::show()
            .expect("Could not scan interfaces")
            .into_iter()
            .flat_map(|i| {
                i.addr.into_iter().filter(|a| match a {
                    #[rustfmt::skip]
                    Addr::V4(_) => true,
                    _ => false,
                })
            });

        interface_address_list.clone().map(|a| Locator::from_ip_and_port(&a, port as u32)).collect()
    }

    /// Multicast listener for discovery traffic.
    ///
    /// Every participant needs discovery reception, so a failed join on the
    /// sending interface is reported but still leaves the remaining interfaces
    /// receiving.
    pub(crate) fn new_discovery_multicast(
        port: u16,
        working_ips: &[String],
        send_interface_ip: Option<Ipv4Addr>,
    ) -> std::io::Result<Self> {
        Self::new_multicast(port, working_ips, send_interface_ip, false, Some(&MULTICAST_IP))
    }

    /// Multicast listener for user data.
    ///
    /// The groups are chosen per DataReader, so the socket is bound without any
    /// membership and every group is joined afterwards through
    /// [`Self::join_multicast_group`] on the retained handle.
    pub(crate) fn new_user_multicast(port: u16, working_ips: &[String]) -> std::io::Result<Self> {
        Self::new_multicast(port, working_ips, None, false, None)
    }

    /// Joining on the interface multicast is sent from matters for the
    /// self-transmission case: sending and receiving on different interfaces
    /// looks like a successful send that nobody ever receives. The remaining
    /// working interfaces are joined individually so reception does not depend
    /// on the OS default route being usable.
    pub(crate) fn join_multicast_group(
        socket: &Socket2,
        group: &Ipv4Addr,
        working_ips: &[String],
        send_interface_ip: Option<Ipv4Addr>,
        send_interface_required: bool,
    ) -> std::io::Result<()> {
        if let Some(addr) = send_interface_ip {
            if let Err(e) = socket.join_multicast_v4(group, &addr) {
                if send_interface_required {
                    return Err(e);
                }
                error!("Fail - join_multicast_v4 on sending interface : {:?} {:?}", e, addr);
            }
        }

        for ip in working_ips {
            match ip.parse::<std::net::Ipv4Addr>() {
                Ok(addr) => {
                    // Joining twice on one interface fails; it is already done above.
                    if Some(addr) == send_interface_ip {
                        continue;
                    }
                    socket.join_multicast_v4(group, &addr).unwrap_or_else(|e| {
                        error!("Fail - join_multicast_v4 : {:?} {:?}", e, addr);
                    });
                }
                Err(e) => {
                    log::warn!("Skipping non-IPv4 address: {} ({})", ip, e);
                }
            }
        }

        Ok(())
    }

    /// Hand over the duplicate handle on the same kernel socket. Registration
    /// needs `&mut` on the mio socket, which the poll thread takes ownership of,
    /// so group membership has to be changed through this separate handle.
    pub(crate) fn take_multicast_group_handle(&mut self) -> Option<Socket2> {
        self.multicast_group_handle.take()
    }

    fn new_multicast(
        port: u16,
        working_ips: &[String],
        send_interface_ip: Option<Ipv4Addr>,
        send_interface_required: bool,
        group: Option<&Ipv4Addr>,
    ) -> std::io::Result<Self> {
        let socket = Socket2::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        socket.set_reuse_address(true)?;
        socket.set_broadcast(true)?;
        #[cfg(unix)]
        socket.set_reuse_port(true)?;

        Self::apply_recv_buffer_size(&socket);

        if let Some(group) = group {
            Self::join_multicast_group(
                &socket,
                group,
                working_ips,
                send_interface_ip,
                send_interface_required,
            )?;
        }

        let addr: SocketAddr = format!("0.0.0.0:{}", port).parse().unwrap();
        let sock_addr = SockAddr::from(addr);
        socket.bind(&sock_addr)?;
        socket.set_nonblocking(true)?;

        let multicast_group_handle = socket.try_clone()?;
        let udp_socket = mio::net::UdpSocket::from_std(socket.into());

        // //1. std socket bind
        // let std_socket = StdUdpSocket::bind(format!("0.0.0.0:{}", port))?;

        // Try with primary usage ip
        // let mut is_error = false;
        // let working_ip = UdpListener::get_working_ip().unwrap();
        // let addr = working_ip.parse().unwrap();
        // std_socket.join_multicast_v4(&MULTICAST_IP, &addr).unwrap_or_else(|e| {
        //     error!("Fail - join_multicast_v4 : {:?} {:?}", e, addr);
        //     is_error = true;
        // });
        // if !is_error {
        //     GlobalData::get_instance().lock().unwrap().add_metatraffic_unicast_ip(addr.to_string());
        //     GlobalData::get_instance().lock().unwrap().add_default_unicast_ip(addr.to_string());
        //     debug!("add_metatraffic_multicast_locator : {:?}", working_ip);
        // }
        // debug!("join_multicast_v4 : {:?} {:?}", MULTICAST_IP.to_string(), addr);
        // std_socket.set_nonblocking(true)?;
        // let socket = mio::net::UdpSocket::from_std(std_socket);

        //2. Configure for all NICs of all IPs
        // let interface_address_list = UdpListener::get_interface_address(port);
        // for interface_address in interface_address_list {
        //     let addr = interface_address.to_ip_v4_addr();

        //     let mut is_error = false;
        //     std_socket.join_multicast_v4(&MULTICAST_IP, &addr).unwrap_or_else(|e| {
        //         eprintln!("Fail - join_multicast_v4 : {:?} {:?}", e, addr);
        //         is_error = true;
        //     });
        //     if !is_error {
        //         if addr.to_string() == "127.0.0.1".to_string() {
        //             continue;
        //         }
        //         GlobalData::get_instance()
        //             .lock()
        //             .unwrap()
        //             .add_metatraffic_unicast_ip(addr.to_string());
        //         GlobalData::get_instance().lock().unwrap().add_default_unicast_ip(addr.to_string());
        //         println!("add_metatraffic_multicast_locator : {:?}", interface_address);
        //     }
        //     println!("join_multicast_v4 : {:?} {:?}", MULTICAST_IP.to_string(), addr);
        // }
        // let socket = mio::net::UdpSocket::from_std(std_socket);

        //2. mio socket bind
        // let socket = {
        //     let saddr: SocketAddr = SocketAddr::new("0.0.0.0".parse().unwrap(), port);
        //     match mio::net::UdpSocket::bind(saddr) {
        //         Ok(socket) => socket,
        //         Err(e) => {
        //             eprintln!("socket bind(port : {}) error : {:?}", port, e);
        //             return Err(e);
        //         }
        //     }
        // };
        // // socket.set_multicast_loop_v4(true)?;
        // let interface_address_list = UdpListener::get_interface_address(port);
        // println!("interface_address_list: {:?}", interface_address_list);

        // for interface_address in interface_address_list {
        //     let addr = interface_address.to_ip_v4_addr();
        //     println!("join_multicast_v4 : {:?} {:?}", ip.to_string(), addr);
        //     socket.join_multicast_v4(&Ipv4Addr::new(239, 255, 0, 1), &addr).unwrap_or_else(|e| {
        //         eprintln!("Fail - join_multicast_v4 : {:?} {:?}", e, addr);
        //     });
        // }

        Ok(Self {
            socket: Some(udp_socket),
            port,
            recv_arena: new_recv_arena(),
            multicast_group_handle: Some(multicast_group_handle),
        })
    }

    pub(crate) fn socket(&mut self) -> &mut mio::net::UdpSocket {
        self.socket.as_mut().unwrap()
    }

    pub(crate) fn get_message(&mut self) -> Option<(Bytes, SocketAddr)> {
        // The arena owns recv: it routes the syscall through a reusable scratch
        // buffer, then hands back a zero-copy Bytes view into its chunk.
        match self.recv_arena.recv_from(self.socket.as_ref().unwrap()) {
            Ok(pair) => Some(pair),
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => None,
            Err(e) => {
                error!("UDPListener::get_message failed: {e:?}");
                None
            }
        }
    }

    pub(crate) fn close(&mut self) {
        if let Some(socket) = self.socket.take() {
            if let Ok(interfaces) = NetworkInterface::show() {
                let interface_address_list: Vec<Locator> = interfaces
                    .into_iter()
                    .flat_map(|i| i.addr.into_iter().filter(|a| matches!(a, Addr::V4(_))))
                    .map(|a| Locator::from_ip_and_port(&a, self.port as u32))
                    .collect();

                if let Ok(local_addr) = socket.local_addr() {
                    let ip = local_addr.ip();
                    for interface_address in interface_address_list {
                        let addr = interface_address.to_ip_v4_addr();
                        if ip == addr {
                            if let Err(e) =
                                socket.leave_multicast_v4(&Ipv4Addr::new(239, 255, 0, 1), &addr)
                            {
                                debug!(
                                    "leave_multicast_v4 failed (may be unicast socket): {:?}",
                                    e
                                );
                            }
                            break;
                        }
                    }
                }
            } else {
                debug!("Could not scan interfaces during close, skipping multicast leave");
            }
            drop(socket);
        }
    }
}
