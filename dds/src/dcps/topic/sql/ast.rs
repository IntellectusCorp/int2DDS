//! Abstract Syntax Tree (AST) for SQL filter expressions.
//!
//! This module defines the AST structures used to represent parsed SQL-like filter
//! expressions for ContentFilteredTopics and QueryConditions. The AST includes tokens,
//! expressions, and operators that form the structure of filter queries.
//!
//! The implementation follows DDS Spec Annex B syntax for field access, comparisons,
//! logical operations, and literal values.

use regex::Regex;

use crate::core::error::{DdsError, DdsResult};

#[derive(PartialEq, Eq, Debug, Clone)]
pub(crate) enum TokenType {
    /*
    // Basic tokens
    // Nested navigation possible using '.'
    // Field names are specified in the structure's IDL definition,
    // and may not match field names visible in language-specific (C/C++, Java, etc.) mappings.
    FieldName,
    // Topic name defined by characters 'a', ..., 'z', 'A',..., 'Z', '0', ..., '9', '-', not starting with a digit
    TopicName,
    */
    Identifier, // FieldName, TopicName
    // May have +, - sign prefix
    // Hexadecimal values start with 0x and must be a valid expression.
    IntegerValue,
    // 'CharValue'
    CharValue,
    // May include +, -, decimal point.
    // Can have a postfix power-of-ten expression in the form e±n.
    FloatValue,
    // 'String'
    String, // String, EnumberLatedValue
    #[allow(dead_code)]
    // 'LabelName', should match the label name specified in the IDL definition.
    EnumeratedValue,
    // Form %n (0<= n < 100, natural number)
    // References the (n+1)th argument in the given context.
    Parameter,

    // Operators
    Equal,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
    NotEqual, // <>
    Like,

    // Logical operators
    And,
    Or,
    Not,

    // Parentheses
    LeftParen,
    RightParen,

    // SQL keywords
    Select,
    From,
    Where,
    OrderBy,
    As,
    Between,

    // Join keywords
    Natural,
    Join,
    Inner,

    // Miscellaneous
    Comma,     // ,
    Semicolon, // ;
    Asterisk,  // *
    Dot,       // .

    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Token {
    pub(crate) token_type: TokenType,
    pub(crate) value: Parameter,
    pub(crate) position: usize,
}

#[derive(Debug, PartialEq, Clone)]
#[allow(clippy::enum_variant_names)]
pub(crate) enum Expression {
    // ContentFilteredTopic
    FilterExpression(Box<Condition>), // Condition
    // MultiTopic
    TopicExpression {
        select: SelectClause,
        from: FromClause,
        where_clause: Option<WhereClause>,
    }, // SelectFrom {Where } ';'
    // QueryReadCondition
    QueryExpression {
        condition: Option<Box<Condition>>,
        order_by: Option<Vec<String>>, // ORDER BY field1, field2 }, // {Condition}{'ORDER BY' (FIELDNAME // ',') }
    },
}

#[derive(Debug, PartialEq, Clone)]
pub(crate) enum Condition {
    Predicate(Predicate),
    Binary {
        left: Box<Condition>,
        op: BinaryOp, // AND, OR
        right: Box<Condition>,
    },
    Unary {
        op: UnaryOp, // NOT
        operand: Box<Condition>,
    },
    Parentheses(Box<Condition>),
}

#[derive(Debug, PartialEq, Clone)]
pub(crate) struct SelectClause {
    pub(crate) aggregation: Aggregation,
}

#[derive(Debug, PartialEq, Clone)]
pub(crate) enum Aggregation {
    All,                           // *
    Fields(Vec<SubjectFieldSpec>), // (SubjectFieldSpec // ',')
}

#[derive(Debug, PartialEq, Clone)]
pub(crate) enum SubjectFieldSpec {
    Field(String),              // field_name
    FieldField(String, String), // field_name field_name
    FieldAs(String, String),    // field_name AS alias
}

#[derive(Debug, PartialEq, Clone)]
pub(crate) struct FromClause {
    pub(crate) selection: Selection,
}

#[derive(Debug, PartialEq, Clone)]
pub(crate) enum Selection {
    Topic(String), // 'TopicName'
    // JoinItem ::= TOPICNAME
    //          |   TOPICNAME NaturalJoin JoinItem
    //          |   '(' TOPICNAME NaturalJoin JoinItem ')'
    JoinItem {
        left: String,           // 'Topic1'
        join_type: NaturalJoin, // 'NATURAL JOIN'
        right: Box<Selection>,
    },
}

#[derive(Debug, PartialEq, Clone)]
#[allow(clippy::enum_variant_names)]
// NaturalJoin ::= 'INNER NATURAL JOIN'
//             |    'NATURAL JOIN'
//             |    'NATURAL INNER JOIN'
pub(crate) enum NaturalJoin {
    NaturalJoin,
    NaturalInnerJoin,
    InnerNaturalJoin,
}

#[derive(Debug, PartialEq, Clone)]
pub(crate) struct WhereClause {
    pub(crate) condition: Box<Condition>,
}

#[derive(Debug, PartialEq, Clone)]
pub(crate) enum BinaryOp {
    And,
    Or,
}

#[derive(Debug, PartialEq, Clone)]
pub(crate) enum UnaryOp {
    Not,
}

#[derive(Debug, PartialEq, Clone)]
pub(crate) enum Predicate {
    Comparison { left: Parameter, op: RelOp, right: Parameter },
    Between { field: String, negated: bool, range: Range },
}

#[derive(Debug, Clone)]
pub(crate) enum RelOp {
    Equal,       // =
    Greater,     // >
    GreaterEq,   // >=
    Less,        // <
    LessEq,      // <=
    NotEqual,    // <>
    Like(Regex), // LIKE
}

impl PartialEq for RelOp {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (RelOp::Equal, RelOp::Equal) => true,
            (RelOp::Greater, RelOp::Greater) => true,
            (RelOp::GreaterEq, RelOp::GreaterEq) => true,
            (RelOp::Less, RelOp::Less) => true,
            (RelOp::LessEq, RelOp::LessEq) => true,
            (RelOp::NotEqual, RelOp::NotEqual) => true,
            (RelOp::Like(regex1), RelOp::Like(regex2)) => regex1.as_str() == regex2.as_str(),
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Range {
    pub(crate) start: Parameter,
    pub(crate) end: Parameter,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum Parameter {
    IntegerValue(i128),
    CharValue(char),
    FloatValue(f64),
    String(String),
    EnumeratedValue { type_name: Option<String>, value: String },
    Parameter(usize), // %0 = Parameter(0), %1 = Parameter(1)
}

impl Expression {
    pub(crate) fn get_order_by_fields(&self) -> Option<&Vec<String>> {
        match self {
            Expression::QueryExpression { order_by, .. } => order_by.as_ref(),
            _ => None,
        }
    }
}

impl Parameter {
    /// Compare parameter values (for ORDER BY)
    pub(crate) fn compare_for_ordering(&self, other: &Parameter) -> std::cmp::Ordering {
        use std::cmp::Ordering;

        match (self, other) {
            (Parameter::IntegerValue(a), Parameter::IntegerValue(b)) => a.cmp(b),
            (Parameter::FloatValue(a), Parameter::FloatValue(b)) => {
                a.partial_cmp(b).unwrap_or(Ordering::Equal)
            }
            (Parameter::String(a), Parameter::String(b)) => a.cmp(b),
            (Parameter::CharValue(a), Parameter::CharValue(b)) => a.cmp(b),
            (
                Parameter::EnumeratedValue { value: a, .. },
                Parameter::EnumeratedValue { value: b, .. },
            ) => a.cmp(b),
            // If types differ, sort by type priority: Integer < Float < String < Char < Enum
            (Parameter::IntegerValue(_), _) => Ordering::Less,
            (_, Parameter::IntegerValue(_)) => Ordering::Greater,
            (Parameter::FloatValue(_), _) => Ordering::Less,
            (_, Parameter::FloatValue(_)) => Ordering::Greater,
            (Parameter::String(_), _) => Ordering::Less,
            (_, Parameter::String(_)) => Ordering::Greater,
            (Parameter::CharValue(_), _) => Ordering::Less,
            (_, Parameter::CharValue(_)) => Ordering::Greater,
            _ => Ordering::Equal,
        }
    }

    pub(crate) fn resolve_parameter(&self, parameters: &[String]) -> DdsResult<Self> {
        match self {
            Parameter::Parameter(param) => {
                let val = parameters
                    .get(*param)
                    .ok_or(DdsError::Error(format!("Parameter at index {:?} not found", param)))?;
                // TODO: Add EnumeratedValue support for enum type resolution
                // Currently only supports basic types (int, float, char, string)
                if let Ok(int_val) = val.parse::<i128>() {
                    Ok(Parameter::IntegerValue(int_val))
                } else if let Ok(float_val) = val.parse::<f64>() {
                    Ok(Parameter::FloatValue(float_val))
                } else if val.chars().count() == 1 {
                    let ch = val
                        .chars()
                        .next()
                        .ok_or(DdsError::Error("Parameter string is empty".to_string()))?;
                    Ok(Parameter::CharValue(ch))
                } else {
                    Ok(Parameter::String(val.clone()))
                }
            }
            _ => Ok(self.clone()),
        }
    }
}
