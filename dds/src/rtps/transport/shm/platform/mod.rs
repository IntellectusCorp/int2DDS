//! Platform-specific Shared Memory Implementation
//!
//! This module provides platform-specific shared memory implementations
//! for Windows and Unix-like systems.

#[cfg(windows)]
pub mod windows;

#[cfg(unix)]
pub mod unix;

use std::io;

/// Platform-independent shared memory segment handle
pub struct SharedMemory {
    #[cfg(windows)]
    inner: windows::WindowsSharedMemory,

    #[cfg(unix)]
    inner: unix::UnixSharedMemory,
}

impl std::fmt::Debug for SharedMemory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedMemory")
            .field("size", &self.size())
            .field("is_creator", &self.is_creator())
            .finish()
    }
}

impl SharedMemory {
    /// Create a new shared memory segment
    ///
    /// # Arguments
    /// * `name` - Name of the shared memory segment
    /// * `size` - Size in bytes
    /// * `create` - If true, create new segment; if false, open existing
    pub fn new(name: &str, size: usize, create: bool) -> io::Result<Self> {
        #[cfg(windows)]
        {
            Ok(Self { inner: windows::WindowsSharedMemory::new(name, size, create)? })
        }

        #[cfg(unix)]
        {
            Ok(Self { inner: unix::UnixSharedMemory::new(name, size, create)? })
        }

        #[cfg(not(any(windows, unix)))]
        {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Shared memory not supported on this platform",
            ))
        }
    }

    /// Get a raw pointer to the shared memory
    pub fn as_ptr(&self) -> *mut u8 {
        self.inner.as_ptr()
    }

    /// Get the size of the shared memory segment
    pub fn size(&self) -> usize {
        self.inner.size()
    }

    /// Check if this process created the shared memory
    pub fn is_creator(&self) -> bool {
        self.inner.is_creator()
    }

    /// Give up unlinking on drop. See the platform implementations.
    pub fn disown_creation(&mut self) {
        self.inner.disown_creation();
    }
}

impl Drop for SharedMemory {
    fn drop(&mut self) {
        // Platform-specific cleanup is handled by inner Drop
    }
}

/// Generate a shared memory segment name for a given domain
pub fn shm_segment_name(domain_id: u32) -> String {
    format!("int2dds_shm_d{}", domain_id)
}

/// Whether a process with this pid currently exists.
///
/// "Permission denied" means the process exists but is not ours, and counts as
/// alive: reporting it dead would let a sweep evict a live participant's slot.
pub(crate) fn process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // Safety: signal 0 performs the existence and permission check only.
        if unsafe { libc::kill(pid as libc::pid_t, 0) } == 0 {
            return true;
        }
        std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }
    #[cfg(windows)]
    {
        use winapi::shared::minwindef::DWORD;
        use winapi::shared::winerror::ERROR_ACCESS_DENIED;
        use winapi::um::errhandlingapi::GetLastError;
        use winapi::um::handleapi::CloseHandle;
        use winapi::um::processthreadsapi::{GetExitCodeProcess, OpenProcess};
        use winapi::um::winnt::PROCESS_QUERY_LIMITED_INFORMATION;

        // Stable Windows constant. It lives in winapi's `minwinbase`, a feature
        // this crate does not enable.
        const STILL_ACTIVE: DWORD = 259;

        // Safety: FFI calls; the handle is closed on every path that opens one.
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                return GetLastError() == ERROR_ACCESS_DENIED;
            }
            // Opening the process is not enough: a terminated process keeps its
            // kernel object while any handle to it exists, so `OpenProcess`
            // still succeeds for one. Ask for the exit code instead.
            let mut code: DWORD = 0;
            let queried = GetExitCodeProcess(handle, &mut code) != 0;
            CloseHandle(handle);
            // A failed query means we cannot tell; report alive so a sweep never
            // evicts a participant we were merely unable to inspect.
            !queried || code == STILL_ACTIVE
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        false
    }
}
