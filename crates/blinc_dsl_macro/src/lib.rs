//! `#[extern_widget]` — Rust → Blinc DSL widget export.
//!
//! Generates the JIT thunk decoding FFI args + the `ExternWidget`
//! trait impl carrying the spec. Re-exported from `blinc_dsl_core`
//! so users only need one import.

use proc_macro::TokenStream;
use quote::quote;
use syn::parse_macro_input;

/// FFI / decode / DSL-type tuple for one widget prop. Built from a
/// `syn::Type` by [`classify_param_type`].
struct ParamKind {
    ffi_ty: proc_macro2::TokenStream,
    decode: proc_macro2::TokenStream,
    prop_type_expr: proc_macro2::TokenStream,
    param_type_expr: proc_macro2::TokenStream,
}

/// Recognise `Reactive<T>` as a structural shape — returns `Some(T_ident)`
/// when the field's declared type is the `Reactive<…>` wrapper from
/// `blinc_runtime::reactive_value`. The macro generates a two-slot
/// FFI for these (`tag: i32`, `payload: i64`) instead of the
/// single-arg shape used for plain scalar props.
///
/// Matched on the rightmost segment only — `Reactive<f64>`,
/// `reactive_value::Reactive<f64>`, `blinc_runtime::Reactive<f64>`
/// all match. The inner type identifier is what selects the runtime
/// decoder; one of the per-`T` `Reactive::<T>::decode_ffi` impls in
/// `blinc_runtime::reactive_value`.
fn classify_reactive_field(ty: &syn::Type) -> Option<syn::Ident> {
    let syn::Type::Path(p) = ty else {
        return None;
    };
    let segment = p.path.segments.last()?;
    if segment.ident != "Reactive" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };
    if args.args.len() != 1 {
        return None;
    }
    let syn::GenericArgument::Type(inner) = &args.args[0] else {
        return None;
    };
    let syn::Type::Path(inner_path) = inner else {
        return None;
    };
    let inner_seg = inner_path.path.segments.last()?;
    if !matches!(inner_seg.arguments, syn::PathArguments::None) {
        return None;
    }
    Some(inner_seg.ident.clone())
}

/// Recognise `Vec<T>` — returns `Some(T_ident)` for a collection prop.
///
/// Matched on the rightmost segment, like [`classify_reactive_field`],
/// so `Vec<String>` and `std::vec::Vec<String>` both match.
fn classify_vec_field(ty: &syn::Type) -> Option<syn::Ident> {
    let syn::Type::Path(p) = ty else {
        return None;
    };
    let segment = p.path.segments.last()?;
    if segment.ident != "Vec" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };
    if args.args.len() != 1 {
        return None;
    }
    let syn::GenericArgument::Type(inner) = &args.args[0] else {
        return None;
    };
    let syn::Type::Path(inner_path) = inner else {
        return None;
    };
    let inner_seg = inner_path.path.segments.last()?;
    if !matches!(inner_seg.arguments, syn::PathArguments::None) {
        return None;
    }
    Some(inner_seg.ident.clone())
}

/// The FFI shape for a `Vec<T>` prop.
///
/// One `i64` slot: a pointer to Zyntax's `List<T> { data, len,
/// capacity }`. The element stride is NOT uniform — `bool` is one byte,
/// `i32` four, `i64` / `f64` / any pointer eight — so the decoder is
/// picked from `T` rather than assumed. `String` needs two
/// indirections: an 8-byte element pointer, then a length-prefixed
/// buffer.
fn vec_param_kind(inner: &syn::Ident) -> Result<ParamKind, String> {
    let decode = match inner.to_string().as_str() {
        "String" => quote! {
            unsafe {
                ::blinc_dsl_core::__extern_widget_internals::read_string_list(
                    ::blinc_dsl_core::__extern_widget_internals::decode_list(__arg)
                )
            }
        },
        // Stride comes from the Rust element type, which has to match
        // what the compiler inferred on the DSL side.
        "bool" | "i32" | "i64" | "f64" => quote! {
            unsafe {
                ::blinc_dsl_core::__extern_widget_internals::read_list_elements::<#inner>(
                    ::blinc_dsl_core::__extern_widget_internals::decode_list(__arg)
                )
            }
        },
        other => {
            return Err(format!(
                "#[extern_widget] Vec<{other}> isn't supported yet — only Vec<String>, \
                 Vec<bool>, Vec<i32>, Vec<i64> and Vec<f64>. A list of structs needs the \
                 element layout, which is a follow-up."
            ));
        }
    };
    Ok(ParamKind {
        ffi_ty: quote! { i64 },
        decode,
        // The DSL side is inferred: the compiler types the literal from
        // its elements, so the prop advertises a list of the matching
        // primitive rather than pinning a nominal type here.
        prop_type_expr: quote! {
            ::blinc_dsl_core::__extern_widget_internals::Type::Primitive(
                ::blinc_dsl_core::__extern_widget_internals::PrimitiveType::I64
            )
        },
        param_type_expr: quote! {
            ::blinc_dsl_core::__extern_widget_internals::Type::Primitive(
                ::blinc_dsl_core::__extern_widget_internals::PrimitiveType::I64
            )
        },
    })
}

fn classify_param_type(ty: &syn::Type) -> Option<ParamKind> {
    let syn::Type::Path(p) = ty else {
        return None;
    };
    let segment = p.path.segments.last()?;
    if !matches!(segment.arguments, syn::PathArguments::None) {
        return None;
    }
    let ident = &segment.ident;
    match ident.to_string().as_str() {
        "String" => Some(ParamKind {
            ffi_ty: quote! { *const i32 },
            decode: quote! {
                // SAFETY: registered signature pins `String` here so the JIT
                // hands us a length-prefixed UTF-8 buffer.
                unsafe { ::blinc_dsl_core::__extern_widget_internals::decode_string(__arg) }
            },
            prop_type_expr: quote! {
                ::blinc_dsl_core::__extern_widget_internals::Type::Primitive(
                    ::blinc_dsl_core::__extern_widget_internals::PrimitiveType::String
                )
            },
            param_type_expr: quote! {
                ::blinc_dsl_core::__extern_widget_internals::Type::Primitive(
                    ::blinc_dsl_core::__extern_widget_internals::PrimitiveType::String
                )
            },
        }),
        "i32" => Some(ParamKind {
            ffi_ty: quote! { i32 },
            decode: quote! { __arg },
            prop_type_expr: quote! {
                ::blinc_dsl_core::__extern_widget_internals::Type::Primitive(
                    ::blinc_dsl_core::__extern_widget_internals::PrimitiveType::I32
                )
            },
            param_type_expr: quote! {
                ::blinc_dsl_core::__extern_widget_internals::Type::Primitive(
                    ::blinc_dsl_core::__extern_widget_internals::PrimitiveType::I32
                )
            },
        }),
        "bool" => Some(ParamKind {
            ffi_ty: quote! { i32 },
            decode: quote! { __arg != 0 },
            prop_type_expr: quote! {
                ::blinc_dsl_core::__extern_widget_internals::Type::Primitive(
                    ::blinc_dsl_core::__extern_widget_internals::PrimitiveType::Bool
                )
            },
            param_type_expr: quote! {
                ::blinc_dsl_core::__extern_widget_internals::Type::Primitive(
                    ::blinc_dsl_core::__extern_widget_internals::PrimitiveType::I32
                )
            },
        }),
        "i64" => Some(ParamKind {
            ffi_ty: quote! { i64 },
            decode: quote! { __arg },
            prop_type_expr: quote! {
                ::blinc_dsl_core::__extern_widget_internals::Type::Primitive(
                    ::blinc_dsl_core::__extern_widget_internals::PrimitiveType::I64
                )
            },
            param_type_expr: quote! {
                ::blinc_dsl_core::__extern_widget_internals::Type::Primitive(
                    ::blinc_dsl_core::__extern_widget_internals::PrimitiveType::I64
                )
            },
        }),
        "f64" => Some(ParamKind {
            ffi_ty: quote! { f64 },
            decode: quote! { __arg },
            prop_type_expr: quote! {
                ::blinc_dsl_core::__extern_widget_internals::Type::Primitive(
                    ::blinc_dsl_core::__extern_widget_internals::PrimitiveType::F64
                )
            },
            param_type_expr: quote! {
                ::blinc_dsl_core::__extern_widget_internals::Type::Primitive(
                    ::blinc_dsl_core::__extern_widget_internals::PrimitiveType::F64
                )
            },
        }),
        "Vec" | "Box" | "Option" => None,
        _ => {
            let dsl_type_name = ident.to_string();
            Some(ParamKind {
                ffi_ty: quote! { i64 },
                decode: quote! {
                    {
                        // SAFETY: complex DSL struct props are lowered to
                        // `__new_struct_value__` handles before this thunk is called.
                        let __value = unsafe {
                            ::blinc_dsl_core::__extern_widget_internals::decode_struct(__arg)
                        };
                        <#ty as ::core::convert::TryFrom<
                            ::blinc_dsl_core::__extern_widget_internals::BlincStructValue
                        >>::try_from(__value)
                            .unwrap_or_else(|_| panic!(
                                "failed to decode DSL struct prop `{}` as `{}`",
                                #dsl_type_name,
                                stringify!(#ty)
                            ))
                    }
                },
                prop_type_expr: quote! {
                    ::blinc_dsl_core::__extern_widget_internals::Type::Unresolved(
                        ::blinc_dsl_core::__extern_widget_internals::InternedString::new_global(
                            #dsl_type_name
                        )
                    )
                },
                param_type_expr: quote! {
                    ::blinc_dsl_core::__extern_widget_internals::Type::Primitive(
                        ::blinc_dsl_core::__extern_widget_internals::PrimitiveType::I64
                    )
                },
            })
        }
    }
}

/// Parsed `#[extern_widget(name = "X", namespace = "ns"?, styled?)]` args.
struct ExternWidgetArgs {
    /// Bare DSL-visible name, e.g. `"Button"`. Required.
    name: String,
    /// Optional namespace prefix, e.g. `"cn"`. When set, the
    /// registered DSL name becomes `"<namespace>.<name>"` (e.g.
    /// `"cn.Button"`) and the grammar's dotted-component-call shape
    /// `cn.Button(...)` resolves to this widget. Empty namespace is
    /// equivalent to omitting the field — the widget registers at
    /// the top level under just `<name>`.
    namespace: Option<String>,
    /// When true, the macro wraps the widget in `Styled<W>` and the
    /// spec advertises a `__style` prop the lowering pass populates
    /// from inline DSL styling args.
    styled: bool,
}

impl ExternWidgetArgs {
    /// Qualified DSL name as registered with the runtime. With a
    /// namespace it's `"<ns>.<name>"`; without, just `"<name>"`.
    fn dsl_name(&self) -> String {
        match &self.namespace {
            Some(ns) if !ns.is_empty() => format!("{ns}.{}", self.name),
            _ => self.name.clone(),
        }
    }

    /// Rust-identifier-safe form of the qualified name — the dot in
    /// `cn.Button` becomes an underscore so the thunk fn / JIT symbol
    /// remain valid Rust idents and well-formed linker symbols.
    /// Equivalent to `dsl_name().replace('.', "_")`.
    fn symbol_safe_name(&self) -> String {
        self.dsl_name().replace('.', "_")
    }
}

fn field_has_children_attr(field: &syn::Field) -> bool {
    field
        .attrs
        .iter()
        .any(|attr| attr.path().is_ident("children"))
}

/// `#[skip]` field attribute — the field stays in the struct body
/// but is excluded from the FFI thunk, the prop list, the parameter
/// type list, and the param-count. The struct constructor inside the
/// generated thunk fills the field with `Default::default()`, so any
/// skipped field's type must impl [`Default`].
///
/// Use case: caching `OnceCell<...>` state from the wrapped widget
/// so `ElementBuilder::children_builders()` can return a stable
/// reference instead of an empty slice. The DSL never marshals these
/// fields; they're internal book-keeping.
fn field_is_skipped(field: &syn::Field) -> bool {
    field.attrs.iter().any(|attr| attr.path().is_ident("skip"))
}

fn field_slot_name(field: &syn::Field) -> Option<String> {
    let attr = field.attrs.iter().find(|a| a.path().is_ident("slot"))?;
    let mut name: Option<String> = None;
    let _ = attr.parse_nested_meta(|meta| {
        if meta.path.is_ident("name") {
            let lit: syn::LitStr = meta.value()?.parse()?;
            name = Some(lit.value());
            Ok(())
        } else {
            Err(meta.error("expected `name = \"...\"`"))
        }
    });
    name
}

impl syn::parse::Parse for ExternWidgetArgs {
    /// Accepts `name = "..."`, optional `namespace = "..."`, and the
    /// bare `styled` flag in any order. `name` is required; everything
    /// else defaults. Trailing commas are tolerated.
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut name: Option<(syn::LitStr, proc_macro2::Span)> = None;
        let mut namespace: Option<String> = None;
        let mut styled = false;

        loop {
            if input.is_empty() {
                break;
            }
            let key: syn::Ident = input.parse()?;
            match key.to_string().as_str() {
                "name" => {
                    let _: syn::Token![=] = input.parse()?;
                    let value: syn::LitStr = input.parse()?;
                    name = Some((value, key.span()));
                }
                "namespace" => {
                    let _: syn::Token![=] = input.parse()?;
                    let value: syn::LitStr = input.parse()?;
                    namespace = Some(value.value());
                }
                "styled" => {
                    styled = true;
                }
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!(
                            "unknown #[extern_widget] arg `{other}` — expected one of \
                             `name = \"...\"`, `namespace = \"...\"`, `styled`"
                        ),
                    ));
                }
            }
            if input.is_empty() {
                break;
            }
            let _: syn::Token![,] = input.parse()?;
        }

        let Some((name_lit, _)) = name else {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "#[extern_widget] requires `name = \"<DslName>\"`",
            ));
        };
        if let Some(ns) = &namespace {
            let invalid = ns.is_empty()
                || ns.contains('.')
                || !ns.chars().next().is_some_and(|c| c.is_ascii_lowercase())
                || !ns.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
            if invalid {
                return Err(syn::Error::new(
                    name_lit.span(),
                    "#[extern_widget] `namespace` must be a single lowercase-leading \
                     identifier — e.g. `namespace = \"cn\"`. The DSL grammar disambiguates \
                     namespaced calls (`cn.Button(…)`) from method calls on uppercase \
                     types (`Counter.method(…)`) by requiring a lowercase namespace head, \
                     so this constraint is load-bearing rather than stylistic.",
                ));
            }
        }
        Ok(Self {
            name: name_lit.value(),
            namespace,
            styled,
        })
    }
}

/// Export a Rust struct as a Blinc DSL widget.
///
/// ```ignore
/// #[extern_widget(name = "FancyText")]
/// pub struct FancyText { pub content: String }
///
/// impl ElementBuilder for FancyText { /* … */ }
/// ```
///
/// Named fields become DSL-visible props; `String` / `bool` / `i32` /
/// `i64` / `f64` are supported scalar types today. Mark a `Vec<Box<dyn
/// ElementBuilder>>` field with `#[children]` to receive the parent's
/// body block, or `#[slot(name = "…")]` for named slots.
///
/// Register at runtime via `dsl.register_extern_widget::<FancyText>()?`.
/// The JIT linker symbol is `$Blinc$<Name>$view`.
#[proc_macro_attribute]
pub fn extern_widget(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as ExternWidgetArgs);
    let mut item_struct = parse_macro_input!(item as syn::ItemStruct);

    let struct_ident = item_struct.ident.clone();
    // `dsl_name` is what flows into the runtime registry — the
    // qualified form (`"cn.Button"`) when a namespace is set; the bare
    // `"Button"` otherwise. The grammar's `__component_call__("cn.Button", …)`
    // lookup is name-based, so a dotted DSL name resolves the same way
    // a bare one does.
    let dsl_name = args.dsl_name();
    // `symbol_safe_name` keeps the dot out of Rust identifiers / JIT
    // linker symbols. `cn.Button` → `cn_Button` for the thunk + view
    // symbol; the registry still sees the dotted form via `dsl_name`.
    let symbol_safe_name = args.symbol_safe_name();
    let styled = args.styled;

    if !item_struct.generics.params.is_empty() {
        return syn::Error::new_spanned(
            &item_struct.generics,
            "#[extern_widget] doesn't support generic widgets yet — drop the type parameters \
             or hand-roll the registration via `BlincDsl::register_extern_widget_spec`",
        )
        .to_compile_error()
        .into();
    }

    let syn::Fields::Named(fields) = &item_struct.fields else {
        return syn::Error::new_spanned(
            &item_struct.fields,
            "#[extern_widget] requires a struct with named fields — tuple and unit structs aren't \
             supported",
        )
        .to_compile_error()
        .into();
    };

    let thunk_ident = syn::Ident::new(
        &format!("__blinc_extern_{symbol_safe_name}_view"),
        proc_macro2::Span::call_site(),
    );
    let view_symbol = format!("$Blinc${symbol_safe_name}$view");

    // FFI order: children → slots → scalars. Skipped fields don't
    // participate in the FFI at all; they get `Default::default()`
    // in the generated struct constructor.
    let mut children_field: Option<&syn::Field> = None;
    let mut slot_fields: Vec<(&syn::Field, String)> = Vec::new();
    let mut scalar_fields: Vec<&syn::Field> = Vec::new();
    let mut skipped_fields: Vec<&syn::Field> = Vec::new();
    for field in &fields.named {
        if field_is_skipped(field) {
            skipped_fields.push(field);
        } else if field_has_children_attr(field) {
            if children_field.is_some() {
                return syn::Error::new_spanned(
                    field,
                    "#[extern_widget] supports at most one `#[children]` field",
                )
                .to_compile_error()
                .into();
            }
            children_field = Some(field);
        } else if let Some(slot_name) = field_slot_name(field) {
            slot_fields.push((field, slot_name));
        } else {
            scalar_fields.push(field);
        }
    }

    let mut thunk_params: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut thunk_decodes: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut struct_inits: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut prop_defs: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut param_types: Vec<proc_macro2::TokenStream> = Vec::new();

    // Skipped fields show up in the struct constructor with
    // `Default::default()` so the FFI thunk can build a complete
    // struct without seeing them. Order in the constructor doesn't
    // need to match struct definition order — Rust accepts named
    // initializers in any order — so appending here is fine.
    for field in &skipped_fields {
        let field_ident = field
            .ident
            .as_ref()
            .expect("named fields always have idents");
        struct_inits.push(quote! { #field_ident: ::core::default::Default::default() });
    }

    if let Some(field) = children_field {
        if !matches!(field.vis, syn::Visibility::Public(_)) {
            return syn::Error::new_spanned(
                &field.vis,
                "#[extern_widget] `#[children]` field must be `pub`",
            )
            .to_compile_error()
            .into();
        }
        let field_ident = field
            .ident
            .as_ref()
            .expect("named fields always have idents");
        let ffi_arg_ident = syn::Ident::new("__arg_children", field_ident.span());
        thunk_params.push(quote! { #ffi_arg_ident: i64 });
        thunk_decodes.push(quote! {
            // SAFETY: `lower_children_arrays_to_blocks` is the only producer of
            // these pointers; the call site can't forge one.
            let #field_ident = unsafe {
                ::blinc_dsl_core::__extern_widget_internals::decode_children(#ffi_arg_ident)
            };
        });
        // `From::from` rather than a direct move so the field may be
        // `Vec<Box<dyn ElementBuilder>>` OR `RefCell<Vec<…>>`. The
        // identity `From<T> for T` covers the first, `From<T> for
        // RefCell<T>` the second.
        //
        // A widget with internal structure needs the second: it has to
        // hand the body to the cn builder so the body ends up INSIDE
        // the widget, and `ElementBuilder::build`/`children_builders`
        // both take `&self`, which cannot move out of a plain Vec.
        // Without that the body becomes the wrapper's own layout
        // children and the widget's structure is flattened away.
        struct_inits.push(quote! { #field_ident: ::core::convert::From::from(#field_ident) });
        prop_defs.push(quote! {
            ::blinc_dsl_core::__extern_widget_internals::PropDef {
                name: ::std::sync::Arc::from("children"),
                ty: ::blinc_dsl_core::__extern_widget_internals::Type::Primitive(
                    ::blinc_dsl_core::__extern_widget_internals::PrimitiveType::I64
                ),
                reactive_inner: None,
            }
        });
        param_types.push(quote! {
            ::blinc_dsl_core::__extern_widget_internals::Type::Primitive(
                ::blinc_dsl_core::__extern_widget_internals::PrimitiveType::I64
            )
        });
    }

    for (field, slot_name) in &slot_fields {
        if !matches!(field.vis, syn::Visibility::Public(_)) {
            return syn::Error::new_spanned(
                &field.vis,
                "#[extern_widget] `#[slot]` field must be `pub`",
            )
            .to_compile_error()
            .into();
        }
        let field_ident = field
            .ident
            .as_ref()
            .expect("named fields always have idents");
        let ffi_arg_ident = syn::Ident::new(&format!("__arg_slot_{slot_name}"), field_ident.span());
        let prop_name = format!("slot_{slot_name}");
        thunk_params.push(quote! { #ffi_arg_ident: i64 });
        thunk_decodes.push(quote! {
            // SAFETY: same contract as the default children pointer.
            let #field_ident = unsafe {
                ::blinc_dsl_core::__extern_widget_internals::decode_children(#ffi_arg_ident)
            };
        });
        struct_inits.push(quote! { #field_ident });
        prop_defs.push(quote! {
            ::blinc_dsl_core::__extern_widget_internals::PropDef {
                name: ::std::sync::Arc::from(#prop_name),
                ty: ::blinc_dsl_core::__extern_widget_internals::Type::Primitive(
                    ::blinc_dsl_core::__extern_widget_internals::PrimitiveType::I64
                ),
                reactive_inner: None,
            }
        });
        param_types.push(quote! {
            ::blinc_dsl_core::__extern_widget_internals::Type::Primitive(
                ::blinc_dsl_core::__extern_widget_internals::PrimitiveType::I64
            )
        });
    }

    for (idx, field) in scalar_fields.iter().enumerate() {
        let field_ident = field
            .ident
            .as_ref()
            .expect("named fields always have idents");
        // A raw identifier keeps its `r#` when stringified, so a field
        // named for a Rust keyword — `r#ref`, `r#type` — would register
        // under a name no DSL source can spell, and the prop would be
        // silently ignored at every call site.
        let field_name = field_ident.to_string();
        let field_name = field_name
            .strip_prefix("r#")
            .unwrap_or(&field_name)
            .to_string();

        if !matches!(field.vis, syn::Visibility::Public(_)) {
            return syn::Error::new_spanned(
                &field.vis,
                "#[extern_widget] fields must be `pub` — non-public fields can't be set from DSL \
                 source. Make the field `pub` or move internal state into a wrapper struct.",
            )
            .to_compile_error()
            .into();
        }

        // `Reactive<T>` props occupy two FFI slots — a tag and a
        // payload — that the lowering pass fills with one of:
        //   - `(0, literal_bits)` for a baked-in literal,
        //   - `(1, signal_id_raw)` for a bare-Variable signal ref,
        //   - `(2, derived_id_raw)` for a `computed { … } : T` expr.
        // The decode reconstructs a typed `Reactive<T>` Rust enum;
        // the wrapper pattern-matches and routes to whichever cn-side
        // `IntoReactive<T>` adapter fits the inner type.
        //
        // Single registry prop entry — the lowering pass uses the
        // `Type::Reactive(...)` discriminator to know to emit two
        // values into the arg list.
        // `Vec<T>` props occupy ONE slot: a pointer to Zyntax's
        // `List<T> { data, len, capacity }`. The compiler already
        // lowers an array literal to that, so there is nothing to
        // marshal — the thunk reads the header and copies the elements
        // out, at the stride `T` implies.
        if let Some(inner_ty) = classify_vec_field(&field.ty) {
            let kind = match vec_param_kind(&inner_ty) {
                Ok(kind) => kind,
                Err(msg) => {
                    return syn::Error::new_spanned(&field.ty, msg)
                        .to_compile_error()
                        .into();
                }
            };
            let arg_ident = syn::Ident::new(&format!("__arg_{idx}"), field_ident.span());
            let ffi_ty = kind.ffi_ty;
            let decode = kind.decode;
            let prop_type_expr = kind.prop_type_expr;
            let param_type_expr = kind.param_type_expr;
            thunk_params.push(quote! { #arg_ident: #ffi_ty });
            thunk_decodes.push(quote! {
                let #field_ident = { let __arg = #arg_ident; #decode };
            });
            struct_inits.push(quote! { #field_ident });
            prop_defs.push(quote! {
                ::blinc_dsl_core::__extern_widget_internals::PropDef {
                    name: ::std::sync::Arc::from(#field_name),
                    ty: #prop_type_expr,
                    reactive_inner: None,
                }
            });
            param_types.push(param_type_expr);
            continue;
        }

        if let Some(inner_ty) = classify_reactive_field(&field.ty) {
            let tag_arg_ident = syn::Ident::new(&format!("__arg_{idx}_tag"), field_ident.span());
            let inner_name = inner_ty.to_string();
            let inner_prim = match inner_name.as_str() {
                "i32" => quote! { PrimitiveType::I32 },
                "f64" => quote! { PrimitiveType::F64 },
                "bool" => quote! { PrimitiveType::Bool },
                "String" => quote! { PrimitiveType::String },
                other => {
                    return syn::Error::new_spanned(
                        &field.ty,
                        format!(
                            "#[extern_widget] Reactive<{other}> isn't supported yet — only \
                             Reactive<i32>, Reactive<f64>, Reactive<bool>, Reactive<String> \
                             ship a wire-format decoder. Add the matching constructors in \
                             `blinc_runtime::reactive_value` to extend the set."
                        ),
                    )
                    .to_compile_error()
                    .into();
                }
            };

            if inner_name == "String" {
                // `Reactive<String>` uses a three-slot wire shape:
                // a string literal can't fit in an `i64` payload, so
                // the macro reserves a separate `*const i32`
                // (`literal_ptr`) slot alongside the standard
                // `id_payload: i64` slot. The lowering pass writes the
                // ZRTL string-literal pointer into `literal_ptr` for
                // `REACTIVE_TAG_LITERAL`, leaves it null for the
                // binding shapes, and routes the id through
                // `id_payload` for `REACTIVE_TAG_SIGNAL` /
                // `REACTIVE_TAG_COMPUTED`.
                let id_arg_ident = syn::Ident::new(&format!("__arg_{idx}_id"), field_ident.span());
                let literal_arg_ident =
                    syn::Ident::new(&format!("__arg_{idx}_literal"), field_ident.span());

                thunk_params.push(quote! { #tag_arg_ident: i32 });
                thunk_params.push(quote! { #id_arg_ident: i64 });
                thunk_params.push(quote! { #literal_arg_ident: *const i32 });
                thunk_decodes.push(quote! {
                    let #field_ident = match #tag_arg_ident {
                        ::blinc_dsl_core::__extern_widget_internals::REACTIVE_TAG_SIGNAL => {
                            ::blinc_dsl_core::__extern_widget_internals::Reactive::<String>::from_signal_id(
                                #id_arg_ident as u64,
                            )
                        }
                        ::blinc_dsl_core::__extern_widget_internals::REACTIVE_TAG_COMPUTED => {
                            ::blinc_dsl_core::__extern_widget_internals::Reactive::<String>::from_computed_id(
                                #id_arg_ident as u64,
                            )
                        }
                        _ => {
                            // SAFETY: lowering pass guarantees the
                            // pointer is either null or a length-
                            // prefixed ZRTL string buffer.
                            let __literal = unsafe {
                                ::blinc_dsl_core::__extern_widget_internals::decode_string(
                                    #literal_arg_ident,
                                )
                            };
                            ::blinc_dsl_core::__extern_widget_internals::Reactive::<String>::from_literal(__literal)
                        }
                    };
                });
                struct_inits.push(quote! { #field_ident });
                prop_defs.push(quote! {
                    ::blinc_dsl_core::__extern_widget_internals::PropDef {
                        name: ::std::sync::Arc::from(#field_name),
                        ty: ::blinc_dsl_core::__extern_widget_internals::Type::Primitive(
                            ::blinc_dsl_core::__extern_widget_internals::#inner_prim
                        ),
                        reactive_inner: Some(
                            ::blinc_dsl_core::__extern_widget_internals::Type::Primitive(
                                ::blinc_dsl_core::__extern_widget_internals::#inner_prim
                            ),
                        ),
                    }
                });
                // Three param-type slots: tag (I32), id (I64),
                // literal (String → `*const i32` at the wire level).
                param_types.push(quote! {
                    ::blinc_dsl_core::__extern_widget_internals::Type::Primitive(
                        ::blinc_dsl_core::__extern_widget_internals::PrimitiveType::I32
                    )
                });
                param_types.push(quote! {
                    ::blinc_dsl_core::__extern_widget_internals::Type::Primitive(
                        ::blinc_dsl_core::__extern_widget_internals::PrimitiveType::I64
                    )
                });
                param_types.push(quote! {
                    ::blinc_dsl_core::__extern_widget_internals::Type::Primitive(
                        ::blinc_dsl_core::__extern_widget_internals::PrimitiveType::String
                    )
                });
                continue;
            }

            // Scalar Reactive<T> path — uniform two-slot wire shape.
            let payload_arg_ident =
                syn::Ident::new(&format!("__arg_{idx}_payload"), field_ident.span());

            thunk_params.push(quote! { #tag_arg_ident: i32 });
            thunk_params.push(quote! { #payload_arg_ident: i64 });
            thunk_decodes.push(quote! {
                let #field_ident = ::blinc_dsl_core::__extern_widget_internals::Reactive::<#inner_ty>::decode_ffi(
                    #tag_arg_ident,
                    #payload_arg_ident,
                );
            });
            struct_inits.push(quote! { #field_ident });
            prop_defs.push(quote! {
                ::blinc_dsl_core::__extern_widget_internals::PropDef {
                    name: ::std::sync::Arc::from(#field_name),
                    // `ty` is the INNER type so call-site type-checking
                    // matches against the value the DSL author writes
                    // (e.g. `Reactive<f64>` accepts an f64 literal or
                    // an f64-typed signal). `reactive_inner` is the
                    // discriminator the lowering pass reads to know
                    // this prop expects two FFI args (tag, payload)
                    // and to route literal / signal / computed shapes
                    // accordingly.
                    ty: ::blinc_dsl_core::__extern_widget_internals::Type::Primitive(
                        ::blinc_dsl_core::__extern_widget_internals::#inner_prim
                    ),
                    reactive_inner: Some(
                        ::blinc_dsl_core::__extern_widget_internals::Type::Primitive(
                            ::blinc_dsl_core::__extern_widget_internals::#inner_prim
                        ),
                    ),
                }
            });
            // Two param-type slots to match the two FFI args; both
            // are runtime-tag/payload primitives that the lowering
            // pass owns interpretation of.
            param_types.push(quote! {
                ::blinc_dsl_core::__extern_widget_internals::Type::Primitive(
                    ::blinc_dsl_core::__extern_widget_internals::PrimitiveType::I32
                )
            });
            param_types.push(quote! {
                ::blinc_dsl_core::__extern_widget_internals::Type::Primitive(
                    ::blinc_dsl_core::__extern_widget_internals::PrimitiveType::I64
                )
            });
            continue;
        }

        let Some(kind) = classify_param_type(&field.ty) else {
            return syn::Error::new_spanned(
                &field.ty,
                "#[extern_widget] fields must be String, i32, i64, f64, Reactive<T> (where T is \
                 i32/f64/bool), or a non-generic custom type that implements \
                 TryFrom<BlincStructValue> (or use `#[children]` for a \
                 `Vec<Box<dyn ElementBuilder>>` children slot)",
            )
            .to_compile_error()
            .into();
        };

        let ffi_arg_ident = syn::Ident::new(&format!("__arg_{idx}"), field_ident.span());
        let ffi_ty = &kind.ffi_ty;
        let decode = &kind.decode;
        let prop_type_expr = &kind.prop_type_expr;
        let param_type_expr = &kind.param_type_expr;

        thunk_params.push(quote! { #ffi_arg_ident: #ffi_ty });
        thunk_decodes.push(quote! {
            let #field_ident = {
                let __arg = #ffi_arg_ident;
                #decode
            };
        });
        struct_inits.push(quote! { #field_ident });
        prop_defs.push(quote! {
            ::blinc_dsl_core::__extern_widget_internals::PropDef {
                name: ::std::sync::Arc::from(#field_name),
                ty: #prop_type_expr,
                reactive_inner: None,
            }
        });
        param_types.push(param_type_expr.clone());
    }

    if styled {
        thunk_params.push(quote! { __arg_style: i64 });
        prop_defs.push(quote! {
            ::blinc_dsl_core::__extern_widget_internals::PropDef {
                name: ::std::sync::Arc::from("__style"),
                ty: ::blinc_dsl_core::__extern_widget_internals::Type::Primitive(
                    ::blinc_dsl_core::__extern_widget_internals::PrimitiveType::I64
                ),
                reactive_inner: None,
            }
        });
        param_types.push(quote! {
            ::blinc_dsl_core::__extern_widget_internals::Type::Primitive(
                ::blinc_dsl_core::__extern_widget_internals::PrimitiveType::I64
            )
        });
    }

    // Every extern widget takes `class`, so a `.blinc` author can style
    // one with a CSS rule the same way they style a `Div`. Previously
    // the arg parsed and was dropped on the floor: the call compiled and
    // the rule silently never applied.
    thunk_params.push(quote! { __arg_class: *const i32 });
    prop_defs.push(quote! {
        ::blinc_dsl_core::__extern_widget_internals::PropDef {
            name: ::std::sync::Arc::from("class"),
            ty: ::blinc_dsl_core::__extern_widget_internals::Type::Primitive(
                ::blinc_dsl_core::__extern_widget_internals::PrimitiveType::String
            ),
            reactive_inner: None,
        }
    });
    param_types.push(quote! {
        ::blinc_dsl_core::__extern_widget_internals::Type::Primitive(
            ::blinc_dsl_core::__extern_widget_internals::PrimitiveType::String
        )
    });

    // Strip macro-only field attributes before re-emitting the struct.
    if let syn::Fields::Named(named) = &mut item_struct.fields {
        for field in &mut named.named {
            field.attrs.retain(|attr| {
                !(attr.path().is_ident("children")
                    || attr.path().is_ident("slot")
                    || attr.path().is_ident("skip"))
            });
        }
    }

    let widget_construction = if styled {
        quote! {
            // SAFETY: `__arg_style` is `0` or a `__new_style_overlay__` pointer.
            let __overlay = unsafe {
                ::blinc_dsl_core::__extern_widget_internals::decode_overlay(__arg_style)
            };
            let __widget: Box<dyn ::blinc_layout::div::ElementBuilder> = Box::new(
                ::blinc_dsl_core::__extern_widget_internals::Styled::with_classes(
                    #struct_ident { #(#struct_inits),* },
                    __overlay,
                    ::blinc_dsl_core::__extern_widget_internals::decode_class_names(__arg_class),
                )
            );
        }
    } else {
        quote! {
            // Unstyled widgets are only wrapped when a class was
            // actually written, so the common call stays a bare struct.
            let __classes =
                ::blinc_dsl_core::__extern_widget_internals::decode_class_names(__arg_class);
            let __widget: Box<dyn ::blinc_layout::div::ElementBuilder> = if __classes.is_empty() {
                Box::new(#struct_ident { #(#struct_inits),* })
            } else {
                Box::new(
                    ::blinc_dsl_core::__extern_widget_internals::Styled::with_classes(
                        #struct_ident { #(#struct_inits),* },
                        ::std::default::Default::default(),
                        __classes,
                    )
                )
            };
        }
    };

    let expanded = quote! {
        #item_struct

        #[doc(hidden)]
        #[allow(non_snake_case)]
        extern "C" fn #thunk_ident(#(#thunk_params),*) -> i64 {
            #(#thunk_decodes)*
            #widget_construction
            ::blinc_dsl_core::__extern_widget_internals::into_handle(__widget)
        }

        impl ::blinc_dsl_core::__extern_widget_internals::ExternWidget for #struct_ident {
            // User-facing qualified form — `"cn.Button"` for namespaced
            // widgets, `"Button"` otherwise. Surfaces in diagnostics and
            // is the form a DSL author writes at the call site.
            const DSL_NAME: &'static str = #dsl_name;

            fn extern_widget_spec()
                -> ::blinc_dsl_core::__extern_widget_internals::ExternWidgetSpec
            {
                use ::blinc_dsl_core::__extern_widget_internals::{
                    ExternWidgetSpec, PrimitiveType, Type,
                };
                ExternWidgetSpec {
                    // Registry key is the mangled form (`cn_Button`).
                    // `lower_component_calls` replaces the dot in the
                    // grammar-emitted dotted name with `_` before doing
                    // its lookup, and `primitive_callee_props` reverses
                    // the linker symbol (`$Blinc$cn_Button$view`) to the
                    // same form. Keeping the registry on the mangled
                    // side means both lookup directions agree without
                    // a second substitution.
                    name: #symbol_safe_name.to_string(),
                    view_symbol: #view_symbol.to_string(),
                    props: vec![#(#prop_defs),*],
                    param_types: vec![#(#param_types),*],
                    return_type: Type::Primitive(PrimitiveType::I64),
                    extern_ptr: #thunk_ident as *const u8,
                }
            }
        }
    };

    TokenStream::from(expanded)
}
