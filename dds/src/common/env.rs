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
    // - INT2DDS_USE_LOOPBACK_INTERFACE: Enable loopback interface for discovery and endpoint communication (true, false) - Default: false
    // - INT2DDS_UDP_SOCKET_BUFFER: Set UDP socket buffer size (bytes) - Default: OS default
    // - INT2DDS_SHM_BUFFER_SIZE: Set shared memory buffer size (bytes) - Default: 1048576 (1MB)

    // - INT2DDS_TCP_PORT: set TCP listening port - Default: 7400 + 250 * domain_id
    // - INT2DDS_TCP_CONNECT_TIMEOUT: Set TCP connection timeout (milliseconds) - Default: 5000
    // - INT2DDS_TCP_WRITE_TIMEOUT: Set TCP write timeout (milliseconds) - Default: 10000
    // - INT2DDS_TCP_NODELAY: Enable TCP Nodelay (disable Nagle algorithm) (true, false) - Default: true
    // - INT2DDS_TCP_BIND_TIMEOUT: Set TCP BIND handshake response timeout (milliseconds) - Default: 5000
    // - INT2DDS_TCP_KEEPALIVE_INTERVAL: Set TCP control keepalive send interval (milliseconds) - Default: 30000
    // - INT2DDS_TCP_KEEPALIVE_TIMEOUT: Set TCP keepalive response timeout (milliseconds) - Default: 10000
    // - INT2DDS_TCP_KEEPALIVE_MAX_MISSES: Set TCP keepalive max consecutive misses before disconnect - Default: 3
    // - INT2DDS_TCP_INCOMING_IDLE_TIMEOUT: Set idle timeout for incoming TCP connections (milliseconds) - Default: 10000
    // - INT2DDS_TCP_ORPHAN_DATA_GRACE: Set grace period before orphan data connections are torn down (milliseconds) - Default: 1000
    // - INT2DDS_TCP_SO_RCVBUF: Force SO_RCVBUF on every TCP socket (bytes). Used by tests to induce backpressure - Default: OS-managed
    // - INT2DDS_TCP_SO_SNDBUF: Force SO_SNDBUF on every TCP socket (bytes). Used by tests to induce backpressure - Default: OS-managed

    // - INT2DDS_INITIAL_PEERS: Set initial peers for SPDP unicast discovery (comma-separated, e.g., "192.168.1.10:7400,192.168.1.11:7400") - Default: none

    // - INT2DDS_TCP_PUBLIC_ADDR: Set public address for WAN/NAT traversal (e.g., "203.0.113.5:7400") - Default: none (LAN mode)
    // - INT2DDS_TCP_TLS_ENABLED: Enable TLS for TCP connections (true, false) - Default: false (not yet implemented)
    // - INT2DDS_TCP_TLS_CERT_PATH: TLS certificate file path - Default: none (not yet implemented)
    // - INT2DDS_TCP_TLS_KEY_PATH: TLS private key file path - Default: none (not yet implemented)
    // - INT2DDS_TCP_TLS_CA_PATH: TLS CA certificate file path - Default: none (not yet implemented)

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
                Arg::new("int2dds_tcp_port")
                    .long("int2dds-tcp-port")
                    .value_name("PORT")
                    .help("TCP listening port")
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
                Arg::new("int2dds_tcp_bind_timeout")
                    .long("int2dds-tcp-bind-timeout")
                    .value_name("MILLISECONDS")
                    .help("TCP BIND handshake response timeout (milliseconds)")
                    .num_args(1)
                    .value_hint(ValueHint::Other),
            )
            .arg(
                Arg::new("int2dds_tcp_keepalive_interval")
                    .long("int2dds-tcp-keepalive-interval")
                    .value_name("MILLISECONDS")
                    .help("TCP control keepalive send interval (milliseconds)")
                    .num_args(1)
                    .value_hint(ValueHint::Other),
            )
            .arg(
                Arg::new("int2dds_tcp_keepalive_timeout")
                    .long("int2dds-tcp-keepalive-timeout")
                    .value_name("MILLISECONDS")
                    .help("TCP keepalive response timeout (milliseconds)")
                    .num_args(1)
                    .value_hint(ValueHint::Other),
            )
            .arg(
                Arg::new("int2dds_tcp_keepalive_max_misses")
                    .long("int2dds-tcp-keepalive-max-misses")
                    .value_name("COUNT")
                    .help("TCP keepalive max consecutive misses before disconnect")
                    .num_args(1)
                    .value_hint(ValueHint::Other),
            )
            .arg(
                Arg::new("int2dds_tcp_incoming_idle_timeout")
                    .long("int2dds-tcp-incoming-idle-timeout")
                    .value_name("MILLISECONDS")
                    .help("Idle timeout for incoming TCP connections (milliseconds)")
                    .num_args(1)
                    .value_hint(ValueHint::Other),
            )
            .arg(
                Arg::new("int2dds_tcp_orphan_data_grace")
                    .long("int2dds-tcp-orphan-data-grace")
                    .value_name("MILLISECONDS")
                    .help("Grace period before orphan data connections are torn down (milliseconds)")
                    .num_args(1)
                    .value_hint(ValueHint::Other),
            )
            .arg(
                Arg::new("int2dds_tcp_so_rcvbuf")
                    .long("int2dds-tcp-so-rcvbuf")
                    .value_name("BYTES")
                    .help("Force SO_RCVBUF on every TCP socket (bytes). Used by tests to induce backpressure")
                    .num_args(1)
                    .value_hint(ValueHint::Other),
            )
            .arg(
                Arg::new("int2dds_tcp_so_sndbuf")
                    .long("int2dds-tcp-so-sndbuf")
                    .value_name("BYTES")
                    .help("Force SO_SNDBUF on every TCP socket (bytes). Used by tests to induce backpressure")
                    .num_args(1)
                    .value_hint(ValueHint::Other),
            )
            .arg(
                Arg::new("int2dds_initial_peers")
                    .long("int2dds-initial-peers")
                    .value_name("PEERS")
                    .help("Initial peers for SPDP unicast discovery (comma-separated, e.g., \"192.168.1.10:7400,192.168.1.11:7400\")")
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
    if let Some(v) = matches.get_one::<String>("int2dds_tcp_port") {
        log::info!("Environment variable set: INT2DDS_TCP_PORT = {}", v);
        unsafe { std::env::set_var("INT2DDS_TCP_PORT", v) };
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
    if let Some(v) = matches.get_one::<String>("int2dds_tcp_bind_timeout") {
        log::info!("Environment variable set: INT2DDS_TCP_BIND_TIMEOUT = {}", v);
        unsafe { std::env::set_var("INT2DDS_TCP_BIND_TIMEOUT", v) };
    }
    if let Some(v) = matches.get_one::<String>("int2dds_tcp_keepalive_interval") {
        log::info!("Environment variable set: INT2DDS_TCP_KEEPALIVE_INTERVAL = {}", v);
        unsafe { std::env::set_var("INT2DDS_TCP_KEEPALIVE_INTERVAL", v) };
    }
    if let Some(v) = matches.get_one::<String>("int2dds_tcp_keepalive_timeout") {
        log::info!("Environment variable set: INT2DDS_TCP_KEEPALIVE_TIMEOUT = {}", v);
        unsafe { std::env::set_var("INT2DDS_TCP_KEEPALIVE_TIMEOUT", v) };
    }
    if let Some(v) = matches.get_one::<String>("int2dds_tcp_keepalive_max_misses") {
        log::info!("Environment variable set: INT2DDS_TCP_KEEPALIVE_MAX_MISSES = {}", v);
        unsafe { std::env::set_var("INT2DDS_TCP_KEEPALIVE_MAX_MISSES", v) };
    }
    if let Some(v) = matches.get_one::<String>("int2dds_tcp_incoming_idle_timeout") {
        log::info!("Environment variable set: INT2DDS_TCP_INCOMING_IDLE_TIMEOUT = {}", v);
        unsafe { std::env::set_var("INT2DDS_TCP_INCOMING_IDLE_TIMEOUT", v) };
    }
    if let Some(v) = matches.get_one::<String>("int2dds_tcp_orphan_data_grace") {
        log::info!("Environment variable set: INT2DDS_TCP_ORPHAN_DATA_GRACE = {}", v);
        unsafe { std::env::set_var("INT2DDS_TCP_ORPHAN_DATA_GRACE", v) };
    }
    if let Some(v) = matches.get_one::<String>("int2dds_tcp_so_rcvbuf") {
        log::info!("Environment variable set: INT2DDS_TCP_SO_RCVBUF = {}", v);
        unsafe { std::env::set_var("INT2DDS_TCP_SO_RCVBUF", v) };
    }
    if let Some(v) = matches.get_one::<String>("int2dds_tcp_so_sndbuf") {
        log::info!("Environment variable set: INT2DDS_TCP_SO_SNDBUF = {}", v);
        unsafe { std::env::set_var("INT2DDS_TCP_SO_SNDBUF", v) };
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
pub fn get_initial_peers() -> Vec<std::net::SocketAddr> {
    use crate::rtps::transport::get_transport_type;
    use crate::rtps::transport::port_manager::PortManager;
    use crate::rtps::transport::TransportType;

    let default_port = match get_transport_type() {
        TransportType::TCP | TransportType::Hybrid => {
            // TCP/Hybrid: user domain 0 physical port as default
            // (domain_id is not available here, so use domain 0 base port)
            PortManager::get_tcp_physical_port(0)
        }
        _ => {
            // UDP: use domain 0 discovery multicast port as default
            PortManager::get_discovery_traffic_multicast_port(0)
        }
    };

    std::env::var("INT2DDS_INITIAL_PEERS")
        .ok()
        .map(|peers_str| {
            peers_str
                .split(',')
                .filter_map(|s| {
                    let trimmed = s.trim();
                    // Try parsing as full SocketAddr first (ip:port)
                    match trimmed.parse::<std::net::SocketAddr>() {
                        Ok(addr) => Some(addr),
                        Err(_) => {
                            // Try parsing as IP only, append default port
                            match trimmed.parse::<std::net::IpAddr>() {
                                Ok(ip) => {
                                    let addr = std::net::SocketAddr::new(ip, default_port);
                                    log::info!(
                                        "[ENV] Initial peer '{}' has no port, using default: {}",
                                        trimmed,
                                        addr
                                    );
                                    Some(addr)
                                }
                                Err(e) => {
                                    log::warn!("Failed to parse initial peer '{}': {}", trimmed, e);
                                    None
                                }
                            }
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

/// Get the TCP BIND handshake timeout in milliseconds
/// Default: 5000ms
pub fn get_tcp_bind_timeout_ms() -> u64 {
    std::env::var("INT2DDS_TCP_BIND_TIMEOUT").ok().and_then(|v| v.parse().ok()).unwrap_or(5000)
}

/// Set the TCP BIND handshake timeout via environment variable
pub fn set_tcp_bind_timeout(timeout_ms: u64) {
    log::info!("Environment variable set: INT2DDS_TCP_BIND_TIMEOUT = {}", timeout_ms);
    unsafe { std::env::set_var("INT2DDS_TCP_BIND_TIMEOUT", timeout_ms.to_string()) };
}

/// Get the TCP keepalive send interval in milliseconds
/// Default: 30000ms (30 seconds)
pub fn get_tcp_keepalive_interval_ms() -> u64 {
    std::env::var("INT2DDS_TCP_KEEPALIVE_INTERVAL")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(30000)
}

/// Set the TCP keepalive send interval via environment variable
pub fn set_tcp_keepalive_interval(interval_ms: u64) {
    log::info!("Environment variable set: INT2DDS_TCP_KEEPALIVE_INTERVAL = {}", interval_ms);
    unsafe { std::env::set_var("INT2DDS_TCP_KEEPALIVE_INTERVAL", interval_ms.to_string()) };
}

/// Get the TCP keepalive response timeout in milliseconds
/// Default: 10000ms (10 seconds)
pub fn get_tcp_keepalive_timeout_ms() -> u64 {
    std::env::var("INT2DDS_TCP_KEEPALIVE_TIMEOUT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10000)
}

/// Set the TCP keepalive response timeout via environment variable
pub fn set_tcp_keepalive_timeout(timeout_ms: u64) {
    log::info!("Environment variable set: INT2DDS_TCP_KEEPALIVE_TIMEOUT = {}", timeout_ms);
    unsafe { std::env::set_var("INT2DDS_TCP_KEEPALIVE_TIMEOUT", timeout_ms.to_string()) };
}

/// Get the TCP keepalive max consecutive misses before disconnecting
/// Default: 3
pub fn get_tcp_keepalive_max_misses() -> u32 {
    std::env::var("INT2DDS_TCP_KEEPALIVE_MAX_MISSES").ok().and_then(|v| v.parse().ok()).unwrap_or(3)
}

/// Set the TCP keepalive max misses via environment variable
pub fn set_tcp_keepalive_max_misses(max_misses: u32) {
    log::info!("Environment variable set: INT2DDS_TCP_KEEPALIVE_MAX_MISSES = {}", max_misses);
    unsafe { std::env::set_var("INT2DDS_TCP_KEEPALIVE_MAX_MISSES", max_misses.to_string()) };
}

/// Get the idle timeout for incoming TCP connections in milliseconds
/// Default: 10000ms (10 seconds)
pub fn get_tcp_incoming_idle_timeout_ms() -> u64 {
    std::env::var("INT2DDS_TCP_INCOMING_IDLE_TIMEOUT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10_000)
}

/// Set the idle timeout for incoming TCP connections via environment variable
pub fn set_tcp_incoming_idle_timeout(timeout_ms: u64) {
    log::info!("Environment variable set: INT2DDS_TCP_INCOMING_IDLE_TIMEOUT = {}", timeout_ms);
    unsafe { std::env::set_var("INT2DDS_TCP_INCOMING_IDLE_TIMEOUT", timeout_ms.to_string()) };
}

/// Get the grace period before orphan data connections are torn down in milliseconds
/// Default: 1000ms (1 second)
pub fn get_tcp_orphan_data_grace_ms() -> u64 {
    std::env::var("INT2DDS_TCP_ORPHAN_DATA_GRACE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1_000)
}

/// Set the grace period before orphan data connections are torn down via environment variable
pub fn set_tcp_orphan_data_grace(grace_ms: u64) {
    log::info!("Environment variable set: INT2DDS_TCP_ORPHAN_DATA_GRACE = {}", grace_ms);
    unsafe { std::env::set_var("INT2DDS_TCP_ORPHAN_DATA_GRACE", grace_ms.to_string()) };
}

/// Get the TCP write timeout in milliseconds
/// Default: 10000ms (10 seconds)
pub fn get_tcp_write_timeout_ms() -> u64 {
    std::env::var("INT2DDS_TCP_WRITE_TIMEOUT").ok().and_then(|v| v.parse().ok()).unwrap_or(10_000)
}

/// Get the TCP_NODELAY flag (true = disable Nagle, false = enable Nagle)
/// Default: true
pub fn get_tcp_nodelay() -> bool {
    std::env::var("INT2DDS_TCP_NODELAY").ok().and_then(|v| v.parse().ok()).unwrap_or(true)
}

/// Get the optional SO_RCVBUF override (bytes) for every TCP socket.
/// Used by tests to induce backpressure deterministically.
/// Default: None (OS-managed)
pub fn get_tcp_so_rcvbuf() -> Option<usize> {
    std::env::var("INT2DDS_TCP_SO_RCVBUF").ok().and_then(|v| v.parse().ok())
}

/// Set the optional SO_RCVBUF override via environment variable
pub fn set_tcp_so_rcvbuf(bytes: usize) {
    log::info!("Environment variable set: INT2DDS_TCP_SO_RCVBUF = {}", bytes);
    unsafe { std::env::set_var("INT2DDS_TCP_SO_RCVBUF", bytes.to_string()) };
}

/// Get the optional SO_SNDBUF override (bytes) for every TCP socket.
/// Used by tests to induce backpressure deterministically.
/// Default: None (OS-managed)
pub fn get_tcp_so_sndbuf() -> Option<usize> {
    std::env::var("INT2DDS_TCP_SO_SNDBUF").ok().and_then(|v| v.parse().ok())
}

/// Set the optional SO_SNDBUF override via environment variable
pub fn set_tcp_so_sndbuf(bytes: usize) {
    log::info!("Environment variable set: INT2DDS_TCP_SO_SNDBUF = {}", bytes);
    unsafe { std::env::set_var("INT2DDS_TCP_SO_SNDBUF", bytes.to_string()) };
}

/// Get the TCP public address for WAN/NAT traversal.
/// When set, SPDP locators advertise this address instead of the local working IP.
///
/// Format: "ip:port" (e.g., "203.0.113.5:7400")
/// Default: None (LAN mode, use working IP)
pub fn get_tcp_public_addr() -> Option<std::net::SocketAddr> {
    std::env::var("INT2DDS_TCP_PUBLIC_ADDR").ok().and_then(|v| {
        match v.trim().parse::<std::net::SocketAddr>() {
            Ok(addr) => Some(addr),
            Err(e) => {
                log::warn!("Failed to parse INT2DDS_TCP_PUBLIC_ADDR '{}': {}", v, e);
                None
            }
        }
    })
}

/// Set the TCP public address via environment variable
pub fn set_tcp_public_addr(addr: &std::net::SocketAddr) {
    log::info!("Environment variable set: INT2DDS_TCP_PUBLIC_ADDR = {}", addr);
    unsafe { std::env::set_var("INT2DDS_TCP_PUBLIC_ADDR", addr.to_string()) };
}

/// Get the custom TCP physical port.
/// When set, overrides the default port calculation (7400 + 250 * domain_id).
///
/// Environment variable: INT2DDS_TCP_PORT
/// Default: None (use calculated port)
pub fn get_tcp_port() -> Option<u16> {
    std::env::var("INT2DDS_TCP_PORT").ok().and_then(|v| v.parse::<u16>().ok())
}

/// Set the TCP physical port via environment variable.
pub fn set_tcp_port(port: u16) {
    log::info!("Environment variable set: INT2DDS_TCP_PORT = {}", port);
    unsafe { std::env::set_var("INT2DDS_TCP_PORT", port.to_string()) };
}

/// Check if TLS is enabled for TCP connections.
/// Default: false (not yet implemented)
pub fn is_tcp_tls_enabled() -> bool {
    let enabled = std::env::var("INT2DDS_TCP_TLS_ENABLED")
        .ok()
        .and_then(|v| v.parse::<bool>().ok())
        .unwrap_or(false);

    if enabled {
        log::warn!(
            "INT2DDS_TCP_TLS_ENABLED=true but TLS is not yet implemented. Running plain TCP."
        );
    }
    enabled
}

/// Get the TLS certificate file path (stub, not yet implemented)
pub fn get_tcp_tls_cert_path() -> Option<String> {
    std::env::var("INT2DDS_TCP_TLS_CERT_PATH").ok()
}

/// Get the TLS private key file path (stub, not yet implemented)
pub fn get_tcp_tls_key_path() -> Option<String> {
    std::env::var("INT2DDS_TCP_TLS_KEY_PATH").ok()
}

/// Get the TLS CA certificate file path (stub, not yet implemented)
pub fn get_tcp_tls_ca_path() -> Option<String> {
    std::env::var("INT2DDS_TCP_TLS_CA_PATH").ok()
}
