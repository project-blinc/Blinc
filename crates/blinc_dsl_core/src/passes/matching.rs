//! `match` block lowering.

use crate::*;

/// Desugar `match` marker-statement quads into `if/else if/.../else` chains
/// over string equality. Wildcard arm becomes the trailing `else`.
pub(crate) fn lower_match_blocks(program: &mut TypedProgram) {
    use zyntax_typed_ast::TypedNode;
    use zyntax_typed_ast::typed_ast::{
        BinaryOp, TypedBinary, TypedBlock, TypedDeclaration, TypedExpression, TypedIfExpr,
        TypedLiteral,
    };

    fn is_call_to(stmt: &TypedNode<TypedStatement>, name: &str) -> bool {
        let TypedStatement::Expression(expr) = &stmt.node else {
            return false;
        };
        let TypedExpression::Call(call) = &expr.node else {
            return false;
        };
        let TypedExpression::Variable(callee) = &call.callee.node else {
            return false;
        };
        callee.resolve_global().as_deref() == Some(name)
    }

    fn call_first_arg(stmt: &TypedNode<TypedStatement>) -> Option<&TypedNode<TypedExpression>> {
        let TypedStatement::Expression(expr) = &stmt.node else {
            return None;
        };
        let TypedExpression::Call(call) = &expr.node else {
            return None;
        };
        call.positional_args.first()
    }

    /// Lower every `__match_begin__ … __match_end__` span in `stmts`.
    /// MUST recurse into nested blocks first so inner matches lower before outers see them.
    fn rewrite_stmts(stmts: &mut Vec<TypedNode<TypedStatement>>) {
        for stmt in stmts.iter_mut() {
            recurse_into_stmt(stmt);
        }

        let mut i = 0;
        while i < stmts.len() {
            if !is_call_to(&stmts[i], "__match_begin__") {
                i += 1;
                continue;
            }
            let Some(scrutinee_expr) = call_first_arg(&stmts[i]).cloned() else {
                i += 1;
                continue;
            };

            let mut end_idx = i + 1;
            while end_idx < stmts.len() && !is_call_to(&stmts[end_idx], "__match_end__") {
                end_idx += 1;
            }
            if end_idx >= stmts.len() {
                // Malformed — no end marker.
                i += 1;
                continue;
            }

            // Each arm at i+1..end_idx is a Block whose first stmt is `__match_arm__(pat)`.
            // `pat` is one of:
            //   - `StringLiteral("__wildcard__")` — the `_` arm
            //   - `StringLiteral("literal")`      — a string pattern
            //   - `Call(__struct_pattern__, [StringLiteral(name), StringLiteral(f1), …])`
            //     — a struct destructure pattern, binds each field as a
            //     `let` at the start of the arm body. See
            //     `pattern_struct` in `grammar/blinc.zyn` for the
            //     producer-side rationale.
            enum ArmPattern {
                Literal(String),
                Wildcard,
                Struct {
                    #[allow(dead_code)]
                    name: String,
                    fields: Vec<String>,
                },
            }
            let mut arms: Vec<(Option<ArmPattern>, TypedBlock)> = Vec::new();
            for arm in stmts[(i + 1)..end_idx].iter() {
                let TypedStatement::Block(arm_block) = &arm.node else {
                    continue;
                };
                if arm_block.statements.is_empty() {
                    continue;
                }
                if !is_call_to(&arm_block.statements[0], "__match_arm__") {
                    continue;
                }
                let pat_expr = call_first_arg(&arm_block.statements[0]);
                let pat = pat_expr.and_then(|expr| {
                    // Literal string pattern (and the `__wildcard__` sentinel).
                    if let TypedExpression::Literal(TypedLiteral::String(s)) = &expr.node {
                        let s_arc = s.resolve_global()?;
                        let s_str: &str = &s_arc;
                        return Some(if s_str == "__wildcard__" {
                            ArmPattern::Wildcard
                        } else {
                            ArmPattern::Literal(s_str.to_string())
                        });
                    }
                    // Struct destructure: Call(__struct_pattern__, [name, field1, …]).
                    if let TypedExpression::Call(call) = &expr.node
                        && let TypedExpression::Variable(callee) = &call.callee.node
                        && callee.resolve_global().as_deref() == Some("__struct_pattern__")
                    {
                        let mut args = call.positional_args.iter();
                        let name = args.next().and_then(|a| {
                            if let TypedExpression::Literal(TypedLiteral::String(s)) = &a.node {
                                s.resolve_global().map(|s| s.to_string())
                            } else {
                                None
                            }
                        })?;
                        let fields = args
                            .filter_map(|a| {
                                if let TypedExpression::Literal(TypedLiteral::String(s)) = &a.node {
                                    s.resolve_global().map(|s| s.to_string())
                                } else {
                                    None
                                }
                            })
                            .collect::<Vec<_>>();
                        return Some(ArmPattern::Struct { name, fields });
                    }
                    None
                });
                let body = TypedBlock {
                    statements: arm_block.statements[1..].to_vec(),
                    span: arm_block.span,
                };
                arms.push((pat, body));
            }

            // Build the if/else-if/else chain. The first arm that
            // unconditionally matches (wildcard OR struct pattern in
            // this MVP) becomes the trailing `else`; subsequent
            // always-match arms are dropped. Struct arms prepend
            // `let <field> = <scrutinee>.<field>` bindings to their
            // body so the arm body sees the destructured locals.
            fn wrap_struct_body(
                body: TypedBlock,
                fields: &[String],
                scrutinee: &TypedNode<TypedExpression>,
            ) -> TypedBlock {
                let span = body.span;
                let mut bindings: Vec<TypedNode<TypedStatement>> = fields
                    .iter()
                    .map(|field| {
                        let field_access = TypedNode::new(
                            TypedExpression::Field(zyntax_typed_ast::typed_ast::TypedFieldAccess {
                                object: Box::new(scrutinee.clone()),
                                field: zyntax_typed_ast::InternedString::new_global(field),
                            }),
                            Type::Any,
                            span,
                        );
                        TypedNode::new(
                            TypedStatement::Let(zyntax_typed_ast::typed_ast::TypedLet {
                                name: zyntax_typed_ast::InternedString::new_global(field),
                                ty: Type::Any,
                                mutability: zyntax_typed_ast::Mutability::Immutable,
                                initializer: Some(Box::new(field_access)),
                                span,
                            }),
                            Type::Primitive(PrimitiveType::Unit),
                            span,
                        )
                    })
                    .collect();
                bindings.extend(body.statements);
                TypedBlock {
                    statements: bindings,
                    span,
                }
            }

            let mut else_block: Option<TypedBlock> = None;
            let mut chain_arms: Vec<(String, TypedBlock)> = Vec::new();
            for (pat, body) in arms {
                match pat {
                    Some(ArmPattern::Wildcard) if else_block.is_none() => {
                        else_block = Some(body);
                    }
                    Some(ArmPattern::Wildcard) => {}
                    Some(ArmPattern::Struct { fields, .. }) if else_block.is_none() => {
                        else_block = Some(wrap_struct_body(body, &fields, &scrutinee_expr));
                    }
                    Some(ArmPattern::Struct { .. }) => {
                        // Already have an else — subsequent always-match
                        // arms are unreachable in this MVP.
                    }
                    Some(ArmPattern::Literal(p)) => {
                        chain_arms.push((p, body));
                    }
                    None => {}
                }
            }

            // Fold from last to first into an *expression*-form if/else
            // chain (`TypedExpression::If` wrapping `TypedExpression::Block`
            // branches). The expression form is the only one Zyntax's SSA
            // creates fresh successor blocks for on demand — the statement
            // form (`TypedStatement::If`) relies on pre-built CFG
            // successors, which `translate_closure` doesn't construct.
            // Inside a closure body the statement form silently skips
            // both branches (then + else), so the match arms never fire.
            // The expression form works in both top-level and closure
            // contexts.
            let unit = || Type::Primitive(PrimitiveType::Unit);
            let block_expr = |b: TypedBlock| -> TypedNode<TypedExpression> {
                let s = b.span;
                TypedNode::new(TypedExpression::Block(b), unit(), s)
            };
            let mut tail_else_expr: TypedNode<TypedExpression> = match else_block {
                Some(b) => block_expr(b),
                None => block_expr(TypedBlock {
                    statements: vec![],
                    span: scrutinee_expr.span,
                }),
            };
            for (pat, body) in chain_arms.into_iter().rev() {
                let span = body.span;
                let pat_literal = TypedNode::new(
                    TypedExpression::Literal(TypedLiteral::String(
                        zyntax_typed_ast::InternedString::new_global(&pat),
                    )),
                    Type::Primitive(PrimitiveType::String),
                    span,
                );
                let condition = TypedNode::new(
                    TypedExpression::Binary(TypedBinary {
                        op: BinaryOp::Eq,
                        left: Box::new(scrutinee_expr.clone()),
                        right: Box::new(pat_literal),
                    }),
                    Type::Primitive(PrimitiveType::Bool),
                    span,
                );
                let then_expr = block_expr(body);
                let if_expr = TypedExpression::If(TypedIfExpr {
                    condition: Box::new(condition),
                    then_branch: Box::new(then_expr),
                    else_branch: Box::new(tail_else_expr),
                });
                tail_else_expr = TypedNode::new(if_expr, unit(), span);
            }

            // Splice the chain in place of the marker span. Wrap the
            // expression-form if-chain in a single `TypedStatement::Expression`.
            let chain_span = tail_else_expr.span;
            let chain_stmt = TypedNode::new(
                TypedStatement::Expression(Box::new(tail_else_expr)),
                unit(),
                chain_span,
            );
            stmts.splice(i..=end_idx, [chain_stmt]);
            i += 1;
        }
    }

    fn recurse_into_stmt(stmt: &mut TypedNode<TypedStatement>) {
        match &mut stmt.node {
            TypedStatement::Block(b) => {
                rewrite_stmts(&mut b.statements);
            }
            TypedStatement::If(if_stmt) => {
                rewrite_stmts(&mut if_stmt.then_block.statements);
                if let Some(else_block) = &mut if_stmt.else_block {
                    rewrite_stmts(&mut else_block.statements);
                }
            }
            TypedStatement::While(w) => {
                rewrite_stmts(&mut w.body.statements);
            }
            TypedStatement::Expression(expr) => {
                recurse_into_expr(expr);
            }
            TypedStatement::Let(l) => {
                if let Some(init) = &mut l.initializer {
                    recurse_into_expr(init);
                }
            }
            _ => {}
        }
    }

    fn recurse_into_expr(expr: &mut TypedNode<TypedExpression>) {
        // Lambda bodies need this: `<Fsm>.subscribe(..., || { match … })` must
        // lower before any downstream pass walks the lambda HIR.
        match &mut expr.node {
            TypedExpression::Lambda(lam) => match &mut lam.body {
                zyntax_typed_ast::typed_ast::TypedLambdaBody::Expression(e) => {
                    recurse_into_expr(e);
                }
                zyntax_typed_ast::typed_ast::TypedLambdaBody::Block(block) => {
                    rewrite_stmts(&mut block.statements);
                }
            },
            TypedExpression::Block(block) => {
                rewrite_stmts(&mut block.statements);
            }
            TypedExpression::Call(call) => {
                recurse_into_expr(&mut call.callee);
                for arg in &mut call.positional_args {
                    recurse_into_expr(arg);
                }
            }
            TypedExpression::Binary(b) => {
                recurse_into_expr(&mut b.left);
                recurse_into_expr(&mut b.right);
            }
            TypedExpression::If(if_expr) => {
                recurse_into_expr(&mut if_expr.condition);
                recurse_into_expr(&mut if_expr.then_branch);
                recurse_into_expr(&mut if_expr.else_branch);
            }
            _ => {}
        }
    }

    for decl in &mut program.declarations {
        match &mut decl.node {
            TypedDeclaration::Function(func) => {
                if let Some(body) = &mut func.body {
                    rewrite_stmts(&mut body.statements);
                }
            }
            TypedDeclaration::Impl(imp) => {
                for method in &mut imp.methods {
                    if let Some(body) = &mut method.body {
                        rewrite_stmts(&mut body.statements);
                    }
                }
            }
            _ => {}
        }
    }
}
