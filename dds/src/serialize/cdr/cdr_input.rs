use bytes::Bytes;
use std::borrow::Cow;

// Deserializer input backing store. Contiguous is the non-fragmented and writer
// path (single slice). Chained is the fragment receive path: the fragment chunks
// are kept as-is and reads that cross a chunk boundary are stitched on the fly,
// so no contiguous reassembly buffer is allocated per sample. `skip` hides a
// front prefix (the 4-byte encapsulation header) so positions stay body-relative,
// matching the Contiguous path which slices the header off.
pub(crate) enum CdrInput<'a> {
    Contiguous(&'a [u8]),
    Chained { chunks: &'a [Bytes], skip: usize },
}

impl<'a> CdrInput<'a> {
    // Total body byte length (excludes the skipped header prefix).
    pub(crate) fn len(&self) -> usize {
        match self {
            CdrInput::Contiguous(data) => data.len(),
            CdrInput::Chained { chunks, skip } => {
                chunks.iter().map(|c| c.len()).sum::<usize>() - skip
            }
        }
    }

    // Copy out.len() bytes starting at body offset into out. Caller guarantees
    // offset + out.len() <= len() (checked via check_available).
    pub(crate) fn copy_to(&self, offset: usize, out: &mut [u8]) {
        match self {
            CdrInput::Contiguous(data) => {
                out.copy_from_slice(&data[offset..offset + out.len()]);
            }
            CdrInput::Chained { chunks, skip } => {
                let mut written = 0;
                let mut pos = offset + skip;
                let mut chunk_start = 0;
                for chunk in chunks.iter() {
                    if written == out.len() {
                        break;
                    }
                    let chunk_end = chunk_start + chunk.len();
                    if pos < chunk_end {
                        let within = pos - chunk_start;
                        let take = (chunk.len() - within).min(out.len() - written);
                        out[written..written + take].copy_from_slice(&chunk[within..within + take]);
                        written += take;
                        pos += take;
                    }
                    chunk_start = chunk_end;
                }
            }
        }
    }

    // Copy len bytes starting at offset into a fresh Vec without zero-filling.
    pub(crate) fn copy_to_vec(&self, offset: usize, len: usize) -> Vec<u8> {
        match self {
            CdrInput::Contiguous(data) => data[offset..offset + len].to_vec(),
            CdrInput::Chained { chunks, skip } => {
                let mut out = Vec::with_capacity(len);
                let mut pos = offset + skip;
                let mut chunk_start = 0;
                for chunk in chunks.iter() {
                    if out.len() == len {
                        break;
                    }
                    let chunk_end = chunk_start + chunk.len();
                    if pos < chunk_end {
                        let within = pos - chunk_start;
                        let take = (chunk.len() - within).min(len - out.len());
                        out.extend_from_slice(&chunk[within..within + take]);
                        pos += take;
                    }
                    chunk_start = chunk_end;
                }
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
        let input = CdrInput::Chained { chunks: &cs, skip: 0 };
        // offset 2 spans the last byte of chunk 0 and the first two of chunk 1.
        let mut out = [0u8; 3];
        input.copy_to(2, &mut out);
        assert_eq!(out, [3, 4, 5]);
    }

    #[test]
    fn chained_copy_to_starting_at_chunk_boundary() {
        let cs = chunks(&[&[1, 2, 3], &[4, 5, 6]]);
        let input = CdrInput::Chained { chunks: &cs, skip: 0 };
        let mut out = [0u8; 2];
        input.copy_to(3, &mut out);
        assert_eq!(out, [4, 5]);
    }

    #[test]
    fn chained_read_array_crosses_boundary() {
        let cs = chunks(&[&[0x11, 0x22], &[0x33, 0x44]]);
        let input = CdrInput::Chained { chunks: &cs, skip: 0 };
        let a: [u8; 4] = input.read_array(0);
        assert_eq!(a, [0x11, 0x22, 0x33, 0x44]);
        let b: [u8; 2] = input.read_array(1);
        assert_eq!(b, [0x22, 0x33]);
    }

    #[test]
    fn chained_read_byte_finds_correct_chunk() {
        let cs = chunks(&[&[1, 2], &[3, 4, 5], &[6]]);
        let input = CdrInput::Chained { chunks: &cs, skip: 0 };
        assert_eq!(input.read_byte(0), 1);
        assert_eq!(input.read_byte(2), 3);
        assert_eq!(input.read_byte(4), 5);
        assert_eq!(input.read_byte(5), 6);
    }

    #[test]
    fn chained_copy_to_vec_spans_multiple_chunks() {
        let cs = chunks(&[&[1, 2], &[3, 4, 5], &[6]]);
        let input = CdrInput::Chained { chunks: &cs, skip: 0 };
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
        assert_eq!(CdrInput::Chained { chunks: &cs, skip: 0 }.len(), 6);
        let data = [0u8; 9];
        assert_eq!(CdrInput::Contiguous(&data).len(), 9);
    }

    #[test]
    fn chained_skip_hides_header_prefix() {
        // skip=4 models the encapsulation header living in the first chunk;
        // body offset 0 must map past it, and len reports body size only.
        let cs = chunks(&[&[0xAA, 0xBB, 0xCC, 0xDD, 1, 2], &[3, 4]]);
        let input = CdrInput::Chained { chunks: &cs, skip: 4 };
        assert_eq!(input.len(), 4);
        assert_eq!(input.read_byte(0), 1);
        let a: [u8; 4] = input.read_array(0);
        assert_eq!(a, [1, 2, 3, 4]);
        assert_eq!(input.copy_to_vec(1, 3), vec![2, 3, 4]);
    }
}
