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

// One arena per listener; reused across every incoming datagram on this socket.
fn new_recv_arena() -> RecvArena {
    RecvArena::new(DEFAULT_RECV_ARENA_CHUNK_BYTES, MAX_UDP_PACKET_BYTES)
}

#[derive(Debug)]
pub(crate) struct UdpListener {
    port: u16,
    socket: Option<mio::net::UdpSocket>,
    recv_arena: RecvArena,
    // What the kernel actually granted (getsockopt, post-set) - not what was
    // requested. Linux clamps to net.core.rmem_max and then doubles the result.
    recv_buffer_size: Option<usize>,
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
        let recv_buffer_size;
        let socket = {
            let saddr: SocketAddr = SocketAddr::new("0.0.0.0".parse().unwrap(), port);

            let socket2 = Socket2::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
            if let Some(size) = Self::get_socket_buffer_size() {
                let _ = socket2.set_recv_buffer_size(size);
            } else if let Ok(current) = socket2.recv_buffer_size() {
                let new_size = current.saturating_mul(2);
                let _ = socket2.set_recv_buffer_size(new_size);
            }

            // Re-read via getsockopt rather than trust what was requested: the OS
            // may silently cap it (see set_recv_buffer_size above).
            recv_buffer_size = socket2.recv_buffer_size().ok();

            let sock_addr = SockAddr::from(saddr);
            socket2.bind(&sock_addr)?;
            socket2.set_nonblocking(true)?;

            let std_socket: StdUdpSocket = socket2.into();
            std_socket.set_nonblocking(true)?;

            mio::net::UdpSocket::from_std(std_socket)
        };

        Ok(Self { socket: Some(socket), port, recv_arena: new_recv_arena(), recv_buffer_size })
    }

    pub(crate) fn port(&self) -> u16 {
        self.port
    }

    /// What `getsockopt(SO_RCVBUF)` reported right after this listener bound -
    /// the value actually granted, already doubled by Linux. `None` if the
    /// read-back failed.
    pub(crate) fn recv_buffer_size(&self) -> Option<usize> {
        self.recv_buffer_size
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
        let recv_buffer_size = socket.recv_buffer_size().ok();

        // Join multicast group on each working interface individually,
        // so multicast works regardless of OS default route availability.
        for ip in working_ips {
            match ip.parse::<std::net::Ipv4Addr>() {
                Ok(addr) => {
                    socket.join_multicast_v4(&MULTICAST_IP, &addr).unwrap_or_else(|e| {
                        error!("Fail - join_multicast_v4 : {:?} {:?}", e, addr);
                    });
                }
                Err(e) => {
                    log::warn!("Skipping non-IPv4 address: {} ({})", ip, e);
                }
            }
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

        Ok(Self { socket: Some(socket), port, recv_arena: new_recv_arena(), recv_buffer_size })
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

#[cfg(test)]
mod tests {
    use super::*;

    // Pins the fix this task is about: recv_buffer_size() must be what
    // getsockopt reads back after set_recv_buffer_size, not the pre-set value -
    // Linux always doubles a granted SO_RCVBUF, so requested != granted.
    #[test]
    fn recv_buffer_size_reports_the_post_set_kernel_grant() {
        let listener = UdpListener::new(0).expect("bind ephemeral port");
        let granted = listener.recv_buffer_size().expect("kernel reported a value");

        // Independent oracle: the same set-then-read-back sequence hand-rolled on a throwaway
        // socket rather than through UdpListener::new, so a broken capture cannot also break
        // this. It has to branch on the env exactly as the listener does, or a run that sets
        // INT2DDS_UDP_SOCKET_BUFFER compares two different requests. Where the kernel clamps
        // both to the same ceiling that mismatch hides; where it grants what is asked, it does
        // not.
        let probe = Socket2::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)).unwrap();
        if let Some(size) = std::env::var("INT2DDS_UDP_SOCKET_BUFFER")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
        {
            let _ = probe.set_recv_buffer_size(size);
        } else if let Ok(current) = probe.recv_buffer_size() {
            let _ = probe.set_recv_buffer_size(current.saturating_mul(2));
        }
        let expected = probe.recv_buffer_size().unwrap();

        assert_eq!(granted, expected);
    }
}
