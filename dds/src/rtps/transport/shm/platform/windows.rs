//! Windows Shared Memory Implementation
//!
//! Uses Windows CreateFileMapping/MapViewOfFile APIs for shared memory.

use std::ffi::CString;
use std::io;
use std::ptr;

#[cfg(windows)]
use winapi::shared::minwindef::DWORD;
#[cfg(windows)]
use winapi::shared::winerror::ERROR_ALREADY_EXISTS;
#[cfg(windows)]
use winapi::um::errhandlingapi::GetLastError;
#[cfg(windows)]
use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
#[cfg(windows)]
use winapi::um::memoryapi::{MapViewOfFile, UnmapViewOfFile, FILE_MAP_ALL_ACCESS};
#[cfg(windows)]
use winapi::um::winbase::{CreateFileMappingA, OpenFileMappingA};
#[cfg(windows)]
use winapi::um::winnt::PAGE_READWRITE;
#[cfg(windows)]
#[allow(clippy::upper_case_acronyms)]
type HANDLE = winapi::shared::ntdef::HANDLE;

#[cfg(not(windows))]
#[allow(clippy::upper_case_acronyms)]
type HANDLE = *mut std::ffi::c_void;

/// Windows shared memory implementation
pub struct WindowsSharedMemory {
    handle: HANDLE,
    ptr: *mut u8,
    size: usize,
    is_creator: bool,
}

// Safety: The shared memory handle and pointer are valid across threads
unsafe impl Send for WindowsSharedMemory {}
unsafe impl Sync for WindowsSharedMemory {}

impl WindowsSharedMemory {
    /// Create or open a shared memory segment
    pub fn new(name: &str, size: usize, create: bool) -> io::Result<Self> {
        let name_cstr = CString::new(format!("Local\\{}", name))
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

        let (handle, is_creator) = if create {
            Self::create_mapping(&name_cstr, size)?
        } else {
            (Self::open_mapping(&name_cstr)?, false)
        };

        let ptr = Self::map_view(handle, size)?;

        Ok(Self { handle, ptr, size, is_creator })
    }

    #[cfg(windows)]
    fn create_mapping(name: &CString, size: usize) -> io::Result<(HANDLE, bool)> {
        let size_high: DWORD = ((size as u64) >> 32) as DWORD;
        let size_low: DWORD = size as DWORD;

        let handle = unsafe {
            CreateFileMappingA(
                INVALID_HANDLE_VALUE,
                ptr::null_mut(),
                PAGE_READWRITE,
                size_high,
                size_low,
                name.as_ptr(),
            )
        };

        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }

        // Check if we created it or opened existing
        let last_error = unsafe { GetLastError() };
        let is_creator = last_error != ERROR_ALREADY_EXISTS;

        Ok((handle, is_creator))
    }

    #[cfg(windows)]
    fn open_mapping(name: &CString) -> io::Result<HANDLE> {
        let handle = unsafe { OpenFileMappingA(FILE_MAP_ALL_ACCESS, 0, name.as_ptr()) };

        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }

        Ok(handle)
    }

    #[cfg(windows)]
    fn map_view(handle: HANDLE, size: usize) -> io::Result<*mut u8> {
        let ptr = unsafe { MapViewOfFile(handle, FILE_MAP_ALL_ACCESS, 0, 0, size) };

        if ptr.is_null() {
            return Err(io::Error::last_os_error());
        }

        Ok(ptr as *mut u8)
    }

    #[cfg(not(windows))]
    fn create_mapping(_name: &CString, _size: usize) -> io::Result<(HANDLE, bool)> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "Windows only"))
    }

    #[cfg(not(windows))]
    fn open_mapping(_name: &CString) -> io::Result<HANDLE> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "Windows only"))
    }

    #[cfg(not(windows))]
    fn map_view(_handle: HANDLE, _size: usize) -> io::Result<*mut u8> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "Windows only"))
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

    /// Give up unlinking on drop. See the platform implementations.
    pub fn disown_creation(&mut self) {
        self.is_creator = false;
    }
}

impl Drop for WindowsSharedMemory {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            if !self.ptr.is_null() {
                UnmapViewOfFile(self.ptr as *mut _);
            }
            if !self.handle.is_null() {
                CloseHandle(self.handle);
            }
        }
    }
}
