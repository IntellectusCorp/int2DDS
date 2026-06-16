use std::collections::HashMap;

use crate::dcps::core::error::{DdsError, DdsResult};
use crate::xtypes::{ExtensibilityKind, TryConstructKind, TypeIdentifier};

#[derive(Debug)]
pub(crate) enum XmlTypeDecl {
    Struct(XmlStruct),
    Enum(XmlEnum),
    Typedef(XmlTypedef),
    Union(XmlUnion),
    Bitmask(XmlBitmask),
    Bitset(XmlBitset),
}

impl XmlTypeDecl {
    pub(crate) fn name(&self) -> &str {
        match self {
            XmlTypeDecl::Struct(s) => &s.name,
            XmlTypeDecl::Enum(e) => &e.name,
            XmlTypeDecl::Typedef(t) => &t.name,
            XmlTypeDecl::Union(u) => &u.name,
            XmlTypeDecl::Bitmask(b) => &b.name,
            XmlTypeDecl::Bitset(b) => &b.name,
        }
    }

    pub(crate) fn member_types_mut(&mut self) -> Vec<(String, &mut XmlMemberType)> {
        match self {
            XmlTypeDecl::Struct(s) => {
                s.members.iter_mut().map(|m| (m.name.clone(), &mut m.ty)).collect()
            }
            XmlTypeDecl::Enum(_) | XmlTypeDecl::Bitmask(_) | XmlTypeDecl::Bitset(_) => Vec::new(),
            XmlTypeDecl::Typedef(t) => vec![(t.name.clone(), &mut t.ty)],
            XmlTypeDecl::Union(u) => {
                let mut out = vec![("discriminator".to_string(), &mut u.discriminator)];
                out.extend(u.cases.iter_mut().map(|c| (c.name.clone(), &mut c.ty)));
                out
            }
        }
    }
}

#[derive(Debug)]
pub(crate) struct XmlEnum {
    pub(crate) name: String,
    pub(crate) bit_bound: u16,
    pub(crate) literals: Vec<XmlEnumLiteral>,
}

#[derive(Debug)]
pub(crate) struct XmlEnumLiteral {
    pub(crate) name: String,
    pub(crate) value: Option<i32>,
    pub(crate) is_default: bool,
}

#[derive(Debug)]
pub(crate) struct XmlTypedef {
    pub(crate) name: String,
    pub(crate) ty: XmlMemberType,
}

#[derive(Debug)]
pub(crate) struct XmlUnion {
    pub(crate) name: String,
    pub(crate) extensibility: ExtensibilityKind,
    pub(crate) discriminator: XmlMemberType,
    pub(crate) cases: Vec<XmlUnionCase>,
}

#[derive(Debug)]
pub(crate) struct XmlUnionCase {
    pub(crate) name: String,
    pub(crate) ty: XmlMemberType,
    pub(crate) labels: Vec<XmlCaseLabel>,
    pub(crate) is_default: bool,
}

#[derive(Debug, Clone)]
pub(crate) enum XmlCaseLabel {
    Int(i32),
    // Enum literal name, resolved to its value at load time.
    Name(String),
}

#[derive(Debug)]
pub(crate) struct XmlBitmask {
    pub(crate) name: String,
    pub(crate) bit_bound: u16,
    pub(crate) flags: Vec<XmlBitflag>,
}

#[derive(Debug)]
pub(crate) struct XmlBitflag {
    pub(crate) name: String,
    pub(crate) position: u16,
}

#[derive(Debug)]
pub(crate) struct XmlBitset {
    pub(crate) name: String,
    pub(crate) fields: Vec<XmlBitfield>,
}

#[derive(Debug)]
pub(crate) struct XmlBitfield {
    // None marks an anonymous padding bitfield (advances position, not emitted).
    pub(crate) name: Option<String>,
    pub(crate) bitcount: u8,
    pub(crate) holder: TypeIdentifier,
}

#[derive(Debug)]
pub(crate) struct XmlStruct {
    pub(crate) name: String,
    pub(crate) base_type: Option<String>,
    pub(crate) extensibility: ExtensibilityKind,
    pub(crate) autoid_hash: bool,
    pub(crate) nested: bool,
    pub(crate) data_representation: Option<u16>,
    pub(crate) members: Vec<XmlMember>,
}

#[derive(Debug)]
pub(crate) struct XmlMember {
    pub(crate) name: String,
    pub(crate) ty: XmlMemberType,
    pub(crate) id: Option<u32>,
    pub(crate) hashid: Option<String>,
    pub(crate) key: bool,
    pub(crate) optional: bool,
    pub(crate) external: bool,
    pub(crate) must_understand: bool,
    pub(crate) try_construct: TryConstructKind,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum XmlMemberType {
    Primitive(TypeIdentifier),
    String { bound: Option<u32> },
    WString { bound: Option<u32> },
    Sequence { element: Box<XmlMemberType>, bound: Option<u32> },
    Array { element: Box<XmlMemberType>, dims: Vec<u32> },
    Map { key: Box<XmlMemberType>, value: Box<XmlMemberType>, bound: Option<u32> },
    NonBasic(String),
}

impl XmlMemberType {
    pub(crate) fn visit_nonbasic_mut(
        &mut self,
        f: &mut impl FnMut(&mut String) -> DdsResult<()>,
    ) -> DdsResult<()> {
        match self {
            XmlMemberType::NonBasic(name) => f(name),
            XmlMemberType::Sequence { element, .. } | XmlMemberType::Array { element, .. } => {
                element.visit_nonbasic_mut(f)
            }
            XmlMemberType::Map { key, value, .. } => {
                key.visit_nonbasic_mut(f)?;
                value.visit_nonbasic_mut(f)
            }
            _ => Ok(()),
        }
    }

    pub(crate) fn referenced_names<'a>(&'a self, out: &mut Vec<&'a str>) {
        match self {
            XmlMemberType::NonBasic(name) => out.push(name),
            XmlMemberType::Sequence { element, .. } | XmlMemberType::Array { element, .. } => {
                element.referenced_names(out)
            }
            XmlMemberType::Map { key, value, .. } => {
                key.referenced_names(out);
                value.referenced_names(out);
            }
            _ => {}
        }
    }

    // Replaces typedef references with their aliased types, erroring on cycles.
    pub(crate) fn flatten_typedefs(
        &self,
        typedefs: &HashMap<String, XmlMemberType>,
        stack: &mut Vec<String>,
    ) -> DdsResult<XmlMemberType> {
        Ok(match self {
            XmlMemberType::NonBasic(name) => match typedefs.get(name) {
                Some(inner) => {
                    if stack.iter().any(|s| s == name) {
                        return Err(DdsError::Error(format!(
                            "XML types: circular typedef '{name}'"
                        )));
                    }
                    stack.push(name.clone());
                    let flattened = inner.flatten_typedefs(typedefs, stack)?;
                    stack.pop();
                    flattened
                }
                None => self.clone(),
            },
            XmlMemberType::Sequence { element, bound } => XmlMemberType::Sequence {
                element: Box::new(element.flatten_typedefs(typedefs, stack)?),
                bound: *bound,
            },
            XmlMemberType::Array { element, dims } => XmlMemberType::Array {
                element: Box::new(element.flatten_typedefs(typedefs, stack)?),
                dims: dims.clone(),
            },
            XmlMemberType::Map { key, value, bound } => XmlMemberType::Map {
                key: Box::new(key.flatten_typedefs(typedefs, stack)?),
                value: Box::new(value.flatten_typedefs(typedefs, stack)?),
                bound: *bound,
            },
            other => other.clone(),
        })
    }
}

pub(crate) fn primitive_type_id(name: &str) -> Option<TypeIdentifier> {
    Some(match name {
        "boolean" => TypeIdentifier::Boolean,
        "byte" | "octet" => TypeIdentifier::Byte,
        "char8" | "char" => TypeIdentifier::Char8,
        "char16" | "wchar" => TypeIdentifier::Char16,
        "int8" => TypeIdentifier::Int8,
        "int16" | "short" => TypeIdentifier::Int16,
        "int32" | "long" => TypeIdentifier::Int32,
        "int64" | "longLong" => TypeIdentifier::Int64,
        "uint8" => TypeIdentifier::Uint8,
        "uint16" | "unsignedShort" => TypeIdentifier::Uint16,
        "uint32" | "unsignedLong" => TypeIdentifier::Uint32,
        "uint64" | "unsignedLongLong" => TypeIdentifier::Uint64,
        "float32" | "float" => TypeIdentifier::Float32,
        "float64" | "double" => TypeIdentifier::Float64,
        "float128" | "longDouble" => TypeIdentifier::Float128,
        _ => return None,
    })
}
