//! Dynamic FFI bindings for int2dds-feature library
//!
//! This module provides safe wrappers around dynamically loaded C FFI functions.
//! The library is loaded once on first use and function pointers are cached
//! for zero-overhead subsequent calls.
//!
//! If the library is not found, fallback behavior is used:
//! - `get_working_ip()` uses UDP socket to detect local IP (fallback)
//! - `init_extended_discovery()` returns `Ok(())` (no-op)
//! - `send_extended_discovery()` returns `Ok(())` (no-op)
//! - `get_heartbeat_period_seconds()` returns the default period

use libloading::{Library, Symbol};
use socket2::Socket;
use std::ffi::CStr;
use std::os::raw::c_char;
use std::sync::OnceLock;

#[cfg(windows)]
use std::os::windows::io::AsRawSocket;

#[cfg(unix)]
use std::os::unix::io::AsRawFd;

/// Raw socket handle type
pub type RawSocket = usize;

/// FFI return codes
pub const INT2DDS_FEATURE_OK: i32 = 0;
pub const INT2DDS_FEATURE_ERROR: i32 = 1;
pub const INT2DDS_FEATURE_NULL_POINTER: i32 = 100;
pub const INT2DDS_FEATURE_BUFFER_TOO_SMALL: i32 = 101;

// ============================================================================
// Platform-specific DLL names
// ============================================================================

#[cfg(windows)]
const LIBRARY_NAME: &str = "int2dds_feature.dll";

#[cfg(target_os = "linux")]
const LIBRARY_NAME: &str = "libint2dds_feature.so";

#[cfg(target_os = "macos")]
const LIBRARY_NAME: &str = "libint2dds_feature.dylib";

// ============================================================================
// Function pointer types
// ============================================================================

type GetWorkingIpFn = unsafe extern "C" fn(*mut c_char, usize) -> i32;
type InitExtendedDiscoveryFn = unsafe extern "C" fn(RawSocket) -> i32;
type SendExtendedDiscoveryFn = unsafe extern "C" fn(RawSocket, u16, *const u8, usize, u32) -> i32;
type GetHeartbeatPeriodFn = unsafe extern "C" fn(f64) -> f64;

// ============================================================================
// Cached function pointers (hot path first for cache efficiency)
// ============================================================================

/// Cached function pointers from the dynamic library
struct FeatureFunctions {
    /// HOT PATH: Called on every multicast send - placed first for cache efficiency
    send_extended_discovery: SendExtendedDiscoveryFn,
    /// Cold: Called once at startup
    get_working_ip: GetWorkingIpFn,
    /// Cold: Called once per socket creation
    init_extended_discovery: InitExtendedDiscoveryFn,
    /// Cold: Called once at startup
    get_heartbeat_period: GetHeartbeatPeriodFn,
}

/// Library handle and cached function pointers
struct FeatureLib {
    _library: Library, // Keep library alive for the lifetime of the process
    functions: FeatureFunctions,
}

// SAFETY: Function pointers remain valid for process lifetime.
// Library is never unloaded.
unsafe impl Send for FeatureLib {}
unsafe impl Sync for FeatureLib {}

/// Global singleton - loaded ONCE, never unloaded
static FEATURE_LIB: OnceLock<Option<FeatureLib>> = OnceLock::new();

// ============================================================================
// Library loading
// ============================================================================

/// Try to load the dynamic library from the executable's directory
fn try_load_library() -> std::io::Result<FeatureLib> {
    // Get the directory where the executable is located
    let exe_path = std::env::current_exe()?;
    let exe_dir = exe_path
        .parent()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "Cannot get exe dir"))?;

    let lib_path = exe_dir.join(LIBRARY_NAME);

    log::debug!("[int2dds_feature] Attempting to load library from: {}", lib_path.display());

    // Load the library
    let library = unsafe { Library::new(&lib_path) }.map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Failed to load {}: {}", lib_path.display(), e),
        )
    })?;

    // Load all function pointers
    unsafe {
        let send_extended_discovery: Symbol<SendExtendedDiscoveryFn> =
            library.get(b"int2dds_feature_send_extended_discovery").map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("Symbol int2dds_feature_send_extended_discovery not found: {}", e),
                )
            })?;

        let get_working_ip: Symbol<GetWorkingIpFn> =
            library.get(b"int2dds_feature_get_working_ip").map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("Symbol int2dds_feature_get_working_ip not found: {}", e),
                )
            })?;

        let init_extended_discovery: Symbol<InitExtendedDiscoveryFn> =
            library.get(b"int2dds_feature_init_extended_discovery").map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("Symbol int2dds_feature_init_extended_discovery not found: {}", e),
                )
            })?;

        let get_heartbeat_period: Symbol<GetHeartbeatPeriodFn> =
            library.get(b"int2dds_feature_get_heartbeat_period_seconds").map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("Symbol int2dds_feature_get_heartbeat_period_seconds not found: {}", e),
                )
            })?;

        // Copy function pointers (they remain valid as long as library is loaded)
        let functions = FeatureFunctions {
            send_extended_discovery: *send_extended_discovery,
            get_working_ip: *get_working_ip,
            init_extended_discovery: *init_extended_discovery,
            get_heartbeat_period: *get_heartbeat_period,
        };

        Ok(FeatureLib { _library: library, functions })
    }
}

/// Get the feature library reference, loading it on first access.
/// Returns None if the library is not available (fallback mode).
///
/// Performance after initialization:
/// - 1 atomic load with Acquire ordering (~1-3 cycles)
/// - Branch on Option (well-predicted - usually Some or always None)
#[inline]
fn get_feature_lib() -> Option<&'static FeatureLib> {
    FEATURE_LIB
        .get_or_init(|| match try_load_library() {
            Ok(lib) => {
                log::info!(
                    "[int2dds_feature] Dynamic library {} loaded successfully",
                    LIBRARY_NAME
                );
                Some(lib)
            }
            Err(e) => {
                log::info!(
                    "[int2dds_feature] {} not available, using fallback mode: {}",
                    LIBRARY_NAME,
                    e
                );
                None
            }
        })
        .as_ref()
}

// ============================================================================
// Public API - Safe wrappers (same signatures as before)
// ============================================================================

/// Get working IP address
///
/// Safe wrapper around the FFI function.
/// If the library is not available, falls back to UDP socket detection method.
pub fn get_working_ip() -> std::io::Result<String> {
    // Try dynamic library first
    if let Some(lib) = get_feature_lib() {
        let mut buffer = [0i8; 64];
        let ret = unsafe {
            (lib.functions.get_working_ip)(buffer.as_mut_ptr() as *mut c_char, buffer.len())
        };

        if ret == INT2DDS_FEATURE_OK {
            let cstr = unsafe { CStr::from_ptr(buffer.as_ptr() as *const c_char) };
            let ip = cstr.to_string_lossy().into_owned();
            log::info!("[int2dds_feature] Using IP from library: {}", ip);
            return Ok(ip);
        }
        log::warn!("[int2dds_feature] get_working_ip failed with code {}, using fallback", ret);
    }

    // Fallback: Use UDP socket to detect local IP
    get_working_ip_fallback()
}

/// Fallback method to get working IP using UDP socket
fn get_working_ip_fallback() -> std::io::Result<String> {
    use std::net::UdpSocket;

    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.connect("8.8.8.8:80")?;

    let local_addr = socket.local_addr()?;
    let ip = local_addr.ip().to_string();
    log::info!("[int2dds_feature] Using fallback IP detection: {}", ip);
    Ok(ip)
}

/// Get heartbeat period in seconds
///
/// Safe wrapper around the FFI function.
/// If the library is not available, returns the default_period unchanged.
#[inline]
pub fn get_heartbeat_period_seconds(default_period: f64) -> f64 {
    match get_feature_lib() {
        Some(lib) => unsafe { (lib.functions.get_heartbeat_period)(default_period) },
        None => default_period,
    }
}

/// Initialize extended discovery on a socket
///
/// Safe wrapper around the FFI function.
/// If the library is not available, returns Ok(()) (no-op).
pub fn init_extended_discovery(socket: &Socket) -> std::io::Result<()> {
    let lib = match get_feature_lib() {
        Some(lib) => lib,
        None => return Ok(()), // Fallback: no-op
    };

    let raw = socket_to_raw(socket);
    let ret = unsafe { (lib.functions.init_extended_discovery)(raw) };

    if ret == INT2DDS_FEATURE_OK {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "int2dds_feature_init_extended_discovery failed with code {}",
            ret
        )))
    }
}

/// Send extended discovery message - HOT PATH
///
/// This function is called on every multicast send.
/// Performance characteristics after initialization:
/// - 1 atomic load (Acquire ordering)
/// - 1 Option check (branch prediction friendly)
/// - 1 indirect function call through cached pointer
///
/// If the library is not available, returns Ok(()) (no-op, extended discovery disabled).
#[inline]
pub fn send_extended_discovery(
    socket: &Socket,
    port: u16,
    data: &[u8],
    domain_id: u32,
) -> std::io::Result<()> {
    // Fast path check
    let lib = match get_feature_lib() {
        Some(lib) => lib,
        None => return Ok(()), // Fallback: extended discovery not available
    };

    let raw = socket_to_raw(socket);
    let ret = unsafe {
        (lib.functions.send_extended_discovery)(raw, port, data.as_ptr(), data.len(), domain_id)
    };

    if ret == INT2DDS_FEATURE_OK {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "int2dds_feature_send_extended_discovery failed with code {}",
            ret
        )))
    }
}

// ============================================================================
// Helper functions
// ============================================================================

/// Convert Socket to raw handle (without taking ownership)
#[cfg(windows)]
#[inline]
fn socket_to_raw(socket: &Socket) -> RawSocket {
    socket.as_raw_socket() as usize
}

#[cfg(unix)]
#[inline]
fn socket_to_raw(socket: &Socket) -> RawSocket {
    socket.as_raw_fd() as usize
}
