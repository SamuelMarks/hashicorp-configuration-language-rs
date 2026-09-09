//! Migration engine from legacy HCL 1.0 to modern HCL 2.0 AST.
//!
//! Provides AST translation from [`Hcl1Body`] to canonical [`Body`],
//! converting legacy block assignments (`block "name" = { ... }`) and interpolations (`"${expr}"`)
//! into native HCL2 expressions and blocks, while emitting informative migration diagnostics.

use crate::ast::expr::Expression;
use crate::ast::structure::{Attribute, Block, Body};
use crate::error::HclError;
use crate::hcl1::parser::{Hcl1Block, Hcl1Body, Hcl1Expression, Hcl1Item};
use crate::number::Number;
use crate::span::Span;
use std::str::FromStr;

/// A diagnostic emitted during HCL 1.0 to HCL 2.0 migration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationDiagnostic {
    /// A descriptive message explaining the legacy construct and how it was migrated.
    pub message: String,
    /// The source span where the legacy construct appeared.
    pub span: Span,
    /// A suggested replacement string or syntax.
    pub suggestion: Option<String>,
}

impl MigrationDiagnostic {
    /// Creates a new [`MigrationDiagnostic`].
    ///
    /// # Arguments
    /// * `message` - The diagnostic message.
    /// * `span` - The source span.
    /// * `suggestion` - An optional replacement suggestion.
    #[must_use]
    pub fn new(
        message: impl Into<String>,
        span: Span,
        suggestion: Option<impl Into<String>>,
    ) -> Self {
        Self {
            message: message.into(),
            span,
            suggestion: suggestion.map(Into::into),
        }
    }
}

/// Migrates an [`Hcl1Body`] into a modern canonical HCL2 [`Body`].
///
/// # Arguments
/// * `hcl1_body` - The parsed HCL 1.0 body.
///
/// # Errors
/// Returns [`HclError`] if conversion or interpolation unescaping fails.
pub fn migrate_hcl1_to_hcl2(
    hcl1_body: &Hcl1Body,
) -> Result<(Body, Vec<MigrationDiagnostic>), HclError> {
    let mut diags = Vec::new();
    let body = translate_body(hcl1_body, &mut diags)?;
    Ok((body, diags))
}

fn translate_body(
    hcl1_body: &Hcl1Body,
    diags: &mut Vec<MigrationDiagnostic>,
) -> Result<Body, HclError> {
    let mut body = Body::new(hcl1_body.span.clone());

    for item in &hcl1_body.items {
        match item {
            Hcl1Item::Attribute(attr) => {
                // Check if this attribute is actually a legacy block: e.g. `variable = { ... }`
                if let Hcl1Expression::Map(_pairs, map_span) = &attr.expr {
                    // In HCL1, `variable "name" = { ... }` or `foo = { ... }`
                    // Can be kept as an attribute or converted to block depending on context.
                    let expr = translate_expression(&attr.expr, diags)?;
                    body.attributes.insert(
                        attr.name.clone(),
                        Attribute {
                            name: attr.name.clone(),
                            expr,
                            span: attr.span.clone(),
                            name_span: attr.span.clone(),
                            equals_span: map_span.clone(),
                            leading_comments: Vec::new(),
                            trailing_comment: None,
                        },
                    );
                } else {
                    let expr = translate_expression(&attr.expr, diags)?;
                    body.attributes.insert(
                        attr.name.clone(),
                        Attribute {
                            name: attr.name.clone(),
                            expr,
                            span: attr.span.clone(),
                            name_span: attr.span.clone(),
                            equals_span: attr.span.clone(),
                            leading_comments: Vec::new(),
                            trailing_comment: None,
                        },
                    );
                }
            }
            Hcl1Item::Block(block) => {
                let hcl2_block = translate_block(block, diags)?;
                body.blocks.push(hcl2_block);
            }
        }
    }

    Ok(body)
}

fn translate_block(
    block: &Hcl1Block,
    diags: &mut Vec<MigrationDiagnostic>,
) -> Result<Block, HclError> {
    if block.has_equals_assign {
        diags.push(MigrationDiagnostic::new(
            format!(
                "Legacy block assignment `{} ... = {{ ... }}` migrated to native HCL2 block `{} ... {{ ... }}`",
                block.block_type, block.block_type
            ),
            block.span.clone(),
            Some(format!("{} {{ ... }}", block.block_type)),
        ));
    }

    let inner_body = translate_body(&block.body, diags)?;

    Ok(Block {
        block_type: block.block_type.clone(),
        labels: block.labels.clone(),
        body: inner_body,
        span: block.span.clone(),
        type_span: block.span.clone(),
        label_spans: vec![block.span.clone(); block.labels.len()],
        open_brace_span: block.span.clone(),
        close_brace_span: block.span.clone(),
        leading_comments: Vec::new(),
        trailing_comment: None,
    })
}

fn translate_expression(
    expr: &Hcl1Expression,
    diags: &mut Vec<MigrationDiagnostic>,
) -> Result<Expression, HclError> {
    match expr {
        Hcl1Expression::Null(span) => Ok(Expression::Null(span.clone())),
        Hcl1Expression::Bool(b, span) => Ok(Expression::Bool(*b, span.clone())),
        Hcl1Expression::Number(n, span) => {
            let num = Number::from_str(n)
                .map_err(|e| HclError::Parse(format!("invalid numeric literal `{n}`: {e}")))?;
            Ok(Expression::Number(num, span.clone()))
        }
        Hcl1Expression::Variable(name, span) => {
            Ok(Expression::Variable(name.clone(), span.clone()))
        }
        Hcl1Expression::List(elements, span) => {
            let mut hcl2_elements = Vec::with_capacity(elements.len());
            for el in elements {
                hcl2_elements.push(translate_expression(el, diags)?);
            }
            Ok(Expression::Tuple(hcl2_elements, span.clone()))
        }
        Hcl1Expression::Map(pairs, span) => {
            let mut hcl2_pairs = Vec::with_capacity(pairs.len());
            for (k, v) in pairs {
                let k_expr = Expression::String(k.clone(), span.clone());
                let v_expr = translate_expression(v, diags)?;
                hcl2_pairs.push((k_expr, v_expr));
            }
            Ok(Expression::Object(hcl2_pairs, span.clone()))
        }
        Hcl1Expression::String(s, span) => {
            // Check if string contains `${...}` interpolation
            if s.contains("${") {
                // If it is purely a single interpolation: e.g. `"${var.foo}"`
                let trimmed = s.trim();
                if trimmed.starts_with("${")
                    && trimmed.ends_with('}')
                    && trimmed.matches("${").count() == 1
                {
                    let inner_raw = &trimmed[2..trimmed.len() - 1].trim();
                    diags.push(MigrationDiagnostic::new(
                        format!(
                            "Interpolated expression `\"${{{inner_raw}}}\"` migrated to native HCL2 expression `{inner_raw}`"
                        ),
                        span.clone(),
                        Some((*inner_raw).to_string()),
                    ));

                    // Attempt to parse inner as HCL2 expression
                    let mut parser = crate::parse::parser::Parser::new(inner_raw);
                    if let Some(parsed_expr) = parser.parse_expression() {
                        return Ok(parsed_expr);
                    }
                }

                // If mixed text and interpolation, convert to Expression::Template
                diags.push(MigrationDiagnostic::new(
                    "String containing interpolations migrated to HCL2 template expression",
                    span.clone(),
                    None::<String>,
                ));
                let mut p_diags = crate::diagnostic::Diagnostics::new();
                let quoted = format!("\"{s}\"");
                let parts =
                    crate::parse::parser::Parser::parse_template(&mut p_diags, &quoted, span);
                Ok(Expression::Template(parts, span.clone()))
            } else {
                Ok(Expression::String(s.clone(), span.clone()))
            }
        }
    }
}
