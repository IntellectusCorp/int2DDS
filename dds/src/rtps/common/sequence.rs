//! Sequence number types for RTPS message ordering.
//!
//! This module provides `SequenceNumber` for tracking and ordering RTPS messages and
//! cache changes. Sequence numbers are 64-bit monotonically increasing values used
//! for reliable delivery and duplicate detection.

use speedy::{Context, Readable, Reader, Writable, Writer};
use std::ops::{Add, Sub};
use std::{cmp::max, mem, ops::AddAssign};

#[derive(Default, Debug, Copy, PartialEq, Eq, Readable, Writable, Hash)]
pub struct SequenceNumber {
    pub high: i32,
    pub low: u32,
}

pub type SequenceNumberSet = NumberSet<SequenceNumber>;

impl SequenceNumber {
    pub const UNKNOWN: Self = Self { high: -1, low: 0 };
    pub const INIT: Self = Self { high: 0, low: 1 };
    pub const ZERO: Self = Self { high: 0, low: 0 };

    pub fn new(high: i32, low: u32) -> Self {
        Self { high, low }
    }
    // Using this structure, the 64-bit sequence number is:
    // seq_num = high * 2^32 + low
    pub fn to_i64(&self) -> i64 {
        // high.wrapping_shl(32) == high << 32
        (self.high as i64).wrapping_shl(32) | (self.low as i64)
    }

    pub fn from_i64(value: i64) -> Self {
        let high = (value >> 32) as i32;
        let low = value as u32;
        Self { high, low }
    }

    pub fn next(&self) -> Self {
        let mut result = *self;
        result += 1;
        result
    }
}

impl AddAssign<u32> for SequenceNumber {
    fn add_assign(&mut self, rhs: u32) {
        let (new_low, overflow) = self.low.overflowing_add(rhs);
        self.low = new_low;
        if overflow {
            self.high += 1;
        }
    }
}

impl Add<u32> for SequenceNumber {
    type Output = SequenceNumber;

    fn add(self, rhs: u32) -> SequenceNumber {
        let mut result = self;
        result += rhs;
        result
    }
}

impl PartialOrd for SequenceNumber {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SequenceNumber {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // First compare high values
        match self.high.cmp(&other.high) {
            std::cmp::Ordering::Equal => self.low.cmp(&other.low), // If high values are equal, compare low values
            ordering => ordering, // If high values differ, return high comparison result
        }
    }
}

impl Clone for SequenceNumber {
    fn clone(&self) -> Self {
        *self
    }
}

impl Sub for SequenceNumber {
    type Output = i64;

    fn sub(self, rhs: Self) -> Self::Output {
        self.to_i64() - rhs.to_i64()
    }
}

impl PartialEq<i64> for SequenceNumber {
    fn eq(&self, other: &i64) -> bool {
        self.to_i64() == *other
    }
}

impl PartialOrd<i64> for SequenceNumber {
    fn partial_cmp(&self, other: &i64) -> Option<std::cmp::Ordering> {
        Some(self.to_i64().cmp(other))
    }
}

impl<'a, C: Context> Readable<'a, C> for SequenceNumberSet {
    fn read_from<R: Reader<'a, C>>(reader: &mut R) -> Result<Self, <C as Context>::Error> {
        let bitmap_base: SequenceNumber = reader.read_value()?;
        let num_bits: u32 = reader.read_value()?;
        let num_of_longs: usize = num_bits.div_ceil(32) as usize;
        let mut bitmap = [0_i32; 8];
        for item in bitmap.iter_mut().take(num_of_longs) {
            *item = reader.read_value()?;
        }
        Ok(Self { bitmap_base, bitmap, num_bits })
    }
}

impl<C: Context> Writable<C> for SequenceNumberSet {
    fn write_to<T: ?Sized + Writer<C>>(&self, writer: &mut T) -> Result<(), C::Error> {
        writer.write_value(&self.bitmap_base)?;
        writer.write_u32(self.num_bits)?;
        let num_of_longs: usize = self.num_bits.div_ceil(32) as usize;
        for item in self.bitmap.iter().take(num_of_longs) {
            writer.write_i32(*item)?;
        }
        Ok(())
    }
}

pub type FragmentNumber = u32;

pub type FragmentNumberSet = NumberSet<FragmentNumber>;

impl<'a, C: Context> Readable<'a, C> for FragmentNumberSet {
    fn read_from<R: Reader<'a, C>>(reader: &mut R) -> Result<Self, <C as Context>::Error> {
        let bitmap_base: FragmentNumber = reader.read_value()?;
        let num_bits: u32 = reader.read_value()?;
        let num_of_longs: usize = num_bits.div_ceil(32) as usize;
        let mut bitmap = [0_i32; 8];
        for item in bitmap.iter_mut().take(num_of_longs) {
            *item = reader.read_value()?;
        }
        Ok(Self { bitmap_base, bitmap, num_bits })
    }
}

impl<C: Context> Writable<C> for FragmentNumberSet {
    fn write_to<T: ?Sized + Writer<C>>(&self, writer: &mut T) -> Result<(), C::Error> {
        writer.write_value(&self.bitmap_base)?;
        writer.write_u32(self.num_bits)?;
        let num_of_longs: usize = self.num_bits.div_ceil(32) as usize;
        for i in 0..(num_of_longs) {
            writer.write_i32(self.bitmap[i])?;
        }
        Ok(())
    }
}

// Common trait for implementing NumberSet (SequenceNumberSet, FragmentNumberSet)
pub trait SetNumberType:
    Copy + Clone + PartialEq + Eq + PartialOrd + Ord + std::fmt::Debug
{
    fn to_usize(self) -> usize;
    fn wrapping_sub(self, rhs: Self) -> Self;
    fn wrapping_add(self, rhs: u32) -> Self;
    fn is_valid_base(self) -> bool;
}

impl SetNumberType for SequenceNumber {
    // Safe as SequenceNumber for SequenceNumberSet is always greater than 0
    fn to_usize(self) -> usize {
        self.to_i64() as usize
    }

    fn wrapping_sub(self, rhs: Self) -> Self {
        SequenceNumber::from_i64(self - rhs)
    }

    fn wrapping_add(self, rhs: u32) -> Self {
        self + rhs
    }

    // 9.4.2.6
    fn is_valid_base(self) -> bool {
        self.to_i64() >= 1
    }
}

impl SetNumberType for FragmentNumber {
    fn to_usize(self) -> usize {
        self as usize
    }

    fn wrapping_sub(self, rhs: Self) -> Self {
        self.wrapping_sub(rhs)
    }

    fn wrapping_add(self, rhs: Self) -> Self {
        self + rhs
    }

    // 9.4.2.6
    fn is_valid_base(self) -> bool {
        self >= 1
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NumberSet<T: SetNumberType> {
    bitmap_base: T,
    bitmap: [i32; 8],
    num_bits: u32,
}

impl<T: SetNumberType> NumberSet<T> {
    pub fn new_empty_with_base(bitmap_base: T) -> Self {
        Self { bitmap_base, bitmap: [0_i32; 8], num_bits: 0 }
    }

    pub fn bitmap_base(&self) -> T {
        self.bitmap_base
    }

    pub fn bitmap(&self) -> [i32; 8] {
        self.bitmap
    }

    pub fn num_bits(&self) -> u32 {
        self.num_bits
    }

    pub fn length(&self) -> u16 {
        mem::size_of::<T>() as u16
            + mem::size_of::<u32>() as u16
            + self.num_bits.div_ceil(32) as u16 * mem::size_of::<i32>() as u16
    }

    // 9.4.2.6 && 9.4.2.8
    pub fn is_valid(&self) -> bool {
        // The bitmap base must be greater than or equal to 1
        if !self.bitmap_base.is_valid_base() {
            return false;
        }

        // 0 is not allowed in RTPS 2.1 but it was addressed as an error and resoved in next version
        // https://issues.omg.org/issues/DDSIRTP23-37
        if self.num_bits > 256 {
            return false;
        }

        // There are M = (numBits+31)/32 longs containing the pertinent bits
        let num_of_longs: usize = self.num_bits.div_ceil(32) as usize;
        if self.bitmap[num_of_longs..].iter().any(|&x| x != 0) {
            return false;
        }

        true
    }

    // Set bits in the bitmap for each number in the input vector
    pub fn from_vec(bitmap_base: T, numbers: Vec<T>) -> Self {
        let mut bitmap = [0_i32; 8];
        let mut num_bits = 0;

        for number in numbers {
            let delta_n = number.wrapping_sub(bitmap_base).to_usize();
            let bitmap_idx = delta_n / 32;
            if bitmap_idx < 8 {
                bitmap[bitmap_idx] |= 1 << (31 - (delta_n % 32));
                num_bits = max(num_bits, (delta_n + 1) as u32);
            }
        }

        Self { bitmap_base, num_bits, bitmap }
    }

    // Iterate over each bit in the bitmap and collect the numbers that are set
    pub fn extract_numbers(&self) -> Vec<T> {
        let mut numbers: Vec<T> = Vec::with_capacity(256);

        for (idx, num) in self.bitmap.iter().enumerate() {
            if *num == 0 {
                continue;
            }
            for i in (0..32).rev() {
                if ((num >> i) & 1) == 1 {
                    let offset = (idx * 32) + (31 - i);
                    numbers.push(self.bitmap_base.wrapping_add(offset as u32));
                }
            }
        }

        numbers
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sns_from_numbers() {
        let sns = SequenceNumberSet::from_vec(
            SequenceNumber::new(0, 1),
            vec![SequenceNumber::new(0, 2), SequenceNumber::new(0, 3)],
        );

        assert_eq!(sns.bitmap, [1610612736, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(sns.num_bits, 3);
    }

    #[test]
    fn test_sns_extract_numbers() {
        let sns = SequenceNumberSet {
            bitmap_base: SequenceNumber::new(0, 1),
            bitmap: [1610612736, 0, 0, 0, 0, 0, 0, 0],
            num_bits: 1,
        };

        assert_eq!(
            sns.extract_numbers(),
            vec![SequenceNumber::new(0, 2), SequenceNumber::new(0, 3)]
        );
    }

    #[test]
    fn test_fns_from_numbers() {
        let fns = FragmentNumberSet::from_vec(1, vec![2, 3]);

        assert_eq!(fns.bitmap, [1610612736, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(fns.num_bits, 3);
    }

    #[test]
    fn test_fns_extract_numbers() {
        let fns = FragmentNumberSet {
            bitmap_base: 1,
            bitmap: [1610612736, 0, 0, 0, 0, 0, 0, 0],
            num_bits: 3,
        };

        assert_eq!(fns.extract_numbers(), vec![2, 3]);
    }
}
