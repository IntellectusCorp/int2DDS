/// Internal representation (IR) for resolved IDL types.

/// Fully resolved type information.
#[derive(Debug, Clone)]
pub enum ResolvedType {
    Bool,
    U8,
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
    String { bound: Option<u32> },
    WString { bound: Option<u32> },
    Sequence { element: Box<ResolvedType>, bound: Option<u32> },
    Array { element: Box<ResolvedType>, size: u32 },
    Map { key: Box<ResolvedType>, value: Box<ResolvedType>, bound: Option<u32> },
    Struct(String),
    Enum(String),
    Bitmask(String),
}

/// Constant value for @default annotation.
#[derive(Debug, Clone)]
pub enum ConstValue {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Ident(String),
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
    /// None = not specified in IDL (use CLI default), Some = explicitly set via @extensibility
    pub extensibility: Option<ExtensibilityKind>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtensibilityKind {
    Final,
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
    /// None = not specified in IDL (use CLI default), Some = explicitly set via @extensibility
    pub extensibility: Option<ExtensibilityKind>,
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

/// Complete resolved IDL model.
#[derive(Debug, Clone)]
pub struct IdlModel {
    pub structs: Vec<ResolvedStruct>,
    pub enums: Vec<ResolvedEnum>,
    pub bitmasks: Vec<ResolvedBitmask>,
    pub bitsets: Vec<ResolvedBitset>,
    pub unions: Vec<ResolvedUnion>,
}
