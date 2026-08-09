//! Emits an `@effect(E) fiber def` per `fsm` declaration.
//!
//! For `fsm Play { state Idle  state Busy  initial Idle  on Idle.Go -> Busy }`:
//!
//! ```text
//! effect Play$Events { def Play$next_event(): i64 }
//!
//! @effect(Play$Events)
//! fiber def Play$machine(): i64 {
//!     let mut state: i64 = 0          // initial code
//!     while 1 == 1 {
//!         let ev: i64 = Play$next_event()
//!         if state == 0 { if ev == <Go> { state = 1 } }
//!         yield state
//!     }
//!     return state
//! }
//! ```
//!
//! State codes and event codes are the registry's, so a state name means
//! the same number in both lowerings. Transitions nest from-state
//! outside, event inside, preserving the registry's first-match order.
//!
//! The machine loops until the host drops the fiber; the trailing
//! `return` is unreachable and exists to type the function.
//!
//! A rule's body runs after its state assignment, matching the order
//! the registry path uses, so a body that reads the state sees the one
//! it transitioned into.
//!
//! Context fields belong to the handler, which is what owns them: its
//! state is allocated per instance and the host installs that instance
//! both around the machine's steps and around a read, so two machines
//! hold two contexts. `ctx.<field>` in a transition body lowers to a
//! perform, and a compiled `<Fsm>$read_<field>` gives the host
//! something to call inside a pushed scope, since it cannot perform on
//! its own.
//!
//! Tick guards evaluate when the host sends [`TICK_EVENT_CODE`]. A guard
//! is a condition on top of the state test, which is the nested shape
//! the loop's join cannot see, so the guard's own result folds into the
//! key: doubling the key first leaves the low bit to the guard, and the
//! rule's constant matches only when the state and the guard both do.
//!
//! Nothing consumes these declarations; the registry path still drives
//! every mounted FSM.

use crate::{PrimitiveType, Type, TypedProgram};
use zyntax_typed_ast::Mutability;
// The pass-local definition, keyed by NAME — the code tables do not
// exist until `runtime_bridge` builds them after compilation, so this
// pass derives its own with the same rules.
use crate::fsm_registry::FsmDefinition;
use zyntax_typed_ast::typed_ast::{
    BinaryOp, TypedBinary, TypedBlock, TypedCall, TypedDeclaration, TypedEffect,
    TypedEffectHandler, TypedEffectHandlerImpl, TypedEffectOp, TypedExpression, TypedFunction,
    TypedIf, TypedLet, TypedLiteral, TypedStatement, TypedWhile,
};
use zyntax_typed_ast::{InternedString, Span, TypedNode};

/// Effect declared for `fsm`'s events.
pub(crate) fn events_effect_name(fsm: &str) -> String {
    format!("{fsm}$Events")
}

/// The operation a machine performs to take its next event.
pub(crate) fn next_event_op_name(fsm: &str) -> String {
    format!("{fsm}$next_event")
}

/// The `fiber def` a mounted FSM constructs.
pub(crate) fn machine_fn_name(fsm: &str) -> String {
    format!("{fsm}$machine")
}

/// Handler installed around a step. Owns the FSM's context as its own
/// state and answers events from the host's armed slot.
pub(crate) fn host_events_handler_name(fsm: &str) -> String {
    format!("{fsm}$HostEvents")
}

/// Operation reading context field `field`.
pub(crate) fn ctx_get_op_name(fsm: &str, field: &str) -> String {
    format!("{fsm}$get_{field}")
}

/// Operation writing context field `field`.
pub(crate) fn ctx_set_op_name(fsm: &str, field: &str) -> String {
    format!("{fsm}$set_{field}")
}

/// Compiled reader for context field `field`.
///
/// A perform needs a function to happen inside; the host cannot perform
/// on its own. This is that function: called with the FSM's handler
/// instance installed, it performs the read and the instance answers.
pub(crate) fn ctx_reader_fn_name(fsm: &str, field: &str) -> String {
    format!("{fsm}$read_{field}")
}

thread_local! {
    /// Transition bodies as authored, before `ctx.<field>` was rewritten
    /// into module-level signal access. Filled by the action lifter and
    /// read by this pass during the same compile on the same thread.
    static ACTION_BODIES: std::cell::RefCell<std::collections::HashMap<String, TypedBlock>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Record a transition body under its lifted symbol name.
pub(crate) fn stash_action_body(symbol: &str, body: &TypedBlock) {
    ACTION_BODIES.with(|m| {
        m.borrow_mut().insert(symbol.to_string(), body.clone());
    });
}

fn take_action_body(symbol: &str) -> Option<TypedBlock> {
    ACTION_BODIES.with(|m| m.borrow().get(symbol).cloned())
}

thread_local! {
    /// Tick-guard expressions as authored, keyed `<fsm>#<idx>`.
    static GUARD_EXPRS: std::cell::RefCell<
        std::collections::HashMap<String, TypedNode<TypedExpression>>,
    > = std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Record a tick guard as authored, before its `ctx.<field>` reads were
/// rewritten into module-level signal access.
pub(crate) fn stash_guard_expr(fsm: &str, idx: usize, expr: &TypedNode<TypedExpression>) {
    GUARD_EXPRS.with(|m| {
        m.borrow_mut().insert(format!("{fsm}#{idx}"), expr.clone());
    });
}

fn take_guard_expr(fsm: &str, idx: usize) -> Option<TypedNode<TypedExpression>> {
    GUARD_EXPRS.with(|m| m.borrow().get(&format!("{fsm}#{idx}")).cloned())
}

/// Rewrite `ctx.<field>` in a transition body into performs against the
/// handler that owns the context: a read becomes `<Fsm>$get_<field>()`
/// and an assignment becomes `<Fsm>$set_<field>(rhs)`.
fn ctx_to_performs(fsm: &str, fields: &[(String, Type)], block: &mut TypedBlock) {
    for st in &mut block.statements {
        ctx_to_performs_stmt(fsm, fields, st);
    }
}

fn ctx_to_performs_stmt(fsm: &str, fields: &[(String, Type)], st: &mut TypedNode<TypedStatement>) {
    match &mut st.node {
        TypedStatement::Expression(e) => {
            // An assignment whose target is `ctx.<field>` becomes a
            // write perform; anything else just has its reads rewritten.
            if let TypedExpression::Binary(b) = &mut e.node
                && b.op == BinaryOp::Assign
                && let Some(field) = ctx_field_of(&b.left)
                && let Some((name, ty)) = fields.iter().find(|(n, _)| *n == field)
            {
                ctx_to_performs_expr(fsm, fields, &mut b.right);
                let mut rhs = (*b.right).clone();
                rhs.ty = ty.clone();
                e.node = TypedExpression::Call(TypedCall {
                    callee: Box::new(var(&ctx_set_op_name(fsm, name))),
                    positional_args: vec![rhs],
                    named_args: Vec::new(),
                    type_args: Vec::new(),
                });
                e.ty = Type::Primitive(PrimitiveType::Unit);
                return;
            }
            ctx_to_performs_expr(fsm, fields, e);
        }
        TypedStatement::Let(l) => {
            if let Some(init) = &mut l.initializer {
                ctx_to_performs_expr(fsm, fields, init);
            }
        }
        TypedStatement::Return(Some(e)) => ctx_to_performs_expr(fsm, fields, e),
        TypedStatement::Block(b) => ctx_to_performs(fsm, fields, b),
        _ => {}
    }
}

fn ctx_to_performs_expr(fsm: &str, fields: &[(String, Type)], e: &mut TypedNode<TypedExpression>) {
    if let Some(field) = ctx_field_of(e)
        && let Some((name, ty)) = fields.iter().find(|(n, _)| *n == field)
    {
        e.node = TypedExpression::Call(TypedCall {
            callee: Box::new(var(&ctx_get_op_name(fsm, name))),
            positional_args: Vec::new(),
            named_args: Vec::new(),
            type_args: Vec::new(),
        });
        e.ty = ty.clone();
        return;
    }
    match &mut e.node {
        TypedExpression::Binary(b) => {
            ctx_to_performs_expr(fsm, fields, &mut b.left);
            ctx_to_performs_expr(fsm, fields, &mut b.right);
        }
        TypedExpression::Unary(u) => ctx_to_performs_expr(fsm, fields, &mut u.operand),
        TypedExpression::Call(c) => {
            for a in &mut c.positional_args {
                ctx_to_performs_expr(fsm, fields, a);
            }
        }
        _ => {}
    }
}

/// `ctx.<field>` → the field name.
fn ctx_field_of(e: &TypedNode<TypedExpression>) -> Option<String> {
    let TypedExpression::Field(f) = &e.node else {
        return None;
    };
    let TypedExpression::Variable(obj) = &f.object.node else {
        return None;
    };
    if obj.resolve_global().as_deref() != Some("ctx") {
        return None;
    }
    f.field.resolve_global().map(|s| s.to_string())
}

/// Event code the host sends to make a machine evaluate its tick
/// guards. Distinct from every declared event, which are assigned from
/// the runtime's offset upward.
pub const TICK_EVENT_CODE: u32 = blinc_runtime::fsm::FSM_EVENT_CODE_OFFSET - 1;

/// Multiplier separating a state from an event in the dispatch key.
/// Clears the runtime's event-code offset so the two never overlap.
const STATE_STRIDE: i64 = 1 << 33;

/// Host symbol every handler body calls for its event code.
pub(crate) const NEXT_EVENT_EXTERN: &str = "__blinc_fsm_next_event";

fn i64_ty() -> Type {
    Type::Primitive(PrimitiveType::I64)
}

fn int(value: i64) -> TypedNode<TypedExpression> {
    TypedNode::new(
        TypedExpression::Literal(TypedLiteral::Integer(value as i128)),
        i64_ty(),
        Span::default(),
    )
}

fn var(name: &str) -> TypedNode<TypedExpression> {
    TypedNode::new(
        TypedExpression::Variable(InternedString::new_global(name)),
        i64_ty(),
        Span::default(),
    )
}

fn binary(
    op: BinaryOp,
    lhs: TypedNode<TypedExpression>,
    rhs: TypedNode<TypedExpression>,
    ty: Type,
) -> TypedNode<TypedExpression> {
    TypedNode::new(
        TypedExpression::Binary(TypedBinary {
            op,
            left: Box::new(lhs),
            right: Box::new(rhs),
        }),
        ty,
        Span::default(),
    )
}

fn eq(
    lhs: TypedNode<TypedExpression>,
    rhs: TypedNode<TypedExpression>,
) -> TypedNode<TypedExpression> {
    binary(BinaryOp::Eq, lhs, rhs, Type::Primitive(PrimitiveType::Bool))
}

fn stmt(s: TypedStatement) -> TypedNode<TypedStatement> {
    TypedNode::new(s, Type::Primitive(PrimitiveType::Unit), Span::default())
}

fn block(statements: Vec<TypedNode<TypedStatement>>) -> TypedBlock {
    TypedBlock {
        statements,
        span: Span::default(),
    }
}

/// The guard's result as an i64 in {0, 1}.
///
/// Prefers the authored expression, so `ctx.<field>` reads reach the
/// handler that owns the context rather than a module-level signal.
/// Falls back to calling the lifted guard fn when nothing was stashed.
fn guard_value(
    fsm: &str,
    idx: usize,
    g: &crate::fsm_registry::TickGuard,
    ctx_fields: &[(String, Type)],
) -> Option<TypedNode<TypedExpression>> {
    if let Some(mut expr) = take_guard_expr(fsm, idx) {
        ctx_to_performs_expr(fsm, ctx_fields, &mut expr);
        // A comparison yields bool; the key arithmetic needs an int.
        return Some(TypedNode::new(
            TypedExpression::Cast(zyntax_typed_ast::typed_ast::TypedCast {
                expr: Box::new(expr),
                target_type: i64_ty(),
            }),
            i64_ty(),
            Span::default(),
        ));
    }
    let symbol = g.guard_fn?.resolve_global()?;
    Some(TypedNode::new(
        TypedExpression::Cast(zyntax_typed_ast::typed_ast::TypedCast {
            expr: Box::new(TypedNode::new(
                TypedExpression::Call(TypedCall {
                    callee: Box::new(var(&symbol)),
                    positional_args: Vec::new(),
                    named_args: Vec::new(),
                    type_args: Vec::new(),
                }),
                Type::Primitive(PrimitiveType::I32),
                Span::default(),
            )),
            target_type: i64_ty(),
        }),
        i64_ty(),
        Span::default(),
    ))
}

/// `<lifted_action>()` — a transition body the FSM pass already lifted
/// to a top-level zero-arg fn. Only `Symbol` reaches here; the reduced
/// `SetI32` / `AddI32` forms are not emitted by the DSL.
fn call_action(symbol: &str) -> TypedNode<TypedStatement> {
    stmt(TypedStatement::Expression(Box::new(TypedNode::new(
        TypedExpression::Call(TypedCall {
            callee: Box::new(var(symbol)),
            positional_args: Vec::new(),
            named_args: Vec::new(),
            type_args: Vec::new(),
        }),
        Type::Primitive(PrimitiveType::Unit),
        Span::default(),
    ))))
}

/// `state = <code>`
fn assign_state(code: u32) -> TypedNode<TypedStatement> {
    stmt(TypedStatement::Expression(Box::new(binary(
        BinaryOp::Assign,
        var("state"),
        int(code as i64),
        Type::Primitive(PrimitiveType::Unit),
    ))))
}

/// State variant names of `fsm`, in declaration order. The index is the
/// state code, which is how `runtime_bridge` assigns them too.
fn state_names_of(program: &TypedProgram, fsm_name: InternedString) -> Vec<String> {
    program
        .declarations
        .iter()
        .find_map(|d| match &d.node {
            TypedDeclaration::Enum(e) if e.name == fsm_name => Some(e),
            _ => None,
        })
        .map(|e| {
            e.variants
                .iter()
                .map(|v| v.name.resolve_global().unwrap_or_default().to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// Event codes in first-appearance order, carrying the same offset the
/// runtime registry applies so a code means one event on both paths.
fn event_codes_of(def: &FsmDefinition) -> Vec<(String, u32)> {
    let mut out: Vec<(String, u32)> = Vec::new();
    for t in &def.transitions {
        let Some(name) = t.event.resolve_global() else {
            continue;
        };
        let name = name.to_string();
        if out.iter().any(|(n, _)| *n == name) {
            continue;
        }
        let code = out.len() as u32 + blinc_runtime::fsm::FSM_EVENT_CODE_OFFSET;
        out.push((name, code));
    }
    out
}

/// Emit the effect and the machine for every FSM the registry pass
/// found. Existing declarations are left alone.
pub(crate) fn synthesize_fsm_fibers(
    program: &mut TypedProgram,
    found: &[(InternedString, FsmDefinition)],
) {
    let mut emit: Vec<(TypedEffect, TypedEffectHandler, TypedFunction)> = Vec::new();
    let mut readers: Vec<TypedFunction> = Vec::new();
    for (fsm_name, def) in found {
        let Some(fsm) = fsm_name.resolve_global() else {
            continue;
        };
        let fsm: &str = &fsm;
        let states = state_names_of(program, *fsm_name);
        if states.is_empty() {
            continue;
        }
        emit.push((
            events_effect(fsm, def),
            host_events_handler(fsm, def),
            machine_fn(fsm, def, &states),
        ));
        readers.extend(ctx_readers(fsm, def));
    }
    if emit.is_empty() {
        return;
    }
    // `extern fn __blinc_fsm_next_event(): i64` — the handler bodies
    // call it, and lowering rejects a call to a function it has no
    // declaration for. One declaration serves every machine.
    program.declarations.push(TypedNode::new(
        TypedDeclaration::Function(TypedFunction {
            name: InternedString::new_global(NEXT_EVENT_EXTERN),
            return_type: i64_ty(),
            body: None,
            is_external: true,
            ..Default::default()
        }),
        Type::Unknown,
        Span::default(),
    ));
    for (effect, handler, machine) in emit {
        program.declarations.push(TypedNode::new(
            TypedDeclaration::Effect(effect),
            Type::Unknown,
            Span::default(),
        ));
        program.declarations.push(TypedNode::new(
            TypedDeclaration::EffectHandler(handler),
            Type::Unknown,
            Span::default(),
        ));
        program.declarations.push(TypedNode::new(
            TypedDeclaration::Function(machine),
            Type::Unknown,
            Span::default(),
        ));
    }
    for reader in readers {
        program.declarations.push(TypedNode::new(
            TypedDeclaration::Function(reader),
            Type::Unknown,
            Span::default(),
        ));
    }
}

/// `@effect(<Fsm>$Events) def <Fsm>$read_<field>(): T { return <Fsm>$get_<field>() }`
fn ctx_readers(fsm: &str, def: &FsmDefinition) -> Vec<TypedFunction> {
    def.context_fields
        .iter()
        .filter_map(|f| {
            let name = f.name.resolve_global()?;
            let ty = ctx_field_type(f);
            Some(TypedFunction {
                name: InternedString::new_global(&ctx_reader_fn_name(fsm, &name)),
                effects: vec![InternedString::new_global(&events_effect_name(fsm))],
                return_type: ty.clone(),
                body: Some(block(vec![stmt(TypedStatement::Return(Some(Box::new(
                    TypedNode::new(
                        TypedExpression::Call(TypedCall {
                            callee: Box::new(var(&ctx_get_op_name(fsm, &name))),
                            positional_args: Vec::new(),
                            named_args: Vec::new(),
                            type_args: Vec::new(),
                        }),
                        ty,
                        Span::default(),
                    ),
                ))))])),
                ..Default::default()
            })
        })
        .collect()
}

/// The handler that owns the FSM's context.
///
/// Its fields ARE the context: handler state is allocated per scope by
/// the synthesized `H$new` and carried for a machine's lifetime once
/// bound, so two bound machines hold two contexts with nothing shared.
/// Anything outside reads a field by performing the matching op, which
/// is what "delegate to the FSM that owns it" means concretely.
///
/// `next_event` stays here too: the event belongs to the resume, so it
/// reads the host's armed slot rather than any field.
fn host_events_handler(fsm: &str, def: &FsmDefinition) -> TypedEffectHandler {
    let take_event = TypedEffectHandlerImpl {
        op_name: InternedString::new_global(&next_event_op_name(fsm)),
        return_type: i64_ty(),
        body: Some(block(vec![stmt(TypedStatement::Return(Some(Box::new(
            TypedNode::new(
                TypedExpression::Call(TypedCall {
                    callee: Box::new(var(NEXT_EVENT_EXTERN)),
                    positional_args: Vec::new(),
                    named_args: Vec::new(),
                    type_args: Vec::new(),
                }),
                i64_ty(),
                Span::default(),
            ),
        ))))])),
        ..Default::default()
    };

    let mut fields = Vec::new();
    let mut handlers = vec![take_event];
    for f in &def.context_fields {
        let Some(name) = f.name.resolve_global() else {
            continue;
        };
        let ty = ctx_field_type(f);
        fields.push(zyntax_typed_ast::typed_ast::TypedField {
            name: f.name,
            ty: ty.clone(),
            initializer: Some(Box::new(ctx_field_default(f))),
            visibility: Default::default(),
            mutability: Mutability::Mutable,
            is_static: false,
            span: Span::default(),
        });
        // `def <Fsm>$get_<field>(): T { return self.<field> }`
        handlers.push(TypedEffectHandlerImpl {
            op_name: InternedString::new_global(&ctx_get_op_name(fsm, &name)),
            return_type: ty.clone(),
            body: Some(block(vec![stmt(TypedStatement::Return(Some(Box::new(
                self_field(f.name, ty.clone()),
            ))))])),
            ..Default::default()
        });
        // `def <Fsm>$set_<field>(v: T) { self.<field> = v }`
        handlers.push(TypedEffectHandlerImpl {
            op_name: InternedString::new_global(&ctx_set_op_name(fsm, &name)),
            params: vec![zyntax_typed_ast::typed_ast::TypedParameter {
                name: InternedString::new_global("v"),
                ty: ty.clone(),
                ..Default::default()
            }],
            return_type: Type::Primitive(PrimitiveType::Unit),
            body: Some(block(vec![stmt(TypedStatement::Expression(Box::new(
                binary(
                    BinaryOp::Assign,
                    self_field(f.name, ty.clone()),
                    TypedNode::new(
                        TypedExpression::Variable(InternedString::new_global("v")),
                        ty.clone(),
                        Span::default(),
                    ),
                    Type::Primitive(PrimitiveType::Unit),
                ),
            )))])),
            ..Default::default()
        });
    }

    TypedEffectHandler {
        name: InternedString::new_global(&host_events_handler_name(fsm)),
        effect_name: InternedString::new_global(&events_effect_name(fsm)),
        fields,
        handlers,
        ..Default::default()
    }
}

/// `self.<field>`
fn self_field(field: InternedString, ty: Type) -> TypedNode<TypedExpression> {
    TypedNode::new(
        TypedExpression::Field(zyntax_typed_ast::typed_ast::TypedFieldAccess {
            object: Box::new(TypedNode::new(
                TypedExpression::Variable(InternedString::new_global("self")),
                Type::Unknown,
                Span::default(),
            )),
            field,
        }),
        ty,
        Span::default(),
    )
}

fn events_effect(fsm: &str, def: &FsmDefinition) -> TypedEffect {
    let mut operations = vec![TypedEffectOp {
        name: InternedString::new_global(&next_event_op_name(fsm)),
        return_type: i64_ty(),
        ..Default::default()
    }];
    // One read op per context field. The handler owns the storage, so
    // this is how anything outside the machine asks the FSM for it
    // rather than reaching a global of its own.
    for f in &def.context_fields {
        let Some(name) = f.name.resolve_global() else {
            continue;
        };
        let ty = ctx_field_type(f);
        operations.push(TypedEffectOp {
            name: InternedString::new_global(&ctx_get_op_name(fsm, &name)),
            return_type: ty.clone(),
            ..Default::default()
        });
        operations.push(TypedEffectOp {
            name: InternedString::new_global(&ctx_set_op_name(fsm, &name)),
            params: vec![zyntax_typed_ast::typed_ast::TypedParameter {
                name: InternedString::new_global("v"),
                ty: ty.clone(),
                ..Default::default()
            }],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ..Default::default()
        });
    }
    TypedEffect {
        name: InternedString::new_global(&events_effect_name(fsm)),
        operations,
        ..Default::default()
    }
}

/// The declared type of a context field.
fn ctx_field_type(f: &crate::fsm_registry::ContextField) -> Type {
    use crate::fsm_registry::ContextDefault;
    Type::Primitive(match f.default {
        ContextDefault::I32(_) => PrimitiveType::I32,
        ContextDefault::F64(_) => PrimitiveType::F64,
        ContextDefault::Bool(_) => PrimitiveType::Bool,
        ContextDefault::String(_) => PrimitiveType::String,
    })
}

/// The field's declared default, as a literal.
fn ctx_field_default(f: &crate::fsm_registry::ContextField) -> TypedNode<TypedExpression> {
    use crate::fsm_registry::ContextDefault;
    let (lit, ty) = match &f.default {
        ContextDefault::I32(v) => (
            TypedLiteral::Integer(*v as i128),
            Type::Primitive(PrimitiveType::I32),
        ),
        ContextDefault::F64(v) => (TypedLiteral::Float(*v), Type::Primitive(PrimitiveType::F64)),
        ContextDefault::Bool(v) => (TypedLiteral::Bool(*v), Type::Primitive(PrimitiveType::Bool)),
        ContextDefault::String(v) => (
            TypedLiteral::String(*v),
            Type::Primitive(PrimitiveType::String),
        ),
    };
    TypedNode::new(TypedExpression::Literal(lit), ty, Span::default())
}

fn machine_fn(fsm: &str, def: &FsmDefinition, states: &[String]) -> TypedFunction {
    let ctx_fields: Vec<(String, Type)> = def
        .context_fields
        .iter()
        .filter_map(|f| Some((f.name.resolve_global()?.to_string(), ctx_field_type(f))))
        .collect();
    let code_of = |name: InternedString| -> Option<u32> {
        let n = name.resolve_global()?;
        states.iter().position(|s| *s == *n).map(|i| i as u32)
    };
    let events = event_codes_of(def);
    let event_code_of = |name: InternedString| -> Option<u32> {
        let n = name.resolve_global()?;
        events.iter().find(|(e, _)| *e == *n).map(|(_, c)| *c)
    };
    let initial = def.initial.and_then(code_of).unwrap_or(0);

    // `let mut state: i64 = <initial>`
    let init = stmt(TypedStatement::Let(TypedLet {
        name: InternedString::new_global("state"),
        ty: i64_ty(),
        initializer: Some(Box::new(int(initial as i64))),
        mutability: Mutability::Mutable,
        span: Span::default(),
    }));

    // `let ev: i64 = <Fsm>$next_event()`
    let take_event = stmt(TypedStatement::Let(TypedLet {
        name: InternedString::new_global("ev"),
        ty: i64_ty(),
        initializer: Some(Box::new(TypedNode::new(
            TypedExpression::Call(TypedCall {
                callee: Box::new(var(&next_event_op_name(fsm))),
                positional_args: Vec::new(),
                named_args: Vec::new(),
                type_args: Vec::new(),
            }),
            i64_ty(),
            Span::default(),
        ))),
        mutability: Mutability::Immutable,
        span: Span::default(),
    }));

    // `let key: i64 = state * STATE_STRIDE + ev` — one comparison per
    // rule instead of a state test wrapping an event test.
    //
    // Nesting an `if` inside an `if` inside the loop leaves the
    // assignment invisible to the loop's join, so the machine never
    // advances; `&&` in the condition faults the Cranelift tier. Both
    // are upstream, and both are avoided by making each rule a single
    // equality at the loop's top level.
    //
    // Event codes carry the runtime's high offset, so the stride has to
    // clear them: a state contributes a multiple of 2^33, an event
    // stays under 2^32, and the two never overlap.
    let mut body = vec![take_event];
    body.push(stmt(TypedStatement::Let(TypedLet {
        name: InternedString::new_global("key"),
        ty: i64_ty(),
        initializer: Some(Box::new(binary(
            BinaryOp::Add,
            binary(BinaryOp::Mul, var("state"), int(STATE_STRIDE), i64_ty()),
            var("ev"),
            i64_ty(),
        ))),
        mutability: Mutability::Immutable,
        span: Span::default(),
    })));
    for t in &def.transitions {
        let (Some(from), Some(ev), Some(to)) =
            (code_of(t.from), event_code_of(t.event), code_of(t.to))
        else {
            continue;
        };
        body.push(stmt(TypedStatement::If(TypedIf {
            condition: Box::new(eq(var("key"), int(from as i64 * STATE_STRIDE + ev as i64))),
            // State first, then the body — the order the registry
            // path runs them in, so a body that reads the state sees
            // the one it transitioned into.
            then_block: block({
                let mut arm = vec![assign_state(to)];
                for action in &t.actions {
                    let blinc_runtime::fsm::TransitionAction::Symbol(sym) = action else {
                        continue;
                    };
                    // The authored body, with `ctx.<field>` lowered to
                    // performs, inlined here rather than called: the
                    // lifted fn writes module-level signals, and the
                    // machine's context lives in its handler.
                    match take_action_body(sym) {
                        Some(mut body) => {
                            ctx_to_performs(fsm, &ctx_fields, &mut body);
                            arm.extend(body.statements);
                        }
                        None => arm.push(call_action(sym)),
                    }
                }
                arm
            }),
            else_block: None,
            span: Span::default(),
        })));
    }
    // Tick guards, evaluated when the host sends TICK_EVENT_CODE.
    //
    // A guard is a condition on top of the state test, which is the
    // nested shape the loop's join cannot see. Folding the guard's own
    // result into the key keeps it one comparison: `key * 2 + guard()`
    // is the rule's constant only when the state matches AND the guard
    // returned 1. Doubling first means the guard owns the low bit
    // outright, so no other pair can alias into a match.
    for (idx, g) in def.tick_guards.iter().enumerate() {
        let Some(from) = code_of(g.from) else {
            continue;
        };
        let Some(to) = code_of(g.to) else { continue };
        let Some(fired) = guard_value(fsm, idx, g, &ctx_fields) else {
            continue;
        };
        let probe = format!("g{idx}");
        body.push(stmt(TypedStatement::Let(TypedLet {
            name: InternedString::new_global(&probe),
            ty: i64_ty(),
            initializer: Some(Box::new(binary(
                BinaryOp::Add,
                binary(BinaryOp::Mul, var("key"), int(2), i64_ty()),
                fired,
                i64_ty(),
            ))),
            mutability: Mutability::Immutable,
            span: Span::default(),
        })));
        let want = (from as i64 * STATE_STRIDE + TICK_EVENT_CODE as i64) * 2 + 1;
        body.push(stmt(TypedStatement::If(TypedIf {
            condition: Box::new(eq(var(&probe), int(want))),
            then_block: block(vec![assign_state(to)]),
            else_block: None,
            span: Span::default(),
        })));
    }
    body.push(stmt(TypedStatement::Yield(Box::new(var("state")))));

    // `while 1 == 1` — a machine waits for events for as long as it is
    // mounted, and the host drops the fiber to end it. A literal `true`
    // condition is not something the grammar produces, so the constant
    // comparison keeps the shape inside what the backends already see.
    let pump = stmt(TypedStatement::While(TypedWhile {
        condition: Box::new(eq(int(1), int(1))),
        body: block(body),
        span: Span::default(),
    }));

    let ret = stmt(TypedStatement::Return(Some(Box::new(var("state")))));

    TypedFunction {
        name: InternedString::new_global(&machine_fn_name(fsm)),
        effects: vec![InternedString::new_global(&events_effect_name(fsm))],
        return_type: i64_ty(),
        body: Some(block(vec![init, pump, ret])),
        is_fiber: true,
        ..Default::default()
    }
}
