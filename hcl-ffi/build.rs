//! Build script for `hcl-ffi` to generate C header files.

use std::env;
use std::path::{Path, PathBuf};

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let out_dir = target_dir().join("include");
    let _ = std::fs::create_dir_all(&out_dir);
    let output_file = out_dir.join("hcl.h");

    let config = cbindgen::Config {
        language: cbindgen::Language::C,
        include_guard: Some("HCL_RS_H".to_string()),
        no_includes: false,
        ..Default::default()
    };

    if let Ok(bindings) = cbindgen::Builder::new()
        .with_crate(&crate_dir)
        .with_config(config)
        .generate()
    {
        let _ = bindings.write_to_file(&output_file);
        // Also write to local include/hcl.h
        let local_include = Path::new(&crate_dir).join("include");
        let _ = std::fs::create_dir_all(&local_include);
        let _ = bindings.write_to_file(local_include.join("hcl.h"));
    }
}

fn target_dir() -> PathBuf {
    if let Ok(target) = env::var("CARGO_TARGET_DIR") {
        PathBuf::from(target)
    } else {
        let manifest = env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(manifest).join("target")
    }
}
