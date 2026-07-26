//! `Reactive<T>` argument lowering for signal/computed-bound props.

use crate::*;

// =====================================================================
// Reactive-prop FFI expansion
// =====================================================================
//
// Companion pass to `resolve_extern_widget_named_args`. Walks every
// `$Blinc$<X>$view(...)` call site whose registered widget declares
// one or more `#[reactive] Reactive<T>` props (PropDef carries
// `reactive_inner: Some(T)`). For each such prop's positional arg
// slot, EXPANDS the single arg into two FFI slots — `tag: i32`,
// `payload: i64` — per the user-written value shape:
//
//   * Literal expression       → `(REACTIVE_TAG_LITERAL, encoded_bits)`
//   * Bare-Variable signal ref → `(REACTIVE_TAG_SIGNAL,  signal_id)`
//   * `computed { … } : T` call → `(REACTIVE_TAG_COMPUTED, derived_id)`
//
// Unrecognised arg shapes fall back to LITERAL with the original
// expression as the payload — preserves existing behaviour for
// arbitrary expressions, at the cost of an `f64→i64`-bitcast
// mismatch for runtime-computed floats. The doc on
// `blinc_runtime::reactive_value` describes the workaround:
// wrap arbitrary exprs in `computed { … } : T`.
//
// Runs AFTER `resolve_extern_widget_named_args` so we see fully-
// positionalised args; runs BEFORE Cranelift compile so the new
// arg list matches the macro-generated thunk's two-slot signature.
pub(crate) fn lower_reactive_args(program: &mut TypedProgram) {
    use zyntax_typed_ast::TypedNode;
    use zyntax_typed_ast::typed_ast::{TypedCall, TypedDeclaration, TypedExpression, TypedLiteral};

    /// Type tag constants — kept in sync with
    /// `blinc_runtime::reactive_value::REACTIVE_TAG_*`. We don't
    /// `pub use` those here because this pass should stay
    /// dependency-light; they're three ints, easy to keep aligned.
    const TAG_LITERAL: i128 = 0;
    const TAG_SIGNAL: i128 = 1;
    const TAG_COMPUTED: i128 = 2;

    /// Match the rightmost callee against the registry to recover the
    /// PropDef list. Returns `None` for non-substrate calls so the
    /// pass leaves user-component / closure-target / etc. calls
    /// alone.
    fn registry_props(call: &TypedCall) -> Option<Vec<blinc_runtime::component::PropDef>> {
        let TypedExpression::Variable(callee) = &call.callee.node else {
            return None;
        };
        let sym = callee.resolve_global()?;
        let sym: &str = &sym;
        let name = sym
            .strip_prefix("$Blinc$")
            .and_then(|s| s.strip_suffix("$view"))?;
        blinc_runtime::component::with_component_registry(|r| {
            r.get_by_name(name).map(|def| def.props.clone())
        })
    }

    /// Encode a literal value (per the prop's inner T) into the
    /// payload bit-pattern the runtime decoder reads back. f64 goes
    /// through `to_bits`; bool / i32 cast directly; non-matching
    /// shapes fall back to `0` and let runtime decode produce the
    /// inner type's default.
    fn encode_literal_payload(inner_ty: &Type, value: &TypedNode<TypedExpression>) -> i128 {
        match (inner_ty, &value.node) {
            (
                Type::Primitive(PrimitiveType::I32),
                TypedExpression::Literal(TypedLiteral::Integer(n)),
            ) => *n,
            (
                Type::Primitive(PrimitiveType::Bool),
                TypedExpression::Literal(TypedLiteral::Bool(b)),
            ) if *b => 1,
            (
                Type::Primitive(PrimitiveType::Bool),
                TypedExpression::Literal(TypedLiteral::Bool(_)),
            ) => 0,
            (
                Type::Primitive(PrimitiveType::Bool),
                TypedExpression::Literal(TypedLiteral::Integer(n)),
            ) if *n != 0 => 1,
            (
                Type::Primitive(PrimitiveType::Bool),
                TypedExpression::Literal(TypedLiteral::Integer(_)),
            ) => 0,
            (
                Type::Primitive(PrimitiveType::F64),
                TypedExpression::Literal(TypedLiteral::Float(f)),
            ) => f.to_bits() as i128,
            (
                Type::Primitive(PrimitiveType::F64),
                TypedExpression::Literal(TypedLiteral::Integer(n)),
            ) => (*n as f64).to_bits() as i128,
            // Non-literal / type-mismatched shapes encode as 0 here;
            // the existing `default_literal_for` path on the
            // resolve-named-args pass already left a `0` literal in
            // the slot for unsupplied props.
            _ => 0,
        }
    }

    /// Build a `Literal::Integer(n)` arg node carrying an `i64`-typed
    /// constant — used for both the tag slot and the encoded
    /// payload slot when emitting the expanded two-arg pair.
    fn i64_literal(value: i128, span: zyntax_typed_ast::Span) -> TypedNode<TypedExpression> {
        zyntax_typed_ast::TypedNode::new(
            TypedExpression::Literal(TypedLiteral::Integer(value)),
            Type::Primitive(PrimitiveType::I64),
            span,
        )
    }

    fn i32_literal(value: i128, span: zyntax_typed_ast::Span) -> TypedNode<TypedExpression> {
        zyntax_typed_ast::TypedNode::new(
            TypedExpression::Literal(TypedLiteral::Integer(value)),
            Type::Primitive(PrimitiveType::I32),
            span,
        )
    }

    /// Recognise `__blinc_computed_<T>__(closure_expr)` — the call
    /// shape `computed { … } : T` lowers to (per `grammar/blinc.zyn`).
    /// The call evaluates at runtime to a `DerivedId.to_raw() as i64`,
    /// which is exactly the payload we want under
    /// `REACTIVE_TAG_COMPUTED`.
    fn is_computed_call(expr: &TypedNode<TypedExpression>) -> bool {
        let TypedExpression::Call(c) = &expr.node else {
            return false;
        };
        let TypedExpression::Variable(callee) = &c.callee.node else {
            return false;
        };
        matches!(
            callee.resolve_global().as_deref(),
            Some("__blinc_computed_i32__")
                | Some("__blinc_computed_f64__")
                | Some("__blinc_computed_string__")
                | Some("__blinc_computed_bool__")
        )
    }

    /// Recognise a bare-Variable reference to a declared signal.
    /// Same lookup `lower_styling_args_to_overlays` uses for built-in
    /// widgets — bare identifier → `blinc_runtime::signal::lookup`
    /// hits the process-global signal registry, returns
    /// `Some((id_raw, ty))` when the name is registered.
    fn signal_id_for_variable(value: &TypedNode<TypedExpression>) -> Option<u64> {
        let TypedExpression::Variable(name) = &value.node else {
            return None;
        };
        let name_str = name.resolve_global()?;
        let (id_raw, _ty) = blinc_runtime::signal::lookup(&name_str)?;
        Some(id_raw)
    }

    /// Shape B for substrate reactive props: `__signal_get_by_id_<T>(<id_literal>)`
    /// — the wrapped form that `resolve_signal_calls` produces for
    /// FSM-context signals (`Ticker.pct` after `resolve_dotted_fsm_field_access`).
    /// Returns the raw signal id when the call matches the inner T.
    /// Mirrors the Shape B recognizer in `lower_styling_args_to_overlays`.
    fn signal_id_for_wrapped_getter(
        value: &TypedNode<TypedExpression>,
        inner_ty: &Type,
    ) -> Option<u64> {
        let TypedExpression::Call(c) = &value.node else {
            return None;
        };
        let TypedExpression::Variable(callee) = &c.callee.node else {
            return None;
        };
        let want = match inner_ty {
            Type::Primitive(PrimitiveType::I32) => "__signal_get_by_id_i32",
            Type::Primitive(PrimitiveType::I64) => "__signal_get_by_id_i64",
            Type::Primitive(PrimitiveType::F64) => "__signal_get_by_id_f64",
            Type::Primitive(PrimitiveType::Bool) => "__signal_get_by_id_bool",
            Type::Primitive(PrimitiveType::String) => "__signal_get_by_id_string",
            _ => return None,
        };
        let callee_str = callee.resolve_global()?;
        if callee_str.as_str() != want {
            return None;
        }
        let arg = c.positional_args.first()?;
        let TypedExpression::Literal(TypedLiteral::Integer(id_lit)) = &arg.node else {
            return None;
        };
        Some(*id_lit as i64 as u64)
    }

    /// Unwrap the `if <cond> { 1 } else { 0 }` shape a bool reference
    /// picks up on its way into an integer-typed arg slot.
    ///
    /// `Tog.busy` resolves to `__signal_get_by_id_bool(<id>)`, which
    /// Shape A' recognises -- but only until the bool-to-int coercion
    /// wraps it, and then the whole thing reads as an ordinary
    /// expression and falls through to LITERAL. The prop then snapshots
    /// `false` for the session: the toggle never tracks the FSM, and
    /// because `bool_state` mints its own signal for a literal, clicking
    /// it writes nowhere the DSL can see.
    fn unwrap_bool_int_coercion(
        value: &TypedNode<TypedExpression>,
    ) -> Option<&TypedNode<TypedExpression>> {
        let TypedExpression::If(if_expr) = &value.node else {
            return None;
        };
        let is_int = |n: &TypedNode<TypedExpression>, want: i128| matches!(&n.node, TypedExpression::Literal(TypedLiteral::Integer(v)) if *v == want);
        (is_int(&if_expr.then_branch, 1) && is_int(&if_expr.else_branch, 0))
            .then_some(&if_expr.condition)
    }

    /// Expand one reactive-prop arg into the wire-format slots the
    /// macro thunk expects. Scalar `Reactive<T>` returns two slots
    /// `(tag, payload: i64)`; `Reactive<String>` returns three
    /// `(tag, id_payload: i64, literal_ptr: *const i32)`. The caller
    /// splices the result in place of the original single arg.
    fn expand_reactive(
        inner_ty: &Type,
        arg: TypedNode<TypedExpression>,
    ) -> Vec<TypedNode<TypedExpression>> {
        let span = arg.span;
        let is_string = matches!(inner_ty, Type::Primitive(PrimitiveType::String));
        // Recognisers run against the coercion-stripped expression; the
        // literal path below still encodes the original.
        let probe = unwrap_bool_int_coercion(&arg).unwrap_or(&arg);

        // Shape A: bare-Variable signal ref → SIGNAL tag.
        if let Some(id_raw) = signal_id_for_variable(probe) {
            let tag = i32_literal(TAG_SIGNAL, span);
            let id = i64_literal(id_raw as i128, span);
            if is_string {
                return vec![tag, id, null_string_ptr_literal(span)];
            }
            return vec![tag, id];
        }
        // Shape A': wrapped getter `__signal_get_by_id_<T>(<id_literal>)`.
        // `resolve_signal_calls` force-wraps every FSM-context ctx-signal
        // reference into this form so action-body arithmetic / f-string
        // interp compiles; without this recognizer the wrapper survives
        // into the reactive arg slot and falls back to LITERAL, snapping
        // the value to its initial constant for the entire session.
        if let Some(id_raw) = signal_id_for_wrapped_getter(probe, inner_ty) {
            let tag = i32_literal(TAG_SIGNAL, span);
            let id = i64_literal(id_raw as i128, span);
            if is_string {
                return vec![tag, id, null_string_ptr_literal(span)];
            }
            return vec![tag, id];
        }
        // Shape B: `computed { … } : T` call → COMPUTED tag.
        // For string the call's return value is the raw derived id
        // (an i64); the literal slot stays null.
        if is_computed_call(probe) {
            let tag = i32_literal(TAG_COMPUTED, span);
            if is_string {
                return vec![tag, arg, null_string_ptr_literal(span)];
            }
            return vec![tag, arg];
        }
        // Shape C: literal expression → LITERAL tag.
        let tag = i32_literal(TAG_LITERAL, span);
        if is_string {
            // The literal is a String expression; it flows verbatim
            // into the `literal_ptr` slot. The `id_payload` slot is
            // unused for the literal path — write 0.
            return vec![tag, i64_literal(0, span), arg];
        }
        let payload = encode_literal_payload(inner_ty, &arg);
        vec![tag, i64_literal(payload, span)]
    }

    /// A null `*const i32` literal for the unused `literal_ptr` slot
    /// in non-literal `Reactive<String>` wire-format triples. Zyntax
    /// doesn't have a dedicated null-pointer literal, so we approximate
    /// with an empty `""` string literal — `decode_string` on the
    /// resulting pointer would yield an empty string, but the macro
    /// thunk's match never reaches the literal branch when the tag
    /// is SIGNAL or COMPUTED, so the value is never observed.
    fn null_string_ptr_literal(span: zyntax_typed_ast::Span) -> TypedNode<TypedExpression> {
        TypedNode::new(
            TypedExpression::Literal(zyntax_typed_ast::TypedLiteral::String(
                zyntax_typed_ast::InternedString::new_global(""),
            )),
            Type::Primitive(PrimitiveType::String),
            span,
        )
    }

    /// Per-call expansion. Iterates `props` and `positional_args` in
    /// lockstep; each reactive prop's single arg slot becomes two,
    /// each non-reactive prop passes through.
    fn rewrite_call(call: &mut TypedCall) {
        let Some(props) = registry_props(call) else {
            return;
        };
        // Cheap pre-check: if the widget has no reactive props, the
        // walk would be a no-op. Skip.
        if !props.iter().any(|p| p.reactive_inner.is_some()) {
            return;
        }
        let old_args = std::mem::take(&mut call.positional_args);
        let mut new_args: Vec<TypedNode<TypedExpression>> =
            Vec::with_capacity(old_args.len() + props.len());

        let mut arg_iter = old_args.into_iter();
        for prop in &props {
            let Some(arg) = arg_iter.next() else {
                break;
            };
            if let Some(inner_ty) = &prop.reactive_inner {
                new_args.extend(expand_reactive(inner_ty, arg));
            } else {
                new_args.push(arg);
            }
        }
        // Any args beyond the prop count pass through unchanged —
        // matches existing behaviour for varargs / overflow.
        new_args.extend(arg_iter);

        call.positional_args = new_args;
    }

    fn rewrite_expr(expr: &mut TypedNode<TypedExpression>) {
        match &mut expr.node {
            TypedExpression::Call(call) => {
                rewrite_expr(&mut call.callee);
                for a in &mut call.positional_args {
                    rewrite_expr(a);
                }
                for na in &mut call.named_args {
                    rewrite_expr(&mut na.value);
                }
                rewrite_call(call);
            }
            TypedExpression::MethodCall(mc) => {
                rewrite_expr(&mut mc.receiver);
                for a in &mut mc.positional_args {
                    rewrite_expr(a);
                }
            }
            TypedExpression::Binary(b) => {
                rewrite_expr(&mut b.left);
                rewrite_expr(&mut b.right);
            }
            TypedExpression::Unary(u) => rewrite_expr(&mut u.operand),
            TypedExpression::Field(f) => rewrite_expr(&mut f.object),
            TypedExpression::Index(i) => {
                rewrite_expr(&mut i.object);
                rewrite_expr(&mut i.index);
            }
            TypedExpression::Array(items) | TypedExpression::Tuple(items) => {
                for it in items {
                    rewrite_expr(it);
                }
            }
            TypedExpression::Block(block) => {
                for stmt in &mut block.statements {
                    rewrite_stmt(stmt);
                }
            }
            TypedExpression::If(if_expr) => {
                rewrite_expr(&mut if_expr.condition);
                rewrite_expr(&mut if_expr.then_branch);
                rewrite_expr(&mut if_expr.else_branch);
            }
            _ => {}
        }
    }

    fn rewrite_stmt(stmt: &mut TypedNode<TypedStatement>) {
        match &mut stmt.node {
            TypedStatement::Expression(e) => rewrite_expr(e),
            TypedStatement::Let(l) => {
                if let Some(init) = &mut l.initializer {
                    rewrite_expr(init);
                }
            }
            TypedStatement::Return(Some(e)) => rewrite_expr(e),
            TypedStatement::If(if_stmt) => {
                rewrite_expr(&mut if_stmt.condition);
                for inner in &mut if_stmt.then_block.statements {
                    rewrite_stmt(inner);
                }
                if let Some(else_block) = &mut if_stmt.else_block {
                    for inner in &mut else_block.statements {
                        rewrite_stmt(inner);
                    }
                }
            }
            TypedStatement::While(w) => {
                rewrite_expr(&mut w.condition);
                for inner in &mut w.body.statements {
                    rewrite_stmt(inner);
                }
            }
            TypedStatement::Block(b) => {
                for inner in &mut b.statements {
                    rewrite_stmt(inner);
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
                        rewrite_stmt(stmt);
                    }
                }
            }
            TypedDeclaration::Impl(imp) => {
                for method in &mut imp.methods {
                    if let Some(body) = &mut method.body {
                        for stmt in &mut body.statements {
                            rewrite_stmt(stmt);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Give every `computed { … } : T` lambda a real `Type::Function`
/// annotation on its node.
///
/// Zyntax builds a lambda's compiled signature from the type carried on
/// the Lambda expression node: `Type::Function { return_type, .. }` is
/// honoured, and **anything else falls back to an `I64` return**. The
/// grammar emits the lambda with the node type left as `Unit`, so a
/// `computed { … } : f64` compiled to a function returning an integer in
/// `rax` — while the host side (`blinc_dsl_computed_f64`) transmutes the
/// pointer to `extern "C" fn() -> f64` and reads `xmm0`. The result was a
/// silent `0.0` for every f64 computed binding (and the same class of
/// mismatch for the other widths).
///
/// Annotating the node makes the JIT emit the return in the register the
/// host FFI actually reads. MUST run before the reactive/styling passes
/// consume these calls, and before codegen.
pub(crate) fn annotate_computed_lambda_types(program: &mut TypedProgram) {
    use zyntax_typed_ast::type_registry::{AsyncKind, CallingConvention, NullabilityKind};
    use zyntax_typed_ast::{TypedDeclaration, TypedExpression, TypedNode};

    /// `__blinc_computed_<T>__` → the `T` the host FFI reads back.
    fn computed_return_ty(callee: &str) -> Option<Type> {
        match callee {
            "__blinc_computed_i32__" => Some(Type::Primitive(PrimitiveType::I32)),
            "__blinc_computed_f64__" => Some(Type::Primitive(PrimitiveType::F64)),
            "__blinc_computed_bool__" => Some(Type::Primitive(PrimitiveType::Bool)),
            // The string ABI returns a length-prefixed pointer, which the
            // host decodes — an integer-width return is correct here.
            "__blinc_computed_string__" => Some(Type::Primitive(PrimitiveType::I64)),
            _ => None,
        }
    }

    fn fn_ty(return_type: Type) -> Type {
        Type::Function {
            params: Vec::new(),
            return_type: Box::new(return_type),
            is_varargs: false,
            has_named_params: false,
            has_default_params: false,
            async_kind: AsyncKind::Sync,
            calling_convention: CallingConvention::Default,
            nullability: NullabilityKind::NonNull,
        }
    }

    fn walk_expr(expr: &mut TypedNode<TypedExpression>) {
        if let TypedExpression::Call(call) = &mut expr.node {
            let callee_name = match &call.callee.node {
                TypedExpression::Variable(v) => v.resolve_global().map(|s| s.to_string()),
                _ => None,
            };
            if let Some(ret) = callee_name.as_deref().and_then(computed_return_ty)
                && let Some(arg) = call.positional_args.first_mut()
                && matches!(arg.node, TypedExpression::Lambda(_))
            {
                arg.ty = fn_ty(ret);
                // Normalise the body to the shape zyntax's lambda compiler
                // documents as the implicit-return form: `LambdaBody::Block`
                // whose trailing statement is a bare `Expression`. The
                // grammar emits `LambdaBody::Expression(Block{[Return(e)]})`,
                // which compiles but yields no value.
                if let TypedExpression::Lambda(l) = &mut arg.node
                    && let zyntax_typed_ast::typed_ast::TypedLambdaBody::Expression(be) = &l.body
                    && let TypedExpression::Block(b) = &be.node
                    && b.statements.len() == 1
                    && let TypedStatement::Return(Some(inner)) = &b.statements[0].node
                {
                    let span = b.statements[0].span;
                    let inner = inner.clone();
                    l.body = zyntax_typed_ast::typed_ast::TypedLambdaBody::Block(
                        zyntax_typed_ast::typed_ast::TypedBlock {
                            statements: vec![TypedNode::new(
                                TypedStatement::Expression(inner),
                                Type::Primitive(PrimitiveType::Unit),
                                span,
                            )],
                            span,
                        },
                    );
                }
            }
            walk_expr(&mut call.callee);
            for a in &mut call.positional_args {
                walk_expr(a);
            }
            for n in &mut call.named_args {
                walk_expr(&mut n.value);
            }
        } else {
            match &mut expr.node {
                TypedExpression::Array(items) => items.iter_mut().for_each(walk_expr),
                TypedExpression::Block(b) => b.statements.iter_mut().for_each(walk_stmt),
                TypedExpression::Binary(b) => {
                    walk_expr(&mut b.left);
                    walk_expr(&mut b.right);
                }
                TypedExpression::Lambda(l) => {
                    if let zyntax_typed_ast::typed_ast::TypedLambdaBody::Block(b) = &mut l.body {
                        b.statements.iter_mut().for_each(walk_stmt);
                    }
                }
                _ => {}
            }
        }
    }

    fn walk_stmt(stmt: &mut TypedNode<TypedStatement>) {
        match &mut stmt.node {
            TypedStatement::Expression(e) | TypedStatement::Return(Some(e)) => walk_expr(e),
            TypedStatement::Let(l) => {
                if let Some(i) = &mut l.initializer {
                    walk_expr(i);
                }
            }
            TypedStatement::If(i) => {
                walk_expr(&mut i.condition);
                i.then_block.statements.iter_mut().for_each(walk_stmt);
                if let Some(e) = &mut i.else_block {
                    e.statements.iter_mut().for_each(walk_stmt);
                }
            }
            TypedStatement::While(w) => {
                walk_expr(&mut w.condition);
                w.body.statements.iter_mut().for_each(walk_stmt);
            }
            TypedStatement::Block(b) => b.statements.iter_mut().for_each(walk_stmt),
            _ => {}
        }
    }

    for decl in program.declarations.iter_mut() {
        match &mut decl.node {
            TypedDeclaration::Function(f) => {
                if let Some(b) = &mut f.body {
                    b.statements.iter_mut().for_each(walk_stmt);
                }
            }
            TypedDeclaration::Impl(imp) => {
                for m in imp.methods.iter_mut() {
                    if let Some(b) = &mut m.body {
                        b.statements.iter_mut().for_each(walk_stmt);
                    }
                }
            }
            _ => {}
        }
    }
}
