use std::time::{Duration, Instant};

use log::debug;

// Once this much time has elapsed since the last accepted count, a non-
// increasing count is accepted as a legitimate peer reset (e.g. asymmetric
// lease expiry + rematch) instead of being rejected as stale.
pub(crate) const COUNT_RESET_ACCEPT_AFTER: Duration = Duration::from_millis(200);

// Returns true when `new_count` should be accepted. The caller writes the new
// count and timestamp back into its proxy fields only when this returns true.
pub(crate) fn should_accept_count(
    label: &str,
    new_count: u32,
    prev_count: Option<u32>,
    last_accepted_at: Option<Instant>,
    is_preemptive: bool,
    now: Instant,
) -> bool {
    // Preemptive ACKNACK is an explicit reset signal from the peer.
    if is_preemptive {
        return true;
    }

    // No prior count means this is the first message seen from the peer.
    let Some(prev) = prev_count else {
        debug!("{} First message received: count={}", label, new_count);
        return true;
    };

    // Normal monotonic increase; wrapping_sub handles u32 wrap-around.
    if (new_count.wrapping_sub(prev) as i32) > 0 {
        return true;
    }

    // Non-increasing but old enough that the peer is assumed to have reset.
    match last_accepted_at {
        Some(t) => {
            if now.duration_since(t) >= COUNT_RESET_ACCEPT_AFTER {
                debug!(
                    "{} Accepting non-increasing count as reset: count={} <= last_count={}",
                    label, new_count, prev
                );
                true
            } else {
                debug!(
                    "{} Ignoring old message: count={} <= last_count={}",
                    label, new_count, prev
                );
                false
            }
        }
        // Unreachable: callers always set count and timestamp as a pair.
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const L: &str = "[test]";

    #[test]
    fn first_count_accepted() {
        let now = Instant::now();
        assert!(should_accept_count(L, 1, None, None, false, now));
    }

    #[test]
    fn monotonic_increase_accepted() {
        let now = Instant::now();
        assert!(should_accept_count(L, 5, Some(4), Some(now), false, now));
    }

    #[test]
    fn recent_non_increasing_rejected() {
        let now = Instant::now();
        assert!(!should_accept_count(L, 4, Some(4), Some(now), false, now));
        assert!(!should_accept_count(L, 3, Some(4), Some(now), false, now));
    }

    #[test]
    fn non_increasing_accepted_after_reset_delay() {
        let now = Instant::now();
        let earlier = now - (COUNT_RESET_ACCEPT_AFTER + Duration::from_millis(1));
        assert!(should_accept_count(L, 1, Some(99), Some(earlier), false, now));
    }

    #[test]
    fn non_increasing_rejected_just_before_reset_delay() {
        let now = Instant::now();
        let earlier = now - (COUNT_RESET_ACCEPT_AFTER - Duration::from_millis(1));
        assert!(!should_accept_count(L, 1, Some(99), Some(earlier), false, now));
    }

    #[test]
    fn preemptive_bypasses_filter() {
        let now = Instant::now();
        assert!(should_accept_count(L, 0, Some(99), Some(now), true, now));
    }

    #[test]
    fn wraparound_accepted_as_increase() {
        let now = Instant::now();
        // u32::MAX -> 0 wraps to +1, must be accepted as strict increase.
        assert!(should_accept_count(L, 0, Some(u32::MAX), Some(now), false, now));
    }
}
