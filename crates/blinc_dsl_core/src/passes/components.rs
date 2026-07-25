//! Component call validation, call-site identity, component lowering and prop binding.

use crate::*;

/// Validate every `__component_call__("Name", ...)` marker references a known
/// component. Catches typos before Zyntax's less-helpful unresolved-symbol error.
/// Does NOT rewrite markers — that contract is consumed by `lower_component_calls`.
pub(crate) fn validate_component_calls(program: &TypedProgram) -> Result<(), Vec<String>> {
    use std::collections::HashSet;
    use zyntax_typed_ast::typed_ast::{TypedDeclaration, TypedExpression, TypedLiteral};

    let mut known: HashSet<String> = HashSet::new();
    for decl in &program.declarations {
        if let TypedDeclaration::Class(c) = &decl.node
            && let Some(name) = c.name.resolve_global()
        {
            known.insert(name.to_string());
        }
        // Named imports — whitelist so the validator (pre import-resolution) doesn't flag them.
        if let TypedDeclaration::Import(import) = &decl.node {
            for item in &import.items {
                if let zyntax_typed_ast::TypedImportItem::Named { name, .. } = item
                    && let Some(s) = name.resolve_global()
                {
                    known.insert(s.to_string());
                }
            }
        }
    }
    // Pull pre-registered primitives (`Div`, `Text`, …) from the substrate registry.
    blinc_runtime::component::with_component_registry(|r| {
        for (_, def) in r.iter() {
            known.insert(def.name.as_ref().to_string());
        }
    });

    let mut errors: Vec<String> = Vec::new();

    fn check_expr(
        expr: &zyntax_typed_ast::TypedNode<TypedExpression>,
        known: &HashSet<String>,
        errors: &mut Vec<String>,
    ) {
        match &expr.node {
            TypedExpression::Binary(b) => {
                check_expr(&b.left, known, errors);
                check_expr(&b.right, known, errors);
            }
            TypedExpression::Unary(u) => check_expr(&u.operand, known, errors),
            TypedExpression::Call(c) => {
                check_expr(&c.callee, known, errors);
                for a in &c.positional_args {
                    check_expr(a, known, errors);
                }

                // Check `__component_call__("Name", ...)` against known set.
                // Namespaced calls (`cn.Button`) round-trip the same way the
                // call-site resolver does: dotted form coming out of the
                // grammar maps to the underscore-mangled registry key
                // (`cn_Button`). Check both so the dotted surface and the
                // mangled-key registration agree without an extra
                // duplicate entry.
                if let TypedExpression::Variable(callee_name) = &c.callee.node
                    && callee_name.resolve_global().as_deref() == Some("__component_call__")
                    && let Some(name_node) = c.positional_args.first()
                    && let TypedExpression::Literal(TypedLiteral::String(name)) = &name_node.node
                {
                    let name_str = name.resolve_global().unwrap_or_default();
                    let name_ref: &str = name_str.as_ref();
                    let mangled = name_ref.replace('.', "_");
                    if !known.contains(name_ref) && !known.contains(&mangled) {
                        errors.push(format!(
                            "unknown component `{}` — declare it with \
                                         `component {} {{ ... }}` before use",
                            name_str, name_str
                        ));
                    }
                }
            }
            TypedExpression::Field(f) => check_expr(&f.object, known, errors),
            TypedExpression::Index(idx) => {
                check_expr(&idx.object, known, errors);
                check_expr(&idx.index, known, errors);
            }
            TypedExpression::Array(items) | TypedExpression::Tuple(items) => {
                for it in items {
                    check_expr(it, known, errors);
                }
            }
            TypedExpression::Struct(s) => {
                for field in &s.fields {
                    check_expr(&field.value, known, errors);
                }
            }
            TypedExpression::MethodCall(mc) => {
                check_expr(&mc.receiver, known, errors);
                for a in &mc.positional_args {
                    check_expr(a, known, errors);
                }
            }
            TypedExpression::Block(b) => check_block(b, known, errors),
            TypedExpression::If(if_expr) => {
                check_expr(&if_expr.condition, known, errors);
                check_expr(&if_expr.then_branch, known, errors);
                check_expr(&if_expr.else_branch, known, errors);
            }
            _ => {}
        }
    }

    fn check_block(
        block: &zyntax_typed_ast::typed_ast::TypedBlock,
        known: &HashSet<String>,
        errors: &mut Vec<String>,
    ) {
        for stmt in &block.statements {
            check_stmt(stmt, known, errors);
        }
    }

    fn check_stmt(
        stmt: &zyntax_typed_ast::TypedNode<TypedStatement>,
        known: &HashSet<String>,
        errors: &mut Vec<String>,
    ) {
        match &stmt.node {
            TypedStatement::Expression(e) => check_expr(e, known, errors),
            TypedStatement::Let(l) => {
                if let Some(init) = &l.initializer {
                    check_expr(init, known, errors);
                }
            }
            TypedStatement::Return(Some(e)) => check_expr(e, known, errors),
            TypedStatement::If(if_stmt) => {
                check_expr(&if_stmt.condition, known, errors);
                check_block(&if_stmt.then_block, known, errors);
                if let Some(else_block) = &if_stmt.else_block {
                    check_block(else_block, known, errors);
                }
            }
            TypedStatement::While(w) => {
                check_expr(&w.condition, known, errors);
                check_block(&w.body, known, errors);
            }
            TypedStatement::Block(b) => check_block(b, known, errors),
            _ => {}
        }
    }

    for decl in &program.declarations {
        match &decl.node {
            TypedDeclaration::Function(func) => {
                if let Some(body) = &func.body {
                    check_block(body, &known, &mut errors);
                }
            }
            TypedDeclaration::Impl(imp) => {
                for method in &imp.methods {
                    if let Some(body) = &method.body {
                        check_block(body, &known, &mut errors);
                    }
                }
            }
            _ => {}
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Stable per-call-site instance ID derived from `(filename, byte_offset)`.
/// Plain byte-offset hash. Used by tests + the simpler call sites where
/// component name + class info isn't readily available.
///
/// `DefaultHasher` (SipHash) is deterministic per process but not across
/// processes — that's fine here since instance IDs are scoped to a single
/// run of the JIT runtime.
pub(crate) fn call_site_instance_id(filename: &str, span_start: usize) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    filename.hash(&mut h);
    span_start.hash(&mut h);
    h.finish()
}

/// Path-based per-call-site instance ID — incorporates the component
/// name, an optional CSS class (when present as a string-literal arg
/// at the call site), and the source-location offset. The path string
/// has the shape `ComponentName[.className]:hex_offset` and is then
/// hashed.
///
/// Including the class name as part of identity is a deliberate design
/// choice ([previous design discussion]):
/// - Two `Button(class="hero")` calls at the same source position
///   collapse to the same identity (intended — they're the same widget).
/// - Two `Button(class="hero")` and `Button(class="cta")` calls at the
///   same source position diverge — class is part of identity.
/// - Two `Button(class="hero")` calls at DIFFERENT source positions
///   also diverge — offset is part of identity.
///
/// Two-input redundancy: class alone or offset alone would each be
/// sufficient discriminators in most realistic source files. Combining
/// them adds belt-and-suspenders robustness against pathological
/// reformatting (e.g. an auto-formatter that shuffles named-args).
pub(crate) fn call_site_path_id(
    filename: &str,
    span_start: usize,
    component_name: &str,
    class_name: Option<&str>,
    id_name: Option<&str>,
) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut path = String::with_capacity(component_name.len() + 32);
    path.push_str(component_name);
    if let Some(id) = id_name {
        path.push('#');
        path.push_str(id);
    }
    if let Some(class) = class_name {
        path.push('.');
        path.push_str(class);
    }
    use std::fmt::Write as _;
    let _ = write!(&mut path, ":{span_start:x}");
    let mut h = std::collections::hash_map::DefaultHasher::new();
    filename.hash(&mut h);
    path.hash(&mut h);
    h.finish()
}

/// Rewrite `__component_call__("Name", positionals, __named__(...), body)` markers
/// into `Call(Variable("Name"), positionals, named_args, body)`, then wrap each
/// rewritten call in a `__push_call_id__(ID) ; … ; __pop_call_id__()` bracket
/// so widget FFI can key per-instance state to the source span.
/// MUST run after `validate_component_calls`. Slot markers inside body Blocks
/// are left alone.
pub(crate) fn lower_component_calls(program: &mut TypedProgram, filename: &str) {
    use zyntax_typed_ast::typed_ast::{
        TypedCall, TypedDeclaration, TypedExpression, TypedLiteral, TypedNamedArg,
    };

    // Bracket-wrap injection (push_call_id(ID) ; ORIGINAL_CALL) around
    // each lowered view call is deferred to a follow-up. The scaffolding
    // (`call_site_instance_id` helper + `__push_call_id__` /
    // `__pop_call_id__` / `__current_call_id__` ABI fns) is wired up
    // already; the open question is how to materialise the wrap without
    // tripping Zyntax's SSA value-map on `TypedExpression::Block`-as-
    // expression at the trailing-statement position.

    fn rewrite_expr(expr: &mut zyntax_typed_ast::TypedNode<TypedExpression>, filename: &str) {
        // Recurse bottom-up so nested marker calls also lower.
        match &mut expr.node {
            TypedExpression::Binary(b) => {
                rewrite_expr(&mut b.left, filename);
                rewrite_expr(&mut b.right, filename);
            }
            TypedExpression::Unary(u) => rewrite_expr(&mut u.operand, filename),
            TypedExpression::Call(c) => {
                rewrite_expr(&mut c.callee, filename);
                for a in &mut c.positional_args {
                    rewrite_expr(a, filename);
                }
                for n in &mut c.named_args {
                    rewrite_expr(&mut n.value, filename);
                }
            }
            TypedExpression::Field(f) => rewrite_expr(&mut f.object, filename),
            TypedExpression::Index(idx) => {
                rewrite_expr(&mut idx.object, filename);
                rewrite_expr(&mut idx.index, filename);
            }
            TypedExpression::Array(items) | TypedExpression::Tuple(items) => {
                for it in items {
                    rewrite_expr(it, filename);
                }
            }
            TypedExpression::Struct(s) => {
                for field in &mut s.fields {
                    rewrite_expr(&mut field.value, filename);
                }
            }
            TypedExpression::MethodCall(mc) => {
                rewrite_expr(&mut mc.receiver, filename);
                for a in &mut mc.positional_args {
                    rewrite_expr(a, filename);
                }
            }
            TypedExpression::Block(b) => rewrite_block(b, filename),
            TypedExpression::If(if_expr) => {
                rewrite_expr(&mut if_expr.condition, filename);
                rewrite_expr(&mut if_expr.then_branch, filename);
                rewrite_expr(&mut if_expr.else_branch, filename);
            }
            _ => {}
        }

        // Only act on `__component_call__` markers.
        let TypedExpression::Call(call) = &expr.node else {
            return;
        };
        let TypedExpression::Variable(callee_name) = &call.callee.node else {
            return;
        };
        if callee_name.resolve_global().as_deref() != Some("__component_call__") {
            return;
        }

        // args[0] is the component name as a StringLiteral. If
        // it's missing or shaped wrong the parser would have caught
        // it; this is a defensive bail.
        let Some(name_arg) = call.positional_args.first() else {
            return;
        };
        let TypedExpression::Literal(TypedLiteral::String(component_name)) = &name_arg.node else {
            return;
        };
        let component_name = *component_name;
        let span = expr.span;

        // Split the remaining args into positional + named. A
        // `__named__("name", value)` marker call lifts into a
        // `TypedNamedArg`. Everything else stays positional.
        let mut new_positional: Vec<zyntax_typed_ast::TypedNode<TypedExpression>> = Vec::new();
        let mut new_named: Vec<TypedNamedArg> = Vec::new();

        for arg in call.positional_args.iter().skip(1) {
            // Is this arg a `__named__("name", value)` marker call?
            if let TypedExpression::Call(inner) = &arg.node
                && let TypedExpression::Variable(inner_callee) = &inner.callee.node
                && inner_callee.resolve_global().as_deref() == Some("__named__")
            {
                let name_node = &inner.positional_args[0];
                let value_node = &inner.positional_args[1];
                let TypedExpression::Literal(TypedLiteral::String(arg_name)) = &name_node.node
                else {
                    // Ill-formed marker — fall through as positional.
                    new_positional.push(arg.clone());
                    continue;
                };
                new_named.push(TypedNamedArg {
                    name: *arg_name,
                    value: Box::new(value_node.clone()),
                    span: arg.span,
                });
                continue;
            }
            new_positional.push(arg.clone());
        }

        // Carry pre-existing named_args through (defensive — grammar doesn't emit them).
        new_named.extend(call.named_args.iter().cloned());

        // Resolve callee to the registry's `view_symbol` (substrate primitives use
        // `$Blinc$<Name>$view`; user components use `<Name>$view`).
        //
        // Namespace mangling: dotted DSL names from the grammar
        // (`cn.Button`) lookup against the registry as the mangled form
        // (`cn_Button`). The dot is invalid in Cranelift symbols / Rust
        // idents, so the macro registers the widget under the mangled
        // key and the symbol-name derivation strips the dot too.
        // Keeping the registry on the mangled side means
        // `primitive_callee_props` (which reverses
        // `$Blinc$<key>$view` → `<key>`) finds the same entry without
        // a second character substitution.
        let component_name_str = component_name.resolve_global().unwrap_or_default();
        let component_name_str: &str = component_name_str.as_ref();
        let registry_key = component_name_str.replace('.', "_");
        let view_symbol = blinc_runtime::component::with_component_registry(|r| {
            r.get_by_name(&registry_key)
                .map(|def| def.view_symbol.as_ref().to_string())
        })
        .unwrap_or_else(|| format!("{registry_key}$view"));
        let new_callee = zyntax_typed_ast::TypedNode::new(
            TypedExpression::Variable(zyntax_typed_ast::InternedString::new_global(&view_symbol)),
            Type::Any,
            span,
        );

        expr.node = TypedExpression::Call(TypedCall {
            callee: Box::new(new_callee),
            positional_args: new_positional,
            named_args: new_named,
            type_args: vec![],
        });

        // Compute the call-site instance ID for follow-up wrap-injection
        // passes. The hash is stable per `(filename, span.start)` —
        // anything downstream that wants to key per-call-site state can
        // compute the same value from these inputs.
        let _instance_id = call_site_instance_id(filename, span.start);
    }

    fn rewrite_block(block: &mut zyntax_typed_ast::typed_ast::TypedBlock, filename: &str) {
        let old_stmts = std::mem::take(&mut block.statements);
        let mut new_stmts: Vec<zyntax_typed_ast::TypedNode<TypedStatement>> =
            Vec::with_capacity(old_stmts.len());
        for mut stmt in old_stmts {
            rewrite_stmt(&mut stmt, filename);
            collect_children_into(&mut new_stmts, stmt);
        }
        block.statements = new_stmts;
    }

    /// Handle body-bearing component calls. Substrate primitives: body block
    /// becomes `children: [Widget]` (plus `slot_<Name>` per slot pair).
    /// User components: flatten body statements after the call. MUST keep slot
    /// markers in place for the primitive-partition path.
    fn collect_children_into(
        out: &mut Vec<zyntax_typed_ast::TypedNode<TypedStatement>>,
        mut stmt: zyntax_typed_ast::TypedNode<TypedStatement>,
    ) {
        if let TypedStatement::Expression(expr_node) = &mut stmt.node
            && let TypedExpression::Call(call) = &mut expr_node.node
        {
            let has_body_block = matches!(
                call.positional_args.last().map(|a| &a.node),
                Some(TypedExpression::Block(_))
            );
            if has_body_block {
                if callee_is_substrate_primitive(call) {
                    let block_arg = call.positional_args.pop().unwrap();
                    let block_span = block_arg.span;
                    let TypedExpression::Block(body_block) = block_arg.node else {
                        unreachable!("just confirmed Block via the matches! above");
                    };

                    // Partition body statements: unnamed body
                    // entries → default `children`; entries
                    // inside `__slot_open__("X") … __slot_close__`
                    // marker pairs → `slot_X` named arg.
                    let mut default_children: Vec<zyntax_typed_ast::TypedNode<TypedExpression>> =
                        Vec::new();
                    let mut slot_buckets: Vec<(
                        String,
                        Vec<zyntax_typed_ast::TypedNode<TypedExpression>>,
                    )> = Vec::new();
                    let mut current_slot: Option<String> = None;

                    for s in body_block.statements {
                        if let Some(name) = slot_open_name(&s) {
                            current_slot = Some(name);
                            continue;
                        }
                        if is_slot_close_stmt(&s) {
                            current_slot = None;
                            continue;
                        }
                        let TypedStatement::Expression(e) = s.node else {
                            continue;
                        };
                        match &current_slot {
                            None => default_children.push(*e),
                            Some(name) => {
                                if let Some(bucket) =
                                    slot_buckets.iter_mut().find(|(n, _)| n == name)
                                {
                                    bucket.1.push(*e);
                                } else {
                                    slot_buckets.push((name.clone(), vec![*e]));
                                }
                            }
                        }
                    }

                    if !default_children.is_empty() {
                        call.named_args.push(zyntax_typed_ast::TypedNamedArg {
                            name: zyntax_typed_ast::InternedString::new_global("children"),
                            value: Box::new(zyntax_typed_ast::TypedNode::new(
                                TypedExpression::Array(default_children),
                                Type::Any,
                                block_span,
                            )),
                            span: block_span,
                        });
                    }
                    for (name, exprs) in slot_buckets {
                        let arg_name = format!("slot_{name}");
                        call.named_args.push(zyntax_typed_ast::TypedNamedArg {
                            name: zyntax_typed_ast::InternedString::new_global(&arg_name),
                            value: Box::new(zyntax_typed_ast::TypedNode::new(
                                TypedExpression::Array(exprs),
                                Type::Any,
                                block_span,
                            )),
                            span: block_span,
                        });
                    }

                    out.push(stmt);
                    return;
                }

                // User-declared component with a body — fall
                // back to flatten: push the body-less call,
                // then inline each child statement at the
                // outer level. Slot markers are dropped here
                // (user-component view methods don't accept
                // named slots yet).
                let block_arg = call.positional_args.pop().unwrap();
                let TypedExpression::Block(body_block) = block_arg.node else {
                    unreachable!("just confirmed Block via the matches! above");
                };
                out.push(stmt);
                for inner in body_block.statements {
                    if is_slot_marker_stmt(&inner) {
                        continue;
                    }
                    collect_children_into(out, inner);
                }
                return;
            }
        }

        out.push(stmt);
    }

    /// Match `__slot_open__("name")` and return `"name"`.
    fn slot_open_name(stmt: &zyntax_typed_ast::TypedNode<TypedStatement>) -> Option<String> {
        let TypedStatement::Expression(e) = &stmt.node else {
            return None;
        };
        let TypedExpression::Call(c) = &e.node else {
            return None;
        };
        let TypedExpression::Variable(callee) = &c.callee.node else {
            return None;
        };
        if callee.resolve_global().as_deref() != Some("__slot_open__") {
            return None;
        }
        let arg = c.positional_args.first()?;
        let TypedExpression::Literal(zyntax_typed_ast::TypedLiteral::String(name)) = &arg.node
        else {
            return None;
        };
        name.resolve_global().map(|s| s.to_string())
    }

    /// Match `__slot_close__()` — ends the active slot bucket.
    fn is_slot_close_stmt(stmt: &zyntax_typed_ast::TypedNode<TypedStatement>) -> bool {
        let TypedStatement::Expression(e) = &stmt.node else {
            return false;
        };
        let TypedExpression::Call(c) = &e.node else {
            return false;
        };
        let TypedExpression::Variable(callee) = &c.callee.node else {
            return false;
        };
        callee.resolve_global().as_deref() == Some("__slot_close__")
    }

    /// Callee is a substrate primitive (mangled name begins with `$Blinc$`).
    fn callee_is_substrate_primitive(call: &TypedCall) -> bool {
        let TypedExpression::Variable(callee) = &call.callee.node else {
            return false;
        };
        callee
            .resolve_global()
            .as_deref()
            .is_some_and(|s| s.starts_with("$Blinc$"))
    }

    /// `Expression(Call(Variable("__slot_open__" | "__slot_close__"), _))`.
    fn is_slot_marker_stmt(stmt: &zyntax_typed_ast::TypedNode<TypedStatement>) -> bool {
        let TypedStatement::Expression(expr_node) = &stmt.node else {
            return false;
        };
        let TypedExpression::Call(call) = &expr_node.node else {
            return false;
        };
        let TypedExpression::Variable(callee) = &call.callee.node else {
            return false;
        };
        matches!(
            callee.resolve_global().as_deref(),
            Some("__slot_open__") | Some("__slot_close__")
        )
    }

    fn rewrite_stmt(stmt: &mut zyntax_typed_ast::TypedNode<TypedStatement>, filename: &str) {
        match &mut stmt.node {
            TypedStatement::Expression(e) => rewrite_expr(e, filename),
            TypedStatement::Let(l) => {
                if let Some(init) = &mut l.initializer {
                    rewrite_expr(init, filename);
                }
            }
            TypedStatement::Return(Some(e)) => rewrite_expr(e, filename),
            TypedStatement::If(if_stmt) => {
                rewrite_expr(&mut if_stmt.condition, filename);
                rewrite_block(&mut if_stmt.then_block, filename);
                if let Some(else_block) = &mut if_stmt.else_block {
                    rewrite_block(else_block, filename);
                }
            }
            TypedStatement::While(w) => {
                rewrite_expr(&mut w.condition, filename);
                rewrite_block(&mut w.body, filename);
            }
            TypedStatement::Block(b) => rewrite_block(b, filename),
            _ => {}
        }
    }

    for decl in &mut program.declarations {
        match &mut decl.node {
            TypedDeclaration::Function(func) => {
                if let Some(body) = &mut func.body {
                    rewrite_block(body, filename);
                }
            }
            TypedDeclaration::Impl(imp) => {
                for method in &mut imp.methods {
                    if let Some(body) = &mut method.body {
                        rewrite_block(body, filename);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Prepend `__instance_id__: u64` as the first parameter of every
/// user-component `view` method. Pairs with the
/// [`inject_call_site_keys`] pass: that pass injects a `u64` literal
/// (or an XOR with the enclosing view's `__instance_id__`) as the
/// leading arg at every user-component call site, which slots into
/// this auto-prepended param.
///
/// MUST run AFTER [`bind_component_props`] (so prop params are in
/// place — `__instance_id__` goes BEFORE them) and BEFORE
/// `publish_components_to_runtime_registry` would have an issue — but
/// the registry should NOT see this synthetic param. To keep the prop
/// list clean for downstream code that consults the registry (e.g.
/// [`resolve_extern_widget_named_args`]), we ALSO skip the param
/// during registry publication. The actual filter lives in
/// `runtime_bridge.rs`.
///
/// Idempotent: if the first param is already `__instance_id__`, skip.
pub(crate) fn inject_user_view_instance_id_params(program: &mut TypedProgram) {
    use zyntax_typed_ast::Mutability;
    use zyntax_typed_ast::typed_ast::{ParameterKind, TypedDeclaration, TypedMethodParam};

    for decl in program.declarations.iter_mut() {
        let TypedDeclaration::Impl(imp) = &mut decl.node else {
            continue;
        };
        for method in imp.methods.iter_mut() {
            if method.name.resolve_global().as_deref() != Some("view") {
                continue;
            }
            // Idempotence — bail if already injected.
            if method
                .params
                .first()
                .and_then(|p| p.name.resolve_global())
                .as_deref()
                == Some("__instance_id__")
            {
                continue;
            }
            let param = TypedMethodParam {
                name: zyntax_typed_ast::InternedString::new_global("__instance_id__"),
                ty: Type::Primitive(PrimitiveType::U64),
                mutability: Mutability::Immutable,
                is_self: false,
                kind: ParameterKind::Regular,
                default_value: None,
                attributes: vec![],
                span: method.span,
            };
            method.params.insert(0, param);
        }
    }
}

/// Lift `__component_props__` marker params onto every other method in the impl,
/// then strip the marker. Idempotent.
pub(crate) fn bind_component_props(program: &mut TypedProgram) {
    use zyntax_typed_ast::typed_ast::TypedDeclaration;

    for decl in program.declarations.iter_mut() {
        let TypedDeclaration::Impl(imp) = &mut decl.node else {
            continue;
        };

        let prop_params = imp
            .methods
            .iter_mut()
            .find(|m| m.name.resolve_global().as_deref() == Some("__component_props__"))
            .map(|m| std::mem::take(&mut m.params));

        let Some(prop_params) = prop_params else {
            continue;
        };

        // Props MUST come first — call site lowers `Counter(1, 2)` to `Counter$view(1, 2)`.
        for method in imp.methods.iter_mut() {
            if method.name.resolve_global().as_deref() == Some("__component_props__") {
                continue;
            }
            let mut new_params = prop_params.clone();
            new_params.extend(std::mem::take(&mut method.params));
            method.params = new_params;
        }

        // Strip the marker so compile doesn't expose a `Counter$__component_props__`.
        imp.methods
            .retain(|m| m.name.resolve_global().as_deref() != Some("__component_props__"));
    }
}
