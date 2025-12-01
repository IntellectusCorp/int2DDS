use proc_macro::TokenStream;
use syn::{parse_macro_input, DeriveInput};

mod codegen;
mod dds_type;

/// Procedural macro to derive DdsType for structs
#[proc_macro_derive(DdsType, attributes(dds, dds_type))]
pub fn derive_dds_type(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    TokenStream::from(dds_type::derive_dds_type_impl(&input))
}
