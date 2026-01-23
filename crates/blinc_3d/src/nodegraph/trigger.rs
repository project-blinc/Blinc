//! Trigger system for node graph execution.
//!
//! When a node is triggered (marked with [`Triggered`]), the [`NodeGraphSystem`]
//! executes its [`OnTrigger`] action, passing a [`TriggerContext`] that provides
//! access to inputs, outputs, and the world.

use super::port::PortType;
use super::value::NodeValue;
use crate::ecs::{Component, Entity, World};
use rustc_hash::FxHashMap;
use std::sync::Arc;

/// Marker component that flags a node for execution this frame.
///
/// Add this component to a node entity to have it evaluated by the
/// [`NodeGraphSystem`]. The marker is automatically removed after execution.
///
/// # Example
///
/// ```rust,ignore
/// // Trigger a node for execution
/// world.insert(my_node, Triggered);
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct Triggered;

impl Component for Triggered {
    const STORAGE: crate::ecs::StorageType = crate::ecs::StorageType::Sparse;
}

/// Time information passed to triggered nodes.
#[derive(Debug, Clone, Copy)]
pub struct TimeContext {
    /// Time since last frame in seconds
    pub delta_time: f32,
    /// Total elapsed time since start in seconds
    pub elapsed_time: f32,
    /// Current frame number
    pub frame: u64,
}

impl Default for TimeContext {
    fn default() -> Self {
        Self {
            delta_time: 0.0,
            elapsed_time: 0.0,
            frame: 0,
        }
    }
}

/// Context passed to node trigger actions.
///
/// Provides access to the node's inputs, outputs, the world, and time info.
pub struct TriggerContext<'a> {
    /// The entity being triggered
    pub entity: Entity,
    /// Access to the ECS world
    pub world: &'a mut World,
    /// Input values received from connections
    pub inputs: &'a FxHashMap<String, NodeValue>,
    /// Output values to send to connected nodes
    pub outputs: &'a mut FxHashMap<String, NodeValue>,
    /// Time information
    pub time: TimeContext,
}

impl<'a> TriggerContext<'a> {
    /// Get a typed input value by name.
    ///
    /// Returns `None` if the input doesn't exist or can't be converted.
    pub fn input<T: PortType>(&self, name: &str) -> Option<T> {
        self.inputs.get(name).and_then(|v| T::from_value(v))
    }

    /// Get an input value with a default fallback.
    pub fn input_or<T: PortType>(&self, name: &str, default: T) -> T {
        self.input(name).unwrap_or(default)
    }

    /// Get a raw input NodeValue by name.
    pub fn input_raw(&self, name: &str) -> Option<&NodeValue> {
        self.inputs.get(name)
    }

    /// Check if an input exists and has a non-None value.
    pub fn has_input(&self, name: &str) -> bool {
        self.inputs
            .get(name)
            .is_some_and(|v| !matches!(v, NodeValue::None))
    }

    /// Set a typed output value.
    pub fn output<T: PortType>(&mut self, name: &str, value: T) {
        self.outputs.insert(name.to_string(), value.to_value());
    }

    /// Set a raw output NodeValue.
    pub fn output_raw(&mut self, name: &str, value: NodeValue) {
        self.outputs.insert(name.to_string(), value);
    }

    /// Get a component from this entity.
    pub fn get<C: Component>(&self) -> Option<&C> {
        self.world.get::<C>(self.entity)
    }

    /// Get a mutable component from this entity.
    ///
    /// Note: This requires dropping any existing borrows first.
    pub fn get_mut<C: Component>(&mut self) -> Option<&mut C> {
        self.world.get_mut::<C>(self.entity)
    }

    /// Mark another node for execution.
    ///
    /// The target node will be executed in a future evaluation pass.
    pub fn trigger(&mut self, target: Entity) {
        self.world.insert(target, Triggered);
    }

    /// Get delta time (time since last frame).
    #[inline]
    pub fn delta_time(&self) -> f32 {
        self.time.delta_time
    }

    /// Get elapsed time since start.
    #[inline]
    pub fn elapsed_time(&self) -> f32 {
        self.time.elapsed_time
    }

    /// Get current frame number.
    #[inline]
    pub fn frame(&self) -> u64 {
        self.time.frame
    }
}

/// Type alias for the closure type used in TriggerAction.
pub type TriggerClosure = Arc<dyn Fn(&mut TriggerContext) + Send + Sync>;

/// Defines what action to take when a node is triggered.
#[derive(Clone)]
pub enum TriggerAction {
    /// Execute a closure
    Closure(TriggerClosure),
    /// No action (pure data passthrough node)
    None,
}

impl std::fmt::Debug for TriggerAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TriggerAction::Closure(_) => write!(f, "TriggerAction::Closure(...)"),
            TriggerAction::None => write!(f, "TriggerAction::None"),
        }
    }
}

impl Default for TriggerAction {
    fn default() -> Self {
        TriggerAction::None
    }
}

/// Component that defines what happens when a node is triggered.
///
/// # Example
///
/// ```rust,ignore
/// use blinc_3d::nodegraph::{OnTrigger, TriggerContext};
///
/// // Create a trigger that doubles its input
/// let on_trigger = OnTrigger::run(|ctx: &mut TriggerContext| {
///     let value: f32 = ctx.input_or("value", 0.0);
///     ctx.output("result", value * 2.0);
/// });
///
/// world.spawn()
///     .insert(Node::new().with_input::<f32>("value").with_output::<f32>("result"))
///     .insert(on_trigger);
/// ```
#[derive(Debug, Clone)]
pub struct OnTrigger {
    /// The action to execute
    pub action: TriggerAction,
    /// Whether to propagate triggers to downstream nodes
    pub propagate: bool,
}

impl Component for OnTrigger {
    const STORAGE: crate::ecs::StorageType = crate::ecs::StorageType::Dense;
}

impl Default for OnTrigger {
    fn default() -> Self {
        Self {
            action: TriggerAction::None,
            propagate: true,
        }
    }
}

impl OnTrigger {
    /// Create a trigger that executes a closure.
    pub fn run<F>(f: F) -> Self
    where
        F: Fn(&mut TriggerContext) + Send + Sync + 'static,
    {
        Self {
            action: TriggerAction::Closure(Arc::new(f)),
            propagate: true,
        }
    }

    /// Create a passthrough trigger (no execution, just data flow).
    pub fn passthrough() -> Self {
        Self {
            action: TriggerAction::None,
            propagate: true,
        }
    }

    /// Set whether this trigger propagates to downstream nodes.
    pub fn with_propagation(mut self, propagate: bool) -> Self {
        self.propagate = propagate;
        self
    }

    /// Execute the trigger action.
    pub fn execute(&self, ctx: &mut TriggerContext) {
        match &self.action {
            TriggerAction::Closure(closure) => closure(ctx),
            TriggerAction::None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trigger_context_io() {
        let mut inputs = FxHashMap::default();
        inputs.insert("value".to_string(), NodeValue::Float(42.0));

        let mut outputs = FxHashMap::default();
        let mut world = World::new();

        let entity = world.spawn().id();

        let time = TimeContext::default();

        let mut ctx = TriggerContext {
            entity,
            world: &mut world,
            inputs: &inputs,
            outputs: &mut outputs,
            time,
        };

        assert_eq!(ctx.input::<f32>("value"), Some(42.0));
        assert_eq!(ctx.input_or::<f32>("missing", 0.0), 0.0);

        ctx.output("result", 84.0f32);
        assert!(outputs.contains_key("result"));
    }

    #[test]
    fn test_on_trigger_run() {
        let trigger = OnTrigger::run(|ctx| {
            let v: f32 = ctx.input_or("x", 0.0);
            ctx.output("y", v * 2.0);
        });

        assert!(matches!(trigger.action, TriggerAction::Closure(_)));
        assert!(trigger.propagate);
    }
}
