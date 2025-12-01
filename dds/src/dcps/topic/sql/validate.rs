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
