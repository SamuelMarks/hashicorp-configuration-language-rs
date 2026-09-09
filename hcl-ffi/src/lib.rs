//! C Foreign Function Interface (C-ABI) for HCL (`hcl-ffi`).
//!
//! Provides a stable C-compatible API with opaque pointer handles for parsing,
//! evaluation, formatting, and diagnostics management.

#![deny(clippy::all, clippy::pedantic)]
#![deny(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
#![allow(unsafe_op_in_unsafe_fn)]

use hashicorp_configuration_language_rs::ast::structure::Body;
use hashicorp_configuration_language_rs::cst::format::format_str;
use hashicorp_configuration_language_rs::diagnostic::Diagnostics;
use hashicorp_configuration_language_rs::eval::context::Context;
use hashicorp_configuration_language_rs::eval::evaluator::Evaluator;
use hashicorp_configuration_language_rs::parse::parser::Parser;
use hashicorp_configuration_language_rs::types::json::encode_value_to_json;
use hashicorp_configuration_language_rs::types::{Type, Value, ValueData};
use std::collections::BTreeMap;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;

/// Opaque AST body handle.
#[allow(non_camel_case_types)]
pub struct hcl_body_t {
    pub(crate) inner: Body,
}

/// Opaque evaluation context handle.
#[allow(non_camel_case_types)]
pub struct hcl_context_t {
    pub(crate) inner: Context<'static>,
}

/// Opaque evaluation value handle.
#[allow(non_camel_case_types)]
pub struct hcl_value_t {
    pub(crate) inner: Value,
}

/// Opaque diagnostics handle.
#[allow(non_camel_case_types)]
pub struct hcl_diagnostics_t {
    pub(crate) inner: Diagnostics,
}

/// Creates a new, empty evaluation context with standard library functions enabled.
///
/// # Safety
/// The returned pointer must eventually be released with [`hcl_free_context`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hcl_context_new() -> *mut hcl_context_t {
    let ctx = Context::with_stdlib();
    Box::into_raw(Box::new(hcl_context_t { inner: ctx }))
}

/// Binds a named variable into an evaluation context.
///
/// # Safety
/// * `ctx` must be a valid, non-null pointer to an [`hcl_context_t`].
/// * `name` must be a valid null-terminated C string.
/// * `val` must be a valid pointer to an [`hcl_value_t`]. Its contents will be cloned.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hcl_context_set_variable(
    ctx: *mut hcl_context_t,
    name: *const c_char,
    val: *const hcl_value_t,
) -> i32 {
    if ctx.is_null() || name.is_null() || val.is_null() {
        return -1;
    }

    let Ok(c_str) = CStr::from_ptr(name).to_str() else {
        return -1;
    };

    (*ctx).inner.set_variable(c_str, (*val).inner.clone());
    0
}

/// Releases an evaluation context handle.
///
/// # Safety
/// * `ctx` must be a pointer returned by [`hcl_context_new`] or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hcl_free_context(ctx: *mut hcl_context_t) {
    if !ctx.is_null() {
        drop(Box::from_raw(ctx));
    }
}

/// Parses an HCL configuration string into an AST body.
///
/// # Arguments
/// * `input` - Null-terminated UTF-8 input string.
/// * `out_body` - Output pointer receiving the allocated [`hcl_body_t`].
/// * `out_diags` - Output pointer receiving any syntax/parse diagnostics.
///
/// # Returns
/// `0` on success, or non-zero if parsing encountered errors.
///
/// # Safety
/// * `input` must be a valid null-terminated C string.
/// * `out_body` and `out_diags` must be non-null writable pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hcl_parse_string(
    input: *const c_char,
    out_body: *mut *mut hcl_body_t,
    out_diags: *mut *mut hcl_diagnostics_t,
) -> i32 {
    if input.is_null() || out_body.is_null() || out_diags.is_null() {
        return -1;
    }

    let Ok(input_str) = CStr::from_ptr(input).to_str() else {
        return -1;
    };

    let mut parser = Parser::new(input_str);
    let body = parser.parse_body();
    let diags = parser.errors().clone();

    let has_errors = diags.has_errors();

    *out_body = Box::into_raw(Box::new(hcl_body_t { inner: body }));
    *out_diags = Box::into_raw(Box::new(hcl_diagnostics_t { inner: diags }));

    i32::from(has_errors)
}

/// Evaluates all top-level attributes in an AST body, returning an object value.
///
/// # Arguments
/// * `body` - Pointer to the parsed AST body.
/// * `ctx` - Pointer to the evaluation context.
/// * `out_diags` - Output pointer receiving evaluation diagnostics.
///
/// # Returns
/// An allocated [`hcl_value_t`] containing the evaluated result object, or null on fatal failure.
///
/// # Safety
/// * `body` and `ctx` must be valid, non-null pointers.
/// * `out_diags` must be a non-null writable pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hcl_evaluate(
    body: *const hcl_body_t,
    ctx: *mut hcl_context_t,
    out_diags: *mut *mut hcl_diagnostics_t,
) -> *mut hcl_value_t {
    if body.is_null() || ctx.is_null() || out_diags.is_null() {
        return std::ptr::null_mut();
    }

    let mut accumulated_diags = Diagnostics::new();
    let mut obj_attrs = BTreeMap::new();
    let mut attr_types = BTreeMap::new();

    for (k, attr) in &(*body).inner.attributes {
        let eval = Evaluator::new(&(*ctx).inner);
        match eval.evaluate(&attr.expr) {
            Ok((val, d)) => {
                accumulated_diags.extend(d);
                attr_types.insert(k.clone(), val.ty().clone());
                obj_attrs.insert(k.clone(), val.clone());
                (*ctx).inner.set_variable(k.clone(), val);
            }
            Err(d) => {
                accumulated_diags.extend(d);
            }
        }
    }

    *out_diags = Box::into_raw(Box::new(hcl_diagnostics_t {
        inner: accumulated_diags,
    }));

    let result_val = Value::new(Type::object(attr_types), ValueData::Object(obj_attrs));
    Box::into_raw(Box::new(hcl_value_t { inner: result_val }))
}

/// Formats an HCL string according to canonical styling guidelines.
///
/// # Arguments
/// * `input` - Input HCL source string.
/// * `out_str` - Output pointer receiving the formatted C string.
/// * `out_diags` - Output pointer receiving parse diagnostics on syntax error.
///
/// # Returns
/// `0` on success, or non-zero on failure.
///
/// # Safety
/// * `input` must be a valid null-terminated C string.
/// * `out_str` and `out_diags` must be non-null writable pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hcl_format_string(
    input: *const c_char,
    out_str: *mut *mut c_char,
    out_diags: *mut *mut hcl_diagnostics_t,
) -> i32 {
    if input.is_null() || out_str.is_null() || out_diags.is_null() {
        return -1;
    }

    let Ok(input_str) = CStr::from_ptr(input).to_str() else {
        return -1;
    };

    match format_str(input_str) {
        Ok(formatted) => {
            let c_string = CString::new(formatted).unwrap_or_default();
            *out_str = c_string.into_raw();
            *out_diags = Box::into_raw(Box::new(hcl_diagnostics_t {
                inner: Diagnostics::new(),
            }));
            0
        }
        Err(err) => {
            let mut diags = Diagnostics::new();
            diags.push(
                hashicorp_configuration_language_rs::diagnostic::Diagnostic::error(
                    "Format error",
                    err.to_string(),
                    hashicorp_configuration_language_rs::span::Span::default(),
                ),
            );
            *out_str = std::ptr::null_mut();
            *out_diags = Box::into_raw(Box::new(hcl_diagnostics_t { inner: diags }));
            1
        }
    }
}

/// Releases an AST body handle.
///
/// # Safety
/// `body` must be a pointer returned by [`hcl_parse_string`] or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hcl_free_body(body: *mut hcl_body_t) {
    if !body.is_null() {
        drop(Box::from_raw(body));
    }
}

/// Releases a value handle.
///
/// # Safety
/// `val` must be a pointer returned by [`hcl_evaluate`] or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hcl_free_value(val: *mut hcl_value_t) {
    if !val.is_null() {
        drop(Box::from_raw(val));
    }
}

/// Releases a diagnostics collection handle.
///
/// # Safety
/// `diags` must be a pointer returned by an FFI function or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hcl_free_diagnostics(diags: *mut hcl_diagnostics_t) {
    if !diags.is_null() {
        drop(Box::from_raw(diags));
    }
}

/// Releases a string allocated by this library.
///
/// # Safety
/// `str_ptr` must be a pointer returned by an FFI function or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hcl_free_string(str_ptr: *mut c_char) {
    if !str_ptr.is_null() {
        drop(CString::from_raw(str_ptr));
    }
}

/// Returns the number of diagnostics in a diagnostics handle.
///
/// # Safety
/// `diags` must be a valid, non-null pointer to an [`hcl_diagnostics_t`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hcl_diagnostics_count(diags: *const hcl_diagnostics_t) -> usize {
    if diags.is_null() {
        0
    } else {
        (*diags).inner.errors().len()
    }
}

/// Returns `true` if the diagnostics collection contains error-level items.
///
/// # Safety
/// `diags` must be a valid, non-null pointer to an [`hcl_diagnostics_t`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hcl_diagnostics_has_errors(diags: *const hcl_diagnostics_t) -> bool {
    if diags.is_null() {
        false
    } else {
        (*diags).inner.has_errors()
    }
}

/// Serializes diagnostics to a JSON string.
///
/// # Safety
/// The returned pointer must be released with [`hcl_free_string`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hcl_diagnostics_to_json(diags: *const hcl_diagnostics_t) -> *mut c_char {
    if diags.is_null() {
        return std::ptr::null_mut();
    }
    let json = (*diags).inner.to_json(None).unwrap_or_default();
    CString::new(json).map_or(std::ptr::null_mut(), CString::into_raw)
}

/// Serializes a value to its JSON string representation.
///
/// # Safety
/// The returned pointer must be released with [`hcl_free_string`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hcl_value_to_json(val: *const hcl_value_t) -> *mut c_char {
    if val.is_null() {
        return std::ptr::null_mut();
    }
    let json = encode_value_to_json(&(*val).inner)
        .ok()
        .and_then(|v| serde_json::to_string(&v).ok())
        .unwrap_or_default();
    CString::new(json).map_or(std::ptr::null_mut(), CString::into_raw)
}

/// Returns the type description of a value as a C string.
///
/// # Safety
/// The returned pointer must be released with [`hcl_free_string`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hcl_value_type(val: *const hcl_value_t) -> *mut c_char {
    if val.is_null() {
        return std::ptr::null_mut();
    }
    let ty_str = (*val).inner.ty().to_string();
    CString::new(ty_str).map_or(std::ptr::null_mut(), CString::into_raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_c_abi_parse_evaluate_format_lifecycle() {
        unsafe {
            // 1. Context creation
            let ctx = hcl_context_new();
            assert!(!ctx.is_null());

            // 2. Parse string
            let input = CString::new("port = 8080\nname = \"app\"\n").expect("cstring");
            let mut body: *mut hcl_body_t = std::ptr::null_mut();
            let mut diags: *mut hcl_diagnostics_t = std::ptr::null_mut();

            let parse_res = hcl_parse_string(input.as_ptr(), &raw mut body, &raw mut diags);
            assert_eq!(parse_res, 0);
            assert!(!body.is_null());
            assert_eq!(hcl_diagnostics_count(diags), 0);
            assert!(!hcl_diagnostics_has_errors(diags));

            // Diagnostics to JSON on clean run
            let clean_json = hcl_diagnostics_to_json(diags);
            assert!(!clean_json.is_null());
            let clean_json_str = CStr::from_ptr(clean_json).to_str().expect("utf8");
            assert!(clean_json_str.contains("[]"));
            hcl_free_string(clean_json);

            // 3. Evaluate body
            let mut eval_diags: *mut hcl_diagnostics_t = std::ptr::null_mut();
            let val = hcl_evaluate(body, ctx, &raw mut eval_diags);
            assert!(!val.is_null());
            assert!(!hcl_diagnostics_has_errors(eval_diags));

            // Inspect value type and json
            let val_type = hcl_value_type(val);
            assert!(!val_type.is_null());
            let type_str = CStr::from_ptr(val_type).to_str().expect("utf8");
            assert!(type_str.contains("object"));

            let val_json = hcl_value_to_json(val);
            assert!(!val_json.is_null());
            let json_str = CStr::from_ptr(val_json).to_str().expect("utf8");
            assert!(json_str.contains("8080"));
            assert!(json_str.contains("app"));

            // 4. Format string
            let unformatted = CString::new("a=1\n").expect("cstring");
            let mut formatted_str: *mut c_char = std::ptr::null_mut();
            let mut fmt_diags: *mut hcl_diagnostics_t = std::ptr::null_mut();
            let fmt_res = hcl_format_string(
                unformatted.as_ptr(),
                &raw mut formatted_str,
                &raw mut fmt_diags,
            );
            assert_eq!(fmt_res, 0);
            assert!(!formatted_str.is_null());
            let res_str = CStr::from_ptr(formatted_str).to_str().expect("utf8");
            assert_eq!(res_str, "a = 1\n");

            // 5. Context set variable
            let set_res =
                hcl_context_set_variable(ctx, CString::new("env").expect("cstring").as_ptr(), val);
            assert_eq!(set_res, 0);

            // 6. Cleanup memory
            hcl_free_string(val_type);
            hcl_free_string(val_json);
            hcl_free_string(formatted_str);
            hcl_free_value(val);
            hcl_free_body(body);
            hcl_free_diagnostics(diags);
            hcl_free_diagnostics(eval_diags);
            hcl_free_diagnostics(fmt_diags);
            hcl_free_context(ctx);
        }
    }

    #[test]
    fn test_c_abi_errors_and_diagnostics() {
        unsafe {
            // Parse error
            let bad_input = CString::new("{ invalid hcl").expect("cstring");
            let mut bad_body: *mut hcl_body_t = std::ptr::null_mut();
            let mut bad_diags: *mut hcl_diagnostics_t = std::ptr::null_mut();
            let parse_err =
                hcl_parse_string(bad_input.as_ptr(), &raw mut bad_body, &raw mut bad_diags);
            assert_ne!(parse_err, 0);
            assert!(hcl_diagnostics_has_errors(bad_diags));
            assert!(hcl_diagnostics_count(bad_diags) > 0);

            let json_ptr = hcl_diagnostics_to_json(bad_diags);
            assert!(!json_ptr.is_null());
            let serialized_diag_json = CStr::from_ptr(json_ptr).to_str().expect("utf8");
            assert!(serialized_diag_json.contains("severity"));
            hcl_free_string(json_ptr);
            hcl_free_body(bad_body);
            hcl_free_diagnostics(bad_diags);

            // Format error
            let mut fmt_out: *mut c_char = std::ptr::null_mut();
            let mut fmt_err_diags: *mut hcl_diagnostics_t = std::ptr::null_mut();
            let fmt_fail =
                hcl_format_string(bad_input.as_ptr(), &raw mut fmt_out, &raw mut fmt_err_diags);
            assert_ne!(fmt_fail, 0);
            assert!(fmt_out.is_null());
            assert!(hcl_diagnostics_has_errors(fmt_err_diags));
            hcl_free_diagnostics(fmt_err_diags);

            // Evaluation error
            let ctx = hcl_context_new();
            let eval_err_input = CString::new("err_attr = 1 / 0\n").expect("cstring");
            let mut eval_body: *mut hcl_body_t = std::ptr::null_mut();
            let mut p_diags: *mut hcl_diagnostics_t = std::ptr::null_mut();
            assert_eq!(
                hcl_parse_string(
                    eval_err_input.as_ptr(),
                    &raw mut eval_body,
                    &raw mut p_diags
                ),
                0
            );
            hcl_free_diagnostics(p_diags);

            let mut eval_fail_diags: *mut hcl_diagnostics_t = std::ptr::null_mut();
            let eval_val = hcl_evaluate(eval_body, ctx, &raw mut eval_fail_diags);
            assert!(!eval_val.is_null());
            assert!(hcl_diagnostics_has_errors(eval_fail_diags));
            hcl_free_value(eval_val);
            hcl_free_body(eval_body);
            hcl_free_diagnostics(eval_fail_diags);
            hcl_free_context(ctx);
        }
    }

    #[test]
    fn test_c_abi_null_and_invalid_utf8_safety() {
        unsafe {
            // Null pointers
            assert_eq!(
                hcl_parse_string(std::ptr::null(), std::ptr::null_mut(), std::ptr::null_mut()),
                -1
            );
            let mut b: *mut hcl_body_t = std::ptr::null_mut();
            let mut d: *mut hcl_diagnostics_t = std::ptr::null_mut();
            let valid = CString::new("a=1").expect("cstring");
            assert_eq!(
                hcl_parse_string(valid.as_ptr(), std::ptr::null_mut(), &raw mut d),
                -1
            );
            assert_eq!(
                hcl_parse_string(valid.as_ptr(), &raw mut b, std::ptr::null_mut()),
                -1
            );

            assert!(
                hcl_evaluate(std::ptr::null(), std::ptr::null_mut(), std::ptr::null_mut())
                    .is_null()
            );
            let ctx = hcl_context_new();
            assert!(hcl_evaluate(std::ptr::null(), ctx, &raw mut d).is_null());

            // Parse valid body and evaluate with null ctx / null diags
            assert_eq!(hcl_parse_string(valid.as_ptr(), &raw mut b, &raw mut d), 0);
            assert!(hcl_evaluate(b, std::ptr::null_mut(), &raw mut d).is_null());
            assert!(hcl_evaluate(b, ctx, std::ptr::null_mut()).is_null());
            let mut eval_ok_diags: *mut hcl_diagnostics_t = std::ptr::null_mut();
            let valid_val = hcl_evaluate(b, ctx, &raw mut eval_ok_diags);
            assert!(!valid_val.is_null());
            hcl_free_diagnostics(eval_ok_diags);

            assert_eq!(
                hcl_format_string(std::ptr::null(), std::ptr::null_mut(), std::ptr::null_mut()),
                -1
            );
            let mut s: *mut c_char = std::ptr::null_mut();
            assert_eq!(
                hcl_format_string(valid.as_ptr(), std::ptr::null_mut(), &raw mut d),
                -1
            );
            assert_eq!(
                hcl_format_string(valid.as_ptr(), &raw mut s, std::ptr::null_mut()),
                -1
            );

            assert_eq!(
                hcl_context_set_variable(std::ptr::null_mut(), std::ptr::null(), std::ptr::null()),
                -1
            );
            assert_eq!(
                hcl_context_set_variable(ctx, valid.as_ptr(), std::ptr::null()),
                -1
            );
            assert_eq!(
                hcl_context_set_variable(ctx, std::ptr::null(), valid_val),
                -1
            );
            assert_eq!(hcl_diagnostics_count(std::ptr::null()), 0);
            assert!(!hcl_diagnostics_has_errors(std::ptr::null()));
            assert!(hcl_diagnostics_to_json(std::ptr::null()).is_null());
            assert!(hcl_value_to_json(std::ptr::null()).is_null());
            assert!(hcl_value_type(std::ptr::null()).is_null());

            // Invalid UTF-8 bytes
            let bad_utf8 = [0xff_u8, 0xfe_u8, 0x00_u8];
            let bad_ptr = bad_utf8.as_ptr().cast::<c_char>();
            assert_eq!(hcl_parse_string(bad_ptr, &raw mut b, &raw mut d), -1);
            assert_eq!(hcl_format_string(bad_ptr, &raw mut s, &raw mut d), -1);
            let test_ctx = hcl_context_new();
            assert_eq!(
                hcl_context_set_variable(test_ctx, bad_ptr, std::ptr::null()),
                -1
            );
            assert_eq!(hcl_context_set_variable(test_ctx, bad_ptr, valid_val), -1);
            hcl_free_context(test_ctx);

            // Null frees do not crash
            hcl_free_body(b);
            hcl_free_value(valid_val);
            hcl_free_diagnostics(d);
            hcl_free_context(ctx);
            hcl_free_body(std::ptr::null_mut());
            hcl_free_value(std::ptr::null_mut());
            hcl_free_diagnostics(std::ptr::null_mut());
            hcl_free_string(std::ptr::null_mut());
            hcl_free_context(std::ptr::null_mut());
        }
    }
}
