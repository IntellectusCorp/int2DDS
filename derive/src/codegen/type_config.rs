use quote::quote;
use syn::DeriveInput;

use crate::codegen::utils::AutoIdKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExtensibilityKind {
    Final,
    #[default]
    Appendable,
    Mutable,
}

#[derive(Debug, Clone)]
pub struct DdsTypeConfig {
    pub crate_path: proc_macro2::TokenStream,
    pub extensibility: Option<ExtensibilityKind>,
    pub type_name: Option<String>,
    pub no_default: bool,
    pub no_partialeq: bool,
    pub autoid: Option<AutoIdKind>,
    pub bitmask: bool,
    pub bit_bound: Option<u8>,
    pub bitset: bool,
    pub no_additional_derives: bool,
    pub skip_field_accessor: bool,
    /// @ignore_literal_names on enumerations
    pub ignore_literal_names: bool,
    /// Marks a newtype struct as an IDL alias. Emits `TK_ALIAS` TypeObject
    /// with the inner field's type as the base.
    pub alias: bool,
    /// @nested: type is intended for use only as a member of another aggregated type,
    /// not as a top-level topic type.
    pub nested: bool,
    /// @topic(name, platform): hints the default topic name for a type.
    pub topic_name: Option<String>,
    pub topic_platform: Option<String>,
    /// @data_representation(mask): restricts allowed wire representations.
    /// Bit 0 = XCDR1, bit 1 = XML, bit 2 = XCDR2.
    pub data_representation_mask: Option<u32>,
}

pub fn parse_dds_type_attributes(input: &DeriveInput) -> DdsTypeConfig {
    let mut crate_path: Option<String> = None;
    let mut extensibility: Option<ExtensibilityKind> = None;
    let mut type_name: Option<String> = None;
    let mut no_default = false;
    let mut no_partialeq = false;
    let mut autoid: Option<AutoIdKind> = None;
    let mut bitmask = false;
    let mut bit_bound: Option<u8> = None;
    let mut bitset = false;
    let mut no_additional_derives = false;
    let mut skip_field_accessor = false;
    let mut ignore_literal_names = false;
    let mut alias = false;
    let mut nested = false;
    let mut topic_name: Option<String> = None;
    let mut topic_platform: Option<String> = None;
    let mut data_representation_mask: Option<u32> = None;

    // Parse #[dds_type(...)] attributes
    for attr in &input.attrs {
        if attr.path().is_ident("dds_type") {
            let _ = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("crate_path") {
                    let value = meta.value()?;
                    let lit: syn::LitStr = value.parse()?;
                    crate_path = Some(lit.value());
                } else if meta.path.is_ident("extensibility") {
                    let value = meta.value()?;
                    let lit: syn::LitStr = value.parse()?;
                    extensibility = Some(match lit.value().to_lowercase().as_str() {
                        "final" => ExtensibilityKind::Final,
                        "appendable" => ExtensibilityKind::Appendable,
                        "mutable" => ExtensibilityKind::Mutable,
                        _ => {
                            return Err(meta.error(format!(
                                "Unknown extensibility kind: '{}'. Valid values are: 'final', 'appendable', 'mutable' (case-insensitive)",
                                lit.value()
                            )));
                        }
                    });
                } else if meta.path.is_ident("final") {
                    extensibility = Some(ExtensibilityKind::Final);
                } else if meta.path.is_ident("appendable") {
                    extensibility = Some(ExtensibilityKind::Appendable);
                } else if meta.path.is_ident("mutable") {
                    extensibility = Some(ExtensibilityKind::Mutable);
                } else if meta.path.is_ident("type_name") {
                    let value = meta.value()?;
                    let lit: syn::LitStr = value.parse()?;
                    type_name = Some(lit.value());
                } else if meta.path.is_ident("no_default") {
                    no_default = true;
                } else if meta.path.is_ident("no_partialeq") {
                    no_partialeq = true;
                } else if meta.path.is_ident("autoid") {
                    let value = meta.value()?;
                    let lit: syn::LitStr = value.parse()?;
                    autoid = Some(match lit.value().to_lowercase().as_str() {
                        "sequential" => AutoIdKind::Sequential,
                        "hash" => AutoIdKind::Hash,
                        _ => {
                            return Err(meta.error(format!(
                                "Unknown autoid kind: '{}'. Valid values are: 'Sequential', 'Hash'",
                                lit.value()
                            )));
                        }
                    });
                } else if meta.path.is_ident("bitmask") {
                    bitmask = true;
                } else if meta.path.is_ident("bit_bound") {
                    let value = meta.value()?;
                    let lit: syn::LitInt = value.parse()?;
                    bit_bound = Some(lit.base10_parse::<u8>()?);
                } else if meta.path.is_ident("bitset") {
                    bitset = true;
                } else if meta.path.is_ident("no_additional_derives") {
                    no_additional_derives = true;
                } else if meta.path.is_ident("skip_field_accessor") {
                    skip_field_accessor = true;
                } else if meta.path.is_ident("ignore_literal_names") {
                    ignore_literal_names = true;
                } else if meta.path.is_ident("alias") {
                    alias = true;
                } else if meta.path.is_ident("nested") {
                    nested = true;
                } else if meta.path.is_ident("topic") {
                    meta.parse_nested_meta(|inner| {
                        if inner.path.is_ident("name") {
                            let value = inner.value()?;
                            let lit: syn::LitStr = value.parse()?;
                            topic_name = Some(lit.value());
                        } else if inner.path.is_ident("platform") {
                            let value = inner.value()?;
                            let lit: syn::LitStr = value.parse()?;
                            topic_platform = Some(lit.value());
                        }
                        Ok(())
                    })?;
                } else if meta.path.is_ident("data_representation") {
                    let mut mask: u32 = 0;
                    meta.parse_nested_meta(|inner| {
                        let ident = inner.path.get_ident().map(ToString::to_string).unwrap_or_default();
                        match ident.to_ascii_uppercase().as_str() {
                            "XCDR" | "XCDR1" => mask |= 1 << 0,
                            "XML" => mask |= 1 << 1,
                            "XCDR2" => mask |= 1 << 2,
                            _ => {
                                return Err(inner.error(format!(
                                    "unknown data_representation value '{}', expected XCDR | XML | XCDR2",
                                    ident
                                )));
                            }
                        }
                        Ok(())
                    })?;
                    data_representation_mask = Some(mask);
                } else if meta.path.is_ident("verbatim") {
                    meta.parse_nested_meta(|_| Ok(()))?;
                }
                Ok(())
            });
        }
    }

    // Process crate_path
    let crate_path_tokens = if let Some(path) = crate_path {
        if path == "crate" {
            quote! { crate }
        } else {
            let path_tokens: proc_macro2::TokenStream = path.parse().unwrap();
            quote! { #path_tokens }
        }
    } else {
        quote! { int2dds }
    };

    DdsTypeConfig {
        crate_path: crate_path_tokens,
        extensibility,
        type_name,
        no_default,
        no_partialeq,
        autoid,
        bitmask,
        bit_bound,
        bitset,
        no_additional_derives,
        skip_field_accessor,
        ignore_literal_names,
        alias,
        nested,
        topic_name,
        topic_platform,
        data_representation_mask,
    }
}

/// Generate extensibility kind tokens from ExtensibilityKind enum
pub fn quote_extensibility_tokens(
    ext_kind: ExtensibilityKind,
    crate_path: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    match ext_kind {
        ExtensibilityKind::Final => {
            quote! { #crate_path::serialize::xcdr::ExtensibilityKind::Final }
        }
        ExtensibilityKind::Appendable => {
            quote! { #crate_path::serialize::xcdr::ExtensibilityKind::Appendable }
        }
        ExtensibilityKind::Mutable => {
            quote! { #crate_path::serialize::xcdr::ExtensibilityKind::Mutable }
        }
    }
}
