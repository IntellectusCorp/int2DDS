use quote::quote;
use syn::DeriveInput;

use crate::codegen::{
    generate_additional_derives, generate_field_deserialization,
    generate_field_deserialization_xcdr, generate_field_serialization,
    generate_field_serialization_xcdr, generate_key_field_serialization,
    generate_key_field_serialization_xcdr, generate_key_methods, generate_multi_key_methods,
    generate_non_key_field_serialization, generate_non_key_field_serialization_xcdr,
    parse_field_attributes, KeyFieldInfo, MultiKeyFieldInfo,
};
use crate::codegen::{quote_extensibility_tokens, DdsTypeConfig, ExtensibilityKind};

/// Generate DdsType implementation for struct types
pub fn derive_struct_impl(
    input: &DeriveInput,
    name: &syn::Ident,
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
    type_config: &DdsTypeConfig,
) -> proc_macro2::TokenStream {
    let crate_path = &type_config.crate_path;
    let type_support_name = quote::format_ident!("{}TypeSupport", name);

    // Find all key fields - all fields with #[dds(key)] attribute
    let all_key_fields = find_all_key_fields(fields);

    // Check if key fields exist
    let has_key = !all_key_fields.is_empty();

    // Check if non-key fields exist
    let has_non_key_fields = fields.iter().any(|field| {
        let field_config = parse_field_attributes(field);
        !field_config.key
    });

    // Define TypeSupport struct
    let type_support_struct = quote! {
        #[derive(Default)]
        pub struct #type_support_name;
    };

    // Generate CDR and XCDR field information
    let field_serialization = generate_field_serialization(fields, crate_path);
    let field_deserialization = generate_field_deserialization(fields, name, crate_path);
    let xcdr_field_serialization = generate_field_serialization_xcdr(fields, crate_path);
    let xcdr_field_deserialization = generate_field_deserialization_xcdr(fields, name, crate_path);

    // Generate code to serialize only key fields
    let key_field_serialization = generate_key_field_serialization(fields, crate_path);
    let key_field_serialization_xcdr = generate_key_field_serialization_xcdr(fields, crate_path);

    // Generate code to serialize only non-key fields
    let non_key_field_serialization = generate_non_key_field_serialization(fields, crate_path);
    let non_key_field_serialization_xcdr =
        generate_non_key_field_serialization_xcdr(fields, crate_path);

    // Generate key-related methods - call appropriate function based on key count
    let (serialize_key_impl, deserialize_key_impl, compute_key_impl) = if all_key_fields.is_empty()
    {
        // When there are no keys
        generate_key_methods(None, name, &field_deserialization, crate_path)
    } else if all_key_fields.len() == 1 {
        // Single key
        generate_key_methods(Some(&all_key_fields[0]), name, &field_deserialization, crate_path)
    } else {
        // Multiple keys
        let multi_key_info = MultiKeyFieldInfo { fields: all_key_fields };
        generate_multi_key_methods(Some(&multi_key_info), name, crate_path)
    };

    // Generate field access methods
    let field_access_impl = generate_field_access_methods(fields, name, crate_path);

    // TypeSupport trait implementation (CDR + XCDR unified)
    let type_support_impl = generate_unified_type_support_impl(
        &type_support_name,
        name,
        has_key,
        has_non_key_fields,
        &field_serialization,
        &field_deserialization,
        &xcdr_field_serialization,
        &xcdr_field_deserialization,
        &key_field_serialization,
        &key_field_serialization_xcdr,
        &non_key_field_serialization,
        &non_key_field_serialization_xcdr,
        &serialize_key_impl,
        &deserialize_key_impl,
        &compute_key_impl,
        &field_access_impl,
        crate_path,
        type_config.extensibility,
    );

    let dds_type_impl = quote! {
        impl #crate_path::dcps::topic::type_support::DdsType for #name {
            type TypeSupport = #type_support_name;
        }
    };

    // Generate CdrSerialize and CdrDeserialize trait implementations
    let cdr_serialize_impl = generate_cdr_serialize_impl(name, fields, crate_path);

    let cdr_deserialize_impl = generate_cdr_deserialize_impl(name, fields, crate_path);

    // Generate XcdrSerialize and XcdrDeserialize trait implementations
    let xcdr_serialize_impl =
        generate_xcdr_serialize_impl(name, fields, crate_path, type_config.extensibility);

    let xcdr_deserialize_impl =
        generate_xcdr_deserialize_impl(name, fields, crate_path, type_config.extensibility);

    let additional_derives = generate_additional_derives(input, name);

    quote! {
        #type_support_struct
        #type_support_impl
        #dds_type_impl
        #cdr_serialize_impl
        #cdr_deserialize_impl
        #xcdr_serialize_impl
        #xcdr_deserialize_impl
        #additional_derives
    }
}

/// Find and return all key fields
fn find_all_key_fields(
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
) -> Vec<KeyFieldInfo> {
    fields
        .iter()
        .filter_map(|field| {
            let field_config = parse_field_attributes(field);
            if field_config.key {
                Some(KeyFieldInfo {
                    ident: field.ident.as_ref().unwrap().clone(),
                    field_type: field.ty.clone(),
                    member_id: field_config.id,
                })
            } else {
                None
            }
        })
        .collect()
}

/// Generate serialize method implementation
fn quote_serialize_impl(crate_path: &proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    quote! {
        fn serialize(&self, data: &dyn std::any::Any) -> #crate_path::dcps::core::error::DdsResult<#crate_path::rtps::common::types::SerializedData> {
            self.serialize_with_format(data, &#crate_path::dcps::topic::type_support::SerializationFormat::Cdr)
        }
    }
}

/// Generate deserialize method implementation
fn quote_deserialize_impl(
    extensibility: Option<ExtensibilityKind>,
    crate_path: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    if let Some(ext_kind) = extensibility {
        let extensibility_tokens = quote_extensibility_tokens(ext_kind, crate_path);

        quote! {
            fn deserialize(&self, data: &[u8]) -> #crate_path::dcps::core::error::DdsResult<Box<dyn std::any::Any>> {
                if data.len() >= 2 {
                    let encoding_id = u16::from_be_bytes([data[0], data[1]]);
                    let format = match encoding_id {
                        0x0000 | 0x0001 => #crate_path::dcps::topic::type_support::SerializationFormat::Cdr,
                        0x0006 | 0x0007 => {
                            #crate_path::dcps::topic::type_support::SerializationFormat::Xcdr {
                                extensibility_kind: #extensibility_tokens,
                                use_delimiters: false,
                            }
                        },
                        0x0008..=0x000B => {
                            #crate_path::dcps::topic::type_support::SerializationFormat::Xcdr {
                                extensibility_kind: #extensibility_tokens,
                                use_delimiters: true,
                            }
                        },
                        _ => #crate_path::dcps::topic::type_support::SerializationFormat::Cdr,
                    };
                    self.deserialize_with_format(data, &format)
                } else {
                    Err(#crate_path::dcps::core::error::DdsError::Error("Invalid data length".to_string()))
                }
            }
        }
    } else {
        quote! {
            fn deserialize(&self, data: &[u8]) -> #crate_path::dcps::core::error::DdsResult<Box<dyn std::any::Any>> {
                self.deserialize_with_format(data, &#crate_path::dcps::topic::type_support::SerializationFormat::Cdr)
            }
        }
    }
}

/// Generate serialize_with_format method implementation
fn quote_serialize_with_format_impl(
    name: &syn::Ident,
    cdr_field_serialization: &proc_macro2::TokenStream,
    xcdr_field_serialization: &proc_macro2::TokenStream,
    crate_path: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    quote! {
        fn serialize_with_format(&self, data: &dyn std::any::Any, format: &#crate_path::dcps::topic::type_support::SerializationFormat) -> #crate_path::dcps::core::error::DdsResult<#crate_path::rtps::common::types::SerializedData> {
            if let Some(typed_data) = data.downcast_ref::<#name>() {
                match format {
                    #crate_path::dcps::topic::type_support::SerializationFormat::Cdr => {
                        use #crate_path::serialize::{cdr::CdrSerializer, BufferManager};
                        use #crate_path::serialize::cdr::{PrimitiveSerialize, StringSerialize, ArraySerialize, SequenceSerialize};

                        let mut serializer = CdrSerializer::with_capacity(true, 64);
                        serializer.write_encapsulation_header()
                            .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;

                        #cdr_field_serialization

                        let bytes = serializer.into_bytes();
                        Ok(std::sync::Arc::from(bytes.into_boxed_slice()))
                    },
                    #crate_path::dcps::topic::type_support::SerializationFormat::Xcdr { extensibility_kind, use_delimiters } => {
                        use #crate_path::serialize::{xcdr::Xcdr2Serializer, BufferManager};
                        use #crate_path::serialize::cdr::{PrimitiveSerialize, StringSerialize, ArraySerialize, SequenceSerialize};

                        let effective_extensibility = *extensibility_kind;
                        let use_delimiters = *use_delimiters;
                        let mut serializer = Xcdr2Serializer::with_capacity(true, effective_extensibility, 64);
                        serializer.write_encapsulation_header()
                            .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;

                        let size_pos = if use_delimiters {
                            Some(
                                serializer
                                    .begin_struct()
                                    .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(
                                        e.to_string(),
                                    ))?,
                            )
                        } else {
                            None
                        };

                        #xcdr_field_serialization

                        if let Some(size_pos) = size_pos {
                            serializer
                                .end_struct(size_pos)
                                .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(
                                    e.to_string(),
                                ))?;
                        }

                        let bytes = serializer.into_bytes();
                        Ok(std::sync::Arc::from(bytes.into_boxed_slice()))
                    }
                }
            } else {
                Err(#crate_path::dcps::core::error::DdsError::BadParameter)
            }
        }
    }
}

/// Generate deserialize_with_format method implementation
fn quote_deserialize_with_format_impl(
    name: &syn::Ident,
    cdr_field_deserialization: &proc_macro2::TokenStream,
    xcdr_field_deserialization: &proc_macro2::TokenStream,
    crate_path: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    quote! {
        fn deserialize_with_format(&self, data: &[u8], format: &#crate_path::dcps::topic::type_support::SerializationFormat) -> #crate_path::dcps::core::error::DdsResult<Box<dyn std::any::Any>> {
            match format {
                #crate_path::dcps::topic::type_support::SerializationFormat::Cdr => {
                    use #crate_path::serialize::cdr::CdrDeserializer;

                    let mut deserializer = CdrDeserializer::new(data)
                        .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;

                    #cdr_field_deserialization

                    Ok(Box::new(result))
                },
                #crate_path::dcps::topic::type_support::SerializationFormat::Xcdr { use_delimiters, .. } => {
                    use #crate_path::serialize::xcdr::Xcdr2Deserializer;

                    let use_delimiters = *use_delimiters;
                    let mut parse_body = |mut deserializer: &mut Xcdr2Deserializer<'_>| -> #crate_path::dcps::core::error::DdsResult<#name> {
                        #xcdr_field_deserialization
                        Ok(result)
                    };

                    if use_delimiters {
                        let mut delimited_deserializer = Xcdr2Deserializer::new(data)
                            .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;

                        match (|| -> #crate_path::dcps::core::error::DdsResult<#name> {
                            let (object_size, start_position) = delimited_deserializer
                                .begin_struct()
                                .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
                            let value = parse_body(&mut delimited_deserializer)?;
                            delimited_deserializer
                                .end_struct(object_size, start_position)
                                .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
                            Ok(value)
                        })() {
                            Ok(result) => {
                                return Ok(Box::new(result));
                            }
                            Err(err) => {
                                log::warn!(
                                    "====deserialize_with_format XCDR delimited path failed ({}); retrying without delimiters",
                                    err
                                );
                            }
                        }
                    }

                    let mut fallback_deserializer = Xcdr2Deserializer::new(data)
                        .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;

                    let result = parse_body(&mut fallback_deserializer)?;
                    Ok(Box::new(result))
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn generate_unified_type_support_impl(
    type_support_name: &syn::Ident,
    name: &syn::Ident,
    has_key: bool,
    _has_non_key_fields: bool,
    cdr_field_serialization: &proc_macro2::TokenStream,
    cdr_field_deserialization: &proc_macro2::TokenStream,
    xcdr_field_serialization: &proc_macro2::TokenStream,
    xcdr_field_deserialization: &proc_macro2::TokenStream,
    _key_field_serialization: &proc_macro2::TokenStream,
    _key_field_serialization_xcdr: &proc_macro2::TokenStream,
    _non_key_field_serialization: &proc_macro2::TokenStream,
    _non_key_field_serialization_xcdr: &proc_macro2::TokenStream,
    serialize_key_impl: &proc_macro2::TokenStream,
    deserialize_key_impl: &proc_macro2::TokenStream,
    compute_key_impl: &proc_macro2::TokenStream,
    field_access_impl: &proc_macro2::TokenStream,
    crate_path: &proc_macro2::TokenStream,
    extensibility: Option<ExtensibilityKind>,
) -> proc_macro2::TokenStream {
    let serialize_impl = quote_serialize_impl(crate_path);
    let deserialize_impl = quote_deserialize_impl(extensibility, crate_path);
    let serialize_with_format_impl = quote_serialize_with_format_impl(
        name,
        cdr_field_serialization,
        xcdr_field_serialization,
        crate_path,
    );
    let deserialize_with_format_impl = quote_deserialize_with_format_impl(
        name,
        cdr_field_deserialization,
        xcdr_field_deserialization,
        crate_path,
    );

    let extensibility_tokens = if let Some(ext_kind) = extensibility {
        quote_extensibility_tokens(ext_kind, crate_path)
    } else {
        quote! { #crate_path::serialize::xcdr::ExtensibilityKind::Appendable }
    };

    quote! {
        impl #crate_path::dcps::topic::type_support::TypeSupport for #type_support_name {
            fn get_type_name(&self) -> &str {
                stringify!(#name)
            }

            fn type_id(&self) -> std::any::TypeId {
                std::any::TypeId::of::<#name>()
            }

            fn is_compute_key_provided(&self) -> bool {
                #has_key
            }

            fn get_extensibility_kind(&self) -> #crate_path::serialize::xcdr::ExtensibilityKind {
                #extensibility_tokens
            }

            #serialize_impl

            #deserialize_impl

            #serialize_with_format_impl

            #deserialize_with_format_impl

            #serialize_key_impl

            #deserialize_key_impl

            #compute_key_impl

            fn serialize_key_and_non_key(&self, data: &dyn std::any::Any) -> #crate_path::dcps::core::error::DdsResult<(#crate_path::rtps::common::types::SerializedData, #crate_path::rtps::common::types::SerializedData)> {
                if let Some(typed_data) = data.downcast_ref::<#name>() {
                    let key_data = self.serialize_key(data)?;
                    let full_data = self.serialize(data)?;
                    Ok((key_data, full_data))
                } else {
                    Err(#crate_path::dcps::core::error::DdsError::BadParameter)
                }
            }

            #field_access_impl
        }
    }
}

/// Helper function to convert field value to SQL Parameter
fn quote_field_to_parameter_conversion(
    crate_path: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    quote! {
        // Helper function to convert Any to Parameter
        fn any_to_parameter(field_value: &dyn std::any::Any) -> #crate_path::dcps::core::error::DdsResult<#crate_path::topic::sql::ast::Parameter> {
            use #crate_path::topic::sql::ast::Parameter;

            if let Some(v) = field_value.downcast_ref::<u8>() {
                Ok(Parameter::IntegerValue(*v as i32))
            } else if let Some(v) = field_value.downcast_ref::<u16>() {
                Ok(Parameter::IntegerValue(*v as i32))
            } else if let Some(v) = field_value.downcast_ref::<u32>() {
                Ok(Parameter::IntegerValue(*v as i32))
            } else if let Some(v) = field_value.downcast_ref::<u64>() {
                Ok(Parameter::FloatValue(*v as f64))
            } else if let Some(v) = field_value.downcast_ref::<i8>() {
                Ok(Parameter::IntegerValue(*v as i32))
            } else if let Some(v) = field_value.downcast_ref::<i16>() {
                Ok(Parameter::IntegerValue(*v as i32))
            } else if let Some(v) = field_value.downcast_ref::<i32>() {
                Ok(Parameter::IntegerValue(*v))
            } else if let Some(v) = field_value.downcast_ref::<i64>() {
                Ok(Parameter::FloatValue(*v as f64))
            } else if let Some(v) = field_value.downcast_ref::<f32>() {
                Ok(Parameter::FloatValue(*v as f64))
            } else if let Some(v) = field_value.downcast_ref::<f64>() {
                Ok(Parameter::FloatValue(*v))
            } else if let Some(v) = field_value.downcast_ref::<bool>() {
                Ok(Parameter::IntegerValue(if *v { 1 } else { 0 }))
            } else if let Some(v) = field_value.downcast_ref::<char>() {
                Ok(Parameter::CharValue(*v))
            } else if let Some(v) = field_value.downcast_ref::<String>() {
                Ok(Parameter::String(v.clone()))
            } else if field_value.downcast_ref::<Vec<u8>>().is_some() {
                Err(#crate_path::dcps::core::error::DdsError::Error(
                    "Vec<u8> field type not supported in SQL queries".to_string()
                ))
            } else {
                Err(#crate_path::dcps::core::error::DdsError::Error(
                    "Unsupported field type for SQL queries".to_string()
                ))
            }
        }
    }
}

fn generate_field_access_methods(
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
    name: &syn::Ident,
    crate_path: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let field_names: Vec<_> = fields
        .iter()
        .map(|field| {
            let field_name_str = field.ident.as_ref().unwrap().to_string();
            quote! { #field_name_str }
        })
        .collect();

    let helper_fn = quote_field_to_parameter_conversion(crate_path);

    let field_matches: Vec<_> = fields
        .iter()
        .map(|field| {
            let field_name = field.ident.as_ref().unwrap();
            let field_name_str = field_name.to_string();

            quote! {
                #field_name_str => {
                    any_to_parameter(&typed_data.#field_name as &dyn std::any::Any)
                },
            }
        })
        .collect();

    quote! {
        fn get_field_value(&self, data: &dyn std::any::Any, field_path: &str) -> #crate_path::dcps::core::error::DdsResult<#crate_path::topic::sql::ast::Parameter> {
            #helper_fn

            if let Some(typed_data) = data.downcast_ref::<#name>() {
                match field_path {
                    #(#field_matches)*
                    _ => Err(#crate_path::dcps::core::error::DdsError::Error(format!("Field '{}' not found", field_path))),
                }
            } else {
                Err(#crate_path::dcps::core::error::DdsError::BadParameter)
            }
        }

        fn has_field(&self, field_path: &str) -> bool {
            matches!(field_path, #(#field_names)|*)
        }
    }
}

/// Generate CdrSerialize trait implementation
fn generate_cdr_serialize_impl(
    name: &syn::Ident,
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
    crate_path: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    // Call CdrSerialize::serialize_cdr for each field
    let field_calls: Vec<_> = fields
        .iter()
        .map(|field| {
            let field_name = field.ident.as_ref().unwrap();
            quote! {
                #crate_path::serialize::cdr::CdrSerialize::serialize_cdr(&self.#field_name, serializer)?;
            }
        })
        .collect();

    quote! {
        impl #crate_path::serialize::cdr::CdrSerialize for #name {
            fn serialize_cdr(&self, serializer: &mut #crate_path::serialize::cdr::CdrSerializer) -> #crate_path::serialize::cdr::CdrResult<()> {
                use #crate_path::serialize::cdr::{PrimitiveSerialize, StringSerialize, ArraySerialize, SequenceSerialize};
                #(#field_calls)*
                Ok(())
            }
        }
    }
}

/// Generate CdrDeserialize trait implementation
fn generate_cdr_deserialize_impl(
    name: &syn::Ident,
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
    crate_path: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    // Call CdrDeserialize::deserialize_cdr for each field
    let field_deserializations: Vec<_> = fields
        .iter()
        .map(|field| {
            let field_name = field.ident.as_ref().unwrap();
            let field_type = &field.ty;
            quote! {
                let #field_name = <#field_type as #crate_path::serialize::cdr::CdrDeserialize>::deserialize_cdr(deserializer)?;
            }
        })
        .collect();

    let field_names: Vec<_> = fields.iter().map(|f| f.ident.as_ref().unwrap()).collect();

    quote! {
        impl #crate_path::serialize::cdr::CdrDeserialize for #name {
            fn deserialize_cdr(deserializer: &mut #crate_path::serialize::cdr::CdrDeserializer) -> #crate_path::serialize::cdr::CdrResult<Self> {
                #(#field_deserializations)*

                Ok(#name {
                    #(#field_names,)*
                })
            }
        }
    }
}

/// Generate XcdrSerialize trait implementation
fn generate_xcdr_serialize_impl(
    name: &syn::Ident,
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
    crate_path: &proc_macro2::TokenStream,
    extensibility: Option<ExtensibilityKind>,
) -> proc_macro2::TokenStream {
    let is_mutable = matches!(extensibility, Some(ExtensibilityKind::Mutable));

    // Generate field serialization calls
    let field_calls: Vec<_> = fields
        .iter()
        .enumerate()
        .map(|(index, field)| {
            let field_name = field.ident.as_ref().unwrap();
            let field_config = parse_field_attributes(field);
            let member_id = field_config.id.unwrap_or(index as u32);
            let is_optional = field_config.optional;

            if is_mutable {
                // For Mutable types: write EMHEADER before each field
                if is_optional {
                    // Optional field: only serialize if Some
                    quote! {
                        if let Some(ref opt_value) = self.#field_name {
                            // Reserve space for EMHEADER
                            let emheader_pos = serializer.reserve_dheader();
                            let field_start = serializer.position();

                            // Serialize the field value
                            #crate_path::serialize::xcdr::XcdrSerialize::serialize_xcdr(opt_value, serializer)?;

                            // Calculate field length and backpatch EMHEADER
                            let field_len = (serializer.position() - field_start) as u32;
                            let emheader = (#member_id << 16) | (field_len & 0xFFFF);
                            serializer.write_dheader_at(emheader_pos, emheader);
                        }
                    }
                } else {
                    // Required field: always serialize
                    quote! {
                        {
                            // Reserve space for EMHEADER
                            let emheader_pos = serializer.reserve_dheader();
                            let field_start = serializer.position();

                            // Serialize the field
                            #crate_path::serialize::xcdr::XcdrSerialize::serialize_xcdr(&self.#field_name, serializer)?;

                            // Calculate field length and backpatch EMHEADER
                            let field_len = (serializer.position() - field_start) as u32;
                            let emheader = (#member_id << 16) | (field_len & 0xFFFF);
                            serializer.write_dheader_at(emheader_pos, emheader);
                        }
                    }
                }
            } else {
                // For Final/Appendable: serialize field directly
                if is_optional {
                    quote! {
                        if let Some(ref opt_value) = self.#field_name {
                            serializer.serialize_bool(true)?;
                            #crate_path::serialize::xcdr::XcdrSerialize::serialize_xcdr(opt_value, serializer)?;
                        } else {
                            serializer.serialize_bool(false)?;
                        }
                    }
                } else {
                    quote! {
                        #crate_path::serialize::xcdr::XcdrSerialize::serialize_xcdr(&self.#field_name, serializer)?;
                    }
                }
            }
        })
        .collect();

    let serialization_body = if matches!(
        extensibility,
        Some(ExtensibilityKind::Appendable) | Some(ExtensibilityKind::Mutable)
    ) {
        quote! {
            use #crate_path::serialize::cdr::{PrimitiveSerialize, StringSerialize, ArraySerialize, SequenceSerialize};
            let size_pos = serializer.begin_struct()?;
            #(#field_calls)*
            serializer.end_struct(size_pos)?;
            Ok(())
        }
    } else {
        quote! {
            use #crate_path::serialize::cdr::{PrimitiveSerialize, StringSerialize, ArraySerialize, SequenceSerialize};
            #(#field_calls)*
            Ok(())
        }
    };

    quote! {
        impl #crate_path::serialize::xcdr::XcdrSerialize for #name {
            fn serialize_xcdr(&self, serializer: &mut #crate_path::serialize::xcdr::XcdrSerializer) -> #crate_path::serialize::xcdr::XcdrResult<()> {
                #serialization_body
            }
        }
    }
}

/// Generate XcdrDeserialize trait implementation
fn generate_xcdr_deserialize_impl(
    name: &syn::Ident,
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
    crate_path: &proc_macro2::TokenStream,
    extensibility: Option<ExtensibilityKind>,
) -> proc_macro2::TokenStream {
    let is_mutable = matches!(extensibility, Some(ExtensibilityKind::Mutable));

    if is_mutable {
        generate_mutable_deserialize_impl(name, fields, crate_path)
    } else if matches!(extensibility, Some(ExtensibilityKind::Appendable)) {
        generate_appendable_deserialize_impl(name, fields, crate_path)
    } else {
        generate_final_deserialize_impl(name, fields, crate_path)
    }
}

/// Generate deserialization for Final types (no DHEADER)
fn generate_final_deserialize_impl(
    name: &syn::Ident,
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
    crate_path: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let field_deserializations: Vec<_> = fields
        .iter()
        .map(|field| {
            let field_name = field.ident.as_ref().unwrap();
            let field_type = &field.ty;
            let field_config = parse_field_attributes(field);

            if field_config.optional {
                quote! {
                    let #field_name = {
                        let has_value = deserializer.deserialize_bool()?;
                        if has_value {
                            Some(<#field_type as #crate_path::serialize::xcdr::XcdrDeserialize>::deserialize_xcdr(deserializer)?)
                        } else {
                            None
                        }
                    };
                }
            } else {
                quote! {
                    let #field_name = <#field_type as #crate_path::serialize::xcdr::XcdrDeserialize>::deserialize_xcdr(deserializer)?;
                }
            }
        })
        .collect();

    let field_names: Vec<_> = fields.iter().map(|f| f.ident.as_ref().unwrap()).collect();

    quote! {
        impl #crate_path::serialize::xcdr::XcdrDeserialize for #name {
            fn deserialize_xcdr(deserializer: &mut #crate_path::serialize::xcdr::XcdrDeserializer) -> #crate_path::serialize::xcdr::XcdrResult<Self> {
                #(#field_deserializations)*

                Ok(#name {
                    #(#field_names,)*
                })
            }
        }
    }
}

/// Generate deserialization for Appendable types (with DHEADER, sequential fields)
fn generate_appendable_deserialize_impl(
    name: &syn::Ident,
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
    crate_path: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let field_deserializations: Vec<_> = fields
        .iter()
        .map(|field| {
            let field_name = field.ident.as_ref().unwrap();
            let field_type = &field.ty;
            let field_config = parse_field_attributes(field);

            if field_config.optional {
                quote! {
                    let #field_name = {
                        let has_value = deserializer.deserialize_bool()?;
                        if has_value {
                            Some(<#field_type as #crate_path::serialize::xcdr::XcdrDeserialize>::deserialize_xcdr(deserializer)?)
                        } else {
                            None
                        }
                    };
                }
            } else {
                quote! {
                    let #field_name = <#field_type as #crate_path::serialize::xcdr::XcdrDeserialize>::deserialize_xcdr(deserializer)?;
                }
            }
        })
        .collect();

    let field_names: Vec<_> = fields.iter().map(|f| f.ident.as_ref().unwrap()).collect();

    quote! {
        impl #crate_path::serialize::xcdr::XcdrDeserialize for #name {
            fn deserialize_xcdr(deserializer: &mut #crate_path::serialize::xcdr::XcdrDeserializer) -> #crate_path::serialize::xcdr::XcdrResult<Self> {
                let (object_size, start_position) = deserializer.begin_struct()?;
                #(#field_deserializations)*
                deserializer.end_struct(object_size, start_position)?;

                Ok(#name {
                    #(#field_names,)*
                })
            }
        }
    }
}

/// Generate deserialization for Mutable types (with DHEADER and EMHEADER per field)
fn generate_mutable_deserialize_impl(
    name: &syn::Ident,
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
    crate_path: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    // Generate field declarations with Option wrapper
    let field_declarations: Vec<_> = fields
        .iter()
        .map(|field| {
            let field_name = field.ident.as_ref().unwrap();
            let field_type = &field.ty;
            let field_config = parse_field_attributes(field);

            if field_config.optional {
                // Optional fields are already Option<T>, initialize to None
                quote! {
                    let mut #field_name: #field_type = None;
                }
            } else {
                // Required fields: wrap in Option for tracking
                quote! {
                    let mut #field_name: Option<#field_type> = None;
                }
            }
        })
        .collect();

    // Generate match arms for each member_id
    let field_match_arms: Vec<_> = fields
        .iter()
        .enumerate()
        .map(|(index, field)| {
            let field_name = field.ident.as_ref().unwrap();
            let field_type = &field.ty;
            let field_config = parse_field_attributes(field);
            let member_id = field_config.id.unwrap_or(index as u32);

            if field_config.optional {
                // For optional fields, the inner type needs to be extracted
                // We assume it's Option<InnerType>
                quote! {
                    #member_id => {
                        // Deserialize the inner value for optional field
                        #field_name = Some(#crate_path::serialize::xcdr::XcdrDeserialize::deserialize_xcdr(deserializer)?);
                    }
                }
            } else {
                quote! {
                    #member_id => {
                        #field_name = Some(<#field_type as #crate_path::serialize::xcdr::XcdrDeserialize>::deserialize_xcdr(deserializer)?);
                    }
                }
            }
        })
        .collect();

    // Generate field unwrapping for struct construction
    let field_unwraps: Vec<_> = fields
        .iter()
        .map(|field| {
            let field_name = field.ident.as_ref().unwrap();
            let field_name_str = field_name.to_string();
            let field_config = parse_field_attributes(field);

            if field_config.optional {
                // Optional fields: already Option<T>, just use the value
                quote! {
                    #field_name
                }
            } else {
                // Required fields: must be Some, error if None
                quote! {
                    #field_name.ok_or_else(||
                        #crate_path::serialize::core::SerializationError::DeserializationError(
                            format!("Missing required field: {}", #field_name_str)
                        )
                    )?
                }
            }
        })
        .collect();

    let field_names: Vec<_> = fields.iter().map(|f| f.ident.as_ref().unwrap()).collect();

    quote! {
        impl #crate_path::serialize::xcdr::XcdrDeserialize for #name {
            fn deserialize_xcdr(deserializer: &mut #crate_path::serialize::xcdr::XcdrDeserializer) -> #crate_path::serialize::xcdr::XcdrResult<Self> {
                use #crate_path::serialize::cdr::is_sentinel_member_id;

                let (object_size, start_position) = deserializer.begin_struct()?;
                let object_end = start_position + object_size as usize;

                // Declare all fields as Option for tracking
                #(#field_declarations)*

                // Read member headers and route to appropriate fields
                while deserializer.get_position() < object_end {
                    let (member_id, member_length) = deserializer.read_member_header()?;

                    // Check for sentinel
                    if is_sentinel_member_id(member_id) {
                        break;
                    }

                    let member_start = deserializer.get_position();

                    match member_id {
                        #(#field_match_arms)*
                        _ => {
                            // Unknown member_id - skip for forward compatibility
                            deserializer.skip_member(member_length)?;
                        }
                    }

                    // Verify we consumed the correct number of bytes, skip remaining if needed
                    let consumed = deserializer.get_position() - member_start;
                    if consumed < member_length as usize {
                        deserializer.skip((member_length as usize) - consumed)?;
                    }
                }

                deserializer.end_struct(object_size, start_position)?;

                Ok(#name {
                    #(#field_names: #field_unwraps,)*
                })
            }
        }
    }
}
