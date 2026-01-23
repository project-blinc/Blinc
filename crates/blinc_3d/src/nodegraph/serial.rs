//! Graph serialization for the node graph system.
//!
//! Provides serialization and deserialization of complete node graphs,
//! allowing graphs to be saved to and loaded from JSON.
//!
//! # Example
//!
//! ```rust,ignore
//! use blinc_3d::nodegraph::{NodeGraph, Node, Connection};
//!
//! // Save current graph from world
//! let graph = NodeGraph::from_world(&world);
//! let json = serde_json::to_string_pretty(&graph)?;
//!
//! // Load graph into world
//! let graph: NodeGraph = serde_json::from_str(&json)?;
//! let entity_map = graph.spawn_into(&mut world);
//! ```

use super::builtin::{builtin_registry, NodeTypeRegistry};
use super::{Connection, Node, OnTrigger};
use crate::ecs::{Entity, World};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

/// A serialized node in the graph.
///
/// Contains the node data and a local ID for reference within the graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedNode {
    /// Local ID within this graph (used for connection references)
    pub id: u32,
    /// The node component data
    pub node: Node,
    /// Optional node type identifier (e.g., "builtin.add", "custom.my_node")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_type: Option<String>,
}

/// A serialized connection between nodes.
///
/// References nodes by their local IDs within the graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedConnection {
    /// Local ID of the source node
    pub from_node: u32,
    /// Name of the output port on the source node
    pub from_port: String,
    /// Local ID of the target node
    pub to_node: u32,
    /// Name of the input port on the target node
    pub to_port: String,
    /// Whether this connection is enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

/// A complete serialized node graph.
///
/// Contains all nodes and connections in a format that can be saved/loaded
/// independently of the ECS World.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NodeGraph {
    /// Graph name/identifier
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Version for compatibility checking
    #[serde(default = "default_version")]
    pub version: u32,
    /// All nodes in the graph
    pub nodes: Vec<SerializedNode>,
    /// All connections between nodes
    pub connections: Vec<SerializedConnection>,
}

fn default_version() -> u32 {
    1
}

/// Result of spawning a graph into a World.
///
/// Contains the mapping from local node IDs to spawned Entity IDs.
#[derive(Debug, Clone)]
pub struct SpawnResult {
    /// Maps local node IDs to their spawned Entity IDs
    pub node_entities: FxHashMap<u32, Entity>,
    /// Entity IDs of spawned connection entities
    pub connection_entities: Vec<Entity>,
}

impl NodeGraph {
    /// Create a new empty graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a graph with a name.
    pub fn with_name(name: impl Into<String>) -> Self {
        Self {
            name: Some(name.into()),
            ..Default::default()
        }
    }

    /// Extract the current node graph from a World.
    ///
    /// This collects all entities with Node components and all Connection
    /// components, converting them to a serializable format.
    pub fn from_world(world: &World) -> Self {
        let mut graph = Self::new();
        let mut entity_to_local: FxHashMap<Entity, u32> = FxHashMap::default();
        let mut next_id = 0u32;

        // Collect all nodes
        for (entity, (node,)) in world.query::<(&Node,)>().iter() {
            let local_id = next_id;
            next_id += 1;
            entity_to_local.insert(entity, local_id);

            graph.nodes.push(SerializedNode {
                id: local_id,
                node: node.clone(),
                node_type: None, // Could be extended to store type info
            });
        }

        // Collect all connections (only those between nodes we found)
        for (_, (conn,)) in world.query::<(&Connection,)>().iter() {
            if let (Some(&from_id), Some(&to_id)) = (
                entity_to_local.get(&conn.from),
                entity_to_local.get(&conn.to),
            ) {
                graph.connections.push(SerializedConnection {
                    from_node: from_id,
                    from_port: conn.from_port.clone(),
                    to_node: to_id,
                    to_port: conn.to_port.clone(),
                    enabled: conn.enabled,
                });
            }
        }

        graph
    }

    /// Spawn this graph into a World.
    ///
    /// Creates new entities for each node and connection. Returns a mapping
    /// from local node IDs to the spawned Entity IDs.
    ///
    /// Note: This only spawns Node components. If nodes need OnTrigger
    /// components for execution, use `spawn_with_triggers` instead.
    pub fn spawn_into(&self, world: &mut World) -> SpawnResult {
        let mut node_entities: FxHashMap<u32, Entity> = FxHashMap::default();
        let mut connection_entities = Vec::new();

        // Spawn all nodes
        for serialized_node in &self.nodes {
            let entity = world.spawn().insert(serialized_node.node.clone()).id();
            node_entities.insert(serialized_node.id, entity);
        }

        // Spawn all connections
        for serialized_conn in &self.connections {
            if let (Some(&from_entity), Some(&to_entity)) = (
                node_entities.get(&serialized_conn.from_node),
                node_entities.get(&serialized_conn.to_node),
            ) {
                let mut conn = Connection::new(
                    from_entity,
                    &serialized_conn.from_port,
                    to_entity,
                    &serialized_conn.to_port,
                );
                conn.enabled = serialized_conn.enabled;

                let conn_entity = world.spawn().insert(conn).id();
                connection_entities.push(conn_entity);
            }
        }

        SpawnResult {
            node_entities,
            connection_entities,
        }
    }

    /// Spawn this graph into a World with a trigger factory.
    ///
    /// The `trigger_factory` function is called for each node to create its
    /// OnTrigger component based on the node type.
    pub fn spawn_with_triggers<F>(&self, world: &mut World, trigger_factory: F) -> SpawnResult
    where
        F: Fn(&SerializedNode) -> Option<OnTrigger>,
    {
        let mut node_entities: FxHashMap<u32, Entity> = FxHashMap::default();
        let mut connection_entities = Vec::new();

        // Spawn all nodes with triggers
        for serialized_node in &self.nodes {
            let mut builder = world.spawn();
            builder = builder.insert(serialized_node.node.clone());

            if let Some(trigger) = trigger_factory(serialized_node) {
                builder = builder.insert(trigger);
            }

            let entity = builder.id();
            node_entities.insert(serialized_node.id, entity);
        }

        // Spawn all connections
        for serialized_conn in &self.connections {
            if let (Some(&from_entity), Some(&to_entity)) = (
                node_entities.get(&serialized_conn.from_node),
                node_entities.get(&serialized_conn.to_node),
            ) {
                let mut conn = Connection::new(
                    from_entity,
                    &serialized_conn.from_port,
                    to_entity,
                    &serialized_conn.to_port,
                );
                conn.enabled = serialized_conn.enabled;

                let conn_entity = world.spawn().insert(conn).id();
                connection_entities.push(conn_entity);
            }
        }

        SpawnResult {
            node_entities,
            connection_entities,
        }
    }

    /// Spawn this graph into a World, automatically restoring triggers for built-in nodes.
    ///
    /// Built-in node types (those with `node_type` starting with "builtin.") are
    /// automatically recognized and their OnTrigger components restored from the
    /// global registry.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let graph = NodeGraph::from_json(&json)?;
    /// let result = graph.spawn_with_builtins(&mut world);
    /// // All builtin.add, builtin.multiply, etc. nodes automatically have their triggers
    /// ```
    pub fn spawn_with_builtins(&self, world: &mut World) -> SpawnResult {
        self.spawn_with_registry(world, builtin_registry())
    }

    /// Spawn this graph into a World using a custom registry for trigger restoration.
    ///
    /// Nodes with a `node_type` that exists in the registry will have their
    /// OnTrigger components automatically created.
    pub fn spawn_with_registry(
        &self,
        world: &mut World,
        registry: &NodeTypeRegistry,
    ) -> SpawnResult {
        let mut node_entities: FxHashMap<u32, Entity> = FxHashMap::default();
        let mut connection_entities = Vec::new();

        // Spawn all nodes with triggers from registry
        for serialized_node in &self.nodes {
            let mut builder = world.spawn();
            builder = builder.insert(serialized_node.node.clone());

            // Look up trigger in registry if node_type is set
            if let Some(ref node_type) = serialized_node.node_type {
                if let Some(trigger) = registry.create_trigger(node_type) {
                    builder = builder.insert(trigger);
                }
            }

            let entity = builder.id();
            node_entities.insert(serialized_node.id, entity);
        }

        // Spawn all connections
        for serialized_conn in &self.connections {
            if let (Some(&from_entity), Some(&to_entity)) = (
                node_entities.get(&serialized_conn.from_node),
                node_entities.get(&serialized_conn.to_node),
            ) {
                let mut conn = Connection::new(
                    from_entity,
                    &serialized_conn.from_port,
                    to_entity,
                    &serialized_conn.to_port,
                );
                conn.enabled = serialized_conn.enabled;

                let conn_entity = world.spawn().insert(conn).id();
                connection_entities.push(conn_entity);
            }
        }

        SpawnResult {
            node_entities,
            connection_entities,
        }
    }

    /// Add a node to the graph.
    ///
    /// Returns the local ID assigned to the node.
    pub fn add_node(&mut self, node: Node) -> u32 {
        let id = self.nodes.len() as u32;
        self.nodes.push(SerializedNode {
            id,
            node,
            node_type: None,
        });
        id
    }

    /// Add a node with a type identifier.
    pub fn add_typed_node(&mut self, node: Node, node_type: impl Into<String>) -> u32 {
        let id = self.nodes.len() as u32;
        self.nodes.push(SerializedNode {
            id,
            node,
            node_type: Some(node_type.into()),
        });
        id
    }

    /// Add a built-in node to the graph.
    ///
    /// Automatically creates the node with proper ports and sets the node_type
    /// for trigger restoration during deserialization.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use blinc_3d::nodegraph::{NodeGraph, builtin::AddNode};
    ///
    /// let mut graph = NodeGraph::new();
    /// let add_id = graph.add_builtin::<AddNode>();
    /// ```
    pub fn add_builtin<T: super::builtin::BuiltinNode>(&mut self) -> u32 {
        let node = T::create_node().with_name(T::display_name());
        self.add_typed_node(node, T::type_id())
    }

    /// Add a connection between two nodes.
    pub fn connect(
        &mut self,
        from_node: u32,
        from_port: impl Into<String>,
        to_node: u32,
        to_port: impl Into<String>,
    ) {
        self.connections.push(SerializedConnection {
            from_node,
            from_port: from_port.into(),
            to_node,
            to_port: to_port.into(),
            enabled: true,
        });
    }

    /// Get a node by its local ID.
    pub fn get_node(&self, id: u32) -> Option<&SerializedNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// Get a mutable node by its local ID.
    pub fn get_node_mut(&mut self, id: u32) -> Option<&mut SerializedNode> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }

    /// Get all connections from a specific node.
    pub fn connections_from(&self, node_id: u32) -> impl Iterator<Item = &SerializedConnection> {
        self.connections
            .iter()
            .filter(move |c| c.from_node == node_id)
    }

    /// Get all connections to a specific node.
    pub fn connections_to(&self, node_id: u32) -> impl Iterator<Item = &SerializedConnection> {
        self.connections.iter().filter(move |c| c.to_node == node_id)
    }

    /// Serialize the graph to JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Serialize the graph to pretty-printed JSON.
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Deserialize a graph from JSON.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nodegraph::port::PortType;
    use blinc_core::Vec2;

    #[test]
    fn test_graph_creation() {
        let mut graph = NodeGraph::with_name("Test Graph");

        let node_a = graph.add_node(
            Node::new()
                .with_input::<f32>("value")
                .with_output::<f32>("result")
                .with_name("Node A"),
        );

        let node_b = graph.add_node(
            Node::new()
                .with_input::<f32>("input")
                .with_output::<f32>("output")
                .with_name("Node B"),
        );

        graph.connect(node_a, "result", node_b, "input");

        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(graph.connections.len(), 1);
    }

    #[test]
    fn test_graph_serialization() {
        let mut graph = NodeGraph::with_name("Serialization Test");

        let node_a = graph.add_node(
            Node::new()
                .with_output::<f32>("out")
                .with_name("Source")
                .with_position(Vec2::new(100.0, 100.0)),
        );

        let node_b = graph.add_node(
            Node::new()
                .with_input::<f32>("in")
                .with_name("Sink")
                .with_position(Vec2::new(300.0, 100.0)),
        );

        graph.connect(node_a, "out", node_b, "in");

        // Serialize
        let json = graph.to_json_pretty().unwrap();
        assert!(json.contains("Serialization Test"));
        assert!(json.contains("Source"));
        assert!(json.contains("Sink"));

        // Deserialize
        let restored = NodeGraph::from_json(&json).unwrap();
        assert_eq!(restored.name, Some("Serialization Test".to_string()));
        assert_eq!(restored.nodes.len(), 2);
        assert_eq!(restored.connections.len(), 1);
    }

    #[test]
    fn test_spawn_into_world() {
        let mut graph = NodeGraph::new();

        let node_a = graph.add_node(Node::new().with_output::<f32>("out"));
        let node_b = graph.add_node(Node::new().with_input::<f32>("in"));
        graph.connect(node_a, "out", node_b, "in");

        let mut world = World::new();
        let result = graph.spawn_into(&mut world);

        assert_eq!(result.node_entities.len(), 2);
        assert_eq!(result.connection_entities.len(), 1);

        // Verify nodes exist
        for (_, &entity) in &result.node_entities {
            assert!(world.get::<Node>(entity).is_some());
        }

        // Verify connection exists and references correct entities
        let conn_entity = result.connection_entities[0];
        let conn = world.get::<Connection>(conn_entity).unwrap();
        assert_eq!(conn.from, result.node_entities[&node_a]);
        assert_eq!(conn.to, result.node_entities[&node_b]);
    }

    #[test]
    fn test_from_world_roundtrip() {
        // Create initial world with nodes and connections
        let mut world = World::new();

        let entity_a = world
            .spawn()
            .insert(Node::new().with_output::<f32>("out").with_name("A"))
            .id();

        let entity_b = world
            .spawn()
            .insert(Node::new().with_input::<f32>("in").with_name("B"))
            .id();

        world
            .spawn()
            .insert(Connection::new(entity_a, "out", entity_b, "in"));

        // Extract graph
        let graph = NodeGraph::from_world(&world);
        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(graph.connections.len(), 1);

        // Serialize and deserialize
        let json = graph.to_json().unwrap();
        let restored = NodeGraph::from_json(&json).unwrap();

        // Spawn into new world
        let mut new_world = World::new();
        let result = restored.spawn_into(&mut new_world);

        assert_eq!(result.node_entities.len(), 2);
        assert_eq!(result.connection_entities.len(), 1);
    }

    #[test]
    fn test_builtin_node_registry() {
        use crate::nodegraph::builtin::{builtin_registry, AddNode, BuiltinNode};

        // Verify registry contains builtin nodes
        let registry = builtin_registry();
        assert!(registry.contains("builtin.add"));
        assert!(registry.contains("builtin.multiply"));
        assert!(registry.contains("builtin.vec3_add"));

        // Verify we can create a trigger
        let trigger = registry.create_trigger("builtin.add");
        assert!(trigger.is_some());

        // Verify type_id matches
        assert_eq!(AddNode::type_id(), "builtin.add");
    }

    #[test]
    fn test_spawn_with_builtins() {
        use crate::nodegraph::builtin::AddNode;
        use crate::nodegraph::OnTrigger;

        // Create a graph with builtin nodes
        let mut graph = NodeGraph::new();
        let add_id = graph.add_builtin::<AddNode>();

        assert_eq!(graph.nodes.len(), 1);
        assert_eq!(
            graph.nodes[0].node_type,
            Some("builtin.add".to_string())
        );

        // Serialize and deserialize
        let json = graph.to_json().unwrap();
        let restored = NodeGraph::from_json(&json).unwrap();

        // Spawn with builtins - should automatically restore triggers
        let mut world = World::new();
        let result = restored.spawn_with_builtins(&mut world);

        assert_eq!(result.node_entities.len(), 1);

        // Verify OnTrigger was restored
        let entity = result.node_entities[&add_id];
        let trigger = world.get::<OnTrigger>(entity);
        assert!(trigger.is_some());
    }
}
