pub mod ast;
pub mod grammar;
pub mod lexer;

use ast::Definition;
use grammar::ParseError;
use lexer::LexError;

#[derive(Debug)]
pub enum IdlParseError {
    Lex(LexError),
    Parse(ParseError),
}

impl std::fmt::Display for IdlParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IdlParseError::Lex(e) => write!(f, "lex error: {}", e),
            IdlParseError::Parse(e) => write!(f, "parse error: {}", e),
        }
    }
}

/// Parse IDL source text into AST definitions.
pub fn parse_idl(source: &str) -> Result<Vec<Definition>, IdlParseError> {
    let tokens = lexer::tokenize(source).map_err(IdlParseError::Lex)?;
    let mut parser = grammar::Parser::new(tokens);
    parser.parse().map_err(IdlParseError::Parse)
}
