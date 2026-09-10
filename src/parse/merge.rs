use crate::ast::structure::Body;
use crate::diagnostic::{Diagnostic, Diagnostics};
use crate::span::Span;

/// Merges multiple `Body` structs into a single unified `Body`.
///
/// The order of the `bodies` slice determines the load order.
///
/// Returns an error if an attribute is defined multiple times at the same level.
/// # Errors
/// Returns diagnostics if merging fails due to duplicated attributes.
pub fn merge_bodies(bodies: Vec<Body>) -> Result<Body, Diagnostics> {
    let mut merged = Body::new(Span::new(0, 0, 0, 0, 0, 0));
    let mut diagnostics = Diagnostics::new();

    for body in bodies {
        merged.span = merged.span.merge(&body.span);

        for (name, attr) in body.attributes {
            if let Some(existing) = merged.attributes.get(&name) {
                let mut diag = Diagnostic::error(
                    format!("Attribute '{name}' defined multiple times"),
                    "Attributes can only be defined once per block or file level",
                    attr.span,
                );
                diag.context = Some(existing.span.clone());
                diagnostics.push(diag);
            } else {
                merged.attributes.insert(name, attr);
            }
        }

        merged.blocks.extend(body.blocks);
        merged.functions.extend(body.functions);
        merged.dynamic_blocks.extend(body.dynamic_blocks);
        merged.validations.extend(body.validations);
        merged.preconditions.extend(body.preconditions);
        merged.postconditions.extend(body.postconditions);
    }

    if diagnostics.has_errors() {
        Err(diagnostics)
    } else {
        Ok(merged)
    }
}

/// Parses and merges multiple named configuration files into a single unified [`Body`].
///
/// Each tuple in `files` contains `(filename, content)`.
///
/// All AST nodes retain their original source file references in their spans.
///
/// # Errors
/// Returns [`Diagnostics`] if any file fails parsing or if duplicate attribute definitions collide.
pub fn merge_files(files: &[(&str, &str)]) -> Result<Body, Diagnostics> {
    let mut parser = crate::parse::file_manager::FileManager::new();
    let mut diags = Diagnostics::new();

    for (filename, content) in files {
        if let Err(d) = parser.parse_hcl_string(filename, content) {
            for diag in d.errors() {
                diags.push(diag.clone());
            }
        }
    }

    if diags.has_errors() {
        return Err(diags);
    }

    parser.merge_all()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::expr::Expression;
    use crate::ast::structure::{Attribute, Block};

    fn empty_span() -> Span {
        Span::new(0, 0, 0, 0, 0, 0)
    }

    #[test]
    fn test_merge_bodies_success() {
        let mut b1 = Body::new(empty_span());
        b1.attributes.insert(
            "foo".to_string(),
            Attribute {
                name: "foo".to_string(),
                expr: Expression::Null(empty_span()),
                span: empty_span(),
                name_span: empty_span(),
                equals_span: empty_span(),
                leading_comments: Vec::new(),
                trailing_comment: None,
            },
        );

        let mut b2 = Body::new(empty_span());
        b2.attributes.insert(
            "bar".to_string(),
            Attribute {
                name: "bar".to_string(),
                expr: Expression::Null(empty_span()),
                span: empty_span(),
                name_span: empty_span(),
                equals_span: empty_span(),
                leading_comments: Vec::new(),
                trailing_comment: None,
            },
        );
        b2.blocks.push(Block {
            block_type: "my_block".to_string(),
            labels: vec![],
            body: Body::new(empty_span()),
            span: empty_span(),
            type_span: empty_span(),
            label_spans: vec![],
            open_brace_span: empty_span(),
            close_brace_span: empty_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        });

        let merged = merge_bodies(vec![b1, b2]).expect("expected value");
        assert_eq!(merged.attributes.len(), 2);
        assert_eq!(merged.blocks.len(), 1);
        assert!(merged.attributes.contains_key("foo"));
        assert!(merged.attributes.contains_key("bar"));
    }

    #[test]
    fn test_merge_bodies_duplicate_attribute() {
        let mut b1 = Body::new(empty_span());
        b1.attributes.insert(
            "foo".to_string(),
            Attribute {
                name: "foo".to_string(),
                expr: Expression::Null(empty_span()),
                span: empty_span(),
                name_span: empty_span(),
                equals_span: empty_span(),
                leading_comments: Vec::new(),
                trailing_comment: None,
            },
        );

        let mut b2 = Body::new(empty_span());
        b2.attributes.insert(
            "foo".to_string(),
            Attribute {
                name: "foo".to_string(),
                expr: Expression::Bool(true, empty_span()),
                span: empty_span(),
                name_span: empty_span(),
                equals_span: empty_span(),
                leading_comments: Vec::new(),
                trailing_comment: None,
            },
        );

        let errs = merge_bodies(vec![b1, b2]).expect_err("expected error");
        assert_eq!(errs.errors().len(), 1);
        assert!(
            errs.errors()[0]
                .error
                .to_string()
                .contains("defined multiple times")
        );
    }

    #[test]
    fn test_merge_bodies_validations_and_assertions() {
        use crate::ast::structure::{PostconditionBlock, PreconditionBlock, ValidationBlock};

        let mut b1 = Body::new(empty_span());
        b1.validations.push(ValidationBlock::new(
            Expression::Null(empty_span()),
            Expression::Null(empty_span()),
            empty_span(),
        ));
        b1.preconditions.push(PreconditionBlock::new(
            Expression::Null(empty_span()),
            Expression::Null(empty_span()),
            empty_span(),
        ));

        let mut b2 = Body::new(empty_span());
        b2.postconditions.push(PostconditionBlock::new(
            Expression::Null(empty_span()),
            Expression::Null(empty_span()),
            empty_span(),
        ));

        let merged = merge_bodies(vec![b1, b2]).expect("merge success");
        assert_eq!(merged.validations.len(), 1);
        assert_eq!(merged.preconditions.len(), 1);
        assert_eq!(merged.postconditions.len(), 1);
    }

    #[test]
    fn test_merge_files_success_and_errors() {
        let f1 = (
            "a.hcl",
            "attr1 = \"val1\"\nservice \"web\" {\n  port = 80\n}\n",
        );
        let f2 = ("b.hcl", "attr2 = 42\nservice \"api\" {\n  port = 8080\n}\n");

        let merged = merge_files(&[f1, f2]).expect("merge files ok");
        assert_eq!(merged.attributes.len(), 2);
        assert_eq!(merged.blocks.len(), 2);

        // Check that spans retained original file identifiers
        let attr1 = merged.attributes.get("attr1").expect("attr1 exists");
        assert_eq!(attr1.span.file.as_deref(), Some("a.hcl"));

        let attr2 = merged.attributes.get("attr2").expect("attr2 exists");
        assert_eq!(attr2.span.file.as_deref(), Some("b.hcl"));

        // Conflict across files
        let f_conflict1 = ("conf1.hcl", "x = 1\n");
        let f_conflict2 = ("conf2.hcl", "x = 2\n");
        let errs = merge_files(&[f_conflict1, f_conflict2]).expect_err("should have collision");
        assert_eq!(errs.errors().len(), 1);
        let diag = &errs.errors()[0];
        assert_eq!(diag.subject.file.as_deref(), Some("conf2.hcl"));
        assert_eq!(
            diag.context.as_ref().and_then(|c| c.file.as_deref()),
            Some("conf1.hcl")
        );

        // Parse error in one of the files
        let f_bad = ("bad.hcl", "invalid = =\n");
        let bad_errs = merge_files(&[f1, f_bad]).expect_err("should have parse error");
        assert!(bad_errs.has_errors());
    }
}
