//! Tests for procedural macro expansion logic in `hcl-macros`.

use crate::{
    expand_derive_capsule_type, expand_derive_decode_body, expand_derive_decode_value,
    expand_derive_encode_body, expand_derive_encode_value, expand_derive_implied_body_schema,
};
use syn::DeriveInput;

/// Tests expansion of `DecodeBody` across various field types and configurations.
#[test]
fn test_expand_decode_body_comprehensive() {
    // 1. Struct with all valid annotations: label, blocks (Vec, Option, plain), exprs, remains, attrs
    let input: DeriveInput = syn::parse_quote! {
        struct FullConfig {
            #[hcl(label)]
            name: String,
            #[hcl(block)]
            server: Vec<ServerConfig>,
            #[hcl(block)]
            database: Option<DbConfig>,
            #[hcl(block, name = "primary_network")]
            network: NetConfig,
            #[hcl(block)]
            non_path_block: (u32, u32),
            #[hcl(expr)]
            raw_rule: Expression,
            #[hcl(expr)]
            opt_rule: Option<Expression>,
            #[hcl(expr)]
            arc_rule: std::sync::Arc<Expression>,
            #[hcl(expr)]
            opt_arc_rule: Option<std::sync::Arc<Expression>>,
            #[hcl(expr)]
            non_path_expr: (u32, u32),
            #[hcl(remain)]
            remaining_hashmap: std::collections::HashMap<String, Value>,
            #[hcl(remain_attrs)]
            extra_attrs_btreemap: std::collections::BTreeMap<String, Value>,
            #[hcl(remain_blocks)]
            extra_blocks: Vec<Block>,
            #[hcl(optional)]
            opt_flag: bool,
            #[hcl(default)]
            def_count: u32,
            #[hcl(name = "custom_timeout")]
            timeout: Option<u64>,
            required_flag: bool,
            non_path_attr: (u32, u32),
        }
    };
    let output = expand_derive_decode_body(input);
    assert!(!output.is_empty());

    // 2. Struct with BTreeMap remain and HashMap remain_attrs
    let input_remain2: DeriveInput = syn::parse_quote! {
        struct ConfigWithMaps {
            #[hcl(remain)]
            remain_btreemap: std::collections::BTreeMap<String, Value>,
            #[hcl(remain_attrs)]
            attrs_hashmap: std::collections::HashMap<String, Value>,
        }
    };
    let output2 = expand_derive_decode_body(input_remain2);
    assert!(!output2.is_empty());

    // 3. Struct with raw Body remain and non-path remain / remain_attrs
    let input_body_remain: DeriveInput = syn::parse_quote! {
        struct ConfigRawRemain {
            #[hcl(remain)]
            raw_remain: (u32, u32),
            #[hcl(remain_attrs)]
            raw_attrs: (u32, u32),
        }
    };
    let output_body = expand_derive_decode_body(input_body_remain);
    assert!(!output_body.is_empty());

    // 4. Unit struct (non-named fields)
    let unit_input: DeriveInput = syn::parse_quote! {
        struct UnitConfig;
    };
    let unit_output = expand_derive_decode_body(unit_input);
    assert!(!unit_output.is_empty());

    // 5. Tuple struct
    let tuple_input: DeriveInput = syn::parse_quote! {
        struct TupleConfig(String, u32);
    };
    let tuple_output = expand_derive_decode_body(tuple_input);
    assert!(!tuple_output.is_empty());
}

/// Tests error diagnostics produced by `DecodeBody` during invalid usage.
#[test]
fn test_expand_decode_body_errors() {
    // 1. Label on non-string field (path and non-path)
    let bad_label: DeriveInput = syn::parse_quote! {
        struct BadLabel {
            #[hcl(label)]
            id: u32,
            #[hcl(label)]
            tuple_id: (u32, u32),
        }
    };
    let out_label = expand_derive_decode_body(bad_label);
    assert!(
        out_label
            .to_string()
            .contains("hcl(label) field must be of type String")
    );

    // 2. Duplicate remain fields
    let dup_remain: DeriveInput = syn::parse_quote! {
        struct DupRemain {
            #[hcl(remain)]
            r1: std::collections::HashMap<String, Value>,
            #[hcl(remain)]
            r2: std::collections::HashMap<String, Value>,
        }
    };
    let out_dup = expand_derive_decode_body(dup_remain);
    assert!(
        out_dup
            .to_string()
            .contains("hcl(remain) can only be applied to a single field")
    );

    // 3. Duplicate remain_attrs fields
    let dup_attrs: DeriveInput = syn::parse_quote! {
        struct DupAttrs {
            #[hcl(remain_attrs)]
            a1: std::collections::HashMap<String, Value>,
            #[hcl(remain_attrs)]
            a2: std::collections::HashMap<String, Value>,
        }
    };
    let out_attrs = expand_derive_decode_body(dup_attrs);
    assert!(
        out_attrs
            .to_string()
            .contains("hcl(remain_attrs) can only be applied to a single field")
    );

    // 4. Duplicate remain_blocks fields
    let dup_blocks: DeriveInput = syn::parse_quote! {
        struct DupBlocks {
            #[hcl(remain_blocks)]
            b1: Vec<Block>,
            #[hcl(remain_blocks)]
            b2: Vec<Block>,
        }
    };
    let out_blocks = expand_derive_decode_body(dup_blocks);
    assert!(
        out_blocks
            .to_string()
            .contains("hcl(remain_blocks) can only be applied to a single field")
    );

    // 5. Both attr and block on the same field
    let attr_and_block: DeriveInput = syn::parse_quote! {
        struct BothAttrBlock {
            #[hcl(attr, block)]
            item: String,
        }
    };
    let out_both = expand_derive_decode_body(attr_and_block);
    assert!(
        out_both
            .to_string()
            .contains("field cannot be marked as both #[hcl(attr)] and #[hcl(block)]")
    );

    // 6. Label and block on the same field
    let label_and_block: DeriveInput = syn::parse_quote! {
        struct BothLabelBlock {
            #[hcl(label, block)]
            item: String,
        }
    };
    let out_label_block = expand_derive_decode_body(label_and_block);
    assert!(
        out_label_block
            .to_string()
            .contains("field cannot be marked as #[hcl(label)] and block/attr")
    );

    // 7. Label and attr on the same field
    let label_and_attr: DeriveInput = syn::parse_quote! {
        struct BothLabelAttr {
            #[hcl(label, attr)]
            item: String,
        }
    };
    let out_label_attr = expand_derive_decode_body(label_and_attr);
    assert!(
        out_label_attr
            .to_string()
            .contains("field cannot be marked as #[hcl(label)] and block/attr")
    );

    // 8. Unsupported attribute name
    let unsupported: DeriveInput = syn::parse_quote! {
        struct BadAttr {
            #[hcl(invalid_option)]
            item: String,
        }
    };
    let out_unsupported = expand_derive_decode_body(unsupported);
    assert!(
        out_unsupported
            .to_string()
            .contains("unsupported hcl attribute")
    );
}

/// Tests expansion of `EncodeBody` for valid struct configurations and error cases.
#[test]
fn test_expand_encode_body_comprehensive() {
    let input: DeriveInput = syn::parse_quote! {
        struct FullEncode {
            #[hcl(label)]
            name: String,
            #[hcl(block)]
            vec_blocks: Vec<SubItem>,
            #[hcl(block)]
            opt_block: Option<SubItem>,
            #[hcl(block)]
            plain_block: SubItem,
            #[hcl(block)]
            non_path_block: (u32, u32),
            #[hcl(expr)]
            expr_field: Expression,
            #[hcl(expr)]
            opt_expr: Option<Expression>,
            #[hcl(expr)]
            non_path_expr: (u32, u32),
            #[hcl(remain)]
            rem: std::collections::HashMap<String, Value>,
            #[hcl(remain_attrs)]
            rem_attrs: std::collections::HashMap<String, Value>,
            #[hcl(remain_blocks)]
            rem_blocks: Vec<Block>,
            #[hcl(name = "custom_title")]
            title: Option<String>,
            count: u32,
            non_path_attr: (u32, u32),
        }
    };
    let output = expand_derive_encode_body(input);
    assert!(!output.is_empty());

    // Unit struct
    let unit_input: DeriveInput = syn::parse_quote! {
        struct EmptyEncode;
    };
    let unit_out = expand_derive_encode_body(unit_input);
    assert!(!unit_out.is_empty());

    // Struct with attribute error
    let bad_input: DeriveInput = syn::parse_quote! {
        struct BadEncode {
            #[hcl(attr, block)]
            bad: String,
        }
    };
    let bad_out = expand_derive_encode_body(bad_input);
    assert!(
        bad_out
            .to_string()
            .contains("field cannot be marked as both #[hcl(attr)] and #[hcl(block)]")
    );
}

/// Tests expansion of `ImpliedBodySchema` for various field types.
#[test]
fn test_expand_implied_body_schema_comprehensive() {
    let input: DeriveInput = syn::parse_quote! {
        struct FullSchema {
            #[hcl(label)]
            name: String,
            #[hcl(block)]
            vec_blocks: Vec<SubItem>,
            #[hcl(block)]
            opt_block: Option<SubItem>,
            #[hcl(block)]
            plain_block: SubItem,
            #[hcl(remain)]
            rem: std::collections::HashMap<String, Value>,
            #[hcl(remain_attrs)]
            rem_attrs: std::collections::HashMap<String, Value>,
            #[hcl(remain_blocks)]
            rem_blocks: Vec<Block>,
            #[hcl(optional)]
            opt_flag: bool,
            #[hcl(default)]
            def_flag: bool,
            #[hcl(name = "custom_schema_attr")]
            custom_attr: String,
            plain_option: Option<String>,
            required_attr: u32,
            non_path_attr: (u32, u32),
        }
    };
    let output = expand_derive_implied_body_schema(input);
    assert!(!output.is_empty());

    // Unit struct
    let unit_input: DeriveInput = syn::parse_quote! {
        struct EmptySchema;
    };
    let unit_out = expand_derive_implied_body_schema(unit_input);
    assert!(!unit_out.is_empty());
}

/// Tests expansion of `DecodeValue` for structs and enums, including error paths.
#[test]
fn test_expand_decode_value_comprehensive() {
    // 1. Struct with option, optional, default, required, non-path, and renamed fields
    let struct_input: DeriveInput = syn::parse_quote! {
        struct ValueStruct {
            opt_field: Option<String>,
            #[hcl(optional)]
            optional_field: u32,
            #[hcl(default)]
            default_field: bool,
            #[hcl(name = "custom_field")]
            renamed_field: String,
            required_field: f64,
            non_path_field: (u32, u32),
        }
    };
    let struct_out = expand_derive_decode_value(struct_input);
    assert!(!struct_out.is_empty());

    // 2. Enum with unit variants and custom names
    let enum_input: DeriveInput = syn::parse_quote! {
        enum Status {
            Active,
            #[hcl(name = "in_progress")]
            Pending,
            Inactive,
        }
    };
    let enum_out = expand_derive_decode_value(enum_input);
    assert!(!enum_out.to_string().contains("compile_error"));

    // 3. Enum with non-unit variant error
    let bad_enum: DeriveInput = syn::parse_quote! {
        enum BadEnum {
            Valid,
            Invalid(String),
        }
    };
    let bad_enum_out = expand_derive_decode_value(bad_enum);
    assert!(
        bad_enum_out
            .to_string()
            .contains("DecodeValue only supports unit enum variants")
    );

    // 4. Union error
    let union_input: DeriveInput = syn::parse_quote! {
        union MyUnion {
            f1: u32,
            f2: f32,
        }
    };
    let union_out = expand_derive_decode_value(union_input);
    assert!(
        union_out
            .to_string()
            .contains("DecodeValue can only be derived on structs or enums")
    );

    // 5. Enum with invalid attribute error
    let bad_attr_enum: DeriveInput = syn::parse_quote! {
        enum AttrEnum {
            #[hcl(unknown_variant_attr)]
            Variant,
        }
    };
    let bad_attr_out = expand_derive_decode_value(bad_attr_enum);
    assert!(
        bad_attr_out
            .to_string()
            .contains("unsupported hcl enum attribute")
    );

    // 6. Struct with attribute errors
    let bad_attr_struct: DeriveInput = syn::parse_quote! {
        struct BadAttrStruct {
            #[hcl(attr, block)]
            x: String,
        }
    };
    let bad_attr_struct_out = expand_derive_decode_value(bad_attr_struct);
    assert!(
        bad_attr_struct_out
            .to_string()
            .contains("field cannot be marked as both #[hcl(attr)] and #[hcl(block)]")
    );
}

/// Tests expansion of `EncodeValue` for structs and enums, including error paths.
#[test]
fn test_expand_encode_value_comprehensive() {
    // 1. Struct with option, required, and non-path fields
    let struct_input: DeriveInput = syn::parse_quote! {
        struct ValueStruct {
            opt_field: Option<String>,
            #[hcl(name = "custom_field")]
            renamed_field: String,
            required_field: f64,
            non_path_field: (u32, u32),
        }
    };
    let struct_out = expand_derive_encode_value(struct_input);
    assert!(!struct_out.is_empty());

    // 2. Enum with unit variants and custom names
    let enum_input: DeriveInput = syn::parse_quote! {
        enum Status {
            Active,
            #[hcl(name = "in_progress")]
            Pending,
            Inactive,
        }
    };
    let enum_out = expand_derive_encode_value(enum_input);
    assert!(!enum_out.is_empty());

    // 3. Union error
    let union_input: DeriveInput = syn::parse_quote! {
        union MyUnion {
            f1: u32,
            f2: f32,
        }
    };
    let union_out = expand_derive_encode_value(union_input);
    assert!(
        union_out
            .to_string()
            .contains("EncodeValue can only be derived on structs or enums")
    );

    // 4. Enum with non-unit variant error
    let bad_enum: DeriveInput = syn::parse_quote! {
        enum BadEnum {
            Valid,
            Invalid(String),
        }
    };
    let bad_enum_out = expand_derive_encode_value(bad_enum);
    assert!(
        bad_enum_out
            .to_string()
            .contains("EncodeValue only supports unit enum variants")
    );

    // 5. Enum with invalid attribute error
    let bad_attr_enum: DeriveInput = syn::parse_quote! {
        enum AttrEnum {
            #[hcl(unknown_variant_attr)]
            Variant,
        }
    };
    let bad_attr_out = expand_derive_encode_value(bad_attr_enum);
    assert!(
        bad_attr_out
            .to_string()
            .contains("unsupported hcl enum attribute")
    );

    // 6. Struct with attribute errors
    let bad_attr_struct: DeriveInput = syn::parse_quote! {
        struct BadAttrStruct {
            #[hcl(attr, block)]
            x: String,
        }
    };
    let bad_attr_struct_out = expand_derive_encode_value(bad_attr_struct);
    assert!(
        bad_attr_struct_out
            .to_string()
            .contains("field cannot be marked as both #[hcl(attr)] and #[hcl(block)]")
    );
}

/// Tests expansion of `CapsuleType` derive macro across valid configurations.
#[test]
fn test_expand_capsule_type_comprehensive() {
    // 1. Struct with custom type_name and conversions
    let input: DeriveInput = syn::parse_quote! {
        #[hcl(type_name = "custom_database", convert_to = "db_to_value", convert_from = "value_to_db")]
        struct DatabaseHandle {
            host: String,
            port: u16,
        }
    };
    let output = expand_derive_capsule_type(input);
    assert!(!output.is_empty());
    let code = output.to_string();
    assert!(code.contains("custom_database"));
    assert!(code.contains("capsule_ops"));
    assert!(code.contains("From < DatabaseHandle > for crate :: types :: val :: Value"));
    assert!(code.contains("db_to_value"));
    assert!(code.contains("value_to_db"));

    // 2. Struct with default type_name and no conversions
    let input_default: DeriveInput = syn::parse_quote! {
        struct PlainResource {
            id: u64,
        }
    };
    let out_default = expand_derive_capsule_type(input_default);
    let code_default = out_default.to_string();
    assert!(code_default.contains("PlainResource"));
    assert!(code_default.contains("capsule_ops"));

    // 3. Enum with CapsuleType
    let input_enum: DeriveInput = syn::parse_quote! {
        #[hcl(type_name = "connection_state")]
        enum ConnectionState {
            Disconnected,
            Connected,
        }
    };
    let out_enum = expand_derive_capsule_type(input_enum);
    let code_enum = out_enum.to_string();
    assert!(code_enum.contains("connection_state"));
    assert!(code_enum.contains("ConnectionState"));

    // 4. Unit struct
    let input_unit: DeriveInput = syn::parse_quote! {
        struct EmptyCapsule;
    };
    let out_unit = expand_derive_capsule_type(input_unit);
    assert!(!out_unit.is_empty());

    // 5. Struct with unquoted convert_to and convert_from paths
    let input_unquoted: DeriveInput = syn::parse_quote! {
        #[hcl(convert_to = my_convert_fn, convert_from = my_from_fn)]
        struct UnquotedConversions {
            data: u32,
        }
    };
    let out_unquoted = expand_derive_capsule_type(input_unquoted);
    let code_unquoted = out_unquoted.to_string();
    assert!(code_unquoted.contains("my_convert_fn"));
    assert!(code_unquoted.contains("my_from_fn"));
}

/// Tests error handling in `CapsuleType` derive macro.
#[test]
fn test_expand_capsule_type_errors() {
    // 1. Generic struct (unsupported)
    let bad_generic: DeriveInput = syn::parse_quote! {
        struct GenericCapsule<T> {
            data: T,
        }
    };
    let err_generic = expand_derive_capsule_type(bad_generic);
    assert!(
        err_generic
            .to_string()
            .contains("CapsuleType does not support generic type parameters")
    );

    // 2. Unsupported union
    let bad_union: DeriveInput = syn::parse_quote! {
        union MyUnion {
            f1: u32,
            f2: f32,
        }
    };
    let err_union = expand_derive_capsule_type(bad_union);
    assert!(
        err_union
            .to_string()
            .contains("CapsuleType can only be derived on structs or enums")
    );

    // 3. Invalid attribute
    let bad_attr: DeriveInput = syn::parse_quote! {
        #[hcl(invalid_capsule_attr = "foo")]
        struct BadAttrCapsule {
            id: u32,
        }
    };
    let err_attr = expand_derive_capsule_type(bad_attr);
    assert!(
        err_attr
            .to_string()
            .contains("unsupported hcl capsule attribute")
    );
}

/// Tests handling of non-HCL attributes across all macros.
#[test]
fn test_non_hcl_attributes_coverage() {
    // 1. Container with non-HCL attribute in CapsuleType
    let capsule_input: DeriveInput = syn::parse_quote! {
        #[doc = "container doc"]
        #[inline]
        struct DocCapsule {
            id: u32,
        }
    };
    assert!(!expand_derive_capsule_type(capsule_input).is_empty());

    // 2. Enum variants with non-HCL attribute in DecodeValue and EncodeValue
    let enum_input: DeriveInput = syn::parse_quote! {
        enum DocEnum {
            #[doc = "variant doc"]
            #[inline]
            FirstVariant,
            #[hcl(name = "second_custom")]
            SecondVariant,
        }
    };
    assert!(!expand_derive_decode_value(enum_input.clone()).is_empty());
    assert!(!expand_derive_encode_value(enum_input).is_empty());

    // 3. Struct fields with non-HCL attribute in DecodeBody and EncodeBody
    let struct_input: DeriveInput = syn::parse_quote! {
        struct DocStruct {
            #[doc = "field doc"]
            #[inline]
            valid_field: String,
        }
    };
    assert!(!expand_derive_decode_body(struct_input.clone()).is_empty());
    assert!(!expand_derive_encode_body(struct_input.clone()).is_empty());
    assert!(!expand_derive_implied_body_schema(struct_input).is_empty());
}

/// Tests handling of synthetic fields with `ident: None` in named fields.
#[test]
fn test_named_field_without_ident_coverage() {
    let mut fields = syn::punctuated::Punctuated::new();
    fields.push(syn::Field {
        attrs: Vec::new(),
        vis: syn::Visibility::Inherited,
        mutability: syn::FieldMutability::None,
        ident: Some(syn::Ident::new(
            "valid_field",
            proc_macro2::Span::call_site(),
        )),
        colon_token: Some(syn::token::Colon::default()),
        ty: syn::parse_quote!(String),
    });
    fields.push(syn::Field {
        attrs: Vec::new(),
        vis: syn::Visibility::Inherited,
        mutability: syn::FieldMutability::None,
        ident: None,
        colon_token: None,
        ty: syn::parse_quote!(u32),
    });

    let input = syn::DeriveInput {
        attrs: Vec::new(),
        vis: syn::Visibility::Inherited,
        ident: syn::Ident::new("SyntheticStruct", proc_macro2::Span::call_site()),
        generics: syn::Generics::default(),
        data: syn::Data::Struct(syn::DataStruct {
            struct_token: syn::token::Struct::default(),
            fields: syn::Fields::Named(syn::FieldsNamed {
                brace_token: syn::token::Brace::default(),
                named: fields,
            }),
            semi_token: None,
        }),
    };

    assert!(!expand_derive_decode_body(input.clone()).is_empty());
    assert!(!expand_derive_encode_body(input.clone()).is_empty());
    assert!(!expand_derive_implied_body_schema(input.clone()).is_empty());
    assert!(!expand_derive_decode_value(input.clone()).is_empty());
    assert!(!expand_derive_encode_value(input).is_empty());
}

/// Tests attribute error paths in `DecodeValue`, `EncodeValue`, and `CapsuleType`.
#[test]
fn test_macro_attribute_error_coverage() {
    // 1. DecodeValue enum variant name errors
    let bad_val_name: DeriveInput = syn::parse_quote! {
        enum BadNameEnum {
            #[hcl(name)]
            First,
        }
    };
    assert!(
        !expand_derive_decode_value(bad_val_name)
            .to_string()
            .is_empty()
    );

    let bad_val_lit: DeriveInput = syn::parse_quote! {
        enum BadLitEnum {
            #[hcl(name = 123)]
            First,
        }
    };
    assert!(
        !expand_derive_decode_value(bad_val_lit)
            .to_string()
            .is_empty()
    );

    // 2. EncodeValue enum variant name errors
    let bad_enc_name: DeriveInput = syn::parse_quote! {
        enum BadEncNameEnum {
            #[hcl(name)]
            First,
        }
    };
    assert!(
        !expand_derive_encode_value(bad_enc_name)
            .to_string()
            .is_empty()
    );

    let bad_enc_lit: DeriveInput = syn::parse_quote! {
        enum BadEncLitEnum {
            #[hcl(name = 123)]
            First,
        }
    };
    assert!(
        !expand_derive_encode_value(bad_enc_lit)
            .to_string()
            .is_empty()
    );

    // 3. CapsuleType type_name, convert_to, convert_from errors
    let cap_missing_type_name: DeriveInput = syn::parse_quote! {
        #[hcl(type_name)]
        struct CapTypeNameMissing { id: u32 }
    };
    assert!(
        !expand_derive_capsule_type(cap_missing_type_name)
            .to_string()
            .is_empty()
    );

    let cap_bad_type_name: DeriveInput = syn::parse_quote! {
        #[hcl(type_name = 123)]
        struct CapTypeNameBad { id: u32 }
    };
    assert!(
        !expand_derive_capsule_type(cap_bad_type_name)
            .to_string()
            .is_empty()
    );

    let cap_missing_to: DeriveInput = syn::parse_quote! {
        #[hcl(convert_to)]
        struct CapToMissing { id: u32 }
    };
    assert!(
        !expand_derive_capsule_type(cap_missing_to)
            .to_string()
            .is_empty()
    );

    let cap_bad_to_lit: DeriveInput = syn::parse_quote! {
        #[hcl(convert_to = ":::invalid_path:::")]
        struct CapToBadLit { id: u32 }
    };
    assert!(
        !expand_derive_capsule_type(cap_bad_to_lit)
            .to_string()
            .is_empty()
    );

    let cap_bad_to_unquoted: DeriveInput = syn::parse_quote! {
        #[hcl(convert_to = 123)]
        struct CapToBadUnquoted { id: u32 }
    };
    assert!(
        !expand_derive_capsule_type(cap_bad_to_unquoted)
            .to_string()
            .is_empty()
    );

    let cap_missing_from: DeriveInput = syn::parse_quote! {
        #[hcl(convert_from)]
        struct CapFromMissing { id: u32 }
    };
    assert!(
        !expand_derive_capsule_type(cap_missing_from)
            .to_string()
            .is_empty()
    );

    let cap_bad_from_lit: DeriveInput = syn::parse_quote! {
        #[hcl(convert_from = ":::invalid_path:::")]
        struct CapFromBadLit { id: u32 }
    };
    assert!(
        !expand_derive_capsule_type(cap_bad_from_lit)
            .to_string()
            .is_empty()
    );

    let cap_bad_from_unquoted: DeriveInput = syn::parse_quote! {
        #[hcl(convert_from = 123)]
        struct CapFromBadUnquoted { id: u32 }
    };
    assert!(
        !expand_derive_capsule_type(cap_bad_from_unquoted)
            .to_string()
            .is_empty()
    );
}

/// Tests expansion of `DecodeBody`, `EncodeBody`, and `ImpliedBodySchema` for label-keyed block maps.
#[test]
fn test_expand_label_keyed_block_maps_comprehensive() {
    // 1. Single-level label map (HashMap and BTreeMap)
    let input_single: DeriveInput = syn::parse_quote! {
        struct SingleMapConfig {
            #[hcl(block)]
            servers: std::collections::HashMap<String, ServerConfig>,
            #[hcl(block)]
            databases: std::collections::BTreeMap<String, DbConfig>,
            #[hcl(block)]
            repeated_servers: std::collections::HashMap<String, Vec<ServerConfig>>,
        }
    };
    let decode_out = expand_derive_decode_body(input_single.clone());
    assert!(!decode_out.is_empty());
    let encode_out = expand_derive_encode_body(input_single.clone());
    assert!(!encode_out.is_empty());
    let schema_out = expand_derive_implied_body_schema(input_single);
    assert!(!schema_out.is_empty());

    // 2. Multi-level label maps (2-level and 3-level, with Single and Vec terminals)
    let input_multi: DeriveInput = syn::parse_quote! {
        struct MultiMapConfig {
            #[hcl(block)]
            resources: std::collections::HashMap<String, std::collections::HashMap<String, ResourceConfig>>,
            #[hcl(block)]
            multi_repeated: std::collections::HashMap<String, std::collections::BTreeMap<String, Vec<ResourceConfig>>>,
            #[hcl(block)]
            three_level: std::collections::HashMap<String, std::collections::HashMap<String, std::collections::HashMap<String, LeafConfig>>>,
            #[hcl(block)]
            four_level: std::collections::BTreeMap<String, std::collections::BTreeMap<String, std::collections::BTreeMap<String, std::collections::BTreeMap<String, LeafConfig>>>>,
        }
    };
    let decode_multi_out = expand_derive_decode_body(input_multi.clone());
    assert!(!decode_multi_out.is_empty());
    let encode_multi_out = expand_derive_encode_body(input_multi.clone());
    assert!(!encode_multi_out.is_empty());
    let schema_multi_out = expand_derive_implied_body_schema(input_multi);
    assert!(!schema_multi_out.is_empty());
}

/// Tests expansion of `DecodeBody`, `EncodeBody`, and `ImpliedBodySchema` for flattened and squashed structs.
#[test]
fn test_expand_flatten_and_squash_comprehensive() {
    let input: DeriveInput = syn::parse_quote! {
        struct ParentConfig {
            #[hcl(attr)]
            parent_id: String,
            #[hcl(flatten)]
            child: ChildConfig,
            #[hcl(squash)]
            squashed_child: Option<AnotherChildConfig>,
        }
    };

    let decode_out = expand_derive_decode_body(input.clone());
    assert!(!decode_out.is_empty());
    let encode_out = expand_derive_encode_body(input.clone());
    assert!(!encode_out.is_empty());
    let schema_out = expand_derive_implied_body_schema(input);
    assert!(!schema_out.is_empty());
}

/// Tests compile-time error detection for duplicate declared attributes, blocks, and invalid flatten attributes.
#[test]
fn test_expand_flatten_and_collision_errors() {
    // 1. Duplicate attribute names on struct
    let dup_attr: DeriveInput = syn::parse_quote! {
        struct DupAttrConfig {
            #[hcl(attr, name = "port")]
            port_a: u16,
            #[hcl(attr, name = "port")]
            port_b: u16,
        }
    };
    let out_dup_attr = expand_derive_decode_body(dup_attr);
    assert!(
        out_dup_attr
            .to_string()
            .contains("duplicate attribute/block name 'port' declared on struct")
    );

    // 2. Duplicate block names on struct
    let dup_block: DeriveInput = syn::parse_quote! {
        struct DupBlockConfig {
            #[hcl(block, name = "server")]
            s1: Vec<Server>,
            #[hcl(block, name = "server")]
            s2: Vec<Server>,
        }
    };
    let out_dup_block = expand_derive_decode_body(dup_block);
    assert!(
        out_dup_block
            .to_string()
            .contains("duplicate attribute/block name 'server' declared on struct")
    );

    // 3. Field marked with both flatten and attr
    let bad_flatten: DeriveInput = syn::parse_quote! {
        struct BadFlattenConfig {
            #[hcl(flatten, attr)]
            conflict: ChildConfig,
        }
    };
    let out_bad_flatten = expand_derive_decode_body(bad_flatten);
    assert!(
        out_bad_flatten
            .to_string()
            .contains("field cannot be marked as both #[hcl(flatten)] and another HCL attribute")
    );
}

/// Tests expansion of `DecodeValue` and `EncodeValue` for flattened fields.
#[test]
fn test_expand_flatten_value_comprehensive() {
    let input: DeriveInput = syn::parse_quote! {
        struct FlattenValueConfig {
            #[hcl(attr)]
            id: String,
            #[hcl(flatten)]
            inner: InnerValueConfig,
            #[hcl(squash)]
            opt_inner: Option<OptValueConfig>,
        }
    };

    let decode_val_out = expand_derive_decode_value(input.clone());
    assert!(!decode_val_out.is_empty());
    let encode_val_out = expand_derive_encode_value(input);
    assert!(!encode_val_out.is_empty());
}

/// Tests expansion of `DecodeBody`, `EncodeBody`, and `ImpliedBodySchema` with `#[hcl(body)]`, `#[hcl(with)]`, `#[hcl(default_expr)]`, and `#[hcl(default_fn)]`.
#[test]
fn test_expand_body_retention_and_custom_hooks_comprehensive() {
    let input: DeriveInput = syn::parse_quote! {
        struct HookedConfig {
            #[hcl(attr)]
            name: String,
            #[hcl(body)]
            raw_body: crate::ast::structure::Body,
            #[hcl(attr, with = "my_custom_decoder")]
            custom: CustomType,
            #[hcl(attr, default_expr = "8080")]
            port: u16,
            #[hcl(attr, default_fn = "default_cluster_name")]
            cluster: String,
        }
    };

    let decode_out = expand_derive_decode_body(input.clone());
    assert!(!decode_out.is_empty());
    let encode_out = expand_derive_encode_body(input.clone());
    assert!(!encode_out.is_empty());
    let schema_out = expand_derive_implied_body_schema(input);
    assert!(!schema_out.is_empty());
}

/// Tests compile-time error detection for invalid `body`, `default_expr`, and `default_fn` usage.
#[test]
fn test_expand_body_and_hook_errors() {
    // 1. Multiple body fields
    let dup_body: DeriveInput = syn::parse_quote! {
        struct DupBodyConfig {
            #[hcl(body)]
            b1: crate::ast::structure::Body,
            #[hcl(body)]
            b2: crate::ast::structure::Body,
        }
    };
    let out_dup_body = expand_derive_decode_body(dup_body);
    assert!(
        out_dup_body
            .to_string()
            .contains("hcl(body) can only be applied to a single field")
    );

    // 2. Both default_expr and default_fn on same field
    let both_defaults: DeriveInput = syn::parse_quote! {
        struct BothDefaultsConfig {
            #[hcl(attr, default_expr = "42", default_fn = "default_fn")]
            val: u32,
        }
    };
    let out_both = expand_derive_decode_body(both_defaults);
    assert!(
        out_both
            .to_string()
            .contains("field cannot specify both #[hcl(default_expr)] and #[hcl(default_fn)]")
    );

    // 3. Body marked with another attribute
    let body_conflict: DeriveInput = syn::parse_quote! {
        struct BodyConflictConfig {
            #[hcl(body, attr)]
            conflict: crate::ast::structure::Body,
        }
    };
    let out_conflict = expand_derive_decode_body(body_conflict);
    assert!(
        out_conflict
            .to_string()
            .contains("field cannot be marked as both #[hcl(body)] and another HCL attribute")
    );

    // 4. Custom decoder on a block field
    let block_with: DeriveInput = syn::parse_quote! {
        struct BlockWithConfig {
            #[hcl(block, with = "custom_dec")]
            service: ServiceConfig,
        }
    };
    let out_block_with = expand_derive_decode_body(block_with);
    assert!(
        out_block_with
            .to_string()
            .contains("#[hcl(with)], #[hcl(default_expr)], and #[hcl(default_fn)] can only be applied to attribute fields")
    );
}

/// Tests remaining edge cases, container types, and attribute conflicts across derive macros.
#[test]
fn test_macro_edge_cases_and_attribute_conflicts() {
    // 1. Optional body and default_expr / default_fn on Option fields
    let input_opts: DeriveInput = syn::parse_quote! {
        struct OptsConfig {
            #[hcl(body)]
            opt_body: Option<crate::ast::structure::Body>,
            #[hcl(attr, default_expr = "10")]
            opt_num: Option<u32>,
            #[hcl(attr, default_fn = "def_str")]
            opt_str: Option<String>,
            #[hcl(flatten)]
            tuple_flatten: (ChildA, u32),
            #[hcl(expr)]
            opt_tuple_expr: Option<(u32, u32)>,
            #[hcl(block)]
            bad_map: std::collections::HashMap<'static, String>,
        }
    };
    let dec_opts = expand_derive_decode_body(input_opts.clone());
    assert!(!dec_opts.to_string().is_empty());
    let enc_opts = expand_derive_encode_body(input_opts.clone());
    assert!(!enc_opts.to_string().is_empty());
    let schema_opts = expand_derive_implied_body_schema(input_opts);
    assert!(!schema_opts.to_string().is_empty());

    // Non-path type for body field
    let input_tuple_body: DeriveInput = syn::parse_quote! {
        struct TupleBodyConfig {
            #[hcl(body)]
            tuple_body: (crate::ast::structure::Body, u32),
        }
    };
    assert!(
        !expand_derive_decode_body(input_tuple_body.clone())
            .to_string()
            .is_empty()
    );
    assert!(
        !expand_derive_encode_body(input_tuple_body)
            .to_string()
            .is_empty()
    );

    // 2. Duplicate expr field name
    let dup_expr: DeriveInput = syn::parse_quote! {
        struct DupExprConfig {
            #[hcl(expr, name = "rule")]
            r1: Expression,
            #[hcl(expr, name = "rule")]
            r2: Expression,
        }
    };
    assert!(
        expand_derive_decode_body(dup_expr)
            .to_string()
            .contains("duplicate attribute/block name 'rule'")
    );

    // 3. Invalid syntax for with and default_fn paths
    let bad_paths: DeriveInput = syn::parse_quote! {
        struct BadPathsConfig {
            #[hcl(attr, with = "invalid:::path")]
            f1: String,
            #[hcl(attr, default_fn = "invalid:::fn")]
            f2: String,
        }
    };
    let bad_paths_out = expand_derive_decode_body(bad_paths).to_string();
    assert!(bad_paths_out.contains("invalid path for custom decoder"));
    assert!(bad_paths_out.contains("invalid path for default_fn"));

    // 4. Body conflicting with block, label, remain, remain_attrs, remain_blocks, expr, flatten
    let body_conflicts: DeriveInput = syn::parse_quote! {
        struct BodyConflicts {
            #[hcl(body, block)]
            b: Body,
            #[hcl(body, label)]
            l: Body,
            #[hcl(body, remain)]
            r: Body,
            #[hcl(body, remain_attrs)]
            ra: Body,
            #[hcl(body, remain_blocks)]
            rb: Body,
            #[hcl(body, expr)]
            e: Body,
            #[hcl(body, flatten)]
            f: Body,
        }
    };
    assert!(
        expand_derive_decode_body(body_conflicts)
            .to_string()
            .contains("field cannot be marked as both #[hcl(body)]")
    );

    // 5. with/default on label, remain, remain_attrs, remain_blocks, flatten, body
    let with_conflicts: DeriveInput = syn::parse_quote! {
        struct WithConflicts {
            #[hcl(label, with = "foo")]
            l: String,
            #[hcl(remain, with = "foo")]
            r: HashMap<String, Value>,
            #[hcl(remain_attrs, default_expr = "1")]
            ra: HashMap<String, Value>,
            #[hcl(remain_blocks, default_fn = "foo")]
            rb: Vec<Block>,
            #[hcl(flatten, with = "foo")]
            f: Child,
            #[hcl(body, with = "foo")]
            by: Body,
        }
    };
    assert!(
        expand_derive_decode_body(with_conflicts)
            .to_string()
            .contains("can only be applied to attribute fields")
    );

    // 6. flatten conflicting with block, label, remain, remain_attrs, remain_blocks, expr, body
    let flatten_conflicts: DeriveInput = syn::parse_quote! {
        struct FlattenConflicts {
            #[hcl(flatten, block)]
            b: Child,
            #[hcl(flatten, label)]
            l: Child,
            #[hcl(flatten, remain)]
            r: Child,
            #[hcl(flatten, remain_attrs)]
            ra: Child,
            #[hcl(flatten, remain_blocks)]
            rb: Child,
            #[hcl(flatten, expr)]
            e: Child,
            #[hcl(flatten, body)]
            by: Child,
        }
    };
    assert!(
        expand_derive_decode_body(flatten_conflicts)
            .to_string()
            .contains("field cannot be marked as both #[hcl(flatten)]")
    );

    // 7. Missing value or non-string literals for meta pairs (with, default_expr, default_fn, name)
    let meta_invalid_forms: DeriveInput = syn::parse_quote! {
        struct MetaInvalidForms {
            #[hcl(attr, with)]
            f1: String,
            #[hcl(attr, with = 123)]
            f2: String,
            #[hcl(attr, default_expr)]
            f3: String,
            #[hcl(attr, default_expr = 123)]
            f4: String,
            #[hcl(attr, default_fn)]
            f5: String,
            #[hcl(attr, default_fn = 123)]
            f6: String,
            #[hcl(attr, name)]
            f7: String,
            #[hcl(attr, name = 123)]
            f8: String,
        }
    };
    assert!(
        expand_derive_decode_body(meta_invalid_forms)
            .to_string()
            .contains("unsupported hcl attribute")
    );

    // 8. Unbracketed containers and lifetime arguments for extract_generic_inner_type
    let containers_input: DeriveInput = syn::parse_quote! {
        struct ContainerForms {
            #[hcl(flatten)]
            vec_child: Vec<ChildA>,
            #[hcl(flatten)]
            hashmap_child: std::collections::HashMap<String, ChildA>,
            #[hcl(flatten)]
            btreemap_child: std::collections::BTreeMap<String, ChildA>,
            #[hcl(flatten)]
            raw_vec: Vec,
            #[hcl(flatten)]
            raw_opt: Option,
            #[hcl(flatten)]
            raw_map: std::collections::HashMap,
            #[hcl(flatten)]
            lifetime_vec: Vec<'static>,
            #[hcl(flatten)]
            lifetime_map: std::collections::HashMap<'static, String>,
            #[hcl(expr)]
            raw_opt_expr: Option,
            #[hcl(expr)]
            arc_opt_expr: Option<std::sync::Arc<Expression>>,
            #[hcl(block)]
            raw_vec_blk: Vec,
            #[hcl(block)]
            raw_opt_blk: Option,
            #[hcl(block)]
            raw_map_blk: std::collections::HashMap,
        }
    };
    let cont_dec = expand_derive_decode_body(containers_input.clone()).to_string();
    assert!(!cont_dec.is_empty());
    let cont_enc = expand_derive_encode_body(containers_input.clone()).to_string();
    assert!(!cont_enc.is_empty());
    let cont_schema = expand_derive_implied_body_schema(containers_input).to_string();
    assert!(!cont_schema.is_empty());

    // 9. Synthetic fields with empty Path segments
    let empty_path_field: syn::Field = syn::Field {
        attrs: vec![syn::parse_quote!(#[hcl(expr)])],
        vis: syn::Visibility::Inherited,
        mutability: syn::FieldMutability::None,
        ident: Some(syn::Ident::new(
            "empty_path",
            proc_macro2::Span::call_site(),
        )),
        colon_token: Some(syn::token::Colon::default()),
        ty: syn::Type::Path(syn::TypePath {
            qself: None,
            path: syn::Path {
                leading_colon: None,
                segments: syn::punctuated::Punctuated::new(),
            },
        }),
    };
    let empty_path_block: syn::Field = syn::Field {
        attrs: vec![syn::parse_quote!(#[hcl(block)])],
        vis: syn::Visibility::Inherited,
        mutability: syn::FieldMutability::None,
        ident: Some(syn::Ident::new(
            "empty_path_blk",
            proc_macro2::Span::call_site(),
        )),
        colon_token: Some(syn::token::Colon::default()),
        ty: syn::Type::Path(syn::TypePath {
            qself: None,
            path: syn::Path {
                leading_colon: None,
                segments: syn::punctuated::Punctuated::new(),
            },
        }),
    };
    let empty_path_flatten: syn::Field = syn::Field {
        attrs: vec![syn::parse_quote!(#[hcl(flatten)])],
        vis: syn::Visibility::Inherited,
        mutability: syn::FieldMutability::None,
        ident: Some(syn::Ident::new(
            "empty_path_flat",
            proc_macro2::Span::call_site(),
        )),
        colon_token: Some(syn::token::Colon::default()),
        ty: syn::Type::Path(syn::TypePath {
            qself: None,
            path: syn::Path {
                leading_colon: None,
                segments: syn::punctuated::Punctuated::new(),
            },
        }),
    };
    let input_empty_paths = DeriveInput {
        attrs: vec![],
        vis: syn::Visibility::Inherited,
        ident: syn::Ident::new("EmptyPathStruct", proc_macro2::Span::call_site()),
        generics: syn::Generics::default(),
        data: syn::Data::Struct(syn::DataStruct {
            struct_token: syn::token::Struct::default(),
            fields: syn::Fields::Named(syn::FieldsNamed {
                brace_token: syn::token::Brace::default(),
                named: syn::punctuated::Punctuated::from_iter([
                    empty_path_field,
                    empty_path_block,
                    empty_path_flatten,
                ]),
            }),
            semi_token: None,
        }),
    };
    assert!(
        !expand_derive_decode_body(input_empty_paths.clone())
            .to_string()
            .is_empty()
    );
    assert!(
        !expand_derive_encode_body(input_empty_paths.clone())
            .to_string()
            .is_empty()
    );
    assert!(
        !expand_derive_implied_body_schema(input_empty_paths)
            .to_string()
            .is_empty()
    );
}
