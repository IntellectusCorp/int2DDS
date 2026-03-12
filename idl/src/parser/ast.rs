/// IDL Abstract Syntax Tree node definitions.

/// Top-level IDL definition.
#[derive(Debug, Clone)]
pub enum Definition {
    Module(ModuleDef),
    Struct(StructDef),
    Enum(EnumDef),
    Typedef(TypedefDef),
    Bitmask(BitmaskDef),
    Bitset(BitsetDef),
    Union(UnionDef),
}

#[derive(Debug, Clone)]
pub struct ModuleDef {
    pub name: String,
    pub definitions: Vec<Definition>,
}

#[derive(Debug, Clone)]
pub struct StructDef {
    pub name: String,
    pub base_type: Option<String>,
    pub members: Vec<StructMember>,
    pub annotations: Vec<Annotation>,
}

#[derive(Debug, Clone)]
pub struct StructMember {
    pub name: String,
    pub type_spec: TypeSpec,
    pub annotations: Vec<Annotation>,
}

#[derive(Debug, Clone)]
pub struct EnumDef {
    pub name: String,
    pub variants: Vec<EnumVariant>,
    pub annotations: Vec<Annotation>,
}

#[derive(Debug, Clone)]
pub struct EnumVariant {
    pub name: String,
    pub value: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct TypedefDef {
    pub name: String,
    pub type_spec: TypeSpec,
}

#[derive(Debug, Clone)]
pub struct BitmaskDef {
    pub name: String,
    pub flags: Vec<BitmaskFlag>,
    pub annotations: Vec<Annotation>,
}

#[derive(Debug, Clone)]
pub struct BitmaskFlag {
    pub name: String,
    pub annotations: Vec<Annotation>,
}

#[derive(Debug, Clone)]
pub struct BitsetDef {
    pub name: String,
    pub fields: Vec<BitsetField>,
    pub annotations: Vec<Annotation>,
}

#[derive(Debug, Clone)]
pub struct BitsetField {
    pub name: String,
    pub bit_width: u32,
}

#[derive(Debug, Clone)]
pub struct UnionDef {
    pub name: String,
    pub discriminant_type: TypeSpec,
    pub cases: Vec<UnionCase>,
    pub default_case: Option<UnionCaseMember>,
    pub annotations: Vec<Annotation>,
}

#[derive(Debug, Clone)]
pub struct UnionCase {
    pub labels: Vec<ConstExpr>,
    pub member: UnionCaseMember,
}

#[derive(Debug, Clone)]
pub struct UnionCaseMember {
    pub type_spec: TypeSpec,
    pub name: String,
}

/// Type specifier in IDL.
#[derive(Debug, Clone)]
pub enum TypeSpec {
    // Primitives
    Boolean,
    Octet,
    Char,
    WChar,
    Int16,
    Uint16,
    Int32,
    Uint32,
    Int64,
    Uint64,
    Float32,
    Float64,

    // Strings
    String(Option<u32>),
    WString(Option<u32>),

    // Collections
    Sequence(Box<TypeSpec>, Option<u32>),
    Array(Box<TypeSpec>, u32),
    Map(Box<TypeSpec>, Box<TypeSpec>, Option<u32>),

    // Named type reference
    Named(String),
}

#[derive(Debug, Clone)]
pub struct Annotation {
    pub name: String,
    pub params: Vec<AnnotationParam>,
}

#[derive(Debug, Clone)]
pub enum AnnotationParam {
    Positional(ConstExpr),
    Named(String, ConstExpr),
}

#[derive(Debug, Clone)]
pub enum ConstExpr {
    Int(i64),
    Float(f64),
    String(String),
    Ident(String),
    Bool(bool),
}
