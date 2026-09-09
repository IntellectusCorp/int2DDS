/// Internal representation (IR) for resolved IDL types.
use std::collections::{HashMap, HashSet};

/// Fully resolved type information.
#[derive(Debug, Clone)]
pub enum ResolvedType {
    Bool,
    /// `octet` — maps to TK_BYTE (FIELD_BYTE). Serializes as a raw byte. Distinct XTypes
    /// kind from `UInt8`: octet and uint8 are NOT assignable, so they must stay separate.
    U8,
    /// `uint8` — maps to TK_UINT8 (FIELD_UINT8). Byte-identical CDR wire to `U8`, but a
    /// distinct TypeObject kind (and `#[dds(uint8)]` in generated Rust).
    UInt8,
    I8,
    I16,
    U16,
    I32,
    U32,
    I64,
    U64,
    F32,
    F64,
    Char,
    WChar,
    String {
        bound: Option<u32>,
    },
    WString {
        bound: Option<u32>,
    },
    Sequence {
        element: Box<ResolvedType>,
        bound: Option<u32>,
    },
    Array {
        element: Box<ResolvedType>,
        size: u32,
    },
    Map {
        key: Box<ResolvedType>,
        value: Box<ResolvedType>,
        bound: Option<u32>,
    },
    Struct(String),
    Enum(String),
    Bitmask(String),
}

/// Constant value for @default annotation and `const` declarations.
#[derive(Debug, Clone)]
pub enum ConstValue {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Ident(String),
}

/// A resolved `const` declaration.
#[derive(Debug, Clone)]
pub struct ResolvedConst {
    pub name: String,
    pub qualified_name: String,
    pub resolved_type: ResolvedType,
    pub value: ConstValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoIdKind {
    Sequential,
    Hash,
}

#[derive(Debug, Clone)]
pub struct ResolvedStruct {
    pub name: String,
    pub qualified_name: String,
    pub extensibility: ExtensibilityKind,
    pub autoid: Option<AutoIdKind>,
    pub base_type: Option<String>,
    pub members: Vec<ResolvedMember>,
}

#[derive(Debug, Clone)]
pub struct ResolvedMember {
    pub name: String,
    pub resolved_type: ResolvedType,
    pub is_key: bool,
    pub member_id: Option<u32>,
    pub is_optional: bool,
    pub must_understand: bool,
    pub is_external: bool,
    pub default_value: Option<ConstValue>,
    pub hashid: Option<Option<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExtensibilityKind {
    Final,
    #[default]
    Appendable,
    Mutable,
}

#[derive(Debug, Clone)]
pub struct ResolvedEnum {
    pub name: String,
    pub qualified_name: String,
    pub variants: Vec<ResolvedEnumVariant>,
}

#[derive(Debug, Clone)]
pub struct ResolvedEnumVariant {
    pub name: String,
    pub value: i32,
}

#[derive(Debug, Clone)]
pub struct ResolvedBitmask {
    pub name: String,
    pub qualified_name: String,
    pub bit_bound: u32,
    pub flags: Vec<ResolvedBitmaskFlag>,
}

#[derive(Debug, Clone)]
pub struct ResolvedBitmaskFlag {
    pub name: String,
    pub position: u32,
}

#[derive(Debug, Clone)]
pub struct ResolvedBitset {
    pub name: String,
    pub qualified_name: String,
    pub fields: Vec<ResolvedBitsetField>,
    pub total_bits: u32,
}

#[derive(Debug, Clone)]
pub struct ResolvedBitsetField {
    pub name: String,
    pub bit_width: u32,
}

#[derive(Debug, Clone)]
pub struct ResolvedUnion {
    pub name: String,
    pub qualified_name: String,
    pub discriminant_type: ResolvedType,
    pub cases: Vec<ResolvedUnionCase>,
    pub default_case: Option<ResolvedUnionCaseMember>,
    pub extensibility: ExtensibilityKind,
}

#[derive(Debug, Clone)]
pub struct ResolvedUnionCase {
    pub labels: Vec<ResolvedUnionLabel>,
    pub member: ResolvedUnionCaseMember,
}

#[derive(Debug, Clone)]
pub enum ResolvedUnionLabel {
    Int(i64),
    Bool(bool),
    Ident(String),
}

#[derive(Debug, Clone)]
pub struct ResolvedUnionCaseMember {
    pub name: String,
    pub resolved_type: ResolvedType,
}

#[derive(Debug, Clone)]
pub struct ResolvedInterface {
    pub name: String,
    pub qualified_name: String,
    pub base_interfaces: Vec<String>,
    pub operations: Vec<ResolvedOperation>,
    pub attributes: Vec<ResolvedAttribute>,
}

#[derive(Debug, Clone)]
pub struct ResolvedOperation {
    pub name: String,
    pub return_type: Option<ResolvedType>, // None = void
    pub params: Vec<ResolvedParam>,
    pub raises: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedParam {
    pub name: String,
    pub resolved_type: ResolvedType,
    pub direction: ResolvedParamDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedParamDirection {
    In,
    Out,
    Inout,
}

#[derive(Debug, Clone)]
pub struct ResolvedAttribute {
    pub name: String,
    pub resolved_type: ResolvedType,
    pub readonly: bool,
    pub raises: Vec<String>,
}

/// Resolved exception definition.
#[derive(Debug, Clone)]
pub struct ResolvedException {
    pub name: String,
    pub qualified_name: String,
    pub members: Vec<ResolvedMember>,
}

/// Types pulled in from `#include`d files: resolved so cross-file references
/// work, but not emitted into this file's output (their own file emits them).
/// Codegen consults these to recurse into nested-struct key fields and to emit
/// the language-level import for a referenced imported type. `modules` maps a
/// type's qualified and leaf name to the output module basename it lives in
/// (populated by the CLI, which knows each included file's origin).
#[derive(Debug, Clone, Default)]
pub struct ImportedTypes {
    pub structs: Vec<ResolvedStruct>,
    pub enums: Vec<ResolvedEnum>,
    pub bitmasks: Vec<ResolvedBitmask>,
    pub modules: HashMap<String, String>,
}

/// Complete resolved IDL model.
#[derive(Debug, Clone)]
pub struct IdlModel {
    pub structs: Vec<ResolvedStruct>,
    pub enums: Vec<ResolvedEnum>,
    pub bitmasks: Vec<ResolvedBitmask>,
    pub bitsets: Vec<ResolvedBitset>,
    pub unions: Vec<ResolvedUnion>,
    pub interfaces: Vec<ResolvedInterface>,
    pub exceptions: Vec<ResolvedException>,
    pub constants: Vec<ResolvedConst>,
    pub imported: ImportedTypes,
}

impl IdlModel {
    /// Iterate the qualified name of every named type in the model.
    pub fn qualified_names(&self) -> impl Iterator<Item = &str> {
        self.structs
            .iter()
            .map(|s| s.qualified_name.as_str())
            .chain(self.enums.iter().map(|e| e.qualified_name.as_str()))
            .chain(self.bitmasks.iter().map(|b| b.qualified_name.as_str()))
            .chain(self.bitsets.iter().map(|b| b.qualified_name.as_str()))
            .chain(self.unions.iter().map(|u| u.qualified_name.as_str()))
            .chain(self.exceptions.iter().map(|e| e.qualified_name.as_str()))
    }

    /// Drop every named type and constant whose qualified name is not in `keep`.
    /// Used for resolve-only `#include` handling: types pulled in from included
    /// files populate the resolver symbol table but are not emitted.
    pub fn retain_qualified(&mut self, keep: &HashSet<String>) {
        self.structs.retain(|s| keep.contains(&s.qualified_name));
        self.enums.retain(|e| keep.contains(&e.qualified_name));
        self.bitmasks.retain(|b| keep.contains(&b.qualified_name));
        self.bitsets.retain(|b| keep.contains(&b.qualified_name));
        self.unions.retain(|u| keep.contains(&u.qualified_name));
        self.interfaces.retain(|i| keep.contains(&i.qualified_name));
        self.exceptions.retain(|e| keep.contains(&e.qualified_name));
        self.constants.retain(|c| keep.contains(&c.qualified_name));
    }

    /// Rewrite the qualified name of every named type through `f`.
    pub fn map_qualified_names(&mut self, f: impl Fn(&str) -> String) {
        for s in &mut self.structs {
            s.qualified_name = f(&s.qualified_name);
        }
        for e in &mut self.enums {
            e.qualified_name = f(&e.qualified_name);
        }
        for b in &mut self.bitmasks {
            b.qualified_name = f(&b.qualified_name);
        }
        for b in &mut self.bitsets {
            b.qualified_name = f(&b.qualified_name);
        }
        for u in &mut self.unions {
            u.qualified_name = f(&u.qualified_name);
        }
        for ex in &mut self.exceptions {
            ex.qualified_name = f(&ex.qualified_name);
        }
    }
}
