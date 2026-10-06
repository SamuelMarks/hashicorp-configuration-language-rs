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
                if let Hcl1Expression::Map(_pairs, map_span) = &attr.expr {
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
        diags
            .push(
                MigrationDiagnostic::new(
                    format!(
                        "Legacy block assignment `{} ... = {{ ... }}` migrated to native HCL2 block `{} ... {{ ... }}`",
                        block.block_type, block.block_type
                    ),
                    block.span.clone(),
                    Some(format!("{} {{ ... }}", block.block_type)),
                ),
            );
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
            if s.contains("${") {
                let trimmed = s.trim();
                if trimmed.starts_with("${")
                    && trimmed.ends_with('}')
                    && trimmed.matches("${").count() == 1
                {
                    let inner_raw = &trimmed[2..trimmed.len() - 1].trim();
                    diags
                        .push(
                            MigrationDiagnostic::new(
                                format!(
                                    "Interpolated expression `\"${{{inner_raw}}}\"` migrated to native HCL2 expression `{inner_raw}`"
                                ),
                                span.clone(),
                                Some((*inner_raw).to_string()),
                            ),
                        );
                    let mut parser = crate::parse::parser::Parser::new(inner_raw);
                    if let Some(parsed_expr) = parser.parse_expression() {
                        return Ok(parsed_expr);
                    }
                }
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
pub(crate) fn compile_regex(pattern: &str) -> regex::Regex {
    match regex::Regex::new(pattern) {
        Ok(re) => re,
        Err(_) => compile_regex("$^"),
    }
}
static RE_USER: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| compile_regex(r"\{\{\s*user\s*`([^`]+)`\s*\}\}"));
static RE_ENV: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| compile_regex(r"\{\{\s*env\s*`([^`]+)`\s*\}\}"));
static RE_TS: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| compile_regex(r"\{\{\s*timestamp\s*\}\}"));
static RE_PATH: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| compile_regex(r"\{\{\s*(?:pwd|template_dir)\s*\}\}"));
static RE_BUILD_NAME: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| compile_regex(r"\{\{\s*build_name\s*\}\}"));
static RE_BUILD_TYPE: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| compile_regex(r"\{\{\s*build_type\s*\}\}"));
/// Translates legacy Packer and Terraform template interpolations (`"{{ ... }}"`)
/// into modern canonical HCL2 syntax (`"${...}"` or bare expressions).
///
/// Supported patterns:
/// - `{{ user `name` }}` -> `${var.name}`
/// - `{{ env `NAME` }}` -> `${env("NAME")}`
/// - `{{ timestamp }}` -> `${timestamp()}`
/// - `{{ pwd }}` / `{{ template_dir }}` -> `${path.root}`
/// - `{{ build_name }}` -> `${build.name}`
/// - `{{ build_type }}` -> `${build.type}`
///
/// # Arguments
/// * `input` - The input string containing legacy interpolations.
#[must_use]
pub fn translate_legacy_interpolations(input: &str) -> String {
    let s1 = RE_USER.replace_all(input, |caps: &regex::Captures| {
        format!("${{var.{}}}", &caps[1])
    });
    let s2 = RE_ENV.replace_all(&s1, |caps: &regex::Captures| {
        format!("${{env(\"{}\")}}", &caps[1])
    });
    let s3 = RE_TS.replace_all(&s2, |_caps: &regex::Captures| "${timestamp()}");
    let s4 = RE_PATH.replace_all(&s3, |_caps: &regex::Captures| "${path.root}");
    let s5 = RE_BUILD_NAME.replace_all(&s4, |_caps: &regex::Captures| "${build.name}");
    let s6 = RE_BUILD_TYPE.replace_all(&s5, |_caps: &regex::Captures| "${build.type}");
    s6.into_owned()
}
/// Migrates a legacy JSON template (such as Packer JSON) into a canonical HCL2 [`Body`].
///
/// Converts:
/// - `"variables"` map into `variable "<name>" { default = ... }` blocks
/// - `"builders"` array into `source "<type>" "<name>" { ... }` blocks
/// - `"provisioners"` array into `provisioner "<type>" { ... }` blocks
/// - Top-level attributes and legacy `{{ ... }}` interpolations into canonical HCL2.
///
/// # Arguments
/// * `json_str` - The JSON template string.
///
/// # Errors
/// Returns [`HclError::Parse`] if JSON decoding fails.
pub fn migrate_legacy_json(json_str: &str) -> Result<(Body, Vec<MigrationDiagnostic>), HclError> {
    let root: serde_json::Value = serde_json::from_str(json_str)
        .map_err(|e| HclError::Parse(format!("Invalid JSON template: {e}")))?;
    let serde_json::Value::Object(map) = root else {
        return Err(HclError::Parse(
            "JSON template root must be an object".to_string(),
        ));
    };
    let empty_span = Span::new(0, 0, 0, 0, 0, 0);
    let mut body = Body::new(empty_span.clone());
    let mut diags = Vec::new();
    for (k, v) in map {
        match k.as_str() {
            "variables" => {
                if let serde_json::Value::Object(vars) = v {
                    for (var_name, var_val) in vars {
                        let mut var_body = Body::new(empty_span.clone());
                        let default_expr = json_to_expr(&var_val, &empty_span, &mut diags);
                        var_body.attributes.insert(
                            "default".to_string(),
                            Attribute {
                                name: "default".to_string(),
                                expr: default_expr,
                                span: empty_span.clone(),
                                name_span: empty_span.clone(),
                                equals_span: empty_span.clone(),
                                leading_comments: Vec::new(),
                                trailing_comment: None,
                            },
                        );
                        body.blocks.push(Block {
                            block_type: "variable".to_string(),
                            labels: vec![var_name.clone()],
                            body: var_body,
                            span: empty_span.clone(),
                            type_span: empty_span.clone(),
                            label_spans: vec![empty_span.clone()],
                            open_brace_span: empty_span.clone(),
                            close_brace_span: empty_span.clone(),
                            leading_comments: Vec::new(),
                            trailing_comment: None,
                        });
                        diags
                            .push(
                                MigrationDiagnostic::new(
                                    format!(
                                        "Migrated legacy variable '{var_name}' into HCL2 'variable \"{var_name}\"' block"
                                    ),
                                    empty_span.clone(),
                                    None::<String>,
                                ),
                            );
                    }
                }
            }
            "builders" => {
                if let serde_json::Value::Array(builders) = v {
                    for b_val in builders {
                        if let serde_json::Value::Object(b_map) = b_val {
                            let builder_type = b_map
                                .get("type")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("unknown")
                                .to_string();
                            let builder_name = b_map
                                .get("name")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or(&builder_type)
                                .to_string();
                            let mut b_body = Body::new(empty_span.clone());
                            for (bk, bv) in b_map {
                                if bk != "type" && bk != "name" {
                                    let expr = json_to_expr(&bv, &empty_span, &mut diags);
                                    b_body.attributes.insert(
                                        bk.clone(),
                                        Attribute {
                                            name: bk.clone(),
                                            expr,
                                            span: empty_span.clone(),
                                            name_span: empty_span.clone(),
                                            equals_span: empty_span.clone(),
                                            leading_comments: Vec::new(),
                                            trailing_comment: None,
                                        },
                                    );
                                }
                            }
                            body.blocks.push(Block {
                                block_type: "source".to_string(),
                                labels: vec![builder_type.clone(), builder_name.clone()],
                                body: b_body,
                                span: empty_span.clone(),
                                type_span: empty_span.clone(),
                                label_spans: vec![empty_span.clone(), empty_span.clone()],
                                open_brace_span: empty_span.clone(),
                                close_brace_span: empty_span.clone(),
                                leading_comments: Vec::new(),
                                trailing_comment: None,
                            });
                            diags
                                .push(
                                    MigrationDiagnostic::new(
                                        format!(
                                            "Migrated legacy builder '{builder_type}.{builder_name}' into HCL2 'source' block"
                                        ),
                                        empty_span.clone(),
                                        None::<String>,
                                    ),
                                );
                        }
                    }
                }
            }
            "provisioners" => {
                if let serde_json::Value::Array(provisioners) = v {
                    for p_val in provisioners {
                        if let serde_json::Value::Object(p_map) = p_val {
                            let prov_type = p_map
                                .get("type")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("shell")
                                .to_string();
                            let mut p_body = Body::new(empty_span.clone());
                            for (pk, pv) in p_map {
                                if pk != "type" {
                                    let expr = json_to_expr(&pv, &empty_span, &mut diags);
                                    p_body.attributes.insert(
                                        pk.clone(),
                                        Attribute {
                                            name: pk.clone(),
                                            expr,
                                            span: empty_span.clone(),
                                            name_span: empty_span.clone(),
                                            equals_span: empty_span.clone(),
                                            leading_comments: Vec::new(),
                                            trailing_comment: None,
                                        },
                                    );
                                }
                            }
                            body.blocks.push(Block {
                                block_type: "provisioner".to_string(),
                                labels: vec![prov_type.clone()],
                                body: p_body,
                                span: empty_span.clone(),
                                type_span: empty_span.clone(),
                                label_spans: vec![empty_span.clone()],
                                open_brace_span: empty_span.clone(),
                                close_brace_span: empty_span.clone(),
                                leading_comments: Vec::new(),
                                trailing_comment: None,
                            });
                            diags.push(MigrationDiagnostic::new(
                                format!(
                                    "Migrated legacy provisioner '{prov_type}' into HCL2 block"
                                ),
                                empty_span.clone(),
                                None::<String>,
                            ));
                        }
                    }
                }
            }
            other => {
                let expr = json_to_expr(&v, &empty_span, &mut diags);
                body.attributes.insert(
                    other.to_string(),
                    Attribute {
                        name: other.to_string(),
                        expr,
                        span: empty_span.clone(),
                        name_span: empty_span.clone(),
                        equals_span: empty_span.clone(),
                        leading_comments: Vec::new(),
                        trailing_comment: None,
                    },
                );
            }
        }
    }
    Ok((body, diags))
}
fn json_to_expr(
    val: &serde_json::Value,
    span: &Span,
    diags: &mut Vec<MigrationDiagnostic>,
) -> Expression {
    match val {
        serde_json::Value::Null => Expression::Null(span.clone()),
        serde_json::Value::Bool(b) => Expression::Bool(*b, span.clone()),
        serde_json::Value::Number(n) => {
            let num = Number::from_str(&n.to_string()).unwrap_or(Number::from(0_i64));
            Expression::Number(num, span.clone())
        }
        serde_json::Value::String(s) => {
            let translated = translate_legacy_interpolations(s);
            if translated != *s {
                diags.push(MigrationDiagnostic::new(
                    format!("Translated legacy interpolation in string `{s}`"),
                    span.clone(),
                    Some(translated.clone()),
                ));
            }
            if translated.contains("${") {
                let trimmed = translated.trim();
                let single_interp = trimmed.starts_with("${")
                    && trimmed.ends_with('}')
                    && trimmed.matches("${").count() == 1;
                if single_interp {
                    let inner = &trimmed[2..trimmed.len() - 1].trim();
                    let mut parser = crate::parse::parser::Parser::new(inner);
                    if let Some(parsed) = parser.parse_expression() {
                        parsed
                    } else {
                        let mut p_diags = crate::diagnostic::Diagnostics::new();
                        let quoted = format!("\"{translated}\"");
                        let parts = crate::parse::parser::Parser::parse_template(
                            &mut p_diags,
                            &quoted,
                            span,
                        );
                        Expression::Template(parts, span.clone())
                    }
                } else {
                    let mut p_diags = crate::diagnostic::Diagnostics::new();
                    let quoted = format!("\"{translated}\"");
                    let parts =
                        crate::parse::parser::Parser::parse_template(&mut p_diags, &quoted, span);
                    Expression::Template(parts, span.clone())
                }
            } else {
                Expression::String(translated, span.clone())
            }
        }
        serde_json::Value::Array(arr) => {
            let elements = arr
                .iter()
                .map(|item| json_to_expr(item, span, diags))
                .collect();
            Expression::Tuple(elements, span.clone())
        }
        serde_json::Value::Object(obj) => {
            let mut pairs = Vec::with_capacity(obj.len());
            for (k, v) in obj {
                let k_expr = Expression::String(k.clone(), span.clone());
                let v_expr = json_to_expr(v, span, diags);
                pairs.push((k_expr, v_expr));
            }
            Expression::Object(pairs, span.clone())
        }
    }
}
