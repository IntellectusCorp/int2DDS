/// IDL lexer / tokenizer.

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Keywords
    Struct,
    Enum,
    Module,
    Typedef,
    Sequence,
    StringKw,
    WStringKw,
    WCharKw,
    Boolean,
    Octet,
    Char,
    Short,
    Long,
    Float,
    Double,
    Unsigned,
    True,
    False,
    Map,
    Bitmask,
    Bitset,
    Bitfield,
    Union,
    Switch,
    Case,
    Default,
    Interface,
    In,
    Out,
    Inout,
    Void,
    Raises,
    Attribute,
    Readonly,
    Exception,
    Const,

    // Literals
    IntLiteral(i64),
    FloatLiteral(f64),
    StringLiteral(String),

    // Identifier
    Ident(String),

    // Punctuation
    At,
    LeftBrace,
    RightBrace,
    LeftParen,
    RightParen,
    LeftAngle,
    RightAngle,
    LeftBracket,
    RightBracket,
    Semicolon,
    Comma,
    Colon,
    ColonColon,
    Equals,

    Eof,
}

#[derive(Debug, Clone)]
pub struct SpannedToken {
    pub token: Token,
    pub line: usize,
    pub col: usize,
}

#[derive(Debug)]
pub struct LexError {
    pub line: usize,
    pub col: usize,
    pub message: String,
}

impl std::fmt::Display for LexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.col, self.message)
    }
}

pub fn tokenize(source: &str) -> Result<Vec<SpannedToken>, LexError> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = source.chars().collect();
    let len = chars.len();
    let mut pos = 0;
    let mut line = 1;
    let mut col = 1;

    while pos < len {
        // Skip whitespace
        if chars[pos].is_whitespace() {
            if chars[pos] == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
            pos += 1;
            continue;
        }

        // Skip line comments
        if pos + 1 < len && chars[pos] == '/' && chars[pos + 1] == '/' {
            pos += 2;
            col += 2;
            while pos < len && chars[pos] != '\n' {
                pos += 1;
            }
            continue;
        }

        // Skip block comments
        if pos + 1 < len && chars[pos] == '/' && chars[pos + 1] == '*' {
            let start_line = line;
            let start_col = col;
            pos += 2;
            col += 2;
            let mut depth = 1;
            while pos < len && depth > 0 {
                if pos + 1 < len && chars[pos] == '/' && chars[pos + 1] == '*' {
                    depth += 1;
                    pos += 2;
                    col += 2;
                } else if pos + 1 < len && chars[pos] == '*' && chars[pos + 1] == '/' {
                    depth -= 1;
                    pos += 2;
                    col += 2;
                } else {
                    if chars[pos] == '\n' {
                        line += 1;
                        col = 1;
                    } else {
                        col += 1;
                    }
                    pos += 1;
                }
            }
            if depth > 0 {
                return Err(LexError {
                    line: start_line,
                    col: start_col,
                    message: "unterminated block comment".to_string(),
                });
            }
            continue;
        }

        // Skip preprocessor directives (#include, #pragma, etc.)
        if chars[pos] == '#' {
            while pos < len && chars[pos] != '\n' {
                pos += 1;
            }
            continue;
        }

        let tok_line = line;
        let tok_col = col;

        // String literal
        if chars[pos] == '"' {
            pos += 1;
            col += 1;
            let mut s = String::new();
            while pos < len && chars[pos] != '"' {
                if chars[pos] == '\\' && pos + 1 < len {
                    pos += 1;
                    col += 1;
                    match chars[pos] {
                        'n' => s.push('\n'),
                        't' => s.push('\t'),
                        '\\' => s.push('\\'),
                        '"' => s.push('"'),
                        _ => s.push(chars[pos]),
                    }
                } else {
                    s.push(chars[pos]);
                }
                pos += 1;
                col += 1;
            }
            if pos >= len {
                return Err(LexError {
                    line: tok_line,
                    col: tok_col,
                    message: "unterminated string literal".to_string(),
                });
            }
            pos += 1; // skip closing "
            col += 1;
            tokens.push(SpannedToken {
                token: Token::StringLiteral(s),
                line: tok_line,
                col: tok_col,
            });
            continue;
        }

        // Number literal
        if chars[pos].is_ascii_digit()
            || (chars[pos] == '-' && pos + 1 < len && chars[pos + 1].is_ascii_digit())
        {
            let mut num_str = String::new();
            if chars[pos] == '-' {
                num_str.push('-');
                pos += 1;
                col += 1;
            }

            // Hex prefix
            if pos + 1 < len && chars[pos] == '0' && (chars[pos + 1] == 'x' || chars[pos + 1] == 'X')
            {
                num_str.push('0');
                num_str.push('x');
                pos += 2;
                col += 2;
                while pos < len && chars[pos].is_ascii_hexdigit() {
                    num_str.push(chars[pos]);
                    pos += 1;
                    col += 1;
                }
                let val =
                    i64::from_str_radix(&num_str[2..], 16).map_err(|_| LexError {
                        line: tok_line,
                        col: tok_col,
                        message: format!("invalid hex literal: {}", num_str),
                    })?;
                tokens.push(SpannedToken {
                    token: Token::IntLiteral(val),
                    line: tok_line,
                    col: tok_col,
                });
                continue;
            }

            let mut is_float = false;
            while pos < len && chars[pos].is_ascii_digit() {
                num_str.push(chars[pos]);
                pos += 1;
                col += 1;
            }
            if pos < len && chars[pos] == '.' {
                is_float = true;
                num_str.push('.');
                pos += 1;
                col += 1;
                while pos < len && chars[pos].is_ascii_digit() {
                    num_str.push(chars[pos]);
                    pos += 1;
                    col += 1;
                }
            }
            if pos < len && (chars[pos] == 'e' || chars[pos] == 'E') {
                is_float = true;
                num_str.push('e');
                pos += 1;
                col += 1;
                if pos < len && (chars[pos] == '+' || chars[pos] == '-') {
                    num_str.push(chars[pos]);
                    pos += 1;
                    col += 1;
                }
                while pos < len && chars[pos].is_ascii_digit() {
                    num_str.push(chars[pos]);
                    pos += 1;
                    col += 1;
                }
            }

            if is_float {
                let val: f64 = num_str.parse().map_err(|_| LexError {
                    line: tok_line,
                    col: tok_col,
                    message: format!("invalid float literal: {}", num_str),
                })?;
                tokens.push(SpannedToken {
                    token: Token::FloatLiteral(val),
                    line: tok_line,
                    col: tok_col,
                });
            } else {
                let val: i64 = num_str.parse().map_err(|_| LexError {
                    line: tok_line,
                    col: tok_col,
                    message: format!("invalid integer literal: {}", num_str),
                })?;
                tokens.push(SpannedToken {
                    token: Token::IntLiteral(val),
                    line: tok_line,
                    col: tok_col,
                });
            }
            continue;
        }

        // Identifier or keyword
        if chars[pos].is_ascii_alphabetic() || chars[pos] == '_' {
            let mut ident = String::new();
            while pos < len && (chars[pos].is_ascii_alphanumeric() || chars[pos] == '_') {
                ident.push(chars[pos]);
                pos += 1;
                col += 1;
            }

            let token = match ident.as_str() {
                "struct" => Token::Struct,
                "enum" => Token::Enum,
                "module" => Token::Module,
                "typedef" => Token::Typedef,
                "sequence" => Token::Sequence,
                "string" => Token::StringKw,
                "wstring" => Token::WStringKw,
                "boolean" => Token::Boolean,
                "octet" | "uint8" | "int8" => {
                    // uint8/int8 are aliases
                    if ident == "int8" {
                        Token::Ident("int8".to_string())
                    } else {
                        Token::Octet
                    }
                }
                "char" => Token::Char,
                "wchar" => Token::WCharKw,
                "short" => Token::Short,
                "long" => Token::Long,
                "float" => Token::Float,
                "double" => Token::Double,
                "unsigned" => Token::Unsigned,
                "TRUE" | "true" => Token::True,
                "FALSE" | "false" => Token::False,
                "map" => Token::Map,
                "bitmask" => Token::Bitmask,
                "bitset" => Token::Bitset,
                "bitfield" => Token::Bitfield,
                "union" => Token::Union,
                "switch" => Token::Switch,
                "case" => Token::Case,
                "default" => Token::Default,
                "interface" => Token::Interface,
                "in" => Token::In,
                "out" => Token::Out,
                "inout" => Token::Inout,
                "void" => Token::Void,
                "raises" => Token::Raises,
                "attribute" => Token::Attribute,
                "readonly" => Token::Readonly,
                "exception" => Token::Exception,
                "const" => Token::Const,
                _ => Token::Ident(ident),
            };

            tokens.push(SpannedToken {
                token,
                line: tok_line,
                col: tok_col,
            });
            continue;
        }

        // Single/double character tokens
        let token = match chars[pos] {
            '@' => {
                pos += 1;
                col += 1;
                Token::At
            }
            '{' => {
                pos += 1;
                col += 1;
                Token::LeftBrace
            }
            '}' => {
                pos += 1;
                col += 1;
                Token::RightBrace
            }
            '(' => {
                pos += 1;
                col += 1;
                Token::LeftParen
            }
            ')' => {
                pos += 1;
                col += 1;
                Token::RightParen
            }
            '<' => {
                pos += 1;
                col += 1;
                Token::LeftAngle
            }
            '>' => {
                pos += 1;
                col += 1;
                Token::RightAngle
            }
            '[' => {
                pos += 1;
                col += 1;
                Token::LeftBracket
            }
            ']' => {
                pos += 1;
                col += 1;
                Token::RightBracket
            }
            ';' => {
                pos += 1;
                col += 1;
                Token::Semicolon
            }
            ',' => {
                pos += 1;
                col += 1;
                Token::Comma
            }
            '=' => {
                pos += 1;
                col += 1;
                Token::Equals
            }
            ':' if pos + 1 < len && chars[pos + 1] == ':' => {
                pos += 2;
                col += 2;
                Token::ColonColon
            }
            ':' => {
                pos += 1;
                col += 1;
                Token::Colon
            }
            ch => {
                return Err(LexError {
                    line: tok_line,
                    col: tok_col,
                    message: format!("unexpected character: '{}'", ch),
                });
            }
        };

        tokens.push(SpannedToken {
            token,
            line: tok_line,
            col: tok_col,
        });
    }

    tokens.push(SpannedToken {
        token: Token::Eof,
        line,
        col,
    });
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_struct() {
        let tokens = tokenize("struct Foo { long x; };").unwrap();
        assert!(matches!(tokens[0].token, Token::Struct));
        assert!(matches!(&tokens[1].token, Token::Ident(n) if n == "Foo"));
        assert!(matches!(tokens[2].token, Token::LeftBrace));
        assert!(matches!(tokens[3].token, Token::Long));
        assert!(matches!(&tokens[4].token, Token::Ident(n) if n == "x"));
        assert!(matches!(tokens[5].token, Token::Semicolon));
        assert!(matches!(tokens[6].token, Token::RightBrace));
        assert!(matches!(tokens[7].token, Token::Semicolon));
    }

    #[test]
    fn test_annotation() {
        let tokens = tokenize("@key @extensibility(APPENDABLE)").unwrap();
        assert!(matches!(tokens[0].token, Token::At));
        assert!(matches!(&tokens[1].token, Token::Ident(n) if n == "key"));
        assert!(matches!(tokens[2].token, Token::At));
        assert!(matches!(&tokens[3].token, Token::Ident(n) if n == "extensibility"));
        assert!(matches!(tokens[4].token, Token::LeftParen));
        assert!(matches!(&tokens[5].token, Token::Ident(n) if n == "APPENDABLE"));
        assert!(matches!(tokens[6].token, Token::RightParen));
    }

    #[test]
    fn test_sequence_type() {
        let tokens = tokenize("sequence<long, 10>").unwrap();
        assert!(matches!(tokens[0].token, Token::Sequence));
        assert!(matches!(tokens[1].token, Token::LeftAngle));
        assert!(matches!(tokens[2].token, Token::Long));
        assert!(matches!(tokens[3].token, Token::Comma));
        assert!(matches!(tokens[4].token, Token::IntLiteral(10)));
        assert!(matches!(tokens[5].token, Token::RightAngle));
    }

    #[test]
    fn test_comments() {
        let tokens = tokenize("// line comment\nstruct /* block */ Foo {};").unwrap();
        assert!(matches!(tokens[0].token, Token::Struct));
        assert!(matches!(&tokens[1].token, Token::Ident(n) if n == "Foo"));
    }

    #[test]
    fn test_wstring_wchar() {
        let tokens = tokenize("wstring<128> wchar").unwrap();
        assert!(matches!(tokens[0].token, Token::WStringKw));
        assert!(matches!(tokens[1].token, Token::LeftAngle));
        assert!(matches!(tokens[2].token, Token::IntLiteral(128)));
        assert!(matches!(tokens[3].token, Token::RightAngle));
        assert!(matches!(tokens[4].token, Token::WCharKw));
    }

    #[test]
    fn test_colon() {
        let tokens = tokenize("struct Derived : Base {};").unwrap();
        assert!(matches!(tokens[0].token, Token::Struct));
        assert!(matches!(&tokens[1].token, Token::Ident(n) if n == "Derived"));
        assert!(matches!(tokens[2].token, Token::Colon));
        assert!(matches!(&tokens[3].token, Token::Ident(n) if n == "Base"));
    }

    #[test]
    fn test_new_keywords() {
        let tokens = tokenize("map bitmask bitset bitfield union switch case default").unwrap();
        assert!(matches!(tokens[0].token, Token::Map));
        assert!(matches!(tokens[1].token, Token::Bitmask));
        assert!(matches!(tokens[2].token, Token::Bitset));
        assert!(matches!(tokens[3].token, Token::Bitfield));
        assert!(matches!(tokens[4].token, Token::Union));
        assert!(matches!(tokens[5].token, Token::Switch));
        assert!(matches!(tokens[6].token, Token::Case));
        assert!(matches!(tokens[7].token, Token::Default));
    }
}
