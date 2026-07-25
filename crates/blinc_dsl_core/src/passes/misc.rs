//! Return normalisation, call-site keys, and compound-assign desugaring.

use super::*;
use crate::*;

/// Append `Return(None)` to user fns so the body classifier can't promote a
/// trailing Expression into a value-bearing return.
pub(crate) fn ensure_unit_return(program: &mut TypedProgram) {
    use zyntax_typed_ast::TypedDeclaration;

    fn add_trailing_return_if_missing(body: &mut zyntax_typed_ast::typed_ast::TypedBlock) {
        let trailing_is_return = matches!(
            body.statements.last().map(|s| &s.node),
            Some(TypedStatement::Return(_))
        );
        if !trailing_is_return {
            body.statements.push(typed_node(
                TypedStatement::Return(None),
                Type::Primitive(PrimitiveType::Unit),
                Span::default(),
            ));
        }
    }

    for decl in program.declarations.iter_mut() {
        match &mut decl.node {
            TypedDeclaration::Function(func) => {
                if func.is_external {
                    continue;
                }
                if let Some(body) = func.body.as_mut() {
                    add_trailing_return_if_missing(body);
                }
            }
            // Impl methods compile to `<TypeName>$<method>` free fns — need the
            // same `Return(None)` so `call::<()>` doesn't hit the value-return path.
            TypedDeclaration::Impl(imp) => {
                for method in &mut imp.methods {
                    if let Some(body) = method.body.as_mut() {
                        add_trailing_return_if_missing(body);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Prepend a path-derived `u64` call-site key as the leading positional
/// argument to every widget-view call (substrate primitives + user
/// components).
///
/// **Substrate primitives** (e.g. `$Blinc$Button$view`): widget FFIs
/// consume the leading u64 as the state-allocation seed via
/// `dsl_state_key`. Dup-labelled Buttons at distinct call sites hold
/// distinct state because their span hashes differ.
///
/// **User components** (e.g. `Counter$view`): the `__instance_id__: u64`
/// param injected by [`inject_user_view_instance_id_params`] catches
/// the value; downstream calls inside Counter's view body emit
/// `__instance_id__ ^ LOCAL_HASH` instead of just `LOCAL_HASH`, so two
/// `Counter()` invocations produce sub-trees with distinct keys even
/// though Counter's body source is shared.
///
/// The XOR composition is the runtime piece that makes the
/// shared-body case work — `LOCAL_HASH` alone is identical across all
/// Counter instances (same source position), but XOR'd with the
/// caller's distinct `__instance_id__`, the composed key is per-instance.
///
/// MUST run AFTER `lower_children_arrays_to_blocks` so we walk the
/// final shape of widget calls including those that were moved into
/// `__push_child__` arg positions during children-array expansion.
pub(crate) fn inject_call_site_keys(program: &mut TypedProgram, filename: &str) {
    use zyntax_typed_ast::typed_ast::{
        TypedBinary, TypedDeclaration, TypedExpression, TypedLiteral,
    };

    /// Is `callee_name` a substrate-primitive view symbol (auto-injected
    /// leading u64; FFI consumes it)?
    fn is_substrate_view_symbol(callee_name: &str) -> bool {
        crate::abi::is_substrate_widget_view_public(callee_name)
    }

    /// Is `callee_name` a DSL-declared user-component view symbol
    /// (auto-prepended `__instance_id__` param by
    /// [`inject_user_view_instance_id_params`])?
    ///
    /// Heuristic: ends with `$view`, doesn't start with `$Blinc$`, and
    /// is in the component registry. This correctly excludes
    /// externally-registered widgets via `register_extern_widget_spec`
    /// (whose view_symbols start with `$Blinc$` by convention but
    /// whose Rust FFIs don't have the auto-injected leading u64).
    fn is_user_view_symbol(callee_name: &str) -> bool {
        if !callee_name.ends_with("$view") || callee_name.starts_with("$Blinc$") {
            return false;
        }
        let bare = match callee_name.strip_suffix("$view") {
            Some(s) => s,
            None => return false,
        };
        blinc_runtime::component::with_component_registry(|r| r.get_by_name(bare).is_some())
    }

    /// Does this Call need a leading call-site key injected? Either kind
    /// of view symbol qualifies.
    fn needs_call_id_injection(callee_name: &str) -> bool {
        is_substrate_view_symbol(callee_name) || is_user_view_symbol(callee_name)
    }

    /// Walker context — tracks whether we're inside a user-component
    /// view body (where injected keys must XOR with `__instance_id__`).
    struct Ctx<'a> {
        filename: &'a str,
        in_user_view: bool,
    }

    fn rewrite_expr(expr: &mut zyntax_typed_ast::TypedNode<TypedExpression>, ctx: &Ctx<'_>) {
        match &mut expr.node {
            TypedExpression::Call(call) => {
                rewrite_expr(&mut call.callee, ctx);
                for a in &mut call.positional_args {
                    rewrite_expr(a, ctx);
                }
                for n in &mut call.named_args {
                    rewrite_expr(&mut n.value, ctx);
                }
            }
            TypedExpression::MethodCall(mc) => {
                rewrite_expr(&mut mc.receiver, ctx);
                for a in &mut mc.positional_args {
                    rewrite_expr(a, ctx);
                }
            }
            TypedExpression::Binary(b) => {
                rewrite_expr(&mut b.left, ctx);
                rewrite_expr(&mut b.right, ctx);
            }
            TypedExpression::Unary(u) => rewrite_expr(&mut u.operand, ctx),
            TypedExpression::Field(f) => rewrite_expr(&mut f.object, ctx),
            TypedExpression::Index(idx) => {
                rewrite_expr(&mut idx.object, ctx);
                rewrite_expr(&mut idx.index, ctx);
            }
            TypedExpression::Array(items) | TypedExpression::Tuple(items) => {
                for it in items {
                    rewrite_expr(it, ctx);
                }
            }
            TypedExpression::Struct(s) => {
                for field in &mut s.fields {
                    rewrite_expr(&mut field.value, ctx);
                }
            }
            TypedExpression::Block(b) => rewrite_block(b, ctx),
            TypedExpression::If(if_expr) => {
                rewrite_expr(&mut if_expr.condition, ctx);
                rewrite_expr(&mut if_expr.then_branch, ctx);
                rewrite_expr(&mut if_expr.else_branch, ctx);
            }
            _ => {}
        }

        // Inject leading u64 key if this Call's callee is a view symbol
        // (substrate primitive OR user component). Use the call
        // expression's OWN span as the key source — that's the unique
        // source-location of this invocation.
        let span = expr.span;
        let TypedExpression::Call(call) = &mut expr.node else {
            return;
        };
        let TypedExpression::Variable(callee_name) = &call.callee.node else {
            return;
        };
        let Some(resolved) = callee_name.resolve_global() else {
            return;
        };
        if !needs_call_id_injection(resolved.as_ref()) {
            return;
        }

        // Build a path-shaped key: `ComponentName[.className]:hex_offset`.
        // The `class` arg, when present and a string literal, contributes
        // to identity — `Button(class="hero")` and `Button(class="cta")`
        // at the same source position diverge. Look it up via the
        // component registry's prop list, which gives us the position
        // of the `class` slot for this widget.
        let component_name = strip_view_suffix(resolved.as_ref()).unwrap_or("");
        let class_name = extract_class_arg(call, resolved.as_ref());
        let key = call_site_path_id(
            ctx.filename,
            span.start,
            component_name,
            class_name.as_deref(),
            None, // id args aren't a thing on substrate primitives today
        );

        // Cranelift backend (zyntax-compiler) doesn't handle
        // `HirConstant::U64` in its `value_map` population step — the
        // match at `cranelift_backend.rs:1471-1499` only knows about
        // I8/I16/I32/U32/I64/Bool/F32/F64 and silently `continue`s
        // for anything else. We type the literal as I64 (same bit-
        // width, same calling-convention slot) and let the abi.rs side
        // tag the param as TypeTag::U64 — the int is reinterpreted as
        // u64 on the Rust receive side without any value mangling.
        let literal = zyntax_typed_ast::TypedNode::new(
            TypedExpression::Literal(TypedLiteral::Integer(key as i64 as i128)),
            Type::Primitive(PrimitiveType::I64),
            span,
        );

        // When INSIDE a user-component view body, the leading arg is
        // `__instance_id__ ^ LOCAL_LITERAL` so the caller's distinct
        // instance id distinguishes each Counter() invocation's
        // sub-tree from another's. At the TOP-LEVEL view body (or any
        // non-user-view function), there's no `__instance_id__` in
        // scope; the literal stands alone.
        let key_arg = if ctx.in_user_view {
            let instance_id_var = zyntax_typed_ast::TypedNode::new(
                TypedExpression::Variable(zyntax_typed_ast::InternedString::new_global(
                    "__instance_id__",
                )),
                Type::Primitive(PrimitiveType::I64),
                span,
            );
            let xor_expr = TypedExpression::Binary(TypedBinary {
                op: zyntax_typed_ast::typed_ast::BinaryOp::BitXor,
                left: Box::new(instance_id_var),
                right: Box::new(literal),
            });
            zyntax_typed_ast::TypedNode::new(xor_expr, Type::Primitive(PrimitiveType::I64), span)
        } else {
            literal
        };
        call.positional_args.insert(0, key_arg);
    }

    /// Strip the `$Blinc$` prefix (if any) and the `$view` suffix from
    /// a view symbol to recover the bare component name. Returns `None`
    /// if the symbol doesn't have the expected shape.
    fn strip_view_suffix(view_symbol: &str) -> Option<&str> {
        let stripped = view_symbol.strip_suffix("$view")?;
        Some(stripped.strip_prefix("$Blinc$").unwrap_or(stripped))
    }

    /// Find the `class` arg in `call`'s positional_args and return its
    /// string-literal value, if present. The arg's POSITION is looked
    /// up from the component registry's prop list — `class` is usually
    /// the 3rd or 4th positional for substrate primitives but the exact
    /// index varies (e.g. `Image` has no class slot).
    fn extract_class_arg(
        call: &zyntax_typed_ast::typed_ast::TypedCall,
        callee_name: &str,
    ) -> Option<String> {
        let component_name = strip_view_suffix(callee_name)?;
        let class_idx = blinc_runtime::component::with_component_registry(|r| {
            r.get_by_name(component_name)
                .and_then(|def| def.props.iter().position(|p| p.name.as_ref() == "class"))
        })?;
        let class_arg = call.positional_args.get(class_idx)?;
        let TypedExpression::Literal(TypedLiteral::String(s)) = &class_arg.node else {
            return None;
        };
        s.resolve_global().map(|s| s.to_string())
    }

    fn rewrite_block(block: &mut zyntax_typed_ast::typed_ast::TypedBlock, ctx: &Ctx<'_>) {
        for stmt in &mut block.statements {
            rewrite_stmt(stmt, ctx);
        }
    }

    fn rewrite_stmt(stmt: &mut zyntax_typed_ast::TypedNode<TypedStatement>, ctx: &Ctx<'_>) {
        match &mut stmt.node {
            TypedStatement::Expression(e) => rewrite_expr(e, ctx),
            TypedStatement::Let(l) => {
                if let Some(init) = &mut l.initializer {
                    rewrite_expr(init, ctx);
                }
            }
            TypedStatement::Return(Some(e)) => rewrite_expr(e, ctx),
            TypedStatement::If(if_stmt) => {
                rewrite_expr(&mut if_stmt.condition, ctx);
                rewrite_block(&mut if_stmt.then_block, ctx);
                if let Some(else_block) = &mut if_stmt.else_block {
                    rewrite_block(else_block, ctx);
                }
            }
            TypedStatement::While(w) => {
                rewrite_expr(&mut w.condition, ctx);
                rewrite_block(&mut w.body, ctx);
            }
            TypedStatement::Block(b) => rewrite_block(b, ctx),
            _ => {}
        }
    }

    for decl in &mut program.declarations {
        match &mut decl.node {
            // Top-level functions — `render_view`, plus any user helpers.
            // None of these have `__instance_id__` in scope.
            TypedDeclaration::Function(func) => {
                if let Some(body) = &mut func.body {
                    let ctx = Ctx {
                        filename,
                        in_user_view: false,
                    };
                    rewrite_block(body, &ctx);
                }
            }
            // Impl methods. A method named `view` is the user-component
            // view body — `__instance_id__` IS in scope as the leading
            // synthetic param, so children inside the body must XOR
            // with it. Other methods (init, helpers) walk with
            // in_user_view=false.
            TypedDeclaration::Impl(imp) => {
                for method in &mut imp.methods {
                    if let Some(body) = &mut method.body {
                        let in_user_view = method.name.resolve_global().as_deref() == Some("view");
                        let ctx = Ctx {
                            filename,
                            in_user_view,
                        };
                        rewrite_block(body, &ctx);
                    }
                }
            }
            _ => {}
        }
    }
}

// =====================================================================
// FSM context + transition actions
// =====================================================================
//
// The grammar emits three new shapes when an FSM uses extended state:
//
//   1. `__fsm_context_field__("name", "ty", default_literal)` markers
//      inside `__fsm_meta__` — one per `context { … }` field.
//   2. `__fsm_transition__("from", "event", "to", Block { stmts })` —
//      the optional 4th positional arg carries the action body.
//   3. `__compound_assign__("Add", lhs, rhs)` marker calls produced by
//      `+=` / `-=` / `*=` / `/=` (works everywhere statements appear,
//      not just inside FSM bodies).
//
// The lowering pipeline turns these into ordinary Blinc machinery:
//   - Context fields become top-level `signal __fsm_ctx_<Fsm>_<field>: <ty>`
//     decls so `resolve_signal_calls` handles get/set uniformly.
//   - Action bodies get lifted to top-level fns
//     `__fsm_action_<Fsm>_<idx>__` whose body has `ctx.<field>` rewritten
//     to the mangled signal name. The `__fsm_transition__` marker's 4th
//     arg is rewritten from Block to a string-literal carrying the lifted
//     symbol name so `populate_fsm_registry_pass` reads it as
//     `TransitionAction::Symbol(...)`.
//   - Tick-guard expressions go through the same `ctx.<field>` rewrite.
//   - Compound-assign markers expand to plain `target = target op value`
//     (with the LHS cloned).
//   - From-outside dotted access `<Fsm>.<field>` (typically followed by
//     `.get()` / `.set(...)` or appearing on either side of an `=`) is
//     rewritten to the mangled signal identifier so `resolve_signal_calls`
//     picks it up.

/// Desugar `__compound_assign__("Add", lhs, rhs)` marker calls into
/// `lhs = lhs <op> rhs`. Runs early so subsequent passes only see plain
/// `Binary Assign` shapes — no special-casing of `+=` etc. downstream.
///
/// Walks every expression position reachable from the program's
/// declarations, including inside lambda bodies and nested blocks. The
/// LHS is cloned to share between the outer Assign and the inner Binary
/// arithmetic — TypedNodes are owned trees, no aliasing concerns.
pub(crate) fn desugar_compound_assigns(program: &mut TypedProgram) {
    use zyntax_typed_ast::TypedNode;
    use zyntax_typed_ast::typed_ast::{
        BinaryOp, TypedBinary, TypedCall, TypedDeclaration, TypedExpression, TypedLambdaBody,
        TypedLiteral,
    };

    fn op_from_str(s: &str) -> Option<BinaryOp> {
        match s {
            "+=" => Some(BinaryOp::Add),
            "-=" => Some(BinaryOp::Sub),
            "*=" => Some(BinaryOp::Mul),
            "/=" => Some(BinaryOp::Div),
            _ => None,
        }
    }

    fn rewrite_expr(node: &mut TypedNode<TypedExpression>) {
        // Walk children first so nested compound assigns inside arg
        // expressions are handled before the outer is examined.
        match &mut node.node {
            TypedExpression::Call(c) => {
                rewrite_expr(&mut c.callee);
                for a in &mut c.positional_args {
                    rewrite_expr(a);
                }
            }
            TypedExpression::MethodCall(mc) => {
                rewrite_expr(&mut mc.receiver);
                for a in &mut mc.positional_args {
                    rewrite_expr(a);
                }
            }
            TypedExpression::Binary(b) => {
                rewrite_expr(&mut b.left);
                rewrite_expr(&mut b.right);
            }
            TypedExpression::Unary(u) => rewrite_expr(&mut u.operand),
            TypedExpression::Field(f) => rewrite_expr(&mut f.object),
            TypedExpression::Index(i) => {
                rewrite_expr(&mut i.object);
                rewrite_expr(&mut i.index);
            }
            TypedExpression::Array(items) | TypedExpression::Tuple(items) => {
                for it in items {
                    rewrite_expr(it);
                }
            }
            TypedExpression::Block(block) => {
                for stmt in &mut block.statements {
                    rewrite_stmt(stmt);
                }
            }
            TypedExpression::If(if_expr) => {
                rewrite_expr(&mut if_expr.condition);
                rewrite_expr(&mut if_expr.then_branch);
                rewrite_expr(&mut if_expr.else_branch);
            }
            TypedExpression::Lambda(lam) => match &mut lam.body {
                TypedLambdaBody::Expression(e) => rewrite_expr(e),
                TypedLambdaBody::Block(block) => {
                    for stmt in &mut block.statements {
                        rewrite_stmt(stmt);
                    }
                }
            },
            _ => {}
        }

        // Now look for `__compound_assign__(op_str, lhs, rhs)` at this
        // node and rewrite in place.
        let TypedExpression::Call(call) = &node.node else {
            return;
        };
        let TypedExpression::Variable(callee_name) = &call.callee.node else {
            return;
        };
        if callee_name.resolve_global().as_deref() != Some("__compound_assign__") {
            return;
        }
        if call.positional_args.len() != 3 {
            return;
        }
        let TypedExpression::Literal(TypedLiteral::String(op_intern)) =
            &call.positional_args[0].node
        else {
            return;
        };
        let Some(op_str) = op_intern.resolve_global() else {
            return;
        };
        let Some(op) = op_from_str(&op_str) else {
            return;
        };
        let lhs = call.positional_args[1].clone();
        let rhs = call.positional_args[2].clone();
        let span = node.span;
        let lhs_for_rhs = lhs.clone();
        let inner_ty = lhs.ty.clone();
        let combined = TypedNode::new(
            TypedExpression::Binary(TypedBinary {
                op,
                left: Box::new(lhs_for_rhs),
                right: Box::new(rhs),
            }),
            inner_ty,
            span,
        );
        node.node = TypedExpression::Binary(TypedBinary {
            op: BinaryOp::Assign,
            left: Box::new(lhs),
            right: Box::new(combined),
        });
        node.ty = Type::Primitive(PrimitiveType::Unit);
        // Silence "unused" lints on imports only used through pattern matches above.
        let _ = std::any::type_name::<TypedCall>();
    }

    fn rewrite_stmt(stmt: &mut TypedNode<TypedStatement>) {
        match &mut stmt.node {
            TypedStatement::Expression(e) => rewrite_expr(e),
            TypedStatement::Let(l) => {
                if let Some(init) = &mut l.initializer {
                    rewrite_expr(init);
                }
            }
            TypedStatement::Return(Some(e)) => rewrite_expr(e),
            TypedStatement::Block(b) => {
                for inner in &mut b.statements {
                    rewrite_stmt(inner);
                }
            }
            TypedStatement::If(if_stmt) => {
                rewrite_expr(&mut if_stmt.condition);
                for inner in &mut if_stmt.then_block.statements {
                    rewrite_stmt(inner);
                }
                if let Some(else_block) = &mut if_stmt.else_block {
                    for inner in &mut else_block.statements {
                        rewrite_stmt(inner);
                    }
                }
            }
            TypedStatement::While(w) => {
                rewrite_expr(&mut w.condition);
                for inner in &mut w.body.statements {
                    rewrite_stmt(inner);
                }
            }
            _ => {}
        }
    }

    for decl in &mut program.declarations {
        match &mut decl.node {
            TypedDeclaration::Function(f) => {
                if let Some(body) = &mut f.body {
                    for stmt in &mut body.statements {
                        rewrite_stmt(stmt);
                    }
                }
            }
            TypedDeclaration::Impl(imp) => {
                for m in &mut imp.methods {
                    if let Some(body) = &mut m.body {
                        for stmt in &mut body.statements {
                            rewrite_stmt(stmt);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
