//! Node value types for the node graph system.
//!
//! [`NodeValue`] represents all possible data types that can flow through
//! node connections. It provides type-safe conversion methods and is the
//! foundation of the port system.

use crate::ecs::Entity;
use crate::geometry::GeometryHandle;
use crate::materials::MaterialHandle;
use crate::math::{Mat4Ext, Quat};
use blinc_core::{Color, Mat4, Vec2, Vec3};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Type-safe value that flows through node connections.
///
/// This enum represents all possible data types that can be passed
/// between nodes via their ports.
///
/// Serialization converts blinc_core types to simple arrays:
/// - Vec2 -> [f32; 2]
/// - Vec3 -> [f32; 3]
/// - Mat4 -> [f32; 16]
/// - Color -> [f32; 4] (rgba)
#[derive(Clone, Debug)]
pub enum NodeValue {
    /// No value / null
    None,

    // Primitives
    /// Boolean value
    Bool(bool),
    /// 32-bit signed integer
    Int(i32),
    /// 32-bit floating point
    Float(f32),

    // Math types
    /// 2D vector
    Vec2(Vec2),
    /// 3D vector
    Vec3(Vec3),
    /// Quaternion rotation
    Quat(Quat),
    /// 4x4 transformation matrix
    Mat4(Mat4),

    // Graphics types
    /// RGBA color
    Color(Color),

    // Resource handles
    /// Entity reference
    Entity(Entity),
    /// Geometry handle
    Geometry(GeometryHandle),
    /// Material handle
    Material(MaterialHandle),

    // Complex types
    /// UTF-8 string
    String(String),
    /// Array of values (homogeneous)
    Array(Vec<NodeValue>),
}

impl NodeValue {
    /// Returns true if this is a None value.
    #[inline]
    pub fn is_none(&self) -> bool {
        matches!(self, NodeValue::None)
    }

    /// Returns true if this is a numeric type (Int or Float).
    #[inline]
    pub fn is_numeric(&self) -> bool {
        matches!(self, NodeValue::Int(_) | NodeValue::Float(_))
    }

    /// Returns true if this is a vector type (Vec2, Vec3).
    #[inline]
    pub fn is_vector(&self) -> bool {
        matches!(self, NodeValue::Vec2(_) | NodeValue::Vec3(_))
    }

    /// Try to convert to f32, coercing from Int if needed.
    pub fn as_f32(&self) -> Option<f32> {
        match self {
            NodeValue::Float(v) => Some(*v),
            NodeValue::Int(v) => Some(*v as f32),
            _ => None,
        }
    }

    /// Try to convert to i32, truncating from Float if needed.
    pub fn as_i32(&self) -> Option<i32> {
        match self {
            NodeValue::Int(v) => Some(*v),
            NodeValue::Float(v) => Some(*v as i32),
            _ => None,
        }
    }

    /// Try to get as bool.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            NodeValue::Bool(v) => Some(*v),
            _ => None,
        }
    }

    /// Try to get as Vec2.
    pub fn as_vec2(&self) -> Option<Vec2> {
        match self {
            NodeValue::Vec2(v) => Some(*v),
            _ => None,
        }
    }

    /// Try to get as Vec3.
    pub fn as_vec3(&self) -> Option<Vec3> {
        match self {
            NodeValue::Vec3(v) => Some(*v),
            NodeValue::Color(c) => Some(Vec3::new(c.r, c.g, c.b)),
            _ => None,
        }
    }

    /// Try to get as Quat.
    pub fn as_quat(&self) -> Option<Quat> {
        match self {
            NodeValue::Quat(v) => Some(*v),
            _ => None,
        }
    }

    /// Try to get as Mat4.
    pub fn as_mat4(&self) -> Option<Mat4> {
        match self {
            NodeValue::Mat4(v) => Some(*v),
            _ => None,
        }
    }

    /// Try to get as Color.
    pub fn as_color(&self) -> Option<Color> {
        match self {
            NodeValue::Color(c) => Some(*c),
            NodeValue::Vec3(v) => Some(Color::rgb(v.x, v.y, v.z)),
            _ => None,
        }
    }

    /// Try to get as Entity.
    pub fn as_entity(&self) -> Option<Entity> {
        match self {
            NodeValue::Entity(e) => Some(*e),
            _ => None,
        }
    }

    /// Try to get as String reference.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            NodeValue::String(s) => Some(s),
            _ => None,
        }
    }

    /// Try to get as array reference.
    pub fn as_array(&self) -> Option<&[NodeValue]> {
        match self {
            NodeValue::Array(arr) => Some(arr),
            _ => None,
        }
    }

    /// Get the type name of this value.
    pub fn type_name(&self) -> &'static str {
        match self {
            NodeValue::None => "None",
            NodeValue::Bool(_) => "Bool",
            NodeValue::Int(_) => "Int",
            NodeValue::Float(_) => "Float",
            NodeValue::Vec2(_) => "Vec2",
            NodeValue::Vec3(_) => "Vec3",
            NodeValue::Quat(_) => "Quat",
            NodeValue::Mat4(_) => "Mat4",
            NodeValue::Color(_) => "Color",
            NodeValue::Entity(_) => "Entity",
            NodeValue::Geometry(_) => "Geometry",
            NodeValue::Material(_) => "Material",
            NodeValue::String(_) => "String",
            NodeValue::Array(_) => "Array",
        }
    }
}

impl Default for NodeValue {
    fn default() -> Self {
        NodeValue::None
    }
}

// Conversion implementations from Rust types to NodeValue

impl From<bool> for NodeValue {
    fn from(v: bool) -> Self {
        NodeValue::Bool(v)
    }
}

impl From<i32> for NodeValue {
    fn from(v: i32) -> Self {
        NodeValue::Int(v)
    }
}

impl From<f32> for NodeValue {
    fn from(v: f32) -> Self {
        NodeValue::Float(v)
    }
}

impl From<Vec2> for NodeValue {
    fn from(v: Vec2) -> Self {
        NodeValue::Vec2(v)
    }
}

impl From<Vec3> for NodeValue {
    fn from(v: Vec3) -> Self {
        NodeValue::Vec3(v)
    }
}

impl From<Quat> for NodeValue {
    fn from(v: Quat) -> Self {
        NodeValue::Quat(v)
    }
}

impl From<Mat4> for NodeValue {
    fn from(v: Mat4) -> Self {
        NodeValue::Mat4(v)
    }
}

impl From<Color> for NodeValue {
    fn from(v: Color) -> Self {
        NodeValue::Color(v)
    }
}

impl From<Entity> for NodeValue {
    fn from(v: Entity) -> Self {
        NodeValue::Entity(v)
    }
}

impl From<String> for NodeValue {
    fn from(v: String) -> Self {
        NodeValue::String(v)
    }
}

impl From<&str> for NodeValue {
    fn from(v: &str) -> Self {
        NodeValue::String(v.to_string())
    }
}

impl<T: Into<NodeValue>> From<Vec<T>> for NodeValue {
    fn from(v: Vec<T>) -> Self {
        NodeValue::Array(v.into_iter().map(Into::into).collect())
    }
}

// Serializable representation for NodeValue
// This handles blinc_core types that don't have serde derives
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
enum NodeValueSerde {
    None,
    Bool(bool),
    Int(i32),
    Float(f32),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Quat(Quat),
    Mat4([f32; 16]),
    Color([f32; 4]),
    Entity(Entity),
    Geometry(GeometryHandle),
    Material(MaterialHandle),
    String(String),
    Array(Vec<NodeValueSerde>),
}

impl From<&NodeValue> for NodeValueSerde {
    fn from(value: &NodeValue) -> Self {
        match value {
            NodeValue::None => NodeValueSerde::None,
            NodeValue::Bool(v) => NodeValueSerde::Bool(*v),
            NodeValue::Int(v) => NodeValueSerde::Int(*v),
            NodeValue::Float(v) => NodeValueSerde::Float(*v),
            NodeValue::Vec2(v) => NodeValueSerde::Vec2([v.x, v.y]),
            NodeValue::Vec3(v) => NodeValueSerde::Vec3([v.x, v.y, v.z]),
            NodeValue::Quat(v) => NodeValueSerde::Quat(*v),
            NodeValue::Mat4(v) => NodeValueSerde::Mat4(v.to_cols_array()),
            NodeValue::Color(c) => NodeValueSerde::Color([c.r, c.g, c.b, c.a]),
            NodeValue::Entity(e) => NodeValueSerde::Entity(*e),
            NodeValue::Geometry(h) => NodeValueSerde::Geometry(*h),
            NodeValue::Material(h) => NodeValueSerde::Material(*h),
            NodeValue::String(s) => NodeValueSerde::String(s.clone()),
            NodeValue::Array(arr) => {
                NodeValueSerde::Array(arr.iter().map(NodeValueSerde::from).collect())
            }
        }
    }
}

impl From<NodeValueSerde> for NodeValue {
    fn from(value: NodeValueSerde) -> Self {
        match value {
            NodeValueSerde::None => NodeValue::None,
            NodeValueSerde::Bool(v) => NodeValue::Bool(v),
            NodeValueSerde::Int(v) => NodeValue::Int(v),
            NodeValueSerde::Float(v) => NodeValue::Float(v),
            NodeValueSerde::Vec2([x, y]) => NodeValue::Vec2(Vec2::new(x, y)),
            NodeValueSerde::Vec3([x, y, z]) => NodeValue::Vec3(Vec3::new(x, y, z)),
            NodeValueSerde::Quat(v) => NodeValue::Quat(v),
            NodeValueSerde::Mat4(arr) => NodeValue::Mat4(Mat4::from_cols_array(&arr)),
            NodeValueSerde::Color([r, g, b, a]) => NodeValue::Color(Color::rgba(r, g, b, a)),
            NodeValueSerde::Entity(e) => NodeValue::Entity(e),
            NodeValueSerde::Geometry(h) => NodeValue::Geometry(h),
            NodeValueSerde::Material(h) => NodeValue::Material(h),
            NodeValueSerde::String(s) => NodeValue::String(s),
            NodeValueSerde::Array(arr) => {
                NodeValue::Array(arr.into_iter().map(NodeValue::from).collect())
            }
        }
    }
}

impl Serialize for NodeValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        NodeValueSerde::from(self).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for NodeValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        NodeValueSerde::deserialize(deserializer).map(NodeValue::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_conversions() {
        let v = NodeValue::from(42.0f32);
        assert_eq!(v.as_f32(), Some(42.0));
        assert_eq!(v.as_i32(), Some(42));

        let v = NodeValue::from(Vec3::new(1.0, 2.0, 3.0));
        assert!(v.as_vec3().is_some());
    }

    #[test]
    fn test_type_names() {
        assert_eq!(NodeValue::None.type_name(), "None");
        assert_eq!(NodeValue::Float(1.0).type_name(), "Float");
        assert_eq!(NodeValue::Vec3(Vec3::ZERO).type_name(), "Vec3");
    }

    #[test]
    fn test_serialization_primitives() {
        // Test float
        let v = NodeValue::Float(42.5);
        let json = serde_json::to_string(&v).unwrap();
        let restored: NodeValue = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.as_f32(), Some(42.5));

        // Test bool
        let v = NodeValue::Bool(true);
        let json = serde_json::to_string(&v).unwrap();
        let restored: NodeValue = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.as_bool(), Some(true));

        // Test int
        let v = NodeValue::Int(-123);
        let json = serde_json::to_string(&v).unwrap();
        let restored: NodeValue = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.as_i32(), Some(-123));
    }

    #[test]
    fn test_serialization_vectors() {
        // Test Vec2
        let v = NodeValue::Vec2(Vec2::new(1.0, 2.0));
        let json = serde_json::to_string(&v).unwrap();
        let restored: NodeValue = serde_json::from_str(&json).unwrap();
        let vec = restored.as_vec2().unwrap();
        assert!((vec.x - 1.0).abs() < f32::EPSILON);
        assert!((vec.y - 2.0).abs() < f32::EPSILON);

        // Test Vec3
        let v = NodeValue::Vec3(Vec3::new(1.0, 2.0, 3.0));
        let json = serde_json::to_string(&v).unwrap();
        let restored: NodeValue = serde_json::from_str(&json).unwrap();
        let vec = restored.as_vec3().unwrap();
        assert!((vec.x - 1.0).abs() < f32::EPSILON);
        assert!((vec.y - 2.0).abs() < f32::EPSILON);
        assert!((vec.z - 3.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_serialization_color() {
        let v = NodeValue::Color(Color::rgba(0.5, 0.6, 0.7, 0.8));
        let json = serde_json::to_string(&v).unwrap();
        let restored: NodeValue = serde_json::from_str(&json).unwrap();
        let color = restored.as_color().unwrap();
        assert!((color.r - 0.5).abs() < f32::EPSILON);
        assert!((color.g - 0.6).abs() < f32::EPSILON);
        assert!((color.b - 0.7).abs() < f32::EPSILON);
        assert!((color.a - 0.8).abs() < f32::EPSILON);
    }

    #[test]
    fn test_serialization_mat4() {
        let v = NodeValue::Mat4(Mat4::translation(1.0, 2.0, 3.0));
        let json = serde_json::to_string(&v).unwrap();
        let restored: NodeValue = serde_json::from_str(&json).unwrap();
        let mat = restored.as_mat4().unwrap();
        // Check translation column
        assert!((mat.cols[3][0] - 1.0).abs() < f32::EPSILON);
        assert!((mat.cols[3][1] - 2.0).abs() < f32::EPSILON);
        assert!((mat.cols[3][2] - 3.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_serialization_string() {
        let v = NodeValue::String("hello world".to_string());
        let json = serde_json::to_string(&v).unwrap();
        let restored: NodeValue = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.as_str(), Some("hello world"));
    }
}
