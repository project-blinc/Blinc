//! Runtime-agnostic FSM substrate.
//!
//! This module holds the shared data structures and dispatch
//! plumbing that lets DSL-side finite state machines plug into
//! widget-side `Stateful<S>` machinery, regardless of whether
//! the DSL was JIT-compiled at app runtime (`blinc_dsl_core` via
//! Zyntax/Cranelift) or AOT-compiled into the binary (`blinc_dsl_aot`
//! via Zyntax/LLVM — future).
//!
//! ## Architecture
//!
//! ```text
//! DSL source (.blinc)
//!         │
//!         ▼
//!    Zyntax IR
//!        / \
//!       /   \
//!   JIT     AOT
//!  (Cran)   (LLVM)
//!     \      /
//!      \    /
//!  blinc_runtime::fsm
//!  (this crate — runtime-agnostic registry + dispatch)
//!         │
//!         ▼
//!  blinc_layout::Stateful<FsmStateId>
//!  (widget integration via StateTransitions)
//! ```
//!
//! Both compile paths produce the same shape:
//! - A set of callable symbols (`Counter$view`, lifted guards
//!   `__fsm_tick_guard_<FsmName>_<idx>__`, etc.) that the linker /
//!   JIT resolves.
//! - A pure-data FSM definition (state names, transitions, guard
//!   symbol names) that gets published into this module's
//!   [`FsmRegistry`] singleton at startup.
//!
//! The widget layer reads from the registry through [`FsmStateId`],
//! a `Copy + StateTransitions` newtype keyed by [`FsmId`] and a
//! state-variant code. Event transitions resolve in pure Rust via
//! HashMap lookup; tick guards route through a
//! [`GuardDispatcher`] trait that backends implement to call the
//! lifted guard function (Cranelift `call_function` for JIT, plain
//! function-pointer call for AOT).
//!
//! ## Module layout
//!
//! - [`registry`] — `FsmId`, `FsmDefinition`, `FsmRegistry`,
//!   process-wide accessors.
//! - [`dispatch`] — `GuardDispatcher` trait + the
//!   process-wide dispatcher slot.
//! - [`instance`] — `FsmStateId` newtype that implements
//!   `StateTransitions` for `Stateful<FsmStateId>`.

pub mod default_instance;
pub mod dispatch;
pub mod instance;
pub mod registry;

pub use default_instance::{
    TransitionEffect, TransitionSubscriber, TransitionSubscriberPathed, current_state_code,
    current_state_name, default_state, dispatch_default, register_subscriber,
    register_subscriber_all, register_transition_effect, reset_default,
};
pub use dispatch::{
    GuardDispatcher, MachineDriver, clear_guard_dispatcher, clear_machine_driver, machine_driver,
    set_guard_dispatcher, set_machine_driver,
};
pub use instance::FsmStateId;
pub use registry::{
    EventTransition, FSM_EVENT_CODE_OFFSET, FsmDefinition, FsmId, FsmRegistry, TickGuard,
    TransitionAction, with_fsm_registry, with_fsm_registry_mut,
};
