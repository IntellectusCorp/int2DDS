//! Time and duration types for DDS.
//!
//! This module defines time-related types used throughout the DDS API, including `Duration`
//! for representing time intervals and `Time` for representing absolute timestamps.
//!
//! These types are used in QoS policies (deadlines, liveliness lease duration), timeout
//! parameters for blocking operations, and timestamping of data samples.

use crate::dcps::topic::type_support::DdsType;
use const_default::ConstDefault;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::{
    borrow::Borrow,
    cmp::Ordering,
    time::{SystemTime, UNIX_EPOCH},
};

use super::error::{DdsError, DdsResult};

#[derive(DdsType, ConstDefault, Copy, Eq, Deserialize, Serialize)]
#[dds_type(crate_path = "crate")]
pub struct Duration {
    #[serde(deserialize_with = "deserialize_duration_field")]
    #[serde(serialize_with = "serialize_duration_sec")]
    pub sec: i32,

    #[serde(deserialize_with = "deserialize_duration_field_u32")]
    #[serde(serialize_with = "serialize_duration_nsec")]
    pub nanosec: u32,
}
impl Duration {
    pub const INFINITE_SEC: i32 = 0x7fffffff;
    pub const INFINITE_NSEC: u32 = 0x7fffffff;
    pub const ZERO_SEC: i32 = 0;
    pub const ZERO_NSEC: u32 = 0;

    pub fn new(sec: i32, nanosec: u32) -> Self {
        let sec = sec + (nanosec / 1_000_000_000) as i32;
        let nanosec = nanosec % 1_000_000_000;
        Self { sec, nanosec }
    }

    pub fn infinite() -> Self {
        Self { sec: Self::INFINITE_SEC, nanosec: Self::INFINITE_NSEC }
    }

    pub fn zero() -> Self {
        Self { sec: Self::ZERO_SEC, nanosec: Self::ZERO_NSEC }
    }

    pub fn from_seconds(seconds: i32) -> Self {
        Self { sec: seconds, nanosec: 0 }
    }

    pub fn from_millis(millis: i64) -> Self {
        let sec = (millis / 1000) as i32;
        let nanosec = ((millis % 1000) * 1_000_000) as u32;
        Self { sec, nanosec }
    }

    pub fn from_nanos(nanos: i64) -> Self {
        let sec = (nanos / 1_000_000_000_i64) as i32;
        let nanosec = (nanos % 1_000_000_000_i64) as u32;
        Self { sec, nanosec }
    }

    pub fn is_infinite(&self) -> bool {
        self.sec == Self::INFINITE_SEC && self.nanosec == Self::INFINITE_NSEC
    }

    pub fn is_zero(&self) -> bool {
        self.sec == Self::ZERO_SEC && self.nanosec == Self::ZERO_NSEC
    }

    pub fn is_positive(&self) -> bool {
        self.sec > 0 || (self.sec == 0 && self.nanosec > 0)
    }

    pub fn is_negative(&self) -> bool {
        self.sec < 0
    }

    pub fn as_seconds(&self) -> f64 {
        self.sec as f64 + (self.nanosec as f64 / 1_000_000_000_f64)
    }

    pub fn as_millis(&self) -> i64 {
        (self.sec as i64 * 1000) + (self.nanosec as i64 / 1_000_000)
    }

    /// Convert duration to nanoseconds.
    pub fn as_nanos(&self) -> i64 {
        (self.sec as i64 * 1_000_000_000_i64) + self.nanosec as i64
    }
}

impl TryFrom<Duration> for std::time::Duration {
    type Error = DdsError;

    fn try_from(duration: Duration) -> Result<Self, Self::Error> {
        if duration.is_infinite() {
            return Err(DdsError::Error(
                "Cannot convert infinite Duration to std::time::Duration".to_string(),
            ));
        }

        if duration.is_negative() {
            return Err(DdsError::Error(format!(
                "Cannot convert negative Duration ({} sec) to std::time::Duration",
                duration.sec
            )));
        }

        Ok(std::time::Duration::new(duration.sec as u64, duration.nanosec))
    }
}

impl TryFrom<std::time::Duration> for Duration {
    type Error = DdsError;

    fn try_from(duration: std::time::Duration) -> Result<Self, Self::Error> {
        Ok(Duration::from_nanos(duration.as_nanos() as i64))
    }
}

impl std::ops::Add<Duration> for Duration {
    type Output = Duration;

    fn add(self, other: Duration) -> Self::Output {
        if self.is_infinite() || other.is_infinite() {
            return Self::infinite();
        }

        let mut sec = self.sec + other.sec;
        let mut nanosec = self.nanosec + other.nanosec;

        if nanosec >= 1_000_000_000 {
            sec += 1;
            nanosec -= 1_000_000_000;
        }

        Self { sec, nanosec }
    }
}

impl std::ops::Add<&Duration> for Duration {
    type Output = Duration;

    fn add(self, other: &Duration) -> Self::Output {
        if self.is_infinite() || other.is_infinite() {
            return Self::infinite();
        }

        let mut sec = self.sec + other.sec;
        let mut nanosec = self.nanosec + other.nanosec;

        if nanosec >= 1_000_000_000 {
            sec += 1;
            nanosec -= 1_000_000_000;
        }

        Self { sec, nanosec }
    }
}

impl std::ops::Add<Time> for Duration {
    type Output = Time;

    fn add(self, time: Time) -> Self::Output {
        if self.is_infinite() {
            return Time { sec: i32::MAX, nanosec: 999_999_999 };
        }

        let mut sec = time.sec + self.sec;
        let mut nanosec = time.nanosec + self.nanosec;

        if nanosec >= 1_000_000_000 {
            sec += 1;
            nanosec -= 1_000_000_000;
        }

        Time { sec, nanosec }
    }
}

impl std::ops::Add<&Time> for Duration {
    type Output = Time;

    fn add(self, time: &Time) -> Self::Output {
        if self.is_infinite() {
            return Time { sec: i32::MAX, nanosec: 999_999_999 };
        }

        let mut sec = time.sec + self.sec;
        let mut nanosec = time.nanosec + self.nanosec;

        if nanosec >= 1_000_000_000 {
            sec += 1;
            nanosec -= 1_000_000_000;
        }

        Time { sec, nanosec }
    }
}

impl<T: Borrow<Duration>> std::ops::Sub<T> for Duration {
    type Output = Duration;

    fn sub(self, other: T) -> Self::Output {
        let other = other.borrow();

        let mut sec = self.sec - other.sec;
        let mut nanosec = self.nanosec as i64 - other.nanosec as i64;

        if nanosec < 0 {
            sec -= 1;
            nanosec += 1_000_000_000_i64;
        }

        Self { sec, nanosec: nanosec as u32 }
    }
}

impl std::ops::Mul<f64> for Duration {
    type Output = Duration;

    fn mul(self, scalar: f64) -> Self::Output {
        if self.is_infinite() || scalar.is_infinite() {
            return Self::infinite();
        }
        if scalar == 0.0 {
            return Self::zero();
        }

        let total_nanos = (self.as_seconds() * scalar * 1_000_000_000_f64) as i64;
        Self::from_nanos(total_nanos)
    }
}

impl std::ops::Div<f64> for Duration {
    type Output = Self;

    fn div(self, divisor: f64) -> Self::Output {
        if divisor == 0.0 {
            return Self::infinite();
        }

        if self.is_infinite() {
            return Self::infinite();
        }

        self * (1.0 / divisor)
    }
}

impl PartialOrd for Duration {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Duration {
    fn cmp(&self, other: &Self) -> Ordering {
        if self.is_infinite() && other.is_infinite() {
            return Ordering::Equal;
        } else if self.is_infinite() {
            return Ordering::Greater;
        } else if other.is_infinite() {
            return Ordering::Less;
        }

        match self.sec.cmp(&other.sec) {
            Ordering::Equal => self.nanosec.cmp(&other.nanosec),
            ordering => ordering,
        }
    }
}

fn deserialize_duration_field<'de, D>(deserializer: D) -> Result<i32, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum IntOrString {
        Int(i32),
        String(String),
    }

    match IntOrString::deserialize(deserializer)? {
        IntOrString::Int(v) => Ok(v),
        IntOrString::String(s) => match s.as_str() {
            "DURATION_INFINITY" | "DURATION_INFINITE_SEC" => Ok(Duration::INFINITE_SEC),
            "DURATION_ZERO_SEC" => Ok(Duration::ZERO_SEC),
            _ => Err(de::Error::custom(format!("invalid duration constant: {}", s))),
        },
    }
}

// u32 필드 역직렬화
fn deserialize_duration_field_u32<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum IntOrString {
        Int(u32),
        String(String),
    }

    match IntOrString::deserialize(deserializer)? {
        IntOrString::Int(v) => Ok(v),
        IntOrString::String(s) => match s.as_str() {
            "DURATION_INFINITY" | "DURATION_INFINITE_NSEC" => Ok(Duration::INFINITE_NSEC),
            "DURATION_ZERO_NSEC" => Ok(Duration::ZERO_NSEC),
            _ => Err(de::Error::custom(format!("invalid duration constant: {}", s))),
        },
    }
}

// 직렬화 (무한 값은 상수 문자열로, 일반 값은 숫자로)
fn serialize_duration_sec<S>(sec: &i32, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    if *sec == Duration::INFINITE_SEC {
        serializer.serialize_str("DURATION_INFINITE_SEC")
    } else {
        serializer.serialize_i32(*sec)
    }
}

fn serialize_duration_nsec<S>(nanosec: &u32, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    if *nanosec == Duration::INFINITE_NSEC {
        serializer.serialize_str("DURATION_INFINITE_NSEC")
    } else {
        serializer.serialize_u32(*nanosec)
    }
}

#[derive(Debug, Default, ConstDefault, Clone, Copy, PartialEq, Eq)]
pub struct Time {
    pub sec: i32,
    pub nanosec: u32,
}

impl Time {
    pub const INFINITE_SEC: i32 = 0x7fffffff;
    pub const INFINITE_NSEC: u32 = 0x7fffffff;
    pub const INVALID_SEC: i32 = -1;
    pub const INVALID_NSEC: u32 = 0xffffffff;

    pub fn new(sec: i32, nanosec: u32) -> Self {
        let sec = sec + (nanosec / 1_000_000_000) as i32;
        let nanosec = nanosec % 1_000_000_000;
        Self { sec, nanosec }
    }

    pub fn now() -> Self {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| {
                let sec = i32::try_from(duration.as_secs()).unwrap_or(i32::MAX); // Prevent overflow after year 2038
                let nanosec = duration.subsec_nanos();
                Self { sec, nanosec }
            })
            .unwrap_or_else(|_| Self::invalid())
    }

    pub fn epoch() -> Self {
        Self { sec: 0, nanosec: 0 }
    }

    pub fn invalid() -> Self {
        Self { sec: Self::INVALID_SEC, nanosec: Self::INVALID_NSEC }
    }

    pub fn is_valid(&self) -> bool {
        !(self.sec == Self::INVALID_SEC && self.nanosec == Self::INVALID_NSEC)
    }

    pub fn infinite() -> Self {
        Self { sec: Self::INFINITE_SEC, nanosec: Self::INFINITE_NSEC }
    }

    pub fn is_infinite(&self) -> bool {
        self.sec == Self::INFINITE_SEC && self.nanosec == Self::INFINITE_NSEC
    }

    pub fn is_epoch(&self) -> bool {
        self.sec == 0 && self.nanosec == 0
    }

    pub fn elapsed(&self) -> DdsResult<Duration> {
        let now = Time::now(); // If there is a function to get current time

        if !self.is_valid() || !now.is_valid() {
            return Err(DdsError::BadParameter);
        }

        let mut elapsed_sec = now.sec - self.sec;
        let elapsed_nsec = if now.nanosec >= self.nanosec {
            now.nanosec - self.nanosec
        } else {
            elapsed_sec -= 1;
            1_000_000_000 + now.nanosec - self.nanosec
        };

        Ok(Duration::new(elapsed_sec, elapsed_nsec))
    }

    fn subtract_nanoseconds(self, sec: i32, nanosec: u32) -> (i32, i64) {
        let mut result_sec = self.sec - sec;
        let mut result_nanosec = self.nanosec as i64 - nanosec as i64;

        if result_nanosec < 0 {
            result_sec -= 1;
            result_nanosec += 1_000_000_000;
        }

        (result_sec, result_nanosec)
    }
}

// Time + Duration = Time
impl<T: Borrow<Duration>> std::ops::Add<T> for Time {
    type Output = Time;

    fn add(self, duration: T) -> Self::Output {
        let duration = duration.borrow();

        if duration.is_infinite() {
            // If Duration is infinite, set Time to maximum value
            return Time { sec: i32::MAX, nanosec: 999_999_999 };
        }

        let mut sec = self.sec + duration.sec;
        let mut nanosec = self.nanosec + duration.nanosec;

        if nanosec >= 1_000_000_000 {
            sec += 1;
            nanosec -= 1_000_000_000;
        }

        Time { sec, nanosec }
    }
}

// Time - Time = Duration
impl std::ops::Sub<Time> for Time {
    type Output = Duration;

    fn sub(self, other: Time) -> Self::Output {
        let (sec, nanosec) = self.subtract_nanoseconds(other.sec, other.nanosec);
        Duration { sec, nanosec: nanosec as u32 }
    }
}

// Time - &Time = Duration
impl std::ops::Sub<&Time> for Time {
    type Output = Duration;

    fn sub(self, other: &Time) -> Self::Output {
        let (sec, nanosec) = self.subtract_nanoseconds(other.sec, other.nanosec);
        Duration { sec, nanosec: nanosec as u32 }
    }
}

// Time - Duration = Time
impl std::ops::Sub<Duration> for Time {
    type Output = Time;

    fn sub(self, duration: Duration) -> Self::Output {
        if duration.is_infinite() {
            return Time { sec: i32::MIN, nanosec: 0 };
        }

        let (sec, nanosec) = self.subtract_nanoseconds(duration.sec, duration.nanosec);
        Time { sec, nanosec: nanosec as u32 }
    }
}

// Time - &Duration = Time
impl std::ops::Sub<&Duration> for Time {
    type Output = Time;

    fn sub(self, duration: &Duration) -> Self::Output {
        if duration.is_infinite() {
            return Time { sec: i32::MIN, nanosec: 0 };
        }

        let (sec, nanosec) = self.subtract_nanoseconds(duration.sec, duration.nanosec);
        Time { sec, nanosec: nanosec as u32 }
    }
}

// Duration -> Time conversion (relative to epoch)
impl From<Duration> for Time {
    fn from(duration: Duration) -> Self {
        Time::epoch() + duration
    }
}

// Time -> Duration conversion (elapsed time relative to epoch)
impl From<Time> for Duration {
    fn from(time: Time) -> Self {
        time - Time::epoch()
    }
}

impl PartialOrd for Time {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Time {
    fn cmp(&self, other: &Self) -> Ordering {
        if self.is_infinite() && other.is_infinite() {
            return Ordering::Equal;
        } else if self.is_infinite() {
            return Ordering::Greater;
        } else if other.is_infinite() {
            return Ordering::Less;
        }

        match self.sec.cmp(&other.sec) {
            Ordering::Equal => self.nanosec.cmp(&other.nanosec),
            ordering => ordering,
        }
    }
}

// Test code
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dur_basic_functions() {
        let zero = Duration::zero();
        assert!(zero.is_zero());
        assert!(!zero.is_infinite());

        let infinite = Duration::infinite();
        assert!(infinite.is_infinite());
        assert!(!infinite.is_zero());
    }

    #[test]
    fn test_dur_normalization() {
        let duration = Duration::new(1, 1_500_000_000);
        assert_eq!(duration.sec, 2);
        assert_eq!(duration.nanosec, 500_000_000);
    }

    #[test]
    fn test_dur_addition() {
        let d1 = Duration::new(1, 500_000_000);
        let d2 = Duration::new(2, 700_000_000);
        let result = d1.clone() + &d2;
        assert_eq!(result.sec, 4);
        assert_eq!(result.nanosec, 200_000_000);

        let zero = Duration::zero();
        let result = d1.clone() + &zero;
        assert_eq!(result.sec, d1.sec);
        assert_eq!(result.nanosec, d1.nanosec);

        let infinite = Duration::infinite();
        let result = d1 + &infinite;
        assert!(result.is_infinite());
    }

    #[test]
    fn test_dur_subtraction() {
        let d1 = Duration::new(5, 200_000_000);
        let d2 = Duration::new(2, 700_000_000);
        let result = d1.clone() - &d2;
        assert_eq!(result.sec, 2);
        assert_eq!(result.nanosec, 500_000_000);

        let result = d2.clone() - &d1;
        assert_eq!(result.sec, -3);
        assert_eq!(result.nanosec, 500_000_000);
    }

    #[test]
    fn test_dur_multiplication() {
        let d = Duration::new(2, 500_000_000);
        let result = d.clone() * 2.0;
        assert_eq!(result.sec, 5);
        assert_eq!(result.nanosec, 0);

        let result = d.clone() * 0.5;
        assert_eq!(result.sec, 1);
        assert_eq!(result.nanosec, 250_000_000);
    }

    #[test]
    fn test_dur_division() {
        let d = Duration::new(5, 0);
        let result = d.clone() / 2.0;
        assert_eq!(result.sec, 2);
        assert_eq!(result.nanosec, 500_000_000);

        let result = d.clone() / 0.5;
        assert_eq!(result.sec, 10);
        assert_eq!(result.nanosec, 0);
    }

    #[test]
    fn test_dur_comparison() {
        let d1 = Duration::new(1, 500_000_000);
        let d2 = Duration::new(2, 0);
        let d3 = Duration::new(1, 500_000_000);

        assert!(d1 < d2);
        assert!(d2 > d1);
        assert_eq!(d1, d3);

        let infinite = Duration::infinite();
        assert!(d1 < infinite);
        assert!(infinite > d1);
    }

    #[test]
    fn test_time_creation() {
        let epoch = Time::epoch();
        assert_eq!(epoch.sec, 0);
        assert_eq!(epoch.nanosec, 0);
        assert!(epoch.is_epoch());
        assert!(epoch.is_valid());

        let invalid = Time::invalid();
        assert!(!invalid.is_valid());

        let custom_time = Time::new(1000, 500_000_000);
        assert_eq!(custom_time.sec, 1000);
        assert_eq!(custom_time.nanosec, 500_000_000);
    }

    #[test]
    fn test_time_now() {
        let now = Time::now();
        assert!(now.is_valid());
        assert!(now.sec > 0);
    }

    #[test]
    fn test_time_plus_duration() {
        let time = Time::new(100, 500_000_000);
        let duration = Duration::new(50, 600_000_000);
        let result = time + duration;

        assert_eq!(result.sec, 151);
        assert_eq!(result.nanosec, 100_000_000);
    }

    #[test]
    fn test_time_minus_duration() {
        let time = Time::new(100, 500_000_000);
        let duration = Duration::new(50, 600_000_000);
        let result = time - duration;

        assert_eq!(result.sec, 49);
        assert_eq!(result.nanosec, 900_000_000);
    }

    #[test]
    fn test_duration_plus_time() {
        let duration = Duration::new(50, 600_000_000);
        let time = Time::new(100, 500_000_000);
        let result = duration + time;

        assert_eq!(result.sec, 151);
        assert_eq!(result.nanosec, 100_000_000);
    }

    #[test]
    fn test_conversions() {
        let duration = Duration::new(123, 456_789_000);
        let time_from_duration: Time = duration.into();
        let duration_from_time: Duration = time_from_duration.into();

        assert_eq!(duration, duration_from_time);
    }

    #[test]
    fn test_infinite_duration() {
        let time = Time::new(100, 0);
        let infinite_duration = Duration::infinite();
        let result = time + infinite_duration;

        assert_eq!(result.sec, i32::MAX);
        assert_eq!(result.nanosec, 999_999_999);
    }
}
