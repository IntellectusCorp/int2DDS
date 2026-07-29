//! Network locator types for RTPS endpoint addressing.
//!
//! This module defines `Locator` for identifying network endpoints in RTPS communication.
//! Supports UDP and TCP transports with IPv4/IPv6 addressing. Locators specify both the
//! transport type and network address for sending and receiving RTPS messages.
use crate::dcps::topic::type_support::DdsType;

use std::{fmt::Debug, net::IpAddr};

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

    /// Create a TCP locator that carries BOTH ports.
    ///
    /// The RTPS `port` field holds the logical (RTPS) port a peer must reserve
    /// to reach this endpoint over the mux, while the physical listener port is
    /// packed into the first two address bytes. The IPv4 address stays in the
    /// last four bytes. Peers dial `tcp_physical_port()` and reserve
    /// `tcp_logical_port()`, so the logical port no longer has to be guessed
    /// from the receiver's participant id.
    pub fn from_tcp_v4_dual(ip_addr: Ipv4Addr, logical_port: u16, physical_port: u16) -> Self {
        let mut address = [0u8; 16];
        address[0..2].copy_from_slice(&physical_port.to_be_bytes());
        address[12..16].copy_from_slice(&ip_addr.octets());
        Self::new(LOCATOR_KIND_TCP_V4, logical_port as u32, address)
    }

    /// Logical (RTPS) port of a TCP locator — the value in the `port` field.
    pub fn tcp_logical_port(&self) -> u16 {
        self.port as u16
    }

    /// Physical listener port of a TCP locator, decoded from the first two
    /// address bytes (see `from_tcp_v4_dual`).
    pub fn tcp_physical_port(&self) -> u16 {
        u16::from_be_bytes([self.address[0], self.address[1]])
    }

    /// Port a peer is actually reached at over the wire.
    ///
    /// For UDP the `port` field already holds the physical port. But for a TCP v4
    /// dual-port locator the `port` field carries the logical port. So the
    /// physical listener port packed in the address bytes is returned instead.
    pub fn access_port(&self) -> u32 {
        if self.kind == LOCATOR_KIND_TCP_V4 {
            self.tcp_physical_port() as u32
        } else {
            self.port
        }
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
