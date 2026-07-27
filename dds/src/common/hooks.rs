//! Static enterprise hook seams.
//!
//! The open core exposes a fixed set of named C-ABI seams. With the
//! `enterprise-hooks` feature off (default) every `call_*` below runs the
//! core's own fallback with zero overhead. With it on, the enterprise build
//! registers a hook per seam through the `__int2dds_hook_set_*` symbols and
//! `call_*` dispatches to it.

use socket2::Socket;

use crate::dcps::core::error::DdsResult;

#[cfg(feature = "enterprise-hooks")]
pub type ParticipantGateFn = extern "C" fn() -> i32;
#[cfg(feature = "enterprise-hooks")]
pub type ResolveIpFn = extern "C" fn(*mut std::os::raw::c_char, usize) -> i32;
#[cfg(feature = "enterprise-hooks")]
pub type DiscoveryInitFn = extern "C" fn(usize) -> i32;
#[cfg(feature = "enterprise-hooks")]
pub type DiscoverySendFn = extern "C" fn(usize, u16, *const u8, usize, u32) -> i32;
#[cfg(feature = "enterprise-hooks")]
pub type HeartbeatPeriodFn = extern "C" fn(f64) -> f64;

#[cfg(feature = "enterprise-hooks")]
mod slots {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    pub(super) static PARTICIPANT_GATE: AtomicUsize = AtomicUsize::new(0);
    pub(super) static RESOLVE_IP: AtomicUsize = AtomicUsize::new(0);
    pub(super) static DISCOVERY_INIT: AtomicUsize = AtomicUsize::new(0);
    pub(super) static DISCOVERY_SEND: AtomicUsize = AtomicUsize::new(0);
    pub(super) static HEARTBEAT_PERIOD: AtomicUsize = AtomicUsize::new(0);

    #[no_mangle]
    pub extern "C" fn __int2dds_hook_set_participant_gate(f: ParticipantGateFn) {
        PARTICIPANT_GATE.store(f as usize, std::sync::atomic::Ordering::SeqCst);
    }
    #[no_mangle]
    pub extern "C" fn __int2dds_hook_set_resolve_ip(f: ResolveIpFn) {
        RESOLVE_IP.store(f as usize, std::sync::atomic::Ordering::SeqCst);
    }
    #[no_mangle]
    pub extern "C" fn __int2dds_hook_set_discovery_init(f: DiscoveryInitFn) {
        DISCOVERY_INIT.store(f as usize, std::sync::atomic::Ordering::SeqCst);
    }
    #[no_mangle]
    pub extern "C" fn __int2dds_hook_set_discovery_send(f: DiscoverySendFn) {
        DISCOVERY_SEND.store(f as usize, std::sync::atomic::Ordering::SeqCst);
    }
    #[no_mangle]
    pub extern "C" fn __int2dds_hook_set_heartbeat_period(f: HeartbeatPeriodFn) {
        HEARTBEAT_PERIOD.store(f as usize, std::sync::atomic::Ordering::SeqCst);
    }
}

#[cfg(all(feature = "enterprise-hooks", windows))]
#[inline]
fn socket_to_raw(socket: &Socket) -> usize {
    use std::os::windows::io::AsRawSocket;
    socket.as_raw_socket() as usize
}

#[cfg(all(feature = "enterprise-hooks", unix))]
#[inline]
fn socket_to_raw(socket: &Socket) -> usize {
    use std::os::unix::io::AsRawFd;
    socket.as_raw_fd() as usize
}

/// Seam 1: participant-creation gate. Fail-closed under the feature (feature on
/// but nothing registered => deny), matching the previous `factory-hook`.
#[inline]
pub(crate) fn call_participant_gate() -> DdsResult<()> {
    #[cfg(feature = "enterprise-hooks")]
    {
        use std::sync::atomic::Ordering;
        let v = slots::PARTICIPANT_GATE.load(Ordering::SeqCst);
        let code = if v == 0 {
            1
        } else {
            let f: ParticipantGateFn = unsafe { std::mem::transmute(v) };
            f()
        };
        return match code {
            0 => Ok(()),
            code => Err(crate::dcps::core::error::DdsError::Error(format!(
                "participant creation refused (code {code})"
            ))),
        };
    }
    #[cfg(not(feature = "enterprise-hooks"))]
    {
        Ok(())
    }
}

/// Seam 2: resolve the local working IP. `None` => core uses its own default.
#[inline]
pub(crate) fn call_resolve_ip() -> Option<String> {
    #[cfg(feature = "enterprise-hooks")]
    {
        use std::sync::atomic::Ordering;
        let v = slots::RESOLVE_IP.load(Ordering::SeqCst);
        if v != 0 {
            let f: ResolveIpFn = unsafe { std::mem::transmute(v) };
            let mut buffer = [0 as std::os::raw::c_char; 64];
            let ret = f(buffer.as_mut_ptr(), buffer.len());
            if ret == 0 {
                let cstr = unsafe { std::ffi::CStr::from_ptr(buffer.as_ptr()) };
                return Some(cstr.to_string_lossy().into_owned());
            }
        }
    }
    None
}

/// Seam 3: initialise extended discovery on a freshly created socket.
#[inline]
#[cfg_attr(not(feature = "enterprise-hooks"), allow(unused_variables))]
pub(crate) fn call_discovery_init(socket: &Socket) -> std::io::Result<()> {
    #[cfg(feature = "enterprise-hooks")]
    {
        use std::sync::atomic::Ordering;
        let v = slots::DISCOVERY_INIT.load(Ordering::SeqCst);
        if v != 0 {
            let f: DiscoveryInitFn = unsafe { std::mem::transmute(v) };
            let ret = f(socket_to_raw(socket));
            if ret != 0 {
                return Err(std::io::Error::other(format!(
                    "discovery_init hook failed (code {ret})"
                )));
            }
        }
    }
    Ok(())
}

/// Seam 4: extended-discovery multicast send (hot path).
#[inline]
#[cfg_attr(not(feature = "enterprise-hooks"), allow(unused_variables))]
pub(crate) fn call_discovery_send(
    socket: &Socket,
    port: u16,
    data: &[u8],
    domain_id: u32,
) -> std::io::Result<()> {
    #[cfg(feature = "enterprise-hooks")]
    {
        use std::sync::atomic::Ordering;
        let v = slots::DISCOVERY_SEND.load(Ordering::SeqCst);
        if v != 0 {
            let f: DiscoverySendFn = unsafe { std::mem::transmute(v) };
            let ret = f(socket_to_raw(socket), port, data.as_ptr(), data.len(), domain_id);
            if ret != 0 {
                return Err(std::io::Error::other(format!(
                    "discovery_send hook failed (code {ret})"
                )));
            }
        }
    }
    Ok(())
}

/// Seam 5: adjust the builtin-endpoint heartbeat period.
#[inline]
pub(crate) fn call_heartbeat_period(default_period: f64) -> f64 {
    #[cfg(feature = "enterprise-hooks")]
    {
        use std::sync::atomic::Ordering;
        let v = slots::HEARTBEAT_PERIOD.load(Ordering::SeqCst);
        if v != 0 {
            let f: HeartbeatPeriodFn = unsafe { std::mem::transmute(v) };
            return f(default_period);
        }
    }
    default_period
}

#[cfg(all(test, feature = "enterprise-hooks"))]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;

    // participant_gate is the only seam with two tests; serialize them.
    static GATE: Mutex<()> = Mutex::new(());

    extern "C" fn allow() -> i32 {
        0
    }
    extern "C" fn deny() -> i32 {
        5
    }

    #[test]
    fn participant_gate_allow_then_deny() {
        let _g = GATE.lock().unwrap();
        slots::__int2dds_hook_set_participant_gate(allow);
        assert!(call_participant_gate().is_ok());
        slots::__int2dds_hook_set_participant_gate(deny);
        match call_participant_gate().unwrap_err() {
            crate::dcps::core::error::DdsError::Error(m) => assert!(m.contains("code 5")),
            other => panic!("unexpected: {other:?}"),
        }
    }

    extern "C" fn write_ip(buffer: *mut std::os::raw::c_char, len: usize) -> i32 {
        let ip = b"10.0.0.7";
        if ip.len() + 1 > len {
            return 101;
        }
        unsafe {
            std::ptr::copy_nonoverlapping(ip.as_ptr(), buffer as *mut u8, ip.len());
            *buffer.add(ip.len()) = 0;
        }
        0
    }

    #[test]
    fn resolve_ip_dispatches() {
        slots::__int2dds_hook_set_resolve_ip(write_ip);
        assert_eq!(call_resolve_ip(), Some("10.0.0.7".to_string()));
    }

    static INIT_CALLED: AtomicBool = AtomicBool::new(false);
    extern "C" fn mark_init(_s: usize) -> i32 {
        INIT_CALLED.store(true, Ordering::SeqCst);
        0
    }

    #[test]
    fn discovery_init_dispatches() {
        use socket2::{Domain, Protocol, Type};
        slots::__int2dds_hook_set_discovery_init(mark_init);
        let s = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)).unwrap();
        assert!(call_discovery_init(&s).is_ok());
        assert!(INIT_CALLED.load(Ordering::SeqCst));
    }

    extern "C" fn double_period(p: f64) -> f64 {
        p * 2.0
    }

    #[test]
    fn heartbeat_period_dispatches() {
        slots::__int2dds_hook_set_heartbeat_period(double_period);
        assert_eq!(call_heartbeat_period(1.5), 3.0);
    }
}
