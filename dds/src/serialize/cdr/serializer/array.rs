use super::string::StringSerialize;
use super::CdrSerializerCommon;
use crate::serialize::cdr::{CdrError, CdrSerializer, Xcdr2Serializer};
use crate::serialize::{
    to_bytes_f32, to_bytes_f64, to_bytes_i16, to_bytes_i32, to_bytes_i64, to_bytes_u16,
    to_bytes_u32, to_bytes_u64,
};

/// Trait for fixed-size array serialization (no length prefix)
/// Provides default implementations that work for both CdrSerializer and Xcdr2Serializer
pub trait ArraySerialize: CdrSerializerCommon + StringSerialize {
    /// Serialize fixed-size byte array (no length prefix)
    fn serialize_byte_array(&mut self, data: &[u8]) -> Result<(), CdrError> {
        self.buffer_mut().extend_from_slice(data);
        Ok(())
    }

    /// Serialize fixed-size u16 array (no length prefix)
    fn serialize_u16_array(&mut self, data: &[u16]) -> Result<(), CdrError> {
        self.align(2);
        for &value in data {
            let bytes = to_bytes_u16(value, self.endianness());
            self.buffer_mut().extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize fixed-size u32 array (no length prefix)
    fn serialize_u32_array(&mut self, data: &[u32]) -> Result<(), CdrError> {
        self.align(4);
        for &value in data {
            let bytes = to_bytes_u32(value, self.endianness());
            self.buffer_mut().extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize fixed-size u64 array (no length prefix)
    fn serialize_u64_array(&mut self, data: &[u64]) -> Result<(), CdrError> {
        self.align(8);
        for &value in data {
            let bytes = to_bytes_u64(value, self.endianness());
            self.buffer_mut().extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize fixed-size i8 array (no length prefix)
    fn serialize_i8_array(&mut self, data: &[i8]) -> Result<(), CdrError> {
        for &value in data {
            self.buffer_mut().push(value as u8);
        }
        Ok(())
    }

    /// Serialize fixed-size i16 array (no length prefix)
    fn serialize_i16_array(&mut self, data: &[i16]) -> Result<(), CdrError> {
        self.align(2);
        for &value in data {
            let bytes = to_bytes_i16(value, self.endianness());
            self.buffer_mut().extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize fixed-size i32 array (no length prefix)
    fn serialize_i32_array(&mut self, data: &[i32]) -> Result<(), CdrError> {
        self.align(4);
        for &value in data {
            let bytes = to_bytes_i32(value, self.endianness());
            self.buffer_mut().extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize fixed-size i64 array (no length prefix)
    fn serialize_i64_array(&mut self, data: &[i64]) -> Result<(), CdrError> {
        self.align(8);
        for &value in data {
            let bytes = to_bytes_i64(value, self.endianness());
            self.buffer_mut().extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize fixed-size f32 array (no length prefix)
    fn serialize_f32_array(&mut self, data: &[f32]) -> Result<(), CdrError> {
        self.align(4);
        for &value in data {
            let bytes = to_bytes_f32(value, self.endianness());
            self.buffer_mut().extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize fixed-size f64 array (no length prefix)
    fn serialize_f64_array(&mut self, data: &[f64]) -> Result<(), CdrError> {
        self.align(8);
        for &value in data {
            let bytes = to_bytes_f64(value, self.endianness());
            self.buffer_mut().extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize fixed-size bool array (no length prefix)
    fn serialize_bool_array(&mut self, data: &[bool]) -> Result<(), CdrError> {
        for &value in data {
            self.buffer_mut().push(if value { 1 } else { 0 });
        }
        Ok(())
    }

    /// Serialize fixed-size char array (no length prefix)
    fn serialize_char_array_fixed(&mut self, data: &[char]) -> Result<(), CdrError> {
        for &value in data {
            self.buffer_mut().push(value as u8);
        }
        Ok(())
    }

    /// Serialize fixed-size string array (no length prefix)
    fn serialize_string_array(&mut self, data: &[String]) -> Result<(), CdrError> {
        for value in data {
            self.serialize_string(value)?;
        }
        Ok(())
    }
}

// Implement ArraySerialize for both serializer types
impl ArraySerialize for CdrSerializer {}
impl ArraySerialize for Xcdr2Serializer {}

#[cfg(test)]
#[allow(unused_imports)]
mod cdr_array_tests {
    use crate::{
        dcps::topic::type_support::{DdsType, FieldAccessor},
        serialize::{
            cdr::{
                CdrDeserialize, CdrDeserializer, CdrSerialize, CdrSerializer, ExtensibilityKind,
                XcdrDeserialize, XcdrDeserializer, XcdrSerialize, XcdrSerializer,
            },
            BufferManager, DeserializerReader, WChar, WString,
        },
    };
    use std::collections::HashMap;
    #[test]
    fn test_cdr_array_i32() {
        let value: [i32; 5] = [1, 2, 3, 4, 5];

        let mut serializer = CdrSerializer::new(true);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_cdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();
        let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
        let result = <[i32; 5]>::deserialize_cdr(&mut deserializer).unwrap();
        assert_eq!(result, value);
    }

    #[test]
    fn test_cdr_multidim_array_row_major() {
        let value: [[u16; 3]; 2] = [[1, 2, 3], [4, 5, 6]];

        let mut serializer = CdrSerializer::new(true);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_cdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();
        let expected: &[u8] =
            &[0x01, 0x00, 0x02, 0x00, 0x03, 0x00, 0x04, 0x00, 0x05, 0x00, 0x06, 0x00];
        assert_eq!(&bytes[4..], expected);

        let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
        let result = <[[u16; 3]; 2]>::deserialize_cdr(&mut deserializer).unwrap();
        assert_eq!(result, value);
    }

    #[test]
    fn test_xcdr2_multidim_array_row_major() {
        let value: [[u16; 3]; 2] = [[1, 2, 3], [4, 5, 6]];

        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Final);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_xcdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();
        let expected: &[u8] =
            &[0x01, 0x00, 0x02, 0x00, 0x03, 0x00, 0x04, 0x00, 0x05, 0x00, 0x06, 0x00];
        assert_eq!(&bytes[4..], expected);

        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = <[[u16; 3]; 2]>::deserialize_xcdr(&mut deserializer).unwrap();
        assert_eq!(result, value);
    }

    #[test]
    fn test_xcdr2_3d_array_row_major() {
        let value: [[[u8; 2]; 3]; 2] = [[[1, 2], [3, 4], [5, 6]], [[7, 8], [9, 10], [11, 12]]];

        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Final);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_xcdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();
        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = <[[[u8; 2]; 3]; 2]>::deserialize_xcdr(&mut deserializer).unwrap();
        assert_eq!(result, value);

        let payload = &bytes[4..];
        let expected: &[u8] = &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
        assert_eq!(payload, expected);
    }

    // HashMap Tests - CDR

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Final")]
    struct MatrixFinal {
        a: u32,
        b: u32,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
    struct MatrixAppendable {
        a: u32,
        b: u32,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Mutable")]
    struct MatrixMutable {
        a: u32,
        b: u32,
    }

    fn xcdr1_format() -> crate::dcps::topic::type_support::SerializationFormat {
        crate::dcps::topic::type_support::SerializationFormat::Cdr
    }

    fn xcdr2_format(
        ext: ExtensibilityKind,
    ) -> crate::dcps::topic::type_support::SerializationFormat {
        crate::dcps::topic::type_support::SerializationFormat::Xcdr {
            extensibility_kind: ext,
            use_delimiters: !matches!(ext, ExtensibilityKind::Final),
        }
    }

    macro_rules! matrix_case {
        ($name:ident, $ty:ident, $field_setter:expr, $field_getter:expr, $format:expr, $encap:expr) => {
            #[test]
            fn $name() {
                use crate::dcps::topic::type_support::TypeSupport;

                let value: $ty = $field_setter;
                let ts = <$ty>::get_type_support();
                let fmt = $format;

                // serialize → bytes
                let serialized = ts.serialize(&value, Some(&fmt)).unwrap();
                assert_eq!(
                    &serialized[0..2],
                    &$encap,
                    concat!(stringify!($name), ": serialize() encap mismatch"),
                );
                let got_a = ts.deserialize(&serialized, Some(&fmt)).unwrap();
                let got_a = got_a.downcast_ref::<$ty>().unwrap();
                $field_getter(got_a, &value);

                // serialize_into → buffer
                let mut buf = Vec::new();
                ts.serialize_into(&value, &mut buf, Some(&fmt)).unwrap();
                assert_eq!(
                    &buf[0..2],
                    &$encap,
                    concat!(stringify!($name), ": serialize_into() encap mismatch"),
                );
                assert_eq!(
                    &buf[..],
                    &serialized[..],
                    concat!(stringify!($name), ": serialize_into() differs from serialize()"),
                );
                let got_b = ts.deserialize(&buf, Some(&fmt)).unwrap();
                let got_b = got_b.downcast_ref::<$ty>().unwrap();
                $field_getter(got_b, &value);
            }
        };
    }

    fn matrix_check(got: &MatrixFinal, want: &MatrixFinal) {
        assert_eq!(got.a, want.a);
        assert_eq!(got.b, want.b);
    }
    fn matrix_check_a(got: &MatrixAppendable, want: &MatrixAppendable) {
        assert_eq!(got.a, want.a);
        assert_eq!(got.b, want.b);
    }
    fn matrix_check_m(got: &MatrixMutable, want: &MatrixMutable) {
        assert_eq!(got.a, want.a);
        assert_eq!(got.b, want.b);
    }

    matrix_case!(
        matrix_xcdr1_final,
        MatrixFinal,
        MatrixFinal { a: 0x1111_1111, b: 0x2222_2222 },
        matrix_check,
        xcdr1_format(),
        [0x00, 0x01]
    );
    matrix_case!(
        matrix_xcdr1_appendable,
        MatrixAppendable,
        MatrixAppendable { a: 0x3333_3333, b: 0x4444_4444 },
        matrix_check_a,
        xcdr1_format(),
        [0x00, 0x01]
    );
    matrix_case!(
        matrix_xcdr1_mutable,
        MatrixMutable,
        MatrixMutable { a: 0x5555_5555, b: 0x6666_6666 },
        matrix_check_m,
        xcdr1_format(),
        [0x00, 0x03]
    );
    matrix_case!(
        matrix_xcdr2_final,
        MatrixFinal,
        MatrixFinal { a: 0x7777_7777, b: 0x8888_8888 },
        matrix_check,
        xcdr2_format(ExtensibilityKind::Final),
        [0x00, 0x07]
    );
    matrix_case!(
        matrix_xcdr2_appendable,
        MatrixAppendable,
        MatrixAppendable { a: 0x9999_9999, b: 0xAAAA_AAAA },
        matrix_check_a,
        xcdr2_format(ExtensibilityKind::Appendable),
        [0x00, 0x09]
    );
    matrix_case!(
        matrix_xcdr2_mutable,
        MatrixMutable,
        MatrixMutable { a: 0xBBBB_BBBB, b: 0xCCCC_CCCC },
        matrix_check_m,
        xcdr2_format(ExtensibilityKind::Mutable),
        [0x00, 0x0B]
    );
}
