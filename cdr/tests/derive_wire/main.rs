//! CDR/XCDR wire vectors that go through `#[derive(DdsType)]`.
//!
//! An integration test rather than a `#[cfg(test)]` module inside the kernel: derive
//! output names the `int2dds` facade, and a unit-test build would link two distinct
//! `int2dds-cdr` instances -- its own `--test` build and the rlib that the `int2dds`
//! dev-dependency pulls in -- so `crate::` types and `int2dds::` types would stop
//! unifying. An integration target links only the rlib, so there is one instance.
//! It also means these exercise the facade from outside the crate, the way generated
//! code does. Tests of kernel-internal items stay in `cdr/src/**`.

mod cdr_mod;
mod deserializer_sequence;
mod deserializer_string;
mod serializer_array;
mod serializer_mod;
mod serializer_string;
mod xcdr1;
mod xcdr2;
