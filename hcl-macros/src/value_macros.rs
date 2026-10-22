//! Expansion implementations for `ImpliedBodySchema`, `DecodeValue`, `EncodeValue`, and `CapsuleType` derive macros.

use crate::{
    BlockTypeKind, HclFieldAttrs, inspect_block_type, is_type_ident, iter_named_fields,
    last_path_segment, map_depth_and_terminal, parse_hcl_field_attrs,
};
use quote::quote;
use syn::{Data, DeriveInput, Field, Fields, Type};

/// Expands the `ImpliedBodySchema` derive macro for the given AST input.
pub fn expand_derive_implied_body_schema(input: DeriveInput) -> proc_macro2::TokenStream {
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
        for (field, fname) in iter_named_fields(fields) {
            process_schema_field(
                field,
                fname,
                &mut schema_stmts,
                &mut label_names,
                &mut flatten_label_stmts,
            );
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
    let attrs: HclFieldAttrs = parse_hcl_field_attrs(field, &mut Vec::new());
    let name_str = attrs
        .custom_name
        .clone()
        .unwrap_or_else(|| fname.to_string());

    if attrs.is_label() {
        label_names.push(name_str);
    } else if attrs.is_body() {
        // Body captures raw unparsed AST body without constraining schema
    } else if attrs.is_flatten() {
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
    } else if attrs.is_block() {
        let ty = &field.ty;
        let block_kind = inspect_block_type(ty);
        let inner_ty = extract_generic_inner_type(ty);
        if let BlockTypeKind::Map { .. } = block_kind {
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
        } else {
            schema_stmts.push(quote! {
                let block_labels = <#inner_ty as crate::ast::schema::ImpliedBodySchema>::implied_label_names();
                let block_body_schema = <#inner_ty as crate::ast::schema::ImpliedBodySchema>::implied_body_schema();
                schema = schema.with_block(
                    crate::ast::schema::BlockHeaderSchema::new(#name_str, block_labels)
                        .with_body_schema(block_body_schema),
                );
            });
        }
    } else if attrs.is_remain() || attrs.is_remain_attrs() || attrs.is_remain_blocks() {
        // Skip
    } else {
        let is_option = is_type_ident(&field.ty, "Option");

        if is_option
            || attrs.is_optional()
            || attrs.is_default()
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
pub fn extract_generic_inner_type(ty: &Type) -> &Type {
    if let Some(segment) = last_path_segment(ty) {
        if segment.ident == "Vec" || segment.ident == "Option" {
            if let syn::PathArguments::AngleBracketed(ref args) = segment.arguments {
                if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                    return extract_generic_inner_type(inner);
                }
            }
        }
        if segment.ident == "HashMap" || segment.ident == "BTreeMap" {
            if let syn::PathArguments::AngleBracketed(ref args) = segment.arguments {
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
    }
    ty
}

/// Expands `DecodeValue` for a struct with named fields.
fn expand_decode_value_struct(
    name: &syn::Ident,
    generics: &syn::Generics,
    fields: &syn::FieldsNamed,
) -> proc_macro2::TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let mut decode_stmts = Vec::new();
    let mut field_names = Vec::new();
    let mut errors = Vec::new();

    for (field, fname) in iter_named_fields(fields) {
        field_names.push(fname.clone());
        let attrs = parse_hcl_field_attrs(field, &mut errors);
        let name_str = attrs
            .custom_name
            .clone()
            .unwrap_or_else(|| fname.to_string());
        let ty = &field.ty;
        let is_optional = is_type_ident(ty, "Option") || attrs.is_optional();

        let missing_fallback = if is_optional {
            quote! { None }
        } else if attrs.is_default() {
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

        let decode_field = if attrs.is_flatten() {
            let inner_ty = extract_generic_inner_type(ty);
            if is_optional {
                quote! {
                    let #fname = match <#inner_ty as crate::decode::DecodeValue>::decode_value(value, span.clone()) {
                        Ok(v) => Some(v),
                        Err(d) => { diags.extend(d); None }
                    };
                }
            } else {
                quote! {
                    let #fname = match <#inner_ty as crate::decode::DecodeValue>::decode_value(value, span.clone()) {
                        Ok(v) => v,
                        Err(d) => { diags.extend(d); return Err(diags); }
                    };
                }
            }
        } else {
            quote! {
                let #fname = if let Some(field_val) = map.get(#name_str) {
                    match crate::decode::DecodeValue::decode_value(field_val, span.clone()) {
                        Ok(v) => v,
                        Err(d) => { diags.extend(d); #missing_fallback }
                    }
                } else {
                    #missing_fallback
                };
            }
        };
        decode_stmts.push(decode_field);
    }

    if !errors.is_empty() {
        return quote! { #(#errors)* };
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

/// Expands `DecodeValue` for an enum.
fn expand_decode_value_enum(
    name: &syn::Ident,
    generics: &syn::Generics,
    data_enum: &syn::DataEnum,
) -> proc_macro2::TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let mut arms = Vec::new();
    let mut errors = Vec::new();

    for variant in &data_enum.variants {
        let vname = &variant.ident;
        if !matches!(variant.fields, syn::Fields::Unit) {
            errors.push(
                syn::Error::new_spanned(variant, "DecodeValue only supports unit enum variants")
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
        let name_str = custom_name.unwrap_or_else(|| vname.to_string());
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

/// Expands the `DecodeValue` derive macro for the given AST input.
pub fn expand_derive_decode_value(input: DeriveInput) -> proc_macro2::TokenStream {
    match input.data {
        Data::Struct(syn::DataStruct {
            fields: Fields::Named(ref fields),
            ..
        }) => expand_decode_value_struct(&input.ident, &input.generics, fields),
        Data::Enum(ref data_enum) => {
            expand_decode_value_enum(&input.ident, &input.generics, data_enum)
        }
        _ => syn::Error::new_spanned(
            input.ident,
            "DecodeValue can only be derived on structs or enums",
        )
        .to_compile_error(),
    }
}

/// Expands `EncodeValue` for a struct with named fields.
fn expand_encode_value_struct(
    name: &syn::Ident,
    generics: &syn::Generics,
    fields: &syn::FieldsNamed,
) -> proc_macro2::TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let mut encode_stmts = Vec::new();
    let mut errors = Vec::new();

    for (field, fname) in iter_named_fields(fields) {
        let attrs = parse_hcl_field_attrs(field, &mut errors);
        let name_str = attrs
            .custom_name
            .clone()
            .unwrap_or_else(|| fname.to_string());
        let is_option = is_type_ident(&field.ty, "Option");

        if attrs.is_flatten() {
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

/// Expands `EncodeValue` for an enum.
fn expand_encode_value_enum(
    name: &syn::Ident,
    generics: &syn::Generics,
    data_enum: &syn::DataEnum,
) -> proc_macro2::TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let mut arms = Vec::new();
    let mut errors = Vec::new();

    for variant in &data_enum.variants {
        let vname = &variant.ident;
        if !matches!(variant.fields, syn::Fields::Unit) {
            errors.push(
                syn::Error::new_spanned(variant, "EncodeValue only supports unit enum variants")
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
        let name_str = custom_name.unwrap_or_else(|| vname.to_string());
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

/// Expands the `EncodeValue` derive macro for the given AST input.
pub fn expand_derive_encode_value(input: DeriveInput) -> proc_macro2::TokenStream {
    match input.data {
        Data::Struct(syn::DataStruct {
            fields: Fields::Named(ref fields),
            ..
        }) => expand_encode_value_struct(&input.ident, &input.generics, fields),
        Data::Enum(ref data_enum) => {
            expand_encode_value_enum(&input.ident, &input.generics, data_enum)
        }
        _ => syn::Error::new_spanned(
            input.ident,
            "EncodeValue can only be derived on structs or enums",
        )
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

/// Parses `#[hcl(...)]` attributes on a capsule struct/enum.
fn parse_capsule_attrs(
    attrs_slice: &[syn::Attribute],
    errors: &mut Vec<proc_macro2::TokenStream>,
) -> HclCapsuleAttrs {
    let mut attrs = HclCapsuleAttrs::default();
    for attr in attrs_slice {
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
    attrs
}

/// Expands the `CapsuleType` derive macro for the given AST input.
pub fn expand_derive_capsule_type(input: DeriveInput) -> proc_macro2::TokenStream {
    let name = input.ident;

    if !input.generics.params.is_empty() {
        return syn::Error::new_spanned(
            &input.generics,
            "CapsuleType does not support generic type parameters",
        )
        .to_compile_error();
    }

    match input.data {
        Data::Struct(_) | Data::Enum(_) => {}
        Data::Union(_) => {
            return syn::Error::new_spanned(
                name,
                "CapsuleType can only be derived on structs or enums",
            )
            .to_compile_error();
        }
    }

    let mut errors = Vec::new();
    let attrs = parse_capsule_attrs(&input.attrs, &mut errors);

    if !errors.is_empty() {
        return quote! {
            #(#errors)*
        };
    }

    let type_name_str = attrs.type_name.unwrap_or_else(|| name.to_string());

    let conversion_to = attrs.convert_to.map_or_else(
        || quote! { None },
        |conv_to| {
            quote! {
                Some(std::sync::Arc::new(|a: &dyn std::any::Any, target: &crate::types::ty::Type| -> Option<crate::types::val::Value> {
                    if let Some(concrete) = a.downcast_ref::<#name>() {
                        #conv_to(concrete, target)
                    } else {
                        None
                    }
                }))
            }
        },
    );

    let conversion_from = attrs.convert_from.map_or_else(
        || quote! { None },
        |conv_from| {
            quote! {
                Some(std::sync::Arc::new(|val: &crate::types::val::Value| -> Option<std::sync::Arc<dyn std::any::Any + Send + Sync>> {
                    #conv_from(val).map(|c: #name| std::sync::Arc::new(c) as std::sync::Arc<dyn std::any::Any + Send + Sync>)
                }))
            }
        },
    );

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
