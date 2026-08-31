//! Per-mode transport configuration derived from `PropertyQosPolicy`.
//!
//! Each transport mode (`UdpConfig`, `TcpConfig`, `HybridConfig`) resolves its
//! own parameters from a participant's `PropertyQosPolicy` via the
//! [`TransportConfig`] trait, so participants in one process are tuned
//! independently.
//!
//! Transport-tunable property keys (e.g. [`PROP_MULTICAST_TTL`]) live in the
//! `dcps::infrastructure::qos_policy` module so they form the public API surface
//! alongside the property setter helpers; this file just consumes them.

use std::net::SocketAddr;
use std::time::Duration;

use crate::dcps::infrastructure::qos_policy::{
    PropertyQosPolicy, PROP_ACCEPT_UNDEFINED_PEERS, PROP_INITIAL_PEERS, PROP_MULTICAST_TTL,
    PROP_TCP_BIND_PORT, PROP_TCP_CONNECT_TIMEOUT_MS, PROP_TCP_KEEPALIVE_INTERVAL_MS,
    PROP_TCP_KEEPALIVE_MAX_MISSES, PROP_TCP_KEEPALIVE_TIMEOUT_MS, PROP_TCP_NODELAY,
    PROP_TCP_PEER_HANDSHAKE_TIMEOUT_MS, PROP_TCP_PUBLIC_ADDRESS, PROP_TCP_SO_RCVBUF,
    PROP_TCP_SO_SNDBUF, PROP_TCP_TLS_HANDSHAKE_TIMEOUT_MS, PROP_TCP_UNACKED_TIMEOUT_MS,
    PROP_TRANSPORT,
};
use crate::rtps::transport::TransportType;

/// IPv4 multicast TTL fallback. Matches RFC 1112 / `IP_MULTICAST_TTL` defaults
/// on Linux and Windows (`1` — link-local only).
pub(crate) const DEFAULT_MULTICAST_TTL: u8 = 1;

pub(crate) trait TransportConfig {
    /// Resolve this mode's config from a participant's `PropertyQosPolicy`,
    /// applying property → env → default precedence per field.
    fn from_property(property: &PropertyQosPolicy) -> Self
    where
        Self: Sized;
}

/// UDP transport parameters.
#[derive(Debug, Clone, Copy)]
pub(crate) struct UdpConfig {
    pub multicast_ttl: u8,
}

impl TransportConfig for UdpConfig {
    fn from_property(property: &PropertyQosPolicy) -> Self {
        // Multicast TTL: property → env → default. A per-key typo logs a warning
        // and falls back rather than aborting `DomainParticipant` creation.
        let multicast_ttl = property
            .find_property(PROP_MULTICAST_TTL)
            .and_then(|v| match v.parse::<u8>() {
                Ok(ttl) => Some(ttl),
                Err(e) => {
                    log::warn!(
                        "invalid {} property value '{}': {}. Falling back to env/default.",
                        PROP_MULTICAST_TTL,
                        v,
                        e
                    );
                    None
                }
            })
            .or_else(crate::common::env::get_multicast_ttl_override)
            .unwrap_or(DEFAULT_MULTICAST_TTL);
        Self { multicast_ttl }
    }
}

/// TCP transport parameters — all resolved per participant.
#[derive(Debug, Clone)]
pub(crate) struct TcpConfig {
    pub transport_type: TransportType,
    pub bind_port: Option<u16>,
    pub public_address: Option<SocketAddr>,
    pub initial_peers: Vec<SocketAddr>,
    pub accept_undefined_peers: bool,
    pub nodelay: bool,
    pub connect_timeout: Duration,
    pub first_frame_timeout: Duration,
    pub tls_handshake_timeout: Duration,
    pub unacked_timeout: Option<Duration>,
    pub keepalive_interval: Duration,
    pub keepalive_timeout: Duration,
    pub keepalive_max_misses: u32,
    pub so_rcvbuf: Option<usize>,
    pub so_sndbuf: Option<usize>,
}

impl TransportConfig for TcpConfig {
    fn from_property(property: &PropertyQosPolicy) -> Self {
        let ms = |key, default| {
            Duration::from_millis(prop_parse::<u64>(property, key).unwrap_or(default))
        };
        Self {
            transport_type: property
                .find_property(PROP_TRANSPORT)
                .and_then(|v| v.parse().ok())
                .unwrap_or_else(crate::rtps::transport::get_transport_type),
            bind_port: prop_parse::<u16>(property, PROP_TCP_BIND_PORT),
            public_address: prop_parse::<SocketAddr>(property, PROP_TCP_PUBLIC_ADDRESS),
            initial_peers: property
                .find_property(PROP_INITIAL_PEERS)
                .map(crate::common::env::parse_initial_peers)
                .unwrap_or_else(crate::common::env::get_initial_peers),
            accept_undefined_peers: prop_parse::<bool>(property, PROP_ACCEPT_UNDEFINED_PEERS)
                .unwrap_or(false),
            nodelay: prop_parse::<bool>(property, PROP_TCP_NODELAY).unwrap_or(true),
            connect_timeout: ms(PROP_TCP_CONNECT_TIMEOUT_MS, 1_000),
            first_frame_timeout: ms(PROP_TCP_PEER_HANDSHAKE_TIMEOUT_MS, 20_000),
            tls_handshake_timeout: ms(PROP_TCP_TLS_HANDSHAKE_TIMEOUT_MS, 5_000),
            unacked_timeout: match prop_parse::<u64>(property, PROP_TCP_UNACKED_TIMEOUT_MS) {
                Some(0) => None,
                Some(v) => Some(Duration::from_millis(v)),
                None => Some(Duration::from_millis(25_000)),
            },
            keepalive_interval: ms(PROP_TCP_KEEPALIVE_INTERVAL_MS, 10_000),
            keepalive_timeout: ms(PROP_TCP_KEEPALIVE_TIMEOUT_MS, 5_000),
            keepalive_max_misses: match prop_parse::<u32>(property, PROP_TCP_KEEPALIVE_MAX_MISSES) {
                Some(0) => {
                    log::warn!("{PROP_TCP_KEEPALIVE_MAX_MISSES} = 0 is invalid (min 1); using 1");
                    1
                }
                Some(v) => v,
                None => 3,
            },
            so_rcvbuf: prop_parse::<usize>(property, PROP_TCP_SO_RCVBUF),
            so_sndbuf: prop_parse::<usize>(property, PROP_TCP_SO_SNDBUF),
        }
    }
}

impl Default for TcpConfig {
    fn default() -> Self {
        Self::from_property(&PropertyQosPolicy::default())
    }
}

/// Hybrid composes both sides: UDP multicast discovery + TCP/UDP unicast.
#[derive(Debug, Clone)]
pub(crate) struct HybridConfig {
    pub udp: UdpConfig,
    pub tcp: TcpConfig,
}

impl TransportConfig for HybridConfig {
    fn from_property(property: &PropertyQosPolicy) -> Self {
        Self { udp: UdpConfig::from_property(property), tcp: TcpConfig::from_property(property) }
    }
}

/// Parse a single text property into `T`, ignoring absent/invalid values.
fn prop_parse<T: std::str::FromStr>(property: &PropertyQosPolicy, key: &str) -> Option<T> {
    property.find_property(key).and_then(|v| v.trim().parse::<T>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clear_env() {
        unsafe { std::env::remove_var("INT2DDS_MULTICAST_TTL") };
    }

    #[test]
    fn default_when_property_missing() {
        clear_env();
        assert_eq!(
            UdpConfig::from_property(&PropertyQosPolicy::default()).multicast_ttl,
            DEFAULT_MULTICAST_TTL
        );
    }

    #[test]
    fn parses_valid_u8_values_including_boundaries() {
        clear_env();
        for raw in ["0", "1", "32", "255"] {
            let mut p = PropertyQosPolicy::default();
            p.add_property(PROP_MULTICAST_TTL, raw, false);
            assert_eq!(
                UdpConfig::from_property(&p).multicast_ttl,
                raw.parse::<u8>().unwrap(),
                "parse {raw}"
            );
        }
    }

    #[test]
    fn invalid_values_fall_back_to_default() {
        clear_env();
        for bad in ["abc", "256", "-1", ""] {
            let mut p = PropertyQosPolicy::default();
            p.add_property(PROP_MULTICAST_TTL, bad, false);
            assert_eq!(
                UdpConfig::from_property(&p).multicast_ttl,
                DEFAULT_MULTICAST_TTL,
                "{bad} should fall back"
            );
        }
    }

    // TcpConfig reads only properties for its TCP-specific fields (no env), so
    // these defaults are independent of any INT2DDS_TCP_* env var.
    #[test]
    fn tcp_config_defaults_when_property_missing() {
        let cfg = TcpConfig::from_property(&PropertyQosPolicy::default());
        assert_eq!(cfg.bind_port, None);
        assert_eq!(cfg.public_address, None);
        assert!(cfg.nodelay);
        assert_eq!(cfg.connect_timeout, Duration::from_millis(1_000));
        assert_eq!(cfg.first_frame_timeout, Duration::from_millis(20_000));
        assert_eq!(cfg.tls_handshake_timeout, Duration::from_millis(5_000));
        assert_eq!(cfg.unacked_timeout, Some(Duration::from_millis(25_000)));
        assert_eq!(cfg.keepalive_interval, Duration::from_millis(10_000));
        assert_eq!(cfg.keepalive_timeout, Duration::from_millis(5_000));
        assert_eq!(cfg.keepalive_max_misses, 3);
        assert_eq!(cfg.so_rcvbuf, None);
        assert_eq!(cfg.so_sndbuf, None);
    }

    #[test]
    fn tcp_config_reads_properties_and_ignores_invalid() {
        let mut p = PropertyQosPolicy::default();
        p.add_property(PROP_TCP_BIND_PORT, "17400", false);
        p.add_property(PROP_TCP_NODELAY, "false", false);
        p.add_property(PROP_TCP_CONNECT_TIMEOUT_MS, "1111", false);
        p.add_property(PROP_TCP_PEER_HANDSHAKE_TIMEOUT_MS, "2222", false);
        p.add_property(PROP_TCP_TLS_HANDSHAKE_TIMEOUT_MS, "3333", false);
        p.add_property(PROP_TCP_KEEPALIVE_MAX_MISSES, "7", false);
        p.add_property(PROP_TCP_PUBLIC_ADDRESS, "203.0.113.5:7400", false);
        // Invalid value must be ignored (falls back to default), not panic.
        p.add_property(PROP_TCP_SO_RCVBUF, "not-a-number", false);

        let cfg = TcpConfig::from_property(&p);
        assert_eq!(cfg.bind_port, Some(17400));
        assert!(!cfg.nodelay);
        assert_eq!(cfg.connect_timeout, Duration::from_millis(1111));
        assert_eq!(cfg.first_frame_timeout, Duration::from_millis(2222));
        assert_eq!(cfg.tls_handshake_timeout, Duration::from_millis(3333));
        assert_eq!(cfg.keepalive_max_misses, 7);
        assert_eq!(cfg.public_address, Some("203.0.113.5:7400".parse().unwrap()));
        assert_eq!(cfg.so_rcvbuf, None);
    }

    #[test]
    fn keepalive_max_misses_zero_is_clamped_to_one() {
        // TCP_KEEPCNT must be >= 1; a 0 property is clamped rather than passed
        // through to the OS where it would be rejected.
        let mut p = PropertyQosPolicy::default();
        p.add_property(PROP_TCP_KEEPALIVE_MAX_MISSES, "0", false);
        assert_eq!(TcpConfig::from_property(&p).keepalive_max_misses, 1);
    }
}
