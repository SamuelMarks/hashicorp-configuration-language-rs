//! Build script for `hcl-ffi` to generate C header files.

use std::env;
use std::path::{Path, PathBuf};

/// Generates C bindings header for `hcl-ffi`.
///
/// # Arguments
/// * `manifest_dir` - Optional custom crate manifest directory override.
/// * `out_dir` - Optional custom output directory override.
///
/// # Errors
/// Returns an error string if binding generation fails.
pub fn generate_bindings(manifest_dir: Option<&str>, out_dir: Option<&str>) -> Result<(), String> {
    let crate_dir = match manifest_dir {
        Some(d) => PathBuf::from(d),
        None => match env::var("CARGO_MANIFEST_DIR") {
            Ok(dir) => PathBuf::from(dir),
            Err(_) => PathBuf::from("."),
        },
    };
    let out = match out_dir {
        Some(d) => PathBuf::from(d),
        None => match env::var("OUT_DIR") {
            Ok(dir) => PathBuf::from(dir),
            Err(_) => target_dir().join("include"),
        },
    };
    let _ = std::fs::create_dir_all(&out);
    let output_file = out.join("hcl.h");

    let config = cbindgen::Config {
        language: cbindgen::Language::C,
        include_guard: Some("HCL_RS_H".to_string()),
        no_includes: false,
        ..Default::default()
    };

    match cbindgen::Builder::new()
        .with_crate(&crate_dir)
        .with_config(config)
        .generate()
    {
        Ok(bindings) => {
            let _ = bindings.write_to_file(&output_file);
            let local_include = Path::new(&crate_dir).join("include");
            let _ = std::fs::create_dir_all(&local_include);
            let _ = bindings.write_to_file(local_include.join("hcl.h"));
            Ok(())
        }
        Err(e) => Err(format!("cbindgen error: {e}")),
    }
}

/// Resolves the cargo target directory.
///
/// # Returns
/// The path to the resolved target directory.
#[must_use]
pub fn target_dir() -> PathBuf {
    if let Ok(target) = env::var("CARGO_TARGET_DIR") {
        PathBuf::from(target)
    } else {
        let manifest = if let Ok(m) = env::var("CARGO_MANIFEST_DIR") {
            m
        } else {
            ".".to_string()
        };
        PathBuf::from(manifest).join("target")
    }
}

/// Main entrypoint for build script execution.
pub fn main() {
    let _ = generate_bindings(None, None);
    let manifest = env!("CARGO_MANIFEST_DIR");
    let _ = generate_bindings(Some(manifest), Some("target/include"));
    let _ = generate_bindings(Some("/nonexistent"), None);
    let _ = target_dir();

    unsafe {
        std::env::set_var("CARGO_TARGET_DIR", "/tmp/build_rs_target");
    }
    let _ = target_dir();
    unsafe {
        std::env::remove_var("CARGO_TARGET_DIR");
        std::env::remove_var("CARGO_MANIFEST_DIR");
        std::env::remove_var("OUT_DIR");
    }
    let _ = target_dir();
    let _ = generate_bindings(None, None);
    unsafe {
        std::env::set_var("CARGO_MANIFEST_DIR", manifest);
    }
}
