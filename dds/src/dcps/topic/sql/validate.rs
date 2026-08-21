//! Validator for SQL filter expressions.
//!
//! This module provides validation logic for SQL filter expressions, ensuring that
//! parameter references are valid and that the expression structure is semantically correct.
//! Validation occurs before evaluation to catch errors early.

use crate::{
    core::error::{DdsError, DdsResult},
    topic::sql::ast::{Condition, Expression, Parameter, Predicate},
};

impl Expression {
    pub(crate) fn validate_expression_parameters(
        &self,
        expression_parameters: &[String],
    ) -> DdsResult<()> {
        match self {
            Expression::FilterExpression(condition) => {
                condition.validate_expression_parameters(expression_parameters)
            }
            Expression::QueryExpression { condition, order_by: _ } => {
                if let Some(condition) = condition {
                    condition.validate_expression_parameters(expression_parameters)
                } else {
                    Ok(())
                }
            }
            Expression::TopicExpression { select: _, from: _, where_clause } => {
                if let Some(where_clause) = where_clause {
                    where_clause.condition.validate_expression_parameters(expression_parameters)
                } else {
                    Ok(())
                }
            }
        }
    }
}

impl Condition {
    pub(crate) fn validate_expression_parameters(
        &self,
        expression_parameters: &[String],
    ) -> DdsResult<()> {
        match self {
            Condition::Predicate(predicate) => {
                predicate.validate_expression_parameters(expression_parameters)
            }
            Condition::Binary { left, op: _, right } => {
                left.validate_expression_parameters(expression_parameters)?;
                right.validate_expression_parameters(expression_parameters)
            }
            Condition::Unary { op: _, operand } => {
                operand.validate_expression_parameters(expression_parameters)
            }
            Condition::Parentheses(condition) => {
                condition.validate_expression_parameters(expression_parameters)
            }
        }
    }
}

impl Predicate {
    pub(crate) fn validate_expression_parameters(
        &self,
        expression_parameters: &[String],
    ) -> DdsResult<()> {
        match self {
            Predicate::Comparison { left, op: _, right } => {
                left.validate_expression_parameters(expression_parameters)?;
                right.validate_expression_parameters(expression_parameters)
            }
            Predicate::Between { field: _, negated: _, range } => {
                range.start.validate_expression_parameters(expression_parameters)?;
                range.end.validate_expression_parameters(expression_parameters)
            }
        }
    }
}

impl Expression {
    /// Validate the fields the expression references against the topic type
    /// (RTI parity: an expression that can never match fails at creation).
    ///
    /// `field_exists` answers from the same metadata the runtime filter uses;
    /// `None` means no metadata, and that field is accepted unchecked. A
    /// comparison with a `%n` parameter operand is accepted as a whole, since
    /// the parameter may name a field only known at read time.
    pub(crate) fn validate_fields(
        &self,
        field_exists: &dyn Fn(&str) -> Option<bool>,
    ) -> DdsResult<()> {
        match self {
            Expression::FilterExpression(condition) => condition.validate_fields(field_exists),
            Expression::QueryExpression { condition, order_by } => {
                if let Some(condition) = condition {
                    condition.validate_fields(field_exists)?;
                }
                if let Some(fields) = order_by {
                    for field in fields {
                        if field_exists(field) == Some(false) {
                            return Err(unknown_field_error(field));
                        }
                    }
                }
                Ok(())
            }
            Expression::TopicExpression { select: _, from: _, where_clause } => {
                match where_clause {
                    Some(where_clause) => where_clause.condition.validate_fields(field_exists),
                    None => Ok(()),
                }
            }
        }
    }
}

impl Condition {
    fn validate_fields(&self, field_exists: &dyn Fn(&str) -> Option<bool>) -> DdsResult<()> {
        match self {
            Condition::Predicate(predicate) => predicate.validate_fields(field_exists),
            Condition::Binary { left, op: _, right } => {
                left.validate_fields(field_exists)?;
                right.validate_fields(field_exists)
            }
            Condition::Unary { op: _, operand } => operand.validate_fields(field_exists),
            Condition::Parentheses(condition) => condition.validate_fields(field_exists),
        }
    }
}

impl Predicate {
    fn validate_fields(&self, field_exists: &dyn Fn(&str) -> Option<bool>) -> DdsResult<()> {
        match self {
            Predicate::Comparison { left, op: _, right } => {
                if matches!(left, Parameter::Parameter(_))
                    || matches!(right, Parameter::Parameter(_))
                {
                    return Ok(());
                }
                let candidates: Vec<&str> = [left, right]
                    .into_iter()
                    .filter_map(|p| match p {
                        Parameter::String(s) => Some(s.as_str()),
                        _ => None,
                    })
                    .collect();
                if candidates.is_empty() {
                    return Err(DdsError::Error(format!(
                        "Filter comparison references no field: {:?} vs {:?}",
                        left, right
                    )));
                }
                if candidates.iter().any(|c| field_exists(c) != Some(false)) {
                    Ok(())
                } else {
                    Err(DdsError::Error(format!(
                        "Filter expression references unknown field(s): {}",
                        candidates.join(", ")
                    )))
                }
            }
            Predicate::Between { field, negated: _, range: _ } => {
                if field_exists(field) == Some(false) {
                    Err(unknown_field_error(field))
                } else {
                    Ok(())
                }
            }
        }
    }
}

fn unknown_field_error(field: &str) -> DdsError {
    DdsError::Error(format!("Filter expression references unknown field '{}'", field))
}

impl Parameter {
    pub(crate) fn validate_expression_parameters(&self, parameters: &[String]) -> DdsResult<()> {
        match self {
            Parameter::Parameter(index) => {
                if *index >= parameters.len() {
                    return Err(DdsError::Error(format!(
                        "Parameter %{} not found: only {} parameters provided",
                        index,
                        parameters.len()
                    )));
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}
