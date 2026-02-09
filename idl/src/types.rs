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
    String { bound: Option<u32> },
    Sequence { element: Box<ResolvedType>, bound: Option<u32> },
    Array { element: Box<ResolvedType>, size: u32 },
    Struct(String),
    Enum(String),
}

#[derive(Debug, Clone)]
pub struct ResolvedStruct {
    pub name: String,
    pub qualified_name: String,
    pub extensibility: ExtensibilityKind,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExtensibilityKind {
    #[default]
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

/// Complete resolved IDL model.
#[derive(Debug, Clone)]
pub struct IdlModel {
    pub structs: Vec<ResolvedStruct>,
    pub enums: Vec<ResolvedEnum>,
}
