/// Recursive descent parser for OMG IDL subset.

use super::ast::*;
use super::lexer::{SpannedToken, Token};

#[derive(Debug)]
pub struct ParseError {
    pub line: usize,
    pub col: usize,
    pub message: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.col, self.message)
    }
}

pub struct Parser {
    tokens: Vec<SpannedToken>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<SpannedToken>) -> Self {
        Self { tokens, pos: 0 }
    }

    pub fn parse(&mut self) -> Result<Vec<Definition>, ParseError> {
        let mut defs = Vec::new();
        while !self.at_eof() {
            defs.push(self.parse_definition()?);
        }
        Ok(defs)
    }

    // ---- Helpers ----

    fn peek(&self) -> &Token {
        &self.tokens[self.pos].token
    }

    fn current(&self) -> &SpannedToken {
        &self.tokens[self.pos]
    }

    fn at_eof(&self) -> bool {
        matches!(self.peek(), Token::Eof)
    }

    fn advance(&mut self) -> &SpannedToken {
        let t = &self.tokens[self.pos];
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        t
    }

    fn expect(&mut self, expected: &Token) -> Result<&SpannedToken, ParseError> {
        if self.peek() == expected {
            Ok(self.advance())
        } else {
            let cur = self.current();
            Err(ParseError {
                line: cur.line,
                col: cur.col,
                message: format!("expected {:?}, found {:?}", expected, cur.token),
            })
        }
    }

    fn expect_ident(&mut self) -> Result<String, ParseError> {
        if let Token::Ident(name) = self.peek().clone() {
            self.advance();
            Ok(name)
        } else {
            let cur = self.current();
            Err(ParseError {
                line: cur.line,
                col: cur.col,
                message: format!("expected identifier, found {:?}", cur.token),
            })
        }
    }

    /// Like expect_ident but also accepts keyword tokens (for annotation names like @default).
    fn expect_annotation_name(&mut self) -> Result<String, ParseError> {
        let name = match self.peek().clone() {
            Token::Ident(name) => name,
            Token::Default => "default".to_string(),
            Token::Switch => "switch".to_string(),
            Token::Case => "case".to_string(),
            _ => {
                let cur = self.current();
                return Err(ParseError {
                    line: cur.line,
                    col: cur.col,
                    message: format!("expected annotation name, found {:?}", cur.token),
                });
            }
        };
        self.advance();
        Ok(name)
    }

    fn expect_int(&mut self) -> Result<i64, ParseError> {
        if let Token::IntLiteral(val) = self.peek().clone() {
            self.advance();
            Ok(val)
        } else {
            let cur = self.current();
            Err(ParseError {
                line: cur.line,
                col: cur.col,
                message: format!("expected integer literal, found {:?}", cur.token),
            })
        }
    }

    fn eat_semicolons(&mut self) {
        while matches!(self.peek(), Token::Semicolon) {
            self.advance();
        }
    }

    // ---- Parsing ----

    fn parse_definition(&mut self) -> Result<Definition, ParseError> {
        // Collect annotations
        let annotations = self.parse_annotations()?;

        match self.peek() {
            Token::Module => {
                if !annotations.is_empty() {
                    let cur = self.current();
                    return Err(ParseError {
                        line: cur.line,
                        col: cur.col,
                        message: "annotations on modules are not supported".to_string(),
                    });
                }
                Ok(Definition::Module(self.parse_module()?))
            }
            Token::Struct => Ok(Definition::Struct(self.parse_struct(annotations)?)),
            Token::Enum => Ok(Definition::Enum(self.parse_enum(annotations)?)),
            Token::Typedef => {
                if !annotations.is_empty() {
                    let cur = self.current();
                    return Err(ParseError {
                        line: cur.line,
                        col: cur.col,
                        message: "annotations on typedefs are not supported".to_string(),
                    });
                }
                Ok(Definition::Typedef(self.parse_typedef()?))
            }
            Token::Bitmask => Ok(Definition::Bitmask(self.parse_bitmask(annotations)?)),
            Token::Bitset => Ok(Definition::Bitset(self.parse_bitset(annotations)?)),
            Token::Union => Ok(Definition::Union(self.parse_union(annotations)?)),
            Token::Interface => Ok(Definition::Interface(self.parse_interface(annotations)?)),
            Token::Exception => {
                if !annotations.is_empty() {
                    let cur = self.current();
                    return Err(ParseError {
                        line: cur.line,
                        col: cur.col,
                        message: "annotations on exceptions are not supported".to_string(),
                    });
                }
                Ok(Definition::Exception(self.parse_exception()?))
            }
            Token::Const => {
                if !annotations.is_empty() {
                    let cur = self.current();
                    return Err(ParseError {
                        line: cur.line,
                        col: cur.col,
                        message: "annotations on constants are not supported".to_string(),
                    });
                }
                Ok(Definition::Const(self.parse_const()?))
            }
            _ => {
                let cur = self.current();
                Err(ParseError {
                    line: cur.line,
                    col: cur.col,
                    message: format!(
                        "expected definition keyword, found {:?}",
                        cur.token
                    ),
                })
            }
        }
    }

    fn parse_annotations(&mut self) -> Result<Vec<Annotation>, ParseError> {
        let mut annotations = Vec::new();
        while matches!(self.peek(), Token::At) {
            self.advance(); // consume '@'
            annotations.push(self.parse_annotation()?);
        }
        Ok(annotations)
    }

    fn parse_annotation(&mut self) -> Result<Annotation, ParseError> {
        // Annotation names can be keywords like @default, @key etc.
        let name = self.expect_annotation_name()?;
        let mut params = Vec::new();

        if matches!(self.peek(), Token::LeftParen) {
            self.advance(); // consume '('

            // Parse parameters
            if !matches!(self.peek(), Token::RightParen) {
                // Check if it's named params (key=value) or positional
                let first = self.parse_const_expr()?;

                if matches!(self.peek(), Token::Equals) {
                    // Named parameter: first must be an Ident
                    let key = match first {
                        ConstExpr::Ident(name) => name,
                        _ => {
                            let cur = self.current();
                            return Err(ParseError {
                                line: cur.line,
                                col: cur.col,
                                message: "expected identifier before '='".to_string(),
                            });
                        }
                    };
                    self.advance(); // consume '='
                    let value = self.parse_const_expr()?;
                    params.push(AnnotationParam::Named(key, value));

                    while matches!(self.peek(), Token::Comma) {
                        self.advance();
                        let key = self.expect_ident()?;
                        self.expect(&Token::Equals)?;
                        let value = self.parse_const_expr()?;
                        params.push(AnnotationParam::Named(key, value));
                    }
                } else {
                    // Positional parameter
                    params.push(AnnotationParam::Positional(first));
                    while matches!(self.peek(), Token::Comma) {
                        self.advance();
                        let value = self.parse_const_expr()?;
                        params.push(AnnotationParam::Positional(value));
                    }
                }
            }

            self.expect(&Token::RightParen)?;
        }

        Ok(Annotation { name, params })
    }

    fn parse_const_expr(&mut self) -> Result<ConstExpr, ParseError> {
        match self.peek().clone() {
            Token::IntLiteral(v) => {
                self.advance();
                Ok(ConstExpr::Int(v))
            }
            Token::FloatLiteral(v) => {
                self.advance();
                Ok(ConstExpr::Float(v))
            }
            Token::StringLiteral(s) => {
                self.advance();
                Ok(ConstExpr::String(s))
            }
            Token::True => {
                self.advance();
                Ok(ConstExpr::Bool(true))
            }
            Token::False => {
                self.advance();
                Ok(ConstExpr::Bool(false))
            }
            Token::Ident(name) => {
                self.advance();
                Ok(ConstExpr::Ident(name))
            }
            _ => {
                let cur = self.current();
                Err(ParseError {
                    line: cur.line,
                    col: cur.col,
                    message: format!("expected constant expression, found {:?}", cur.token),
                })
            }
        }
    }

    fn parse_module(&mut self) -> Result<ModuleDef, ParseError> {
        self.expect(&Token::Module)?;
        let name = self.expect_ident()?;
        self.expect(&Token::LeftBrace)?;

        let mut definitions = Vec::new();
        while !matches!(self.peek(), Token::RightBrace) {
            definitions.push(self.parse_definition()?);
        }

        self.expect(&Token::RightBrace)?;
        self.eat_semicolons();

        Ok(ModuleDef { name, definitions })
    }

    fn parse_struct(&mut self, annotations: Vec<Annotation>) -> Result<StructDef, ParseError> {
        self.expect(&Token::Struct)?;
        let name = self.expect_ident()?;

        // Optional inheritance: struct Derived : Base { ... }
        let base_type = if matches!(self.peek(), Token::Colon) {
            self.advance(); // consume ':'
            let mut base_name = self.expect_ident()?;
            // Handle scoped names: Foo::Bar
            while matches!(self.peek(), Token::ColonColon) {
                self.advance();
                let part = self.expect_ident()?;
                base_name = format!("{}::{}", base_name, part);
            }
            Some(base_name)
        } else {
            None
        };

        self.expect(&Token::LeftBrace)?;

        let mut members = Vec::new();
        while !matches!(self.peek(), Token::RightBrace) {
            members.push(self.parse_struct_member()?);
        }

        self.expect(&Token::RightBrace)?;
        self.eat_semicolons();

        Ok(StructDef {
            name,
            base_type,
            members,
            annotations,
        })
    }

    fn parse_struct_member(&mut self) -> Result<StructMember, ParseError> {
        let annotations = self.parse_annotations()?;
        let type_spec = self.parse_type_spec()?;
        let (name, type_spec) = self.parse_declarator(type_spec)?;
        self.expect(&Token::Semicolon)?;

        Ok(StructMember {
            name,
            type_spec,
            annotations,
        })
    }

    fn parse_enum(&mut self, annotations: Vec<Annotation>) -> Result<EnumDef, ParseError> {
        self.expect(&Token::Enum)?;
        let name = self.expect_ident()?;
        self.expect(&Token::LeftBrace)?;

        let mut variants = Vec::new();
        if !matches!(self.peek(), Token::RightBrace) {
            variants.push(self.parse_enum_variant()?);
            while matches!(self.peek(), Token::Comma) {
                self.advance();
                if matches!(self.peek(), Token::RightBrace) {
                    break; // trailing comma
                }
                variants.push(self.parse_enum_variant()?);
            }
        }

        self.expect(&Token::RightBrace)?;
        self.eat_semicolons();

        Ok(EnumDef {
            name,
            variants,
            annotations,
        })
    }

    fn parse_enum_variant(&mut self) -> Result<EnumVariant, ParseError> {
        let annotations = self.parse_annotations()?;
        let name = self.expect_ident()?;
        let value = if matches!(self.peek(), Token::Equals) {
            self.advance();
            Some(self.expect_int()?)
        } else {
            None
        };
        Ok(EnumVariant { name, value, annotations })
    }

    fn parse_typedef(&mut self) -> Result<TypedefDef, ParseError> {
        self.expect(&Token::Typedef)?;
        let type_spec = self.parse_type_spec()?;
        let (name, type_spec) = self.parse_declarator(type_spec)?;
        self.expect(&Token::Semicolon)?;

        Ok(TypedefDef { name, type_spec })
    }

    fn parse_bitmask(&mut self, annotations: Vec<Annotation>) -> Result<BitmaskDef, ParseError> {
        self.expect(&Token::Bitmask)?;
        let name = self.expect_ident()?;
        self.expect(&Token::LeftBrace)?;

        let mut flags = Vec::new();
        if !matches!(self.peek(), Token::RightBrace) {
            flags.push(self.parse_bitmask_flag()?);
            while matches!(self.peek(), Token::Comma) {
                self.advance();
                if matches!(self.peek(), Token::RightBrace) {
                    break;
                }
                flags.push(self.parse_bitmask_flag()?);
            }
        }

        self.expect(&Token::RightBrace)?;
        self.eat_semicolons();

        Ok(BitmaskDef { name, flags, annotations })
    }

    fn parse_bitmask_flag(&mut self) -> Result<BitmaskFlag, ParseError> {
        let annotations = self.parse_annotations()?;
        let name = self.expect_ident()?;
        Ok(BitmaskFlag { name, annotations })
    }

    fn parse_bitset(&mut self, annotations: Vec<Annotation>) -> Result<BitsetDef, ParseError> {
        self.expect(&Token::Bitset)?;
        let name = self.expect_ident()?;
        self.expect(&Token::LeftBrace)?;

        let mut fields = Vec::new();
        while !matches!(self.peek(), Token::RightBrace) {
            // bitfield<N> name;
            self.expect(&Token::Bitfield)?;
            self.expect(&Token::LeftAngle)?;
            let bit_width = self.expect_int()? as u32;
            self.expect(&Token::RightAngle)?;
            let field_name = self.expect_ident()?;
            self.expect(&Token::Semicolon)?;
            fields.push(BitsetField { name: field_name, bit_width });
        }

        self.expect(&Token::RightBrace)?;
        self.eat_semicolons();

        Ok(BitsetDef { name, fields, annotations })
    }

    fn parse_union(&mut self, annotations: Vec<Annotation>) -> Result<UnionDef, ParseError> {
        self.expect(&Token::Union)?;
        let name = self.expect_ident()?;
        self.expect(&Token::Switch)?;
        self.expect(&Token::LeftParen)?;
        let discriminant_type = self.parse_type_spec()?;
        self.expect(&Token::RightParen)?;
        self.expect(&Token::LeftBrace)?;

        let mut cases = Vec::new();
        let mut default_case = None;

        while !matches!(self.peek(), Token::RightBrace) {
            if matches!(self.peek(), Token::Default) {
                // default: Type name;
                self.advance();
                self.expect(&Token::Colon)?;
                let type_spec = self.parse_type_spec()?;
                let member_name = self.expect_ident()?;
                self.expect(&Token::Semicolon)?;
                default_case = Some(UnionCaseMember { type_spec, name: member_name });
            } else {
                // case LABEL: [case LABEL: ...] Type name;
                let mut labels = Vec::new();
                while matches!(self.peek(), Token::Case) {
                    self.advance();
                    let label = self.parse_const_expr()?;
                    self.expect(&Token::Colon)?;
                    labels.push(label);
                }
                if labels.is_empty() {
                    let cur = self.current();
                    return Err(ParseError {
                        line: cur.line,
                        col: cur.col,
                        message: format!("expected 'case' or 'default', found {:?}", cur.token),
                    });
                }
                let type_spec = self.parse_type_spec()?;
                let member_name = self.expect_ident()?;
                self.expect(&Token::Semicolon)?;
                cases.push(UnionCase {
                    labels,
                    member: UnionCaseMember { type_spec, name: member_name },
                });
            }
        }

        self.expect(&Token::RightBrace)?;
        self.eat_semicolons();

        Ok(UnionDef { name, discriminant_type, cases, default_case, annotations })
    }

    /// (7) interface_header + body
    fn parse_interface(&mut self, annotations: Vec<Annotation>) -> Result<InterfaceDef, ParseError> {
        self.expect(&Token::Interface)?;
        let name = self.expect_ident()?;

        // Optional inheritance: interface Foo : Bar, Baz
        let mut base_interfaces = Vec::new();
        if matches!(self.peek(), Token::Colon) {
            self.advance();
            base_interfaces.push(self.parse_scoped_name()?);
            while matches!(self.peek(), Token::Comma) {
                self.advance();
                base_interfaces.push(self.parse_scoped_name()?);
            }
        }

        self.expect(&Token::LeftBrace)?;

        // (9) export: attr_dcl | op_dcl
        let mut operations = Vec::new();
        let mut attributes = Vec::new();
        while !matches!(self.peek(), Token::RightBrace) {
            let member_annotations = self.parse_annotations()?;

            match self.peek() {
                Token::Readonly => {
                    attributes.push(self.parse_readonly_attr(member_annotations)?);
                }
                Token::Attribute => {
                    attributes.push(self.parse_attr(member_annotations)?);
                }
                _ => {
                    // op_dcl: return_type name(params) [raises(...)];
                    operations.push(self.parse_operation(member_annotations)?);
                }
            }
        }

        self.expect(&Token::RightBrace)?;
        self.eat_semicolons();

        Ok(InterfaceDef {
            name,
            base_interfaces,
            operations,
            attributes,
            annotations,
        })
    }

    /// (87) op_dcl
    fn parse_operation(&mut self, annotations: Vec<Annotation>) -> Result<OperationDef, ParseError> {
        // (88) op_type_spec: void | type_spec
        let return_type = if matches!(self.peek(), Token::Void) {
            self.advance();
            None
        } else {
            Some(self.parse_type_spec()?)
        };

        let name = self.expect_ident()?;

        // parameter_dcls: '(' [param_dcl {',' param_dcl}] ')'
        self.expect(&Token::LeftParen)?;
        let mut params = Vec::new();
        if !matches!(self.peek(), Token::RightParen) {
            params.push(self.parse_param()?);
            while matches!(self.peek(), Token::Comma) {
                self.advance();
                params.push(self.parse_param()?);
            }
        }
        self.expect(&Token::RightParen)?;

        // Optional raises_expr: 'raises' '(' name {',' name} ')'
        let raises = if matches!(self.peek(), Token::Raises) {
            self.advance();
            self.parse_raises_list()?
        } else {
            Vec::new()
        };

        self.expect(&Token::Semicolon)?;

        Ok(OperationDef {
            name,
            return_type,
            params,
            raises,
            annotations,
        })
    }

    /// (91) param_dcl
    fn parse_param(&mut self) -> Result<ParamDef, ParseError> {
        let annotations = self.parse_annotations()?;

        // (92) param_attribute
        let direction = match self.peek() {
            Token::In => { self.advance(); ParamDirection::In }
            Token::Out => { self.advance(); ParamDirection::Out }
            Token::Inout => { self.advance(); ParamDirection::Inout }
            _ => {
                let cur = self.current();
                return Err(ParseError {
                    line: cur.line,
                    col: cur.col,
                    message: format!("expected 'in', 'out', or 'inout', found {:?}", cur.token),
                });
            }
        };

        let type_spec = self.parse_type_spec()?;
        let name = self.expect_ident()?;

        Ok(ParamDef { name, type_spec, direction, annotations })
    }

    /// (104) readonly_attr_spec
    fn parse_readonly_attr(&mut self, annotations: Vec<Annotation>) -> Result<AttributeDef, ParseError> {
        self.expect(&Token::Readonly)?;
        self.expect(&Token::Attribute)?;
        let type_spec = self.parse_type_spec()?;
        let name = self.expect_ident()?;

        let raises = if matches!(self.peek(), Token::Raises) {
            self.advance();
            self.parse_raises_list()?
        } else {
            Vec::new()
        };

        self.expect(&Token::Semicolon)?;

        Ok(AttributeDef { name, type_spec, readonly: true, raises, annotations })
    }

    /// (106) attr_spec
    fn parse_attr(&mut self, annotations: Vec<Annotation>) -> Result<AttributeDef, ParseError> {
        self.expect(&Token::Attribute)?;
        let type_spec = self.parse_type_spec()?;
        let name = self.expect_ident()?;

        let raises = if matches!(self.peek(), Token::Raises) {
            self.advance();
            self.parse_raises_list()?
        } else {
            Vec::new()
        };

        self.expect(&Token::Semicolon)?;

        Ok(AttributeDef { name, type_spec, readonly: false, raises, annotations })
    }

    /// exception declaration — like struct but with 'exception' keyword
    fn parse_exception(&mut self) -> Result<ExceptionDef, ParseError> {
        self.expect(&Token::Exception)?;
        let name = self.expect_ident()?;
        self.expect(&Token::LeftBrace)?;

        let mut members = Vec::new();
        while !matches!(self.peek(), Token::RightBrace) {
            members.push(self.parse_struct_member()?);
        }

        self.expect(&Token::RightBrace)?;
        self.eat_semicolons();

        Ok(ExceptionDef { name, members })
    }

    /// const declaration: 'const' type_spec name '=' const_expr ';'
    fn parse_const(&mut self) -> Result<ConstDef, ParseError> {
        self.expect(&Token::Const)?;
        let type_spec = self.parse_type_spec()?;
        let name = self.expect_ident()?;
        self.expect(&Token::Equals)?;
        let value = self.parse_const_expr()?;
        self.expect(&Token::Semicolon)?;

        Ok(ConstDef { name, type_spec, value })
    }

    /// raises_expr: '(' scoped_name {',' scoped_name} ')'
    fn parse_raises_list(&mut self) -> Result<Vec<String>, ParseError> {
        self.expect(&Token::LeftParen)?;
        let mut names = Vec::new();
        names.push(self.parse_scoped_name()?);
        while matches!(self.peek(), Token::Comma) {
            self.advance();
            names.push(self.parse_scoped_name()?);
        }
        self.expect(&Token::RightParen)?;
        Ok(names)
    }

    /// Parse a scoped name: Foo or Foo::Bar::Baz
    fn parse_scoped_name(&mut self) -> Result<String, ParseError> {
        let mut name = self.expect_ident()?;
        while matches!(self.peek(), Token::ColonColon) {
            self.advance();
            let part = self.expect_ident()?;
            name = format!("{}::{}", name, part);
        }
        Ok(name)
    }

    /// Parse a type specifier (without the field name / array dimensions).
    fn parse_type_spec(&mut self) -> Result<TypeSpec, ParseError> {
        match self.peek().clone() {
            Token::Boolean => {
                self.advance();
                Ok(TypeSpec::Boolean)
            }
            Token::Octet => {
                self.advance();
                Ok(TypeSpec::Octet)
            }
            Token::Char => {
                self.advance();
                Ok(TypeSpec::Char)
            }
            Token::WCharKw => {
                self.advance();
                Ok(TypeSpec::WChar)
            }
            Token::Float => {
                self.advance();
                Ok(TypeSpec::Float32)
            }
            Token::Double => {
                self.advance();
                Ok(TypeSpec::Float64)
            }
            Token::Short => {
                self.advance();
                Ok(TypeSpec::Int16)
            }
            Token::Long => {
                self.advance();
                // Check for "long long"
                if matches!(self.peek(), Token::Long) {
                    self.advance();
                    Ok(TypeSpec::Int64)
                } else {
                    Ok(TypeSpec::Int32)
                }
            }
            Token::Unsigned => {
                self.advance();
                match self.peek() {
                    Token::Short => {
                        self.advance();
                        Ok(TypeSpec::Uint16)
                    }
                    Token::Long => {
                        self.advance();
                        if matches!(self.peek(), Token::Long) {
                            self.advance();
                            Ok(TypeSpec::Uint64)
                        } else {
                            Ok(TypeSpec::Uint32)
                        }
                    }
                    _ => {
                        let cur = self.current();
                        Err(ParseError {
                            line: cur.line,
                            col: cur.col,
                            message: format!(
                                "expected 'short' or 'long' after 'unsigned', found {:?}",
                                cur.token
                            ),
                        })
                    }
                }
            }
            Token::StringKw => {
                self.advance();
                let bound = if matches!(self.peek(), Token::LeftAngle) {
                    self.advance();
                    let n = self.expect_int()? as u32;
                    self.expect(&Token::RightAngle)?;
                    Some(n)
                } else {
                    None
                };
                Ok(TypeSpec::String(bound))
            }
            Token::WStringKw => {
                self.advance();
                let bound = if matches!(self.peek(), Token::LeftAngle) {
                    self.advance();
                    let n = self.expect_int()? as u32;
                    self.expect(&Token::RightAngle)?;
                    Some(n)
                } else {
                    None
                };
                Ok(TypeSpec::WString(bound))
            }
            Token::Sequence => {
                self.advance();
                self.expect(&Token::LeftAngle)?;
                let element = self.parse_type_spec()?;
                let bound = if matches!(self.peek(), Token::Comma) {
                    self.advance();
                    Some(self.expect_int()? as u32)
                } else {
                    None
                };
                self.expect(&Token::RightAngle)?;
                Ok(TypeSpec::Sequence(Box::new(element), bound))
            }
            Token::Map => {
                self.advance();
                self.expect(&Token::LeftAngle)?;
                let key_type = self.parse_type_spec()?;
                self.expect(&Token::Comma)?;
                let value_type = self.parse_type_spec()?;
                let bound = if matches!(self.peek(), Token::Comma) {
                    self.advance();
                    Some(self.expect_int()? as u32)
                } else {
                    None
                };
                self.expect(&Token::RightAngle)?;
                Ok(TypeSpec::Map(Box::new(key_type), Box::new(value_type), bound))
            }
            Token::Ident(name) => {
                self.advance();
                // Handle scoped names: Foo::Bar
                let mut full_name = name;
                while matches!(self.peek(), Token::ColonColon) {
                    self.advance();
                    let part = self.expect_ident()?;
                    full_name = format!("{}::{}", full_name, part);
                }
                Ok(TypeSpec::Named(full_name))
            }
            _ => {
                let cur = self.current();
                Err(ParseError {
                    line: cur.line,
                    col: cur.col,
                    message: format!("expected type specifier, found {:?}", cur.token),
                })
            }
        }
    }

    /// Parse declarator: name followed by optional array dimensions.
    /// `long name[3][4]` -> declarator returns ("name", Array(Array(Int32, 4), 3))
    fn parse_declarator(&mut self, base_type: TypeSpec) -> Result<(String, TypeSpec), ParseError> {
        let name = self.expect_ident()?;
        let mut dims = Vec::new();

        while matches!(self.peek(), Token::LeftBracket) {
            self.advance();
            let size = self.expect_int()? as u32;
            self.expect(&Token::RightBracket)?;
            dims.push(size);
        }

        // Build nested array types from innermost to outermost
        let mut type_spec = base_type;
        for dim in dims.into_iter().rev() {
            type_spec = TypeSpec::Array(Box::new(type_spec), dim);
        }

        Ok((name, type_spec))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::lexer::tokenize;

    fn parse_str(input: &str) -> Vec<Definition> {
        let tokens = tokenize(input).expect("lex error");
        let mut parser = Parser::new(tokens);
        parser.parse().expect("parse error")
    }

    #[test]
    fn test_hello_world() {
        let defs = parse_str(
            r#"
            @extensibility(APPENDABLE)
            struct HelloWorld {
                unsigned long index;
                string message;
            };
            "#,
        );
        assert_eq!(defs.len(), 1);
        if let Definition::Struct(s) = &defs[0] {
            assert_eq!(s.name, "HelloWorld");
            assert_eq!(s.members.len(), 2);
            assert_eq!(s.members[0].name, "index");
            assert!(matches!(s.members[0].type_spec, TypeSpec::Uint32));
            assert_eq!(s.members[1].name, "message");
            assert!(matches!(s.members[1].type_spec, TypeSpec::String(None)));
            assert_eq!(s.annotations.len(), 1);
            assert_eq!(s.annotations[0].name, "extensibility");
        } else {
            panic!("expected struct");
        }
    }

    #[test]
    fn test_keyed_struct() {
        let defs = parse_str(
            r#"
            struct SensorData {
                @key long sensor_id;
                double temperature;
                string<128> location;
            };
            "#,
        );
        if let Definition::Struct(s) = &defs[0] {
            assert_eq!(s.members[0].annotations.len(), 1);
            assert_eq!(s.members[0].annotations[0].name, "key");
            assert!(matches!(
                s.members[2].type_spec,
                TypeSpec::String(Some(128))
            ));
        } else {
            panic!("expected struct");
        }
    }

    #[test]
    fn test_enum() {
        let defs = parse_str(
            r#"
            enum Color { RED, GREEN = 5, BLUE };
            "#,
        );
        if let Definition::Enum(e) = &defs[0] {
            assert_eq!(e.name, "Color");
            assert_eq!(e.variants.len(), 3);
            assert_eq!(e.variants[0].name, "RED");
            assert_eq!(e.variants[0].value, None);
            assert_eq!(e.variants[1].name, "GREEN");
            assert_eq!(e.variants[1].value, Some(5));
            assert_eq!(e.variants[2].name, "BLUE");
        } else {
            panic!("expected enum");
        }
    }

    #[test]
    fn test_enum_value_annotation() {
        let defs = parse_str(
            r#"
            enum Priority {
                LOW,
                @value(5) MEDIUM,
                @value(10) HIGH,
                @value(100) CRITICAL
            };
            "#,
        );
        if let Definition::Enum(e) = &defs[0] {
            assert_eq!(e.name, "Priority");
            assert_eq!(e.variants.len(), 4);
            assert_eq!(e.variants[0].name, "LOW");
            assert_eq!(e.variants[0].value, None);
            assert!(e.variants[0].annotations.is_empty());
            assert_eq!(e.variants[1].name, "MEDIUM");
            assert_eq!(e.variants[1].annotations[0].name, "value");
            assert_eq!(e.variants[2].name, "HIGH");
            assert_eq!(e.variants[2].annotations[0].name, "value");
            assert_eq!(e.variants[3].name, "CRITICAL");
            assert_eq!(e.variants[3].annotations[0].name, "value");
        } else {
            panic!("expected enum");
        }
    }

    #[test]
    fn test_module() {
        let defs = parse_str(
            r#"
            module Sensors {
                struct Data {
                    long value;
                };
            };
            "#,
        );
        if let Definition::Module(m) = &defs[0] {
            assert_eq!(m.name, "Sensors");
            assert_eq!(m.definitions.len(), 1);
        } else {
            panic!("expected module");
        }
    }

    #[test]
    fn test_sequence_and_array() {
        let defs = parse_str(
            r#"
            struct Data {
                sequence<long> values;
                sequence<double, 100> bounded;
                float matrix[3];
            };
            "#,
        );
        if let Definition::Struct(s) = &defs[0] {
            assert!(matches!(
                &s.members[0].type_spec,
                TypeSpec::Sequence(elem, None) if matches!(elem.as_ref(), TypeSpec::Int32)
            ));
            assert!(matches!(
                &s.members[1].type_spec,
                TypeSpec::Sequence(elem, Some(100)) if matches!(elem.as_ref(), TypeSpec::Float64)
            ));
            assert!(matches!(
                &s.members[2].type_spec,
                TypeSpec::Array(elem, 3) if matches!(elem.as_ref(), TypeSpec::Float32)
            ));
        } else {
            panic!("expected struct");
        }
    }

    #[test]
    fn test_typedef() {
        let defs = parse_str("typedef sequence<long> IntList;");
        if let Definition::Typedef(td) = &defs[0] {
            assert_eq!(td.name, "IntList");
            assert!(matches!(
                &td.type_spec,
                TypeSpec::Sequence(elem, None) if matches!(elem.as_ref(), TypeSpec::Int32)
            ));
        } else {
            panic!("expected typedef");
        }
    }

    #[test]
    fn test_mutable_with_ids() {
        let defs = parse_str(
            r#"
            @extensibility(MUTABLE)
            struct MutableType {
                @key @id(0) unsigned long id;
                @id(1) string<64> name;
                @optional @id(2) double value;
            };
            "#,
        );
        if let Definition::Struct(s) = &defs[0] {
            assert_eq!(s.members[0].annotations.len(), 2); // @key, @id(0)
            assert_eq!(s.members[1].annotations.len(), 1); // @id(1)
            assert_eq!(s.members[2].annotations.len(), 2); // @optional, @id(2)
        } else {
            panic!("expected struct");
        }
    }

    #[test]
    fn test_wstring_wchar() {
        let defs = parse_str(
            r#"
            struct WideData {
                wchar wc;
                wstring ws;
                wstring<128> bounded_ws;
            };
            "#,
        );
        if let Definition::Struct(s) = &defs[0] {
            assert!(matches!(s.members[0].type_spec, TypeSpec::WChar));
            assert!(matches!(s.members[1].type_spec, TypeSpec::WString(None)));
            assert!(matches!(s.members[2].type_spec, TypeSpec::WString(Some(128))));
        } else {
            panic!("expected struct");
        }
    }

    #[test]
    fn test_struct_inheritance() {
        let defs = parse_str(
            r#"
            struct Base {
                long x;
            };
            struct Derived : Base {
                long y;
            };
            "#,
        );
        assert_eq!(defs.len(), 2);
        if let Definition::Struct(s) = &defs[1] {
            assert_eq!(s.name, "Derived");
            assert_eq!(s.base_type, Some("Base".to_string()));
            assert_eq!(s.members.len(), 1);
        } else {
            panic!("expected struct");
        }
    }

    #[test]
    fn test_map() {
        let defs = parse_str(
            r#"
            struct MapData {
                map<long, string> lookup;
                map<string, double, 100> bounded_map;
            };
            "#,
        );
        if let Definition::Struct(s) = &defs[0] {
            assert!(matches!(&s.members[0].type_spec, TypeSpec::Map(k, v, None)
                if matches!(k.as_ref(), TypeSpec::Int32) && matches!(v.as_ref(), TypeSpec::String(None))));
            assert!(matches!(&s.members[1].type_spec, TypeSpec::Map(k, v, Some(100))
                if matches!(k.as_ref(), TypeSpec::String(None)) && matches!(v.as_ref(), TypeSpec::Float64)));
        } else {
            panic!("expected struct");
        }
    }

    #[test]
    fn test_bitmask() {
        let defs = parse_str(
            r#"
            @bit_bound(8)
            bitmask MyFlags {
                FLAG_A,
                @position(3) FLAG_B,
                FLAG_C
            };
            "#,
        );
        if let Definition::Bitmask(b) = &defs[0] {
            assert_eq!(b.name, "MyFlags");
            assert_eq!(b.flags.len(), 3);
            assert_eq!(b.flags[0].name, "FLAG_A");
            assert_eq!(b.flags[1].name, "FLAG_B");
            assert_eq!(b.flags[1].annotations.len(), 1);
            assert_eq!(b.flags[1].annotations[0].name, "position");
        } else {
            panic!("expected bitmask");
        }
    }

    #[test]
    fn test_bitset() {
        let defs = parse_str(
            r#"
            bitset MyBitset {
                bitfield<3> field_a;
                bitfield<5> field_b;
            };
            "#,
        );
        if let Definition::Bitset(b) = &defs[0] {
            assert_eq!(b.name, "MyBitset");
            assert_eq!(b.fields.len(), 2);
            assert_eq!(b.fields[0].name, "field_a");
            assert_eq!(b.fields[0].bit_width, 3);
            assert_eq!(b.fields[1].name, "field_b");
            assert_eq!(b.fields[1].bit_width, 5);
        } else {
            panic!("expected bitset");
        }
    }

    #[test]
    fn test_union() {
        let defs = parse_str(
            r#"
            union MyUnion switch(long) {
                case 0: long int_val;
                case 1:
                case 2: string str_val;
                default: octet default_val;
            };
            "#,
        );
        if let Definition::Union(u) = &defs[0] {
            assert_eq!(u.name, "MyUnion");
            assert!(matches!(u.discriminant_type, TypeSpec::Int32));
            assert_eq!(u.cases.len(), 2);
            assert_eq!(u.cases[0].labels.len(), 1);
            assert_eq!(u.cases[1].labels.len(), 2); // case 1 and case 2
            assert!(u.default_case.is_some());
            assert_eq!(u.default_case.as_ref().unwrap().name, "default_val");
        } else {
            panic!("expected union");
        }
    }

    #[test]
    fn test_interface() {
        let defs = parse_str(
            r#"
            @service
            interface RobotControl {
                long moveTo(in float x, in float y, in float z);
                void stop();
                readonly attribute string status;
                attribute float speed;
            };
            "#,
        );
        if let Definition::Interface(i) = &defs[0] {
            assert_eq!(i.name, "RobotControl");
            assert_eq!(i.annotations.len(), 1);
            assert_eq!(i.annotations[0].name, "service");
            assert_eq!(i.base_interfaces.len(), 0);
            assert_eq!(i.operations.len(), 2);

            assert_eq!(i.operations[0].name, "moveTo");
            assert!(matches!(i.operations[0].return_type, Some(TypeSpec::Int32)));
            assert_eq!(i.operations[0].params.len(), 3);
            assert_eq!(i.operations[0].params[0].direction, ParamDirection::In);
            assert_eq!(i.operations[0].params[0].name, "x");

            assert_eq!(i.operations[1].name, "stop");
            assert!(i.operations[1].return_type.is_none());
            assert_eq!(i.operations[1].params.len(), 0);

            assert_eq!(i.attributes.len(), 2);
            assert!(i.attributes[0].readonly);
            assert_eq!(i.attributes[0].name, "status");
            assert!(!i.attributes[1].readonly);
            assert_eq!(i.attributes[1].name, "speed");
        } else {
            panic!("expected interface");
        }
    }

    #[test]
    fn test_interface_inheritance_and_raises() {
        let defs = parse_str(
            r#"
            exception CollisionEx {
                string message;
            };
            interface Base {
                void ping();
            };
            @service
            interface Robot : Base {
                long moveTo(in float x, in float y) raises (CollisionEx);
            };
            "#,
        );
        assert_eq!(defs.len(), 3);

        if let Definition::Exception(e) = &defs[0] {
            assert_eq!(e.name, "CollisionEx");
            assert_eq!(e.members.len(), 1);
        } else {
            panic!("expected exception");
        }

        if let Definition::Interface(i) = &defs[2] {
            assert_eq!(i.name, "Robot");
            assert_eq!(i.base_interfaces, vec!["Base".to_string()]);
            assert_eq!(i.operations[0].raises, vec!["CollisionEx".to_string()]);
        } else {
            panic!("expected interface");
        }
    }

    #[test]
    fn test_const() {
        let defs = parse_str("const long MAX_SIZE = 100;");
        if let Definition::Const(c) = &defs[0] {
            assert_eq!(c.name, "MAX_SIZE");
            assert!(matches!(c.type_spec, TypeSpec::Int32));
            assert!(matches!(c.value, ConstExpr::Int(100)));
        } else {
            panic!("expected const");
        }
    }

    #[test]
    fn test_operation_with_out_inout() {
        let defs = parse_str(
            r#"
            interface Calc {
                void divide(in long a, in long b, out long quotient, inout long remainder);
            };
            "#,
        );
        if let Definition::Interface(i) = &defs[0] {
            let op = &i.operations[0];
            assert_eq!(op.params.len(), 4);
            assert_eq!(op.params[0].direction, ParamDirection::In);
            assert_eq!(op.params[1].direction, ParamDirection::In);
            assert_eq!(op.params[2].direction, ParamDirection::Out);
            assert_eq!(op.params[3].direction, ParamDirection::Inout);
        } else {
            panic!("expected interface");
        }
    }
}
