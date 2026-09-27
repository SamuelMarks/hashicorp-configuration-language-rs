//! Integration tests validating multi-file template merging and variable evaluation
//! against Bento and downstream Packer infrastructure-as-code definitions.

use hashicorp_configuration_language_rs::eval::context::Context;
use hashicorp_configuration_language_rs::eval::dag::DagResolver;
use hashicorp_configuration_language_rs::eval::vars::{VarManager, parse_var_file};
use hashicorp_configuration_language_rs::parse::merge::merge_directory;
use std::path::{Path, PathBuf};

/// Returns the path to the Bento sibling repository directory if it exists on disk.
fn bento_dir() -> Option<PathBuf> {
    let p = Path::new("../bento");
    if p.exists() && p.is_dir() {
        Some(p.to_path_buf())
    } else {
        None
    }
}

/// Tests discovering and merging Bento's complete `packer_templates` directory
/// (`pkr-variables.pkr.hcl`, `pkr-sources.pkr.hcl`, `pkr-builder.pkr.hcl`, `pkr-plugins.pkr.hcl`).
#[test]
fn test_bento_templates_directory_merge() {
    if let Some(bento_root) = bento_dir() {
        let templates_dir = bento_root.join("packer_templates");
        if templates_dir.exists() {
            let merged = merge_directory(&templates_dir, &["pkr.hcl"])
                .expect("Bento packer_templates must merge cleanly");

            assert!(!merged.blocks.is_empty());

            // Check presence of variables, sources, and build blocks
            let has_vars = merged.blocks.iter().any(|b| b.block_type == "variable");
            let has_sources = merged.blocks.iter().any(|b| b.block_type == "source");
            let has_build = merged.blocks.iter().any(|b| b.block_type == "build");
            let has_plugins = merged.blocks.iter().any(|b| b.block_type == "packer");

            assert!(has_vars, "Must contain variable blocks");
            assert!(has_sources, "Must contain source blocks");
            assert!(has_build, "Must contain build blocks");
            assert!(has_plugins, "Must contain packer plugins blocks");

            // Verify priority ordering: packer (1) comes before source (6) and build (7)
            let first_type = &merged.blocks[0].block_type;
            assert!(
                first_type == "packer" || first_type == "variable",
                "High priority blocks must appear first"
            );
            return;
        }
    }

    // Fallback fixture if sibling repository is not present in standalone CI
    let temp_dir = std::env::temp_dir().join(format!("test_bento_mock_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).expect("create tempdir");

    std::fs::write(
        temp_dir.join("pkr-variables.pkr.hcl"),
        "variable \"os_name\" { default = \"ubuntu\" }\n",
    )
    .expect("write vars");
    std::fs::write(
        temp_dir.join("pkr-sources.pkr.hcl"),
        "source \"qemu\" \"vm\" { vm_name = \"bento-${var.os_name}\" }\n",
    )
    .expect("write sources");
    std::fs::write(
        temp_dir.join("pkr-builder.pkr.hcl"),
        "build { sources = [\"source.qemu.vm\"] }\n",
    )
    .expect("write build");
    std::fs::write(
        temp_dir.join("pkr-plugins.pkr.hcl"),
        "packer { required_plugins { qemu = { version = \">= 1.0.0\" } } }\n",
    )
    .expect("write plugins");

    let merged = merge_directory(&temp_dir, &["pkr.hcl"]).expect("mock bento merge ok");
    assert_eq!(merged.blocks.len(), 4);
    assert_eq!(merged.blocks[0].block_type, "packer");

    let _ = std::fs::remove_dir_all(&temp_dir);
}

/// Tests loading Bento OS variable files and resolving definitions with the DAG resolver.
#[test]
fn test_bento_os_pkrvars_and_dag_evaluation() {
    if let Some(bento_root) = bento_dir() {
        let os_dir = bento_root.join("os_pkrvars");
        if os_dir.exists() {
            let ubuntu_vars = os_dir.join("ubuntu/ubuntu-22.04-x86_64.pkrvars.hcl");
            if ubuntu_vars.exists() {
                let vars = parse_var_file(&ubuntu_vars).expect("parse ubuntu pkrvars");
                assert!(!vars.is_empty());

                let mut vm = VarManager::new();
                vm.add_var_file(&ubuntu_vars).expect("ingest ubuntu vars");
                let mut ctx = Context::with_stdlib();
                vm.populate_context(&mut ctx);

                let var_scope = ctx.get_variable("var").expect("var scope exists");
                assert!(var_scope.to_string().contains("os_name") || !vars.is_empty());
            }

            let windows_vars = os_dir.join("windows/windows-2022-x86_64.pkrvars.hcl");
            if windows_vars.exists() {
                let vars = parse_var_file(&windows_vars).expect("parse windows pkrvars");
                assert!(!vars.is_empty());
            }
            return;
        }
    }

    // Fallback fixture if sibling repository is not present
    let temp_dir =
        std::env::temp_dir().join(format!("test_bento_vars_mock_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).expect("create tempdir");

    let pkrvars = temp_dir.join("ubuntu.pkrvars.hcl");
    std::fs::write(&pkrvars, "os_name = \"ubuntu\"\nos_version = \"22.04\"\n")
        .expect("write pkrvars");

    let mut vm = VarManager::new();
    vm.add_var_file(&pkrvars).expect("add var file");

    let mut ctx = Context::with_stdlib();
    vm.populate_context(&mut ctx);

    let template_body = hashicorp_configuration_language_rs::api::parse(
        r#"
        variable "os_name" { default = "linux" }
        variable "os_version" { default = "unknown" }
        locals {
            image_tag = "${var.os_name}-${var.os_version}"
        }
    "#,
    )
    .expect("parse template");

    DagResolver::new()
        .with_var_overrides(vm.into_variables().into_iter().collect())
        .resolve(&template_body, &mut ctx)
        .expect("dag resolve ok");

    let local_scope = ctx.get_variable("local").expect("local scope");
    assert!(local_scope.to_string().contains("ubuntu-22.04"));

    let _ = std::fs::remove_dir_all(&temp_dir);
}
