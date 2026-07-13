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
            // Drop the 4-byte encapsulation prefix; CDR alignment was relative to it.
            let mut bytes = serializer.into_bytes();
            bytes.drain(..4);
            while bytes.len() > 1 && bytes[bytes.len() - 1] == 0 {
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
            // Drop the 4-byte encapsulation prefix; CDR alignment was relative to it.
            let mut bytes = serializer.into_bytes();
            bytes.drain(..4);
            Ok(std::sync::Arc::from(bytes))
        }
    };

    let serialize_key_impl = quote! {
        fn serialize_key(&self, data: &dyn std::any::Any) -> #crate_path::dcps::core::error::DdsResult<#crate_path::rtps::common::types::SerializedData> {
            use #crate_path::serialize::cdr::{CdrSerialize, CdrSerializer};
            use #crate_path::serialize::BufferManager;

            if let Some(typed_data) = data.downcast_ref::<#full_type>() {
                // Per RTPS KeyHash spec: big-endian CDR of key fields, no encapsulation header.
                // We still write the header so the serializer's alignment math (which assumes
                // a 4-byte encapsulation prefix) yields correct CDR alignment, and strip it after.
                let mut serializer = CdrSerializer::with_capacity(false, 64);
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

        fn serialize_key_payload(
            &self,
            data: &dyn std::any::Any,
            format: &#crate_path::dcps::topic::type_support::SerializationFormat,
        ) -> #crate_path::dcps::core::error::DdsResult<#crate_path::rtps::common::types::SerializedData> {
            use #crate_path::dcps::topic::type_support::{SerializationFormat, TypeSupport};
            use #crate_path::serialize::xcdr::{ExtensibilityKind, Xcdr2Serializer, XcdrSerialize};
            use #crate_path::serialize::BufferManager;

            match format {
                // XCDR1 CDR_BE: header + headerless big-endian key (KeyHash format).
                SerializationFormat::Cdr => {
                    let key = self.serialize_key(data)?;
                    let mut payload = Vec::with_capacity(key.len() + 4);
                    payload.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
                    payload.extend_from_slice(&key);
                    Ok(std::sync::Arc::from(payload))
                }
                // XCDR2: PLAIN_CDR2 (Final) or DELIMITED_CDR2 (Appendable/Mutable) key members.
                SerializationFormat::Xcdr { extensibility_kind, .. } => {
                    let typed_data = data.downcast_ref::<#full_type>().ok_or_else(|| {
                        #crate_path::dcps::core::error::DdsError::Error(
                            "Type mismatch for key payload serialization".to_string(),
                        )
                    })?;
                    let mut serializer = Xcdr2Serializer::with_capacity(true, *extensibility_kind, 64);
                    serializer.write_encapsulation_header()
                        .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
                    let dheader_pos = if matches!(extensibility_kind, ExtensibilityKind::Final) {
                        None
                    } else {
                        Some(serializer.begin_struct()
                            .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?)
                    };
                    #(
                        XcdrSerialize::serialize_xcdr(&typed_data.#key_fields, &mut serializer)
                            .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(
                                format!("Failed to serialize field {}: {}", stringify!(#key_fields), e)
                            ))?;
                    )*
                    if let Some(size_pos) = dheader_pos {
                        serializer.end_struct(size_pos)
                            .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
                    }
                    Ok(std::sync::Arc::from(serializer.into_bytes().into_boxed_slice()))
                }
            }
        }
    };

    let deserialize_key_impl = quote! {
        fn deserialize_key(&self, serialized_key: &[u8]) -> #crate_path::dcps::core::error::DdsResult<Box<dyn std::any::Any + Send + Sync>> {
            use #crate_path::serialize::cdr::{CdrDeserialize, CdrDeserializer};

            // Key bytes are big-endian CDR with no encapsulation header (RTPS KeyHash format).
            let mut deserializer = CdrDeserializer::new_without_header(serialized_key, false);
            let mut key_holder = <#full_type as Default>::default();

            #(
                key_holder.#key_fields = <#key_types as #crate_path::serialize::cdr::CdrDeserialize>::deserialize_cdr(&mut deserializer)
                    .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(
                        format!("Failed to deserialize field {}: {}", stringify!(#key_fields), e)
                    ))?;
            )*

            Ok(Box::new(key_holder))
        }

        fn deserialize_key_payload(&self, payload: &[u8]) -> #crate_path::dcps::core::error::DdsResult<Box<dyn std::any::Any + Send + Sync>> {
            use #crate_path::serialize::cdr::CdrDeserializer;
            use #crate_path::serialize::xcdr::{XcdrDeserialize, Xcdr2Deserializer};

            // Wire serializedKey: CDR with a 4-byte encapsulation header. Follow the
            // sender's representation from the encapsulation id (endianness + XCDR1/XCDR2).
            if payload.len() < 4 {
                return Err(#crate_path::dcps::core::error::DdsError::Error(
                    "serializedKey payload shorter than its encapsulation header".to_string(),
                ));
            }
            let encap_id = u16::from_be_bytes([payload[0], payload[1]]);
            let mut key_holder = <#full_type as Default>::default();

            match encap_id {
                // XCDR2 PLAIN_CDR2 (Final): key members directly, no DHEADER.
                0x0006 | 0x0007 => {
                    let mut deserializer = Xcdr2Deserializer::new(payload)
                        .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
                    #(
                        key_holder.#key_fields = <#key_types as XcdrDeserialize>::deserialize_xcdr(&mut deserializer)
                            .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(
                                format!("Failed to deserialize field {}: {}", stringify!(#key_fields), e)
                            ))?;
                    )*
                }
                // XCDR2 DELIMITED_CDR2 (Appendable): a single struct DHEADER wraps the key members.
                0x0008 | 0x0009 => {
                    let mut deserializer = Xcdr2Deserializer::new(payload)
                        .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
                    let (object_size, start_position) = deserializer.begin_struct()
                        .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
                    #(
                        key_holder.#key_fields = <#key_types as XcdrDeserialize>::deserialize_xcdr(&mut deserializer)
                            .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(
                                format!("Failed to deserialize field {}: {}", stringify!(#key_fields), e)
                            ))?;
                    )*
                    deserializer.end_struct(object_size, start_position)
                        .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
                }
                // XCDR1 CDR_BE/CDR_LE/PL_CDR: read endianness from the header.
                _ => {
                    let mut deserializer = CdrDeserializer::new(payload)
                        .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(e.to_string()))?;
                    #(
                        key_holder.#key_fields = <#key_types as #crate_path::serialize::cdr::CdrDeserialize>::deserialize_cdr(&mut deserializer)
                            .map_err(|e| #crate_path::dcps::core::error::DdsError::Error(
                                format!("Failed to deserialize field {}: {}", stringify!(#key_fields), e)
                            ))?;
                    )*
                }
            }

            Ok(Box::new(key_holder))
        }

    };

    let compute_logic = if is_single_unbounded_string {
        quote! {
            match self.serialize_key(data) {
                Ok(cdr_data) => {
                    #crate_path::common::instance_handle::InstanceHandle::from_key_cdr_hashed(&cdr_data)
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
                    #crate_path::common::instance_handle::InstanceHandle::from_key_cdr(&cdr_data)
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
