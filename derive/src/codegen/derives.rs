use quote::quote;
use syn::{Data, DeriveInput, Fields};

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
) -> proc_macro2::TokenStream {
    let default_fields = generate_default_fields(input);
    let debug_fields = generate_debug_fields(input);
    let clone_fields = generate_clone_fields(input);
    let eq_fields = generate_eq_fields(input);

    quote! {
        #[automatically_derived]
        impl Default for #name {
            fn default() -> Self {
                Self {
                    #(#default_fields,)*
                }
            }
        }

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

        #[automatically_derived]
        impl PartialEq for #name {
            fn eq(&self, other: &Self) -> bool {
                true #(&& #eq_fields)*
            }
        }
    }
}
