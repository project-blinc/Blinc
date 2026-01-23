//! Node component for the node graph system.
//!
//! The [`Node`] component marks an entity as a participant in the node graph.
//! When added to an entity, it stores the ports discovered from the entity's
//! components and manages the flow of values through those ports.

use super::port::{PortDef, PortDirection, PortType, PortTypeId};
use super::value::NodeValue;
use crate::ecs::Component;
use blinc_core::Vec2;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use std::any::TypeId;

/// A port on a node, storing both definition and current value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Port {
    /// Port name (unique within the node)
    pub name: String,
    /// Port direction
    pub direction: PortDirection,
    /// Data type of the port
    pub value_type: PortTypeId,
    /// Current value
    value: NodeValue,
    /// Component TypeId this port maps to (for auto-sync)
    /// Skipped during serialization as TypeId is runtime-specific
    #[serde(skip)]
    pub component_type: Option<TypeId>,
    /// Field path within component (e.g., "position.x")
    pub field_path: Option<String>,
}

impl Port {
    /// Create a new port from a definition.
    pub fn from_def(def: &PortDef) -> Self {
        Self {
            name: def.name.clone(),
            direction: def.direction,
            value_type: def.value_type,
            value: def.default_value.clone().unwrap_or(NodeValue::None),
            component_type: None,
            field_path: None,
        }
    }

    /// Create a new input port.
    pub fn input<T: PortType>(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            direction: PortDirection::Input,
            value_type: T::port_type_id(),
            value: NodeValue::None,
            component_type: None,
            field_path: None,
        }
    }

    /// Create a new output port.
    pub fn output<T: PortType>(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            direction: PortDirection::Output,
            value_type: T::port_type_id(),
            value: NodeValue::None,
            component_type: None,
            field_path: None,
        }
    }

    /// Create a bidirectional port.
    pub fn both<T: PortType>(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            direction: PortDirection::Both,
            value_type: T::port_type_id(),
            value: NodeValue::None,
            component_type: None,
            field_path: None,
        }
    }

    /// Set the initial value.
    pub fn with_value(mut self, value: impl Into<NodeValue>) -> Self {
        self.value = value.into();
        self
    }

    /// Link this port to a component field for auto-sync.
    pub fn linked_to<C: 'static>(mut self, field_path: impl Into<String>) -> Self {
        self.component_type = Some(TypeId::of::<C>());
        self.field_path = Some(field_path.into());
        self
    }

    /// Get the current value.
    #[inline]
    pub fn value(&self) -> &NodeValue {
        &self.value
    }

    /// Set the value.
    #[inline]
    pub fn set_value(&mut self, value: NodeValue) {
        self.value = value;
    }

    /// Get the value as a specific type.
    pub fn get<T: PortType>(&self) -> Option<T> {
        T::from_value(&self.value)
    }

    /// Set the value from a typed value.
    pub fn set<T: PortType>(&mut self, value: T) {
        self.value = value.to_value();
    }

    /// Returns true if this port can receive connections.
    #[inline]
    pub fn is_input(&self) -> bool {
        matches!(self.direction, PortDirection::Input | PortDirection::Both)
    }

    /// Returns true if this port can send connections.
    #[inline]
    pub fn is_output(&self) -> bool {
        matches!(self.direction, PortDirection::Output | PortDirection::Both)
    }
}

/// Makes an entity participate in the node graph.
///
/// The Node component stores all ports (inputs and outputs) and manages
/// the flow of values between connected nodes.
///
/// # Example
///
/// ```rust,ignore
/// use blinc_3d::nodegraph::{Node, Port};
///
/// // Create a node with explicit ports
/// let node = Node::new()
///     .with_input::<f32>("value")
///     .with_output::<f32>("result");
///
/// // Or create from an entity (auto-discovers ports from components)
/// let node = Node::from_entity(entity, &world);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    /// All ports on this node
    ports: SmallVec<[Port; 8]>,
    /// Quick lookup for input values by name
    /// Skipped during serialization - rebuilt from ports
    #[serde(skip)]
    input_cache: FxHashMap<String, NodeValue>,
    /// Quick lookup for output values by name
    /// Skipped during serialization - rebuilt from ports
    #[serde(skip)]
    output_cache: FxHashMap<String, NodeValue>,
    /// Visual position in editor (optional)
    /// Serialized as [f32; 2] since Vec2 doesn't have serde derives
    #[serde(with = "option_vec2_serde")]
    pub position: Option<Vec2>,
    /// Collapsed state in editor
    pub collapsed: bool,
    /// Display name override
    pub display_name: Option<String>,
}

/// Custom serde module for Option<Vec2>
mod option_vec2_serde {
    use blinc_core::Vec2;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(value: &Option<Vec2>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            Some(v) => Some([v.x, v.y]).serialize(serializer),
            None => None::<[f32; 2]>.serialize(serializer),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Vec2>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt: Option<[f32; 2]> = Option::deserialize(deserializer)?;
        Ok(opt.map(|[x, y]| Vec2::new(x, y)))
    }
}

impl Component for Node {
    const STORAGE: crate::ecs::StorageType = crate::ecs::StorageType::Dense;
}

impl Default for Node {
    fn default() -> Self {
        Self::new()
    }
}

impl Node {
    /// Create a new empty node.
    pub fn new() -> Self {
        Self {
            ports: SmallVec::new(),
            input_cache: FxHashMap::default(),
            output_cache: FxHashMap::default(),
            position: None,
            collapsed: false,
            display_name: None,
        }
    }

    /// Add an input port of the given type.
    pub fn with_input<T: PortType>(mut self, name: impl Into<String>) -> Self {
        self.ports.push(Port::input::<T>(name));
        self
    }

    /// Add an output port of the given type.
    pub fn with_output<T: PortType>(mut self, name: impl Into<String>) -> Self {
        self.ports.push(Port::output::<T>(name));
        self
    }

    /// Add a bidirectional port of the given type.
    pub fn with_port<T: PortType>(
        mut self,
        name: impl Into<String>,
        direction: PortDirection,
    ) -> Self {
        self.ports.push(Port {
            name: name.into(),
            direction,
            value_type: T::port_type_id(),
            value: NodeValue::None,
            component_type: None,
            field_path: None,
        });
        self
    }

    /// Add a port from a definition.
    pub fn with_port_def(mut self, def: &PortDef) -> Self {
        self.ports.push(Port::from_def(def));
        self
    }

    /// Add a pre-built port.
    pub fn with_port_instance(mut self, port: Port) -> Self {
        self.ports.push(port);
        self
    }

    /// Set the display name.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.display_name = Some(name.into());
        self
    }

    /// Set the visual position.
    pub fn with_position(mut self, pos: Vec2) -> Self {
        self.position = Some(pos);
        self
    }

    /// Add a port dynamically.
    pub fn add_port(&mut self, port: Port) {
        self.ports.push(port);
    }

    /// Remove a port by name.
    pub fn remove_port(&mut self, name: &str) -> Option<Port> {
        if let Some(idx) = self.ports.iter().position(|p| p.name == name) {
            Some(self.ports.remove(idx))
        } else {
            None
        }
    }

    /// Get a port by name.
    pub fn port(&self, name: &str) -> Option<&Port> {
        self.ports.iter().find(|p| p.name == name)
    }

    /// Get a mutable port by name.
    pub fn port_mut(&mut self, name: &str) -> Option<&mut Port> {
        self.ports.iter_mut().find(|p| p.name == name)
    }

    /// Iterate over all input ports.
    pub fn inputs(&self) -> impl Iterator<Item = &Port> {
        self.ports.iter().filter(|p| p.is_input())
    }

    /// Iterate over all output ports.
    pub fn outputs(&self) -> impl Iterator<Item = &Port> {
        self.ports.iter().filter(|p| p.is_output())
    }

    /// Iterate over all ports.
    pub fn ports(&self) -> impl Iterator<Item = &Port> {
        self.ports.iter()
    }

    /// Set an input value by name.
    pub fn set_input(&mut self, name: &str, value: NodeValue) {
        if let Some(port) = self.port_mut(name) {
            if port.is_input() {
                port.set_value(value.clone());
            }
        }
        self.input_cache.insert(name.to_string(), value);
    }

    /// Get an input value by name.
    pub fn get_input(&self, name: &str) -> Option<&NodeValue> {
        self.input_cache
            .get(name)
            .or_else(|| self.port(name).map(|p| p.value()))
    }

    /// Get a typed input value.
    pub fn get_input_as<T: PortType>(&self, name: &str) -> Option<T> {
        self.get_input(name).and_then(T::from_value)
    }

    /// Set an output value by name.
    pub fn set_output(&mut self, name: &str, value: NodeValue) {
        if let Some(port) = self.port_mut(name) {
            if port.is_output() {
                port.set_value(value.clone());
            }
        }
        self.output_cache.insert(name.to_string(), value);
    }

    /// Get an output value by name.
    pub fn get_output(&self, name: &str) -> Option<NodeValue> {
        self.output_cache
            .get(name)
            .cloned()
            .or_else(|| self.port(name).map(|p| p.value().clone()))
    }

    /// Get a typed output value.
    pub fn get_output_as<T: PortType>(&self, name: &str) -> Option<T> {
        self.get_output(name).and_then(|v| T::from_value(&v))
    }

    /// Get all current input values as a map.
    pub fn input_values(&self) -> FxHashMap<String, NodeValue> {
        let mut values = self.input_cache.clone();
        for port in self.inputs() {
            values
                .entry(port.name.clone())
                .or_insert_with(|| port.value().clone());
        }
        values
    }

    /// Get all current output values as a map.
    pub fn output_values(&self) -> FxHashMap<String, NodeValue> {
        let mut values = self.output_cache.clone();
        for port in self.outputs() {
            values
                .entry(port.name.clone())
                .or_insert_with(|| port.value().clone());
        }
        values
    }

    /// Clear all cached values.
    pub fn clear_cache(&mut self) {
        self.input_cache.clear();
        self.output_cache.clear();
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_creation() {
        let node = Node::new()
            .with_input::<f32>("a")
            .with_input::<f32>("b")
            .with_output::<f32>("result");

        assert_eq!(node.inputs().count(), 2);
        assert_eq!(node.outputs().count(), 1);
    }

    #[test]
    fn test_node_values() {
        let mut node = Node::new()
            .with_input::<f32>("value")
            .with_output::<f32>("result");

        node.set_input("value", NodeValue::Float(42.0));
        assert_eq!(node.get_input_as::<f32>("value"), Some(42.0));

        node.set_output("result", NodeValue::Float(84.0));
        assert_eq!(node.get_output_as::<f32>("result"), Some(84.0));
    }

    #[test]
    fn test_port_lookup() {
        let node = Node::new().with_input::<f32>("test");

        assert!(node.port("test").is_some());
        assert!(node.port("nonexistent").is_none());
    }

    #[test]
    fn test_node_serialization() {
        let node = Node::new()
            .with_input::<f32>("a")
            .with_output::<f32>("result")
            .with_name("Test Node")
            .with_position(Vec2::new(100.0, 200.0));

        let json = serde_json::to_string(&node).unwrap();
        let restored: Node = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.display_name, Some("Test Node".to_string()));
        let pos = restored.position.unwrap();
        assert!((pos.x - 100.0).abs() < f32::EPSILON);
        assert!((pos.y - 200.0).abs() < f32::EPSILON);
        assert_eq!(restored.inputs().count(), 1);
        assert_eq!(restored.outputs().count(), 1);
    }

    #[test]
    fn test_port_serialization() {
        let port = Port::input::<f32>("value").with_value(NodeValue::Float(42.0));
        let json = serde_json::to_string(&port).unwrap();
        let restored: Port = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.name, "value");
        assert_eq!(restored.direction, PortDirection::Input);
        assert_eq!(restored.value_type, PortTypeId::Float);
        assert_eq!(restored.get::<f32>(), Some(42.0));
    }
}
