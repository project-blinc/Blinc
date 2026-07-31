//! `items.map(|it| { … })` — one child per element.
//!
//! The declarative alternative to a loop in a widget body. The list is
//! declared outside the body (a `let` in the view function, which keeps
//! its bindings) and the map call sits in child position.
//!
//! Expanded at compile time: each element becomes its own child
//! expression with the lambda parameter substituted. That is not a
//! shortcut around a runtime iteration that ought to exist, it is
//! complete for what the language can currently express — there is no
//! module-scope list and no list-typed signal, so every list is a
//! literal whose elements are known here. A dynamic source would need a
//! runtime walk, and that is the point to add one.
//!
//! Runs before the children passes so the expansion is indistinguishable
//! from having written the children out by hand.

use crate::*;
use zyntax_typed_ast::{TypedDeclaration, TypedExpression, TypedNode, TypedStatement};

/// Element lists of `let <name> = [a, b, c]` bindings, by binding name.
type ArrayBindings = std::collections::HashMap<String, Vec<TypedNode<TypedExpression>>>;

pub(crate) fn expand_map_calls(program: &mut TypedProgram) {
    // Module-scope lists first: `const items = [...]` parses to a
    // `__blinc_const_list__` marker carrying its name and elements, and
    // is in scope for every body in the file.
    let module_lists = collect_module_lists(program);

    for decl in program.declarations.iter_mut() {
        match &mut decl.node {
            TypedDeclaration::Function(func) => {
                if let Some(body) = func.body.as_mut() {
                    expand_block(body, &mut module_lists.clone());
                }
            }
            TypedDeclaration::Impl(imp) => {
                for method in imp.methods.iter_mut() {
                    if let Some(body) = method.body.as_mut() {
                        expand_block(body, &mut module_lists.clone());
                    }
                }
            }
            _ => {}
        }
    }
}

/// Read `const <name> = [...]` markers into bindings, and drop the
/// marker declarations.
///
/// They are compile-time only: nothing downstream knows the shape, and
/// an array that reached the JIT would be a `List<T>` whose indexing
/// faults, so leaving one behind would trade a clean "unresolved name"
/// for a segfault.
fn collect_module_lists(program: &mut TypedProgram) -> ArrayBindings {
    use zyntax_typed_ast::TypedDeclaration;
    let mut out = ArrayBindings::new();
    program.declarations.retain(|decl| {
        let TypedDeclaration::Variable(var) = &decl.node else {
            return true;
        };
        let Some(init) = var.initializer.as_ref() else {
            return true;
        };
        let TypedExpression::Array(elements) = &init.node else {
            return true;
        };
        let Some(name) = var.name.resolve_global() else {
            return true;
        };
        out.insert(name.to_string(), elements.clone());
        false
    });
    out
}

/// Rewrite a statement list in place, splicing each map expansion where
/// the call stood so source order between static and mapped children is
/// kept.
///
/// `arrays` accumulates as the block is walked, so a `let` is visible to
/// the statements after it and not before, matching the binding's scope.
/// It is passed down into nested blocks because the list is declared in
/// the view body while the map call sits inside a widget body under it.
fn expand_block(block: &mut zyntax_typed_ast::typed_ast::TypedBlock, arrays: &mut ArrayBindings) {
    let mut out: Vec<TypedNode<TypedStatement>> = Vec::with_capacity(block.statements.len());
    for mut stmt in std::mem::take(&mut block.statements) {
        // Record `let xs = [...]` before it can be referenced.
        if let TypedStatement::Let(l) = &stmt.node
            && let Some(init) = l.initializer.as_ref()
            && let TypedExpression::Array(elements) = &init.node
            && let Some(name) = l.name.resolve_global()
        {
            arrays.insert(name.to_string(), elements.clone());
        }

        // A map call standing alone as a statement is the child-position
        // case; expand it into one statement per element.
        if let TypedStatement::Expression(expr) = &stmt.node
            && let Some(expanded) = try_expand(expr, arrays)
        {
            for child in expanded {
                let span = child.span;
                out.push(TypedNode::new(
                    TypedStatement::Expression(Box::new(child)),
                    Type::Primitive(PrimitiveType::Unit),
                    span,
                ));
            }
            continue;
        }

        descend_into_nested_blocks(&mut stmt, arrays);
        out.push(stmt);
    }
    block.statements = out;
}

/// Walk into the places a widget body can hide: a call's arguments carry
/// the `Block` that holds a widget's children.
fn descend_into_nested_blocks(stmt: &mut TypedNode<TypedStatement>, arrays: &mut ArrayBindings) {
    match &mut stmt.node {
        TypedStatement::Expression(e) | TypedStatement::Return(Some(e)) => descend_expr(e, arrays),
        TypedStatement::Let(l) => {
            if let Some(init) = l.initializer.as_mut() {
                descend_expr(init, arrays);
            }
        }
        _ => {}
    }
}

fn descend_expr(expr: &mut TypedNode<TypedExpression>, arrays: &mut ArrayBindings) {
    match &mut expr.node {
        TypedExpression::Call(call) => {
            for arg in call.positional_args.iter_mut() {
                descend_expr(arg, arrays);
            }
            for named in call.named_args.iter_mut() {
                descend_expr(&mut named.value, arrays);
            }
        }
        TypedExpression::Block(b) => expand_block(b, arrays),
        TypedExpression::Array(items) => {
            // Children reach this pass as the elements of a `children`
            // array, not as statements, so the expansion has to splice
            // here too: one element in, N out, in place.
            let mut out = Vec::with_capacity(items.len());
            for mut item in std::mem::take(items) {
                if let Some(expanded) = try_expand(&item, arrays) {
                    out.extend(expanded);
                    continue;
                }
                descend_expr(&mut item, arrays);
                out.push(item);
            }
            *items = out;
        }
        _ => {}
    }
}

/// `<array>.map(|p| body)` → one expression per element, with `p`
/// replaced by that element.
///
/// Returns `None` for anything that is not a map over a list this pass
/// can see, which leaves the call untouched rather than dropping it.
fn try_expand(
    expr: &TypedNode<TypedExpression>,
    arrays: &ArrayBindings,
) -> Option<Vec<TypedNode<TypedExpression>>> {
    let TypedExpression::Call(call) = &expr.node else {
        return None;
    };
    let TypedExpression::Field(f) = &call.callee.node else {
        return None;
    };
    let (object, field) = (&f.object, f.field);
    if field.resolve_global().as_deref() != Some("map") {
        return None;
    }

    // Receiver: a literal, or a name bound to one earlier in scope.
    let elements: Vec<TypedNode<TypedExpression>> = match &object.node {
        TypedExpression::Array(items) => items.clone(),
        TypedExpression::Variable(name) => {
            let key = name.resolve_global()?;
            arrays.get(&key as &str)?.clone()
        }
        _ => return None,
    };

    if call.positional_args.len() != 1 {
        return None;
    }
    let TypedExpression::Lambda(lambda) = &call.positional_args[0].node else {
        return None;
    };
    let param = lambda.params.first()?.name;

    let body_block = match &lambda.body {
        zyntax_typed_ast::typed_ast::TypedLambdaBody::Expression(inner) => match &inner.node {
            TypedExpression::Block(b) => b.clone(),
            _ => zyntax_typed_ast::typed_ast::TypedBlock {
                statements: vec![TypedNode::new(
                    TypedStatement::Expression(Box::new((**inner).clone())),
                    Type::Primitive(PrimitiveType::Unit),
                    inner.span,
                )],
                span: inner.span,
            },
        },
        zyntax_typed_ast::typed_ast::TypedLambdaBody::Block(b) => b.clone(),
    };

    Some(
        elements
            .into_iter()
            .map(|element| {
                let mut block = body_block.clone();
                for stmt in block.statements.iter_mut() {
                    substitute_in_stmt(stmt, param, &element);
                }
                // A single-expression body becomes that expression, not a
                // Block wrapping it. `collect_children_into` treats a bare
                // Block in child position as ambiguous -- a widget with
                // children is itself lowered to a trailing-expression
                // Block -- so the unwrapped form is what a hand-written
                // child looks like and is what survives downstream.
                if block.statements.len() == 1
                    && let TypedStatement::Expression(e) = &block.statements[0].node
                {
                    return (**e).clone();
                }
                let span = element.span;
                TypedNode::new(
                    TypedExpression::Block(block),
                    Type::Primitive(PrimitiveType::I64),
                    span,
                )
            })
            .collect(),
    )
}

fn substitute_in_stmt(
    stmt: &mut TypedNode<TypedStatement>,
    param: zyntax_typed_ast::InternedString,
    value: &TypedNode<TypedExpression>,
) {
    match &mut stmt.node {
        TypedStatement::Expression(e) | TypedStatement::Return(Some(e)) => {
            substitute_in_expr(e, param, value)
        }
        TypedStatement::Let(l) => {
            if let Some(init) = l.initializer.as_mut() {
                substitute_in_expr(init, param, value);
            }
        }
        _ => {}
    }
}

fn substitute_in_expr(
    expr: &mut TypedNode<TypedExpression>,
    param: zyntax_typed_ast::InternedString,
    value: &TypedNode<TypedExpression>,
) {
    match &mut expr.node {
        TypedExpression::Variable(name) => {
            if name.resolve_global() == param.resolve_global() {
                expr.node = value.node.clone();
            }
        }
        TypedExpression::Call(call) => {
            substitute_in_expr(&mut call.callee, param, value);
            for arg in call.positional_args.iter_mut() {
                substitute_in_expr(arg, param, value);
            }
            for named in call.named_args.iter_mut() {
                substitute_in_expr(&mut named.value, param, value);
            }
        }
        TypedExpression::Block(b) => {
            for stmt in b.statements.iter_mut() {
                substitute_in_stmt(stmt, param, value);
            }
        }
        TypedExpression::Array(items) => {
            for item in items.iter_mut() {
                substitute_in_expr(item, param, value);
            }
        }
        TypedExpression::Binary(b) => {
            substitute_in_expr(&mut b.left, param, value);
            substitute_in_expr(&mut b.right, param, value);
        }
        TypedExpression::Field(f) => substitute_in_expr(&mut f.object, param, value),
        _ => {}
    }
}
