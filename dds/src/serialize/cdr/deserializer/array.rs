use crate::serialize::cdr::{try_vec_prealloc, CdrDeserializer, CdrError, Xcdr2Deserializer};

// Fixed-size arrays carry no length prefix, so each of these is the sequence body read
// with the element count supplied by the caller. The bulk machinery is shared with
// deserializer/sequence.rs — see `read_prim_run` there.
//
// The method bodies are identical for both deserializers; the macros below emit them
// once for each so the two cannot drift apart.

macro_rules! prim_array_reads {
    ($($name:ident => $ty:ty),+ $(,)?) => {$(
        #[doc = concat!("Deserialize fixed-size `", stringify!($ty), "` array (no length prefix)")]
        pub fn $name(&mut self, size: usize) -> Result<Vec<$ty>, CdrError> {
            self.read_prim_run(size)
        }
    )+};
}

macro_rules! impl_shared_array_reads {
    ($($de:ident),+ $(,)?) => {$(
        impl<'a> $de<'a> {
            prim_array_reads! {
                deserialize_byte_array => u8,
                deserialize_u16_array => u16,
                deserialize_u32_array => u32,
                deserialize_u64_array => u64,
                deserialize_i8_array => i8,
                deserialize_i16_array => i16,
                deserialize_i32_array => i32,
                deserialize_i64_array => i64,
                deserialize_f32_array => f32,
                deserialize_f64_array => f64,
            }

            /// Deserialize fixed-size bool array (no length prefix)
            pub fn deserialize_bool_array(&mut self, size: usize) -> Result<Vec<bool>, CdrError> {
                self.read_octet_run(size, |b| b != 0)
            }

            /// Deserialize fixed-size char array (no length prefix)
            pub fn deserialize_char_array_fixed(
                &mut self,
                size: usize,
            ) -> Result<Vec<char>, CdrError> {
                self.read_octet_run(size, |b| b as char)
            }

            /// Deserialize fixed-size string array (no length prefix)
            pub fn deserialize_string_array(
                &mut self,
                size: usize,
            ) -> Result<Vec<String>, CdrError> {
                let mut result = try_vec_prealloc(self.checked_capacity(size, 4)?)?;
                for _ in 0..size {
                    result.push(self.deserialize_string()?);
                }
                Ok(result)
            }
        }
    )+};
}

impl_shared_array_reads!(CdrDeserializer, Xcdr2Deserializer);
