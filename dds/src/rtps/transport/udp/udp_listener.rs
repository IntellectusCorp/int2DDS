#![allow(dead_code)]
#![allow(unused_variables)]

use bytes::Bytes;
use core::net::{Ipv4Addr, SocketAddr};
use log::{debug, error};
use network_interface::{Addr, NetworkInterface, NetworkInterfaceConfig};
use socket2::{Domain, Protocol, SockAddr, Socket as Socket2, Type};
use std::env;
use std::net::UdpSocket as StdUdpSocket;

use crate::rtps::common::locator::{Locator, MULTICAST_IP};
use crate::rtps::transport::udp::recv_arena::{
    RecvArena, DEFAULT_RECV_ARENA_CHUNK_BYTES, MAX_UDP_PACKET_BYTES,
};
use crate::rtps::transport::Listener;

// One arena per listener; reused across every incoming datagram on this socket.
fn new_recv_arena() -> RecvArena {
    RecvArena::new(DEFAULT_RECV_ARENA_CHUNK_BYTES, MAX_UDP_PACKET_BYTES)
}

#[derive(Debug)]
pub(crate) struct UdpListener {
    port: u16,
    socket: Option<mio::net::UdpSocket>,
    recv_arena: RecvArena,
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

    pub(crate) fn new(port: u16) -> std::io::Result<Self> {
        let socket = {
            let saddr: SocketAddr = SocketAddr::new("0.0.0.0".parse().unwrap(), port);

            let socket2 = Socket2::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
            if let Some(size) = Self::get_socket_buffer_size() {
                let _ = socket2.set_recv_buffer_size(size);
            } else if let Ok(current) = socket2.recv_buffer_size() {
                let new_size = current.saturating_mul(2);
                let _ = socket2.set_recv_buffer_size(new_size);
            }

            // println!("[socket2] new_listener recv_buffer_size: {:?}", socket2.recv_buffer_size());

            let sock_addr = SockAddr::from(saddr);
            socket2.bind(&sock_addr)?;
            socket2.set_nonblocking(true)?;

            let std_socket: StdUdpSocket = socket2.into();
            std_socket.set_nonblocking(true)?;

            mio::net::UdpSocket::from_std(std_socket)
        };

        Ok(Self { socket: Some(socket), port, recv_arena: new_recv_arena() })
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

    pub(crate) fn new_multicast(port: u16, working_ips: &[String]) -> std::io::Result<Self> {
        let socket = Socket2::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        socket.set_reuse_address(true)?;
        socket.set_broadcast(true)?;
        #[cfg(unix)]
        socket.set_reuse_port(true)?;

        if let Some(size) = Self::get_socket_buffer_size() {
            let _ = socket.set_recv_buffer_size(size);
        } else if let Ok(current) = socket.recv_buffer_size() {
            let new_size = current.saturating_mul(2);
            let _ = socket.set_recv_buffer_size(new_size);
        }

        // Join multicast group on each working interface individually,
        // so multicast works regardless of OS default route availability.
        for ip in working_ips {
            let addr: std::net::Ipv4Addr = ip.parse().unwrap();
            socket.join_multicast_v4(&MULTICAST_IP, &addr).unwrap_or_else(|e| {
                error!("Fail - join_multicast_v4 : {:?} {:?}", e, addr);
            });
        }

        let addr: SocketAddr = format!("0.0.0.0:{}", port).parse().unwrap();
        let sock_addr = SockAddr::from(addr);
        socket.bind(&sock_addr)?;
        socket.set_nonblocking(true)?;

        let socket = mio::net::UdpSocket::from_std(socket.into());

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

        Ok(Self { socket: Some(socket), port, recv_arena: new_recv_arena() })
    }

    pub(crate) fn socket(&mut self) -> &mut mio::net::UdpSocket {
        self.socket.as_mut().unwrap()
    }

    pub(crate) fn get_message(&mut self) -> Option<(Bytes, SocketAddr)> {
        // Reserve a slot inside the arena's current chunk; this is the buffer recv_from writes into.
        let slot = self.recv_arena.reserve_packet_slot();
        match self.socket.as_mut().unwrap().recv_from(slot) {
            Ok((nbytes, sender)) => {
                // Freeze only the bytes actually written; the zero-filled tail is truncated so the next reservation can reuse the capacity.
                let bytes = self.recv_arena.commit_received_packet(nbytes);
                Some((bytes, sender))
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // Nothing to read; give back the slot so idle polls don't accumulate dead tails.
                self.recv_arena.release_unused_slot();
                None
            }
            Err(e) => {
                error!("UDPListener::get_message failed: {e:?}");
                self.recv_arena.release_unused_slot();
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

/// Implementation of Listener trait for UdpListener
///
/// This allows UdpListener to be used through the generic Listener interface,
/// enabling transport-agnostic listener management.
impl Listener for UdpListener {
    fn socket_udp(&mut self) -> Option<&mut mio::net::UdpSocket> {
        self.socket.as_mut()
    }

    fn socket_tcp(&mut self) -> Option<&mut mio::net::TcpListener> {
        // UDP listener doesn't have a TCP socket
        None
    }

    fn port(&self) -> u16 {
        self.port
    }

    fn close(&mut self) {
        // Call the UdpListener's own close method
        UdpListener::close(self)
    }
}
