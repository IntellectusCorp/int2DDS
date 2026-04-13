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
