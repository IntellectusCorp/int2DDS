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
    // - INT2DDS_FORCE_LOOPBACK_MULTICAST: Force multicast egress through the loopback interface (127.0.0.1) for local-only testing (true, false) - Default: false
    // - INT2DDS_DISABLE_SAME_HOST_LOOPBACK: Address a co-located peer at every address it announced instead of 127.0.0.1 (true, false) - Default: false
    // - INT2DDS_UDP_SOCKET_BUFFER: Set UDP socket buffer size (bytes) - Default: OS default
    // - INT2DDS_SHM_BUFFER_SIZE: Set shared memory buffer size (bytes) - Default: 1048576 (1MB)
    // - INT2DDS_DATA_FRAG_SIZE: Set DATA_FRAG fragment size (1-65000) when the writer QoS specifies none - Default: 65000
    // - INT2DDS_MAX_MESSAGE_SIZE: Set max UDP message size (1-65000), header-inclusive datagram budget bounding fragments packed per message - Default: 65000
    // - INT2DDS_DISABLE_PIGGYBACK_HEARTBEAT_DEFAULT: Set the default for the disable_piggyback_heartbeat writer QoS (true, false) - Default: false
    // - INT2DDS_DISABLE_PREEMPTIVE: Disable preemptive ACKNACK and preemptive HEARTBEAT on new endpoint matches (true, false) - Default: false
    // - INT2DDS_NACK_FRAG_RESPONSE_DELAY_MS: Reader delay before the first NACK_FRAG for missing fragments (ms) - Default: 5
    // - INT2DDS_NACK_FRAG_RETRY_MS: Reader retry interval when a NACK_FRAG got no reply (ms) - Default: 200
    // - INT2DDS_NACK_FRAG_MAX_RETRIES: Reader retries before yielding to the periodic heartbeat - Default: 10
    // - INT2DDS_NACK_RESPONSE_DELAY_MS: Writer delay before answering an ACKNACK or NACK_FRAG (ms) - Default: 0
    // - INT2DDS_SEND_CREDIT_BACKSTOP_MS: Writer age at which a send charge toward a silent peer stops counting (ms) - Default: 250
    // - INT2DDS_ENABLE_SEND_WINDOW: Bound a fragment burst by the peer's receive buffer. False sends every fragment of a change in one go (true, false) - Default: true

    // - INT2DDS_INITIAL_PEERS: Set initial peers for SPDP unicast discovery (comma-separated, e.g., "192.168.1.10:7400,192.168.1.11:7400") - Default: none
    // - INT2DDS_TCP_PEER_SEARCH_SLOTS: Set how many participant slots a TCP peer named with the wildcard port stands for (1-125, one domain's port block) - Default: 16

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

/// Get the force loopback multicast setting from environment variable
pub fn get_force_loopback_multicast() -> bool {
    std::env::var("INT2DDS_FORCE_LOOPBACK_MULTICAST")
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false)
}

/// Set the force loopback multicast via environment variable
pub fn set_force_loopback_multicast(enabled: bool) {
    log::info!("Environment variable set: INT2DDS_FORCE_LOOPBACK_MULTICAST = {}", enabled);
    unsafe { std::env::set_var("INT2DDS_FORCE_LOOPBACK_MULTICAST", enabled.to_string()) };
}

/// Read the same-host loopback gate from `INT2DDS_DISABLE_SAME_HOST_LOOPBACK`
pub fn get_disable_same_host_loopback() -> bool {
    get_bool_env("INT2DDS_DISABLE_SAME_HOST_LOOPBACK").unwrap_or(false)
}

/// Set the same-host loopback gate via environment variable
pub fn set_disable_same_host_loopback(is_disabled: bool) {
    log::info!("Environment variable set: INT2DDS_DISABLE_SAME_HOST_LOOPBACK = {}", is_disabled);
    unsafe { std::env::set_var("INT2DDS_DISABLE_SAME_HOST_LOOPBACK", is_disabled.to_string()) };
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

/// Read the TCP peer search width from `INT2DDS_TCP_PEER_SEARCH_SLOTS`.
/// Returns `None` when unset, empty, or not an integer; the range is the
/// transport's to check, since the ceiling is one domain's port block.
pub fn get_tcp_peer_search_slots() -> Option<u32> {
    let raw = std::env::var("INT2DDS_TCP_PEER_SEARCH_SLOTS").ok().filter(|s| !s.is_empty())?;
    match raw.parse::<u32>() {
        Ok(slots) => Some(slots),
        Err(e) => {
            log::warn!(
                "Invalid INT2DDS_TCP_PEER_SEARCH_SLOTS value '{}': {}. Ignoring env default.",
                raw,
                e
            );
            None
        }
    }
}

/// Read the DATA_FRAG fragment size fallback from `INT2DDS_DATA_FRAG_SIZE`.
/// Returns `None` when unset, empty, or not an integer; the range is the QoS policy's to check.
pub fn get_data_frag_size_override() -> Option<i32> {
    let raw = std::env::var("INT2DDS_DATA_FRAG_SIZE").ok().filter(|s| !s.is_empty())?;
    match raw.parse::<i32>() {
        Ok(size) => Some(size),
        Err(e) => {
            log::warn!(
                "Invalid INT2DDS_DATA_FRAG_SIZE value '{}': {}. Ignoring env override.",
                raw,
                e
            );
            None
        }
    }
}

/// Set the DATA_FRAG fragment size fallback via `INT2DDS_DATA_FRAG_SIZE`.
/// Must be called before the DataWriter is created in order to take effect.
pub fn set_data_frag_size(size: i32) {
    log::info!("Environment variable set: INT2DDS_DATA_FRAG_SIZE = {}", size);
    unsafe { std::env::set_var("INT2DDS_DATA_FRAG_SIZE", size.to_string()) };
}

// Read the max UDP message size override from `INT2DDS_MAX_MESSAGE_SIZE`.
// Header-inclusive datagram budget. `None` when unset, empty, or not an integer.
pub fn get_max_message_size_override() -> Option<i32> {
    let raw = std::env::var("INT2DDS_MAX_MESSAGE_SIZE").ok().filter(|s| !s.is_empty())?;
    match raw.parse::<i32>() {
        Ok(size) => Some(size),
        Err(e) => {
            log::warn!(
                "Invalid INT2DDS_MAX_MESSAGE_SIZE value '{}': {}. Ignoring env override.",
                raw,
                e
            );
            None
        }
    }
}

// Set the max UDP message size via `INT2DDS_MAX_MESSAGE_SIZE`.
// Must be called before the DataWriter is created in order to take effect.
pub fn set_max_message_size(size: i32) {
    log::info!("Environment variable set: INT2DDS_MAX_MESSAGE_SIZE = {}", size);
    unsafe { std::env::set_var("INT2DDS_MAX_MESSAGE_SIZE", size.to_string()) };
}

// Resolve INT2DDS_MAX_MESSAGE_SIZE to a concrete size, clamped to 1..=65000, default 65000.
pub fn get_max_message_size() -> usize {
    get_max_message_size_override().filter(|&size| (1..=65000).contains(&size)).unwrap_or(65000)
        as usize
}

// Read the disable_piggyback_heartbeat QoS default from
// `INT2DDS_DISABLE_PIGGYBACK_HEARTBEAT_DEFAULT`.
// Read a boolean env var, accepting `true`/`false`/`1`/`0` case-insensitively.
// Returns `None` when unset, empty, or not a recognized boolean.
fn get_bool_env(name: &str) -> Option<bool> {
    let raw = std::env::var(name).ok().filter(|s| !s.is_empty())?;

    if raw.eq_ignore_ascii_case("true") || raw == "1" {
        Some(true)
    } else if raw.eq_ignore_ascii_case("false") || raw == "0" {
        Some(false)
    } else {
        log::warn!("Invalid {} value '{}'. Ignoring env default.", name, raw);
        None
    }
}

// Read the disable_piggyback_heartbeat QoS default from
// `INT2DDS_DISABLE_PIGGYBACK_HEARTBEAT_DEFAULT`.
// Returns `None` when unset, empty, or not a recognized boolean.
pub fn get_disable_piggyback_heartbeat_default() -> Option<bool> {
    get_bool_env("INT2DDS_DISABLE_PIGGYBACK_HEARTBEAT_DEFAULT")
}

// Set the disable_piggyback_heartbeat QoS default via
// `INT2DDS_DISABLE_PIGGYBACK_HEARTBEAT_DEFAULT`.
// Must be called before the DataWriter QoS is constructed in order to take effect.
pub fn set_disable_piggyback_heartbeat_default(is_disabled: bool) {
    log::info!(
        "Environment variable set: INT2DDS_DISABLE_PIGGYBACK_HEARTBEAT_DEFAULT = {}",
        is_disabled
    );
    unsafe {
        std::env::set_var("INT2DDS_DISABLE_PIGGYBACK_HEARTBEAT_DEFAULT", is_disabled.to_string())
    };
}

// Read the preemptive ACKNACK/HEARTBEAT gate from `INT2DDS_DISABLE_PREEMPTIVE`.
// Read at match time, so it only affects endpoint matches made after it is set.
pub fn get_disable_preemptive() -> bool {
    get_bool_env("INT2DDS_DISABLE_PREEMPTIVE").unwrap_or(false)
}

// Disable the preemptive ACKNACK and preemptive HEARTBEAT sent on a new endpoint
// match, via `INT2DDS_DISABLE_PREEMPTIVE`. Responses to a peer's preemptive ACKNACK are unaffected.
pub fn set_disable_preemptive(is_disabled: bool) {
    log::info!("Environment variable set: INT2DDS_DISABLE_PREEMPTIVE = {}", is_disabled);
    unsafe { std::env::set_var("INT2DDS_DISABLE_PREEMPTIVE", is_disabled.to_string()) };
}

/// SEDP heartbeat period override from `INT2DDS_SEDP_HEARTBEAT_MS`, in ms.
/// A lost announcement waits one period before the reader NACKs for it.
pub fn get_sedp_heartbeat_ms() -> Option<u64> {
    static CACHED: std::sync::OnceLock<Option<u64>> = std::sync::OnceLock::new();
    *CACHED.get_or_init(|| {
        let raw = std::env::var("INT2DDS_SEDP_HEARTBEAT_MS").ok()?;
        match raw.trim().parse::<u64>() {
            Ok(ms) if ms > 0 => {
                log::info!("Environment variable set: INT2DDS_SEDP_HEARTBEAT_MS = {}", ms);
                Some(ms)
            }
            _ => None,
        }
    })
}

/// Reader NACK_FRAG response-delay override from `INT2DDS_NACK_FRAG_RESPONSE_DELAY_MS`, in ms.
/// Delay before the reader sends its first NACK_FRAG for a sample's missing fragments.
pub fn get_nack_frag_response_delay_ms_override() -> Option<u32> {
    let raw =
        std::env::var("INT2DDS_NACK_FRAG_RESPONSE_DELAY_MS").ok().filter(|s| !s.is_empty())?;
    match raw.parse::<u32>() {
        Ok(ms) => Some(ms),
        Err(e) => {
            log::warn!(
                "Invalid INT2DDS_NACK_FRAG_RESPONSE_DELAY_MS value '{}': {}. Ignoring env override.",
                raw,
                e
            );
            None
        }
    }
}

/// Set the NACK_FRAG response delay via `INT2DDS_NACK_FRAG_RESPONSE_DELAY_MS`.
pub fn set_nack_frag_response_delay_ms(ms: u32) {
    log::info!("Environment variable set: INT2DDS_NACK_FRAG_RESPONSE_DELAY_MS = {}", ms);
    unsafe { std::env::set_var("INT2DDS_NACK_FRAG_RESPONSE_DELAY_MS", ms.to_string()) };
}

/// Reader NACK_FRAG retry-interval override from `INT2DDS_NACK_FRAG_RETRY_MS`, in ms.
/// Delay before the reader re-asks when a NACK_FRAG produced no fragments.
pub fn get_nack_frag_retry_ms_override() -> Option<u32> {
    let raw = std::env::var("INT2DDS_NACK_FRAG_RETRY_MS").ok().filter(|s| !s.is_empty())?;
    match raw.parse::<u32>() {
        Ok(ms) => Some(ms),
        Err(e) => {
            log::warn!(
                "Invalid INT2DDS_NACK_FRAG_RETRY_MS value '{}': {}. Ignoring env override.",
                raw,
                e
            );
            None
        }
    }
}

/// Set the NACK_FRAG retry interval via `INT2DDS_NACK_FRAG_RETRY_MS`.
pub fn set_nack_frag_retry_ms(ms: u32) {
    log::info!("Environment variable set: INT2DDS_NACK_FRAG_RETRY_MS = {}", ms);
    unsafe { std::env::set_var("INT2DDS_NACK_FRAG_RETRY_MS", ms.to_string()) };
}

/// Reader NACK_FRAG max-retries override from `INT2DDS_NACK_FRAG_MAX_RETRIES`.
/// Retries before a stalled fragment repair yields to the periodic heartbeat.
pub fn get_nack_frag_max_retries_override() -> Option<u32> {
    let raw = std::env::var("INT2DDS_NACK_FRAG_MAX_RETRIES").ok().filter(|s| !s.is_empty())?;
    match raw.parse::<u32>() {
        Ok(retries) => Some(retries),
        Err(e) => {
            log::warn!(
                "Invalid INT2DDS_NACK_FRAG_MAX_RETRIES value '{}': {}. Ignoring env override.",
                raw,
                e
            );
            None
        }
    }
}

/// Set the NACK_FRAG max retries via `INT2DDS_NACK_FRAG_MAX_RETRIES`.
pub fn set_nack_frag_max_retries(retries: u32) {
    log::info!("Environment variable set: INT2DDS_NACK_FRAG_MAX_RETRIES = {}", retries);
    unsafe { std::env::set_var("INT2DDS_NACK_FRAG_MAX_RETRIES", retries.to_string()) };
}

/// Writer NACK response-delay override from `INT2DDS_NACK_RESPONSE_DELAY_MS`, in ms.
/// Delay before the writer answers a reader's ACKNACK or NACK_FRAG with the repair.
pub fn get_nack_response_delay_ms_override() -> Option<u32> {
    let raw = std::env::var("INT2DDS_NACK_RESPONSE_DELAY_MS").ok().filter(|s| !s.is_empty())?;
    match raw.parse::<u32>() {
        Ok(ms) => Some(ms),
        Err(e) => {
            log::warn!(
                "Invalid INT2DDS_NACK_RESPONSE_DELAY_MS value '{}': {}. Ignoring env override.",
                raw,
                e
            );
            None
        }
    }
}

/// Set the writer NACK response delay via `INT2DDS_NACK_RESPONSE_DELAY_MS`.
pub fn set_nack_response_delay_ms(ms: u32) {
    log::info!("Environment variable set: INT2DDS_NACK_RESPONSE_DELAY_MS = {}", ms);
    unsafe { std::env::set_var("INT2DDS_NACK_RESPONSE_DELAY_MS", ms.to_string()) };
}

/// Send-credit backstop override from `INT2DDS_SEND_CREDIT_BACKSTOP_MS`, in ms.
/// Age at which wire bytes charged toward a remote participant stop counting against its send
/// window, for a peer that never sends the ACKNACK or NACK_FRAG that would release them.
pub fn get_send_credit_backstop_ms_override() -> Option<u32> {
    let raw = std::env::var("INT2DDS_SEND_CREDIT_BACKSTOP_MS").ok().filter(|s| !s.is_empty())?;
    match raw.parse::<u32>() {
        Ok(ms) => Some(ms),
        Err(e) => {
            log::warn!(
                "Invalid INT2DDS_SEND_CREDIT_BACKSTOP_MS value '{}': {}. Ignoring env override.",
                raw,
                e
            );
            None
        }
    }
}

/// Set the send-credit backstop via `INT2DDS_SEND_CREDIT_BACKSTOP_MS`.
pub fn set_send_credit_backstop_ms(ms: u32) {
    log::info!("Environment variable set: INT2DDS_SEND_CREDIT_BACKSTOP_MS = {}", ms);
    unsafe { std::env::set_var("INT2DDS_SEND_CREDIT_BACKSTOP_MS", ms.to_string()) };
}

// Read the send-window gate from `INT2DDS_ENABLE_SEND_WINDOW`. False puts every fragment of a
// change on the wire in one go instead of bounding the burst by the peer's receive buffer.
pub fn get_enable_send_window() -> bool {
    get_bool_env("INT2DDS_ENABLE_SEND_WINDOW").unwrap_or(true)
}

// Set the send-window gate via `INT2DDS_ENABLE_SEND_WINDOW`.
pub fn set_enable_send_window(is_enabled: bool) {
    log::info!("Environment variable set: INT2DDS_ENABLE_SEND_WINDOW = {}", is_enabled);
    unsafe { std::env::set_var("INT2DDS_ENABLE_SEND_WINDOW", is_enabled.to_string()) };
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
    use super::{
        get_disable_preemptive, get_multicast_ttl_override, get_nack_frag_max_retries_override,
        get_nack_frag_response_delay_ms_override, get_nack_frag_retry_ms_override,
        get_enable_send_window, get_nack_response_delay_ms_override,
        get_send_credit_backstop_ms_override, set_disable_preemptive, set_enable_send_window,
        set_multicast_ttl, set_nack_frag_max_retries, set_nack_frag_response_delay_ms,
        set_nack_frag_retry_ms, set_nack_response_delay_ms, set_send_credit_backstop_ms,
    };

    const ENV_KEY: &str = "INT2DDS_MULTICAST_TTL";
    const PREEMPTIVE_KEY: &str = "INT2DDS_DISABLE_PREEMPTIVE";

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

    /// Unset and unparseable both have to read as "enabled", or a typo would silently
    /// turn preemptive traffic off for the whole participant.
    #[test]
    fn preemptive_gate_defaults_to_enabled() {
        unsafe { std::env::remove_var(PREEMPTIVE_KEY) };
        assert!(!get_disable_preemptive(), "unset → preemptive stays enabled");

        for value in ["true", "TRUE", "1"] {
            unsafe { std::env::set_var(PREEMPTIVE_KEY, value) };
            assert!(get_disable_preemptive(), "{} → disabled", value);
        }

        for value in ["false", "0", "yes", ""] {
            unsafe { std::env::set_var(PREEMPTIVE_KEY, value) };
            assert!(!get_disable_preemptive(), "{:?} → enabled", value);
        }

        set_disable_preemptive(true);
        assert!(get_disable_preemptive(), "setter round-trip");

        unsafe { std::env::remove_var(PREEMPTIVE_KEY) };
    }

    #[test]
    fn nack_frag_response_delay_env_round_trips_and_rejects_invalid() {
        const KEY: &str = "INT2DDS_NACK_FRAG_RESPONSE_DELAY_MS";
        unsafe { std::env::remove_var(KEY) };
        assert_eq!(get_nack_frag_response_delay_ms_override(), None, "unset → None");

        set_nack_frag_response_delay_ms(500);
        assert_eq!(get_nack_frag_response_delay_ms_override(), Some(500));

        unsafe { std::env::set_var(KEY, "abc") };
        assert_eq!(get_nack_frag_response_delay_ms_override(), None, "non-numeric → None");

        unsafe { std::env::set_var(KEY, "-1") };
        assert_eq!(get_nack_frag_response_delay_ms_override(), None, "negative → None");

        unsafe { std::env::remove_var(KEY) };
    }

    #[test]
    fn nack_frag_retry_ms_env_round_trips_and_rejects_invalid() {
        const KEY: &str = "INT2DDS_NACK_FRAG_RETRY_MS";
        unsafe { std::env::remove_var(KEY) };
        assert_eq!(get_nack_frag_retry_ms_override(), None, "unset → None");

        set_nack_frag_retry_ms(750);
        assert_eq!(get_nack_frag_retry_ms_override(), Some(750));

        unsafe { std::env::set_var(KEY, "abc") };
        assert_eq!(get_nack_frag_retry_ms_override(), None, "non-numeric → None");

        unsafe { std::env::set_var(KEY, "-1") };
        assert_eq!(get_nack_frag_retry_ms_override(), None, "negative → None");

        unsafe { std::env::remove_var(KEY) };
    }

    #[test]
    fn nack_response_delay_env_round_trips_and_rejects_invalid() {
        const KEY: &str = "INT2DDS_NACK_RESPONSE_DELAY_MS";
        unsafe { std::env::remove_var(KEY) };
        assert_eq!(get_nack_response_delay_ms_override(), None, "unset → None");

        set_nack_response_delay_ms(100);
        assert_eq!(get_nack_response_delay_ms_override(), Some(100));

        unsafe { std::env::set_var(KEY, "abc") };
        assert_eq!(get_nack_response_delay_ms_override(), None, "non-numeric → None");

        unsafe { std::env::set_var(KEY, "-1") };
        assert_eq!(get_nack_response_delay_ms_override(), None, "negative → None");

        unsafe { std::env::remove_var(KEY) };
    }

    #[test]
    fn nack_frag_max_retries_env_round_trips_and_rejects_invalid() {
        const KEY: &str = "INT2DDS_NACK_FRAG_MAX_RETRIES";
        unsafe { std::env::remove_var(KEY) };
        assert_eq!(get_nack_frag_max_retries_override(), None, "unset → None");

        set_nack_frag_max_retries(25);
        assert_eq!(get_nack_frag_max_retries_override(), Some(25));

        unsafe { std::env::set_var(KEY, "abc") };
        assert_eq!(get_nack_frag_max_retries_override(), None, "non-numeric → None");

        unsafe { std::env::set_var(KEY, "-1") };
        assert_eq!(get_nack_frag_max_retries_override(), None, "negative → None");

        unsafe { std::env::remove_var(KEY) };
    }

    #[test]
    fn send_window_gate_env_round_trips_and_rejects_invalid() {
        const KEY: &str = "INT2DDS_ENABLE_SEND_WINDOW";
        unsafe { std::env::remove_var(KEY) };
        assert!(get_enable_send_window(), "unset keeps sends bounded, as they have always been");

        set_enable_send_window(false);
        assert!(!get_enable_send_window());

        set_enable_send_window(true);
        assert!(get_enable_send_window());

        unsafe { std::env::set_var(KEY, "neither") };
        assert!(get_enable_send_window(), "unparsable falls back to bounded");

        unsafe { std::env::remove_var(KEY) };
    }

    #[test]
    fn send_credit_backstop_env_round_trips_and_rejects_invalid() {
        const KEY: &str = "INT2DDS_SEND_CREDIT_BACKSTOP_MS";
        unsafe { std::env::remove_var(KEY) };
        assert_eq!(get_send_credit_backstop_ms_override(), None, "unset → None");

        set_send_credit_backstop_ms(750);
        assert_eq!(get_send_credit_backstop_ms_override(), Some(750));

        unsafe { std::env::set_var(KEY, "abc") };
        assert_eq!(get_send_credit_backstop_ms_override(), None, "non-numeric → None");

        unsafe { std::env::set_var(KEY, "-1") };
        assert_eq!(get_send_credit_backstop_ms_override(), None, "negative → None");

        unsafe { std::env::remove_var(KEY) };
    }
}
