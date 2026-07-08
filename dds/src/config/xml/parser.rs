use roxmltree::{Document, Node};

use crate::dcps::core::error::{DdsError, DdsResult};
use crate::xtypes::{ExtensibilityKind, TryConstructKind, TypeIdentifier};

use super::ast::{
    primitive_type_id, XmlBitfield, XmlBitflag, XmlBitmask, XmlBitset, XmlCaseLabel, XmlEnum,
    XmlEnumLiteral, XmlMember, XmlMemberType, XmlStruct, XmlTypeDecl, XmlTypedef, XmlUnion,
    XmlUnionCase,
};

// Reserved namespace for future int2DDS-only extension attributes.
const I2_NS: &str = "urn:int2dds:xml";

const MAX_MEMBER_ID: u32 = 0x0FFF_FFFF;

const STRUCT_ATTRS: &[&str] =
    &["name", "baseType", "baseClass", "extensibility", "autoid", "nested", "data_representation"];
const TYPE_SPEC_ATTRS: &[&str] = &[
    "type",
    "stringMaxLength",
    "sequenceMaxLength",
    "arrayDimensions",
    "nonBasicTypeName",
    "key_type",
    "keyType",
    "mapMaxLength",
];
const MEMBER_ONLY_ATTRS: &[&str] = &[
    "name",
    "id",
    "hashid",
    "key",
    "optional",
    "external",
    "mustUnderstand",
    "try_construct",
    "visibility",
];
const ENUM_ATTRS: &[&str] = &["name", "bit_bound", "bitBound"];
const ENUMERATOR_ATTRS: &[&str] = &["name", "value", "default_literal", "defaultLiteral"];
const UNION_ATTRS: &[&str] = &["name", "extensibility"];
const CASE_LABEL_ATTRS: &[&str] = &["value"];
const BITMASK_ATTRS: &[&str] = &["name", "bit_bound", "bitBound"];
const BIT_VALUE_ATTRS: &[&str] = &["name", "position"];
const BITSET_ATTRS: &[&str] = &["name"];
const BITFIELD_ATTRS: &[&str] = &["name", "bit_bound", "bitBound", "type"];
const CONST_ATTRS: &[&str] = &["name", "type", "value"];
const FORWARD_DCL_ATTRS: &[&str] = &["name", "kind", "type"];
const INCLUDE_ATTRS: &[&str] = &["file"];
const SKIPPED_DDS_SECTIONS: &[&str] = &[
    "qos_library",
    "domain_library",
    "domain_participant_library",
    "profiles",
    "log",
    "library_settings",
];

type ConstTable = std::collections::HashMap<String, ConstValue>;

// Only integer consts feed numeric contexts (bounds, dims, enum values);
// other primitive consts are validated on declaration but unused.
#[derive(Debug, Clone)]
enum ConstValue {
    Int(i64),
    Other,
}

// Const table + current module scope, for resolving const-name refs with IDL scoping.
#[derive(Clone, Copy)]
struct Cx<'a> {
    consts: &'a ConstTable,
    scope: &'a str,
}

impl<'a> Cx<'a> {
    fn with_scope(self, scope: &'a str) -> Cx<'a> {
        Cx { consts: self.consts, scope }
    }
}

pub(crate) fn parse_types(xml: &str, lenient: bool) -> DdsResult<Vec<XmlTypeDecl>> {
    let doc = Document::parse(xml)
        .map_err(|e| DdsError::Error(format!("XML types: parse error: {e}")))?;
    let root = doc.root_element();
    let mut consts = ConstTable::new();
    collect_consts(&doc, root, "", &mut consts, lenient)?;
    let cx = Cx { consts: &consts, scope: "" };
    let mut decls = Vec::new();
    match root.tag_name().name() {
        "dds" => {
            for child in root.children().filter(Node::is_element) {
                match child.tag_name().name() {
                    "types" => parse_section(&doc, child, cx, &mut decls, lenient)?,
                    // Loaded separately (see parse_includes); validated only here.
                    "include" => {
                        check_attrs(&doc, child, INCLUDE_ATTRS, lenient)?;
                        required_attr(&doc, child, "file")?;
                    }
                    tag if SKIPPED_DDS_SECTIONS.contains(&tag) => {}
                    tag => {
                        skip_unknown(&doc, child, lenient, &format!("element <{tag}>"))?;
                    }
                }
            }
        }
        "types" => parse_section(&doc, root, cx, &mut decls, lenient)?,
        other => {
            return Err(err_at(
                &doc,
                root,
                &format!("expected <dds> or <types> root, found <{other}>"),
            ))
        }
    }
    Ok(decls)
}

// Gather every <include file="..."> reference for the loader to resolve relative to this file.
pub(crate) fn parse_includes(xml: &str, lenient: bool) -> DdsResult<Vec<String>> {
    let doc = Document::parse(xml)
        .map_err(|e| DdsError::Error(format!("XML types: parse error: {e}")))?;
    let mut files = Vec::new();
    collect_includes(&doc, doc.root_element(), &mut files, lenient)?;
    Ok(files)
}

fn collect_includes(
    doc: &Document,
    node: Node,
    files: &mut Vec<String>,
    lenient: bool,
) -> DdsResult<()> {
    for child in node.children().filter(Node::is_element) {
        match child.tag_name().name() {
            "include" => {
                check_attrs(doc, child, INCLUDE_ATTRS, lenient)?;
                files.push(required_attr(doc, child, "file")?.to_string());
            }
            "dds" | "types" | "type" | "module" => collect_includes(doc, child, files, lenient)?,
            _ => {}
        }
    }
    Ok(())
}

fn parse_section(
    doc: &Document,
    node: Node,
    cx: Cx,
    decls: &mut Vec<XmlTypeDecl>,
    lenient: bool,
) -> DdsResult<()> {
    for child in node.children().filter(Node::is_element) {
        let tag = child.tag_name().name();
        match tag {
            "type" => parse_section(doc, child, cx, decls, lenient)?,
            "module" => {
                let name = required_attr(doc, child, "name")?;
                let inner = qualify(cx.scope, name);
                parse_section(doc, child, cx.with_scope(&inner), decls, lenient)?;
            }
            "struct" | "valuetype" => {
                decls.push(XmlTypeDecl::Struct(parse_struct(doc, child, cx, lenient)?))
            }
            "enum" => decls.push(XmlTypeDecl::Enum(parse_enum(doc, child, cx, lenient)?)),
            "typedef" => decls.push(XmlTypeDecl::Typedef(parse_typedef(doc, child, cx, lenient)?)),
            "union" => decls.push(XmlTypeDecl::Union(parse_union(doc, child, cx, lenient)?)),
            "bitmask" => decls.push(XmlTypeDecl::Bitmask(parse_bitmask(doc, child, cx, lenient)?)),
            "bitset" => decls.push(XmlTypeDecl::Bitset(parse_bitset(doc, child, cx, lenient)?)),
            // const is collected in the pre-pass; forward_dcl needs no type object.
            "const" => {}
            "forward_dcl" => {
                check_attrs(doc, child, FORWARD_DCL_ATTRS, lenient)?;
                required_attr(doc, child, "name")?;
            }
            // Loaded separately (see parse_includes); validated only here.
            "include" => {
                check_attrs(doc, child, INCLUDE_ATTRS, lenient)?;
                required_attr(doc, child, "file")?;
            }
            other => {
                skip_unknown(doc, child, lenient, &format!("element <{other}>"))?;
            }
        }
    }
    Ok(())
}

fn parse_struct(doc: &Document, node: Node, cx: Cx, lenient: bool) -> DdsResult<XmlStruct> {
    check_attrs(doc, node, STRUCT_ATTRS, lenient)?;
    let name = required_attr(doc, node, "name")?;

    let extensibility = extensibility_attr(doc, node)?;
    let autoid_hash = match node.attribute("autoid") {
        None | Some("sequential") => false,
        Some("hash") => true,
        Some(v) => return Err(err_at(doc, node, &format!("invalid autoid '{v}'"))),
    };
    let nested = bool_attr(doc, node, "nested")?;
    let data_representation = data_representation_attr(doc, node)?;

    let mut members = Vec::new();
    for child in node.children().filter(Node::is_element) {
        match child.tag_name().name() {
            "member" => members.push(parse_member(doc, child, cx, lenient)?),
            other => {
                skip_unknown(doc, child, lenient, &format!("element <{other}>"))?;
            }
        }
    }

    Ok(XmlStruct {
        name: qualify(cx.scope, name),
        base_type: node
            .attribute("baseType")
            .or_else(|| node.attribute("baseClass"))
            .map(str::to_string),
        extensibility,
        autoid_hash,
        nested,
        data_representation,
        members,
    })
}

fn parse_enum(doc: &Document, node: Node, cx: Cx, lenient: bool) -> DdsResult<XmlEnum> {
    check_attrs(doc, node, ENUM_ATTRS, lenient)?;
    let name = required_attr(doc, node, "name")?;
    let bit_bound = match node.attribute("bit_bound").or_else(|| node.attribute("bitBound")) {
        None => 32,
        Some(v) => {
            let n = v
                .parse::<u16>()
                .map_err(|e| err_at(doc, node, &format!("invalid 'bit_bound' value '{v}': {e}")))?;
            if n == 0 || n > 32 {
                return Err(err_at(doc, node, "'bit_bound' must be between 1 and 32"));
            }
            n
        }
    };

    let mut literals = Vec::new();
    for child in node.children().filter(Node::is_element) {
        match child.tag_name().name() {
            "enumerator" => {
                check_attrs(doc, child, ENUMERATOR_ATTRS, lenient)?;
                let value = match child.attribute("value") {
                    None => None,
                    Some(v) => {
                        let n = resolve_int_token(cx, v)
                            .ok_or_else(|| err_at(doc, child, &format!("invalid 'value' '{v}'")))?;
                        Some(i32::try_from(n).map_err(|_| {
                            err_at(doc, child, &format!("'value' '{v}' out of i32 range"))
                        })?)
                    }
                };
                let is_default = bool_attr(doc, child, "default_literal")?
                    || bool_attr(doc, child, "defaultLiteral")?;
                literals.push(XmlEnumLiteral {
                    name: required_attr(doc, child, "name")?.to_string(),
                    value,
                    is_default,
                });
            }
            other => {
                skip_unknown(doc, child, lenient, &format!("element <{other}>"))?;
            }
        }
    }
    if literals.is_empty() {
        return Err(err_at(doc, node, "enum requires at least one enumerator"));
    }

    Ok(XmlEnum { name: qualify(cx.scope, name), bit_bound, literals })
}

fn parse_union(doc: &Document, node: Node, cx: Cx, lenient: bool) -> DdsResult<XmlUnion> {
    check_attrs(doc, node, UNION_ATTRS, lenient)?;
    let name = required_attr(doc, node, "name")?;
    let extensibility = extensibility_attr(doc, node)?;

    let mut discriminator = None;
    let mut cases = Vec::new();
    for child in node.children().filter(Node::is_element) {
        match child.tag_name().name() {
            "discriminator" => {
                if discriminator.is_some() {
                    return Err(err_at(doc, child, "union has more than one <discriminator>"));
                }
                check_attrs_split(doc, child, &[], lenient)?;
                let ty = parse_type_spec(doc, child, cx)?;
                if !is_valid_discriminator(&ty) {
                    return Err(err_at(
                        doc,
                        child,
                        "union discriminator must be an integral, boolean, char, or enum type",
                    ));
                }
                discriminator = Some(ty);
            }
            "case" => cases.push(parse_union_case(doc, child, cx, lenient)?),
            other => {
                skip_unknown(doc, child, lenient, &format!("element <{other}>"))?;
            }
        }
    }

    let discriminator =
        discriminator.ok_or_else(|| err_at(doc, node, "union requires a <discriminator>"))?;
    if cases.is_empty() {
        return Err(err_at(doc, node, "union requires at least one <case>"));
    }
    if cases.iter().filter(|c| c.is_default).count() > 1 {
        return Err(err_at(doc, node, "union has more than one default case"));
    }

    Ok(XmlUnion { name: qualify(cx.scope, name), extensibility, discriminator, cases })
}

fn parse_union_case(doc: &Document, node: Node, cx: Cx, lenient: bool) -> DdsResult<XmlUnionCase> {
    check_attrs(doc, node, &[], lenient)?;
    let mut labels = Vec::new();
    let mut is_default = false;
    let mut member = None;
    for child in node.children().filter(Node::is_element) {
        match child.tag_name().name() {
            "caseDiscriminator" => {
                check_attrs(doc, child, CASE_LABEL_ATTRS, lenient)?;
                let value = required_attr(doc, child, "value")?;
                if value == "default" {
                    is_default = true;
                } else if let Ok(n) = value.parse::<i32>() {
                    labels.push(XmlCaseLabel::Int(n));
                } else {
                    labels.push(XmlCaseLabel::Name(value.to_string()));
                }
            }
            "member" => {
                if member.is_some() {
                    return Err(err_at(doc, child, "union case has more than one <member>"));
                }
                member = Some(parse_member(doc, child, cx, lenient)?);
            }
            other => {
                skip_unknown(doc, child, lenient, &format!("element <{other}>"))?;
            }
        }
    }
    let member = member.ok_or_else(|| err_at(doc, node, "union case requires a <member>"))?;
    if labels.is_empty() && !is_default {
        return Err(err_at(doc, node, "union case requires a <caseDiscriminator>"));
    }
    Ok(XmlUnionCase { name: member.name, ty: member.ty, labels, is_default })
}

fn is_valid_discriminator(ty: &XmlMemberType) -> bool {
    match ty {
        XmlMemberType::NonBasic(_) => true,
        XmlMemberType::Primitive(id) => matches!(
            id,
            TypeIdentifier::Boolean
                | TypeIdentifier::Byte
                | TypeIdentifier::Char8
                | TypeIdentifier::Char16
                | TypeIdentifier::Int8
                | TypeIdentifier::Int16
                | TypeIdentifier::Int32
                | TypeIdentifier::Int64
                | TypeIdentifier::Uint8
                | TypeIdentifier::Uint16
                | TypeIdentifier::Uint32
                | TypeIdentifier::Uint64
        ),
        _ => false,
    }
}

fn parse_bitmask(doc: &Document, node: Node, cx: Cx, lenient: bool) -> DdsResult<XmlBitmask> {
    check_attrs(doc, node, BITMASK_ATTRS, lenient)?;
    let name = required_attr(doc, node, "name")?;
    let bit_bound = match node.attribute("bit_bound").or_else(|| node.attribute("bitBound")) {
        None => 32,
        Some(v) => {
            let n = v
                .parse::<u16>()
                .map_err(|e| err_at(doc, node, &format!("invalid 'bit_bound' value '{v}': {e}")))?;
            if n == 0 || n > 64 {
                return Err(err_at(doc, node, "'bit_bound' must be between 1 and 64"));
            }
            n
        }
    };

    let mut flags = Vec::new();
    for child in node.children().filter(Node::is_element) {
        match child.tag_name().name() {
            "bit_value" => {
                check_attrs(doc, child, BIT_VALUE_ATTRS, lenient)?;
                let fname = required_attr(doc, child, "name")?;
                let position = match child.attribute("position") {
                    None => u16::try_from(flags.len())
                        .map_err(|_| err_at(doc, child, "too many bit flags"))?,
                    Some(v) => v.parse::<u16>().map_err(|e| {
                        err_at(doc, child, &format!("invalid 'position' value '{v}': {e}"))
                    })?,
                };
                flags.push(XmlBitflag { name: fname.to_string(), position });
            }
            other => {
                skip_unknown(doc, child, lenient, &format!("element <{other}>"))?;
            }
        }
    }
    if flags.is_empty() {
        return Err(err_at(doc, node, "bitmask requires at least one <bit_value>"));
    }

    Ok(XmlBitmask { name: qualify(cx.scope, name), bit_bound, flags })
}

fn parse_bitset(doc: &Document, node: Node, cx: Cx, lenient: bool) -> DdsResult<XmlBitset> {
    check_attrs(doc, node, BITSET_ATTRS, lenient)?;
    let name = required_attr(doc, node, "name")?;

    let mut fields = Vec::new();
    for child in node.children().filter(Node::is_element) {
        match child.tag_name().name() {
            "bitfield" => {
                check_attrs(doc, child, BITFIELD_ATTRS, lenient)?;
                let bb = required_attr(doc, child, "bit_bound")
                    .or_else(|_| required_attr(doc, child, "bitBound"))?;
                let bitcount = bb.parse::<u8>().map_err(|e| {
                    err_at(doc, child, &format!("invalid 'bit_bound' value '{bb}': {e}"))
                })?;
                if bitcount == 0 || bitcount > 64 {
                    return Err(err_at(
                        doc,
                        child,
                        "bitfield 'bit_bound' must be between 1 and 64",
                    ));
                }
                let holder = match child.attribute("type") {
                    Some(t) => bitfield_holder(doc, child, t)?,
                    None => infer_holder(bitcount),
                };
                fields.push(XmlBitfield {
                    name: child.attribute("name").map(str::to_string),
                    bitcount,
                    holder,
                });
            }
            other => {
                skip_unknown(doc, child, lenient, &format!("element <{other}>"))?;
            }
        }
    }
    if fields.is_empty() {
        return Err(err_at(doc, node, "bitset requires at least one <bitfield>"));
    }

    Ok(XmlBitset { name: qualify(cx.scope, name), fields })
}

fn bitfield_holder(doc: &Document, node: Node, name: &str) -> DdsResult<TypeIdentifier> {
    match primitive_type_id(name) {
        Some(id)
            if matches!(
                id,
                TypeIdentifier::Boolean
                    | TypeIdentifier::Byte
                    | TypeIdentifier::Int8
                    | TypeIdentifier::Int16
                    | TypeIdentifier::Int32
                    | TypeIdentifier::Int64
                    | TypeIdentifier::Uint8
                    | TypeIdentifier::Uint16
                    | TypeIdentifier::Uint32
                    | TypeIdentifier::Uint64
            ) =>
        {
            Ok(id)
        }
        _ => Err(err_at(doc, node, &format!("bitfield holder type '{name}' must be integral"))),
    }
}

fn infer_holder(bitcount: u8) -> TypeIdentifier {
    match bitcount {
        1..=8 => TypeIdentifier::Uint8,
        9..=16 => TypeIdentifier::Uint16,
        17..=32 => TypeIdentifier::Uint32,
        _ => TypeIdentifier::Uint64,
    }
}

fn parse_typedef(doc: &Document, node: Node, cx: Cx, lenient: bool) -> DdsResult<XmlTypedef> {
    check_attrs_split(doc, node, &["name"], lenient)?;
    let name = required_attr(doc, node, "name")?;
    Ok(XmlTypedef { name: qualify(cx.scope, name), ty: parse_type_spec(doc, node, cx)? })
}

fn parse_member(doc: &Document, node: Node, cx: Cx, lenient: bool) -> DdsResult<XmlMember> {
    check_attrs_split(doc, node, MEMBER_ONLY_ATTRS, lenient)?;
    let name = required_attr(doc, node, "name")?;
    let ty = parse_type_spec(doc, node, cx)?;

    let id = match node.attribute("id") {
        Some(v) => {
            let id = v
                .parse::<u32>()
                .map_err(|e| err_at(doc, node, &format!("invalid 'id' value '{v}': {e}")))?;
            if id > MAX_MEMBER_ID {
                return Err(err_at(doc, node, &format!("id {id} exceeds 28-bit member id range")));
            }
            Some(id)
        }
        None => None,
    };
    let hashid = node.attribute("hashid").map(str::to_string);
    if id.is_some() && hashid.is_some() {
        return Err(err_at(doc, node, "'id' and 'hashid' cannot both be specified"));
    }
    let try_construct = match node.attribute("try_construct") {
        None | Some("discard") => TryConstructKind::Discard,
        Some("use_default") => TryConstructKind::UseDefault,
        Some("trim") => TryConstructKind::Trim,
        Some(v) => return Err(err_at(doc, node, &format!("invalid try_construct '{v}'"))),
    };

    Ok(XmlMember {
        name: name.to_string(),
        ty,
        id,
        hashid,
        key: bool_attr(doc, node, "key")?,
        optional: bool_attr(doc, node, "optional")?,
        external: bool_attr(doc, node, "external")?,
        must_understand: bool_attr(doc, node, "mustUnderstand")?,
        try_construct,
    })
}

fn parse_type_spec(doc: &Document, node: Node, cx: Cx) -> DdsResult<XmlMemberType> {
    let ty_name = required_attr(doc, node, "type")?;
    let string_bound = length_attr(doc, node, "stringMaxLength", cx)?;
    if string_bound.is_some() && !matches!(ty_name, "string" | "wstring") {
        return Err(err_at(doc, node, "'stringMaxLength' requires a string type"));
    }
    let non_basic_name = node.attribute("nonBasicTypeName");
    if non_basic_name.is_some() && ty_name != "nonBasic" {
        return Err(err_at(doc, node, "'nonBasicTypeName' requires type=\"nonBasic\""));
    }
    let scalar = match primitive_type_id(ty_name) {
        Some(id) => XmlMemberType::Primitive(id),
        None => match ty_name {
            "string" => XmlMemberType::String { bound: string_bound.flatten() },
            "wstring" => XmlMemberType::WString { bound: string_bound.flatten() },
            "nonBasic" => match non_basic_name {
                Some(target) => XmlMemberType::NonBasic(target.to_string()),
                None => {
                    return Err(err_at(doc, node, "type 'nonBasic' requires 'nonBasicTypeName'"))
                }
            },
            other => return Err(err_at(doc, node, &format!("unknown type '{other}'"))),
        },
    };

    let key_type = node.attribute("key_type").or_else(|| node.attribute("keyType"));
    let map_bound = length_attr(doc, node, "mapMaxLength", cx)?;
    let seq_bound = length_attr(doc, node, "sequenceMaxLength", cx)?;
    let dims = dims_attr(doc, node, cx)?;

    if let Some(key_name) = key_type {
        if seq_bound.is_some() || dims.is_some() {
            return Err(err_at(
                doc,
                node,
                "map cannot be combined with 'sequenceMaxLength' or 'arrayDimensions'",
            ));
        }
        let key = match key_name {
            "string" => XmlMemberType::String { bound: None },
            "wstring" => XmlMemberType::WString { bound: None },
            _ => match primitive_type_id(key_name) {
                Some(
                    TypeIdentifier::Float32 | TypeIdentifier::Float64 | TypeIdentifier::Float128,
                )
                | None => {
                    return Err(err_at(
                        doc,
                        node,
                        &format!("map key type '{key_name}' must be an integral type or string"),
                    ))
                }
                Some(id) => XmlMemberType::Primitive(id),
            },
        };
        return Ok(XmlMemberType::Map {
            key: Box::new(key),
            value: Box::new(scalar),
            bound: map_bound.flatten(),
        });
    }
    if map_bound.is_some() {
        return Err(err_at(doc, node, "'mapMaxLength' requires 'key_type'"));
    }

    // sequenceMaxLength + arrayDimensions on one member is an array of sequences.
    Ok(match (seq_bound, dims) {
        (Some(bound), Some(dims)) => XmlMemberType::Array {
            element: Box::new(XmlMemberType::Sequence { element: Box::new(scalar), bound }),
            dims,
        },
        (Some(bound), None) => XmlMemberType::Sequence { element: Box::new(scalar), bound },
        (None, Some(dims)) => XmlMemberType::Array { element: Box::new(scalar), dims },
        (None, None) => scalar,
    })
}

fn extensibility_attr(doc: &Document, node: Node) -> DdsResult<ExtensibilityKind> {
    Ok(match node.attribute("extensibility") {
        None | Some("appendable") => ExtensibilityKind::Appendable,
        Some("final") => ExtensibilityKind::Final,
        Some("mutable") => ExtensibilityKind::Mutable,
        Some(v) => return Err(err_at(doc, node, &format!("invalid extensibility '{v}'"))),
    })
}

fn data_representation_attr(doc: &Document, node: Node) -> DdsResult<Option<u16>> {
    let Some(v) = node.attribute("data_representation") else {
        return Ok(None);
    };
    let mut mask: u16 = 0;
    for token in v.split(['|', ',', ' ']).filter(|t| !t.is_empty()) {
        match token.to_ascii_lowercase().as_str() {
            "xcdr" | "xcdr1" => mask |= 1,
            "xml" => mask |= 1 << 1,
            "xcdr2" => mask |= 1 << 2,
            other => {
                return Err(err_at(
                    doc,
                    node,
                    &format!(
                        "invalid data_representation value '{other}', expected XCDR1 | XML | XCDR2"
                    ),
                ))
            }
        }
    }
    if mask == 0 {
        return Err(err_at(doc, node, "empty 'data_representation'"));
    }
    Ok(Some(mask))
}

// Outer Option = present; inner = bound (-1 → unbounded). Value may be a literal or const name.
fn length_attr(doc: &Document, node: Node, attr: &str, cx: Cx) -> DdsResult<Option<Option<u32>>> {
    let Some(raw) = node.attribute(attr) else {
        return Ok(None);
    };
    let Some(n) = resolve_int_token(cx, raw) else {
        return Err(err_at(doc, node, &format!("invalid '{attr}' value '{raw}'")));
    };
    if n == -1 {
        return Ok(Some(None));
    }
    match u32::try_from(n) {
        Ok(0) => Err(err_at(doc, node, &format!("'{attr}' must be positive or -1"))),
        Ok(bound) => Ok(Some(Some(bound))),
        Err(_) => Err(err_at(doc, node, &format!("invalid '{attr}' value '{raw}'"))),
    }
}

fn dims_attr(doc: &Document, node: Node, cx: Cx) -> DdsResult<Option<Vec<u32>>> {
    let Some(v) = node.attribute("arrayDimensions") else {
        return Ok(None);
    };
    let mut dims = Vec::new();
    for part in v.split(',') {
        let n = resolve_int_token(cx, part.trim())
            .ok_or_else(|| err_at(doc, node, &format!("invalid 'arrayDimensions' value '{v}'")))?;
        if n <= 0 {
            return Err(err_at(doc, node, "'arrayDimensions' entries must be positive"));
        }
        let dim = u32::try_from(n)
            .map_err(|_| err_at(doc, node, &format!("invalid 'arrayDimensions' value '{v}'")))?;
        dims.push(dim);
    }
    Ok(Some(dims))
}

fn qualify(scope: &str, name: &str) -> String {
    if scope.is_empty() {
        name.to_string()
    } else {
        format!("{scope}::{name}")
    }
}

// Pre-pass: gather every <const> into a scope-qualified table for later numeric refs.
fn collect_consts(
    doc: &Document,
    node: Node,
    scope: &str,
    table: &mut ConstTable,
    lenient: bool,
) -> DdsResult<()> {
    for child in node.children().filter(Node::is_element) {
        match child.tag_name().name() {
            "const" => {
                let (name, value) = parse_const(doc, child, scope, lenient)?;
                if table.contains_key(&name) {
                    return Err(err_at(doc, child, &format!("const '{name}' redefined")));
                }
                table.insert(name, value);
            }
            "module" => {
                if let Some(name) = child.attribute("name") {
                    collect_consts(doc, child, &qualify(scope, name), table, lenient)?;
                }
            }
            "dds" | "types" | "type" => collect_consts(doc, child, scope, table, lenient)?,
            _ => {}
        }
    }
    Ok(())
}

fn parse_const(
    doc: &Document,
    node: Node,
    scope: &str,
    lenient: bool,
) -> DdsResult<(String, ConstValue)> {
    check_attrs(doc, node, CONST_ATTRS, lenient)?;
    let name = required_attr(doc, node, "name")?;
    let ty = required_attr(doc, node, "type")?;
    let raw = required_attr(doc, node, "value")?;
    Ok((qualify(scope, name), parse_const_value(doc, node, ty, raw)?))
}

fn parse_const_value(doc: &Document, node: Node, ty: &str, raw: &str) -> DdsResult<ConstValue> {
    match ty {
        "boolean" => match raw {
            "true" | "1" | "false" | "0" => Ok(ConstValue::Other),
            _ => Err(err_at(doc, node, &format!("invalid boolean const value '{raw}'"))),
        },
        "string" | "wstring" => Ok(ConstValue::Other),
        "char8" | "char" | "char16" | "wchar" => {
            if raw.chars().count() == 1 {
                Ok(ConstValue::Other)
            } else {
                Err(err_at(
                    doc,
                    node,
                    &format!("char const value '{raw}' must be a single character"),
                ))
            }
        }
        _ => match primitive_type_id(ty) {
            Some(TypeIdentifier::Float32 | TypeIdentifier::Float64 | TypeIdentifier::Float128) => {
                raw.parse::<f64>().map(|_| ConstValue::Other).map_err(|e| {
                    err_at(doc, node, &format!("invalid float const value '{raw}': {e}"))
                })
            }
            Some(id) if is_integral(&id) => raw.parse::<i64>().map(ConstValue::Int).map_err(|e| {
                err_at(doc, node, &format!("invalid integer const value '{raw}': {e}"))
            }),
            _ => Err(err_at(doc, node, &format!("const type '{ty}' must be a primitive type"))),
        },
    }
}

fn is_integral(id: &TypeIdentifier) -> bool {
    matches!(
        id,
        TypeIdentifier::Byte
            | TypeIdentifier::Int8
            | TypeIdentifier::Int16
            | TypeIdentifier::Int32
            | TypeIdentifier::Int64
            | TypeIdentifier::Uint8
            | TypeIdentifier::Uint16
            | TypeIdentifier::Uint32
            | TypeIdentifier::Uint64
    )
}

// Resolve a const name from the current scope outward (IDL scoping).
fn lookup_const<'a>(name: &str, cx: Cx<'a>) -> Option<&'a ConstValue> {
    let mut scope = cx.scope;
    loop {
        let candidate =
            if scope.is_empty() { name.to_string() } else { format!("{scope}::{name}") };
        if let Some(v) = cx.consts.get(&candidate) {
            return Some(v);
        }
        if scope.is_empty() {
            return None;
        }
        scope = scope.rsplit_once("::").map_or("", |(s, _)| s);
    }
}

// An integer literal or a reference to an integer const; None if neither.
fn resolve_int_token(cx: Cx, raw: &str) -> Option<i64> {
    if let Ok(n) = raw.parse::<i64>() {
        return Some(n);
    }
    match lookup_const(raw, cx) {
        Some(ConstValue::Int(n)) => Some(*n),
        _ => None,
    }
}

fn check_attrs(doc: &Document, node: Node, allowed: &[&str], lenient: bool) -> DdsResult<()> {
    for attr in node.attributes() {
        if attr.namespace() == Some(I2_NS) {
            continue;
        }
        let name = attr.name();
        if allowed.contains(&name) {
            continue;
        }
        skip_unknown(doc, node, lenient, &format!("attribute '{name}'"))?;
    }
    Ok(())
}

fn check_attrs_split(doc: &Document, node: Node, own: &[&str], lenient: bool) -> DdsResult<()> {
    for attr in node.attributes() {
        if attr.namespace() == Some(I2_NS) {
            continue;
        }
        let name = attr.name();
        if own.contains(&name) || TYPE_SPEC_ATTRS.contains(&name) {
            continue;
        }
        skip_unknown(doc, node, lenient, &format!("attribute '{name}'"))?;
    }
    Ok(())
}

// In lenient mode unknown vocabulary is skipped with a warning; returns true when skipped.
fn skip_unknown(doc: &Document, node: Node, lenient: bool, what: &str) -> DdsResult<bool> {
    if lenient {
        warn_at(doc, node, &format!("ignoring unknown {what}"));
        Ok(true)
    } else {
        Err(err_at(doc, node, &format!("unknown {what}")))
    }
}

fn bool_attr(doc: &Document, node: Node, attr: &str) -> DdsResult<bool> {
    match node.attribute(attr) {
        None => Ok(false),
        Some("true") | Some("1") => Ok(true),
        Some("false") | Some("0") => Ok(false),
        Some(v) => Err(err_at(doc, node, &format!("invalid '{attr}' value '{v}'"))),
    }
}

fn required_attr<'a>(doc: &Document, node: Node<'a, '_>, name: &str) -> DdsResult<&'a str> {
    node.attribute(name).ok_or_else(|| err_at(doc, node, &format!("missing attribute '{name}'")))
}

fn err_at(doc: &Document, node: Node, msg: &str) -> DdsError {
    let pos = doc.text_pos_at(node.range().start);
    DdsError::Error(format!("XML types ({}:{}): {msg}", pos.row, pos.col))
}

fn warn_at(doc: &Document, node: Node, msg: &str) {
    let pos = doc.text_pos_at(node.range().start);
    log::warn!("XML types ({}:{}): {msg}", pos.row, pos.col);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_one(xml: &str) -> XmlStruct {
        let mut decls = parse_types(xml, false).unwrap();
        assert_eq!(decls.len(), 1);
        match decls.remove(0) {
            XmlTypeDecl::Struct(s) => s,
            other => panic!("expected struct, got {other:?}"),
        }
    }

    fn err_msg(xml: &str) -> String {
        match parse_types(xml, false) {
            Err(DdsError::Error(msg)) => msg,
            other => panic!("expected error, got {other:?}"),
        }
    }

    #[test]
    fn primitive_names_map() {
        let cases = [
            ("boolean", TypeIdentifier::Boolean),
            ("byte", TypeIdentifier::Byte),
            ("char8", TypeIdentifier::Char8),
            ("char16", TypeIdentifier::Char16),
            ("int8", TypeIdentifier::Int8),
            ("int16", TypeIdentifier::Int16),
            ("int32", TypeIdentifier::Int32),
            ("int64", TypeIdentifier::Int64),
            ("uint8", TypeIdentifier::Uint8),
            ("uint16", TypeIdentifier::Uint16),
            ("uint32", TypeIdentifier::Uint32),
            ("uint64", TypeIdentifier::Uint64),
            ("float32", TypeIdentifier::Float32),
            ("float64", TypeIdentifier::Float64),
            ("float128", TypeIdentifier::Float128),
            // IDL-style vendor aliases
            ("octet", TypeIdentifier::Byte),
            ("char", TypeIdentifier::Char8),
            ("wchar", TypeIdentifier::Char16),
            ("short", TypeIdentifier::Int16),
            ("long", TypeIdentifier::Int32),
            ("longLong", TypeIdentifier::Int64),
            ("unsignedShort", TypeIdentifier::Uint16),
            ("unsignedLong", TypeIdentifier::Uint32),
            ("unsignedLongLong", TypeIdentifier::Uint64),
            ("float", TypeIdentifier::Float32),
            ("double", TypeIdentifier::Float64),
            ("longDouble", TypeIdentifier::Float128),
        ];
        for (name, expected) in cases {
            assert_eq!(primitive_type_id(name), Some(expected), "{name}");
        }
        assert_eq!(primitive_type_id("int"), None);
    }

    #[test]
    fn root_forms_equivalent() {
        let body = r#"<struct name="T"><member name="x" type="int32"/></struct>"#;
        let plain = format!("<types>{body}</types>");
        let dds = format!("<dds><types>{body}</types></dds>");
        let wrapped = format!("<types><type>{body}</type></types>");
        for xml in [&plain, &dds, &wrapped] {
            let s = parse_one(xml);
            assert_eq!(s.name, "T");
            assert_eq!(s.members.len(), 1);
        }
    }

    #[test]
    fn dds_root_skips_reserved_sections() {
        let s = parse_one(
            r#"<dds>
                 <qos_library name="L"><qos_profile name="P"/></qos_library>
                 <profiles><participant profile_name="p"/></profiles>
                 <log><use_default>true</use_default></log>
                 <library_settings><intraprocess_delivery>FULL</intraprocess_delivery></library_settings>
                 <types><struct name="T"><member name="x" type="int32"/></struct></types>
               </dds>"#,
        );
        assert_eq!(s.name, "T");
    }

    #[test]
    fn nested_modules_qualify_name() {
        let s = parse_one(
            r#"<types><module name="a"><module name="b">
                 <struct name="T"><member name="x" type="int32"/></struct>
               </module></module></types>"#,
        );
        assert_eq!(s.name, "a::b::T");
    }

    #[test]
    fn annotations_land_in_ast() {
        let s = parse_one(
            r#"<types>
                 <struct name="T" baseType="P" extensibility="mutable" autoid="hash" nested="1">
                   <member name="a" type="uint32" id="10" key="true"/>
                   <member name="b" type="int32" hashid="crc" optional="true"
                           external="true" mustUnderstand="true" try_construct="trim"/>
                   <member name="c" type="int32" hashid=""/>
                 </struct>
               </types>"#,
        );
        assert_eq!(s.base_type.as_deref(), Some("P"));
        assert_eq!(s.extensibility, ExtensibilityKind::Mutable);
        assert!(s.autoid_hash);
        assert!(s.nested);
        let a = &s.members[0];
        assert_eq!(a.id, Some(10));
        assert!(a.key);
        let b = &s.members[1];
        assert_eq!(b.hashid.as_deref(), Some("crc"));
        assert!(b.optional && b.external && b.must_understand);
        assert_eq!(b.try_construct, TryConstructKind::Trim);
        assert_eq!(s.members[2].hashid.as_deref(), Some(""));
    }

    #[test]
    fn data_representation_masks() {
        let cases = [
            ("xcdr1", 0b001),
            ("XCDR2", 0b100),
            ("xcdr|xml|xcdr2", 0b111),
            ("xcdr1, xcdr2", 0b101),
        ];
        for (attr, mask) in cases {
            let s = parse_one(&format!(
                r#"<types><struct name="T" data_representation="{attr}">
                     <member name="x" type="int32"/>
                   </struct></types>"#
            ));
            assert_eq!(s.data_representation, Some(mask), "{attr}");
        }
        let s = parse_one(
            r#"<types><struct name="T"><member name="x" type="int32"/></struct></types>"#,
        );
        assert_eq!(s.data_representation, None);
    }

    #[test]
    fn strings_and_collections_land_in_ast() {
        let s = parse_one(
            r#"<types>
                 <struct name="T">
                   <member name="a" type="string"/>
                   <member name="b" type="string" stringMaxLength="128"/>
                   <member name="c" type="wstring" stringMaxLength="-1"/>
                   <member name="d" type="int32" sequenceMaxLength="-1"/>
                   <member name="e" type="string" sequenceMaxLength="10"/>
                   <member name="f" type="float64" arrayDimensions="4"/>
                   <member name="g" type="int32" arrayDimensions="2, 3"/>
                 </struct>
               </types>"#,
        );
        assert!(matches!(s.members[0].ty, XmlMemberType::String { bound: None }));
        assert!(matches!(s.members[1].ty, XmlMemberType::String { bound: Some(128) }));
        assert!(matches!(s.members[2].ty, XmlMemberType::WString { bound: None }));
        match &s.members[3].ty {
            XmlMemberType::Sequence { element, bound: None } => {
                assert!(matches!(**element, XmlMemberType::Primitive(TypeIdentifier::Int32)))
            }
            other => panic!("expected unbounded sequence, got {other:?}"),
        }
        match &s.members[4].ty {
            XmlMemberType::Sequence { element, bound: Some(10) } => {
                assert!(matches!(**element, XmlMemberType::String { bound: None }))
            }
            other => panic!("expected bounded string sequence, got {other:?}"),
        }
        match &s.members[5].ty {
            XmlMemberType::Array { element, dims } => {
                assert!(matches!(**element, XmlMemberType::Primitive(TypeIdentifier::Float64)));
                assert_eq!(dims, &[4]);
            }
            other => panic!("expected array, got {other:?}"),
        }
        match &s.members[6].ty {
            XmlMemberType::Array { dims, .. } => assert_eq!(dims, &[2, 3]),
            other => panic!("expected 2-dim array, got {other:?}"),
        }
    }

    #[test]
    fn array_of_sequences_lands_in_ast() {
        let s = parse_one(
            r#"<types><struct name="T">
                 <member name="a" type="int32" sequenceMaxLength="8" arrayDimensions="2,3"/>
               </struct></types>"#,
        );
        match &s.members[0].ty {
            XmlMemberType::Array { element, dims } => {
                assert_eq!(dims, &[2, 3]);
                assert_eq!(
                    **element,
                    XmlMemberType::Sequence {
                        element: Box::new(XmlMemberType::Primitive(TypeIdentifier::Int32)),
                        bound: Some(8),
                    }
                );
            }
            other => panic!("expected array of sequences, got {other:?}"),
        }
    }

    #[test]
    fn map_lands_in_ast() {
        let s = parse_one(
            r#"<types><struct name="T">
                 <member name="a" type="int32" key_type="string" mapMaxLength="16"/>
                 <member name="b" type="string" keyType="uint64"/>
                 <member name="c" type="nonBasic" nonBasicTypeName="P" key_type="int32" mapMaxLength="-1"/>
               </struct></types>"#,
        );
        assert_eq!(
            s.members[0].ty,
            XmlMemberType::Map {
                key: Box::new(XmlMemberType::String { bound: None }),
                value: Box::new(XmlMemberType::Primitive(TypeIdentifier::Int32)),
                bound: Some(16),
            }
        );
        assert_eq!(
            s.members[1].ty,
            XmlMemberType::Map {
                key: Box::new(XmlMemberType::Primitive(TypeIdentifier::Uint64)),
                value: Box::new(XmlMemberType::String { bound: None }),
                bound: None,
            }
        );
        assert_eq!(
            s.members[2].ty,
            XmlMemberType::Map {
                key: Box::new(XmlMemberType::Primitive(TypeIdentifier::Int32)),
                value: Box::new(XmlMemberType::NonBasic("P".to_string())),
                bound: None,
            }
        );
    }

    #[test]
    fn typedef_lands_in_ast() {
        let decls = parse_types(
            r#"<types>
                 <typedef name="Vec3" type="float64" arrayDimensions="3"/>
                 <module name="m"><typedef name="Ids" type="uint32" sequenceMaxLength="-1"/></module>
               </types>"#,
            false,
        )
        .unwrap();
        let XmlTypeDecl::Typedef(t) = &decls[0] else { panic!("expected typedef") };
        assert_eq!(t.name, "Vec3");
        assert_eq!(
            t.ty,
            XmlMemberType::Array {
                element: Box::new(XmlMemberType::Primitive(TypeIdentifier::Float64)),
                dims: vec![3],
            }
        );
        let XmlTypeDecl::Typedef(t) = &decls[1] else { panic!("expected typedef") };
        assert_eq!(t.name, "m::Ids");
    }

    #[test]
    fn enum_and_nonbasic_land_in_ast() {
        let decls = parse_types(
            r#"<types>
                 <enum name="Color">
                   <enumerator name="RED"/>
                   <enumerator name="GREEN" value="5"/>
                   <enumerator name="BLUE" default_literal="true"/>
                 </enum>
                 <module name="geo">
                   <enum name="Axis" bit_bound="8"><enumerator name="X"/></enum>
                 </module>
                 <struct name="Holder">
                   <member name="color" type="nonBasic" nonBasicTypeName="Color"/>
                   <member name="colors" type="nonBasic" nonBasicTypeName="Color" sequenceMaxLength="-1"/>
                   <member name="grid" type="nonBasic" nonBasicTypeName="geo::Axis" arrayDimensions="2"/>
                 </struct>
               </types>"#,
            false,
        )
        .unwrap();
        assert_eq!(decls.len(), 3);
        let XmlTypeDecl::Enum(color) = &decls[0] else { panic!("expected enum") };
        assert_eq!(color.name, "Color");
        assert_eq!(color.bit_bound, 32);
        assert_eq!(color.literals.len(), 3);
        assert_eq!(color.literals[0].value, None);
        assert_eq!(color.literals[1].value, Some(5));
        assert!(color.literals[2].is_default);
        let XmlTypeDecl::Enum(axis) = &decls[1] else { panic!("expected enum") };
        assert_eq!(axis.name, "geo::Axis");
        assert_eq!(axis.bit_bound, 8);
        let XmlTypeDecl::Struct(holder) = &decls[2] else { panic!("expected struct") };
        assert!(matches!(&holder.members[0].ty, XmlMemberType::NonBasic(n) if n == "Color"));
        let mut refs = Vec::new();
        holder.members[1].ty.referenced_names(&mut refs);
        holder.members[2].ty.referenced_names(&mut refs);
        assert_eq!(refs, ["Color", "geo::Axis"]);
    }

    #[test]
    fn union_lands_in_ast() {
        let decls = parse_types(
            r#"<types><union name="Cmd" extensibility="appendable">
                 <discriminator type="int32"/>
                 <case>
                   <caseDiscriminator value="0"/>
                   <caseDiscriminator value="1"/>
                   <member name="a" type="uint32"/>
                 </case>
                 <case>
                   <caseDiscriminator value="default"/>
                   <member name="b" type="float64"/>
                 </case>
               </union></types>"#,
            false,
        )
        .unwrap();
        let XmlTypeDecl::Union(u) = &decls[0] else { panic!("expected union") };
        assert_eq!(u.name, "Cmd");
        assert_eq!(u.extensibility, ExtensibilityKind::Appendable);
        assert!(matches!(u.discriminator, XmlMemberType::Primitive(TypeIdentifier::Int32)));
        assert_eq!(u.cases.len(), 2);
        assert_eq!(u.cases[0].name, "a");
        assert_eq!(u.cases[0].labels.len(), 2);
        assert!(!u.cases[0].is_default);
        assert!(matches!(u.cases[0].labels[0], XmlCaseLabel::Int(0)));
        assert!(matches!(u.cases[0].labels[1], XmlCaseLabel::Int(1)));
        assert_eq!(u.cases[1].name, "b");
        assert!(u.cases[1].is_default);
        assert!(u.cases[1].labels.is_empty());
    }

    #[test]
    fn union_enum_discriminator_named_labels() {
        let decls = parse_types(
            r#"<types><union name="U">
                 <discriminator type="nonBasic" nonBasicTypeName="Mode"/>
                 <case><caseDiscriminator value="IDLE"/><member name="m" type="int32"/></case>
               </union></types>"#,
            false,
        )
        .unwrap();
        let XmlTypeDecl::Union(u) = &decls[0] else { panic!("expected union") };
        assert!(matches!(&u.discriminator, XmlMemberType::NonBasic(n) if n == "Mode"));
        assert!(matches!(&u.cases[0].labels[0], XmlCaseLabel::Name(n) if n == "IDLE"));
    }

    #[test]
    fn bitmask_lands_in_ast() {
        let decls = parse_types(
            r#"<types><bitmask name="Flags" bit_bound="8">
                 <bit_value name="A"/>
                 <bit_value name="B" position="3"/>
                 <bit_value name="C"/>
               </bitmask></types>"#,
            false,
        )
        .unwrap();
        let XmlTypeDecl::Bitmask(b) = &decls[0] else { panic!("expected bitmask") };
        assert_eq!(b.name, "Flags");
        assert_eq!(b.bit_bound, 8);
        assert_eq!(b.flags[0].position, 0);
        assert_eq!(b.flags[1].position, 3);
        assert_eq!(b.flags[2].position, 2);
    }

    #[test]
    fn bitset_lands_in_ast() {
        let decls = parse_types(
            r#"<types><bitset name="Hdr">
                 <bitfield name="version" bit_bound="4" type="uint8"/>
                 <bitfield bit_bound="4"/>
                 <bitfield name="flags" bit_bound="8" type="uint16"/>
               </bitset></types>"#,
            false,
        )
        .unwrap();
        let XmlTypeDecl::Bitset(b) = &decls[0] else { panic!("expected bitset") };
        assert_eq!(b.fields.len(), 3);
        assert_eq!(b.fields[0].name.as_deref(), Some("version"));
        assert_eq!(b.fields[0].bitcount, 4);
        assert_eq!(b.fields[0].holder, TypeIdentifier::Uint8);
        assert_eq!(b.fields[1].name, None);
        assert_eq!(b.fields[1].holder, TypeIdentifier::Uint8);
        assert_eq!(b.fields[2].name.as_deref(), Some("flags"));
        assert_eq!(b.fields[2].holder, TypeIdentifier::Uint16);
    }

    #[test]
    fn valuetype_maps_to_struct_with_base() {
        let s = parse_one(
            r#"<types><valuetype name="Derived" baseClass="Base">
                 <member name="x" type="int32" visibility="public"/>
               </valuetype></types>"#,
        );
        assert_eq!(s.name, "Derived");
        assert_eq!(s.base_type.as_deref(), Some("Base"));
        assert_eq!(s.members.len(), 1);
        assert_eq!(s.members[0].name, "x");
    }

    #[test]
    fn vendor_aliases_accepted() {
        let decls = parse_types(
            r#"<types>
                 <enum name="E" bitBound="16">
                   <enumerator name="A" defaultLiteral="true"/>
                 </enum>
               </types>"#,
            false,
        )
        .unwrap();
        let XmlTypeDecl::Enum(e) = &decls[0] else { panic!("expected enum") };
        assert_eq!(e.bit_bound, 16);
        assert!(e.literals[0].is_default);
    }

    #[test]
    fn lenient_skips_unknown_vocabulary() {
        let xml = r#"<dds>
             <unknown_section/>
             <types>
               <widget/>
               <const name="X" type="int32" value="1"/>
               <struct name="T" useVector="true">
                 <member name="x" type="int32" transferMode="shmem"/>
               </struct>
             </types>
           </dds>"#;
        assert!(parse_types(xml, false).is_err());
        let decls = parse_types(xml, true).unwrap();
        assert_eq!(decls.len(), 1);
        assert_eq!(decls[0].name(), "T");
    }

    #[test]
    fn i2_namespace_attrs_skipped() {
        let s = parse_one(
            r#"<types xmlns:i2="urn:int2dds:xml">
                 <struct name="T" i2:future="x"><member name="a" type="int32" i2:hint="y"/></struct>
               </types>"#,
        );
        assert_eq!(s.name, "T");
    }

    #[test]
    fn const_resolves_in_bounds_and_dims() {
        let s = parse_one(
            r#"<types>
                 <const name="MAX" type="int32" value="8"/>
                 <const name="DIM" type="uint32" value="3"/>
                 <const name="ANY" type="long" value="-1"/>
                 <struct name="T">
                   <member name="a" type="int32" sequenceMaxLength="MAX"/>
                   <member name="b" type="float64" arrayDimensions="DIM, 2"/>
                   <member name="c" type="int32" sequenceMaxLength="ANY"/>
                 </struct>
               </types>"#,
        );
        assert!(matches!(&s.members[0].ty, XmlMemberType::Sequence { bound: Some(8), .. }));
        match &s.members[1].ty {
            XmlMemberType::Array { dims, .. } => assert_eq!(dims, &[3, 2]),
            other => panic!("expected array, got {other:?}"),
        }
        assert!(matches!(&s.members[2].ty, XmlMemberType::Sequence { bound: None, .. }));
    }

    #[test]
    fn const_resolves_in_enum_value() {
        let decls = parse_types(
            r#"<types>
                 <const name="BASE" type="int32" value="100"/>
                 <enum name="E"><enumerator name="A" value="BASE"/><enumerator name="B"/></enum>
               </types>"#,
            false,
        )
        .unwrap();
        let XmlTypeDecl::Enum(e) = &decls[0] else { panic!("expected enum") };
        assert_eq!(e.literals[0].value, Some(100));
        assert_eq!(e.literals[1].value, None);
    }

    #[test]
    fn const_scoped_within_module() {
        let s = parse_one(
            r#"<types><module name="m">
                 <const name="N" type="int32" value="5"/>
                 <struct name="T"><member name="a" type="int32" sequenceMaxLength="N"/></struct>
               </module></types>"#,
        );
        assert_eq!(s.name, "m::T");
        assert!(matches!(&s.members[0].ty, XmlMemberType::Sequence { bound: Some(5), .. }));
    }

    #[test]
    fn includes_collected_from_anywhere() {
        let files = parse_includes(
            r#"<dds>
                 <include file="a.xml"/>
                 <types>
                   <include file="b.xml"/>
                   <module name="m"><include file="c.xml"/></module>
                   <struct name="T"><member name="x" type="int32"/></struct>
                 </types>
               </dds>"#,
            false,
        )
        .unwrap();
        assert_eq!(files, ["a.xml", "b.xml", "c.xml"]);
        // The same document still parses into decls without erroring on <include>.
        let decls = parse_types(
            r#"<types><include file="a.xml"/><struct name="T"><member name="x" type="int32"/></struct></types>"#,
            false,
        )
        .unwrap();
        assert_eq!(decls.len(), 1);
    }

    #[test]
    fn forward_dcl_accepted_and_ignored() {
        let decls = parse_types(
            r#"<types>
                 <forward_dcl name="Node" kind="struct"/>
                 <struct name="Node"><member name="x" type="int32"/></struct>
               </types>"#,
            false,
        )
        .unwrap();
        assert_eq!(decls.len(), 1);
        assert_eq!(decls[0].name(), "Node");
    }

    #[test]
    fn error_cases() {
        let cases = [
            (
                r#"<types><struct name="T" foo="1"><member name="a" type="int32"/></struct></types>"#,
                "unknown attribute 'foo'",
            ),
            (r#"<types><widget/></types>"#, "unknown element <widget>"),
            (r#"<profiles/>"#, "expected <dds> or <types> root"),
            (
                r#"<types><struct name="T"><member name="a" type="int32" id="4294967296"/></struct></types>"#,
                "invalid 'id' value",
            ),
            (
                r#"<types><struct name="T"><member name="a" type="int32" id="268435456"/></struct></types>"#,
                "exceeds 28-bit",
            ),
            (
                r#"<types><struct name="T"><member name="a" type="int32" id="1" hashid="x"/></struct></types>"#,
                "'id' and 'hashid' cannot both be specified",
            ),
            (
                r#"<types><struct name="T"><member name="a" type="nonBasic"/></struct></types>"#,
                "type 'nonBasic' requires 'nonBasicTypeName'",
            ),
            (
                r#"<types><struct name="T"><member name="a" type="int32" nonBasicTypeName="P"/></struct></types>"#,
                "'nonBasicTypeName' requires type=\"nonBasic\"",
            ),
            (r#"<types><enum name="E"/></types>"#, "enum requires at least one enumerator"),
            (
                r#"<types><enum name="E" bit_bound="33"><enumerator name="A"/></enum></types>"#,
                "'bit_bound' must be between 1 and 32",
            ),
            (
                r#"<types><enum name="E"><enumerator name="A" value="x"/></enum></types>"#,
                "invalid 'value'",
            ),
            (
                r#"<types><struct name="T"><member name="a" type="int32" stringMaxLength="8"/></struct></types>"#,
                "'stringMaxLength' requires a string type",
            ),
            (
                r#"<types><struct name="T"><member name="a" type="string" stringMaxLength="0"/></struct></types>"#,
                "'stringMaxLength' must be positive or -1",
            ),
            (
                r#"<types><struct name="T"><member name="a" type="int32" sequenceMaxLength="4294967296"/></struct></types>"#,
                "invalid 'sequenceMaxLength' value",
            ),
            (
                r#"<types><struct name="T"><member name="a" type="int32" arrayDimensions="2,0"/></struct></types>"#,
                "'arrayDimensions' entries must be positive",
            ),
            (
                r#"<types><struct name="T"><member name="a" type="int32" arrayDimensions=""/></struct></types>"#,
                "invalid 'arrayDimensions' value",
            ),
            (
                r#"<types><struct name="T"><member name="a" type="int32" mapMaxLength="2"/></struct></types>"#,
                "'mapMaxLength' requires 'key_type'",
            ),
            (
                r#"<types><struct name="T"><member name="a" type="int32" key_type="int32" sequenceMaxLength="2"/></struct></types>"#,
                "map cannot be combined",
            ),
            (
                r#"<types><struct name="T"><member name="a" type="int32" key_type="float32"/></struct></types>"#,
                "map key type 'float32' must be an integral type or string",
            ),
            (
                r#"<types><struct name="T" data_representation="xcdr3"><member name="a" type="int32"/></struct></types>"#,
                "invalid data_representation value 'xcdr3'",
            ),
            (
                r#"<types><struct name="T" data_representation=""><member name="a" type="int32"/></struct></types>"#,
                "empty 'data_representation'",
            ),
            (r#"<types><include/></types>"#, "missing attribute 'file'"),
            (
                r#"<types><const name="X" type="int32" value="1"/><const name="X" type="int32" value="2"/></types>"#,
                "const 'X' redefined",
            ),
            (
                r#"<types><struct name="T"><member name="a" type="int32" sequenceMaxLength="NOPE"/></struct></types>"#,
                "invalid 'sequenceMaxLength' value",
            ),
            (
                r#"<types><const name="X" type="float64" value="1.5"/><enum name="E"><enumerator name="A" value="X"/></enum></types>"#,
                "invalid 'value'",
            ),
            (r#"<types><bitmask name="B"/></types>"#, "bitmask requires at least one <bit_value>"),
            (
                r#"<types><bitset name="S"><bitfield name="a" bit_bound="4" type="float32"/></bitset></types>"#,
                "bitfield holder type 'float32' must be integral",
            ),
            (
                r#"<types><bitset name="S"><bitfield name="a" bit_bound="65" type="uint64"/></bitset></types>"#,
                "bitfield 'bit_bound' must be between 1 and 64",
            ),
            (r#"<types><union name="U"/></types>"#, "union requires a <discriminator>"),
            (
                r#"<types><union name="U"><discriminator type="float64"/><case><caseDiscriminator value="0"/><member name="m" type="int32"/></case></union></types>"#,
                "discriminator must be an integral",
            ),
            (
                r#"<types><union name="U"><discriminator type="int32"/></union></types>"#,
                "union requires at least one <case>",
            ),
            (
                r#"<types><union name="U"><discriminator type="int32"/><case><member name="m" type="int32"/></case></union></types>"#,
                "union case requires a <caseDiscriminator>",
            ),
            (
                r#"<types><union name="U"><discriminator type="int32"/><case><caseDiscriminator value="0"/></case></union></types>"#,
                "union case requires a <member>",
            ),
            (
                r#"<types><union name="U"><discriminator type="int32"/><case><caseDiscriminator value="default"/><member name="a" type="int32"/></case><case><caseDiscriminator value="default"/><member name="b" type="int32"/></case></union></types>"#,
                "more than one default case",
            ),
            (
                r#"<types><struct name="T"><member name="a" type="int32" key="yes"/></struct></types>"#,
                "invalid 'key' value 'yes'",
            ),
        ];
        for (xml, expected) in cases {
            let msg = err_msg(xml);
            assert!(msg.contains(expected), "expected '{expected}' in '{msg}'");
            assert!(msg.starts_with("XML types ("), "missing position prefix: '{msg}'");
        }
    }
}
