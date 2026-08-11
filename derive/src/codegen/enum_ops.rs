use proc_macro2::TokenStream;
use quote::quote;
use syn::punctuated::Punctuated;
use syn::token::Comma;
use syn::Variant;

use crate::codegen::utils::{get_discriminant_value, DiscriminantType};

/// Generate CdrSerialize implementation for C-style enum
pub fn generate_enum_cdr_serialize_impl(
    name: &syn::Ident,
    variants: &Punctuated<Variant, Comma>,
    crate_path: &TokenStream,
    disc_type: DiscriminantType,
) -> TokenStream {
    let serialize_method = syn::Ident::new(disc_type.serialize_method(), name.span());
    let disc_rust_type = syn::Ident::new(disc_type.rust_type(), name.span());

    let match_arms: Vec<_> = variants
        .iter()
        .enumerate()
        .map(|(idx, variant)| {
            let variant_name = &variant.ident;
            let disc_value = get_discriminant_value(variant, idx);
            quote! {
                #name::#variant_name => #disc_value as #disc_rust_type,
            }
        })
        .collect();

    quote! {
        impl #crate_path::serialize::cdr::CdrSerialize for #name {
            fn serialize_cdr(&self, serializer: &mut #crate_path::serialize::cdr::CdrSerializer) -> #crate_path::serialize::cdr::CdrResult<()> {
                use #crate_path::serialize::cdr::PrimitiveSerialize;
                let discriminant: #disc_rust_type = match self {
                    #(#match_arms)*
                };
                serializer.#serialize_method(discriminant)
            }
        }
    }
}

/// Generate CdrDeserialize implementation for C-style enum
pub fn generate_enum_cdr_deserialize_impl(
    name: &syn::Ident,
    variants: &Punctuated<Variant, Comma>,
    crate_path: &TokenStream,
    disc_type: DiscriminantType,
) -> TokenStream {
    let deserialize_method = syn::Ident::new(disc_type.deserialize_method(), name.span());

    let match_arms: Vec<_> = variants
        .iter()
        .enumerate()
        .map(|(idx, variant)| {
            let variant_name = &variant.ident;
            let disc_value = get_discriminant_value(variant, idx);
            quote! {
                #disc_value => Ok(#name::#variant_name),
            }
        })
        .collect();

    quote! {
        impl #crate_path::serialize::cdr::CdrDeserialize for #name {
            fn deserialize_cdr(deserializer: &mut #crate_path::serialize::cdr::CdrDeserializer) -> #crate_path::serialize::cdr::CdrResult<Self> {
                let discriminant = deserializer.#deserialize_method()? as i64;
                match discriminant {
                    #(#match_arms)*
                    _ => Err(#crate_path::serialize::core::SerializationError::InvalidEnumDiscriminant(discriminant as i32)),
                }
            }
        }
    }
}

/// Generate XcdrSerialize implementation for C-style enum
pub fn generate_enum_xcdr_serialize_impl(
    name: &syn::Ident,
    variants: &Punctuated<Variant, Comma>,
    crate_path: &TokenStream,
    disc_type: DiscriminantType,
) -> TokenStream {
    let serialize_method = syn::Ident::new(disc_type.serialize_method(), name.span());
    let disc_rust_type = syn::Ident::new(disc_type.rust_type(), name.span());

    let match_arms: Vec<_> = variants
        .iter()
        .enumerate()
        .map(|(idx, variant)| {
            let variant_name = &variant.ident;
            let disc_value = get_discriminant_value(variant, idx);
            quote! {
                #name::#variant_name => #disc_value as #disc_rust_type,
            }
        })
        .collect();

    quote! {
        // Enumerated types are not primitive types, so collections of them carry a
        // DHEADER (DDS-XTypes 7.4.3.5.3/7.4.3.5.4). OMG issue DDSXTY14-56 proposes
        // treating them as primitives, but it is unresolved.
        impl #crate_path::serialize::cdr::XcdrSerialize for #name {
            const IS_PRIMITIVE: bool = false;
            fn serialize_xcdr(&self, serializer: &mut #crate_path::serialize::cdr::XcdrSerializer) -> #crate_path::serialize::cdr::XcdrResult<()> {
                use #crate_path::serialize::cdr::PrimitiveSerialize;
                let discriminant: #disc_rust_type = match self {
                    #(#match_arms)*
                };
                serializer.#serialize_method(discriminant)
            }
        }
    }
}

/// Generate XcdrDeserialize implementation for C-style enum
pub fn generate_enum_xcdr_deserialize_impl(
    name: &syn::Ident,
    variants: &Punctuated<Variant, Comma>,
    crate_path: &TokenStream,
    disc_type: DiscriminantType,
) -> TokenStream {
    let deserialize_method = syn::Ident::new(disc_type.deserialize_method(), name.span());

    let match_arms: Vec<_> = variants
        .iter()
        .enumerate()
        .map(|(idx, variant)| {
            let variant_name = &variant.ident;
            let disc_value = get_discriminant_value(variant, idx);
            quote! {
                #disc_value => Ok(#name::#variant_name),
            }
        })
        .collect();

    quote! {
        impl #crate_path::serialize::cdr::XcdrDeserialize for #name {
            const IS_PRIMITIVE: bool = false;
            fn deserialize_xcdr(deserializer: &mut #crate_path::serialize::cdr::XcdrDeserializer) -> #crate_path::serialize::cdr::XcdrResult<Self> {
                let discriminant = deserializer.#deserialize_method()? as i64;
                match discriminant {
                    #(#match_arms)*
                    _ => Err(#crate_path::serialize::core::SerializationError::InvalidEnumDiscriminant(discriminant as i32)),
                }
            }
        }
    }
}
