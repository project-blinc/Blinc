//! View/stateful detection, declaration collection, stylesheet extraction, source pre-processing.

use crate::*;

/// Shape-based recognition for `signal <name>: <T>` decls: extern fn, no body,
/// no params, primitive return, `link_name: None` (so we don't catch host builtins).
pub(crate) fn is_signal_decl(func: &zyntax_typed_ast::typed_ast::TypedFunction) -> bool {
    func.is_external
        && func.params.is_empty()
        && func.link_name.is_none()
        // A list signal's type is Named (`List`) rather than a
        // primitive -- deliberately, since a list has no inline
        // representation. Nothing else emits an external, param-less,
        // link-name-less decl with a named return type.
        && matches!(
            func.return_type,
            Type::Primitive(_) | Type::Named { .. } | Type::Unresolved(_)
        )
        // A body is allowed: `signal x: T = <literal>` parks its initial
        // value there, and `signal x = [...]` parks its elements.
        // Everything else about the shape still has to match, and the
        // signals pass strips these decls before lowering, so the body
        // never reaches Cranelift.
        && func
            .body
            .as_ref()
            .is_none_or(|b| is_single_literal_body(b) || is_single_array_body(b))
}

/// The one-statement array body a `signal x = [...]` carries.
fn is_single_array_body(body: &zyntax_typed_ast::typed_ast::TypedBlock) -> bool {
    use zyntax_typed_ast::typed_ast::TypedExpression;
    matches!(
        body.statements.as_slice(),
        [stmt] if matches!(
            &stmt.node,
            TypedStatement::Expression(e) if matches!(&e.node, TypedExpression::Array(_))
        )
    )
}

/// The one-statement literal body a `signal … = <literal>` carries.
fn is_single_literal_body(body: &zyntax_typed_ast::typed_ast::TypedBlock) -> bool {
    use zyntax_typed_ast::typed_ast::TypedExpression;
    matches!(
        body.statements.as_slice(),
        [stmt] if matches!(
            &stmt.node,
            TypedStatement::Expression(e)
                if matches!(e.node, TypedExpression::Literal(_))
        )
    )
}

/// The initial value a `signal … = <literal>` declares, if any.
pub(crate) fn signal_initial_literal(
    func: &zyntax_typed_ast::typed_ast::TypedFunction,
) -> Option<&zyntax_typed_ast::typed_ast::TypedLiteral> {
    use zyntax_typed_ast::typed_ast::TypedExpression;
    let [stmt] = func.body.as_ref()?.statements.as_slice() else {
        return None;
    };
    let TypedStatement::Expression(e) = &stmt.node else {
        return None;
    };
    let TypedExpression::Literal(lit) = &e.node else {
        return None;
    };
    Some(lit)
}

/// Run the JIT'd view once and box the resulting widget builder (no reactive wrapper).
pub(crate) fn materialize_view(
    renderer: &std::sync::Arc<dyn blinc_runtime::view::ViewRenderer>,
) -> Box<dyn blinc_layout::div::ElementBuilder> {
    use zyntax_embed::ZyntaxValue;
    let value = blinc_runtime::view::render_main(renderer).expect("render_main");
    let ZyntaxValue::Int(handle) = value else {
        return Box::new(blinc_layout::div::Div::new());
    };
    let inner = unsafe { materialize_widget(handle) }
        .map(|w| w.into_element_builder())
        .unwrap_or_else(|| Box::new(blinc_layout::div::Div::new()));

    Box::new(view_root(inner))
}

/// The root every DSL view mounts under, so it inherits the viewport.
///
/// The body of `view { … }` materialises as an `Auto`-sized element, and
/// `Auto` resolves by axis: on a host's cross axis it stretches, on the
/// main axis it shrinks to content. An inner `width: 100%` therefore saw
/// the window or a shrink-wrapped box purely according to the host's
/// flex direction -- the same stylesheet laying out differently for a
/// reason invisible from the DSL.
///
/// `w_full` / `h_full` are percentages of the host, so they fill
/// whichever axis they sit on, and the column direction puts width on
/// the cross axis so the body stretches across it.
///
/// Used by both the plain and the `@stateful` paths; they disagreed
/// before, which meant adding `@stateful` anywhere silently changed
/// how the whole view sized.
pub(crate) fn view_root(
    inner: Box<dyn blinc_layout::div::ElementBuilder>,
) -> blinc_layout::div::Div {
    blinc_layout::div::div()
        .w_full()
        .h_full()
        .flex_col()
        .child_box(inner)
}

/// Detect view decorators and strip the synthetic marker calls.
/// Returns `(saw_stateful, explicit_signal_deps, explicit_fsms,
/// ctx_value_reads)`.
///
/// Empty `signal_deps` with `saw_stateful=true` means subscribe to all
/// declared signals. `ctx_value_reads` holds `"<Fsm>.<field>"` for every
/// context field the decorated body reads as a VALUE -- see
/// [`collect_ctx_value_reads`].
pub(crate) fn detect_and_strip_stateful_views(
    program: &mut TypedProgram,
) -> (bool, Vec<String>, Vec<String>, Vec<String>, Vec<String>) {
    use zyntax_typed_ast::typed_ast::{TypedDeclaration, TypedExpression, TypedLiteral};

    // Strip a leading marker call matching `expected_callee` and return its string args.
    fn strip_leading_marker(
        body: &mut zyntax_typed_ast::typed_ast::TypedBlock,
        expected_callee: &str,
    ) -> Option<Vec<String>> {
        let matches = body.statements.first().and_then(|s| match &s.node {
            TypedStatement::Expression(e) => match &e.node {
                TypedExpression::Call(c) => match &c.callee.node {
                    TypedExpression::Variable(name)
                        if name.resolve_global().as_deref() == Some(expected_callee) =>
                    {
                        Some(c.positional_args.clone())
                    }
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        });
        let args = matches?;
        let names: Vec<String> = args
            .into_iter()
            .filter_map(|arg| match &arg.node {
                TypedExpression::Literal(TypedLiteral::String(s)) => {
                    s.resolve_global().map(|n| n.to_string())
                }
                _ => None,
            })
            .collect();
        body.statements.remove(0);
        Some(names)
    }

    let mut saw_stateful = false;
    let mut signal_deps: Vec<String> = Vec::new();
    let mut fsms: Vec<String> = Vec::new();
    let mut ctx_value_reads: Vec<String> = Vec::new();
    // Components carrying `@stateful`, by name. The entry `view { }`
    // compiles to `render_view` and is not one — it has no call site
    // to scope to.
    let mut decorated_components: Vec<String> = Vec::new();

    for decl in program.declarations.iter_mut() {
        // `__blinc_component_attrs__` — decorators written on the
        // component. The grammar emits one per component: first
        // statement the name, the rest the decorator markers. This is
        // the placement that carries the name, so it is the only one
        // that can attribute a decoration to a component without
        // inferring it from a mangled symbol.
        if let TypedDeclaration::Function(func) = &mut decl.node
            && func.name.resolve_global().as_deref() == Some("__blinc_component_attrs__")
        {
            if let Some(body) = &mut func.body {
                let stmts = std::mem::take(&mut body.statements);
                let mut iter = stmts.into_iter();
                let name = iter.next().and_then(|s| match &s.node {
                    TypedStatement::Expression(e) => match &e.node {
                        TypedExpression::Literal(TypedLiteral::String(v)) => {
                            v.resolve_global().map(|n| n.to_string())
                        }
                        _ => None,
                    },
                    _ => None,
                });
                let mut decorated_here = false;
                for stmt in iter {
                    let TypedStatement::Expression(e) = &stmt.node else {
                        continue;
                    };
                    let TypedExpression::Call(call) = &e.node else {
                        continue;
                    };
                    let TypedExpression::Variable(callee) = &call.callee.node else {
                        continue;
                    };
                    let marker = callee.resolve_global().unwrap_or_default();
                    let args: Vec<String> = call
                        .positional_args
                        .iter()
                        .filter_map(|a| match &a.node {
                            TypedExpression::Literal(TypedLiteral::String(v)) => {
                                v.resolve_global().map(|n| n.to_string())
                            }
                            TypedExpression::Variable(v) => {
                                v.resolve_global().map(|n| n.to_string())
                            }
                            _ => None,
                        })
                        .collect();
                    match marker.as_ref() {
                        "__stateful_view__" => {
                            saw_stateful = true;
                            decorated_here = true;
                            for n in args {
                                if !signal_deps.contains(&n) {
                                    signal_deps.push(n);
                                }
                            }
                        }
                        "__fsm_view__" => {
                            saw_stateful = true;
                            decorated_here = true;
                            for n in args {
                                if !fsms.contains(&n) {
                                    fsms.push(n);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                if decorated_here
                    && let Some(name) = name
                    && !decorated_components.contains(&name)
                {
                    decorated_components.push(name);
                }
            }
            continue;
        }
    }

    let mut process = |body: &mut Option<zyntax_typed_ast::typed_ast::TypedBlock>,
                       owner: Option<&str>| {
        let Some(body) = body else {
            return;
        };
        let mut decorated = false;
        // Decorators can stack either way; strip until no more match.
        loop {
            if let Some(names) = strip_leading_marker(body, "__stateful_view__") {
                saw_stateful = true;
                decorated = true;
                for n in names {
                    if !signal_deps.contains(&n) {
                        signal_deps.push(n);
                    }
                }
                continue;
            }
            if let Some(names) = strip_leading_marker(body, "__fsm_view__") {
                saw_stateful = true;
                decorated = true;
                for n in names {
                    if !fsms.contains(&n) {
                        fsms.push(n);
                    }
                }
                continue;
            }
            break;
        }
        if decorated {
            collect_ctx_value_reads(body, &mut ctx_value_reads);
            // Attribute the decoration to its component. A view
            // function is `<Component>$view`; the entry `view { }`
            // compiles to `render_view`, which has no component to
            // scope to and stays a whole-program wrap.
            if let Some(component) = owner
                .and_then(|n| n.strip_suffix("$view"))
                .filter(|n| !n.is_empty())
                && !decorated_components.iter().any(|c: &String| c == component)
            {
                decorated_components.push(component.to_string());
            }
        }
    };

    for decl in program.declarations.iter_mut() {
        match &mut decl.node {
            TypedDeclaration::Function(func) => {
                let name = func.name.resolve_global().map(|n| n.to_string());
                process(&mut func.body, name.as_deref());
            }
            TypedDeclaration::Impl(imp) => {
                for method in imp.methods.iter_mut() {
                    let name = method.name.resolve_global().map(|n| n.to_string());
                    process(&mut method.body, name.as_deref());
                }
            }
            _ => {}
        }
    }

    (
        saw_stateful,
        signal_deps,
        fsms,
        ctx_value_reads,
        decorated_components,
    )
}

/// Context fields a decorated view reads AS A VALUE, as `"<Fsm>.<field>"`.
///
/// The two ways to mention a context field are not equivalent:
///
///   `Play.pct`        a binding handle. Passed to a prop, the property
///                     channel writes it in place -- a repaint, no
///                     re-render, no layout.
///   `Play.busy.get()` a value. A branch condition or an interpolation
///                     can only follow it by re-rendering.
///
/// A stateful therefore needs the second kind in its deps and must NOT
/// have the first: one `Grow` action writing five bound fields cost five
/// full re-renders of the program, ~6ms apart, for a frame the binding
/// registry had already handled.
///
/// `computed { … }` bodies are skipped: reads in there feed a derived,
/// and the derived is what the prop binds to.
pub(crate) fn collect_ctx_value_reads(
    body: &zyntax_typed_ast::typed_ast::TypedBlock,
    out: &mut Vec<String>,
) {
    use zyntax_typed_ast::TypedNode;
    use zyntax_typed_ast::typed_ast::TypedExpression;

    fn is_computed_call(call: &zyntax_typed_ast::typed_ast::TypedCall) -> bool {
        let TypedExpression::Variable(callee) = &call.callee.node else {
            return false;
        };
        callee
            .resolve_global()
            .is_some_and(|n| n.starts_with("__blinc_computed_"))
    }

    /// `X.y` where `X` is a bare name -- the handle shape. Anything
    /// deeper (`a.b.c`, an index, a call result) is not one.
    fn dotted_name(expr: &TypedNode<TypedExpression>) -> Option<String> {
        let TypedExpression::Field(f) = &expr.node else {
            return None;
        };
        let TypedExpression::Variable(obj) = &f.object.node else {
            return None;
        };
        Some(format!(
            "{}.{}",
            obj.resolve_global()?,
            f.field.resolve_global()?
        ))
    }

    fn record(expr: &TypedNode<TypedExpression>, out: &mut Vec<String>) {
        if let Some(name) = dotted_name(expr)
            && !out.contains(&name)
        {
            out.push(name);
        }
    }

    fn walk_expr(expr: &TypedNode<TypedExpression>, out: &mut Vec<String>) {
        record(expr, out);
        match &expr.node {
            TypedExpression::Call(call) => {
                if is_computed_call(call) {
                    return;
                }
                walk_expr(&call.callee, out);
                // An argument that IS a bare `X.y` is a binding handle:
                // do not descend, do not record.
                for a in &call.positional_args {
                    if dotted_name(a).is_none() {
                        walk_expr(a, out);
                    }
                }
                for na in &call.named_args {
                    if dotted_name(&na.value).is_none() {
                        walk_expr(&na.value, out);
                    }
                }
            }
            TypedExpression::MethodCall(mc) => {
                // `.get()` on a handle is precisely a value read, so the
                // receiver counts here even though a bare arg does not.
                walk_expr(&mc.receiver, out);
                for a in &mc.positional_args {
                    walk_expr(a, out);
                }
            }
            TypedExpression::Binary(b) => {
                walk_expr(&b.left, out);
                walk_expr(&b.right, out);
            }
            TypedExpression::Unary(u) => walk_expr(&u.operand, out),
            TypedExpression::Field(f) => walk_expr(&f.object, out),
            TypedExpression::Index(i) => {
                walk_expr(&i.object, out);
                walk_expr(&i.index, out);
            }
            TypedExpression::Array(items) | TypedExpression::Tuple(items) => {
                for it in items {
                    walk_expr(it, out);
                }
            }
            TypedExpression::Block(b) => walk_block(b, out),
            TypedExpression::If(i) => {
                walk_expr(&i.condition, out);
                walk_expr(&i.then_branch, out);
                walk_expr(&i.else_branch, out);
            }
            _ => {}
        }
    }

    fn walk_block(block: &zyntax_typed_ast::typed_ast::TypedBlock, out: &mut Vec<String>) {
        for stmt in &block.statements {
            walk_stmt(stmt, out);
        }
    }

    fn walk_stmt(stmt: &TypedNode<TypedStatement>, out: &mut Vec<String>) {
        match &stmt.node {
            TypedStatement::Expression(e) => walk_expr(e, out),
            TypedStatement::Let(l) => {
                if let Some(init) = &l.initializer {
                    walk_expr(init, out);
                }
            }
            TypedStatement::Return(Some(e)) => walk_expr(e, out),
            TypedStatement::If(i) => {
                walk_expr(&i.condition, out);
                walk_block(&i.then_block, out);
                if let Some(else_block) = &i.else_block {
                    walk_block(else_block, out);
                }
            }
            TypedStatement::While(w) => {
                walk_expr(&w.condition, out);
                walk_block(&w.body, out);
            }
            TypedStatement::Block(b) => walk_block(b, out),
            _ => {}
        }
    }

    walk_block(body, out);
}

/// Snapshot `signal <name>: <T>` and `fsm <Name> { … }` decls. MUST run BEFORE
/// the signal-rewrite / fsm-meta-strip passes — they erase the originating decls.
pub(crate) fn collect_declared(program: &TypedProgram) -> (Vec<(String, Type)>, Vec<String>) {
    use zyntax_typed_ast::typed_ast::TypedDeclaration;
    let mut signals = Vec::new();
    let mut fsms = Vec::new();
    for decl in &program.declarations {
        match &decl.node {
            TypedDeclaration::Function(func) if is_signal_decl(func) => {
                if let Some(name) = func.name.resolve_global() {
                    signals.push((name.to_string(), func.return_type.clone()));
                }
            }
            TypedDeclaration::Impl(imp) => {
                let is_fsm = imp
                    .methods
                    .iter()
                    .any(|m| m.name.resolve_global().as_deref() == Some("__fsm_meta__"));
                if is_fsm && let Some(name) = imp.trait_name.resolve_global() {
                    fsms.push(name.to_string());
                }
            }
            _ => {}
        }
    }
    (signals, fsms)
}

/// Extract CSS from `__blinc_stylesheet__` marker fns and remove them from the
/// program. The CSS text is run through [`auto_inject_semicolons`] so `;`-free
/// declarations work.
pub(crate) fn extract_and_strip_stylesheets(program: &mut TypedProgram, out: &mut Vec<String>) {
    use zyntax_typed_ast::typed_ast::{
        TypedDeclaration, TypedExpression, TypedLiteral, TypedStatement,
    };
    program.declarations.retain(|decl| {
        let TypedDeclaration::Function(func) = &decl.node else {
            return true;
        };
        if func.name.resolve_global().as_deref() != Some("__blinc_stylesheet__") {
            return true;
        }
        let Some(body) = &func.body else {
            return false;
        };
        for stmt in &body.statements {
            let TypedStatement::Expression(expr) = &stmt.node else {
                continue;
            };
            if let TypedExpression::Literal(TypedLiteral::String(s)) = &expr.node
                && let Some(text) = s.resolve_global()
            {
                out.push(auto_inject_semicolons(&text));
            }
        }
        false
    });
}

/// Turn `export signal x: T = v` markers into ordinary signal
/// declarations, recording each name as exported.
///
/// One grammar rule yields one declaration, and this needs two things:
/// a signal to exist, and its name to join the export list. Same split
/// `const ( … )` uses — the grammar marks, a pass expands.
///
/// Runs BEFORE the signals pass, which would otherwise see a decl named
/// `__blinc_export_signal__` and mint a signal called that.
pub(crate) fn expand_exported_signals(program: &mut TypedProgram, exports: &mut Vec<String>) {
    use zyntax_typed_ast::typed_ast::{
        TypedDeclaration, TypedExpression, TypedLiteral, TypedStatement,
    };
    for decl in &mut program.declarations {
        let TypedDeclaration::Function(func) = &mut decl.node else {
            continue;
        };
        if func.name.resolve_global().as_deref() != Some("__blinc_export_signal__") {
            continue;
        }
        let Some(body) = &mut func.body else { continue };
        // The name rides as the first statement; anything after it is
        // the initial value the signals pass expects to find alone.
        let name = body.statements.first().and_then(|stmt| {
            let TypedStatement::Expression(expr) = &stmt.node else {
                return None;
            };
            let TypedExpression::Literal(TypedLiteral::String(s)) = &expr.node else {
                return None;
            };
            s.resolve_global().map(|n| n.to_string())
        });
        let Some(name) = name else { continue };

        body.statements.remove(0);
        // A no-init `export signal x: T` leaves nothing behind. The
        // plain form carries `None` there, not an empty block, and the
        // mint path follows that shape — left as `Some([])` the signal
        // was never declared.
        if body.statements.is_empty() {
            func.body = None;
        }
        func.name = zyntax_typed_ast::InternedString::new_global(&name);
        if !exports.contains(&name) {
            exports.push(name);
        }
    }
}

/// Extract the names from `__blinc_exports__` markers and strip them.
///
/// An exported signal is one a host may reach by name. Everything else
/// belongs to the program, which is what makes scoping possible later:
/// today every signal is reachable, so nothing can be made private
/// without taking the host's grip away.
///
/// Names are captured verbatim by the grammar, so splitting on commas
/// and trimming is the whole parse. Empty entries are dropped rather
/// than recorded as a signal named "".
pub(crate) fn extract_and_strip_exports(program: &mut TypedProgram, out: &mut Vec<String>) {
    use zyntax_typed_ast::typed_ast::{
        TypedDeclaration, TypedExpression, TypedLiteral, TypedStatement,
    };
    program.declarations.retain(|decl| {
        let TypedDeclaration::Function(func) = &decl.node else {
            return true;
        };
        if func.name.resolve_global().as_deref() != Some("__blinc_exports__") {
            return true;
        }
        let Some(body) = &func.body else {
            return false;
        };
        for stmt in &body.statements {
            let TypedStatement::Expression(expr) = &stmt.node else {
                continue;
            };
            if let TypedExpression::Literal(TypedLiteral::String(s)) = &expr.node
                && let Some(text) = s.resolve_global()
            {
                for name in text.split(',') {
                    let name = name.trim();
                    if !name.is_empty() && !out.contains(&name.to_string()) {
                        out.push(name.to_string());
                    }
                }
            }
        }
        false
    });
}

/// Append `;` inside `{ ... }` blocks where the line's last char doesn't already
/// terminate. Brace depth tracking is naïve — string/comment braces will skew it.
pub(crate) fn auto_inject_semicolons(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() + raw.len() / 8);
    let mut depth: i32 = 0;
    for line in raw.split_inclusive('\n') {
        // Separate body from trailing whitespace so we inject `;` before the newline.
        let line_end_idx = line
            .rfind(|c: char| !c.is_whitespace())
            .map(|i| i + line[i..].chars().next().map(char::len_utf8).unwrap_or(0))
            .unwrap_or(line.len());
        let body = &line[..line_end_idx];
        let tail = &line[line_end_idx..];

        let depth_before = depth;
        depth += body.matches('{').count() as i32;
        depth -= body.matches('}').count() as i32;

        out.push_str(body);

        let trimmed = body.trim_start();
        let last_char = body.chars().rev().find(|c| !c.is_whitespace());
        let is_comment_line =
            trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with("*");
        let inside_block = depth_before > 0;
        let needs_semi = inside_block
            && !is_comment_line
            && match last_char {
                None => false,
                Some(c) => !matches!(c, ';' | '{' | '}' | ',' | '/' | '*'),
            };
        if needs_semi {
            out.push(';');
        }
        out.push_str(tail);
    }
    out
}
