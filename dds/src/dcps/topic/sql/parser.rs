//! Parser for SQL filter expressions.
//!
//! The `Parser` transforms a sequence of tokens (from the Lexer) into an Abstract Syntax Tree
//! (AST) representing the structure of the SQL filter expression. It implements a recursive
//! descent parser following DDS Spec Annex B grammar rules.
//!
//! The parser handles expressions, predicates, operators, and ensures syntactic correctness
//! of filter queries used in ContentFilteredTopics and QueryConditions.

use regex::Regex;

use crate::{
    core::error::{DdsError, DdsResult},
    topic::sql::ast::{
        Aggregation, BinaryOp, Condition, Expression, FromClause, NaturalJoin, Parameter,
        Predicate, Range, RelOp, SelectClause, Selection, SubjectFieldSpec, Token, TokenType,
        UnaryOp, WhereClause,
    },
};

pub(crate) struct Parser {
    tokens: Vec<Token>,
    current: usize,
}

impl Parser {
    pub(crate) fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, current: 0 }
    }

    fn current_token(&self) -> Token {
        self.tokens.get(self.current).cloned().unwrap_or(
            Token {
                token_type: TokenType::Eof,
                value: Parameter::String("".to_string()),
                position: 0,
            }
            .clone(),
        )
    }

    fn next_token(&self) -> Token {
        self.tokens.get(self.current + 1).cloned().unwrap_or(
            Token {
                token_type: TokenType::Eof,
                value: Parameter::String("".to_string()),
                position: 0,
            }
            .clone(),
        )
    }

    fn advance(&mut self) {
        if self.current < self.tokens.len() {
            self.current += 1;
        }
    }

    fn match_token(&self, token_type: TokenType) -> bool {
        self.current_token().token_type == token_type
    }

    fn match_next_token(&self, token_type: TokenType) -> bool {
        self.next_token().token_type == token_type
    }

    fn expect(&mut self, expected: TokenType) -> DdsResult<Token> {
        let token = self.current_token().clone();
        if token.token_type == expected {
            self.advance();
            Ok(token)
        } else {
            Err(DdsError::Error(format!("Expected {:?}, got {:?}", expected, token.token_type)))
        }
    }

    // Expression ::= FilterExpression | TopicExpression | QueryExpression
    pub(crate) fn parse_expression(&mut self, query: bool) -> DdsResult<Expression> {
        // TopicExpression ::= SelectFrom {Where } ';'
        if self.match_token(TokenType::Select) {
            return self.parse_topic_expression();
        }

        // If query is true, QueryExpression; if false, FilterExpression
        if query {
            // QueryExpression ::= {Condition} {'ORDER BY' (FIELDNAME // ',')}
            let condition = if self.is_condition_start() {
                Some(Box::new(self.parse_condition()?))
            } else {
                None
            };

            let order_by = if self.match_token(TokenType::OrderBy)
                || (self.current_token().value == Parameter::String("Order".to_string()))
            {
                self.advance();

                if self.match_token(TokenType::OrderBy)
                    || (self.current_token().value == Parameter::String("By".to_string()))
                {
                    self.advance();
                    Some(self.parse_field_list()?)
                } else {
                    return Err(DdsError::Error("Expected BY after ORDER".to_string()));
                }
            } else {
                None
            };

            Ok(Expression::QueryExpression { condition, order_by })
        } else {
            // FilterExpression ::= Condition
            if self.is_condition_start() {
                let condition = Box::new(self.parse_condition()?);
                Ok(Expression::FilterExpression(condition))
            } else {
                Err(DdsError::Error("Expected condition for filter expression".to_string()))
            }
        }
    }

    fn is_condition_start(&self) -> bool {
        matches!(
            self.current_token().token_type,
            TokenType::Identifier
                | TokenType::Not
                | TokenType::LeftParen
                | TokenType::IntegerValue
                | TokenType::FloatValue
                | TokenType::String
                | TokenType::CharValue
                | TokenType::Parameter
        )
    }

    // TopicExpression ::= SelectFrom {Where } ';'
    fn parse_topic_expression(&mut self) -> DdsResult<Expression> {
        let select = self.parse_select_clause()?;
        let from = self.parse_from_clause()?;

        let where_clause = if self.match_token(TokenType::Where) {
            self.advance();
            Some(WhereClause { condition: Box::new(self.parse_condition()?) })
        } else {
            None
        };

        if self.match_token(TokenType::Semicolon) {
            self.advance();
        }

        Ok(Expression::TopicExpression { select, from, where_clause })
    }

    // ‘SELECT’ Aggregation
    fn parse_select_clause(&mut self) -> DdsResult<SelectClause> {
        self.expect(TokenType::Select)?;
        let aggregation = self.parse_aggregation()?;
        Ok(SelectClause { aggregation })
    }

    // Aggregation ::= '*' | (SubjectFieldSpec // ',')
    fn parse_aggregation(&mut self) -> DdsResult<Aggregation> {
        if self.match_token(TokenType::Asterisk) {
            self.advance(); // Consume asterisk token
            Ok(Aggregation::All)
        } else {
            let mut fields = Vec::new();

            loop {
                let field_spec = self.parse_subject_field_spec()?;
                fields.push(field_spec);

                if self.match_token(TokenType::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }

            if fields.is_empty() {
                return Err(DdsError::Error(
                    "Expected at least one field in aggregation".to_string(),
                ));
            }

            Ok(Aggregation::Fields(fields))
        }
    }

    // SubjectFieldSpec ::= FIELDNAME | FIELDNAME 'AS' FIELDNAME | FIELDNAME FIELDNAME
    fn parse_subject_field_spec(&mut self) -> DdsResult<SubjectFieldSpec> {
        let first_field = self.expect(TokenType::Identifier)?;

        if let Parameter::String(field_name) = first_field.value {
            if self.match_token(TokenType::As) {
                self.advance();
                let alias_token = self.expect(TokenType::Identifier)?;
                if let Parameter::String(alias) = alias_token.value {
                    Ok(SubjectFieldSpec::FieldAs(field_name, alias))
                } else {
                    Err(DdsError::Error("Expected alias after AS".to_string()))
                }
            } else if self.match_token(TokenType::Identifier) {
                let second_token = self.current_token().clone();
                self.advance();
                if let Parameter::String(second_field) = second_token.value {
                    Ok(SubjectFieldSpec::FieldField(field_name, second_field))
                } else {
                    Err(DdsError::Error("Expected second field name".to_string()))
                }
            } else {
                Ok(SubjectFieldSpec::Field(field_name))
            }
        } else {
            Err(DdsError::Error("Expected field name".to_string()))
        }
    }

    // ‘FROM’ Selection
    fn parse_from_clause(&mut self) -> DdsResult<FromClause> {
        self.expect(TokenType::From)?;
        let selection = self.parse_selection()?;
        Ok(FromClause { selection })
    }

    //     Selection ::= TOPICNAME
    //               |   TOPICTNAME NaturalJoin JoinItem
    fn parse_selection(&mut self) -> DdsResult<Selection> {
        let topic_token = if self.match_token(TokenType::Identifier) {
            self.expect(TokenType::Identifier)?
        } else if self.match_token(TokenType::String) {
            self.expect(TokenType::String)?
        } else {
            return Err(DdsError::Error("Expected topic name (Identifier or String)".to_string()));
        };

        if let Parameter::String(topic_name) = topic_token.value {
            if self.match_token(TokenType::Natural) || self.match_token(TokenType::Inner) {
                let join_type = self.parse_join_type()?;
                let right = Box::new(self.parse_selection()?);
                Ok(Selection::JoinItem { left: topic_name, join_type, right })
            } else {
                Ok(Selection::Topic(topic_name))
            }
        } else {
            Err(DdsError::Error("Expected topic name".to_string()))
        }
    }

    fn parse_join_type(&mut self) -> DdsResult<NaturalJoin> {
        if self.match_token(TokenType::Natural) {
            self.advance();
            if self.match_token(TokenType::Inner) {
                self.advance();
                self.expect(TokenType::Join)?;
                Ok(NaturalJoin::NaturalInnerJoin)
            } else {
                self.expect(TokenType::Join)?;
                Ok(NaturalJoin::NaturalJoin)
            }
        } else {
            self.expect(TokenType::Inner)?;
            self.expect(TokenType::Natural)?;
            self.expect(TokenType::Join)?;
            Ok(NaturalJoin::InnerNaturalJoin)
        }
    }

    // Condition ::= Predicate
    //           |   Condition ‘AND’ Condition
    //           |   Condition ‘OR’ Condition
    //           |   ‘NOT’ Condition
    //           |   ‘(’ Condition ‘)’
    // Predicate ::= ComparisonPredicate
    //           | BetweenPredicate
    fn parse_condition(&mut self) -> DdsResult<Condition> {
        self.parse_or_condition()
    }

    fn parse_or_condition(&mut self) -> DdsResult<Condition> {
        let mut left = self.parse_and_condition()?;

        while self.match_token(TokenType::Or) {
            self.advance();
            let right = self.parse_and_condition()?;
            left = Condition::Binary {
                left: Box::new(left),
                op: BinaryOp::Or,
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn parse_and_condition(&mut self) -> DdsResult<Condition> {
        let mut left = self.parse_unary_condition()?;

        while self.match_token(TokenType::And) {
            self.advance();
            let right = self.parse_unary_condition()?;
            left = Condition::Binary {
                left: Box::new(left),
                op: BinaryOp::And,
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn parse_unary_condition(&mut self) -> DdsResult<Condition> {
        if self.match_token(TokenType::Not) {
            let is_not_between = self.is_not_between_predicate();
            if is_not_between {
                let predicate = self.parse_predicate()?;
                Ok(Condition::Predicate(predicate))
            } else {
                self.advance();
                let operand = self.parse_primary_condition()?;
                Ok(Condition::Unary { op: UnaryOp::Not, operand: Box::new(operand) })
            }
        } else {
            self.parse_primary_condition()
        }
    }

    fn parse_primary_condition(&mut self) -> DdsResult<Condition> {
        if self.match_token(TokenType::LeftParen) {
            self.advance();
            let condition = self.parse_condition()?;
            self.expect(TokenType::RightParen)?;
            Ok(Condition::Parentheses(Box::new(condition)))
        } else {
            let predicate = self.parse_predicate()?;
            Ok(Condition::Predicate(predicate))
        }
    }

    fn parse_predicate(&mut self) -> DdsResult<Predicate> {
        let left = self.parse_parameter()?;

        if self.match_token(TokenType::Not) {
            self.advance();
            self.expect(TokenType::Between)?;
            let start = self.parse_parameter()?;
            self.expect(TokenType::And)?;
            let end = self.parse_parameter()?;
            let range = Range { start, end };

            // left must be a field name
            if let Parameter::String(field) = left {
                Ok(Predicate::Between { field, negated: true, range })
            } else {
                Err(DdsError::Error(
                    "BETWEEN predicate requires field name on left side".to_string(),
                ))
            }
        } else if self.match_token(TokenType::Between) {
            self.advance();
            let start = self.parse_parameter()?;
            self.expect(TokenType::And)?;
            let end = self.parse_parameter()?;
            let range = Range { start, end };

            if let Parameter::String(field) = left {
                Ok(Predicate::Between { field, negated: false, range })
            } else {
                Err(DdsError::Error(
                    "BETWEEN predicate requires field name on left side".to_string(),
                ))
            }
        } else if self.match_token(TokenType::Like) {
            self.advance();
            let pattern = self.parse_parameter()?;
            if let Parameter::String(pattern_str) = &pattern {
                let regex_pattern = pattern_str.replace('%', ".*").replace('_', ".");
                let regex = Regex::new(&format!("^{}$", regex_pattern))
                    .map_err(|e| DdsError::Error(format!("Invalid LIKE pattern: {}", e)))?;
                let op = RelOp::Like(regex);
                Ok(Predicate::Comparison { left, op, right: pattern })
            } else {
                Err(DdsError::Error("LIKE pattern must be a string".to_string()))
            }
        } else {
            let op = self.parse_comparison_op()?;
            let right = self.parse_parameter()?;
            Ok(Predicate::Comparison { left, op, right })
        }
    }

    fn parse_comparison_op(&mut self) -> DdsResult<RelOp> {
        let op = match self.current_token().token_type {
            TokenType::Equal => RelOp::Equal,
            TokenType::Greater => RelOp::Greater,
            TokenType::GreaterEqual => RelOp::GreaterEq,
            TokenType::Less => RelOp::Less,
            TokenType::LessEqual => RelOp::LessEq,
            TokenType::NotEqual => RelOp::NotEqual,
            TokenType::Like => {
                return Err(DdsError::Error(
                    "LIKE should be handled in parse_predicate".to_string(),
                ))
            }
            _ => return Err(DdsError::Error("Expected comparison operator".to_string())),
        };
        self.advance();
        Ok(op)
    }

    fn parse_parameter(&mut self) -> DdsResult<Parameter> {
        // TODO: Add support for EnumeratedValue parsing (TypeName::Value syntax)
        let token = self.current_token().clone();
        self.advance();

        match token.token_type {
            TokenType::IntegerValue => Ok(token.value),
            TokenType::FloatValue => Ok(token.value),
            TokenType::String => Ok(token.value),
            TokenType::CharValue => Ok(token.value),
            TokenType::Parameter => Ok(token.value),
            TokenType::Identifier => Ok(token.value), // Field name or other identifier
            _ => Err(DdsError::Error(format!(
                "Expected parameter, but found token type: {:?}, value: {:?}",
                token.token_type, token.value
            ))),
        }
    }

    fn parse_field_list(&mut self) -> DdsResult<Vec<String>> {
        let mut fields = Vec::new();

        loop {
            let token = self.expect(TokenType::Identifier)?;

            if let Parameter::String(field) = token.value {
                fields.push(field);
            }

            if self.match_token(TokenType::Comma) {
                self.advance();
            } else {
                break;
            }
        }

        Ok(fields)
    }

    fn is_not_between_predicate(&self) -> bool {
        if self.match_token(TokenType::Not) && self.match_next_token(TokenType::Between) {
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use crate::topic::sql::lexer::Lexer;

    use super::*;

    #[test]
    fn test_parse_topic_expression() {
        let mut lexer = Lexer::new("SELECT * FROM 'Location'".to_string());
        let tokens = lexer.tokenize();
        let mut parser = Parser::new(tokens);

        match parser.parse_expression(false) {
            Ok(Expression::TopicExpression { select, from, where_clause }) => {
                assert!(matches!(select.aggregation, Aggregation::All));
                assert!(matches!(from.selection, Selection::Topic(_)));
                assert!(where_clause.is_none());
            }
            Ok(result) => panic!("Expected TopicExpression, got: {:?}", result),
            Err(e) => panic!("Parse error: {}", e),
        }
    }

    #[test]
    fn test_parse_subject_field_specs() {
        // Test FIELDNAME AS FIELDNAME pattern
        let mut lexer = Lexer::new("SELECT field AS alias FROM 'Topic'".to_string());
        let tokens = lexer.tokenize();
        let mut parser = Parser::new(tokens);

        match parser.parse_expression(false) {
            Ok(Expression::TopicExpression { select, .. }) => {
                if let Aggregation::Fields(fields) = select.aggregation {
                    assert!(matches!(fields[0], SubjectFieldSpec::FieldAs(_, _)));
                }
            }
            _ => panic!("Expected TopicExpression with FieldAs"),
        }

        // Test FIELDNAME FIELDNAME pattern
        let mut lexer = Lexer::new("SELECT field alias FROM 'Topic'".to_string());
        let tokens = lexer.tokenize();
        let mut parser = Parser::new(tokens);

        match parser.parse_expression(false) {
            Ok(Expression::TopicExpression { select, .. }) => {
                if let Aggregation::Fields(fields) = select.aggregation {
                    assert!(matches!(fields[0], SubjectFieldSpec::FieldField(_, _)));
                }
            }
            _ => panic!("Expected TopicExpression with FieldField"),
        }
    }

    #[test]
    fn test_parse_complex_query() {
        let mut lexer = Lexer::new(
            "SELECT flight_name, x, y, z AS height FROM 'Location' WHERE height < 1000 AND x < 23"
                .to_string(),
        );
        let tokens = lexer.tokenize();
        let mut parser = Parser::new(tokens);

        let result = parser.parse_expression(false);
        println!("result: {:?}", result);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_query_or_filter() {
        let mut lexer = Lexer::new("height < 1000 AND x <23".to_string());
        let tokens = lexer.tokenize();
        let mut parser = Parser::new(tokens);

        let result = parser.parse_expression(false);
        println!("result: {:?}", result);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_filter_expression_basic() {
        // FilterExpression ::= Condition
        let test_cases = vec![
            (
                "height < 1000",
                Expression::FilterExpression(Box::new(Condition::Predicate(
                    Predicate::Comparison {
                        left: Parameter::String("height".to_string()),
                        op: RelOp::Less,
                        right: Parameter::IntegerValue(1000),
                    },
                ))),
            ),
            (
                "flight_name = 'ABC123'",
                Expression::FilterExpression(Box::new(Condition::Predicate(
                    Predicate::Comparison {
                        left: Parameter::String("flight_name".to_string()),
                        op: RelOp::Equal,
                        right: Parameter::String("ABC123".to_string()),
                    },
                ))),
            ),
            (
                "x >= 50.5",
                Expression::FilterExpression(Box::new(Condition::Predicate(
                    Predicate::Comparison {
                        left: Parameter::String("x".to_string()),
                        op: RelOp::GreaterEq,
                        right: Parameter::FloatValue(50.5),
                    },
                ))),
            ),
            (
                "status <> 'ACTIVE'",
                Expression::FilterExpression(Box::new(Condition::Predicate(
                    Predicate::Comparison {
                        left: Parameter::String("status".to_string()),
                        op: RelOp::NotEqual,
                        right: Parameter::String("ACTIVE".to_string()),
                    },
                ))),
            ),
        ];

        for (case, expected) in test_cases {
            let mut lexer = Lexer::new(case.to_string());
            let tokens = lexer.tokenize();
            let mut parser = Parser::new(tokens);

            match parser.parse_expression(false) {
                Ok(result) => {
                    assert_eq!(result, expected, "Mismatch for case: '{}'", case);
                }
                Err(e) => panic!("Parse error for '{}': {}", case, e),
            }
        }
    }

    #[test]
    fn test_parse_between_predicate() {
        // BetweenPredicate ::= FIELDNAME 'BETWEEN' Range | FIELDNAME 'NOT BETWEEN' Range
        let test_cases = vec![
            (
                "height BETWEEN 100 AND 1000",
                Expression::FilterExpression(Box::new(Condition::Predicate(Predicate::Between {
                    field: "height".to_string(),
                    negated: false,
                    range: Range {
                        start: Parameter::IntegerValue(100),
                        end: Parameter::IntegerValue(1000),
                    },
                }))),
            ),
            (
                "x BETWEEN 10.5 AND 50.0",
                Expression::FilterExpression(Box::new(Condition::Predicate(Predicate::Between {
                    field: "x".to_string(),
                    negated: false,
                    range: Range {
                        start: Parameter::FloatValue(10.5),
                        end: Parameter::FloatValue(50.0),
                    },
                }))),
            ),
        ];

        for (case, expected) in test_cases {
            let mut lexer = Lexer::new(case.to_string());
            let tokens = lexer.tokenize();
            let mut parser = Parser::new(tokens);

            match parser.parse_expression(false) {
                Ok(result) => {
                    assert_eq!(result, expected, "Mismatch for case: '{}'", case);
                }
                Err(e) => panic!("Parse error for '{}': {}", case, e),
            }
        }
    }

    #[test]
    fn test_parse_comparison_predicate_variants() {
        // ComparisonPredicate ::= FIELDNAME RelOp Parameter | Parameter RelOp FIELDNAME | FIELDNAME RelOp FIELDNAME
        let test_cases = vec![
            (
                "height > 1000",
                Expression::FilterExpression(Box::new(Condition::Predicate(
                    Predicate::Comparison {
                        left: Parameter::String("height".to_string()),
                        op: RelOp::Greater,
                        right: Parameter::IntegerValue(1000),
                    },
                ))),
            ),
            (
                "'ACTIVE' = status",
                Expression::FilterExpression(Box::new(Condition::Predicate(
                    Predicate::Comparison {
                        left: Parameter::String("ACTIVE".to_string()),
                        op: RelOp::Equal,
                        right: Parameter::String("status".to_string()),
                    },
                ))),
            ),
            (
                "x < y",
                Expression::FilterExpression(Box::new(Condition::Predicate(
                    Predicate::Comparison {
                        left: Parameter::String("x".to_string()),
                        op: RelOp::Less,
                        right: Parameter::String("y".to_string()),
                    },
                ))),
            ),
        ];

        for (case, expected) in test_cases {
            let mut lexer = Lexer::new(case.to_string());
            let tokens = lexer.tokenize();
            let mut parser = Parser::new(tokens);

            match parser.parse_expression(false) {
                Ok(result) => {
                    assert_eq!(result, expected, "Mismatch for case: '{}'", case);
                }
                Err(e) => panic!("Parse error for '{}': {}", case, e),
            }
        }
    }

    #[test]
    fn test_parse_complex_conditions() {
        // Test complex logical operations
        let test_cases = vec![
            (
                "height < 1000 AND x < 23",
                Expression::FilterExpression(Box::new(Condition::Binary {
                    left: Box::new(Condition::Predicate(Predicate::Comparison {
                        left: Parameter::String("height".to_string()),
                        op: RelOp::Less,
                        right: Parameter::IntegerValue(1000),
                    })),
                    op: BinaryOp::And,
                    right: Box::new(Condition::Predicate(Predicate::Comparison {
                        left: Parameter::String("x".to_string()),
                        op: RelOp::Less,
                        right: Parameter::IntegerValue(23),
                    })),
                })),
            ),
            (
                "NOT height >= 1000",
                Expression::FilterExpression(Box::new(Condition::Unary {
                    op: UnaryOp::Not,
                    operand: Box::new(Condition::Predicate(Predicate::Comparison {
                        left: Parameter::String("height".to_string()),
                        op: RelOp::GreaterEq,
                        right: Parameter::IntegerValue(1000),
                    })),
                })),
            ),
            (
                "(x > 10)",
                Expression::FilterExpression(Box::new(Condition::Parentheses(Box::new(
                    Condition::Predicate(Predicate::Comparison {
                        left: Parameter::String("x".to_string()),
                        op: RelOp::Greater,
                        right: Parameter::IntegerValue(10),
                    }),
                )))),
            ),
        ];

        for (case, expected) in test_cases {
            let mut lexer = Lexer::new(case.to_string());
            let tokens = lexer.tokenize();
            let mut parser = Parser::new(tokens);

            match parser.parse_expression(false) {
                Ok(result) => {
                    assert_eq!(result, expected, "Mismatch for case: '{}'", case);
                }
                Err(e) => panic!("Parse error for '{}': {}", case, e),
            }
        }
    }

    #[test]
    fn test_parse_natural_join_topic_expression() {
        // TopicExpression with NATURAL JOIN
        let test_cases = vec![
            (
                "SELECT * FROM 'Location' NATURAL JOIN 'FlightPlan'",
                Expression::TopicExpression {
                    select: SelectClause { aggregation: Aggregation::All },
                    from: FromClause {
                        selection: Selection::JoinItem {
                            left: "Location".to_string(),
                            join_type: NaturalJoin::NaturalJoin,
                            right: Box::new(Selection::Topic("FlightPlan".to_string())),
                        },
                    },
                    where_clause: None,
                },
            ),
            (
                "SELECT flight_name FROM 'Location' INNER NATURAL JOIN 'FlightPlan'",
                Expression::TopicExpression {
                    select: SelectClause {
                        aggregation: Aggregation::Fields(vec![SubjectFieldSpec::Field(
                            "flight_name".to_string(),
                        )]),
                    },
                    from: FromClause {
                        selection: Selection::JoinItem {
                            left: "Location".to_string(),
                            join_type: NaturalJoin::InnerNaturalJoin,
                            right: Box::new(Selection::Topic("FlightPlan".to_string())),
                        },
                    },
                    where_clause: None,
                },
            ),
        ];

        for (case, expected) in test_cases {
            let mut lexer = Lexer::new(case.to_string());
            let tokens = lexer.tokenize();
            let mut parser = Parser::new(tokens);

            match parser.parse_expression(false) {
                Ok(result) => {
                    assert_eq!(result, expected, "Mismatch for case: '{}'", case);
                }
                Err(e) => panic!("Parse error for '{}': {}", case, e),
            }
        }
    }

    #[test]
    fn test_parse_query_expression_with_order_by() {
        // QueryExpression ::= {Condition}{'ORDER BY' (FIELDNAME // ',') }
        let test_cases = vec![
            (
                "height < 1000 ORDER BY flight_name",
                Expression::QueryExpression {
                    condition: Some(Box::new(Condition::Predicate(Predicate::Comparison {
                        left: Parameter::String("height".to_string()),
                        op: RelOp::Less,
                        right: Parameter::IntegerValue(1000),
                    }))),
                    order_by: Some(vec!["flight_name".to_string()]),
                },
            ),
            (
                "x > 10 ORDER BY x, y, z",
                Expression::QueryExpression {
                    condition: Some(Box::new(Condition::Predicate(Predicate::Comparison {
                        left: Parameter::String("x".to_string()),
                        op: RelOp::Greater,
                        right: Parameter::IntegerValue(10),
                    }))),
                    order_by: Some(vec!["x".to_string(), "y".to_string(), "z".to_string()]),
                },
            ),
        ];

        for (case, expected) in test_cases {
            let mut lexer = Lexer::new(case.to_string());
            let tokens = lexer.tokenize();
            let mut parser = Parser::new(tokens);

            match parser.parse_expression(true) {
                Ok(result) => {
                    assert_eq!(result, expected, "Mismatch for case: '{}'", case);
                }
                Err(e) => panic!("Parse error for '{}': {}", case, e),
            }
        }
    }

    #[test]
    fn test_parse_parameter_types() {
        // Parameter ::= INTEGERVALUE | CHARVALUE | FLOATVALUE | STRING | ENUMERATEDVALUE | PARAMETER
        let test_cases = vec![
            ("x = 42", "INTEGERVALUE"),
            ("status = 'A'", "CHARVALUE"),
            ("height = 123.45", "FLOATVALUE"),
            ("name = 'test string'", "STRING"),
            ("level = 'HIGH'", "ENUMERATEDVALUE"),
            ("id = %0", "PARAMETER"),
            ("value = %10", "PARAMETER"),
        ];

        for (case, param_type) in test_cases {
            let mut lexer = Lexer::new(case.to_string());
            let tokens = lexer.tokenize();
            let mut parser = Parser::new(tokens);

            let result = parser.parse_expression(false);
            assert!(
                result.is_ok(),
                "Failed to parse {} parameter ({}): {:?}",
                param_type,
                case,
                result
            );
        }
    }

    #[test]
    fn test_parse_nested_field_names() {
        // FIELDNAME with dots for nested structures
        let test_cases = vec![
            "position.x > 100",
            "aircraft.engine.temperature < 500",
            "flight.route.waypoints.altitude >= 30000",
            "sensor.data.readings.value = 42.5",
        ];

        for case in test_cases {
            let mut lexer = Lexer::new(case.to_string());
            let tokens = lexer.tokenize();
            let mut parser = Parser::new(tokens);

            let result = parser.parse_expression(false);
            assert!(result.is_ok(), "Failed to parse nested field: {}", case);
        }
    }

    #[test]
    fn test_parse_hex_and_scientific_notation() {
        // INTEGERVALUE with hex, FLOATVALUE with scientific notation
        let test_cases = vec![
            "flags = 0xFF",
            "mask = 0x1A2B",
            "value = 1.23e5",
            "threshold = -4.56e-3",
            "coefficient = +2.0e+10",
        ];

        for case in test_cases {
            let mut lexer = Lexer::new(case.to_string());
            let tokens = lexer.tokenize();
            let mut parser = Parser::new(tokens);

            let result = parser.parse_expression(false);
            assert!(result.is_ok(), "Failed to parse numeric format: {}", case);
        }
    }

    #[test]
    fn test_parse_spec_examples() {
        // Examples from DDS Spec Annex B

        // Topic expression example
        let topic_expr = "SELECT flight_name, x, y, z AS height FROM 'Location' NATURAL JOIN 'FlightPlan' WHERE height < 1000 AND x < 23";
        let mut lexer = Lexer::new(topic_expr.to_string());
        let tokens = lexer.tokenize();
        let mut parser = Parser::new(tokens);

        match parser.parse_expression(false) {
            Ok(Expression::TopicExpression { .. }) => {}
            Ok(other) => panic!("Expected TopicExpression for spec example, got {:?}", other),
            Err(e) => panic!("Parse error for spec topic example: {}", e),
        }

        // Query/Filter expression example
        let filter_expr = "height < 1000 AND x < 23";
        let mut lexer = Lexer::new(filter_expr.to_string());
        let tokens = lexer.tokenize();
        let mut parser = Parser::new(tokens);

        let result = parser.parse_expression(false);
        assert!(result.is_ok(), "Failed to parse spec filter example");
    }
}
