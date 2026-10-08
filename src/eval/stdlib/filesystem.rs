//! Filesystem and template standard library functions.
use crate::ast::expr::Expression;
use crate::diagnostic::Diagnostics;
use crate::eval::context::Context;
use crate::eval::evaluator::Evaluator;
use crate::eval::fs::{FileSystem, OsFileSystem};
use crate::eval::func::Function;
use crate::span::Span;
use crate::types::ty::Type;
use crate::types::val::{Value, ValueData};
use base64::prelude::*;
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha512};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
/// Returns all filesystem and template standard library functions using the default OS filesystem.
#[must_use]
pub fn functions() -> Vec<Function> {
    functions_with_fs(Arc::new(OsFileSystem))
}
/// Returns all filesystem and template standard library functions bound to a custom virtual filesystem.
#[must_use]
pub fn functions_with_fs(fs: Arc<dyn FileSystem>) -> Vec<Function> {
    vec![
        abspath_func(fs.clone()),
        basename_func(),
        dirname_func(),
        file_func(fs.clone()),
        filebase64_func(fs.clone()),
        filebase64sha256_func(fs.clone()),
        fileexists_func(fs.clone()),
        filemd5_func(fs.clone()),
        fileset_func(fs.clone()),
        filesha1_func(fs.clone()),
        filesha256_func(fs.clone()),
        filesha512_func(fs.clone()),
        pathexpand_func(fs.clone()),
        templatefile_func(fs),
    ]
}
fn file_func(fs: Arc<dyn FileSystem>) -> Function {
    Function {
        name: "file".to_string(),
        func: Arc::new(move |args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("file expects 1 argument: path".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }
            let ValueData::String(path) = &*args[0].data else {
                return Err("file expects a string path argument".to_string());
            };
            let content = fs
                .read_to_string(Path::new(path))
                .map_err(|e| format!("Failed to read file '{path}': {e}"))?;
            Ok(Value::new(Type::String, ValueData::String(content)))
        }),
        signature: None,
    }
}
fn fileexists_func(fs: Arc<dyn FileSystem>) -> Function {
    Function {
        name: "fileexists".to_string(),
        func: Arc::new(move |args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("fileexists expects 1 argument: path".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::Bool));
            }
            let ValueData::String(path) = &*args[0].data else {
                return Err("fileexists expects a string path argument".to_string());
            };
            let exists = fs.is_file(Path::new(path));
            Ok(Value::new(Type::Bool, ValueData::Bool(exists)))
        }),
        signature: None,
    }
}
fn fileset_func(fs: Arc<dyn FileSystem>) -> Function {
    Function {
        name: "fileset".to_string(),
        func: Arc::new(move |args: &[Value]| -> Result<Value, String> {
            if args.len() != 2 {
                return Err("fileset expects 2 arguments: base_path and glob_pattern".to_string());
            }
            if args[0].is_unknown() || args[1].is_unknown() {
                return Ok(Value::unknown(Type::Set(Box::new(Type::String))));
            }
            let ValueData::String(base_path) = &*args[0].data else {
                return Err("First argument to fileset must be a string path".to_string());
            };
            let ValueData::String(pattern) = &*args[1].data else {
                return Err("Second argument to fileset must be a glob pattern string".to_string());
            };
            let regex = glob_to_regex(pattern)
                .map_err(|e| format!("Invalid glob pattern '{pattern}': {e}"))?;
            let mut matches = BTreeSet::new();
            let base_p = Path::new(base_path);
            walk_dir_recursive(&*fs, base_p, base_p, &regex, &mut matches)
                .map_err(|e| format!("Error traversing '{base_path}': {e}"))?;
            Ok(Value::new(
                Type::Set(Box::new(Type::String)),
                ValueData::Set(matches),
            ))
        }),
        signature: None,
    }
}
fn filebase64_func(fs: Arc<dyn FileSystem>) -> Function {
    Function {
        name: "filebase64".to_string(),
        func: Arc::new(move |args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("filebase64 expects 1 argument: path".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }
            let ValueData::String(path) = &*args[0].data else {
                return Err("filebase64 expects a string path argument".to_string());
            };
            let bytes = fs
                .read(Path::new(path))
                .map_err(|e| format!("Failed to read file '{path}': {e}"))?;
            let b64 = BASE64_STANDARD.encode(&bytes);
            Ok(Value::new(Type::String, ValueData::String(b64)))
        }),
        signature: None,
    }
}
fn filemd5_func(fs: Arc<dyn FileSystem>) -> Function {
    Function {
        name: "filemd5".to_string(),
        func: Arc::new(move |args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("filemd5 expects 1 argument: path".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }
            let ValueData::String(path) = &*args[0].data else {
                return Err("filemd5 expects a string path argument".to_string());
            };
            let bytes = fs
                .read(Path::new(path))
                .map_err(|e| format!("Failed to read file '{path}': {e}"))?;
            let digest = md5::compute(&bytes);
            Ok(Value::new(
                Type::String,
                ValueData::String(format!("{digest:x}")),
            ))
        }),
        signature: None,
    }
}
fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(out, "{b:02x}");
    }
    out
}
fn filesha1_func(fs: Arc<dyn FileSystem>) -> Function {
    Function {
        name: "filesha1".to_string(),
        func: Arc::new(move |args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("filesha1 expects 1 argument: path".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }
            let ValueData::String(path) = &*args[0].data else {
                return Err("filesha1 expects a string path argument".to_string());
            };
            let bytes = fs
                .read(Path::new(path))
                .map_err(|e| format!("Failed to read file '{path}': {e}"))?;
            let mut hasher = Sha1::new();
            hasher.update(&bytes);
            let digest = hasher.finalize();
            Ok(Value::new(
                Type::String,
                ValueData::String(hex_encode(&digest)),
            ))
        }),
        signature: None,
    }
}
fn filesha256_func(fs: Arc<dyn FileSystem>) -> Function {
    Function {
        name: "filesha256".to_string(),
        func: Arc::new(move |args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("filesha256 expects 1 argument: path".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }
            let ValueData::String(path) = &*args[0].data else {
                return Err("filesha256 expects a string path argument".to_string());
            };
            let bytes = fs
                .read(Path::new(path))
                .map_err(|e| format!("Failed to read file '{path}': {e}"))?;
            let mut hasher = Sha256::new();
            hasher.update(&bytes);
            let digest = hasher.finalize();
            Ok(Value::new(
                Type::String,
                ValueData::String(hex_encode(&digest)),
            ))
        }),
        signature: None,
    }
}
fn filesha512_func(fs: Arc<dyn FileSystem>) -> Function {
    Function {
        name: "filesha512".to_string(),
        func: Arc::new(move |args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("filesha512 expects 1 argument: path".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }
            let ValueData::String(path) = &*args[0].data else {
                return Err("filesha512 expects a string path argument".to_string());
            };
            let bytes = fs
                .read(Path::new(path))
                .map_err(|e| format!("Failed to read file '{path}': {e}"))?;
            let mut hasher = Sha512::new();
            hasher.update(&bytes);
            let digest = hasher.finalize();
            Ok(Value::new(
                Type::String,
                ValueData::String(hex_encode(&digest)),
            ))
        }),
        signature: None,
    }
}
fn filebase64sha256_func(fs: Arc<dyn FileSystem>) -> Function {
    Function {
        name: "filebase64sha256".to_string(),
        func: Arc::new(move |args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("filebase64sha256 expects 1 argument: path".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }
            let ValueData::String(path) = &*args[0].data else {
                return Err("filebase64sha256 expects a string path argument".to_string());
            };
            let bytes = fs
                .read(Path::new(path))
                .map_err(|e| format!("Failed to read file '{path}': {e}"))?;
            let mut hasher = Sha256::new();
            hasher.update(&bytes);
            let digest = hasher.finalize();
            let b64 = BASE64_STANDARD.encode(digest);
            Ok(Value::new(Type::String, ValueData::String(b64)))
        }),
        signature: None,
    }
}
/// Resolves the current working directory using the provided resolver closure.
///
/// # Arguments
/// * `cwd_fn` - A closure returning the current directory result.
fn resolve_current_dir(cwd_fn: &dyn Fn() -> std::io::Result<PathBuf>) -> PathBuf {
    match cwd_fn() {
        Ok(dir) => dir,
        Err(_) => PathBuf::from("."),
    }
}
fn abspath_func(fs: Arc<dyn FileSystem>) -> Function {
    Function {
        name: "abspath".to_string(),
        func: Arc::new(move |args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("abspath expects 1 argument: path".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }
            let ValueData::String(path_str) = &*args[0].data else {
                return Err("abspath expects a string path argument".to_string());
            };
            let p = Path::new(path_str);
            let abs_path = if let Ok(canon) = fs.canonicalize(p) {
                canon
            } else if p.is_absolute() {
                p.to_path_buf()
            } else {
                let cwd = resolve_current_dir(&std::env::current_dir);
                cwd.join(p)
            };
            let res = abs_path.to_string_lossy().replace('\\', "/");
            Ok(Value::new(Type::String, ValueData::String(res)))
        }),
        signature: None,
    }
}
fn dirname_func() -> Function {
    Function {
        name: "dirname".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("dirname expects 1 argument: path".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }
            let ValueData::String(path_str) = &*args[0].data else {
                return Err("dirname expects a string path argument".to_string());
            };
            let p = Path::new(path_str);
            let dir = p.parent().map_or(".", |par| {
                let s = par.to_str().unwrap_or(".");
                if s.is_empty() { "." } else { s }
            });
            let clean = dir.replace('\\', "/");
            Ok(Value::new(Type::String, ValueData::String(clean)))
        }),
        signature: None,
    }
}
fn basename_func() -> Function {
    Function {
        name: "basename".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("basename expects 1 argument: path".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }
            let ValueData::String(path_str) = &*args[0].data else {
                return Err("basename expects a string path argument".to_string());
            };
            let p = Path::new(path_str);
            let name = p
                .file_name()
                .map_or_else(|| p.to_string_lossy(), |f| f.to_string_lossy());
            Ok(Value::new(
                Type::String,
                ValueData::String(name.to_string()),
            ))
        }),
        signature: None,
    }
}
/// Resolves user home directory using the provided environment lookup closure.
///
/// # Arguments
/// * `var_fn` - An environment variable lookup closure.
fn resolve_home_dir(var_fn: &dyn Fn(&str) -> Result<String, std::env::VarError>) -> String {
    match var_fn("HOME").or_else(|_| var_fn("USERPROFILE")) {
        Ok(dir) => dir,
        Err(_) => ".".to_string(),
    }
}
fn pathexpand_func(_fs: Arc<dyn FileSystem>) -> Function {
    Function {
        name: "pathexpand".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("pathexpand expects 1 argument: path".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }
            let ValueData::String(path_str) = &*args[0].data else {
                return Err("pathexpand expects a string path argument".to_string());
            };
            if path_str == "~" || path_str.starts_with("~/") || path_str.starts_with("~\\") {
                let home = resolve_home_dir(&|k| std::env::var(k));
                let expanded = if path_str == "~" {
                    home
                } else {
                    format!("{}{}", home, &path_str[1..])
                };
                return Ok(Value::new(
                    Type::String,
                    ValueData::String(expanded.replace('\\', "/")),
                ));
            }
            Ok(Value::new(
                Type::String,
                ValueData::String(path_str.replace('\\', "/")),
            ))
        }),
        signature: None,
    }
}
fn templatefile_func(fs: Arc<dyn FileSystem>) -> Function {
    Function {
        name: "templatefile".to_string(),
        func: Arc::new(move |args: &[Value]| -> Result<Value, String> {
            if args.len() != 2 {
                return Err("templatefile expects 2 arguments: path and vars".to_string());
            }
            if args[0].is_unknown() || args[1].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }
            let ValueData::String(path_str) = &*args[0].data else {
                return Err("First argument to templatefile must be a string path".to_string());
            };
            let content = fs
                .read_to_string(Path::new(path_str))
                .map_err(|e| format!("Failed to read template file '{path_str}': {e}"))?;
            let mut ctx = Context::with_stdlib().with_filesystem(fs.clone());
            let ValueData::Object(map) = &*args[1].data else {
                return Err(
                    "Second argument to templatefile must be an object of variables".to_string(),
                );
            };
            for (k, v) in map {
                ctx.set_variable(k, v.clone());
            }
            let dummy_span = Span::new(0, 0, 0, 0, 0, 0);
            let quoted = format!("\"{content}\"");
            let mut diags = Diagnostics::new();
            let parts =
                crate::parse::parser::Parser::parse_template(&mut diags, &quoted, &dummy_span);
            if diags.has_errors() {
                return Err(format!("Failed to parse template: {:?}", diags.errors()));
            }
            let expr = Expression::Template(parts, dummy_span);
            let (val, _eval_diags) = Evaluator::new(&ctx)
                .evaluate(&expr)
                .map_err(|d| format!("Failed to evaluate template: {:?}", d.errors()))?;
            Ok(val)
        }),
        signature: None,
    }
}
fn glob_to_regex(pattern: &str) -> Result<regex::Regex, String> {
    if pattern.contains("***") {
        return Err("glob pattern cannot contain '***'".to_string());
    }
    let mut regex_pattern = String::from("^");
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' => {
                if chars.peek() == Some(&'*') {
                    chars.next();
                    if chars.peek() == Some(&'/') {
                        chars.next();
                        regex_pattern.push_str("(?:.*/)?");
                    } else {
                        regex_pattern.push_str(".*");
                    }
                } else {
                    regex_pattern.push_str("[^/]*");
                }
            }
            '?' => regex_pattern.push_str("[^/]"),
            '.' | '+' | '(' | ')' | '{' | '}' | '^' | '$' | '|' | '\\' => {
                regex_pattern.push('\\');
                regex_pattern.push(c);
            }
            _ => regex_pattern.push(c),
        }
    }
    regex_pattern.push('$');
    regex::Regex::new(&regex_pattern).map_err(|e| e.to_string())
}
fn walk_dir_recursive(
    fs: &dyn FileSystem,
    dir: &Path,
    base: &Path,
    regex: &regex::Regex,
    matches: &mut BTreeSet<Value>,
) -> Result<(), String> {
    let entries = fs
        .read_dir(dir)
        .map_err(|e| format!("Failed to read directory '{}': {e}", dir.display()))?;
    for path in entries {
        if fs.is_dir(&path) {
            walk_dir_recursive(fs, &path, base, regex, matches)?;
        } else {
            let rel = match path.strip_prefix(base) {
                Ok(r) => r,
                Err(_) => &path,
            };
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            if regex.is_match(&rel_str) {
                matches.insert(Value::new(Type::String, ValueData::String(rel_str)));
            }
        }
    }
    Ok(())
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
    use crate::error::HclError;
    use std::collections::BTreeMap;
    fn eval_fs_func(name: &str, args: &[Value]) -> Result<Value, String> {
        let func = functions().into_iter().find(|f| f.name == name).unwrap();
        (func.func)(args)
    }
    fn unwrap_val(res: Result<Value, String>) -> Value {
        match res {
            Ok(v) => v,
            Err(_) => Value::null(Type::Dynamic),
        }
    }
    #[test]
    fn test_filesystem_functions() {
        let temp_dir = std::env::temp_dir().join(format!("hcl_test_fs_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let file_path = temp_dir.join("test_file.txt");
        std::fs::write(&file_path, "Hello Filesystem!").unwrap();
        let path_str = file_path.to_str().unwrap();
        let p_val = Value::new(Type::String, ValueData::String(path_str.to_string()));
        let content = eval_fs_func("file", std::slice::from_ref(&p_val)).unwrap();
        assert_eq!(
            content.data.as_ref(),
            &ValueData::String("Hello Filesystem!".into())
        );
        let exists = eval_fs_func("fileexists", std::slice::from_ref(&p_val)).unwrap();
        assert_eq!(exists.data.as_ref(), &ValueData::Bool(true));
        let not_exists = eval_fs_func(
            "fileexists",
            &[Value::new(
                Type::String,
                ValueData::String("nonexistent.txt".into()),
            )],
        )
        .unwrap();
        assert_eq!(not_exists.data.as_ref(), &ValueData::Bool(false));
        let b64 = eval_fs_func("filebase64", std::slice::from_ref(&p_val)).unwrap();
        assert_eq!(
            b64.data.as_ref(),
            &ValueData::String(BASE64_STANDARD.encode("Hello Filesystem!"))
        );
        let b64sha = eval_fs_func("filebase64sha256", std::slice::from_ref(&p_val)).unwrap();
        let mut hasher = Sha256::new();
        hasher.update(b"Hello Filesystem!");
        let expected_hash = BASE64_STANDARD.encode(hasher.finalize());
        assert_eq!(b64sha.data.as_ref(), &ValueData::String(expected_hash));
        let dir = unwrap_val(eval_fs_func("dirname", std::slice::from_ref(&p_val)));
        assert_eq!(dir.ty(), &Type::String);
        let base = unwrap_val(eval_fs_func("basename", std::slice::from_ref(&p_val)));
        assert_eq!(
            base.data.as_ref(),
            &ValueData::String("test_file.txt".into())
        );
        let abs = unwrap_val(eval_fs_func("abspath", std::slice::from_ref(&p_val)));
        assert_eq!(abs.ty(), &Type::String);
        let expanded = unwrap_val(eval_fs_func(
            "pathexpand",
            &[Value::new(Type::String, ValueData::String("~/docs".into()))],
        ));
        assert_eq!(expanded.ty(), &Type::String);
        let check_tilde = |v: &Value| match &*v.data {
            ValueData::String(s) => s.starts_with('~'),
            _ => true,
        };
        assert!(!check_tilde(&expanded));
        assert!(check_tilde(&Value::null(Type::Dynamic)));
        let sub_file = temp_dir.join("nested.txt");
        let _ = std::fs::write(&sub_file, "nested content");
        let dir_val = Value::new(
            Type::String,
            ValueData::String(temp_dir.to_str().unwrap_or_default().to_string()),
        );
        let glob_val = Value::new(Type::String, ValueData::String("*.txt".into()));
        let set_res = unwrap_val(eval_fs_func("fileset", &[dir_val, glob_val]));
        let set_len = |v: &Value| match &*v.data {
            ValueData::Set(s) => s.len(),
            _ => 0,
        };
        assert!(set_len(&set_res) > 0);
        assert_eq!(set_len(&Value::null(Type::Dynamic)), 0);
        let tpl_path = temp_dir.join("tpl.txt");
        let _ = std::fs::write(&tpl_path, "Hello, ${name}! Port: ${port}");
        let mut vars_map = BTreeMap::new();
        vars_map.insert(
            "name".to_string(),
            Value::new(Type::String, ValueData::String("Rust".into())),
        );
        vars_map.insert(
            "port".to_string(),
            Value::new(Type::Number, ValueData::Number(8080_i32.into())),
        );
        let vars_val = Value::new(Type::Dynamic, ValueData::Object(vars_map));
        let tpl_res = eval_fs_func(
            "templatefile",
            &[
                Value::new(
                    Type::String,
                    ValueData::String(tpl_path.to_str().unwrap_or_default().to_string()),
                ),
                vars_val,
            ],
        )
        .unwrap();
        assert_eq!(
            tpl_res.data.as_ref(),
            &ValueData::String("Hello, Rust! Port: 8080".into())
        );
        let fmd5 = eval_fs_func("filemd5", std::slice::from_ref(&p_val)).unwrap();
        assert_eq!(
            fmd5.data.as_ref(),
            &ValueData::String(format!("{:x}", md5::compute(b"Hello Filesystem!")))
        );
        let fsha1 = eval_fs_func("filesha1", std::slice::from_ref(&p_val)).unwrap();
        let mut hasher1 = Sha1::new();
        hasher1.update(b"Hello Filesystem!");
        assert_eq!(
            fsha1.data.as_ref(),
            &ValueData::String(hex_encode(&hasher1.finalize()))
        );
        let fsha256 = eval_fs_func("filesha256", std::slice::from_ref(&p_val)).unwrap();
        let mut hasher256 = Sha256::new();
        hasher256.update(b"Hello Filesystem!");
        assert_eq!(
            fsha256.data.as_ref(),
            &ValueData::String(hex_encode(&hasher256.finalize()))
        );
        let fsha512 = eval_fs_func("filesha512", std::slice::from_ref(&p_val)).unwrap();
        let mut hasher512 = Sha512::new();
        hasher512.update(b"Hello Filesystem!");
        assert_eq!(
            fsha512.data.as_ref(),
            &ValueData::String(hex_encode(&hasher512.finalize()))
        );
        let unk_str = Value::unknown(Type::String);
        assert!(
            eval_fs_func("filemd5", std::slice::from_ref(&unk_str))
                .unwrap()
                .is_unknown()
        );
        assert!(
            eval_fs_func("filesha1", std::slice::from_ref(&unk_str))
                .unwrap()
                .is_unknown()
        );
        assert!(
            eval_fs_func("filesha256", std::slice::from_ref(&unk_str))
                .unwrap()
                .is_unknown()
        );
        assert!(
            eval_fs_func("filesha512", std::slice::from_ref(&unk_str))
                .unwrap()
                .is_unknown()
        );
        let missing = Value::new(
            Type::String,
            ValueData::String("non_existent_file_987654.txt".into()),
        );
        assert!(eval_fs_func("filemd5", std::slice::from_ref(&missing)).is_err());
        assert!(eval_fs_func("filesha1", std::slice::from_ref(&missing)).is_err());
        assert!(eval_fs_func("filesha256", std::slice::from_ref(&missing)).is_err());
        assert!(eval_fs_func("filesha512", std::slice::from_ref(&missing)).is_err());
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
    #[test]
    fn test_filesystem_error_handling() {
        assert!(eval_fs_func("file", &[]).is_err());
        assert!(
            eval_fs_func(
                "file",
                &[Value::new(
                    Type::String,
                    ValueData::String("nonexistent_path_12345.txt".into())
                )]
            )
            .is_err()
        );
        assert!(eval_fs_func("fileexists", &[]).is_err());
        assert!(
            eval_fs_func(
                "fileset",
                &[Value::new(Type::String, ValueData::String(".".into()))]
            )
            .is_err()
        );
        assert!(eval_fs_func("templatefile", &[]).is_err());
        assert!(
            eval_fs_func(
                "templatefile",
                &[
                    Value::new(
                        Type::String,
                        ValueData::String("truly_nonexistent_file_12345.txt".into())
                    ),
                    Value::new(Type::Dynamic, ValueData::Object(BTreeMap::new())),
                ]
            )
            .is_err()
        );
    }
    #[test]
    fn test_filesystem_exhaustive_coverage() {
        let temp_dir = std::env::temp_dir().join(format!("hcl_test_fs_ex_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let fs_funcs = [
            "file",
            "fileexists",
            "filebase64",
            "filemd5",
            "filesha1",
            "filesha256",
            "filesha512",
            "filebase64sha256",
            "abspath",
            "dirname",
            "basename",
            "pathexpand",
        ];
        let num_arg = Value::new(Type::Number, ValueData::Number(1.into()));
        let unk_arg = Value::unknown(Type::String);
        for name in fs_funcs {
            assert!(eval_fs_func(name, &[]).is_err());
            let unk_res = eval_fs_func(name, std::slice::from_ref(&unk_arg));
            assert_eq!(unk_res.map(|v| v.is_unknown()), Ok(true));
            assert!(eval_fs_func(name, std::slice::from_ref(&num_arg)).is_err());
        }
        let missing = Value::new(
            Type::String,
            ValueData::String("nonexistent_file_987654.txt".into()),
        );
        assert!(eval_fs_func("filebase64", std::slice::from_ref(&missing)).is_err());
        assert!(eval_fs_func("filebase64sha256", std::slice::from_ref(&missing)).is_err());
        let unk_set = eval_fs_func(
            "fileset",
            &[
                Value::unknown(Type::String),
                Value::new(Type::String, ValueData::String("*.txt".into())),
            ],
        );
        assert_eq!(unk_set.map(|v| v.is_unknown()), Ok(true));
        let unk_set_2 = eval_fs_func(
            "fileset",
            &[
                Value::new(Type::String, ValueData::String(".".into())),
                Value::unknown(Type::String),
            ],
        );
        assert_eq!(unk_set_2.map(|v| v.is_unknown()), Ok(true));
        assert!(
            eval_fs_func(
                "fileset",
                &[
                    num_arg.clone(),
                    Value::new(Type::String, ValueData::String("*.txt".into()))
                ]
            )
            .is_err()
        );
        assert!(
            eval_fs_func(
                "fileset",
                &[
                    Value::new(Type::String, ValueData::String(".".into())),
                    num_arg.clone()
                ]
            )
            .is_err()
        );
        assert!(
            eval_fs_func(
                "fileset",
                &[
                    Value::new(Type::String, ValueData::String(".".into())),
                    Value::new(Type::String, ValueData::String("[unclosed".into()))
                ]
            )
            .is_err()
        );
        assert!(
            eval_fs_func(
                "fileset",
                &[
                    Value::new(Type::String, ValueData::String(".".into())),
                    Value::new(Type::String, ValueData::String("***".into()))
                ]
            )
            .is_err()
        );
        assert!(
            eval_fs_func(
                "fileset",
                &[
                    Value::new(
                        Type::String,
                        ValueData::String("/nonexistent_base_dir_xyz_987".into())
                    ),
                    Value::new(Type::String, ValueData::String("*.txt".into()))
                ]
            )
            .is_err()
        );
        let nested_dir = temp_dir.join("suba");
        let _ = std::fs::create_dir_all(&nested_dir);
        let nested_file = nested_dir.join("fa.txt");
        let _ = std::fs::write(&nested_file, "content");
        let ignored_file = temp_dir.join("ignored.dat");
        let _ = std::fs::write(&ignored_file, "dat");
        let dir_val = Value::new(
            Type::String,
            ValueData::String(temp_dir.to_str().unwrap_or_default().to_string()),
        );
        let glob_res = eval_fs_func(
            "fileset",
            &[
                dir_val.clone(),
                Value::new(Type::String, ValueData::String("**a/f?.txt".into())),
            ],
        );
        let glob_star = eval_fs_func(
            "fileset",
            &[
                dir_val,
                Value::new(Type::String, ValueData::String("**/*.txt".into())),
            ],
        );
        let check_set = |res: &Result<Value, String>| -> usize {
            match res {
                Ok(val) => match &*val.data {
                    ValueData::Set(s) => s.len(),
                    _ => 0,
                },
                Err(_) => 0,
            }
        };
        assert!(check_set(&glob_res) > 0);
        assert!(check_set(&glob_star) > 0);
        assert_eq!(check_set(&Ok(Value::null(Type::Dynamic))), 0);
        assert_eq!(check_set(&Err("err".into())), 0);
        let abs_rel = eval_fs_func(
            "abspath",
            &[Value::new(
                Type::String,
                ValueData::String("relative/path/test.txt".into()),
            )],
        );
        assert!(abs_rel.is_ok());
        let abs_nonexistent = eval_fs_func(
            "abspath",
            &[Value::new(
                Type::String,
                ValueData::String("/nonexistent/path/xyz_12345.txt".into()),
            )],
        );
        assert!(abs_nonexistent.is_ok());
        let expand_tilde = eval_fs_func(
            "pathexpand",
            &[Value::new(Type::String, ValueData::String("~".into()))],
        );
        assert!(expand_tilde.is_ok());
        assert_eq!(resolve_home_dir(&|_| Ok("/home/user".into())), "/home/user");
        assert_eq!(
            resolve_home_dir(&|k| {
                if k == "USERPROFILE" {
                    Ok("C:\\Users\\user".into())
                } else {
                    Err(std::env::VarError::NotPresent)
                }
            }),
            "C:\\Users\\user"
        );
        assert_eq!(
            resolve_home_dir(&|_| Err(std::env::VarError::NotPresent)),
            "."
        );
        let dir_empty = eval_fs_func(
            "dirname",
            &[Value::new(
                Type::String,
                ValueData::String("file.txt".into()),
            )],
        );
        assert_eq!(
            dir_empty
                .unwrap_or(Value::null(Type::Dynamic))
                .data
                .as_ref(),
            &ValueData::String(".".into())
        );
        let base_empty = eval_fs_func(
            "basename",
            &[Value::new(Type::String, ValueData::String(String::new()))],
        );
        assert_eq!(
            base_empty
                .unwrap_or(Value::null(Type::Dynamic))
                .data
                .as_ref(),
            &ValueData::String(String::new())
        );
        let expand_home = eval_fs_func(
            "pathexpand",
            &[Value::new(Type::String, ValueData::String("~".into()))],
        );
        assert!(expand_home.is_ok());
        let expand_win = eval_fs_func(
            "pathexpand",
            &[Value::new(
                Type::String,
                ValueData::String("~\\winpath".into()),
            )],
        );
        assert!(expand_win.is_ok());
        let expand_norm = eval_fs_func(
            "pathexpand",
            &[Value::new(
                Type::String,
                ValueData::String("foo\\bar\\baz".into()),
            )],
        );
        assert_eq!(
            expand_norm
                .unwrap_or(Value::null(Type::Dynamic))
                .data
                .as_ref(),
            &ValueData::String("foo/bar/baz".into())
        );
        let tpl_unk = eval_fs_func(
            "templatefile",
            &[
                Value::unknown(Type::String),
                Value::new(Type::Dynamic, ValueData::Object(BTreeMap::new())),
            ],
        );
        assert_eq!(tpl_unk.map(|v| v.is_unknown()), Ok(true));
        let tpl_unk_2 = eval_fs_func(
            "templatefile",
            &[
                Value::new(Type::String, ValueData::String("template.hcl".into())),
                Value::unknown(Type::Dynamic),
            ],
        );
        assert_eq!(tpl_unk_2.map(|v| v.is_unknown()), Ok(true));
        assert!(
            eval_fs_func(
                "templatefile",
                &[
                    num_arg,
                    Value::new(Type::Dynamic, ValueData::Object(BTreeMap::new()))
                ]
            )
            .is_err()
        );
        assert!(
            eval_fs_func(
                "templatefile",
                &[
                    Value::new(
                        Type::String,
                        ValueData::String(nested_file.to_str().unwrap_or_default().to_string())
                    ),
                    Value::new(Type::String, ValueData::String("not an object".into()))
                ]
            )
            .is_err()
        );
        let bad_tpl_file = temp_dir.join("bad_syntax.tpl");
        let _ = std::fs::write(&bad_tpl_file, "%{ if true }");
        assert!(
            eval_fs_func(
                "templatefile",
                &[
                    Value::new(
                        Type::String,
                        ValueData::String(bad_tpl_file.to_str().unwrap_or_default().to_string())
                    ),
                    Value::new(Type::Dynamic, ValueData::Object(BTreeMap::new()))
                ]
            )
            .is_err()
        );
        let bad_eval_file = temp_dir.join("bad_eval.tpl");
        let _ = std::fs::write(&bad_eval_file, "${ undefined_var }");
        assert!(
            eval_fs_func(
                "templatefile",
                &[
                    Value::new(
                        Type::String,
                        ValueData::String(bad_eval_file.to_str().unwrap_or_default().to_string())
                    ),
                    Value::new(Type::Dynamic, ValueData::Object(BTreeMap::new()))
                ]
            )
            .is_err()
        );
        assert_eq!(unwrap_val(Err("err".into())).ty(), &Type::Dynamic);
        assert_eq!(
            resolve_current_dir(&|| Err(std::io::Error::other("mock error"))),
            PathBuf::from(".")
        );
        assert_eq!(
            resolve_current_dir(&|| Ok(PathBuf::from("/mock/dir"))),
            PathBuf::from("/mock/dir")
        );
        #[derive(Debug)]
        struct FailingDirFs;
        impl FileSystem for FailingDirFs {
            fn read(&self, _: &Path) -> Result<Vec<u8>, HclError> {
                Err(HclError::FileSystem("err".into()))
            }
            fn read_to_string(&self, _: &Path) -> Result<String, HclError> {
                Err(HclError::FileSystem("err".into()))
            }
            fn exists(&self, _: &Path) -> bool {
                true
            }
            fn is_file(&self, _: &Path) -> bool {
                false
            }
            fn is_dir(&self, p: &Path) -> bool {
                p == Path::new("sub")
            }
            fn canonicalize(&self, p: &Path) -> Result<PathBuf, HclError> {
                Ok(p.to_path_buf())
            }
            fn read_dir(&self, p: &Path) -> Result<Vec<PathBuf>, HclError> {
                if p == Path::new(".") {
                    Ok(vec![PathBuf::from("sub")])
                } else {
                    Err(HclError::FileSystem("simulated failure".into()))
                }
            }
        }
        let failing_fs = FailingDirFs;
        assert!(failing_fs.read(Path::new("x")).is_err());
        assert!(failing_fs.read_to_string(Path::new("x")).is_err());
        assert!(failing_fs.exists(Path::new("x")));
        assert!(!failing_fs.is_file(Path::new("x")));
        assert!(failing_fs.canonicalize(Path::new("x")).is_ok());
        let func = fileset_func(Arc::new(failing_fs));
        let res = (func.func)(&[
            Value::new(Type::String, ValueData::String(".".into())),
            Value::new(Type::String, ValueData::String("*".into())),
        ]);
        assert!(res.is_err());
        let mut mem_fs = crate::eval::fs::MemFileSystem::new();
        mem_fs.insert_file("sub/file.txt", "content");
        let mut regexes = Vec::new();
        for pattern in ["**", "***"] {
            if let Ok(r) = glob_to_regex(pattern) {
                regexes.push(r);
            }
        }
        let mut matches = BTreeSet::new();
        for r in &regexes {
            let res = walk_dir_recursive(
                &mem_fs,
                Path::new("sub"),
                Path::new("unrelated_base"),
                r,
                &mut matches,
            );
            assert!(res.is_ok());
            assert_eq!(matches.len(), 1);
        }
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
