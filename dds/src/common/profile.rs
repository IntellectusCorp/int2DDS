//! Opt-in hot-path profiling, gated so a switched-off build pays almost nothing.
//!
//! The five instrumented functions (`DataWriterHistoryCache::add_change_with_cleanup`,
//! `WriterHistoryCache::add_change`, `UdpSender::send`,
//! `UserUnicastListeningTask::process_rtps_message` and
//! `DataReader::read_or_take_serialized_bytes`) each used to resolve
//! `RMW_INT2DDS_PROFILE` per call and then read the clock unconditionally, keeping the
//! results only if the lookup came back set.
//!
//! Both halves were real cost. `env::var_os` scans the whole environment on a miss --
//! 47.6 ns at 66 entries and about 0.33 ns per entry beyond that, so 70-90 ns under a
//! ROS 2 launch environment. `Instant::now()` lowers to `clock_gettime` through the vDSO,
//! which LLVM cannot prove side-effect-free, so the reads survive optimisation; the
//! profile-off path was verified in disassembly to emit two `bl Instant::now` before the
//! first branch on the flag. At 71.3 ns each on aarch64 that added up: a 1 KB best-effort
//! round trip paid 4 environment scans and 17 clock reads with profiling switched off,
//! and 5 and 30 on the serialized path the language bindings use.
//!
//! Nothing in the tree sets `RMW_INT2DDS_PROFILE` programmatically -- it is exported
//! before launch -- so latching it once per process changes no observable behaviour. The
//! switch stays a runtime one rather than a cargo feature on purpose: C#, Python and Java
//! consume a prebuilt `libint2dds_ffi.so` and cannot rebuild it to turn profiling on.

use std::sync::OnceLock;
use std::time::Instant;

static ENABLED: OnceLock<bool> = OnceLock::new();

/// Whether hot-path profiling is switched on for this process.
///
/// Latched on first call, so the environment scan happens once rather than per sample.
#[inline]
pub(crate) fn enabled() -> bool {
    *ENABLED.get_or_init(|| std::env::var_os("RMW_INT2DDS_PROFILE").is_some())
}

/// Reads the clock only when profiling is on.
///
/// The `Option` is what keeps the disabled path free: there is no `Instant` to produce,
/// so there is no `clock_gettime` to emit.
#[inline]
pub(crate) fn now_if(profile: bool) -> Option<Instant> {
    profile.then(Instant::now)
}

/// Microseconds elapsed since `start`, or 0 when profiling is off.
///
/// Mirrors what the hand-rolled `elapsed_us` helpers returned in their `else` arm, so the
/// accumulators keep reporting exactly what they reported before.
#[inline]
pub(crate) fn elapsed_us(start: Option<Instant>) -> u64 {
    start.map_or(0, |t0| Instant::now().duration_since(t0).as_micros() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole point of the gate: with profiling off there is no `Instant` to hand back,
    /// so no `clock_gettime` is emitted on the hot path.
    #[test]
    fn now_if_reads_the_clock_only_when_profiling_is_on() {
        assert!(now_if(false).is_none(), "profiling off must not produce an Instant");
        assert!(now_if(true).is_some(), "profiling on must produce an Instant");
    }

    /// Preserves the `else { 0 }` arm the per-file `elapsed_us` helpers used, so a
    /// switched-off build still feeds zeros to the accumulators rather than garbage.
    #[test]
    fn elapsed_us_is_zero_without_a_start_instant() {
        assert_eq!(elapsed_us(None), 0);
    }

    /// A measured span is reported in whole microseconds, matching the previous
    /// `as_micros() as u64` truncation the recorders were written against.
    #[test]
    fn elapsed_us_measures_from_the_given_instant() {
        let start = now_if(true);
        std::thread::sleep(std::time::Duration::from_millis(2));
        assert!(elapsed_us(start) >= 1_000, "a 2ms sleep must register at least 1000us");
    }

    /// `enabled()` is latched, so every call site sees one answer for the process
    /// lifetime even though only the first one touches the environment.
    #[test]
    fn enabled_is_stable_across_calls() {
        let first = enabled();
        for _ in 0..64 {
            assert_eq!(enabled(), first);
        }
    }
}
