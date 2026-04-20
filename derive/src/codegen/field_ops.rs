use quote::quote;

use crate::codegen::utils::{
    get_array_size, get_serialization_method, is_map_type, is_option_type, literal_to_tokens,
    parse_field_attributes, FieldConfig, SerializationMethod, TryConstructKind,
};

/// Emit the local binding for a `@non_serialized` field. Uses `@default` literal if present,
/// otherwise `Default::default()`.
pub fn non_serialized_binding(
    field_name: &syn::Ident,
    field_type: &syn::Type,
    field_config: &FieldConfig,
) -> proc_macro2::TokenStream {
    if let Some(lit) = &field_config.default {
        let default_expr = literal_to_tokens(lit, field_type);
        quote! { let #field_name: #field_type = #default_expr; }
    } else {
        quote! { let #field_name: #field_type = Default::default(); }
    }
}

/// Which Err(..) variant the generated bound-violation code should return.
#[derive(Debug, Clone, Copy)]
pub enum DeserErrorKind {
    /// TypeSupport / DdsType path returning `DdsResult`.
    Dds,
    /// CdrDeserialize / XcdrDeserialize trait path returning `SerializationResult`.
    Serialization,
}

/// Post-deserialize bound enforcement for the given field (applies @try_construct).
/// Emits nothing for types where bound is not applicable (primitives, arrays, maps, custom).
/// Expects `#field_name` to be a `mut` binding in scope.
pub fn gen_post_deserialize_bound_check(
    field_name: &syn::Ident,
    field_type: &syn::Type,
    bound: Option<usize>,
    try_construct: TryConstructKind,
    crate_path: &proc_macro2::TokenStream,
    err_kind: DeserErrorKind,
) -> proc_macro2::TokenStream {
    let Some(max_len) = bound else {
        return quote! {};
    };
    let method = get_serialization_method(field_type);

    match method {
        SerializationMethod::String => {
            let truncate = quote! {
                while #field_name.len() > #max_len {
                    #field_name.pop();
                }
            };
            let action = gen_bound_violation_action(
                field_name,
                "string field",
                quote! { #field_name.len() },
                quote! { #max_len },
                crate_path,
                try_construct,
                err_kind,
                truncate,
            );
            quote! {
                if #field_name.len() > #max_len {
                    #action
                }
            }
        }
        SerializationMethod::WString => {
            let truncate = quote! {
                let mut __inner = #field_name.as_str().to_string();
                while __inner.encode_utf16().count() > #max_len {
                    __inner.pop();
                }
                #field_name = #crate_path::serialize::core::bounded_types::WString::new(__inner);
            };
            let action = gen_bound_violation_action(
                field_name,
                "WString field",
                quote! { __utf16_len },
                quote! { #max_len },
                crate_path,
                try_construct,
                err_kind,
                truncate,
            );
            quote! {
                {
                    let __utf16_len = #field_name.as_str().encode_utf16().count();
                    if __utf16_len > #max_len {
                        #action
                    }
                }
            }
        }
        SerializationMethod::VecU8
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
            let truncate = quote! { #field_name.truncate(#max_len); };
            let action = gen_bound_violation_action(
                field_name,
                "sequence field",
                quote! { #field_name.len() },
                quote! { #max_len },
                crate_path,
                try_construct,
                err_kind,
                truncate,
            );
            quote! {
                if #field_name.len() > #max_len {
                    #action
                }
            }
        }
        _ => quote! {},
    }
}

/// Emit the deserialize-side action for a bound violation according to @try_construct.
/// `truncate` is the type-specific truncation code (ignored for Discard/UseDefault).
fn gen_bound_violation_action(
    field_name: &syn::Ident,
    field_kind: &str,
    len_expr: proc_macro2::TokenStream,
    max_len: proc_macro2::TokenStream,
    crate_path: &proc_macro2::TokenStream,
    try_construct: TryConstructKind,
    err_kind: DeserErrorKind,
    truncate: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    match try_construct {
        TryConstructKind::Discard => {
            let msg = quote! {
                format!("Deserialized {} '{}' length {} exceeds maximum {}",
                    #field_kind, stringify!(#field_name), #len_expr, #max_len)
            };
            let err_ctor = match err_kind {
                DeserErrorKind::Dds => quote! {
                    #crate_path::dcps::core::error::DdsError::Error(#msg)
                },
                DeserErrorKind::Serialization => quote! {
                    #crate_path::serialize::cdr::CdrError::DeserializationError(#msg)
                },
            };
            quote! { return Err(#err_ctor); }
        }
        TryConstructKind::UseDefault => quote! {
            #field_name = Default::default();
        },
        TryConstructKind::Trim => truncate,
    }
}

/// Helper function to generate serialize code for primitive types
fn gen_primitive_serialize(
    method: &str,
    field_name: &syn::Ident,
    crate_path: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let method_ident = syn::Ident::new(method, field_name.span());
    quote! {
        serializer.#method_ident(typed_data.#field_name)
            .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
    }
}

/// Helper function to generate serialize code for variable length sequences
fn gen_sequence_serialize(
    method: &str,
    field_name: &syn::Ident,
    crate_path: &proc_macro2::TokenStream,
    bound: Option<usize>,
) -> proc_macro2::TokenStream {
    let method_ident = syn::Ident::new(method, field_name.span());

    if let Some(max_len) = bound {
        quote! {
            if typed_data.#field_name.len() > #max_len {
                return Err(#crate_path::dcps::core::error::DdsError::Error(
                    format!("Sequence field '{}' length {} exceeds maximum {}",
                        stringify!(#field_name), typed_data.#field_name.len(), #max_len)
                ));
            }
            serializer.#method_ident(&typed_data.#field_name)
                .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
        }
    } else {
        quote! {
            serializer.#method_ident(&typed_data.#field_name)
                .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
        }
    }
}

/// Helper function to generate serialize code for fixed-size arrays
fn gen_array_serialize(
    method: &str,
    field_name: &syn::Ident,
    crate_path: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let method_ident = syn::Ident::new(method, field_name.span());
    quote! {
        serializer.#method_ident(&typed_data.#field_name)
            .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
    }
}

/// Helper function to generate deserialize code for primitive types
fn gen_primitive_deserialize(
    method: &str,
    field_name: &syn::Ident,
    crate_path: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let method_ident = syn::Ident::new(method, field_name.span());
    quote! {
        let #field_name = deserializer.#method_ident()
            .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
    }
}

/// Helper function to generate deserialize code for sequences with bound checking
fn gen_sequence_deserialize(
    method: &str,
    field_name: &syn::Ident,
    crate_path: &proc_macro2::TokenStream,
    bound: Option<usize>,
    try_construct: TryConstructKind,
) -> proc_macro2::TokenStream {
    let method_ident = syn::Ident::new(method, field_name.span());

    if let Some(max_len) = bound {
        let truncate = quote! { #field_name.truncate(#max_len); };
        let action = gen_bound_violation_action(
            field_name,
            "sequence field",
            quote! { #field_name.len() },
            quote! { #max_len },
            crate_path,
            try_construct,
            DeserErrorKind::Dds,
            truncate,
        );
        quote! {
            let mut #field_name = deserializer.#method_ident()
                .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
            if #field_name.len() > #max_len {
                #action
            }
        }
    } else {
        quote! {
            let #field_name = deserializer.#method_ident()
                .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
        }
    }
}

#[allow(clippy::unnecessary_unwrap)]
fn gen_serialize_code(
    method: SerializationMethod,
    field_name: &syn::Ident,
    field_type: &syn::Type,
    crate_path: &proc_macro2::TokenStream,
    xcdr: bool,
    bound: Option<usize>,
) -> proc_macro2::TokenStream {
    match method {
        SerializationMethod::U8 => gen_primitive_serialize("serialize_u8", field_name, crate_path),
        SerializationMethod::U16 => {
            gen_primitive_serialize("serialize_u16", field_name, crate_path)
        }
        SerializationMethod::U32 => {
            gen_primitive_serialize("serialize_u32", field_name, crate_path)
        }
        SerializationMethod::U64 => {
            gen_primitive_serialize("serialize_u64", field_name, crate_path)
        }
        SerializationMethod::I8 => gen_primitive_serialize("serialize_i8", field_name, crate_path),
        SerializationMethod::I16 => {
            gen_primitive_serialize("serialize_i16", field_name, crate_path)
        }
        SerializationMethod::I32 => {
            gen_primitive_serialize("serialize_i32", field_name, crate_path)
        }
        SerializationMethod::I64 => {
            gen_primitive_serialize("serialize_i64", field_name, crate_path)
        }
        SerializationMethod::F32 => {
            gen_primitive_serialize("serialize_f32", field_name, crate_path)
        }
        SerializationMethod::F64 => {
            gen_primitive_serialize("serialize_f64", field_name, crate_path)
        }
        SerializationMethod::Char => {
            gen_primitive_serialize("serialize_char", field_name, crate_path)
        }
        SerializationMethod::Bool => {
            gen_primitive_serialize("serialize_bool", field_name, crate_path)
        }
        SerializationMethod::String => {
            if let Some(max_len) = bound {
                quote! {
                    if typed_data.#field_name.len() > #max_len {
                        return Err(#crate_path::dcps::core::error::DdsError::Error(
                            format!("String field '{}' length {} exceeds maximum {}",
                                stringify!(#field_name), typed_data.#field_name.len(), #max_len)
                        ));
                    }
                    serializer.serialize_string(&typed_data.#field_name)
                        .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
                }
            } else {
                quote! {
                    serializer.serialize_string(&typed_data.#field_name)
                        .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
                }
            }
        }
        SerializationMethod::WString => {
            let trait_path = if xcdr {
                quote!(#crate_path::serialize::xcdr::XcdrSerialize::serialize_xcdr)
            } else {
                quote!(#crate_path::serialize::cdr::CdrSerialize::serialize_cdr)
            };

            if let Some(max_len) = bound {
                quote! {
                    {
                        let __utf16_len = typed_data.#field_name.as_str().encode_utf16().count();
                        if __utf16_len > #max_len {
                            return Err(#crate_path::dcps::core::error::DdsError::Error(
                                format!("WString field '{}' UTF-16 length {} exceeds bound {}",
                                    stringify!(#field_name), __utf16_len, #max_len)
                            ));
                        }
                    }
                    #trait_path(&typed_data.#field_name, &mut serializer)
                        .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
                }
            } else {
                quote! {
                    #trait_path(&typed_data.#field_name, &mut serializer)
                        .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
                }
            }
        }
        SerializationMethod::U8Array => {
            gen_array_serialize("serialize_byte_array", field_name, crate_path)
        }
        SerializationMethod::U16Array => {
            gen_array_serialize("serialize_u16_array", field_name, crate_path)
        }
        SerializationMethod::U32Array => {
            gen_array_serialize("serialize_u32_array", field_name, crate_path)
        }
        SerializationMethod::U64Array => {
            gen_array_serialize("serialize_u64_array", field_name, crate_path)
        }
        SerializationMethod::I8Array => {
            gen_array_serialize("serialize_i8_array", field_name, crate_path)
        }
        SerializationMethod::I16Array => {
            gen_array_serialize("serialize_i16_array", field_name, crate_path)
        }
        SerializationMethod::I32Array => {
            gen_array_serialize("serialize_i32_array", field_name, crate_path)
        }
        SerializationMethod::I64Array => {
            gen_array_serialize("serialize_i64_array", field_name, crate_path)
        }
        SerializationMethod::F32Array => {
            gen_array_serialize("serialize_f32_array", field_name, crate_path)
        }
        SerializationMethod::F64Array => {
            gen_array_serialize("serialize_f64_array", field_name, crate_path)
        }
        SerializationMethod::VecU8 => {
            gen_sequence_serialize("serialize_byte_sequence", field_name, crate_path, bound)
        }
        SerializationMethod::VecU16 => {
            gen_sequence_serialize("serialize_u16_sequence", field_name, crate_path, bound)
        }
        SerializationMethod::VecU32 => {
            gen_sequence_serialize("serialize_u32_sequence", field_name, crate_path, bound)
        }
        SerializationMethod::VecU64 => {
            gen_sequence_serialize("serialize_u64_sequence", field_name, crate_path, bound)
        }
        SerializationMethod::VecI8 => {
            gen_sequence_serialize("serialize_i8_sequence", field_name, crate_path, bound)
        }
        SerializationMethod::VecI16 => {
            gen_sequence_serialize("serialize_i16_sequence", field_name, crate_path, bound)
        }
        SerializationMethod::VecI32 => {
            gen_sequence_serialize("serialize_i32_sequence", field_name, crate_path, bound)
        }
        SerializationMethod::VecI64 => {
            gen_sequence_serialize("serialize_i64_sequence", field_name, crate_path, bound)
        }
        SerializationMethod::VecF32 => {
            gen_sequence_serialize("serialize_f32_sequence", field_name, crate_path, bound)
        }
        SerializationMethod::VecF64 => {
            gen_sequence_serialize("serialize_f64_sequence", field_name, crate_path, bound)
        }
        SerializationMethod::VecBool => {
            gen_sequence_serialize("serialize_bool_sequence", field_name, crate_path, bound)
        }
        SerializationMethod::VecChar => {
            gen_sequence_serialize("serialize_char_sequence", field_name, crate_path, bound)
        }
        SerializationMethod::VecString => {
            gen_sequence_serialize("serialize_string_sequence", field_name, crate_path, bound)
        }
        SerializationMethod::BoolArray => {
            gen_array_serialize("serialize_bool_array", field_name, crate_path)
        }
        SerializationMethod::CharArray => {
            gen_array_serialize("serialize_char_array_fixed", field_name, crate_path)
        }
        SerializationMethod::StringArray => {
            gen_array_serialize("serialize_string_array", field_name, crate_path)
        }
        SerializationMethod::Fallback => {
            if !xcdr && is_option_type(field_type) {
                let msg = format!(
                    "XCDR1 does not support Option<T> field '{}' (PL_CDR v1 pending T2-1); use XCDR2",
                    field_name
                );
                return quote! {
                    (|| -> ::std::result::Result<(), #crate_path::dcps::core::error::DdsError> {
                        Err(#crate_path::dcps::core::error::DdsError::Error(#msg.to_string()))
                    })()?;
                };
            }
            let trait_path = if xcdr {
                quote!(#crate_path::serialize::xcdr::XcdrSerialize::serialize_xcdr)
            } else {
                quote!(#crate_path::serialize::cdr::CdrSerialize::serialize_cdr)
            };

            // Check if this is a map type with a bound
            if is_map_type(field_type) && bound.is_some() {
                let max_len = bound.unwrap();
                quote! {
                    if typed_data.#field_name.len() > #max_len {
                        return Err(#crate_path::dcps::core::error::DdsError::Error(
                            format!("Map field '{}' length {} exceeds maximum {}",
                                stringify!(#field_name), typed_data.#field_name.len(), #max_len)
                        ));
                    }
                    #trait_path(&typed_data.#field_name, &mut serializer)
                        .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
                }
            } else {
                quote! {
                    #trait_path(&typed_data.#field_name, &mut serializer)
                        .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
                }
            }
        }
    }
}

#[allow(clippy::unnecessary_unwrap)]
fn gen_deserialize_code(
    method: SerializationMethod,
    field_name: &syn::Ident,
    field_type: &syn::Type,
    crate_path: &proc_macro2::TokenStream,
    xcdr: bool,
    bound: Option<usize>,
    try_construct: TryConstructKind,
) -> proc_macro2::TokenStream {
    match method {
        SerializationMethod::U8 => {
            gen_primitive_deserialize("deserialize_u8", field_name, crate_path)
        }
        SerializationMethod::U16 => {
            gen_primitive_deserialize("deserialize_u16", field_name, crate_path)
        }
        SerializationMethod::U32 => {
            gen_primitive_deserialize("deserialize_u32", field_name, crate_path)
        }
        SerializationMethod::U64 => {
            gen_primitive_deserialize("deserialize_u64", field_name, crate_path)
        }
        SerializationMethod::I8 => {
            gen_primitive_deserialize("deserialize_i8", field_name, crate_path)
        }
        SerializationMethod::I16 => {
            gen_primitive_deserialize("deserialize_i16", field_name, crate_path)
        }
        SerializationMethod::I32 => {
            gen_primitive_deserialize("deserialize_i32", field_name, crate_path)
        }
        SerializationMethod::I64 => {
            gen_primitive_deserialize("deserialize_i64", field_name, crate_path)
        }
        SerializationMethod::F32 => {
            gen_primitive_deserialize("deserialize_f32", field_name, crate_path)
        }
        SerializationMethod::F64 => {
            gen_primitive_deserialize("deserialize_f64", field_name, crate_path)
        }
        SerializationMethod::Char => {
            gen_primitive_deserialize("deserialize_char", field_name, crate_path)
        }
        SerializationMethod::Bool => {
            gen_primitive_deserialize("deserialize_bool", field_name, crate_path)
        }
        SerializationMethod::String => {
            if let Some(max_len) = bound {
                let truncate = quote! {
                    while #field_name.len() > #max_len {
                        #field_name.pop();
                    }
                };
                let action = gen_bound_violation_action(
                    field_name,
                    "string field",
                    quote! { #field_name.len() },
                    quote! { #max_len },
                    crate_path,
                    try_construct,
                    DeserErrorKind::Dds,
                    truncate,
                );
                quote! {
                    let mut #field_name = deserializer.deserialize_string()
                        .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
                    if #field_name.len() > #max_len {
                        #action
                    }
                }
            } else {
                gen_primitive_deserialize("deserialize_string", field_name, crate_path)
            }
        }
        SerializationMethod::WString => {
            let (trait_name, method_name) = if xcdr {
                (quote!(#crate_path::serialize::xcdr::XcdrDeserialize), quote!(deserialize_xcdr))
            } else {
                (quote!(#crate_path::serialize::cdr::CdrDeserialize), quote!(deserialize_cdr))
            };

            if let Some(max_len) = bound {
                let truncate = quote! {
                    let mut __inner = #field_name.as_str().to_string();
                    while __inner.encode_utf16().count() > #max_len {
                        __inner.pop();
                    }
                    #field_name = #crate_path::serialize::core::bounded_types::WString::new(__inner);
                };
                let action = gen_bound_violation_action(
                    field_name,
                    "WString field",
                    quote! { __utf16_len },
                    quote! { #max_len },
                    crate_path,
                    try_construct,
                    DeserErrorKind::Dds,
                    truncate,
                );
                quote! {
                    let mut #field_name = <#field_type as #trait_name>::#method_name(&mut deserializer)
                        .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
                    {
                        let __utf16_len = #field_name.as_str().encode_utf16().count();
                        if __utf16_len > #max_len {
                            #action
                        }
                    }
                }
            } else {
                quote! {
                    let #field_name = <#field_type as #trait_name>::#method_name(&mut deserializer)
                        .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
                }
            }
        }
        SerializationMethod::U8Array => {
            if let Some(size) = get_array_size(field_type) {
                quote! {
                    let byte_data = deserializer.deserialize_byte_array(#size)
                        .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
                    let #field_name: #field_type = {
                        let mut array = [0u8; #size];
                        array.copy_from_slice(&byte_data);
                        array
                    };
                }
            } else {
                quote! { compile_error!("Cannot determine array size for field"); }
            }
        }
        SerializationMethod::U16Array => {
            if let Some(size) = get_array_size(field_type) {
                quote! {
                    let data_vec = deserializer.deserialize_u16_array(#size)
                        .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
                    let #field_name: #field_type = {
                        let mut array = [0u16; #size];
                        array.copy_from_slice(&data_vec);
                        array
                    };
                }
            } else {
                quote! { compile_error!("Cannot determine array size for field"); }
            }
        }
        SerializationMethod::U32Array => {
            if let Some(size) = get_array_size(field_type) {
                quote! {
                    let data_vec = deserializer.deserialize_u32_array(#size)
                        .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
                    let #field_name: #field_type = {
                        let mut array = [0u32; #size];
                        array.copy_from_slice(&data_vec);
                        array
                    };
                }
            } else {
                quote! { compile_error!("Cannot determine array size for field"); }
            }
        }
        SerializationMethod::U64Array => {
            if let Some(size) = get_array_size(field_type) {
                quote! {
                    let data_vec = deserializer.deserialize_u64_array(#size)
                        .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
                    let #field_name: #field_type = {
                        let mut array = [0u64; #size];
                        array.copy_from_slice(&data_vec);
                        array
                    };
                }
            } else {
                quote! { compile_error!("Cannot determine array size for field"); }
            }
        }
        SerializationMethod::I8Array => {
            if let Some(size) = get_array_size(field_type) {
                quote! {
                    let data_vec = deserializer.deserialize_i8_array(#size)
                        .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
                    let #field_name: #field_type = {
                        let mut array = [0i8; #size];
                        array.copy_from_slice(&data_vec);
                        array
                    };
                }
            } else {
                quote! { compile_error!("Cannot determine array size for field"); }
            }
        }
        SerializationMethod::I16Array => {
            if let Some(size) = get_array_size(field_type) {
                quote! {
                    let data_vec = deserializer.deserialize_i16_array(#size)
                        .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
                    let #field_name: #field_type = {
                        let mut array = [0i16; #size];
                        array.copy_from_slice(&data_vec);
                        array
                    };
                }
            } else {
                quote! { compile_error!("Cannot determine array size for field"); }
            }
        }
        SerializationMethod::I32Array => {
            if let Some(size) = get_array_size(field_type) {
                quote! {
                    let data_vec = deserializer.deserialize_i32_array(#size)
                        .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
                    let #field_name: #field_type = {
                        let mut array = [0i32; #size];
                        array.copy_from_slice(&data_vec);
                        array
                    };
                }
            } else {
                quote! { compile_error!("Cannot determine array size for field"); }
            }
        }
        SerializationMethod::I64Array => {
            if let Some(size) = get_array_size(field_type) {
                quote! {
                    let data_vec = deserializer.deserialize_i64_array(#size)
                        .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
                    let #field_name: #field_type = {
                        let mut array = [0i64; #size];
                        array.copy_from_slice(&data_vec);
                        array
                    };
                }
            } else {
                quote! { compile_error!("Cannot determine array size for field"); }
            }
        }
        SerializationMethod::F32Array => {
            if let Some(size) = get_array_size(field_type) {
                quote! {
                    let data_vec = deserializer.deserialize_f32_array(#size)
                        .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
                    let #field_name: #field_type = {
                        let mut array = [0.0f32; #size];
                        array.copy_from_slice(&data_vec);
                        array
                    };
                }
            } else {
                quote! { compile_error!("Cannot determine array size for field"); }
            }
        }
        SerializationMethod::F64Array => {
            if let Some(size) = get_array_size(field_type) {
                quote! {
                    let data_vec = deserializer.deserialize_f64_array(#size)
                        .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
                    let #field_name: #field_type = {
                        let mut array = [0.0f64; #size];
                        array.copy_from_slice(&data_vec);
                        array
                    };
                }
            } else {
                quote! { compile_error!("Cannot determine array size for field"); }
            }
        }
        SerializationMethod::VecU8 => {
            if let Some(max_len) = bound {
                let truncate = quote! { #field_name.truncate(#max_len); };
                let action = gen_bound_violation_action(
                    field_name,
                    "sequence field",
                    quote! { #field_name.len() },
                    quote! { #max_len },
                    crate_path,
                    try_construct,
                    DeserErrorKind::Dds,
                    truncate,
                );
                quote! {
                    let mut #field_name: #field_type = match deserializer.deserialize_byte_sequence() {
                        Ok(value) => value,
                        Err(#crate_path::serialize::cdr::CdrError::InsufficientData) => Vec::new(),
                        Err(e) => {
                            return Err(#crate_path::dcps::core::error::DdsError::Error(e.to_string()));
                        }
                    };
                    if #field_name.len() > #max_len {
                        #action
                    }
                }
            } else {
                quote! {
                    let #field_name: #field_type = match deserializer.deserialize_byte_sequence() {
                        Ok(value) => value,
                        Err(#crate_path::serialize::cdr::CdrError::InsufficientData) => Vec::new(),
                        Err(e) => {
                            return Err(#crate_path::dcps::core::error::DdsError::Error(e.to_string()));
                        }
                    };
                }
            }
        }
        SerializationMethod::VecU16 => gen_sequence_deserialize(
            "deserialize_u16_sequence",
            field_name,
            crate_path,
            bound,
            try_construct,
        ),
        SerializationMethod::VecU32 => gen_sequence_deserialize(
            "deserialize_u32_sequence",
            field_name,
            crate_path,
            bound,
            try_construct,
        ),
        SerializationMethod::VecU64 => gen_sequence_deserialize(
            "deserialize_u64_sequence",
            field_name,
            crate_path,
            bound,
            try_construct,
        ),
        SerializationMethod::VecI8 => gen_sequence_deserialize(
            "deserialize_i8_sequence",
            field_name,
            crate_path,
            bound,
            try_construct,
        ),
        SerializationMethod::VecI16 => gen_sequence_deserialize(
            "deserialize_i16_sequence",
            field_name,
            crate_path,
            bound,
            try_construct,
        ),
        SerializationMethod::VecI32 => gen_sequence_deserialize(
            "deserialize_i32_sequence",
            field_name,
            crate_path,
            bound,
            try_construct,
        ),
        SerializationMethod::VecI64 => gen_sequence_deserialize(
            "deserialize_i64_sequence",
            field_name,
            crate_path,
            bound,
            try_construct,
        ),
        SerializationMethod::VecF32 => gen_sequence_deserialize(
            "deserialize_f32_sequence",
            field_name,
            crate_path,
            bound,
            try_construct,
        ),
        SerializationMethod::VecF64 => gen_sequence_deserialize(
            "deserialize_f64_sequence",
            field_name,
            crate_path,
            bound,
            try_construct,
        ),
        SerializationMethod::VecBool => gen_sequence_deserialize(
            "deserialize_bool_sequence",
            field_name,
            crate_path,
            bound,
            try_construct,
        ),
        SerializationMethod::VecChar => gen_sequence_deserialize(
            "deserialize_char_sequence",
            field_name,
            crate_path,
            bound,
            try_construct,
        ),
        SerializationMethod::VecString => gen_sequence_deserialize(
            "deserialize_string_sequence",
            field_name,
            crate_path,
            bound,
            try_construct,
        ),
        SerializationMethod::BoolArray => {
            if let Some(size) = get_array_size(field_type) {
                quote! {
                    let data_vec = deserializer.deserialize_bool_array(#size)
                        .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
                    let #field_name: #field_type = {
                        let mut array = [false; #size];
                        array.copy_from_slice(&data_vec);
                        array
                    };
                }
            } else {
                quote! { compile_error!("Cannot determine array size for field"); }
            }
        }
        SerializationMethod::CharArray => {
            if let Some(size) = get_array_size(field_type) {
                quote! {
                    let data_vec = deserializer.deserialize_char_array_fixed(#size)
                        .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
                    let #field_name: #field_type = {
                        let mut array = ['\0'; #size];
                        array.copy_from_slice(&data_vec);
                        array
                    };
                }
            } else {
                quote! { compile_error!("Cannot determine array size for field"); }
            }
        }
        SerializationMethod::StringArray => {
            if let Some(size) = get_array_size(field_type) {
                quote! {
                    let data_vec = deserializer.deserialize_string_array(#size)
                        .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
                    let #field_name: #field_type = {
                        let mut array: [String; #size] = std::array::from_fn(|_| String::new());
                        for (i, s) in data_vec.into_iter().enumerate() {
                            array[i] = s;
                        }
                        array
                    };
                }
            } else {
                quote! { compile_error!("Cannot determine array size for field"); }
            }
        }
        SerializationMethod::Fallback => {
            if !xcdr && is_option_type(field_type) {
                let msg = format!(
                    "XCDR1 does not support Option<T> field '{}' (PL_CDR v1 pending T2-1); use XCDR2",
                    field_name
                );
                return quote! {
                    (|| -> ::std::result::Result<(), #crate_path::dcps::core::error::DdsError> {
                        Err(#crate_path::dcps::core::error::DdsError::Error(#msg.to_string()))
                    })()?;
                    let #field_name: #field_type = None;
                };
            }
            if xcdr {
                let trait_name = quote!(#crate_path::serialize::xcdr::XcdrDeserialize);
                let method_name = quote!(deserialize_xcdr);

                if is_map_type(field_type) && bound.is_some() {
                    let max_len = bound.unwrap();
                    quote! {
                        let #field_name = <#field_type as #trait_name>::#method_name(&mut deserializer)
                            .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
                        if #field_name.len() > #max_len {
                            return Err(#crate_path::dcps::core::error::DdsError::Error(
                                format!("Deserialized map field '{}' length {} exceeds maximum {}",
                                    stringify!(#field_name), #field_name.len(), #max_len)
                            ));
                        }
                    }
                } else {
                    quote! {
                        let #field_name = <#field_type as #trait_name>::#method_name(&mut deserializer)
                            .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
                    }
                }
            } else {
                let trait_name = quote!(#crate_path::serialize::cdr::CdrDeserialize);
                let method_name = quote!(deserialize_cdr);

                if is_map_type(field_type) && bound.is_some() {
                    let max_len = bound.unwrap();
                    quote! {
                        let #field_name = <#field_type as #trait_name>::#method_name(&mut deserializer)
                            .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
                        if #field_name.len() > #max_len {
                            return Err(#crate_path::dcps::core::error::DdsError::Error(
                                format!("Deserialized map field '{}' length {} exceeds maximum {}",
                                    stringify!(#field_name), #field_name.len(), #max_len)
                            ));
                        }
                    }
                } else {
                    quote! {
                        let #field_name = <#field_type as #trait_name>::#method_name(&mut deserializer)
                            .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
                    }
                }
            }
        }
    }
}

fn generate_field_serialization_internal(
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
    crate_path: &proc_macro2::TokenStream,
    include_key_fields: bool,
    xcdr: bool,
) -> proc_macro2::TokenStream {
    let field_serializations = fields.iter().filter_map(|field| {
        let field_config = parse_field_attributes(field);
        if field_config.non_serialized {
            return None;
        }
        if !include_key_fields && field_config.key {
            return None;
        }
        let field_name = field.ident.as_ref().unwrap();
        let method = get_serialization_method(&field.ty);
        Some(gen_serialize_code(
            method,
            field_name,
            &field.ty,
            crate_path,
            xcdr,
            field_config.bound,
        ))
    });

    quote! { #(#field_serializations)* }
}

fn generate_key_field_serialization_internal(
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
    crate_path: &proc_macro2::TokenStream,
    xcdr: bool,
) -> proc_macro2::TokenStream {
    let field_serializations = fields.iter().filter_map(|field| {
        let field_config = parse_field_attributes(field);
        if field_config.non_serialized {
            return None;
        }
        if !field_config.key {
            return None;
        }
        let field_name = field.ident.as_ref().unwrap();
        let method = get_serialization_method(&field.ty);
        Some(gen_serialize_code(
            method,
            field_name,
            &field.ty,
            crate_path,
            xcdr,
            field_config.bound,
        ))
    });

    quote! { #(#field_serializations)* }
}

pub fn generate_field_serialization(
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
    crate_path: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    generate_field_serialization_internal(fields, crate_path, true, false)
}

pub fn generate_field_serialization_xcdr(
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
    crate_path: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    generate_field_serialization_internal(fields, crate_path, true, true)
}

pub fn generate_non_key_field_serialization(
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
    crate_path: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    generate_field_serialization_internal(fields, crate_path, false, false)
}

pub fn generate_non_key_field_serialization_xcdr(
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
    crate_path: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    generate_field_serialization_internal(fields, crate_path, false, true)
}

pub fn generate_key_field_serialization(
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
    crate_path: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    generate_key_field_serialization_internal(fields, crate_path, false)
}

pub fn generate_key_field_serialization_xcdr(
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
    crate_path: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    generate_key_field_serialization_internal(fields, crate_path, true)
}

fn generate_field_deserialization_internal(
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
    name: &syn::Ident,
    crate_path: &proc_macro2::TokenStream,
    xcdr: bool,
) -> proc_macro2::TokenStream {
    let field_deserializations = fields.iter().map(|field| {
        let field_config = parse_field_attributes(field);
        let field_name = field.ident.as_ref().unwrap();
        if field_config.non_serialized {
            return non_serialized_binding(field_name, &field.ty, &field_config);
        }
        let method = get_serialization_method(&field.ty);
        gen_deserialize_code(
            method,
            field_name,
            &field.ty,
            crate_path,
            xcdr,
            field_config.bound,
            field_config.try_construct,
        )
    });

    let field_names: Vec<_> = fields.iter().map(|f| f.ident.as_ref().unwrap()).collect();

    quote! {
        #(#field_deserializations)*

        let result = #name {
            #(#field_names, )*
        };
    }
}

pub fn generate_field_deserialization(
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
    name: &syn::Ident,
    crate_path: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    generate_field_deserialization_internal(fields, name, crate_path, false)
}
pub fn generate_field_deserialization_xcdr(
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
    name: &syn::Ident,
    crate_path: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    generate_field_deserialization_internal(fields, name, crate_path, true)
}

/// Generate XCDR deserialization code with per-field DHEADER reading.
/// This is for interoperability with implementations that serialize APPENDABLE types
/// with a DHEADER before each field instead of a single DHEADER for the whole struct.
/// Uses DHEADER value for forward compatibility - skips remaining bytes if field has extra data.
pub fn generate_field_deserialization_xcdr_per_field_dheader(
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
    name: &syn::Ident,
    crate_path: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let field_deserializations = fields.iter().map(|field| {
        let field_config = parse_field_attributes(field);
        let field_name = field.ident.as_ref().unwrap();
        if field_config.non_serialized {
            return non_serialized_binding(field_name, &field.ty, &field_config);
        }
        let method = get_serialization_method(&field.ty);
        let inner_deserialize = gen_deserialize_code(
            method,
            field_name,
            &field.ty,
            crate_path,
            true,
            field_config.bound,
            field_config.try_construct,
        );

        // Read DHEADER before each field (interoperability format)
        // Use DHEADER value to skip remaining bytes for forward compatibility
        quote! {
            let __field_size = deserializer.read_dheader()
                .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
            let __field_start = {
                use #crate_path::serialize::DeserializerReader;
                deserializer.get_position()
            };

            #inner_deserialize

            // Skip remaining bytes for forward compatibility (unknown additional data)
            {
                use #crate_path::serialize::DeserializerReader;
                let __bytes_consumed = deserializer.get_position() - __field_start;
                if __bytes_consumed < __field_size as usize {
                    deserializer.skip((__field_size as usize) - __bytes_consumed)
                        .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
                }
            }
        }
    });

    let field_names: Vec<_> = fields.iter().map(|f| f.ident.as_ref().unwrap()).collect();

    quote! {
        #(#field_deserializations)*

        let result = #name {
            #(#field_names, )*
        };
    }
}
