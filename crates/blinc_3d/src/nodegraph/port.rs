//! Port types and definitions for the node graph system.
//!
//! Ports are the connection points on nodes. Each port has a direction
//! (input, output, or both) and a type that determines what values can
//! flow through it.

use super::value::NodeValue;
use serde::{Deserialize, Serialize};
use crate::ecs::Entity;
use crate::geometry::GeometryHandle;
use crate::materials::MaterialHandle;
use crate::math::Quat;
use blinc_core::{Color, Mat4, Vec2, Vec3};

/// Identifies the data type of a port.
///
/// Used for type checking connections and auto-discovery of compatible ports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PortTypeId {
    /// No specific type / any type
    Any,
    /// Boolean
    Bool,
    /// 32-bit signed integer
    Int,
    /// 32-bit floating point
    Float,
    /// 2D vector
    Vec2,
    /// 3D vector
    Vec3,
    /// Quaternion rotation
    Quat,
    /// 4x4 matrix
    Mat4,
    /// RGBA color
    Color,
    /// Entity reference
    Entity,
    /// Geometry handle
    Geometry,
    /// Material handle
    Material,
    /// String
    String,
    /// Array of values
    Array,
    /// Execution flow (for event/trigger connections)
    Execution,
}

impl PortTypeId {
    /// Returns true if this type can be connected to another type.
    ///
    /// Most types only connect to themselves, but some implicit conversions
    /// are allowed (e.g., Int → Float, Vec3 → Color).
    pub fn is_compatible_with(&self, other: &PortTypeId) -> bool {
        if *self == PortTypeId::Any || *other == PortTypeId::Any {
            return true;
        }
        if self == other {
            return true;
        }
        // Allow some implicit conversions
        matches!(
            (self, other),
            (PortTypeId::Int, PortTypeId::Float)
                | (PortTypeId::Float, PortTypeId::Int)
                | (PortTypeId::Vec3, PortTypeId::Color)
                | (PortTypeId::Color, PortTypeId::Vec3)
        )
    }

    /// Get the display name for this type.
    pub fn display_name(&self) -> &'static str {
        match self {
            PortTypeId::Any => "Any",
            PortTypeId::Bool => "Bool",
            PortTypeId::Int => "Int",
            PortTypeId::Float => "Float",
            PortTypeId::Vec2 => "Vec2",
            PortTypeId::Vec3 => "Vec3",
            PortTypeId::Quat => "Quat",
            PortTypeId::Mat4 => "Mat4",
            PortTypeId::Color => "Color",
            PortTypeId::Entity => "Entity",
            PortTypeId::Geometry => "Geometry",
            PortTypeId::Material => "Material",
            PortTypeId::String => "String",
            PortTypeId::Array => "Array",
            PortTypeId::Execution => "Exec",
        }
    }
}

/// Trait for types that can flow through node ports.
///
/// Implement this trait for custom types to allow them to be used
/// in node connections.
pub trait PortType: 'static + Send + Sync + Clone {
    /// Returns the type identifier for this port type.
    fn port_type_id() -> PortTypeId;

    /// Try to convert from a NodeValue.
    fn from_value(value: &NodeValue) -> Option<Self>;

    /// Convert to a NodeValue.
    fn to_value(&self) -> NodeValue;
}

// Implement PortType for standard types

impl PortType for bool {
    fn port_type_id() -> PortTypeId {
        PortTypeId::Bool
    }

    fn from_value(value: &NodeValue) -> Option<Self> {
        value.as_bool()
    }

    fn to_value(&self) -> NodeValue {
        NodeValue::Bool(*self)
    }
}

impl PortType for i32 {
    fn port_type_id() -> PortTypeId {
        PortTypeId::Int
    }

    fn from_value(value: &NodeValue) -> Option<Self> {
        value.as_i32()
    }

    fn to_value(&self) -> NodeValue {
        NodeValue::Int(*self)
    }
}

impl PortType for f32 {
    fn port_type_id() -> PortTypeId {
        PortTypeId::Float
    }

    fn from_value(value: &NodeValue) -> Option<Self> {
        value.as_f32()
    }

    fn to_value(&self) -> NodeValue {
        NodeValue::Float(*self)
    }
}

impl PortType for Vec2 {
    fn port_type_id() -> PortTypeId {
        PortTypeId::Vec2
    }

    fn from_value(value: &NodeValue) -> Option<Self> {
        value.as_vec2()
    }

    fn to_value(&self) -> NodeValue {
        NodeValue::Vec2(*self)
    }
}

impl PortType for Vec3 {
    fn port_type_id() -> PortTypeId {
        PortTypeId::Vec3
    }

    fn from_value(value: &NodeValue) -> Option<Self> {
        value.as_vec3()
    }

    fn to_value(&self) -> NodeValue {
        NodeValue::Vec3(*self)
    }
}

impl PortType for Quat {
    fn port_type_id() -> PortTypeId {
        PortTypeId::Quat
    }

    fn from_value(value: &NodeValue) -> Option<Self> {
        value.as_quat()
    }

    fn to_value(&self) -> NodeValue {
        NodeValue::Quat(*self)
    }
}

impl PortType for Mat4 {
    fn port_type_id() -> PortTypeId {
        PortTypeId::Mat4
    }

    fn from_value(value: &NodeValue) -> Option<Self> {
        value.as_mat4()
    }

    fn to_value(&self) -> NodeValue {
        NodeValue::Mat4(*self)
    }
}

impl PortType for Color {
    fn port_type_id() -> PortTypeId {
        PortTypeId::Color
    }

    fn from_value(value: &NodeValue) -> Option<Self> {
        value.as_color()
    }

    fn to_value(&self) -> NodeValue {
        NodeValue::Color(*self)
    }
}

impl PortType for Entity {
    fn port_type_id() -> PortTypeId {
        PortTypeId::Entity
    }

    fn from_value(value: &NodeValue) -> Option<Self> {
        value.as_entity()
    }

    fn to_value(&self) -> NodeValue {
        NodeValue::Entity(*self)
    }
}

impl PortType for String {
    fn port_type_id() -> PortTypeId {
        PortTypeId::String
    }

    fn from_value(value: &NodeValue) -> Option<Self> {
        value.as_str().map(|s| s.to_string())
    }

    fn to_value(&self) -> NodeValue {
        NodeValue::String(self.clone())
    }
}

impl PortType for GeometryHandle {
    fn port_type_id() -> PortTypeId {
        PortTypeId::Geometry
    }

    fn from_value(value: &NodeValue) -> Option<Self> {
        match value {
            NodeValue::Geometry(h) => Some(*h),
            _ => None,
        }
    }

    fn to_value(&self) -> NodeValue {
        NodeValue::Geometry(*self)
    }
}

impl PortType for MaterialHandle {
    fn port_type_id() -> PortTypeId {
        PortTypeId::Material
    }

    fn from_value(value: &NodeValue) -> Option<Self> {
        match value {
            NodeValue::Material(h) => Some(*h),
            _ => None,
        }
    }

    fn to_value(&self) -> NodeValue {
        NodeValue::Material(*self)
    }
}

/// Definition of a port on a node.
///
/// Used to declare what ports a component or node type exposes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortDef {
    /// Port name (unique within the node)
    pub name: String,
    /// Port direction (input, output, or both)
    pub direction: PortDirection,
    /// Data type of the port
    pub value_type: PortTypeId,
    /// Default value when no connection exists
    pub default_value: Option<NodeValue>,
    /// Human-readable description
    pub description: Option<String>,
}

impl PortDef {
    /// Create a new port definition.
    pub fn new(name: impl Into<String>, direction: PortDirection, value_type: PortTypeId) -> Self {
        Self {
            name: name.into(),
            direction,
            value_type,
            default_value: None,
            description: None,
        }
    }

    /// Create an input port.
    pub fn input<T: PortType>(name: impl Into<String>) -> Self {
        Self::new(name, PortDirection::Input, T::port_type_id())
    }

    /// Create an output port.
    pub fn output<T: PortType>(name: impl Into<String>) -> Self {
        Self::new(name, PortDirection::Output, T::port_type_id())
    }

    /// Create a bidirectional port (can be used as input or output).
    pub fn both<T: PortType>(name: impl Into<String>) -> Self {
        Self::new(name, PortDirection::Both, T::port_type_id())
    }

    /// Set the default value.
    pub fn with_default(mut self, value: impl Into<NodeValue>) -> Self {
        self.default_value = Some(value.into());
        self
    }

    /// Set the description.
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Returns true if this port can receive connections (input or both).
    pub fn is_input(&self) -> bool {
        matches!(self.direction, PortDirection::Input | PortDirection::Both)
    }

    /// Returns true if this port can send connections (output or both).
    pub fn is_output(&self) -> bool {
        matches!(self.direction, PortDirection::Output | PortDirection::Both)
    }
}

/// Direction of a port - determines how it can be connected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PortDirection {
    /// Can only receive values from connections
    Input,
    /// Can only send values to connections
    Output,
    /// Can both receive and send values
    Both,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_port_type_compatibility() {
        assert!(PortTypeId::Float.is_compatible_with(&PortTypeId::Float));
        assert!(PortTypeId::Int.is_compatible_with(&PortTypeId::Float));
        assert!(PortTypeId::Vec3.is_compatible_with(&PortTypeId::Color));
        assert!(PortTypeId::Any.is_compatible_with(&PortTypeId::Vec3));
        assert!(!PortTypeId::Bool.is_compatible_with(&PortTypeId::Float));
    }

    #[test]
    fn test_port_def_creation() {
        let port = PortDef::input::<f32>("value").with_default(1.0f32);
        assert_eq!(port.name, "value");
        assert!(port.is_input());
        assert!(!port.is_output());
        assert!(port.default_value.is_some());
    }

    #[test]
    fn test_port_type_trait() {
        let value = 42.0f32;
        let node_value = value.to_value();
        assert!(matches!(node_value, NodeValue::Float(42.0)));

        let back: f32 = PortType::from_value(&node_value).unwrap();
        assert_eq!(back, 42.0);
    }
}
