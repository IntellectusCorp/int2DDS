//! Environment variable and command-line argument configuration.
//!
//! This module provides initialization from environment variables and command-line arguments
//! for configuring int2dds behavior, including transport type, logging, network interface
//! selection, and performance monitoring features.

use clap::{Arg, ArgAction, Command, ValueHint};

use crate::{
    common::log::{setting_log, LogLevel, LogType},
    rtps::transport::TransportType,
};

pub fn init_from_env() {
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
    // - INT2DDS_UDP_SOCKET_BUFFER: Set UDP socket buffer size (bytes) - Default: OS default
    // - INT2DDS_SHM_BUFFER_SIZE: Set shared memory buffer size (bytes) - Default: 1048576 (1MB)

    // - INT2DDS_TCP_CONNECT_TIMEOUT: Set TCP connection timeout (milliseconds) - Default: 5000
    // - INT2DDS_TCP_WRITE_TIMEOUT: Set TCP write timeout (milliseconds) - Default: 10000
    // - INT2DDS_TCP_NODELAY: Enable TCP Nodelay (disable Nagle algorithm) (true, false) - Default: true

    // - INT2DDS_INITIAL_PEERS: Set initial peers for TCP/Hybrid discovery (comma-separated, e.g., "192.168.1.10:7412,192.168.1.11:7412") - Default: none

    apply_cli_args_to_env();

    setting_log();
}

fn apply_cli_args_to_env() {
    fn build_command() -> Command {
        Command::new("int2dds")
            // .disable_help_subcommand(true)
            // .arg_required_else_help(false)
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
                    .help("Initial peers for TCP/Hybrid discovery (comma-separated, e.g., \"192.168.1.10:7412,192.168.1.11:7412\")")
                    .num_args(1)
                    .value_hint(ValueHint::Other),
            )
    }

    let matches = build_command()
        .try_get_matches_from(std::env::args())
        .unwrap_or_else(|_| build_command().get_matches_from(Vec::<String>::new()));

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

/// Set the network interface via environment variable
pub fn set_network_interface(interface: &str) {
    log::info!("Environment variable set: INT2DDS_NETWORK_INTERFACE = {}", interface);
    unsafe { std::env::set_var("INT2DDS_NETWORK_INTERFACE", interface) };
}

/// Set the network IP via environment variable
pub fn set_network_ip(ip: &str) {
    log::info!("Environment variable set: INT2DDS_NETWORK_IP = {}", ip);
    unsafe { std::env::set_var("INT2DDS_NETWORK_IP", ip) };
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

/// Get initial peers from environment variable for TCP-only discovery
///
/// Reads INT2DDS_INITIAL_PEERS environment variable.
/// Format: "ip:port,ip:port,..." (comma-separated socket addresses)
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
