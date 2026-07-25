//! Named-argument resolution for bare calls and extern widgets.

use crate::*;

/// Resolve named-arg calls to user-declared top-level functions.
/// `step(by = 5, x = 10)` lowers via the `bare_call_args_list`
/// grammar to `Call(Variable("step"), [__named__("by", 5),
/// __named__("x", 10)])`. This pass:
///
/// 1. Collects each top-level `TypedDeclaration::Function`'s
///    signature (param names + which have `default_value`s).
/// 2. Walks every Call(Variable(name), …) whose callee matches a
///    declared fn.
/// 3. Extracts `__named__(name, value)` markers from
///    `positional_args` into `named_args`.
/// 4. Builds slots[N] = function-arity slots, fills from
///    positional then named, splices each missing slot's declared
///    `default_value`.
/// 5. Writes the fully-positional list back to `positional_args`,
///    clears `named_args`.
///
/// Same shape as `resolve_extern_widget_named_args` but pulls
/// signatures from `TypedDeclaration::Function` instead of the
/// substrate `ComponentRegistry`. Without this pass, the
/// `__named__` markers leak into the JIT lowering as undefined
/// function calls.
pub(crate) fn lower_bare_call_named_args(program: &mut TypedProgram) {
    use std::collections::HashMap;
    use zyntax_typed_ast::InternedString;
    use zyntax_typed_ast::TypedNode;
    use zyntax_typed_ast::typed_ast::{
        TypedDeclaration, TypedExpression, TypedLiteral, TypedNamedArg,
    };

    // Step 1: collect signatures — each entry maps a fn name to
    // its ordered list of `(param_name, optional_default_clone)`.
    // Position is encoded by index into the inner Vec; the
    // slot-resolution step looks up params by name and inherits
    // the position from the Vec ordering.
    #[derive(Clone)]
    struct ParamInfo {
        default: Option<TypedNode<TypedExpression>>,
    }
    #[derive(Clone)]
    struct FnSig {
        params: Vec<(InternedString, ParamInfo)>,
    }
    let mut signatures: HashMap<InternedString, FnSig> = HashMap::new();
    for decl in &program.declarations {
        let TypedDeclaration::Function(func) = &decl.node else {
            continue;
        };
        if func.body.is_none() {
            continue;
        }
        // Skip extern decls (no body) and synthetic markers.
        if func.is_external {
            continue;
        }
        let Some(name_arc) = func.name.resolve_global() else {
            continue;
        };
        if name_arc.starts_with("__") {
            continue;
        }
        let mut params: Vec<(InternedString, ParamInfo)> = Vec::new();
        for p in &func.params {
            let default = p.default_value.as_ref().map(|d| (**d).clone());
            params.push((p.name, ParamInfo { default }));
        }
        signatures.insert(func.name, FnSig { params });
    }
    if signatures.is_empty() {
        return;
    }

    // Step 2 + 3 + 4: visitor that walks every Call(Variable(name)),
    // extracts __named__ markers, slot-resolves against the signature.
    struct Lowerer<'a> {
        signatures: &'a HashMap<InternedString, FnSig>,
    }
    impl Lowerer<'_> {
        fn rewrite_expr(&self, expr: &mut TypedNode<TypedExpression>) {
            match &mut expr.node {
                TypedExpression::Call(call) => {
                    self.rewrite_expr(&mut call.callee);
                    for a in &mut call.positional_args {
                        self.rewrite_expr(a);
                    }
                    for na in &mut call.named_args {
                        self.rewrite_expr(&mut na.value);
                    }
                }
                TypedExpression::Binary(b) => {
                    self.rewrite_expr(&mut b.left);
                    self.rewrite_expr(&mut b.right);
                }
                TypedExpression::Unary(u) => self.rewrite_expr(&mut u.operand),
                TypedExpression::Field(f) => self.rewrite_expr(&mut f.object),
                TypedExpression::Index(idx) => {
                    self.rewrite_expr(&mut idx.object);
                    self.rewrite_expr(&mut idx.index);
                }
                TypedExpression::Array(items) | TypedExpression::Tuple(items) => {
                    for it in items {
                        self.rewrite_expr(it);
                    }
                }
                TypedExpression::MethodCall(mc) => {
                    self.rewrite_expr(&mut mc.receiver);
                    for a in &mut mc.positional_args {
                        self.rewrite_expr(a);
                    }
                }
                TypedExpression::Block(b) => self.rewrite_block(b),
                TypedExpression::If(if_expr) => {
                    self.rewrite_expr(&mut if_expr.condition);
                    self.rewrite_expr(&mut if_expr.then_branch);
                    self.rewrite_expr(&mut if_expr.else_branch);
                }
                TypedExpression::Lambda(lam) => match &mut lam.body {
                    zyntax_typed_ast::typed_ast::TypedLambdaBody::Expression(e) => {
                        self.rewrite_expr(e);
                    }
                    zyntax_typed_ast::typed_ast::TypedLambdaBody::Block(block) => {
                        self.rewrite_block(block);
                    }
                },
                _ => {}
            }
            // After recursing into children, try to lower THIS expression
            // if it's a known bare fn call.
            self.try_resolve(expr);
        }

        fn try_resolve(&self, expr: &mut TypedNode<TypedExpression>) {
            let TypedExpression::Call(call) = &mut expr.node else {
                return;
            };
            // Only Variable callees — skip Field / MethodCall / etc.
            let TypedExpression::Variable(callee_name) = &call.callee.node else {
                return;
            };
            let Some(sig) = self.signatures.get(callee_name) else {
                return;
            };

            // Extract __named__ markers from positional_args.
            let mut positionals: Vec<TypedNode<TypedExpression>> = Vec::new();
            let mut named: Vec<TypedNamedArg> = std::mem::take(&mut call.named_args);
            for arg in std::mem::take(&mut call.positional_args) {
                if let TypedExpression::Call(inner) = &arg.node
                    && let TypedExpression::Variable(inner_callee) = &inner.callee.node
                    && inner_callee.resolve_global().as_deref() == Some("__named__")
                    && inner.positional_args.len() == 2
                    && let TypedExpression::Literal(TypedLiteral::String(arg_name)) =
                        &inner.positional_args[0].node
                {
                    named.push(TypedNamedArg {
                        name: *arg_name,
                        value: Box::new(inner.positional_args[1].clone()),
                        span: arg.span,
                    });
                    continue;
                }
                positionals.push(arg);
            }

            // No named args AND positional count matches → nothing to
            // splice. Restore positional_args verbatim and bail.
            if named.is_empty() && positionals.len() == sig.params.len() {
                call.positional_args = positionals;
                return;
            }
            // Also bail when the call has neither named args nor missing
            // trailing slots — `step(5)` against `fn step(x, by = 1)`
            // is the "splice default" case Zyntax already handles
            // natively (see `dsl_fn_default_param_splices_when_omitted_at_call_site`).
            // We only intervene when there's something to actually resolve.
            if named.is_empty() {
                call.positional_args = positionals;
                return;
            }

            // Slot-fill: positional first (by index), then named (by
            // name), then defaults for any still-empty slot.
            let mut slots: Vec<Option<TypedNode<TypedExpression>>> =
                (0..sig.params.len()).map(|_| None).collect();
            for (i, arg) in positionals.into_iter().enumerate() {
                if i < slots.len() {
                    slots[i] = Some(arg);
                }
                // Excess positionals are dropped; type-checker / arity
                // check would catch that as a separate error.
            }
            for na in named {
                if let Some(pos) = sig.params.iter().position(|(n, _)| *n == na.name) {
                    // Last-write-wins if a positional already filled
                    // this slot — matches Rust/Python semantics where
                    // `foo(x = 5)` with `x` already positional is a
                    // duplicate-arg error. We accept silently here;
                    // a stricter diagnostic is a follow-up.
                    slots[pos] = Some(*na.value);
                }
                // Unknown name → silently drop. Future: emit a
                // BLINC-NAMED-ARG-UNKNOWN diagnostic.
            }
            let mut new_positional: Vec<TypedNode<TypedExpression>> =
                Vec::with_capacity(slots.len());
            for (slot, (_, info)) in slots.into_iter().zip(sig.params.iter()) {
                if let Some(arg) = slot {
                    new_positional.push(arg);
                } else if let Some(default) = &info.default {
                    new_positional.push(default.clone());
                } else {
                    // Missing required arg with no default — let the
                    // downstream lowering surface the arity error.
                    break;
                }
            }
            call.positional_args = new_positional;
            // named_args was drained into `named`; leave it empty.
        }

        fn rewrite_block(&self, block: &mut zyntax_typed_ast::typed_ast::TypedBlock) {
            for stmt in &mut block.statements {
                self.rewrite_stmt(stmt);
            }
        }

        fn rewrite_stmt(&self, stmt: &mut TypedNode<TypedStatement>) {
            match &mut stmt.node {
                TypedStatement::Expression(e) => self.rewrite_expr(e),
                TypedStatement::Return(Some(e)) => self.rewrite_expr(e),
                TypedStatement::Let(l) => {
                    if let Some(init) = &mut l.initializer {
                        self.rewrite_expr(init);
                    }
                }
                TypedStatement::If(if_stmt) => {
                    self.rewrite_expr(&mut if_stmt.condition);
                    self.rewrite_block(&mut if_stmt.then_block);
                    if let Some(else_block) = &mut if_stmt.else_block {
                        self.rewrite_block(else_block);
                    }
                }
                TypedStatement::While(w) => {
                    self.rewrite_expr(&mut w.condition);
                    self.rewrite_block(&mut w.body);
                }
                TypedStatement::Block(b) => self.rewrite_block(b),
                _ => {}
            }
        }
    }

    let lowerer = Lowerer {
        signatures: &signatures,
    };
    for decl in &mut program.declarations {
        match &mut decl.node {
            TypedDeclaration::Function(func) => {
                if let Some(body) = &mut func.body {
                    lowerer.rewrite_block(body);
                }
            }
            TypedDeclaration::Impl(imp) => {
                for method in &mut imp.methods {
                    if let Some(body) = &mut method.body {
                        lowerer.rewrite_block(body);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Positionalise named args on `$Blinc$<X>$view` calls using the substrate
/// `ComponentRegistry` prop order. Zyntax's auto-injected extern decls carry
/// synthetic param names (`p0`, `p1`, …) that can't bind by name. Skipped
/// for user-declared components (handled by `bind_component_props`).
pub(crate) fn resolve_extern_widget_named_args(program: &mut TypedProgram) {
    use zyntax_typed_ast::{TypedCall, TypedDeclaration, TypedExpression};

    fn walk_stmt(stmt: &mut zyntax_typed_ast::TypedNode<TypedStatement>) {
        match &mut stmt.node {
            TypedStatement::Expression(e) => rewrite_expr(e),
            TypedStatement::Return(Some(e)) => rewrite_expr(e),
            TypedStatement::Let(l) => {
                if let Some(init) = &mut l.initializer {
                    rewrite_expr(init);
                }
            }
            TypedStatement::If(if_stmt) => {
                rewrite_expr(&mut if_stmt.condition);
                for s in &mut if_stmt.then_block.statements {
                    walk_stmt(s);
                }
                if let Some(else_block) = &mut if_stmt.else_block {
                    for s in &mut else_block.statements {
                        walk_stmt(s);
                    }
                }
            }
            _ => {}
        }
    }

    fn rewrite_expr(expr: &mut zyntax_typed_ast::TypedNode<TypedExpression>) {
        // Recurse first — nested calls resolved before the outer.
        match &mut expr.node {
            TypedExpression::Call(call) => {
                rewrite_expr(&mut call.callee);
                for arg in &mut call.positional_args {
                    rewrite_expr(arg);
                }
                for na in &mut call.named_args {
                    rewrite_expr(&mut na.value);
                }
            }
            TypedExpression::Array(items) => {
                for item in items {
                    rewrite_expr(item);
                }
            }
            TypedExpression::Block(block) => {
                for stmt in &mut block.statements {
                    walk_stmt(stmt);
                }
            }
            TypedExpression::Binary(b) => {
                rewrite_expr(&mut b.left);
                rewrite_expr(&mut b.right);
            }
            _ => {}
        }

        let span = expr.span;
        let TypedExpression::Call(call) = &mut expr.node else {
            return;
        };
        let Some(props) = primitive_callee_props(call) else {
            return;
        };
        // Don't early-out on empty named_args — trailing slots still need defaults
        // so call arity matches the extern signature.

        let mut slots: Vec<Option<zyntax_typed_ast::TypedNode<TypedExpression>>> =
            (0..props.len()).map(|_| None).collect();
        let existing_positional = std::mem::take(&mut call.positional_args);
        let mut overflow: Vec<zyntax_typed_ast::TypedNode<TypedExpression>> = Vec::new();
        for (i, arg) in existing_positional.into_iter().enumerate() {
            if i < slots.len() {
                slots[i] = Some(coerce_extern_arg_for_prop(arg, &props[i].1));
            } else {
                overflow.push(arg);
            }
        }

        let existing_named = std::mem::take(&mut call.named_args);
        let mut unresolved_named: Vec<zyntax_typed_ast::TypedNamedArg> = Vec::new();
        for na in existing_named {
            let Some(name) = na.name.resolve_global() else {
                unresolved_named.push(na);
                continue;
            };
            let name_str: &str = &name;
            if let Some(pos) = props.iter().position(|(n, _)| n == name_str) {
                if slots[pos].is_some() {
                    unresolved_named.push(na);
                } else {
                    slots[pos] = Some(coerce_extern_arg_for_prop(*na.value, &props[pos].1));
                }
            } else {
                unresolved_named.push(na);
            }
        }

        // Fill unfilled slots with type-appropriate defaults so call arity matches.
        let mut new_positional: Vec<zyntax_typed_ast::TypedNode<TypedExpression>> =
            Vec::with_capacity(slots.len());
        for (slot, (_, ty)) in slots.into_iter().zip(props.iter()) {
            if let Some(arg) = slot {
                new_positional.push(arg);
            } else {
                new_positional.push(default_literal_for(ty, span));
            }
        }
        new_positional.extend(overflow);

        call.positional_args = new_positional;
        call.named_args = unresolved_named;
    }

    /// Bool-like widget props keep an `i32` extern ABI slot, so a DSL
    /// `true`/`false` literal is lowered to `1`/`0` before the final call.
    fn coerce_extern_arg_for_prop(
        arg: zyntax_typed_ast::TypedNode<TypedExpression>,
        ty: &Type,
    ) -> zyntax_typed_ast::TypedNode<TypedExpression> {
        if !matches!(ty, Type::Primitive(PrimitiveType::Bool)) {
            return arg;
        }
        let span = arg.span;
        let arg_ty = arg.ty;
        match arg.node {
            TypedExpression::Literal(zyntax_typed_ast::TypedLiteral::Bool(value)) => typed_node(
                TypedExpression::Literal(zyntax_typed_ast::TypedLiteral::Integer(if value {
                    1
                } else {
                    0
                })),
                Type::Primitive(PrimitiveType::I32),
                span,
            ),
            other if matches!(&arg_ty, Type::Primitive(PrimitiveType::Bool)) => {
                let bool_expr = typed_node(other, Type::Primitive(PrimitiveType::Bool), span);
                typed_node(
                    TypedExpression::If(zyntax_typed_ast::typed_ast::TypedIfExpr {
                        condition: Box::new(bool_expr),
                        then_branch: Box::new(typed_node(
                            TypedExpression::Literal(zyntax_typed_ast::TypedLiteral::Integer(1)),
                            Type::Primitive(PrimitiveType::I32),
                            span,
                        )),
                        else_branch: Box::new(typed_node(
                            TypedExpression::Literal(zyntax_typed_ast::TypedLiteral::Integer(0)),
                            Type::Primitive(PrimitiveType::I32),
                            span,
                        )),
                    }),
                    Type::Primitive(PrimitiveType::I32),
                    span,
                )
            }
            other => typed_node(other, arg_ty, span),
        }
    }

    /// Substrate primitive's prop (name, type) pairs in declaration order.
    fn primitive_callee_props(call: &TypedCall) -> Option<Vec<(String, Type)>> {
        let TypedExpression::Variable(callee) = &call.callee.node else {
            return None;
        };
        let sym = callee.resolve_global()?;
        let sym: &str = &sym;
        let name = sym
            .strip_prefix("$Blinc$")
            .and_then(|s| s.strip_suffix("$view"))?;
        blinc_runtime::component::with_component_registry(|r| {
            r.get_by_name(name).map(|def| {
                def.props
                    .iter()
                    .map(|p| (p.name.to_string(), p.ty.clone()))
                    .collect()
            })
        })
    }

    /// Default literal for an unsupplied prop slot (`0` / `0.0` / `""`).
    fn default_literal_for(
        ty: &Type,
        span: zyntax_typed_ast::Span,
    ) -> zyntax_typed_ast::TypedNode<TypedExpression> {
        match ty {
            Type::Primitive(PrimitiveType::Bool) => typed_node(
                TypedExpression::Literal(zyntax_typed_ast::TypedLiteral::Integer(0)),
                Type::Primitive(PrimitiveType::I32),
                span,
            ),
            Type::Primitive(PrimitiveType::F64) => typed_node(
                TypedExpression::Literal(zyntax_typed_ast::TypedLiteral::Float(0.0)),
                ty.clone(),
                span,
            ),
            Type::Primitive(PrimitiveType::String) => typed_node(
                TypedExpression::Literal(zyntax_typed_ast::TypedLiteral::String(
                    zyntax_typed_ast::InternedString::new_global(""),
                )),
                ty.clone(),
                span,
            ),
            _ => typed_node(
                TypedExpression::Literal(zyntax_typed_ast::TypedLiteral::Integer(0)),
                ty.clone(),
                span,
            ),
        }
    }

    for decl in program.declarations.iter_mut() {
        match &mut decl.node {
            TypedDeclaration::Function(func) => {
                if let Some(body) = func.body.as_mut() {
                    for stmt in &mut body.statements {
                        walk_stmt(stmt);
                    }
                }
            }
            TypedDeclaration::Impl(imp) => {
                for method in &mut imp.methods {
                    if let Some(body) = method.body.as_mut() {
                        for stmt in &mut body.statements {
                            walk_stmt(stmt);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
