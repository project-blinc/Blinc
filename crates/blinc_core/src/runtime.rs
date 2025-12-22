//! Blinc Runtime
//!
//! The main runtime that orchestrates all subsystems.

use crate::events::EventDispatcher;
use crate::fsm::FsmRuntime;
use crate::reactive::ReactiveGraph;

/// The Blinc reactive runtime - owns all reactive state, state machines, and event handling
pub struct BlincReactiveRuntime {
    pub reactive: ReactiveGraph,
    pub fsm: FsmRuntime,
    pub events: EventDispatcher,
}

impl BlincReactiveRuntime {
    pub fn new() -> Self {
        Self {
            reactive: ReactiveGraph::new(),
            fsm: FsmRuntime::new(),
            events: EventDispatcher::new(),
        }
    }

    /// Get statistics about the runtime
    pub fn stats(&self) -> RuntimeStats {
        RuntimeStats {
            reactive: self.reactive.stats(),
            fsm_count: self.fsm.len(),
        }
    }
}

impl Default for BlincReactiveRuntime {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about the runtime
#[derive(Debug, Clone)]
pub struct RuntimeStats {
    pub reactive: crate::reactive::ReactiveStats,
    pub fsm_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fsm::StateMachine;

    #[test]
    fn test_runtime_integration() {
        let mut runtime = BlincReactiveRuntime::new();

        // Create a signal
        let count = runtime.reactive.create_signal(0i32);

        // Create an FSM
        const IDLE: u32 = 0;
        const ACTIVE: u32 = 1;
        const CLICK: u32 = 1;

        let fsm = runtime.fsm.create(
            StateMachine::builder(IDLE)
                .on(IDLE, CLICK, ACTIVE)
                .on(ACTIVE, CLICK, IDLE)
                .build(),
        );

        // Verify initial state
        assert_eq!(runtime.reactive.get(count), Some(0));
        assert_eq!(runtime.fsm.current_state(fsm), Some(IDLE));

        // Make changes
        runtime.reactive.set(count, 42);
        runtime.fsm.send(fsm, CLICK);

        assert_eq!(runtime.reactive.get(count), Some(42));
        assert_eq!(runtime.fsm.current_state(fsm), Some(ACTIVE));

        // Check stats
        let stats = runtime.stats();
        assert_eq!(stats.reactive.signal_count, 1);
        assert_eq!(stats.fsm_count, 1);
    }
}
