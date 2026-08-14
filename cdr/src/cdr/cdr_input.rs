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
// `cursor` is where the chunk containing the previous read ended up. Without it
// every read rescans the chunk list from the front, which costs O(chunks) per
// read: a payload read member by member then degrades in proportion to how many
// fragments it arrived in, and reassembling into one buffer wins instead. Reads
// advance monotonically, so resuming from the last chunk makes the scan O(1)
// amortized. It is only a hint — a read that lands before it rescans from the
// front, so a wrong cursor costs time and never bytes.
pub(crate) enum CdrInput<'a> {
    Contiguous(&'a [u8]),
    Chained { chunks: &'a [Bytes], skip: usize, len: usize, cursor: Cell<(usize, usize)> },
}

impl<'a> CdrInput<'a> {
    pub(crate) fn chained(chunks: &'a [Bytes], skip: usize) -> Self {
        // Summed once. Every read bounds-checks against this, so recomputing it
        // would be the same per-read walk over the chunks the cursor removes.
        let len = chunks.iter().map(|c| c.len()).sum::<usize>().saturating_sub(skip);
        CdrInput::Chained { chunks, skip, len, cursor: Cell::new((0, 0)) }
    }

    // Total body byte length (excludes the skipped header prefix).
    pub(crate) fn len(&self) -> usize {
        match self {
            CdrInput::Contiguous(data) => data.len(),
            CdrInput::Chained { len, .. } => *len,
        }
    }

    // Hand the caller the chunk pieces covering `len` bytes from body `offset`,
    // in order. One walk for every reader below. Callers guarantee the range is
    // in bounds (checked by `check_available` before the read).
    fn visit(&self, offset: usize, len: usize, mut piece: impl FnMut(&[u8])) {
        match self {
            CdrInput::Contiguous(data) => piece(&data[offset..offset + len]),
            CdrInput::Chained { chunks, skip, cursor, .. } => {
                let mut pos = offset + skip;
                let mut remaining = len;

                // Resume at the cached chunk when the read starts at or after it,
                // else start over.
                let (mut index, mut start) = cursor.get();
                if index >= chunks.len() || start > pos {
                    index = 0;
                    start = 0;
                }
                while index < chunks.len() && start + chunks[index].len() <= pos {
                    start += chunks[index].len();
                    index += 1;
                }

                while remaining > 0 {
                    let chunk = &chunks[index];
                    let within = pos - start;
                    let take = (chunk.len() - within).min(remaining);
                    piece(&chunk[within..within + take]);
                    pos += take;
                    remaining -= take;
                    // Step off this chunk only once it is spent, so the next read
                    // — usually still inside it — resumes here instead of rescanning.
                    if within + take == chunk.len() {
                        start += chunk.len();
                        index += 1;
                    }
                }
                cursor.set((index, start));
            }
        }
    }

    // Copy out.len() bytes starting at body offset into out. Caller guarantees
    // offset + out.len() <= len() (checked via check_available).
    pub(crate) fn copy_to(&self, offset: usize, out: &mut [u8]) {
        let mut written = 0;
        self.visit(offset, out.len(), |piece| {
            out[written..written + piece.len()].copy_from_slice(piece);
            written += piece.len();
        });
    }

    // Copy len bytes starting at offset into a fresh Vec without zero-filling.
    pub(crate) fn copy_to_vec(&self, offset: usize, len: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(len);
        self.visit(offset, len, |piece| out.extend_from_slice(piece));
        out
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
    #[cfg(test)]
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

    // The cursor makes sequential reads cheap, so it must not make any other
    // order wrong: reads that land before it have to rescan, not misread.
    #[test]
    fn chained_reads_do_not_depend_on_the_order_they_are_made_in() {
        let cs = chunks(&[&[1, 2], &[3, 4, 5], &[6, 7], &[8]]);
        let input = CdrInput::chained(&cs, 0);

        for offset in [0usize, 5, 2, 7, 1, 6, 0] {
            assert_eq!(input.copy_to_vec(offset, 1), vec![offset as u8 + 1], "offset {offset}");
        }
        assert_eq!(input.copy_to_vec(1, 6), vec![2, 3, 4, 5, 6, 7]);
        assert_eq!(input.copy_to_vec(0, 8), vec![1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(input.copy_to_vec(3, 2), vec![4, 5]);
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
}
