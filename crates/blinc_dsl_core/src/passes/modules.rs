//! Multi-module support: namespace prefixing and cross-file component call rewriting.

use crate::*;
use std::path::Path;

/// Rename every user-component (Class + matching Impl) with the
/// module namespace prefix so cross-file declarations don't collide
/// in the JIT symbol table or the component registry.
///
/// Mangling shape: `Counter` declared in module `widgets` becomes
/// `widgets$Counter`. Multi-segment paths use `$` as the separator:
/// `ui/widgets.blinc` → `ui$widgets$Counter`. Matches Zyntax's
/// existing inherent-impl symbol convention (`Class$method`), so the
/// downstream `<Component>$view` symbol naturally lands as
/// `widgets$Counter$view` without any change in the symbol emitter.
///
/// Scope: ONLY user-declared components — a `Class` decl whose name
/// has a matching `Impl` decl pointing at it. Marker classes
/// (`__blinc_*`), structs without sibling impls, FSMs, the synthetic
/// `render_view` function, and substrate primitives are all left
/// untouched.
///
/// Side effects this pass handles atomically:
/// 1. `Class.name` is renamed.
/// 2. Every matching `Impl.for_type` (`Type::Unresolved(name)`
///    pre-resolution; `Type::Named { id, … }` post-resolution) is
///    repointed at the mangled name.
/// 3. Every `__component_call__("local_name")` marker call in the
///    program is rewritten to `__component_call__("mangled_name")`
///    so [`lower_component_calls`] resolves the callee against the
///    same mangled-name entry in the runtime component registry.
///
/// No-op when `namespace` is empty — single-file `compile_source` /
/// `compile_directory` paths keep emitting un-mangled symbols, so
/// existing tests that assert `Counter$view` stay green.
///
/// Cross-module reference rewriting (entry's `Counter()` →
/// `widgets$Counter()` for an import from `./widgets`) happens
/// separately in [`crate::BlincDsl::inject_imported_view_externs`]
/// because that hook already knows each import's source file.
pub(crate) fn apply_module_namespace_prefix(program: &mut TypedProgram, namespace: &str) {
    use std::collections::{HashMap, HashSet};
    use zyntax_typed_ast::InternedString;
    use zyntax_typed_ast::typed_ast::TypedDeclaration;

    if namespace.is_empty() {
        return;
    }

    // Step 1: identify mangleable top-level types. Two categories
    // get mangled:
    //
    // 1. Component classes — a `Class` decl whose name has a
    //    matching `Impl` decl pointing at it. Marker classes
    //    (`__blinc_*`) and structs without impls pass through
    //    un-mangled.
    //
    // 2. FSM state enums — an `Enum` decl whose name has a matching
    //    `Impl` decl whose `__fsm_meta__` marker method identifies
    //    it as an FSM. Same-named cross-file FSMs would otherwise
    //    collide in the global `FsmRegistry`.
    //
    // Both categories share one `to_mangle` map so the downstream
    // call-site rewrites can resolve same-file references against
    // either kind uniformly.
    let class_names: Vec<InternedString> = program
        .declarations
        .iter()
        .filter_map(|d| {
            if let TypedDeclaration::Class(c) = &d.node {
                Some(c.name)
            } else {
                None
            }
        })
        .collect();

    let enum_names: Vec<InternedString> = program
        .declarations
        .iter()
        .filter_map(|d| {
            if let TypedDeclaration::Enum(e) = &d.node {
                Some(e.name)
            } else {
                None
            }
        })
        .collect();

    let impl_targets: HashSet<InternedString> = program
        .declarations
        .iter()
        .filter_map(|d| {
            let TypedDeclaration::Impl(imp) = &d.node else {
                return None;
            };
            match &imp.for_type {
                Type::Unresolved(name) => Some(*name),
                Type::Named { id, .. } => program.type_registry.get_type_by_id(*id).map(|t| t.name),
                _ => None,
            }
        })
        .collect();

    // FSM-impl set: target names for impls that carry a `__fsm_meta__`
    // method. Used to discriminate "this enum is a state enum for an
    // FSM" from "this enum is a plain data enum the user declared".
    let fsm_impl_targets: HashSet<InternedString> = program
        .declarations
        .iter()
        .filter_map(|d| {
            let TypedDeclaration::Impl(imp) = &d.node else {
                return None;
            };
            let has_fsm_meta = imp
                .methods
                .iter()
                .any(|m| m.name.resolve_global().as_deref() == Some("__fsm_meta__"));
            if !has_fsm_meta {
                return None;
            }
            match &imp.for_type {
                Type::Unresolved(name) => Some(*name),
                Type::Named { id, .. } => program.type_registry.get_type_by_id(*id).map(|t| t.name),
                _ => None,
            }
        })
        .collect();

    let mut to_mangle: HashMap<InternedString, InternedString> = HashMap::new();

    // Components.
    for name in class_names {
        let Some(name_str_arc) = name.resolve_global() else {
            continue;
        };
        let name_str: &str = &name_str_arc;
        if name_str.starts_with("__blinc_") || name_str.starts_with("__") {
            continue;
        }
        if !impl_targets.contains(&name) {
            continue;
        }
        let mangled_str = format!("{namespace}${name_str}");
        to_mangle.insert(name, InternedString::new_global(&mangled_str));
    }

    // FSMs.
    for name in enum_names {
        let Some(name_str_arc) = name.resolve_global() else {
            continue;
        };
        let name_str: &str = &name_str_arc;
        if name_str.starts_with("__blinc_") || name_str.starts_with("__") {
            continue;
        }
        if !fsm_impl_targets.contains(&name) {
            continue;
        }
        // Skip if already mangled (e.g., a name collision between a
        // component class and an FSM enum — pathological but defensive).
        if to_mangle.contains_key(&name) {
            continue;
        }
        let mangled_str = format!("{namespace}${name_str}");
        to_mangle.insert(name, InternedString::new_global(&mangled_str));
    }

    if to_mangle.is_empty() {
        return;
    }

    // Step 2: rename Class.name / Enum.name + every matching
    // Impl.for_type AND Impl.trait_name. FSM impls use the same
    // string for both trait_name (the inherent-impl convention) and
    // for_type — `populate_fsm_registry_pass` keys off trait_name,
    // so renaming both keeps the registry entry's identity in sync
    // with the type-level rename.
    for decl in &mut program.declarations {
        match &mut decl.node {
            TypedDeclaration::Class(c) => {
                if let Some(&new_name) = to_mangle.get(&c.name) {
                    c.name = new_name;
                }
            }
            TypedDeclaration::Enum(e) => {
                if let Some(&new_name) = to_mangle.get(&e.name) {
                    e.name = new_name;
                }
            }
            TypedDeclaration::Impl(imp) => {
                if let Some(&new_name) = to_mangle.get(&imp.trait_name) {
                    imp.trait_name = new_name;
                }
                match &mut imp.for_type {
                    Type::Unresolved(name) => {
                        if let Some(&new_name) = to_mangle.get(name) {
                            *name = new_name;
                        }
                    }
                    Type::Named { .. } => {
                        // Post-resolution shape — the type registry entry's
                        // own `name` is what `publish_components_to_runtime_registry`
                        // reads. The mangling pass runs pre-resolution so
                        // this arm is reached only if some earlier pass
                        // ran the type resolver; we conservatively skip
                        // it to avoid mutating the registry mid-pipeline.
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    // Step 3: rewrite same-file references to mangled names. Two
    // shapes share the rewrite helper:
    //
    // - Component calls: `__component_call__("Name", …)` markers
    //   (emitted by the `component_call_*` grammar rules) have
    //   their first string-literal arg rewritten.
    // - FSM receivers: `<FsmName>.trigger(...)` / `.subscribe(...)`
    //   parse as MethodCall whose receiver is a `Variable(FsmName)`.
    //   The helper walks receiver positions and rewrites the Variable
    //   name when it matches the mangled set.
    //
    // Cross-file references (imports) are handled separately in
    // `inject_imported_view_externs`, which calls into the same
    // helper with its own (import-local-name → mangled-name) map.
    rewrite_component_calls_in_program(program, &to_mangle);
}

/// Walk every function / impl body in `program` and rewrite every
/// `__component_call__("local", …)` marker call where `local` matches
/// a key in `rewrites` to `__component_call__("rewrites[local]", …)`.
/// Shared between [`apply_module_namespace_prefix`] (which uses it
/// for same-file component renames) and
/// [`crate::BlincDsl::inject_imported_view_externs`] (which uses it
/// for cross-file import renames). The rewrite never touches a
/// component's structural shape — only the leading string-literal
/// name arg.
pub(crate) fn rewrite_component_calls_in_program(
    program: &mut TypedProgram,
    rewrites: &std::collections::HashMap<
        zyntax_typed_ast::InternedString,
        zyntax_typed_ast::InternedString,
    >,
) {
    use std::collections::HashMap;
    use zyntax_typed_ast::InternedString;
    use zyntax_typed_ast::TypedNode;
    use zyntax_typed_ast::typed_ast::{TypedDeclaration, TypedExpression, TypedLiteral};

    fn rewrite_expr(
        expr: &mut TypedNode<TypedExpression>,
        rewrites: &HashMap<InternedString, InternedString>,
    ) {
        if let TypedExpression::Call(call) = &mut expr.node {
            // FSM trigger/subscribe shape via the
            // `method_call_stmt` grammar — `MyFsm.trigger(...)`
            // lowers to `Call(Field(Variable(MyFsm), trigger), …)`.
            // Rewrite the inner Variable to the mangled name before
            // recursing so the `resolve_fsm_*_calls` passes see the
            // mangled receiver. The downstream MethodCall arm below
            // handles the alternate `MethodCall { receiver: Variable,
            // method, args }` AST shape uniformly.
            if let TypedExpression::Field(f) = &mut call.callee.node
                && let TypedExpression::Variable(name) = &f.object.node
                && let Some(&new_name) = rewrites.get(name)
            {
                f.object.node = TypedExpression::Variable(new_name);
            }
            rewrite_expr(&mut call.callee, rewrites);
            if let TypedExpression::Variable(callee) = &call.callee.node
                && callee.resolve_global().as_deref() == Some("__component_call__")
                && let Some(name_arg) = call.positional_args.first_mut()
                && let TypedExpression::Literal(TypedLiteral::String(name)) = &name_arg.node
                && let Some(&new_name) = rewrites.get(name)
            {
                name_arg.node = TypedExpression::Literal(TypedLiteral::String(new_name));
            }
            for a in &mut call.positional_args {
                rewrite_expr(a, rewrites);
            }
            for na in &mut call.named_args {
                rewrite_expr(&mut na.value, rewrites);
            }
            return;
        }
        match &mut expr.node {
            TypedExpression::Binary(b) => {
                rewrite_expr(&mut b.left, rewrites);
                rewrite_expr(&mut b.right, rewrites);
            }
            TypedExpression::Unary(u) => rewrite_expr(&mut u.operand, rewrites),
            TypedExpression::Field(f) => rewrite_expr(&mut f.object, rewrites),
            TypedExpression::Index(idx) => {
                rewrite_expr(&mut idx.object, rewrites);
                rewrite_expr(&mut idx.index, rewrites);
            }
            TypedExpression::Array(items) | TypedExpression::Tuple(items) => {
                for it in items {
                    rewrite_expr(it, rewrites);
                }
            }
            TypedExpression::MethodCall(mc) => {
                // FSM receiver rewrite: `<FsmName>.trigger(...)` /
                // `.subscribe(...)` parse as MethodCall whose
                // receiver is a `Variable(FsmName)`. After mangling,
                // the local `FsmName` no longer matches the
                // registered FSM identity — rewrite the receiver
                // Variable's name to the mangled form here so the
                // downstream `resolve_fsm_trigger_calls` /
                // `resolve_fsm_subscribe_calls` passes resolve
                // against the same key as the registry.
                if let TypedExpression::Variable(name) = &mc.receiver.node
                    && let Some(&new_name) = rewrites.get(name)
                {
                    mc.receiver.node = TypedExpression::Variable(new_name);
                }
                rewrite_expr(&mut mc.receiver, rewrites);
                for a in &mut mc.positional_args {
                    rewrite_expr(a, rewrites);
                }
            }
            TypedExpression::Block(block) => {
                for stmt in &mut block.statements {
                    rewrite_stmt(stmt, rewrites);
                }
            }
            TypedExpression::If(if_expr) => {
                rewrite_expr(&mut if_expr.condition, rewrites);
                rewrite_expr(&mut if_expr.then_branch, rewrites);
                rewrite_expr(&mut if_expr.else_branch, rewrites);
            }
            TypedExpression::Lambda(lam) => match &mut lam.body {
                zyntax_typed_ast::typed_ast::TypedLambdaBody::Expression(e) => {
                    rewrite_expr(e, rewrites);
                }
                zyntax_typed_ast::typed_ast::TypedLambdaBody::Block(block) => {
                    for stmt in &mut block.statements {
                        rewrite_stmt(stmt, rewrites);
                    }
                }
            },
            _ => {}
        }
    }

    fn rewrite_stmt(
        stmt: &mut TypedNode<TypedStatement>,
        rewrites: &HashMap<InternedString, InternedString>,
    ) {
        match &mut stmt.node {
            TypedStatement::Expression(e) => rewrite_expr(e, rewrites),
            TypedStatement::Return(Some(e)) => rewrite_expr(e, rewrites),
            TypedStatement::Let(l) => {
                if let Some(init) = &mut l.initializer {
                    rewrite_expr(init, rewrites);
                }
            }
            TypedStatement::If(if_stmt) => {
                rewrite_expr(&mut if_stmt.condition, rewrites);
                for s in &mut if_stmt.then_block.statements {
                    rewrite_stmt(s, rewrites);
                }
                if let Some(else_block) = &mut if_stmt.else_block {
                    for s in &mut else_block.statements {
                        rewrite_stmt(s, rewrites);
                    }
                }
            }
            TypedStatement::While(w) => {
                rewrite_expr(&mut w.condition, rewrites);
                for s in &mut w.body.statements {
                    rewrite_stmt(s, rewrites);
                }
            }
            TypedStatement::Block(b) => {
                for s in &mut b.statements {
                    rewrite_stmt(s, rewrites);
                }
            }
            _ => {}
        }
    }

    for decl in &mut program.declarations {
        match &mut decl.node {
            TypedDeclaration::Function(func) => {
                if let Some(body) = &mut func.body {
                    for stmt in &mut body.statements {
                        rewrite_stmt(stmt, rewrites);
                    }
                }
            }
            TypedDeclaration::Impl(imp) => {
                for method in &mut imp.methods {
                    if let Some(body) = &mut method.body {
                        for stmt in &mut body.statements {
                            rewrite_stmt(stmt, rewrites);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Derive a module namespace from a file path relative to a source
/// root. Returns an empty string when `entry` isn't inside
/// `source_root` (defensive — single-file compile paths use the
/// empty-namespace branch in `apply_module_namespace_prefix`).
///
/// Shape: `widgets.blinc` → `"widgets"`,
/// `ui/widgets.blinc` → `"ui$widgets"`. Hyphens / dots inside path
/// segments survive; the `.blinc` extension is stripped.
pub(crate) fn module_namespace_from_path(entry: &Path, source_root: &Path) -> String {
    let rel = entry.strip_prefix(source_root).unwrap_or(entry);
    let mut segments: Vec<String> = Vec::new();
    for component in rel.components() {
        if let std::path::Component::Normal(os) = component
            && let Some(s) = os.to_str()
        {
            let stem = s.strip_suffix(".blinc").unwrap_or(s);
            if !stem.is_empty() {
                segments.push(stem.to_string());
            }
        }
    }
    segments.join("$")
}
