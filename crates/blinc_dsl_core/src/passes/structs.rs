//! Struct literal lowering and struct-typed widget props to handles.

use crate::*;

/// Lower explicit `struct` constructor calls (`MyData(field = value)`) into
/// native Zyntax struct literals. Component and widget calls keep flowing
/// through the normal `__component_call__` path.
pub(crate) fn lower_struct_literals(program: &mut TypedProgram) -> Result<(), Vec<String>> {
    use std::collections::{HashMap, HashSet};
    use zyntax_typed_ast::typed_ast::{
        TypedDeclaration, TypedExpression, TypedFieldInit, TypedLiteral, TypedStructLiteral,
    };

    #[derive(Clone)]
    struct StructInfo {
        name: zyntax_typed_ast::InternedString,
        fields: Vec<zyntax_typed_ast::typed_ast::TypedField>,
    }

    fn marker_struct_name(decl: &zyntax_typed_ast::TypedNode<TypedDeclaration>) -> Option<String> {
        let TypedDeclaration::Function(func) = &decl.node else {
            return None;
        };
        if func.name.resolve_global().as_deref() != Some("__blinc_struct_type__") {
            return None;
        }
        let body = func.body.as_ref()?;
        let first = body.statements.first()?;
        let TypedStatement::Expression(expr) = &first.node else {
            return None;
        };
        let TypedExpression::Literal(TypedLiteral::String(name)) = &expr.node else {
            return None;
        };
        name.resolve_global().map(|s| s.to_string())
    }

    let explicit_structs: HashSet<String> = program
        .declarations
        .iter()
        .filter_map(marker_struct_name)
        .collect();

    if explicit_structs.is_empty() {
        return Ok(());
    }

    let mut structs: HashMap<String, StructInfo> = HashMap::new();
    for decl in &program.declarations {
        if let TypedDeclaration::Class(class) = &decl.node {
            let Some(name) = class.name.resolve_global() else {
                continue;
            };
            if explicit_structs.contains::<str>(name.as_ref()) {
                structs.insert(
                    name.to_string(),
                    StructInfo {
                        name: class.name,
                        fields: class.fields.clone(),
                    },
                );
            }
        }
    }

    program
        .declarations
        .retain(|decl| marker_struct_name(decl).is_none());

    let mut errors = Vec::new();

    fn named_marker_arg(
        arg: &zyntax_typed_ast::TypedNode<TypedExpression>,
    ) -> Option<(
        zyntax_typed_ast::InternedString,
        zyntax_typed_ast::TypedNode<TypedExpression>,
    )> {
        let TypedExpression::Call(inner) = &arg.node else {
            return None;
        };
        let TypedExpression::Variable(inner_callee) = &inner.callee.node else {
            return None;
        };
        if inner_callee.resolve_global().as_deref() != Some("__named__") {
            return None;
        }
        let [name_node, value_node] = inner.positional_args.as_slice() else {
            return None;
        };
        let TypedExpression::Literal(TypedLiteral::String(arg_name)) = &name_node.node else {
            return None;
        };
        Some((*arg_name, value_node.clone()))
    }

    fn rewrite_expr(
        expr: &mut zyntax_typed_ast::TypedNode<TypedExpression>,
        structs: &HashMap<String, StructInfo>,
        errors: &mut Vec<String>,
    ) {
        match &mut expr.node {
            TypedExpression::Binary(b) => {
                rewrite_expr(&mut b.left, structs, errors);
                rewrite_expr(&mut b.right, structs, errors);
            }
            TypedExpression::Unary(u) => rewrite_expr(&mut u.operand, structs, errors),
            TypedExpression::Call(c) => {
                rewrite_expr(&mut c.callee, structs, errors);
                for a in &mut c.positional_args {
                    rewrite_expr(a, structs, errors);
                }
                for n in &mut c.named_args {
                    rewrite_expr(&mut n.value, structs, errors);
                }
            }
            TypedExpression::Field(f) => rewrite_expr(&mut f.object, structs, errors),
            TypedExpression::Index(idx) => {
                rewrite_expr(&mut idx.object, structs, errors);
                rewrite_expr(&mut idx.index, structs, errors);
            }
            TypedExpression::Array(items) | TypedExpression::Tuple(items) => {
                for it in items {
                    rewrite_expr(it, structs, errors);
                }
            }
            TypedExpression::Struct(s) => {
                for field in &mut s.fields {
                    rewrite_expr(&mut field.value, structs, errors);
                }
            }
            TypedExpression::MethodCall(mc) => {
                rewrite_expr(&mut mc.receiver, structs, errors);
                for a in &mut mc.positional_args {
                    rewrite_expr(a, structs, errors);
                }
            }
            TypedExpression::Block(b) => rewrite_block(b, structs, errors),
            TypedExpression::If(if_expr) => {
                rewrite_expr(&mut if_expr.condition, structs, errors);
                rewrite_expr(&mut if_expr.then_branch, structs, errors);
                rewrite_expr(&mut if_expr.else_branch, structs, errors);
            }
            _ => {}
        }

        let TypedExpression::Call(call) = &expr.node else {
            return;
        };
        let TypedExpression::Variable(callee_name) = &call.callee.node else {
            return;
        };
        if callee_name.resolve_global().as_deref() != Some("__component_call__") {
            return;
        }
        let Some(name_node) = call.positional_args.first() else {
            return;
        };
        let TypedExpression::Literal(TypedLiteral::String(type_name)) = &name_node.node else {
            return;
        };
        let Some(type_name_str) = type_name.resolve_global().map(|s| s.to_string()) else {
            return;
        };
        let Some(info) = structs.get(&type_name_str) else {
            return;
        };

        let mut values: HashMap<String, zyntax_typed_ast::TypedNode<TypedExpression>> =
            HashMap::new();
        let mut seen = HashSet::new();

        for arg in call.positional_args.iter().skip(1) {
            if matches!(arg.node, TypedExpression::Block(_)) {
                errors.push(format!(
                    "struct `{}` constructors do not accept child bodies; use `{}(field = value)`",
                    type_name_str, type_name_str
                ));
                continue;
            }

            let Some((field_name, value)) = named_marker_arg(arg) else {
                errors.push(format!(
                    "struct `{}` constructors require named fields, e.g. `{}(field = value)`",
                    type_name_str, type_name_str
                ));
                continue;
            };
            let Some(field_name_str) = field_name.resolve_global().map(|s| s.to_string()) else {
                continue;
            };
            if !seen.insert(field_name_str.clone()) {
                errors.push(format!(
                    "struct `{}` field `{}` is specified more than once",
                    type_name_str, field_name_str
                ));
                continue;
            }
            values.insert(field_name_str, value);
        }

        let declared: HashSet<String> = info
            .fields
            .iter()
            .filter_map(|f| f.name.resolve_global().map(|s| s.to_string()))
            .collect();
        for supplied in values.keys() {
            if !declared.contains(supplied) {
                errors.push(format!(
                    "struct `{}` has no field named `{}`",
                    type_name_str, supplied
                ));
            }
        }

        let mut lowered_fields = Vec::with_capacity(info.fields.len());
        for field in &info.fields {
            let Some(field_name) = field.name.resolve_global().map(|s| s.to_string()) else {
                continue;
            };
            let Some(value) = values.remove(&field_name) else {
                errors.push(format!(
                    "struct `{}` constructor is missing field `{}`",
                    type_name_str, field_name
                ));
                continue;
            };
            lowered_fields.push(TypedFieldInit {
                name: field.name,
                value: Box::new(value),
            });
        }

        if errors.is_empty() {
            expr.ty = Type::Unresolved(info.name);
            expr.node = TypedExpression::Struct(TypedStructLiteral {
                name: info.name,
                fields: lowered_fields,
            });
        }
    }

    fn rewrite_block(
        block: &mut zyntax_typed_ast::typed_ast::TypedBlock,
        structs: &HashMap<String, StructInfo>,
        errors: &mut Vec<String>,
    ) {
        for stmt in &mut block.statements {
            rewrite_stmt(stmt, structs, errors);
        }
    }

    fn rewrite_stmt(
        stmt: &mut zyntax_typed_ast::TypedNode<TypedStatement>,
        structs: &HashMap<String, StructInfo>,
        errors: &mut Vec<String>,
    ) {
        match &mut stmt.node {
            TypedStatement::Expression(e) => rewrite_expr(e, structs, errors),
            TypedStatement::Let(l) => {
                if let Some(init) = &mut l.initializer {
                    rewrite_expr(init, structs, errors);
                }
            }
            TypedStatement::Return(Some(e)) => rewrite_expr(e, structs, errors),
            TypedStatement::If(if_stmt) => {
                rewrite_expr(&mut if_stmt.condition, structs, errors);
                rewrite_block(&mut if_stmt.then_block, structs, errors);
                if let Some(else_block) = &mut if_stmt.else_block {
                    rewrite_block(else_block, structs, errors);
                }
            }
            TypedStatement::While(w) => {
                rewrite_expr(&mut w.condition, structs, errors);
                rewrite_block(&mut w.body, structs, errors);
            }
            TypedStatement::Block(b) => rewrite_block(b, structs, errors),
            _ => {}
        }
    }

    for decl in &mut program.declarations {
        match &mut decl.node {
            TypedDeclaration::Function(func) => {
                if let Some(body) = &mut func.body {
                    rewrite_block(body, &structs, &mut errors);
                }
            }
            TypedDeclaration::Impl(imp) => {
                for method in &mut imp.methods {
                    if let Some(body) = &mut method.body {
                        rewrite_block(body, &structs, &mut errors);
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

/// Marshal complex extern-widget prop struct literals into opaque `i64` handles.
/// The widget registry keeps the DSL-facing named type; the generated extern
/// thunk accepts a stable handle ABI.
pub(crate) fn lower_struct_widget_props_to_handles(
    program: &mut TypedProgram,
) -> Result<(), Vec<String>> {
    use std::collections::HashMap;
    use zyntax_typed_ast::{TypedCall, TypedDeclaration, TypedExpression};

    type FieldMap = HashMap<String, Type>;
    type StructFields = HashMap<String, FieldMap>;

    fn is_complex_type(ty: &Type) -> bool {
        matches!(
            ty,
            Type::Unresolved(_) | Type::Named { .. } | Type::Struct { .. }
        )
    }

    fn collect_struct_fields(program: &TypedProgram) -> StructFields {
        let mut out = HashMap::new();
        for decl in &program.declarations {
            let TypedDeclaration::Class(class) = &decl.node else {
                continue;
            };
            let Some(class_name) = class.name.resolve_global().map(|s| s.to_string()) else {
                continue;
            };
            let mut fields = HashMap::new();
            for field in &class.fields {
                if let Some(name) = field.name.resolve_global() {
                    fields.insert(name.to_string(), field.ty.clone());
                }
            }
            out.insert(class_name, fields);
        }
        out
    }

    fn next_id(counter: &mut u32) -> u32 {
        let id = *counter;
        *counter += 1;
        id
    }

    fn setter_for_type(ty: &Type) -> &'static str {
        match ty {
            Type::Primitive(PrimitiveType::Bool) => "__set_struct_bool__",
            Type::Primitive(PrimitiveType::I32) => "__set_struct_i32__",
            Type::Primitive(PrimitiveType::F64) => "__set_struct_f64__",
            Type::Primitive(PrimitiveType::String) => "__set_struct_string__",
            Type::Unresolved(_) | Type::Named { .. } | Type::Struct { .. } => {
                "__set_struct_handle__"
            }
            _ => "__set_struct_i64__",
        }
    }

    fn string_expr(
        value: &str,
        span: zyntax_typed_ast::Span,
    ) -> zyntax_typed_ast::TypedNode<TypedExpression> {
        typed_node(
            TypedExpression::Literal(zyntax_typed_ast::TypedLiteral::String(
                zyntax_typed_ast::InternedString::new_global(value),
            )),
            Type::Primitive(PrimitiveType::String),
            span,
        )
    }

    fn bool_literal_as_i32(
        expr: zyntax_typed_ast::TypedNode<TypedExpression>,
    ) -> zyntax_typed_ast::TypedNode<TypedExpression> {
        let span = expr.span;
        let expr_ty = expr.ty.clone();
        match expr.node {
            TypedExpression::Literal(zyntax_typed_ast::TypedLiteral::Bool(value)) => typed_node(
                TypedExpression::Literal(zyntax_typed_ast::TypedLiteral::Integer(if value {
                    1
                } else {
                    0
                })),
                Type::Primitive(PrimitiveType::I32),
                span,
            ),
            other if matches!(&expr_ty, Type::Primitive(PrimitiveType::Bool)) => {
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
            other => typed_node(other, expr_ty, span),
        }
    }

    fn call_expr(
        callee: &str,
        args: Vec<zyntax_typed_ast::TypedNode<TypedExpression>>,
        ty: Type,
        span: zyntax_typed_ast::Span,
    ) -> zyntax_typed_ast::TypedNode<TypedExpression> {
        typed_node(
            TypedExpression::Call(TypedCall {
                callee: Box::new(typed_node(
                    TypedExpression::Variable(zyntax_typed_ast::InternedString::new_global(callee)),
                    Type::Any,
                    span,
                )),
                positional_args: args,
                named_args: vec![],
                type_args: vec![],
            }),
            ty,
            span,
        )
    }

    fn lower_struct_to_handle(
        expr: zyntax_typed_ast::TypedNode<TypedExpression>,
        structs: &StructFields,
        prelude: &mut Vec<zyntax_typed_ast::TypedNode<TypedStatement>>,
        counter: &mut u32,
    ) -> zyntax_typed_ast::TypedNode<TypedExpression> {
        let span = expr.span;
        let i64_ty = Type::Primitive(PrimitiveType::I64);
        let unit_ty = Type::Primitive(PrimitiveType::Unit);
        let TypedExpression::Struct(struct_lit) = expr.node else {
            return expr;
        };

        let struct_name = struct_lit
            .name
            .resolve_global()
            .map(|s| s.to_string())
            .unwrap_or_default();
        let id = next_id(counter);
        let ident =
            zyntax_typed_ast::InternedString::new_global(&format!("__blinc_struct_value_{id}"));

        prelude.push(typed_node(
            TypedStatement::Let(zyntax_typed_ast::typed_ast::TypedLet {
                name: ident,
                ty: i64_ty.clone(),
                mutability: zyntax_typed_ast::Mutability::Immutable,
                initializer: Some(Box::new(call_expr(
                    "__new_struct_value__",
                    vec![],
                    i64_ty.clone(),
                    span,
                ))),
                span,
            }),
            unit_ty.clone(),
            span,
        ));

        let field_types = structs.get(&struct_name);
        for field in struct_lit.fields {
            let field_name = field
                .name
                .resolve_global()
                .map(|s| s.to_string())
                .unwrap_or_default();
            let field_ty = field_types
                .and_then(|fields| fields.get(&field_name))
                .cloned()
                .unwrap_or_else(|| field.value.ty.clone());
            let setter = setter_for_type(&field_ty);
            let mut value = *field.value;
            if is_complex_type(&field_ty) && matches!(value.node, TypedExpression::Struct(_)) {
                value = lower_struct_to_handle(value, structs, prelude, counter);
            }
            if matches!(field_ty, Type::Primitive(PrimitiveType::Bool)) {
                value = bool_literal_as_i32(value);
            } else {
                value.ty = field_ty;
            }

            prelude.push(typed_node(
                TypedStatement::Expression(Box::new(call_expr(
                    setter,
                    vec![
                        typed_node(TypedExpression::Variable(ident), i64_ty.clone(), span),
                        string_expr(&field_name, span),
                        value,
                    ],
                    unit_ty.clone(),
                    span,
                ))),
                unit_ty.clone(),
                span,
            ));
        }

        typed_node(TypedExpression::Variable(ident), i64_ty, span)
    }

    fn lower_arg_if_needed(
        arg: &mut zyntax_typed_ast::TypedNode<TypedExpression>,
        prop_ty: &Type,
        structs: &StructFields,
        prelude: &mut Vec<zyntax_typed_ast::TypedNode<TypedStatement>>,
        counter: &mut u32,
    ) {
        if !is_complex_type(prop_ty) || !matches!(arg.node, TypedExpression::Struct(_)) {
            return;
        }
        let span = arg.span;
        let old = std::mem::replace(
            arg,
            typed_node(
                TypedExpression::Literal(zyntax_typed_ast::TypedLiteral::Integer(0)),
                Type::Primitive(PrimitiveType::I64),
                span,
            ),
        );
        *arg = lower_struct_to_handle(old, structs, prelude, counter);
    }

    fn extern_props(call: &TypedCall) -> Option<Vec<(String, Type)>> {
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

    fn walk_stmt(
        stmt: &mut zyntax_typed_ast::TypedNode<TypedStatement>,
        structs: &StructFields,
        counter: &mut u32,
    ) {
        match &mut stmt.node {
            TypedStatement::Expression(e) => rewrite_expr(e, structs, counter),
            TypedStatement::Return(Some(e)) => rewrite_expr(e, structs, counter),
            TypedStatement::Let(l) => {
                if let Some(init) = &mut l.initializer {
                    rewrite_expr(init, structs, counter);
                }
            }
            TypedStatement::If(if_stmt) => {
                rewrite_expr(&mut if_stmt.condition, structs, counter);
                for s in &mut if_stmt.then_block.statements {
                    walk_stmt(s, structs, counter);
                }
                if let Some(else_block) = &mut if_stmt.else_block {
                    for s in &mut else_block.statements {
                        walk_stmt(s, structs, counter);
                    }
                }
            }
            TypedStatement::While(w) => {
                rewrite_expr(&mut w.condition, structs, counter);
                for s in &mut w.body.statements {
                    walk_stmt(s, structs, counter);
                }
            }
            TypedStatement::Block(block) => {
                for s in &mut block.statements {
                    walk_stmt(s, structs, counter);
                }
            }
            _ => {}
        }
    }

    fn rewrite_expr(
        expr: &mut zyntax_typed_ast::TypedNode<TypedExpression>,
        structs: &StructFields,
        counter: &mut u32,
    ) {
        match &mut expr.node {
            TypedExpression::Call(call) => {
                rewrite_expr(&mut call.callee, structs, counter);
                for arg in &mut call.positional_args {
                    rewrite_expr(arg, structs, counter);
                }
                for na in &mut call.named_args {
                    rewrite_expr(&mut na.value, structs, counter);
                }
            }
            TypedExpression::Array(items) | TypedExpression::Tuple(items) => {
                for item in items {
                    rewrite_expr(item, structs, counter);
                }
            }
            TypedExpression::Struct(s) => {
                for field in &mut s.fields {
                    rewrite_expr(&mut field.value, structs, counter);
                }
            }
            TypedExpression::Block(block) => {
                for stmt in &mut block.statements {
                    walk_stmt(stmt, structs, counter);
                }
            }
            TypedExpression::Binary(b) => {
                rewrite_expr(&mut b.left, structs, counter);
                rewrite_expr(&mut b.right, structs, counter);
            }
            TypedExpression::Unary(u) => rewrite_expr(&mut u.operand, structs, counter),
            TypedExpression::Field(f) => rewrite_expr(&mut f.object, structs, counter),
            TypedExpression::Index(idx) => {
                rewrite_expr(&mut idx.object, structs, counter);
                rewrite_expr(&mut idx.index, structs, counter);
            }
            TypedExpression::MethodCall(mc) => {
                rewrite_expr(&mut mc.receiver, structs, counter);
                for arg in &mut mc.positional_args {
                    rewrite_expr(arg, structs, counter);
                }
            }
            TypedExpression::If(if_expr) => {
                rewrite_expr(&mut if_expr.condition, structs, counter);
                rewrite_expr(&mut if_expr.then_branch, structs, counter);
                rewrite_expr(&mut if_expr.else_branch, structs, counter);
            }
            _ => {}
        }

        let span = expr.span;
        let Some(props) = (match &expr.node {
            TypedExpression::Call(call) => extern_props(call),
            _ => None,
        }) else {
            return;
        };
        let TypedExpression::Call(call) = &mut expr.node else {
            return;
        };
        let mut prelude = Vec::new();

        for (i, arg) in call.positional_args.iter_mut().enumerate() {
            if let Some((_, prop_ty)) = props.get(i) {
                lower_arg_if_needed(arg, prop_ty, structs, &mut prelude, counter);
            }
        }
        for named in &mut call.named_args {
            let Some(name) = named.name.resolve_global() else {
                continue;
            };
            let Some((_, prop_ty)) = props.iter().find(|(prop_name, _)| prop_name == &*name) else {
                continue;
            };
            lower_arg_if_needed(&mut named.value, prop_ty, structs, &mut prelude, counter);
        }

        if prelude.is_empty() {
            return;
        }

        let final_call = TypedExpression::Call(TypedCall {
            callee: call.callee.clone(),
            positional_args: std::mem::take(&mut call.positional_args),
            named_args: std::mem::take(&mut call.named_args),
            type_args: std::mem::take(&mut call.type_args),
        });
        prelude.push(typed_node(
            TypedStatement::Expression(Box::new(typed_node(
                final_call,
                Type::Primitive(PrimitiveType::I64),
                span,
            ))),
            Type::Primitive(PrimitiveType::I64),
            span,
        ));
        expr.node = TypedExpression::Block(zyntax_typed_ast::typed_ast::TypedBlock {
            statements: prelude,
            span,
        });
    }

    let structs = collect_struct_fields(program);
    let mut counter = 0;
    for decl in program.declarations.iter_mut() {
        match &mut decl.node {
            TypedDeclaration::Function(func) => {
                if let Some(body) = func.body.as_mut() {
                    for stmt in &mut body.statements {
                        walk_stmt(stmt, &structs, &mut counter);
                    }
                }
            }
            TypedDeclaration::Impl(imp) => {
                for method in &mut imp.methods {
                    if let Some(body) = method.body.as_mut() {
                        for stmt in &mut body.statements {
                            walk_stmt(stmt, &structs, &mut counter);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    Ok(())
}
