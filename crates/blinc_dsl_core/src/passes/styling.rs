//! Styling prop tables and inline styling-arg to overlay lowering.

use crate::*;

/// Inline styling props recognised on DSL primitive call sites. Each maps to
/// an overlay-setter extern (`__set_overlay_*__`).
const STYLING_PROP_NAMES: &[(&str, &str, StylingValueKind)] = &[
    ("bg", "__set_overlay_bg__", StylingValueKind::IntColor),
    (
        "opacity",
        "__set_overlay_opacity__",
        StylingValueKind::Float,
    ),
    (
        "corner_radius",
        "__set_overlay_corner_radius__",
        StylingValueKind::Float,
    ),
    (
        "border_width",
        "__set_overlay_border_width__",
        StylingValueKind::Float,
    ),
    (
        "border_color",
        "__set_overlay_border_color__",
        StylingValueKind::IntColor,
    ),
];

#[derive(Clone, Copy)]
enum StylingValueKind {
    IntColor,
    Float,
}

/// Gather inline styling args (`bg`, `opacity`, …) into a `__new_style_overlay__`
/// Block and attach overlay pointer as `__style` named arg. MUST run after
/// `lower_children_arrays_to_blocks` and before `resolve_extern_widget_named_args`.
pub(crate) fn lower_styling_args_to_overlays(program: &mut TypedProgram) {
    use zyntax_typed_ast::{TypedCall, TypedDeclaration, TypedExpression, TypedNamedArg};

    fn callee_is_styled_primitive(call: &TypedCall) -> bool {
        let TypedExpression::Variable(callee) = &call.callee.node else {
            return false;
        };
        let Some(sym) = callee.resolve_global() else {
            return false;
        };
        let sym: &str = &sym;
        let Some(name) = sym
            .strip_prefix("$Blinc$")
            .and_then(|s| s.strip_suffix("$view"))
        else {
            return false;
        };
        blinc_runtime::component::with_component_registry(|r| {
            r.get_by_name(name)
                .map(|def| def.props.iter().any(|p| p.name.as_ref() == "__style"))
                .unwrap_or(false)
        })
    }

    /// If `value` is a bare identifier naming a DSL-declared signal whose
    /// registered type matches `expected_ty`, return its raw `SignalId`.
    /// Used by the styling-arg lowering to redirect prop setters like
    /// `__set_overlay_opacity__` to their `_signal` counterparts so the
    /// overlay reads the live value through the reactive primitive at
    /// paint time instead of baking a snapshot.
    fn signal_id_for_variable(
        value: &zyntax_typed_ast::TypedNode<TypedExpression>,
        expected_ty: blinc_runtime::signal::SignalType,
    ) -> Option<u64> {
        // Shape A: bare Variable. User-declared top-level signals
        // (`signal foo: T` + `Div(opacity = foo)`) reach the styling
        // pass in this form because `resolve_signal_calls` leaves
        // user signals alone.
        if let TypedExpression::Variable(name) = &value.node {
            let name_str = name.resolve_global()?;
            let (id_raw, ty) = blinc_runtime::signal::lookup(&name_str)?;
            if ty != expected_ty {
                return None;
            }
            return Some(id_raw);
        }
        // Shape B: `__signal_get_by_id_<T>(<id_literal>)` call.
        // FSM-context signals (`Ticker.pct`) reach the styling pass
        // in this form because `resolve_signal_calls` force-wraps
        // every bare ctx-signal Variable into a typed getter call —
        // needed for action-body arithmetic + f-string interp where a
        // bare reference would be an undefined local. The wrap
        // collapses the value to a runtime getter; we still want the
        // STYLING side to route to the live `_signal__` setter, so
        // peel the wrap back to recover the raw id.
        if let TypedExpression::Call(c) = &value.node {
            let TypedExpression::Variable(callee) = &c.callee.node else {
                return None;
            };
            let callee_str = callee.resolve_global()?;
            let getter_ty = match (callee_str.as_ref(), expected_ty) {
                ("__signal_get_by_id_f64", blinc_runtime::signal::SignalType::F64) => {
                    blinc_runtime::signal::SignalType::F64
                }
                ("__signal_get_by_id_i32", blinc_runtime::signal::SignalType::I32) => {
                    blinc_runtime::signal::SignalType::I32
                }
                _ => return None,
            };
            let arg = c.positional_args.first()?;
            let TypedExpression::Literal(zyntax_typed_ast::TypedLiteral::Integer(id_lit)) =
                &arg.node
            else {
                return None;
            };
            // The id literal carries the raw signal id as i64. Cast
            // back to u64 — same wire convention as the `_signal__`
            // extern's arg.
            let _ = getter_ty; // type already matched above
            return Some(*id_lit as i64 as u64);
        }
        None
    }

    /// Recognise `__blinc_computed_<T>__(closure_expr)` — the call
    /// shape `computed { … } : T` lowers to (per `grammar/blinc.zyn`).
    /// Returns `true` when the value is one of these calls AND the
    /// inner T matches what the styling prop expects.
    ///
    /// At runtime the call evaluates to a `DerivedId.to_raw() as i64`,
    /// which is exactly the payload the `_computed__` setters need.
    /// Mirrors the recognizer inside `lower_reactive_args`.
    fn is_computed_call_of_kind(
        value: &zyntax_typed_ast::TypedNode<TypedExpression>,
        kind: StylingValueKind,
    ) -> bool {
        let TypedExpression::Call(c) = &value.node else {
            return false;
        };
        let TypedExpression::Variable(callee) = &c.callee.node else {
            return false;
        };
        let want = match kind {
            StylingValueKind::Float => "__blinc_computed_f64__",
            StylingValueKind::IntColor => "__blinc_computed_i32__",
        };
        matches!(callee.resolve_global().as_deref(), Some(name) if name == want)
    }

    fn walk_stmt(stmt: &mut zyntax_typed_ast::TypedNode<TypedStatement>, counter: &mut u32) {
        match &mut stmt.node {
            TypedStatement::Expression(e) => rewrite_expr(e, counter),
            TypedStatement::Return(Some(e)) => rewrite_expr(e, counter),
            TypedStatement::Let(l) => {
                if let Some(init) = &mut l.initializer {
                    rewrite_expr(init, counter);
                }
            }
            TypedStatement::If(if_stmt) => {
                rewrite_expr(&mut if_stmt.condition, counter);
                for s in &mut if_stmt.then_block.statements {
                    walk_stmt(s, counter);
                }
                if let Some(else_block) = &mut if_stmt.else_block {
                    for s in &mut else_block.statements {
                        walk_stmt(s, counter);
                    }
                }
            }
            _ => {}
        }
    }

    fn rewrite_expr(expr: &mut zyntax_typed_ast::TypedNode<TypedExpression>, counter: &mut u32) {
        match &mut expr.node {
            TypedExpression::Call(call) => {
                rewrite_expr(&mut call.callee, counter);
                for arg in &mut call.positional_args {
                    rewrite_expr(arg, counter);
                }
                for na in &mut call.named_args {
                    rewrite_expr(&mut na.value, counter);
                }
            }
            TypedExpression::Array(items) => {
                for item in items {
                    rewrite_expr(item, counter);
                }
            }
            TypedExpression::Block(block) => {
                for stmt in &mut block.statements {
                    walk_stmt(stmt, counter);
                }
            }
            TypedExpression::Binary(b) => {
                rewrite_expr(&mut b.left, counter);
                rewrite_expr(&mut b.right, counter);
            }
            _ => {}
        }

        let TypedExpression::Call(call) = &mut expr.node else {
            return;
        };
        if !callee_is_styled_primitive(call) {
            return;
        }

        // Partition named args into styling args (consumed by
        // overlay setters) vs other args (left in place). We carry the
        // value-kind through so the redirect step below knows which
        // signal type to look for when a value is a bare Variable.
        let mut styling_args: Vec<(&'static str, StylingValueKind, TypedNamedArg)> = Vec::new();
        let mut remaining_named: Vec<TypedNamedArg> = Vec::new();
        let existing_named = std::mem::take(&mut call.named_args);
        for na in existing_named {
            let resolved = na.name.resolve_global();
            let name_str: Option<&str> = resolved.as_deref();
            if let Some(name) = name_str
                && let Some(entry) = STYLING_PROP_NAMES.iter().find(|(n, _, _)| *n == name)
            {
                styling_args.push((entry.1, entry.2, na));
                continue;
            }
            remaining_named.push(na);
        }

        let span = expr.span;
        let i64_ty = Type::Primitive(PrimitiveType::I64);
        let unit_ty = Type::Primitive(PrimitiveType::Unit);

        if styling_args.is_empty() {
            // Restore other named args and inject a null overlay
            // pointer so the call's `__style` slot is filled.
            call.named_args = remaining_named;
            call.named_args.push(TypedNamedArg {
                name: zyntax_typed_ast::InternedString::new_global("__style"),
                value: Box::new(typed_node(
                    TypedExpression::Literal(zyntax_typed_ast::TypedLiteral::Integer(0)),
                    i64_ty.clone(),
                    span,
                )),
                span,
            });
            return;
        }

        // Allocate a unique ident for the overlay let-binding.
        let id = {
            let i = *counter;
            *counter += 1;
            i
        };
        let overlay_ident =
            zyntax_typed_ast::InternedString::new_global(&format!("__blinc_style_{id}"));

        let mut stmts: Vec<zyntax_typed_ast::TypedNode<TypedStatement>> = Vec::new();

        // let __blinc_style_N = __new_style_overlay__()
        stmts.push(typed_node(
            TypedStatement::Let(zyntax_typed_ast::typed_ast::TypedLet {
                name: overlay_ident,
                ty: i64_ty.clone(),
                mutability: zyntax_typed_ast::Mutability::Immutable,
                initializer: Some(Box::new(typed_node(
                    TypedExpression::Call(TypedCall {
                        callee: Box::new(typed_node(
                            TypedExpression::Variable(
                                zyntax_typed_ast::InternedString::new_global(
                                    "__new_style_overlay__",
                                ),
                            ),
                            Type::Any,
                            span,
                        )),
                        positional_args: vec![],
                        named_args: vec![],
                        type_args: vec![],
                    }),
                    i64_ty.clone(),
                    span,
                ))),
                span,
            }),
            unit_ty.clone(),
            span,
        ));

        // One setter call per styling arg. The arg's expression shape
        // picks which variant we emit:
        //
        //   * Bare signal identifier of the matching SignalType →
        //     `_signal__` variant, with the raw signal id as payload.
        //   * `computed { … } : T` call (already desugared to
        //     `__blinc_computed_<T>__(closure)` by `process_statement`)
        //     → `_computed__` variant, with the call expression itself
        //     as payload (it returns a `DerivedId.to_raw() as i64` at
        //     runtime).
        //   * Anything else → original literal-baking setter.
        //
        // Signal takes priority over computed only because the test
        // is cheaper; in practice each value matches at most one
        // shape so order doesn't change behaviour.
        for (setter_name, kind, na) in styling_args {
            let value_node = *na.value;
            let expected_signal_ty = match kind {
                StylingValueKind::Float => blinc_runtime::signal::SignalType::F64,
                StylingValueKind::IntColor => blinc_runtime::signal::SignalType::I32,
            };
            let signal_redirect =
                signal_id_for_variable(&value_node, expected_signal_ty).map(|id_raw_u64| {
                    // Derive the `_signal__` variant name by replacing
                    // the trailing `__` with `_signal__`. Every styling
                    // setter follows the `__set_overlay_*__` convention
                    // and has a registered `_signal__` peer in the abi
                    // table.
                    let signal_setter_name =
                        format!("{}_signal__", setter_name.trim_end_matches("__"));
                    let signal_setter =
                        zyntax_typed_ast::InternedString::new_global(&signal_setter_name);
                    let id_arg = typed_node(
                        TypedExpression::Literal(zyntax_typed_ast::TypedLiteral::Integer(
                            id_raw_u64 as i64 as i128,
                        )),
                        i64_ty.clone(),
                        span,
                    );
                    (signal_setter, id_arg)
                });
            let (effective_setter, effective_arg) = match signal_redirect {
                Some((setter, arg)) => (setter, arg),
                None if is_computed_call_of_kind(&value_node, kind) => {
                    // Computed redirect. Same suffix-swap dance as the
                    // signal variant: `__set_overlay_X__` → `__set_overlay_X_computed__`.
                    // The arg keeps the original `__blinc_computed_<T>__`
                    // call expression — it already returns the raw
                    // derived id as i64 at runtime.
                    let computed_setter_name =
                        format!("{}_computed__", setter_name.trim_end_matches("__"));
                    let computed_setter =
                        zyntax_typed_ast::InternedString::new_global(&computed_setter_name);
                    (computed_setter, value_node)
                }
                None => (
                    zyntax_typed_ast::InternedString::new_global(setter_name),
                    value_node,
                ),
            };
            let setter_call = TypedExpression::Call(TypedCall {
                callee: Box::new(typed_node(
                    TypedExpression::Variable(effective_setter),
                    Type::Any,
                    span,
                )),
                positional_args: vec![
                    typed_node(
                        TypedExpression::Variable(overlay_ident),
                        i64_ty.clone(),
                        span,
                    ),
                    effective_arg,
                ],
                named_args: vec![],
                type_args: vec![],
            });
            stmts.push(typed_node(
                TypedStatement::Expression(Box::new(typed_node(
                    setter_call,
                    unit_ty.clone(),
                    span,
                ))),
                unit_ty.clone(),
                span,
            ));
        }

        // Trailing call: keep the original shape but attach
        // `__style = Var(__blinc_style_N)` so the named-args
        // resolution pass routes it to the right slot.
        call.named_args = remaining_named;
        call.named_args.push(TypedNamedArg {
            name: zyntax_typed_ast::InternedString::new_global("__style"),
            value: Box::new(typed_node(
                TypedExpression::Variable(overlay_ident),
                i64_ty.clone(),
                span,
            )),
            span,
        });

        // The Call expression itself is what closes the Block;
        // we extract a clone of the (now-modified) call to push
        // as the trailing Expression statement, then replace
        // `expr` with the Block.
        let final_call = TypedExpression::Call(TypedCall {
            callee: call.callee.clone(),
            positional_args: std::mem::take(&mut call.positional_args),
            named_args: std::mem::take(&mut call.named_args),
            type_args: std::mem::take(&mut call.type_args),
        });
        stmts.push(typed_node(
            TypedStatement::Expression(Box::new(typed_node(final_call, i64_ty.clone(), span))),
            i64_ty.clone(),
            span,
        ));

        expr.node = TypedExpression::Block(zyntax_typed_ast::typed_ast::TypedBlock {
            statements: stmts,
            span,
        });
    }

    let mut counter: u32 = 0;
    for decl in program.declarations.iter_mut() {
        match &mut decl.node {
            TypedDeclaration::Function(func) => {
                if let Some(body) = func.body.as_mut() {
                    for stmt in &mut body.statements {
                        walk_stmt(stmt, &mut counter);
                    }
                }
            }
            TypedDeclaration::Impl(imp) => {
                for method in &mut imp.methods {
                    if let Some(body) = method.body.as_mut() {
                        for stmt in &mut body.statements {
                            walk_stmt(stmt, &mut counter);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
