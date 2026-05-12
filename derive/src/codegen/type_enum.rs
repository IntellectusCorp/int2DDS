use quote::quote;
use syn::punctuated::Punctuated;
use syn::token::Comma;
use syn::{DeriveInput, Variant};

use crate::codegen::type_config::ExtensibilityKind;
use crate::codegen::DdsTypeConfig;
use crate::codegen::{
    generate_additional_derives, generate_enum_cdr_deserialize_impl,
    generate_enum_cdr_serialize_impl, generate_enum_xcdr_deserialize_impl,
    generate_enum_xcdr_serialize_impl, generate_has_type_object_enum_impl,
    generate_has_type_object_union_impl, generate_union_cdr_deserialize_impl,
    generate_union_cdr_serialize_impl, generate_union_xcdr_deserialize_impl,
    generate_union_xcdr_serialize_impl, is_c_style_enum, parse_repr_attribute,
};

/// Generate DdsType implementation for enum types (DDS enum or union)
pub fn derive_enum_impl(
    input: &DeriveInput,
    name: &syn::Ident,
    variants: &Punctuated<Variant, Comma>,
    type_config: &DdsTypeConfig,
) -> proc_macro2::TokenStream {
    let crate_path = &type_config.crate_path;
    let type_support_name = quote::format_ident!("{}TypeSupport", name);

    // Parse #[repr(...)] attribute for discriminant type
    let disc_type = parse_repr_attribute(&input.attrs);

    // Determine if this is a C-style enum (DDS enum) or has data (DDS union)
    let is_enum = is_c_style_enum(variants);

    // Generate TypeSupport struct
    let type_support_struct = quote! {
        #[derive(Default)]
        pub struct #type_support_name;
    };

    // Generate DdsType trait implementation
    let dds_type_impl = quote! {
        impl #crate_path::dcps::topic::type_support::DdsType for #name {
            type TypeSupport = #type_support_name;
            type FieldAccessor = #type_support_name;
        }
    };

    // Resolve extensibility for unions (defaults to Appendable per XTypes spec
    // when unspecified). C-style enums have no extensibility but the helper
    // still emits the hardcoded Final variant via this value.
    let union_extensibility = type_config.extensibility.unwrap_or(ExtensibilityKind::Appendable);

    // Generate TypeSupport trait implementation for enum/union
    let type_support_impl = generate_enum_type_support_impl(
        &type_support_name,
        name,
        crate_path,
        type_config.type_name.as_deref(),
        union_extensibility,
    );

    // Generate CdrSerialize/CdrDeserialize and XcdrSerialize/XcdrDeserialize
    let (cdr_serialize_impl, cdr_deserialize_impl, xcdr_serialize_impl, xcdr_deserialize_impl) =
        if is_enum {
            // C-style enum (DDS enum)
            (
                generate_enum_cdr_serialize_impl(name, variants, crate_path, disc_type),
                generate_enum_cdr_deserialize_impl(name, variants, crate_path, disc_type),
                generate_enum_xcdr_serialize_impl(name, variants, crate_path, disc_type),
                generate_enum_xcdr_deserialize_impl(name, variants, crate_path, disc_type),
            )
        } else {
            // Enum with data (DDS union)
            (
                generate_union_cdr_serialize_impl(name, variants, crate_path, disc_type),
                generate_union_cdr_deserialize_impl(name, variants, crate_path, disc_type),
                generate_union_xcdr_serialize_impl(
                    name,
                    variants,
                    crate_path,
                    disc_type,
                    union_extensibility,
                ),
                generate_union_xcdr_deserialize_impl(
                    name,
                    variants,
                    crate_path,
                    disc_type,
                    union_extensibility,
                ),
            )
        };

    // Generate additional derives (Default, Debug, Clone, PartialEq, speedy traits)
    let additional_derives = generate_additional_derives(input, name, type_config);

    // Generate HasTypeObject implementation
    let has_type_object_impl = if is_enum {
        generate_has_type_object_enum_impl(name, variants, type_config)
    } else {
        generate_has_type_object_union_impl(name, variants, type_config, disc_type)
    };

    let xcdr_members_impls = quote! {
        impl #crate_path::serialize::xcdr::XcdrSerializeMembers for #name {
            fn serialize_xcdr_members(&self, serializer: &mut #crate_path::serialize::xcdr::XcdrSerializer) -> #crate_path::serialize::xcdr::XcdrResult<()> {
                #crate_path::serialize::xcdr::XcdrSerialize::serialize_xcdr(self, serializer)
            }
        }
        impl #crate_path::serialize::xcdr::XcdrDeserializeMembers for #name {
            fn deserialize_xcdr_members(deserializer: &mut #crate_path::serialize::xcdr::XcdrDeserializer) -> #crate_path::serialize::xcdr::XcdrResult<Self> {
                #crate_path::serialize::xcdr::XcdrDeserialize::deserialize_xcdr(deserializer)
            }
        }
    };

    quote! {
        #type_support_struct
        #type_support_impl
        #dds_type_impl
        #cdr_serialize_impl
        #cdr_deserialize_impl
        #xcdr_serialize_impl
        #xcdr_deserialize_impl
        #xcdr_members_impls
        #has_type_object_impl
        #additional_derives
    }
}

/// Generate TypeSupport implementation for enum/union types
pub fn generate_enum_type_support_impl(
    type_support_name: &syn::Ident,
    name: &syn::Ident,
    crate_path: &proc_macro2::TokenStream,
    type_name_override: Option<&str>,
    extensibility: ExtensibilityKind,
) -> proc_macro2::TokenStream {
    let get_type_name_body = if let Some(tn) = type_name_override {
        quote! { #tn }
    } else {
        quote! { stringify!(#name) }
    };
    let extensibility_tokens =
        crate::codegen::type_config::quote_extensibility_tokens(extensibility, crate_path);
    quote! {
        impl #crate_path::dcps::topic::type_support::TypeSupport for #type_support_name {
            fn get_type_name(&self) -> &str {
                #get_type_name_body
            }

            fn type_id(&self) -> std::any::TypeId {
                std::any::TypeId::of::<#name>()
            }

            fn is_compute_key_provided(&self) -> bool {
                false
            }

            fn get_extensibility_kind(&self) -> #crate_path::serialize::xcdr::ExtensibilityKind {
                #extensibility_tokens
            }

            fn serialize(&self, data: &dyn std::any::Any, format: Option<&#crate_path::dcps::topic::type_support::SerializationFormat>) -> #crate_path::dcps::core::error::DdsResult<#crate_path::rtps::common::types::SerializedData> {
                let default_format = #crate_path::dcps::topic::type_support::SerializationFormat::Cdr;
                let format = format.unwrap_or(&default_format);
                if let Some(typed_data) = data.downcast_ref::<#name>() {
                    match format {
                        #crate_path::dcps::topic::type_support::SerializationFormat::Cdr => {
                            use #crate_path::serialize::{cdr::CdrSerializer, BufferManager};
                            use #crate_path::serialize::cdr::CdrSerialize;

                            let mut serializer = CdrSerializer::with_capacity(true, 64);
                            serializer.write_encapsulation_header()
                                .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;

                            typed_data.serialize_cdr(&mut serializer)
                                .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;

                            let bytes = serializer.into_bytes();
                            Ok(std::sync::Arc::from(bytes.into_boxed_slice()))
                        }
                        #crate_path::dcps::topic::type_support::SerializationFormat::Xcdr { extensibility_kind, .. } => {
                            use #crate_path::serialize::{xcdr::Xcdr2Serializer, BufferManager};
                            use #crate_path::serialize::cdr::XcdrSerialize;

                            let mut serializer = Xcdr2Serializer::with_capacity(true, *extensibility_kind, 64);
                            serializer.write_encapsulation_header()
                                .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;

                            typed_data.serialize_xcdr(&mut serializer)
                                .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;

                            let bytes = serializer.into_bytes();
                            Ok(std::sync::Arc::from(bytes.into_boxed_slice()))
                        }
                    }
                } else {
                    Err(#crate_path::dcps::core::error::DdsError::BadParameter)
                }
            }

            fn deserialize(&self, data: &[u8], format: Option<&#crate_path::dcps::topic::type_support::SerializationFormat>) -> #crate_path::dcps::core::error::DdsResult<Box<dyn std::any::Any>> {
                let default_format = #crate_path::dcps::topic::type_support::SerializationFormat::Cdr;
                let format = format.unwrap_or(&default_format);
                match format {
                    #crate_path::dcps::topic::type_support::SerializationFormat::Cdr => {
                        use #crate_path::serialize::cdr::CdrDeserializer;
                        use #crate_path::serialize::cdr::CdrDeserialize;

                        let mut deserializer = CdrDeserializer::new(data)
                            .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;

                        let result = #name::deserialize_cdr(&mut deserializer)
                            .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;

                        Ok(Box::new(result))
                    }
                    #crate_path::dcps::topic::type_support::SerializationFormat::Xcdr { .. } => {
                        use #crate_path::serialize::xcdr::Xcdr2Deserializer;
                        use #crate_path::serialize::cdr::XcdrDeserialize;

                        let mut deserializer = Xcdr2Deserializer::new(data)
                            .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;

                        let result = #name::deserialize_xcdr(&mut deserializer)
                            .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;

                        Ok(Box::new(result))
                    }
                }
            }

            fn serialize_key(&self, _data: &dyn std::any::Any) -> #crate_path::dcps::core::error::DdsResult<#crate_path::rtps::common::types::SerializedData> {
                Err(#crate_path::dcps::core::error::DdsError::Error("Enum/Union types do not support key serialization".to_string()))
            }

            fn deserialize_key(&self, _serialized_key: &[u8]) -> #crate_path::dcps::core::error::DdsResult<Box<dyn std::any::Any + Send + Sync>> {
                Err(#crate_path::dcps::core::error::DdsError::Error("Enum/Union types do not support key deserialization".to_string()))
            }

            fn compute_key(&self, _data: &dyn std::any::Any) -> #crate_path::common::instance_handle::InstanceHandle {
                #crate_path::common::instance_handle::InstanceHandle::default()
            }

            fn serialize_key_and_non_key(&self, data: &dyn std::any::Any) -> #crate_path::dcps::core::error::DdsResult<(#crate_path::rtps::common::types::SerializedData, #crate_path::rtps::common::types::SerializedData)> {
                let full_data = self.serialize(data, None)?;
                Ok((full_data.clone(), full_data))
            }

            // Enums (including unions) use default get_type_identifier/get_type_object (None)
            // Only C-style enums have HasTypeObject, but TypeSupport is shared
        }

        impl #crate_path::dcps::topic::type_support::FieldAccessor for #type_support_name {
            fn get_field_value(&self, _data: &dyn std::any::Any, _field_path: &str) -> #crate_path::dcps::core::error::DdsResult<#crate_path::topic::sql::ast::Parameter> {
                Err(#crate_path::dcps::core::error::DdsError::Error("Enum/Union types do not support field access".to_string()))
            }

            fn has_field(&self, _field_path: &str) -> bool {
                false
            }
        }
    }
}
