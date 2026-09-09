# Usage Guide

The `hashicorp-configuration-language-rs` crate exposes several ways to interact with HCL configurations, from high-level struct mapping to low-level AST manipulation.

## 1. High-Level Macro Decoding (Recommended)

The most idiomatic way to use this crate is to use the `hcl_macros::DecodeBody` procedural macro. This is equivalent to HashiCorp's `gohcl` library, natively coercing evaluated HCL straight into Rust structs.

```rust
use hcl_macros::DecodeBody;
use hashicorp_configuration_language_rs::api::from_str;

#[derive(DecodeBody, Debug)]
struct ServerConfig {
    name: String,
    port: i64,
    features: Vec<String>,
    enabled: bool,
}

fn main() {
    let hcl = r#"
        name = "web_server"
        port = 8080
        features = ["http2", "ssl"]
        enabled = true
    "#;

    match from_str::<ServerConfig>(hcl) {
        Ok(config) => println!("Config loaded: {:#?}", config),
        Err(diagnostics) => {
            // Diagnostics handle multiple errors at once!
            for err in diagnostics.errors() {
                eprintln!("Error: {} at {:?}", err.summary, err.subject);
            }
        }
    }
}
```

## 2. Using an Evaluation Context

HCL's power comes from dynamic evaluation—variable references, standard library functions, and mathematical operations. You can execute evaluations by passing a `Context`.

```rust
use hashicorp_configuration_language_rs::api::from_str_with_context;
use hashicorp_configuration_language_rs::eval::context::Context;
use hashicorp_configuration_language_rs::types::val::{Value, ValueData};
use hashicorp_configuration_language_rs::types::ty::Type;
use hcl_macros::DecodeBody;

#[derive(DecodeBody, Debug)]
struct AppConfig {
    app_id: String,
    workers: i64,
}

fn main() -> Result<(), hashicorp_configuration_language_rs::error::HclError> {
    let hcl = r#"
        # Uses standard library 'upper' and custom variable 'base_name'
        app_id = upper("${base_name}-prod")
        # Evaluates math
        workers = 2 * 4
    "#;

    let mut ctx = Context::new();
    // Inject a variable into the context
    ctx.set_variable("base_name", Value::new(Type::String, ValueData::String("billing".to_string())));

    let config: AppConfig = from_str_with_context(hcl, &mut ctx)?;
    
    assert_eq!(config.app_id, "BILLING-PROD");
    assert_eq!(config.workers, 8);
    Ok(())
}
```

## 3. Injecting Custom Functions

If you are building a tool like Vagrant or Packer, you often need to provide domain-specific functions to the user's HCL context.

```rust
use std::sync::Arc;
use hashicorp_configuration_language_rs::eval::context::Context;
use hashicorp_configuration_language_rs::eval::func::Function;
use hashicorp_configuration_language_rs::types::val::{Value, ValueData};
use hashicorp_configuration_language_rs::types::ty::Type;
use hashicorp_configuration_language_rs::api::from_str_with_context;
use hcl_macros::DecodeBody;

#[derive(DecodeBody)]
struct Config {
    path: String,
}

fn main() -> Result<(), hashicorp_configuration_language_rs::error::HclError> {
    let mut ctx = Context::new();

    let custom_func = Function {
        name: "resolve_path".to_string(),
        func: Arc::new(|args| {
            if let ValueData::String(ref s) = *args[0].data {
                // Perform arbitrary Rust logic here
                Ok(Value::new(Type::String, ValueData::String(format!("/opt/app/{}", s))))
            } else {
                Err("Type unification guarantees this is safe if properly defined".to_string())
            }
        })
    };

    ctx.set_function("resolve_path", custom_func);

    let hcl = r#"path = resolve_path("config.yml")"#;
    let config: Config = from_str_with_context(hcl, &mut ctx)?;

    assert_eq!(config.path, "/opt/app/config.yml");
    Ok(())
}
```

## 4. Serde Integration

If you want standard serialization/deserialization into types that implement `serde::Serialize` and `serde::DeserializeOwned`, you can use the `serde` module directly.

```rust
use serde::{Deserialize, Serialize};
use hashicorp_configuration_language_rs::serde::{from_str, to_string};

#[derive(Serialize, Deserialize, Debug)]
struct GeneralConfig {
    host: String,
    port: i32,
}

fn main() -> Result<(), hashicorp_configuration_language_rs::error::HclError> {
    let hcl_input = r#"
        host = "localhost"
        port = 5432
    "#;

    // Parse HCL into a Serde-compatible struct
    let config: GeneralConfig = from_str(hcl_input)?;
    
    // Serialize a Struct back into cleanly formatted HCL
    let hcl_output = to_string(&config)?;
    println!("{}", hcl_output);
    Ok(())
}
```

## 5. Working directly with the AST

For tooling that needs to format, lint, or analyze HCL without fully evaluating it, you can parse directly into the AST:

```rust
use hashicorp_configuration_language_rs::api::parse;

fn main() -> Result<(), hashicorp_configuration_language_rs::error::HclError> {
    let hcl = r#"
        resource "aws_instance" "web" {
            ami = "ami-123456"
        }
    "#;

    let body = parse(hcl)?;

    // Iterate over blocks
    for block in &body.blocks {
        println!("Block Type: {}", block.block_type);
        println!("Labels: {:?}", block.labels);

        // Inspect the block's inner body
        for (attr_name, attr) in &block.body.attributes {
            println!("Attribute: {} = {:?}", attr_name, attr.expr);
        }
    }
    Ok(())
}
```
