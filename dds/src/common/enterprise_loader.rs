//! Runtime loader for the optional enterprise library.

use libloading::{Library, Symbol};
use socket2::Socket;
use std::ffi::CStr;
use std::os::raw::c_char;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::dcps::core::error::{DdsError, DdsResult};

/// Must match `INT2DDS_EE_ABI_VERSION` in the enterprise library.
const EXPECTED_ABI_VERSION: u32 = 1;

#[cfg(windows)]
const LIBRARY_NAME: &str = "int2dds_enterprise.dll";
#[cfg(target_os = "linux")]
const LIBRARY_NAME: &str = "libint2dds_enterprise.so";
#[cfg(target_os = "macos")]
const LIBRARY_NAME: &str = "libint2dds_enterprise.dylib";

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct GateResult {
    code: i32,
    days: i32,
}

type AbiVersionFn = unsafe extern "C" fn() -> u32;
type GuardFn = unsafe extern "C" fn(*mut GateResult) -> i32;
type ResolveIpFn = unsafe extern "C" fn(*mut c_char, usize) -> i32;
type DiscoveryInitFn = unsafe extern "C" fn(usize) -> i32;
type DiscoverySendFn = unsafe extern "C" fn(usize, u16, *const u8, usize, u32) -> i32;
type HeartbeatPeriodFn = unsafe extern "C" fn(f64) -> f64;

struct Entries {
    discovery_send: DiscoverySendFn,
    guard: GuardFn,
    resolve_ip: ResolveIpFn,
    discovery_init: DiscoveryInitFn,
    heartbeat_period: HeartbeatPeriodFn,
}

struct Enterprise {
    /// Keeps the library mapped for the process lifetime; never unloaded.
    _library: Library,
    entries: Entries,
}

// SAFETY: the function pointers stay valid as long as the library is mapped,
// and it is never unloaded.
unsafe impl Send for Enterprise {}
unsafe impl Sync for Enterprise {}

enum LoadState {
    Loaded(Enterprise),
    Absent(Vec<String>),
    Failed(String),
}

static ENTERPRISE: OnceLock<LoadState> = OnceLock::new();

/// Where to look, and whether the caller asked explicitly.
///
/// A set `INT2DDS_ENTERPRISE_PATH` replaces the search entirely rather than going
/// in front of the default, which is what makes "set, and not there" a hard
/// failure instead of a silent fall-through to some other copy.
///
/// Every candidate is a complete path; never a bare file name.
fn candidate_paths() -> (Vec<PathBuf>, bool) {
    if let Ok(configured) = std::env::var("INT2DDS_ENTERPRISE_PATH") {
        let paths = std::env::split_paths(&configured)
            .filter(|p| !p.as_os_str().is_empty())
            // Accept either the library file itself or the directory holding it.
            .map(|p| if p.is_dir() { p.join(LIBRARY_NAME) } else { p })
            .map(|p| std::path::absolute(&p).unwrap_or(p))
            .collect();
        return (paths, true);
    }
    let mut out = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            out.push(dir.join(LIBRARY_NAME));
        }
    }
    (out, false)
}

fn try_load() -> LoadState {
    let (paths, explicit) = candidate_paths();
    let mut tried: Vec<String> = Vec::new();
    // Subset of `tried` that existed on disk, as opposed to simply not being there.
    let mut rejected: Vec<String> = Vec::new();
    for path in &paths {
        match unsafe { Library::new(path) } {
            Ok(library) => match resolve_entries(library, path) {
                Ok(e) => return LoadState::Loaded(e),
                Err(why) => {
                    tried.push(why.clone());
                    rejected.push(why);
                }
            },
            Err(e) => {
                let msg = format!("{}: {e}", path.display());
                tried.push(msg.clone());
                if path.exists() {
                    rejected.push(msg);
                }
            }
        }
    }
    if explicit {
        LoadState::Failed(explicit_failure_message(&tried))
    } else {
        LoadState::Absent(rejected)
    }
}

fn explicit_failure_message(tried: &[String]) -> String {
    let mut m = String::from(
        "INT2DDS_ENTERPRISE_PATH is set, but no usable enterprise library was found.\nTried:\n",
    );
    if tried.is_empty() {
        m.push_str("  (no candidate path — the variable is set to an empty value)\n");
    }
    for t in tried {
        m.push_str("  ");
        m.push_str(t);
        m.push('\n');
    }
    m.push_str(&format!("Expected file name: {LIBRARY_NAME}\n"));
    m.push_str("Point it at the library file or the directory holding it:\n");
    // `\x20` keeps the two-space indent: a `\` before the newline eats the next
    // line's leading whitespace along with it.
    #[cfg(windows)]
    m.push_str(
        "  PowerShell : $env:INT2DDS_ENTERPRISE_PATH = 'C:\\path\\to\\dir'\n\
         \x20 cmd.exe    : set INT2DDS_ENTERPRISE_PATH=C:\\path\\to\\dir\n\
         Or clear it to go back to searching next to the executable:\n\
         \x20 PowerShell : Remove-Item Env:INT2DDS_ENTERPRISE_PATH\n\
         \x20 cmd.exe    : set INT2DDS_ENTERPRISE_PATH=\n",
    );
    // Spelling out `unset` matters: `export INT2DDS_ENTERPRISE_PATH=` is the
    // natural guess and lands right back on the empty-value failure above.
    #[cfg(unix)]
    m.push_str(
        "  bash/zsh   : export INT2DDS_ENTERPRISE_PATH=/path/to/dir\n\
         Or clear it to go back to searching next to the executable:\n\
         \x20 bash/zsh   : unset INT2DDS_ENTERPRISE_PATH\n\
         (`export INT2DDS_ENTERPRISE_PATH=` leaves it set to an empty value, \
         which still fails here.)\n",
    );
    m.push_str(
        "With it unset and no library next to the executable, the process runs \
         open-core behaviour.",
    );
    m
}

fn resolve_entries(library: Library, path: &Path) -> Result<Enterprise, String> {
    unsafe {
        let abi: Symbol<AbiVersionFn> = library
            .get(b"int2dds_ee_abi_version")
            .map_err(|e| format!("{}: int2dds_ee_abi_version missing: {e}", path.display()))?;
        let found = abi();
        if found != EXPECTED_ABI_VERSION {
            return Err(format!(
                "{}: ABI version {found}, this core expects {EXPECTED_ABI_VERSION}",
                path.display()
            ));
        }

        macro_rules! entry {
            ($ty:ty, $name:literal) => {{
                let s: Symbol<$ty> = library.get($name).map_err(|e| {
                    format!("{}: {} missing: {e}", path.display(), String::from_utf8_lossy($name))
                })?;
                *s
            }};
        }

        let entries = Entries {
            discovery_send: entry!(DiscoverySendFn, b"int2dds_ee_discovery_send"),
            guard: entry!(GuardFn, b"int2dds_ee_guard"),
            resolve_ip: entry!(ResolveIpFn, b"int2dds_ee_resolve_ip"),
            discovery_init: entry!(DiscoveryInitFn, b"int2dds_ee_discovery_init"),
            heartbeat_period: entry!(HeartbeatPeriodFn, b"int2dds_ee_heartbeat_period"),
        };
        log::info!("[enterprise] loaded {}", path.display());
        Ok(Enterprise { _library: library, entries })
    }
}

#[inline]
fn state() -> &'static LoadState {
    ENTERPRISE.get_or_init(|| {
        let s = try_load();
        match &s {
            LoadState::Loaded(_) => {}
            LoadState::Absent(reasons) if reasons.is_empty() => log::info!(
                "[enterprise] no enterprise library next to the executable; \
                 running open-core behaviour"
            ),
            LoadState::Absent(reasons) => {
                log::warn!("[enterprise] enterprise library not loaded: {}", reasons.join("; "))
            }
            LoadState::Failed(why) => log::error!("[enterprise] {why}"),
        }
        s
    })
}

#[inline]
fn enterprise() -> Option<&'static Enterprise> {
    match state() {
        LoadState::Loaded(e) => Some(e),
        _ => None,
    }
}

#[inline]
pub(crate) fn is_loaded() -> bool {
    enterprise().is_some()
}

/// Fail-open when no library is present; refuse when `INT2DDS_ENTERPRISE_PATH`
/// was set and the library could not be loaded.
pub(crate) fn participant_gate() -> DdsResult<()> {
    let ee = match state() {
        LoadState::Loaded(e) => e,
        LoadState::Absent(_) => return Ok(()),
        LoadState::Failed(why) => return Err(DdsError::Error(why.clone())),
    };
    let mut result = GateResult::default();
    // SAFETY: `result` is a live, correctly typed out-parameter and the pointer
    // came from the library whose ABI version we verified at load time.
    let code = unsafe { (ee.entries.guard)(&mut result) };
    if code != 0 {
        return Err(DdsError::Error(format!("participant creation refused (code {code})")));
    }
    if result.days > 0 {
        log::warn!("int2dds license expires in {} day(s)", result.days);
    } else if result.days < 0 {
        log::warn!("int2dds license EXPIRED {} day(s) ago (grace period)", -result.days);
    }
    Ok(())
}

/// `None` means the core uses its own detection.
pub(crate) fn resolve_ip() -> Option<String> {
    let ee = enterprise()?;
    let mut buffer = [0 as c_char; 64];
    // SAFETY: `buffer` is a live array of exactly `buffer.len()` elements.
    let ret = unsafe { (ee.entries.resolve_ip)(buffer.as_mut_ptr(), buffer.len()) };
    if ret != 0 {
        log::warn!("[enterprise] resolve_ip failed with code {ret}");
        return None;
    }
    // SAFETY: on success the library wrote a null-terminated string in range.
    let s = unsafe { CStr::from_ptr(buffer.as_ptr()) };
    Some(s.to_string_lossy().into_owned())
}

#[cfg(windows)]
#[inline]
fn socket_to_raw(socket: &Socket) -> usize {
    use std::os::windows::io::AsRawSocket;
    socket.as_raw_socket() as usize
}

#[cfg(unix)]
#[inline]
fn socket_to_raw(socket: &Socket) -> usize {
    use std::os::unix::io::AsRawFd;
    socket.as_raw_fd() as usize
}

/// No-op without the library.
pub(crate) fn extended_discovery_init(socket: &Socket) -> std::io::Result<()> {
    let Some(ee) = enterprise() else { return Ok(()) };
    // SAFETY: the handle is borrowed from a live socket the caller still owns;
    // the library documents that it does not close it.
    let ret = unsafe { (ee.entries.discovery_init)(socket_to_raw(socket)) };
    if ret != 0 {
        return Err(std::io::Error::other(format!(
            "enterprise discovery_init failed (code {ret})"
        )));
    }
    Ok(())
}

/// No-op without the library.
#[inline]
pub(crate) fn extended_discovery_send(
    socket: &Socket,
    port: u16,
    data: &[u8],
    domain_id: u32,
) -> std::io::Result<()> {
    let Some(ee) = enterprise() else { return Ok(()) };
    // SAFETY: the handle is borrowed from a live socket, and `data` is a live
    // slice of exactly `data.len()` bytes.
    let ret = unsafe {
        (ee.entries.discovery_send)(
            socket_to_raw(socket),
            port,
            data.as_ptr(),
            data.len(),
            domain_id,
        )
    };
    if ret != 0 {
        return Err(std::io::Error::other(format!(
            "enterprise discovery_send failed (code {ret})"
        )));
    }
    Ok(())
}

/// Returns `default_period` unchanged without the library.
#[inline]
pub(crate) fn heartbeat_period(default_period: f64) -> f64 {
    match enterprise() {
        // SAFETY: plain f64 in, f64 out; no pointers cross the boundary.
        Some(ee) => unsafe { (ee.entries.heartbeat_period)(default_period) },
        None => default_period,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `INT2DDS_ENTERPRISE_PATH` may well be exported in the shell `cargo test`
    /// runs from. Clear it before anything touches the loader, which caches its
    /// answer for the whole process.
    fn open_core_env() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| unsafe { std::env::remove_var("INT2DDS_ENTERPRISE_PATH") });
    }

    #[test]
    fn absent_library_allows_participant_creation() {
        open_core_env();
        assert!(participant_gate().is_ok());
    }

    #[test]
    fn absent_library_resolves_no_ip() {
        open_core_env();
        assert_eq!(resolve_ip(), None);
    }

    #[test]
    fn absent_library_keeps_default_heartbeat() {
        open_core_env();
        assert_eq!(heartbeat_period(2.0), 2.0);
    }

    #[test]
    fn absent_library_discovery_is_noop() {
        open_core_env();
        use socket2::{Domain, Protocol, Type};
        let s = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)).unwrap();
        assert!(extended_discovery_init(&s).is_ok());
        assert!(extended_discovery_send(&s, 7400, b"x", 0).is_ok());
    }

    #[test]
    fn absent_library_reports_not_loaded() {
        open_core_env();
        assert!(!is_loaded());
    }

    #[test]
    fn an_unset_path_searches_only_next_to_the_executable() {
        open_core_env();
        let (paths, explicit) = candidate_paths();
        assert!(!explicit);
        assert!(
            paths.iter().all(|p| p.parent().is_some_and(|d| !d.as_os_str().is_empty())),
            "every candidate must be a complete path: {paths:?}"
        );
    }
}
