//! CDR/XCDR tests that go through `#[derive(DdsType)]`.
//!
//! They live here rather than in the `int2dds-cdr` kernel because they exercise
//! derive output, and derive names the `int2dds` facade. A `#[cfg(test)]` module
//! inside the kernel cannot: the kernel's own test build and the kernel rlib that
//! an `int2dds` dev-dependency would pull in are two distinct crate instances, so
//! `crate::` types and `int2dds::` types stop unifying. Tests of kernel-internal
//! items stay in the kernel; these do not touch any.

mod cdr_mod;
mod deserializer_sequence;
mod deserializer_string;
mod serializer_array;
mod serializer_mod;
mod serializer_string;
mod xcdr1;
mod xcdr2;
