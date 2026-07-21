/// TryConstruct behavior for bound-violating deserialized values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TryConstructKind {
    #[default]
    Discard,
    UseDefault,
    Trim,
}

/// Field attribute configuration parsed from #[dds(...)] attributes
#[derive(Debug, Clone, Default)]
pub struct FieldConfig {
    pub key: bool,
    pub id: Option<u32>,
    pub optional: bool,
    pub must_understand: bool,
    pub bound: Option<usize>,
    /// Default value for the field when not present in serialized data (DDS-XTYPES @default)
    pub default: Option<syn::Lit>,
    /// @external: field is stored as Box<T>, wire format is inline
    pub external: bool,
    /// @hashid: compute member_id from hash. None=not set, Some("")=use field name, Some("name")=use custom name
    pub hashid: Option<String>,
    /// @parent: this field represents an inherited base type (struct inheritance)
    pub parent: bool,
    /// @position: bit position for bitmask variants
    pub position: Option<u8>,
    /// @bitfield: bit width for bitset fields
    pub bitfield: Option<u8>,
    /// #[dds(char)]: storage is u8 / [u8; N] but TypeObject must register CHAR8.
    pub as_char: bool,
    /// #[dds(uint8)]: storage is u8 / Vec<u8> / [u8; N] but TypeObject must register UINT8 (not BYTE).
    pub as_uint8: bool,
    /// @try_construct: deserialize-side behavior on bound violation.
    pub try_construct: TryConstructKind,
    /// @non_serialized: field is excluded from wire and from TypeObject.
    pub non_serialized: bool,
}

/// Convert a literal to a TokenStream for code generation
/// Handles type conversion for String (adds .to_string())
pub fn literal_to_tokens(lit: &syn::Lit, ty: &syn::Type) -> proc_macro2::TokenStream {
    use quote::quote;

    match lit {
        syn::Lit::Str(s) => {
            // Check if target type is String
            if let syn::Type::Path(type_path) = ty {
                if let Some(seg) = type_path.path.segments.last() {
                    if seg.ident == "String" {
                        return quote! { #s.to_string() };
                    }
                }
            }
            quote! { #s }
        }
        syn::Lit::Int(i) => quote! { #i },
        syn::Lit::Float(f) => quote! { #f },
        syn::Lit::Bool(b) => quote! { #b },
        syn::Lit::Char(c) => quote! { #c },
        syn::Lit::Byte(b) => quote! { #b },
        syn::Lit::ByteStr(bs) => quote! { #bs },
        _ => quote! { Default::default() },
    }
}

/// Parse field attributes from #[dds(...)] annotations.
/// Panics on mutually-exclusive combinations such as `@key` + `@non_serialized`.
pub fn parse_field_attributes(field: &syn::Field) -> FieldConfig {
    let mut config = FieldConfig::default();

    for attr in &field.attrs {
        if attr.path().is_ident("dds") {
            let _ = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("key") {
                    config.key = true;
                } else if meta.path.is_ident("id") {
                    let value = meta.value()?;
                    let lit: syn::LitInt = value.parse()?;
                    config.id = Some(lit.base10_parse::<u32>()?);
                } else if meta.path.is_ident("optional") {
                    config.optional = true;
                } else if meta.path.is_ident("must_understand") {
                    config.must_understand = true;
                } else if meta.path.is_ident("bound") {
                    let value = meta.value()?;
                    let lit: syn::LitInt = value.parse()?;
                    config.bound = Some(lit.base10_parse::<usize>()?);
                } else if meta.path.is_ident("default") {
                    // Parse @default annotation: #[dds(default = <literal>)]
                    // Supported literals: integer, float, bool, string, char
                    let value = meta.value()?;
                    let lit: syn::Lit = value.parse()?;
                    config.default = Some(lit);
                } else if meta.path.is_ident("external") {
                    config.external = true;
                } else if meta.path.is_ident("hashid") {
                    // #[dds(hashid)] or #[dds(hashid = "custom_name")]
                    if let Ok(value) = meta.value() {
                        let lit: syn::LitStr = value.parse()?;
                        config.hashid = Some(lit.value());
                    } else {
                        config.hashid = Some(String::new());
                    }
                } else if meta.path.is_ident("parent") {
                    config.parent = true;
                } else if meta.path.is_ident("position") {
                    let value = meta.value()?;
                    let lit: syn::LitInt = value.parse()?;
                    config.position = Some(lit.base10_parse::<u8>()?);
                } else if meta.path.is_ident("bitfield") {
                    let value = meta.value()?;
                    let lit: syn::LitInt = value.parse()?;
                    config.bitfield = Some(lit.base10_parse::<u8>()?);
                } else if meta.path.is_ident("char") {
                    config.as_char = true;
                } else if meta.path.is_ident("uint8") {
                    config.as_uint8 = true;
                } else if meta.path.is_ident("non_serialized") {
                    config.non_serialized = true;
                } else if meta.path.is_ident("try_construct") {
                    let value = meta.value()?;
                    let lit: syn::LitStr = value.parse()?;
                    config.try_construct = match lit.value().to_ascii_lowercase().as_str() {
                        "discard" => TryConstructKind::Discard,
                        "use_default" => TryConstructKind::UseDefault,
                        "trim" => TryConstructKind::Trim,
                        other => {
                            return Err(meta.error(format!(
                                "unknown try_construct value '{}', expected 'discard' | 'use_default' | 'trim'",
                                other
                            )));
                        }
                    };
                }
                Ok(())
            });
        }
    }

    if is_option_type(&field.ty) {
        config.optional = true;
    }

    if config.key && config.non_serialized {
        let field_name =
            field.ident.as_ref().map(ToString::to_string).unwrap_or_else(|| "<anon>".to_string());
        panic!(
            "Field '{}' cannot be both #[dds(key)] and #[dds(non_serialized)] (XTypes 7.3.1.2.1.14)",
            field_name
        );
    }

    if config.as_char && config.as_uint8 {
        let field_name =
            field.ident.as_ref().map(ToString::to_string).unwrap_or_else(|| "<anon>".to_string());
        panic!(
            "Field '{}' cannot be both #[dds(char)] and #[dds(uint8)]: a u8 maps to exactly one of CHAR8/UINT8/BYTE",
            field_name
        );
    }

    if config.key && config.optional {
        let field_name =
            field.ident.as_ref().map(ToString::to_string).unwrap_or_else(|| "<anon>".to_string());
        panic!(
            "Field '{}' cannot be both #[dds(key)] and optional: keyed members cannot be optional (XTypes 7.2.2.4.4.4.7)",
            field_name
        );
    }

    config
}

#[derive(Debug, Clone, Copy)]
pub enum SerializationMethod {
    U8,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    I64,
    F32,
    F64,
    String,
    WString,
    Char,
    Bool,
    U8Array,
    U16Array,
    U32Array,
    U64Array,
    I8Array,
    I16Array,
    I32Array,
    I64Array,
    F32Array,
    F64Array,
    VecU8,
    VecU16,
    VecU32,
    VecU64,
    VecI8,
    VecI16,
    VecI32,
    VecI64,
    VecF32,
    VecF64,
    // New sequence types for ROS2 RMW support
    VecBool,
    VecChar,
    VecString,
    // New array types for ROS2 RMW support
    BoolArray,
    CharArray,
    StringArray,
    Fallback,
}

pub fn get_serialization_method(ty: &syn::Type) -> SerializationMethod {
    if let syn::Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            match segment.ident.to_string().as_str() {
                "u8" => SerializationMethod::U8,
                "u16" => SerializationMethod::U16,
                "u32" => SerializationMethod::U32,
                "u64" => SerializationMethod::U64,
                "i8" => SerializationMethod::I8,
                "i16" => SerializationMethod::I16,
                "i32" => SerializationMethod::I32,
                "i64" => SerializationMethod::I64,
                "f32" => SerializationMethod::F32,
                "f64" => SerializationMethod::F64,
                "String" => SerializationMethod::String,
                "WString" => SerializationMethod::WString,
                "char" => SerializationMethod::Char,
                "bool" => SerializationMethod::Bool,
                "Vec" => {
                    // Check Vec<T> generic argument
                    if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                        if let Some(syn::GenericArgument::Type(syn::Type::Path(inner_path))) =
                            args.args.first()
                        {
                            if let Some(inner_segment) = inner_path.path.segments.last() {
                                return match inner_segment.ident.to_string().as_str() {
                                    "u8" => SerializationMethod::VecU8,
                                    "u16" => SerializationMethod::VecU16,
                                    "u32" => SerializationMethod::VecU32,
                                    "u64" => SerializationMethod::VecU64,
                                    "i8" => SerializationMethod::VecI8,
                                    "i16" => SerializationMethod::VecI16,
                                    "i32" => SerializationMethod::VecI32,
                                    "i64" => SerializationMethod::VecI64,
                                    "f32" => SerializationMethod::VecF32,
                                    "f64" => SerializationMethod::VecF64,
                                    "bool" => SerializationMethod::VecBool,
                                    "char" => SerializationMethod::VecChar,
                                    "String" => SerializationMethod::VecString,
                                    _ => SerializationMethod::Fallback,
                                };
                            }
                        }
                    }
                    SerializationMethod::Fallback
                }
                "HashMap" | "BTreeMap" => {
                    // HashMap<K, V> and BTreeMap<K, V> are supported via trait-based serialization
                    // Use Fallback to invoke CdrSerialize/XcdrSerialize trait methods
                    SerializationMethod::Fallback
                }
                _ => SerializationMethod::Fallback,
            }
        } else {
            SerializationMethod::Fallback
        }
    } else if let syn::Type::Array(array) = ty {
        if let syn::Type::Path(element_type) = &*array.elem {
            if let Some(segment) = element_type.path.segments.last() {
                return match segment.ident.to_string().as_str() {
                    "u8" => SerializationMethod::U8Array,
                    "u16" => SerializationMethod::U16Array,
                    "u32" => SerializationMethod::U32Array,
                    "u64" => SerializationMethod::U64Array,
                    "i8" => SerializationMethod::I8Array,
                    "i16" => SerializationMethod::I16Array,
                    "i32" => SerializationMethod::I32Array,
                    "i64" => SerializationMethod::I64Array,
                    "f32" => SerializationMethod::F32Array,
                    "f64" => SerializationMethod::F64Array,
                    "bool" => SerializationMethod::BoolArray,
                    "char" => SerializationMethod::CharArray,
                    "String" => SerializationMethod::StringArray,
                    _ => SerializationMethod::Fallback,
                };
            }
        }
        SerializationMethod::Fallback
    } else {
        SerializationMethod::Fallback
    }
}

/// Extract array size from [T; N] type
pub fn get_array_size(ty: &syn::Type) -> Option<usize> {
    if let syn::Type::Array(array) = ty {
        if let syn::Expr::Lit(lit) = &array.len {
            if let syn::Lit::Int(int_lit) = &lit.lit {
                return int_lit.base10_parse::<usize>().ok();
            }
        }
    }
    None
}

/// Check if a type is an unbounded String (not a generic like Option<String> or Vec<String>)
/// This checks for the exact type `String` or `std::string::String` without generic parameters
pub fn is_unbounded_string(ty: &syn::Type) -> bool {
    if let syn::Type::Path(type_path) = ty {
        // Check if the path has no leading colons and segments
        let path = &type_path.path;

        // Get the last segment of the path
        if let Some(last_segment) = path.segments.last() {
            // Check if the last segment is "String" and has no generic arguments
            if last_segment.ident == "String" {
                match &last_segment.arguments {
                    syn::PathArguments::None => {
                        // No generic arguments - this is an unbounded String
                        return true;
                    }
                    _ => {
                        // Has generic arguments - not an unbounded String
                        return false;
                    }
                }
            }
        }
    }
    false
}

/// Check if a type is `Option<T>` (by last path segment).
pub fn is_option_type(ty: &syn::Type) -> bool {
    if let syn::Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Option" {
                return matches!(segment.arguments, syn::PathArguments::AngleBracketed(_));
            }
        }
    }
    false
}

pub fn extract_option_inner_type(ty: &syn::Type) -> Option<syn::Type> {
    if let syn::Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Option" {
                if let syn::PathArguments::AngleBracketed(ref args) = segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                        return Some(inner.clone());
                    }
                }
            }
        }
    }
    None
}

pub fn is_map_type(ty: &syn::Type) -> bool {
    if let syn::Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            matches!(segment.ident.to_string().as_str(), "HashMap" | "BTreeMap")
        } else {
            false
        }
    } else {
        false
    }
}

/// Discriminant type for enum/union
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiscriminantType {
    #[default]
    I32, // switch (long) - default
    I16,  // switch (short)
    U8,   // switch (octet)
    Bool, // switch (boolean)
}

impl DiscriminantType {
    pub fn bit_bound(&self) -> u16 {
        match self {
            DiscriminantType::I32 => 32,
            DiscriminantType::I16 => 16,
            DiscriminantType::U8 | DiscriminantType::Bool => 8,
        }
    }

    /// Get the Rust type name for this discriminant type
    pub fn rust_type(&self) -> &'static str {
        match self {
            DiscriminantType::I32 => "i32",
            DiscriminantType::I16 => "i16",
            DiscriminantType::U8 => "u8",
            DiscriminantType::Bool => "bool",
        }
    }

    /// Get the serialization method name for this discriminant type
    pub fn serialize_method(&self) -> &'static str {
        match self {
            DiscriminantType::I32 => "serialize_i32",
            DiscriminantType::I16 => "serialize_i16",
            DiscriminantType::U8 => "serialize_u8",
            DiscriminantType::Bool => "serialize_bool",
        }
    }

    /// Get the deserialization method name for this discriminant type
    pub fn deserialize_method(&self) -> &'static str {
        match self {
            DiscriminantType::I32 => "deserialize_i32",
            DiscriminantType::I16 => "deserialize_i16",
            DiscriminantType::U8 => "deserialize_u8",
            DiscriminantType::Bool => "deserialize_bool",
        }
    }
}

/// Parse #[repr(...)] attribute to determine discriminant type
pub fn parse_repr_attribute(attrs: &[syn::Attribute]) -> DiscriminantType {
    for attr in attrs {
        if attr.path().is_ident("repr") {
            if let Ok(repr) = attr.parse_args::<syn::Ident>() {
                return match repr.to_string().as_str() {
                    "i32" => DiscriminantType::I32,
                    "i16" => DiscriminantType::I16,
                    "u8" => DiscriminantType::U8,
                    "bool" => DiscriminantType::Bool,
                    _ => DiscriminantType::I32, // default
                };
            }
        }
    }
    DiscriminantType::I32 // default
}

/// Check if an enum variant has associated data
pub fn variant_has_data(variant: &syn::Variant) -> bool {
    !matches!(variant.fields, syn::Fields::Unit)
}

/// Check if enum is C-style (all variants have no data)
pub fn is_c_style_enum(
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::token::Comma>,
) -> bool {
    variants.iter().all(|v| !variant_has_data(v))
}

/// Get the type of a tuple variant (single field only)
pub fn get_variant_type(variant: &syn::Variant) -> Option<&syn::Type> {
    match &variant.fields {
        syn::Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
            Some(&fields.unnamed.first()?.ty)
        }
        _ => None,
    }
}

/// Get the discriminant value for a variant. Priority:
/// 1. `#[dds(value = N)]` attribute (XTypes @value, 7.3.1.2.1.5)
/// 2. Rust `= N` discriminant syntax
/// 3. Positional index
pub fn get_discriminant_value(variant: &syn::Variant, index: usize) -> i64 {
    for attr in &variant.attrs {
        if attr.path().is_ident("dds") {
            let mut value: Option<i64> = None;
            let _ = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("value") {
                    let parsed = meta.value()?;
                    let lit: syn::LitInt = parsed.parse()?;
                    value = Some(lit.base10_parse::<i64>()?);
                }
                Ok(())
            });
            if let Some(v) = value {
                return v;
            }
        }
    }
    if let Some((_, expr)) = &variant.discriminant {
        if let syn::Expr::Lit(lit) = expr {
            if let syn::Lit::Int(int_lit) = &lit.lit {
                return int_lit.base10_parse().unwrap_or(index as i64);
            }
        }
        if let syn::Expr::Unary(unary) = expr {
            if matches!(unary.op, syn::UnOp::Neg(_)) {
                if let syn::Expr::Lit(lit) = &*unary.expr {
                    if let syn::Lit::Int(int_lit) = &lit.lit {
                        let val: i64 = int_lit.base10_parse().unwrap_or(index as i64);
                        return -val;
                    }
                }
            }
        }
    }
    index as i64
}

/// Compute enumerated-literal values for an entire variant list with XTypes 7.3.1.2.1.5
/// progression semantics: unspecified values continue from the most-recently specified value.
pub fn compute_enumerated_values(
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::token::Comma>,
) -> Vec<i64> {
    let mut values = Vec::with_capacity(variants.len());
    let mut next_implicit: i64 = 0;
    for (index, variant) in variants.iter().enumerate() {
        let explicit = explicit_dds_value(variant).or_else(|| explicit_rust_discriminant(variant));
        let resolved = match explicit {
            Some(v) => v,
            None => {
                let _ = index;
                next_implicit
            }
        };
        values.push(resolved);
        next_implicit = resolved + 1;
    }
    values
}

fn explicit_dds_value(variant: &syn::Variant) -> Option<i64> {
    for attr in &variant.attrs {
        if attr.path().is_ident("dds") {
            let mut found = None;
            let _ = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("value") {
                    let parsed = meta.value()?;
                    let lit: syn::LitInt = parsed.parse()?;
                    found = Some(lit.base10_parse::<i64>()?);
                }
                Ok(())
            });
            if found.is_some() {
                return found;
            }
        }
    }
    None
}

fn explicit_rust_discriminant(variant: &syn::Variant) -> Option<i64> {
    let (_, expr) = variant.discriminant.as_ref()?;
    if let syn::Expr::Lit(lit) = expr {
        if let syn::Lit::Int(int_lit) = &lit.lit {
            return int_lit.base10_parse().ok();
        }
    }
    if let syn::Expr::Unary(unary) = expr {
        if matches!(unary.op, syn::UnOp::Neg(_)) {
            if let syn::Expr::Lit(lit) = &*unary.expr {
                if let syn::Lit::Int(int_lit) = &lit.lit {
                    let val: i64 = int_lit.base10_parse().ok()?;
                    return Some(-val);
                }
            }
        }
    }
    None
}

/// Returns true if the variant is marked `#[dds(default_literal)]` (XTypes 7.3.1.2.1.10).
pub fn variant_is_default_literal(variant: &syn::Variant) -> bool {
    for attr in &variant.attrs {
        if attr.path().is_ident("dds") {
            let mut found = false;
            let _ = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("default_literal") {
                    found = true;
                }
                Ok(())
            });
            if found {
                return true;
            }
        }
    }
    false
}

/// Returns true if the variant is marked `#[dds(default)]` — the union `default:`
/// case selected by any discriminant that matches no other label.
pub fn variant_is_union_default(variant: &syn::Variant) -> bool {
    for attr in &variant.attrs {
        if attr.path().is_ident("dds") {
            let mut found = false;
            let _ = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("default") && !meta.input.peek(syn::Token![=]) {
                    found = true;
                }
                Ok(())
            });
            if found {
                return true;
            }
        }
    }
    false
}

/// Get bitmask position from variant attributes (#[dds(position = N)]).
/// Falls back to variant index if not specified.
pub fn get_bitmask_position(variant: &syn::Variant, index: usize) -> u8 {
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
            if let Some(pos) = position {
                return pos;
            }
        }
    }
    index as u8
}

/// Emit `<crate_path>::xtypes::TryConstructKind::<Variant>` for MemberFlag construction.
pub fn try_construct_to_tokens(
    kind: TryConstructKind,
    crate_path: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    use quote::quote;
    match kind {
        TryConstructKind::Discard => quote! { #crate_path::xtypes::TryConstructKind::Discard },
        TryConstructKind::UseDefault => {
            quote! { #crate_path::xtypes::TryConstructKind::UseDefault }
        }
        TryConstructKind::Trim => quote! { #crate_path::xtypes::TryConstructKind::Trim },
    }
}

/// AutoId kind for struct-level auto ID assignment
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoIdKind {
    Sequential,
    Hash,
}

/// Compute member ID hash from a name string.
/// Algorithm: MD5(name) first 4 bytes as little-endian u32, masked to 28 bits.
pub fn compute_member_id_hash(name: &str) -> u32 {
    let digest = md5::compute(name.as_bytes());
    let bytes: [u8; 4] = [digest[0], digest[1], digest[2], digest[3]];
    let raw = u32::from_le_bytes(bytes);
    raw & 0x0FFF_FFFF
}

/// Resolve member ID for a field based on priority:
/// 1. Explicit @id
/// 2. @hashid (field name or custom name)
/// 3. @autoid(Hash) — hash of field name
/// 4. Sequential index (default or @autoid(Sequential))
pub fn resolve_member_id(
    field_config: &FieldConfig,
    field_name: &str,
    index: usize,
    autoid: Option<AutoIdKind>,
) -> u32 {
    // @id and @hashid are mutually exclusive
    if field_config.id.is_some() && field_config.hashid.is_some() {
        panic!(
            "Field '{}' cannot have both #[dds(id = ...)] and #[dds(hashid)] attributes",
            field_name
        );
    }

    // @id takes highest priority
    if let Some(id) = field_config.id {
        return id;
    }

    // @hashid takes next priority
    if let Some(ref hash_name) = field_config.hashid {
        let name = if hash_name.is_empty() { field_name } else { hash_name };
        return compute_member_id_hash(name);
    }

    // @autoid(Hash) at struct level
    if matches!(autoid, Some(AutoIdKind::Hash)) {
        return compute_member_id_hash(field_name);
    }

    // Default: sequential index
    index as u32
}
