//! Bulk moves for runs of same-width primitives.
//!
//! CDR lays a sequence or array of same-width primitives out back to back — no
//! inter-element padding, and no per-element transform beyond byte order. So the whole
//! run can be copied in one go instead of one element at a time, which removes a bounds
//! check per element and lets the copy vectorize. On the common little-endian host with a
//! little-endian stream it degenerates to a plain `memcpy`.
//!
//! The generated C codec takes the same shortcut via `int2dds_cdr_write_prim_array` /
//! `int2dds_cdr_read_prim_array`; the two sides are kept byte-for-byte identical.

use speedy::Endianness;

/// Types whose in-memory bytes are exactly their native-endian wire bytes.
///
/// # Safety
///
/// Only implement for primitive numeric scalars: no padding bytes, and every bit pattern
/// must be a valid value. Both bulk helpers reinterpret `&[Self]` as raw bytes and build a
/// `Vec<Self>` straight out of wire bytes, neither of which is sound otherwise. In
/// particular `bool` does **not** qualify — an octet off the wire may be neither 0 nor 1.
pub(crate) unsafe trait NativeBytes: Copy {}

// SAFETY: all of these are fixed-width scalars with no padding, and every bit pattern is a
// valid value (including the float types, where any pattern is some NaN at worst).
unsafe impl NativeBytes for u8 {}
unsafe impl NativeBytes for i8 {}
unsafe impl NativeBytes for u16 {}
unsafe impl NativeBytes for i16 {}
unsafe impl NativeBytes for u32 {}
unsafe impl NativeBytes for i32 {}
unsafe impl NativeBytes for u64 {}
unsafe impl NativeBytes for i64 {}
unsafe impl NativeBytes for f32 {}
unsafe impl NativeBytes for f64 {}

/// Whether the stream's byte order differs from the host's, i.e. whether a bulk copy has
/// to be followed by a per-element reversal.
#[inline]
pub(crate) fn needs_swap(endianness: Endianness) -> bool {
    matches!(endianness, Endianness::LittleEndian) != cfg!(target_endian = "little")
}

/// Reverse each `elem_size`-wide element in place. Only reached when the stream byte order
/// differs from the host's, so never on the common LE-host/LE-stream path.
#[inline]
fn swap_in_place(bytes: &mut [u8], elem_size: usize) {
    for chunk in bytes.chunks_exact_mut(elem_size) {
        chunk.reverse();
    }
}

/// Append `data`'s CDR wire bytes to `buf`.
///
/// Byte-identical to pushing each element's `to_bytes` output in turn.
#[inline]
pub(crate) fn extend_prim_slice<T: NativeBytes>(
    buf: &mut Vec<u8>,
    data: &[T],
    endianness: Endianness,
) {
    let elem_size = std::mem::size_of::<T>();
    let len = std::mem::size_of_val(data);
    buf.reserve(len);
    let start = buf.len();

    // SAFETY: `T: NativeBytes` guarantees a scalar with no padding, so `data` occupies
    // exactly `len` initialized bytes and any bit pattern in them is meaningful. The
    // borrow of `data` ends before `buf` is touched again.
    let native = unsafe { std::slice::from_raw_parts(data.as_ptr().cast::<u8>(), len) };
    buf.extend_from_slice(native);

    if elem_size > 1 && needs_swap(endianness) {
        swap_in_place(&mut buf[start..], elem_size);
    }
}

/// Build a `Vec<T>` of `count` elements from wire bytes supplied by `fill`.
///
/// `fill` receives a slice of exactly `count * size_of::<T>()` bytes and must write all of
/// them. Callers must have validated `count` against the bytes actually remaining (via
/// `checked_capacity`) before calling, so this never over-allocates on a hostile length.
#[inline]
pub(crate) fn read_prim_vec<T: NativeBytes>(
    count: usize,
    endianness: Endianness,
    fill: impl FnOnce(&mut [u8]),
) -> Vec<T> {
    let elem_size = std::mem::size_of::<T>();
    let mut out: Vec<T> = Vec::with_capacity(count);

    // SAFETY: `with_capacity` reserved `count * elem_size` bytes, correctly aligned for
    // `T`, so the byte view is valid to write. `fill` initializes every byte of it, and
    // `T: NativeBytes` makes every resulting bit pattern a valid `T`, so `set_len` hands
    // out only initialized, valid elements.
    unsafe {
        let bytes =
            std::slice::from_raw_parts_mut(out.as_mut_ptr().cast::<u8>(), count * elem_size);
        fill(bytes);
        if elem_size > 1 && needs_swap(endianness) {
            swap_in_place(bytes, elem_size);
        }
        out.set_len(count);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serialize::{to_bytes_f64, to_bytes_i16, to_bytes_u32};

    fn both() -> [Endianness; 2] {
        [Endianness::LittleEndian, Endianness::BigEndian]
    }

    #[test]
    fn extend_matches_per_element_for_every_width() {
        for e in both() {
            let v16: Vec<i16> = (0..37).map(|i| (i * 1103) as i16).collect();
            let mut bulk = Vec::new();
            extend_prim_slice(&mut bulk, &v16, e);
            let mut one = Vec::new();
            for &x in &v16 {
                one.extend_from_slice(&to_bytes_i16(x, e));
            }
            assert_eq!(bulk, one, "i16 {:?}", e);

            let v32: Vec<u32> = (0..37).map(|i| i * 0x0101_0101).collect();
            let mut bulk = Vec::new();
            extend_prim_slice(&mut bulk, &v32, e);
            let mut one = Vec::new();
            for &x in &v32 {
                one.extend_from_slice(&to_bytes_u32(x, e));
            }
            assert_eq!(bulk, one, "u32 {:?}", e);

            let v64: Vec<f64> = (0..37).map(|i| i as f64 * -3.5).collect();
            let mut bulk = Vec::new();
            extend_prim_slice(&mut bulk, &v64, e);
            let mut one = Vec::new();
            for &x in &v64 {
                one.extend_from_slice(&to_bytes_f64(x, e));
            }
            assert_eq!(bulk, one, "f64 {:?}", e);
        }
    }

    #[test]
    fn read_round_trips_what_extend_wrote() {
        for e in both() {
            let want: Vec<u32> = (0..64u32).map(|i| i.wrapping_mul(2_654_435_761)).collect();
            let mut wire = Vec::new();
            extend_prim_slice(&mut wire, &want, e);
            let got: Vec<u32> = read_prim_vec(want.len(), e, |dst| dst.copy_from_slice(&wire));
            assert_eq!(got, want, "{:?}", e);
        }
    }

    #[test]
    fn empty_runs_allocate_and_copy_nothing() {
        let mut buf = vec![0xAAu8; 3];
        extend_prim_slice(&mut buf, &[] as &[u64], Endianness::BigEndian);
        assert_eq!(buf, vec![0xAA; 3]);

        let got: Vec<u64> = read_prim_vec(0, Endianness::BigEndian, |dst| assert!(dst.is_empty()));
        assert!(got.is_empty());
    }
}
