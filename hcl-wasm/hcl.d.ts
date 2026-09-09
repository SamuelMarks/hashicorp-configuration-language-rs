/**
 * TypeScript definitions for HCL WebAssembly bindings (`hcl-wasm`).
 */

/**
 * Parses an HCL source string into a JavaScript-compatible JSON object representation.
 *
 * @param source - The HCL configuration source string.
 * @returns An object mapping attribute names to their parsed expression representations.
 * @throws An error if the source contains syntax or lexical errors.
 */
export function parse_hcl(source: string): Record<string, any>;

/**
 * Formats an HCL source string according to canonical styling guidelines.
 *
 * @param source - The unformatted HCL source string.
 * @returns The canonically formatted HCL string.
 * @throws An error if the source cannot be parsed into a CST.
 */
export function format_hcl(source: string): string;

/**
 * Evaluates an HCL source string using variable bindings provided as a JSON string.
 *
 * @param source - The HCL configuration source string.
 * @param context_json - A JSON-encoded object of variables (e.g. `{"env": "prod", "count": 5}`).
 * @returns An object mapping attribute names to their evaluated values.
 * @throws An error if evaluation or parsing fails.
 */
export function evaluate_hcl(source: string, context_json: string): Record<string, any>;
