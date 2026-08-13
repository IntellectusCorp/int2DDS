//! Compile gate for the absolute paths `#[derive(DdsType)]` emits.
//!
//! This target links `int2dds` and `int2dds-derive` and nothing else, so every path
//! the macro writes has to be reachable through the `int2dds` facade. Inside the
//! workspace that property is invisible: `dds`, `ffi`, `idl` and `rpc` all carry
//! `log`/`speedy` themselves, so a bare `speedy::` in macro output resolves there by
//! accident and only breaks for downstream users.
//!
//! The macro emits ~175 distinct absolute paths, 54 of them under
//! `serialize::`. Those 54 are exactly what moves when the CDR kernel is split into
//! its own crate, so this file is the enforceable form of "the facade still
//! re-exports everything the macro names".
//!
//! Types below exist to be compiled, not run; keep adding shapes rather than
//! assertions.

use int2dds::serialize::WString;
use int2dds_derive::DdsType;
use std::collections::HashMap;

#[derive(DdsType)]
#[dds_type(nested)]
pub struct Inner {
    pub a: i32,
}

#[derive(DdsType)]
#[repr(i32)]
pub enum Color {
    Red = 0,
    Green = 1,
}

#[derive(DdsType)]
#[dds_type(bitmask, bit_bound = 8)]
pub enum Flags {
    #[dds(position = 0)]
    A = 0,
    #[dds(position = 1)]
    B = 1,
}

#[derive(DdsType)]
#[dds_type(bitset)]
pub struct Bits {
    #[dds(bitfield = 3)]
    pub a: u8,
    #[dds(bitfield = 5)]
    pub b: u8,
}

#[derive(DdsType)]
#[dds_type(alias)]
pub struct Meters(pub f64);

#[derive(DdsType)]
pub struct TupleStruct(pub i32, pub String);

#[derive(DdsType)]
#[dds_type(final)]
pub struct FinalStruct {
    #[dds(key)]
    pub id: i32,
    pub prims: [i32; 3],
    pub nums: Vec<i32>,
    #[dds(char)]
    pub letter: u8,
    #[dds(uint8)]
    pub blob: Vec<u8>,
    #[dds(non_serialized)]
    pub scratch: i64,
}

#[derive(DdsType)]
#[dds_type(final)]
pub struct BaseStruct {
    #[dds(key)]
    pub base_id: i32,
}

#[derive(DdsType)]
#[dds_type(appendable)]
pub struct DerivedStruct {
    #[dds(parent)]
    pub base: BaseStruct,
    #[dds(id = 20, must_understand)]
    pub extra: i32,
}

#[derive(DdsType)]
#[dds_type(mutable)]
pub struct MutableStruct {
    #[dds(key)]
    pub id: i32,
    #[dds(id = 7)]
    pub name: String,
    #[dds(bound = 8)]
    pub short_name: String,
    #[dds(bound = 4, try_construct = "trim")]
    pub note: WString,
    #[dds(bound = 4, try_construct = "use_default")]
    pub other_note: String,
    #[dds(optional)]
    pub maybe: Option<i32>,
    #[dds(external)]
    pub boxed: Box<Inner>,
    pub items: Vec<Inner>,
    pub arr: [Inner; 2],
    pub table: HashMap<i32, String>,
    pub color: Color,
    pub flags: FlagsValue,
    pub bits: Bits,
    pub distance: Meters,
}

#[derive(DdsType)]
#[dds_type(appendable, autoid = "Hash")]
pub struct HashIdStruct {
    #[dds(hashid)]
    pub id: i32,
    pub inner: Inner,
}

#[derive(DdsType)]
#[dds_type(appendable, autoid = "Sequential", no_default, no_partialeq)]
pub struct SequentialStruct {
    pub first: i32,
    pub second: String,
}

impl Default for SequentialStruct {
    fn default() -> Self {
        Self { first: 0, second: String::new() }
    }
}
