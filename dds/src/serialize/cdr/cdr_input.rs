use bytes::Bytes;
use std::borrow::Cow;
use std::cell::Cell;

// Deserializer input backing store. Contiguous is the non-fragmented and writer
// path (single slice). Chained is the fragment receive path: the fragment chunks
// are kept as-is and reads that cross a chunk boundary are stitched on the fly,
// so no contiguous reassembly buffer is allocated per sample. `skip` hides a
// front prefix (the 4-byte encapsulation header) so positions stay body-relative,
// matching the Contiguous path which slices the header off.
//
// `total_len` is computed once at construction so `len()` stays O(1) even though
// `check_available` runs on nearly every primitive read. `cursor` caches the
// chunk index / absolute start offset the last access ended in: reads are
// monotonic in practice, so the walk resumes there instead of rescanning from
// chunk 0. `set_position` is public and can seek backwards, so a walk starting
// before the cached chunk falls back to a scan from the front.
pub(crate) enum CdrInput<'a> {
    Contiguous(&'a [u8]),
    Chained { chunks: &'a [Bytes], skip: usize, total_len: usize, cursor: Cell<(usize, usize)> },
}

impl<'a> CdrInput<'a> {
    pub(crate) fn chained(chunks: &'a [Bytes], skip: usize) -> Self {
        let total: usize = chunks.iter().map(|c| c.len()).sum();
        CdrInput::Chained { chunks, skip, total_len: total - skip, cursor: Cell::new((0, 0)) }
    }

    // Total body byte length (excludes the skipped header prefix).
    pub(crate) fn len(&self) -> usize {
        match self {
            CdrInput::Contiguous(data) => data.len(),
            CdrInput::Chained { total_len, .. } => *total_len,
        }
    }

    // Walk `len` bytes starting at absolute position `pos`, invoking `f` with each
    // chunk sub-slice in order. Caller guarantees the range is in bounds.
    fn walk_chunks(
        chunks: &[Bytes],
        cursor: &Cell<(usize, usize)>,
        mut pos: usize,
        len: usize,
        mut f: impl FnMut(&[u8]),
    ) {
        let mut remaining = len;
        let (mut idx, mut chunk_start) = cursor.get();
        if pos < chunk_start {
            idx = 0;
            chunk_start = 0;
        }
        while idx < chunks.len() && remaining > 0 {
            let chunk = &chunks[idx];
            let chunk_end = chunk_start + chunk.len();
            if pos < chunk_end {
                let within = pos - chunk_start;
                let take = (chunk.len() - within).min(remaining);
                f(&chunk[within..within + take]);
                pos += take;
                remaining -= take;
                if remaining == 0 {
                    break;
                }
            }
            chunk_start = chunk_end;
            idx += 1;
        }
        cursor.set((idx, chunk_start));
    }

    // Copy out.len() bytes starting at body offset into out. Caller guarantees
    // offset + out.len() <= len() (checked via check_available).
    pub(crate) fn copy_to(&self, offset: usize, out: &mut [u8]) {
        match self {
            CdrInput::Contiguous(data) => {
                out.copy_from_slice(&data[offset..offset + out.len()]);
            }
            CdrInput::Chained { chunks, skip, cursor, .. } => {
                let mut written = 0;
                Self::walk_chunks(chunks, cursor, offset + skip, out.len(), |s| {
                    out[written..written + s.len()].copy_from_slice(s);
                    written += s.len();
                });
            }
        }
    }

    // Copy len bytes starting at offset into a fresh Vec without zero-filling.
    pub(crate) fn copy_to_vec(&self, offset: usize, len: usize) -> Vec<u8> {
        match self {
            CdrInput::Contiguous(data) => data[offset..offset + len].to_vec(),
            CdrInput::Chained { chunks, skip, cursor, .. } => {
                let mut out = Vec::with_capacity(len);
                Self::walk_chunks(chunks, cursor, offset + skip, len, |s| {
                    out.extend_from_slice(s);
                });
                out
            }
        }
    }

    // Borrow body bytes when contiguous, else gather them into an owned Vec.
    // Lets slice-consuming callers avoid copying on the contiguous path.
    pub(crate) fn bytes(&self, offset: usize, len: usize) -> Cow<'a, [u8]> {
        match self {
            CdrInput::Contiguous(data) => Cow::Borrowed(&data[offset..offset + len]),
            CdrInput::Chained { .. } => Cow::Owned(self.copy_to_vec(offset, len)),
        }
    }

    // Read a single byte at offset.
    pub(crate) fn read_byte(&self, offset: usize) -> u8 {
        let mut b = [0u8; 1];
        self.copy_to(offset, &mut b);
        b[0]
    }

    // Read a fixed-size N-byte array at offset.
    pub(crate) fn read_array<const N: usize>(&self, offset: usize) -> [u8; N] {
        let mut a = [0u8; N];
        self.copy_to(offset, &mut a);
        a
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunks(parts: &[&[u8]]) -> Vec<Bytes> {
        parts.iter().map(|p| Bytes::copy_from_slice(p)).collect()
    }

    #[test]
    fn contiguous_copy_to_reads_subrange() {
        let data = [1u8, 2, 3, 4, 5];
        let input = CdrInput::Contiguous(&data);
        let mut out = [0u8; 2];
        input.copy_to(1, &mut out);
        assert_eq!(out, [2, 3]);
    }

    #[test]
    fn chained_copy_to_crosses_one_boundary() {
        let cs = chunks(&[&[1, 2, 3], &[4, 5, 6]]);
        let input = CdrInput::chained(&cs, 0);
        // offset 2 spans the last byte of chunk 0 and the first two of chunk 1.
        let mut out = [0u8; 3];
        input.copy_to(2, &mut out);
        assert_eq!(out, [3, 4, 5]);
    }

    #[test]
    fn chained_copy_to_starting_at_chunk_boundary() {
        let cs = chunks(&[&[1, 2, 3], &[4, 5, 6]]);
        let input = CdrInput::chained(&cs, 0);
        let mut out = [0u8; 2];
        input.copy_to(3, &mut out);
        assert_eq!(out, [4, 5]);
    }

    #[test]
    fn chained_read_array_crosses_boundary() {
        let cs = chunks(&[&[0x11, 0x22], &[0x33, 0x44]]);
        let input = CdrInput::chained(&cs, 0);
        let a: [u8; 4] = input.read_array(0);
        assert_eq!(a, [0x11, 0x22, 0x33, 0x44]);
        let b: [u8; 2] = input.read_array(1);
        assert_eq!(b, [0x22, 0x33]);
    }

    #[test]
    fn chained_read_byte_finds_correct_chunk() {
        let cs = chunks(&[&[1, 2], &[3, 4, 5], &[6]]);
        let input = CdrInput::chained(&cs, 0);
        assert_eq!(input.read_byte(0), 1);
        assert_eq!(input.read_byte(2), 3);
        assert_eq!(input.read_byte(4), 5);
        assert_eq!(input.read_byte(5), 6);
    }

    #[test]
    fn chained_copy_to_vec_spans_multiple_chunks() {
        let cs = chunks(&[&[1, 2], &[3, 4, 5], &[6]]);
        let input = CdrInput::chained(&cs, 0);
        let v = input.copy_to_vec(1, 4);
        assert_eq!(v, vec![2, 3, 4, 5]);
    }

    #[test]
    fn contiguous_copy_to_vec_reads_subrange() {
        let data = [1u8, 2, 3, 4, 5];
        let input = CdrInput::Contiguous(&data);
        assert_eq!(input.copy_to_vec(2, 3), vec![3, 4, 5]);
    }

    #[test]
    fn len_reports_total_across_chunks() {
        let cs = chunks(&[&[1, 2], &[3, 4, 5], &[6]]);
        assert_eq!(CdrInput::chained(&cs, 0).len(), 6);
        let data = [0u8; 9];
        assert_eq!(CdrInput::Contiguous(&data).len(), 9);
    }

    #[test]
    fn chained_skip_hides_header_prefix() {
        // skip=4 models the encapsulation header living in the first chunk;
        // body offset 0 must map past it, and len reports body size only.
        let cs = chunks(&[&[0xAA, 0xBB, 0xCC, 0xDD, 1, 2], &[3, 4]]);
        let input = CdrInput::chained(&cs, 4);
        assert_eq!(input.len(), 4);
        assert_eq!(input.read_byte(0), 1);
        let a: [u8; 4] = input.read_array(0);
        assert_eq!(a, [1, 2, 3, 4]);
        assert_eq!(input.copy_to_vec(1, 3), vec![2, 3, 4]);
    }

    #[test]
    fn chained_backward_access_after_forward_access_rescans() {
        let cs = chunks(&[&[1, 2], &[3, 4, 5], &[6]]);
        let input = CdrInput::chained(&cs, 0);
        assert_eq!(input.read_byte(5), 6);
        assert_eq!(input.read_byte(0), 1);
        assert_eq!(input.read_byte(3), 4);
        assert_eq!(input.copy_to_vec(0, 6), vec![1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn chained_repeated_reads_at_same_offset_stay_correct() {
        // A peek (is_at_sentinel) re-reads without advancing; the cursor cache
        // must return the same bytes both times.
        let cs = chunks(&[&[1, 2, 3], &[4, 5, 6]]);
        let input = CdrInput::chained(&cs, 0);
        assert_eq!(input.read_byte(4), 5);
        assert_eq!(input.read_byte(4), 5);
        let a: [u8; 2] = input.read_array(2);
        assert_eq!(a, [3, 4]);
        let a: [u8; 2] = input.read_array(2);
        assert_eq!(a, [3, 4]);
    }

    #[test]
    fn chained_empty_chunks_are_skipped() {
        let cs = chunks(&[&[1], &[], &[2, 3], &[], &[4]]);
        let input = CdrInput::chained(&cs, 0);
        assert_eq!(input.len(), 4);
        assert_eq!(input.copy_to_vec(0, 4), vec![1, 2, 3, 4]);
        assert_eq!(input.read_byte(1), 2);
        assert_eq!(input.read_byte(3), 4);
    }
}
