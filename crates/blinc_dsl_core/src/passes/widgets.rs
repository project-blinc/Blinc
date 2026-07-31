//! View value-returning lowering and children-array to child-list expansion.

use crate::*;

/// Rewrite view fns to value-returning (`I64` widget handle) when their body
/// ends in a `$Blinc$<X>$view` call. MUST run before [`ensure_unit_return`].
/// Pinning return to a concrete `I64` (not `Any`) keeps Zyntax's body
/// classifier on the well-trodden specialised-call path.
pub(crate) fn lower_view_to_value_returning(
    program: &mut TypedProgram,
    value_returning_symbols: &mut std::collections::HashSet<String>,
) {
    use zyntax_typed_ast::{TypedDeclaration, TypedExpression};

    fn is_view_name(name: zyntax_typed_ast::InternedString) -> bool {
        matches!(
            name.resolve_global().as_deref(),
            Some("render_view") | Some("view")
        )
    }

    /// `fn row(label: string): View { … }` — a user function that hands
    /// back a widget.
    ///
    /// The declared return type is the opt-in rather than the trailing
    /// statement's shape: promoting any function that happens to end in
    /// a widget call would rewrite the return type of helpers that build
    /// a widget for their own reasons and return something else.
    ///
    /// The marker resolves as `Named` or `Unresolved` depending on
    /// whether a type of that name exists, and neither is registered, so
    /// both spellings are accepted.
    fn returns_view_marker(
        ty: &Type,
        registry: &zyntax_typed_ast::type_registry::TypeRegistry,
    ) -> bool {
        let name = match ty {
            Type::Unresolved(n) => n.resolve_global().map(|s| s.to_string()),
            Type::Named { id, .. } => registry
                .get_type_by_id(*id)
                .and_then(|t| t.name.resolve_global())
                .map(|s| s.to_string()),
            _ => None,
        };
        name.as_deref() == Some("View")
    }

    /// Match `Expression(Call(Variable("<X>$view"), ...))` where `<X>` is a
    /// substrate primitive or a user component whose own view is value-returning.
    fn is_primitive_view_call_stmt(
        stmt: &TypedStatement,
        value_returning_symbols: &std::collections::HashSet<String>,
    ) -> bool {
        let TypedStatement::Expression(expr) = stmt else {
            return false;
        };
        let TypedExpression::Call(call) = &expr.node else {
            return false;
        };
        let TypedExpression::Variable(callee) = &call.callee.node else {
            return false;
        };
        let Some(name) = callee.resolve_global() else {
            return false;
        };
        let s: &str = name.as_ref();
        // Substrate primitives are always i64-returning.
        if s.starts_with("$Blinc$") && s.ends_with("$view") {
            return true;
        }
        // A `with` region's builtin hands back the mounted `Stateful`'s
        // handle, so a view ending in one is value-returning too.
        if s == "__blinc_with__" {
            return true;
        }
        // User components: only if promoted by an earlier pass.
        value_returning_symbols.contains(s)
    }

    /// Rewrite trailing primitive-call to `Return(Some(call))` and return whether converted.
    fn try_convert_trailing(
        body: &mut zyntax_typed_ast::typed_ast::TypedBlock,
        value_returning_symbols: &std::collections::HashSet<String>,
    ) -> bool {
        let Some(last) = body.statements.last() else {
            return false;
        };
        if !is_primitive_view_call_stmt(&last.node, value_returning_symbols) {
            return false;
        }
        let last = body
            .statements
            .last_mut()
            .expect("just confirmed last exists above");
        let placeholder = TypedStatement::Continue;
        let original = std::mem::replace(&mut last.node, placeholder);
        let TypedStatement::Expression(expr) = original else {
            unreachable!("just confirmed Expression shape above");
        };
        last.node = TypedStatement::Return(Some(expr));
        true
    }

    let widget_handle_type = Type::Primitive(PrimitiveType::I64);

    // Pass 1: impl `view` methods. Pass 2: free-standing view fns referencing them.
    for decl in program.declarations.iter_mut() {
        let TypedDeclaration::Impl(imp) = &mut decl.node else {
            continue;
        };
        // `<TypeName>$<method>` mangling — pull the type name once per impl.
        let type_name: Option<String> = match &imp.for_type {
            Type::Unresolved(name) => name.resolve_global().map(|s| s.to_string()),
            Type::Named { id, .. } => program
                .type_registry
                .get_type_by_id(*id)
                .and_then(|t| t.name.resolve_global())
                .map(|s| s.to_string()),
            _ => None,
        };
        for method in &mut imp.methods {
            if !is_view_name(method.name) {
                continue;
            }
            let Some(body) = method.body.as_mut() else {
                continue;
            };
            if try_convert_trailing(body, value_returning_symbols) {
                method.return_type = widget_handle_type.clone();
                if let (Some(t), Some(m)) = (type_name.as_ref(), method.name.resolve_global()) {
                    value_returning_symbols.insert(format!("{t}${m}"));
                }
            }
        }
    }

    // Free functions: the `view` / `render_view` entry points, plus any
    // user function declared `: View`.
    //
    // To a fixpoint, because a view function may call another one and
    // `is_primitive_view_call_stmt` only recognises a callee already in
    // `value_returning_symbols`. A single pass would promote them only
    // when they happen to be declared callee-first. Each round promotes
    // at least one or stops, so this terminates in at most one round per
    // function.
    loop {
        let mut promoted_this_round = false;
        for decl in program.declarations.iter_mut() {
            let TypedDeclaration::Function(func) = &mut decl.node else {
                continue;
            };
            if func.is_external {
                continue;
            }
            let already = func
                .name
                .resolve_global()
                .is_some_and(|n| value_returning_symbols.contains(&n as &str));
            if already {
                continue;
            }
            if !is_view_name(func.name)
                && !returns_view_marker(&func.return_type, &program.type_registry)
            {
                continue;
            }
            let Some(body) = func.body.as_mut() else {
                continue;
            };
            if try_convert_trailing(body, value_returning_symbols) {
                func.return_type = widget_handle_type.clone();
                if let Some(name) = func.name.resolve_global() {
                    value_returning_symbols.insert(name.to_string());
                }
                promoted_this_round = true;
            }
        }
        if !promoted_this_round {
            break;
        }
    }
}

/// Expand substrate primitive calls carrying `children = Array([...])` (and slot
/// arrays) into Block expansions backed by `__new_child_list__` / `__push_child__`.
/// Post-order recursive. MUST run after `lower_view_to_value_returning` and
/// before `ensure_unit_return`.
pub(crate) fn lower_children_arrays_to_blocks(
    program: &mut TypedProgram,
    value_returning_symbols: &std::collections::HashSet<String>,
) {
    use zyntax_typed_ast::{TypedCall, TypedDeclaration, TypedExpression, TypedNamedArg};

    /// Counter for unique `__blinc_children_<N>` idents.
    fn next_id(counter: &mut u32) -> u32 {
        let id = *counter;
        *counter += 1;
        id
    }

    /// Walk a statement and rewrite any nested primitive calls.
    fn walk_stmt(
        stmt: &mut zyntax_typed_ast::TypedNode<TypedStatement>,
        counter: &mut u32,
        vrs: &std::collections::HashSet<String>,
    ) {
        match &mut stmt.node {
            TypedStatement::Expression(e) => rewrite_expr(e, counter, vrs),
            TypedStatement::Return(Some(e)) => rewrite_expr(e, counter, vrs),
            TypedStatement::Let(l) => {
                if let Some(init) = &mut l.initializer {
                    rewrite_expr(init, counter, vrs);
                }
            }
            TypedStatement::If(if_stmt) => {
                rewrite_expr(&mut if_stmt.condition, counter, vrs);
                for s in &mut if_stmt.then_block.statements {
                    walk_stmt(s, counter, vrs);
                }
                if let Some(else_block) = &mut if_stmt.else_block {
                    for s in &mut else_block.statements {
                        walk_stmt(s, counter, vrs);
                    }
                }
            }
            // A widget nested in a loop body needs its OWN children
            // expanded too. Without this arm the inner call kept a raw
            // `children` Array that never became a child list, so the
            // nested widget rendered childless.
            TypedStatement::While(w) => {
                rewrite_expr(&mut w.condition, counter, vrs);
                for s in &mut w.body.statements {
                    walk_stmt(s, counter, vrs);
                }
            }
            TypedStatement::Block(b) => {
                for s in &mut b.statements {
                    walk_stmt(s, counter, vrs);
                }
            }
            _ => {}
        }
    }

    /// Post-order — recurse before rewriting `expr`.
    fn rewrite_expr(
        expr: &mut zyntax_typed_ast::TypedNode<TypedExpression>,
        counter: &mut u32,
        vrs: &std::collections::HashSet<String>,
    ) {
        match &mut expr.node {
            TypedExpression::Call(call) => {
                rewrite_expr(&mut call.callee, counter, vrs);
                for arg in &mut call.positional_args {
                    rewrite_expr(arg, counter, vrs);
                }
                for named in &mut call.named_args {
                    rewrite_expr(&mut named.value, counter, vrs);
                }
            }
            TypedExpression::Array(items) => {
                for item in items {
                    rewrite_expr(item, counter, vrs);
                }
            }
            TypedExpression::Block(block) => {
                for stmt in &mut block.statements {
                    walk_stmt(stmt, counter, vrs);
                }
            }
            TypedExpression::Binary(b) => {
                rewrite_expr(&mut b.left, counter, vrs);
                rewrite_expr(&mut b.right, counter, vrs);
            }
            _ => {}
        }

        // For primitives with child-slot props (`children`, `slot_<Name>`), gather each
        // Array into a `__new_child_list__` Block. The final call carries the lists as
        // named args; `resolve_extern_widget_named_args` later positionalises them.
        let span = expr.span;
        let i64_ty = Type::Primitive(PrimitiveType::I64);
        let unit_ty = Type::Primitive(PrimitiveType::Unit);

        let TypedExpression::Call(call) = &mut expr.node else {
            return;
        };
        let Some(slot_prop_names) = callee_slot_prop_names(call) else {
            return;
        };

        let mut prelude: Vec<zyntax_typed_ast::TypedNode<TypedStatement>> = Vec::new();
        let mut had_real_slot = false;

        for slot_name in &slot_prop_names {
            let na_idx = call
                .named_args
                .iter()
                .position(|na| na.name.resolve_global().as_deref() == Some(slot_name.as_str()));
            let Some(idx) = na_idx else {
                // Slot not supplied — inject a `0` literal so
                // the registry-driven resolution finds something
                // at this slot's named position.
                call.named_args.push(TypedNamedArg {
                    name: zyntax_typed_ast::InternedString::new_global(slot_name),
                    value: Box::new(typed_node(
                        TypedExpression::Literal(zyntax_typed_ast::TypedLiteral::Integer(0)),
                        i64_ty.clone(),
                        span,
                    )),
                    span,
                });
                continue;
            };
            let mut na = call.named_args.remove(idx);
            let TypedExpression::Array(child_exprs) = std::mem::replace(
                &mut na.value.node,
                TypedExpression::Literal(zyntax_typed_ast::TypedLiteral::Integer(0)),
            ) else {
                // Already a non-Array value (e.g., user passed
                // a raw i64 list pointer). Leave it alone.
                call.named_args.push(na);
                continue;
            };
            had_real_slot = true;

            let id = next_id(counter);
            let list_ident =
                zyntax_typed_ast::InternedString::new_global(&format!("__blinc_children_{id}"));

            // let __blinc_children_<id> = __new_child_list__()
            prelude.push(typed_node(
                TypedStatement::Let(zyntax_typed_ast::typed_ast::TypedLet {
                    name: list_ident,
                    ty: i64_ty.clone(),
                    mutability: zyntax_typed_ast::Mutability::Immutable,
                    initializer: Some(Box::new(typed_node(
                        TypedExpression::Call(TypedCall {
                            callee: Box::new(typed_node(
                                TypedExpression::Variable(
                                    zyntax_typed_ast::InternedString::new_global(
                                        "__new_child_list__",
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

            // for each child: __push_child__(__list, child)
            for child_expr in child_exprs {
                // `__cf_children__(Block)` is the control-flow carrier planted
                // by `collect_children_into` (`if` / `while` in a widget
                // body). Splice its statements into the prelude with the
                // child-producing calls inside rewritten to push onto this
                // list, so children created in a branch or loop iteration
                // land in source order.
                if is_cf_carrier(&child_expr) {
                    let TypedExpression::Call(mut carrier_call) = child_expr.node else {
                        unreachable!("is_cf_carrier just confirmed the Call shape");
                    };
                    if let TypedExpression::Block(mut carrier) =
                        carrier_call.positional_args.remove(0).node
                    {
                        for stmt in &mut carrier.statements {
                            rewrite_child_pushes_in_stmt(stmt, list_ident, vrs);
                        }
                        prelude.extend(carrier.statements);
                    }
                    continue;
                }
                // Not every statement in a widget body produces a child.
                // `sig.set(v)` and friends are side effects: emit them as
                // plain prelude statements. Pushing one would hand
                // `__push_child__` a void argument, which codegen cannot
                // resolve. Same rule the control-flow carrier applies to
                // statements inside a branch or loop body.
                if !is_widget_producing_expr(&child_expr, vrs) {
                    let ty = child_expr.ty.clone();
                    prelude.push(typed_node(
                        TypedStatement::Expression(Box::new(child_expr)),
                        ty,
                        span,
                    ));
                    continue;
                }
                let push_call = TypedExpression::Call(TypedCall {
                    callee: Box::new(typed_node(
                        TypedExpression::Variable(zyntax_typed_ast::InternedString::new_global(
                            "__push_child__",
                        )),
                        Type::Any,
                        span,
                    )),
                    positional_args: vec![
                        typed_node(TypedExpression::Variable(list_ident), i64_ty.clone(), span),
                        child_expr,
                    ],
                    named_args: vec![],
                    type_args: vec![],
                });
                prelude.push(typed_node(
                    TypedStatement::Expression(Box::new(typed_node(
                        push_call,
                        unit_ty.clone(),
                        span,
                    ))),
                    unit_ty.clone(),
                    span,
                ));
            }

            // Re-attach the slot as a named arg pointing at the ident.
            call.named_args.push(TypedNamedArg {
                name: zyntax_typed_ast::InternedString::new_global(slot_name),
                value: Box::new(typed_node(
                    TypedExpression::Variable(list_ident),
                    i64_ty.clone(),
                    span,
                )),
                span,
            });
        }

        if !had_real_slot {
            // No body-supplied slots — `0`-literal fills already on call.
            return;
        }

        // Wrap call in a trailing-expression Block.
        let final_call = TypedExpression::Call(TypedCall {
            callee: call.callee.clone(),
            positional_args: std::mem::take(&mut call.positional_args),
            named_args: std::mem::take(&mut call.named_args),
            type_args: std::mem::take(&mut call.type_args),
        });
        prelude.push(typed_node(
            TypedStatement::Expression(Box::new(typed_node(final_call, i64_ty.clone(), span))),
            i64_ty.clone(),
            span,
        ));

        expr.node = TypedExpression::Block(zyntax_typed_ast::typed_ast::TypedBlock {
            statements: prelude,
            span,
        });
    }

    /// True for the `__cf_children__(Block)` marker planted by
    /// `collect_children_into` for `if` / `while` in a widget body.
    fn is_cf_carrier(expr: &zyntax_typed_ast::TypedNode<TypedExpression>) -> bool {
        let TypedExpression::Call(c) = &expr.node else {
            return false;
        };
        let TypedExpression::Variable(callee) = &c.callee.node else {
            return false;
        };
        callee.resolve_global().as_deref() == Some("__cf_children__")
            && c.positional_args.len() == 1
            && matches!(c.positional_args[0].node, TypedExpression::Block(_))
    }

    /// True when the expression evaluates to a widget handle: a call to a
    /// `…$view` symbol (substrate primitive or user component), or a Block
    /// whose trailing expression is one (a widget-with-children after this
    /// pass's own expansion). The shape test alone (`Expression(Call)`) is
    /// NOT enough: by this stage `sig.set(v)` has been lowered to a bare
    /// `__signal_set_by_id_*` call — same shape, but a VOID result.
    /// Wrapping that in `__push_child__` feeds codegen a void value as an
    /// argument, which dies in Cranelift with "arg not in value_map".
    fn is_widget_producing_expr(
        expr: &zyntax_typed_ast::TypedNode<TypedExpression>,
        value_returning_symbols: &std::collections::HashSet<String>,
    ) -> bool {
        match &expr.node {
            TypedExpression::Call(c) => {
                let TypedExpression::Variable(callee) = &c.callee.node else {
                    return false;
                };
                callee.resolve_global().is_some_and(|n| {
                    // A user `fn row(): View` carries no marker in its
                    // name, so the set of symbols promoted by
                    // `lower_view_to_value_returning` is what identifies
                    // it. Without this it fails the suffix test and its
                    // child is silently dropped.
                    n.ends_with("$view")
                        || n == "__component_call__"
                        || n == "__blinc_with__"
                        || value_returning_symbols.contains(&n as &str)
                })
            }
            TypedExpression::Block(b) => b.statements.last().is_some_and(|s| match &s.node {
                TypedStatement::Expression(e) => {
                    is_widget_producing_expr(e, value_returning_symbols)
                }
                TypedStatement::Return(Some(e)) => {
                    is_widget_producing_expr(e, value_returning_symbols)
                }
                _ => false,
            }),
            _ => false,
        }
    }

    /// Rewrite child-producing calls inside a control-flow statement into
    /// `__push_child__(<list>, …)`, so children created in a branch or a
    /// loop iteration append to the enclosing widget's child list.
    ///
    /// Only widget-producing expressions are wrapped (see
    /// `is_widget_producing_expr`); side-effect statements like
    /// `sig.set(…)` stay untouched, so
    /// `while c { Row {…} i.set(i.get() + 1) }` both emits rows and
    /// advances the counter.
    fn rewrite_child_pushes_in_stmt(
        stmt: &mut zyntax_typed_ast::TypedNode<TypedStatement>,
        list_ident: zyntax_typed_ast::InternedString,
        vrs: &std::collections::HashSet<String>,
    ) {
        let span = stmt.span;
        match &mut stmt.node {
            TypedStatement::Expression(e) => {
                if !is_widget_producing_expr(e, vrs) {
                    return;
                }
                let placeholder = typed_node(
                    TypedExpression::Literal(zyntax_typed_ast::TypedLiteral::Integer(0)),
                    Type::Primitive(PrimitiveType::I64),
                    span,
                );
                let child = std::mem::replace(&mut **e, placeholder);
                **e = typed_node(
                    TypedExpression::Call(TypedCall {
                        callee: Box::new(typed_node(
                            TypedExpression::Variable(
                                zyntax_typed_ast::InternedString::new_global("__push_child__"),
                            ),
                            Type::Any,
                            span,
                        )),
                        positional_args: vec![
                            typed_node(
                                TypedExpression::Variable(list_ident),
                                Type::Primitive(PrimitiveType::I64),
                                span,
                            ),
                            child,
                        ],
                        named_args: vec![],
                        type_args: vec![],
                    }),
                    Type::Primitive(PrimitiveType::Unit),
                    span,
                );
            }
            TypedStatement::If(if_stmt) => {
                for s in &mut if_stmt.then_block.statements {
                    rewrite_child_pushes_in_stmt(s, list_ident, vrs);
                }
                if let Some(else_block) = &mut if_stmt.else_block {
                    for s in &mut else_block.statements {
                        rewrite_child_pushes_in_stmt(s, list_ident, vrs);
                    }
                }
            }
            TypedStatement::While(w) => {
                for s in &mut w.body.statements {
                    rewrite_child_pushes_in_stmt(s, list_ident, vrs);
                }
            }
            TypedStatement::Block(b) => {
                for s in &mut b.statements {
                    rewrite_child_pushes_in_stmt(s, list_ident, vrs);
                }
            }
            _ => {}
        }
    }

    /// Return child-slot prop names (`children`, `slot_<Name>`) in registry order.
    /// `None` for leaf primitives or non-primitives.
    fn callee_slot_prop_names(call: &TypedCall) -> Option<Vec<String>> {
        let TypedExpression::Variable(callee) = &call.callee.node else {
            return None;
        };
        let sym = callee.resolve_global()?;
        let sym: &str = &sym;
        let name = sym
            .strip_prefix("$Blinc$")
            .and_then(|s| s.strip_suffix("$view"))?;
        let slots: Vec<String> = blinc_runtime::component::with_component_registry(|r| {
            r.get_by_name(name)
                .map(|def| {
                    def.props
                        .iter()
                        .filter_map(|p| {
                            let n = p.name.as_ref();
                            (n == "children" || n.starts_with("slot_")).then(|| n.to_string())
                        })
                        .collect()
                })
                .unwrap_or_default()
        });
        if slots.is_empty() { None } else { Some(slots) }
    }

    /// Legacy — unused after refactor.
    #[allow(dead_code)]
    fn callee_takes_children(call: &TypedCall) -> bool {
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
                .map(|def| def.props.iter().any(|p| p.name.as_ref() == "children"))
                .unwrap_or(false)
        })
    }

    let mut counter: u32 = 0;
    for decl in program.declarations.iter_mut() {
        match &mut decl.node {
            TypedDeclaration::Function(func) => {
                if let Some(body) = func.body.as_mut() {
                    for stmt in &mut body.statements {
                        walk_stmt(stmt, &mut counter, value_returning_symbols);
                    }
                }
            }
            TypedDeclaration::Impl(imp) => {
                for method in &mut imp.methods {
                    if let Some(body) = method.body.as_mut() {
                        for stmt in &mut body.statements {
                            walk_stmt(stmt, &mut counter, value_returning_symbols);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
