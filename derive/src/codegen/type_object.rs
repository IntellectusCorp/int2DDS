//! Code generation for HasTypeObject trait implementation.
//!
//! Generates TypeObject metadata for DdsType derived structs and enums.

use quote::quote;

use crate::codegen::type_config::DdsTypeConfig;
use crate::codegen::utils::{
    get_discriminant_value, get_serialization_method, get_variant_type, parse_field_attributes,
    resolve_member_id, variant_has_data, DiscriminantType, SerializationMethod,
};

/// Generate TypeIdentifier expression for a Rust type.
///
/// `as_char`: when true and the type is `u8` / `[u8; N]`, advertise it as
/// `Char8` (resp. `Char8` array element) in XTypes metadata so that codegen
/// from other languages — which keep IDL `char` as native char — can match.
fn type_to_identifier(
    ty: &syn::Type,
    crate_path: &proc_macro2::TokenStream,
    as_char: bool,
) -> proc_macro2::TokenStream {
    let method = get_serialization_method(ty);

    if as_char {
        // Override only u8 / [u8; N] cases. Other types fall through to the
        // normal mapping below.
        if matches!(method, SerializationMethod::U8) {
            return quote! { #crate_path::xtypes::TypeIdentifier::Char8 };
        }
        if matches!(method, SerializationMethod::U8Array) {
            if let syn::Type::Array(array) = ty {
                let size = &array.len;
                return quote! {
                    #crate_path::xtypes::TypeIdentifier::PlainArrayLarge {
                        header: #crate_path::xtypes::PlainCollectionHeader::default(),
                        array_bound_seq: vec![#size as u32],
                        element_identifier: Box::new(
                            #crate_path::xtypes::TypeIdentifier::Char8
                        ),
                    }
                };
            }
        }
    }

    match method {
        SerializationMethod::Bool => quote! { #crate_path::xtypes::TypeIdentifier::Boolean },
        SerializationMethod::U8 => quote! { #crate_path::xtypes::TypeIdentifier::Byte },
        SerializationMethod::I8 => quote! { #crate_path::xtypes::TypeIdentifier::Int8 },
        SerializationMethod::I16 => quote! { #crate_path::xtypes::TypeIdentifier::Int16 },
        SerializationMethod::I32 => quote! { #crate_path::xtypes::TypeIdentifier::Int32 },
        SerializationMethod::I64 => quote! { #crate_path::xtypes::TypeIdentifier::Int64 },
        SerializationMethod::U16 => quote! { #crate_path::xtypes::TypeIdentifier::Uint16 },
        SerializationMethod::U32 => quote! { #crate_path::xtypes::TypeIdentifier::Uint32 },
        SerializationMethod::U64 => quote! { #crate_path::xtypes::TypeIdentifier::Uint64 },
        SerializationMethod::F32 => quote! { #crate_path::xtypes::TypeIdentifier::Float32 },
        SerializationMethod::F64 => quote! { #crate_path::xtypes::TypeIdentifier::Float64 },
        SerializationMethod::Char => quote! { #crate_path::xtypes::TypeIdentifier::Char8 },
        SerializationMethod::String => quote! { #crate_path::xtypes::TypeIdentifier::String8 },
        SerializationMethod::WString => quote! { #crate_path::xtypes::TypeIdentifier::String16 },
        SerializationMethod::DdsBytes
        | SerializationMethod::VecU8
        | SerializationMethod::VecU16
        | SerializationMethod::VecU32
        | SerializationMethod::VecU64
        | SerializationMethod::VecI8
        | SerializationMethod::VecI16
        | SerializationMethod::VecI32
        | SerializationMethod::VecI64
        | SerializationMethod::VecF32
        | SerializationMethod::VecF64
        | SerializationMethod::VecBool
        | SerializationMethod::VecChar
        | SerializationMethod::VecString => {
            // Get inner type for sequences
            if let syn::Type::Path(type_path) = ty {
                if let Some(segment) = type_path.path.segments.last() {
                    if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                        if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                            let inner_id = type_to_identifier(inner_ty, crate_path, false);
                            return quote! {
                                #crate_path::xtypes::TypeIdentifier::PlainSequenceLarge {
                                    header: #crate_path::xtypes::PlainCollectionHeader::default(),
                                    bound: 0,
                                    element_identifier: Box::new(#inner_id),
                                }
                            };
                        }
                    }
                }
            }
            // Fallback
            quote! { #crate_path::xtypes::TypeIdentifier::None }
        }
        SerializationMethod::U8Array
        | SerializationMethod::U16Array
        | SerializationMethod::U32Array
        | SerializationMethod::U64Array
        | SerializationMethod::I8Array
        | SerializationMethod::I16Array
        | SerializationMethod::I32Array
        | SerializationMethod::I64Array
        | SerializationMethod::F32Array
        | SerializationMethod::F64Array
        | SerializationMethod::BoolArray
        | SerializationMethod::CharArray
        | SerializationMethod::StringArray => {
            // Arrays - get size from type
            if let syn::Type::Array(array) = ty {
                let inner_id = type_to_identifier(&array.elem, crate_path, false);
                let size = &array.len;
                return quote! {
                    #crate_path::xtypes::TypeIdentifier::PlainArrayLarge {
                        header: #crate_path::xtypes::PlainCollectionHeader::default(),
                        array_bound_seq: vec![#size as u32],
                        element_identifier: Box::new(#inner_id),
                    }
                };
            }
            // Fallback
            quote! { #crate_path::xtypes::TypeIdentifier::None }
        }
        // Fallback for complex types (structs, enums, etc.)
        SerializationMethod::Fallback => {
            // For complex types that may or may not implement HasTypeObject,
            // use a static approach based on type name hash
            let type_str = quote!(#ty).to_string();
            quote! {
                // Use type name to create a minimal identifier
                #crate_path::xtypes::TypeIdentifier::MinimalTypeId(
                    #crate_path::xtypes::EquivalenceHash::compute(#type_str.as_bytes())
                )
            }
        }
    }
}

/// Generate HasTypeObject implementation for a struct.
pub fn generate_has_type_object_impl(
    name: &syn::Ident,
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
    type_config: &DdsTypeConfig,
    gc: &super::type_struct::GenCtx,
) -> proc_macro2::TokenStream {
    let crate_path = &type_config.crate_path;
    let type_name_str = name.to_string();
    let autoid = type_config.autoid;

    // Find parent field (struct inheritance) for base_type reference
    let parent_field = fields.iter().find(|f| {
        let fc = parse_field_attributes(f);
        fc.parent
    });
    let base_type_expr = if let Some(pf) = parent_field {
        let parent_type = &pf.ty;
        let parent_type_str = quote!(#parent_type).to_string();
        quote! {
            Some(#crate_path::xtypes::TypeIdentifier::MinimalTypeId(
                #crate_path::xtypes::EquivalenceHash::compute(#parent_type_str.as_bytes())
            ))
        }
    } else {
        quote! { None }
    };

    // Generate member definitions for MinimalStructType
    let minimal_members: Vec<_> = fields
        .iter()
        .enumerate()
        .map(|(index, field)| {
            let field_name = field.ident.as_ref().unwrap();
            let field_name_str = field_name.to_string();
            let field_config = parse_field_attributes(field);
            let member_id = resolve_member_id(&field_config, &field_name_str, index, autoid);
            let type_id = type_to_identifier(&field.ty, crate_path, field_config.as_char);

            let is_key = field_config.key;
            let is_optional = field_config.optional;
            let is_must_understand = field_config.must_understand;
            let is_external = field_config.external;

            quote! {
                #crate_path::xtypes::MinimalStructMember::new(
                    #member_id,
                    #crate_path::xtypes::MemberFlag::new(
                        #crate_path::xtypes::TryConstructKind::Discard,
                        #is_external,
                        #is_optional,
                        #is_must_understand,
                        #is_key,
                        false, // is_default
                    ),
                    #type_id,
                    #field_name_str,
                )
            }
        })
        .collect();

    // Generate member definitions for CompleteStructType
    let complete_members: Vec<_> = fields
        .iter()
        .enumerate()
        .map(|(index, field)| {
            let field_name = field.ident.as_ref().unwrap();
            let field_name_str = field_name.to_string();
            let field_config = parse_field_attributes(field);
            let member_id = resolve_member_id(&field_config, &field_name_str, index, autoid);
            let type_id = type_to_identifier(&field.ty, crate_path, field_config.as_char);

            let is_key = field_config.key;
            let is_optional = field_config.optional;
            let is_must_understand = field_config.must_understand;
            let is_external = field_config.external;

            quote! {
                #crate_path::xtypes::CompleteStructMember::new(
                    #member_id,
                    #crate_path::xtypes::MemberFlag::new(
                        #crate_path::xtypes::TryConstructKind::Discard,
                        #is_external,
                        #is_optional,
                        #is_must_understand,
                        #is_key,
                        false, // is_default
                    ),
                    #type_id,
                    #field_name_str.to_string(),
                )
            }
        })
        .collect();

    // Determine extensibility kind (default: Appendable)
    let ext_kind = match type_config.extensibility.unwrap_or_default() {
        crate::codegen::type_config::ExtensibilityKind::Final => {
            quote! { #crate_path::xtypes::ExtensibilityKind::Final }
        }
        crate::codegen::type_config::ExtensibilityKind::Appendable => {
            quote! { #crate_path::xtypes::ExtensibilityKind::Appendable }
        }
        crate::codegen::type_config::ExtensibilityKind::Mutable => {
            quote! { #crate_path::xtypes::ExtensibilityKind::Mutable }
        }
    };

    let impl_generics = &gc.impl_generics;
    let ty_generics = &gc.ty_generics;
    let where_clause = &gc.where_clause;

    let type_identifier_impl = if gc.has_type_params {
        // Generic types cannot use static OnceLock; compute each time
        quote! {
            fn type_identifier() -> #crate_path::xtypes::TypeIdentifier {
                let complete = Self::complete_type_object();
                let type_obj = #crate_path::xtypes::TypeObject::Complete(complete);
                let hash = type_obj.compute_hash();
                #crate_path::xtypes::TypeIdentifier::CompleteTypeId(hash)
            }
        }
    } else {
        quote! {
            fn type_identifier() -> #crate_path::xtypes::TypeIdentifier {
                // For complex types, compute hash from CompleteTypeObject
                // This ensures consistency with DynamicTypeSupport which also uses CompleteTypeObject
                static TYPE_ID: std::sync::OnceLock<#crate_path::xtypes::TypeIdentifier> = std::sync::OnceLock::new();
                TYPE_ID.get_or_init(|| {
                    let complete = Self::complete_type_object();
                    let type_obj = #crate_path::xtypes::TypeObject::Complete(complete);
                    let hash = type_obj.compute_hash();
                    #crate_path::xtypes::TypeIdentifier::CompleteTypeId(hash)
                }).clone()
            }
        }
    };

    quote! {
        impl #impl_generics #crate_path::xtypes::HasTypeObject for #name #ty_generics #where_clause {
            #type_identifier_impl

            fn minimal_type_object() -> #crate_path::xtypes::MinimalTypeObject {
                let mut struct_type = #crate_path::xtypes::MinimalStructType::new(
                    #crate_path::xtypes::TypeFlag::new(#ext_kind, false, false),
                    #base_type_expr
                );
                #(struct_type.add_member(#minimal_members);)*
                #crate_path::xtypes::MinimalTypeObject::Struct(struct_type)
            }

            fn complete_type_object() -> #crate_path::xtypes::CompleteTypeObject {
                let mut struct_type = #crate_path::xtypes::CompleteStructType::new(
                    #crate_path::xtypes::TypeFlag::new(#ext_kind, false, false),
                    #type_name_str.to_string(),
                    #base_type_expr
                );
                #(struct_type.add_member(#complete_members);)*
                #crate_path::xtypes::CompleteTypeObject::Struct(struct_type)
            }

            fn dds_type_name() -> &'static str {
                #type_name_str
            }
        }
    }
}

/// Generate HasTypeObject implementation for an enum.
pub fn generate_has_type_object_enum_impl(
    name: &syn::Ident,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::token::Comma>,
    type_config: &DdsTypeConfig,
) -> proc_macro2::TokenStream {
    let crate_path = &type_config.crate_path;
    let type_name_str = name.to_string();

    // Generate literal definitions for MinimalEnumeratedType
    let minimal_literals: Vec<_> = variants
        .iter()
        .enumerate()
        .map(|(index, variant)| {
            let variant_name = &variant.ident;
            let variant_name_str = variant_name.to_string();
            let value = crate::codegen::utils::get_discriminant_value(variant, index) as i32;

            quote! {
                #crate_path::xtypes::MinimalEnumeratedLiteral::new(
                    #value,
                    #crate_path::xtypes::EnumeratedLiteralFlag::default(),
                    #variant_name_str,
                )
            }
        })
        .collect();

    // Generate literal definitions for CompleteEnumeratedType
    let complete_literals: Vec<_> = variants
        .iter()
        .enumerate()
        .map(|(index, variant)| {
            let variant_name = &variant.ident;
            let variant_name_str = variant_name.to_string();
            let value = crate::codegen::utils::get_discriminant_value(variant, index) as i32;

            quote! {
                #crate_path::xtypes::CompleteEnumeratedLiteral::new(
                    #value,
                    #crate_path::xtypes::EnumeratedLiteralFlag::default(),
                    #variant_name_str.to_string(),
                )
            }
        })
        .collect();

    quote! {
        impl #crate_path::xtypes::HasTypeObject for #name {
            fn type_identifier() -> #crate_path::xtypes::TypeIdentifier {
                // For complex types, compute hash from CompleteTypeObject
                // This ensures consistency with DynamicTypeSupport which also uses CompleteTypeObject
                static TYPE_ID: std::sync::OnceLock<#crate_path::xtypes::TypeIdentifier> = std::sync::OnceLock::new();
                TYPE_ID.get_or_init(|| {
                    let complete = Self::complete_type_object();
                    let type_obj = #crate_path::xtypes::TypeObject::Complete(complete);
                    let hash = type_obj.compute_hash();
                    #crate_path::xtypes::TypeIdentifier::CompleteTypeId(hash)
                }).clone()
            }

            fn minimal_type_object() -> #crate_path::xtypes::MinimalTypeObject {
                let mut enum_type = #crate_path::xtypes::MinimalEnumeratedType::new(
                    #crate_path::xtypes::TypeFlag::default(),
                    32, // bit_bound for i32
                );
                #(enum_type.add_literal(#minimal_literals);)*
                #crate_path::xtypes::MinimalTypeObject::Enum(enum_type)
            }

            fn complete_type_object() -> #crate_path::xtypes::CompleteTypeObject {
                let mut enum_type = #crate_path::xtypes::CompleteEnumeratedType::new(
                    #crate_path::xtypes::TypeFlag::default(),
                    #type_name_str.to_string(),
                    32, // bit_bound for i32
                );
                #(enum_type.add_literal(#complete_literals);)*
                #crate_path::xtypes::CompleteTypeObject::Enum(enum_type)
            }

            fn dds_type_name() -> &'static str {
                #type_name_str
            }
        }
    }
}

/// Generate HasTypeObject implementation for a union (enum with data).
pub fn generate_has_type_object_union_impl(
    name: &syn::Ident,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::token::Comma>,
    type_config: &DdsTypeConfig,
    disc_type: DiscriminantType,
) -> proc_macro2::TokenStream {
    let crate_path = &type_config.crate_path;
    let type_name_str = name.to_string();

    let disc_type_id = match disc_type {
        DiscriminantType::I32 => quote! { #crate_path::xtypes::TypeIdentifier::Int32 },
        DiscriminantType::I16 => quote! { #crate_path::xtypes::TypeIdentifier::Int16 },
        DiscriminantType::U8 => quote! { #crate_path::xtypes::TypeIdentifier::Byte },
        DiscriminantType::Bool => quote! { #crate_path::xtypes::TypeIdentifier::Boolean },
    };

    let minimal_members: Vec<_> = variants
        .iter()
        .enumerate()
        .filter(|(_, v)| variant_has_data(v))
        .map(|(index, variant)| {
            let variant_name_str = variant.ident.to_string();
            let disc_value = get_discriminant_value(variant, index) as i32;
            let member_type_id = match get_variant_type(variant) {
                Some(ty) => type_to_identifier(ty, crate_path, false),
                None => quote! { #crate_path::xtypes::TypeIdentifier::None },
            };

            quote! {
                #crate_path::xtypes::MinimalUnionMember::new(
                    #index as u32,
                    #crate_path::xtypes::MemberFlag::default(),
                    #member_type_id,
                    vec![#disc_value],
                    #variant_name_str,
                )
            }
        })
        .collect();

    let complete_members: Vec<_> = variants
        .iter()
        .enumerate()
        .filter(|(_, v)| variant_has_data(v))
        .map(|(index, variant)| {
            let variant_name_str = variant.ident.to_string();
            let disc_value = get_discriminant_value(variant, index) as i32;
            let member_type_id = match get_variant_type(variant) {
                Some(ty) => type_to_identifier(ty, crate_path, false),
                None => quote! { #crate_path::xtypes::TypeIdentifier::None },
            };

            quote! {
                #crate_path::xtypes::CompleteUnionMember::new(
                    #index as u32,
                    #crate_path::xtypes::MemberFlag::default(),
                    #member_type_id,
                    vec![#disc_value],
                    #variant_name_str.to_string(),
                )
            }
        })
        .collect();

    quote! {
        impl #crate_path::xtypes::HasTypeObject for #name {
            fn type_identifier() -> #crate_path::xtypes::TypeIdentifier {
                static TYPE_ID: std::sync::OnceLock<#crate_path::xtypes::TypeIdentifier> = std::sync::OnceLock::new();
                TYPE_ID.get_or_init(|| {
                    let complete = Self::complete_type_object();
                    let type_obj = #crate_path::xtypes::TypeObject::Complete(complete);
                    let hash = type_obj.compute_hash();
                    #crate_path::xtypes::TypeIdentifier::CompleteTypeId(hash)
                }).clone()
            }

            fn minimal_type_object() -> #crate_path::xtypes::MinimalTypeObject {
                let mut union_type = #crate_path::xtypes::MinimalUnionType::new(
                    #crate_path::xtypes::TypeFlag::default(),
                    #crate_path::xtypes::MemberFlag::default(),
                    #disc_type_id,
                );
                #(union_type.add_member(#minimal_members);)*
                #crate_path::xtypes::MinimalTypeObject::Union(union_type)
            }

            fn complete_type_object() -> #crate_path::xtypes::CompleteTypeObject {
                let mut union_type = #crate_path::xtypes::CompleteUnionType::new(
                    #crate_path::xtypes::TypeFlag::default(),
                    #crate_path::xtypes::MemberFlag::default(),
                    #disc_type_id,
                    #type_name_str.to_string(),
                );
                #(union_type.add_member(#complete_members);)*
                #crate_path::xtypes::CompleteTypeObject::Union(union_type)
            }

            fn dds_type_name() -> &'static str {
                #type_name_str
            }
        }
    }
}

/// Generate HasTypeObject implementation for a bitmask enum.
pub fn generate_has_type_object_bitmask_impl(
    name: &syn::Ident,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::token::Comma>,
    type_config: &DdsTypeConfig,
) -> proc_macro2::TokenStream {
    let crate_path = &type_config.crate_path;
    let type_name_str = name.to_string();
    let bit_bound = type_config.bit_bound.unwrap_or(32) as u16;

    let minimal_flags: Vec<_> = variants
        .iter()
        .enumerate()
        .map(|(index, variant)| {
            let variant_name_str = variant.ident.to_string();
            let position = crate::codegen::utils::get_bitmask_position(variant, index) as u16;

            quote! {
                #crate_path::xtypes::MinimalBitflag::new(
                    #position,
                    #crate_path::xtypes::MemberFlag::default(),
                    #variant_name_str,
                )
            }
        })
        .collect();

    let complete_flags: Vec<_> = variants
        .iter()
        .enumerate()
        .map(|(index, variant)| {
            let variant_name_str = variant.ident.to_string();
            let position = crate::codegen::utils::get_bitmask_position(variant, index) as u16;

            quote! {
                #crate_path::xtypes::CompleteBitflag::new(
                    #position,
                    #crate_path::xtypes::MemberFlag::default(),
                    #variant_name_str.to_string(),
                )
            }
        })
        .collect();

    quote! {
        impl #crate_path::xtypes::HasTypeObject for #name {
            fn type_identifier() -> #crate_path::xtypes::TypeIdentifier {
                static TYPE_ID: std::sync::OnceLock<#crate_path::xtypes::TypeIdentifier> = std::sync::OnceLock::new();
                TYPE_ID.get_or_init(|| {
                    let complete = Self::complete_type_object();
                    let type_obj = #crate_path::xtypes::TypeObject::Complete(complete);
                    let hash = type_obj.compute_hash();
                    #crate_path::xtypes::TypeIdentifier::CompleteTypeId(hash)
                }).clone()
            }

            fn minimal_type_object() -> #crate_path::xtypes::MinimalTypeObject {
                let mut bitmask_type = #crate_path::xtypes::MinimalBitmaskType::new(
                    #crate_path::xtypes::TypeFlag::default(),
                    #bit_bound,
                );
                #(bitmask_type.add_flag(#minimal_flags);)*
                #crate_path::xtypes::MinimalTypeObject::Bitmask(bitmask_type)
            }

            fn complete_type_object() -> #crate_path::xtypes::CompleteTypeObject {
                let mut bitmask_type = #crate_path::xtypes::CompleteBitmaskType::new(
                    #crate_path::xtypes::TypeFlag::default(),
                    #type_name_str.to_string(),
                    #bit_bound,
                );
                #(bitmask_type.add_flag(#complete_flags);)*
                #crate_path::xtypes::CompleteTypeObject::Bitmask(bitmask_type)
            }

            fn dds_type_name() -> &'static str {
                #type_name_str
            }
        }
    }
}

/// Generate HasTypeObject implementation for a bitset struct.
pub fn generate_has_type_object_bitset_impl(
    name: &syn::Ident,
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
    type_config: &DdsTypeConfig,
) -> proc_macro2::TokenStream {
    let crate_path = &type_config.crate_path;
    let type_name_str = name.to_string();

    let mut position: u16 = 0;
    let minimal_fields: Vec<_> = fields
        .iter()
        .map(|field| {
            let field_name = field.ident.as_ref().map(|i| i.to_string()).unwrap_or_default();
            let attrs = parse_field_attributes(field);
            let bitcount = attrs.bitfield.unwrap_or(1) as u8;
            let field_type_id = type_to_identifier(&field.ty, crate_path, false);
            let pos = position;
            position += bitcount as u16;

            quote! {
                #crate_path::xtypes::MinimalBitfield::new(
                    #pos,
                    #crate_path::xtypes::MemberFlag::default(),
                    #bitcount,
                    #field_type_id,
                    #field_name,
                )
            }
        })
        .collect();

    position = 0;
    let complete_fields: Vec<_> = fields
        .iter()
        .map(|field| {
            let field_name = field.ident.as_ref().map(|i| i.to_string()).unwrap_or_default();
            let attrs = parse_field_attributes(field);
            let bitcount = attrs.bitfield.unwrap_or(1) as u8;
            let field_type_id = type_to_identifier(&field.ty, crate_path, false);
            let pos = position;
            position += bitcount as u16;

            quote! {
                #crate_path::xtypes::CompleteBitfield::new(
                    #pos,
                    #crate_path::xtypes::MemberFlag::default(),
                    #bitcount,
                    #field_type_id,
                    #field_name.to_string(),
                )
            }
        })
        .collect();

    quote! {
        impl #crate_path::xtypes::HasTypeObject for #name {
            fn type_identifier() -> #crate_path::xtypes::TypeIdentifier {
                static TYPE_ID: std::sync::OnceLock<#crate_path::xtypes::TypeIdentifier> = std::sync::OnceLock::new();
                TYPE_ID.get_or_init(|| {
                    let complete = Self::complete_type_object();
                    let type_obj = #crate_path::xtypes::TypeObject::Complete(complete);
                    let hash = type_obj.compute_hash();
                    #crate_path::xtypes::TypeIdentifier::CompleteTypeId(hash)
                }).clone()
            }

            fn minimal_type_object() -> #crate_path::xtypes::MinimalTypeObject {
                let mut bitset_type = #crate_path::xtypes::MinimalBitsetType::new(
                    #crate_path::xtypes::TypeFlag::default(),
                );
                #(bitset_type.add_field(#minimal_fields);)*
                #crate_path::xtypes::MinimalTypeObject::Bitset(bitset_type)
            }

            fn complete_type_object() -> #crate_path::xtypes::CompleteTypeObject {
                let mut bitset_type = #crate_path::xtypes::CompleteBitsetType::new(
                    #crate_path::xtypes::TypeFlag::default(),
                    #type_name_str.to_string(),
                );
                #(bitset_type.add_field(#complete_fields);)*
                #crate_path::xtypes::CompleteTypeObject::Bitset(bitset_type)
            }

            fn dds_type_name() -> &'static str {
                #type_name_str
            }
        }
    }
}
