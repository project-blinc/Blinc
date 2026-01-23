//! # Node Graph System
//!
//! A visual/programmatic node-based system for connecting entities and controlling
//! data flow between them. Combines concepts from Blender's Geometry Nodes and
//! Unreal Engine's Blueprints.
//!
//! ## Core Concepts
//!
//! - **World IS the Graph**: The ECS World contains all nodes (entities) and connections
//! - **Entity = Node**: Any entity becomes a node by adding the [`Node`] component
//! - **Component-Driven Ports**: Ports are auto-discovered from entity components
//! - **System Execution**: When a node is triggered, its associated systems execute
//!
//! ## Quick Start
//!
//! ```rust,ignore
//! use blinc_3d::prelude::*;
//!
//! // Create entities as usual
//! let player = world.spawn()
//!     .insert(Object3D::default())
//!     .name("Player")
//!     .id();
//!
//! let camera = world.spawn()
//!     .insert(PerspectiveCamera::default())
//!     .name("Camera")
//!     .id();
//!
//! // Make them nodes (auto-discovers ports from components)
//! world.insert(player, Node::from_entity(player, &world));
//! world.insert(camera, Node::from_entity(camera, &world));
//!
//! // Connect them
//! world.spawn().insert(Connection::new(
//!     player, "position",
//!     camera, "target",
//! ));
//!
//! // Trigger execution
//! world.insert(player, Triggered);
//! ```

mod connection;
mod node;
mod port;
mod serial;
mod trigger;
mod value;

pub mod builtin;

// Re-exports
pub use builtin::{builtin_registry, BuiltinNode, NodeTypeRegistry};
pub use connection::Connection;
pub use node::{Node, Port};
pub use port::{PortDef, PortDirection, PortType, PortTypeId};
pub use serial::{NodeGraph, SerializedConnection, SerializedNode, SpawnResult};
pub use trigger::{OnTrigger, TriggerAction, TriggerContext, Triggered};
pub use value::NodeValue;

use crate::ecs::{Component, Entity, System, SystemContext, SystemStage, World};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

/// System that evaluates the node graph each frame.
///
/// This system:
/// 1. Collects all entities with the [`Triggered`] marker
/// 2. Topologically sorts them based on connections
/// 3. For each node: pulls inputs → executes trigger action → pushes outputs
/// 4. Clears the [`Triggered`] markers
pub struct NodeGraphSystem;

impl System for NodeGraphSystem {
    fn run(&mut self, ctx: &mut SystemContext) {
        // 1. Collect all triggered nodes
        let triggered: Vec<Entity> = ctx
            .world
            .query::<(&Node, &Triggered)>()
            .iter()
            .map(|(e, _)| e)
            .collect();

        if triggered.is_empty() {
            return;
        }

        // 2. Build dependency graph and topologically sort
        let sorted = topological_sort(&triggered, ctx.world);

        // 3. Execute each node in order
        for entity in sorted {
            execute_node(entity, ctx);
        }

        // 4. Clear triggered flags
        for entity in triggered {
            ctx.world.remove::<Triggered>(entity);
        }
    }

    fn name(&self) -> &'static str {
        "NodeGraphSystem"
    }

    fn stage(&self) -> SystemStage {
        SystemStage::Update
    }

    fn priority(&self) -> i32 {
        -100 // Run early in the update stage
    }
}

/// Topologically sort triggered nodes based on their connections.
fn topological_sort(triggered: &[Entity], world: &World) -> Vec<Entity> {
    // Build adjacency map: node -> nodes it depends on
    let mut dependencies: FxHashMap<Entity, SmallVec<[Entity; 4]>> = FxHashMap::default();
    let mut in_degree: FxHashMap<Entity, usize> = FxHashMap::default();

    // Initialize all triggered nodes
    for &entity in triggered {
        dependencies.entry(entity).or_default();
        in_degree.entry(entity).or_insert(0);
    }

    // Find all connections and build dependency graph
    for (_, (conn,)) in world.query::<(&Connection,)>().iter() {
        // If 'to' depends on 'from', and both are triggered
        if triggered.contains(&conn.from) && triggered.contains(&conn.to) {
            dependencies.entry(conn.to).or_default().push(conn.from);
            *in_degree.entry(conn.to).or_insert(0) += 1;
        }
    }

    // Kahn's algorithm for topological sort
    let mut result = Vec::with_capacity(triggered.len());
    let mut queue: Vec<Entity> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(&e, _)| e)
        .collect();

    while let Some(entity) = queue.pop() {
        result.push(entity);

        // Find nodes that depend on this one
        for (_, (conn,)) in world.query::<(&Connection,)>().iter() {
            if conn.from == entity && triggered.contains(&conn.to) {
                if let Some(deg) = in_degree.get_mut(&conn.to) {
                    *deg = deg.saturating_sub(1);
                    if *deg == 0 {
                        queue.push(conn.to);
                    }
                }
            }
        }
    }

    // If we couldn't sort all nodes, there's a cycle - just return them in order
    if result.len() < triggered.len() {
        tracing::warn!(
            "Cycle detected in node graph, some nodes may not execute in correct order"
        );
        for &entity in triggered {
            if !result.contains(&entity) {
                result.push(entity);
            }
        }
    }

    result
}

/// Execute a single node: pull inputs, run trigger, push outputs.
fn execute_node(entity: Entity, ctx: &mut SystemContext) {
    // Pull input values from connected nodes
    pull_inputs(entity, ctx.world);

    // Execute the node's trigger action
    if let Some(on_trigger) = ctx.world.get::<OnTrigger>(entity) {
        let action = on_trigger.action.clone();
        drop(on_trigger);

        // Build trigger context
        let inputs = ctx
            .world
            .get::<Node>(entity)
            .map(|n| n.input_values())
            .unwrap_or_default();

        let mut outputs = FxHashMap::default();

        let time_ctx = trigger::TimeContext {
            delta_time: ctx.delta_time,
            elapsed_time: ctx.elapsed_time,
            frame: ctx.frame,
        };

        // Execute action
        match action {
            TriggerAction::Closure(mut closure) => {
                let mut trigger_ctx = TriggerContext {
                    entity,
                    world: ctx.world,
                    inputs: &inputs,
                    outputs: &mut outputs,
                    time: time_ctx,
                };
                closure(&mut trigger_ctx);
            }
            TriggerAction::None => {}
        }

        // Store outputs back to node
        if let Some(node) = ctx.world.get_mut::<Node>(entity) {
            for (name, value) in outputs {
                node.set_output(&name, value);
            }
        }
    }

    // Push outputs to connected nodes
    push_outputs(entity, ctx.world);
}

/// Pull input values from connected source nodes.
fn pull_inputs(entity: Entity, world: &mut World) {
    // Find all connections where this entity is the target
    let connections: Vec<(String, Entity, String)> = world
        .query::<(&Connection,)>()
        .iter()
        .filter(|(_, (conn,))| conn.to == entity)
        .map(|(_, (conn,))| (conn.to_port.clone(), conn.from, conn.from_port.clone()))
        .collect();

    // For each connection, get the source value and set it as input
    for (to_port, from_entity, from_port) in connections {
        if let Some(source_node) = world.get::<Node>(from_entity) {
            if let Some(value) = source_node.get_output(&from_port) {
                drop(source_node);
                if let Some(target_node) = world.get_mut::<Node>(entity) {
                    target_node.set_input(&to_port, value);
                }
            }
        }
    }
}

/// Push output values to connected target nodes.
fn push_outputs(entity: Entity, world: &mut World) {
    // Find all connections where this entity is the source
    let connections: Vec<(String, Entity, String)> = world
        .query::<(&Connection,)>()
        .iter()
        .filter(|(_, (conn,))| conn.from == entity)
        .map(|(_, (conn,))| (conn.from_port.clone(), conn.to, conn.to_port.clone()))
        .collect();

    // For each connection, get our output value and set it as their input
    for (from_port, to_entity, to_port) in connections {
        if let Some(source_node) = world.get::<Node>(entity) {
            if let Some(value) = source_node.get_output(&from_port) {
                drop(source_node);
                if let Some(target_node) = world.get_mut::<Node>(to_entity) {
                    target_node.set_input(&to_port, value);
                }
            }
        }
    }
}

/// Trait for components that expose ports to the node graph.
///
/// Components can implement this trait to define which of their fields
/// should be exposed as input/output ports when the entity becomes a node.
///
/// # Example
///
/// ```rust,ignore
/// use blinc_3d::nodegraph::{NodePorts, PortDef, PortDirection, PortTypeId};
///
/// impl NodePorts for Object3D {
///     fn ports() -> Vec<PortDef> {
///         vec![
///             PortDef::new("position", PortDirection::Both, PortTypeId::Vec3),
///             PortDef::new("rotation", PortDirection::Both, PortTypeId::Quat),
///             PortDef::new("scale", PortDirection::Both, PortTypeId::Vec3),
///         ]
///     }
/// }
/// ```
pub trait NodePorts {
    /// Return port definitions for this component type.
    fn ports() -> Vec<PortDef>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_creation() {
        let node = Node::new();
        assert!(node.inputs().count() == 0);
        assert!(node.outputs().count() == 0);
    }

    #[test]
    fn test_node_with_ports() {
        let node = Node::new()
            .with_input::<f32>("value")
            .with_output::<f32>("result");

        assert_eq!(node.inputs().count(), 1);
        assert_eq!(node.outputs().count(), 1);
    }

    #[test]
    fn test_connection_creation() {
        let conn = Connection::new(Entity::default(), "out", Entity::default(), "in");
        assert_eq!(conn.from_port, "out");
        assert_eq!(conn.to_port, "in");
    }

    #[test]
    fn test_data_flow_through_connections() {
        let mut world = World::new();

        // Create a source node that outputs a constant value
        let source = world
            .spawn()
            .insert(
                Node::new()
                    .with_output::<f32>("out")
                    .with_name("Source"),
            )
            .insert(OnTrigger::run(|ctx| {
                ctx.output("out", 42.0f32);
            }))
            .id();

        // Create a sink node that receives the value
        let sink = world
            .spawn()
            .insert(
                Node::new()
                    .with_input::<f32>("in")
                    .with_output::<f32>("result")
                    .with_name("Sink"),
            )
            .insert(OnTrigger::run(|ctx| {
                let input: f32 = ctx.input_or("in", 0.0);
                ctx.output("result", input * 2.0);
            }))
            .id();

        // Connect source -> sink
        world
            .spawn()
            .insert(Connection::new(source, "out", sink, "in"));

        // Trigger both nodes
        world.insert(source, Triggered);
        world.insert(sink, Triggered);

        // Run the node graph system
        let mut system = NodeGraphSystem;
        let mut ctx = crate::ecs::SystemContext {
            world: &mut world,
            delta_time: 0.016,
            elapsed_time: 0.0,
            frame: 0,
        };
        system.run(&mut ctx);

        // Verify the sink received the value and processed it
        let sink_node = world.get::<Node>(sink).unwrap();
        let result = sink_node.get_output_as::<f32>("result");
        assert_eq!(result, Some(84.0), "Expected 42.0 * 2.0 = 84.0");
    }

    #[test]
    fn test_chained_node_execution() {
        use crate::nodegraph::builtin::AddNode;

        let mut world = World::new();

        // Create a chain: Constant(10) -> Add(+5) -> Add(+3) -> Result should be 18

        // First node: outputs constant 10
        let const_node = world
            .spawn()
            .insert(Node::new().with_output::<f32>("out"))
            .insert(OnTrigger::run(|ctx| {
                ctx.output("out", 10.0f32);
            }))
            .id();

        // Second node: adds 5 to input
        let add1 = world
            .spawn()
            .insert(
                Node::new()
                    .with_input::<f32>("a")
                    .with_input::<f32>("b")
                    .with_output::<f32>("result"),
            )
            .insert(OnTrigger::run(|ctx| {
                let a: f32 = ctx.input_or("a", 0.0);
                let b: f32 = ctx.input_or("b", 0.0);
                ctx.output("result", a + b);
            }))
            .id();

        // Set b=5 as constant on add1
        if let Some(node) = world.get_mut::<Node>(add1) {
            node.set_input("b", NodeValue::Float(5.0));
        }

        // Third node: adds 3 to input
        let add2 = world
            .spawn()
            .insert(
                Node::new()
                    .with_input::<f32>("a")
                    .with_input::<f32>("b")
                    .with_output::<f32>("result"),
            )
            .insert(OnTrigger::run(|ctx| {
                let a: f32 = ctx.input_or("a", 0.0);
                let b: f32 = ctx.input_or("b", 0.0);
                ctx.output("result", a + b);
            }))
            .id();

        // Set b=3 as constant on add2
        if let Some(node) = world.get_mut::<Node>(add2) {
            node.set_input("b", NodeValue::Float(3.0));
        }

        // Connect: const_node.out -> add1.a, add1.result -> add2.a
        world
            .spawn()
            .insert(Connection::new(const_node, "out", add1, "a"));
        world
            .spawn()
            .insert(Connection::new(add1, "result", add2, "a"));

        // Trigger all nodes
        world.insert(const_node, Triggered);
        world.insert(add1, Triggered);
        world.insert(add2, Triggered);

        // Run the node graph system
        let mut system = NodeGraphSystem;
        let mut ctx = crate::ecs::SystemContext {
            world: &mut world,
            delta_time: 0.016,
            elapsed_time: 0.0,
            frame: 0,
        };
        system.run(&mut ctx);

        // Verify the final result: 10 + 5 + 3 = 18
        let result_node = world.get::<Node>(add2).unwrap();
        let result = result_node.get_output_as::<f32>("result");
        assert_eq!(result, Some(18.0), "Expected 10 + 5 + 3 = 18");
    }

    #[test]
    fn test_builtin_nodes_with_connections() {
        use crate::nodegraph::builtin::{AddNode, MultiplyNode, BuiltinNode};

        let mut world = World::new();

        // Create: (5 + 3) * 2 = 16
        let add = AddNode::spawn(&mut world);
        let mul = MultiplyNode::spawn(&mut world);

        // Set inputs
        if let Some(node) = world.get_mut::<Node>(add) {
            node.set_input("a", NodeValue::Float(5.0));
            node.set_input("b", NodeValue::Float(3.0));
        }
        if let Some(node) = world.get_mut::<Node>(mul) {
            node.set_input("b", NodeValue::Float(2.0));
        }

        // Connect add.result -> mul.a
        world
            .spawn()
            .insert(Connection::new(add, "result", mul, "a"));

        // Trigger both
        world.insert(add, Triggered);
        world.insert(mul, Triggered);

        // Run
        let mut system = NodeGraphSystem;
        let mut ctx = crate::ecs::SystemContext {
            world: &mut world,
            delta_time: 0.016,
            elapsed_time: 0.0,
            frame: 0,
        };
        system.run(&mut ctx);

        // Verify: (5 + 3) * 2 = 16
        let result_node = world.get::<Node>(mul).unwrap();
        let result = result_node.get_output_as::<f32>("result");
        assert_eq!(result, Some(16.0), "Expected (5 + 3) * 2 = 16");
    }

    #[test]
    fn test_graph_serialization_and_execution() {
        use crate::nodegraph::builtin::AddNode;

        // Create a graph programmatically
        let mut graph = NodeGraph::with_name("Test Execution Graph");
        let add1 = graph.add_builtin::<AddNode>();
        let add2 = graph.add_builtin::<AddNode>();
        graph.connect(add1, "result", add2, "a");

        // Serialize and deserialize
        let json = graph.to_json().unwrap();
        let restored = NodeGraph::from_json(&json).unwrap();

        // Spawn with builtins
        let mut world = World::new();
        let result = restored.spawn_with_builtins(&mut world);

        // Set inputs: add1(10 + 5) -> add2(? + 2) = 17
        let add1_entity = result.node_entities[&add1];
        let add2_entity = result.node_entities[&add2];

        if let Some(node) = world.get_mut::<Node>(add1_entity) {
            node.set_input("a", NodeValue::Float(10.0));
            node.set_input("b", NodeValue::Float(5.0));
        }
        if let Some(node) = world.get_mut::<Node>(add2_entity) {
            node.set_input("b", NodeValue::Float(2.0));
        }

        // Trigger both
        world.insert(add1_entity, Triggered);
        world.insert(add2_entity, Triggered);

        // Run
        let mut system = NodeGraphSystem;
        let mut ctx = crate::ecs::SystemContext {
            world: &mut world,
            delta_time: 0.016,
            elapsed_time: 0.0,
            frame: 0,
        };
        system.run(&mut ctx);

        // Verify: (10 + 5) + 2 = 17
        let final_node = world.get::<Node>(add2_entity).unwrap();
        let final_result = final_node.get_output_as::<f32>("result");
        assert_eq!(final_result, Some(17.0), "Expected (10 + 5) + 2 = 17");
    }
}
