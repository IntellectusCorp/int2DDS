//! Unix Shared Memory Implementation
//!
//! Uses POSIX shm_open/mmap APIs for shared memory.

use std::ffi::CString;
use std::io;

#[cfg(unix)]
use libc::{
    close, ftruncate, mmap, munmap, shm_open, shm_unlink, MAP_FAILED, MAP_SHARED, O_CREAT, O_EXCL,
    O_RDWR, PROT_READ, PROT_WRITE,
};

/// Unix shared memory implementation
pub struct UnixSharedMemory {
    fd: i32,
    ptr: *mut u8,
    size: usize,
    name: String,
    is_creator: bool,
}

// Safety: The shared memory fd and pointer are valid across threads
unsafe impl Send for UnixSharedMemory {}
unsafe impl Sync for UnixSharedMemory {}

impl UnixSharedMemory {
    /// Create or open a shared memory segment
    pub fn new(name: &str, size: usize, create: bool) -> io::Result<Self> {
        let shm_name = format!("/{}", name);
        let name_cstr = CString::new(shm_name.clone())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

        let (fd, is_creator) = if create {
            Self::create_shm(&name_cstr, size)?
        } else {
            (Self::open_shm(&name_cstr)?, false)
        };

        let ptr = Self::map_memory(fd, size)?;

        Ok(Self { fd, ptr, size, name: shm_name, is_creator })
    }

    #[cfg(unix)]
    fn create_shm(name: &CString, size: usize) -> io::Result<(i32, bool)> {
        // Try to create exclusively first
        let fd = unsafe { shm_open(name.as_ptr(), O_CREAT | O_EXCL | O_RDWR, 0o666) };

        if fd >= 0 {
            // We created it, set the size
            // `off_t` is i64 on 64-bit targets but i32 on 32-bit (e.g. armhf),
            // so cast to off_t rather than a fixed i64 to stay portable.
            let ret = unsafe { ftruncate(fd, size as libc::off_t) };
            if ret < 0 {
                unsafe { close(fd) };
                return Err(io::Error::last_os_error());
            }
            return Ok((fd, true));
        }

        // Already exists, open it
        let fd = unsafe { shm_open(name.as_ptr(), O_RDWR, 0o666) };

        if fd < 0 {
            return Err(io::Error::last_os_error());
        }

        Ok((fd, false))
    }

    #[cfg(unix)]
    fn open_shm(name: &CString) -> io::Result<i32> {
        let fd = unsafe { shm_open(name.as_ptr(), O_RDWR, 0o666) };

        if fd < 0 {
            return Err(io::Error::last_os_error());
        }

        Ok(fd)
    }

    #[cfg(unix)]
    fn map_memory(fd: i32, size: usize) -> io::Result<*mut u8> {
        let ptr =
            unsafe { mmap(std::ptr::null_mut(), size, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0) };

        if ptr == MAP_FAILED {
            return Err(io::Error::last_os_error());
        }

        Ok(ptr as *mut u8)
    }

    #[cfg(not(unix))]
    fn create_shm(_name: &CString, _size: usize) -> io::Result<(i32, bool)> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "Unix only"))
    }

    #[cfg(not(unix))]
    fn open_shm(_name: &CString) -> io::Result<i32> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "Unix only"))
    }

    #[cfg(not(unix))]
    fn map_memory(_fd: i32, _size: usize) -> io::Result<*mut u8> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "Unix only"))
    }

    /// Get a raw pointer to the shared memory
    pub fn as_ptr(&self) -> *mut u8 {
        self.ptr
    }

    /// Get the size of the shared memory segment
    pub fn size(&self) -> usize {
        self.size
    }

    /// Check if this process created the shared memory
    pub fn is_creator(&self) -> bool {
        self.is_creator
    }

    /// Give up unlinking on drop. The registry outlives any one participant;
    /// only `unlink_registry` removes it.
    pub fn disown_creation(&mut self) {
        self.is_creator = false;
    }
}

impl Drop for UnixSharedMemory {
    fn drop(&mut self) {
        #[cfg(unix)]
        unsafe {
            if !self.ptr.is_null() {
                munmap(self.ptr as *mut _, self.size);
            }
            if self.fd >= 0 {
                close(self.fd);
            }
            // Only unlink if we're the creator
            if self.is_creator {
                let name_cstr = CString::new(self.name.clone()).ok();
                if let Some(name) = name_cstr {
                    shm_unlink(name.as_ptr());
                }
            }
        }
    }
}
