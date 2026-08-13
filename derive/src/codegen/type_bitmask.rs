use quote::quote;
use syn::DeriveInput;

use crate::codegen::type_config::DdsTypeConfig;

/// Generate DdsType implementation for bitmask enum types.
/// Bitmask enums represent bit flags with @position annotations.
///
/// Wire type is determined by bit_bound:
/// - bit_bound <= 8  → u8
/// - bit_bound <= 16 → u16
/// - bit_bound <= 32 → u32
/// - bit_bound <= 64 → u64
pub fn derive_bitmask_impl(
    input: &DeriveInput,
    name: &syn::Ident,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::token::Comma>,
    type_config: &DdsTypeConfig,
) -> proc_macro2::TokenStream {
    let crate_path = &type_config.crate_path;
    let bit_bound = type_config.bit_bound.unwrap_or(32);

    // Validate positions
    let mut positions: Vec<(String, u8)> = Vec::new();
    for variant in variants.iter() {
        let variant_name = variant.ident.to_string();
        // Parse position from #[dds(position = N)] on variant
        let fc = parse_variant_attributes(variant);
        let pos = fc.unwrap_or_else(|| {
            panic!("Bitmask variant '{}' must have #[dds(position = N)] attribute", variant_name);
        });
        if pos >= bit_bound {
            panic!(
                "Bitmask variant '{}' position {} exceeds bit_bound {}",
                variant_name, pos, bit_bound
            );
        }
        if positions.iter().any(|(_, p)| *p == pos) {
            panic!("Duplicate position {} in bitmask variant '{}'", pos, variant_name);
        }
        positions.push((variant_name, pos));
    }

    // Determine wire type
    let (wire_type, wire_ser, wire_deser) = if bit_bound <= 8 {
        (quote!(u8), quote!(serialize_u8), quote!(deserialize_u8))
    } else if bit_bound <= 16 {
        (quote!(u16), quote!(serialize_u16), quote!(deserialize_u16))
    } else if bit_bound <= 32 {
        (quote!(u32), quote!(serialize_u32), quote!(deserialize_u32))
    } else {
        (quote!(u64), quote!(serialize_u64), quote!(deserialize_u64))
    };

    // Generate companion value type name
    let value_name = quote::format_ident!("{}Value", name);

    // Generate flag constants for the companion type
    let flag_constants: Vec<_> = variants
        .iter()
        .map(|variant| {
            let variant_ident = &variant.ident;
            let pos = parse_variant_attributes(variant).unwrap();
            let shift: proc_macro2::TokenStream = format!(
                "1{}_{}",
                if bit_bound <= 8 {
                    "u8"
                } else if bit_bound <= 16 {
                    "u16"
                } else if bit_bound <= 32 {
                    "u32"
                } else {
                    "u64"
                },
                ""
            )
            .parse()
            .unwrap();
            let _ = shift; // unused, compute directly
            quote! {
                pub const #variant_ident: #value_name = #value_name(1 << #pos);
            }
        })
        .collect();

    // Generate From<enum> for value type
    let from_variant_arms: Vec<_> = variants
        .iter()
        .map(|variant| {
            let variant_ident = &variant.ident;
            let pos = parse_variant_attributes(variant).unwrap();
            quote! {
                #name::#variant_ident => #value_name(1 << #pos),
            }
        })
        .collect();

    // Mask of every declared flag bit, used to reject unknown bits in `from_bits`.
    let valid_mask: u64 = positions.iter().map(|(_, p)| 1u64 << *p).fold(0, |a, b| a | b);
    let valid_mask_lit: proc_macro2::TokenStream =
        format!("{}_{}", valid_mask, quote!(#wire_type)).parse().unwrap();

    // Generate additional derives
    let additional_derives =
        crate::codegen::derives::generate_additional_derives(input, name, type_config);

    // Generate HasTypeObject
    let has_type_object_impl = crate::codegen::type_object::generate_has_type_object_bitmask_impl(
        name,
        variants,
        type_config,
    );

    quote! {
        /// Companion value type for bitmask, supports bitwise operations
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
        pub struct #value_name(pub #wire_type);

        impl #value_name {
            #(#flag_constants)*

            /// Create an empty bitmask value (no flags set)
            pub fn empty() -> Self {
                Self(0)
            }

            /// Check if a flag is set
            pub fn contains(&self, flag: #value_name) -> bool {
                (self.0 & flag.0) == flag.0
            }

            /// Insert a flag (set its bits)
            pub fn insert(&mut self, flag: #value_name) {
                self.0 |= flag.0;
            }

            /// Remove a flag (clear its bits)
            pub fn remove(&mut self, flag: #value_name) {
                self.0 &= !flag.0;
            }

            /// Get the raw value
            pub fn bits(&self) -> #wire_type {
                self.0
            }

            /// Construct from a raw value, returning `None` when a bit is set that
            /// corresponds to no declared flag.
            pub fn from_bits(bits: #wire_type) -> Option<Self> {
                if bits & !#valid_mask_lit != 0 {
                    None
                } else {
                    Some(Self(bits))
                }
            }
        }

        impl std::ops::BitXor for #value_name {
            type Output = Self;
            fn bitxor(self, rhs: Self) -> Self {
                Self(self.0 ^ rhs.0)
            }
        }

        impl std::ops::BitXorAssign for #value_name {
            fn bitxor_assign(&mut self, rhs: Self) {
                self.0 ^= rhs.0;
            }
        }

        impl std::ops::BitOr for #value_name {
            type Output = Self;
            fn bitor(self, rhs: Self) -> Self {
                Self(self.0 | rhs.0)
            }
        }

        impl std::ops::BitAnd for #value_name {
            type Output = Self;
            fn bitand(self, rhs: Self) -> Self {
                Self(self.0 & rhs.0)
            }
        }

        impl std::ops::BitOrAssign for #value_name {
            fn bitor_assign(&mut self, rhs: Self) {
                self.0 |= rhs.0;
            }
        }

        impl std::ops::BitAndAssign for #value_name {
            fn bitand_assign(&mut self, rhs: Self) {
                self.0 &= rhs.0;
            }
        }

        impl std::ops::Not for #value_name {
            type Output = Self;
            fn not(self) -> Self {
                Self(!self.0)
            }
        }

        impl From<#name> for #value_name {
            fn from(flag: #name) -> Self {
                match flag {
                    #(#from_variant_arms)*
                }
            }
        }

        // CdrSerialize for companion value type
        impl #crate_path::serialize::cdr::CdrSerialize for #value_name {
            fn serialize_cdr(&self, serializer: &mut #crate_path::serialize::cdr::CdrSerializer) -> #crate_path::serialize::cdr::CdrResult<()> {
                use #crate_path::serialize::cdr::PrimitiveSerialize;
                serializer.#wire_ser(self.0)
            }
        }

        impl #crate_path::serialize::cdr::CdrDeserialize for #value_name {
            fn deserialize_cdr(deserializer: &mut #crate_path::serialize::cdr::CdrDeserializer) -> #crate_path::serialize::cdr::CdrResult<Self> {
                use #crate_path::serialize::cdr::PrimitiveSerialize;
                Ok(Self(deserializer.#wire_deser()?))
            }
        }

        // Bitmask is not a primitive type, so collections of it carry a DHEADER
        // (DDS-XTypes 7.4.3.5.3/7.4.3.5.4, and DDSXTY14-56 names bitmask explicitly
        // as a type that *would* need a spec change to become primitive).
        impl #crate_path::serialize::xcdr::XcdrSerialize for #value_name {
            const IS_PRIMITIVE: bool = false;
            fn serialize_xcdr(&self, serializer: &mut #crate_path::serialize::xcdr::XcdrSerializer) -> #crate_path::serialize::xcdr::XcdrResult<()> {
                use #crate_path::serialize::cdr::PrimitiveSerialize;
                serializer.#wire_ser(self.0)
            }
        }

        impl #crate_path::serialize::xcdr::XcdrDeserialize for #value_name {
            const IS_PRIMITIVE: bool = false;
            fn deserialize_xcdr(deserializer: &mut #crate_path::serialize::xcdr::XcdrDeserializer) -> #crate_path::serialize::xcdr::XcdrResult<Self> {
                use #crate_path::serialize::cdr::PrimitiveSerialize;
                Ok(Self(deserializer.#wire_deser()?))
            }
        }

        impl #crate_path::serialize::xcdr::XcdrSerializeMembers for #value_name {
            fn serialize_xcdr_members(&self, serializer: &mut #crate_path::serialize::xcdr::XcdrSerializer) -> #crate_path::serialize::xcdr::XcdrResult<()> {
                #crate_path::serialize::xcdr::XcdrSerialize::serialize_xcdr(self, serializer)
            }
        }

        impl #crate_path::serialize::xcdr::XcdrDeserializeMembers for #value_name {
            fn deserialize_xcdr_members(deserializer: &mut #crate_path::serialize::xcdr::XcdrDeserializer) -> #crate_path::serialize::xcdr::XcdrResult<Self> {
                #crate_path::serialize::xcdr::XcdrDeserialize::deserialize_xcdr(deserializer)
            }
        }

        #[automatically_derived]
        impl<C: #crate_path::speedy::Context> #crate_path::speedy::Writable<C> for #value_name {
            fn write_to<T: ?Sized + #crate_path::speedy::Writer<C>>(&self, writer: &mut T) -> Result<(), C::Error> {
                writer.write_value(&self.0)
            }
        }

        #[automatically_derived]
        impl<'a, C: #crate_path::speedy::Context> #crate_path::speedy::Readable<'a, C> for #value_name {
            fn read_from<R: #crate_path::speedy::Reader<'a, C>>(reader: &mut R) -> Result<Self, C::Error> {
                Ok(Self(reader.read_value()?))
            }
        }

        #additional_derives

        #has_type_object_impl
    }
}

/// Parse variant-level #[dds(position = N)] attribute
fn parse_variant_attributes(variant: &syn::Variant) -> Option<u8> {
    for attr in &variant.attrs {
        if attr.path().is_ident("dds") {
            let mut position = None;
            let _ = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("position") {
                    let value = meta.value()?;
                    let lit: syn::LitInt = value.parse()?;
                    position = Some(lit.base10_parse::<u8>()?);
                }
                Ok(())
            });
            if position.is_some() {
                return position;
            }
        }
    }
    None
}
