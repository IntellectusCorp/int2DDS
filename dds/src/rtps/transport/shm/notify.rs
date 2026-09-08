//! Cross-process wakeup for a ring's consumer.
//!
//! Windows uses a named auto-reset event; Linux uses a futex on a word inside
//! the shared segment. Other platforms report unsupported and callers fall back.

use std::io;
use std::sync::atomic::AtomicU32;
#[cfg(target_os = "linux")]
use std::sync::atomic::Ordering;
use std::time::Duration;

pub(crate) fn notify_supported() -> bool {
    cfg!(any(windows, target_os = "linux"))
}

pub(crate) struct Notifier {
    #[cfg(windows)]
    handle: winapi::shared::ntdef::HANDLE,
    // Unused where neither backend applies; kept so the struct shape is uniform.
    #[cfg_attr(not(any(windows, target_os = "linux")), allow(dead_code))]
    futex: *const AtomicU32,
}

// Safety: `handle` is a Windows kernel event HANDLE. Kernel objects are
// reference-counted by the OS and the WinAPI calls used here (SetEvent,
// WaitForSingleObject, CloseHandle) are documented as safe to call
// concurrently from multiple threads on the same handle. `futex` is a raw
// pointer into a shared-memory segment that outlives the Notifier and is
// only ever touched through atomic operations, so sharing it across threads
// does not race.
unsafe impl Send for Notifier {}
unsafe impl Sync for Notifier {}

impl Notifier {
    pub(crate) fn create(name: &str, futex: *const AtomicU32) -> io::Result<Notifier> {
        Self::make(name, futex)
    }

    pub(crate) fn open(name: &str, futex: *const AtomicU32) -> io::Result<Notifier> {
        Self::make(name, futex)
    }

    #[cfg(windows)]
    fn make(name: &str, futex: *const AtomicU32) -> io::Result<Notifier> {
        use std::ffi::CString;
        use winapi::um::synchapi::CreateEventA;
        let cname = CString::new(format!("Local\\{}", name))
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        // Safety: `cname` is a valid, NUL-terminated C string kept alive for the
        // duration of the call. CreateEventA both creates a new named event and,
        // when one of that name already exists, opens a handle to it -- which is
        // why `create` and `open` both route through this function. The other
        // arguments (no security attributes, manual-reset = FALSE, initial
        // state = FALSE) are plain values, not pointers that need upholding.
        let handle = unsafe {
            CreateEventA(std::ptr::null_mut(), 0 /* auto-reset */, 0, cname.as_ptr())
        };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Notifier { handle, futex })
    }

    #[cfg(all(unix, target_os = "linux"))]
    fn make(_name: &str, futex: *const AtomicU32) -> io::Result<Notifier> {
        Ok(Notifier { futex })
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    fn make(_name: &str, _futex: *const AtomicU32) -> io::Result<Notifier> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "notification unsupported"))
    }

    pub(crate) fn signal(&self) {
        #[cfg(windows)]
        // Safety: `self.handle` was created by `make` and is closed only in
        // `Drop`, which cannot run concurrently with this `&self` call, so the
        // handle is valid for the duration of this call.
        unsafe {
            winapi::um::synchapi::SetEvent(self.handle);
        }
        #[cfg(all(unix, target_os = "linux"))]
        // Safety: `self.futex` points at a `AtomicU32` inside a shared-memory
        // segment kept mapped for as long as this `Notifier` exists, so the
        // dereference is valid; the fetch_add uses the atomic itself, and the
        // raw `syscall` only reads that same address plus plain integer
        // arguments, matching the documented FUTEX_WAKE calling convention.
        unsafe {
            (*self.futex).fetch_add(1, Ordering::Release);
            libc::syscall(libc::SYS_futex, self.futex, libc::FUTEX_WAKE, 1i32, 0, 0, 0);
        }
        #[cfg(not(any(windows, target_os = "linux")))]
        {
            let _ = self.futex;
        }
    }

    /// Sample the wakeup state before testing the condition. Pass the result to
    /// `wait_if`: a signal that lands in between changes the word, and the
    /// kernel then refuses to sleep instead of losing the wakeup.
    pub(crate) fn prepare_wait(&self) -> u32 {
        #[cfg(all(unix, target_os = "linux"))]
        // Safety: `self.futex` is valid for the same reason as in `signal`.
        unsafe {
            (*self.futex).load(Ordering::Acquire)
        }
        #[cfg(not(all(unix, target_os = "linux")))]
        0
    }

    /// Sleep until signalled or `timeout` elapses. `expected` must come from a
    /// `prepare_wait` taken before the caller tested its condition.
    pub(crate) fn wait_if(&self, expected: u32, timeout: Duration) {
        #[cfg(windows)]
        // Safety: same handle-validity argument as in `signal`; the timeout is
        // a plain millisecond count, not a pointer. The event is auto-reset and
        // holds a signal that arrived before this call, so `expected` is not
        // needed on this backend.
        unsafe {
            let _ = expected;
            winapi::um::synchapi::WaitForSingleObject(self.handle, timeout.as_millis() as u32);
        }
        #[cfg(all(unix, target_os = "linux"))]
        // Safety: `self.futex` is valid for the same reason as in `signal`. The
        // kernel compares the word against `expected` atomically before
        // sleeping and returns immediately on a mismatch, so a signal that
        // landed any time after the caller's `prepare_wait` is not lost. `ts`
        // is a local kept alive for the call, so passing its address is sound.
        unsafe {
            let ts = libc::timespec {
                tv_sec: timeout.as_secs() as libc::time_t,
                tv_nsec: timeout.subsec_nanos() as libc::c_long,
            };
            libc::syscall(
                libc::SYS_futex,
                self.futex,
                libc::FUTEX_WAIT,
                expected as i32,
                &ts as *const libc::timespec,
                0,
                0,
            );
        }
        #[cfg(not(any(windows, target_os = "linux")))]
        {
            let _ = expected;
            std::thread::sleep(timeout);
        }
    }
}

#[cfg(windows)]
impl Drop for Notifier {
    fn drop(&mut self) {
        // Safety: `self.handle` was created by `make` and is not shared or
        // closed anywhere else, so this is the single closing call for it.
        unsafe {
            winapi::um::handleapi::CloseHandle(self.handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;
    use std::sync::Arc;

    #[test]
    fn signal_wakes_a_waiter() {
        if !notify_supported() {
            return;
        }
        let word = Arc::new(AtomicU32::new(0));
        let name = format!("int2dds_test_notify_{}", std::process::id());
        let producer = Notifier::create(&name, word.as_ref() as *const AtomicU32).unwrap();
        let consumer = Notifier::open(&name, word.as_ref() as *const AtomicU32).unwrap();

        let token = consumer.prepare_wait();
        let handle = std::thread::spawn(move || {
            let start = std::time::Instant::now();
            consumer.wait_if(token, Duration::from_secs(5));
            start.elapsed()
        });
        std::thread::sleep(Duration::from_millis(50));
        producer.signal();
        // Without this bound the test also passes when `signal` is a no-op:
        // `wait` simply returns on its own 5s timeout instead.
        let waited = handle.join().unwrap();
        assert!(waited < Duration::from_secs(1), "wait was not woken by signal: {waited:?}");
    }

    #[test]
    fn wait_returns_on_timeout_without_a_signal() {
        if !notify_supported() {
            return;
        }
        let word = AtomicU32::new(0);
        let name = format!("int2dds_test_timeout_{}", std::process::id());
        let n = Notifier::create(&name, &word as *const AtomicU32).unwrap();
        let token = n.prepare_wait();
        let start = std::time::Instant::now();
        n.wait_if(token, Duration::from_millis(50));
        assert!(start.elapsed() >= Duration::from_millis(40));
    }
}
