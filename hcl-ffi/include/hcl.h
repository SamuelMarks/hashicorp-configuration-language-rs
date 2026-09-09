#ifndef HCL_RS_H
#define HCL_RS_H

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

/**
 * Opaque AST body handle.
 */
typedef struct hcl_body_t hcl_body_t;

/**
 * Opaque evaluation context handle.
 */
typedef struct hcl_context_t hcl_context_t;

/**
 * Opaque diagnostics handle.
 */
typedef struct hcl_diagnostics_t hcl_diagnostics_t;

/**
 * Opaque evaluation value handle.
 */
typedef struct hcl_value_t hcl_value_t;

/**
 * Creates a new, empty evaluation context with standard library functions enabled.
 *
 * # Safety
 * The returned pointer must eventually be released with [`hcl_free_context`].
 */
struct hcl_context_t *hcl_context_new(void);

/**
 * Binds a named variable into an evaluation context.
 *
 * # Safety
 * * `ctx` must be a valid, non-null pointer to an [`hcl_context_t`].
 * * `name` must be a valid null-terminated C string.
 * * `val` must be a valid pointer to an [`hcl_value_t`]. Its contents will be cloned.
 */
int32_t hcl_context_set_variable(struct hcl_context_t *ctx,
                                 const char *name,
                                 const struct hcl_value_t *val);

/**
 * Releases an evaluation context handle.
 *
 * # Safety
 * * `ctx` must be a pointer returned by [`hcl_context_new`] or null.
 */
void hcl_free_context(struct hcl_context_t *ctx);

/**
 * Parses an HCL configuration string into an AST body.
 *
 * # Arguments
 * * `input` - Null-terminated UTF-8 input string.
 * * `out_body` - Output pointer receiving the allocated [`hcl_body_t`].
 * * `out_diags` - Output pointer receiving any syntax/parse diagnostics.
 *
 * # Returns
 * `0` on success, or non-zero if parsing encountered errors.
 *
 * # Safety
 * * `input` must be a valid null-terminated C string.
 * * `out_body` and `out_diags` must be non-null writable pointers.
 */
int32_t hcl_parse_string(const char *input,
                         struct hcl_body_t **out_body,
                         struct hcl_diagnostics_t **out_diags);

/**
 * Evaluates all top-level attributes in an AST body, returning an object value.
 *
 * # Arguments
 * * `body` - Pointer to the parsed AST body.
 * * `ctx` - Pointer to the evaluation context.
 * * `out_diags` - Output pointer receiving evaluation diagnostics.
 *
 * # Returns
 * An allocated [`hcl_value_t`] containing the evaluated result object, or null on fatal failure.
 *
 * # Safety
 * * `body` and `ctx` must be valid, non-null pointers.
 * * `out_diags` must be a non-null writable pointer.
 */
struct hcl_value_t *hcl_evaluate(const struct hcl_body_t *body,
                                 struct hcl_context_t *ctx,
                                 struct hcl_diagnostics_t **out_diags);

/**
 * Formats an HCL string according to canonical styling guidelines.
 *
 * # Arguments
 * * `input` - Input HCL source string.
 * * `out_str` - Output pointer receiving the formatted C string.
 * * `out_diags` - Output pointer receiving parse diagnostics on syntax error.
 *
 * # Returns
 * `0` on success, or non-zero on failure.
 *
 * # Safety
 * * `input` must be a valid null-terminated C string.
 * * `out_str` and `out_diags` must be non-null writable pointers.
 */
int32_t hcl_format_string(const char *input, char **out_str, struct hcl_diagnostics_t **out_diags);

/**
 * Releases an AST body handle.
 *
 * # Safety
 * `body` must be a pointer returned by [`hcl_parse_string`] or null.
 */
void hcl_free_body(struct hcl_body_t *body);

/**
 * Releases a value handle.
 *
 * # Safety
 * `val` must be a pointer returned by [`hcl_evaluate`] or null.
 */
void hcl_free_value(struct hcl_value_t *val);

/**
 * Releases a diagnostics collection handle.
 *
 * # Safety
 * `diags` must be a pointer returned by an FFI function or null.
 */
void hcl_free_diagnostics(struct hcl_diagnostics_t *diags);

/**
 * Releases a string allocated by this library.
 *
 * # Safety
 * `str_ptr` must be a pointer returned by an FFI function or null.
 */
void hcl_free_string(char *str_ptr);

/**
 * Returns the number of diagnostics in a diagnostics handle.
 *
 * # Safety
 * `diags` must be a valid, non-null pointer to an [`hcl_diagnostics_t`].
 */
uintptr_t hcl_diagnostics_count(const struct hcl_diagnostics_t *diags);

/**
 * Returns `true` if the diagnostics collection contains error-level items.
 *
 * # Safety
 * `diags` must be a valid, non-null pointer to an [`hcl_diagnostics_t`].
 */
bool hcl_diagnostics_has_errors(const struct hcl_diagnostics_t *diags);

/**
 * Serializes diagnostics to a JSON string.
 *
 * # Safety
 * The returned pointer must be released with [`hcl_free_string`].
 */
char *hcl_diagnostics_to_json(const struct hcl_diagnostics_t *diags);

/**
 * Serializes a value to its JSON string representation.
 *
 * # Safety
 * The returned pointer must be released with [`hcl_free_string`].
 */
char *hcl_value_to_json(const struct hcl_value_t *val);

/**
 * Returns the type description of a value as a C string.
 *
 * # Safety
 * The returned pointer must be released with [`hcl_free_string`].
 */
char *hcl_value_type(const struct hcl_value_t *val);

#endif  /* HCL_RS_H */
