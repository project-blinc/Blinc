//! FSM lowering: triggers, subscriptions, context markers, registry, event enums, trait interfaces, ctx field access.

use super::*;
use crate::*;

/// Rewrite `<FsmName>.trigger(<path>)` → `__fsm_runtime_trigger__("<FsmName>", <path>)`.
///
/// Two sources of "this is a known FSM" are checked at each call
/// site:
///
/// 1. **Local impls in this program** (same-file FSMs): collected
///    up-front into `fsm_names` from `__fsm_meta__`-bearing impls.
/// 2. **Global `FsmRegistry`** (cross-file FSMs imported from
///    previously-compiled modules under the same `module` key):
///    queried per call site when the receiver's name doesn't match
///    a local entry. Lets `MyFsm.trigger("Idle.Start")` in main.blinc
///    resolve to a `MyFsm` (or its module-mangled form like
///    `alpha$MyFsm` after import-rewrite) declared in alpha.blinc.
///
/// Without (2) the early-return at the top of this pass would
/// bail when the entry program has no local FSM impls, leaving
/// cross-file trigger calls unresolved at run time.
pub(crate) fn resolve_fsm_trigger_calls(
    program: &mut TypedProgram,
    module: zyntax_typed_ast::InternedString,
) {
    use std::collections::HashSet;
    use zyntax_typed_ast::InternedString;
    use zyntax_typed_ast::typed_ast::{TypedCall, TypedDeclaration, TypedExpression, TypedLiteral};

    // Step 1: collect declared FSM names from `__fsm_meta__`-bearing impls.
    let mut fsm_names: HashSet<InternedString> = HashSet::new();
    for decl in &program.declarations {
        if let TypedDeclaration::Impl(imp) = &decl.node
            && imp.trait_name.resolve_global().is_some()
            && imp
                .methods
                .iter()
                .any(|m| m.name.resolve_global().as_deref() == Some("__fsm_meta__"))
        {
            fsm_names.insert(imp.trait_name);
        }
    }
    // NOTE: don't early-return on `fsm_names.is_empty()` — the entry
    // program in a multi-file project may declare zero local FSMs
    // but still reference imported ones. The per-call global-
    // registry lookup below covers that case.

    // Visitor wrapper keeps `fsm_names` + `module` in scope across
    // the recursive rewrite without threading them through every
    // helper signature.
    struct Rewriter<'a> {
        fsm_names: &'a HashSet<InternedString>,
        module: InternedString,
    }

    impl Rewriter<'_> {
        fn rewrite_expr(&self, expr: &mut zyntax_typed_ast::TypedNode<TypedExpression>) {
            // Recurse children first.
            match &mut expr.node {
                TypedExpression::Binary(b) => {
                    self.rewrite_expr(&mut b.left);
                    self.rewrite_expr(&mut b.right);
                }
                TypedExpression::Unary(u) => self.rewrite_expr(&mut u.operand),
                TypedExpression::Call(c) => {
                    self.rewrite_expr(&mut c.callee);
                    for a in &mut c.positional_args {
                        self.rewrite_expr(a);
                    }
                }
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
            self.try_rewrite_trigger(expr);
        }

        fn try_rewrite_trigger(&self, expr: &mut zyntax_typed_ast::TypedNode<TypedExpression>) {
            // Match `<FsmName>.trigger(<arg>)` in both AST shapes (MethodCall / Call+Field).
            let trigger_call = match &expr.node {
                TypedExpression::MethodCall(mc) if mc.positional_args.len() == 1 => {
                    if let TypedExpression::Variable(receiver_name) = &mc.receiver.node {
                        Some((
                            *receiver_name,
                            mc.method,
                            mc.positional_args[0].clone(),
                            expr.span,
                        ))
                    } else {
                        None
                    }
                }
                TypedExpression::Call(c) if c.positional_args.len() == 1 => {
                    if let TypedExpression::Field(f) = &c.callee.node {
                        if let TypedExpression::Variable(receiver_name) = &f.object.node {
                            Some((
                                *receiver_name,
                                f.field,
                                c.positional_args[0].clone(),
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

            let Some((receiver_name, method, path_arg, span)) = trigger_call else {
                return;
            };
            if method.resolve_global().as_deref() != Some("trigger") {
                return;
            }
            // Local FSMs (current program) take precedence; cross-file
            // FSMs (previously-compiled modules) found in the global
            // registry are accepted second. Receiver names that match
            // neither leave the original `MethodCall` shape alone — the
            // type-checker / linker surfaces them as undefined later.
            if !self.fsm_names.contains(&receiver_name) {
                let Some(name_str_arc) = receiver_name.resolve_global() else {
                    return;
                };
                let name_str: &str = &name_str_arc;
                let found_in_global = crate::fsm_registry::with_fsm_registry(|r| {
                    r.find_by_name(self.module, name_str).is_some()
                });
                if !found_in_global {
                    return;
                }
            }

            let fsm_name_arg = zyntax_typed_ast::TypedNode::new(
                TypedExpression::Literal(TypedLiteral::String(receiver_name)),
                Type::Primitive(PrimitiveType::String),
                span,
            );
            let callee = zyntax_typed_ast::TypedNode::new(
                TypedExpression::Variable(InternedString::new_global("__fsm_runtime_trigger__")),
                Type::Unknown,
                span,
            );
            expr.node = TypedExpression::Call(TypedCall {
                callee: Box::new(callee),
                positional_args: vec![fsm_name_arg, path_arg],
                named_args: vec![],
                type_args: vec![],
            });
            expr.ty = Type::Primitive(PrimitiveType::Unit);
        }

        fn rewrite_block(&self, block: &mut zyntax_typed_ast::typed_ast::TypedBlock) {
            for stmt in &mut block.statements {
                self.rewrite_stmt(stmt);
            }
        }

        fn rewrite_stmt(&self, stmt: &mut zyntax_typed_ast::TypedNode<TypedStatement>) {
            match &mut stmt.node {
                TypedStatement::Expression(e) => self.rewrite_expr(e),
                TypedStatement::Let(l) => {
                    if let Some(init) = &mut l.initializer {
                        self.rewrite_expr(init);
                    }
                }
                TypedStatement::Return(Some(e)) => self.rewrite_expr(e),
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

    let rewriter = Rewriter {
        fsm_names: &fsm_names,
        module,
    };

    for decl in &mut program.declarations {
        match &mut decl.node {
            TypedDeclaration::Function(func) => {
                if let Some(body) = &mut func.body {
                    rewriter.rewrite_block(body);
                }
            }
            TypedDeclaration::Impl(imp) => {
                for method in &mut imp.methods {
                    if let Some(body) = &mut method.body {
                        rewriter.rewrite_block(body);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Rewrite `<FsmName>.subscribe(<path>, <closure>)` →
/// `__fsm_subscribe__("<FsmName>", <path>, <closure>)`. Path filtering happens
/// host-side in `blinc_runtime::fsm::register_subscriber`.
///
/// Same two-source resolution as [`resolve_fsm_trigger_calls`] —
/// local impls AND global-registry imports both count. See that
/// doc for the rationale.
pub(crate) fn resolve_fsm_subscribe_calls(
    program: &mut TypedProgram,
    module: zyntax_typed_ast::InternedString,
) {
    use std::collections::HashSet;
    use zyntax_typed_ast::InternedString;
    use zyntax_typed_ast::typed_ast::{TypedCall, TypedDeclaration, TypedExpression, TypedLiteral};

    let mut fsm_names: HashSet<InternedString> = HashSet::new();
    for decl in &program.declarations {
        if let TypedDeclaration::Impl(imp) = &decl.node
            && imp.trait_name.resolve_global().is_some()
            && imp
                .methods
                .iter()
                .any(|m| m.name.resolve_global().as_deref() == Some("__fsm_meta__"))
        {
            fsm_names.insert(imp.trait_name);
        }
    }
    // Don't early-return on empty local set — cross-file FSMs found
    // via the global registry are still resolvable.

    struct Rewriter<'a> {
        fsm_names: &'a HashSet<InternedString>,
        module: InternedString,
    }

    impl Rewriter<'_> {
        fn rewrite_expr(&self, expr: &mut zyntax_typed_ast::TypedNode<TypedExpression>) {
            match &mut expr.node {
                TypedExpression::Binary(b) => {
                    self.rewrite_expr(&mut b.left);
                    self.rewrite_expr(&mut b.right);
                }
                TypedExpression::Unary(u) => self.rewrite_expr(&mut u.operand),
                TypedExpression::Call(c) => {
                    self.rewrite_expr(&mut c.callee);
                    for a in &mut c.positional_args {
                        self.rewrite_expr(a);
                    }
                }
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
            self.try_rewrite_subscribe(expr);
        }

        fn try_rewrite_subscribe(&self, expr: &mut zyntax_typed_ast::TypedNode<TypedExpression>) {
            // Two shapes:
            //   * 2-arg `subscribe(path, closure)` → `__fsm_subscribe__(name, path, closure)`
            //   * 1-arg `subscribe(closure)`       → `__fsm_subscribe_all__(name, closure)`
            // Both ASTs (MethodCall / Call+Field) carry through. Tuple is
            // `(receiver, method, args:Vec<expr>, span)` where the args
            // vector's length distinguishes the two forms.
            let subscribe_call = match &expr.node {
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

            let Some((receiver_name, method, args, span)) = subscribe_call else {
                return;
            };
            if method.resolve_global().as_deref() != Some("subscribe") {
                return;
            }
            if !self.fsm_names.contains(&receiver_name) {
                let Some(name_str_arc) = receiver_name.resolve_global() else {
                    return;
                };
                let name_str: &str = &name_str_arc;
                let found_in_global = crate::fsm_registry::with_fsm_registry(|r| {
                    r.find_by_name(self.module, name_str).is_some()
                });
                if !found_in_global {
                    return;
                }
            }

            let fsm_name_arg = zyntax_typed_ast::TypedNode::new(
                TypedExpression::Literal(TypedLiteral::String(receiver_name)),
                Type::Primitive(PrimitiveType::String),
                span,
            );

            match args.len() {
                2 => {
                    // Path-filtered form.
                    let path_arg = args[0].clone();
                    let closure_arg = args[1].clone();
                    let callee = zyntax_typed_ast::TypedNode::new(
                        TypedExpression::Variable(InternedString::new_global("__fsm_subscribe__")),
                        Type::Unknown,
                        span,
                    );
                    expr.node = TypedExpression::Call(TypedCall {
                        callee: Box::new(callee),
                        positional_args: vec![fsm_name_arg, path_arg, closure_arg],
                        named_args: vec![],
                        type_args: vec![],
                    });
                    expr.ty = Type::Primitive(PrimitiveType::Unit);
                }
                1 => {
                    // All-paths form. The closure is a one-arg lambda
                    // whose body receives the matched `"From.Event"`
                    // path string each transition.
                    let closure_arg = args[0].clone();
                    let callee = zyntax_typed_ast::TypedNode::new(
                        TypedExpression::Variable(InternedString::new_global(
                            "__fsm_subscribe_all__",
                        )),
                        Type::Unknown,
                        span,
                    );
                    expr.node = TypedExpression::Call(TypedCall {
                        callee: Box::new(callee),
                        positional_args: vec![fsm_name_arg, closure_arg],
                        named_args: vec![],
                        type_args: vec![],
                    });
                    expr.ty = Type::Primitive(PrimitiveType::Unit);
                }
                _ => {
                    // Wrong arity — leave the call shape alone; the
                    // type checker / linker surfaces the error.
                }
            }
        }

        fn rewrite_block(&self, block: &mut zyntax_typed_ast::typed_ast::TypedBlock) {
            for stmt in &mut block.statements {
                self.rewrite_stmt(stmt);
            }
        }

        fn rewrite_stmt(&self, stmt: &mut zyntax_typed_ast::TypedNode<TypedStatement>) {
            match &mut stmt.node {
                TypedStatement::Expression(e) => self.rewrite_expr(e),
                TypedStatement::Let(l) => {
                    if let Some(init) = &mut l.initializer {
                        self.rewrite_expr(init);
                    }
                }
                TypedStatement::Return(Some(e)) => self.rewrite_expr(e),
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

    let rewriter = Rewriter {
        fsm_names: &fsm_names,
        module,
    };

    for decl in &mut program.declarations {
        match &mut decl.node {
            TypedDeclaration::Function(func) => {
                if let Some(body) = &mut func.body {
                    rewriter.rewrite_block(body);
                }
            }
            TypedDeclaration::Impl(imp) => {
                for method in &mut imp.methods {
                    if let Some(body) = &mut method.body {
                        rewriter.rewrite_block(body);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Wrap each `__fsm_meta__` body with `__fsm_begin__("Name")` / `__fsm_end__()`
/// so inner marker calls know which fsm they're configuring. Idempotent.
pub(crate) fn inject_fsm_context_markers(program: &mut TypedProgram) {
    use zyntax_typed_ast::typed_ast::{
        TypedCall, TypedDeclaration, TypedExpression, TypedLiteral, TypedStatement,
    };
    use zyntax_typed_ast::{InternedString, TypedNode};

    fn make_marker_call(callee: &str, str_args: &[&str]) -> TypedNode<TypedStatement> {
        let args: Vec<TypedNode<TypedExpression>> = str_args
            .iter()
            .map(|s| {
                TypedNode::new(
                    TypedExpression::Literal(TypedLiteral::String(InternedString::new_global(s))),
                    Type::Primitive(PrimitiveType::String),
                    Span::default(),
                )
            })
            .collect();

        let call = TypedExpression::Call(TypedCall {
            callee: Box::new(TypedNode::new(
                TypedExpression::Variable(InternedString::new_global(callee)),
                Type::Unknown,
                Span::default(),
            )),
            positional_args: args,
            named_args: vec![],
            type_args: vec![],
        });

        TypedNode::new(
            TypedStatement::Expression(Box::new(TypedNode::new(
                call,
                Type::Primitive(PrimitiveType::Unit),
                Span::default(),
            ))),
            Type::Primitive(PrimitiveType::Unit),
            Span::default(),
        )
    }

    for decl in &mut program.declarations {
        let TypedDeclaration::Impl(imp) = &mut decl.node else {
            continue;
        };
        let Some(fsm_name) = imp.trait_name.resolve_global() else {
            continue;
        };

        for method in &mut imp.methods {
            if method.name.resolve_global().as_deref() != Some("__fsm_meta__") {
                continue;
            }
            let Some(body) = method.body.as_mut() else {
                continue;
            };

            // Skip if already wrapped (defensive against double-application).
            let already_wrapped = body
                .statements
                .first()
                .map(|s| {
                    let TypedStatement::Expression(e) = &s.node else {
                        return false;
                    };
                    let TypedExpression::Call(c) = &e.node else {
                        return false;
                    };
                    let TypedExpression::Variable(callee) = &c.callee.node else {
                        return false;
                    };
                    callee.resolve_global().as_deref() == Some("__fsm_begin__")
                })
                .unwrap_or(false);
            if already_wrapped {
                continue;
            }

            let begin = make_marker_call("__fsm_begin__", &[&fsm_name]);
            let end = make_marker_call("__fsm_end__", &[]);
            body.statements.insert(0, begin);
            body.statements.push(end);
        }
    }
}

/// Populate the global `FsmRegistry` from each fsm's `__fsm_meta__` body and
/// strip the meta method. Three phases: scan, pin TypeIds, strip markers.
pub(crate) fn populate_fsm_registry_pass(
    program: &mut TypedProgram,
    module: zyntax_typed_ast::InternedString,
) {
    use zyntax_typed_ast::InternedString;
    use zyntax_typed_ast::type_registry::{
        TypeDefinition, TypeId, TypeKind, VariantDef, VariantFields, Visibility,
    };
    use zyntax_typed_ast::typed_ast::{
        TypedDeclaration, TypedExpression, TypedLiteral, TypedVariantFields,
    };

    // Step 1: scan. Collect (fsm_name, FsmDefinition) tuples.
    let mut found: Vec<(InternedString, FsmDefinition)> = Vec::new();
    let mut guards_to_lift: Vec<(
        InternedString,
        zyntax_typed_ast::TypedNode<zyntax_typed_ast::TypedExpression>,
    )> = Vec::new();

    for decl in &program.declarations {
        let TypedDeclaration::Impl(imp) = &decl.node else {
            continue;
        };
        let Some(meta) = imp
            .methods
            .iter()
            .find(|m| m.name.resolve_global().as_deref() == Some("__fsm_meta__"))
        else {
            continue;
        };
        let Some(body) = meta.body.as_ref() else {
            continue;
        };

        let fsm_name = imp.trait_name;
        let mut def = FsmDefinition {
            name: Some(fsm_name),
            ..Default::default()
        };

        for stmt_node in &body.statements {
            let TypedStatement::Expression(expr_node) = &stmt_node.node else {
                continue;
            };
            let TypedExpression::Call(call) = &expr_node.node else {
                continue;
            };
            let TypedExpression::Variable(callee_id) = &call.callee.node else {
                continue;
            };
            let callee = callee_id.resolve_global().unwrap_or_default();

            // Helper: pull a string-literal arg at index `idx`.
            let str_arg = |idx: usize| -> Option<InternedString> {
                call.positional_args.get(idx).and_then(|a| {
                    if let TypedExpression::Literal(TypedLiteral::String(s)) = &a.node {
                        Some(*s)
                    } else {
                        None
                    }
                })
            };

            match callee.as_str() {
                "__fsm_initial__" => {
                    if let Some(state) = str_arg(0) {
                        def.initial = Some(state);
                    }
                }
                "__fsm_context_field__" => {
                    // arg 0 = name (StringLiteral)
                    // arg 1 = type-name (StringLiteral, e.g. "i32")
                    // arg 2 = default literal expression
                    use zyntax_typed_ast::typed_ast::TypedLiteral as TL;
                    let (Some(field_name), Some(field_ty_name)) = (str_arg(0), str_arg(1)) else {
                        continue;
                    };
                    let Some(default_node) = call.positional_args.get(2) else {
                        continue;
                    };
                    let Some(field_ty_str) = field_ty_name.resolve_global() else {
                        continue;
                    };
                    let default = match (&field_ty_str[..], &default_node.node) {
                        ("i32", TypedExpression::Literal(TL::Integer(n))) => {
                            crate::fsm_registry::ContextDefault::I32(*n as i32)
                        }
                        ("f64", TypedExpression::Literal(TL::Float(f))) => {
                            crate::fsm_registry::ContextDefault::F64(*f)
                        }
                        ("f64", TypedExpression::Literal(TL::Integer(n))) => {
                            crate::fsm_registry::ContextDefault::F64(*n as f64)
                        }
                        ("bool", TypedExpression::Literal(TL::Bool(b))) => {
                            crate::fsm_registry::ContextDefault::Bool(*b)
                        }
                        ("string", TypedExpression::Literal(TL::String(s)))
                        | ("str", TypedExpression::Literal(TL::String(s))) => {
                            crate::fsm_registry::ContextDefault::String(*s)
                        }
                        _ => {
                            // Default for unsupported (ty, literal) combos —
                            // emit a zero-of-type so downstream init stays
                            // sane and the lowered signal still exists.
                            match &field_ty_str[..] {
                                "i32" => crate::fsm_registry::ContextDefault::I32(0),
                                "f64" => crate::fsm_registry::ContextDefault::F64(0.0),
                                "bool" => crate::fsm_registry::ContextDefault::Bool(false),
                                _ => crate::fsm_registry::ContextDefault::String(
                                    InternedString::new_global(""),
                                ),
                            }
                        }
                    };
                    def.context_fields.push(crate::fsm_registry::ContextField {
                        name: field_name,
                        ty: field_ty_name,
                        default,
                    });
                }
                "__fsm_transition__" => {
                    if let (Some(from), Some(event), Some(to)) =
                        (str_arg(0), str_arg(1), str_arg(2))
                    {
                        // Optional 4th positional arg: lifted action
                        // symbol name (a StringLiteral the early
                        // `synthesize_fsm_context_and_actions` pass
                        // wrote in place of the original Block body).
                        let actions = if let Some(action_sym) = str_arg(3) {
                            action_sym
                                .resolve_global()
                                .map(|s| {
                                    vec![blinc_runtime::fsm::TransitionAction::Symbol(
                                        std::sync::Arc::from(s.as_ref()),
                                    )]
                                })
                                .unwrap_or_default()
                        } else {
                            vec![]
                        };
                        def.transitions.push(EventTransition {
                            from,
                            event,
                            to,
                            actions,
                        });
                    }
                }
                "__fsm_tick__" => {
                    // args: 0=from, 1=guard expr, 2=to. Lift guard into a top-level fn
                    // so it survives `__fsm_meta__` stripping.
                    if let (Some(from), Some(to)) = (str_arg(0), str_arg(2)) {
                        let idx = def.tick_guards.len();
                        let fsm_name_str = fsm_name.resolve_global().unwrap_or_default();
                        let guard_fn_name = format!("__fsm_tick_guard_{fsm_name_str}_{idx}__");
                        let guard_fn = InternedString::new_global(&guard_fn_name);

                        // Clone the guard expression to escape the read borrow on `program`.
                        if let Some(expr_node) = call.positional_args.get(1) {
                            guards_to_lift.push((guard_fn, expr_node.clone()));
                        }

                        def.tick_guards.push(TickGuard {
                            from,
                            to,
                            guard_fn: Some(guard_fn),
                        });
                    }
                }
                _ => {}
            }
        }

        found.push((fsm_name, def));
    }

    // Step 2: pin TypeIds + populate the registry. Pre-register so Zyntax's
    // compile path short-circuits and respects our id.
    for (fsm_name, def) in &found {
        let type_id = TypeId::next();

        // Pin `decl.ty` so Zyntax's enum-registration check respects our id.
        let named_ty = program.type_registry.make_type(type_id, Vec::new());
        for decl in &mut program.declarations {
            let TypedDeclaration::Enum(enum_decl) = &decl.node else {
                continue;
            };
            if enum_decl.name == *fsm_name {
                decl.ty = named_ty.clone();
                break;
            }
        }

        // Pre-register so Zyntax skips double-registration with a fresh TypeId.
        if let Some(enum_decl) = program.declarations.iter().find_map(|d| match &d.node {
            TypedDeclaration::Enum(e) if e.name == *fsm_name => Some(e),
            _ => None,
        }) {
            let variants: Vec<VariantDef> = enum_decl
                .variants
                .iter()
                .enumerate()
                .map(|(i, v)| VariantDef {
                    name: v.name,
                    fields: match &v.fields {
                        TypedVariantFields::Unit => VariantFields::Unit,
                        TypedVariantFields::Tuple(types) => VariantFields::Tuple(types.clone()),
                        TypedVariantFields::Named(_) => VariantFields::Unit,
                    },
                    discriminant: Some(i as i64),
                    span: v.span,
                })
                .collect();

            let type_def = TypeDefinition {
                id: type_id,
                // The module the state enum belongs to, which the
                // registry now resolves names through. Same module the
                // `FsmId` below is keyed by, so a same-named FSM in
                // another file stays a different type.
                module: Some(module),
                name: enum_decl.name,
                kind: TypeKind::Enum { variants },
                type_params: Vec::new(),
                constraints: Vec::new(),
                fields: Vec::new(),
                methods: Vec::new(),
                constructors: Vec::new(),
                metadata: Default::default(),
                span: enum_decl.span,
            };
            let _: TypeId = program.type_registry.register_type(type_def);
            let _ = Visibility::Public; // silence unused-import in case the path changes upstream
        }

        let id = FsmId { module, type_id };
        with_fsm_registry_mut(|r| r.upsert(id, def.clone()));
    }

    // Step 3: lift each captured tick-guard expression into a top-level fn
    // returning i32 (1 if guard fires, 0 otherwise). i32 chosen because bool-return
    // ABI marshaling through `runtime.call::<bool>` is untested upstream.
    use zyntax_typed_ast::typed_ast::{TypedFunction, TypedIf};
    for (fn_name, guard_expr) in guards_to_lift {
        let i32_ty = Type::Primitive(PrimitiveType::I32);

        // `return 1`
        let return_one = zyntax_typed_ast::TypedNode::new(
            TypedStatement::Return(Some(Box::new(zyntax_typed_ast::TypedNode::new(
                TypedExpression::Literal(zyntax_typed_ast::typed_ast::TypedLiteral::Integer(1)),
                i32_ty.clone(),
                Span::default(),
            )))),
            i32_ty.clone(),
            Span::default(),
        );

        let then_block = zyntax_typed_ast::typed_ast::TypedBlock {
            statements: vec![return_one],
            span: Span::default(),
        };

        // `if <guard> { return 1 }`
        let if_stmt = zyntax_typed_ast::TypedNode::new(
            TypedStatement::If(TypedIf {
                condition: Box::new(guard_expr),
                then_block,
                else_block: None,
                span: Span::default(),
            }),
            Type::Primitive(PrimitiveType::Unit),
            Span::default(),
        );

        // `return 0`
        let return_zero = zyntax_typed_ast::TypedNode::new(
            TypedStatement::Return(Some(Box::new(zyntax_typed_ast::TypedNode::new(
                TypedExpression::Literal(zyntax_typed_ast::typed_ast::TypedLiteral::Integer(0)),
                i32_ty.clone(),
                Span::default(),
            )))),
            i32_ty.clone(),
            Span::default(),
        );

        let body = zyntax_typed_ast::typed_ast::TypedBlock {
            statements: vec![if_stmt, return_zero],
            span: Span::default(),
        };

        let func = TypedFunction {
            name: fn_name,
            return_type: i32_ty.clone(),
            body: Some(body),
            ..Default::default()
        };
        let decl_node = zyntax_typed_ast::TypedNode::new(
            TypedDeclaration::Function(func),
            Type::Unknown,
            Span::default(),
        );
        program.declarations.push(decl_node);
    }

    // Step 4: strip `__fsm_meta__` so compile doesn't try to resolve markers.
    for decl in &mut program.declarations {
        let TypedDeclaration::Impl(imp) = &mut decl.node else {
            continue;
        };
        imp.methods
            .retain(|m| m.name.resolve_global().as_deref() != Some("__fsm_meta__"));
    }

    // Step 5: the same machines again, as fibers. Additive — nothing
    // consumes them yet, and the registry above still drives every
    // mounted FSM. Emitted here because this is where the definitions
    // exist in one place.
    crate::passes::synthesize_fsm_fibers(program, &found[..]);
}

/// Synthesise a sibling `<FSM>Event` enum for every fsm with transitions.
/// Variants are the unique event names in declaration order. Tick transitions
/// don't have user-facing event names and never appear here.
pub(crate) fn synthesize_fsm_event_enums(program: &mut TypedProgram) {
    use std::collections::HashSet;
    use zyntax_typed_ast::type_registry::Visibility;
    use zyntax_typed_ast::typed_ast::{
        TypedDeclaration, TypedEnum, TypedExpression, TypedLiteral, TypedVariant,
        TypedVariantFields,
    };
    use zyntax_typed_ast::{InternedString, TypedNode};

    let mut event_enums: Vec<TypedNode<TypedDeclaration>> = Vec::new();

    for decl in &program.declarations {
        let TypedDeclaration::Impl(imp) = &decl.node else {
            continue;
        };

        // Find the synthesised `__fsm_meta__` method.
        let Some(meta) = imp
            .methods
            .iter()
            .find(|m| m.name.resolve_global().as_deref() == Some("__fsm_meta__"))
        else {
            continue;
        };
        let Some(body) = meta.body.as_ref() else {
            continue;
        };

        // Collect unique event names from `__fsm_transition__(_, event,
        // _)` markers, preserving declaration order so the runtime
        // discriminant assignment is stable.
        let mut events: Vec<InternedString> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        for stmt_node in &body.statements {
            let TypedStatement::Expression(expr_node) = &stmt_node.node else {
                continue;
            };
            let TypedExpression::Call(call) = &expr_node.node else {
                continue;
            };
            let TypedExpression::Variable(callee) = &call.callee.node else {
                continue;
            };
            if callee.resolve_global().as_deref() != Some("__fsm_transition__") {
                continue;
            }
            let Some(event_arg) = call.positional_args.get(1) else {
                continue;
            };
            let TypedExpression::Literal(TypedLiteral::String(name)) = &event_arg.node else {
                continue;
            };
            let key = name.resolve_global().unwrap_or_default();
            if !key.is_empty() && seen.insert(key) {
                events.push(*name);
            }
        }

        if events.is_empty() {
            // Tick-only fsm — nothing to synthesise.
            continue;
        }

        // Use `trait_name` (bare ident) rather than `for_type` (Type::Named).
        let fsm_name = imp.trait_name.resolve_global().unwrap_or_default();
        let event_enum_name = InternedString::new_global(&format!("{fsm_name}Event"));

        let variants: Vec<TypedVariant> = events
            .into_iter()
            .map(|name| TypedVariant {
                name,
                fields: TypedVariantFields::Unit,
                discriminant: None,
                span: Span::default(),
            })
            .collect();

        let event_enum = TypedDeclaration::Enum(TypedEnum {
            name: event_enum_name,
            type_params: vec![],
            variants,
            visibility: Visibility::Public,
            span: Span::default(),
        });

        event_enums.push(TypedNode::new(event_enum, Type::Unknown, Span::default()));
    }

    // Append at the end so `find_map` lookups still return user-declared decls first.
    program.declarations.extend(event_enums);
}

/// Mint a placeholder `Interface { name: <FsmName> }` for each FSM impl so
/// Zyntax's compiler doesn't log "Trait not found" and drop the impl's methods.
pub(crate) fn synthesize_fsm_trait_interfaces(program: &mut TypedProgram) {
    use std::collections::HashSet;
    use zyntax_typed_ast::type_registry::Visibility;
    use zyntax_typed_ast::typed_ast::{TypedDeclaration, TypedInterface};
    use zyntax_typed_ast::{InternedString, TypedNode};

    let mut fsm_names: HashSet<InternedString> = HashSet::new();
    for decl in &program.declarations {
        let TypedDeclaration::Impl(imp) = &decl.node else {
            continue;
        };
        if imp
            .methods
            .iter()
            .any(|m| m.name.resolve_global().as_deref() == Some("__fsm_meta__"))
        {
            fsm_names.insert(imp.trait_name);
        }
    }

    let interfaces: Vec<TypedNode<TypedDeclaration>> = fsm_names
        .into_iter()
        .map(|name| {
            let iface = TypedInterface {
                name,
                type_params: vec![],
                extends: vec![],
                methods: vec![],
                associated_types: vec![],
                visibility: Visibility::Public,
                span: Span::default(),
            };
            TypedNode::new(
                TypedDeclaration::Interface(iface),
                Type::Unknown,
                Span::default(),
            )
        })
        .collect();

    program.declarations.extend(interfaces);
}

/// Rewrite `ctx.<field>` → `<mangled_signal_name>` inside the supplied
/// block. Used by both the action-body lifter and the tick-guard
/// expression handler so the two paths share one resolution rule.
///
/// `Field { object: Variable("ctx"), field: <name> }` → `Variable(__fsm_ctx_<Fsm>_<name>)`.
///
/// Any other appearance of bare `ctx` (e.g. `let x = ctx`,
/// `someFn(ctx)`) is left untouched — downstream resolution will error
/// out because `ctx` isn't a real binding. We rely on that to surface
/// misuse rather than implementing a separate check here.
fn rewrite_fsm_ctx_access_block(
    fsm_name: &str,
    block: &mut zyntax_typed_ast::typed_ast::TypedBlock,
) {
    for stmt in &mut block.statements {
        rewrite_fsm_ctx_access_stmt(fsm_name, stmt);
    }
}

fn rewrite_fsm_ctx_access_stmt(
    fsm_name: &str,
    stmt: &mut zyntax_typed_ast::TypedNode<TypedStatement>,
) {
    match &mut stmt.node {
        TypedStatement::Expression(e) => rewrite_fsm_ctx_access_expr(fsm_name, e),
        TypedStatement::Let(l) => {
            if let Some(init) = &mut l.initializer {
                rewrite_fsm_ctx_access_expr(fsm_name, init);
            }
        }
        TypedStatement::Return(Some(e)) => rewrite_fsm_ctx_access_expr(fsm_name, e),
        TypedStatement::Block(b) => rewrite_fsm_ctx_access_block(fsm_name, b),
        TypedStatement::If(if_stmt) => {
            rewrite_fsm_ctx_access_expr(fsm_name, &mut if_stmt.condition);
            rewrite_fsm_ctx_access_block(fsm_name, &mut if_stmt.then_block);
            if let Some(else_block) = &mut if_stmt.else_block {
                rewrite_fsm_ctx_access_block(fsm_name, else_block);
            }
        }
        TypedStatement::While(w) => {
            rewrite_fsm_ctx_access_expr(fsm_name, &mut w.condition);
            rewrite_fsm_ctx_access_block(fsm_name, &mut w.body);
        }
        _ => {}
    }
}

fn rewrite_fsm_ctx_access_expr(
    fsm_name: &str,
    expr: &mut zyntax_typed_ast::TypedNode<zyntax_typed_ast::typed_ast::TypedExpression>,
) {
    use zyntax_typed_ast::InternedString;
    use zyntax_typed_ast::typed_ast::{TypedExpression, TypedLambdaBody};

    // Walk children first so deeper accesses are rewritten before the
    // outer node is inspected.
    match &mut expr.node {
        TypedExpression::Call(c) => {
            rewrite_fsm_ctx_access_expr(fsm_name, &mut c.callee);
            for a in &mut c.positional_args {
                rewrite_fsm_ctx_access_expr(fsm_name, a);
            }
        }
        TypedExpression::MethodCall(mc) => {
            rewrite_fsm_ctx_access_expr(fsm_name, &mut mc.receiver);
            for a in &mut mc.positional_args {
                rewrite_fsm_ctx_access_expr(fsm_name, a);
            }
        }
        TypedExpression::Binary(b) => {
            rewrite_fsm_ctx_access_expr(fsm_name, &mut b.left);
            rewrite_fsm_ctx_access_expr(fsm_name, &mut b.right);
        }
        TypedExpression::Unary(u) => {
            rewrite_fsm_ctx_access_expr(fsm_name, &mut u.operand);
        }
        TypedExpression::Field(f) => {
            // Recurse into the object first — handles
            // `something.ctx.field` shapes if they ever appear; mostly
            // a no-op since `ctx` is leftmost.
            rewrite_fsm_ctx_access_expr(fsm_name, &mut f.object);
        }
        TypedExpression::Index(i) => {
            rewrite_fsm_ctx_access_expr(fsm_name, &mut i.object);
            rewrite_fsm_ctx_access_expr(fsm_name, &mut i.index);
        }
        TypedExpression::Array(items) | TypedExpression::Tuple(items) => {
            for it in items {
                rewrite_fsm_ctx_access_expr(fsm_name, it);
            }
        }
        TypedExpression::Block(block) => {
            rewrite_fsm_ctx_access_block(fsm_name, block);
        }
        TypedExpression::If(if_expr) => {
            rewrite_fsm_ctx_access_expr(fsm_name, &mut if_expr.condition);
            rewrite_fsm_ctx_access_expr(fsm_name, &mut if_expr.then_branch);
            rewrite_fsm_ctx_access_expr(fsm_name, &mut if_expr.else_branch);
        }
        TypedExpression::Lambda(lam) => match &mut lam.body {
            TypedLambdaBody::Expression(e) => rewrite_fsm_ctx_access_expr(fsm_name, e),
            TypedLambdaBody::Block(block) => rewrite_fsm_ctx_access_block(fsm_name, block),
        },
        _ => {}
    }

    // Now look at THIS node: is it `Field { object: Variable("ctx"), field: <name> }`?
    let is_ctx_field = match &expr.node {
        TypedExpression::Field(f) => match &f.object.node {
            TypedExpression::Variable(name) => name.resolve_global().as_deref() == Some("ctx"),
            _ => false,
        },
        _ => false,
    };
    if !is_ctx_field {
        return;
    }
    let TypedExpression::Field(f) = &expr.node else {
        return;
    };
    let Some(field_name) = f.field.resolve_global() else {
        return;
    };
    let mangled = crate::fsm_registry::mangle_ctx_signal(fsm_name, &field_name);
    let span = expr.span;
    let ty = expr.ty.clone();
    expr.node = TypedExpression::Variable(InternedString::new_global(&mangled));
    expr.ty = ty;
    expr.span = span;
}

/// Early FSM-context pass: scans each `__fsm_meta__` body for
/// `__fsm_context_field__` and `__fsm_transition__` markers; emits
/// top-level signal declarations for context fields; lifts action
/// bodies (4th-positional `Block` on `__fsm_transition__`) to
/// top-level fns `__fsm_action_<Fsm>_<idx>__`; rewrites `ctx.<field>`
/// access inside each lifted body to the mangled signal name. Also
/// applies the ctx-rewrite to tick-guard expressions
/// (`__fsm_tick__("from", <guard>, "to")` arg 1) so guards can read
/// context like actions can.
///
/// Side effects on `__fsm_meta__`:
///   - `__fsm_context_field__` markers are LEFT in place so the
///     publish-step can read them.
///   - `__fsm_transition__` markers with a 4th-arg Block are rewritten:
///     the Block is replaced by a `StringLiteral(<lifted_symbol_name>)`.
///     `populate_fsm_registry_pass` reads that string and emits
///     `TransitionAction::Symbol(...)` on the `EventTransition`.
pub(crate) fn synthesize_fsm_context_and_actions(program: &mut TypedProgram) {
    use zyntax_typed_ast::TypedNode;
    use zyntax_typed_ast::typed_ast::{
        TypedBlock, TypedCall, TypedDeclaration, TypedExpression, TypedFunction, TypedLiteral,
    };
    use zyntax_typed_ast::{InternedString, Mutability};

    // Collected work — applied after the read loop so we don't hold
    // borrows across mutations.
    struct ActionLift {
        fn_name: InternedString,
        body: TypedBlock,
    }
    struct CtxFieldDecl {
        signal_name: String,
        ty: Type,
    }

    let mut signal_decls: Vec<CtxFieldDecl> = Vec::new();
    let mut action_lifts: Vec<ActionLift> = Vec::new();

    fn type_from_name(ty_name: &str) -> Option<Type> {
        Some(Type::Primitive(match ty_name {
            "i32" => PrimitiveType::I32,
            "f64" => PrimitiveType::F64,
            "bool" => PrimitiveType::Bool,
            "string" | "str" => PrimitiveType::String,
            _ => return None,
        }))
    }

    for decl in &mut program.declarations {
        let TypedDeclaration::Impl(imp) = &mut decl.node else {
            continue;
        };
        let Some(fsm_name) = imp.trait_name.resolve_global() else {
            continue;
        };
        let fsm_name_str: &str = &fsm_name;

        for method in &mut imp.methods {
            if method.name.resolve_global().as_deref() != Some("__fsm_meta__") {
                continue;
            }
            let Some(body) = method.body.as_mut() else {
                continue;
            };

            let mut action_idx: usize = 0;
            for stmt in &mut body.statements {
                let TypedStatement::Expression(expr_node) = &mut stmt.node else {
                    continue;
                };
                let TypedExpression::Call(call) = &mut expr_node.node else {
                    continue;
                };
                let TypedExpression::Variable(callee_id) = &call.callee.node else {
                    continue;
                };
                let Some(callee) = callee_id.resolve_global() else {
                    continue;
                };
                let callee_str: &str = &callee;

                match callee_str {
                    "__fsm_context_field__" => {
                        // arg[0] = name (StringLiteral)
                        // arg[1] = type (StringLiteral, e.g. "i32")
                        // arg[2] = default expression (literal)
                        let Some(name_arg) = call.positional_args.first() else {
                            continue;
                        };
                        let Some(ty_arg) = call.positional_args.get(1) else {
                            continue;
                        };
                        let TypedExpression::Literal(TypedLiteral::String(name_intern)) =
                            &name_arg.node
                        else {
                            continue;
                        };
                        let TypedExpression::Literal(TypedLiteral::String(ty_intern)) =
                            &ty_arg.node
                        else {
                            continue;
                        };
                        let Some(name_str) = name_intern.resolve_global() else {
                            continue;
                        };
                        let Some(ty_str) = ty_intern.resolve_global() else {
                            continue;
                        };
                        let Some(field_ty) = type_from_name(&ty_str) else {
                            // Unknown type — skip; populate_fsm_registry_pass
                            // will emit a diagnostic on the same marker shape.
                            continue;
                        };
                        let signal_name =
                            crate::fsm_registry::mangle_ctx_signal(fsm_name_str, &name_str);
                        signal_decls.push(CtxFieldDecl {
                            signal_name,
                            ty: field_ty,
                        });
                    }
                    "__fsm_transition__" => {
                        // 4th positional arg (if present) is the action
                        // body `Block`. Lift it; rewrite the marker arg
                        // to a string literal carrying the lifted symbol.
                        if call.positional_args.len() < 4 {
                            continue;
                        }
                        let body_arg = call.positional_args[3].clone();
                        let TypedExpression::Block(action_block) = body_arg.node else {
                            continue;
                        };
                        // Apply ctx-rewrite to the lifted body before
                        // it leaves this FSM's scope.
                        let mut rewritten = action_block;
                        rewrite_fsm_ctx_access_block(fsm_name_str, &mut rewritten);

                        let fn_name_str = format!("__fsm_action_{fsm_name_str}_{action_idx}__");
                        let fn_name = InternedString::new_global(&fn_name_str);
                        action_lifts.push(ActionLift {
                            fn_name,
                            body: rewritten,
                        });

                        // Replace the Block arg with a StringLiteral
                        // carrying the lifted symbol name. populate
                        // reads it as args[3] and emits
                        // TransitionAction::Symbol(...).
                        call.positional_args[3] = TypedNode::new(
                            TypedExpression::Literal(TypedLiteral::String(
                                InternedString::new_global(&fn_name_str),
                            )),
                            Type::Primitive(PrimitiveType::String),
                            body_arg.span,
                        );
                        action_idx += 1;
                    }
                    "__fsm_tick__" => {
                        // arg[1] is the raw guard expression; apply
                        // ctx-rewrite so guards can read context fields
                        // the same way actions do.
                        if let Some(guard_expr) = call.positional_args.get_mut(1) {
                            rewrite_fsm_ctx_access_expr(fsm_name_str, guard_expr);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // Emit top-level signal decls (extern fn-with-no-body shape) so
    // `resolve_signal_calls` recognises them. The signal-init pass
    // (publish-side) seeds non-zero defaults at FSM-registration time.
    for ctx_field in signal_decls {
        let sig_func = TypedFunction {
            name: InternedString::new_global(&ctx_field.signal_name),
            params: vec![],
            return_type: ctx_field.ty,
            body: None,
            is_external: true,
            link_name: None,
            ..Default::default()
        };
        program.declarations.push(TypedNode::new(
            TypedDeclaration::Function(sig_func),
            Type::Unknown,
            Span::default(),
        ));
    }

    // Emit lifted action fns.
    for ActionLift { fn_name, body } in action_lifts {
        let func = TypedFunction {
            name: fn_name,
            params: vec![],
            return_type: Type::Primitive(PrimitiveType::Unit),
            body: Some(body),
            ..Default::default()
        };
        program.declarations.push(TypedNode::new(
            TypedDeclaration::Function(func),
            Type::Unknown,
            Span::default(),
        ));
    }

    // Silence "unused" lints on imports only used through pattern matches above.
    let _ = std::any::type_name::<TypedCall>();
    let _ = std::any::type_name::<Mutability>();
}

/// Resolve `<FsmName>.<field>` field-access expressions appearing
/// OUTSIDE an FSM body (view, init, other components) by rewriting them
/// to the mangled signal identifier. Required so user-facing code can
/// read / write FSM context like a signal:
///
///   `Text(f"{CounterFsm.count.get()}")`     // read
///   `CounterFsm.count.set(0)`               // write via set()
///   `@stateful([CounterFsm.count])`         // binding list
///
/// After this pass, the surface forms reach `resolve_signal_calls` as
/// plain `Variable(<mangled>).get()` etc.
///
/// Discrimination: only rewrites when a matching synthetic signal decl
/// (`__fsm_ctx_<Fsm>_<field>`) exists. Non-FSM field-access shapes
/// (struct.field) pass through unchanged.
pub(crate) fn resolve_dotted_fsm_field_access(program: &mut TypedProgram) {
    use std::collections::HashSet;
    use zyntax_typed_ast::InternedString;
    use zyntax_typed_ast::typed_ast::{TypedDeclaration, TypedExpression, TypedLambdaBody};

    // Build the set of known mangled context-signal names from the
    // synthesized extern decls.
    let mut known_ctx_signals: HashSet<String> = HashSet::new();
    for decl in &program.declarations {
        if let TypedDeclaration::Function(f) = &decl.node {
            if !is_signal_decl(f) {
                continue;
            }
            let Some(name) = f.name.resolve_global() else {
                continue;
            };
            if name.starts_with("__fsm_ctx_") {
                known_ctx_signals.insert(name.to_string());
            }
        }
    }
    if known_ctx_signals.is_empty() {
        return;
    }

    fn rewrite_expr(
        expr: &mut zyntax_typed_ast::TypedNode<TypedExpression>,
        known: &HashSet<String>,
    ) {
        // Children first.
        match &mut expr.node {
            TypedExpression::Call(c) => {
                rewrite_expr(&mut c.callee, known);
                for a in &mut c.positional_args {
                    rewrite_expr(a, known);
                }
                // MUST also walk named args. Without this, `Div(opacity = Ticker.pct)`
                // ships its `Ticker.pct` Field access through to the
                // styling-args lowering as a raw Field — never matches
                // `signal_id_for_variable`, falls back to literal path,
                // overlay's `*_signal_id_raw` stays None, and the live
                // binding never wires up.
                for na in &mut c.named_args {
                    rewrite_expr(&mut na.value, known);
                }
            }
            TypedExpression::MethodCall(mc) => {
                rewrite_expr(&mut mc.receiver, known);
                for a in &mut mc.positional_args {
                    rewrite_expr(a, known);
                }
                for na in &mut mc.named_args {
                    rewrite_expr(&mut na.value, known);
                }
            }
            TypedExpression::Binary(b) => {
                rewrite_expr(&mut b.left, known);
                rewrite_expr(&mut b.right, known);
            }
            TypedExpression::Unary(u) => rewrite_expr(&mut u.operand, known),
            TypedExpression::Field(f) => rewrite_expr(&mut f.object, known),
            TypedExpression::Index(i) => {
                rewrite_expr(&mut i.object, known);
                rewrite_expr(&mut i.index, known);
            }
            TypedExpression::Array(items) | TypedExpression::Tuple(items) => {
                for it in items {
                    rewrite_expr(it, known);
                }
            }
            TypedExpression::Block(block) => {
                for stmt in &mut block.statements {
                    rewrite_stmt(stmt, known);
                }
            }
            TypedExpression::If(if_expr) => {
                rewrite_expr(&mut if_expr.condition, known);
                rewrite_expr(&mut if_expr.then_branch, known);
                rewrite_expr(&mut if_expr.else_branch, known);
            }
            TypedExpression::Lambda(lam) => match &mut lam.body {
                TypedLambdaBody::Expression(e) => rewrite_expr(e, known),
                TypedLambdaBody::Block(block) => {
                    for stmt in &mut block.statements {
                        rewrite_stmt(stmt, known);
                    }
                }
            },
            _ => {}
        }

        // Now check THIS node for `Field { object: Variable(<FsmName>),
        // field: <name> }` and rewrite to a Variable lookup of the
        // mangled signal name — but only when that mangled signal
        // actually exists.
        let TypedExpression::Field(f) = &expr.node else {
            return;
        };
        let TypedExpression::Variable(obj_name) = &f.object.node else {
            return;
        };
        let Some(obj_str) = obj_name.resolve_global() else {
            return;
        };
        let Some(field_str) = f.field.resolve_global() else {
            return;
        };
        // Under `compile_project` every file carries a module
        // namespace, so the FSM declaration is mangled (`Play` ->
        // `shell$Play`) and its synthesised context signal with it,
        // while the source-level reference stays `Play.pct`. Match the
        // unmangled candidate first, then fall back to any known
        // context signal whose FSM segment matches after the `$`
        // namespace prefix. Without the fallback the Field access is
        // left untouched, SSA reads a bare `Play` as an undefined
        // variable, and the whole view fn is silently dropped.
        let candidate = crate::fsm_registry::mangle_ctx_signal(&obj_str, &field_str);
        let candidate = if known.contains(&candidate) {
            candidate
        } else {
            let suffix = format!("${obj_str}_{field_str}");
            let Some(namespaced) = known
                .iter()
                .find(|n| n.starts_with("__fsm_ctx_") && n.ends_with(&suffix))
            else {
                return;
            };
            namespaced.clone()
        };
        let span = expr.span;
        let ty = expr.ty.clone();
        expr.node = TypedExpression::Variable(InternedString::new_global(&candidate));
        expr.ty = ty;
        expr.span = span;
    }

    fn rewrite_stmt(
        stmt: &mut zyntax_typed_ast::TypedNode<TypedStatement>,
        known: &HashSet<String>,
    ) {
        match &mut stmt.node {
            TypedStatement::Expression(e) => rewrite_expr(e, known),
            TypedStatement::Let(l) => {
                if let Some(init) = &mut l.initializer {
                    rewrite_expr(init, known);
                }
            }
            TypedStatement::Return(Some(e)) => rewrite_expr(e, known),
            TypedStatement::Block(b) => {
                for inner in &mut b.statements {
                    rewrite_stmt(inner, known);
                }
            }
            TypedStatement::If(if_stmt) => {
                rewrite_expr(&mut if_stmt.condition, known);
                for inner in &mut if_stmt.then_block.statements {
                    rewrite_stmt(inner, known);
                }
                if let Some(else_block) = &mut if_stmt.else_block {
                    for inner in &mut else_block.statements {
                        rewrite_stmt(inner, known);
                    }
                }
            }
            TypedStatement::While(w) => {
                rewrite_expr(&mut w.condition, known);
                for inner in &mut w.body.statements {
                    rewrite_stmt(inner, known);
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
                        rewrite_stmt(stmt, &known_ctx_signals);
                    }
                }
            }
            TypedDeclaration::Impl(imp) => {
                for m in &mut imp.methods {
                    if let Some(body) = &mut m.body {
                        for stmt in &mut body.statements {
                            rewrite_stmt(stmt, &known_ctx_signals);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
