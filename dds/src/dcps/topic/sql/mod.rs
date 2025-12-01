//! SQL expression parsing and evaluation for QueryCondition and ContentFilteredTopic.
//!
//! This is an internal module.

use crate::{
    core::error::DdsResult,
    topic::sql::{ast::Expression, lexer::Lexer, parser::Parser},
};

pub mod ast;
pub(crate) mod evaluator;
pub(crate) mod lexer;
pub(crate) mod parser;
pub(crate) mod validate;

pub(crate) fn parse_expression(expression: &str, query: bool) -> DdsResult<Expression> {
    let mut lexer = Lexer::new(expression.to_owned());
    let tokens = lexer.tokenize();
    let mut parser = Parser::new(tokens);
    parser.parse_expression(query)
}
