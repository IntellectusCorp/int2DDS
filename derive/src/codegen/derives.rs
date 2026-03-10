use quote::quote;
use syn::{Data, DeriveInput, Fields};

use super::type_config::DdsTypeConfig;

/// Generate speedy Writable write_to field statements
pub fn generate_speedy_write_fields(input: &DeriveInput) -> Vec<proc_macro2::TokenStream> {
    if let Data::Struct(data) = &input.data {
        if let Fields::Named(fields) = &data.fields {
            return fields
                .named
                .iter()
                .map(|field| {
                    let name = field.ident.as_ref().unwrap();
                    quote! { writer.write_value(&self.#name)?; }
                })
                .collect();
        }
    }
    vec![]
}

/// Generate speedy Readable read_from field statements
pub fn generate_speedy_read_fields(input: &DeriveInput) -> Vec<proc_macro2::TokenStream> {
    if let Data::Struct(data) = &input.data {
        if let Fields::Named(fields) = &data.fields {
            return fields
                .named
                .iter()
                .map(|field| {
                    let name = field.ident.as_ref().unwrap();
                    quote! { #name: reader.read_value()? }
                })
                .collect();
        }
    }
    vec![]
}

pub fn generate_default_fields(input: &DeriveInput) -> Vec<proc_macro2::TokenStream> {
    if let Data::Struct(data) = &input.data {
        if let Fields::Named(fields) = &data.fields {
            return fields
                .named
                .iter()
                .map(|field| {
                    let name = field.ident.as_ref().unwrap();
                    quote! { #name: Default::default() }
                })
                .collect();
        }
    }
    vec![]
}

pub fn generate_debug_fields(input: &DeriveInput) -> Vec<proc_macro2::TokenStream> {
    if let Data::Struct(data) = &input.data {
        if let Fields::Named(fields) = &data.fields {
            return fields
                .named
                .iter()
                .map(|field| {
                    let name = field.ident.as_ref().unwrap();
                    let name_str = name.to_string();
                    quote! { .field(#name_str, &self.#name) }
                })
                .collect();
        }
    }
    vec![]
}

pub fn generate_clone_fields(input: &DeriveInput) -> Vec<proc_macro2::TokenStream> {
    if let Data::Struct(data) = &input.data {
        if let Fields::Named(fields) = &data.fields {
            return fields
                .named
                .iter()
                .map(|field| {
                    let name = field.ident.as_ref().unwrap();
                    quote! { #name: self.#name.clone() }
                })
                .collect();
        }
    }
    vec![]
}

pub fn generate_eq_fields(input: &DeriveInput) -> Vec<proc_macro2::TokenStream> {
    if let Data::Struct(data) = &input.data {
        if let Fields::Named(fields) = &data.fields {
            return fields
                .named
                .iter()
                .map(|field| {
                    let name = field.ident.as_ref().unwrap();
                    quote! { self.#name == other.#name }
                })
                .collect();
        }
    }
    vec![]
}

pub fn generate_additional_derives(
    input: &DeriveInput,
    name: &syn::Ident,
    config: &DdsTypeConfig,
) -> proc_macro2::TokenStream {
    // Check if this is an enum
    if let Data::Enum(_) = &input.data {
        return generate_enum_additional_derives(input, name, config);
    }

    // Check if this is a tuple struct
    if let Data::Struct(data) = &input.data {
        if let Fields::Unnamed(_) = &data.fields {
            return generate_tuple_struct_additional_derives(input, name, config);
        }
    }

    if config.no_additional_derives {
        return quote! {};
    }

    let debug_fields = generate_debug_fields(input);
    let clone_fields = generate_clone_fields(input);
    let eq_fields = generate_eq_fields(input);
    let speedy_write_fields = generate_speedy_write_fields(input);
    let speedy_read_fields = generate_speedy_read_fields(input);

    // Generate Default impl only if no_default is false
    let default_impl = if config.no_default {
        quote! {}
    } else {
        let default_fields = generate_default_fields(input);
        quote! {
            #[automatically_derived]
            impl Default for #name {
                fn default() -> Self {
                    Self {
                        #(#default_fields,)*
                    }
                }
            }
        }
    };

    // Generate PartialEq impl only if no_partialeq is false
    let partialeq_impl = if config.no_partialeq {
        quote! {}
    } else {
        quote! {
            #[automatically_derived]
            impl PartialEq for #name {
                fn eq(&self, other: &Self) -> bool {
                    true #(&& #eq_fields)*
                }
            }
        }
    };

    quote! {
        #default_impl

        #[automatically_derived]
        impl std::fmt::Debug for #name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct(stringify!(#name))
                    #(#debug_fields)*
                    .finish()
            }
        }

        #[automatically_derived]
        impl Clone for #name {
            fn clone(&self) -> Self {
                Self {
                    #(#clone_fields,)*
                }
            }
        }

        #partialeq_impl

        #[automatically_derived]
        impl<C: speedy::Context> speedy::Writable<C> for #name {
            fn write_to<T: ?Sized + speedy::Writer<C>>(&self, writer: &mut T) -> Result<(), C::Error> {
                #(#speedy_write_fields)*
                Ok(())
            }
        }

        #[automatically_derived]
        impl<'a, C: speedy::Context> speedy::Readable<'a, C> for #name {
            fn read_from<R: speedy::Reader<'a, C>>(reader: &mut R) -> Result<Self, C::Error> {
                Ok(Self {
                    #(#speedy_read_fields,)*
                })
            }
        }
    }
}

/// Generate additional derives for tuple struct types
fn generate_tuple_struct_additional_derives(
    input: &DeriveInput,
    name: &syn::Ident,
    config: &DdsTypeConfig,
) -> proc_macro2::TokenStream {
    if let Data::Struct(data) = &input.data {
        if let Fields::Unnamed(fields) = &data.fields {
            let field_count = fields.unnamed.len();

            // Generate Debug impl
            let debug_fields: Vec<_> = (0..field_count)
                .map(|idx| {
                    let idx = syn::Index::from(idx);
                    quote! { .field(&self.#idx) }
                })
                .collect();

            // Generate Clone impl
            let clone_fields: Vec<_> = (0..field_count)
                .map(|idx| {
                    let idx = syn::Index::from(idx);
                    quote! { self.#idx.clone() }
                })
                .collect();

            // Generate PartialEq fields
            let eq_fields: Vec<_> = (0..field_count)
                .map(|idx| {
                    let idx = syn::Index::from(idx);
                    quote! { self.#idx == other.#idx }
                })
                .collect();

            // Generate speedy Writable impl
            let speedy_write_fields: Vec<_> = (0..field_count)
                .map(|idx| {
                    let idx = syn::Index::from(idx);
                    quote! { writer.write_value(&self.#idx)?; }
                })
                .collect();

            // Generate speedy Readable impl
            let speedy_read_fields: Vec<_> =
                (0..field_count).map(|_| quote! { reader.read_value()? }).collect();

            // Generate Default impl only if no_default is false
            let default_impl = if config.no_default {
                quote! {}
            } else {
                let default_fields: Vec<_> =
                    (0..field_count).map(|_| quote! { Default::default() }).collect();
                quote! {
                    #[automatically_derived]
                    impl Default for #name {
                        fn default() -> Self {
                            Self(#(#default_fields),*)
                        }
                    }
                }
            };

            // Generate PartialEq impl only if no_partialeq is false
            let partialeq_impl = if config.no_partialeq {
                quote! {}
            } else {
                quote! {
                    #[automatically_derived]
                    impl PartialEq for #name {
                        fn eq(&self, other: &Self) -> bool {
                            true #(&& #eq_fields)*
                        }
                    }
                }
            };

            return quote! {
                #default_impl

                #[automatically_derived]
                impl std::fmt::Debug for #name {
                    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        f.debug_tuple(stringify!(#name))
                            #(#debug_fields)*
                            .finish()
                    }
                }

                #[automatically_derived]
                impl Clone for #name {
                    fn clone(&self) -> Self {
                        Self(#(#clone_fields),*)
                    }
                }

                #partialeq_impl

                #[automatically_derived]
                impl<C: speedy::Context> speedy::Writable<C> for #name {
                    fn write_to<T: ?Sized + speedy::Writer<C>>(&self, writer: &mut T) -> Result<(), C::Error> {
                        #(#speedy_write_fields)*
                        Ok(())
                    }
                }

                #[automatically_derived]
                impl<'a, C: speedy::Context> speedy::Readable<'a, C> for #name {
                    fn read_from<R: speedy::Reader<'a, C>>(reader: &mut R) -> Result<Self, C::Error> {
                        Ok(Self(#(#speedy_read_fields),*))
                    }
                }
            };
        }
    }
    quote! {}
}

/// Generate additional derives for enum types
fn generate_enum_additional_derives(
    input: &DeriveInput,
    name: &syn::Ident,
    config: &DdsTypeConfig,
) -> proc_macro2::TokenStream {
    if config.no_additional_derives {
        return quote! {};
    }

    if let Data::Enum(data) = &input.data {
        let variants = &data.variants;

        // Generate Default impl only if no_default is false
        let default_impl = if config.no_default {
            quote! {}
        } else {
            // Find variant with #[default] attribute, or fall back to first variant
            let default_variant = variants
                .iter()
                .find(|v| v.attrs.iter().any(|attr| attr.path().is_ident("default")))
                .or_else(|| variants.first());

            if let Some(variant) = default_variant {
                let variant_name = &variant.ident;
                // Check if variant has data
                match &variant.fields {
                    Fields::Unit => quote! {
                        #[automatically_derived]
                        impl Default for #name {
                            fn default() -> Self {
                                Self::#variant_name
                            }
                        }
                    },
                    Fields::Unnamed(fields) if fields.unnamed.len() == 1 => quote! {
                        #[automatically_derived]
                        impl Default for #name {
                            fn default() -> Self {
                                Self::#variant_name(Default::default())
                            }
                        }
                    },
                    _ => quote! {}, // Cannot generate default for complex variants
                }
            } else {
                quote! {}
            }
        };

        // Generate Debug impl for enum
        let debug_arms: Vec<_> = variants
            .iter()
            .map(|variant| {
                let variant_name = &variant.ident;
                let variant_str = variant_name.to_string();
                match &variant.fields {
                    Fields::Unit => quote! {
                        Self::#variant_name => write!(f, "{}", #variant_str),
                    },
                    Fields::Unnamed(fields) if fields.unnamed.len() == 1 => quote! {
                        Self::#variant_name(value) => write!(f, "{}({:?})", #variant_str, value),
                    },
                    _ => quote! {
                        Self::#variant_name { .. } => write!(f, "{} {{ ... }}", #variant_str),
                    },
                }
            })
            .collect();

        // Generate Clone impl for enum
        let clone_arms: Vec<_> = variants
            .iter()
            .map(|variant| {
                let variant_name = &variant.ident;
                match &variant.fields {
                    Fields::Unit => quote! {
                        Self::#variant_name => Self::#variant_name,
                    },
                    Fields::Unnamed(fields) if fields.unnamed.len() == 1 => quote! {
                        Self::#variant_name(value) => Self::#variant_name(value.clone()),
                    },
                    _ => quote! {
                        Self::#variant_name { .. } => unimplemented!("Clone not supported for this variant"),
                    },
                }
            })
            .collect();

        // Generate PartialEq impl for enum
        let eq_arms: Vec<_> = variants
            .iter()
            .map(|variant| {
                let variant_name = &variant.ident;
                match &variant.fields {
                    Fields::Unit => quote! {
                        (Self::#variant_name, Self::#variant_name) => true,
                    },
                    Fields::Unnamed(fields) if fields.unnamed.len() == 1 => quote! {
                        (Self::#variant_name(a), Self::#variant_name(b)) => a == b,
                    },
                    _ => quote! {
                        (Self::#variant_name { .. }, Self::#variant_name { .. }) => false,
                    },
                }
            })
            .collect();

        // Generate speedy Writable impl for enum
        let speedy_write_arms: Vec<_> = variants
            .iter()
            .enumerate()
            .map(|(idx, variant)| {
                let variant_name = &variant.ident;
                let idx = idx as u32;
                match &variant.fields {
                    Fields::Unit => quote! {
                        Self::#variant_name => {
                            writer.write_value(&#idx)?;
                        }
                    },
                    Fields::Unnamed(fields) if fields.unnamed.len() == 1 => quote! {
                        Self::#variant_name(value) => {
                            writer.write_value(&#idx)?;
                            writer.write_value(value)?;
                        }
                    },
                    _ => quote! {
                        Self::#variant_name { .. } => unimplemented!("Writable not supported for this variant"),
                    },
                }
            })
            .collect();

        // Generate speedy Readable impl for enum
        let speedy_read_arms: Vec<_> = variants
            .iter()
            .enumerate()
            .map(|(idx, variant)| {
                let variant_name = &variant.ident;
                let idx = idx as u32;
                match &variant.fields {
                    Fields::Unit => quote! {
                        #idx => Self::#variant_name,
                    },
                    Fields::Unnamed(fields) if fields.unnamed.len() == 1 => quote! {
                        #idx => Self::#variant_name(reader.read_value()?),
                    },
                    _ => quote! {
                        #idx => unimplemented!("Readable not supported for this variant"),
                    },
                }
            })
            .collect();

        // Generate PartialEq impl only if no_partialeq is false
        let partialeq_impl = if config.no_partialeq {
            quote! {}
        } else {
            quote! {
                #[automatically_derived]
                impl PartialEq for #name {
                    fn eq(&self, other: &Self) -> bool {
                        match (self, other) {
                            #(#eq_arms)*
                            _ => false,
                        }
                    }
                }
            }
        };

        quote! {
            #default_impl

            #[automatically_derived]
            impl std::fmt::Debug for #name {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    match self {
                        #(#debug_arms)*
                    }
                }
            }

            #[automatically_derived]
            impl Clone for #name {
                fn clone(&self) -> Self {
                    match self {
                        #(#clone_arms)*
                    }
                }
            }

            #partialeq_impl

            #[automatically_derived]
            impl<C: speedy::Context> speedy::Writable<C> for #name {
                fn write_to<T: ?Sized + speedy::Writer<C>>(&self, writer: &mut T) -> Result<(), C::Error> {
                    match self {
                        #(#speedy_write_arms)*
                    }
                    Ok(())
                }
            }

            #[automatically_derived]
            impl<'a, C: speedy::Context> speedy::Readable<'a, C> for #name {
                fn read_from<R: speedy::Reader<'a, C>>(reader: &mut R) -> Result<Self, C::Error> {
                    let discriminant: u32 = reader.read_value()?;
                    Ok(match discriminant {
                        #(#speedy_read_arms)*
                        _ => return Err(speedy::Error::custom("Invalid enum discriminant").into()),
                    })
                }
            }
        }
    } else {
        quote! {}
    }
}
