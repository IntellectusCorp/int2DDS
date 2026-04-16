use quote::quote;
use syn::DeriveInput;

use crate::codegen::union_ops::wrap_with_emheader;
use crate::codegen::utils::{
    get_serialization_method, is_option_type, resolve_member_id, SerializationMethod,
};
use crate::codegen::{
    generate_additional_derives, generate_field_deserialization,
    generate_field_deserialization_xcdr, generate_field_deserialization_xcdr_per_field_dheader,
    generate_field_serialization, generate_field_serialization_xcdr,
    generate_key_field_serialization, generate_key_field_serialization_xcdr, generate_key_methods,
    generate_multi_key_methods, generate_non_key_field_serialization,
    generate_non_key_field_serialization_xcdr, parse_field_attributes, KeyFieldInfo,
    MultiKeyFieldInfo,
};
use crate::codegen::{quote_extensibility_tokens, DdsTypeConfig, ExtensibilityKind};

/// Generics context for code generation.
/// For non-generic types, all fields are empty token streams,
/// producing identical output to the non-generic case.
pub(crate) struct GenCtx {
    /// e.g., `<T: DdsType + CdrSerialize + ...>` for impl headers
    pub impl_generics: proc_macro2::TokenStream,
    /// e.g., `<T>` for type references
    pub ty_generics: proc_macro2::TokenStream,
    /// e.g., `where T: ...`
    pub where_clause: proc_macro2::TokenStream,
    /// `#name #ty_generics` for use in type position (downcast_ref, TypeId, etc.)
    pub full_type: proc_macro2::TokenStream,
    /// `#type_support_name #ty_generics` for TypeSupport type references
    pub full_ts_type: proc_macro2::TokenStream,
    /// Whether there are generic type params
    pub has_type_params: bool,
}

/// Add DDS trait bounds to all type parameters for generated impl blocks.
fn add_dds_bounds(
    generics: &syn::Generics,
    crate_path: &proc_macro2::TokenStream,
) -> syn::Generics {
    let mut bounded = generics.clone();
    for param in &mut bounded.params {
        if let syn::GenericParam::Type(ref mut tp) = *param {
            tp.bounds.push(syn::parse_quote!(#crate_path::dcps::topic::type_support::DdsType));
            tp.bounds.push(syn::parse_quote!(#crate_path::serialize::cdr::CdrSerialize));
            tp.bounds.push(syn::parse_quote!(#crate_path::serialize::cdr::CdrDeserialize));
            tp.bounds.push(syn::parse_quote!(#crate_path::serialize::xcdr::XcdrSerialize));
            tp.bounds.push(syn::parse_quote!(#crate_path::serialize::xcdr::XcdrDeserialize));
        }
    }
    bounded
}

/// Generate DdsType implementation for struct types
pub fn derive_struct_impl(
    input: &DeriveInput,
    name: &syn::Ident,
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
    type_config: &DdsTypeConfig,
) -> proc_macro2::TokenStream {
    let crate_path = &type_config.crate_path;
    let type_support_name = quote::format_ident!("{}TypeSupport", name);

    // Generics support
    let has_type_params = input.generics.type_params().count() > 0;
    let (_, ty_gen, _) = input.generics.split_for_impl();
    let ty_generics_ts = quote! { #ty_gen };
    let bounded_generics = add_dds_bounds(&input.generics, crate_path);
    let (bounded_impl_gen, _, bounded_where) = bounded_generics.split_for_impl();
    let impl_generics_ts = quote! { #bounded_impl_gen };
    let where_clause_ts = quote! { #bounded_where };
    let full_type = quote! { #name #ty_generics_ts };
    let full_ts_type = quote! { #type_support_name #ty_generics_ts };

    let gc = GenCtx {
        impl_generics: impl_generics_ts.clone(),
        ty_generics: ty_generics_ts.clone(),
        where_clause: where_clause_ts.clone(),
        full_type: full_type.clone(),
        full_ts_type: full_ts_type.clone(),
        has_type_params,
    };

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
    let type_support_struct = if has_type_params {
        let type_params: Vec<_> = input.generics.type_params().map(|tp| &tp.ident).collect();
        let (orig_impl_gen, _, _) = input.generics.split_for_impl();
        quote! {
            pub struct #type_support_name #ty_generics_ts (
                #(core::marker::PhantomData<#type_params>,)*
            );
            impl #orig_impl_gen Default for #type_support_name #ty_generics_ts {
                fn default() -> Self {
                    Self(#(core::marker::PhantomData::<#type_params>,)*)
                }
            }
        }
    } else {
        quote! {
            #[derive(Default)]
            pub struct #type_support_name;
        }
    };

    // Generate CDR and XCDR field information
    let field_serialization = generate_field_serialization(fields, crate_path);
    let field_deserialization = generate_field_deserialization(fields, name, crate_path);
    let xcdr_field_serialization = generate_field_serialization_xcdr(fields, crate_path);
    let xcdr_field_deserialization = generate_field_deserialization_xcdr(fields, name, crate_path);

    // Generate per-field DHEADER deserialization for dust-dds compatibility
    let xcdr_field_deserialization_per_field_dheader =
        generate_field_deserialization_xcdr_per_field_dheader(fields, name, crate_path);

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
        generate_key_methods(None, &gc.full_type, &field_deserialization, crate_path)
    } else if all_key_fields.len() == 1 {
        // Single key
        generate_key_methods(
            Some(&all_key_fields[0]),
            &gc.full_type,
            &field_deserialization,
            crate_path,
        )
    } else {
        // Multiple keys
        let multi_key_info = MultiKeyFieldInfo { fields: all_key_fields };
        generate_multi_key_methods(Some(&multi_key_info), &gc.full_type, crate_path)
    };

    // Generate field access methods
    let field_access_impl = generate_field_access_methods(fields, name, crate_path, &gc);

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
        &xcdr_field_deserialization_per_field_dheader,
        &key_field_serialization,
        &key_field_serialization_xcdr,
        &non_key_field_serialization,
        &non_key_field_serialization_xcdr,
        &serialize_key_impl,
        &deserialize_key_impl,
        &compute_key_impl,
        crate_path,
        type_config.extensibility,
        type_config.type_name.as_deref(),
        &gc,
    );

    let (field_accessor_impl, dds_type_impl) = if type_config.skip_field_accessor {
        // User will manually impl DdsType with custom FieldAccessor
        (quote! {}, quote! {})
    } else {
        (
            quote! {
                impl #impl_generics_ts #crate_path::dcps::topic::type_support::FieldAccessor for #full_ts_type #where_clause_ts {
                    #field_access_impl
                }
            },
            quote! {
                impl #impl_generics_ts #crate_path::dcps::topic::type_support::DdsType for #full_type #where_clause_ts {
                    type TypeSupport = #full_ts_type;
                    type FieldAccessor = #full_ts_type;
                }
            },
        )
    };

    // Generate CdrSerialize and CdrDeserialize trait implementations
    let cdr_serialize_impl = generate_cdr_serialize_impl(name, fields, crate_path, &gc);

    let cdr_deserialize_impl = generate_cdr_deserialize_impl(name, fields, crate_path, &gc);

    // Generate XcdrSerialize and XcdrDeserialize trait implementations
    let xcdr_serialize_impl = generate_xcdr_serialize_impl(
        name,
        fields,
        crate_path,
        type_config.extensibility,
        type_config.autoid,
        &gc,
    );

    let xcdr_deserialize_impl = generate_xcdr_deserialize_impl(
        name,
        fields,
        crate_path,
        type_config.extensibility,
        type_config.autoid,
        &gc,
    );

    let additional_derives = generate_additional_derives(input, name, type_config);

    // Generate HasTypeObject implementation for XTypes support
    let has_type_object_impl =
        crate::codegen::type_object::generate_has_type_object_impl(name, fields, type_config, &gc);

    quote! {
        #type_support_struct
        #type_support_impl
        #field_accessor_impl
        #dds_type_impl
        #cdr_serialize_impl
        #cdr_deserialize_impl
        #xcdr_serialize_impl
        #xcdr_deserialize_impl
        #has_type_object_impl
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

/// Generate serialize method implementation (unified: handles both None and Some format)
fn quote_serialize_impl(
    _name: &syn::Ident,
    cdr_field_serialization: &proc_macro2::TokenStream,
    xcdr_field_serialization: &proc_macro2::TokenStream,
    crate_path: &proc_macro2::TokenStream,
    gc: &GenCtx,
) -> proc_macro2::TokenStream {
    let full_type = &gc.full_type;
    quote! {
        fn serialize(&self, data: &dyn std::any::Any, format: Option<&#crate_path::dcps::topic::type_support::SerializationFormat>) -> #crate_path::dcps::core::error::DdsResult<#crate_path::rtps::common::types::SerializedData> {
            let default_format = #crate_path::dcps::topic::type_support::SerializationFormat::Cdr;
            let format = format.unwrap_or(&default_format);
            if let Some(typed_data) = data.downcast_ref::<#full_type>() {
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

/// Generate deserialize method implementation (unified: handles both None and Some format)
fn quote_deserialize_impl(
    _name: &syn::Ident,
    extensibility: Option<ExtensibilityKind>,
    cdr_field_deserialization: &proc_macro2::TokenStream,
    xcdr_field_deserialization: &proc_macro2::TokenStream,
    xcdr_field_deserialization_per_field_dheader: &proc_macro2::TokenStream,
    crate_path: &proc_macro2::TokenStream,
    gc: &GenCtx,
) -> proc_macro2::TokenStream {
    let full_type = &gc.full_type;
    let builtin_pl_cdr_fallback = quote! {
        if data.len() >= 4 {
            let encoding_id = u16::from_be_bytes([data[0], data[1]]);
            if matches!(encoding_id, 0x0002 | 0x0003) {
                // Discovery builtins are serialized as PL_CDR parameter lists rather than
                // regular CDR/XCDR structs, so use their dedicated parser.
                let type_id = std::any::TypeId::of::<#full_type>();

                if type_id == std::any::TypeId::of::<#crate_path::common::builtin::topic::publication_builtin_topic_data::PublicationBuiltinTopicData>() {
                    let value = #crate_path::common::builtin::topic::publication_builtin_topic_data::PublicationBuiltinTopicData::from_serialized_data(std::sync::Arc::<[u8]>::from(data))
                        .map_err(#crate_path::dcps::core::error::DdsError::Error)?;
                    return Ok(Box::new(value));
                }

                if type_id == std::any::TypeId::of::<#crate_path::common::builtin::topic::subscription_builtin_topic_data::SubscriptionBuiltinTopicData>() {
                    let value = #crate_path::common::builtin::topic::subscription_builtin_topic_data::SubscriptionBuiltinTopicData::from_serialized_data(std::sync::Arc::<[u8]>::from(data))
                        .map_err(#crate_path::dcps::core::error::DdsError::Error)?;
                    return Ok(Box::new(value));
                }
            }
        }
    };
    // Generate format resolution for None case
    let ext_kind = extensibility.unwrap_or(ExtensibilityKind::Appendable);
    let extensibility_tokens = quote_extensibility_tokens(ext_kind, crate_path);
    let none_format_resolution = quote! {
        if data.len() >= 2 {
            let encoding_id = u16::from_be_bytes([data[0], data[1]]);
            match encoding_id {
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
            }
        } else {
            return Err(#crate_path::dcps::core::error::DdsError::Error("Invalid data length".to_string()));
        }
    };

    quote! {
        fn deserialize(&self, data: &[u8], format: Option<&#crate_path::dcps::topic::type_support::SerializationFormat>) -> #crate_path::dcps::core::error::DdsResult<Box<dyn std::any::Any>> {
            #builtin_pl_cdr_fallback

            let resolved_format = match format {
                Some(f) => f.clone(),
                None => {
                    #none_format_resolution
                }
            };
            match &resolved_format {
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

                    // Standard XCDR2 deserialization (single DHEADER for struct)
                    let mut parse_body = |mut deserializer: &mut Xcdr2Deserializer<'_>| -> #crate_path::dcps::core::error::DdsResult<#full_type> {
                        #xcdr_field_deserialization
                        Ok(result)
                    };

                    // Alternative deserialization with per-field DHEADER (for interoperability)
                    let mut parse_body_per_field_dheader = |mut deserializer: &mut Xcdr2Deserializer<'_>| -> #crate_path::dcps::core::error::DdsResult<#full_type> {
                        #xcdr_field_deserialization_per_field_dheader
                        Ok(result)
                    };

                    if use_delimiters {
                        // Try 1: Standard XCDR2 delimited format (single DHEADER for entire struct)
                        let mut delimited_deserializer = Xcdr2Deserializer::new(data)
                            .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;

                        match (|| -> #crate_path::dcps::core::error::DdsResult<#full_type> {
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
                                log::debug!(
                                    "XCDR2 delimited path failed ({}); trying per-field DHEADER mode",
                                    err
                                );
                            }
                        }

                        // Try 2: Per-field DHEADER format (interoperability fallback)
                        let mut compat_deserializer = Xcdr2Deserializer::new(data)
                            .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;

                        match (|| -> #crate_path::dcps::core::error::DdsResult<#full_type> {
                            let value = parse_body_per_field_dheader(&mut compat_deserializer)?;
                            Ok(value)
                        })() {
                            Ok(result) => {
                                log::debug!("XCDR2 per-field DHEADER mode succeeded");
                                return Ok(Box::new(result));
                            }
                            Err(err) => {
                                log::debug!(
                                    "XCDR2 per-field DHEADER path failed ({}); trying without delimiters",
                                    err
                                );
                            }
                        }
                    }

                    // Try 3: Fallback - no delimiters
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
    _type_support_name: &syn::Ident,
    name: &syn::Ident,
    has_key: bool,
    _has_non_key_fields: bool,
    cdr_field_serialization: &proc_macro2::TokenStream,
    cdr_field_deserialization: &proc_macro2::TokenStream,
    xcdr_field_serialization: &proc_macro2::TokenStream,
    xcdr_field_deserialization: &proc_macro2::TokenStream,
    xcdr_field_deserialization_per_field_dheader: &proc_macro2::TokenStream,
    _key_field_serialization: &proc_macro2::TokenStream,
    _key_field_serialization_xcdr: &proc_macro2::TokenStream,
    _non_key_field_serialization: &proc_macro2::TokenStream,
    _non_key_field_serialization_xcdr: &proc_macro2::TokenStream,
    serialize_key_impl: &proc_macro2::TokenStream,
    deserialize_key_impl: &proc_macro2::TokenStream,
    compute_key_impl: &proc_macro2::TokenStream,
    crate_path: &proc_macro2::TokenStream,
    extensibility: Option<ExtensibilityKind>,
    type_name_override: Option<&str>,
    gc: &GenCtx,
) -> proc_macro2::TokenStream {
    let serialize_impl = quote_serialize_impl(
        name,
        cdr_field_serialization,
        xcdr_field_serialization,
        crate_path,
        gc,
    );
    let deserialize_impl = quote_deserialize_impl(
        name,
        extensibility,
        cdr_field_deserialization,
        xcdr_field_deserialization,
        xcdr_field_deserialization_per_field_dheader,
        crate_path,
        gc,
    );

    let extensibility_tokens = quote_extensibility_tokens(
        extensibility.unwrap_or(ExtensibilityKind::Appendable),
        crate_path,
    );

    let impl_generics = &gc.impl_generics;
    let where_clause = &gc.where_clause;
    let full_ts_type = &gc.full_ts_type;
    let full_type = &gc.full_type;

    let get_type_name_impl = if let Some(tn) = type_name_override {
        quote! {
            fn get_type_name(&self) -> &str {
                #tn
            }
        }
    } else if gc.has_type_params {
        quote! {
            fn get_type_name(&self) -> &str {
                // std::any::type_name returns &'static str, unique per concrete type
                std::any::type_name::<#full_type>()
            }
        }
    } else {
        quote! {
            fn get_type_name(&self) -> &str {
                stringify!(#name)
            }
        }
    };

    quote! {
        impl #impl_generics #crate_path::dcps::topic::type_support::TypeSupport for #full_ts_type #where_clause {
            #get_type_name_impl

            fn type_id(&self) -> std::any::TypeId {
                std::any::TypeId::of::<#full_type>()
            }

            fn is_compute_key_provided(&self) -> bool {
                #has_key
            }

            fn get_extensibility_kind(&self) -> #crate_path::serialize::xcdr::ExtensibilityKind {
                #extensibility_tokens
            }

            #serialize_impl

            #deserialize_impl

            #serialize_key_impl

            #deserialize_key_impl

            #compute_key_impl

            fn serialize_key_and_non_key(&self, data: &dyn std::any::Any) -> #crate_path::dcps::core::error::DdsResult<(#crate_path::rtps::common::types::SerializedData, #crate_path::rtps::common::types::SerializedData)> {
                if let Some(typed_data) = data.downcast_ref::<#full_type>() {
                    let key_data = self.serialize_key(data)?;
                    let full_data = self.serialize(data, None)?;
                    Ok((key_data, full_data))
                } else {
                    Err(#crate_path::dcps::core::error::DdsError::BadParameter)
                }
            }

            fn get_type_identifier(&self) -> Option<#crate_path::xtypes::TypeIdentifier> {
                Some(<#full_type as #crate_path::xtypes::HasTypeObject>::type_identifier())
            }

            fn get_type_object(&self) -> Option<#crate_path::xtypes::TypeObject> {
                Some(#crate_path::xtypes::TypeObject::Complete(
                    <#full_type as #crate_path::xtypes::HasTypeObject>::complete_type_object()
                ))
            }
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
    _name: &syn::Ident,
    crate_path: &proc_macro2::TokenStream,
    gc: &GenCtx,
) -> proc_macro2::TokenStream {
    let full_type = &gc.full_type;
    // Handle empty struct case
    if fields.is_empty() {
        return quote! {
            fn get_field_value(&self, data: &dyn std::any::Any, field_path: &str) -> #crate_path::dcps::core::error::DdsResult<#crate_path::topic::sql::ast::Parameter> {
                if data.downcast_ref::<#full_type>().is_some() {
                    Err(#crate_path::dcps::core::error::DdsError::Error(format!("Field '{}' not found", field_path)))
                } else {
                    Err(#crate_path::dcps::core::error::DdsError::BadParameter)
                }
            }

            fn has_field(&self, _field_path: &str) -> bool {
                false
            }
        };
    }

    let field_names: Vec<_> = fields
        .iter()
        .map(|field| {
            let field_name_str = field.ident.as_ref().unwrap().to_string();
            quote! { #field_name_str }
        })
        .collect();

    let helper_fn = quote_field_to_parameter_conversion(crate_path);

    // Direct field access (no dot) — leaf value via any_to_parameter
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

    // Nested field access (dot notation) — delegate to nested type's TypeSupport using autoref specialization.
    // If the field type implements DdsType, the inherent method on NestedAccessor<T>
    // is resolved. Otherwise, the fallback trait on &NestedAccessor<T> returns false/error.
    let nested_get_matches: Vec<_> = fields
        .iter()
        .map(|field| {
            let field_name = field.ident.as_ref().unwrap();
            let field_name_str = field_name.to_string();
            let field_type = &field.ty;

            quote! {
                #field_name_str => {
                    use #crate_path::dcps::topic::type_support::nested_access::*;
                    let accessor = NestedAccessor::<#field_type>(core::marker::PhantomData);
                    accessor.nested_get_field_value(&typed_data.#field_name as &dyn std::any::Any, rest)
                },
            }
        })
        .collect();

    let nested_has_matches: Vec<_> = fields
        .iter()
        .map(|field| {
            let field_name_str = field.ident.as_ref().unwrap().to_string();
            let field_type = &field.ty;

            quote! {
                #field_name_str => {
                    use #crate_path::dcps::topic::type_support::nested_access::*;
                    let accessor = NestedAccessor::<#field_type>(core::marker::PhantomData);
                    accessor.nested_has_field(rest)
                },
            }
        })
        .collect();

    quote! {
        fn get_field_value(&self, data: &dyn std::any::Any, field_path: &str) -> #crate_path::dcps::core::error::DdsResult<#crate_path::topic::sql::ast::Parameter> {
            #helper_fn

            if let Some(typed_data) = data.downcast_ref::<#full_type>() {
                if let Some((first, rest)) = field_path.split_once('.') {
                    match first {
                        #(#nested_get_matches)*
                        _ => Err(#crate_path::dcps::core::error::DdsError::Error(
                            format!("Field '{}' not found or not a nested type", first)
                        )),
                    }
                } else {
                    match field_path {
                        #(#field_matches)*
                        _ => Err(#crate_path::dcps::core::error::DdsError::Error(format!("Field '{}' not found", field_path))),
                    }
                }
            } else {
                Err(#crate_path::dcps::core::error::DdsError::BadParameter)
            }
        }

        fn has_field(&self, field_path: &str) -> bool {
            if let Some((first, rest)) = field_path.split_once('.') {
                match first {
                    #(#nested_has_matches)*
                    _ => false,
                }
            } else {
                matches!(field_path, #(#field_names)|*)
            }
        }
    }
}

/// Generate CdrSerialize trait implementation
fn generate_cdr_serialize_impl(
    name: &syn::Ident,
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
    crate_path: &proc_macro2::TokenStream,
    gc: &GenCtx,
) -> proc_macro2::TokenStream {
    let impl_generics = &gc.impl_generics;
    let ty_generics = &gc.ty_generics;
    let where_clause = &gc.where_clause;
    // Handle empty struct case
    if fields.is_empty() {
        return quote! {
            impl #impl_generics #crate_path::serialize::cdr::CdrSerialize for #name #ty_generics #where_clause {
                fn serialize_cdr(&self, _serializer: &mut #crate_path::serialize::cdr::CdrSerializer) -> #crate_path::serialize::cdr::CdrResult<()> {
                    Ok(())
                }
            }
        };
    }

    // XCDR1 has no spec-compliant Option<T> wire format.
    // If any field is Option<T>, emit a stub impl that errors at runtime so discovery
    // types (which carry Option fields but never actually use XCDR1) still compile.
    if fields.iter().any(|f| is_option_type(&f.ty)) {
        return quote! {
            impl #impl_generics #crate_path::serialize::cdr::CdrSerialize for #name #ty_generics #where_clause {
                fn serialize_cdr(&self, _serializer: &mut #crate_path::serialize::cdr::CdrSerializer) -> #crate_path::serialize::cdr::CdrResult<()> {
                    Err(#crate_path::serialize::cdr::CdrError::SerializationError(
                        concat!("XCDR1 does not support Option<T> fields in '", stringify!(#name), "' (PL_CDR v1 pending T2-1); use XCDR2").to_string(),
                    ))
                }
            }
        };
    }

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
        impl #impl_generics #crate_path::serialize::cdr::CdrSerialize for #name #ty_generics #where_clause {
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
    gc: &GenCtx,
) -> proc_macro2::TokenStream {
    let impl_generics = &gc.impl_generics;
    let ty_generics = &gc.ty_generics;
    let where_clause = &gc.where_clause;
    // Handle empty struct case
    if fields.is_empty() {
        return quote! {
            impl #impl_generics #crate_path::serialize::cdr::CdrDeserialize for #name #ty_generics #where_clause {
                fn deserialize_cdr(_deserializer: &mut #crate_path::serialize::cdr::CdrDeserializer) -> #crate_path::serialize::cdr::CdrResult<Self> {
                    Ok(#name {})
                }
            }
        };
    }

    // XCDR1 has no spec-compliant Option<T> wire format (PL_CDR v1 tracked in T2-1).
    // Emit a stub impl that errors at runtime for structs with Option fields.
    if fields.iter().any(|f| is_option_type(&f.ty)) {
        return quote! {
            impl #impl_generics #crate_path::serialize::cdr::CdrDeserialize for #name #ty_generics #where_clause {
                fn deserialize_cdr(_deserializer: &mut #crate_path::serialize::cdr::CdrDeserializer) -> #crate_path::serialize::cdr::CdrResult<Self> {
                    Err(#crate_path::serialize::cdr::CdrError::DeserializationError(
                        concat!("XCDR1 does not support Option<T> fields in '", stringify!(#name), "' (PL_CDR v1 pending T2-1); use XCDR2").to_string(),
                    ))
                }
            }
        };
    }

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
        impl #impl_generics #crate_path::serialize::cdr::CdrDeserialize for #name #ty_generics #where_clause {
            fn deserialize_cdr(deserializer: &mut #crate_path::serialize::cdr::CdrDeserializer) -> #crate_path::serialize::cdr::CdrResult<Self> {
                #(#field_deserializations)*

                Ok(#name {
                    #(#field_names,)*
                })
            }
        }
    }
}

/// Map primitive Vec SerializationMethod to the specialized serializer method name.
/// Returns None for non-primitive types (they use the blanket XcdrSerialize impl with DHEADER).
fn primitive_vec_serialize_method(method: SerializationMethod) -> Option<&'static str> {
    match method {
        SerializationMethod::VecU8 => Some("serialize_byte_sequence"),
        SerializationMethod::VecU16 => Some("serialize_u16_sequence"),
        SerializationMethod::VecU32 => Some("serialize_u32_sequence"),
        SerializationMethod::VecU64 => Some("serialize_u64_sequence"),
        SerializationMethod::VecI8 => Some("serialize_i8_sequence"),
        SerializationMethod::VecI16 => Some("serialize_i16_sequence"),
        SerializationMethod::VecI32 => Some("serialize_i32_sequence"),
        SerializationMethod::VecI64 => Some("serialize_i64_sequence"),
        SerializationMethod::VecF32 => Some("serialize_f32_sequence"),
        SerializationMethod::VecF64 => Some("serialize_f64_sequence"),
        SerializationMethod::VecBool => Some("serialize_bool_sequence"),
        SerializationMethod::VecChar => Some("serialize_char_sequence"),
        SerializationMethod::VecString => Some("serialize_string_sequence"),
        _ => None,
    }
}

/// Map primitive Vec SerializationMethod to the specialized deserializer method name.
/// Returns None for non-primitive types (they use the blanket XcdrDeserialize impl with DHEADER).
fn primitive_vec_deserialize_method(method: SerializationMethod) -> Option<&'static str> {
    match method {
        SerializationMethod::VecU8 => Some("deserialize_byte_sequence"),
        SerializationMethod::VecU16 => Some("deserialize_u16_sequence"),
        SerializationMethod::VecU32 => Some("deserialize_u32_sequence"),
        SerializationMethod::VecU64 => Some("deserialize_u64_sequence"),
        SerializationMethod::VecI8 => Some("deserialize_i8_sequence"),
        SerializationMethod::VecI16 => Some("deserialize_i16_sequence"),
        SerializationMethod::VecI32 => Some("deserialize_i32_sequence"),
        SerializationMethod::VecI64 => Some("deserialize_i64_sequence"),
        SerializationMethod::VecF32 => Some("deserialize_f32_sequence"),
        SerializationMethod::VecF64 => Some("deserialize_f64_sequence"),
        SerializationMethod::VecBool => Some("deserialize_bool_sequence"),
        SerializationMethod::VecChar => Some("deserialize_char_sequence"),
        SerializationMethod::VecString => Some("deserialize_string_sequence"),
        _ => None,
    }
}

/// Generate XcdrSerialize trait implementation
fn generate_xcdr_serialize_impl(
    name: &syn::Ident,
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
    crate_path: &proc_macro2::TokenStream,
    extensibility: Option<ExtensibilityKind>,
    autoid: Option<crate::codegen::utils::AutoIdKind>,
    gc: &GenCtx,
) -> proc_macro2::TokenStream {
    let impl_generics = &gc.impl_generics;
    let ty_generics = &gc.ty_generics;
    let where_clause = &gc.where_clause;
    let is_mutable = matches!(extensibility, Some(ExtensibilityKind::Mutable));

    // Handle empty struct case
    if fields.is_empty() {
        let serialization_body = if !matches!(extensibility, Some(ExtensibilityKind::Final)) {
            quote! {
                let size_pos = serializer.begin_struct()?;
                serializer.end_struct(size_pos)?;
                Ok(())
            }
        } else {
            quote! {
                Ok(())
            }
        };

        return quote! {
            impl #impl_generics #crate_path::serialize::xcdr::XcdrSerialize for #name #ty_generics #where_clause {
                fn serialize_xcdr(&self, serializer: &mut #crate_path::serialize::xcdr::XcdrSerializer) -> #crate_path::serialize::xcdr::XcdrResult<()> {
                    #serialization_body
                }
            }
        };
    }

    // Generate field serialization calls
    // For primitive Vec types, use specialized serializer methods (no DHEADER).
    // For non-primitive types (String, struct, etc.), use XcdrSerialize trait (blanket impl adds DHEADER).
    let field_calls: Vec<_> = fields
        .iter()
        .enumerate()
        .map(|(index, field)| {
            let field_name = field.ident.as_ref().unwrap();
            let field_config = parse_field_attributes(field);
            let member_id = resolve_member_id(&field_config, &field_name.to_string(), index, autoid);
            let is_optional = field_config.optional;

            // Check if this field is a primitive Vec type that needs specialized serialization
            let method = get_serialization_method(&field.ty);
            let field_serialize = if let Some(ser_method) = primitive_vec_serialize_method(method) {
                let method_ident = syn::Ident::new(ser_method, field_name.span());
                quote! {
                    serializer.#method_ident(&self.#field_name)?;
                }
            } else {
                quote! {
                    #crate_path::serialize::xcdr::XcdrSerialize::serialize_xcdr(&self.#field_name, serializer)?;
                }
            };

            if is_mutable {
                let must_understand = field_config.must_understand;
                if is_optional {
                    let inner = wrap_with_emheader(
                        quote! { #member_id as u32 },
                        must_understand,
                        quote! {
                            #crate_path::serialize::xcdr::XcdrSerialize::serialize_xcdr(opt_value, serializer)?;
                        },
                    );
                    quote! {
                        if let Some(ref opt_value) = self.#field_name {
                            #inner
                        }
                    }
                } else {
                    wrap_with_emheader(
                        quote! { #member_id as u32 },
                        must_understand,
                        field_serialize,
                    )
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
                    field_serialize
                }
            }
        })
        .collect();

    let serialization_body = if !matches!(extensibility, Some(ExtensibilityKind::Final)) {
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
        impl #impl_generics #crate_path::serialize::xcdr::XcdrSerialize for #name #ty_generics #where_clause {
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
    autoid: Option<crate::codegen::utils::AutoIdKind>,
    gc: &GenCtx,
) -> proc_macro2::TokenStream {
    let impl_generics = &gc.impl_generics;
    let ty_generics = &gc.ty_generics;
    let where_clause = &gc.where_clause;
    // Handle empty struct case
    if fields.is_empty() {
        let deserialization_body = if !matches!(extensibility, Some(ExtensibilityKind::Final)) {
            quote! {
                let (object_size, start_position) = deserializer.begin_struct()?;
                deserializer.end_struct(object_size, start_position)?;
                Ok(#name {})
            }
        } else {
            quote! {
                Ok(#name {})
            }
        };

        return quote! {
            impl #impl_generics #crate_path::serialize::xcdr::XcdrDeserialize for #name #ty_generics #where_clause {
                fn deserialize_xcdr(deserializer: &mut #crate_path::serialize::xcdr::XcdrDeserializer) -> #crate_path::serialize::xcdr::XcdrResult<Self> {
                    #deserialization_body
                }
            }
        };
    }

    let is_mutable = matches!(extensibility, Some(ExtensibilityKind::Mutable));

    if is_mutable {
        generate_mutable_deserialize_impl(name, fields, crate_path, autoid, gc)
    } else if matches!(extensibility, Some(ExtensibilityKind::Final)) {
        generate_final_deserialize_impl(name, fields, crate_path, gc)
    } else {
        generate_appendable_deserialize_impl(name, fields, crate_path, gc)
    }
}

/// Generate deserialization for Final types (no DHEADER)
fn generate_final_deserialize_impl(
    name: &syn::Ident,
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
    crate_path: &proc_macro2::TokenStream,
    gc: &GenCtx,
) -> proc_macro2::TokenStream {
    let impl_generics = &gc.impl_generics;
    let ty_generics = &gc.ty_generics;
    let where_clause = &gc.where_clause;
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
                // For primitive Vec types, use specialized deserializer methods (no DHEADER)
                let method = get_serialization_method(&field.ty);
                if let Some(deser_method) = primitive_vec_deserialize_method(method) {
                    let method_ident = syn::Ident::new(deser_method, field_name.span());
                    quote! {
                        let #field_name = deserializer.#method_ident()?;
                    }
                } else {
                    quote! {
                        let #field_name = <#field_type as #crate_path::serialize::xcdr::XcdrDeserialize>::deserialize_xcdr(deserializer)?;
                    }
                }
            }
        })
        .collect();

    let field_names: Vec<_> = fields.iter().map(|f| f.ident.as_ref().unwrap()).collect();

    quote! {
        impl #impl_generics #crate_path::serialize::xcdr::XcdrDeserialize for #name #ty_generics #where_clause {
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
    gc: &GenCtx,
) -> proc_macro2::TokenStream {
    let impl_generics = &gc.impl_generics;
    let ty_generics = &gc.ty_generics;
    let where_clause = &gc.where_clause;
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
                // For primitive Vec types, use specialized deserializer methods (no DHEADER)
                let method = get_serialization_method(&field.ty);
                if let Some(deser_method) = primitive_vec_deserialize_method(method) {
                    let method_ident = syn::Ident::new(deser_method, field_name.span());
                    quote! {
                        let #field_name = deserializer.#method_ident()?;
                    }
                } else {
                    quote! {
                        let #field_name = <#field_type as #crate_path::serialize::xcdr::XcdrDeserialize>::deserialize_xcdr(deserializer)?;
                    }
                }
            }
        })
        .collect();

    let field_names: Vec<_> = fields.iter().map(|f| f.ident.as_ref().unwrap()).collect();

    quote! {
        impl #impl_generics #crate_path::serialize::xcdr::XcdrDeserialize for #name #ty_generics #where_clause {
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
    autoid: Option<crate::codegen::utils::AutoIdKind>,
    gc: &GenCtx,
) -> proc_macro2::TokenStream {
    let impl_generics = &gc.impl_generics;
    let ty_generics = &gc.ty_generics;
    let where_clause = &gc.where_clause;
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
            let member_id = resolve_member_id(&field_config, &field_name.to_string(), index, autoid);

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
                // For primitive Vec types, use specialized deserializer methods
                let method = get_serialization_method(&field.ty);
                if let Some(deser_method) = primitive_vec_deserialize_method(method) {
                    let method_ident = syn::Ident::new(deser_method, field_name.span());
                    quote! {
                        #member_id => {
                            #field_name = Some(deserializer.#method_ident()?);
                        }
                    }
                } else {
                    quote! {
                        #member_id => {
                            #field_name = Some(<#field_type as #crate_path::serialize::xcdr::XcdrDeserialize>::deserialize_xcdr(deserializer)?);
                        }
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
            let field_type = &field.ty;
            let field_config = parse_field_attributes(field);

            if field_config.optional {
                // Optional fields: already Option<T>, just use the value
                quote! {
                    #field_name
                }
            } else if let Some(ref default_lit) = field_config.default {
                // Field has @default annotation: use default value if not present
                let default_value =
                    crate::codegen::utils::literal_to_tokens(default_lit, field_type);
                quote! {
                    #field_name.unwrap_or_else(|| #default_value)
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
        impl #impl_generics #crate_path::serialize::xcdr::XcdrDeserialize for #name #ty_generics #where_clause {
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

// ============================================================================
// Tuple Struct Support
// ============================================================================

/// Generate DdsType implementation for tuple struct types (e.g., struct Foo(pub u8))
pub fn derive_tuple_struct_impl(
    input: &DeriveInput,
    name: &syn::Ident,
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
    type_config: &DdsTypeConfig,
) -> proc_macro2::TokenStream {
    let crate_path = &type_config.crate_path;
    let type_support_name = quote::format_ident!("{}TypeSupport", name);

    // Tuple structs don't have key fields (no named fields with #[dds(key)] attribute)
    let has_key = false;

    // Non-generic GenCtx for tuple structs
    let tuple_gc = GenCtx {
        impl_generics: quote! {},
        ty_generics: quote! {},
        where_clause: quote! {},
        full_type: quote! { #name },
        full_ts_type: quote! { #type_support_name },
        has_type_params: false,
    };

    // Define TypeSupport struct
    let type_support_struct = quote! {
        #[derive(Default)]
        pub struct #type_support_name;
    };

    // Generate field serialization/deserialization for tuple fields
    let field_count = fields.len();

    // Generate CDR serialization
    let cdr_field_serialization = generate_tuple_field_serialization(fields, crate_path);
    let cdr_field_deserialization = generate_tuple_field_deserialization(fields, name, crate_path);

    // Generate XCDR serialization
    let xcdr_field_serialization = generate_tuple_field_serialization_xcdr(fields, crate_path);
    let xcdr_field_deserialization =
        generate_tuple_field_deserialization_xcdr(fields, name, crate_path);

    // Generate per-field DHEADER deserialization for interoperability
    let xcdr_field_deserialization_per_field_dheader =
        generate_tuple_field_deserialization_xcdr_per_field_dheader(fields, name, crate_path);

    // Generate key-related methods (no keys for tuple structs)
    let (serialize_key_impl, deserialize_key_impl, compute_key_impl) =
        generate_key_methods(None, &tuple_gc.full_type, &cdr_field_deserialization, crate_path);

    // Generate field access methods
    let field_access_impl = generate_tuple_field_access_methods(field_count, name, crate_path);

    // TypeSupport trait implementation
    let type_support_impl = generate_tuple_type_support_impl(
        &type_support_name,
        name,
        has_key,
        &cdr_field_serialization,
        &cdr_field_deserialization,
        &xcdr_field_serialization,
        &xcdr_field_deserialization,
        &xcdr_field_deserialization_per_field_dheader,
        &serialize_key_impl,
        &deserialize_key_impl,
        &compute_key_impl,
        crate_path,
        type_config.extensibility,
    );

    let field_accessor_impl = quote! {
        impl #crate_path::dcps::topic::type_support::FieldAccessor for #type_support_name {
            #field_access_impl
        }
    };

    let dds_type_impl = quote! {
        impl #crate_path::dcps::topic::type_support::DdsType for #name {
            type TypeSupport = #type_support_name;
            type FieldAccessor = #type_support_name;
        }
    };

    // Generate CdrSerialize and CdrDeserialize trait implementations
    let cdr_serialize_impl = generate_tuple_cdr_serialize_impl(name, fields, crate_path);
    let cdr_deserialize_impl = generate_tuple_cdr_deserialize_impl(name, fields, crate_path);

    // Generate XcdrSerialize and XcdrDeserialize trait implementations
    let xcdr_serialize_impl =
        generate_tuple_xcdr_serialize_impl(name, fields, crate_path, type_config.extensibility);
    let xcdr_deserialize_impl =
        generate_tuple_xcdr_deserialize_impl(name, fields, crate_path, type_config.extensibility);

    let additional_derives = generate_additional_derives(input, name, type_config);

    quote! {
        #type_support_struct
        #type_support_impl
        #field_accessor_impl
        #dds_type_impl
        #cdr_serialize_impl
        #cdr_deserialize_impl
        #xcdr_serialize_impl
        #xcdr_deserialize_impl
        #additional_derives
    }
}

/// Generate tuple field serialization code for CDR
fn generate_tuple_field_serialization(
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
    crate_path: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let field_serializations: Vec<_> = fields
        .iter()
        .enumerate()
        .map(|(idx, _field)| {
            let idx = syn::Index::from(idx);
            quote! {
                #crate_path::serialize::cdr::CdrSerialize::serialize_cdr(&typed_data.#idx, &mut serializer)
                    .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
            }
        })
        .collect();

    quote! { #(#field_serializations)* }
}

/// Generate tuple field deserialization code for CDR
fn generate_tuple_field_deserialization(
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
    name: &syn::Ident,
    crate_path: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let field_deserializations: Vec<_> = fields
        .iter()
        .enumerate()
        .map(|(idx, field)| {
            let field_var = quote::format_ident!("field_{}", idx);
            let field_type = &field.ty;
            quote! {
                let #field_var = <#field_type as #crate_path::serialize::cdr::CdrDeserialize>::deserialize_cdr(&mut deserializer)
                    .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
            }
        })
        .collect();

    let field_vars: Vec<_> =
        (0..fields.len()).map(|idx| quote::format_ident!("field_{}", idx)).collect();

    quote! {
        #(#field_deserializations)*
        let result = #name(#(#field_vars),*);
    }
}

/// Generate tuple field serialization code for XCDR
fn generate_tuple_field_serialization_xcdr(
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
    crate_path: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let field_serializations: Vec<_> = fields
        .iter()
        .enumerate()
        .map(|(idx, _field)| {
            let idx = syn::Index::from(idx);
            quote! {
                #crate_path::serialize::xcdr::XcdrSerialize::serialize_xcdr(&typed_data.#idx, &mut serializer)
                    .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
            }
        })
        .collect();

    quote! { #(#field_serializations)* }
}

/// Generate tuple field deserialization code for XCDR
fn generate_tuple_field_deserialization_xcdr(
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
    name: &syn::Ident,
    crate_path: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let field_deserializations: Vec<_> = fields
        .iter()
        .enumerate()
        .map(|(idx, field)| {
            let field_var = quote::format_ident!("field_{}", idx);
            let field_type = &field.ty;
            quote! {
                let #field_var = <#field_type as #crate_path::serialize::xcdr::XcdrDeserialize>::deserialize_xcdr(&mut deserializer)
                    .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
            }
        })
        .collect();

    let field_vars: Vec<_> =
        (0..fields.len()).map(|idx| quote::format_ident!("field_{}", idx)).collect();

    quote! {
        #(#field_deserializations)*
        let result = #name(#(#field_vars),*);
    }
}

/// Generate tuple field deserialization code for XCDR with per-field DHEADER
/// Uses DHEADER value for forward compatibility - skips remaining bytes if field has extra data.
fn generate_tuple_field_deserialization_xcdr_per_field_dheader(
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
    name: &syn::Ident,
    crate_path: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let field_deserializations: Vec<_> = fields
        .iter()
        .enumerate()
        .map(|(idx, field)| {
            let field_var = quote::format_ident!("field_{}", idx);
            let field_type = &field.ty;
            // Read DHEADER before each field (interoperability format)
            // Use DHEADER value to skip remaining bytes for forward compatibility
            quote! {
                let __field_size = deserializer.read_dheader()
                    .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
                let __field_start = {
                    use #crate_path::serialize::DeserializerReader;
                    deserializer.get_position()
                };

                let #field_var = <#field_type as #crate_path::serialize::xcdr::XcdrDeserialize>::deserialize_xcdr(&mut deserializer)
                    .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;

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
        })
        .collect();

    let field_vars: Vec<_> =
        (0..fields.len()).map(|idx| quote::format_ident!("field_{}", idx)).collect();

    quote! {
        #(#field_deserializations)*
        let result = #name(#(#field_vars),*);
    }
}

/// Generate field access methods for tuple struct
fn generate_tuple_field_access_methods(
    field_count: usize,
    name: &syn::Ident,
    crate_path: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    // Handle empty tuple struct case
    if field_count == 0 {
        return quote! {
            fn get_field_value(&self, data: &dyn std::any::Any, field_path: &str) -> #crate_path::dcps::core::error::DdsResult<#crate_path::topic::sql::ast::Parameter> {
                if data.downcast_ref::<#name>().is_some() {
                    Err(#crate_path::dcps::core::error::DdsError::Error(format!("Field '{}' not found", field_path)))
                } else {
                    Err(#crate_path::dcps::core::error::DdsError::BadParameter)
                }
            }

            fn has_field(&self, _field_path: &str) -> bool {
                false
            }
        };
    }

    // For tuple structs, we support accessing fields by index as string ("0", "1", etc.)
    let field_indices: Vec<_> = (0..field_count)
        .map(|idx| {
            let idx_str = idx.to_string();
            quote! { #idx_str }
        })
        .collect();

    let field_matches: Vec<_> = (0..field_count)
        .map(|idx| {
            let idx_str = idx.to_string();
            let idx_token = syn::Index::from(idx);
            quote! {
                #idx_str => {
                    any_to_parameter(&typed_data.#idx_token as &dyn std::any::Any)
                },
            }
        })
        .collect();

    let helper_fn = quote_field_to_parameter_conversion(crate_path);

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
            matches!(field_path, #(#field_indices)|*)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn generate_tuple_type_support_impl(
    type_support_name: &syn::Ident,
    name: &syn::Ident,
    has_key: bool,
    cdr_field_serialization: &proc_macro2::TokenStream,
    cdr_field_deserialization: &proc_macro2::TokenStream,
    xcdr_field_serialization: &proc_macro2::TokenStream,
    xcdr_field_deserialization: &proc_macro2::TokenStream,
    xcdr_field_deserialization_per_field_dheader: &proc_macro2::TokenStream,
    serialize_key_impl: &proc_macro2::TokenStream,
    deserialize_key_impl: &proc_macro2::TokenStream,
    compute_key_impl: &proc_macro2::TokenStream,
    crate_path: &proc_macro2::TokenStream,
    extensibility: Option<ExtensibilityKind>,
) -> proc_macro2::TokenStream {
    // Non-generic GenCtx for tuple structs
    let tuple_gc = GenCtx {
        impl_generics: quote! {},
        ty_generics: quote! {},
        where_clause: quote! {},
        full_type: quote! { #name },
        full_ts_type: quote! { #type_support_name },
        has_type_params: false,
    };

    let serialize_impl = quote_serialize_impl(
        name,
        cdr_field_serialization,
        xcdr_field_serialization,
        crate_path,
        &tuple_gc,
    );
    let deserialize_impl = quote_deserialize_impl(
        name,
        extensibility,
        cdr_field_deserialization,
        xcdr_field_deserialization,
        xcdr_field_deserialization_per_field_dheader,
        crate_path,
        &tuple_gc,
    );

    let extensibility_tokens = quote_extensibility_tokens(
        extensibility.unwrap_or(ExtensibilityKind::Appendable),
        crate_path,
    );

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

            #serialize_key_impl

            #deserialize_key_impl

            #compute_key_impl

            fn serialize_key_and_non_key(&self, data: &dyn std::any::Any) -> #crate_path::dcps::core::error::DdsResult<(#crate_path::rtps::common::types::SerializedData, #crate_path::rtps::common::types::SerializedData)> {
                if data.downcast_ref::<#name>().is_some() {
                    let key_data = self.serialize_key(data)?;
                    let full_data = self.serialize(data, None)?;
                    Ok((key_data, full_data))
                } else {
                    Err(#crate_path::dcps::core::error::DdsError::BadParameter)
                }
            }

            // Tuple structs don't have HasTypeObject, use default (None)
        }
    }
}

/// Generate CdrSerialize trait implementation for tuple struct
fn generate_tuple_cdr_serialize_impl(
    name: &syn::Ident,
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
    crate_path: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    if fields.is_empty() {
        return quote! {
            impl #crate_path::serialize::cdr::CdrSerialize for #name {
                fn serialize_cdr(&self, _serializer: &mut #crate_path::serialize::cdr::CdrSerializer) -> #crate_path::serialize::cdr::CdrResult<()> {
                    Ok(())
                }
            }
        };
    }

    let field_calls: Vec<_> = fields
        .iter()
        .enumerate()
        .map(|(idx, _field)| {
            let idx = syn::Index::from(idx);
            quote! {
                #crate_path::serialize::cdr::CdrSerialize::serialize_cdr(&self.#idx, serializer)?;
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

/// Generate CdrDeserialize trait implementation for tuple struct
fn generate_tuple_cdr_deserialize_impl(
    name: &syn::Ident,
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
    crate_path: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    if fields.is_empty() {
        return quote! {
            impl #crate_path::serialize::cdr::CdrDeserialize for #name {
                fn deserialize_cdr(_deserializer: &mut #crate_path::serialize::cdr::CdrDeserializer) -> #crate_path::serialize::cdr::CdrResult<Self> {
                    Ok(#name())
                }
            }
        };
    }

    let field_deserializations: Vec<_> = fields
        .iter()
        .enumerate()
        .map(|(idx, field)| {
            let field_var = quote::format_ident!("field_{}", idx);
            let field_type = &field.ty;
            quote! {
                let #field_var = <#field_type as #crate_path::serialize::cdr::CdrDeserialize>::deserialize_cdr(deserializer)?;
            }
        })
        .collect();

    let field_vars: Vec<_> =
        (0..fields.len()).map(|idx| quote::format_ident!("field_{}", idx)).collect();

    quote! {
        impl #crate_path::serialize::cdr::CdrDeserialize for #name {
            fn deserialize_cdr(deserializer: &mut #crate_path::serialize::cdr::CdrDeserializer) -> #crate_path::serialize::cdr::CdrResult<Self> {
                #(#field_deserializations)*
                Ok(#name(#(#field_vars),*))
            }
        }
    }
}

/// Generate XcdrSerialize trait implementation for tuple struct
fn generate_tuple_xcdr_serialize_impl(
    name: &syn::Ident,
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
    crate_path: &proc_macro2::TokenStream,
    extensibility: Option<ExtensibilityKind>,
) -> proc_macro2::TokenStream {
    if fields.is_empty() {
        let serialization_body = if matches!(
            extensibility,
            Some(ExtensibilityKind::Appendable) | Some(ExtensibilityKind::Mutable)
        ) {
            quote! {
                let size_pos = serializer.begin_struct()?;
                serializer.end_struct(size_pos)?;
                Ok(())
            }
        } else {
            quote! {
                Ok(())
            }
        };

        return quote! {
            impl #crate_path::serialize::xcdr::XcdrSerialize for #name {
                fn serialize_xcdr(&self, serializer: &mut #crate_path::serialize::xcdr::XcdrSerializer) -> #crate_path::serialize::xcdr::XcdrResult<()> {
                    #serialization_body
                }
            }
        };
    }

    let field_calls: Vec<_> = fields
        .iter()
        .enumerate()
        .map(|(idx, field)| {
            let idx = syn::Index::from(idx);
            // For primitive Vec types, use specialized serializer methods (no DHEADER)
            let method = get_serialization_method(&field.ty);
            if let Some(ser_method) = primitive_vec_serialize_method(method) {
                let method_ident = syn::Ident::new(ser_method, proc_macro2::Span::call_site());
                quote! {
                    serializer.#method_ident(&self.#idx)?;
                }
            } else {
                quote! {
                    #crate_path::serialize::xcdr::XcdrSerialize::serialize_xcdr(&self.#idx, serializer)?;
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

/// Generate XcdrDeserialize trait implementation for tuple struct
fn generate_tuple_xcdr_deserialize_impl(
    name: &syn::Ident,
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
    crate_path: &proc_macro2::TokenStream,
    extensibility: Option<ExtensibilityKind>,
) -> proc_macro2::TokenStream {
    if fields.is_empty() {
        let deserialization_body = if matches!(
            extensibility,
            Some(ExtensibilityKind::Appendable) | Some(ExtensibilityKind::Mutable)
        ) {
            quote! {
                let (object_size, start_position) = deserializer.begin_struct()?;
                deserializer.end_struct(object_size, start_position)?;
                Ok(#name())
            }
        } else {
            quote! {
                Ok(#name())
            }
        };

        return quote! {
            impl #crate_path::serialize::xcdr::XcdrDeserialize for #name {
                fn deserialize_xcdr(deserializer: &mut #crate_path::serialize::xcdr::XcdrDeserializer) -> #crate_path::serialize::xcdr::XcdrResult<Self> {
                    #deserialization_body
                }
            }
        };
    }

    let field_deserializations: Vec<_> = fields
        .iter()
        .enumerate()
        .map(|(idx, field)| {
            let field_var = quote::format_ident!("field_{}", idx);
            let field_type = &field.ty;
            // For primitive Vec types, use specialized deserializer methods (no DHEADER)
            let method = get_serialization_method(&field.ty);
            if let Some(deser_method) = primitive_vec_deserialize_method(method) {
                let method_ident = syn::Ident::new(deser_method, proc_macro2::Span::call_site());
                quote! {
                    let #field_var = deserializer.#method_ident()?;
                }
            } else {
                quote! {
                    let #field_var = <#field_type as #crate_path::serialize::xcdr::XcdrDeserialize>::deserialize_xcdr(deserializer)?;
                }
            }
        })
        .collect();

    let field_vars: Vec<_> =
        (0..fields.len()).map(|idx| quote::format_ident!("field_{}", idx)).collect();

    let deserialization_body = if matches!(
        extensibility,
        Some(ExtensibilityKind::Appendable) | Some(ExtensibilityKind::Mutable)
    ) {
        quote! {
            let (object_size, start_position) = deserializer.begin_struct()?;
            #(#field_deserializations)*
            deserializer.end_struct(object_size, start_position)?;
            Ok(#name(#(#field_vars),*))
        }
    } else {
        quote! {
            #(#field_deserializations)*
            Ok(#name(#(#field_vars),*))
        }
    };

    quote! {
        impl #crate_path::serialize::xcdr::XcdrDeserialize for #name {
            fn deserialize_xcdr(deserializer: &mut #crate_path::serialize::xcdr::XcdrDeserializer) -> #crate_path::serialize::xcdr::XcdrResult<Self> {
                #deserialization_body
            }
        }
    }
}
