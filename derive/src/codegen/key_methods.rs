use crate::codegen::utils::is_unbounded_string;
use quote::quote;

pub struct KeyFieldInfo {
    pub ident: syn::Ident,
    pub field_type: syn::Type,
    pub member_id: Option<u32>, // @id attribute value, None if not specified
}

pub fn generate_key_methods(
    key_field_info: Option<&KeyFieldInfo>,
    full_type: &proc_macro2::TokenStream,
    _field_deserialization: &proc_macro2::TokenStream,
    crate_path: &proc_macro2::TokenStream,
) -> (proc_macro2::TokenStream, proc_macro2::TokenStream, proc_macro2::TokenStream) {
    if let Some(key_info) = key_field_info {
        generate_key_impls_from_fields(std::slice::from_ref(key_info), full_type, crate_path)
    } else {
        generate_no_key_impls(crate_path)
    }
}

pub struct MultiKeyFieldInfo {
    pub fields: Vec<KeyFieldInfo>,
}

pub fn generate_multi_key_methods(
    multi_key_info: Option<&MultiKeyFieldInfo>,
    full_type: &proc_macro2::TokenStream,
    crate_path: &proc_macro2::TokenStream,
) -> (proc_macro2::TokenStream, proc_macro2::TokenStream, proc_macro2::TokenStream) {
    if let Some(multi_key) = multi_key_info {
        generate_key_impls_from_fields(&multi_key.fields, full_type, crate_path)
    } else {
        generate_no_key_impls(crate_path)
    }
}

fn generate_no_key_impls(
    crate_path: &proc_macro2::TokenStream,
) -> (proc_macro2::TokenStream, proc_macro2::TokenStream, proc_macro2::TokenStream) {
    let serialize_key_impl = quote! {
        fn serialize_key(&self, _data: &dyn std::any::Any) -> #crate_path::dcps::core::error::DdsResult<#crate_path::rtps::common::types::SerializedData> {
            Err(#crate_path::dcps::core::error::DdsError::Error("No key field defined for this type".to_string()))
        }
    };

    let deserialize_key_impl = quote! {
        fn deserialize_key(&self, _serialized_key: &[u8]) -> #crate_path::dcps::core::error::DdsResult<Box<dyn std::any::Any + Send + Sync>> {
            Err(#crate_path::dcps::core::error::DdsError::Error("No key field defined for this type".to_string()))
        }
    };

    let compute_key_impl = quote! {
        fn compute_key(&self, _data: &dyn std::any::Any) -> #crate_path::common::instance_handle::InstanceHandle {
            #crate_path::common::instance_handle::InstanceHandle::NIL
        }
    };

    (serialize_key_impl, deserialize_key_impl, compute_key_impl)
}

fn generate_key_impls_from_fields(
    fields: &[KeyFieldInfo],
    full_type: &proc_macro2::TokenStream,
    crate_path: &proc_macro2::TokenStream,
) -> (proc_macro2::TokenStream, proc_macro2::TokenStream, proc_macro2::TokenStream) {
    assert!(!fields.is_empty());
    let mut sorted_fields: Vec<_> = fields.iter().collect();

    sorted_fields.sort_by_key(|f| f.member_id.unwrap_or(u32::MAX));

    let key_fields: Vec<_> = sorted_fields.iter().map(|f| &f.ident).collect();
    let key_types: Vec<_> = sorted_fields.iter().map(|f| &f.field_type).collect();
    let is_single_unbounded_string =
        sorted_fields.len() == 1 && is_unbounded_string(&sorted_fields[0].field_type);

    let bytes_post_process = if is_single_unbounded_string {
        quote! {
            let mut bytes = serializer.into_bytes();
            while bytes.len() > 5 && bytes[bytes.len() - 1] == 0 {
                let prev_byte = bytes[bytes.len() - 2];
                if prev_byte == 0 {
                    bytes.pop();
                } else {
                    break;
                }
            }
            Ok(std::sync::Arc::from(bytes))
        }
    } else {
        quote! {
            let bytes = serializer.into_bytes();
            Ok(std::sync::Arc::from(bytes))
        }
    };

    let serialize_key_impl = quote! {
        fn serialize_key(&self, data: &dyn std::any::Any) -> #crate_path::dcps::core::error::DdsResult<#crate_path::rtps::common::types::SerializedData> {
            use #crate_path::serialize::cdr::{CdrSerialize, CdrSerializer};
            use #crate_path::serialize::BufferManager;

            if let Some(typed_data) = data.downcast_ref::<#full_type>() {
                // Match regular serialize: little-endian CDR with encapsulation header
                let mut serializer = CdrSerializer::with_capacity(true, 64);
                serializer.write_encapsulation_header()
                    .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
                #(
                    #crate_path::serialize::cdr::CdrSerialize::serialize_cdr(&typed_data.#key_fields, &mut serializer)
                        .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(
                            format!("Failed to serialize field {}: {}", stringify!(#key_fields), e)
                        ))?;
                )*

                #bytes_post_process
            } else {
                Err(#crate_path::dcps::core::error::DdsError::Error(format!(
                    "Type mismatch: expected {}, but received incompatible type for key serialization",
                    std::any::type_name::<#full_type>()
                )))
            }
        }
    };

    let deserialize_key_impl = quote! {
        fn deserialize_key(&self, serialized_key: &[u8]) -> #crate_path::dcps::core::error::DdsResult<Box<dyn std::any::Any + Send + Sync>> {
            use #crate_path::serialize::cdr::{CdrDeserialize, CdrDeserializer};

            let mut deserializer = CdrDeserializer::new(serialized_key)
                .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
            let mut key_holder = <#full_type as Default>::default();

            #(
                key_holder.#key_fields = <#key_types as #crate_path::serialize::cdr::CdrDeserialize>::deserialize_cdr(&mut deserializer)
                    .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(
                        format!("Failed to deserialize field {}: {}", stringify!(#key_fields), e)
                    ))?;
            )*

            Ok(Box::new(key_holder))
        }

    };

    let compute_logic = if is_single_unbounded_string {
        quote! {
            match self.serialize_key(data) {
                Ok(cdr_data) => {
                    let hash = ::md5::compute(&cdr_data);
                    #crate_path::common::instance_handle::InstanceHandle::new(hash.0)
                }

                Err(e) => {
                    log::error!("Warning: Key serialization failed for type {}: {:?}. Using NIL instance handle.",
                        std::any::type_name::<#full_type>(), e);
                    #crate_path::common::instance_handle::InstanceHandle::NIL
                }
            }
        }
    } else {
        quote! {
            match self.serialize_key(data) {
                Ok(cdr_data) => {
                    let mut result = [0u8; 16];

                    if cdr_data.len() <= 16 {
                        result[..cdr_data.len()].copy_from_slice(&cdr_data);
                    } else {
                        let hash = ::md5::compute(&cdr_data);
                        result = hash.0;
                    }

                    #crate_path::common::instance_handle::InstanceHandle::new(result)
                }
                Err(e) => {
                    log::error!("Warning: Key serialization failed for type {}: {:?}. Using NIL instance handle.",
                        std::any::type_name::<#full_type>(), e);
                    #crate_path::common::instance_handle::InstanceHandle::NIL
                }
            }
        }
    };

    let compute_key_impl = quote! {
        fn compute_key(&self, data: &dyn std::any::Any) -> #crate_path::common::instance_handle::InstanceHandle {
            if data.downcast_ref::<#full_type>().is_some() {
                #compute_logic
            } else {
                #crate_path::common::instance_handle::InstanceHandle::NIL
            }
        }
    };
    (serialize_key_impl, deserialize_key_impl, compute_key_impl)
}
