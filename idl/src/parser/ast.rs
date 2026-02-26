/// IDL Abstract Syntax Tree node definitions.

/// Top-level IDL definition.
#[derive(Debug, Clone)]
pub enum Definition {
    Module(ModuleDef),
    Struct(StructDef),
    Enum(EnumDef),
    Typedef(TypedefDef),
}

#[derive(Debug, Clone)]
pub struct ModuleDef {
    pub name: String,
    pub definitions: Vec<Definition>,
}

#[derive(Debug, Clone)]
pub struct StructDef {
    pub name: String,
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

/// Type specifier in IDL.
#[derive(Debug, Clone)]
pub enum TypeSpec {
    // Primitives
    Boolean,
    Octet,
    Char,
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

    // Collections
    Sequence(Box<TypeSpec>, Option<u32>),
    Array(Box<TypeSpec>, u32),

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
