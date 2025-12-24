use syn::{Data, DeriveInput, Fields};

use crate::codegen::{
    derive_enum_impl, derive_struct_impl, derive_tuple_struct_impl, parse_dds_type_attributes,
};

pub fn derive_dds_type_impl(input: &DeriveInput) -> proc_macro2::TokenStream {
    let type_config = parse_dds_type_attributes(input);

    match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => {
                derive_struct_impl(input, &input.ident, &fields.named, &type_config)
            }
            Fields::Unnamed(fields) => {
                derive_tuple_struct_impl(input, &input.ident, &fields.unnamed, &type_config)
            }
            Fields::Unit => panic!("DdsType cannot be derived for unit structs"),
        },
        Data::Enum(data) => derive_enum_impl(input, &input.ident, &data.variants, &type_config),
        Data::Union(_) => panic!("DdsType cannot be derived for Rust unions. Use enum instead."),
    }
}
