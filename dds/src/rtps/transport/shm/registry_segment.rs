//! The domain registry on real shared memory. The first participant in a
//! domain creates and initializes it; the rest wait for its magic gate.

use std::io;
use std::time::{Duration, Instant};

use crate::rtps::transport::shm::layout::LayoutError;
use crate::rtps::transport::shm::platform::SharedMemory;
use crate::rtps::transport::shm::registry::Registry;
use crate::rtps::transport::shm::segment::registry_name;

pub(crate) struct RegistrySegment {
    registry: Registry,
    // Declared last: the mapping must outlive everything pointing into it.
    _shm: SharedMemory,
}

impl RegistrySegment {
    /// Two failures leave a stale object behind, and neither clears itself:
    /// a timeout, where the creator may have died before publishing its magic,
    /// and a layout mismatch, where the object predates a build whose
    /// `Registry::size()` differs. Nothing unlinks the registry on its own --
    /// the creator disowns it so that it outlives any one participant -- so
    /// the caller may `unlink_registry(domain)` and retry. `open` does not do
    /// that itself: a creator that has not published yet may simply be slow.
    pub(crate) fn open(domain: u32) -> io::Result<RegistrySegment> {
        let name = registry_name(domain);
        let size = Registry::size() as usize;
        let mut shm = SharedMemory::new(&name, size, true)?;

        if shm.is_creator() {
            // Safety: we created the object, so nobody else has initialized it,
            // and it is zeroed and at least `size` bytes.
            let registry = unsafe { Registry::init(shm.as_ptr()) };
            // The registry outlives this participant; only `unlink_registry`
            // removes it.
            shm.disown_creation();
            return Ok(RegistrySegment { registry, _shm: shm });
        }

        // Someone else created it and may still be initializing.
        let deadline = Instant::now() + Duration::from_millis(100);
        loop {
            // Safety: `shm` mapped `size` bytes at this address. Whether the
            // object is really that long is what `attach` decides by comparing
            // `reg_size`; mapping past the end of a POSIX object succeeds.
            match unsafe { Registry::attach(shm.as_ptr(), size as u64) } {
                Ok(registry) => return Ok(RegistrySegment { registry, _shm: shm }),
                Err(LayoutError::NotReady) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(LayoutError::NotReady) => {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "registry never became ready",
                    ))
                }
                Err(_) => return Err(io::Error::other("registry layout mismatch")),
            }
        }
    }

    pub(crate) fn registry(&self) -> &Registry {
        &self.registry
    }
}

/// Remove the domain registry object. Unix only; on Windows the mapping
/// disappears with its last handle.
pub(crate) fn unlink_registry(domain: u32) {
    #[cfg(unix)]
    {
        use std::ffi::CString;
        if let Ok(name) = CString::new(format!("/{}", registry_name(domain))) {
            // Safety: FFI call with a NUL-terminated name; a missing object is
            // reported as an error we ignore.
            unsafe { libc::shm_unlink(name.as_ptr()) };
        }
    }
    #[cfg(not(unix))]
    {
        let _ = domain;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtps::transport::shm::registry::MAX_PARTICIPANTS;

    const DOMAIN: u32 = 241;

    #[test]
    fn open_twice_shares_one_registry() {
        unlink_registry(DOMAIN);
        let a = RegistrySegment::open(DOMAIN).unwrap();
        let b = RegistrySegment::open(DOMAIN).unwrap();
        let (slot, epoch) = a.registry().claim(std::process::id(), [3; 12], 0).unwrap();
        assert_eq!(b.registry().find_active(&[3; 12]), Some((slot, 1)));
        a.registry().release(slot, epoch);
        drop(b);
        drop(a);
        unlink_registry(DOMAIN);
    }

    #[test]
    fn a_fresh_registry_has_every_slot_free() {
        unlink_registry(DOMAIN + 1);
        let r = RegistrySegment::open(DOMAIN + 1).unwrap();
        assert!(r.registry().find_active(&[0; 12]).is_none());
        for i in 0..MAX_PARTICIPANTS {
            assert!(r.registry().claim(std::process::id(), [i as u8; 12], 0).is_some());
        }
        assert!(r.registry().claim(std::process::id(), [255; 12], 0).is_none());
        drop(r);
        unlink_registry(DOMAIN + 1);
    }

    #[test]
    fn attaching_waits_for_the_creator_to_publish() {
        const D: u32 = 9243;
        unlink_registry(D);
        // Create the object but leave it uninitialized: `open` must find it
        // present, see no magic, and wait rather than fail.
        let raw = SharedMemory::new(&registry_name(D), Registry::size() as usize, true).unwrap();
        assert!(raw.is_creator());
        let ptr = raw.as_ptr() as usize;

        let publisher = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            // Safety: the region was sized to `Registry::size()` above and no
            // other thread has published it yet.
            unsafe { Registry::init(ptr as *mut u8) };
        });

        let seg = RegistrySegment::open(D).expect("should wait for the creator");
        publisher.join().unwrap();
        assert!(seg.registry().find_active(&[0; 12]).is_none());
        drop(seg);
        drop(raw);
        unlink_registry(D);
    }

    #[test]
    fn a_registry_outlives_the_participant_that_created_it() {
        const D: u32 = 9244;
        unlink_registry(D);
        let first = RegistrySegment::open(D).unwrap();
        let (slot, _) = first.registry().claim(std::process::id(), [5; 12], 1).unwrap();
        let second = RegistrySegment::open(D).unwrap();
        // The creator leaving must not take the registry with it.
        drop(first);
        assert_eq!(second.registry().find_active(&[5; 12]).map(|(s, _)| s), Some(slot));
        let third = RegistrySegment::open(D).unwrap();
        assert_eq!(third.registry().find_active(&[5; 12]).map(|(s, _)| s), Some(slot));
        drop(second);
        drop(third);
        unlink_registry(D);
    }
}
