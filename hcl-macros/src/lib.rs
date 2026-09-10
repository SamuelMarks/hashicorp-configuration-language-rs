//! Procedural macros for the HCL-RS crate.
//!
//! Provides `DecodeBody` derive macro for structural decoding of HCL configuration.

#![deny(missing_docs)]

extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Field, Fields, Type, parse_macro_input};

/// Internal parsed representation of `#[hcl(...)]` field attributes.
#[derive(Default)]
struct HclFieldAttrs {
    /// True if field is marked with `#[hcl(label)]`.
    is_label: bool,
    /// True if field is marked with `#[hcl(block)]`.
    is_block: bool,
    /// True if field is marked with `#[hcl(attr)]`.
    is_attr: bool,
    /// True if field is marked with `#[hcl(optional)]`.
    is_optional: bool,
    /// True if field is marked with `#[hcl(default)]`.
    is_default: bool,
    /// True if field is marked with `#[hcl(remain)]`.
    is_remain: bool,
    /// True if field is marked with `#[hcl(remain_attrs)]`.
    is_remain_attrs: bool,
    /// True if field is marked with `#[hcl(remain_blocks)]`.
    is_remain_blocks: bool,
    /// True if field is marked with `#[hcl(expr)]`.
    is_expr: bool,
    /// True if field is marked with `#[hcl(flatten)]` or `#[hcl(squash)]`.
    is_flatten: bool,
    /// True if field is marked with `#[hcl(body)]`.
    is_body: bool,
    /// Custom name override if specified via `#[hcl(name = "...")]`.
    custom_name: Option<String>,
    /// Custom decoding function path via `#[hcl(with = "...")]`.
    custom_decoder: Option<syn::Path>,
    /// Default fallback HCL expression via `#[hcl(default_expr = "...")]`.
    default_expr: Option<String>,
    /// Default factory function path via `#[hcl(default_fn = "...")]`.
    default_fn: Option<syn::Path>,
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

/// Expands the `DecodeBody` derive macro for the given AST input.
pub(crate) fn expand_derive_decode_body(input: DeriveInput) -> proc_macro2::TokenStream {
    let name = input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let mut decode_fields = Vec::new();
    let mut field_names = Vec::new();
    let mut slot_extracts = Vec::new();
    let mut remain_field: Option<(syn::Ident, syn::Type)> = None;
    let mut remain_attrs_field: Option<(syn::Ident, syn::Type)> = None;
    let mut remain_blocks_field: Option<(syn::Ident, syn::Type)> = None;
    let mut body_field: Option<(syn::Ident, syn::Type)> = None;
    let mut label_count = 0;
    let mut errors = Vec::new();
    let mut parent_attrs = Vec::new();
    let mut parent_blocks = Vec::new();
    let mut declared_names = std::collections::HashMap::<String, proc_macro2::Span>::new();

    if let Data::Struct(syn::DataStruct {
        fields: Fields::Named(ref fields),
        ..
    }) = input.data
    {
        for field in &fields.named {
            if let Some(fname) = &field.ident {
                process_field(
                    field,
                    fname,
                    &mut decode_fields,
                    &mut field_names,
                    &mut slot_extracts,
                    &mut remain_field,
                    &mut remain_attrs_field,
                    &mut remain_blocks_field,
                    &mut body_field,
                    &mut label_count,
                    &mut parent_attrs,
                    &mut parent_blocks,
                    &mut declared_names,
                    &mut errors,
                );
            }
        }
    }

    if !errors.is_empty() {
        return quote! {
            #(#errors)*
        };
    }

    let remain_setup = quote! {
        let mut remain_body: Option<crate::ast::structure::Body> = Some(body.clone());
    };

    let collision_setup = quote! {
        let mut seen_attrs = std::collections::HashSet::<String>::new();
        let mut seen_blocks = std::collections::HashSet::<String>::new();
        #(
            seen_attrs.insert(#parent_attrs.to_string());
        )*
        #(
            seen_blocks.insert(#parent_blocks.to_string());
        )*
    };

    let remain_assign = if let Some((fname, ty)) = remain_field {
        let is_hashmap = match &ty {
            Type::Path(p) => p.path.segments.last().is_some_and(|s| s.ident == "HashMap"),
            _ => false,
        };
        let is_btreemap = match &ty {
            Type::Path(p) => p
                .path
                .segments
                .last()
                .is_some_and(|s| s.ident == "BTreeMap"),
            _ => false,
        };

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
    } else {
        quote! {}
    };

    let remain_attrs_assign = if let Some((fname, ty)) = remain_attrs_field {
        let is_btreemap = match &ty {
            Type::Path(p) => p
                .path
                .segments
                .last()
                .is_some_and(|s| s.ident == "BTreeMap"),
            _ => false,
        };

        if is_btreemap {
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
    } else {
        quote! {}
    };

    let remain_blocks_assign = if let Some((fname, _)) = remain_blocks_field {
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

    let label_validation = quote! {
        if labels.len() != #label_count {
            diags.push(crate::diagnostic::Diagnostic::error(
                "Invalid Label Count",
                format!("Expected {} block label(s), got {}", #label_count, labels.len()),
                body.span.clone(),
            ));
        }
    };

    let expanded = quote! {
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
    };

    expanded
}

/// Dispatches decoding logic for a single struct field based on its attributes.
#[allow(clippy::too_many_arguments)]
fn process_field(
    field: &Field,
    fname: &syn::Ident,
    decode_fields: &mut Vec<proc_macro2::TokenStream>,
    field_names: &mut Vec<syn::Ident>,
    slot_extracts: &mut Vec<proc_macro2::TokenStream>,
    remain_field: &mut Option<(syn::Ident, syn::Type)>,
    remain_attrs_field: &mut Option<(syn::Ident, syn::Type)>,
    remain_blocks_field: &mut Option<(syn::Ident, syn::Type)>,
    body_field: &mut Option<(syn::Ident, syn::Type)>,
    label_count: &mut usize,
    parent_attrs: &mut Vec<String>,
    parent_blocks: &mut Vec<String>,
    declared_names: &mut std::collections::HashMap<String, proc_macro2::Span>,
    errors: &mut Vec<proc_macro2::TokenStream>,
) {
    field_names.push(fname.clone());

    let attrs = parse_hcl_field_attrs(field, errors);
    let name_str = match attrs.custom_name.clone() {
        Some(s) => s,
        None => fname.to_string(),
    };

    if attrs.is_label {
        handle_label_field(field, fname, label_count, decode_fields, errors);
    } else if attrs.is_body {
        if body_field.is_some() {
            errors.push(
                syn::Error::new_spanned(field, "hcl(body) can only be applied to a single field")
                    .to_compile_error(),
            );
        } else {
            *body_field = Some((fname.clone(), field.ty.clone()));
        }
        let ty = &field.ty;
        let is_option = match ty {
            Type::Path(p) => p.path.segments.last().is_some_and(|s| s.ident == "Option"),
            _ => false,
        };
        if is_option {
            decode_fields.push(quote! {
                let #fname = Some(body.clone());
            });
        } else {
            decode_fields.push(quote! {
                let #fname = body.clone();
            });
        }
    } else if attrs.is_flatten {
        handle_flatten_field(field, fname, decode_fields, slot_extracts);
    } else if attrs.is_block {
        if declared_names.contains_key(&name_str) {
            errors.push(
                syn::Error::new_spanned(
                    field,
                    format!(
                        "duplicate attribute/block name '{}' declared on struct",
                        name_str
                    ),
                )
                .to_compile_error(),
            );
        } else {
            declared_names.insert(
                name_str.clone(),
                field
                    .ident
                    .as_ref()
                    .map_or_else(proc_macro2::Span::call_site, |i| i.span()),
            );
            parent_blocks.push(name_str.clone());
        }
        handle_block_field(field, fname, &name_str, decode_fields, slot_extracts);
    } else if attrs.is_expr {
        if declared_names.contains_key(&name_str) {
            errors.push(
                syn::Error::new_spanned(
                    field,
                    format!(
                        "duplicate attribute/block name '{}' declared on struct",
                        name_str
                    ),
                )
                .to_compile_error(),
            );
        } else {
            declared_names.insert(
                name_str.clone(),
                field
                    .ident
                    .as_ref()
                    .map_or_else(proc_macro2::Span::call_site, |i| i.span()),
            );
            parent_attrs.push(name_str.clone());
        }
        handle_expr_field(field, fname, &name_str, decode_fields, slot_extracts);
    } else if attrs.is_remain {
        if remain_field.is_some() {
            errors.push(
                syn::Error::new_spanned(field, "hcl(remain) can only be applied to a single field")
                    .to_compile_error(),
            );
        } else {
            *remain_field = Some((fname.clone(), field.ty.clone()));
        }
    } else if attrs.is_remain_attrs {
        if remain_attrs_field.is_some() {
            errors.push(
                syn::Error::new_spanned(
                    field,
                    "hcl(remain_attrs) can only be applied to a single field",
                )
                .to_compile_error(),
            );
        } else {
            *remain_attrs_field = Some((fname.clone(), field.ty.clone()));
        }
    } else if attrs.is_remain_blocks {
        if remain_blocks_field.is_some() {
            errors.push(
                syn::Error::new_spanned(
                    field,
                    "hcl(remain_blocks) can only be applied to a single field",
                )
                .to_compile_error(),
            );
        } else {
            *remain_blocks_field = Some((fname.clone(), field.ty.clone()));
        }
    } else {
        if declared_names.contains_key(&name_str) {
            errors.push(
                syn::Error::new_spanned(
                    field,
                    format!(
                        "duplicate attribute/block name '{}' declared on struct",
                        name_str
                    ),
                )
                .to_compile_error(),
            );
        } else {
            declared_names.insert(
                name_str.clone(),
                field
                    .ident
                    .as_ref()
                    .map_or_else(proc_macro2::Span::call_site, |i| i.span()),
            );
            parent_attrs.push(name_str.clone());
        }
        handle_attribute_field(
            field,
            fname,
            &name_str,
            &attrs,
            decode_fields,
            slot_extracts,
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
    let is_option = match ty {
        Type::Path(p) => p.path.segments.last().is_some_and(|s| s.ident == "Option"),
        _ => false,
    };
    let inner_ty = extract_generic_inner_type(ty);
    let slot_ident = syn::Ident::new(&format!("__slot_{}", fname), proc_macro2::Span::call_site());

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
    let is_option = match ty {
        Type::Path(p) => p.path.segments.last().is_some_and(|s| s.ident == "Option"),
        _ => false,
    };
    let is_arc = match ty {
        Type::Path(p) => {
            if let Some(seg) = p.path.segments.last() {
                if seg.ident == "Arc" {
                    true
                } else if seg.ident == "Option"
                    && let syn::PathArguments::AngleBracketed(ref args) = seg.arguments
                    && let Some(syn::GenericArgument::Type(Type::Path(inner_p))) = args.args.first()
                {
                    inner_p
                        .path
                        .segments
                        .last()
                        .is_some_and(|s| s.ident == "Arc")
                } else {
                    false
                }
            } else {
                false
            }
        }
        _ => false,
    };

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
            syn::Ident::new(&format!("__slot_{}", fname), proc_macro2::Span::call_site());
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
    let is_string = match &field.ty {
        Type::Path(p) => {
            p.path.segments.last().map(|s| s.ident.to_string()) == Some("String".to_string())
        }
        _ => false,
    };
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
    if let Type::Path(p) = ty
        && let Some(segment) = p.path.segments.last()
    {
        let ident_str = segment.ident.to_string();
        if ident_str == "Vec" {
            return BlockTypeKind::Vec;
        }
        if ident_str == "Option" {
            return BlockTypeKind::Option;
        }
        if (ident_str == "HashMap" || ident_str == "BTreeMap")
            && let syn::PathArguments::AngleBracketed(ref args) = segment.arguments
        {
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
            let (depth, terminal) = map_depth_and_terminal(&block_kind);
            let label_binds: Vec<proc_macro2::TokenStream> = (0..depth)
                .map(|i| {
                    let lbl_ident =
                        syn::Ident::new(&format!("__lbl_{i}"), proc_macro2::Span::call_site());
                    quote! {
                        let #lbl_ident = block.labels[#i].clone();
                    }
                })
                .collect();

            let mut entry_chain = quote! { #fname };
            for i in 0..(depth - 1) {
                let lbl_ident =
                    syn::Ident::new(&format!("__lbl_{i}"), proc_macro2::Span::call_site());
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
        BlockTypeKind::Single => {
            let slot_ident =
                syn::Ident::new(&format!("__slot_{}", fname), proc_macro2::Span::call_site());
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
    let is_option = match ty {
        Type::Path(p) => {
            p.path.segments.last().map(|s| s.ident.to_string()) == Some("Option".to_string())
        }
        _ => false,
    };

    let slot_ident = syn::Ident::new(&format!("__slot_{}", fname), proc_macro2::Span::call_site());

    let decode_val_call = if let Some(ref custom_dec) = attrs.custom_decoder {
        quote! { #custom_dec(&value, attr.span.clone(), ctx) }
    } else {
        quote! { crate::decode::DecodeValue::decode_value(&value, attr.span.clone()) }
    };

    let default_val_call = if let Some(ref custom_dec) = attrs.custom_decoder {
        quote! { #custom_dec(&value, body.span.clone(), ctx) }
    } else {
        quote! { crate::decode::DecodeValue::decode_value(&value, body.span.clone()) }
    };

    let default_branch = if let Some(ref expr_str) = attrs.default_expr {
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
            quote! {
                #eval_default_value
                match #default_val_call {
                    Ok(v) => Some(Some(v)),
                    Err(d) => {
                        diags.extend(d);
                        None
                    }
                }
            }
        } else {
            quote! {
                #eval_default_value
                match #default_val_call {
                    Ok(v) => Some(v),
                    Err(d) => {
                        diags.extend(d);
                        None
                    }
                }
            }
        }
    } else if let Some(ref default_fn) = attrs.default_fn {
        if is_option {
            quote! {
                Some(Some(#default_fn()))
            }
        } else {
            quote! {
                Some(#default_fn())
            }
        }
    } else if is_option || attrs.is_optional {
        quote! {
            Some(None)
        }
    } else if attrs.is_default {
        quote! {
            Some(Default::default())
        }
    } else {
        quote! {
            diags.push(crate::diagnostic::Diagnostic::error(
                "Missing Attribute",
                format!("Attribute '{}' is missing", #name_str),
                body.span.clone(),
            ));
            None
        }
    };

    if is_option {
        decode_fields.push(quote! {
            let #slot_ident: Option<#ty> = if let Some(attr) = body.attributes.get(#name_str) {
                if let Some(r) = remain_body.as_mut() {
                    r.attributes.remove(#name_str);
                }
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
        for field in &fields.named {
            if let Some(fname) = &field.ident {
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
    let field_name_str = match attrs.custom_name {
        Some(s) => s,
        None => fname.to_string(),
    };

    if attrs.is_label {
        label_extracts.push(quote! { self.#fname.clone() });
    } else if attrs.is_body {
        let ty = &field.ty;
        let is_option = match ty {
            Type::Path(p) => p.path.segments.last().is_some_and(|s| s.ident == "Option"),
            _ => false,
        };
        if is_option {
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
    } else if attrs.is_flatten {
        let ty = &field.ty;
        let is_option = match ty {
            Type::Path(p) => p.path.segments.last().is_some_and(|s| s.ident == "Option"),
            _ => false,
        };
        if is_option {
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
    } else if attrs.is_block {
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
                let encode_loops =
                    generate_map_encode_loops(fname, &field_name_str, depth, &terminal);
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
    } else if attrs.is_remain || attrs.is_remain_attrs || attrs.is_remain_blocks {
        // Skip remaining capture fields during direct encoding
    } else if attrs.is_expr {
        let ty = &field.ty;
        let is_option = match ty {
            Type::Path(p) => p.path.segments.last().is_some_and(|s| s.ident == "Option"),
            _ => false,
        };
        if is_option {
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
    } else {
        let ty = &field.ty;
        let is_option = match ty {
            Type::Path(p) => {
                p.path.segments.last().map(|s| s.ident.to_string()) == Some("Option".to_string())
            }
            _ => false,
        };

        if is_option {
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
            let res = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("label") {
                    attrs.is_label = true;
                } else if meta.path.is_ident("block") {
                    attrs.is_block = true;
                } else if meta.path.is_ident("attr") {
                    attrs.is_attr = true;
                } else if meta.path.is_ident("optional") {
                    attrs.is_optional = true;
                } else if meta.path.is_ident("default") {
                    attrs.is_default = true;
                } else if meta.path.is_ident("remain") {
                    attrs.is_remain = true;
                } else if meta.path.is_ident("remain_attrs") {
                    attrs.is_remain_attrs = true;
                } else if meta.path.is_ident("remain_blocks") {
                    attrs.is_remain_blocks = true;
                } else if meta.path.is_ident("expr") {
                    attrs.is_expr = true;
                } else if meta.path.is_ident("flatten") || meta.path.is_ident("squash") {
                    attrs.is_flatten = true;
                } else if meta.path.is_ident("body") {
                    attrs.is_body = true;
                } else if meta.path.is_ident("with")
                    && let Ok(val) = meta.value()
                    && let Ok(lit) = val.parse::<syn::LitStr>()
                {
                    match syn::parse_str::<syn::Path>(&lit.value()) {
                        Ok(p) => attrs.custom_decoder = Some(p),
                        Err(e) => {
                            return Err(meta.error(format!("invalid path for custom decoder: {e}")));
                        }
                    }
                } else if meta.path.is_ident("default_expr")
                    && let Ok(val) = meta.value()
                    && let Ok(lit) = val.parse::<syn::LitStr>()
                {
                    attrs.default_expr = Some(lit.value());
                } else if meta.path.is_ident("default_fn")
                    && let Ok(val) = meta.value()
                    && let Ok(lit) = val.parse::<syn::LitStr>()
                {
                    match syn::parse_str::<syn::Path>(&lit.value()) {
                        Ok(p) => attrs.default_fn = Some(p),
                        Err(e) => {
                            return Err(meta.error(format!("invalid path for default_fn: {e}")));
                        }
                    }
                } else if meta.path.is_ident("name")
                    && let Ok(val) = meta.value()
                    && let Ok(lit) = val.parse::<syn::LitStr>()
                {
                    attrs.custom_name = Some(lit.value());
                } else {
                    return Err(meta.error("unsupported hcl attribute"));
                }
                Ok(())
            });
            if let Err(e) = res {
                errors.push(e.to_compile_error());
            }
        }
    }

    if attrs.default_expr.is_some() && attrs.default_fn.is_some() {
        errors.push(
            syn::Error::new_spanned(
                field,
                "field cannot specify both #[hcl(default_expr)] and #[hcl(default_fn)]",
            )
            .to_compile_error(),
        );
    }

    if attrs.is_body
        && (attrs.is_attr
            || attrs.is_block
            || attrs.is_label
            || attrs.is_remain
            || attrs.is_remain_attrs
            || attrs.is_remain_blocks
            || attrs.is_expr
            || attrs.is_flatten)
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
        && (attrs.is_block
            || attrs.is_label
            || attrs.is_remain
            || attrs.is_remain_attrs
            || attrs.is_remain_blocks
            || attrs.is_flatten
            || attrs.is_body)
    {
        errors.push(
            syn::Error::new_spanned(
                field,
                "#[hcl(with)], #[hcl(default_expr)], and #[hcl(default_fn)] can only be applied to attribute fields",
            )
            .to_compile_error(),
        );
    }

    if attrs.is_attr && attrs.is_block {
        errors.push(
            syn::Error::new_spanned(
                field,
                "field cannot be marked as both #[hcl(attr)] and #[hcl(block)]",
            )
            .to_compile_error(),
        );
    }
    if attrs.is_label && (attrs.is_block || attrs.is_attr) {
        errors.push(
            syn::Error::new_spanned(
                field,
                "field cannot be marked as #[hcl(label)] and block/attr",
            )
            .to_compile_error(),
        );
    }
    if attrs.is_flatten
        && (attrs.is_attr
            || attrs.is_block
            || attrs.is_label
            || attrs.is_remain
            || attrs.is_remain_attrs
            || attrs.is_remain_blocks
            || attrs.is_expr)
    {
        errors.push(
            syn::Error::new_spanned(
                field,
                "field cannot be marked as both #[hcl(flatten)] and another HCL attribute",
            )
            .to_compile_error(),
        );
    }

    attrs
}

/// Derives the `ImpliedBodySchema` trait for a struct.
///
/// This macro inspects the struct's fields to automatically construct a `BodySchema`.
#[cfg(not(tarpaulin_include))]
#[proc_macro_derive(ImpliedBodySchema, attributes(hcl))]
pub fn derive_implied_body_schema(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_derive_implied_body_schema(input).into()
}

/// Expands the `ImpliedBodySchema` derive macro for the given AST input.
pub(crate) fn expand_derive_implied_body_schema(input: DeriveInput) -> proc_macro2::TokenStream {
    let name = input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let mut schema_stmts = Vec::new();
    let mut label_names = Vec::new();
    let mut flatten_label_stmts = Vec::new();

    if let Data::Struct(syn::DataStruct {
        fields: Fields::Named(ref fields),
        ..
    }) = input.data
    {
        for field in &fields.named {
            if let Some(fname) = &field.ident {
                process_schema_field(
                    field,
                    fname,
                    &mut schema_stmts,
                    &mut label_names,
                    &mut flatten_label_stmts,
                );
            }
        }
    }

    quote! {
        impl #impl_generics crate::ast::schema::ImpliedBodySchema for #name #ty_generics #where_clause {
            fn implied_body_schema() -> crate::ast::schema::BodySchema {
                let mut schema = crate::ast::schema::BodySchema::new();
                #(#schema_stmts)*
                schema
            }

            fn implied_label_names() -> Vec<String> {
                let mut labels = vec![#(#label_names.to_string()),*];
                #(#flatten_label_stmts)*
                labels
            }
        }
    }
}

/// Dispatches schema generation statements for a single struct field.
///
/// # Arguments
/// * `field` - The struct field syntax AST node.
/// * `fname` - Identifier of the field.
/// * `schema_stmts` - Accumulator for schema construction statements.
/// * `label_names` - Accumulator for direct label names.
/// * `flatten_label_stmts` - Accumulator for flattened child label extraction statements.
fn process_schema_field(
    field: &Field,
    fname: &syn::Ident,
    schema_stmts: &mut Vec<proc_macro2::TokenStream>,
    label_names: &mut Vec<String>,
    flatten_label_stmts: &mut Vec<proc_macro2::TokenStream>,
) {
    let attrs = parse_hcl_field_attrs(field, &mut Vec::new());
    let name_str = match attrs.custom_name.clone() {
        Some(s) => s,
        None => fname.to_string(),
    };

    if attrs.is_label {
        label_names.push(name_str);
    } else if attrs.is_body {
        // Body captures raw unparsed AST body without constraining schema
    } else if attrs.is_flatten {
        let ty = &field.ty;
        let inner_ty = extract_generic_inner_type(ty);
        schema_stmts.push(quote! {
            let child_schema = <#inner_ty as crate::ast::schema::ImpliedBodySchema>::implied_body_schema();
            for (_attr_name, attr_schema) in child_schema.attributes {
                schema = schema.with_attribute(attr_schema);
            }
            for (_block_type, block_schema) in child_schema.blocks {
                schema = schema.with_block(block_schema);
            }
        });
        flatten_label_stmts.push(quote! {
            labels.extend(<#inner_ty as crate::ast::schema::ImpliedBodySchema>::implied_label_names());
        });
    } else if attrs.is_block {
        let ty = &field.ty;
        let block_kind = inspect_block_type(ty);
        let inner_ty = extract_generic_inner_type(ty);
        match block_kind {
            BlockTypeKind::Map { .. } => {
                let (depth, _) = map_depth_and_terminal(&block_kind);
                let default_labels: Vec<String> = match depth {
                    1 => vec!["name".to_string()],
                    2 => vec!["type".to_string(), "name".to_string()],
                    3 => vec![
                        "type".to_string(),
                        "subtype".to_string(),
                        "name".to_string(),
                    ],
                    n => (1..=n).map(|i| format!("label_{i}")).collect(),
                };
                schema_stmts.push(quote! {
                    let mut block_labels = vec![#(#default_labels.to_string()),*];
                    block_labels.extend(<#inner_ty as crate::ast::schema::ImpliedBodySchema>::implied_label_names());
                    let block_body_schema = <#inner_ty as crate::ast::schema::ImpliedBodySchema>::implied_body_schema();
                    schema = schema.with_block(
                        crate::ast::schema::BlockHeaderSchema::new(#name_str, block_labels)
                            .with_body_schema(block_body_schema),
                    );
                });
            }
            _ => {
                schema_stmts.push(quote! {
                    let block_labels = <#inner_ty as crate::ast::schema::ImpliedBodySchema>::implied_label_names();
                    let block_body_schema = <#inner_ty as crate::ast::schema::ImpliedBodySchema>::implied_body_schema();
                    schema = schema.with_block(
                        crate::ast::schema::BlockHeaderSchema::new(#name_str, block_labels)
                            .with_body_schema(block_body_schema),
                    );
                });
            }
        }
    } else if attrs.is_remain || attrs.is_remain_attrs || attrs.is_remain_blocks {
        // Skip
    } else {
        let ty = &field.ty;
        let is_option = match ty {
            Type::Path(p) => {
                p.path.segments.last().map(|s| s.ident.to_string()) == Some("Option".to_string())
            }
            _ => false,
        };

        if is_option
            || attrs.is_optional
            || attrs.is_default
            || attrs.default_expr.is_some()
            || attrs.default_fn.is_some()
        {
            schema_stmts.push(quote! {
                schema = schema.with_attribute(crate::ast::schema::AttributeSchema::optional(#name_str));
            });
        } else {
            schema_stmts.push(quote! {
                schema = schema.with_attribute(crate::ast::schema::AttributeSchema::required(#name_str));
            });
        }
    }
}

/// Recursively inspects container types (`Vec`, `Option`, `HashMap`, `BTreeMap`) to extract the leaf element type.
///
/// # Arguments
/// * `ty` - The syn syntax tree type to inspect.
fn extract_generic_inner_type(ty: &Type) -> &Type {
    if let Type::Path(p) = ty
        && let Some(segment) = p.path.segments.last()
    {
        if (segment.ident == "Vec" || segment.ident == "Option")
            && let syn::PathArguments::AngleBracketed(ref args) = segment.arguments
            && let Some(syn::GenericArgument::Type(inner)) = args.args.first()
        {
            return extract_generic_inner_type(inner);
        }
        if (segment.ident == "HashMap" || segment.ident == "BTreeMap")
            && let syn::PathArguments::AngleBracketed(ref args) = segment.arguments
        {
            let mut type_args = args.args.iter().filter_map(|arg| {
                if let syn::GenericArgument::Type(t) = arg {
                    Some(t)
                } else {
                    None
                }
            });
            let _key = type_args.next();
            if let Some(val) = type_args.next() {
                return extract_generic_inner_type(val);
            }
        }
    }
    ty
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

/// Expands the `DecodeValue` derive macro for the given AST input.
pub(crate) fn expand_derive_decode_value(input: DeriveInput) -> proc_macro2::TokenStream {
    let name = input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    match input.data {
        Data::Struct(syn::DataStruct {
            fields: Fields::Named(ref fields),
            ..
        }) => {
            let mut decode_stmts = Vec::new();
            let mut field_names = Vec::new();
            let mut errors = Vec::new();

            for field in &fields.named {
                if let Some(ref fname) = field.ident {
                    field_names.push(fname.clone());

                    let attrs = parse_hcl_field_attrs(field, &mut errors);
                    let name_str = match attrs.custom_name.clone() {
                        Some(s) => s,
                        None => fname.to_string(),
                    };
                    let ty = &field.ty;
                    let is_option = match ty {
                        Type::Path(p) => {
                            p.path.segments.last().map(|s| s.ident.to_string())
                                == Some("Option".to_string())
                        }
                        _ => false,
                    };
                    let is_optional = is_option || attrs.is_optional;
                    let is_default = attrs.is_default;

                    let missing_fallback = if is_optional {
                        quote! { None }
                    } else if is_default {
                        quote! { Default::default() }
                    } else {
                        quote! {
                            diags.push(crate::diagnostic::Diagnostic::error(
                                "Missing Required Field",
                                format!("Attribute '{}' is missing in object", #name_str),
                                span.clone(),
                            ));
                            return Err(diags);
                        }
                    };

                    let decode_field = if attrs.is_flatten {
                        let inner_ty = extract_generic_inner_type(ty);
                        if is_optional {
                            quote! {
                                let #fname = match <#inner_ty as crate::decode::DecodeValue>::decode_value(value, span.clone()) {
                                    Ok(v) => Some(v),
                                    Err(d) => {
                                        diags.extend(d);
                                        None
                                    }
                                };
                            }
                        } else {
                            quote! {
                                let #fname = match <#inner_ty as crate::decode::DecodeValue>::decode_value(value, span.clone()) {
                                    Ok(v) => v,
                                    Err(d) => {
                                        diags.extend(d);
                                        return Err(diags);
                                    }
                                };
                            }
                        }
                    } else {
                        quote! {
                            let #fname = if let Some(field_val) = map.get(#name_str) {
                                match crate::decode::DecodeValue::decode_value(field_val, span.clone()) {
                                    Ok(v) => v,
                                    Err(d) => {
                                        diags.extend(d);
                                        #missing_fallback
                                    }
                                }
                            } else {
                                #missing_fallback
                            };
                        }
                    };

                    decode_stmts.push(decode_field);
                }
            }

            if !errors.is_empty() {
                return quote! {
                    #(#errors)*
                };
            }

            quote! {
                impl #impl_generics crate::decode::DecodeValue for #name #ty_generics #where_clause {
                    fn decode_value(
                        value: &crate::types::val::Value,
                        span: crate::span::Span,
                    ) -> Result<Self, crate::diagnostic::Diagnostics> {
                        match &*value.data {
                            crate::types::val::ValueData::Object(map) => {
                                let mut diags = crate::diagnostic::Diagnostics::new();
                                #( #decode_stmts )*
                                if diags.has_errors() {
                                    Err(diags)
                                } else {
                                    Ok(Self { #( #field_names ),* })
                                }
                            }
                            _ => Err(crate::diagnostic::Diagnostics::from(
                                crate::diagnostic::Diagnostic::error(
                                    "Type Mismatch",
                                    format!("Expected object for {}, got {}", stringify!(#name), value.ty()),
                                    span,
                                ),
                            )),
                        }
                    }
                }
            }
        }
        Data::Enum(ref data_enum) => {
            let mut arms = Vec::new();
            let mut errors = Vec::new();

            for variant in &data_enum.variants {
                let vname = &variant.ident;
                if !matches!(variant.fields, syn::Fields::Unit) {
                    errors.push(
                        syn::Error::new_spanned(
                            variant,
                            "DecodeValue only supports unit enum variants",
                        )
                        .to_compile_error(),
                    );
                    continue;
                }

                let mut custom_name: Option<String> = None;
                for attr in &variant.attrs {
                    if attr.path().is_ident("hcl") {
                        let res = attr.parse_nested_meta(|meta| {
                            if meta.path.is_ident("name") {
                                let val = meta.value()?;
                                let lit: syn::LitStr = val.parse()?;
                                custom_name = Some(lit.value());
                                Ok(())
                            } else {
                                Err(meta.error("unsupported hcl enum attribute"))
                            }
                        });
                        if let Err(e) = res {
                            errors.push(e.to_compile_error());
                        }
                    }
                }
                let name_str = match custom_name {
                    Some(s) => s,
                    None => vname.to_string(),
                };
                arms.push(quote! {
                    #name_str => Ok(#name::#vname),
                });
            }

            if !errors.is_empty() {
                return quote! {
                    #(#errors)*
                };
            }

            quote! {
                impl #impl_generics crate::decode::DecodeValue for #name #ty_generics #where_clause {
                    fn decode_value(
                        value: &crate::types::val::Value,
                        span: crate::span::Span,
                    ) -> Result<Self, crate::diagnostic::Diagnostics> {
                        match &*value.data {
                            crate::types::val::ValueData::String(s) => match s.as_str() {
                                #( #arms )*
                                _ => Err(crate::diagnostic::Diagnostics::from(
                                    crate::diagnostic::Diagnostic::error(
                                        "Invalid Enum Variant",
                                        format!("Unknown variant '{}' for enum '{}'", s, stringify!(#name)),
                                        span,
                                    ),
                                )),
                            },
                            _ => Err(crate::diagnostic::Diagnostics::from(
                                crate::diagnostic::Diagnostic::error(
                                    "Type Mismatch",
                                    format!("Expected string for enum '{}', got {}", stringify!(#name), value.ty()),
                                    span,
                                ),
                            )),
                        }
                    }
                }
            }
        }
        _ => syn::Error::new_spanned(name, "DecodeValue can only be derived on structs or enums")
            .to_compile_error(),
    }
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

/// Expands the `EncodeValue` derive macro for the given AST input.
pub(crate) fn expand_derive_encode_value(input: DeriveInput) -> proc_macro2::TokenStream {
    let name = input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    match input.data {
        Data::Struct(syn::DataStruct {
            fields: Fields::Named(ref fields),
            ..
        }) => {
            let mut encode_stmts = Vec::new();
            let mut errors = Vec::new();

            for field in &fields.named {
                if let Some(ref fname) = field.ident {
                    let attrs = parse_hcl_field_attrs(field, &mut errors);
                    let name_str = match attrs.custom_name.clone() {
                        Some(s) => s,
                        None => fname.to_string(),
                    };
                    let ty = &field.ty;
                    let is_option = match ty {
                        Type::Path(p) => {
                            p.path.segments.last().map(|s| s.ident.to_string())
                                == Some("Option".to_string())
                        }
                        _ => false,
                    };

                    if attrs.is_flatten {
                        if is_option {
                            encode_stmts.push(quote! {
                                if let Some(ref child) = self.#fname {
                                    let child_val = crate::encode::EncodeValue::encode_value(child);
                                    if let crate::types::val::ValueData::Object(child_map) = &*child_val.data {
                                        for (k, v) in child_map {
                                            attrs.insert(k.clone(), v.ty().clone());
                                            map.insert(k.clone(), v.clone());
                                        }
                                    }
                                }
                            });
                        } else {
                            encode_stmts.push(quote! {
                                let child_val = crate::encode::EncodeValue::encode_value(&self.#fname);
                                if let crate::types::val::ValueData::Object(child_map) = &*child_val.data {
                                    for (k, v) in child_map {
                                        attrs.insert(k.clone(), v.ty().clone());
                                        map.insert(k.clone(), v.clone());
                                    }
                                }
                            });
                        }
                    } else if is_option {
                        encode_stmts.push(quote! {
                            optional_attrs.insert(#name_str.to_string());
                            if let Some(ref val) = self.#fname {
                                let encoded = crate::encode::EncodeValue::encode_value(val);
                                attrs.insert(#name_str.to_string(), encoded.ty().clone());
                                map.insert(#name_str.to_string(), encoded);
                            } else {
                                attrs.insert(#name_str.to_string(), crate::types::ty::Type::Dynamic);
                            }
                        });
                    } else {
                        encode_stmts.push(quote! {
                            let encoded = crate::encode::EncodeValue::encode_value(&self.#fname);
                            attrs.insert(#name_str.to_string(), encoded.ty().clone());
                            map.insert(#name_str.to_string(), encoded);
                        });
                    }
                }
            }

            if !errors.is_empty() {
                return quote! {
                    #(#errors)*
                };
            }

            quote! {
                impl #impl_generics crate::encode::EncodeValue for #name #ty_generics #where_clause {
                    fn encode_value(&self) -> crate::types::val::Value {
                        let mut map = std::collections::BTreeMap::new();
                        let mut attrs = std::collections::BTreeMap::new();
                        let mut optional_attrs = std::collections::BTreeSet::new();

                        #( #encode_stmts )*

                        let obj_ty = crate::types::ty::Type::Object {
                            attrs,
                            optional_attrs,
                        };
                        crate::types::val::Value::new(obj_ty, crate::types::val::ValueData::Object(map))
                    }
                }
            }
        }
        Data::Enum(ref data_enum) => {
            let mut arms = Vec::new();
            let mut errors = Vec::new();

            for variant in &data_enum.variants {
                let vname = &variant.ident;
                if !matches!(variant.fields, syn::Fields::Unit) {
                    errors.push(
                        syn::Error::new_spanned(
                            variant,
                            "EncodeValue only supports unit enum variants",
                        )
                        .to_compile_error(),
                    );
                    continue;
                }

                let mut custom_name: Option<String> = None;
                for attr in &variant.attrs {
                    if attr.path().is_ident("hcl") {
                        let res = attr.parse_nested_meta(|meta| {
                            if meta.path.is_ident("name") {
                                let val = meta.value()?;
                                let lit: syn::LitStr = val.parse()?;
                                custom_name = Some(lit.value());
                                Ok(())
                            } else {
                                Err(meta.error("unsupported hcl enum attribute"))
                            }
                        });
                        if let Err(e) = res {
                            errors.push(e.to_compile_error());
                        }
                    }
                }
                let name_str = match custom_name {
                    Some(s) => s,
                    None => vname.to_string(),
                };
                arms.push(quote! {
                    #name::#vname => {
                        crate::types::val::Value::new(
                            crate::types::ty::Type::String,
                            crate::types::val::ValueData::String(#name_str.to_string()),
                        )
                    }
                });
            }

            if !errors.is_empty() {
                return quote! {
                    #(#errors)*
                };
            }

            quote! {
                impl #impl_generics crate::encode::EncodeValue for #name #ty_generics #where_clause {
                    fn encode_value(&self) -> crate::types::val::Value {
                        match self {
                            #( #arms )*
                        }
                    }
                }
            }
        }
        _ => syn::Error::new_spanned(name, "EncodeValue can only be derived on structs or enums")
            .to_compile_error(),
    }
}

/// Helper attributes for `#[derive(CapsuleType)]`.
#[derive(Default)]
struct HclCapsuleAttrs {
    /// Custom type name (e.g. `#[hcl(type_name = "...")]`).
    type_name: Option<String>,
    /// Conversion function to another HCL type (`#[hcl(convert_to = "...")]`).
    convert_to: Option<syn::Path>,
    /// Conversion function from an HCL value (`#[hcl(convert_from = "...")]`).
    convert_from: Option<syn::Path>,
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

/// Expands the `CapsuleType` derive macro for the given AST input.
pub(crate) fn expand_derive_capsule_type(input: DeriveInput) -> proc_macro2::TokenStream {
    let name = &input.ident;

    if !input.generics.params.is_empty() {
        return syn::Error::new_spanned(
            &input.generics,
            "CapsuleType does not support generic type parameters",
        )
        .to_compile_error();
    }

    match input.data {
        Data::Struct(_) | Data::Enum(_) => {}
        _ => {
            return syn::Error::new_spanned(
                name,
                "CapsuleType can only be derived on structs or enums",
            )
            .to_compile_error();
        }
    }

    let mut attrs = HclCapsuleAttrs::default();
    let mut errors = Vec::new();

    for attr in &input.attrs {
        if attr.path().is_ident("hcl") {
            let res = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("type_name") {
                    let val = meta.value()?;
                    let lit: syn::LitStr = val.parse()?;
                    attrs.type_name = Some(lit.value());
                    Ok(())
                } else if meta.path.is_ident("convert_to") {
                    let val = meta.value()?;
                    if let Ok(lit) = val.parse::<syn::LitStr>() {
                        let path: syn::Path = lit.parse()?;
                        attrs.convert_to = Some(path);
                    } else {
                        let path: syn::Path = val.parse()?;
                        attrs.convert_to = Some(path);
                    }
                    Ok(())
                } else if meta.path.is_ident("convert_from") {
                    let val = meta.value()?;
                    if let Ok(lit) = val.parse::<syn::LitStr>() {
                        let path: syn::Path = lit.parse()?;
                        attrs.convert_from = Some(path);
                    } else {
                        let path: syn::Path = val.parse()?;
                        attrs.convert_from = Some(path);
                    }
                    Ok(())
                } else {
                    Err(meta.error("unsupported hcl capsule attribute"))
                }
            });
            if let Err(e) = res {
                errors.push(e.to_compile_error());
            }
        }
    }

    if !errors.is_empty() {
        return quote! {
            #(#errors)*
        };
    }

    let type_name_str = match attrs.type_name {
        Some(s) => s,
        None => name.to_string(),
    };

    let conversion_to = if let Some(conv_to) = attrs.convert_to {
        quote! {
            Some(std::sync::Arc::new(|a: &dyn std::any::Any, target: &crate::types::ty::Type| -> Option<crate::types::val::Value> {
                if let Some(concrete) = a.downcast_ref::<#name>() {
                    #conv_to(concrete, target)
                } else {
                    None
                }
            }))
        }
    } else {
        quote! { None }
    };

    let conversion_from = if let Some(conv_from) = attrs.convert_from {
        quote! {
            Some(std::sync::Arc::new(|val: &crate::types::val::Value| -> Option<std::sync::Arc<dyn std::any::Any + Send + Sync>> {
                #conv_from(val).map(|c: #name| std::sync::Arc::new(c) as std::sync::Arc<dyn std::any::Any + Send + Sync>)
            }))
        }
    } else {
        quote! { None }
    };

    quote! {
        impl #name {
            /// Returns the static [`crate::types::ty::CapsuleOps`] operations table registered for this capsule type.
            pub fn capsule_ops() -> std::sync::Arc<crate::types::ty::CapsuleOps> {
                static OPS: std::sync::LazyLock<std::sync::Arc<crate::types::ty::CapsuleOps>> = std::sync::LazyLock::new(|| {
                    std::sync::Arc::new(crate::types::ty::CapsuleOps {
                        type_name: #type_name_str,
                        equals: std::sync::Arc::new(|a: &dyn std::any::Any, b: &dyn std::any::Any| -> bool {
                            if let (Some(x), Some(y)) = (
                                a.downcast_ref::<#name>(),
                                b.downcast_ref::<#name>(),
                            ) {
                                x == y
                            } else {
                                false
                            }
                        }),
                        hash: std::sync::Arc::new(|a: &dyn std::any::Any| -> u64 {
                            if let Some(x) = a.downcast_ref::<#name>() {
                                use std::hash::{Hash, Hasher};
                                let mut s = std::collections::hash_map::DefaultHasher::new();
                                x.hash(&mut s);
                                s.finish()
                            } else {
                                0
                            }
                        }),
                        conversion_to: #conversion_to,
                        conversion_from: #conversion_from,
                        add: None,
                        sub: None,
                        mul: None,
                        div: None,
                        modulo: None,
                        neg: None,
                        cmp: None,
                        index_get: None,
                        attr_get: None,
                        methods: std::collections::HashMap::new(),
                    })
                });
                OPS.clone()
            }
        }

        impl From<#name> for crate::types::val::Value {
            fn from(val: #name) -> Self {
                crate::types::val::Value::from_capsule(val, #name::capsule_ops())
            }
        }
    }
}

#[cfg(test)]
mod test_macro;
