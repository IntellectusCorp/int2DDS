//! `TypeSupport::deserialize_chained` — the fragment receive path.
//!
//! The kernel vectors cover reading across chunks; what these cover is the
//! routing above it, which is derive-generated: which encapsulations decode
//! straight across the fragments and which reassemble first. Every route has to
//! land on the value the contiguous path produces, so each case decodes at every
//! chunk boundary and compares against `deserialize` on the whole payload.
//!
//! `format` is `None` throughout because that is what the receive path passes
//! (`dcps/subscription/data_sample.rs`).

#[allow(unused_imports)]
mod chained_tests {
    use int2dds::bytes::Bytes;
    use int2dds::dcps::topic::type_support::{DdsType, SerializationFormat, TypeSupport};
    use int2dds::serialize::xcdr::ExtensibilityKind;

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
    struct AppendableSample {
        pub id: u32,
        pub name: String,
        pub values: Vec<i64>,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Mutable")]
    struct MutableSample {
        pub id: u32,
        pub weight: f64,
        pub label: String,
    }

    fn xcdr(extensibility_kind: ExtensibilityKind) -> SerializationFormat {
        SerializationFormat::Xcdr { extensibility_kind, use_delimiters: true }
    }

    fn cut_at(data: &[u8], cut: usize) -> Vec<Bytes> {
        vec![Bytes::copy_from_slice(&data[..cut]), Bytes::copy_from_slice(&data[cut..])]
    }

    /// Decode `payload` at every chunk boundary and require each result to equal
    /// the contiguous decode.
    fn agrees_at_every_boundary<T>(ts: &dyn TypeSupport, payload: &[u8], expected: &T)
    where
        T: std::fmt::Debug + PartialEq + 'static,
    {
        let contiguous = ts.deserialize(payload, None).unwrap().downcast::<T>().unwrap();
        assert_eq!(&*contiguous, expected, "contiguous decode");

        for cut in 1..payload.len() {
            let chunks = cut_at(payload, cut);
            let value = ts
                .deserialize_chained(&chunks, None)
                .unwrap_or_else(|e| panic!("chunk boundary at {cut}: {e:?}"))
                .downcast::<T>()
                .expect("wrong type");
            assert_eq!(&*value, expected, "chunk boundary at {cut}");
        }
    }

    #[test]
    fn appendable_xcdr2_fragments_decode_as_one_buffer() {
        let value = AppendableSample {
            id: 0x1122_3344,
            name: "fragmented".to_string(),
            values: vec![-1, 2, -3],
        };
        let ts = AppendableSampleTypeSupport;
        let payload = ts
            .serialize(&value as &dyn std::any::Any, Some(&xcdr(ExtensibilityKind::Appendable)))
            .unwrap();
        assert_eq!(&payload[..2], &[0x00, 0x09], "XCDR2 appendable, little endian");

        agrees_at_every_boundary(&ts, &payload, &value);
    }

    /// Mutable puts an EMHEADER in front of every member, so the boundary sweep
    /// walks one across each header as well as each member.
    #[test]
    fn mutable_xcdr2_fragments_decode_as_one_buffer() {
        let value = MutableSample { id: 7, weight: 1.5, label: "member headers".to_string() };
        let ts = MutableSampleTypeSupport;
        let payload = ts
            .serialize(&value as &dyn std::any::Any, Some(&xcdr(ExtensibilityKind::Mutable)))
            .unwrap();
        assert_eq!(&payload[..2], &[0x00, 0x0B], "XCDR2 mutable, little endian");

        agrees_at_every_boundary(&ts, &payload, &value);
    }

    /// The classic CDR route is untouched by the XCDR2 one; keep it pinned so a
    /// later edit to the encapsulation gate cannot silently swap them.
    #[test]
    fn classic_cdr_fragments_still_decode_as_one_buffer() {
        let value = AppendableSample { id: 9, name: "classic".to_string(), values: vec![4, 5] };
        let ts = AppendableSampleTypeSupport;
        let payload =
            ts.serialize(&value as &dyn std::any::Any, Some(&SerializationFormat::Cdr)).unwrap();
        assert_eq!(&payload[..2], &[0x00, 0x01], "classic CDR, little endian");

        agrees_at_every_boundary(&ts, &payload, &value);
    }
}
