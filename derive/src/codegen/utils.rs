/// Field attribute configuration parsed from #[dds(...)] attributes
#[derive(Debug, Clone, Default)]
pub struct FieldConfig {
    pub key: bool,
    pub id: Option<u32>,
    pub optional: bool,
    pub must_understand: bool,
    pub bound: Option<usize>,
}

/// Parse field attributes from #[dds(...)] annotations
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
                }
                Ok(())
            });
        }
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
                "char" => SerializationMethod::Char,
                "bool" => SerializationMethod::Bool,
                "Vec" => {
                    // Check Vec<T> generic argument
                    if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                        if let Some(syn::GenericArgument::Type(inner_type)) = args.args.first() {
                            if let syn::Type::Path(inner_path) = inner_type {
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

/// Check if a type is HashMap or BTreeMap
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
