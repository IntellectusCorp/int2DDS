use proc_macro2::TokenStream;
use quote::quote;
use syn::punctuated::Punctuated;
use syn::token::Comma;
use syn::Variant;

use crate::codegen::type_config::ExtensibilityKind;
use crate::codegen::utils::{
    get_discriminant_value, get_serialization_method, get_variant_type, variant_has_data,
    variant_is_union_default, DiscriminantType, SerializationMethod,
};

fn generate_unknown_discriminant_arm(
    name: &syn::Ident,
    variants: &Punctuated<Variant, Comma>,
    crate_path: &TokenStream,
    xcdr: bool,
    consume_member_header: bool,
) -> TokenStream {
    let err_path = quote! {
        #crate_path::serialize::core::SerializationError::InvalidUnionDiscriminant(discriminant as i32)
    };

    let Some(variant) = variants.iter().find(|v| variant_is_union_default(v)) else {
        return quote! { _ => Err(#err_path), };
    };

    let variant_name = &variant.ident;
    if !variant_has_data(variant) {
        return quote! { _ => Ok(#name::#variant_name), };
    }

    let value_deserialization = if let Some(field_type) = get_variant_type(variant) {
        generate_value_deserialization(field_type, crate_path, xcdr)
    } else if xcdr {
        quote! {
            let value = #crate_path::serialize::cdr::XcdrDeserialize::deserialize_xcdr(deserializer)?;
        }
    } else {
        quote! {
            let value = #crate_path::serialize::cdr::CdrDeserialize::deserialize_cdr(deserializer)?;
        }
    };

    if consume_member_header {
        quote! {
            _ => {
                let (_mid, _mlen) = deserializer.read_member_header()?;
                #value_deserialization
                Ok(#name::#variant_name(value))
            }
        }
    } else {
        quote! {
            _ => {
                #value_deserialization
                Ok(#name::#variant_name(value))
            }
        }
    }
}

pub fn wrap_with_emheader(
    member_id_expr: TokenStream,
    must_understand: bool,
    lc_hint: TokenStream,
    payload: TokenStream,
) -> TokenStream {
    quote! {
        serializer.write_member_with_lc(
            (#member_id_expr) as u32,
            #must_understand,
            #lc_hint,
            |serializer| -> ::std::result::Result<(), _> {
                #payload
                Ok(())
            },
        )?;
    }
}

/// Generate CdrSerialize implementation for union (enum with data)
pub fn generate_union_cdr_serialize_impl(
    name: &syn::Ident,
    variants: &Punctuated<Variant, Comma>,
    crate_path: &TokenStream,
    disc_type: DiscriminantType,
) -> TokenStream {
    let serialize_disc_method = syn::Ident::new(disc_type.serialize_method(), name.span());
    let disc_rust_type = syn::Ident::new(disc_type.rust_type(), name.span());

    let match_arms: Vec<_> = variants
        .iter()
        .enumerate()
        .map(|(idx, variant)| {
            let variant_name = &variant.ident;
            let disc_value = get_discriminant_value(variant, idx);

            if variant_has_data(variant) {
                // Variant with data
                let value_serialization = if let Some(field_type) = get_variant_type(variant) {
                    generate_value_serialization(field_type, crate_path, false)
                } else {
                    // Multiple fields not supported - use trait fallback
                    quote! {
                        #crate_path::serialize::cdr::CdrSerialize::serialize_cdr(value, serializer)?;
                    }
                };

                quote! {
                    #name::#variant_name(value) => {
                        serializer.#serialize_disc_method(#disc_value as #disc_rust_type)?;
                        #value_serialization
                    }
                }
            } else {
                // Unit variant - only discriminant
                quote! {
                    #name::#variant_name => {
                        serializer.#serialize_disc_method(#disc_value as #disc_rust_type)?;
                    }
                }
            }
        })
        .collect();

    quote! {
        impl #crate_path::serialize::cdr::CdrSerialize for #name {
            fn serialize_cdr(&self, serializer: &mut #crate_path::serialize::cdr::CdrSerializer) -> #crate_path::serialize::cdr::CdrResult<()> {
                match self {
                    #(#match_arms)*
                }
                Ok(())
            }
        }
    }
}

/// Generate CdrDeserialize implementation for union (enum with data)
pub fn generate_union_cdr_deserialize_impl(
    name: &syn::Ident,
    variants: &Punctuated<Variant, Comma>,
    crate_path: &TokenStream,
    disc_type: DiscriminantType,
) -> TokenStream {
    let deserialize_disc_method = syn::Ident::new(disc_type.deserialize_method(), name.span());

    let match_arms: Vec<_> = variants
        .iter()
        .enumerate()
        .map(|(idx, variant)| {
            let variant_name = &variant.ident;
            let disc_value = get_discriminant_value(variant, idx);

            if variant_has_data(variant) {
                let value_deserialization = if let Some(field_type) = get_variant_type(variant) {
                    generate_value_deserialization(field_type, crate_path, false)
                } else {
                    // Multiple fields not supported - use trait fallback
                    quote! {
                        let value = #crate_path::serialize::cdr::CdrDeserialize::deserialize_cdr(deserializer)?;
                    }
                };

                quote! {
                    #disc_value => {
                        #value_deserialization
                        Ok(#name::#variant_name(value))
                    }
                }
            } else {
                quote! {
                    #disc_value => Ok(#name::#variant_name),
                }
            }
        })
        .collect();

    let unknown_arm = generate_unknown_discriminant_arm(name, variants, crate_path, false, false);

    quote! {
        impl #crate_path::serialize::cdr::CdrDeserialize for #name {
            fn deserialize_cdr(deserializer: &mut #crate_path::serialize::cdr::CdrDeserializer) -> #crate_path::serialize::cdr::CdrResult<Self> {
                let discriminant = deserializer.#deserialize_disc_method()? as i64;
                match discriminant {
                    #(#match_arms)*
                    #unknown_arm
                }
            }
        }
    }
}

/// Generate XcdrSerialize implementation for union (enum with data)
///
/// Encoding per DDS-XTypes 7.4.4 (XCDR2):
///  - Final     : discriminant + selected branch value (no DHEADER, no EMHEADER)
///  - Appendable: DHEADER + (discriminant + value)
///  - Mutable   : DHEADER + EMHEADER(0)+discriminant + EMHEADER(branch_id)+value
///
/// branch_id = variant_index + 1; 0 is reserved for the discriminant.
pub fn generate_union_xcdr_serialize_impl(
    name: &syn::Ident,
    variants: &Punctuated<Variant, Comma>,
    crate_path: &TokenStream,
    disc_type: DiscriminantType,
    extensibility: ExtensibilityKind,
) -> TokenStream {
    let serialize_disc_method = syn::Ident::new(disc_type.serialize_method(), name.span());
    let disc_rust_type = syn::Ident::new(disc_type.rust_type(), name.span());
    let is_mutable = matches!(extensibility, ExtensibilityKind::Mutable);

    let wrap_member = |member_id: u32, payload: TokenStream| -> TokenStream {
        wrap_with_emheader(
            quote! { #member_id },
            false,
            quote! { #crate_path::serialize::cdr::LcHint::Auto },
            payload,
        )
    };

    let match_arms: Vec<_> = variants
        .iter()
        .enumerate()
        .map(|(idx, variant)| {
            let variant_name = &variant.ident;
            let disc_value = get_discriminant_value(variant, idx);
            let branch_id = (idx as u32) + 1; // 0 reserved for discriminant

            let disc_payload = quote! {
                serializer.#serialize_disc_method(#disc_value as #disc_rust_type)?;
            };

            let body = if variant_has_data(variant) {
                let value_serialization = if let Some(field_type) = get_variant_type(variant) {
                    generate_value_serialization(field_type, crate_path, true)
                } else {
                    quote! {
                        #crate_path::serialize::cdr::XcdrSerialize::serialize_xcdr(value, serializer)?;
                    }
                };

                if is_mutable {
                    let disc_block = wrap_member(0, disc_payload);
                    let val_block = wrap_member(branch_id, value_serialization);
                    quote! {
                        #name::#variant_name(value) => {
                            #disc_block
                            #val_block
                        }
                    }
                } else {
                    quote! {
                        #name::#variant_name(value) => {
                            #disc_payload
                            #value_serialization
                        }
                    }
                }
            } else if is_mutable {
                let disc_block = wrap_member(0, disc_payload);
                quote! {
                    #name::#variant_name => {
                        #disc_block
                    }
                }
            } else {
                quote! {
                    #name::#variant_name => {
                        #disc_payload
                    }
                }
            };

            body
        })
        .collect();

    let needs_dheader = !matches!(extensibility, ExtensibilityKind::Final);
    let body = if needs_dheader {
        quote! {
            let size_pos = serializer.begin_struct()?;
            match self {
                #(#match_arms)*
            }
            serializer.end_struct(size_pos)?;
            Ok(())
        }
    } else {
        quote! {
            match self {
                #(#match_arms)*
            }
            Ok(())
        }
    };

    quote! {
        impl #crate_path::serialize::cdr::XcdrSerialize for #name {
            fn serialize_xcdr(&self, serializer: &mut #crate_path::serialize::cdr::XcdrSerializer) -> #crate_path::serialize::cdr::XcdrResult<()> {
                #body
            }
        }
    }
}

/// Generate XcdrDeserialize implementation for union (enum with data)
///
/// Mirrors `generate_union_xcdr_serialize_impl`. For Mutable, the deserializer
/// expects the discriminant member (member_id = 0) before the branch member,
/// matching what the serializer emits. Unknown member IDs are skipped for
/// forward compatibility.
pub fn generate_union_xcdr_deserialize_impl(
    name: &syn::Ident,
    variants: &Punctuated<Variant, Comma>,
    crate_path: &TokenStream,
    disc_type: DiscriminantType,
    extensibility: ExtensibilityKind,
) -> TokenStream {
    let deserialize_disc_method = syn::Ident::new(disc_type.deserialize_method(), name.span());

    let is_mutable = matches!(extensibility, ExtensibilityKind::Mutable);

    let match_arms: Vec<_> = variants
        .iter()
        .enumerate()
        .map(|(idx, variant)| {
            let variant_name = &variant.ident;
            let disc_value = get_discriminant_value(variant, idx);

            if variant_has_data(variant) {
                let value_deserialization = if let Some(field_type) = get_variant_type(variant) {
                    generate_value_deserialization(field_type, crate_path, true)
                } else {
                    quote! {
                        let value = #crate_path::serialize::cdr::XcdrDeserialize::deserialize_xcdr(deserializer)?;
                    }
                };

                if is_mutable {
                    // Consume the branch's EMHEADER before reading payload.
                    quote! {
                        #disc_value => {
                            let (_mid, _mlen) = deserializer.read_member_header()?;
                            #value_deserialization
                            Ok(#name::#variant_name(value))
                        }
                    }
                } else {
                    quote! {
                        #disc_value => {
                            #value_deserialization
                            Ok(#name::#variant_name(value))
                        }
                    }
                }
            } else {
                quote! {
                    #disc_value => Ok(#name::#variant_name),
                }
            }
        })
        .collect();

    let unknown_arm =
        generate_unknown_discriminant_arm(name, variants, crate_path, true, is_mutable);

    let body = match extensibility {
        ExtensibilityKind::Final => quote! {
            let discriminant = deserializer.#deserialize_disc_method()? as i64;
            match discriminant {
                #(#match_arms)*
                #unknown_arm
            }
        },
        ExtensibilityKind::Appendable => quote! {
            let (object_size, start_position) = deserializer.begin_struct()?;
            let discriminant = deserializer.#deserialize_disc_method()? as i64;
            let result = match discriminant {
                #(#match_arms)*
                #unknown_arm
            };
            deserializer.end_struct(object_size, start_position)?;
            result
        },
        ExtensibilityKind::Mutable => quote! {
            let (object_size, start_position) = deserializer.begin_struct()?;

            // Read discriminant member (member_id == 0).
            let (mid, _mlen) = deserializer.read_member_header()?;
            if mid != 0 {
                return Err(#crate_path::serialize::core::SerializationError::InvalidMemberHeader);
            }
            let discriminant = deserializer.#deserialize_disc_method()? as i64;

            // Then the branch member header + payload (consumed inside arm).
            let result = match discriminant {
                #(#match_arms)*
                #unknown_arm
            };

            deserializer.end_struct(object_size, start_position)?;
            result
        },
    };

    quote! {
        impl #crate_path::serialize::cdr::XcdrDeserialize for #name {
            fn deserialize_xcdr(deserializer: &mut #crate_path::serialize::cdr::XcdrDeserializer) -> #crate_path::serialize::cdr::XcdrResult<Self> {
                #body
            }
        }
    }
}

/// Generate serialization code for a value based on its type
fn generate_value_serialization(
    ty: &syn::Type,
    crate_path: &TokenStream,
    xcdr: bool,
) -> TokenStream {
    let method = get_serialization_method(ty);

    match method {
        SerializationMethod::U8 => quote! { serializer.serialize_u8(*value)?; },
        SerializationMethod::U16 => quote! { serializer.serialize_u16(*value)?; },
        SerializationMethod::U32 => quote! { serializer.serialize_u32(*value)?; },
        SerializationMethod::U64 => quote! { serializer.serialize_u64(*value)?; },
        SerializationMethod::I8 => quote! { serializer.serialize_i8(*value)?; },
        SerializationMethod::I16 => quote! { serializer.serialize_i16(*value)?; },
        SerializationMethod::I32 => quote! { serializer.serialize_i32(*value)?; },
        SerializationMethod::I64 => quote! { serializer.serialize_i64(*value)?; },
        SerializationMethod::F32 => quote! { serializer.serialize_f32(*value)?; },
        SerializationMethod::F64 => quote! { serializer.serialize_f64(*value)?; },
        SerializationMethod::Bool => quote! { serializer.serialize_bool(*value)?; },
        SerializationMethod::Char => quote! { serializer.serialize_char(*value)?; },
        SerializationMethod::String => quote! { serializer.serialize_string(value)?; },
        SerializationMethod::VecU8 => quote! { serializer.serialize_byte_sequence(value)?; },
        SerializationMethod::VecU16 => quote! { serializer.serialize_u16_sequence(value)?; },
        SerializationMethod::VecU32 => quote! { serializer.serialize_u32_sequence(value)?; },
        SerializationMethod::VecU64 => quote! { serializer.serialize_u64_sequence(value)?; },
        SerializationMethod::VecI8 => quote! { serializer.serialize_i8_sequence(value)?; },
        SerializationMethod::VecI16 => quote! { serializer.serialize_i16_sequence(value)?; },
        SerializationMethod::VecI32 => quote! { serializer.serialize_i32_sequence(value)?; },
        SerializationMethod::VecI64 => quote! { serializer.serialize_i64_sequence(value)?; },
        SerializationMethod::VecF32 => quote! { serializer.serialize_f32_sequence(value)?; },
        SerializationMethod::VecF64 => quote! { serializer.serialize_f64_sequence(value)?; },
        SerializationMethod::VecBool => quote! { serializer.serialize_bool_sequence(value)?; },
        SerializationMethod::VecChar => quote! { serializer.serialize_char_sequence(value)?; },
        SerializationMethod::VecString => quote! { serializer.serialize_string_sequence(value)?; },
        _ => {
            // Fallback to trait-based serialization
            if xcdr {
                quote! {
                    #crate_path::serialize::cdr::XcdrSerialize::serialize_xcdr(value, serializer)?;
                }
            } else {
                quote! {
                    #crate_path::serialize::cdr::CdrSerialize::serialize_cdr(value, serializer)?;
                }
            }
        }
    }
}

/// Generate deserialization code for a value based on its type
fn generate_value_deserialization(
    ty: &syn::Type,
    crate_path: &TokenStream,
    xcdr: bool,
) -> TokenStream {
    let method = get_serialization_method(ty);

    match method {
        SerializationMethod::U8 => quote! { let value = deserializer.deserialize_u8()?; },
        SerializationMethod::U16 => quote! { let value = deserializer.deserialize_u16()?; },
        SerializationMethod::U32 => quote! { let value = deserializer.deserialize_u32()?; },
        SerializationMethod::U64 => quote! { let value = deserializer.deserialize_u64()?; },
        SerializationMethod::I8 => quote! { let value = deserializer.deserialize_i8()?; },
        SerializationMethod::I16 => quote! { let value = deserializer.deserialize_i16()?; },
        SerializationMethod::I32 => quote! { let value = deserializer.deserialize_i32()?; },
        SerializationMethod::I64 => quote! { let value = deserializer.deserialize_i64()?; },
        SerializationMethod::F32 => quote! { let value = deserializer.deserialize_f32()?; },
        SerializationMethod::F64 => quote! { let value = deserializer.deserialize_f64()?; },
        SerializationMethod::Bool => quote! { let value = deserializer.deserialize_bool()?; },
        SerializationMethod::Char => quote! { let value = deserializer.deserialize_char()?; },
        SerializationMethod::String => quote! { let value = deserializer.deserialize_string()?; },
        SerializationMethod::VecU8 => {
            quote! { let value = deserializer.deserialize_byte_sequence()?; }
        }
        SerializationMethod::VecU16 => {
            quote! { let value = deserializer.deserialize_u16_sequence()?; }
        }
        SerializationMethod::VecU32 => {
            quote! { let value = deserializer.deserialize_u32_sequence()?; }
        }
        SerializationMethod::VecU64 => {
            quote! { let value = deserializer.deserialize_u64_sequence()?; }
        }
        SerializationMethod::VecI8 => {
            quote! { let value = deserializer.deserialize_i8_sequence()?; }
        }
        SerializationMethod::VecI16 => {
            quote! { let value = deserializer.deserialize_i16_sequence()?; }
        }
        SerializationMethod::VecI32 => {
            quote! { let value = deserializer.deserialize_i32_sequence()?; }
        }
        SerializationMethod::VecI64 => {
            quote! { let value = deserializer.deserialize_i64_sequence()?; }
        }
        SerializationMethod::VecF32 => {
            quote! { let value = deserializer.deserialize_f32_sequence()?; }
        }
        SerializationMethod::VecF64 => {
            quote! { let value = deserializer.deserialize_f64_sequence()?; }
        }
        SerializationMethod::VecBool => {
            quote! { let value = deserializer.deserialize_bool_sequence()?; }
        }
        SerializationMethod::VecChar => {
            quote! { let value = deserializer.deserialize_char_sequence()?; }
        }
        SerializationMethod::VecString => {
            quote! { let value = deserializer.deserialize_string_sequence()?; }
        }
        _ => {
            // Fallback to trait-based deserialization
            let field_type = ty;
            if xcdr {
                quote! {
                    let value = <#field_type as #crate_path::serialize::cdr::XcdrDeserialize>::deserialize_xcdr(deserializer)?;
                }
            } else {
                quote! {
                    let value = <#field_type as #crate_path::serialize::cdr::CdrDeserialize>::deserialize_cdr(deserializer)?;
                }
            }
        }
    }
}
