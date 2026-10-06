//! Declarative validation and assertion evaluation engine.
//!
//! Evaluates `validation`, `precondition`, and `postcondition` blocks against an evaluation context.
use crate::ast::structure::{Body, PostconditionBlock, PreconditionBlock, ValidationBlock};
use crate::diagnostic::{Diagnostic, Diagnostics};
use crate::error::HclError;
use crate::eval::context::Context;
use crate::eval::evaluator::Evaluator;
use crate::types::{Type, ValueData};
/// Evaluates a single [`ValidationBlock`] against the provided context.
///
/// # Behavior
/// 1. Evaluates the `condition` expression.
/// 2. If the condition evaluates to an unknown value, failure is postponed and `Ok(())` is returned.
/// 3. Asserts that the condition evaluates strictly to `Type::Bool` and is not null.
/// 4. If the condition is `true`, validation passes and returns `Ok(())`.
/// 5. If the condition is `false`, evaluates the `error_message` expression and emits a validation diagnostic error.
///
/// # Arguments
/// * `val` - The validation block to evaluate.
/// * `ctx` - The evaluation context.
///
/// # Errors
/// Returns [`Diagnostics`] if evaluation fails, condition type is not bool, or the validation rule fails.
pub fn evaluate_validation(val: &ValidationBlock, ctx: &mut Context) -> Result<(), Diagnostics> {
    evaluate_validation_internal(val, ctx, "Validation rule failed", None)
}
/// Evaluates a lifecycle [`PreconditionBlock`] against the provided context.
///
/// # Arguments
/// * `pre` - The precondition block to evaluate.
/// * `ctx` - The evaluation context.
///
/// # Errors
/// Returns [`Diagnostics`] if evaluation fails, condition type is not bool, or precondition fails.
pub fn evaluate_precondition(
    pre: &PreconditionBlock,
    ctx: &mut Context,
) -> Result<(), Diagnostics> {
    let val = pre.to_validation();
    evaluate_validation_internal(
        &val,
        ctx,
        "Precondition failed",
        Some("Precondition failed: "),
    )
}
/// Evaluates a lifecycle [`PostconditionBlock`] against the provided context.
///
/// # Arguments
/// * `post` - The postcondition block to evaluate.
/// * `ctx` - The evaluation context.
///
/// # Errors
/// Returns [`Diagnostics`] if evaluation fails, condition type is not bool, or postcondition fails.
pub fn evaluate_postcondition(
    post: &PostconditionBlock,
    ctx: &mut Context,
) -> Result<(), Diagnostics> {
    let val = post.to_validation();
    evaluate_validation_internal(
        &val,
        ctx,
        "Postcondition failed",
        Some("Postcondition failed: "),
    )
}
/// Evaluates all validations, preconditions, and postconditions in a [`Body`] recursively.
///
/// # Arguments
/// * `body` - The AST body containing possible validation or assertion blocks.
/// * `ctx` - The evaluation context.
///
/// # Errors
/// Returns aggregated [`Diagnostics`] if any validation rule or assertion fails.
pub fn evaluate_all_validations(body: &Body, ctx: &mut Context) -> Result<(), Diagnostics> {
    let mut diags = Diagnostics::new();
    for val in &body.validations {
        if let Err(errs) = evaluate_validation(val, ctx) {
            diags.extend(errs);
        }
    }
    for pre in &body.preconditions {
        if let Err(errs) = evaluate_precondition(pre, ctx) {
            diags.extend(errs);
        }
    }
    for post in &body.postconditions {
        if let Err(errs) = evaluate_postcondition(post, ctx) {
            diags.extend(errs);
        }
    }
    for block in &body.blocks {
        if let Err(errs) = evaluate_all_validations(&block.body, ctx) {
            diags.extend(errs);
        }
    }
    if diags.has_errors() {
        Err(diags)
    } else {
        Ok(())
    }
}
fn evaluate_validation_internal(
    val: &ValidationBlock,
    ctx: &mut Context,
    summary_title: &str,
    err_prefix: Option<&str>,
) -> Result<(), Diagnostics> {
    let evaluator = Evaluator::new(ctx);
    let (cond_val, _) = evaluator.evaluate(&val.condition)?;
    if cond_val.is_unknown() {
        return Ok(());
    }
    if cond_val.is_null() {
        let mut diags = Diagnostics::new();
        diags.push(
            Diagnostic::error(
                "Invalid condition value",
                "Condition evaluated to null, but a non-null boolean is required.",
                val.condition.span(),
            )
            .with_error(HclError::Validation("condition evaluated to null".into())),
        );
        return Err(diags);
    }
    if cond_val.ty() != &Type::Bool {
        let mut diags = Diagnostics::new();
        diags.push(
            Diagnostic::error(
                "Invalid condition type",
                format!(
                    "Condition expression must produce a boolean value, got {}.",
                    cond_val.ty()
                ),
                val.condition.span(),
            )
            .with_error(HclError::Validation(format!(
                "condition must evaluate to bool, got {}",
                cond_val.ty()
            ))),
        );
        return Err(diags);
    }
    match cond_val.data.as_ref() {
        ValueData::Bool(true) => Ok(()),
        ValueData::Bool(false) => {
            let message_evaluator = Evaluator::new(ctx);
            let message = match message_evaluator.evaluate(&val.error_message) {
                Ok((msg_val, _)) => match msg_val.data.as_ref() {
                    ValueData::String(s) => s.clone(),
                    ValueData::Number(n) => n.0.to_string(),
                    ValueData::Bool(b) => b.to_string(),
                    _ => format!("{:?}", msg_val.data),
                },
                Err(errs) => {
                    return Err(errs);
                }
            };
            let err_string = if let Some(pfx) = err_prefix {
                format!("{pfx}{message}")
            } else {
                message.clone()
            };
            let mut diags = Diagnostics::new();
            diags.push(
                Diagnostic::error(summary_title, message, val.span.clone())
                    .with_error(HclError::Validation(err_string)),
            );
            Err(diags)
        }
        _ => {
            let mut diags = Diagnostics::new();
            diags.push(
                Diagnostic::error(
                    "Invalid condition value",
                    "Condition evaluated to unexpected non-boolean data.",
                    val.condition.span(),
                )
                .with_error(HclError::Validation(
                    "condition produced unexpected data".into(),
                )),
            );
            Err(diags)
        }
    }
}
#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::pedantic,
        clippy::nursery
    )]
    use super::*;
    use crate::api::parse;
    use crate::ast::expr::Expression;
    use crate::span::Span;
    use crate::types::Value;
    use std::collections::BTreeMap;
    fn empty_span() -> Span {
        Span::new(0, 0, 0, 0, 0, 0)
    }
    fn setup_var_context(pairs: &[(&str, Value)]) -> Context<'static> {
        let mut ctx = Context::with_stdlib();
        let mut map = BTreeMap::new();
        let mut types = BTreeMap::new();
        for (name, val) in pairs {
            ctx.set_variable(*name, val.clone());
            types.insert((*name).to_string(), val.ty().clone());
            map.insert((*name).to_string(), val.clone());
        }
        ctx.set_variable(
            "var",
            Value::new(Type::object(types), ValueData::Object(map)),
        );
        ctx
    }
    #[test]
    fn test_evaluate_validation_success() {
        let mut ctx = setup_var_context(&[(
            "port",
            Value::new(Type::Number, ValueData::Number(8080.into())),
        )]);
        let input = r#"
            validation {
                condition = var.port == 8080
                error_message = "Port must be 8080."
            }
        "#;
        let body = parse(input).unwrap();
        assert_eq!(body.validations.len(), 1);
        let res = evaluate_validation(&body.validations[0], &mut ctx);
        assert!(res.is_ok());
    }
    #[test]
    fn test_evaluate_validation_failure() {
        let mut ctx = setup_var_context(&[(
            "port",
            Value::new(Type::Number, ValueData::Number(80.into())),
        )]);
        let input = r#"
            validation {
                condition = var.port == 8080
                error_message = format("Port %d is invalid, must be 8080.", var.port)
            }
        "#;
        let body = parse(input).unwrap();
        assert_eq!(body.validations.len(), 1);
        let res = evaluate_validation(&body.validations[0], &mut ctx);
        assert!(res.is_err());
        let errs = res.unwrap_err();
        assert_eq!(errs.errors().len(), 1);
        let diag = &errs.errors()[0];
        assert_eq!(diag.summary_str(), "Validation rule failed");
        assert!(
            diag.to_string()
                .contains("Port 80 is invalid, must be 8080.")
        );
    }
    #[test]
    fn test_evaluate_validation_unknown_condition_passes() {
        let mut ctx = setup_var_context(&[("status", Value::unknown(Type::Bool))]);
        let input = r#"
            validation {
                condition = var.status
                error_message = "Status must be true."
            }
        "#;
        let body = parse(input).unwrap();
        let res = evaluate_validation(&body.validations[0], &mut ctx);
        assert!(res.is_ok());
    }
    #[test]
    fn test_evaluate_validation_null_condition_errors() {
        let mut ctx = setup_var_context(&[("flag", Value::null(Type::Bool))]);
        let input = r#"
            validation {
                condition = var.flag
                error_message = "Flag must not be null."
            }
        "#;
        let body = parse(input).unwrap();
        let res = evaluate_validation(&body.validations[0], &mut ctx);
        assert!(res.is_err());
        let errs = res.unwrap_err();
        assert!(
            errs.errors()[0]
                .to_string()
                .contains("condition evaluated to null")
        );
    }
    #[test]
    fn test_evaluate_validation_type_mismatch() {
        let mut ctx = setup_var_context(&[(
            "name",
            Value::new(Type::String, ValueData::String("prod".into())),
        )]);
        let input = r#"
            validation {
                condition = var.name
                error_message = "Should be bool."
            }
        "#;
        let body = parse(input).unwrap();
        let res = evaluate_validation(&body.validations[0], &mut ctx);
        assert!(res.is_err());
        let errs = res.unwrap_err();
        assert!(
            errs.errors()[0]
                .to_string()
                .contains("condition must evaluate to bool")
        );
    }
    #[test]
    fn test_evaluate_precondition_and_postcondition() {
        let mut ctx = setup_var_context(&[
            ("ready", Value::new(Type::Bool, ValueData::Bool(true))),
            ("health", Value::new(Type::Bool, ValueData::Bool(false))),
        ]);
        let input = r#"
            lifecycle {
                precondition {
                    condition = var.ready
                    error_message = "Not ready"
                }
                postcondition {
                    condition = var.health
                    error_message = "Unhealthy status"
                }
            }
        "#;
        let body = parse(input).unwrap();
        let lifecycle_block = &body.blocks[0];
        assert_eq!(lifecycle_block.body.preconditions.len(), 1);
        assert_eq!(lifecycle_block.body.postconditions.len(), 1);
        let pre_res = evaluate_precondition(&lifecycle_block.body.preconditions[0], &mut ctx);
        assert!(pre_res.is_ok());
        let post_res = evaluate_postcondition(&lifecycle_block.body.postconditions[0], &mut ctx);
        assert!(post_res.is_err());
        let post_errs = post_res.unwrap_err();
        assert_eq!(post_errs.errors()[0].summary_str(), "Postcondition failed");
        assert!(
            post_errs.errors()[0]
                .to_string()
                .contains("Unhealthy status")
        );
    }
    #[test]
    fn test_evaluate_all_validations_recursive() {
        let mut ctx = setup_var_context(&[
            ("val1", Value::new(Type::Bool, ValueData::Bool(true))),
            ("val2", Value::new(Type::Bool, ValueData::Bool(false))),
            ("pre_fail", Value::new(Type::Bool, ValueData::Bool(false))),
            ("post_fail", Value::new(Type::Bool, ValueData::Bool(false))),
        ]);
        let input = r#"
            validation {
                condition = var.val1
                error_message = "Validation 1 ok"
            }
            resource "app" "server" {
                validation {
                    condition = var.val2
                    error_message = "Validation 2 failed"
                }
                lifecycle {
                    precondition {
                        condition = var.pre_fail
                        error_message = "Precondition failed"
                    }
                    postcondition {
                        condition = var.post_fail
                        error_message = "Postcondition failed"
                    }
                }
            }
        "#;
        let body = parse(input).unwrap();
        let all_res = evaluate_all_validations(&body, &mut ctx);
        assert!(all_res.is_err());
        let errs = all_res.unwrap_err();
        assert_eq!(errs.errors().len(), 3);
        assert!(
            errs.errors()
                .iter()
                .any(|e| e.to_string().contains("Validation 2 failed"))
        );
        assert!(
            errs.errors()
                .iter()
                .any(|e| e.to_string().contains("Precondition failed"))
        );
        assert!(
            errs.errors()
                .iter()
                .any(|e| e.to_string().contains("Postcondition failed"))
        );
    }
    #[test]
    fn test_evaluate_validation_error_in_error_message_expr() {
        let mut ctx = Context::new();
        let val_block = ValidationBlock::new(
            Expression::Bool(false, empty_span()),
            Expression::Variable("undefined_variable_in_msg".to_string(), empty_span()),
            empty_span(),
        );
        let res = evaluate_validation(&val_block, &mut ctx);
        assert!(res.is_err());
        let errs = res.unwrap_err();
        assert!(errs.errors()[0].to_string().contains("Unknown variable"));
    }
    #[test]
    fn test_evaluate_validation_non_string_error_messages() {
        let mut ctx = Context::new();
        let val_block_bool = ValidationBlock::new(
            Expression::Bool(false, empty_span()),
            Expression::Bool(false, empty_span()),
            empty_span(),
        );
        let res = evaluate_validation(&val_block_bool, &mut ctx);
        assert!(res.is_err());
        let errs = res.unwrap_err();
        assert!(errs.errors()[0].to_string().contains("false"));
        let val_block_tuple = ValidationBlock::new(
            Expression::Bool(false, empty_span()),
            Expression::Tuple(vec![Expression::Bool(true, empty_span())], empty_span()),
            empty_span(),
        );
        let res = evaluate_validation(&val_block_tuple, &mut ctx);
        assert!(res.is_err());
        let val_block_num = ValidationBlock::new(
            Expression::Bool(false, empty_span()),
            Expression::Number(404.into(), empty_span()),
            empty_span(),
        );
        let res = evaluate_validation(&val_block_num, &mut ctx);
        assert!(res.is_err());
        let errs = res.unwrap_err();
        assert!(errs.errors()[0].to_string().contains("404"));
    }
    #[test]
    fn test_evaluate_validation_condition_corrupted_data() {
        let mut ctx = Context::new();
        ctx.set_variable(
            "bad_bool",
            Value::new(Type::Bool, ValueData::Number(123.into())),
        );
        let val_block = ValidationBlock::new(
            Expression::Variable("bad_bool".to_string(), empty_span()),
            Expression::String("error".to_string(), empty_span()),
            empty_span(),
        );
        let res = evaluate_validation(&val_block, &mut ctx);
        assert!(res.is_err());
        let errs = res.unwrap_err();
        assert_eq!(errs.errors()[0].summary_str(), "Invalid condition value");
    }
    #[test]
    fn test_evaluate_precondition_postcondition_type_error() {
        let mut ctx = Context::new();
        ctx.set_variable(
            "bad_cond",
            Value::new(Type::String, ValueData::String("str".into())),
        );
        let pre = PreconditionBlock::new(
            Expression::Variable("bad_cond".to_string(), empty_span()),
            Expression::String("error".to_string(), empty_span()),
            empty_span(),
        );
        let res_pre = evaluate_precondition(&pre, &mut ctx);
        assert!(res_pre.is_err());
        assert_eq!(
            res_pre.unwrap_err().errors()[0].summary_str(),
            "Invalid condition type"
        );
        let post = PostconditionBlock::new(
            Expression::Variable("bad_cond".to_string(), empty_span()),
            Expression::String("error".to_string(), empty_span()),
            empty_span(),
        );
        let res_post = evaluate_postcondition(&post, &mut ctx);
        assert!(res_post.is_err());
        assert_eq!(
            res_post.unwrap_err().errors()[0].summary_str(),
            "Invalid condition type"
        );
    }
    #[test]
    fn test_evaluate_validation_condition_eval_failure() {
        let mut ctx = Context::new();
        let val_block = ValidationBlock::new(
            Expression::Variable("undefined_condition_var".to_string(), empty_span()),
            Expression::String("msg".to_string(), empty_span()),
            empty_span(),
        );
        let res = evaluate_validation(&val_block, &mut ctx);
        assert!(res.is_err());
    }
    #[test]
    fn test_evaluate_all_validations_all_pass() {
        let mut ctx = Context::new();
        let input = r#"
            validation {
                condition = true
                error_message = "ok"
            }
            nested {
                precondition {
                    condition = true
                    error_message = "ok"
                }
                postcondition {
                    condition = true
                    error_message = "ok"
                }
            }
        "#;
        let body = parse(input).unwrap();
        let res = evaluate_all_validations(&body, &mut ctx);
        assert!(res.is_ok());
    }
}
