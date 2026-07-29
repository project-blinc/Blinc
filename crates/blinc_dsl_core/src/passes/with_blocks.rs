//! `with @fsm([…]) { … }` — inline reactive regions.
//!
//! The block's body is lifted into a synthetic component view, and the
//! site it stood at becomes `__blinc_with__(<region-id>, <that view's
//! call>)`. The host builtin mounts a `Stateful` over the returned
//! widget and re-renders only that view when a listed dependency
//! changes, so a signal write no longer tears down the whole program.
//!
//! Lifting to a real component rather than keeping the body inline is
//! what makes the rest of the pipeline work unchanged: component-call
//! lowering, children expansion and the value-returning promotion all
//! key on an impl method named `view`, and none of them descend into
//! lambda bodies.

use crate::*;
use zyntax_typed_ast::typed_ast::TypedBlock;

/// One `with` block: the synthetic component its body became, plus the
/// dependencies written on it.
#[derive(Debug, Clone)]
pub(crate) struct WithRegion {
    /// Region id, unique process-wide. Also the builtin's first arg.
    pub id: i64,
    /// `__blinc_with_<id>` — the component name `render_component` takes.
    pub name: String,
    /// Signal names from `@stateful([a, b])`. Empty means "all declared".
    pub signal_deps: Vec<String>,
    /// FSM names from `@fsm([A, B])`.
    pub fsms: Vec<String>,
    /// Names from the bare `with count, Play { … }` form, still
    /// unclassified. `BlincDsl::register_with_regions` sorts them into
    /// signals and FSMs against what the program declared — the grammar
    /// cannot tell the two apart, and capitalisation is a convention,
    /// not a rule.
    pub named_deps: Vec<String>,
    /// `"<Fsm>.<field>"` for every context field the body reads as a
    /// value — see [`super::collect_ctx_value_reads`].
    pub ctx_value_reads: Vec<String>,
}

/// Process-wide so ids stay unique across recompiles. A hot reload
/// mints fresh regions rather than reusing a previous compile's, which
/// keeps a stale entry from being mounted against a new body.
static NEXT_REGION_ID: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(1);

/// A lifted region and the body that became its view.
type Lifted = (WithRegion, TypedBlock);

/// Lift every `with` block into a synthetic component and rewrite its
/// site. Returns one entry per lifted block.
///
/// MUST run before [`super::detect_and_strip_stateful_views`]: a `with`
/// block's decorators belong to the region, and left in place they
/// would be read as a decoration on the enclosing view, which is
/// precisely the whole-program wrap this exists to avoid.
pub(crate) fn lower_with_blocks(program: &mut TypedProgram) -> Vec<WithRegion> {
    use zyntax_typed_ast::typed_ast::TypedDeclaration;

    let mut lifted: Vec<Lifted> = Vec::new();

    for decl in program.declarations.iter_mut() {
        match &mut decl.node {
            TypedDeclaration::Function(func) => {
                if let Some(body) = func.body.as_mut() {
                    rewrite_block(body, &mut lifted);
                }
            }
            TypedDeclaration::Impl(imp) => {
                for method in imp.methods.iter_mut() {
                    if let Some(body) = method.body.as_mut() {
                        rewrite_block(body, &mut lifted);
                    }
                }
            }
            _ => {}
        }
    }

    // Appended after the walk, so the synthetic views are not re-walked
    // and a `with` nested inside another `with` does not lift. Nesting
    // one reactive region inside another has no meaning the outer
    // region doesn't already cover.
    let mut regions = Vec::with_capacity(lifted.len());
    for (region, body) in lifted {
        program.declarations.push(synthetic_class(&region));
        program.declarations.push(synthetic_impl(&region, body));
        regions.push(region);
    }
    regions
}

/// The marker the grammar plants: `__blinc_with__(<zero-arg lambda>)`.
/// Takes the lambda's block out, leaving an empty one behind.
fn with_marker_body(
    expr: &mut zyntax_typed_ast::TypedNode<zyntax_typed_ast::typed_ast::TypedExpression>,
) -> Option<TypedBlock> {
    use zyntax_typed_ast::typed_ast::{TypedExpression, TypedLambdaBody};

    let span = expr.span;
    let TypedExpression::Call(call) = &mut expr.node else {
        return None;
    };
    let TypedExpression::Variable(callee) = &call.callee.node else {
        return None;
    };
    if callee.resolve_global().as_deref() != Some("__blinc_with__") {
        return None;
    }
    // Already lowered (two args, no lambda) — leave it alone.
    let [arg] = call.positional_args.as_mut_slice() else {
        return None;
    };
    let TypedExpression::Lambda(lam) = &mut arg.node else {
        return None;
    };
    // The action language builds `Lambda { body: Block { … } }` as an
    // EXPRESSION body holding a block expression, not as a block body.
    // Both shapes are the same statement list; accept either.
    let empty = || TypedBlock {
        statements: vec![],
        span,
    };
    match &mut lam.body {
        TypedLambdaBody::Block(block) => Some(std::mem::replace(block, empty())),
        TypedLambdaBody::Expression(e) => match &mut e.node {
            TypedExpression::Block(block) => Some(std::mem::replace(block, empty())),
            _ => None,
        },
    }
}

fn rewrite_block(block: &mut TypedBlock, lifted: &mut Vec<Lifted>) {
    for stmt in block.statements.iter_mut() {
        rewrite_stmt(stmt, lifted);
    }
}

fn rewrite_stmt(stmt: &mut zyntax_typed_ast::TypedNode<TypedStatement>, lifted: &mut Vec<Lifted>) {
    match &mut stmt.node {
        TypedStatement::Expression(e) => rewrite_expr(e, lifted),
        TypedStatement::Return(Some(e)) => rewrite_expr(e, lifted),
        TypedStatement::Let(l) => {
            if let Some(init) = &mut l.initializer {
                rewrite_expr(init, lifted);
            }
        }
        TypedStatement::If(i) => {
            rewrite_expr(&mut i.condition, lifted);
            rewrite_block(&mut i.then_block, lifted);
            if let Some(else_block) = &mut i.else_block {
                rewrite_block(else_block, lifted);
            }
        }
        TypedStatement::While(w) => {
            rewrite_expr(&mut w.condition, lifted);
            rewrite_block(&mut w.body, lifted);
        }
        TypedStatement::Block(b) => rewrite_block(b, lifted),
        _ => {}
    }
}

fn rewrite_expr(
    expr: &mut zyntax_typed_ast::TypedNode<zyntax_typed_ast::typed_ast::TypedExpression>,
    lifted: &mut Vec<Lifted>,
) {
    use zyntax_typed_ast::typed_ast::{TypedExpression, TypedLambdaBody};

    // Recurse first: a `with` inside a widget body sits in the body
    // Block that rides as the component call's trailing arg.
    match &mut expr.node {
        TypedExpression::Call(c) => {
            for a in c.positional_args.iter_mut() {
                rewrite_expr(a, lifted);
            }
            for n in c.named_args.iter_mut() {
                rewrite_expr(&mut n.value, lifted);
            }
        }
        TypedExpression::Block(b) => rewrite_block(b, lifted),
        TypedExpression::Binary(b) => {
            rewrite_expr(&mut b.left, lifted);
            rewrite_expr(&mut b.right, lifted);
        }
        TypedExpression::Unary(u) => rewrite_expr(&mut u.operand, lifted),
        TypedExpression::Array(items) | TypedExpression::Tuple(items) => {
            for it in items {
                rewrite_expr(it, lifted);
            }
        }
        TypedExpression::Lambda(lam) => match &mut lam.body {
            TypedLambdaBody::Expression(e) => rewrite_expr(e, lifted),
            TypedLambdaBody::Block(b) => rewrite_block(b, lifted),
        },
        _ => {}
    }

    let Some(mut body) = with_marker_body(expr) else {
        return;
    };

    let id = NEXT_REGION_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let name = format!("__blinc_with_{id}");

    let (signal_deps, fsms, named_deps) = strip_decorator_markers(&mut body);
    let mut ctx_value_reads = Vec::new();
    super::collect_ctx_value_reads(&body, &mut ctx_value_reads);

    lifted.push((
        WithRegion {
            id,
            name: name.clone(),
            signal_deps,
            fsms,
            named_deps,
            ctx_value_reads,
        },
        body,
    ));

    // `__blinc_with__(<id>, __component_call__("__blinc_with_<id>"))`.
    // The inner marker goes through the ordinary component-call
    // lowering, so it picks up the `$view` mangling and the instance-id
    // ABI without this pass knowing either. Argument order means the
    // region renders BEFORE the builtin runs, so the builtin adopts an
    // already-built widget instead of re-entering the JIT mid-render.
    let span = expr.span;
    let i64_ty = Type::Primitive(PrimitiveType::I64);
    let inner = typed_node(
        TypedExpression::Call(zyntax_typed_ast::TypedCall {
            callee: Box::new(typed_node(
                TypedExpression::Variable(zyntax_typed_ast::InternedString::new_global(
                    "__component_call__",
                )),
                Type::Any,
                span,
            )),
            positional_args: vec![typed_node(
                TypedExpression::Literal(zyntax_typed_ast::TypedLiteral::String(
                    zyntax_typed_ast::InternedString::new_global(&name),
                )),
                Type::Primitive(PrimitiveType::String),
                span,
            )],
            named_args: vec![],
            type_args: vec![],
        }),
        i64_ty.clone(),
        span,
    );
    expr.node = TypedExpression::Call(zyntax_typed_ast::TypedCall {
        callee: Box::new(typed_node(
            TypedExpression::Variable(zyntax_typed_ast::InternedString::new_global(
                "__blinc_with__",
            )),
            Type::Any,
            span,
        )),
        positional_args: vec![
            // The region's read scope opens HERE, in argument position,
            // because arguments evaluate left to right: the scope is
            // open before the view call beside it runs, so every read
            // the body makes lands in it. `__blinc_with__` closes it.
            typed_node(
                TypedExpression::Call(zyntax_typed_ast::TypedCall {
                    callee: Box::new(typed_node(
                        TypedExpression::Variable(zyntax_typed_ast::InternedString::new_global(
                            "__blinc_scope_enter__",
                        )),
                        Type::Any,
                        span,
                    )),
                    positional_args: vec![typed_node(
                        TypedExpression::Literal(zyntax_typed_ast::TypedLiteral::Integer(
                            id.into(),
                        )),
                        i64_ty.clone(),
                        span,
                    )],
                    named_args: vec![],
                    type_args: vec![],
                }),
                i64_ty.clone(),
                span,
            ),
            inner,
        ],
        named_args: vec![],
        type_args: vec![],
    });
    expr.ty = i64_ty;
}

/// Consume the leading `__stateful_view__` / `__fsm_view__` /
/// `__with_deps__` marker statements the grammar folded into the block,
/// returning their arguments as `(signals, fsms, unclassified)`.
/// Decorators may stack in either order.
fn strip_decorator_markers(body: &mut TypedBlock) -> (Vec<String>, Vec<String>, Vec<String>) {
    use zyntax_typed_ast::typed_ast::{TypedExpression, TypedLiteral};

    fn marker(stmt: &zyntax_typed_ast::TypedNode<TypedStatement>) -> Option<(String, Vec<String>)> {
        let TypedStatement::Expression(e) = &stmt.node else {
            return None;
        };
        let TypedExpression::Call(call) = &e.node else {
            return None;
        };
        let TypedExpression::Variable(callee) = &call.callee.node else {
            return None;
        };
        let name = callee.resolve_global()?.to_string();
        if name != "__stateful_view__" && name != "__fsm_view__" && name != "__with_deps__" {
            return None;
        }
        let args = call
            .positional_args
            .iter()
            .filter_map(|a| match &a.node {
                TypedExpression::Literal(TypedLiteral::String(s)) => {
                    s.resolve_global().map(|n| n.to_string())
                }
                TypedExpression::Variable(v) => v.resolve_global().map(|n| n.to_string()),
                _ => None,
            })
            .collect();
        Some((name, args))
    }

    let mut signal_deps = Vec::new();
    let mut fsms = Vec::new();
    let mut named_deps = Vec::new();
    while let Some((name, args)) = body.statements.first().and_then(marker) {
        match name.as_str() {
            "__stateful_view__" => signal_deps.extend(args),
            "__fsm_view__" => fsms.extend(args),
            _ => named_deps.extend(args),
        }
        body.statements.remove(0);
    }
    (signal_deps, fsms, named_deps)
}

/// The data half of the synthetic component. `validate_component_calls`
/// checks the site's name against declared classes, so the region needs
/// one even though it holds no fields.
fn synthetic_class(
    region: &WithRegion,
) -> zyntax_typed_ast::TypedNode<zyntax_typed_ast::typed_ast::TypedDeclaration> {
    use zyntax_typed_ast::typed_ast::{TypedClass, TypedDeclaration};

    let span = zyntax_typed_ast::Span::default();
    typed_node(
        TypedDeclaration::Class(TypedClass {
            name: zyntax_typed_ast::InternedString::new_global(&region.name),
            type_params: vec![],
            extends: None,
            implements: vec![],
            fields: vec![],
            methods: vec![],
            constructors: vec![],
            visibility: zyntax_typed_ast::Visibility::Public,
            is_abstract: false,
            is_final: false,
            annotations: vec![],
            span,
        }),
        Type::Primitive(PrimitiveType::Unit),
        span,
    )
}

/// The inherent `impl <region> { fn view() }` — the same shape
/// A region has to hand back a widget handle: the builtin takes it as
/// an argument, so a body that produces nothing leaves a Unit value in
/// an i64 argument slot, which trips Cranelift's value map rather than
/// failing anywhere legible.
///
/// A body that already ends in a widget call is left alone, so the
/// common `with @fsm([Play]) { Div { … } }` keeps exactly the boxes it
/// had. Anything else — a bare `if`, a loop, a body ending in a `.set()`
/// — is wrapped in a `Div`, which also gives its branches a child list
/// to push onto.
fn ensure_widget_returning(body: TypedBlock) -> TypedBlock {
    use zyntax_typed_ast::typed_ast::TypedExpression;

    let produces_widget = body.statements.last().is_some_and(|stmt| {
        let TypedStatement::Expression(e) = &stmt.node else {
            return false;
        };
        let TypedExpression::Call(call) = &e.node else {
            return false;
        };
        let TypedExpression::Variable(callee) = &call.callee.node else {
            return false;
        };
        callee
            .resolve_global()
            .is_some_and(|n| n == "__component_call__" || n.ends_with("$view"))
    });
    if produces_widget {
        return body;
    }

    let span = body.span;
    let inner = typed_node(
        TypedExpression::Block(body),
        Type::Primitive(PrimitiveType::Unit),
        span,
    );
    let wrapped = typed_node(
        TypedExpression::Call(zyntax_typed_ast::TypedCall {
            callee: Box::new(typed_node(
                TypedExpression::Variable(zyntax_typed_ast::InternedString::new_global(
                    "__component_call__",
                )),
                Type::Any,
                span,
            )),
            positional_args: vec![
                typed_node(
                    TypedExpression::Literal(zyntax_typed_ast::TypedLiteral::String(
                        zyntax_typed_ast::InternedString::new_global("Div"),
                    )),
                    Type::Primitive(PrimitiveType::String),
                    span,
                ),
                inner,
            ],
            named_args: vec![],
            type_args: vec![],
        }),
        Type::Primitive(PrimitiveType::I64),
        span,
    );
    TypedBlock {
        statements: vec![typed_node(
            TypedStatement::Expression(Box::new(wrapped)),
            Type::Primitive(PrimitiveType::Unit),
            span,
        )],
        span,
    }
}

/// `component_folded` emits, which is what gives the `<name>$view`
/// mangling `render_component` resolves against.
fn synthetic_impl(
    region: &WithRegion,
    body: TypedBlock,
) -> zyntax_typed_ast::TypedNode<zyntax_typed_ast::typed_ast::TypedDeclaration> {
    use zyntax_typed_ast::typed_ast::{TypedDeclaration, TypedMethod, TypedTraitImpl};

    let span = body.span;
    let body = ensure_widget_returning(body);
    typed_node(
        TypedDeclaration::Impl(TypedTraitImpl {
            // Empty trait name marks an INHERENT impl, which is what
            // gives the `<Type>$<method>` mangling. See the note on
            // `component_folded` in the grammar.
            trait_name: zyntax_typed_ast::InternedString::new_global(""),
            trait_type_args: vec![],
            for_type: Type::Unresolved(zyntax_typed_ast::InternedString::new_global(&region.name)),
            methods: vec![TypedMethod {
                name: zyntax_typed_ast::InternedString::new_global("view"),
                type_params: vec![],
                params: vec![],
                return_type: Type::Primitive(PrimitiveType::Unit),
                body: Some(body),
                visibility: zyntax_typed_ast::Visibility::Public,
                is_static: false,
                is_async: false,
                is_override: false,
                span,
            }],
            associated_types: vec![],
            span,
        }),
        Type::Primitive(PrimitiveType::Unit),
        span,
    )
}
