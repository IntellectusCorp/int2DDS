use quote::quote;
use syn::DeriveInput;

use crate::codegen::type_config::DdsTypeConfig;
use crate::codegen::utils::parse_field_attributes;

/// Generate DdsType implementation for bitset struct types.
/// Each field has #[dds(bitfield = N)] specifying bit width.
/// Fields are packed sequentially into a single integer.
///
/// Total bits determine wire type:
/// - total <= 8  → u8
/// - total <= 16 → u16
/// - total <= 32 → u32
/// - total <= 64 → u64
pub fn derive_bitset_impl(
    input: &DeriveInput,
    name: &syn::Ident,
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
    type_config: &DdsTypeConfig,
) -> proc_macro2::TokenStream {
    let crate_path = &type_config.crate_path;

    // Collect field info: (name, type, bit_width, bit_offset)
    let mut bit_offset: u8 = 0;
    let mut field_info: Vec<(&syn::Ident, &syn::Type, u8, u8)> = Vec::new();

    for field in fields.iter() {
        let field_name = field.ident.as_ref().unwrap();
        let field_config = parse_field_attributes(field);
        let bit_width = field_config.bitfield.unwrap_or_else(|| {
            panic!("Bitset field '{}' must have #[dds(bitfield = N)] attribute", field_name);
        });

        field_info.push((field_name, &field.ty, bit_width, bit_offset));
        bit_offset += bit_width;
    }

    let total_bits = bit_offset;
    if total_bits > 64 {
        panic!("Bitset '{}' total bits {} exceeds maximum 64", name, total_bits);
    }

    // Determine wire type
    let (wire_type, wire_ser, wire_deser) = if total_bits <= 8 {
        (quote!(u8), quote!(serialize_u8), quote!(deserialize_u8))
    } else if total_bits <= 16 {
        (quote!(u16), quote!(serialize_u16), quote!(deserialize_u16))
    } else if total_bits <= 32 {
        (quote!(u32), quote!(serialize_u32), quote!(deserialize_u32))
    } else {
        (quote!(u64), quote!(serialize_u64), quote!(deserialize_u64))
    };

    // Generate serialize: pack fields into integer
    let pack_fields: Vec<_> = field_info
        .iter()
        .map(|(field_name, _field_type, bit_width, offset)| {
            let mask = (1u64 << *bit_width) - 1;
            let mask_lit = proc_macro2::Literal::u64_unsuffixed(mask);
            let offset_lit = proc_macro2::Literal::u8_unsuffixed(*offset);
            quote! {
                packed |= ((self.#field_name as #wire_type) & #mask_lit as #wire_type) << #offset_lit;
            }
        })
        .collect();

    // Generate deserialize: extract fields from integer
    let unpack_fields: Vec<_> = field_info
        .iter()
        .map(|(field_name, field_type, bit_width, offset)| {
            let mask = (1u64 << *bit_width) - 1;
            let mask_lit = proc_macro2::Literal::u64_unsuffixed(mask);
            let offset_lit = proc_macro2::Literal::u8_unsuffixed(*offset);
            quote! {
                let #field_name = ((packed >> #offset_lit) & #mask_lit as #wire_type) as #field_type;
            }
        })
        .collect();

    let field_names: Vec<_> = field_info.iter().map(|(name, _, _, _)| name).collect();

    // Generate additional derives
    let additional_derives =
        crate::codegen::derives::generate_additional_derives(input, name, type_config);

    quote! {
        impl #crate_path::serialize::cdr::CdrSerialize for #name {
            fn serialize_cdr(&self, serializer: &mut #crate_path::serialize::cdr::CdrSerializer) -> #crate_path::serialize::cdr::CdrResult<()> {
                use #crate_path::serialize::cdr::PrimitiveSerialize;
                let mut packed: #wire_type = 0;
                #(#pack_fields)*
                serializer.#wire_ser(packed)
            }
        }

        impl #crate_path::serialize::cdr::CdrDeserialize for #name {
            fn deserialize_cdr(deserializer: &mut #crate_path::serialize::cdr::CdrDeserializer) -> #crate_path::serialize::cdr::CdrResult<Self> {
                use #crate_path::serialize::cdr::PrimitiveSerialize;
                let packed = deserializer.#wire_deser()?;
                #(#unpack_fields)*
                Ok(#name { #(#field_names,)* })
            }
        }

        impl #crate_path::serialize::xcdr::XcdrSerialize for #name {
            fn serialize_xcdr(&self, serializer: &mut #crate_path::serialize::xcdr::XcdrSerializer) -> #crate_path::serialize::xcdr::XcdrResult<()> {
                use #crate_path::serialize::cdr::PrimitiveSerialize;
                let mut packed: #wire_type = 0;
                #(#pack_fields)*
                serializer.#wire_ser(packed)
            }
        }

        impl #crate_path::serialize::xcdr::XcdrDeserialize for #name {
            fn deserialize_xcdr(deserializer: &mut #crate_path::serialize::xcdr::XcdrDeserializer) -> #crate_path::serialize::xcdr::XcdrResult<Self> {
                use #crate_path::serialize::cdr::PrimitiveSerialize;
                let packed = deserializer.#wire_deser()?;
                #(#unpack_fields)*
                Ok(#name { #(#field_names,)* })
            }
        }

        #additional_derives
    }
}
