//! Calling user code from a middleware thread.
//!
//! DDS listeners are invoked synchronously on the RTPS receive and discovery threads. That
//! makes two ordinary user mistakes fatal to the middleware unless the call site is careful:
//!
//! * A `panic!` -- or any `unwrap` on an empty value -- unwinds out of the thread's closure and
//!   the thread simply ends. Delivery stops for that participant and the application is never
//!   told; the only sign is a stack trace on stderr naming a thread it did not create.
//! * Holding the callback's own mutex across the call makes the listener re-entrant-unsafe:
//!   `std::sync::Mutex` is not reentrant, so a listener that reaches an API touching the same
//!   slot hangs, and an unwind through the call leaves the mutex poisoned for good.
//!
//! [`callback_handle`] answers the second by lifting the callback out before the call, and
//! [`notify_user`] answers the first. `ffi/src/listener.rs` already draws this boundary for C
//! callbacks; these are the same boundary for Rust ones.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Mutex};

/// Clones a registered callback out from behind its mutex, so the caller can invoke it with
/// the lock released.
///
/// Returns `None` when no callback is registered.
///
/// Poisoning is recovered rather than propagated. The slot holds a single
/// `Option<Arc<_>>` that an unwind cannot leave half-written, and the alternative -- the
/// `Err(e) => log` arm this replaces -- silently disabled every future notification on that
/// entity for the rest of the process.
pub(crate) fn callback_handle<T: ?Sized>(slot: &Mutex<Option<Arc<T>>>) -> Option<Arc<T>> {
    slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone()
}

/// Runs a user listener without letting it take the calling middleware thread down.
///
/// The panic is contained, not hidden: it is logged with its payload, which is strictly more
/// visible than the current behaviour of killing a background thread the application cannot
/// observe. The callback's own work is still lost -- only the thread is saved.
pub(crate) fn notify_user<F: FnOnce()>(context: &str, notify: F) {
    if let Err(payload) = catch_unwind(AssertUnwindSafe(notify)) {
        let reason = panic_message(&payload);
        log::error!(
            "{context}: user listener panicked ({reason}). The panic was contained so the DDS \
             thread survives, but that callback did not complete."
        );
    }
}

/// Best-effort text for a caught panic payload, which is `&str` or `String` for `panic!`.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> &str {
    if let Some(message) = payload.downcast_ref::<&'static str>() {
        message
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.as_str()
    } else {
        "non-string panic payload"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notify_user_contains_a_panicking_callback() {
        // The bare closure would abort the calling thread's stack; the point of the boundary is
        // that control returns here instead.
        notify_user("test", || panic!("boom"));
    }

    #[test]
    fn notify_user_runs_the_callback() {
        let mut ran = false;
        notify_user("test", || ran = true);
        assert!(ran, "the callback did not run");
    }

    #[test]
    fn callback_handle_recovers_a_poisoned_slot() {
        let slot: Arc<Mutex<Option<Arc<u32>>>> = Arc::new(Mutex::new(Some(Arc::new(7))));

        let victim = Arc::clone(&slot);
        let _ = std::thread::spawn(move || {
            let _guard = victim.lock().unwrap();
            panic!("poison the slot");
        })
        .join();
        assert!(slot.is_poisoned(), "the slot under test was not actually poisoned");

        assert_eq!(
            callback_handle(&slot).as_deref(),
            Some(&7),
            "a poisoned slot must still yield its callback, or notifications stop forever"
        );
    }

    #[test]
    fn callback_handle_reports_an_empty_slot() {
        let slot: Mutex<Option<Arc<u32>>> = Mutex::new(None);
        assert!(callback_handle(&slot).is_none());
    }
}
