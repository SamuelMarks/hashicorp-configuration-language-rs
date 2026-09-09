# Usage Guide

The `hashicorp-configuration-language-rs` crate exposes multiple levels of interaction with HCL configurations: high-level struct mapping, dynamic evaluation, lossless CST formatting, spec-driven decoding, and low-level AST manipulation.

---

## 1. High-Level Macro Decoding (Recommended)

The most idiomatic way to decode HCL into strongly typed Rust structs is via `hcl_macros::DecodeBody`. This matches HashiCorp's `gohcl` library, mapping evaluated HCL attributes and blocks directly into Rust structures.

```rust
use hashicorp_configuration_language_rs::api::from_str;
use hashicorp_configuration_language_rs::diagnostic::Diagnostics;
use hcl_macros::DecodeBody;

#[derive(DecodeBody, Debug, PartialEq)]
struct ServerConfig {
    name: String,
    port: i64,
    features: Vec<String>,
    enabled: bool,
}

fn main() -> Result<(), Diagnostics> {
    let hcl = r#"
        name     = "web_server"
        port     = 8080
        features = ["http2", "ssl"]
        enabled  = true
    "#;

    let config: ServerConfig = from_str(hcl)?;
    println!("Loaded config: {:?}", config);
    Ok(())
}
```

### Advanced Macro Attributes

- **`#[hcl(block)]`**: Decodes nested blocks into child structs, lists of structs, or label-keyed maps (`HashMap<String, T>`, `BTreeMap<String, T>`).
- **`#[hcl(flatten)]` / `#[hcl(squash)]`**: Inlines child struct fields into the current body scope.
- **`#[hcl(body)]`**: Captures the raw unevaluated `crate::ast::structure::Body` for custom inspection.
- **`#[derive(EncodeBody)]`**: Re-encodes native structs back into formatted CST bodies.

```rust
use hcl_macros::DecodeBody;
use std::collections::HashMap;

#[derive(DecodeBody, Debug)]
struct ResourceBlock {
    ami: String,
    instance_type: String,
}

#[derive(DecodeBody, Debug)]
struct AppConfig {
    // Maps `resource "aws_instance" "web" { ... }` into a nested map
    #[hcl(block)]
    resources: HashMap<String, HashMap<String, ResourceBlock>>,
}
```

---

## 2. Dynamic Evaluation Context

HCL configurations typically feature variable interpolations, math operations, and function calls. You can control the evaluation environment using `Context`.

```rust
use hashicorp_configuration_language_rs::api::from_str_with_context;
use hashicorp_configuration_language_rs::eval::context::Context;
use hashicorp_configuration_language_rs::types::ty::Type;
use hashicorp_configuration_language_rs::types::val::{Value, ValueData};
use hcl_macros::DecodeBody;

#[derive(DecodeBody, Debug)]
struct AppConfig {
    app_id: String,
    workers: i64,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let hcl = r#"
        # Uses built-in standard library function 'upper'
        app_id  = upper("${base_name}-prod")
        workers = base_workers * 2
    "#;

    let mut ctx = Context::with_stdlib();
    ctx.set_variable("base_name", Value::new(Type::String, ValueData::String("billing".into())));
    ctx.set_variable("base_workers", Value::new(Type::Number, ValueData::Number(4.into())));

    let config: AppConfig = from_str_with_context(hcl, &mut ctx)
        .map_err(|diags| format!("Evaluation diagnostics: {:?}", diags))?;

    assert_eq!(config.app_id, "BILLING-PROD");
    assert_eq!(config.workers, 8);
    Ok(())
}
```

---

## 3. Registering Custom Functions

You can expose domain-specific functions to HCL evaluation contexts using `Function::new`:

```rust
use std::sync::Arc;
use hashicorp_configuration_language_rs::api::from_str_with_context;
use hashicorp_configuration_language_rs::eval::context::Context;
use hashicorp_configuration_language_rs::eval::func::Function;
use hashicorp_configuration_language_rs::types::ty::Type;
use hashicorp_configuration_language_rs::types::val::{Value, ValueData};
use hcl_macros::DecodeBody;

#[derive(DecodeBody, Debug)]
struct Config {
    path: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut ctx = Context::with_stdlib();

    let custom_func = Function::new("resolve_path", Arc::new(|args| {
        match &args[0].data {
            ValueData::String(s) => {
                Ok(Value::new(Type::String, ValueData::String(format!("/opt/app/{}", s))))
            }
            _ => Err("expected string argument".to_string()),
        }
    }));

    ctx.set_function("resolve_path", custom_func);

    let hcl = r#"path = resolve_path("config.yml")"#;
    let config: Config = from_str_with_context(hcl, &mut ctx)
        .map_err(|diags| format!("{:?}", diags))?;

    assert_eq!(config.path, "/opt/app/config.yml");
    Ok(())
}
```

---

## 4. Unknown Values in Planning Phases

For infrastructure-as-code planning phases (e.g. Terraform plan), attributes may not be known until apply time. `Value::unknown` propagates safely through calculations and logic without crashing:

```rust
use hashicorp_configuration_language_rs::api::evaluate_expr;
use hashicorp_configuration_language_rs::eval::context::Context;
use hashicorp_configuration_language_rs::types::ty::Type;
use hashicorp_configuration_language_rs::types::val::Value;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut ctx = Context::with_stdlib();
    ctx.set_variable("server_ip", Value::unknown(Type::String));

    // Unknown values propagate safely through functions and templates
    let res = evaluate_expr(r#"upper(server_ip)"#, Some(&ctx))
        .map_err(|diags| format!("{:?}", diags))?;

    assert!(res.is_unknown());
    assert_eq!(res.ty, Type::String);
    Ok(())
}
```

---

## 5. Lossless CST Formatting & Mutation

To format HCL canonically or mutate attributes while preserving original whitespace and comments:

```rust
use hashicorp_configuration_language_rs::cst::format::format_str;

fn main() -> Result<(), hashicorp_configuration_language_rs::error::HclError> {
    let unformatted = r#"
        resource "server" "app" {
        ip="10.0.0.1"
          port =8080
        # Preserve this comment!
        }
    "#;

    let formatted = format_str(unformatted)?;
    println!("{}", formatted);
    Ok(())
}
```

---

## 6. Specification-Driven Decoding (`hcldec`)

When schemas are dynamic and not known at Rust compile time, use `hcldec`:

```rust
use hashicorp_configuration_language_rs::api::parse;
use hashicorp_configuration_language_rs::hcldec::{decode, AttrSpec, BlockSpec, Spec};
use hashicorp_configuration_language_rs::types::ty::Type;
use std::collections::BTreeMap;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = r#"
        region  = "us-east-1"
        workers = 5
    "#;

    let body = parse(input).map_err(|diags| format!("{:?}", diags))?;

    let mut attrs = BTreeMap::new();
    attrs.insert("region".to_string(), AttrSpec {
        name: "region".to_string(),
        ty: Type::String,
        required: true,
    });
    attrs.insert("workers".to_string(), AttrSpec {
        name: "workers".to_string(),
        ty: Type::Number,
        required: true,
    });

    let spec = Spec::Block(BlockSpec {
        attributes: attrs,
        block_types: BTreeMap::new(),
    });

    let val = decode(&body, &spec)?;
    println!("Decoded cty value: {:?}", val);
    Ok(())
}
```

---

## 7. Serde Interoperability

Treat HCL like JSON, YAML, or TOML using the `serde` module:

```rust
use hashicorp_configuration_language_rs::serde::{from_str, to_string};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct DatabaseSettings {
    host: String,
    port: u16,
    active: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = r#"
        host   = "localhost"
        port   = 5432
        active = true
    "#;

    let config: DatabaseSettings = from_str(input)?;
    assert_eq!(config.port, 5432);

    let output = to_string(&config)?;
    println!("HCL Output:\n{}", output);
    Ok(())
}
```

---

## 8. Static Analysis & Linting

Analyze configurations without executing expressions:

```rust
use hashicorp_configuration_language_rs::analysis::{Linter, TypeChecker, ScopeSchema};
use hashicorp_configuration_language_rs::api::parse;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = r#"
        variable "env" {
            default = true ? "prod" : "dev"
        }
    "#;

    let body = parse(input).map_err(|diags| format!("{:?}", diags))?;

    // Lint for dead branches and tautological conditionals
    let linter = Linter::new();
    let lint_diags = linter.lint_body(&body);
    for diag in lint_diags.diagnostics() {
        println!("Lint Warning: {:?}", diag.summary);
    }
    Ok(())
}
```

---

## 9. Migrating Legacy HCL 1.0 to HCL 2.0

Transform legacy HCL1 files (such as old Packer or Terraform 0.11 configs) into modern HCL2:

```rust
use hashicorp_configuration_language_rs::hcl1::parser::Hcl1Parser;
use hashicorp_configuration_language_rs::hcl1::migrate::migrate_hcl1_to_hcl2;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let hcl1_input = r#"
        variable "name" {
            default = "${var.prefix}-app"
        }
    "#;

    let hcl1_file = Hcl1Parser::parse(hcl1_input)?;
    let hcl2_body = migrate_hcl1_to_hcl2(&hcl1_file);

    println!("Migrated HCL2 Body blocks: {}", hcl2_body.blocks.len());
    Ok(())
}
```

---

## 10. CLI Tools

Install or run the included command-line utilities:

```bash
# Format files in-place with 2-space canonical indentation
cargo run --bin hclfmt -- -w main.hcl

# Convert HCL files to JSON
cargo run --bin hcl2json -- config.hcl

# Start the interactive HCL evaluation console
cargo run --bin hcl-repl

# Validate HCL syntax and schemas recursively in CI
cargo run --bin hcl-validate -- ./configs/
```

---

## 11. IDE & Language Server (`hcl-lsp`)

Start the Language Server Protocol daemon for editor integration:

```bash
# Run over stdio (standard for VS Code, Neovim, Helix)
cargo run --bin hcl-lsp -- --stdio

# Or run over a TCP port
cargo run --bin hcl-lsp -- --tcp 127.0.0.1:9257
```
