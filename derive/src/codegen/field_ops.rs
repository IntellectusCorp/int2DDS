use quote::quote;

use crate::codegen::utils::{
    get_array_size, get_serialization_method, is_map_type, parse_field_attributes,
    SerializationMethod,
};

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
) -> proc_macro2::TokenStream {
    let method_ident = syn::Ident::new(method, field_name.span());

    if let Some(max_len) = bound {
        quote! {
            let #field_name = deserializer.#method_ident()
                .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
            if #field_name.len() > #max_len {
                return Err(#crate_path::dcps::core::error::DdsError::Error(
                    format!("Deserialized sequence field '{}' length {} exceeds maximum {}",
                        stringify!(#field_name), #field_name.len(), #max_len)
                ));
            }
        }
    } else {
        quote! {
            let #field_name = deserializer.#method_ident()
                .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
        }
    }
}

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

fn gen_deserialize_code(
    method: SerializationMethod,
    field_name: &syn::Ident,
    field_type: &syn::Type,
    crate_path: &proc_macro2::TokenStream,
    xcdr: bool,
    bound: Option<usize>,
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
                quote! {
                    let #field_name = deserializer.deserialize_string()
                        .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
                    if #field_name.len() > #max_len {
                        return Err(#crate_path::dcps::core::error::DdsError::Error(
                            format!("Deserialized string field '{}' length {} exceeds maximum {}",
                                stringify!(#field_name), #field_name.len(), #max_len)
                        ));
                    }
                }
            } else {
                gen_primitive_deserialize("deserialize_string", field_name, crate_path)
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
                quote! {
                    let #field_name: #field_type = match deserializer.deserialize_byte_sequence() {
                        Ok(value) => {
                            if value.len() > #max_len {
                                return Err(#crate_path::dcps::core::error::DdsError::Error(
                                    format!("Deserialized sequence field '{}' length {} exceeds maximum {}",
                                        stringify!(#field_name), value.len(), #max_len)
                                ));
                            }
                            value
                        },
                        Err(#crate_path::serialize::cdr::CdrError::InsufficientData) => Vec::new(),
                        Err(e) => {
                            return Err(#crate_path::dcps::core::error::DdsError::Error(e.to_string()));
                        }
                    };
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
        SerializationMethod::VecU16 => {
            gen_sequence_deserialize("deserialize_u16_sequence", field_name, crate_path, bound)
        }
        SerializationMethod::VecU32 => {
            gen_sequence_deserialize("deserialize_u32_sequence", field_name, crate_path, bound)
        }
        SerializationMethod::VecU64 => {
            gen_sequence_deserialize("deserialize_u64_sequence", field_name, crate_path, bound)
        }
        SerializationMethod::VecI8 => {
            gen_sequence_deserialize("deserialize_i8_sequence", field_name, crate_path, bound)
        }
        SerializationMethod::VecI16 => {
            gen_sequence_deserialize("deserialize_i16_sequence", field_name, crate_path, bound)
        }
        SerializationMethod::VecI32 => {
            gen_sequence_deserialize("deserialize_i32_sequence", field_name, crate_path, bound)
        }
        SerializationMethod::VecI64 => {
            gen_sequence_deserialize("deserialize_i64_sequence", field_name, crate_path, bound)
        }
        SerializationMethod::VecF32 => {
            gen_sequence_deserialize("deserialize_f32_sequence", field_name, crate_path, bound)
        }
        SerializationMethod::VecF64 => {
            gen_sequence_deserialize("deserialize_f64_sequence", field_name, crate_path, bound)
        }
        SerializationMethod::VecBool => {
            gen_sequence_deserialize("deserialize_bool_sequence", field_name, crate_path, bound)
        }
        SerializationMethod::VecChar => {
            gen_sequence_deserialize("deserialize_char_sequence", field_name, crate_path, bound)
        }
        SerializationMethod::VecString => {
            gen_sequence_deserialize("deserialize_string_sequence", field_name, crate_path, bound)
        }
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
        let method = get_serialization_method(&field.ty);
        gen_deserialize_code(method, field_name, &field.ty, crate_path, xcdr, field_config.bound)
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
pub fn generate_field_deserialization_xcdr_per_field_dheader(
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
    name: &syn::Ident,
    crate_path: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let field_deserializations = fields.iter().map(|field| {
        let field_config = parse_field_attributes(field);
        let field_name = field.ident.as_ref().unwrap();
        let method = get_serialization_method(&field.ty);
        let inner_deserialize = gen_deserialize_code(
            method,
            field_name,
            &field.ty,
            crate_path,
            true,
            field_config.bound,
        );

        // Read DHEADER before each field (interoperability format)
        // Note: field variable must be declared outside block to stay in scope
        quote! {
            let _field_dheader = deserializer.read_dheader()
                .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
            #inner_deserialize
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
