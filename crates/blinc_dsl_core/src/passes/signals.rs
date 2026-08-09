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
    /// `(fsm, field)` when this signal is an FSM context field, so its
    /// reads resolve per instance rather than to `id_raw`.
    ctx_origin: Option<(String, String)>,
}

/// The expression a read or write uses to name its signal.
///
/// An ordinary signal bakes its id: there is one, and it is known now.
/// An FSM context field cannot, because which signal `Play.pct` means
/// depends on which component instance is asking, and that is only known
/// while the code runs. It lowers instead to a call that resolves the
/// current instance's signal, carrying the baked id as the value to fall
/// back on when no machine backs the read.
fn signal_id_expr(
    entry: &SignalEntry,
    span: zyntax_typed_ast::Span,
) -> zyntax_typed_ast::TypedNode<zyntax_typed_ast::typed_ast::TypedExpression> {
    use zyntax_typed_ast::InternedString;
    use zyntax_typed_ast::typed_ast::{TypedCall, TypedExpression, TypedLiteral};

    let baked = zyntax_typed_ast::TypedNode::new(
        TypedExpression::Literal(TypedLiteral::Integer(entry.id_raw as i128)),
        Type::Primitive(PrimitiveType::I64),
        span,
    );
    let Some((fsm, field)) = &entry.ctx_origin else {
        return baked;
    };
    let str_arg = |s: &str| {
        zyntax_typed_ast::TypedNode::new(
            TypedExpression::Literal(TypedLiteral::String(InternedString::new_global(s))),
            Type::Primitive(PrimitiveType::String),
            span,
        )
    };
    zyntax_typed_ast::TypedNode::new(
        TypedExpression::Call(TypedCall {
            callee: Box::new(zyntax_typed_ast::TypedNode::new(
                TypedExpression::Variable(InternedString::new_global("__blinc_fsm_ctx_id__")),
                Type::Unknown,
                span,
            )),
            positional_args: vec![str_arg(fsm), str_arg(field), baked],
            named_args: Vec::new(),
            type_args: Vec::new(),
        }),
        Type::Primitive(PrimitiveType::I64),
        span,
    )
}

/// Declare `__blinc_fsm_ctx_id__` so the calls [`signal_id_expr`] emits
/// can be lowered.
///
/// Registering the host symbol is not enough on its own: lowering
/// resolves a call against the program's own declarations, and a call to
/// something undeclared makes it skip the whole enclosing function
/// rather than fail. A view that reads FSM context would simply go
/// missing, and the error would name the view's caller.
fn declare_ctx_id_extern(program: &mut TypedProgram) {
    use zyntax_typed_ast::typed_ast::{TypedDeclaration, TypedFunction, TypedParameter};
    use zyntax_typed_ast::{InternedString, Span};

    const NAME: &str = "__blinc_fsm_ctx_id__";
    let already = program.declarations.iter().any(|d| {
        matches!(&d.node, TypedDeclaration::Function(f)
            if f.name.resolve_global().as_deref() == Some(NAME))
    });
    if already {
        return;
    }
    let param = |i: usize, ty: Type| TypedParameter {
        name: InternedString::new_global(&format!("a{i}")),
        ty,
        ..Default::default()
    };
    program.declarations.push(zyntax_typed_ast::TypedNode::new(
        TypedDeclaration::Function(TypedFunction {
            name: InternedString::new_global(NAME),
            params: vec![
                param(0, Type::Primitive(PrimitiveType::String)),
                param(1, Type::Primitive(PrimitiveType::String)),
                param(2, Type::Primitive(PrimitiveType::I64)),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            body: None,
            is_external: true,
            ..Default::default()
        }),
        Type::Unknown,
        Span::default(),
    ));
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

/// The module path of a `.blinc` file: its directories and its name,
/// relative to the source root — `pages/nav.blinc` is `pages.nav`.
///
/// `namespace` already carries those segments, since that is what
/// `module_namespace_from_path` produces. It joins them with `$`
/// because a mangled component name has to survive codegen as an
/// identifier; a registry key has no such constraint, so the segments
/// are re-joined the way the language spells a module path.
///
/// The entry file compiles with no namespace, deliberately — mangling
/// it would break the bare names a host and the source both address it
/// by. Its file stem is still its module.
pub(crate) fn module_of(filename: &str, namespace: &str) -> String {
    if !namespace.is_empty() {
        return namespace.replace('$', ".");
    }
    std::path::Path::new(filename)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

thread_local! {
    /// The module whose passes are running on this thread right now.
    static COMPILING: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// Sets [`COMPILING`] for as long as it lives, restoring the previous
/// value on drop so a nested compile cannot leave a stale module behind.
pub(crate) struct ModuleScope(String);

impl Drop for ModuleScope {
    fn drop(&mut self) {
        COMPILING.with(|m| *m.borrow_mut() = std::mem::take(&mut self.0));
    }
}

/// Name the module the following passes belong to.
///
/// Several passes turn a bare identifier into a signal id — a bound
/// prop (`Div(bg = tint)`), a styling arg, an FSM guard. Each needs to
/// know which module's `tint` is meant, and none of them can be handed
/// the answer without threading a parameter through every nested
/// rewrite helper they own.
///
/// A thread-local is safe HERE and was not safe for the render path,
/// which is the distinction the two reverted attempts got wrong: this
/// is read synchronously, inside the same call that set it, while a
/// module is being compiled. The render path reads long after every
/// compile has finished, when the answer is whichever module went last.
pub(crate) fn enter_module(module: &str) -> ModuleScope {
    let previous = COMPILING.with(|m| m.replace(module.to_string()));
    ModuleScope(previous)
}

/// Resolve `name` as the module currently compiling sees it: its own
/// declaration first, then the ordinary qualified-name rules for a
/// signal some other module owns.
pub(crate) fn signal_in_scope(name: &str) -> Option<(u64, blinc_runtime::signal::SignalType)> {
    let module = COMPILING.with(|m| m.borrow().clone());
    if !module.is_empty() {
        let key = blinc_runtime::signal::qualify(&module, name);
        if let Some(hit) = blinc_runtime::signal::lookup_exact(&key) {
            return Some(hit);
        }
    }
    blinc_runtime::signal::lookup(name)
}

pub(crate) fn resolve_signal_calls(program: &mut TypedProgram) {
    resolve_signal_calls_scoped(program, "");
}

/// As [`resolve_signal_calls`], minting each declaration under the
/// module that declares it.
///
/// Only the REGISTRY key is qualified. References inside the module
/// still say `page` and resolve through this pass's own map to a baked
/// id, so nothing in the AST needs renaming — which is what makes this
/// a small change rather than the component-mangling one.
///
/// A host still reaches an unambiguous `page` by that bare name:
/// `signal::resolve` matches a lone `module.page` by suffix. Exports
/// are not consulted, because they were only ever a way to say "leave
/// this name reachable", and suffix resolution leaves every unique name
/// reachable already.
pub(crate) fn resolve_signal_calls_scoped(program: &mut TypedProgram, module: &str) {
    use std::collections::HashMap;
    use zyntax_typed_ast::InternedString;
    use zyntax_typed_ast::typed_ast::{TypedCall, TypedDeclaration, TypedExpression};

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
            other if is_string_list_type(other, &program.type_registry) => {
                blinc_runtime::signal::SignalType::StringList
            }
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
        let key = blinc_runtime::signal::qualify(module, name_str.as_ref());
        let is_new = blinc_runtime::signal::lookup_exact(&key).is_none();
        let id_raw_u64 = blinc_runtime::signal::mint_qualified(module, name_str.as_ref(), sig_ty);

        // `signal feed = ["a", "b"]` — seed the elements on first mint.
        // Only on first mint, matching the scalar rule: the registry
        // outlives a compile, so re-applying would throw away whatever
        // the program has since written.
        if sig_ty == blinc_runtime::signal::SignalType::StringList {
            if is_new && let Some(elements) = declared_list_elements(func) {
                blinc_runtime::signal::set_string_list(&key, elements);
            }
            continue;
        }
        if let Some(lit) = crate::passes::signal_initial_literal(func) {
            // Recorded unconditionally, BEFORE the `is_new` test: `||`
            // short-circuits, so folding this into the condition meant
            // the first compile never recorded anything and the next one
            // read "no previous default", called it a change, and
            // clobbered the live value.
            let changed = declared_default_changed(&key, lit);
            if is_new || changed {
                seed_signal(&key, sig_ty, lit);
            }
        } else {
            // The default was removed; forget it, so re-adding the same
            // literal later still reads as a change.
            declared_defaults()
                .lock()
                .expect("declared defaults poisoned")
                .remove(key.as_str());
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
                ctx_origin: super::fsm::ctx_signal_origin(name_str.as_ref()),
            },
        );
    }

    if signals.is_empty() {
        return;
    }

    if signals.values().any(|e| e.ctx_origin.is_some()) {
        declare_ctx_id_extern(program);
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
            let id_arg = signal_id_expr(entry, expr.span);
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

            let id_arg = signal_id_expr(&entry, expr.span);
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
                    positional_args: vec![signal_id_expr(&entry, span)],
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
                    positional_args: vec![signal_id_expr(&entry, span)],
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
                    positional_args: vec![signal_id_expr(&entry, span), value],
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

/// Is this the `List` spelling from `state_type_list`?
///
/// The grammar emits it by name, and by the time this pass runs the
/// name has usually been interned into the type registry, so it arrives
/// as `Named { id }` rather than `Unresolved`. Both are accepted: the
/// id form is resolved back through the registry.
pub(crate) fn is_string_list_type(
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
    name.as_deref() == Some("BlincStringList")
}

/// Elements of a `signal x = ["a", "b"]` declaration.
///
/// Only string literals are taken: a list holds strings today, and a
/// non-string element is dropped rather than coerced, so a mistake
/// shows up as a missing row instead of a surprising one.
fn declared_list_elements(
    func: &zyntax_typed_ast::typed_ast::TypedFunction,
) -> Option<Vec<String>> {
    use zyntax_typed_ast::typed_ast::{TypedExpression, TypedLiteral};
    let [stmt] = func.body.as_ref()?.statements.as_slice() else {
        return None;
    };
    let TypedStatement::Expression(e) = &stmt.node else {
        return None;
    };
    let TypedExpression::Array(elements) = &e.node else {
        return None;
    };
    Some(
        elements
            .iter()
            .filter_map(|el| match &el.node {
                TypedExpression::Literal(TypedLiteral::String(s)) => {
                    s.resolve_global().map(|s| s.to_string())
                }
                _ => None,
            })
            .collect(),
    )
}
