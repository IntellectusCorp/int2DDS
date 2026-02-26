//! HelloWorld data type for FFI examples
//! Based on IDL-generated code from HelloWorld.idl

use int2dds::topic::type_support::DdsType;

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "final")]
pub struct HelloWorld {
    pub index: u32,
    pub message: String,
}
