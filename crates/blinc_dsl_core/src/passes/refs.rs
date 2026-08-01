//! `ref name: Scroll` — element handles, and what a source does with them.
//!
//! A ref is a value, not a name: `ref = pages` hands the handle itself
//! to a widget, which binds it, and `pages.scroll_to_top()` acts on
//! that handle. The same shape signals already use, where a prop
//! carries a `SignalId` rather than a signal's name.
//!
//! The id comes from the declaration's own source span, so nothing asks
//! the author for a key. Two instances of a component that declares a
//! ref get two handles, and a key chosen by hand could not promise
//! that.

use crate::*;

/// The marker types `ref_kind` emits, and the kind each denotes.
///
/// A marker is what separates a ref declaration from `signal x: T`,
/// since both are external, param-less decls; the author writes
/// `Scroll` or `Div` and the AST carries a name nothing else can spell.
const REF_MARKERS: &[(&str, crate::refs::RefKind)] = &[
    ("BlincScrollRef", crate::refs::RefKind::Scroll),
    ("BlincDivRef", crate::refs::RefKind::Element),
    ("BlincInputRef", crate::refs::RefKind::Input),
];

/// Is this the declaration a `ref` lowered to?
///
/// The marker resolves as `Unresolved` or as a registered `Named`
/// depending on whether anything else claimed the name, so both
/// spellings are accepted — the same latitude the `View` return marker
/// needs.
pub(crate) fn is_ref_decl(
    func: &zyntax_typed_ast::typed_ast::TypedFunction,
    registry: &zyntax_typed_ast::type_registry::TypeRegistry,
) -> bool {
    ref_kind_of(func, registry).is_some()
}

/// The kind this declaration denotes, or `None` if it is not a ref.
fn ref_kind_of(
    func: &zyntax_typed_ast::typed_ast::TypedFunction,
    registry: &zyntax_typed_ast::type_registry::TypeRegistry,
) -> Option<crate::refs::RefKind> {
    if !(func.is_external && func.params.is_empty() && func.body.is_none()) {
        return None;
    }
    let name = type_name(&func.return_type, registry)?;
    REF_MARKERS
        .iter()
        .find(|(marker, _)| *marker == name)
        .map(|(_, kind)| *kind)
}

/// The marker resolves as `Unresolved` or as a registered `Named`
/// depending on whether anything else claimed the name, so both
/// spellings are read — the same latitude the `View` return marker needs.
fn type_name(
    ty: &Type,
    registry: &zyntax_typed_ast::type_registry::TypeRegistry,
) -> Option<String> {
    match ty {
        Type::Unresolved(n) => n.resolve_global().map(|s| s.to_string()),
        Type::Named { id, .. } => registry
            .get_type_by_id(*id)
            .and_then(|t| t.name.resolve_global())
            .map(|s| s.to_string()),
        _ => None,
    }
}

/// Rewrite every use of a declared ref, and strip the declarations.
///
/// - `pages.scroll_to_top()` → `__scroll_to_top_by_id(<id>)`
/// - a bare `pages`, as in `ref = pages` → the id, so the prop carries
///   the handle rather than a name to look one up by
pub(crate) fn resolve_ref_calls(program: &mut TypedProgram, filename: &str) {
    use std::collections::HashMap;
    use zyntax_typed_ast::InternedString;
    use zyntax_typed_ast::typed_ast::{TypedCall, TypedDeclaration, TypedExpression, TypedLiteral};

    // Declaration site → handle id. `call_site_instance_id` is what the
    // component pass already keys per-call-site state on, so a ref is
    // stable across rebuilds and distinct per declaration for free.
    let mut refs: HashMap<InternedString, (u64, crate::refs::RefKind)> = HashMap::new();
    for decl in &program.declarations {
        let TypedDeclaration::Function(func) = &decl.node else {
            continue;
        };
        let Some(kind) = ref_kind_of(func, &program.type_registry) else {
            continue;
        };
        let id = crate::passes::call_site_instance_id(filename, decl.span.start);
        crate::refs::mint(id, kind);
        refs.insert(func.name, (id, kind));
    }
    if refs.is_empty() {
        return;
    }

    fn id_literal(id: u64, span: Span) -> zyntax_typed_ast::TypedNode<TypedExpression> {
        zyntax_typed_ast::TypedNode::new(
            TypedExpression::Literal(TypedLiteral::Integer(id as i128)),
            Type::Primitive(PrimitiveType::I64),
            span,
        )
    }

    /// The extern a method on a scroll handle lowers to, and how many
    /// arguments it takes beyond the id.
    fn method_extern(
        kind: crate::refs::RefKind,
        method: &str,
        argc: usize,
    ) -> Option<&'static str> {
        use crate::refs::RefKind;
        match (kind, method, argc) {
            (RefKind::Scroll, "scroll_to_top", 0) => Some("__scroll_to_top_by_id"),
            (RefKind::Scroll, "scroll_to_bottom", 0) => Some("__scroll_to_bottom_by_id"),
            (RefKind::Scroll, "scroll_by", 2) => Some("__scroll_by_id"),
            (RefKind::Element, "focus", 0) => Some("__ref_focus_by_id"),
            (RefKind::Element, "blur", 0) => Some("__ref_blur_by_id"),
            (RefKind::Element, "scroll_into_view", 0) => Some("__ref_scroll_into_view_by_id"),
            (RefKind::Input, "focus", 0) => Some("__input_focus_by_id"),
            (RefKind::Input, "blur", 0) => Some("__input_blur_by_id"),
            (RefKind::Input, "clear", 0) => Some("__input_clear_by_id"),
            (RefKind::Input, "select_all", 0) => Some("__input_select_all_by_id"),
            _ => None,
        }
    }

    fn rewrite_expr(
        expr: &mut zyntax_typed_ast::TypedNode<TypedExpression>,
        refs: &HashMap<InternedString, (u64, crate::refs::RefKind)>,
    ) {
        let span = expr.span;
        // Two shapes reach here for `pages.scroll_to_top()`: a
        // `MethodCall` in expression position, and a `Call` over a
        // `Field` in statement position. Same call, and the statement
        // form is the one a handler body produces.
        let receiver_and_method = match &expr.node {
            TypedExpression::MethodCall(mc) => match &mc.receiver.node {
                TypedExpression::Variable(name) => {
                    Some((*name, mc.method, mc.positional_args.clone()))
                }
                _ => None,
            },
            TypedExpression::Call(c) => match &c.callee.node {
                TypedExpression::Field(f) => match &f.object.node {
                    TypedExpression::Variable(name) => {
                        Some((*name, f.field, c.positional_args.clone()))
                    }
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        };
        if let Some((receiver, method, mut args)) = receiver_and_method
            && let Some((id, kind)) = refs.get(&receiver).copied()
        {
            for arg in &mut args {
                rewrite_expr(arg, refs);
            }
            let Some(name) = method.resolve_global() else {
                return;
            };
            let Some(extern_name) = method_extern(kind, name.as_ref(), args.len()) else {
                tracing::warn!(
                    method = %name,
                    args = args.len(),
                    ?kind,
                    "no such method on this ref",
                );
                return;
            };
            let mut positional_args = vec![id_literal(id, span)];
            positional_args.append(&mut args);
            expr.node = TypedExpression::Call(TypedCall {
                callee: Box::new(zyntax_typed_ast::TypedNode::new(
                    TypedExpression::Variable(InternedString::new_global(extern_name)),
                    Type::Unknown,
                    span,
                )),
                positional_args,
                named_args: vec![],
                type_args: vec![],
            });
            expr.ty = Type::Primitive(PrimitiveType::Unit);
            return;
        }

        match &mut expr.node {
            // `ref = pages` — the prop takes the handle itself.
            TypedExpression::Variable(name) => {
                if let Some((id, _)) = refs.get(name).copied() {
                    *expr = id_literal(id, span);
                }
            }
            TypedExpression::Call(call) => {
                for a in &mut call.positional_args {
                    rewrite_expr(a, refs);
                }
                for a in &mut call.named_args {
                    rewrite_expr(&mut a.value, refs);
                }
            }
            TypedExpression::Block(b) => rewrite_block(b, refs),
            TypedExpression::Lambda(lam) => match &mut lam.body {
                zyntax_typed_ast::typed_ast::TypedLambdaBody::Expression(e) => {
                    rewrite_expr(e, refs)
                }
                zyntax_typed_ast::typed_ast::TypedLambdaBody::Block(b) => rewrite_block(b, refs),
            },
            TypedExpression::Binary(b) => {
                rewrite_expr(&mut b.left, refs);
                rewrite_expr(&mut b.right, refs);
            }
            TypedExpression::If(if_expr) => {
                rewrite_expr(&mut if_expr.condition, refs);
                rewrite_expr(&mut if_expr.then_branch, refs);
                rewrite_expr(&mut if_expr.else_branch, refs);
            }
            _ => {}
        }
    }

    fn rewrite_block(
        block: &mut zyntax_typed_ast::typed_ast::TypedBlock,
        refs: &HashMap<InternedString, (u64, crate::refs::RefKind)>,
    ) {
        for stmt in &mut block.statements {
            rewrite_stmt(stmt, refs);
        }
    }

    fn rewrite_stmt(
        stmt: &mut zyntax_typed_ast::TypedNode<TypedStatement>,
        refs: &HashMap<InternedString, (u64, crate::refs::RefKind)>,
    ) {
        match &mut stmt.node {
            TypedStatement::Expression(e) => rewrite_expr(e, refs),
            TypedStatement::Let(l) => {
                if let Some(init) = &mut l.initializer {
                    rewrite_expr(init, refs);
                }
            }
            TypedStatement::Return(Some(e)) => rewrite_expr(e, refs),
            TypedStatement::Block(b) => rewrite_block(b, refs),
            TypedStatement::If(i) => {
                rewrite_expr(&mut i.condition, refs);
                rewrite_block(&mut i.then_block, refs);
                if let Some(e) = &mut i.else_block {
                    rewrite_block(e, refs);
                }
            }
            TypedStatement::While(w) => {
                rewrite_expr(&mut w.condition, refs);
                rewrite_block(&mut w.body, refs);
            }
            _ => {}
        }
    }

    for decl in &mut program.declarations {
        match &mut decl.node {
            TypedDeclaration::Function(func) => {
                if let Some(body) = &mut func.body {
                    rewrite_block(body, &refs);
                }
            }
            TypedDeclaration::Impl(imp) => {
                for method in &mut imp.methods {
                    if let Some(body) = &mut method.body {
                        rewrite_block(body, &refs);
                    }
                }
            }
            _ => {}
        }
    }

    // Declare the host functions the rewrites call. Registering the
    // pointer is not enough: the lowerer resolves against declarations
    // in the program, and these names appear only after this pass has
    // run, so nothing at parse time could have injected them.
    for (name, argc) in [
        ("__scroll_to_top_by_id", 0usize),
        ("__scroll_to_bottom_by_id", 0),
        ("__scroll_by_id", 2),
        ("__ref_focus_by_id", 0),
        ("__ref_blur_by_id", 0),
        ("__ref_scroll_into_view_by_id", 0),
        ("__input_focus_by_id", 0),
        ("__input_blur_by_id", 0),
        ("__input_clear_by_id", 0),
        ("__input_select_all_by_id", 0),
    ] {
        let mut params = vec![zyntax_typed_ast::typed_ast::TypedParameter {
            name: InternedString::new_global("id"),
            ty: Type::Primitive(PrimitiveType::I64),
            ..Default::default()
        }];
        for i in 0..argc {
            params.push(zyntax_typed_ast::typed_ast::TypedParameter {
                name: InternedString::new_global(&format!("a{i}")),
                ty: Type::Primitive(PrimitiveType::F64),
                ..Default::default()
            });
        }
        program.declarations.push(zyntax_typed_ast::TypedNode::new(
            TypedDeclaration::Function(zyntax_typed_ast::typed_ast::TypedFunction {
                name: InternedString::new_global(name),
                params,
                return_type: Type::Primitive(PrimitiveType::Unit),
                body: None,
                is_external: true,
                link_name: None,
                ..Default::default()
            }),
            Type::Unknown,
            Span::default(),
        ));
    }

    // The declarations are markers; nothing lowers them.
    let registry = program.type_registry.clone();
    program.declarations.retain(|decl| match &decl.node {
        TypedDeclaration::Function(func) => !is_ref_decl(func, &registry),
        _ => true,
    });
}
