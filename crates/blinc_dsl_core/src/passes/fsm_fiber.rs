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
//! Scope: state only. Context fields stay as module-level signals, since
//! widgets read them through a signal id. Tick guards are not lowered —
//! a guard is a lifted function the host calls, with no route into the
//! fiber until guards arrive through the same effect.
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

/// Handler installed around a step, answering from the host's armed
/// event slot.
pub(crate) fn host_events_handler_name(fsm: &str) -> String {
    format!("{fsm}$HostEvents")
}

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
            events_effect(fsm),
            host_events_handler(fsm),
            machine_fn(fsm, def, &states),
        ));
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
}

/// `handler <Fsm>$HostEvents for <Fsm>$Events {
///      def <Fsm>$next_event(): i64 { return __blinc_fsm_next_event() }
///  }`
///
/// Stateless: the event belongs to the resume, not to the handler, so
/// there is nothing to carry between steps.
fn host_events_handler(fsm: &str) -> TypedEffectHandler {
    let call = TypedNode::new(
        TypedExpression::Call(TypedCall {
            callee: Box::new(var(NEXT_EVENT_EXTERN)),
            positional_args: Vec::new(),
            named_args: Vec::new(),
            type_args: Vec::new(),
        }),
        i64_ty(),
        Span::default(),
    );
    TypedEffectHandler {
        name: InternedString::new_global(&host_events_handler_name(fsm)),
        effect_name: InternedString::new_global(&events_effect_name(fsm)),
        handlers: vec![TypedEffectHandlerImpl {
            op_name: InternedString::new_global(&next_event_op_name(fsm)),
            return_type: i64_ty(),
            body: Some(block(vec![stmt(TypedStatement::Return(Some(Box::new(
                call,
            ))))])),
            ..Default::default()
        }],
        ..Default::default()
    }
}

fn events_effect(fsm: &str) -> TypedEffect {
    TypedEffect {
        name: InternedString::new_global(&events_effect_name(fsm)),
        operations: vec![TypedEffectOp {
            name: InternedString::new_global(&next_event_op_name(fsm)),
            return_type: i64_ty(),
            ..Default::default()
        }],
        ..Default::default()
    }
}

fn machine_fn(fsm: &str, def: &FsmDefinition, states: &[String]) -> TypedFunction {
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
