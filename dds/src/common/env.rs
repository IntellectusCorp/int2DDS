//! Environment variable and command-line argument configuration.
//!
//! This module provides initialization from environment variables and command-line arguments
//! for configuring int2dds behavior, including transport type, logging, network interface
//! selection, and performance monitoring features.

use clap::{Arg, ArgAction, Command, ValueHint};

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

    // - INT2DDS_TCP_CONNECT_TIMEOUT: Set TCP connection timeout (milliseconds) - Default: 5000
    // - INT2DDS_TCP_WRITE_TIMEOUT: Set TCP write timeout (milliseconds) - Default: 10000
    // - INT2DDS_TCP_NODELAY: Enable TCP Nodelay (disable Nagle algorithm) (true, false) - Default: true

    // - INT2DDS_INITIAL_PEERS: Set initial peers for SPDP unicast discovery (comma-separated, e.g., "192.168.1.10:7410,192.168.1.11:7410") - Default: none

    // - INT2DDS_MULTICAST_TTL: Set IPv4 multicast TTL fallback (0-255) when no PropertyQosPolicy entry is present - Default: OS default (1)

    // - INT2DDS_EXTERNAL_ADDRESS: Public IPv4 advertised in SPDP for NAT/WAN traversal. Sockets still bind to local NICs.
    // - INT2DDS_META_PORT: Pinned metatraffic unicast port; ignores domain_id when set, applies +2*pid offset for multi-participant.
    // - INT2DDS_USER_PORT: Pinned user-traffic unicast port; ignores domain_id when set, applies +2*pid offset for multi-participant.

    apply_cli_args_to_env();

    setting_log();
}

fn apply_cli_args_to_env() {
    fn build_command() -> Command {
        Command::new("int2dds")
            // .disable_help_subcommand(true)
            // .arg_required_else_help(false)
            .arg(
                Arg::new("dds_domain_id")
                    .long("int2dds-domain-id")
                    .value_name("ID")
                    .help("Set DDS domain ID")
                    .num_args(1)
                    .value_hint(ValueHint::Other),
            )
            .arg(
                Arg::new("dds_qos_profile")
                    .long("int2dds-qos-profile")
                    .value_name("PATH")
                    .help("Set QoS profile JSON file path")
                    .num_args(1)
                    .value_hint(ValueHint::FilePath),
            )
            .arg(
                Arg::new("dds_default_qos_profile")
                    .long("int2dds-default-qos-profile")
                    .value_name("LIB::PROFILE")
                    .help("Set default QoS profile path \"Library::Profile\"")
                    .num_args(1)
                    .value_hint(ValueHint::Other),
            )
            .arg(
                Arg::new("int2dds_transport")
                    .long("int2dds-transport")
                    .value_name("udp|tcp")
                    .help("Set transport protocol type")
                    .num_args(1)
                    .value_hint(ValueHint::Other),
            )
            .arg(
                Arg::new("int2dds_discovery_mode")
                    .long("int2dds-discovery-mode")
                    .value_name("udp|tcp|hybrid")
                    .help("Set discovery mode")
                    .num_args(1)
                    .value_hint(ValueHint::Other),
            )
            .arg(
                Arg::new("int2dds_log_type")
                    .long("int2dds-log-type")
                    .value_name("console|file|all|none")
                    .help("Set log output type")
                    .num_args(1),
            )
            .arg(
                Arg::new("int2dds_console_log_level")
                    .long("int2dds-console-log-level")
                    .value_name("trace|debug|info|warn|error")
                    .help("Set console log level")
                    .num_args(1),
            )
            .arg(
                Arg::new("int2dds_file_log_level")
                    .long("int2dds-file-log-level")
                    .value_name("trace|debug|info|warn|error")
                    .help("Set file log level")
                    .num_args(1),
            )
            .arg(
                Arg::new("int2dds_thread_monitoring")
                    .long("int2dds-thread-monitoring")
                    .help("Enable thread monitoring")
                    .action(ArgAction::SetTrue),
            )
            .arg(
                Arg::new("int2dds_thread_monitoring_log_path")
                    .long("int2dds-thread-monitoring-log-path")
                    .value_name("PATH")
                    .help("Thread monitoring log file path")
                    .num_args(1)
                    .value_hint(ValueHint::FilePath),
            )
            .arg(
                Arg::new("int2dds_function_timing")
                    .long("int2dds-function-timing")
                    .help("Enable function execution time measurement")
                    .action(ArgAction::SetTrue),
            )
            .arg(
                Arg::new("int2dds_function_timing_log_path")
                    .long("int2dds-function-timing-log-path")
                    .value_name("PATH")
                    .help("Function timing log file path")
                    .num_args(1)
                    .value_hint(ValueHint::FilePath),
            )
            .arg(
                Arg::new("int2dds_extended_discovery")
                    .long("int2dds-extended-discovery")
                    .help("Enable extended discovery")
                    .action(ArgAction::SetTrue),
            )
            .arg(
                Arg::new("int2dds_network_interface")
                    .long("int2dds-network-interface")
                    .value_name("INTERFACE")
                    .help("Network interface name to use (e.g., eth0, wlan0)")
                    .num_args(1)
                    .value_hint(ValueHint::Other),
            )
            .arg(
                Arg::new("int2dds_network_ip")
                    .long("int2dds-network-ip")
                    .value_name("IP")
                    .help("Network IP address to use (e.g., 192.168.1.100)")
                    .num_args(1)
                    .value_hint(ValueHint::Other),
            )
            .arg(
                Arg::new("int2dds_use_loopback_interface")
                    .long("int2dds-use-loopback-interface")
                    .help("Enable loopback interface for discovery and endpoint communication")
                    .action(ArgAction::SetTrue),
            )
            .arg(
                Arg::new("int2dds_udp_socket_buffer")
                    .long("int2dds-udp-socket-buffer")
                    .value_name("SIZE")
                    .help("UDP socket buffer size (bytes)")
                    .num_args(1)
                    .value_hint(ValueHint::Other),
            )
            .arg(
                Arg::new("int2dds_shm_buffer_size")
                    .long("int2dds-shm-buffer-size")
                    .value_name("SIZE")
                    .help("Shared memory buffer size (bytes)")
                    .num_args(1)
                    .value_hint(ValueHint::Other),
            )
            .arg(
                Arg::new("int2dds_tcp_connect_timeout")
                    .long("int2dds-tcp-connect-timeout")
                    .value_name("MILLISECONDS")
                    .help("TCP connection timeout (milliseconds)")
                    .num_args(1)
                    .value_hint(ValueHint::Other),
            )
            .arg(
                Arg::new("int2dds_tcp_write_timeout")
                    .long("int2dds-tcp-write-timeout")
                    .value_name("MILLISECONDS")
                    .help("TCP write timeout (milliseconds)")
                    .num_args(1)
                    .value_hint(ValueHint::Other),
            )
            .arg(
                Arg::new("int2dds_tcp_nodelay")
                    .long("int2dds-tcp-nodelay")
                    .help("Enable TCP Nodelay (disable Nagle algorithm)")
                    .action(ArgAction::SetTrue),
            )
            .arg(
                Arg::new("int2dds_initial_peers")
                    .long("int2dds-initial-peers")
                    .value_name("PEERS")
                    .help("Initial peers for SPDP unicast discovery (comma-separated, e.g., \"192.168.1.10:7410,192.168.1.11:7410\")")
                    .num_args(1)
                    .value_hint(ValueHint::Other),
            )
            .arg(
                Arg::new("int2dds_external_address")
                    .long("int2dds-external-address")
                    .value_name("IPV4")
                    .help("Public IPv4 advertised in SPDP for NAT/WAN traversal (bind unaffected)")
                    .num_args(1)
                    .value_hint(ValueHint::Other),
            )
            .arg(
                Arg::new("int2dds_meta_port")
                    .long("int2dds-meta-port")
                    .value_name("PORT")
                    .help("Pinned metatraffic unicast port; ignores domain_id when set")
                    .num_args(1)
                    .value_hint(ValueHint::Other),
            )
            .arg(
                Arg::new("int2dds_user_port")
                    .long("int2dds-user-port")
                    .value_name("PORT")
                    .help("Pinned user-traffic unicast port; ignores domain_id when set")
                    .num_args(1)
                    .value_hint(ValueHint::Other),
            )
            .arg(
                Arg::new("int2dds_multicast_ttl")
                    .long("int2dds-multicast-ttl")
                    .value_name("TTL")
                    .help("IPv4 multicast TTL fallback (0-255) used when PropertyQosPolicy has no multicast_ttl entry")
                    .num_args(1)
                    .value_hint(ValueHint::Other),
            )
    }

    let matches = build_command()
        .try_get_matches_from(std::env::args())
        .unwrap_or_else(|_| build_command().get_matches_from(Vec::<String>::new()));

    if let Some(v) = matches.get_one::<String>("dds_domain_id") {
        log::info!("Environment variable set: DDS_DOMAIN_ID = {}", v);
        unsafe { std::env::set_var("DDS_DOMAIN_ID", v) };
    }
    if let Some(v) = matches.get_one::<String>("dds_qos_profile") {
        log::info!("Environment variable set: DDS_QOS_PROFILE = {}", v);
        unsafe { std::env::set_var("DDS_QOS_PROFILE", v) };
    }
    if let Some(v) = matches.get_one::<String>("dds_default_qos_profile") {
        log::info!("Environment variable set: DDS_DEFAULT_QOS_PROFILE = {}", v);
        unsafe { std::env::set_var("DDS_DEFAULT_QOS_PROFILE", v) };
    }
    if let Some(v) = matches.get_one::<String>("int2dds_transport") {
        log::info!("Environment variable set: INT2DDS_TRANSPORT = {}", v);
        unsafe { std::env::set_var("INT2DDS_TRANSPORT", v) };
    }
    if let Some(v) = matches.get_one::<String>("int2dds_discovery_mode") {
        log::info!("Environment variable set: INT2DDS_DISCOVERY_MODE = {}", v);
        unsafe { std::env::set_var("INT2DDS_DISCOVERY_MODE", v) };
    }
    if let Some(v) = matches.get_one::<String>("int2dds_log_type") {
        log::info!("Environment variable set: INT2DDS_LOG_TYPE = {}", v);
        unsafe { std::env::set_var("INT2DDS_LOG_TYPE", v) };
    }
    if let Some(v) = matches.get_one::<String>("int2dds_console_log_level") {
        log::info!("Environment variable set: INT2DDS_CONSOLE_LOG_LEVEL = {}", v);
        unsafe { std::env::set_var("INT2DDS_CONSOLE_LOG_LEVEL", v) };
    }
    if let Some(v) = matches.get_one::<String>("int2dds_file_log_level") {
        log::info!("Environment variable set: INT2DDS_FILE_LOG_LEVEL = {}", v);
        unsafe { std::env::set_var("INT2DDS_FILE_LOG_LEVEL", v) };
    }
    if matches.get_flag("int2dds_thread_monitoring") {
        log::info!("Environment variable set: INT2DDS_THREAD_MONITORING = true");
        unsafe { std::env::set_var("INT2DDS_THREAD_MONITORING", "true") };
    }
    if let Some(v) = matches.get_one::<String>("int2dds_thread_monitoring_log_path") {
        log::info!("Environment variable set: INT2DDS_THREAD_MONITORING_LOG_PATH = {}", v);
        unsafe { std::env::set_var("INT2DDS_THREAD_MONITORING_LOG_PATH", v) };
    }
    if matches.get_flag("int2dds_function_timing") {
        log::info!("Environment variable set: INT2DDS_FUNCTION_TIMING = true");
        unsafe { std::env::set_var("INT2DDS_FUNCTION_TIMING", "true") };
    }
    if let Some(v) = matches.get_one::<String>("int2dds_function_timing_log_path") {
        log::info!("Environment variable set: INT2DDS_FUNCTION_TIMING_LOG_PATH = {}", v);
        unsafe { std::env::set_var("INT2DDS_FUNCTION_TIMING_LOG_PATH", v) };
    }
    if matches.get_flag("int2dds_extended_discovery") {
        log::info!("Environment variable set: INT2DDS_EXTENDED_DISCOVERY = true");
        unsafe { std::env::set_var("INT2DDS_EXTENDED_DISCOVERY", "true") };
    }
    if let Some(v) = matches.get_one::<String>("int2dds_network_interface") {
        log::info!("Environment variable set: INT2DDS_NETWORK_INTERFACE = {}", v);
        unsafe { std::env::set_var("INT2DDS_NETWORK_INTERFACE", v) };
    }
    if let Some(v) = matches.get_one::<String>("int2dds_network_ip") {
        log::info!("Environment variable set: INT2DDS_NETWORK_IP = {}", v);
        unsafe { std::env::set_var("INT2DDS_NETWORK_IP", v) };
    }
    if matches.get_flag("int2dds_use_loopback_interface") {
        log::info!("Environment variable set: INT2DDS_USE_LOOPBACK_INTERFACE = true");
        unsafe { std::env::set_var("INT2DDS_USE_LOOPBACK_INTERFACE", "true") };
    }
    if let Some(v) = matches.get_one::<String>("int2dds_udp_socket_buffer") {
        log::info!("Environment variable set: INT2DDS_UDP_SOCKET_BUFFER = {}", v);
        unsafe { std::env::set_var("INT2DDS_UDP_SOCKET_BUFFER", v) };
    }
    if let Some(v) = matches.get_one::<String>("int2dds_shm_buffer_size") {
        log::info!("Environment variable set: INT2DDS_SHM_BUFFER_SIZE = {}", v);
        unsafe { std::env::set_var("INT2DDS_SHM_BUFFER_SIZE", v) };
    }
    if let Some(v) = matches.get_one::<String>("int2dds_tcp_connect_timeout") {
        log::info!("Environment variable set: INT2DDS_TCP_CONNECT_TIMEOUT = {}", v);
        unsafe { std::env::set_var("INT2DDS_TCP_CONNECT_TIMEOUT", v) };
    }
    if let Some(v) = matches.get_one::<String>("int2dds_tcp_write_timeout") {
        log::info!("Environment variable set: INT2DDS_TCP_WRITE_TIMEOUT = {}", v);
        unsafe { std::env::set_var("INT2DDS_TCP_WRITE_TIMEOUT", v) };
    }
    if matches.get_flag("int2dds_tcp_nodelay") {
        log::info!("Environment variable set: INT2DDS_TCP_NODELAY = true");
        unsafe { std::env::set_var("INT2DDS_TCP_NODELAY", "true") };
    }
    if let Some(v) = matches.get_one::<String>("int2dds_initial_peers") {
        log::info!("Environment variable set: INT2DDS_INITIAL_PEERS = {}", v);
        unsafe { std::env::set_var("INT2DDS_INITIAL_PEERS", v) };
    }
    if let Some(v) = matches.get_one::<String>("int2dds_multicast_ttl") {
        log::info!("Environment variable set: INT2DDS_MULTICAST_TTL = {}", v);
        unsafe { std::env::set_var("INT2DDS_MULTICAST_TTL", v) };
    }
    if let Some(v) = matches.get_one::<String>("int2dds_external_address") {
        log::info!("Environment variable set: INT2DDS_EXTERNAL_ADDRESS = {}", v);
        unsafe { std::env::set_var("INT2DDS_EXTERNAL_ADDRESS", v) };
    }
    if let Some(v) = matches.get_one::<String>("int2dds_meta_port") {
        log::info!("Environment variable set: INT2DDS_META_PORT = {}", v);
        unsafe { std::env::set_var("INT2DDS_META_PORT", v) };
    }
    if let Some(v) = matches.get_one::<String>("int2dds_user_port") {
        log::info!("Environment variable set: INT2DDS_USER_PORT = {}", v);
        unsafe { std::env::set_var("INT2DDS_USER_PORT", v) };
    }
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

/// Set the TCP connect timeout via environment variable
pub fn set_tcp_connect_timeout(timeout_ms: u64) {
    log::info!("Environment variable set: INT2DDS_TCP_CONNECT_TIMEOUT = {}", timeout_ms);
    unsafe { std::env::set_var("INT2DDS_TCP_CONNECT_TIMEOUT", timeout_ms.to_string()) };
}

/// Set the TCP write timeout via environment variable
pub fn set_tcp_write_timeout(timeout_ms: u64) {
    log::info!("Environment variable set: INT2DDS_TCP_WRITE_TIMEOUT = {}", timeout_ms);
    unsafe { std::env::set_var("INT2DDS_TCP_WRITE_TIMEOUT", timeout_ms.to_string()) };
}

/// Set the TCP nodelay via environment variable
pub fn set_tcp_nodelay(enabled: bool) {
    log::info!("Environment variable set: INT2DDS_TCP_NODELAY = {}", enabled);
    unsafe { std::env::set_var("INT2DDS_TCP_NODELAY", enabled.to_string()) };
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
/// Format: "ip:port,ip:port,..." (comma-separated socket addresses, port must be metatraffic unicast port)
///
/// # Examples
///
/// ```no_run
/// use int2dds::common::env::get_initial_peers;
///
/// // Set environment variable:
/// // INT2DDS_INITIAL_PEERS="192.168.1.10:7400,192.168.1.11:7400"
///
/// let peers = get_initial_peers();
/// for peer in peers {
///     println!("Initial peer: {}", peer);
/// }
/// ```
pub fn get_initial_peers() -> Vec<std::net::SocketAddr> {
    std::env::var("INT2DDS_INITIAL_PEERS")
        .ok()
        .map(|peers_str| {
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
        })
        .unwrap_or_default()
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
