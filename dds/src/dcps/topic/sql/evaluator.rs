//! Expression evaluator for SQL filter expressions.
//!
//! This module implements the evaluation logic for parsed SQL filter expressions.
//! It takes an AST (Abstract Syntax Tree) and evaluates it against actual data samples,
//! determining whether the sample matches the filter criteria.
//!
//! The evaluator supports field access, comparisons, logical operations, and parameter
//! substitution, allowing ContentFilteredTopics and QueryConditions to filter data
//! based on content.

use crate::{
    core::error::{DdsError, DdsResult},
    topic::sql::ast::{
        BinaryOp, Condition, Expression, Parameter, Predicate, Range, RelOp, UnaryOp,
    },
    DdsType,
};

impl Expression {
    pub(crate) fn evaluate<Foo>(&self, data: &Foo, parameters: &[String]) -> DdsResult<bool>
    where
        Foo: DdsType,
    {
        match self {
            // ContentFilteredTopic
            Expression::FilterExpression(condition) => condition.evaluate(data, parameters),
            // MultiTopic
            Expression::TopicExpression { select: _select, from: _from, where_clause } => {
                // TODO: Implement Select From
                if let Some(where_clause) = where_clause {
                    where_clause.condition.evaluate(data, parameters)
                } else {
                    Ok(true)
                }
            }
            // QueryReadCondition
            Expression::QueryExpression { condition, order_by: _order_by } => {
                // TODO: Implement Order By
                if let Some(cond) = condition {
                    cond.evaluate(data, parameters)
                } else {
                    Ok(true)
                }
            }
        }
    }
}

impl Condition {
    pub(crate) fn evaluate<Foo>(&self, data: &Foo, parameters: &[String]) -> DdsResult<bool>
    where
        Foo: DdsType,
    {
        match self {
            Condition::Predicate(predicate) => predicate.evaluate(data, parameters),
            Condition::Binary { left, op, right } => {
                let left_eval = left.evaluate(data, parameters)?;
                let right_eval = right.evaluate(data, parameters)?;
                match op {
                    BinaryOp::And => Ok(left_eval && right_eval),
                    BinaryOp::Or => Ok(left_eval || right_eval),
                }
            }
            Condition::Unary { op, operand } => {
                let oper_eval = operand.evaluate(data, parameters)?;
                match op {
                    UnaryOp::Not => Ok(!oper_eval),
                }
            }
            Condition::Parentheses(condition) => condition.evaluate(data, parameters),
        }
    }
}

impl Predicate {
    pub(crate) fn evaluate<Foo>(&self, data: &Foo, parameters: &[String]) -> DdsResult<bool>
    where
        Foo: DdsType,
    {
        match self {
            Predicate::Comparison { left, op, right } => {
                let resolved_left = left.resolve_parameter(parameters)?;
                let resolved_right = right.resolve_parameter(parameters)?;

                match (&resolved_left, &resolved_right) {
                    (Parameter::String(field1), Parameter::String(field2))
                        if data.has_field(field1)? && data.has_field(field2)? =>
                    {
                        let field_val1 = data.get_field_value(field1)?;
                        let field_val2 = data.get_field_value(field2)?;
                        op.compare_parameters(&field_val1, &field_val2)
                    }
                    (Parameter::String(field), value) if data.has_field(field)? => {
                        let field_val = data.get_field_value(field)?;
                        op.compare_parameters(&field_val, value)
                    }
                    (value, Parameter::String(field)) if data.has_field(field)? => {
                        let field_val = data.get_field_value(field)?;
                        op.compare_parameters(value, &field_val)
                    }
                    _ => Err(DdsError::Error(format!(
                        "Cannot compare parameters: left={:?}, right={:?} - incompatible types or field not found",
                        resolved_left, resolved_right
                    ))),
                }
            }
            Predicate::Between { field, negated, range } => {
                let field_value = data.get_field_value(field)?;
                if matches!(field_value, Parameter::Unset) {
                    return Ok(false);
                }
                if *negated {
                    match (&field_value, &range.start, &range.end) {
                        (
                            Parameter::IntegerValue(value),
                            Parameter::IntegerValue(start),
                            Parameter::IntegerValue(end),
                        ) => Ok(start > value || end < value),
                        (
                            Parameter::CharValue(value),
                            Parameter::CharValue(start),
                            Parameter::CharValue(end),
                        ) => Ok(start > value || end < value),
                        (
                            Parameter::FloatValue(value),
                            Parameter::FloatValue(start),
                            Parameter::FloatValue(end),
                        ) => Ok(start > value || end < value),
                        (
                            Parameter::String(value),
                            Parameter::String(start),
                            Parameter::String(end),
                        ) => Ok(start > value || end < value),
                        (
                            Parameter::EnumeratedValue { type_name, value },
                            Parameter::EnumeratedValue { type_name: start_type_name, value: start },
                            Parameter::EnumeratedValue { type_name: end_type_name, value: end },
                        ) => {
                            if type_name == start_type_name && type_name == end_type_name {
                                Ok(start > value || end < value)
                            } else {
                                Err(DdsError::Error(format!(
                                    "Enum type mismatch in NOT BETWEEN for field '{}': range types '{}' and '{}' don't match field enum type '{}'",
                                    field,
                                    start_type_name.as_deref().unwrap_or("unknown"),
                                    end_type_name.as_deref().unwrap_or("unknown"),
                                    type_name.as_deref().unwrap_or("unknown")
                                )))
                            }
                        }
                        (
                            Parameter::FloatValue(value),
                            Parameter::IntegerValue(start),
                            Parameter::IntegerValue(end),
                        ) => Ok((*start as f64) > *value || (*end as f64) < *value),
                        (
                            Parameter::IntegerValue(value),
                            Parameter::FloatValue(start),
                            Parameter::IntegerValue(end),
                        ) => Ok(*start > (*value as f64) || (*end as f64) < (*value as f64)),
                        (
                            Parameter::IntegerValue(value),
                            Parameter::IntegerValue(start),
                            Parameter::FloatValue(end),
                        ) => Ok((*start as f64) > (*value as f64) || *end < (*value as f64)),
                        (
                            Parameter::FloatValue(value),
                            Parameter::IntegerValue(start),
                            Parameter::FloatValue(end),
                        ) => Ok((*start as f64) > *value || *end < *value),
                        (
                            Parameter::FloatValue(value),
                            Parameter::FloatValue(start),
                            Parameter::IntegerValue(end),
                        ) => Ok(*start > *value || (*end as f64) < *value),
                        (
                            Parameter::IntegerValue(value),
                            Parameter::FloatValue(start),
                            Parameter::FloatValue(end),
                        ) => Ok(*start > (*value as f64) || *end < (*value as f64)),
                        (_, Parameter::Parameter(_), Parameter::Parameter(_)) => {
                            let resolved_start = range.start.resolve_parameter(parameters)?;
                            let resolved_end = range.end.resolve_parameter(parameters)?;

                            let new_range = Range { start: resolved_start, end: resolved_end };
                            let new_predicate = Predicate::Between {
                                field: field.clone(),
                                negated: *negated,
                                range: new_range,
                            };
                            new_predicate.evaluate(data, parameters)
                        }
                        _ => Err(DdsError::Error(format!(
                            "Type mismatch in NOT BETWEEN for field '{}': field type {:?} doesn't match range types {:?} and {:?}",
                            field, field_value, range.start, range.end
                        ))),
                    }
                } else {
                    match (&field_value, &range.start, &range.end) {
                        (
                            Parameter::IntegerValue(value),
                            Parameter::IntegerValue(start),
                            Parameter::IntegerValue(end),
                        ) => Ok(start <= value && value <= end),
                        (
                            Parameter::CharValue(value),
                            Parameter::CharValue(start),
                            Parameter::CharValue(end),
                        ) => Ok(start <= value && value <= end),
                        (
                            Parameter::FloatValue(value),
                            Parameter::FloatValue(start),
                            Parameter::FloatValue(end),
                        ) => Ok(start <= value && value <= end),
                        (
                            Parameter::String(value),
                            Parameter::String(start),
                            Parameter::String(end),
                        ) => Ok(start <= value && value <= end),
                        (
                            Parameter::EnumeratedValue { type_name, value },
                            Parameter::EnumeratedValue { type_name: start_type_name, value: start },
                            Parameter::EnumeratedValue { type_name: end_type_name, value: end },
                        ) => {
                            if type_name == start_type_name && type_name == end_type_name {
                                Ok(start <= value && value <= end)
                            } else {
                                Err(DdsError::Error(format!(
                                    "Enum type mismatch in BETWEEN for field '{}': range types '{}' and '{}' don't match field enum type '{}'",
                                    field,
                                    start_type_name.as_deref().unwrap_or("unknown"),
                                    end_type_name.as_deref().unwrap_or("unknown"),
                                    type_name.as_deref().unwrap_or("unknown")
                                )))
                            }
                        }
                        (
                            Parameter::FloatValue(value),
                            Parameter::IntegerValue(start),
                            Parameter::IntegerValue(end),
                        ) => Ok((*start as f64) <= *value && *value <= (*end as f64)),
                        (
                            Parameter::IntegerValue(value),
                            Parameter::FloatValue(start),
                            Parameter::IntegerValue(end),
                        ) => Ok(*start <= (*value as f64) && (*value as f64) <= (*end as f64)),
                        (
                            Parameter::IntegerValue(value),
                            Parameter::IntegerValue(start),
                            Parameter::FloatValue(end),
                        ) => Ok((*start as f64) <= (*value as f64) && (*value as f64) <= *end),
                        (
                            Parameter::FloatValue(value),
                            Parameter::IntegerValue(start),
                            Parameter::FloatValue(end),
                        ) => Ok((*start as f64) <= *value && *value <= *end),
                        (
                            Parameter::FloatValue(value),
                            Parameter::FloatValue(start),
                            Parameter::IntegerValue(end),
                        ) => Ok(*start <= *value && *value <= (*end as f64)),
                        (_, Parameter::Parameter(_), Parameter::Parameter(_)) => {
                            let resolved_start = range.start.resolve_parameter(parameters)?;
                            let resolved_end = range.end.resolve_parameter(parameters)?;

                            let new_range = Range { start: resolved_start, end: resolved_end };
                            let new_predicate = Predicate::Between {
                                field: field.clone(),
                                negated: *negated,
                                range: new_range,
                            };
                            new_predicate.evaluate(data, parameters)
                        }
                        _ => Err(DdsError::Error(format!(
                            "Type mismatch in BETWEEN for field '{}': field type {:?} doesn't match range types {:?} and {:?}",
                            field, field_value, range.start, range.end
                        ))),
                    }
                }
            }
        }
    }
}

impl RelOp {
    fn compare_parameters(&self, left: &Parameter, right: &Parameter) -> DdsResult<bool> {
        // RTI parity: any comparison involving an unset member is false, so the
        // other branch of an OR still gets evaluated.
        if matches!(left, Parameter::Unset) || matches!(right, Parameter::Unset) {
            return Ok(false);
        }
        match self {
            RelOp::Equal => match (left, right) {
                (Parameter::IntegerValue(left), Parameter::IntegerValue(right)) => {
                    Ok(left == right)
                }
                (Parameter::CharValue(left), Parameter::CharValue(right)) => Ok(left == right),
                (Parameter::FloatValue(left), Parameter::FloatValue(right)) => {
                    Ok((left - right).abs() < f64::EPSILON)
                }
                (Parameter::String(left), Parameter::String(right)) => Ok(left == right),
                (
                    Parameter::EnumeratedValue { type_name: left_type_name, value: left },
                    Parameter::EnumeratedValue { type_name: right_type_name, value: right },
                ) => {
                    if left_type_name == right_type_name {
                        Ok(left == right)
                    } else {
                        Err(DdsError::Error(format!(
                            "Cannot compare enum types: '{}' and '{}' are different enum types",
                            left_type_name.as_deref().unwrap_or("unknown"),
                            right_type_name.as_deref().unwrap_or("unknown")
                        )))
                    }
                }
                (Parameter::IntegerValue(left), Parameter::FloatValue(right)) => {
                    Ok((*left as f64 - right).abs() < f64::EPSILON)
                }
                (Parameter::FloatValue(left), Parameter::IntegerValue(right)) => {
                    Ok((left - *right as f64).abs() < f64::EPSILON)
                }
                _ => Err(DdsError::Error(format!(
                    "Cannot compare {:?} and {:?} with EQUAL operator: incompatible types",
                    left, right
                ))),
            },
            RelOp::Greater => match (left, right) {
                (Parameter::IntegerValue(left), Parameter::IntegerValue(right)) => Ok(left > right),
                (Parameter::CharValue(left), Parameter::CharValue(right)) => Ok(left > right),
                (Parameter::FloatValue(left), Parameter::FloatValue(right)) => Ok(left > right),
                (Parameter::String(left), Parameter::String(right)) => Ok(left > right),
                (
                    Parameter::EnumeratedValue { type_name: left_type_name, value: left },
                    Parameter::EnumeratedValue { type_name: right_type_name, value: right },
                ) => {
                    if left_type_name == right_type_name {
                        Ok(left > right)
                    } else {
                        Err(DdsError::Error(format!(
                            "Cannot compare enum types: '{}' and '{}' are different enum types",
                            left_type_name.as_deref().unwrap_or("unknown"),
                            right_type_name.as_deref().unwrap_or("unknown")
                        )))
                    }
                }
                (Parameter::IntegerValue(left), Parameter::FloatValue(right)) => {
                    Ok((*left as f64) > *right)
                }
                (Parameter::FloatValue(left), Parameter::IntegerValue(right)) => {
                    Ok(*left > (*right as f64))
                }
                _ => Err(DdsError::Error(format!(
                    "Cannot compare {:?} and {:?} with GREATER operator: incompatible types",
                    left, right
                ))),
            },
            RelOp::GreaterEq => match (left, right) {
                (Parameter::IntegerValue(left), Parameter::IntegerValue(right)) => {
                    Ok(left >= right)
                }
                (Parameter::CharValue(left), Parameter::CharValue(right)) => Ok(left >= right),
                (Parameter::FloatValue(left), Parameter::FloatValue(right)) => Ok(left >= right),
                (Parameter::String(left), Parameter::String(right)) => Ok(left >= right),
                (
                    Parameter::EnumeratedValue { type_name: left_type_name, value: left },
                    Parameter::EnumeratedValue { type_name: right_type_name, value: right },
                ) => {
                    if left_type_name == right_type_name {
                        Ok(left >= right)
                    } else {
                        Err(DdsError::Error(format!(
                            "Cannot compare enum types: '{}' and '{}' are different enum types",
                            left_type_name.as_deref().unwrap_or("unknown"),
                            right_type_name.as_deref().unwrap_or("unknown")
                        )))
                    }
                }
                (Parameter::IntegerValue(left), Parameter::FloatValue(right)) => {
                    Ok((*left as f64) >= *right)
                }
                (Parameter::FloatValue(left), Parameter::IntegerValue(right)) => {
                    Ok(*left >= (*right as f64))
                }
                _ => Err(DdsError::Error(format!(
                    "Cannot compare {:?} and {:?} with GREATER EQUAL operator: incompatible types",
                    left, right
                ))),
            },
            RelOp::Less => match (left, right) {
                (Parameter::IntegerValue(left), Parameter::IntegerValue(right)) => Ok(left < right),
                (Parameter::CharValue(left), Parameter::CharValue(right)) => Ok(left < right),
                (Parameter::FloatValue(left), Parameter::FloatValue(right)) => Ok(left < right),
                (Parameter::String(left), Parameter::String(right)) => Ok(left < right),
                (
                    Parameter::EnumeratedValue { type_name: left_type_name, value: left },
                    Parameter::EnumeratedValue { type_name: right_type_name, value: right },
                ) => {
                    if left_type_name == right_type_name {
                        Ok(left < right)
                    } else {
                        Err(DdsError::Error(format!(
                            "Cannot compare enum types: '{}' and '{}' are different enum types",
                            left_type_name.as_deref().unwrap_or("unknown"),
                            right_type_name.as_deref().unwrap_or("unknown")
                        )))
                    }
                }
                (Parameter::IntegerValue(left), Parameter::FloatValue(right)) => {
                    Ok((*left as f64) < *right)
                }
                (Parameter::FloatValue(left), Parameter::IntegerValue(right)) => {
                    Ok(*left < (*right as f64))
                }
                _ => Err(DdsError::Error(format!(
                    "Cannot compare {:?} and {:?} with LESS operator: incompatible types",
                    left, right
                ))),
            },
            RelOp::LessEq => match (left, right) {
                (Parameter::IntegerValue(left), Parameter::IntegerValue(right)) => {
                    Ok(left <= right)
                }
                (Parameter::CharValue(left), Parameter::CharValue(right)) => Ok(left <= right),
                (Parameter::FloatValue(left), Parameter::FloatValue(right)) => Ok(left <= right),
                (Parameter::String(left), Parameter::String(right)) => Ok(left <= right),
                (
                    Parameter::EnumeratedValue { type_name: left_type_name, value: left },
                    Parameter::EnumeratedValue { type_name: right_type_name, value: right },
                ) => {
                    if left_type_name == right_type_name {
                        Ok(left <= right)
                    } else {
                        Err(DdsError::Error(format!(
                            "Cannot compare enum types: '{}' and '{}' are different enum types",
                            left_type_name.as_deref().unwrap_or("unknown"),
                            right_type_name.as_deref().unwrap_or("unknown")
                        )))
                    }
                }
                (Parameter::IntegerValue(left), Parameter::FloatValue(right)) => {
                    Ok((*left as f64) <= *right)
                }
                (Parameter::FloatValue(left), Parameter::IntegerValue(right)) => {
                    Ok(*left <= (*right as f64))
                }
                _ => Err(DdsError::Error(format!(
                    "Cannot compare {:?} and {:?} with LESS EQUAL operator: incompatible types",
                    left, right
                ))),
            },
            RelOp::NotEqual => match (left, right) {
                (Parameter::IntegerValue(left), Parameter::IntegerValue(right)) => {
                    Ok(left != right)
                }
                (Parameter::CharValue(left), Parameter::CharValue(right)) => Ok(left != right),
                (Parameter::FloatValue(left), Parameter::FloatValue(right)) => {
                    Ok((left - right).abs() >= f64::EPSILON)
                }
                (Parameter::String(left), Parameter::String(right)) => Ok(left != right),
                (
                    Parameter::EnumeratedValue { type_name: left_type_name, value: left },
                    Parameter::EnumeratedValue { type_name: right_type_name, value: right },
                ) => Ok((left_type_name != right_type_name) || (left != right)),
                (Parameter::IntegerValue(left), Parameter::FloatValue(right)) => {
                    Ok((*left as f64 - right).abs() >= f64::EPSILON)
                }
                (Parameter::FloatValue(left), Parameter::IntegerValue(right)) => {
                    Ok((left - *right as f64).abs() >= f64::EPSILON)
                }
                _ => Err(DdsError::Error(format!(
                    "Cannot compare {:?} and {:?} with NOT EQUAL operator: incompatible types",
                    left, right
                ))),
            },
            RelOp::Like(compiled_regex) => match (left, right) {
                (Parameter::String(text), Parameter::String(_pattern)) => {
                    Ok(compiled_regex.is_match(text))
                }
                _ => Err(DdsError::Error(format!(
                    "LIKE operator requires string operands, got {:?} and {:?}",
                    left, right
                ))),
            },
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::topic::sql::{lexer::Lexer, parser::Parser};

    #[derive(DdsType)]
    struct TestData {
        pub id: i32,
        pub name: String,
        pub score: f64,
        pub grade: char,
        pub active: bool,
    }

    impl TestData {
        fn new(id: i32, name: &str, score: f64, grade: char, active: bool) -> Self {
            TestData { id, name: name.to_string(), score, grade, active }
        }
    }

    #[derive(DdsType)]
    struct WideData {
        pub seq: i64,
    }

    #[test]
    fn test_int64_filter_full_width_comparison() {
        let eval = |seq: i64| {
            let data = WideData { seq };
            let mut lexer = Lexer::new("seq > 3000000000".to_string());
            let tokens = lexer.tokenize();
            let mut parser = Parser::new(tokens);
            let expression = parser.parse_expression(false).unwrap();
            expression.evaluate(&data, &["".to_string()]).unwrap()
        };

        assert!(eval(3_000_000_001), "value above the bound must pass");
        assert!(!eval(2_999_999_999), "value below the bound must be filtered");
    }

    #[test]
    fn test_simple_equal_comparison() {
        let test_data = TestData::new(1, "Alice", 85.5, 'A', true);

        let mut lexer = Lexer::new("grade <> 'A'".to_string());
        let tokens = lexer.tokenize();
        let mut parser = Parser::new(tokens);

        let expression = parser.parse_expression(false).unwrap();
        let result = expression.evaluate(&test_data, &["".to_string()]).unwrap();

        assert_eq!(result, false);

        let mut lexer = Lexer::new("grade = 'A'".to_string());
        let tokens = lexer.tokenize();
        let mut parser = Parser::new(tokens);

        let expression = parser.parse_expression(false).unwrap();
        let result = expression.evaluate(&test_data, &["".to_string()]).unwrap();

        assert_eq!(result, true);
    }

    #[test]
    fn test_simple_between_comparison() {
        let test_data = TestData::new(1, "Alice", 85.5, 'A', true);

        let mut lexer = Lexer::new("score NOT BETWEEN 90 AND 100".to_string());
        let tokens = lexer.tokenize();
        let mut parser = Parser::new(tokens);

        let expression = parser.parse_expression(false).unwrap();
        let result = expression.evaluate(&test_data, &["".to_string()]).unwrap();

        assert_eq!(result, true);

        let mut lexer = Lexer::new("score BETWEEN 80 AND 90".to_string());
        let tokens = lexer.tokenize();
        let mut parser = Parser::new(tokens);

        let expression = parser.parse_expression(false).unwrap();
        let result = expression.evaluate(&test_data, &["".to_string()]).unwrap();

        assert_eq!(result, true);
    }

    #[test]
    fn test_like_patterns() {
        let test_data = TestData::new(1, "Alice", 85.5, 'A', true);

        let mut lexer = Lexer::new("name LIKE 'Al%'".to_string());
        let tokens = lexer.tokenize();
        let mut parser = Parser::new(tokens);

        let expression = parser.parse_expression(false).unwrap();
        let result = expression.evaluate(&test_data, &["".to_string()]).unwrap();

        assert_eq!(result, true);

        let mut lexer = Lexer::new("name LIKE 'Bob%'".to_string());
        let tokens = lexer.tokenize();
        let mut parser = Parser::new(tokens);

        let expression = parser.parse_expression(false).unwrap();
        let result = expression.evaluate(&test_data, &["".to_string()]).unwrap();

        assert_eq!(result, false);

        let mut lexer = Lexer::new("name LIKE 'A____'".to_string());
        let tokens = lexer.tokenize();
        let mut parser = Parser::new(tokens);

        let expression = parser.parse_expression(false).unwrap();
        let result = expression.evaluate(&test_data, &["".to_string()]).unwrap();

        assert_eq!(result, true);

        let mut lexer = Lexer::new("name LIKE 'A_'".to_string());
        let tokens = lexer.tokenize();
        let mut parser = Parser::new(tokens);

        let expression = parser.parse_expression(false).unwrap();
        let result = expression.evaluate(&test_data, &["".to_string()]).unwrap();

        assert_eq!(result, false);
    }

    #[test]
    fn test_parameter_substitution() {
        let test_data = TestData::new(1, "Alice", 85.5, 'A', true);

        let mut lexer = Lexer::new("score > %0".to_string());
        let tokens = lexer.tokenize();
        let mut parser = Parser::new(tokens);

        let expression = parser.parse_expression(false).unwrap();
        let result = expression.evaluate(&test_data, &["80".to_string()]).unwrap();

        assert_eq!(result, true);

        let mut lexer = Lexer::new("score > %0".to_string());
        let tokens = lexer.tokenize();
        let mut parser = Parser::new(tokens);

        let expression = parser.parse_expression(false).unwrap();
        let result = expression.evaluate(&test_data, &["90".to_string()]).unwrap();

        assert_eq!(result, false);

        let mut lexer = Lexer::new("name = %1".to_string());
        let tokens = lexer.tokenize();
        let mut parser = Parser::new(tokens);

        let expression = parser.parse_expression(false).unwrap();
        let result =
            expression.evaluate(&test_data, &["".to_string(), "Alice".to_string()]).unwrap();

        assert_eq!(result, true);

        let mut lexer = Lexer::new("name <> %1".to_string());
        let tokens = lexer.tokenize();
        let mut parser = Parser::new(tokens);

        let expression = parser.parse_expression(false).unwrap();
        let result =
            expression.evaluate(&test_data, &["".to_string(), "Alice".to_string()]).unwrap();

        assert_eq!(result, false);
    }

    #[test]
    fn test_operator_precedence_and_or() {
        let test_data = TestData::new(1, "Alice", 85.5, 'A', true);

        let mut lexer = Lexer::new("score > 80 OR grade = 'B' AND active = 1".to_string());
        let tokens = lexer.tokenize();
        let mut parser = Parser::new(tokens);

        let expression = parser.parse_expression(false).unwrap();
        let result = expression.evaluate(&test_data, &["".to_string()]).unwrap();

        assert_eq!(result, true);
    }

    #[test]
    fn test_complex_between_with_logical_operators() {
        let test_data = TestData::new(1, "Alice", 85.5, 'A', true);

        let mut lexer =
            Lexer::new("score BETWEEN 80 AND 90 AND (name LIKE 'A%' OR grade = 'B')".to_string());
        let tokens = lexer.tokenize();
        let mut parser = Parser::new(tokens);

        let expression = parser.parse_expression(false).unwrap();
        let result = expression.evaluate(&test_data, &["".to_string()]).unwrap();

        assert_eq!(result, true);
    }

    #[test]
    fn test_not_with_parentheses_grouping() {
        let test_data = TestData::new(1, "Alice", 85.5, 'A', true);

        let mut lexer = Lexer::new("NOT (score < 50 OR grade = 'F')".to_string());
        let tokens = lexer.tokenize();
        let mut parser = Parser::new(tokens);

        let expression = parser.parse_expression(false).unwrap();
        let result = expression.evaluate(&test_data, &["".to_string()]).unwrap();

        assert_eq!(result, true);
    }

    #[test]
    fn test_nested_parentheses_complex_conditions() {
        let test_data = TestData::new(1, "Alice", 85.5, 'A', true);

        let mut lexer = Lexer::new(
            "(score > 80 AND grade = 'A') AND (name LIKE '%ce' AND active = 1)".to_string(),
        );
        let tokens = lexer.tokenize();
        let mut parser = Parser::new(tokens);

        let expression = parser.parse_expression(false).unwrap();
        let result = expression.evaluate(&test_data, &["".to_string()]).unwrap();

        assert_eq!(result, true);
    }

    #[test]
    fn test_mixed_like_and_negation_logic() {
        let test_data = TestData::new(1, "Alice", 85.5, 'A', true);

        let mut lexer =
            Lexer::new("name LIKE 'A%' AND NOT (score < 80 OR grade <> 'A')".to_string());
        let tokens = lexer.tokenize();
        let mut parser = Parser::new(tokens);

        let expression = parser.parse_expression(false).unwrap();
        let result = expression.evaluate(&test_data, &["".to_string()]).unwrap();

        assert_eq!(result, true);
    }
}
