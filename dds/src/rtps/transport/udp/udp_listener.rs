#![allow(dead_code)]
#![allow(unused_variables)]

use bytes::Bytes;
use core::net::{Ipv4Addr, SocketAddr};
use log::{debug, error};
use network_interface::{Addr, NetworkInterface, NetworkInterfaceConfig};
use socket2::{Domain, Protocol, SockAddr, Socket as Socket2, Type};
use std::net::UdpSocket as StdUdpSocket;

use crate::rtps::common::locator::{Locator, MULTICAST_IP};
use crate::rtps::transport::port_manager::PortManager;
use crate::rtps::transport::udp::recv_arena::{
    RecvArena, DEFAULT_RECV_ARENA_CHUNK_BYTES, MAX_UDP_PACKET_BYTES,
};
use crate::rtps::transport::udp::socket_buffer::configure_recv_buffer;

// One arena per listener; reused across every incoming datagram on this socket.
fn new_recv_arena() -> RecvArena {
    RecvArena::new(DEFAULT_RECV_ARENA_CHUNK_BYTES, MAX_UDP_PACKET_BYTES)
}

fn is_port_in_use(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::AddrInUse
}

fn unicast_bind_error(port: u16, participant_id: u32, error: std::io::Error) -> std::io::Error {
    std::io::Error::new(
        error.kind(),
        format!("Failed to bind unicast port {port} for participant id {participant_id}: {error}"),
    )
}

#[derive(Debug)]
pub(crate) struct UdpListener {
    port: u16,
    socket: Option<mio::net::UdpSocket>,
    recv_arena: RecvArena,
    // What the kernel actually granted (getsockopt, post-set) - not what was requested.
    // Linux doubles a set size after capping it; an untouched socket reports rmem_default.
    recv_buffer_size: Option<usize>,
}

impl Drop for UdpListener {
    fn drop(&mut self) {
        self.close();
    }
}

impl UdpListener {
    // Binds the discovery and user unicast listeners of one participant id and
    // returns them with the id. A searched id moves on while a port is in use.
    pub(crate) fn bind_discovery_and_user_unicast_listeners(
        port_manager: &PortManager,
        domain_id: u32,
        first_participant_id: u32,
        is_participant_id_fixed: bool,
    ) -> std::io::Result<(Self, Self, u32)> {
        let max_participant_id = port_manager.max_unicast_participant_id()?;

        if first_participant_id > max_participant_id {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "participant id {} is past the last unicast slot {} of domain {}",
                    first_participant_id, max_participant_id, domain_id
                ),
            ));
        }

        let last_participant_id =
            if is_participant_id_fixed { first_participant_id } else { max_participant_id };

        for participant_id in first_participant_id..=last_participant_id {
            let discovery_port =
                port_manager.calculate_discovery_traffic_unicast_port(domain_id, participant_id)?;
            let user_port =
                port_manager.calculate_user_traffic_unicast_port(domain_id, participant_id)?;

            let discovery_listener = match Self::new(discovery_port) {
                Ok(listener) => listener,
                Err(error) if is_port_in_use(&error) && !is_participant_id_fixed => {
                    log::info!(
                        "[UdpListener] Discovery port {} in use, trying participant id {}",
                        discovery_port,
                        participant_id + 1
                    );
                    continue;
                }
                Err(error) => {
                    return Err(unicast_bind_error(discovery_port, participant_id, error))
                }
            };

            match Self::new(user_port) {
                Ok(user_listener) => {
                    return Ok((discovery_listener, user_listener, participant_id));
                }
                Err(error) if is_port_in_use(&error) && !is_participant_id_fixed => {
                    log::info!(
                        "[UdpListener] User port {} in use, closing discovery and trying participant id {}",
                        user_port,
                        participant_id + 1
                    );

                    // Release this id's discovery port so both ports are rebound at the next id
                    drop(discovery_listener);
                }
                Err(error) => return Err(unicast_bind_error(user_port, participant_id, error)),
            }
        }

        Err(std::io::Error::new(
            std::io::ErrorKind::AddrInUse,
            format!(
                "No free unicast port pair in domain {}: participant ids {}..={} are all in use",
                domain_id, first_participant_id, last_participant_id
            ),
        ))
    }

    pub(crate) fn new(port: u16) -> std::io::Result<Self> {
        let recv_buffer_size;
        let bound_port;
        let socket = {
            let saddr: SocketAddr = SocketAddr::new("0.0.0.0".parse().unwrap(), port);

            let socket2 = Socket2::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
            // Granted size, read back: the OS may silently cap the request.
            recv_buffer_size = configure_recv_buffer(&socket2);

            let sock_addr = SockAddr::from(saddr);
            socket2.bind(&sock_addr)?;

            // Port the OS bound, which differs from the requested port when it is 0.
            bound_port = socket2.local_addr()?.as_socket().map_or(port, |addr| addr.port());
            socket2.set_nonblocking(true)?;

            let std_socket: StdUdpSocket = socket2.into();
            std_socket.set_nonblocking(true)?;

            mio::net::UdpSocket::from_std(std_socket)
        };

        Ok(Self {
            socket: Some(socket),
            port: bound_port,
            recv_arena: new_recv_arena(),
            recv_buffer_size,
        })
    }

    pub(crate) fn port(&self) -> u16 {
        self.port
    }

    /// What `getsockopt(SO_RCVBUF)` reported after the buffer was configured -
    /// the value actually granted. `None` if the read-back failed.
    pub(crate) fn recv_buffer_size(&self) -> Option<usize> {
        self.recv_buffer_size
    }

    fn get_interface_address(port: u16) -> Vec<Locator> {
        let interface_address_list = NetworkInterface::show()
            .expect("Could not scan interfaces")
            .into_iter()
            .flat_map(|i| i.addr.into_iter().filter(|a| matches!(a, Addr::V4(_))));

        interface_address_list.clone().map(|a| Locator::from_ip_and_port(&a, port as u32)).collect()
    }

    pub(crate) fn new_multicast(port: u16, working_ips: &[String]) -> std::io::Result<Self> {
        let socket = Socket2::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        socket.set_reuse_address(true)?;
        socket.set_broadcast(true)?;
        #[cfg(unix)]
        socket.set_reuse_port(true)?;

        let recv_buffer_size = configure_recv_buffer(&socket);

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
        socket.bind(&sock_addr).map_err(|e| {
            std::io::Error::new(e.kind(), format!("bind multicast port {}: {}", port, e))
        })?;
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
    // Linux doubles a granted SO_RCVBUF, so requested != granted.
    #[test]
    fn recv_buffer_size_reports_the_post_set_kernel_grant() {
        let listener = UdpListener::new(0).expect("bind ephemeral port");
        let granted = listener.recv_buffer_size().expect("kernel reported a value");

        // Independent oracle: the same set-then-read-back sequence hand-rolled on a throwaway
        // socket rather than through UdpListener::new, so a broken capture cannot also break
        // this. It has to follow the same request as the listener, or a run that sets an
        // explicit size compares two different requests. Where the kernel clamps both to the
        // same ceiling that mismatch hides; where it grants what is asked, it does not.
        let probe = Socket2::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)).unwrap();
        let current = probe.recv_buffer_size().unwrap();
        if let Some(size) = crate::rtps::transport::udp::socket_buffer::recv_buffer_request()
            .size_to_request(current)
        {
            let _ = probe.set_recv_buffer_size(size);
        }
        let expected = probe.recv_buffer_size().unwrap();

        assert_eq!(granted, expected);
    }

    #[test]
    fn port_reports_the_os_assigned_port_for_port_zero() {
        let listener = UdpListener::new(0).expect("bind ephemeral port");
        let bound_port = listener.socket.as_ref().unwrap().local_addr().unwrap().port();

        assert_ne!(listener.port(), 0);
        assert_eq!(listener.port(), bound_port);
    }

    #[test]
    fn unicast_listener_search_moves_past_an_id_whose_ports_are_in_use() {
        let domain_id = crate::test_utils::unique_domain_id() as u32;
        let ports = PortManager::default();

        // Another process may already hold lower ids of this domain, so only the
        // step between two searches and the fixed-id failure are asserted.
        let (first_discovery, first_user, first_id) =
            UdpListener::bind_discovery_and_user_unicast_listeners(&ports, domain_id, 0, false)
                .expect("first listeners");

        let (_, _, second_id) =
            UdpListener::bind_discovery_and_user_unicast_listeners(&ports, domain_id, 0, false)
                .expect("second listeners");
        assert_eq!(second_id, first_id + 1);

        let error = UdpListener::bind_discovery_and_user_unicast_listeners(
            &ports, domain_id, first_id, true,
        )
        .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::AddrInUse);
        assert!(error.to_string().contains(&format!("participant id {first_id}")), "{error}");

        drop((first_discovery, first_user));
    }

    #[test]
    fn unicast_listeners_reject_an_id_past_the_domain_block() {
        let domain_id = crate::test_utils::unique_domain_id() as u32;
        let ports = PortManager::default();

        let error =
            UdpListener::bind_discovery_and_user_unicast_listeners(&ports, domain_id, 120, false)
                .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);

        let error =
            UdpListener::bind_discovery_and_user_unicast_listeners(&ports, domain_id, 120, true)
                .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    }

    #[test]
    fn unicast_listeners_reject_a_domain_gain_that_leaves_no_unicast_slot() {
        let domain_id = crate::test_utils::unique_domain_id() as u32;

        // d3 = 11 does not fit below DG = 10, so participant id 0 already lands in the next domain
        let ports = PortManager { domain_id_gain: 10, ..PortManager::default() };

        let error =
            UdpListener::bind_discovery_and_user_unicast_listeners(&ports, domain_id, 0, false)
                .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    }
}
