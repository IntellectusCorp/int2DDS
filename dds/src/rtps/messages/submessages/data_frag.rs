//! DATAFRAG submessage for transmitting fragmented data samples.
//!
//! This module implements the DATAFRAG submessage used to transmit large data samples
//! that exceed the maximum transport message size. Samples are fragmented and
//! reassembled at the receiver.

use crate::rtps::common::time::RtpsTime;
use bytes::{Bytes, BytesMut};

use speedy::{Context, Error, Readable, Writable, Writer};
use std::io;
use std::time::Instant;

use crate::rtps::{
    common::{
        entity_id::EntityId,
        parameters::ParameterList,
        rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult},
        sequence::{FragmentNumber, SequenceNumber},
        types::SubmessagePayload,
    },
    messages::submessage_header::SubmessageHeader,
};

const EXTRA_FLAGS: u16 = 0; // 9.4.5.3.2 - extraFlags
const OCTETS_TO_INLINE_QOS: u16 = 28; // 9.4.5.3.3 - octetsToInlineQos

// Not spec text: no validity rule bounds total_fragments itself -- a
// self-consistent sample_size/fragment_size pair can still name far more
// fragments than any real sample needs (781 is the largest in this file's tests).
// Caps the fragment count; MAX_SAMPLE_BYTES below caps the bytes.
const MAX_FRAGMENTS_PER_SAMPLE: u32 = 1_048_576; // 2^20

// Bounds what FragmentBuffer allocates up front -- sample_size bytes, per matched
// reader. The fragment count cap above does not bound bytes at all.
const MAX_SAMPLE_BYTES: u32 = 32 * 1024 * 1024; // 32 MiB

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DataFrag<'a> {
    pub reader_id: EntityId,
    pub writer_id: EntityId,
    pub writer_sn: SequenceNumber,
    pub fragment_starting_num: FragmentNumber,
    pub fragments_in_submessage: u16,
    pub fragment_size: u16, // fragmentSize is the unit size determined by Writer when splitting the sample. This value must always be the same for the same Writer and the same sample
    pub sample_size: u32,
    inline_qos: Option<ParameterList>,
    serialized_data: SubmessagePayload<'a>,
}

impl<'a> DataFrag<'a> {
    pub(crate) fn new(
        reader_id: EntityId,
        writer_id: EntityId,
        writer_sn: SequenceNumber,
        fragment_starting_num: FragmentNumber,
        fragments_in_submessage: u16,
        fragment_size: u16,
        sample_size: u32,
    ) -> Self {
        Self {
            reader_id,
            writer_id,
            writer_sn,
            fragment_starting_num,
            fragments_in_submessage,
            fragment_size,
            sample_size,
            inline_qos: None,
            serialized_data: SubmessagePayload::default(),
        }
    }

    pub(crate) fn add_serialized_data(&mut self, serialized_data: SubmessagePayload<'a>) {
        self.serialized_data = serialized_data;
    }

    /// Bytes of padding needed to end this submessage on a 4-byte boundary.
    ///
    /// RTPS 2.5 - 9.4.1: submessages begin on 4-byte boundaries, and 9.4.5.1.3 defines
    /// octetsToNextHeader as the distance to the header of the *next* submessage. A
    /// receiver walks that chain, so a body left at an odd length puts it on a misaligned
    /// offset. Cyclone DDS rejects the whole datagram as malformed rather than reading
    /// misaligned data (`ddsi_receive.c`, "not 0 mod 4 and yet also not the number of
    /// octets remaining").
    ///
    /// Every fragment but the last carries exactly `fragment_size` bytes, and fragment
    /// sizes are multiples of four in practice, so this only bites on the final fragment
    /// of a sample whose size is not a multiple of the fragment size - and then the sample
    /// never completes, because that one fragment is dropped on every retry. `Data` has
    /// always padded; DATA_FRAG did not.
    fn alignment_padding(payload_len: usize) -> u16 {
        let rem = payload_len % 4;
        if rem == 0 {
            0
        } else {
            (4 - rem) as u16
        }
    }

    /// Length of the body as written, excluding any alignment padding.
    fn unpadded_body_length(&self) -> u16 {
        2  /* extra_flags */
         + 2  /* octets_to_inline_qos */
         + 4  /* reader_id */
         + 4  /* writer_id */
         + 8  /* writer_sn */
         + 4  /* fragment_starting_num */
         + 2  /* fragments_in_submessage */
         + 2  /* fragment_size */
         + 4  /* sample_size */
         + match &self.inline_qos {
            Some(param_list) => {
                param_list.length() as u16
            },
            None => 0,
         }  /* inline_qos */
         + self.serialized_data.len() as u16
    }

    pub(crate) fn octets_to_next_header(&self) -> u16 {
        let payload_len = self.unpadded_body_length();
        payload_len + Self::alignment_padding(payload_len as usize)
    }

    /// Return the payload as `Bytes` for zero-copy sub-slicing on the receive path.
    ///
    /// Returns `None` if the payload is `Borrowed` (only happens on the send
    /// path, where the caller would not need shared ownership anyway).
    pub(crate) fn serialized_bytes(&self) -> Option<Bytes> {
        match &self.serialized_data {
            SubmessagePayload::Owned(b) => Some(b.clone()),
            SubmessagePayload::Borrowed(_) => None,
        }
    }

    pub(crate) fn deserialize(
        buffer: &Bytes,
        submessage_header: &SubmessageHeader,
    ) -> RtpsResult<Self> {
        let mut cursor = io::Cursor::new(&buffer);
        let map_speedy_err = |p: Error| RtpsError::new(RtpsErrorCode::Io, p.to_string());

        // Extract flags from the submessage header
        let endianness = submessage_header
            .endianness_flag()
            .ok_or_else(|| RtpsError::new(RtpsErrorCode::UnsupportedSubmessageType, None))?;
        let inline_qos_flag = submessage_header
            .inline_qos_flag()
            .ok_or_else(|| RtpsError::new(RtpsErrorCode::UnsupportedSubmessageType, None))?;

        let _extra_flags = u16::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
            .map_err(map_speedy_err)?;
        let octets_to_inline_qos =
            u16::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
                .map_err(map_speedy_err)?;
        let reader_id = EntityId::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
            .map_err(map_speedy_err)?;
        let writer_id = EntityId::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
            .map_err(map_speedy_err)?;
        let writer_sn =
            SequenceNumber::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
                .map_err(map_speedy_err)?;

        // 8.3.7.3.3
        if writer_sn.to_i64() <= 0 || writer_sn == SequenceNumber::UNKNOWN {
            return Err(RtpsError::new(
                RtpsErrorCode::InvalidSubmessageBody,
                "writerSN value should be strictly positive or not SEQUENCENUMBER_UNKNOWN",
            ));
        }

        let fragment_starting_num =
            FragmentNumber::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
                .map_err(map_speedy_err)?;
        let fragments_in_submessage =
            u16::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
                .map_err(map_speedy_err)?;
        let fragment_size = u16::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
            .map_err(map_speedy_err)?;
        let sample_size = u32::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
            .map_err(map_speedy_err)?;

        // 9.4.5.3.3 - should always use the octetsToInlineQos to skip any submessage headers it does not expect or understand
        cursor.set_position(
            4 + octets_to_inline_qos as u64, // extraFlags + octetsToInlineQos
        );

        let inline_qos = if inline_qos_flag {
            Some(
                ParameterList::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
                    .map_err(map_speedy_err)?,
            )
        } else {
            None
        };

        let start_pos = cursor.position() as usize;
        let serialized_data_bytes = buffer.slice(start_pos..);

        // 8.3.7.3.3 Validity
        // allow up to 3 extra bytes for RTPS submessage 4-byte alignment padding.
        // The last fragment range may be shorter than `fragment_size`, so bound
        // the expected data length by `sample_size`, not only by
        // `fragments_in_submessage * fragment_size`.
        if fragment_starting_num == 0 || fragments_in_submessage == 0 || fragment_size == 0 {
            return Err(RtpsError::new(
                RtpsErrorCode::InvalidSubmessageBody,
                "Invalid DATA_FRAG fragment numbering or size",
            ));
        }

        // 8.3.7.3.3 Validity: "fragmentSize exceeds dataSize" is invalid. Compare
        // as u32 -- narrowing sample_size to u16 first (the disabled form of this
        // check) truncated it, so any sample_size whose low 16 bits happened to
        // fall below fragment_size rejected an otherwise-legitimate fragment.
        if fragment_size as u32 > sample_size {
            return Err(RtpsError::new(
                RtpsErrorCode::InvalidSubmessageBody,
                "Fragment size exceeds sample size",
            ));
        }

        // The spec's own formula (8.3.7.3.3 Logical Interpretation) -- and the
        // exact quantity MAX_FRAGMENTS_PER_SAMPLE bounds, above.
        let total_fragments_num = (sample_size / fragment_size as u32)
            + if !sample_size.is_multiple_of(fragment_size as u32) { 1 } else { 0 };
        if total_fragments_num > MAX_FRAGMENTS_PER_SAMPLE {
            return Err(RtpsError::new(
                RtpsErrorCode::InvalidSubmessageBody,
                "Total fragment count exceeds the maximum allowed for a single sample",
            ));
        }

        if sample_size > MAX_SAMPLE_BYTES {
            return Err(RtpsError::new(
                RtpsErrorCode::InvalidSubmessageBody,
                "Sample size exceeds the maximum allowed for a fragmented sample",
            ));
        }

        // Also enforces 8.3.7.3.3's "fragmentStartingNum ... exceeds the total
        // number of fragments": for fragment_size > 0, offset >= sample_size
        // holds exactly when fragment_starting_num > total_fragments_num.
        let fragment_start_offset = (fragment_starting_num - 1) as usize * fragment_size as usize;
        if fragment_start_offset >= sample_size as usize {
            return Err(RtpsError::new(
                RtpsErrorCode::InvalidSubmessageBody,
                "FragmentStartingNum exceeds sample_size",
            ));
        }

        let max_fragment_data_size = fragments_in_submessage as usize * fragment_size as usize;
        let remaining_sample_size = sample_size as usize - fragment_start_offset;
        let expected_data_size = std::cmp::min(max_fragment_data_size, remaining_sample_size);
        if serialized_data_bytes.len() > expected_data_size + 3 {
            return Err(RtpsError::new(
                RtpsErrorCode::InvalidSubmessageBody,
                "Serialized data size exceeds the expected size based on fragments_in_submessage and fragment_size",
            ));
        }

        // truncate padding bytes - only keep actual fragment data.
        // `serialized_data_bytes` is already a `Bytes` obtained via
        // `buffer.slice(start_pos..)`, so slicing it further is a zero-copy
        // refcount bump on the same backing allocation.
        let actual_len = std::cmp::min(serialized_data_bytes.len(), expected_data_size);

        // All but the last claimed fragment must be fully present, or the
        // receive path's per-fragment slicing reads past the payload end.
        // fragments_in_submessage == 1 makes this 0, so a short final
        // fragment sent alone is never rejected.
        let min_data_size = (fragments_in_submessage as usize - 1) * fragment_size as usize;
        if actual_len <= min_data_size {
            return Err(RtpsError::new(
                RtpsErrorCode::InvalidSubmessageBody,
                "Serialized data size is too small to reach the last claimed fragment",
            ));
        }

        let serialized_data = SubmessagePayload::Owned(serialized_data_bytes.slice(..actual_len));

        Ok(Self {
            reader_id,
            writer_id,
            writer_sn,
            fragment_starting_num,
            fragments_in_submessage,
            fragment_size,
            sample_size,
            inline_qos,
            serialized_data,
        })
    }
}

impl<C: Context> Writable<C> for DataFrag<'_> {
    fn write_to<T: ?Sized + Writer<C>>(&self, writer: &mut T) -> Result<(), C::Error> {
        writer.write_u16(EXTRA_FLAGS)?;
        writer.write_u16(OCTETS_TO_INLINE_QOS)?;
        writer.write_value(&self.reader_id)?;
        writer.write_value(&self.writer_id)?;
        writer.write_value(&self.writer_sn)?;
        writer.write_value(&self.fragment_starting_num)?;
        writer.write_value(&self.fragments_in_submessage)?;
        writer.write_value(&self.fragment_size)?;
        writer.write_value(&self.sample_size)?;
        if let Some(ref inline_qos) = self.inline_qos {
            writer.write_value(inline_qos)?;
        }
        writer.write_bytes(self.serialized_data.as_slice())?;
        let padding = Self::alignment_padding(self.unpadded_body_length() as usize);
        if padding > 0 {
            writer.write_bytes(&vec![0u8; padding as usize])?;
        }

        Ok(())
    }
}

#[allow(dead_code)]
#[derive(Debug)]
pub(crate) struct FragmentBuffer {
    pub sequence_number: SequenceNumber,
    pub total_size: u32,
    // One contiguous buffer for the whole sample. Fragments are written at their
    // offset, so reassembly costs one allocation instead of one per fragment.
    pub buf: BytesMut,
    // Which fragment slots have arrived, so retransmits do not over-count.
    pub filled: Vec<bool>,
    // number of filled slots, for completion check
    pub received_count: u32,
    pub total_fragments: u32,
    pub fragment_size: u16,
    pub source_timestamp: Option<RtpsTime>,
    pub created_at: Instant,
    pub last_updated: Instant,
}

#[allow(dead_code)]
impl FragmentBuffer {
    pub(crate) fn new(
        sequence_number: SequenceNumber,
        total_size: u32,
        fragment_size: u16,
    ) -> Self {
        let total_fragments = (total_size / fragment_size as u32)
            + if !total_size.is_multiple_of(fragment_size as u32) { 1 } else { 0 };
        let now = Instant::now();

        Self {
            sequence_number,
            total_size,
            buf: {
                let mut b = BytesMut::with_capacity(total_size as usize);
                b.resize(total_size as usize, 0);
                b
            },
            filled: vec![false; total_fragments as usize],
            received_count: 0,
            total_fragments,
            fragment_size,
            source_timestamp: None,
            created_at: now,
            last_updated: now,
        }
    }

    pub(crate) fn all_fragments_received(&self) -> bool {
        self.received_count == self.total_fragments
    }

    // Write a fragment into its place in the sample buffer. Copying here detaches
    // the receive arena, whose chunk would otherwise stay resident until delivery.
    pub(crate) fn copy_fragment_data(&mut self, fragment_num: u32, data: Bytes) -> bool {
        if fragment_num == 0 || fragment_num > self.total_fragments {
            return false;
        }

        // Validate fragment data size
        let expected_max = self.fragment_size as usize;
        let remaining =
            self.total_size as usize - ((fragment_num - 1) * self.fragment_size as u32) as usize;
        let expected_size = std::cmp::min(expected_max, remaining);
        // Exact, not at-most: a short fragment would still mark its slot filled and
        // leave a zeroed hole that deserializes as valid data. RTPS 8.3.7.3.
        if data.len() != expected_size {
            return false;
        }

        let idx = (fragment_num - 1) as usize;
        let off = idx * self.fragment_size as usize;
        self.buf[off..off + data.len()].copy_from_slice(&data);
        if !self.filled[idx] {
            self.filled[idx] = true;
            self.received_count += 1;
        }
        self.last_updated = Instant::now();
        true
    }

    // The assembled sample. Fragments were written in place, so this hands the
    // buffer over without another copy.
    pub(crate) fn assemble(self) -> Vec<u8> {
        self.buf.to_vec()
    }

    pub(crate) fn into_bytes(self) -> Bytes {
        self.buf.freeze()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtps::{
        common::{entity_id::EntityId, entity_kind::EntityKind, sequence::SequenceNumber},
        messages::{submessage_header::SubmessageHeader, submessage_id::SubmessageId},
    };

    use speedy::{Endianness, Writable};

    fn create_dummy_datafrag() -> DataFrag<'static> {
        DataFrag {
            reader_id: EntityId::new([0x01, 0x00, 0x00], EntityKind::USER_DEFINED_READER_NO_KEY),
            writer_id: EntityId::new([0x02, 0x00, 0x00], EntityKind::USER_DEFINED_WRITER_NO_KEY),
            writer_sn: SequenceNumber::new(0, 1),
            fragment_starting_num: 1,
            fragments_in_submessage: 1,
            fragment_size: 1,
            sample_size: 2,
            inline_qos: None,
            serialized_data: SubmessagePayload::Owned(Bytes::from_static(&[0u8])),
        }
    }

    fn create_dummy_submessage_header(length: u16) -> SubmessageHeader {
        SubmessageHeader::new(
            SubmessageId::DATA_FRAG,
            0,
            length, // submessage_length
        )
    }

    #[test]
    fn test_datafrag_serialization_roundtrip() {
        let datafrag = create_dummy_datafrag();

        let buffer = datafrag.write_to_vec_with_ctx(Endianness::BigEndian).unwrap();

        let header = create_dummy_submessage_header(buffer.len() as u16);

        let result = DataFrag::deserialize(&Bytes::from(buffer.to_vec()), &header);
        assert!(result.is_ok());

        let deserialized = result.unwrap();
        assert_eq!(datafrag.reader_id, deserialized.reader_id);
        assert_eq!(datafrag.writer_id, deserialized.writer_id);
        assert_eq!(datafrag.writer_sn, deserialized.writer_sn);
        assert_eq!(datafrag.fragment_starting_num, deserialized.fragment_starting_num);
        assert_eq!(datafrag.fragments_in_submessage, deserialized.fragments_in_submessage);
        assert_eq!(datafrag.fragment_size, deserialized.fragment_size);
        assert_eq!(datafrag.sample_size, deserialized.sample_size);
        assert_eq!(datafrag.inline_qos, deserialized.inline_qos);
        assert_eq!(datafrag.serialized_data.as_slice(), deserialized.serialized_data.as_slice());
    }

    #[test]
    fn test_datafrag_invalid_sequence_number() {
        let mut datafrag = create_dummy_datafrag();
        datafrag.writer_sn = SequenceNumber::UNKNOWN;

        let buffer = datafrag.write_to_vec_with_ctx(Endianness::BigEndian).unwrap();

        let header = create_dummy_submessage_header(buffer.len() as u16);

        let result = DataFrag::deserialize(&Bytes::from(buffer.to_vec()), &header);
        assert!(result.is_err());
        if let Err(err) = result {
            assert_eq!(err.code, RtpsErrorCode::InvalidSubmessageBody);
        }
    }

    // A hostile sample_size/fragment_size pair is internally self-consistent --
    // fragment_size (1) does not exceed sample_size (u32::MAX), so the spec's own
    // "fragmentSize exceeds dataSize" rule does not catch it -- but it would ask
    // FragmentBuffer::new to track 4,294,967,295 fragments of a 4 GiB sample from
    // a ~36-byte wire message carrying a single real byte of payload.
    #[test]
    fn test_datafrag_rejects_sample_size_that_would_demand_a_huge_fragment_count() {
        let mut datafrag = create_dummy_datafrag();
        datafrag.sample_size = u32::MAX; // fragment_size stays 1, from the dummy

        let buffer = datafrag.write_to_vec_with_ctx(Endianness::BigEndian).unwrap();
        let header = create_dummy_submessage_header(buffer.len() as u16);

        let result = DataFrag::deserialize(&Bytes::from(buffer.to_vec()), &header);
        assert!(result.is_err());
        if let Err(err) = result {
            assert_eq!(err.code, RtpsErrorCode::InvalidSubmessageBody);
        }
    }

    // A sample_size can clear the fragment-count cap and still be far too large to
    // hold: 32 MiB + 1 with fragment_size 64 names 524,289 fragments, well under
    // MAX_FRAGMENTS_PER_SAMPLE, yet asks for a 32 MiB buffer per matched reader.
    #[test]
    fn test_datafrag_rejects_sample_size_over_the_byte_cap() {
        let mut datafrag = create_dummy_datafrag();
        datafrag.fragment_size = 64;
        datafrag.sample_size = MAX_SAMPLE_BYTES + 1;
        datafrag.serialized_data = SubmessagePayload::Owned(Bytes::from(vec![0xAB; 64]));

        let buffer = datafrag.write_to_vec_with_ctx(Endianness::BigEndian).unwrap();
        let header = create_dummy_submessage_header(buffer.len() as u16);

        let result = DataFrag::deserialize(&Bytes::from(buffer.to_vec()), &header);
        assert!(result.is_err());
        if let Err(err) = result {
            assert_eq!(err.code, RtpsErrorCode::InvalidSubmessageBody);
        }
    }

    // The byte cap must not reject a sample that is merely large but legal.
    #[test]
    fn test_datafrag_accepts_sample_size_at_the_byte_cap() {
        let mut datafrag = create_dummy_datafrag();
        datafrag.fragment_size = 64;
        datafrag.sample_size = MAX_SAMPLE_BYTES;
        datafrag.serialized_data = SubmessagePayload::Owned(Bytes::from(vec![0xAB; 64]));

        let buffer = datafrag.write_to_vec_with_ctx(Endianness::BigEndian).unwrap();
        let header = create_dummy_submessage_header(buffer.len() as u16);

        assert!(DataFrag::deserialize(&Bytes::from(buffer.to_vec()), &header).is_ok());
    }

    // 1,048,576 is a multiple of 65,536, so truncating it to u16 (the disabled
    // check's original cast) gives 0: fragment_size(1344) > 0 would have wrongly
    // rejected this legitimate fragment. The fixed, non-truncating comparison
    // must not reject it.
    #[test]
    fn test_datafrag_accepts_large_sample_size_that_a_truncating_cast_would_zero() {
        let mut datafrag = create_dummy_datafrag();
        datafrag.fragment_size = 1344;
        datafrag.sample_size = 1_048_576;
        datafrag.serialized_data = SubmessagePayload::Owned(Bytes::from(vec![0xAB; 1344]));

        let buffer = datafrag.write_to_vec_with_ctx(Endianness::BigEndian).unwrap();
        let header = create_dummy_submessage_header(buffer.len() as u16);

        let result = DataFrag::deserialize(&Bytes::from(buffer.to_vec()), &header);
        assert!(result.is_ok());
    }

    #[test]
    fn test_datafrag_invalid_serialized_data_too_large() {
        let mut datafrag = create_dummy_datafrag();
        datafrag.fragments_in_submessage = 1;
        datafrag.fragment_size = 3;
        datafrag.sample_size = 3;

        // serialized data of 7 bytes when only 3 (+3 padding max) are expected
        datafrag.serialized_data = SubmessagePayload::Owned(Bytes::from(vec![1, 2, 3, 4, 5, 6, 7]));

        let buffer = datafrag.write_to_vec_with_ctx(Endianness::BigEndian).unwrap();

        let header = create_dummy_submessage_header(buffer.len() as u16);

        let result = DataFrag::deserialize(&Bytes::from(buffer.to_vec()), &header);
        assert!(result.is_err());
        if let Err(err) = result {
            assert_eq!(err.code, RtpsErrorCode::InvalidSubmessageBody);
        }
    }

    #[test]
    fn test_datafrag_rejects_payload_too_small_for_claimed_fragment_count() {
        let mut datafrag = create_dummy_datafrag();
        datafrag.fragment_starting_num = 1;
        datafrag.fragments_in_submessage = 2;
        datafrag.fragment_size = 8;
        datafrag.sample_size = 32;

        // Claims 2 fragments of 8 bytes each but only carries 3 bytes: not
        // even enough to fill fragment 1, let alone reach fragment 2.
        datafrag.serialized_data = SubmessagePayload::Owned(Bytes::from(vec![1, 2, 3]));

        let buffer = datafrag.write_to_vec_with_ctx(Endianness::BigEndian).unwrap();
        let header = create_dummy_submessage_header(buffer.len() as u16);

        let result = DataFrag::deserialize(&Bytes::from(buffer), &header);
        assert!(result.is_err());
        if let Err(err) = result {
            assert_eq!(err.code, RtpsErrorCode::InvalidSubmessageBody);
        }
    }

    // fragments_in_submessage can overclaim past the sample's remaining bytes;
    // sample-size clamping then truncates the stored payload below the
    // minimum even though the raw wire length looked large enough.
    #[test]
    fn test_datafrag_rejects_overclaimed_fragment_count_after_sample_size_truncation() {
        let mut datafrag = create_dummy_datafrag();
        datafrag.fragment_starting_num = 1;
        datafrag.fragments_in_submessage = 5; // claims 5 fragments (40 bytes)...
        datafrag.fragment_size = 8;
        datafrag.sample_size = 30; // ...but the sample only has room for 4

        // 33 bytes passes the too-large check, but truncates to 30 bytes
        // stored - short of the 32 needed to reach fragment 5.
        datafrag.serialized_data = SubmessagePayload::Owned(Bytes::from(vec![0xAB; 33]));

        let buffer = datafrag.write_to_vec_with_ctx(Endianness::BigEndian).unwrap();
        let header = create_dummy_submessage_header(buffer.len() as u16);

        let result = DataFrag::deserialize(&Bytes::from(buffer), &header);
        assert!(result.is_err());
        if let Err(err) = result {
            assert_eq!(err.code, RtpsErrorCode::InvalidSubmessageBody);
        }
    }

    #[test]
    fn test_datafrag_truncates_alignment_padding_on_last_fragment() {
        let mut datafrag = create_dummy_datafrag();
        datafrag.fragment_starting_num = 2;
        datafrag.fragments_in_submessage = 1;
        datafrag.fragment_size = 3;
        datafrag.sample_size = 5;
        datafrag.serialized_data = SubmessagePayload::Owned(Bytes::from(vec![10, 11, 0, 0]));

        let buffer = datafrag.write_to_vec_with_ctx(Endianness::BigEndian).unwrap();
        let header = create_dummy_submessage_header(buffer.len() as u16);

        let deserialized = DataFrag::deserialize(&Bytes::from(buffer), &header).unwrap();
        assert_eq!(deserialized.serialized_data.as_slice(), &[10, 11]);
    }

    // total_size 5, fragment_size 2 -> 3 fragments: [_,_][_,_][_]
    fn three_fragment_buffer() -> FragmentBuffer {
        FragmentBuffer::new(SequenceNumber::new(0, 1), 5, 2)
    }

    #[test]
    fn test_fragment_buffer_assembles_in_fragment_order() {
        let mut buffer = three_fragment_buffer();
        // Insert out of order with distinct bytes per fragment
        assert!(buffer.copy_fragment_data(3, Bytes::from_static(&[30])));
        assert!(buffer.copy_fragment_data(1, Bytes::from_static(&[10, 11])));
        assert!(buffer.copy_fragment_data(2, Bytes::from_static(&[20, 21])));

        assert!(buffer.all_fragments_received());
        assert_eq!(buffer.assemble(), vec![10, 11, 20, 21, 30]);
    }

    #[test]
    fn test_fragment_buffer_into_bytes_in_fragment_order() {
        let mut buffer = three_fragment_buffer();
        buffer.copy_fragment_data(3, Bytes::from_static(&[30]));
        buffer.copy_fragment_data(1, Bytes::from_static(&[10, 11]));
        buffer.copy_fragment_data(2, Bytes::from_static(&[20, 21]));

        // Fragments land at their offset, so arrival order does not matter.
        assert_eq!(&buffer.into_bytes()[..], &[10, 11, 20, 21, 30]);
    }

    #[test]
    fn test_fragment_buffer_incomplete_until_all_received() {
        let mut buffer = three_fragment_buffer();
        assert!(buffer.copy_fragment_data(1, Bytes::from_static(&[10, 11])));
        assert!(!buffer.all_fragments_received());
        assert!(buffer.copy_fragment_data(2, Bytes::from_static(&[20, 21])));
        assert!(!buffer.all_fragments_received());
        assert!(buffer.copy_fragment_data(3, Bytes::from_static(&[30])));
        assert!(buffer.all_fragments_received());
    }

    #[test]
    fn test_fragment_buffer_duplicate_does_not_complete() {
        let mut buffer = three_fragment_buffer();
        buffer.copy_fragment_data(1, Bytes::from_static(&[10, 11]));
        buffer.copy_fragment_data(1, Bytes::from_static(&[10, 11])); // duplicate
        buffer.copy_fragment_data(2, Bytes::from_static(&[20, 21]));
        // Only 2 distinct fragments; fragment 3 still missing
        assert!(!buffer.all_fragments_received());
    }

    #[test]
    fn test_fragment_buffer_rejects_a_short_interior_fragment() {
        let mut buffer = three_fragment_buffer();
        // Fragment 1 must carry a full fragment_size (2). One byte would mark the
        // slot filled and leave buf[1] zeroed, which deserializes as real data.
        assert!(!buffer.copy_fragment_data(1, Bytes::from_static(&[10])));
        assert_eq!(buffer.received_count, 0);
        // The sample's own last fragment is legitimately short (total_size 5).
        assert!(buffer.copy_fragment_data(3, Bytes::from_static(&[30])));
        assert_eq!(buffer.received_count, 1);
    }

    #[test]
    fn test_fragment_buffer_rejects_out_of_range() {
        let mut buffer = three_fragment_buffer();
        assert!(!buffer.copy_fragment_data(0, Bytes::from_static(&[0])));
        assert!(!buffer.copy_fragment_data(4, Bytes::from_static(&[0])));
    }

    #[test]
    fn test_fragment_buffer_rejects_oversized() {
        let mut buffer = three_fragment_buffer();
        // fragment 1 allows at most fragment_size (2) bytes
        assert!(!buffer.copy_fragment_data(1, Bytes::from_static(&[1, 2, 3])));
    }

    /// Build the final fragment of a sample, which is the only one whose payload is not a
    /// whole `fragment_size`.
    fn final_fragment(sample_size: u32, fragment_size: u16) -> DataFrag<'static> {
        let total = sample_size.div_ceil(fragment_size as u32);
        let remainder = sample_size - (total - 1) * fragment_size as u32;
        DataFrag {
            fragment_starting_num: total,
            fragment_size,
            sample_size,
            serialized_data: SubmessagePayload::Owned(Bytes::from(vec![0xAB; remainder as usize])),
            ..create_dummy_datafrag()
        }
    }

    // A receiver locates the next submessage by adding octetsToNextHeader to the current
    // one, so it has to be a multiple of four whenever another submessage follows - and
    // int2DDS always follows DATA_FRAG with a piggyback HEARTBEAT.
    #[test]
    fn octets_to_next_header_is_four_byte_aligned_for_a_ragged_final_fragment() {
        // The Autoware /map/vector_map case: 604663 bytes at 1344 per fragment leaves a
        // 1207-byte fragment 450, which used to declare octetsToNextHeader = 1239.
        let data_frag = final_fragment(604_663, 1344);

        assert_eq!(data_frag.fragment_starting_num, 450);
        assert_eq!(data_frag.serialized_data.len(), 1207);
        assert_eq!(data_frag.octets_to_next_header(), 1240);
        assert_eq!(data_frag.octets_to_next_header() % 4, 0);
    }

    // The declared length has to match the bytes actually written, or the padding shifts
    // the next submessage instead of aligning it.
    #[test]
    fn a_padded_data_frag_writes_exactly_what_it_declares() {
        let data_frag = final_fragment(604_663, 1344);
        let declared = data_frag.octets_to_next_header();

        let written = data_frag.write_to_vec_with_ctx(Endianness::LittleEndian).unwrap();

        assert_eq!(written.len(), declared as usize);
        assert_eq!(written.len(), 1207 + 33, "32 fixed fields + payload + 1 pad byte");
        assert_eq!(&written[written.len() - 1..], &[0u8], "padding is zero-filled");
    }

    // An already-aligned fragment must not grow, or every fragment on the wire changes.
    #[test]
    fn an_aligned_fragment_is_not_padded() {
        let data_frag = final_fragment(604_664, 1344); // remainder 1208, already a multiple of 4
        assert_eq!(data_frag.serialized_data.len(), 1208);
        assert_eq!(data_frag.octets_to_next_header(), 1240);

        let written = data_frag.write_to_vec_with_ctx(Endianness::LittleEndian).unwrap();
        assert_eq!(written.len(), 1240);
    }

    // The padding the sender adds has to survive the round trip: deserialize trims it back
    // to the real fragment length, so reassembly still sees exactly the sample's bytes.
    #[test]
    fn padding_is_trimmed_again_on_deserialize() {
        let data_frag = final_fragment(604_663, 1344);
        let written = data_frag.write_to_vec_with_ctx(Endianness::LittleEndian).unwrap();
        let header = SubmessageHeader::new(SubmessageId::DATA_FRAG, 0x01, written.len() as u16);

        let parsed = DataFrag::deserialize(&Bytes::from(written), &header).unwrap();

        assert_eq!(parsed.serialized_data.len(), 1207);
        assert_eq!(parsed.fragment_starting_num, 450);
    }

    // A short final fragment sent alone must still be accepted:
    // fragments_in_submessage == 1 makes the minimum zero.
    #[test]
    fn test_datafrag_accepts_short_final_fragment_sent_alone() {
        // 1,048,576 bytes at 1,344-byte fragments leaves a 256-byte fragment 781.
        let data_frag = final_fragment(1_048_576, 1344);
        assert_eq!(data_frag.fragment_starting_num, 781);
        assert_eq!(data_frag.serialized_data.len(), 256);

        let written = data_frag.write_to_vec_with_ctx(Endianness::BigEndian).unwrap();
        let header = create_dummy_submessage_header(written.len() as u16);

        let parsed = DataFrag::deserialize(&Bytes::from(written), &header).unwrap();
        assert_eq!(parsed.serialized_data.len(), 256);
    }

    // A packed run that ends at the sample's last fragment is also legitimately
    // short: it carries full-size fragments for everything but the last one.
    #[test]
    fn test_datafrag_accepts_packed_run_ending_at_final_fragment() {
        // Same sample as above, packed as fragments 769..=781 (13 fragments):
        // 12 full fragments plus the 256-byte final one, not 13 full ones.
        let mut datafrag = create_dummy_datafrag();
        datafrag.fragment_starting_num = 769;
        datafrag.fragments_in_submessage = 13;
        datafrag.fragment_size = 1344;
        datafrag.sample_size = 1_048_576;
        let payload_len = 12 * 1344 + 256;
        datafrag.serialized_data = SubmessagePayload::Owned(Bytes::from(vec![0xAB; payload_len]));

        let buffer = datafrag.write_to_vec_with_ctx(Endianness::BigEndian).unwrap();
        let header = create_dummy_submessage_header(buffer.len() as u16);

        let parsed = DataFrag::deserialize(&Bytes::from(buffer), &header).unwrap();
        assert_eq!(parsed.serialized_data.len(), payload_len);
    }
}
