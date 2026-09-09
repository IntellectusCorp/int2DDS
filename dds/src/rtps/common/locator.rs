//! Network locator types for RTPS endpoint addressing.
//!
//! This module defines `Locator` for identifying network endpoints in RTPS communication.
//! Supports UDP and TCP transports with IPv4/IPv6 addressing. Locators specify both the
//! transport type and network address for sending and receiving RTPS messages.
use crate::dcps::topic::type_support::DdsType;

use std::{collections::HashSet, fmt::Debug, net::IpAddr, sync::OnceLock};

use network_interface::Addr;
use speedy::{Endianness, Readable, Writable};
use std::net::Ipv4Addr;

pub const MULTICAST_IP: Ipv4Addr = Ipv4Addr::new(239, 255, 0, 1);

pub const LOCATOR_KIND_INVALID: i32 = -1;
pub const LOCATOR_KIND_RESERVED: i32 = 0;
pub const LOCATOR_KIND_UDP_V4: i32 = 1; // LOCATOR_KIND_UDPv4
pub const LOCATOR_KIND_UDP_V6: i32 = 2; // LOCATOR_KIND_UDPv6
pub const LOCATOR_KIND_TCP_V4: i32 = 4; // LOCATOR_KIND_TCPv4 (RTPS standard)
pub const LOCATOR_KIND_TCP_V6: i32 = 8; // LOCATOR_KIND_TCPv6 (RTPS standard)
pub const LOCATOR_KIND_SHM: i32 = 16; // LOCATOR_KIND_SHM (Shared Memory)
pub const LOCATOR_PORT_INVALID: u32 = 0;

pub const LOCATOR_ADDRESS_INVALID: [u8; 16] = [0; 16];
pub const LOCATOR_INVALID: Locator = Locator {
    kind: LOCATOR_KIND_INVALID,
    port: LOCATOR_PORT_INVALID,
    address: LOCATOR_ADDRESS_INVALID,
};

#[derive(DdsType, Eq, Hash)]
#[dds_type(crate_path = "crate", no_default)]
pub struct Locator {
    kind: i32, // -1: invalid, 1: UDPv4, 2: UDPv6
    port: u32,
    pub(crate) address: [u8; 16], // IPv4 uses upper 12 bytes as 0
}
impl Locator {
    pub fn new(kind: i32, port: u32, address: [u8; 16]) -> Self {
        Self { kind, port, address }
    }
    // By 9.3.2.4 Locator_t
    pub fn from_ip<I: Into<std::net::IpAddr>>(ip_addr: I, port: u32) -> Self {
        let ip_addr = ip_addr.into();

        match ip_addr {
            std::net::IpAddr::V4(ipv4) => {
                let mut address = [0u8; 16];
                address[12..16].copy_from_slice(&ipv4.octets());
                Self::new(LOCATOR_KIND_UDP_V4, port, address)
            }
            std::net::IpAddr::V6(ipv6) => Self::new(LOCATOR_KIND_UDP_V6, port, ipv6.octets()),
        }
    }
    pub fn to_ip_v4_addr(&self) -> std::net::Ipv4Addr {
        std::net::Ipv4Addr::new(
            self.address[12],
            self.address[13],
            self.address[14],
            self.address[15],
        )
    }

    pub fn to_ip_v4_addr_string(&self) -> String {
        let ip_addr = self.to_ip_v4_addr();

        ip_addr.to_string()
    }

    pub fn from_ip_v4_addr_and_port(ip_addr: &Ipv4Addr, port: u32) -> Self {
        Self {
            kind: LOCATOR_KIND_UDP_V4,
            port,
            address: [
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                ip_addr.octets()[0],
                ip_addr.octets()[1],
                ip_addr.octets()[2],
                ip_addr.octets()[3],
            ],
        }
    }

    /// Create a SHM locator from IP address and port
    /// Uses LOCATOR_KIND_SHM to indicate shared memory transport
    pub fn from_shm(ip_addr: &Ipv4Addr, port: u32) -> Self {
        Self {
            kind: LOCATOR_KIND_SHM,
            port,
            address: [
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                ip_addr.octets()[0],
                ip_addr.octets()[1],
                ip_addr.octets()[2],
                ip_addr.octets()[3],
            ],
        }
    }

    pub fn from_ip_and_port(ip_addr: &Addr, port: u32) -> Self {
        match ip_addr.ip() {
            IpAddr::V4(a) => Self {
                kind: LOCATOR_KIND_UDP_V4,
                port,
                address: [
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    a.octets()[0],
                    a.octets()[1],
                    a.octets()[2],
                    a.octets()[3],
                ],
            },
            IpAddr::V6(a) => Self { kind: LOCATOR_KIND_UDP_V6, port, address: a.octets() },
        }
    }

    pub fn kind(&self) -> i32 {
        self.kind
    }

    pub fn port(&self) -> u32 {
        self.port
    }

    /// Create a TCP locator from IPv4 address and port
    ///
    /// # Arguments
    /// * `ip_addr` - IPv4 address
    /// * `port` - Port number
    ///
    /// # Returns
    /// A new Locator with TCP_V4 kind
    ///
    /// # Example
    /// ```ignore
    /// let locator = Locator::from_tcp_v4(Ipv4Addr::new(192, 168, 1, 100), 7400);
    /// assert!(locator.is_tcp());
    /// ```
    pub fn from_tcp_v4(ip_addr: Ipv4Addr, port: u32) -> Self {
        let mut address = [0u8; 16];
        address[12..16].copy_from_slice(&ip_addr.octets());
        Self::new(LOCATOR_KIND_TCP_V4, port, address)
    }

    /// Port a peer is reached at over the wire. TCP now uses the standard
    /// locator port directly; traffic kind is carried in each frame.
    pub fn access_port(&self) -> u32 {
        self.port
    }

    /// Create a TCP locator from IPv6 address and port
    ///
    /// # Arguments
    /// * `ip_addr` - IPv6 address
    /// * `port` - Port number
    ///
    /// # Returns
    /// A new Locator with TCP_V6 kind
    pub fn from_tcp_v6(ip_addr: std::net::Ipv6Addr, port: u32) -> Self {
        Self::new(LOCATOR_KIND_TCP_V6, port, ip_addr.octets())
    }

    /// Check if this locator is a TCP locator
    ///
    /// # Returns
    /// `true` if kind is TCP_V4 or TCP_V6, `false` otherwise
    ///
    /// # Example
    /// ```ignore
    /// let tcp_locator = Locator::from_tcp_v4(Ipv4Addr::new(127, 0, 0, 1), 7400);
    /// assert!(tcp_locator.is_tcp());
    ///
    /// let udp_locator = Locator::from_ip_v4_addr_and_port(&Ipv4Addr::new(127, 0, 0, 1), 7400);
    /// assert!(!udp_locator.is_tcp());
    /// ```
    pub fn is_tcp(&self) -> bool {
        self.kind == LOCATOR_KIND_TCP_V4 || self.kind == LOCATOR_KIND_TCP_V6
    }

    /// Check if this locator is a UDP locator
    ///
    /// # Returns
    /// `true` if kind is UDP_V4 or UDP_V6, `false` otherwise
    ///
    /// # Example
    /// ```ignore
    /// let udp_locator = Locator::from_ip_v4_addr_and_port(&Ipv4Addr::new(127, 0, 0, 1), 7400);
    /// assert!(udp_locator.is_udp());
    ///
    /// let tcp_locator = Locator::from_tcp_v4(Ipv4Addr::new(127, 0, 0, 1), 7400);
    /// assert!(!tcp_locator.is_udp());
    /// ```
    pub fn is_udp(&self) -> bool {
        self.kind == LOCATOR_KIND_UDP_V4 || self.kind == LOCATOR_KIND_UDP_V6
    }

    /// Check if this locator is a Shared Memory locator
    ///
    /// # Returns
    /// `true` if kind is SHM, `false` otherwise
    pub fn is_shm(&self) -> bool {
        self.kind == LOCATOR_KIND_SHM
    }

    /// Short human-readable name of this locator's kind (e.g. "UDP", "TCP",
    /// "SHM", "UNKNOWN"). Used by transport plugins when reporting an
    /// "unsupported locator kind" error so RTPS-layer logs can format the
    /// rejected kind dynamically without per-kind branching.
    pub fn kind_name(&self) -> &'static str {
        if self.is_shm() {
            "SHM"
        } else if self.is_tcp() {
            "TCP"
        } else if self.is_udp() {
            "UDP"
        } else {
            "UNKNOWN"
        }
    }

    /// Check if this locator is valid (not INVALID or RESERVED)
    ///
    /// # Returns
    /// `true` if the locator has a valid kind, `false` otherwise
    pub fn is_valid(&self) -> bool {
        self.kind != LOCATOR_KIND_INVALID && self.kind != LOCATOR_KIND_RESERVED
    }
}

fn interface_addresses() -> HashSet<IpAddr> {
    if_addrs::get_if_addrs()
        .map(|interfaces| interfaces.iter().map(|interface| interface.ip()).collect())
        .unwrap_or_default()
}

/// Every address this host answers on, fixed at the first time.
fn host_addresses() -> &'static HashSet<IpAddr> {
    static HOST_ADDRESSES: OnceLock<HashSet<IpAddr>> = OnceLock::new();
    HOST_ADDRESSES.get_or_init(interface_addresses)
}

/// Whether the peer that sent `from_addr` runs on this host. Every address of
/// the host counts, not just the ones this participant sends through: the
/// interface chosen for egress says nothing about where a peer runs.
pub(crate) fn is_same_host(from_addr: std::net::SocketAddr) -> bool {
    let sender_ip = from_addr.ip();
    sender_ip.is_loopback() || host_addresses().contains(&sender_ip)
}

/// Every locator whose address this host owns, rewritten to loopback and the
/// duplicates that collapses into removed. Such an address never reached the
/// peer anyway, so the rest are left as announced and stay reachable.
pub(crate) fn loopback_locators(locators: &[Locator]) -> Vec<Locator> {
    let host_addresses = host_addresses();

    let mut redirected: Vec<Locator> = Vec::with_capacity(locators.len());

    for locator in locators {
        let announced = match locator.kind {
            LOCATOR_KIND_UDP_V4 | LOCATOR_KIND_TCP_V4 => Some(IpAddr::V4(locator.to_ip_v4_addr())),
            LOCATOR_KIND_UDP_V6 | LOCATOR_KIND_TCP_V6 => {
                Some(IpAddr::V6(std::net::Ipv6Addr::from(locator.address)))
            }
            _ => None,
        };
        let owned_by_this_host = announced
            .is_some_and(|address| address.is_loopback() || host_addresses.contains(&address));

        let resolved = match locator.kind {
            LOCATOR_KIND_UDP_V4 if owned_by_this_host => {
                Locator::from_ip_v4_addr_and_port(&Ipv4Addr::LOCALHOST, locator.port)
            }
            LOCATOR_KIND_TCP_V4 if owned_by_this_host => {
                Locator::from_tcp_v4(Ipv4Addr::LOCALHOST, locator.port)
            }
            LOCATOR_KIND_UDP_V6 if owned_by_this_host => {
                Locator::from_ip(std::net::Ipv6Addr::LOCALHOST, locator.port)
            }
            LOCATOR_KIND_TCP_V6 if owned_by_this_host => {
                Locator::from_tcp_v6(std::net::Ipv6Addr::LOCALHOST, locator.port)
            }
            _ => locator.clone(),
        };
        if !redirected.contains(&resolved) {
            redirected.push(resolved);
        }
    }

    redirected
}

impl std::fmt::Display for Locator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}:{}", self.kind_name(), self.to_ip_v4_addr_string(), self.port)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Readable, Writable)]
pub struct LocatorUDPv4 {
    address: u32,
    port: u32,
}

impl LocatorUDPv4 {
    pub fn new(address: u32, port: u32) -> Self {
        Self { address, port }
    }

    pub fn to_locator(&self, endianness: Endianness) -> Locator {
        let mut address = [0u8; 16];

        match endianness {
            Endianness::BigEndian => {
                // Store the IPv4 address in big-endian (network byte order)
                address[12..16].copy_from_slice(&self.address.to_be_bytes());
            }
            Endianness::LittleEndian => {
                // Store the IPv4 address in little-endian order
                address[12..16].copy_from_slice(&self.address.to_le_bytes());
            }
        }

        Locator { kind: LOCATOR_KIND_UDP_V4, port: self.port, address }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    /// Reserved for documentation by RFC 5737 and therefore never assigned to
    /// an interface, which is what makes it a peer address this host cannot own.
    const OFF_HOST_ADDRESS: Ipv4Addr = Ipv4Addr::new(203, 0, 113, 1);

    fn interface_address_of_this_host() -> Option<Ipv4Addr> {
        if_addrs::get_if_addrs().ok()?.into_iter().find_map(|interface| match interface.ip() {
            IpAddr::V4(address) if !address.is_loopback() => Some(address),
            _ => None,
        })
    }

    #[test]
    fn a_peer_reaching_us_from_any_address_of_this_host_is_co_located() {
        assert!(is_same_host(SocketAddr::from((Ipv4Addr::LOCALHOST, 7410))));

        // The address a peer sends from need not be the one this participant
        // sends through, so every interface of the host has to count.
        let Some(address) = interface_address_of_this_host() else {
            return;
        };
        assert!(is_same_host(SocketAddr::from((address, 7410))));
    }

    #[test]
    fn a_peer_reaching_us_from_elsewhere_is_not_co_located() {
        assert!(!is_same_host(SocketAddr::from((OFF_HOST_ADDRESS, 7410))));
    }

    #[test]
    fn addresses_of_this_host_collapse_into_one_destination() {
        let announced = vec![
            Locator::from_ip_v4_addr_and_port(&Ipv4Addr::new(127, 0, 0, 1), 7410),
            Locator::from_ip_v4_addr_and_port(&Ipv4Addr::new(127, 0, 0, 5), 7410),
        ];

        assert_eq!(
            loopback_locators(&announced),
            vec![Locator::from_ip_v4_addr_and_port(&Ipv4Addr::LOCALHOST, 7410)]
        );
    }

    #[test]
    fn an_interface_address_of_this_host_is_narrowed() {
        // Nothing to narrow on a host whose only address is the loopback one.
        let Some(address) = interface_address_of_this_host() else {
            return;
        };
        let announced = vec![
            Locator::from_ip_v4_addr_and_port(&address, 7410),
            Locator::from_tcp_v4(address, 7410),
        ];

        assert_eq!(
            loopback_locators(&announced),
            vec![
                Locator::from_ip_v4_addr_and_port(&Ipv4Addr::LOCALHOST, 7410),
                Locator::from_tcp_v4(Ipv4Addr::LOCALHOST, 7410),
            ]
        );
    }

    #[test]
    fn an_address_this_host_does_not_own_is_left_as_announced() {
        let announced = vec![
            Locator::from_ip_v4_addr_and_port(&OFF_HOST_ADDRESS, 7410),
            Locator::from_tcp_v4(OFF_HOST_ADDRESS, 7410),
        ];

        assert_eq!(loopback_locators(&announced), announced);
    }

    /// The point of narrowing one locator at a time: a peer wrongly taken for a
    /// co-located one keeps every address that can still reach it.
    #[test]
    fn an_unreachable_address_is_narrowed_without_costing_the_reachable_one() {
        let announced = vec![
            Locator::from_ip_v4_addr_and_port(&OFF_HOST_ADDRESS, 7410),
            Locator::from_ip_v4_addr_and_port(&Ipv4Addr::new(127, 0, 0, 1), 7410),
        ];

        assert_eq!(
            loopback_locators(&announced),
            vec![
                Locator::from_ip_v4_addr_and_port(&OFF_HOST_ADDRESS, 7410),
                Locator::from_ip_v4_addr_and_port(&Ipv4Addr::LOCALHOST, 7410),
            ]
        );
    }

    #[test]
    fn a_locator_without_a_routable_address_is_left_alone() {
        let announced = vec![Locator::from_shm(&Ipv4Addr::LOCALHOST, 7410)];

        assert_eq!(loopback_locators(&announced), announced);
    }
}
