//! Procedural macros for the HCL-RS crate.
//!
//! Provides `DecodeBody` derive macro for structural decoding of HCL configuration.

#![deny(clippy::all, clippy::pedantic, clippy::nursery)]
#![deny(missing_docs)]
#![deny(rustdoc::missing_crate_level_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Field, Fields, Type, parse_macro_input};

/// Bitflag constants for `#[hcl(...)]` field attributes.
mod attr_flags {
    /// Field is marked with `#[hcl(label)]`.
    pub const LABEL: u16 = 1 << 0;
    /// Field is marked with `#[hcl(block)]`.
    pub const BLOCK: u16 = 1 << 1;
    /// Field is marked with `#[hcl(attr)]`.
    pub const ATTR: u16 = 1 << 2;
    /// Field is marked with `#[hcl(optional)]`.
    pub const OPTIONAL: u16 = 1 << 3;
    /// Field is marked with `#[hcl(default)]`.
    pub const DEFAULT: u16 = 1 << 4;
    /// Field is marked with `#[hcl(remain)]`.
    pub const REMAIN: u16 = 1 << 5;
    /// Field is marked with `#[hcl(remain_attrs)]`.
    pub const REMAIN_ATTRS: u16 = 1 << 6;
    /// Field is marked with `#[hcl(remain_blocks)]`.
    pub const REMAIN_BLOCKS: u16 = 1 << 7;
    /// Field is marked with `#[hcl(expr)]`.
    pub const EXPR: u16 = 1 << 8;
    /// Field is marked with `#[hcl(flatten)]` or `#[hcl(squash)]`.
    pub const FLATTEN: u16 = 1 << 9;
    /// Field is marked with `#[hcl(body)]`.
    pub const BODY: u16 = 1 << 10;
    /// Field is marked with `#[hcl(custom)]`.
    pub const CUSTOM: u16 = 1 << 11;
}

/// Internal parsed representation of `#[hcl(...)]` field attributes.
#[derive(Default)]
struct HclFieldAttrs {
    /// Bitset of boolean attribute flags (`attr_flags::*`).
    flags: u16,
    /// Custom name override if specified via `#[hcl(name = "...")]`.
    custom_name: Option<String>,
    /// Custom decoding function path via `#[hcl(with = "...")]`.
    custom_decoder: Option<syn::Path>,
    /// Default fallback HCL expression via `#[hcl(default_expr = "...")]`.
    default_expr: Option<String>,
    /// Default factory function path via `#[hcl(default_fn = "...")]`.
    default_fn: Option<syn::Path>,
}

impl HclFieldAttrs {
    /// Checks whether the given attribute flag bit is set.
    const fn has(&self, flag: u16) -> bool {
        (self.flags & flag) != 0
    }

    /// Sets the given attribute flag bit.
    const fn set(&mut self, flag: u16) {
        self.flags |= flag;
    }

    /// Returns true if field is marked with `#[hcl(label)]`.
    const fn is_label(&self) -> bool {
        self.has(attr_flags::LABEL)
    }

    /// Returns true if field is marked with `#[hcl(block)]`.
    const fn is_block(&self) -> bool {
        self.has(attr_flags::BLOCK)
    }

    /// Returns true if field is marked with `#[hcl(attr)]`.
    const fn is_attr(&self) -> bool {
        self.has(attr_flags::ATTR)
    }

    /// Returns true if field is marked with `#[hcl(optional)]`.
    const fn is_optional(&self) -> bool {
        self.has(attr_flags::OPTIONAL)
    }

    /// Returns true if field is marked with `#[hcl(default)]`.
    const fn is_default(&self) -> bool {
        self.has(attr_flags::DEFAULT)
    }

    /// Returns true if field is marked with `#[hcl(remain)]`.
    const fn is_remain(&self) -> bool {
        self.has(attr_flags::REMAIN)
    }

    /// Returns true if field is marked with `#[hcl(remain_attrs)]`.
    const fn is_remain_attrs(&self) -> bool {
        self.has(attr_flags::REMAIN_ATTRS)
    }

    /// Returns true if field is marked with `#[hcl(remain_blocks)]`.
    const fn is_remain_blocks(&self) -> bool {
        self.has(attr_flags::REMAIN_BLOCKS)
    }

    /// Returns true if field is marked with `#[hcl(expr)]`.
    const fn is_expr(&self) -> bool {
        self.has(attr_flags::EXPR)
    }

    /// Returns true if field is marked with `#[hcl(flatten)]` or `#[hcl(squash)]`.
    const fn is_flatten(&self) -> bool {
        self.has(attr_flags::FLATTEN)
    }

    /// Returns true if field is marked with `#[hcl(body)]`.
    const fn is_body(&self) -> bool {
        self.has(attr_flags::BODY)
    }

    /// Returns true if field is marked with `#[hcl(custom)]`.
    const fn is_custom(&self) -> bool {
        self.has(attr_flags::CUSTOM)
    }
}

/// Extracts named fields paired with their identifiers from a `FieldsNamed` collection.
///
/// # Arguments
/// * `fields` - The named fields syntax node.
///
/// # Returns
/// An iterator yielding `(&Field, &syn::Ident)` pairs.
fn iter_named_fields(fields: &syn::FieldsNamed) -> impl Iterator<Item = (&Field, &syn::Ident)> {
    fields
        .named
        .iter()
        .flat_map(|field| field.ident.iter().map(move |ident| (field, ident)))
}

/// Returns the final path segment of a `syn::Type` if it is a `Type::Path`.
///
/// # Arguments
/// * `ty` - The syntax tree type to inspect.
///
/// # Returns
/// `Some(&syn::PathSegment)` if `ty` is a `Type::Path` with at least one segment, otherwise `None`.
fn last_path_segment(ty: &Type) -> Option<&syn::PathSegment> {
    match ty {
        Type::Path(p) => p.path.segments.last(),
        _ => None,
    }
}

/// Checks whether the final segment of a `syn::Type` matches `ident`.
///
/// # Arguments
/// * `ty` - The syntax tree type to inspect.
/// * `ident` - The expected identifier string.
///
/// # Returns
/// `true` if `ty` is a `Type::Path` whose last segment matches `ident`.
fn is_type_ident(ty: &Type, ident: &str) -> bool {
    last_path_segment(ty).is_some_and(|s| s.ident == ident)
}

/// Derives the `DecodeBody` trait for a struct.
///
/// This macro inspects the struct's fields to automatically map HCL attributes
/// and blocks onto native Rust types.
#[cfg(not(tarpaulin_include))]
#[proc_macro_derive(DecodeBody, attributes(hcl))]
pub fn derive_decode_body(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_derive_decode_body(input).into()
}

/// Generates code to populate `#[hcl(remain)]` fields.
fn generate_remain_assign(
    remain_field: Option<(syn::Ident, syn::Type)>,
) -> proc_macro2::TokenStream {
    let Some((fname, ty)) = remain_field else {
        return quote! {};
    };
    let is_hashmap = is_type_ident(&ty, "HashMap");
    let is_btreemap = is_type_ident(&ty, "BTreeMap");

    if is_hashmap {
        quote! {
            let #fname = {
                let mut map = std::collections::HashMap::new();
                if let Some(r) = remain_body.take() {
                    for (k, attr) in r.attributes {
                        match crate::eval::evaluator::Evaluator::new(ctx).evaluate(&attr.expr) {
                            Ok((v, d)) => {
                                diags.extend(d);
                                map.insert(k, v);
                            }
                            Err(d) => diags.extend(d),
                        }
                    }
                }
                map
            };
        }
    } else if is_btreemap {
        quote! {
            let #fname = {
                let mut map = std::collections::BTreeMap::new();
                if let Some(r) = remain_body.take() {
                    for (k, attr) in r.attributes {
                        match crate::eval::evaluator::Evaluator::new(ctx).evaluate(&attr.expr) {
                            Ok((v, d)) => {
                                diags.extend(d);
                                map.insert(k, v);
                            }
                            Err(d) => diags.extend(d),
                        }
                    }
                }
                map
            };
        }
    } else {
        quote! {
            let #fname = match remain_body.take() {
                Some(b) => b,
                None => crate::ast::structure::Body::new(crate::span::Span::new(0, 0, 0, 0, 0, 0)),
            };
        }
    }
}

/// Generates code to populate `#[hcl(remain_attrs)]` fields.
fn generate_remain_attrs_assign(
    remain_attrs_field: Option<(syn::Ident, syn::Type)>,
) -> proc_macro2::TokenStream {
    let Some((fname, ty)) = remain_attrs_field else {
        return quote! {};
    };
    if is_type_ident(&ty, "BTreeMap") {
        quote! {
            let #fname = {
                let mut map = std::collections::BTreeMap::new();
                if let Some(r) = remain_body.as_mut() {
                    for (k, attr) in std::mem::take(&mut r.attributes) {
                        match crate::eval::evaluator::Evaluator::new(ctx).evaluate(&attr.expr) {
                            Ok((v, d)) => {
                                diags.extend(d);
                                map.insert(k, v);
                            }
                            Err(d) => diags.extend(d),
                        }
                    }
                }
                map
            };
        }
    } else {
        quote! {
            let #fname = {
                let mut map = std::collections::HashMap::new();
                if let Some(r) = remain_body.as_mut() {
                    for (k, attr) in std::mem::take(&mut r.attributes) {
                        match crate::eval::evaluator::Evaluator::new(ctx).evaluate(&attr.expr) {
                            Ok((v, d)) => {
                                diags.extend(d);
                                map.insert(k, v);
                            }
                            Err(d) => diags.extend(d),
                        }
                    }
                }
                map
            };
        }
    }
}

/// State accumulated while expanding `DecodeBody` for a struct's fields.
#[derive(Default)]
struct DecodeBodyAcc {
    decode_fields: Vec<proc_macro2::TokenStream>,
    field_names: Vec<syn::Ident>,
    slot_extracts: Vec<proc_macro2::TokenStream>,
    remain_field: Option<(syn::Ident, syn::Type)>,
    remain_attrs_field: Option<(syn::Ident, syn::Type)>,
    remain_blocks_field: Option<(syn::Ident, syn::Type)>,
    body_field: Option<(syn::Ident, syn::Type)>,
    label_count: usize,
    errors: Vec<proc_macro2::TokenStream>,
    parent_attrs: Vec<String>,
    parent_blocks: Vec<String>,
    declared_names: std::collections::HashMap<String, proc_macro2::Span>,
    has_flatten: bool,
}

/// Expands the `DecodeBody` derive macro for the given AST input.
pub(crate) fn expand_derive_decode_body(input: DeriveInput) -> proc_macro2::TokenStream {
    let name = input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let mut acc = DecodeBodyAcc::default();

    if let Data::Struct(syn::DataStruct {
        fields: Fields::Named(ref fields),
        ..
    }) = input.data
    {
        for (field, fname) in iter_named_fields(fields) {
            process_field(field, fname, &mut acc);
        }
    }

    if !acc.errors.is_empty() {
        let errors = acc.errors;
        return quote! {
            #(#errors)*
        };
    }

    let remain_setup = quote! {
        let mut remain_body: Option<crate::ast::structure::Body> = Some(body.clone());
    };

    let parent_attrs = &acc.parent_attrs;
    let parent_blocks = &acc.parent_blocks;
    let collision_setup = if acc.has_flatten {
        quote! {
            let mut seen_attrs = std::collections::HashSet::<String>::new();
            let mut seen_blocks = std::collections::HashSet::<String>::new();
            #(
                seen_attrs.insert(#parent_attrs.to_string());
            )*
            #(
                seen_blocks.insert(#parent_blocks.to_string());
            )*
        }
    } else {
        quote! {}
    };

    let remain_assign = generate_remain_assign(acc.remain_field);
    let remain_attrs_assign = generate_remain_attrs_assign(acc.remain_attrs_field);

    let remain_blocks_assign = if let Some((fname, _)) = acc.remain_blocks_field {
        quote! {
            let #fname = if let Some(r) = remain_body.as_mut() {
                std::mem::take(&mut r.blocks)
            } else {
                Vec::new()
            };
        }
    } else {
        quote! {}
    };

    let label_count = acc.label_count;
    let label_validation = quote! {
        if labels.len() != #label_count {
            diags.push(crate::diagnostic::Diagnostic::error(
                "Invalid Label Count",
                format!("Expected {} block label(s), got {}", #label_count, labels.len()),
                body.span.clone(),
            ));
        }
    };

    let decode_fields = &acc.decode_fields;
    let slot_extracts = &acc.slot_extracts;
    let field_names = &acc.field_names;

    quote! {
        impl #impl_generics crate::decode::DecodeBody for #name #ty_generics #where_clause {
            fn decode_body(body: &crate::ast::structure::Body, labels: &[String], ctx: &mut crate::eval::context::Context) -> Result<Self, crate::diagnostic::Diagnostics> {
                let mut diags = crate::diagnostic::Diagnostics::new();
                #label_validation
                #remain_setup
                #collision_setup
                #(#decode_fields)*
                #remain_assign
                #remain_attrs_assign
                #remain_blocks_assign

                if diags.has_errors() {
                    Err(diags)
                } else {
                    #(#slot_extracts)*
                    Ok(Self {
                        #(#field_names),*
                    })
                }
            }
        }
    }
}

/// Registers a declared attribute or block name, pushing a compile error on duplicates.
fn register_declared_name(
    field: &Field,
    fname: &syn::Ident,
    name_str: &str,
    declared_names: &mut std::collections::HashMap<String, proc_macro2::Span>,
    parent_list: &mut Vec<String>,
    errors: &mut Vec<proc_macro2::TokenStream>,
) {
    if declared_names.contains_key(name_str) {
        errors.push(
            syn::Error::new_spanned(
                field,
                format!("duplicate attribute/block name '{name_str}' declared on struct"),
            )
            .to_compile_error(),
        );
    } else {
        declared_names.insert(name_str.to_string(), fname.span());
        parent_list.push(name_str.to_string());
    }
}

impl DecodeBodyAcc {
    /// Registers a declared block name on the accumulator.
    fn register_block_name(&mut self, field: &Field, fname: &syn::Ident, name_str: &str) {
        register_declared_name(
            field,
            fname,
            name_str,
            &mut self.declared_names,
            &mut self.parent_blocks,
            &mut self.errors,
        );
    }

    /// Registers a declared attribute name on the accumulator.
    fn register_attr_name(&mut self, field: &Field, fname: &syn::Ident, name_str: &str) {
        register_declared_name(
            field,
            fname,
            name_str,
            &mut self.declared_names,
            &mut self.parent_attrs,
            &mut self.errors,
        );
    }
}

/// Registers a single-occurrence special field (such as `body`, `remain`, `remain_attrs`, or `remain_blocks`).
fn set_unique_field_slot(
    slot: &mut Option<(syn::Ident, syn::Type)>,
    field: &Field,
    fname: &syn::Ident,
    error_msg: &str,
    errors: &mut Vec<proc_macro2::TokenStream>,
) {
    if slot.is_some() {
        errors.push(syn::Error::new_spanned(field, error_msg).to_compile_error());
    } else {
        *slot = Some((fname.clone(), field.ty.clone()));
    }
}

/// Dispatches decoding logic for a single struct field based on its attributes.
fn process_field(field: &Field, fname: &syn::Ident, acc: &mut DecodeBodyAcc) {
    acc.field_names.push(fname.clone());

    let attrs = parse_hcl_field_attrs(field, &mut acc.errors);
    let name_str = attrs
        .custom_name
        .clone()
        .unwrap_or_else(|| fname.to_string());

    if attrs.is_label() {
        handle_label_field(
            field,
            fname,
            &mut acc.label_count,
            &mut acc.decode_fields,
            &mut acc.errors,
        );
    } else if attrs.is_body() {
        set_unique_field_slot(
            &mut acc.body_field,
            field,
            fname,
            "hcl(body) can only be applied to a single field",
            &mut acc.errors,
        );
        if is_type_ident(&field.ty, "Option") {
            acc.decode_fields.push(quote! {
                let #fname = Some(body.clone());
            });
        } else {
            acc.decode_fields.push(quote! {
                let #fname = body.clone();
            });
        }
    } else if attrs.is_flatten() {
        acc.has_flatten = true;
        handle_flatten_field(field, fname, &mut acc.decode_fields, &mut acc.slot_extracts);
    } else if attrs.is_block() {
        acc.register_block_name(field, fname, &name_str);
        handle_block_field(
            field,
            fname,
            &name_str,
            &mut acc.decode_fields,
            &mut acc.slot_extracts,
        );
    } else if attrs.is_expr() {
        acc.register_attr_name(field, fname, &name_str);
        handle_expr_field(
            field,
            fname,
            &name_str,
            &mut acc.decode_fields,
            &mut acc.slot_extracts,
        );
    } else if attrs.is_remain() {
        set_unique_field_slot(
            &mut acc.remain_field,
            field,
            fname,
            "hcl(remain) can only be applied to a single field",
            &mut acc.errors,
        );
    } else if attrs.is_remain_attrs() {
        set_unique_field_slot(
            &mut acc.remain_attrs_field,
            field,
            fname,
            "hcl(remain_attrs) can only be applied to a single field",
            &mut acc.errors,
        );
    } else if attrs.is_remain_blocks() {
        set_unique_field_slot(
            &mut acc.remain_blocks_field,
            field,
            fname,
            "hcl(remain_blocks) can only be applied to a single field",
            &mut acc.errors,
        );
    } else {
        acc.register_attr_name(field, fname, &name_str);
        handle_attribute_field(
            field,
            fname,
            &name_str,
            &attrs,
            &mut acc.decode_fields,
            &mut acc.slot_extracts,
        );
    }
}

/// Handles decoding of a flattened/squashed child struct field (`#[hcl(flatten)]` / `#[hcl(squash)]`).
///
/// Recursively decodes the child struct from the current body scope, performs runtime
/// collision detection against attributes and blocks already declared on the parent or
/// preceding flattened fields, and removes child schema attributes and blocks from `remain_body`.
///
/// # Arguments
/// * `field` - The struct field syntax AST node.
/// * `fname` - Identifier of the field.
/// * `decode_fields` - Accumulator for field decoding statements.
/// * `slot_extracts` - Accumulator for field value extraction statements.
fn handle_flatten_field(
    field: &Field,
    fname: &syn::Ident,
    decode_fields: &mut Vec<proc_macro2::TokenStream>,
    slot_extracts: &mut Vec<proc_macro2::TokenStream>,
) {
    let ty = &field.ty;
    let is_option = is_type_ident(ty, "Option");
    let inner_ty = extract_generic_inner_type(ty);
    let slot_ident = syn::Ident::new(&format!("__slot_{fname}"), proc_macro2::Span::call_site());

    if is_option {
        decode_fields.push(quote! {
            let #slot_ident: Option<#ty> = {
                let child_schema = <#inner_ty as crate::ast::schema::ImpliedBodySchema>::implied_body_schema();
                for attr_name in child_schema.attributes.keys() {
                    if !seen_attrs.insert(attr_name.clone()) {
                        diags.push(crate::diagnostic::Diagnostic::error(
                            "Duplicate Attribute",
                            format!("Conflicting attribute '{}' between parent and flattened struct", attr_name),
                            body.span.clone(),
                        ));
                    }
                }
                for block_type in child_schema.blocks.keys() {
                    if !seen_blocks.insert(block_type.clone()) {
                        diags.push(crate::diagnostic::Diagnostic::error(
                            "Duplicate Block",
                            format!("Conflicting block '{}' between parent and flattened struct", block_type),
                            body.span.clone(),
                        ));
                    }
                }

                let has_any_attr = child_schema.attributes.keys().any(|k| body.attributes.contains_key(k));
                let has_any_block = child_schema.blocks.keys().any(|k| body.blocks.iter().any(|b| &b.block_type == k));

                if has_any_attr || has_any_block {
                    if let Some(r) = remain_body.as_mut() {
                        for attr_name in child_schema.attributes.keys() {
                            r.attributes.remove(attr_name);
                        }
                        for block_type in child_schema.blocks.keys() {
                            r.blocks.retain(|b| &b.block_type != block_type);
                        }
                    }
                    match <#inner_ty as crate::decode::DecodeBody>::decode_body(body, labels, ctx) {
                        Ok(child_val) => Some(Some(child_val)),
                        Err(d) => {
                            diags.extend(d);
                            Some(None)
                        }
                    }
                } else {
                    Some(None)
                }
            };
        });
    } else {
        decode_fields.push(quote! {
            let #slot_ident: Option<#ty> = {
                let child_schema = <#inner_ty as crate::ast::schema::ImpliedBodySchema>::implied_body_schema();
                for attr_name in child_schema.attributes.keys() {
                    if !seen_attrs.insert(attr_name.clone()) {
                        diags.push(crate::diagnostic::Diagnostic::error(
                            "Duplicate Attribute",
                            format!("Conflicting attribute '{}' between parent and flattened struct", attr_name),
                            body.span.clone(),
                        ));
                    }
                }
                for block_type in child_schema.blocks.keys() {
                    if !seen_blocks.insert(block_type.clone()) {
                        diags.push(crate::diagnostic::Diagnostic::error(
                            "Duplicate Block",
                            format!("Conflicting block '{}' between parent and flattened struct", block_type),
                            body.span.clone(),
                        ));
                    }
                }
                if let Some(r) = remain_body.as_mut() {
                    for attr_name in child_schema.attributes.keys() {
                        r.attributes.remove(attr_name);
                    }
                    for block_type in child_schema.blocks.keys() {
                        r.blocks.retain(|b| &b.block_type != block_type);
                    }
                }
                match <#inner_ty as crate::decode::DecodeBody>::decode_body(body, labels, ctx) {
                    Ok(child_val) => Some(child_val),
                    Err(d) => {
                        diags.extend(d);
                        None
                    }
                }
            };
        });
    }

    slot_extracts.push(quote! {
        let #fname = match #slot_ident {
            Some(v) => v,
            None => {
                return Err(diags);
            }
        };
    });
}

/// Handles decoding of an AST expression field (`#[hcl(expr)]`).
///
/// # Arguments
/// * `field` - The struct field syntax AST node.
/// * `fname` - Identifier of the field.
/// * `name_str` - Attribute key name in the body.
/// * `decode_fields` - Accumulator for field decoding statements.
/// * `slot_extracts` - Accumulator for field value extraction statements.
fn handle_expr_field(
    field: &Field,
    fname: &syn::Ident,
    name_str: &str,
    decode_fields: &mut Vec<proc_macro2::TokenStream>,
    slot_extracts: &mut Vec<proc_macro2::TokenStream>,
) {
    let ty = &field.ty;
    let is_option = is_type_ident(ty, "Option");
    let is_arc = is_type_ident(extract_generic_inner_type(ty), "Arc");

    if is_option {
        if is_arc {
            decode_fields.push(quote! {
                let #fname = if let Some(attr) = body.attributes.get(#name_str) {
                    if let Some(r) = remain_body.as_mut() {
                        r.attributes.remove(#name_str);
                    }
                    Some(std::sync::Arc::new(attr.expr.clone()))
                } else {
                    None
                };
            });
        } else {
            decode_fields.push(quote! {
                let #fname = if let Some(attr) = body.attributes.get(#name_str) {
                    if let Some(r) = remain_body.as_mut() {
                        r.attributes.remove(#name_str);
                    }
                    Some(attr.expr.clone())
                } else {
                    None
                };
            });
        }
    } else {
        let slot_ident =
            syn::Ident::new(&format!("__slot_{fname}"), proc_macro2::Span::call_site());
        let val_expr = if is_arc {
            quote! { std::sync::Arc::new(attr.expr.clone()) }
        } else {
            quote! { attr.expr.clone() }
        };
        decode_fields.push(quote! {
            let #slot_ident: Option<#ty> = if let Some(attr) = body.attributes.get(#name_str) {
                if let Some(r) = remain_body.as_mut() {
                    r.attributes.remove(#name_str);
                }
                Some(#val_expr)
            } else {
                diags.push(crate::diagnostic::Diagnostic::error(
                    "Missing Attribute",
                    format!("Attribute '{}' is missing", #name_str),
                    body.span.clone(),
                ));
                None
            };
        });
        slot_extracts.push(quote! {
            let #fname = match #slot_ident {
                Some(v) => v,
                None => return Err(diags),
            };
        });
    }
}

/// Handles decoding of a block label field.
///
/// # Arguments
/// * `field` - The struct field syntax AST node.
/// * `fname` - Identifier of the field.
/// * `label_count` - Running count of labels encountered.
/// * `decode_fields` - Accumulator for field decoding statements.
/// * `errors` - Accumulator for compile-time errors.
fn handle_label_field(
    field: &Field,
    fname: &syn::Ident,
    label_count: &mut usize,
    decode_fields: &mut Vec<proc_macro2::TokenStream>,
    errors: &mut Vec<proc_macro2::TokenStream>,
) {
    let fname_str = fname.to_string();
    let is_string = is_type_ident(&field.ty, "String");
    if !is_string {
        errors.push(
            syn::Error::new_spanned(&field.ty, "hcl(label) field must be of type String")
                .to_compile_error(),
        );
    }

    let l_idx: usize = *label_count;
    *label_count += 1;
    decode_fields.push(quote! {
        let #fname = if #l_idx < labels.len() {
            labels[#l_idx].clone()
        } else {
            diags.push(crate::diagnostic::Diagnostic::error(
                "Missing Label",
                format!("Expected label for '{}'", #fname_str),
                body.span.clone(),
            ));
            String::new()
        };
    });
}

/// Structural classification of a field representing an HCL block.
#[derive(Debug, Clone, PartialEq, Eq)]
enum BlockTypeKind {
    /// A single mandatory block: `T`.
    Single,
    /// An optional block: `Option<T>`.
    Option,
    /// An array of blocks: `Vec<T>`.
    Vec,
    /// A map of blocks indexed by block label: `HashMap<String, ...>` or `BTreeMap<String, ...>`.
    Map {
        /// True if map is `BTreeMap`, false if `HashMap`.
        is_btreemap: bool,
        /// Inner value structure (can be another `Map`, `Vec`, or `Single`).
        inner: Box<BlockTypeKind>,
    },
}

/// Inspects a struct field's type to determine whether it represents a single block,
/// an optional block, a block list, or a label-keyed block map.
///
/// # Arguments
/// * `ty` - The syn syntax tree type of the field.
fn inspect_block_type(ty: &Type) -> BlockTypeKind {
    if let Some(segment) = last_path_segment(ty) {
        let ident_str = segment.ident.to_string();
        if ident_str == "Vec" {
            return BlockTypeKind::Vec;
        }
        if ident_str == "Option" {
            return BlockTypeKind::Option;
        }
        if ident_str == "HashMap" || ident_str == "BTreeMap" {
            if let syn::PathArguments::AngleBracketed(ref args) = segment.arguments {
                let is_btreemap = ident_str == "BTreeMap";
                let mut type_args = args.args.iter().filter_map(|arg| match arg {
                    syn::GenericArgument::Type(t) => Some(t),
                    _ => None,
                });
                let _key_ty = type_args.next();
                if let Some(val_ty) = type_args.next() {
                    return BlockTypeKind::Map {
                        is_btreemap,
                        inner: Box::new(inspect_block_type(val_ty)),
                    };
                }
            }
        }
    }
    BlockTypeKind::Single
}

/// Returns the nesting depth and terminal kind of a [`BlockTypeKind::Map`].
///
/// # Arguments
/// * `kind` - The block type kind to inspect.
fn map_depth_and_terminal(kind: &BlockTypeKind) -> (usize, BlockTypeKind) {
    match kind {
        BlockTypeKind::Map { inner, .. } => {
            let (depth, term) = map_depth_and_terminal(inner);
            (depth + 1, term)
        }
        other => (0, other.clone()),
    }
}

/// Generates decoding statements for a label-keyed block map field.
fn handle_map_block_field(
    field: &Field,
    fname: &syn::Ident,
    name_str: &str,
    block_kind: &BlockTypeKind,
    decode_fields: &mut Vec<proc_macro2::TokenStream>,
) {
    let (depth, terminal) = map_depth_and_terminal(block_kind);
    let label_binds: Vec<proc_macro2::TokenStream> = (0..depth)
        .map(|i| {
            let lbl_ident = syn::Ident::new(&format!("__lbl_{i}"), proc_macro2::Span::call_site());
            quote! {
                let #lbl_ident = block.labels[#i].clone();
            }
        })
        .collect();

    let mut entry_chain = quote! { #fname };
    for i in 0..(depth - 1) {
        let lbl_ident = syn::Ident::new(&format!("__lbl_{i}"), proc_macro2::Span::call_site());
        entry_chain = quote! { #entry_chain.entry(#lbl_ident).or_default() };
    }
    let last_ident = syn::Ident::new(
        &format!("__lbl_{}", depth - 1),
        proc_macro2::Span::call_site(),
    );

    let insertion_code = if terminal == BlockTypeKind::Vec {
        quote! {
            #entry_chain.entry(#last_ident).or_default().push(b);
        }
    } else {
        quote! {
            if #entry_chain.insert(#last_ident, b).is_some() {
                diags.push(crate::diagnostic::Diagnostic::error(
                    "Duplicate Block",
                    format!("Duplicate block '{}' with identical label(s)", #name_str),
                    block.span.clone(),
                ));
            }
        }
    };

    let field_ty = &field.ty;
    decode_fields.push(quote! {
        let mut #fname: #field_ty = Default::default();
        for block in &body.blocks {
            if block.block_type == #name_str {
                if block.labels.len() < #depth {
                    diags.push(crate::diagnostic::Diagnostic::error(
                        "Missing Label",
                        format!(
                            "Expected at least {} block label(s) for '{}', got {}",
                            #depth, #name_str, block.labels.len()
                        ),
                        block.span.clone(),
                    ));
                    continue;
                }
                #(#label_binds)*
                let sub_labels = &block.labels[#depth..];
                match crate::decode::DecodeBody::decode_body(&block.body, sub_labels, ctx) {
                    Ok(b) => {
                        #insertion_code
                    }
                    Err(d) => diags.extend(d),
                }
            }
        }
        if let Some(r) = remain_body.as_mut() {
            r.blocks.retain(|b| b.block_type != #name_str);
        }
    });
}

/// Handles decoding of an HCL block field (`#[hcl(block)]`).
///
/// # Arguments
/// * `field` - The struct field syntax AST node.
/// * `fname` - Identifier of the field.
/// * `name_str` - Block type identifier.
/// * `decode_fields` - Accumulator for field decoding statements.
/// * `slot_extracts` - Accumulator for field value extraction statements.
fn handle_block_field(
    field: &Field,
    fname: &syn::Ident,
    name_str: &str,
    decode_fields: &mut Vec<proc_macro2::TokenStream>,
    slot_extracts: &mut Vec<proc_macro2::TokenStream>,
) {
    let block_kind = inspect_block_type(&field.ty);
    match block_kind {
        BlockTypeKind::Vec => {
            decode_fields.push(quote! {
                let mut #fname = Vec::new();
                for block in &body.blocks {
                    if block.block_type == #name_str {
                        match crate::decode::DecodeBody::decode_body(&block.body, &block.labels, ctx) {
                            Ok(b) => #fname.push(b),
                            Err(d) => diags.extend(d),
                        }
                    }
                }
                if let Some(r) = remain_body.as_mut() {
                    r.blocks.retain(|b| b.block_type != #name_str);
                }
            });
        }
        BlockTypeKind::Option => {
            decode_fields.push(quote! {
                let mut #fname = None;
                for block in &body.blocks {
                    if block.block_type == #name_str {
                        match crate::decode::DecodeBody::decode_body(&block.body, &block.labels, ctx) {
                            Ok(b) => #fname = Some(b),
                            Err(d) => diags.extend(d),
                        }
                        break;
                    }
                }
                if let Some(r) = remain_body.as_mut() {
                    r.blocks.retain(|b| b.block_type != #name_str);
                }
            });
        }
        BlockTypeKind::Map { .. } => {
            handle_map_block_field(field, fname, name_str, &block_kind, decode_fields);
        }
        BlockTypeKind::Single => {
            let slot_ident =
                syn::Ident::new(&format!("__slot_{fname}"), proc_macro2::Span::call_site());
            decode_fields.push(quote! {
                let mut #slot_ident = None;
                for block in &body.blocks {
                    if block.block_type == #name_str {
                        match crate::decode::DecodeBody::decode_body(&block.body, &block.labels, ctx) {
                            Ok(b) => #slot_ident = Some(b),
                            Err(d) => diags.extend(d),
                        }
                        break;
                    }
                }
                if #slot_ident.is_none() {
                    diags.push(crate::diagnostic::Diagnostic::error(
                        "Missing Block",
                        format!("Block '{}' is missing", #name_str),
                        body.span.clone(),
                    ));
                }
                if let Some(r) = remain_body.as_mut() {
                    r.blocks.retain(|b| b.block_type != #name_str);
                }
            });
            slot_extracts.push(quote! {
                let #fname = match #slot_ident {
                    Some(v) => v,
                    None => return Err(diags),
                };
            });
        }
    }
}

/// Generates the fallback branch when an attribute is absent from the HCL body.
fn build_attribute_default_branch(
    name_str: &str,
    attrs: &HclFieldAttrs,
    is_option: bool,
    default_val_call: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    if let Some(ref expr_str) = attrs.default_expr {
        let eval_default_value = quote! {
            let mut parser = crate::parse::parser::Parser::new(#expr_str);
            let value = if let Some(expr) = parser.parse_expression() {
                match crate::eval::evaluator::Evaluator::new(ctx).evaluate(&expr) {
                    Ok((v, mut d)) => {
                        diags.extend(d);
                        v
                    }
                    Err(d) => {
                        diags.extend(d);
                        crate::types::val::Value::null(crate::types::ty::Type::Dynamic)
                    }
                }
            } else {
                diags.push(crate::diagnostic::Diagnostic::error(
                    "Invalid Default Expression",
                    format!("Failed to parse default expression '{}'", #expr_str),
                    body.span.clone(),
                ));
                crate::types::val::Value::null(crate::types::ty::Type::Dynamic)
            };
        };

        if is_option {
            return quote! {
                #eval_default_value
                match #default_val_call {
                    Ok(v) => Some(Some(v)),
                    Err(d) => {
                        diags.extend(d);
                        None
                    }
                }
            };
        }
        return quote! {
            #eval_default_value
            match #default_val_call {
                Ok(v) => Some(v),
                Err(d) => {
                    diags.extend(d);
                    None
                }
            }
        };
    }

    if let Some(ref default_fn) = attrs.default_fn {
        if is_option {
            return quote! {
                Some(Some(#default_fn()))
            };
        }
        return quote! {
            Some(#default_fn())
        };
    }

    if is_option || attrs.is_optional() {
        return quote! {
            Some(None)
        };
    }

    if attrs.is_default() {
        return quote! {
            Some(Default::default())
        };
    }

    quote! {
        diags.push(crate::diagnostic::Diagnostic::error(
            "Missing Attribute",
            format!("Attribute '{}' is missing", #name_str),
            body.span.clone(),
        ));
        None
    }
}

/// Handles decoding of a standard HCL attribute field.
///
/// # Arguments
/// * `field` - The struct field syntax AST node.
/// * `fname` - Identifier of the field.
/// * `name_str` - Attribute key name.
/// * `attrs` - Parsed HCL field attributes.
/// * `decode_fields` - Accumulator for field decoding statements.
/// * `slot_extracts` - Accumulator for field value extraction statements.
fn handle_attribute_field(
    field: &Field,
    fname: &syn::Ident,
    name_str: &str,
    attrs: &HclFieldAttrs,
    decode_fields: &mut Vec<proc_macro2::TokenStream>,
    slot_extracts: &mut Vec<proc_macro2::TokenStream>,
) {
    let ty = &field.ty;
    let is_option = is_type_ident(ty, "Option");

    let slot_ident = syn::Ident::new(&format!("__slot_{fname}"), proc_macro2::Span::call_site());

    let decode_val_call = if attrs.is_custom() {
        quote! { <#ty as crate::ext::customdecode::CustomDecodeExpression>::custom_decode_expr(&attr.expr, ctx) }
    } else if let Some(ref custom_dec) = attrs.custom_decoder {
        quote! { #custom_dec(&value, attr.span.clone(), ctx) }
    } else {
        quote! { crate::decode::DecodeValue::decode_value(&value, attr.span.clone()) }
    };

    let default_val_call = attrs.custom_decoder.as_ref().map_or_else(
        || quote! { crate::decode::DecodeValue::decode_value(&value, body.span.clone()) },
        |custom_dec| quote! { #custom_dec(&value, body.span.clone(), ctx) },
    );

    let default_branch =
        build_attribute_default_branch(name_str, attrs, is_option, &default_val_call);

    let evaluate_val = if attrs.is_custom() {
        quote! {
            let value = crate::types::val::Value::null(crate::types::ty::Type::Dynamic);
        }
    } else {
        quote! {
            let value = match crate::eval::evaluator::Evaluator::new(ctx).evaluate(&attr.expr) {
                Ok((v, mut d)) => {
                    diags.extend(d);
                    v
                }
                Err(d) => {
                    diags.extend(d);
                    crate::types::val::Value::null(crate::types::ty::Type::Dynamic)
                }
            };
        }
    };

    if is_option {
        decode_fields.push(quote! {
            let #slot_ident: Option<#ty> = if let Some(attr) = body.attributes.get(#name_str) {
                if let Some(r) = remain_body.as_mut() {
                    r.attributes.remove(#name_str);
                }
                #evaluate_val
                match #decode_val_call {
                    Ok(v) => Some(Some(v)),
                    Err(d) => {
                        diags.extend(d);
                        Some(None)
                    }
                }
            } else {
                #default_branch
            };
        });
    } else {
        decode_fields.push(quote! {
            let #slot_ident: Option<#ty> = if let Some(attr) = body.attributes.get(#name_str) {
                if let Some(r) = remain_body.as_mut() {
                    r.attributes.remove(#name_str);
                }
                #evaluate_val
                match #decode_val_call {
                    Ok(v) => Some(v),
                    Err(d) => {
                        diags.extend(d);
                        None
                    }
                }
            } else {
                #default_branch
            };
        });
    }

    slot_extracts.push(quote! {
        let #fname = match #slot_ident {
            Some(v) => v,
            None => return Err(diags),
        };
    });
}

/// Derives the `EncodeBody` trait for a struct.
///
/// This macro inspects the struct's fields to automatically serialize native Rust types
/// into HCL CST attributes and blocks.
#[cfg(not(tarpaulin_include))]
#[proc_macro_derive(EncodeBody, attributes(hcl))]
pub fn derive_encode_body(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_derive_encode_body(input).into()
}

/// Expands the `EncodeBody` derive macro for the given AST input.
pub(crate) fn expand_derive_encode_body(input: DeriveInput) -> proc_macro2::TokenStream {
    let name = input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let mut encode_stmts = Vec::new();
    let mut label_extracts = Vec::new();
    let mut flatten_label_extracts = Vec::new();
    let mut errors = Vec::new();

    if let Data::Struct(syn::DataStruct {
        fields: Fields::Named(ref fields),
        ..
    }) = input.data
    {
        for (field, fname) in iter_named_fields(fields) {
            process_encode_field(
                field,
                fname,
                &mut encode_stmts,
                &mut label_extracts,
                &mut flatten_label_extracts,
                &mut errors,
            );
        }
    }

    if !errors.is_empty() {
        return quote! {
            #(#errors)*
        };
    }

    quote! {
        impl #impl_generics crate::encode::EncodeBody for #name #ty_generics #where_clause {
            fn extract_labels(&self) -> Vec<String> {
                let mut labels = vec![#(#label_extracts),*];
                #(#flatten_label_extracts)*
                labels
            }

            fn encode_into_body(&self, body: &mut crate::cst::builder::CstBody) -> Result<(), crate::diagnostic::Diagnostics> {
                let mut diags = crate::diagnostic::Diagnostics::new();
                #(#encode_stmts)*
                if diags.has_errors() {
                    Err(diags)
                } else {
                    Ok(())
                }
            }
        }
    }
}

/// Generates nested encoding iteration loops for label-keyed block maps of arbitrary depth.
///
/// # Arguments
/// * `fname` - Identifier of the field.
/// * `field_name_str` - Block type name string.
/// * `depth` - Label nesting depth.
/// * `terminal` - Leaf block kind (e.g. `Single` or `Vec`).
fn generate_map_encode_loops(
    fname: &syn::Ident,
    field_name_str: &str,
    depth: usize,
    terminal: &BlockTypeKind,
) -> proc_macro2::TokenStream {
    let lbl_idents: Vec<syn::Ident> = (0..depth)
        .map(|i| syn::Ident::new(&format!("__lbl_{i}"), proc_macro2::Span::call_site()))
        .collect();

    let innermost = quote! {
        let mut item_labels = vec![#(#lbl_idents.clone()),*];
        item_labels.extend(crate::encode::EncodeBody::extract_labels(item));
        let mut block = crate::cst::builder::CstBlock::new(#field_name_str, item_labels);
        if let Err(d) = crate::encode::EncodeBody::encode_into_body(item, &mut block.body) {
            diags.extend(d);
        }
        body.append_block(block);
    };

    let last_map_ident = syn::Ident::new(
        &format!("__m_{}", depth - 1),
        proc_macro2::Span::call_site(),
    );
    let last_lbl = &lbl_idents[depth - 1];

    let leaf_loop = if *terminal == BlockTypeKind::Vec {
        quote! {
            for (#last_lbl, items) in #last_map_ident {
                for item in items {
                    #innermost
                }
            }
        }
    } else {
        quote! {
            for (#last_lbl, item) in #last_map_ident {
                #innermost
            }
        }
    };

    if depth == 1 {
        if *terminal == BlockTypeKind::Vec {
            quote! {
                for (#last_lbl, items) in &self.#fname {
                    for item in items {
                        #innermost
                    }
                }
            }
        } else {
            quote! {
                for (#last_lbl, item) in &self.#fname {
                    #innermost
                }
            }
        }
    } else {
        let mut current_loop = leaf_loop;
        for i in (1..depth).rev() {
            let parent_map = if i == 1 {
                quote! { &self.#fname }
            } else {
                let p = syn::Ident::new(&format!("__m_{}", i - 1), proc_macro2::Span::call_site());
                quote! { #p }
            };
            let m_ident = syn::Ident::new(&format!("__m_{i}"), proc_macro2::Span::call_site());
            let lbl = &lbl_idents[i - 1];
            current_loop = quote! {
                for (#lbl, #m_ident) in #parent_map {
                    #current_loop
                }
            };
        }
        current_loop
    }
}

/// Dispatches encoding logic for a single struct field into CST statements.
///
/// # Arguments
/// * `field` - The struct field syntax AST node.
/// * `fname` - Identifier of the field.
/// * `encode_stmts` - Accumulator for encoding statements.
/// * `label_extracts` - Accumulator for direct block label expressions.
/// * `flatten_label_extracts` - Accumulator for flattened block label extraction statements.
/// * `errors` - Accumulator for compile-time errors.
fn process_encode_field(
    field: &Field,
    fname: &syn::Ident,
    encode_stmts: &mut Vec<proc_macro2::TokenStream>,
    label_extracts: &mut Vec<proc_macro2::TokenStream>,
    flatten_label_extracts: &mut Vec<proc_macro2::TokenStream>,
    errors: &mut Vec<proc_macro2::TokenStream>,
) {
    let attrs = parse_hcl_field_attrs(field, errors);
    let field_name_str = attrs
        .custom_name
        .clone()
        .unwrap_or_else(|| fname.to_string());

    if attrs.is_label() {
        label_extracts.push(quote! { self.#fname.clone() });
    } else if attrs.is_body() {
        if is_type_ident(&field.ty, "Option") {
            encode_stmts.push(quote! {
                if let Some(ref b) = self.#fname {
                    if let Err(d) = crate::encode::EncodeBody::encode_into_body(b, body) {
                        diags.extend(d);
                    }
                }
            });
        } else {
            encode_stmts.push(quote! {
                if let Err(d) = crate::encode::EncodeBody::encode_into_body(&self.#fname, body) {
                    diags.extend(d);
                }
            });
        }
    } else if attrs.is_flatten() {
        if is_type_ident(&field.ty, "Option") {
            encode_stmts.push(quote! {
                if let Some(ref item) = self.#fname {
                    if let Err(d) = crate::encode::EncodeBody::encode_into_body(item, body) {
                        diags.extend(d);
                    }
                }
            });
            flatten_label_extracts.push(quote! {
                if let Some(ref item) = self.#fname {
                    labels.extend(crate::encode::EncodeBody::extract_labels(item));
                }
            });
        } else {
            encode_stmts.push(quote! {
                if let Err(d) = crate::encode::EncodeBody::encode_into_body(&self.#fname, body) {
                    diags.extend(d);
                }
            });
            flatten_label_extracts.push(quote! {
                labels.extend(crate::encode::EncodeBody::extract_labels(&self.#fname));
            });
        }
    } else if attrs.is_block() {
        process_encode_block_field(field, fname, &field_name_str, encode_stmts);
    } else if attrs.is_remain() || attrs.is_remain_attrs() || attrs.is_remain_blocks() {
        // Skip remaining capture fields during direct encoding
    } else if attrs.is_expr() {
        if is_type_ident(&field.ty, "Option") {
            encode_stmts.push(quote! {
                if let Some(ref expr) = self.#fname {
                    let toks = crate::cst::builder::tokens_for_expression(expr);
                    body.set_attribute_raw(#field_name_str, toks);
                }
            });
        } else {
            encode_stmts.push(quote! {
                let toks = crate::cst::builder::tokens_for_expression(&self.#fname);
                body.set_attribute_raw(#field_name_str, toks);
            });
        }
    } else if is_type_ident(&field.ty, "Option") {
        encode_stmts.push(quote! {
            if let Some(ref val) = self.#fname {
                let encoded_val = crate::encode::EncodeValue::encode_value(val);
                body.set_attribute_value(#field_name_str, &encoded_val);
            }
        });
    } else {
        encode_stmts.push(quote! {
            let encoded_val = crate::encode::EncodeValue::encode_value(&self.#fname);
            body.set_attribute_value(#field_name_str, &encoded_val);
        });
    }
}

/// Generates CST encoding statements for a block field (`#[hcl(block)]`).
fn process_encode_block_field(
    field: &Field,
    fname: &syn::Ident,
    field_name_str: &str,
    encode_stmts: &mut Vec<proc_macro2::TokenStream>,
) {
    let block_kind = inspect_block_type(&field.ty);
    match block_kind {
        BlockTypeKind::Vec => {
            encode_stmts.push(quote! {
                for item in &self.#fname {
                    let item_labels = crate::encode::EncodeBody::extract_labels(item);
                    let mut block = crate::cst::builder::CstBlock::new(#field_name_str, item_labels);
                    if let Err(d) = crate::encode::EncodeBody::encode_into_body(item, &mut block.body) {
                        diags.extend(d);
                    }
                    body.append_block(block);
                }
            });
        }
        BlockTypeKind::Option => {
            encode_stmts.push(quote! {
                if let Some(ref item) = self.#fname {
                    let item_labels = crate::encode::EncodeBody::extract_labels(item);
                    let mut block = crate::cst::builder::CstBlock::new(#field_name_str, item_labels);
                    if let Err(d) = crate::encode::EncodeBody::encode_into_body(item, &mut block.body) {
                        diags.extend(d);
                    }
                    body.append_block(block);
                }
            });
        }
        BlockTypeKind::Map { .. } => {
            let (depth, terminal) = map_depth_and_terminal(&block_kind);
            let encode_loops = generate_map_encode_loops(fname, field_name_str, depth, &terminal);
            encode_stmts.push(encode_loops);
        }
        BlockTypeKind::Single => {
            encode_stmts.push(quote! {
                let item_labels = crate::encode::EncodeBody::extract_labels(&self.#fname);
                let mut block = crate::cst::builder::CstBlock::new(#field_name_str, item_labels);
                if let Err(d) = crate::encode::EncodeBody::encode_into_body(&self.#fname, &mut block.body) {
                    diags.extend(d);
                }
                body.append_block(block);
            });
        }
    }
}

/// Parses a single nested meta item inside `#[hcl(...)]`.
fn parse_single_hcl_meta(
    meta: &syn::meta::ParseNestedMeta<'_>,
    attrs: &mut HclFieldAttrs,
) -> syn::Result<()> {
    if meta.path.is_ident("label") {
        attrs.set(attr_flags::LABEL);
    } else if meta.path.is_ident("block") {
        attrs.set(attr_flags::BLOCK);
    } else if meta.path.is_ident("attr") {
        attrs.set(attr_flags::ATTR);
    } else if meta.path.is_ident("optional") {
        attrs.set(attr_flags::OPTIONAL);
    } else if meta.path.is_ident("default") {
        attrs.set(attr_flags::DEFAULT);
    } else if meta.path.is_ident("remain") {
        attrs.set(attr_flags::REMAIN);
    } else if meta.path.is_ident("remain_attrs") {
        attrs.set(attr_flags::REMAIN_ATTRS);
    } else if meta.path.is_ident("remain_blocks") {
        attrs.set(attr_flags::REMAIN_BLOCKS);
    } else if meta.path.is_ident("expr") {
        attrs.set(attr_flags::EXPR);
    } else if meta.path.is_ident("flatten") || meta.path.is_ident("squash") {
        attrs.set(attr_flags::FLATTEN);
    } else if meta.path.is_ident("body") {
        attrs.set(attr_flags::BODY);
    } else if meta.path.is_ident("custom") {
        attrs.set(attr_flags::CUSTOM);
    } else if meta.path.is_ident("with") {
        let Ok(val) = meta.value() else {
            return Err(meta.error("unsupported hcl attribute"));
        };
        let Ok(lit) = val.parse::<syn::LitStr>() else {
            return Err(meta.error("unsupported hcl attribute"));
        };
        match syn::parse_str::<syn::Path>(&lit.value()) {
            Ok(p) => attrs.custom_decoder = Some(p),
            Err(e) => return Err(meta.error(format!("invalid path for custom decoder: {e}"))),
        }
    } else if meta.path.is_ident("default_expr") {
        let Ok(val) = meta.value() else {
            return Err(meta.error("unsupported hcl attribute"));
        };
        let Ok(lit) = val.parse::<syn::LitStr>() else {
            return Err(meta.error("unsupported hcl attribute"));
        };
        attrs.default_expr = Some(lit.value());
    } else if meta.path.is_ident("default_fn") {
        let Ok(val) = meta.value() else {
            return Err(meta.error("unsupported hcl attribute"));
        };
        let Ok(lit) = val.parse::<syn::LitStr>() else {
            return Err(meta.error("unsupported hcl attribute"));
        };
        match syn::parse_str::<syn::Path>(&lit.value()) {
            Ok(p) => attrs.default_fn = Some(p),
            Err(e) => return Err(meta.error(format!("invalid path for default_fn: {e}"))),
        }
    } else if meta.path.is_ident("name") {
        let Ok(val) = meta.value() else {
            return Err(meta.error("unsupported hcl attribute"));
        };
        let Ok(lit) = val.parse::<syn::LitStr>() else {
            return Err(meta.error("unsupported hcl attribute"));
        };
        attrs.custom_name = Some(lit.value());
    } else {
        return Err(meta.error("unsupported hcl attribute"));
    }
    Ok(())
}

/// Validates mutual exclusion rules on parsed [`HclFieldAttrs`].
fn validate_hcl_field_attrs(
    field: &Field,
    attrs: &HclFieldAttrs,
    errors: &mut Vec<proc_macro2::TokenStream>,
) {
    if attrs.default_expr.is_some() && attrs.default_fn.is_some() {
        errors.push(
            syn::Error::new_spanned(
                field,
                "field cannot specify both #[hcl(default_expr)] and #[hcl(default_fn)]",
            )
            .to_compile_error(),
        );
    }

    if attrs.is_body()
        && (attrs.is_attr()
            || attrs.is_block()
            || attrs.is_label()
            || attrs.is_remain()
            || attrs.is_remain_attrs()
            || attrs.is_remain_blocks()
            || attrs.is_expr()
            || attrs.is_flatten())
    {
        errors.push(
            syn::Error::new_spanned(
                field,
                "field cannot be marked as both #[hcl(body)] and another HCL attribute",
            )
            .to_compile_error(),
        );
    }

    if (attrs.custom_decoder.is_some()
        || attrs.default_expr.is_some()
        || attrs.default_fn.is_some())
        && (attrs.is_block()
            || attrs.is_label()
            || attrs.is_remain()
            || attrs.is_remain_attrs()
            || attrs.is_remain_blocks()
            || attrs.is_flatten()
            || attrs.is_body())
    {
        errors.push(
            syn::Error::new_spanned(
                field,
                "#[hcl(with)], #[hcl(default_expr)], and #[hcl(default_fn)] can only be applied to attribute fields",
            )
            .to_compile_error(),
        );
    }

    if attrs.is_attr() && attrs.is_block() {
        errors.push(
            syn::Error::new_spanned(
                field,
                "field cannot be marked as both #[hcl(attr)] and #[hcl(block)]",
            )
            .to_compile_error(),
        );
    }
    if attrs.is_label() && (attrs.is_block() || attrs.is_attr()) {
        errors.push(
            syn::Error::new_spanned(
                field,
                "field cannot be marked as #[hcl(label)] and block/attr",
            )
            .to_compile_error(),
        );
    }
    if attrs.is_flatten()
        && (attrs.is_attr()
            || attrs.is_block()
            || attrs.is_label()
            || attrs.is_remain()
            || attrs.is_remain_attrs()
            || attrs.is_remain_blocks()
            || attrs.is_expr())
    {
        errors.push(
            syn::Error::new_spanned(
                field,
                "field cannot be marked as both #[hcl(flatten)] and another HCL attribute",
            )
            .to_compile_error(),
        );
    }
}

/// Parses `#[hcl(...)]` helper attributes on a struct field into a [`HclFieldAttrs`].
///
/// # Arguments
/// * `field` - The struct field syntax AST node.
/// * `errors` - Accumulator for syntax and semantic attribute errors.
fn parse_hcl_field_attrs(
    field: &Field,
    errors: &mut Vec<proc_macro2::TokenStream>,
) -> HclFieldAttrs {
    let mut attrs = HclFieldAttrs::default();

    for attr in &field.attrs {
        if attr.path().is_ident("hcl") {
            let res = attr.parse_nested_meta(|meta| parse_single_hcl_meta(&meta, &mut attrs));
            if let Err(e) = res {
                errors.push(e.to_compile_error());
            }
        }
    }

    validate_hcl_field_attrs(field, &attrs, errors);

    attrs
}

mod value_macros;
pub(crate) use value_macros::{
    expand_derive_capsule_type, expand_derive_decode_value, expand_derive_encode_value,
    expand_derive_implied_body_schema, extract_generic_inner_type,
};

/// Derives the `ImpliedBodySchema` trait for a struct.
///
/// This macro inspects the struct's fields to automatically construct a `BodySchema`.
#[cfg(not(tarpaulin_include))]
#[proc_macro_derive(ImpliedBodySchema, attributes(hcl))]
pub fn derive_implied_body_schema(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_derive_implied_body_schema(input).into()
}

/// Derives the `DecodeValue` trait for a struct or enum.
///
/// Automatically implements `DecodeValue` by inspecting fields and mapping
/// HCL object attributes to struct fields or string values to enum variants.
#[cfg(not(tarpaulin_include))]
#[proc_macro_derive(DecodeValue, attributes(hcl))]
pub fn derive_decode_value(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_derive_decode_value(input).into()
}

/// Derives the `EncodeValue` trait for a struct or enum.
///
/// Automatically implements `EncodeValue` by serializing struct fields
/// into an HCL object value or enum variants into HCL string values.
#[cfg(not(tarpaulin_include))]
#[proc_macro_derive(EncodeValue, attributes(hcl))]
pub fn derive_encode_value(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_derive_encode_value(input).into()
}

/// Derives capsule operations and `From<T>` for `Value`.
///
/// Supports helper attributes:
/// - `#[hcl(type_name = "...")]` to specify a custom type name (defaults to struct name).
/// - `#[hcl(convert_to = "...")]` to register conversion to another HCL type.
/// - `#[hcl(convert_from = "...")]` to register conversion from an HCL value.
#[cfg(not(tarpaulin_include))]
#[proc_macro_derive(CapsuleType, attributes(hcl))]
pub fn derive_capsule_type(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_derive_capsule_type(input).into()
}

#[cfg(test)]
mod test_macro;
