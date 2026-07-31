//! Const groups (Go-style iota) and const reference resolution.

use crate::*;

/// Rewrite `<sig>.get()` / `<sig>.set(v)` / `<sig> = v` into `__signal_<get|set>_<T>` calls.
/// One entry in the per-compile signal map: the declared type plus the
/// Walk every `__blinc_const_group__` decl, hoist each contained
/// member into its own `const` Variable declaration, substitute the
/// member's zero-based index in place of any `__iota__` placeholder
/// in the value expression, then strip the group decl. After this
/// pass runs, `resolve_const_references` sees a flat sequence of
/// individual const decls and treats every group member identically
/// to a standalone `const NAME: T = literal`.
///
/// Group-marker shape (set up by the `const_group` grammar rule):
///   `__blinc_const_group__` function with body =
///     `[Expression(Call(__blinc_const_group_member__,
///                       [StringLiteral(name), value_expr])), …]`
///
/// Iota encoding: `iota` in the grammar lowers to
/// `StringLiteral("__iota__")`. This pass swaps it for an
/// `IntLiteral(index)`. Mixed iota-and-explicit-value members in
/// the same group are supported — only the iota placeholders get
/// substituted; explicit literals pass through unchanged.
///
/// MUST run before [`resolve_const_references`] so the hoisted
/// the hoisted Variable declarations are visible when references are
/// resolved.
pub(crate) fn expand_const_groups(program: &mut TypedProgram) {
    use zyntax_typed_ast::TypedNode;
    use zyntax_typed_ast::typed_ast::{TypedDeclaration, TypedExpression, TypedLiteral};

    fn substitute_iota(expr: &mut TypedNode<TypedExpression>, index: i128) {
        if let TypedExpression::Literal(TypedLiteral::String(s)) = &expr.node
            && let Some(s_arc) = s.resolve_global()
        {
            let s_str: &str = &s_arc;
            if s_str == "__iota__" {
                expr.node = TypedExpression::Literal(TypedLiteral::Integer(index));
                expr.ty = Type::Primitive(PrimitiveType::I32);
                return;
            }
        }
        // Recurse for completeness — iota always sits at the top of
        // the member's value expression today, but future arithmetic
        // (`iota + 1`) would need the descent.
        match &mut expr.node {
            TypedExpression::Binary(b) => {
                substitute_iota(&mut b.left, index);
                substitute_iota(&mut b.right, index);
            }
            TypedExpression::Unary(u) => substitute_iota(&mut u.operand, index),
            TypedExpression::Call(c) => {
                substitute_iota(&mut c.callee, index);
                for a in &mut c.positional_args {
                    substitute_iota(a, index);
                }
            }
            _ => {}
        }
    }

    // Step 1: collect hoisted const decls from each group, recording
    // both the group index (for the diagnostic span hint below) and
    // the spliced const-marker decl. Drop the group markers from the
    // program in the same pass.
    let mut hoisted: Vec<TypedNode<TypedDeclaration>> = Vec::new();
    program.declarations.retain(|decl| {
        let TypedDeclaration::Function(func) = &decl.node else {
            return true;
        };
        if func.name.resolve_global().as_deref() != Some("__blinc_const_group__") {
            return true;
        }
        let Some(body) = &func.body else {
            return false;
        };
        for (index, stmt) in body.statements.iter().enumerate() {
            let TypedStatement::Expression(call_expr) = &stmt.node else {
                continue;
            };
            let TypedExpression::Call(call) = &call_expr.node else {
                continue;
            };
            let TypedExpression::Variable(callee) = &call.callee.node else {
                continue;
            };
            if callee.resolve_global().as_deref() != Some("__blinc_const_group_member__") {
                continue;
            }
            if call.positional_args.len() != 2 {
                continue;
            }
            let TypedExpression::Literal(TypedLiteral::String(name)) =
                &call.positional_args[0].node
            else {
                continue;
            };
            let Some(name_arc) = name.resolve_global() else {
                continue;
            };
            let mut value = call.positional_args[1].clone();
            substitute_iota(&mut value, index as i128);

            // A const is a Variable declaration, not a function. The
            // typed AST has `TypedDeclaration::Variable` for exactly
            // this, and it carries the name, the initializer and a
            // visibility that a future `export` can set -- none of which
            // a marker function models without the reader knowing the
            // encoding.
            let const_var = zyntax_typed_ast::typed_ast::TypedVariable {
                name: *name,
                ty: Type::Any,
                mutability: zyntax_typed_ast::Mutability::Immutable,
                initializer: Some(Box::new(value)),
                visibility: zyntax_typed_ast::type_registry::Visibility::Private,
            };
            let _ = name_arc;
            hoisted.push(TypedNode::new(
                TypedDeclaration::Variable(const_var),
                Type::Primitive(PrimitiveType::Unit),
                decl.span,
            ));
        }
        false
    });

    program.declarations.extend(hoisted);
}

/// Extract every `__blinc_const__` marker function, register the
/// declared constants into a `name → literal-expression` map, strip
/// the markers, then rewrite every `TypedExpression::Variable`
/// reference whose name matches a registered const to a clone of the
/// stored literal. This is how `const PI: f64 = 3.14159` followed by
/// a downstream `text(f"{PI}")` reads as if the literal had been
/// inlined at the call site.
///
/// MVP scope: const values are single literal tokens (int / float /
/// string / bool — see `const_literal` in the grammar). No arithmetic,
/// no references to other consts on the RHS. The declared type
/// annotation is informational only — the substituted expression
/// carries the literal's own type.
///
/// Must run before any pass that walks expressions for symbol
/// resolution (`resolve_signal_calls`, the FSM passes, etc.) so the
/// rewritten literals look identical to author-written ones.
pub(crate) fn resolve_const_references(program: &mut TypedProgram) {
    use std::collections::HashMap;
    use zyntax_typed_ast::TypedNode;
    use zyntax_typed_ast::typed_ast::{TypedDeclaration, TypedExpression, TypedLiteral};

    // Step 1: collect every scalar `const` Variable decl into a name→value
    // map, then strip those decls from the program. The marker
    // function's body is `[Expression(StringLiteral(name)),
    // Expression(<value-literal>)]` — see `const_decl` in
    // `grammar/blinc.zyn`.
    let mut consts: HashMap<String, TypedNode<TypedExpression>> = HashMap::new();
    program.declarations.retain(|decl| {
        let TypedDeclaration::Variable(var) = &decl.node else {
            return true;
        };
        let Some(init) = var.initializer.as_ref() else {
            return true;
        };
        // A list const is left in place: `expand_map_calls` consumes it,
        // and substituting an array into reference sites would put a
        // runtime `List<T>` where indexing faults.
        if matches!(init.node, TypedExpression::Array(_)) {
            return true;
        }
        let Some(name_str) = var.name.resolve_global() else {
            return true;
        };
        consts.insert(name_str.to_string(), (**init).clone());
        false
    });

    if consts.is_empty() {
        return;
    }

    // Step 2: rewrite every `Variable(name)` whose `name` is a
    // registered const to a clone of the stored literal. Recurses
    // through all expression / statement shapes that the existing
    // passes touch.
    fn rewrite_expr(
        expr: &mut TypedNode<TypedExpression>,
        consts: &HashMap<String, TypedNode<TypedExpression>>,
    ) {
        if let TypedExpression::Variable(name) = &expr.node
            && let Some(name_str) = name.resolve_global()
        {
            let key: &str = &name_str;
            if let Some(value) = consts.get(key) {
                *expr = value.clone();
                return;
            }
        }
        // Bare-name uppercase identifiers (`PI`, `ANSWER`, etc.) parse
        // as `Call(__component_call__, [StringLiteral("PI")])` via the
        // `component_call_bare` grammar alternative — capitalised names
        // are claimed by the component-call path before
        // `variable_expr`. Detect that shape too so consts named in
        // the conventional UPPERCASE style still substitute.
        if let TypedExpression::Call(call) = &expr.node
            && let TypedExpression::Variable(callee) = &call.callee.node
            && callee.resolve_global().as_deref() == Some("__component_call__")
            && call.positional_args.len() == 1
            && let TypedExpression::Literal(TypedLiteral::String(name)) =
                &call.positional_args[0].node
            && let Some(name_str) = name.resolve_global()
        {
            let key: &str = &name_str;
            if let Some(value) = consts.get(key) {
                *expr = value.clone();
                return;
            }
        }
        match &mut expr.node {
            TypedExpression::Binary(b) => {
                rewrite_expr(&mut b.left, consts);
                rewrite_expr(&mut b.right, consts);
            }
            TypedExpression::Unary(u) => {
                rewrite_expr(&mut u.operand, consts);
            }
            TypedExpression::Call(c) => {
                rewrite_expr(&mut c.callee, consts);
                for a in &mut c.positional_args {
                    rewrite_expr(a, consts);
                }
                for na in &mut c.named_args {
                    rewrite_expr(&mut na.value, consts);
                }
            }
            TypedExpression::MethodCall(mc) => {
                rewrite_expr(&mut mc.receiver, consts);
                for a in &mut mc.positional_args {
                    rewrite_expr(a, consts);
                }
            }
            TypedExpression::Field(f) => {
                rewrite_expr(&mut f.object, consts);
            }
            TypedExpression::Index(idx) => {
                rewrite_expr(&mut idx.object, consts);
                rewrite_expr(&mut idx.index, consts);
            }
            TypedExpression::Array(items) | TypedExpression::Tuple(items) => {
                for item in items {
                    rewrite_expr(item, consts);
                }
            }
            TypedExpression::If(if_expr) => {
                rewrite_expr(&mut if_expr.condition, consts);
                rewrite_expr(&mut if_expr.then_branch, consts);
                rewrite_expr(&mut if_expr.else_branch, consts);
            }
            TypedExpression::Block(block) => {
                rewrite_block(block, consts);
            }
            TypedExpression::Lambda(lam) => match &mut lam.body {
                zyntax_typed_ast::typed_ast::TypedLambdaBody::Expression(e) => {
                    rewrite_expr(e, consts);
                }
                zyntax_typed_ast::typed_ast::TypedLambdaBody::Block(block) => {
                    rewrite_block(block, consts);
                }
            },
            _ => {}
        }
    }

    fn rewrite_stmt(
        stmt: &mut TypedNode<TypedStatement>,
        consts: &HashMap<String, TypedNode<TypedExpression>>,
    ) {
        match &mut stmt.node {
            TypedStatement::Expression(e) => rewrite_expr(e, consts),
            TypedStatement::Return(Some(e)) => rewrite_expr(e, consts),
            TypedStatement::Let(l) => {
                if let Some(init) = &mut l.initializer {
                    rewrite_expr(init, consts);
                }
            }
            TypedStatement::If(if_stmt) => {
                rewrite_expr(&mut if_stmt.condition, consts);
                rewrite_block(&mut if_stmt.then_block, consts);
                if let Some(else_block) = &mut if_stmt.else_block {
                    rewrite_block(else_block, consts);
                }
            }
            TypedStatement::While(w) => {
                rewrite_expr(&mut w.condition, consts);
                rewrite_block(&mut w.body, consts);
            }
            TypedStatement::Block(b) => {
                rewrite_block(b, consts);
            }
            _ => {}
        }
    }

    fn rewrite_block(
        block: &mut zyntax_typed_ast::typed_ast::TypedBlock,
        consts: &HashMap<String, TypedNode<TypedExpression>>,
    ) {
        for stmt in &mut block.statements {
            rewrite_stmt(stmt, consts);
        }
    }

    for decl in &mut program.declarations {
        match &mut decl.node {
            TypedDeclaration::Function(func) => {
                if let Some(body) = &mut func.body {
                    rewrite_block(body, &consts);
                }
            }
            TypedDeclaration::Impl(imp) => {
                for method in &mut imp.methods {
                    if let Some(body) = &mut method.body {
                        rewrite_block(body, &consts);
                    }
                }
            }
            _ => {}
        }
    }
}
