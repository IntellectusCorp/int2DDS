//! Environment variable configuration.
//!
//! This module provides initialization from environment variables for configuring
//! int2dds behavior, including transport type, logging, network interface
//! selection, and performance monitoring features.

use crate::{
    common::log::{setting_log, LogLevel, LogType},
    core::types::DomainId,
    rtps::transport::TransportType,
};

/// Default domain ID sentinel value.
/// When this value is passed to `create_participant`, the domain ID is read from
/// the `DDS_DOMAIN_ID` environment variable. If the env var is not set, defaults to 0.
pub const DEFAULT_DOMAIN_ID: DomainId = -1;

pub fn init_from_env() {
    // DDS environment variables:
    // - DDS_DOMAIN_ID: Set DDS domain ID (0-232) - Required when DEFAULT_DOMAIN_ID(-1) is used
    // - DDS_QOS_PROFILE: Set QoS profile JSON file path
    // - DDS_DEFAULT_QOS_PROFILE: Set default QoS profile path "Library::Profile"

    // INT2DDS_ environment variables:
    // - INT2DDS_TRANSPORT: Set transport protocol type (udp, tcp) - Default: udp
    // - INT2DDS_DISCOVERY_MODE: Set discovery mode (udp, tcp, hybrid) - Default: udp

    // - INT2DDS_LOG_TYPE: Set log output type (console, file, all, none) - Default: none
    // - INT2DDS_CONSOLE_LOG_LEVEL: Set console log level (trace, debug, info, warn, error) - Default: info
    // - INT2DDS_FILE_LOG_LEVEL: Set file log level (trace, debug, info, warn, error) - Default: info

    // - INT2DDS_THREAD_MONITORING: Enable thread monitoring (true, false) - Default: false
    // - INT2DDS_THREAD_MONITORING_LOG_PATH: Set thread monitoring log file path - Default: ./thread_monitoring.log
    // - INT2DDS_FUNCTION_TIMING: Enable function execution time measurement (true, false) - Default: false
    // - INT2DDS_FUNCTION_TIMING_LOG_PATH: Set function timing log file path - Default: ./function_timing.log
    // - INT2DDS_EXTENDED_DISCOVERY: Enable extended discovery (true, false) - Default: false

    // - INT2DDS_NETWORK_INTERFACE: Set network interface name to use (e.g., eth0, wlan0) - Default: automatic selection
    // - INT2DDS_NETWORK_IP: Set network IP address directly (e.g., 192.168.1.100) - Default: automatic selection
    // - INT2DDS_USE_LOOPBACK_INTERFACE: Enable loopback interface for discovery and endpoint communication (true, false) - Default: false
    // - INT2DDS_UDP_SOCKET_BUFFER: Set UDP socket buffer size (bytes) - Default: OS default
    // - INT2DDS_SHM_BUFFER_SIZE: Set shared memory buffer size (bytes) - Default: 1048576 (1MB)

    // - INT2DDS_INITIAL_PEERS: Set initial peers for SPDP unicast discovery (comma-separated, e.g., "192.168.1.10:7400,192.168.1.11:7400") - Default: none

    // - INT2DDS_MULTICAST_TTL: Set IPv4 multicast TTL fallback (0-255) when no PropertyQosPolicy entry is present - Default: OS default (1)

    // - INT2DDS_EXTERNAL_ADDRESS: Public IPv4 advertised in SPDP for NAT/WAN traversal. Sockets still bind to local NICs.
    // - INT2DDS_META_PORT: Pinned metatraffic unicast port; ignores domain_id when set, applies +2*pid offset for multi-participant.
    // - INT2DDS_USER_PORT: Pinned user-traffic unicast port; ignores domain_id when set, applies +2*pid offset for multi-participant.

    // - TCP transport is configured per-participant via the
    //   int2dds.transport.TCPv4.* QoS properties (see dcps::infrastructure::qos_policy),
    //   not env vars. INT2DDS_INITIAL_PEERS remain shared fallbacks.
    setting_log();
}

/// Set the DDS domain ID via environment variable
pub fn set_domain_id(domain_id: i32) {
    log::info!("Environment variable set: DDS_DOMAIN_ID = {}", domain_id);
    unsafe { std::env::set_var("DDS_DOMAIN_ID", domain_id.to_string()) };
}

/// Set the QoS profile file path via environment variable
pub fn set_qos_profile(path: &str) {
    log::info!("Environment variable set: DDS_QOS_PROFILE = {}", path);
    unsafe { std::env::set_var("DDS_QOS_PROFILE", path) };
}

/// Get the default QoS profile path (`"Library::Profile"`) from the
/// `DDS_DEFAULT_QOS_PROFILE` environment variable, if set.
pub fn get_default_qos_profile() -> Option<String> {
    std::env::var("DDS_DEFAULT_QOS_PROFILE").ok().filter(|s| !s.is_empty())
}

/// Set the default QoS profile path via environment variable.
pub fn set_default_qos_profile(path: &str) {
    log::info!("Environment variable set: DDS_DEFAULT_QOS_PROFILE = {}", path);
    unsafe { std::env::set_var("DDS_DEFAULT_QOS_PROFILE", path) };
}

/// Get QoS profile JSON file paths to auto-load.
/// Returns an empty vector if no profile files are configured/found.
pub fn get_qos_profile_paths() -> Vec<std::path::PathBuf> {
    use std::path::PathBuf;

    let mut paths: Vec<PathBuf> = Vec::new();

    if let Ok(val) = std::env::var("DDS_QOS_PROFILE") {
        // Use OS path separator (`;` on Windows, `:` on Unix) plus `,` as universal separator.
        // Note: `:` is intentionally NOT used on Windows to avoid breaking drive letters (e.g. `C:\...`).
        #[cfg(windows)]
        let is_sep = |c: char| c == ',' || c == ';';
        #[cfg(not(windows))]
        let is_sep = |c: char| c == ',' || c == ':';

        for entry in val.split(is_sep) {
            let trimmed = entry.trim();
            if !trimmed.is_empty() {
                paths.push(PathBuf::from(trimmed));
            }
        }
    }

    if paths.is_empty() {
        let default = PathBuf::from("USER_QOS_PROFILES.json");
        if default.exists() {
            paths.push(default);
        }
    }

    paths
}

/// Set the transport type via environment variable
pub fn set_transport_type(transport_type: TransportType) {
    log::info!("Environment variable set: INT2DDS_TRANSPORT = {}", transport_type);
    unsafe { std::env::set_var("INT2DDS_TRANSPORT", transport_type.to_string()) };
}

/// Set the log type via environment variable
pub fn set_log_type(log_type: LogType) {
    log::info!("Environment variable set: INT2DDS_LOG_TYPE = {}", log_type);
    unsafe { std::env::set_var("INT2DDS_LOG_TYPE", log_type.to_string()) };
}

/// Set the console log level via environment variable
pub fn set_console_log_level(log_level: LogLevel) {
    log::info!("Environment variable set: INT2DDS_CONSOLE_LOG_LEVEL = {}", log_level);
    unsafe { std::env::set_var("INT2DDS_CONSOLE_LOG_LEVEL", log_level.to_string()) };
}

/// Set the file log level via environment variable
pub fn set_file_log_level(log_level: LogLevel) {
    log::info!("Environment variable set: INT2DDS_FILE_LOG_LEVEL = {}", log_level);
    unsafe { std::env::set_var("INT2DDS_FILE_LOG_LEVEL", log_level.to_string()) };
}

/// Set the thread monitoring via environment variable
pub fn set_thread_monitoring(enabled: bool) {
    log::info!("Environment variable set: INT2DDS_THREAD_MONITORING = {}", enabled);
    unsafe { std::env::set_var("INT2DDS_THREAD_MONITORING", enabled.to_string()) };
}

/// Set the thread monitoring log path via environment variable
pub fn set_thread_monitoring_log_path(path: &str) {
    log::info!("Environment variable set: INT2DDS_THREAD_MONITORING_LOG_PATH = {}", path);
    unsafe { std::env::set_var("INT2DDS_THREAD_MONITORING_LOG_PATH", path) };
}

/// Set the function timing via environment variable
pub fn set_function_timing(enabled: bool) {
    log::info!("Environment variable set: INT2DDS_FUNCTION_TIMING = {}", enabled);
    unsafe { std::env::set_var("INT2DDS_FUNCTION_TIMING", enabled.to_string()) };
}

/// Set the function timing log path via environment variable
pub fn set_function_timing_log_path(path: &str) {
    log::info!("Environment variable set: INT2DDS_FUNCTION_TIMING_LOG_PATH = {}", path);
    unsafe { std::env::set_var("INT2DDS_FUNCTION_TIMING_LOG_PATH", path) };
}

/// Set the extended discovery via environment variable
pub fn set_extended_discovery(enabled: bool) {
    log::info!("Environment variable set: INT2DDS_EXTENDED_DISCOVERY = {}", enabled);
    unsafe { std::env::set_var("INT2DDS_EXTENDED_DISCOVERY", enabled.to_string()) };
}

/// Get the network interface from environment variable
pub fn get_network_interface() -> Option<String> {
    std::env::var("INT2DDS_NETWORK_INTERFACE").ok()
}

/// Set the network interface via environment variable
pub fn set_network_interface(interface: &str) {
    log::info!("Environment variable set: INT2DDS_NETWORK_INTERFACE = {}", interface);
    unsafe { std::env::set_var("INT2DDS_NETWORK_INTERFACE", interface) };
}

/// Get the network IP from environment variable
pub fn get_network_ip() -> Option<String> {
    std::env::var("INT2DDS_NETWORK_IP").ok()
}

/// Set the network IP via environment variable
pub fn set_network_ip(ip: &str) {
    log::info!("Environment variable set: INT2DDS_NETWORK_IP = {}", ip);
    unsafe { std::env::set_var("INT2DDS_NETWORK_IP", ip) };
}

/// Get the use loopback interface setting from environment variable
pub fn get_use_loopback_interface() -> bool {
    std::env::var("INT2DDS_USE_LOOPBACK_INTERFACE")
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false)
}

/// Set the use loopback interface via environment variable
pub fn set_use_loopback_interface(enabled: bool) {
    log::info!("Environment variable set: INT2DDS_USE_LOOPBACK_INTERFACE = {}", enabled);
    unsafe { std::env::set_var("INT2DDS_USE_LOOPBACK_INTERFACE", enabled.to_string()) };
}

/// Set the UDP socket buffer size via environment variable
pub fn set_udp_socket_buffer_size(size: usize) {
    log::info!("Environment variable set: INT2DDS_UDP_SOCKET_BUFFER = {}", size);
    unsafe { std::env::set_var("INT2DDS_UDP_SOCKET_BUFFER", size.to_string()) };
}

/// Set the shared memory buffer size via environment variable
pub fn set_shm_buffer_size(size: usize) {
    log::info!("Environment variable set: INT2DDS_SHM_BUFFER_SIZE = {}", size);
    unsafe { std::env::set_var("INT2DDS_SHM_BUFFER_SIZE", size.to_string()) };
}

/// Discovery mode for DDS participant discovery protocol
///
/// Determines how participants discover each other in the network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoveryMode {
    /// UDP-only discovery (legacy mode)
    /// - Discovery: UDP multicast
    /// - User Data: UDP
    Udp,

    /// TCP-only discovery (requires initial peers)
    /// - Discovery: TCP unicast (requires INT2DDS_INITIAL_PEERS)
    /// - User Data: TCP
    Tcp,

    /// Hybrid mode (default, recommended)
    /// - Discovery: UDP multicast (automatic discovery)
    /// - User Data: TCP (reliability)
    Hybrid,
}

impl DiscoveryMode {
    /// Convert string to DiscoveryMode
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "udp" => Some(DiscoveryMode::Udp),
            "tcp" => Some(DiscoveryMode::Tcp),
            "hybrid" => Some(DiscoveryMode::Hybrid),
            _ => None,
        }
    }

    /// Convert DiscoveryMode to string
    pub fn to_string(&self) -> &str {
        match self {
            DiscoveryMode::Udp => "udp",
            DiscoveryMode::Tcp => "tcp",
            DiscoveryMode::Hybrid => "hybrid",
        }
    }
}

/// Get the discovery mode from environment variable
///
/// Reads INT2DDS_DISCOVERY_MODE environment variable.
/// Valid values: "udp", "tcp", "hybrid" (case-insensitive)
/// Default: udp
///
/// # Examples
///
/// ```no_run
/// use int2dds::common::env::{get_discovery_mode, DiscoveryMode};
///
/// let mode = get_discovery_mode();
/// match mode {
///     DiscoveryMode::Udp => println!("UDP-only discovery"),
///     DiscoveryMode::Tcp => println!("TCP-only discovery"),
///     DiscoveryMode::Hybrid => println!("Hybrid discovery (UDP + TCP)"),
/// }
/// ```
pub fn get_discovery_mode() -> DiscoveryMode {
    std::env::var("INT2DDS_DISCOVERY_MODE")
        .ok()
        .and_then(|val| DiscoveryMode::from_str(&val))
        .unwrap_or(DiscoveryMode::Udp)
}

/// Set the discovery mode via environment variable
pub fn set_discovery_mode(mode: DiscoveryMode) {
    log::info!("Environment variable set: INT2DDS_DISCOVERY_MODE = {}", mode.to_string());
    unsafe { std::env::set_var("INT2DDS_DISCOVERY_MODE", mode.to_string()) };
}

/// Get initial peers from environment variable for SPDP unicast discovery.
/// When set, SPDP messages are sent via unicast to these peers instead of multicast.
///
/// Reads INT2DDS_INITIAL_PEERS environment variable.
/// Format: "ip:port,ip:port,..." or "ip,ip,..." (port-less entries default to TCP physical port 7400)
///
/// # Examples
///
/// ```no_run
/// use int2dds::common::env::get_initial_peers;
///
/// // With explicit ports:
/// // INT2DDS_INITIAL_PEERS="192.168.1.10:7400,192.168.1.11:7400"
///
/// // Without ports (defaults to 7400):
/// // INT2DDS_INITIAL_PEERS="192.168.1.10,192.168.1.11"
///
/// let peers = get_initial_peers();
/// for peer in peers {
///     println!("Initial peer: {}", peer);
/// }
/// ```

/// Parse a comma-separated initial peers string into `SocketAddr`s.
pub fn parse_initial_peers(peers_str: &str) -> Vec<std::net::SocketAddr> {
    peers_str
        .split(',')
        .filter_map(|s| {
            let trimmed = s.trim();
            match trimmed.parse::<std::net::SocketAddr>() {
                Ok(addr) => Some(addr),
                Err(e) => {
                    log::warn!("Failed to parse initial peer '{}': {}", trimmed, e);
                    None
                }
            }
        })
        .collect()
}

pub fn get_initial_peers() -> Vec<std::net::SocketAddr> {
    std::env::var("INT2DDS_INITIAL_PEERS").ok().map(|s| parse_initial_peers(&s)).unwrap_or_default()
}

/// Set initial peers via environment variable
///
/// # Arguments
///
/// * `peers` - Vector of socket addresses to use as initial peers
pub fn set_initial_peers(peers: &[std::net::SocketAddr]) {
    let peers_str = peers.iter().map(|addr| addr.to_string()).collect::<Vec<_>>().join(",");

    log::info!("Environment variable set: INT2DDS_INITIAL_PEERS = {}", peers_str);
    unsafe { std::env::set_var("INT2DDS_INITIAL_PEERS", peers_str) };
}

/// Get the fragment size (data_max_size_serialized) for user-defined writers.
/// Default 65000; capped at 65000 (u16 wire limit + 64KB datagram - headers).
pub fn get_fragment_size() -> i32 {
    const DEFAULT: i32 = 65000;
    const MAX: i32 = 65000;
    match std::env::var("INT2DDS_FRAGMENT_SIZE").ok().and_then(|v| v.parse::<i32>().ok()) {
        Some(v) if v > MAX => {
            log::warn!("INT2DDS_FRAGMENT_SIZE={} exceeds max {}, clamping to {}", v, MAX, MAX);
            MAX
        }
        Some(v) if v > 0 => v,
        _ => DEFAULT,
    }
}

/// Set the writer fragment size via environment variable
pub fn set_fragment_size(size: i32) {
    log::info!("Environment variable set: INT2DDS_FRAGMENT_SIZE = {}", size);
    unsafe { std::env::set_var("INT2DDS_FRAGMENT_SIZE", size.to_string()) };
}

/// Read the IPv4 multicast TTL override from `INT2DDS_MULTICAST_TTL`.
///
/// Returns `None` when the variable is unset, empty, or fails to parse as `u8`
/// (0-255). Used as a fallback by `TransportConfig::from_property` when the
/// `PropertyQosPolicy` does not carry an explicit `int2dds.transport.UDPv4.multicast_ttl`
/// entry, so explicit code- or profile-driven settings always win.
pub fn get_multicast_ttl_override() -> Option<u8> {
    let raw = std::env::var("INT2DDS_MULTICAST_TTL").ok().filter(|s| !s.is_empty())?;
    match raw.parse::<u8>() {
        Ok(ttl) => Some(ttl),
        Err(e) => {
            log::warn!(
                "Invalid INT2DDS_MULTICAST_TTL value '{}': {}. Ignoring env override.",
                raw,
                e
            );
            None
        }
    }
}

/// Set the IPv4 multicast TTL fallback via the `INT2DDS_MULTICAST_TTL` environment
/// variable. Must be called before the first `DomainParticipant` is created in
/// order to take effect.
pub fn set_multicast_ttl(ttl: u8) {
    log::info!("Environment variable set: INT2DDS_MULTICAST_TTL = {}", ttl);
    unsafe { std::env::set_var("INT2DDS_MULTICAST_TTL", ttl.to_string()) };
}

// Read the public IPv4 advertised in SPDP from `INT2DDS_EXTERNAL_ADDRESS`.
pub fn get_external_address() -> Option<std::net::Ipv4Addr> {
    let raw = std::env::var("INT2DDS_EXTERNAL_ADDRESS").ok().filter(|s| !s.is_empty())?;
    match raw.parse::<std::net::Ipv4Addr>() {
        Ok(ip) => Some(ip),
        Err(e) => {
            log::error!(
                "Invalid INT2DDS_EXTERNAL_ADDRESS value '{}': {}. Falling back to default.",
                raw,
                e
            );
            None
        }
    }
}

// Read the pinned metatraffic unicast port from `INT2DDS_META_PORT`.
pub fn get_meta_port_override() -> Option<u16> {
    parse_port_env("INT2DDS_META_PORT")
}

// Read the pinned user-traffic unicast port from `INT2DDS_USER_PORT`.
pub fn get_user_port_override() -> Option<u16> {
    parse_port_env("INT2DDS_USER_PORT")
}

fn parse_port_env(name: &str) -> Option<u16> {
    let raw = std::env::var(name).ok().filter(|s| !s.is_empty())?;
    match raw.parse::<u16>() {
        Ok(port) => Some(port),
        Err(e) => {
            log::error!("Invalid {} value '{}': {}. Falling back to default.", name, raw, e);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{get_multicast_ttl_override, set_multicast_ttl};

    const ENV_KEY: &str = "INT2DDS_MULTICAST_TTL";

    fn clear_env() {
        unsafe { std::env::remove_var(ENV_KEY) };
    }

    #[test]
    fn env_override_round_trips_through_helpers() {
        clear_env();
        assert_eq!(get_multicast_ttl_override(), None, "unset → None");

        set_multicast_ttl(64);
        assert_eq!(get_multicast_ttl_override(), Some(64));

        unsafe { std::env::set_var(ENV_KEY, "abc") };
        assert_eq!(get_multicast_ttl_override(), None, "non-numeric → None");

        unsafe { std::env::set_var(ENV_KEY, "256") };
        assert_eq!(get_multicast_ttl_override(), None, "out-of-u8 range → None");

        clear_env();
    }
}
