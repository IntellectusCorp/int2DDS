//! Code generation for HasTypeObject trait implementation.
//!
//! Generates TypeObject metadata for DdsType derived structs and enums.

use quote::quote;

use crate::codegen::type_config::DdsTypeConfig;
use crate::codegen::utils::{
    get_discriminant_value, get_serialization_method, get_variant_type, parse_field_attributes,
    resolve_member_id, try_construct_to_tokens, variant_has_data, DiscriminantType,
    SerializationMethod,
};

fn name_based_minimal_id(
    crate_path: &proc_macro2::TokenStream,
    name_str: &str,
) -> proc_macro2::TokenStream {
    quote! {
        #crate_path::xtypes::TypeIdentifier::MinimalTypeId(
            #crate_path::xtypes::EquivalenceHash::compute(#name_str.as_bytes())
        )
    }
}

fn xtypes_extensibility_tokens(
    extensibility: Option<crate::codegen::type_config::ExtensibilityKind>,
    crate_path: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    use crate::codegen::type_config::ExtensibilityKind;
    match extensibility.unwrap_or_default() {
        ExtensibilityKind::Final => quote! { #crate_path::xtypes::ExtensibilityKind::Final },
        ExtensibilityKind::Appendable => {
            quote! { #crate_path::xtypes::ExtensibilityKind::Appendable }
        }
        ExtensibilityKind::Mutable => quote! { #crate_path::xtypes::ExtensibilityKind::Mutable },
    }
}

/// Emit a String8/String16 `TypeIdentifier`, honoring an optional `#[dds(bound = N)]`.
/// Unbounded (`None` or `0`) -> `String8`/`String16`; `bound <= 255` -> `*Small`; else `*Large`.
/// The SMALL/LARGE choice mirrors the plain-collection rule so the serialized identifier
/// matches other DDS implementations byte-for-byte.
fn string_identifier(
    crate_path: &proc_macro2::TokenStream,
    bound: Option<usize>,
    wide: bool,
) -> proc_macro2::TokenStream {
    match bound {
        Some(b) if b > 0 && b <= 255 => {
            let b = b as u8;
            if wide {
                quote! { #crate_path::xtypes::TypeIdentifier::String16Small { bound: #b } }
            } else {
                quote! { #crate_path::xtypes::TypeIdentifier::String8Small { bound: #b } }
            }
        }
        Some(b) if b > 255 => {
            let b = b as u32;
            if wide {
                quote! { #crate_path::xtypes::TypeIdentifier::String16Large { bound: #b } }
            } else {
                quote! { #crate_path::xtypes::TypeIdentifier::String8Large { bound: #b } }
            }
        }
        _ => {
            if wide {
                quote! { #crate_path::xtypes::TypeIdentifier::String16 }
            } else {
                quote! { #crate_path::xtypes::TypeIdentifier::String8 }
            }
        }
    }
}

/// Compute the `TypeIdentifier` for a field type.
fn type_to_identifier(
    ty: &syn::Type,
    crate_path: &proc_macro2::TokenStream,
    as_char: bool,
    as_uint8: bool,
    bound: Option<usize>,
    minimal: bool,
) -> proc_macro2::TokenStream {
    if let syn::Type::Array(array) = ty {
        let inner_id =
            type_to_identifier(&array.elem, crate_path, as_char, as_uint8, None, minimal);
        let size = &array.len;
        return quote! {
            {
                let __elem = #inner_id;
                let __equiv = #crate_path::xtypes::plain_collection_equiv_kind(&__elem);
                let __flags = #crate_path::xtypes::CollectionElementFlag::default();
                let __n: usize = #size;
                if __n <= 255 {
                    #crate_path::xtypes::TypeIdentifier::PlainArraySmall {
                        header: #crate_path::xtypes::PlainCollectionHeader {
                            equiv_kind: __equiv,
                            element_flags: __flags,
                        },
                        array_bound_seq: vec![__n as u8],
                        element_identifier: Box::new(__elem),
                    }
                } else {
                    #crate_path::xtypes::TypeIdentifier::PlainArrayLarge {
                        header: #crate_path::xtypes::PlainCollectionHeader {
                            equiv_kind: __equiv,
                            element_flags: __flags,
                        },
                        array_bound_seq: vec![__n as u32],
                        element_identifier: Box::new(__elem),
                    }
                }
            }
        };
    }

    if let syn::Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            let wrapper = segment.ident.to_string();
            if matches!(wrapper.as_str(), "Vec" | "Option" | "Box") {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                        if wrapper == "Vec" {
                            let inner_id = type_to_identifier(
                                inner_ty, crate_path, as_char, as_uint8, None, minimal,
                            );
                            let seq_bound = bound.unwrap_or(0);
                            if seq_bound <= 255 {
                                let b = seq_bound as u8;
                                return quote! {
                                    {
                                        let __elem = #inner_id;
                                        #crate_path::xtypes::TypeIdentifier::PlainSequenceSmall {
                                            header: #crate_path::xtypes::PlainCollectionHeader {
                                                equiv_kind: #crate_path::xtypes::plain_collection_equiv_kind(&__elem),
                                                element_flags: #crate_path::xtypes::CollectionElementFlag::default(),
                                            },
                                            bound: #b,
                                            element_identifier: Box::new(__elem),
                                        }
                                    }
                                };
                            }
                            let b = seq_bound as u32;
                            return quote! {
                                {
                                    let __elem = #inner_id;
                                    #crate_path::xtypes::TypeIdentifier::PlainSequenceLarge {
                                        header: #crate_path::xtypes::PlainCollectionHeader {
                                            equiv_kind: #crate_path::xtypes::plain_collection_equiv_kind(&__elem),
                                            element_flags: #crate_path::xtypes::CollectionElementFlag::default(),
                                        },
                                        bound: #b,
                                        element_identifier: Box::new(__elem),
                                    }
                                }
                            };
                        }
                        // Option<T> / Box<T>: transparent wrappers; propagate all hints to the inner type.
                        return type_to_identifier(
                            inner_ty, crate_path, as_char, as_uint8, bound, minimal,
                        );
                    }
                }
            }
        }
    }

    if matches!(get_serialization_method(ty), SerializationMethod::U8) {
        if as_char {
            return quote! { #crate_path::xtypes::TypeIdentifier::Char8 };
        }
        if as_uint8 {
            return quote! { #crate_path::xtypes::TypeIdentifier::Uint8 };
        }
    }

    match get_serialization_method(ty) {
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
        SerializationMethod::String => string_identifier(crate_path, bound, false),
        SerializationMethod::WString => string_identifier(crate_path, bound, true),
        _ => {
            let type_str = quote!(#ty).to_string();
            let fallback = name_based_minimal_id(crate_path, &type_str);
            let method = if minimal {
                quote! { minimal_member_id }
            } else {
                quote! { complete_member_id }
            };
            quote! {
                {
                    use #crate_path::xtypes::member_id::MemberIdFallback as _;
                    #crate_path::xtypes::member_id::Probe::<#ty>(::core::marker::PhantomData)
                        .#method(#fallback)
                }
            }
        }
    }
}

/// Generate the `type_identifier()` (EK_COMPLETE) and `minimal_type_identifier()`
/// (EK_MINIMAL) method bodies, both wrapped in the reentrancy guard. Non-generic
/// types cache in a `OnceLock`; generic types recompute each call.
fn generate_type_id_methods(
    crate_path: &proc_macro2::TokenStream,
    name: &syn::Ident,
    has_type_params: bool,
) -> proc_macro2::TokenStream {
    let fallback = name_based_minimal_id(crate_path, &name.to_string());
    if has_type_params {
        quote! {
            fn type_identifier() -> #crate_path::xtypes::TypeIdentifier {
                #crate_path::xtypes::recursion_guard::with_type_id_guard::<Self>(#fallback, || {
                    let complete = Self::complete_type_object();
                    let hash = #crate_path::xtypes::TypeObject::Complete(complete).compute_hash();
                    #crate_path::xtypes::TypeIdentifier::CompleteTypeId(hash)
                })
            }

            fn minimal_type_identifier() -> #crate_path::xtypes::TypeIdentifier {
                #crate_path::xtypes::recursion_guard::with_type_id_guard::<Self>(#fallback, || {
                    let minimal = Self::minimal_type_object();
                    let hash = #crate_path::xtypes::TypeObject::Minimal(minimal).compute_hash();
                    #crate_path::xtypes::TypeIdentifier::MinimalTypeId(hash)
                })
            }
        }
    } else {
        quote! {
            fn type_identifier() -> #crate_path::xtypes::TypeIdentifier {
                static TYPE_ID: std::sync::OnceLock<#crate_path::xtypes::TypeIdentifier> = std::sync::OnceLock::new();
                #crate_path::xtypes::recursion_guard::with_type_id_guard::<Self>(#fallback, || {
                    TYPE_ID.get_or_init(|| {
                        let complete = Self::complete_type_object();
                        let hash = #crate_path::xtypes::TypeObject::Complete(complete).compute_hash();
                        #crate_path::xtypes::TypeIdentifier::CompleteTypeId(hash)
                    }).clone()
                })
            }

            fn minimal_type_identifier() -> #crate_path::xtypes::TypeIdentifier {
                static MINIMAL_ID: std::sync::OnceLock<#crate_path::xtypes::TypeIdentifier> = std::sync::OnceLock::new();
                #crate_path::xtypes::recursion_guard::with_type_id_guard::<Self>(#fallback, || {
                    MINIMAL_ID.get_or_init(|| {
                        let minimal = Self::minimal_type_object();
                        let hash = #crate_path::xtypes::TypeObject::Minimal(minimal).compute_hash();
                        #crate_path::xtypes::TypeIdentifier::MinimalTypeId(hash)
                    }).clone()
                })
            }
        }
    }
}

/// Emit `CompleteTypeDetail.ann_builtin` assignment from type-level annotation flags, or empty tokens when none apply.
fn build_type_ann_builtin(
    type_config: &DdsTypeConfig,
    crate_path: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let has_builtin = type_config.nested || type_config.data_representation_mask.is_some();
    if !has_builtin {
        return quote! {};
    }

    let nested_expr = if type_config.nested {
        quote! { Some(true) }
    } else {
        quote! { None }
    };
    let data_rep_expr = match type_config.data_representation_mask {
        Some(mask) => {
            let mask_u16 = (mask & 0xFFFF) as u16;
            quote! { Some(#mask_u16) }
        }
        None => quote! { None },
    };

    quote! {
        struct_type.header.detail.ann_builtin = Some(#crate_path::xtypes::AppliedBuiltinTypeAnnotations {
            verbatim: None,
            nested: #nested_expr,
            data_representation: #data_rep_expr,
        });
    }
}

/// Generate the `HasTypeObject::collect_nested_type_objects` method body.
fn generate_collect_nested(
    crate_path: &proc_macro2::TokenStream,
    type_name_str: &str,
    field_types: &[&syn::Type],
) -> proc_macro2::TokenStream {
    let fallback_use = if field_types.is_empty() {
        quote! {}
    } else {
        quote! { use #crate_path::xtypes::nested_closure::CollectFallback as _; }
    };
    let _ = type_name_str;
    let id_expr = quote! { <Self as #crate_path::xtypes::HasTypeObject>::type_identifier() };
    quote! {
        fn collect_nested_type_objects(
            out: &mut Vec<(#crate_path::xtypes::TypeIdentifier, #crate_path::xtypes::TypeObject)>,
        ) {
            #fallback_use
            let __id = #id_expr;
            if let Some(__h) = __id.equivalence_hash().copied() {
                if out.iter().any(|(__i, _)| __i.equivalence_hash() == Some(&__h)) {
                    return;
                }
            }
            out.push((
                __id,
                #crate_path::xtypes::TypeObject::Complete(Self::complete_type_object()),
            ));
            #(
                #crate_path::xtypes::nested_closure::Probe::<#field_types>(::core::marker::PhantomData)
                    .collect_nested(out);
            )*
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
    let type_name_str = type_config.type_name.clone().unwrap_or_else(|| name.to_string());
    let autoid = type_config.autoid;

    // Find parent field (struct inheritance) for base_type reference
    let parent_field = fields.iter().find(|f| {
        let fc = parse_field_attributes(f);
        fc.parent
    });
    let (base_type_expr_complete, base_type_expr_minimal) = if let Some(pf) = parent_field {
        let parent_type = &pf.ty;
        let complete_id = type_to_identifier(parent_type, crate_path, false, false, None, false);
        let minimal_id = type_to_identifier(parent_type, crate_path, false, false, None, true);
        (quote! { Some(#complete_id) }, quote! { Some(#minimal_id) })
    } else {
        (quote! { None }, quote! { None })
    };

    // Generate member definitions for MinimalStructType
    let minimal_members: Vec<_> = fields
        .iter()
        .enumerate()
        .filter_map(|(index, field)| {
            let field_name = field.ident.as_ref().unwrap();
            let field_name_str = field_name.to_string();
            let field_config = parse_field_attributes(field);
            if field_config.non_serialized {
                return None;
            }
            let member_id = resolve_member_id(&field_config, &field_name_str, index, autoid);
            let type_id = type_to_identifier(
                &field.ty,
                crate_path,
                field_config.as_char,
                field_config.as_uint8,
                field_config.bound,
                true,
            );

            let is_key = field_config.key;
            let is_optional = field_config.optional;
            // XTypes 7.2.2.4.4.4.7: key members are implicitly must-understand.
            let is_must_understand = field_config.must_understand || field_config.key;
            let is_external = field_config.external;
            let try_construct = try_construct_to_tokens(field_config.try_construct, crate_path);

            Some(quote! {
                #crate_path::xtypes::MinimalStructMember::new(
                    #member_id,
                    #crate_path::xtypes::MemberFlag::new(
                        #try_construct,
                        #is_external,
                        #is_optional,
                        #is_must_understand,
                        #is_key,
                        false, // is_default
                    ),
                    #type_id,
                    #field_name_str,
                )
            })
        })
        .collect();

    // Generate member definitions for CompleteStructType
    let complete_members: Vec<_> = fields
        .iter()
        .enumerate()
        .filter_map(|(index, field)| {
            let field_name = field.ident.as_ref().unwrap();
            let field_name_str = field_name.to_string();
            let field_config = parse_field_attributes(field);
            if field_config.non_serialized {
                return None;
            }
            let member_id = resolve_member_id(&field_config, &field_name_str, index, autoid);
            let type_id = type_to_identifier(
                &field.ty,
                crate_path,
                field_config.as_char,
                field_config.as_uint8,
                field_config.bound,
                false,
            );

            let is_key = field_config.key;
            let is_optional = field_config.optional;
            // XTypes 7.2.2.4.4.4.7: key members are implicitly must-understand.
            let is_must_understand = field_config.must_understand || field_config.key;
            let is_external = field_config.external;
            let try_construct = try_construct_to_tokens(field_config.try_construct, crate_path);
            let hashid_expr = match field_config.hashid.as_ref() {
                Some(name) if !name.is_empty() => quote! { Some(#name.to_string()) },
                Some(_) => quote! { Some(#field_name_str.to_string()) },
                None => quote! { None },
            };

            Some(quote! {
                {
                    let mut member = #crate_path::xtypes::CompleteStructMember::new(
                        #member_id,
                        #crate_path::xtypes::MemberFlag::new(
                            #try_construct,
                            #is_external,
                            #is_optional,
                            #is_must_understand,
                            #is_key,
                            false,
                        ),
                        #type_id,
                        #field_name_str.to_string(),
                    );
                    let hash_id: Option<String> = #hashid_expr;
                    if hash_id.is_some() {
                        member.detail.ann_builtin = Some(#crate_path::xtypes::AppliedBuiltinMemberAnnotations {
                            unit: None,
                            min: None,
                            max: None,
                            hash_id,
                        });
                    }
                    member
                }
            })
        })
        .collect();

    // Determine extensibility kind (default: Appendable)
    let ext_kind = xtypes_extensibility_tokens(type_config.extensibility, crate_path);
    let is_nested = type_config.nested;
    let is_autoid_hash =
        matches!(type_config.autoid, Some(crate::codegen::utils::AutoIdKind::Hash));
    let type_ann_expr = build_type_ann_builtin(type_config, crate_path);

    let impl_generics = &gc.impl_generics;
    let ty_generics = &gc.ty_generics;
    let where_clause = &gc.where_clause;

    let collect_nested_impl = if gc.has_type_params {
        quote! {}
    } else {
        let nested_field_types: Vec<&syn::Type> = fields
            .iter()
            .filter(|f| !parse_field_attributes(f).non_serialized)
            .map(|f| &f.ty)
            .collect();
        generate_collect_nested(crate_path, &name.to_string(), &nested_field_types)
    };

    let type_identifier_impl = generate_type_id_methods(crate_path, name, gc.has_type_params);

    quote! {
        impl #impl_generics #crate_path::xtypes::HasTypeObject for #name #ty_generics #where_clause {
            #type_identifier_impl

            fn minimal_type_object() -> #crate_path::xtypes::MinimalTypeObject {
                let mut struct_type = #crate_path::xtypes::MinimalStructType::new(
                    #crate_path::xtypes::TypeFlag::new(#ext_kind, #is_nested, #is_autoid_hash),
                    #base_type_expr_minimal
                );
                #(struct_type.add_member(#minimal_members);)*
                #crate_path::xtypes::MinimalTypeObject::Struct(struct_type)
            }

            fn complete_type_object() -> #crate_path::xtypes::CompleteTypeObject {
                let mut struct_type = #crate_path::xtypes::CompleteStructType::new(
                    #crate_path::xtypes::TypeFlag::new(#ext_kind, #is_nested, #is_autoid_hash),
                    #type_name_str.to_string(),
                    #base_type_expr_complete
                );
                #(struct_type.add_member(#complete_members);)*
                #type_ann_expr
                #crate_path::xtypes::CompleteTypeObject::Struct(struct_type)
            }

            fn dds_type_name() -> &'static str {
                #type_name_str
            }

            #collect_nested_impl
        }
    }
}

/// Generate HasTypeObject implementation for an alias (typedef-style) newtype struct.
/// Emits a `TK_ALIAS` TypeObject whose base is the inner field's `TypeIdentifier`.
pub fn generate_has_type_object_alias_impl(
    name: &syn::Ident,
    inner_ty: &syn::Type,
    type_config: &DdsTypeConfig,
) -> proc_macro2::TokenStream {
    let crate_path = &type_config.crate_path;
    let type_name_str = type_config.type_name.clone().unwrap_or_else(|| name.to_string());
    let related_type_complete = type_to_identifier(inner_ty, crate_path, false, false, None, false);
    let related_type_minimal = type_to_identifier(inner_ty, crate_path, false, false, None, true);
    let collect_nested_impl = generate_collect_nested(crate_path, &name.to_string(), &[inner_ty]);
    let id_methods = generate_type_id_methods(crate_path, name, false);

    quote! {
        impl #crate_path::xtypes::HasTypeObject for #name {
            #id_methods

            fn minimal_type_object() -> #crate_path::xtypes::MinimalTypeObject {
                #crate_path::xtypes::MinimalTypeObject::Alias(
                    #crate_path::xtypes::MinimalAliasType::new(
                        #crate_path::xtypes::TypeFlag::default(),
                        #crate_path::xtypes::MemberFlag::default(),
                        #related_type_minimal,
                    )
                )
            }

            fn complete_type_object() -> #crate_path::xtypes::CompleteTypeObject {
                #crate_path::xtypes::CompleteTypeObject::Alias(
                    #crate_path::xtypes::CompleteAliasType::new(
                        #crate_path::xtypes::TypeFlag::default(),
                        #type_name_str.to_string(),
                        #crate_path::xtypes::MemberFlag::default(),
                        #related_type_complete,
                    )
                )
            }

            fn dds_type_name() -> &'static str {
                #type_name_str
            }

            #collect_nested_impl
        }
    }
}

/// Generate HasTypeObject implementation for an enum.
pub fn generate_has_type_object_enum_impl(
    name: &syn::Ident,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::token::Comma>,
    type_config: &DdsTypeConfig,
    disc_type: DiscriminantType,
) -> proc_macro2::TokenStream {
    let crate_path = &type_config.crate_path;
    let type_name_str = type_config.type_name.clone().unwrap_or_else(|| name.to_string());
    let bit_bound = disc_type.bit_bound();

    let literal_flag_expr = |variant: &syn::Variant| {
        if crate::codegen::utils::variant_is_default_literal(variant) {
            quote! { #crate_path::xtypes::EnumeratedLiteralFlag::DEFAULT }
        } else {
            quote! { #crate_path::xtypes::EnumeratedLiteralFlag::default() }
        }
    };
    let sequence = crate::codegen::utils::compute_enumerated_values(variants);

    // Generate literal definitions for MinimalEnumeratedType
    let minimal_literals: Vec<_> = variants
        .iter()
        .enumerate()
        .map(|(index, variant)| {
            let variant_name = &variant.ident;
            let variant_name_str = variant_name.to_string();
            let value = sequence[index] as i32;
            let flag = literal_flag_expr(variant);

            quote! {
                #crate_path::xtypes::MinimalEnumeratedLiteral::new(
                    #value,
                    #flag,
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
            let value = sequence[index] as i32;
            let flag = literal_flag_expr(variant);

            quote! {
                #crate_path::xtypes::CompleteEnumeratedLiteral::new(
                    #value,
                    #flag,
                    #variant_name_str.to_string(),
                )
            }
        })
        .collect();

    let collect_nested_impl = generate_collect_nested(crate_path, &name.to_string(), &[]);
    let id_methods = generate_type_id_methods(crate_path, name, false);

    quote! {
        impl #crate_path::xtypes::HasTypeObject for #name {
            #id_methods

            fn minimal_type_object() -> #crate_path::xtypes::MinimalTypeObject {
                let mut enum_type = #crate_path::xtypes::MinimalEnumeratedType::new(
                    #crate_path::xtypes::TypeFlag::default(),
                    #bit_bound,
                );
                #(enum_type.add_literal(#minimal_literals);)*
                #crate_path::xtypes::MinimalTypeObject::Enum(enum_type)
            }

            fn complete_type_object() -> #crate_path::xtypes::CompleteTypeObject {
                let mut enum_type = #crate_path::xtypes::CompleteEnumeratedType::new(
                    #crate_path::xtypes::TypeFlag::default(),
                    #type_name_str.to_string(),
                    #bit_bound,
                );
                #(enum_type.add_literal(#complete_literals);)*
                #crate_path::xtypes::CompleteTypeObject::Enum(enum_type)
            }

            fn dds_type_name() -> &'static str {
                #type_name_str
            }

            #collect_nested_impl
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
    let type_name_str = type_config.type_name.clone().unwrap_or_else(|| name.to_string());

    let union_ext_kind = xtypes_extensibility_tokens(type_config.extensibility, crate_path);
    let union_flags = quote! { #crate_path::xtypes::TypeFlag::new(#union_ext_kind, false, false) };

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
                Some(ty) => type_to_identifier(ty, crate_path, false, false, None, true),
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
                Some(ty) => type_to_identifier(ty, crate_path, false, false, None, false),
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

    let nested_field_types: Vec<&syn::Type> =
        variants.iter().filter(|v| variant_has_data(v)).filter_map(get_variant_type).collect();
    let collect_nested_impl =
        generate_collect_nested(crate_path, &name.to_string(), &nested_field_types);
    let id_methods = generate_type_id_methods(crate_path, name, false);

    quote! {
        impl #crate_path::xtypes::HasTypeObject for #name {
            #id_methods

            fn minimal_type_object() -> #crate_path::xtypes::MinimalTypeObject {
                let mut union_type = #crate_path::xtypes::MinimalUnionType::new(
                    #union_flags,
                    #crate_path::xtypes::MemberFlag::default(),
                    #disc_type_id,
                );
                #(union_type.add_member(#minimal_members);)*
                #crate_path::xtypes::MinimalTypeObject::Union(union_type)
            }

            fn complete_type_object() -> #crate_path::xtypes::CompleteTypeObject {
                let mut union_type = #crate_path::xtypes::CompleteUnionType::new(
                    #union_flags,
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

            #collect_nested_impl
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
    let type_name_str = type_config.type_name.clone().unwrap_or_else(|| name.to_string());
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

    let collect_nested_impl = generate_collect_nested(crate_path, &name.to_string(), &[]);
    let id_methods = generate_type_id_methods(crate_path, name, false);

    quote! {
        impl #crate_path::xtypes::HasTypeObject for #name {
            #id_methods

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

            #collect_nested_impl
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
    let type_name_str = type_config.type_name.clone().unwrap_or_else(|| name.to_string());

    let mut position: u16 = 0;
    let minimal_fields: Vec<_> = fields
        .iter()
        .map(|field| {
            let field_name = field.ident.as_ref().map(|i| i.to_string()).unwrap_or_default();
            let attrs = parse_field_attributes(field);
            let bitcount = attrs.bitfield.unwrap_or(1);
            let field_type_id = type_to_identifier(&field.ty, crate_path, false, false, None, true);
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
            let bitcount = attrs.bitfield.unwrap_or(1);
            let field_type_id =
                type_to_identifier(&field.ty, crate_path, false, false, None, false);
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

    let collect_nested_impl = generate_collect_nested(crate_path, &name.to_string(), &[]);
    let id_methods = generate_type_id_methods(crate_path, name, false);

    quote! {
        impl #crate_path::xtypes::HasTypeObject for #name {
            #id_methods

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

            #collect_nested_impl
        }
    }
}
