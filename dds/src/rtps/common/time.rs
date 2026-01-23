//! RTPS timestamp representation and conversion.
//!
//! This module provides `RtpsTime` for representing timestamps in RTPS messages.
//! RtpsTime uses seconds and fractions since the UNIX epoch, with conversions
//! to/from DDS time types and standard library time types.

use std::{
    cmp::Ordering,
    convert::From,
    ops::{Add, AddAssign, Sub, SubAssign},
    time::{Duration as StdDuration, SystemTime, UNIX_EPOCH},
};

use speedy::{Context, Readable, Writable, Writer};

use crate::dcps::core::time::{Duration as DdsDuration, Time as DdsTime};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RtpsTime {
    seconds: u32,
    // nanosec: u32,
    fraction: u32,
}
impl RtpsTime {
    pub const ZERO: Self = Self { seconds: 0, fraction: 0 };
    pub const INVALID: Self = Self { seconds: 0xffffffff, fraction: 0xffffffff };
    pub const INFINITE: Self = Self { seconds: 0xffffffff, fraction: 0xfffffffe };

    pub fn new(seconds: u32, fraction: u32) -> Self {
        Self { seconds, fraction }
    }

    pub fn from_seconds(sec: f64) -> Self {
        if sec.is_infinite() {
            return Self::INFINITE;
        }
        if sec.is_nan() || sec < 0.0 {
            return Self::INVALID;
        }

        let seconds = sec.trunc() as u32;
        let frac_part = sec.fract();
        let fraction = (frac_part * (1u64 << 32) as f64) as u32;

        Self { seconds, fraction }
    }

    pub fn from_nanos(nanos: u64) -> Self {
        let seconds = (nanos / 1_000_000_000) as u32;
        let remaining_nanos = (nanos % 1_000_000_000) as u32;
        let fraction = nano_to_frac(remaining_nanos);

        Self { seconds, fraction }
    }

    pub fn now() -> Self {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| {
                let seconds = u32::try_from(duration.as_secs()).unwrap_or(u32::MAX); // Prevent overflow after year 2106
                let nanosec = duration.subsec_nanos();
                let fraction = nano_to_frac(nanosec);
                Self { seconds, fraction }
            })
            .unwrap_or_else(|_| Self::INVALID)
    }

    pub fn seconds(&self) -> u32 {
        self.seconds
    }

    pub fn fraction(&self) -> u32 {
        self.fraction
    }

    pub fn set_seconds(&mut self, sec: u32) {
        self.seconds = sec;
    }

    pub fn set_fraction(&mut self, frac: u32) {
        self.fraction = frac;
    }

    // Convenience method to get nanoseconds from fraction
    pub fn nanosec(&self) -> u32 {
        if self.fraction == 0xffffffff {
            u32::MAX
        } else {
            frac_to_nano(self.fraction)
        }
    }

    // Convenience method to set fraction from nanoseconds
    pub fn set_nanosec(&mut self, mut nanos: u32) {
        const NANOS_PER_SEC: u32 = 1_000_000_000;

        if nanos >= NANOS_PER_SEC {
            nanos %= NANOS_PER_SEC;
        }

        self.fraction = match nanos {
            u32::MAX => u32::MAX,
            n => nano_to_frac(n),
        };
    }

    pub fn to_seconds_f64(&self) -> f64 {
        match *self {
            Self::INFINITE => f64::INFINITY,
            Self::INVALID => f64::NAN,
            _ => self.seconds as f64 + (self.fraction as f64 / (1u64 << 32) as f64),
        }
    }

    pub fn to_nanos(&self) -> u64 {
        if self.is_infinite() || self.is_invalid() {
            return u64::MAX;
        }

        (self.seconds as u64 * 1_000_000_000) + self.nanosec() as u64
    }

    pub fn to_duration(&self) -> RtpsDuration {
        RtpsDuration { seconds: self.seconds as i32, fraction: self.fraction }
    }

    pub fn from_duration(duration: &RtpsDuration) -> Self {
        RtpsTime { seconds: duration.seconds.max(0) as u32, fraction: duration.fraction }
    }

    pub fn is_infinite(&self) -> bool {
        self.seconds == 0xffffffff && self.fraction == 0xfffffffe
    }

    pub fn is_zero(&self) -> bool {
        self.seconds == 0 && self.fraction == 0
    }

    pub fn is_invalid(&self) -> bool {
        self.seconds == 0xffffffff && self.fraction == 0xffffffff
    }

    /// Adds a duration in nanoseconds
    pub fn add_nanos(self, nanos: u64) -> Self {
        if self.is_infinite() || self.is_invalid() {
            return self;
        }

        let total_nanos = self.to_nanos().saturating_add(nanos);
        Self::from_nanos(total_nanos)
    }

    /// Subtracts a duration in nanoseconds
    pub fn sub_nanos(self, nanos: u64) -> Self {
        if self.is_infinite() || self.is_invalid() {
            return self;
        }

        let total_nanos = self.to_nanos();
        if nanos >= total_nanos {
            Self::ZERO
        } else {
            Self::from_nanos(total_nanos - nanos)
        }
    }

    pub fn to_std_duration(&self) -> StdDuration {
        StdDuration::from_nanos(self.to_nanos())
    }
}

impl<C: Context> Writable<C> for RtpsTime {
    fn write_to<T: ?Sized + Writer<C>>(&self, writer: &mut T) -> Result<(), C::Error> {
        writer.write_value(&self.seconds)?;
        writer.write_value(&self.fraction)?;

        Ok(())
    }
}

impl std::fmt::Display for RtpsTime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::INFINITE => write!(f, "INFINITE"),
            Self::INVALID => write!(f, "INVALID"),
            Self::ZERO => write!(f, "0.000000000"),
            _ => {
                let nanos = self.nanosec();
                write!(f, "{}.{:09}", self.seconds, nanos)
            }
        }
    }
}

impl Add for RtpsTime {
    type Output = Self;

    fn add(self, other: Self) -> Self::Output {
        if self.is_infinite() || self.is_invalid() || other.is_infinite() || other.is_invalid() {
            return if self.is_invalid() || other.is_invalid() {
                Self::INVALID
            } else {
                Self::INFINITE
            };
        }

        let total_nanos = self.to_nanos().saturating_add(other.to_nanos());
        Self::from_nanos(total_nanos)
    }
}

impl AddAssign for RtpsTime {
    fn add_assign(&mut self, other: Self) {
        *self = *self + other;
    }
}

impl Sub for RtpsTime {
    type Output = Self;

    fn sub(self, other: Self) -> Self::Output {
        if self.is_infinite() || self.is_invalid() || other.is_infinite() || other.is_invalid() {
            return if self.is_invalid() || other.is_invalid() {
                Self::INVALID
            } else if self.is_infinite() {
                Self::INFINITE
            } else {
                Self::ZERO
            };
        }

        let self_nanos = self.to_nanos();
        let other_nanos = other.to_nanos();

        if other_nanos >= self_nanos {
            Self::ZERO
        } else {
            Self::from_nanos(self_nanos - other_nanos)
        }
    }
}

impl SubAssign for RtpsTime {
    fn sub_assign(&mut self, other: Self) {
        *self = *self - other;
    }
}

impl PartialOrd for RtpsTime {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RtpsTime {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.is_infinite() || self.is_invalid(), other.is_infinite() || other.is_invalid()) {
            (true, true) => {
                if self.is_invalid() && other.is_invalid() {
                    Ordering::Equal
                } else if self.is_invalid() {
                    Ordering::Less // Invalid is considered less than everything
                } else if other.is_invalid() {
                    Ordering::Greater
                } else {
                    Ordering::Equal // Both infinite
                }
            }
            (true, false) => {
                if self.is_invalid() {
                    Ordering::Less
                } else {
                    Ordering::Greater // Infinite
                }
            }
            (false, true) => {
                if other.is_invalid() {
                    Ordering::Greater
                } else {
                    Ordering::Less // Other is infinite
                }
            }
            (false, false) => {
                // Normal comparison
                match self.seconds.cmp(&other.seconds) {
                    Ordering::Equal => self.fraction.cmp(&other.fraction),
                    ordering => ordering,
                }
            }
        }
    }
}
pub type Timestamp = RtpsTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Readable, Writable)]
pub struct RtpsDuration {
    seconds: i32,
    fraction: u32,
}
impl RtpsDuration {
    pub const ZERO: Self = Self { seconds: 0, fraction: 0 };
    pub const INFINITE: Self = Self { seconds: 0x7fffffff, fraction: 0xffffffff };

    pub fn new(seconds: i32, fraction: u32) -> Self {
        Self { seconds, fraction }
    }

    pub fn from_seconds_f64(sec: f64) -> Self {
        if sec.is_infinite() {
            return Self::INFINITE;
        }

        let seconds = sec.trunc() as i32;
        let frac_part = sec.fract().abs();
        let fraction = (frac_part * (1u64 << 32) as f64) as u32;

        Self { seconds, fraction }
    }

    /// Creates a duration from milliseconds
    pub fn from_millis(millis: u64) -> Self {
        let seconds = (millis / 1000) as i32;
        let remaining_millis = millis % 1000;
        // fraction = remaining_millis * 2^32 / 1000
        let fraction = ((remaining_millis << 32) / 1000) as u32;
        Self { seconds, fraction }
    }

    /// Creates a duration from nanoseconds
    pub fn from_nanos(nanos: u64) -> Self {
        let seconds = (nanos / 1_000_000_000) as i32;
        let remaining_nanos = nanos % 1_000_000_000;
        // fraction = remaining_nanos * 2^32 / 1_000_000_000
        let fraction = ((remaining_nanos << 32) / 1_000_000_000) as u32;
        Self { seconds, fraction }
    }

    /// Gets the seconds component
    #[inline]
    pub const fn seconds(&self) -> i32 {
        self.seconds
    }

    /// Gets the fraction component
    #[inline]
    pub const fn fraction(&self) -> u32 {
        self.fraction
    }

    /// Converts to floating-point seconds
    pub fn to_seconds_f64(&self) -> f64 {
        if self.is_infinite() {
            f64::INFINITY
        } else {
            self.seconds as f64 + (self.fraction as f64 / (1u64 << 32) as f64)
        }
    }

    /// Checks if this is infinite duration
    #[inline]
    pub const fn is_infinite(&self) -> bool {
        self.seconds == 0x7fffffff && self.fraction == 0xffffffff
    }

    /// Checks if this is zero duration
    #[inline]
    pub const fn is_zero(&self) -> bool {
        self.seconds == 0 && self.fraction == 0
    }

    pub fn to_std_duration(&self) -> StdDuration {
        StdDuration::from_nanos(RtpsTime::from_duration(self).to_nanos())
    }

    pub fn nanosec(&self) -> u32 {
        if self.fraction == 0xffffffff {
            u32::MAX
        } else {
            frac_to_nano(self.fraction)
        }
    }
}

// impl<C: Context> Writable<C> for Duration {
//     fn write_to<T: ?Sized + Writer<C>>(&self, writer: &mut T) -> Result<(), C::Error> {
//         writer.write_value(&self.seconds)?;
//         writer.write_value(&self.fraction)?;

//         Ok(())
//     }
// }

// impl<'a, C: Context> Readable<'a, C> for Duration {
//     fn read_from<T: ?Sized + Reader<'a, C>>(reader: &mut T) -> Result<Self, C::Error> {
//         let seconds = reader.read_value()?;
//         let fraction = reader.read_value()?;
//         Ok(Self { seconds, fraction })
//     }
// }

impl Default for RtpsDuration {
    fn default() -> Self {
        Self::ZERO
    }
}

impl std::fmt::Display for RtpsDuration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_infinite() {
            write!(f, "INFINITE")
        } else {
            write!(f, "{}s", self.to_seconds_f64())
        }
    }
}

// Helper functions for fraction <-> nanosecond conversion
fn frac_to_nano(frac: u32) -> u32 {
    // Convert RTPS fraction (2^32 units per second) to nanoseconds
    if frac == u32::MAX {
        u32::MAX
    } else {
        (((frac as u64 * 1_000_000_000) + (1u64 << 31)) >> 32) as u32
    }
}

fn nano_to_frac(nano: u32) -> u32 {
    // Convert nanoseconds to RTPS fraction (2^32 units per second)
    if nano == u32::MAX {
        u32::MAX
    } else {
        ((((nano as u64) << 32) + 500_000_000) / 1_000_000_000) as u32
    }
}

impl From<DdsTime> for RtpsTime {
    fn from(dds_time: DdsTime) -> Self {
        if dds_time.sec < 0 {
            return Self::INVALID;
        }
        Self::new(dds_time.sec as u32, nano_to_frac(dds_time.nanosec))
    }
}

impl From<RtpsTime> for DdsTime {
    fn from(time: RtpsTime) -> Self {
        if time.is_invalid() {
            return Self::invalid();
        }
        if time.is_infinite() {
            return Self::infinite();
        }
        Self { sec: time.seconds as i32, nanosec: time.nanosec() }
    }
}

impl From<DdsDuration> for RtpsDuration {
    fn from(dds_duration: DdsDuration) -> Self {
        if dds_duration.is_infinite() {
            return Self::INFINITE;
        }
        Self::new(dds_duration.sec, nano_to_frac(dds_duration.nanosec))
    }
}

impl From<RtpsDuration> for DdsDuration {
    fn from(duration: RtpsDuration) -> Self {
        if duration.is_infinite() {
            return Self::infinite();
        }
        Self { sec: duration.seconds, nanosec: duration.nanosec() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_constants() {
        assert!(RtpsTime::ZERO.is_zero());
        assert!(RtpsTime::INFINITE.is_infinite());
        assert!(RtpsTime::INVALID.is_invalid());
    }

    #[test]
    fn test_time_conversion() {
        let t = RtpsTime::from_seconds(1.5);
        assert_eq!(t.seconds(), 1);
        assert_eq!(t.to_seconds_f64(), 1.5);
    }

    #[test]
    fn test_time_arithmetic() {
        let t1 = RtpsTime::from_seconds(1.0);
        let t2 = RtpsTime::from_seconds(0.5);
        let result = t1 + t2;
        assert_eq!(result.to_seconds_f64(), 1.5);
    }

    #[test]
    fn test_time_ordering() {
        let t1 = RtpsTime::from_seconds(1.0);
        let t2 = RtpsTime::from_seconds(2.0);
        assert!(t1 < t2);
        assert!(RtpsTime::INFINITE > t2);
        assert!(RtpsTime::INVALID < t1);
    }
}
