//! Build script for `hcl-ffi` to generate C header files.
#![forbid(missing_docs)]
#![forbid(rustdoc::missing_crate_level_docs)]
#![forbid(clippy::all, clippy::pedantic, clippy::nursery)]
#![forbid(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::env;
use std::io;
use std::path::{Path, PathBuf};

/// Generates C bindings header for `hcl-ffi`.
///
/// # Arguments
/// * `manifest_dir` - Optional custom crate manifest directory override.
/// * `out_dir` - Optional custom output directory override.
///
/// # Errors
/// Returns [`io::Error`] if binding generation fails.
pub fn generate_bindings(manifest_dir: Option<&str>, out_dir: Option<&str>) -> io::Result<()> {
    let crate_dir = PathBuf::from(manifest_dir.unwrap_or("."));
    let out = out_dir.map_or_else(
        || target_dir(manifest_dir, None).join("include"),
        PathBuf::from,
    );
    let _ = std::fs::create_dir_all(&out);
    let output_file = out.join("hcl.h");

    let config = cbindgen::Config {
        language: cbindgen::Language::C,
        include_guard: Some("HCL_RS_H".to_string()),
        no_includes: false,
        export: cbindgen::ExportConfig {
            item_types: vec![
                cbindgen::ItemType::OpaqueItems,
                cbindgen::ItemType::Functions,
            ],
            rename: std::collections::HashMap::from([
                ("HclBodyT".to_string(), "hcl_body_t".to_string()),
                ("HclContextT".to_string(), "hcl_context_t".to_string()),
                ("HclValueT".to_string(), "hcl_value_t".to_string()),
                (
                    "HclDiagnosticsT".to_string(),
                    "hcl_diagnostics_t".to_string(),
                ),
            ]),
            ..Default::default()
        },
        ..Default::default()
    };

    cbindgen::Builder::new()
        .with_crate(&crate_dir)
        .with_config(config)
        .generate()
        .map_or_else(
            |e| Err(io::Error::other(format!("cbindgen error: {e}"))),
            |bindings| {
                let _ = bindings.write_to_file(&output_file);
                let local_include = Path::new(&crate_dir).join("include");
                let _ = std::fs::create_dir_all(&local_include);
                let _ = bindings.write_to_file(local_include.join("hcl.h"));
                Ok(())
            },
        )
}

/// Resolves the cargo target directory.
///
/// # Arguments
/// * `manifest_override` - Optional manifest directory override.
/// * `target_override` - Optional target directory override.
///
/// # Returns
/// The path to the resolved target directory.
#[must_use]
pub fn target_dir(manifest_override: Option<&str>, target_override: Option<&str>) -> PathBuf {
    target_override.map_or_else(
        || PathBuf::from(manifest_override.unwrap_or(".")).join("target"),
        PathBuf::from,
    )
}

/// Main entrypoint for build script execution.
pub fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").ok();
    let out_dir = env::var("OUT_DIR").ok();
    let _ = generate_bindings(manifest_dir.as_deref(), out_dir.as_deref());
}
