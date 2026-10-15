//! IDL `float128` (`long double`) value carrier.
//!
//! Stable Rust has no `f128` (rust-lang/rust#116909), so the value is kept as raw IEEE 754
//! binary128 bits. Storing it as `f64` dropped the low 60 mantissa bits, and peers that compare
//! the full 16 bytes rejected the sample.

use std::fmt;
use std::str::FromStr;

const MANT_BITS: u32 = 112;
const MANT_MASK: u128 = (1u128 << MANT_BITS) - 1;
const EXP_MASK: u128 = 0x7FFF;
const EXP_BIAS: i64 = 16383;
/// Binary exponent of the least significant bit of a subnormal.
const MIN_LSB_EXP: i64 = 1 - EXP_BIAS - MANT_BITS as i64;

/// IEEE 754 binary128 bits.
///
/// Equality and hashing are bitwise (`NaN == NaN`, `0.0 != -0.0`) because the type is a wire
/// carrier, not an arithmetic type.
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Float128(u128);

impl Float128 {
    pub const ZERO: Float128 = Float128(0);

    pub const fn from_bits(bits: u128) -> Self {
        Float128(bits)
    }

    pub const fn to_bits(self) -> u128 {
        self.0
    }

    /// Nearest `f64`, ties to even.
    pub fn to_f64(self) -> f64 {
        let v = self.0;
        let sign = ((v >> 127) as u64) << 63;
        let exp = ((v >> MANT_BITS) & EXP_MASK) as i32;
        let mant = v & MANT_MASK;
        let bits = match exp {
            0x7FFF => {
                let m = (mant >> 60) as u64;
                sign | (0x7FFu64 << 52) | if mant != 0 && m == 0 { 1u64 << 51 } else { m }
            }
            0 => sign,
            _ => {
                let e = exp - 16383;
                if e > 1023 {
                    sign | (0x7FFu64 << 52)
                } else if e >= -1022 {
                    sign + (((e + 1023) as u64) << 52) + round_shift(mant, 60)
                } else {
                    let shift = (-e - 962) as u32;
                    let m =
                        if shift >= 128 { 0 } else { round_shift((1u128 << 112) | mant, shift) };
                    sign | m
                }
            }
        };
        f64::from_bits(bits)
    }
}

/// Exact widening; every `f64` is representable in binary128.
impl From<f64> for Float128 {
    fn from(value: f64) -> Self {
        let bits = value.to_bits();
        let sign = (bits >> 63) as u128;
        let exp = ((bits >> 52) & 0x7FF) as i32;
        let mant = bits & ((1u64 << 52) - 1);
        let (exp128, mant128): (u128, u128) = match exp {
            0 if mant == 0 => (0, 0),
            0 => {
                let lz = mant.leading_zeros() - 12;
                let norm = (mant << (lz + 1)) & ((1u64 << 52) - 1);
                ((15360 - lz) as u128, (norm as u128) << 60)
            }
            0x7FF => (0x7FFF, (mant as u128) << 60),
            _ => ((exp - 1023 + 16383) as u128, (mant as u128) << 60),
        };
        Float128((sign << 127) | (exp128 << MANT_BITS) | mant128)
    }
}

impl From<Float128> for f64 {
    fn from(value: Float128) -> Self {
        value.to_f64()
    }
}

/// Shows the nearest `f64`; use [`Float128::to_bits`] for the exact value.
impl fmt::Display for Float128 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.to_f64(), f)
    }
}

impl fmt::Debug for Float128 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Float128({} = {:#034x})", self.to_f64(), self.0)
    }
}

// Derived DdsType structs also implement speedy, which requires it of every field.
impl<'a, C: speedy::Context> speedy::Readable<'a, C> for Float128 {
    #[inline]
    fn read_from<R: speedy::Reader<'a, C>>(reader: &mut R) -> Result<Self, C::Error> {
        Ok(Float128(<u128 as speedy::Readable<'a, C>>::read_from(reader)?))
    }

    #[inline]
    fn minimum_bytes_needed() -> usize {
        <u128 as speedy::Readable<'a, C>>::minimum_bytes_needed()
    }
}

impl<C: speedy::Context> speedy::Writable<C> for Float128 {
    #[inline]
    fn write_to<T: ?Sized + speedy::Writer<C>>(&self, writer: &mut T) -> Result<(), C::Error> {
        <u128 as speedy::Writable<C>>::write_to(&self.0, writer)
    }

    #[inline]
    fn bytes_needed(&self) -> Result<usize, C::Error> {
        <u128 as speedy::Writable<C>>::bytes_needed(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseFloat128Error;

impl fmt::Display for ParseFloat128Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid float128 literal")
    }
}

impl std::error::Error for ParseFloat128Error {}

/// Decimal literal correctly rounded (ties to even) to binary128, in the syntax `f64` accepts.
///
/// Parsing through `f64` first would round twice and miss the bits peers produce from the same
/// literal, so the digits are converted exactly with a small bignum.
impl FromStr for Float128 {
    type Err = ParseFloat128Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (neg, body) = match s.as_bytes().first() {
            Some(b'-') => (true, &s[1..]),
            Some(b'+') => (false, &s[1..]),
            _ => (false, s),
        };
        let sign = (neg as u128) << 127;
        let lower = body.to_ascii_lowercase();
        match lower.as_str() {
            "inf" | "infinity" => return Ok(Float128(sign | (EXP_MASK << MANT_BITS))),
            "nan" => return Ok(Float128((EXP_MASK << MANT_BITS) | (1u128 << (MANT_BITS - 1)))),
            _ => {}
        }
        let (digits, exp10) = parse_decimal(body.as_bytes()).ok_or(ParseFloat128Error)?;
        Ok(Float128(sign | decimal_to_bits(&digits, exp10)))
    }
}

/// Splits `ddd.ddde±dd` into significant digits (no leading zeros) and a power of ten.
fn parse_decimal(s: &[u8]) -> Option<(Vec<u8>, i64)> {
    let mut digits = Vec::new();
    let mut exp10: i64 = 0;
    let mut seen_digit = false;
    let mut seen_dot = false;
    let mut i = 0;
    while i < s.len() {
        match s[i] {
            c @ b'0'..=b'9' => {
                seen_digit = true;
                if !(digits.is_empty() && c == b'0') {
                    digits.push(c - b'0');
                }
                if seen_dot {
                    exp10 -= 1;
                }
            }
            b'.' if !seen_dot => seen_dot = true,
            b'e' | b'E' => break,
            _ => return None,
        }
        i += 1;
    }
    if !seen_digit {
        return None;
    }
    if i < s.len() {
        let rest = &s[i + 1..];
        let (neg, num) = match rest.first() {
            Some(b'-') => (true, &rest[1..]),
            Some(b'+') => (false, &rest[1..]),
            _ => (false, rest),
        };
        if num.is_empty() || !num.iter().all(u8::is_ascii_digit) {
            return None;
        }
        // Saturate: anything this far out is already zero or infinity.
        let mut e: i64 = 0;
        for &c in num {
            e = (e * 10 + (c - b'0') as i64).min(1_000_000);
        }
        exp10 = exp10.saturating_add(if neg { -e } else { e });
    }
    Some((digits, exp10))
}

/// Unsigned binary128 bits of `digits × 10^exp10`.
fn decimal_to_bits(digits: &[u8], exp10: i64) -> u128 {
    if digits.is_empty() {
        return 0;
    }
    let magnitude = digits.len() as i64 + exp10;
    // binary128 spans roughly 6.5e-4966 ..= 1.19e4932.
    if magnitude > 4934 {
        return EXP_MASK << MANT_BITS;
    }
    if magnitude < -4967 {
        return 0;
    }

    let mut num = Big::from_digits(digits);
    let mut den = Big::one();
    if exp10 >= 0 {
        num.mul_pow10(exp10 as u32);
    } else {
        den.mul_pow10((-exp10) as u32);
    }

    // Pick k so that q = floor(num / (den·2^k)) holds 113 significant bits.
    let mut k = (num.bit_len() as i64 - den.bit_len() as i64 - 113).max(MIN_LSB_EXP);
    let (mut q, cmp_half) = loop {
        let (n, d) = if k < 0 {
            (num.shl(k.unsigned_abs()), den.clone())
        } else {
            (num.clone(), den.shl(k as u64))
        };
        let (q, rem) = n.div_small_quotient(&d);
        if q >> 113 != 0 {
            k += 1;
            continue;
        }
        break (q, rem.shl(1).cmp(&d));
    };

    let round_up = match cmp_half {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Equal => q & 1 == 1,
        std::cmp::Ordering::Less => false,
    };
    if round_up {
        q += 1;
        if q >> 113 != 0 {
            q >>= 1;
            k += 1;
        }
    }

    if q >> MANT_BITS == 0 {
        // Subnormal (k is pinned at MIN_LSB_EXP); a carry into bit 112 encodes the smallest normal.
        return q;
    }
    let biased = k + MANT_BITS as i64 + EXP_BIAS;
    if biased >= EXP_MASK as i64 {
        return EXP_MASK << MANT_BITS;
    }
    ((biased as u128) << MANT_BITS) | (q & MANT_MASK)
}

fn round_shift(value: u128, shift: u32) -> u64 {
    let q = value >> shift;
    let rem = value & ((1u128 << shift) - 1);
    let half = 1u128 << (shift - 1);
    let up = rem > half || (rem == half && (q & 1) == 1);
    (q + up as u128) as u64
}

/// Little-endian `u64` limbs; only the operations the decimal conversion needs.
#[derive(Clone, PartialEq, Eq)]
struct Big(Vec<u64>);

impl Big {
    fn one() -> Big {
        Big(vec![1])
    }

    fn from_digits(digits: &[u8]) -> Big {
        let mut n = Big(vec![0]);
        for chunk in digits.chunks(19) {
            let mut v = 0u64;
            for &d in chunk {
                v = v * 10 + d as u64;
            }
            n.mul_add_small(10u64.pow(chunk.len() as u32), v);
        }
        n
    }

    fn mul_add_small(&mut self, m: u64, a: u64) {
        let mut carry = a as u128;
        for l in &mut self.0 {
            let p = *l as u128 * m as u128 + carry;
            *l = p as u64;
            carry = p >> 64;
        }
        if carry != 0 {
            self.0.push(carry as u64);
        }
    }

    fn mul_pow10(&mut self, mut e: u32) {
        while e > 0 {
            let step = e.min(19);
            self.mul_add_small(10u64.pow(step), 0);
            e -= step;
        }
    }

    fn trim(mut self) -> Big {
        while self.0.len() > 1 && *self.0.last().unwrap() == 0 {
            self.0.pop();
        }
        self
    }

    fn bit_len(&self) -> u64 {
        let top = self.0.iter().rposition(|&l| l != 0);
        top.map_or(0, |i| i as u64 * 64 + (64 - self.0[i].leading_zeros()) as u64)
    }

    fn shl(&self, bits: u64) -> Big {
        let (limbs, rem) = ((bits / 64) as usize, (bits % 64) as u32);
        let mut out = vec![0u64; limbs];
        let mut carry = 0u64;
        for &l in &self.0 {
            out.push(if rem == 0 { l } else { (l << rem) | carry });
            carry = if rem == 0 { 0 } else { l >> (64 - rem) };
        }
        out.push(carry);
        Big(out).trim()
    }

    fn shr1(&mut self) {
        let mut carry = 0u64;
        for l in self.0.iter_mut().rev() {
            let next = *l & 1;
            *l = (*l >> 1) | (carry << 63);
            carry = next;
        }
    }

    fn sub_assign(&mut self, other: &Big) {
        let mut borrow = false;
        for i in 0..self.0.len() {
            let o = other.0.get(i).copied().unwrap_or(0);
            let (v, b1) = self.0[i].overflowing_sub(o);
            let (v, b2) = v.overflowing_sub(borrow as u64);
            self.0[i] = v;
            borrow = b1 || b2;
        }
    }

    /// Quotient and remainder by restoring division; the caller guarantees `q < 2^115`.
    fn div_small_quotient(&self, d: &Big) -> (u128, Big) {
        let mut rem = self.clone();
        let mut ds = d.shl(114);
        let mut q = 0u128;
        for bit in (0..=114).rev() {
            if rem.cmp(&ds) != std::cmp::Ordering::Less {
                rem.sub_assign(&ds);
                q |= 1u128 << bit;
            }
            ds.shr1();
        }
        (q, rem.trim())
    }
}

impl PartialOrd for Big {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Big {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let n = self.0.len().max(other.0.len());
        for i in (0..n).rev() {
            let a = self.0.get(i).copied().unwrap_or(0);
            let b = other.0.get(i).copied().unwrap_or(0);
            if a != b {
                return a.cmp(&b);
            }
        }
        std::cmp::Ordering::Equal
    }
}
