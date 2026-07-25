//! View/stateful detection, declaration collection, stylesheet extraction, source pre-processing.

use crate::*;

/// Shape-based recognition for `signal <name>: <T>` decls: extern fn, no body,
/// no params, primitive return, `link_name: None` (so we don't catch host builtins).
pub(crate) fn is_signal_decl(func: &zyntax_typed_ast::typed_ast::TypedFunction) -> bool {
    func.is_external
        && func.body.is_none()
        && func.params.is_empty()
        && func.link_name.is_none()
        && matches!(func.return_type, Type::Primitive(_))
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
    unsafe { materialize_widget(handle) }
        .map(|w| w.into_element_builder())
        .unwrap_or_else(|| Box::new(blinc_layout::div::Div::new()))
}

/// Detect view decorators and strip the synthetic marker calls.
/// Returns `(saw_stateful, explicit_signal_deps, explicit_fsms)`.
/// Empty `signal_deps` with `saw_stateful=true` means subscribe to all declared signals.
pub(crate) fn detect_and_strip_stateful_views(
    program: &mut TypedProgram,
) -> (bool, Vec<String>, Vec<String>) {
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

    let mut process = |body: &mut Option<zyntax_typed_ast::typed_ast::TypedBlock>| {
        let Some(body) = body else {
            return;
        };
        // Decorators can stack either way; strip until no more match.
        loop {
            if let Some(names) = strip_leading_marker(body, "__stateful_view__") {
                saw_stateful = true;
                for n in names {
                    if !signal_deps.contains(&n) {
                        signal_deps.push(n);
                    }
                }
                continue;
            }
            if let Some(names) = strip_leading_marker(body, "__fsm_view__") {
                saw_stateful = true;
                for n in names {
                    if !fsms.contains(&n) {
                        fsms.push(n);
                    }
                }
                continue;
            }
            break;
        }
    };

    for decl in program.declarations.iter_mut() {
        match &mut decl.node {
            TypedDeclaration::Function(func) => process(&mut func.body),
            TypedDeclaration::Impl(imp) => {
                for method in imp.methods.iter_mut() {
                    process(&mut method.body);
                }
            }
            _ => {}
        }
    }

    (saw_stateful, signal_deps, fsms)
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
