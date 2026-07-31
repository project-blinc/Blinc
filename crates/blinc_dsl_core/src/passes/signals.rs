//! Signal decl recognition and `signal.get()/.set()` call resolution.

use super::*;
use crate::*;

/// stable `SignalId.to_raw()` minted via the process-global signal
/// registry. Lives at module scope so the inner helper `fn` items in
/// [`resolve_signal_calls`] can name the type in their signatures.
#[derive(Clone)]
struct SignalEntry {
    ty: Type,
    /// `SignalId.to_raw()` cast to i64 — Cranelift's value-map population
    /// doesn't handle `HirConstant::U64`, so we stay in i64-land.
    id_raw: i64,
}

/// Last `= <literal>` seen for each signal name, so a reload can tell an
/// edited default from an unchanged one.
///
/// Process-global because the signal registry is: a hot reload builds a
/// fresh `BlincDsl`, and per-instance state would make every reload look
/// like a first sight and clobber the live value.
fn declared_defaults() -> &'static std::sync::Mutex<std::collections::HashMap<String, String>> {
    static D: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, String>>> =
        std::sync::OnceLock::new();
    D.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// Record this compile's declared default and report whether it differs
/// from the one before it.
fn declared_default_changed(name: &str, lit: &zyntax_typed_ast::typed_ast::TypedLiteral) -> bool {
    use zyntax_typed_ast::typed_ast::TypedLiteral;
    // Tagged, so `7` and `"7"` read as different declarations.
    let key = match lit {
        TypedLiteral::Integer(n) => format!("i:{n}"),
        TypedLiteral::Float(f) => format!("f:{f}"),
        TypedLiteral::Bool(b) => format!("b:{b}"),
        TypedLiteral::String(sym) => format!("s:{}", sym.resolve_global().unwrap_or_default()),
        other => format!("?:{other:?}"),
    };
    let mut map = declared_defaults()
        .lock()
        .expect("declared defaults poisoned");
    map.insert(name.to_string(), key.clone()) != Some(key)
}

/// Write a declared initial value into a freshly minted signal.
///
/// The literal's own type is what the author wrote; the signal's type
/// is what they declared. Integer-to-float is the one widening worth
/// supporting (`signal ratio: f64 = 1`), since a bare `1` is the
/// natural way to write it. Anything else mismatched is ignored rather
/// than guessed at.
fn seed_signal(
    name: &str,
    ty: blinc_runtime::signal::SignalType,
    lit: &zyntax_typed_ast::typed_ast::TypedLiteral,
) {
    use blinc_runtime::signal::{self, SignalType};
    use zyntax_typed_ast::typed_ast::TypedLiteral;
    match (ty, lit) {
        (SignalType::I32, TypedLiteral::Integer(n)) => signal::set_i32(name, *n as i32),
        // No i64 setter in the runtime helpers; the id is the same
        // registry entry either way, so go through the typed handle.
        (SignalType::I64, TypedLiteral::Integer(n)) => {
            if let Some((id, _)) = signal::lookup(name) {
                blinc_core::reactive::Signal::<i64>::from_id(
                    blinc_core::reactive::SignalId::from_raw(id),
                )
                .set(*n as i64);
            }
        }
        (SignalType::F64, TypedLiteral::Float(f)) => signal::set_f64(name, *f),
        (SignalType::F64, TypedLiteral::Integer(n)) => signal::set_f64(name, *n as f64),
        (SignalType::Bool, TypedLiteral::Bool(b)) => signal::set_bool(name, *b),
        (SignalType::String, TypedLiteral::String(sym)) => {
            if let Some(v) = sym.resolve_global() {
                signal::set_str(name, v.to_string());
            }
        }
        _ => tracing::warn!(
            signal = name,
            ?ty,
            "initial value does not match the declared type -- ignored"
        ),
    }
}

pub(crate) fn resolve_signal_calls(program: &mut TypedProgram) {
    use std::collections::HashMap;
    use zyntax_typed_ast::InternedString;
    use zyntax_typed_ast::typed_ast::{TypedCall, TypedDeclaration, TypedExpression, TypedLiteral};

    // Step 1: collect signal name → (type, id_raw). The id_raw is
    // minted on first encounter via the process-global
    // `blinc_dsl_core::signal_registry` — that calls
    // `blinc_core::reactive::signal(default)` and caches the resulting
    // `SignalId.to_raw()`. Subsequent compiles of the same source reuse
    // the existing id.
    //
    // `SignalEntry` declared above this fn so the helper `fn` items
    // (`rewrite_expr`, `rewrite_block`, `rewrite_stmt`) can name the
    // type in their signatures without lifting it to module scope.
    let mut signals: HashMap<InternedString, SignalEntry> = HashMap::new();
    for decl in &program.declarations {
        let TypedDeclaration::Function(func) = &decl.node else {
            continue;
        };
        if !is_signal_decl(func) {
            continue;
        }
        // `signal items: list` arrives as a Named/Unresolved type rather
        // than a primitive -- there is no primitive for it, on purpose:
        // nothing may try to read a list inline in the JIT.
        let sig_ty = match &func.return_type {
            Type::Primitive(PrimitiveType::I32) => blinc_runtime::signal::SignalType::I32,
            Type::Primitive(PrimitiveType::I64) => blinc_runtime::signal::SignalType::I64,
            Type::Primitive(PrimitiveType::F64) => blinc_runtime::signal::SignalType::F64,
            Type::Primitive(PrimitiveType::String) => blinc_runtime::signal::SignalType::String,
            Type::Primitive(PrimitiveType::Bool) => blinc_runtime::signal::SignalType::Bool,
            other if is_string_list_type(other) => blinc_runtime::signal::SignalType::StringList,
            _ => continue,
        };
        let Some(name_str) = func.name.resolve_global() else {
            continue;
        };
        // `= <literal>` applies on first sight, and again whenever the
        // DECLARED value itself changes.
        //
        // The registry outlives a compile, so re-applying on every
        // compile would throw away whatever the user has typed or
        // clicked -- but never re-applying means editing the default in
        // source does nothing on a hot reload, which is just as wrong.
        // Comparing against the last DECLARED default separates the two:
        // an edit to the source is an authoring action and wins, while a
        // recompile of unchanged source leaves the live value alone.
        let is_new = blinc_runtime::signal::lookup(name_str.as_ref()).is_none();
        let id_raw_u64 = blinc_runtime::signal::mint_or_get(name_str.as_ref(), sig_ty);
        if let Some(lit) = crate::passes::signal_initial_literal(func) {
            // Recorded unconditionally, BEFORE the `is_new` test: `||`
            // short-circuits, so folding this into the condition meant
            // the first compile never recorded anything and the next one
            // read "no previous default", called it a change, and
            // clobbered the live value.
            let changed = declared_default_changed(name_str.as_ref(), lit);
            if is_new || changed {
                seed_signal(name_str.as_ref(), sig_ty, lit);
            }
        } else {
            // The default was removed; forget it, so re-adding the same
            // literal later still reads as a change.
            declared_defaults()
                .lock()
                .expect("declared defaults poisoned")
                .remove(name_str.as_ref() as &str);
        }
        signals.insert(
            func.name,
            SignalEntry {
                ty: func.return_type.clone(),
                // i64 over the wire — the Cranelift backend lacks a
                // `HirConstant::U64` case in its value_map population
                // (see commit 54dc831b for context). Re-cast back to
                // u64 inside the extern.
                id_raw: id_raw_u64 as i64,
            },
        );
    }

    if signals.is_empty() {
        return;
    }

    // Step 2: rewrite `<sig>.get()` → `__signal_get_by_id_<T>(<id_literal>)`.
    fn typed_signal_extern_name(ty: &Type) -> Option<&'static str> {
        match ty {
            Type::Primitive(PrimitiveType::I32) => Some("__signal_get_by_id_i32"),
            Type::Primitive(PrimitiveType::I64) => Some("__signal_get_by_id_i64"),
            Type::Primitive(PrimitiveType::F64) => Some("__signal_get_by_id_f64"),
            Type::Primitive(PrimitiveType::String) => Some("__signal_get_by_id_string"),
            Type::Primitive(PrimitiveType::Bool) => Some("__signal_get_by_id_bool"),
            _ => None,
        }
    }

    fn typed_signal_setter_extern_name(ty: &Type) -> Option<&'static str> {
        match ty {
            Type::Primitive(PrimitiveType::I32) => Some("__signal_set_by_id_i32"),
            Type::Primitive(PrimitiveType::I64) => Some("__signal_set_by_id_i64"),
            Type::Primitive(PrimitiveType::F64) => Some("__signal_set_by_id_f64"),
            Type::Primitive(PrimitiveType::String) => Some("__signal_set_by_id_string"),
            Type::Primitive(PrimitiveType::Bool) => Some("__signal_set_by_id_bool"),
            _ => None,
        }
    }

    /// Replace a bare `Variable(<signal>)` read with
    /// `__signal_get_by_id_<T>(<id_literal>)`.
    ///
    /// Only valid in a value context — see the `Lambda` arm in
    /// [`rewrite_expr`]. `shadowed` carries the enclosing lambda's
    /// parameter names so a parameter that happens to share a signal's
    /// name still resolves to the parameter.
    fn rewrite_bare_reads(
        expr: &mut zyntax_typed_ast::TypedNode<TypedExpression>,
        signals: &HashMap<InternedString, SignalEntry>,
        shadowed: &[InternedString],
    ) {
        if let TypedExpression::Variable(name) = &expr.node
            && !shadowed.contains(name)
            && let Some(entry) = signals.get(name)
            && let Some(getter) = typed_signal_extern_name(&entry.ty)
        {
            let id_arg = zyntax_typed_ast::TypedNode::new(
                TypedExpression::Literal(TypedLiteral::Integer(entry.id_raw as i128)),
                Type::Primitive(PrimitiveType::I64),
                expr.span,
            );
            let callee = zyntax_typed_ast::TypedNode::new(
                TypedExpression::Variable(InternedString::new_global(getter)),
                Type::Unknown,
                expr.span,
            );
            expr.node = TypedExpression::Call(TypedCall {
                callee: Box::new(callee),
                positional_args: vec![id_arg],
                named_args: Vec::new(),
                type_args: Vec::new(),
            });
            // The read yields the signal's own type, not the enclosing
            // expression's — a `: bool` computed over an i64 signal
            // reads i64 and compares.
            expr.ty = entry.ty.clone();
            return;
        }

        match &mut expr.node {
            TypedExpression::Binary(b) => {
                rewrite_bare_reads(&mut b.left, signals, shadowed);
                rewrite_bare_reads(&mut b.right, signals, shadowed);
            }
            TypedExpression::Unary(u) => rewrite_bare_reads(&mut u.operand, signals, shadowed),
            TypedExpression::Call(c) => {
                for a in &mut c.positional_args {
                    rewrite_bare_reads(a, signals, shadowed);
                }
            }
            TypedExpression::MethodCall(mc) => {
                for a in &mut mc.positional_args {
                    rewrite_bare_reads(a, signals, shadowed);
                }
            }
            TypedExpression::Index(idx) => {
                rewrite_bare_reads(&mut idx.object, signals, shadowed);
                rewrite_bare_reads(&mut idx.index, signals, shadowed);
            }
            TypedExpression::Array(items) | TypedExpression::Tuple(items) => {
                for it in items {
                    rewrite_bare_reads(it, signals, shadowed);
                }
            }
            TypedExpression::If(if_expr) => {
                rewrite_bare_reads(&mut if_expr.condition, signals, shadowed);
                rewrite_bare_reads(&mut if_expr.then_branch, signals, shadowed);
                rewrite_bare_reads(&mut if_expr.else_branch, signals, shadowed);
            }
            TypedExpression::Block(block) => {
                for stmt in &mut block.statements {
                    rewrite_bare_reads_in_stmt(stmt, signals, shadowed);
                }
            }
            _ => {}
        }
    }

    fn rewrite_bare_reads_in_stmt(
        stmt: &mut zyntax_typed_ast::TypedNode<zyntax_typed_ast::TypedStatement>,
        signals: &HashMap<InternedString, SignalEntry>,
        shadowed: &[InternedString],
    ) {
        use zyntax_typed_ast::TypedStatement;
        match &mut stmt.node {
            TypedStatement::Expression(e) => rewrite_bare_reads(e, signals, shadowed),
            TypedStatement::Return(Some(e)) => rewrite_bare_reads(e, signals, shadowed),
            TypedStatement::Let(l) => {
                if let Some(init) = &mut l.initializer {
                    rewrite_bare_reads(init, signals, shadowed);
                }
            }
            _ => {}
        }
    }

    fn rewrite_expr(
        expr: &mut zyntax_typed_ast::TypedNode<TypedExpression>,
        signals: &HashMap<InternedString, SignalEntry>,
    ) {
        // MUST intercept `<signal> = <expr>` BEFORE the recursive walk — the
        // LHS `Variable` doesn't otherwise trigger a rewrite.
        if let TypedExpression::Binary(b) = &expr.node
            && b.op == zyntax_typed_ast::typed_ast::BinaryOp::Assign
            && let TypedExpression::Variable(name) = &b.left.node
            && let Some(entry) = signals.get(name).cloned()
            && let Some(setter) = typed_signal_setter_extern_name(&entry.ty)
        {
            // Rewrite RHS first so nested signal reads route through getters.
            let mut rhs = (*b.right).clone();
            rewrite_expr(&mut rhs, signals);

            let id_arg = zyntax_typed_ast::TypedNode::new(
                TypedExpression::Literal(TypedLiteral::Integer(entry.id_raw as i128)),
                Type::Primitive(PrimitiveType::I64),
                expr.span,
            );
            let callee = zyntax_typed_ast::TypedNode::new(
                TypedExpression::Variable(InternedString::new_global(setter)),
                Type::Unknown,
                expr.span,
            );
            expr.node = TypedExpression::Call(TypedCall {
                callee: Box::new(callee),
                positional_args: vec![id_arg, rhs],
                named_args: vec![],
                type_args: vec![],
            });
            expr.ty = Type::Primitive(PrimitiveType::Unit);
            return;
        }

        // Children first so nested signal calls (e.g. `text(count.get())`) are rewritten.
        // EXCEPTION: MethodCall.receiver and Call+Field.object aren't walked
        // when they're a bare `Variable(<signal>)` — the dedicated
        // `count.get()` / `count.set(...)` rewrite below needs to see the
        // receiver as a Variable, not as a pre-rewritten getter-Call.
        match &mut expr.node {
            TypedExpression::Binary(b) => {
                rewrite_expr(&mut b.left, signals);
                rewrite_expr(&mut b.right, signals);
            }
            TypedExpression::Unary(u) => {
                rewrite_expr(&mut u.operand, signals);
            }
            TypedExpression::Call(c) => {
                // If the callee is `Field { object: Variable(<signal>), ... }`,
                // skip rewriting the object so the post-walk MethodCall/Call+Field
                // handler can match `<signal>.<method>(args)`. Args are still walked.
                let preserve_callee = matches!(
                    &c.callee.node,
                    TypedExpression::Field(f)
                        if matches!(
                            &f.object.node,
                            TypedExpression::Variable(n) if signals.contains_key(n)
                        )
                );
                if !preserve_callee {
                    rewrite_expr(&mut c.callee, signals);
                }
                for a in &mut c.positional_args {
                    rewrite_expr(a, signals);
                }
            }
            TypedExpression::Field(f) => {
                rewrite_expr(&mut f.object, signals);
            }
            TypedExpression::Index(idx) => {
                rewrite_expr(&mut idx.object, signals);
                rewrite_expr(&mut idx.index, signals);
            }
            TypedExpression::Array(items) | TypedExpression::Tuple(items) => {
                for item in items {
                    rewrite_expr(item, signals);
                }
            }
            TypedExpression::MethodCall(mc) => {
                let preserve_receiver = matches!(
                    &mc.receiver.node,
                    TypedExpression::Variable(n) if signals.contains_key(n)
                );
                if !preserve_receiver {
                    rewrite_expr(&mut mc.receiver, signals);
                }
                for a in &mut mc.positional_args {
                    rewrite_expr(a, signals);
                }
            }
            TypedExpression::Block(block) => {
                rewrite_block(block, signals);
            }
            TypedExpression::If(if_expr) => {
                rewrite_expr(&mut if_expr.condition, signals);
                rewrite_expr(&mut if_expr.then_branch, signals);
                rewrite_expr(&mut if_expr.else_branch, signals);
            }
            TypedExpression::Lambda(lam) => {
                // A lambda body is a pure value context, so a bare
                // `<signal>` there means "read it" — unlike the widget-arg
                // position, where the bare name is the signal *handle* and
                // `lower_reactive_args` needs it left alone.
                //
                // `.get()` shapes are rewritten by the recursive walk;
                // `rewrite_bare_reads` then catches what's left. Without it
                // the read never lowers: SSA can't resolve the name and
                // falls back to an undefined variable (silently reads
                // garbage) or, when the name collides with a function, to
                // an extern ref that fails to link.
                let shadowed: Vec<_> = lam.params.iter().map(|p| p.name).collect();
                match &mut lam.body {
                    zyntax_typed_ast::typed_ast::TypedLambdaBody::Expression(e) => {
                        rewrite_expr(e, signals);
                        rewrite_bare_reads(e, signals, &shadowed);
                    }
                    zyntax_typed_ast::typed_ast::TypedLambdaBody::Block(block) => {
                        rewrite_block(block, signals);
                        for stmt in &mut block.statements {
                            rewrite_bare_reads_in_stmt(stmt, signals, &shadowed);
                        }
                    }
                }
            }
            _ => {}
        }

        // `.get()` / `.set(x)` lands in two AST shapes:
        //   1. `MethodCall` — expression position (postfix-expr).
        //   2. `Call { callee: Field { ... }, ... }` — statement position.
        // Recognise both.
        let method_call = match &expr.node {
            TypedExpression::MethodCall(mc) => {
                if let TypedExpression::Variable(receiver_name) = &mc.receiver.node {
                    Some((
                        *receiver_name,
                        mc.method,
                        mc.positional_args.clone(),
                        expr.span,
                    ))
                } else {
                    None
                }
            }
            TypedExpression::Call(c) => {
                if let TypedExpression::Field(f) = &c.callee.node {
                    if let TypedExpression::Variable(receiver_name) = &f.object.node {
                        Some((
                            *receiver_name,
                            f.field,
                            c.positional_args.clone(),
                            expr.span,
                        ))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        };

        let Some((receiver_name, method, args, span)) = method_call else {
            // Bare `Variable(<signal>)` read — not `.get()`, not an
            // Assign LHS (those returned at the top). Rewrite to a
            // getter call so the JIT issues an actual signal load
            // instead of treating the name as an undefined local.
            //
            // SCOPE: only `__fsm_ctx_*` (FSM context-field) signals.
            // User-declared signals are left as bare Variables because
            // `lower_styling_args_to_overlays` (which runs after us)
            // pattern-matches on bare-Variable args to widget props
            // and rewrites them to the `_signal__` overlay variant for
            // LIVE binding at paint time. Forcing a getter call here
            // would freeze the value at compile time and break that
            // feature for `Div(bg = bg_color)`-style code.
            //
            // For ctx-signals the force-wrap is still required —
            // action bodies use them in arithmetic (`ctx.pct + 0.1`)
            // and f-string interpolation. The styling-args pass has
            // its own recognizer for the wrapped
            // `__signal_get_by_id_<T>(id_literal)` shape so
            // `Div(opacity = Ticker.pct)` still binds live.
            if let TypedExpression::Variable(name) = &expr.node
                && let Some(entry) = signals.get(name).cloned()
                && name
                    .resolve_global()
                    .map(|s| s.starts_with("__fsm_ctx_"))
                    .unwrap_or(false)
                && let Some(extern_name) = typed_signal_extern_name(&entry.ty)
            {
                let span = expr.span;
                expr.node = TypedExpression::Call(TypedCall {
                    callee: Box::new(zyntax_typed_ast::TypedNode::new(
                        TypedExpression::Variable(InternedString::new_global(extern_name)),
                        Type::Unknown,
                        span,
                    )),
                    positional_args: vec![zyntax_typed_ast::TypedNode::new(
                        TypedExpression::Literal(TypedLiteral::Integer(entry.id_raw as i128)),
                        Type::Primitive(PrimitiveType::I64),
                        span,
                    )],
                    named_args: vec![],
                    type_args: vec![],
                });
                expr.ty = entry.ty;
            }
            return;
        };
        let Some(entry) = signals.get(&receiver_name).cloned() else {
            return;
        };
        let method_name = method.resolve_global().map(|s| s.to_string());
        match method_name.as_deref() {
            // `count.get()` — read. Zero args, returns the
            // signal's value type.
            Some("get") if args.is_empty() => {
                let Some(extern_name) = typed_signal_extern_name(&entry.ty) else {
                    return;
                };
                expr.node = TypedExpression::Call(TypedCall {
                    callee: Box::new(zyntax_typed_ast::TypedNode::new(
                        TypedExpression::Variable(InternedString::new_global(extern_name)),
                        Type::Unknown,
                        span,
                    )),
                    positional_args: vec![zyntax_typed_ast::TypedNode::new(
                        TypedExpression::Literal(TypedLiteral::Integer(entry.id_raw as i128)),
                        Type::Primitive(PrimitiveType::I64),
                        span,
                    )],
                    named_args: vec![],
                    type_args: vec![],
                });
                expr.ty = entry.ty;
            }
            // `count.set(value)` — write. Arg already child-rewritten.
            Some("set") if args.len() == 1 => {
                let Some(setter) = typed_signal_setter_extern_name(&entry.ty) else {
                    return;
                };
                let value = args.into_iter().next().expect("len == 1 just checked");
                expr.node = TypedExpression::Call(TypedCall {
                    callee: Box::new(zyntax_typed_ast::TypedNode::new(
                        TypedExpression::Variable(InternedString::new_global(setter)),
                        Type::Unknown,
                        span,
                    )),
                    positional_args: vec![
                        zyntax_typed_ast::TypedNode::new(
                            TypedExpression::Literal(TypedLiteral::Integer(entry.id_raw as i128)),
                            Type::Primitive(PrimitiveType::I64),
                            span,
                        ),
                        value,
                    ],
                    named_args: vec![],
                    type_args: vec![],
                });
                expr.ty = Type::Primitive(PrimitiveType::Unit);
            }
            _ => {}
        }
    }

    fn rewrite_block(
        block: &mut zyntax_typed_ast::typed_ast::TypedBlock,
        signals: &HashMap<InternedString, SignalEntry>,
    ) {
        for stmt in &mut block.statements {
            rewrite_stmt(stmt, signals);
        }
    }

    fn rewrite_stmt(
        stmt: &mut zyntax_typed_ast::TypedNode<TypedStatement>,
        signals: &HashMap<InternedString, SignalEntry>,
    ) {
        match &mut stmt.node {
            TypedStatement::Expression(e) => rewrite_expr(e, signals),
            TypedStatement::Let(l) => {
                if let Some(init) = &mut l.initializer {
                    rewrite_expr(init, signals);
                }
            }
            TypedStatement::Return(Some(e)) => rewrite_expr(e, signals),
            TypedStatement::If(if_stmt) => {
                rewrite_expr(&mut if_stmt.condition, signals);
                rewrite_block(&mut if_stmt.then_block, signals);
                if let Some(else_block) = &mut if_stmt.else_block {
                    rewrite_block(else_block, signals);
                }
            }
            TypedStatement::While(w) => {
                rewrite_expr(&mut w.condition, signals);
                rewrite_block(&mut w.body, signals);
            }
            TypedStatement::Block(b) => rewrite_block(b, signals),
            _ => {}
        }
    }

    for decl in &mut program.declarations {
        let TypedDeclaration::Function(func) = &mut decl.node else {
            continue;
        };
        if let Some(body) = &mut func.body {
            rewrite_block(body, &signals);
        }
    }
    for decl in &mut program.declarations {
        let TypedDeclaration::Impl(imp) = &mut decl.node else {
            continue;
        };
        for method in &mut imp.methods {
            if let Some(body) = &mut method.body {
                rewrite_block(body, &signals);
            }
        }
    }

    // Step 3: strip signal-marker decls (metadata only; usage was rewritten above).
    program.declarations.retain(|decl| {
        let TypedDeclaration::Function(func) = &decl.node else {
            return true;
        };
        !is_signal_decl(func)
    });
}

/// Is this the `list` spelling from `state_type_list`?
///
/// It reaches here as `Named` or `Unresolved` depending on whether a
/// type of that name is registered, and none is, so both are accepted.
pub(crate) fn is_string_list_type(ty: &Type) -> bool {
    match ty {
        Type::Unresolved(n) => n.resolve_global().as_deref() == Some("BlincStringList"),
        Type::Named { .. } => false,
        _ => false,
    }
}
