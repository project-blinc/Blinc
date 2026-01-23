//! Built-in utility nodes for the node graph system.
//!
//! These are pure logic nodes that don't have entity state - they simply
//! transform input values to output values.

mod math;
mod value;

pub use math::*;
pub use value::*;

use super::{Node, OnTrigger};
use crate::ecs::World;
use rustc_hash::FxHashMap;
use std::sync::OnceLock;

/// Trait for built-in node types that can spawn themselves.
pub trait BuiltinNode: Sized {
    /// The unique type identifier for this node (e.g., "builtin.add").
    fn type_id() -> &'static str;

    /// The display name for this node type.
    fn display_name() -> &'static str;

    /// Create the Node component with appropriate ports.
    fn create_node() -> Node;

    /// Create the OnTrigger component with the node's logic.
    fn create_trigger() -> OnTrigger;

    /// Spawn this node type into the world.
    fn spawn(world: &mut World) -> crate::ecs::Entity {
        world
            .spawn()
            .insert(Self::create_node().with_name(Self::display_name()))
            .insert(Self::create_trigger())
            .id()
    }
}

/// Factory function type for creating OnTrigger from a node type.
pub type TriggerFactory = fn() -> OnTrigger;

/// Registry of node types and their trigger factories.
///
/// Used during graph deserialization to automatically restore
/// OnTrigger components for known node types.
pub struct NodeTypeRegistry {
    factories: FxHashMap<&'static str, TriggerFactory>,
}

impl Default for NodeTypeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl NodeTypeRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            factories: FxHashMap::default(),
        }
    }

    /// Register a node type with its trigger factory.
    pub fn register(&mut self, type_id: &'static str, factory: TriggerFactory) {
        self.factories.insert(type_id, factory);
    }

    /// Register a BuiltinNode type.
    pub fn register_builtin<T: BuiltinNode>(&mut self) {
        self.register(T::type_id(), T::create_trigger);
    }

    /// Get the trigger factory for a node type.
    pub fn get(&self, type_id: &str) -> Option<TriggerFactory> {
        self.factories.get(type_id).copied()
    }

    /// Create an OnTrigger for a node type, if registered.
    pub fn create_trigger(&self, type_id: &str) -> Option<OnTrigger> {
        self.get(type_id).map(|factory| factory())
    }

    /// Check if a node type is registered.
    pub fn contains(&self, type_id: &str) -> bool {
        self.factories.contains_key(type_id)
    }

    /// Get all registered type IDs.
    pub fn type_ids(&self) -> impl Iterator<Item = &&'static str> {
        self.factories.keys()
    }
}

/// Get the global registry with all built-in nodes registered.
pub fn builtin_registry() -> &'static NodeTypeRegistry {
    static REGISTRY: OnceLock<NodeTypeRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut registry = NodeTypeRegistry::new();
        register_all_builtins(&mut registry);
        registry
    })
}

/// Register all built-in node types into a registry.
pub fn register_all_builtins(registry: &mut NodeTypeRegistry) {
    // Math nodes
    registry.register_builtin::<AddNode>();
    registry.register_builtin::<SubtractNode>();
    registry.register_builtin::<MultiplyNode>();
    registry.register_builtin::<DivideNode>();
    registry.register_builtin::<NegateNode>();
    registry.register_builtin::<AbsNode>();
    registry.register_builtin::<FloorNode>();
    registry.register_builtin::<CeilNode>();
    registry.register_builtin::<RoundNode>();
    registry.register_builtin::<MinNode>();
    registry.register_builtin::<MaxNode>();
    registry.register_builtin::<ClampNode>();
    registry.register_builtin::<LerpNode>();
    registry.register_builtin::<RemapNode>();
    registry.register_builtin::<SinNode>();
    registry.register_builtin::<CosNode>();
    registry.register_builtin::<PowerNode>();
    registry.register_builtin::<SqrtNode>();
    registry.register_builtin::<ModuloNode>();

    // Vec2 nodes
    registry.register_builtin::<Vec2AddNode>();
    registry.register_builtin::<Vec2SubtractNode>();
    registry.register_builtin::<Vec2ScaleNode>();
    registry.register_builtin::<Vec2LengthNode>();
    registry.register_builtin::<Vec2NormalizeNode>();
    registry.register_builtin::<Vec2DotNode>();
    registry.register_builtin::<Vec2LerpNode>();
    registry.register_builtin::<Vec2DistanceNode>();

    // Vec3 nodes
    registry.register_builtin::<Vec3AddNode>();
    registry.register_builtin::<Vec3SubtractNode>();
    registry.register_builtin::<Vec3MultiplyNode>();
    registry.register_builtin::<Vec3ScaleNode>();
    registry.register_builtin::<Vec3CrossNode>();
    registry.register_builtin::<Vec3DotNode>();
    registry.register_builtin::<Vec3LengthNode>();
    registry.register_builtin::<Vec3NormalizeNode>();
    registry.register_builtin::<Vec3LerpNode>();
    registry.register_builtin::<Vec3DistanceNode>();
    registry.register_builtin::<Vec3NegateNode>();
    registry.register_builtin::<Vec3ReflectNode>();
    registry.register_builtin::<Vec3ProjectNode>();

    // Mat4 nodes
    registry.register_builtin::<Mat4IdentityNode>();
    registry.register_builtin::<Mat4TranslationNode>();
    registry.register_builtin::<Mat4ScaleNode>();
    registry.register_builtin::<Mat4RotationXNode>();
    registry.register_builtin::<Mat4RotationYNode>();
    registry.register_builtin::<Mat4RotationZNode>();
    registry.register_builtin::<Mat4MultiplyNode>();
    registry.register_builtin::<Mat4TransformPointNode>();
    registry.register_builtin::<Mat4TransformDirectionNode>();
    registry.register_builtin::<Mat4ComposeNode>();

    // Value nodes
    registry.register_builtin::<ConstantFloatNode>();
    registry.register_builtin::<ConstantVec3Node>();
    registry.register_builtin::<ConstantColorNode>();
    registry.register_builtin::<TimeNode>();
    registry.register_builtin::<SplitVec3Node>();
    registry.register_builtin::<CombineVec3Node>();

    // Logic nodes
    registry.register_builtin::<CompareNode>();
    registry.register_builtin::<SelectNode>();
    registry.register_builtin::<AndNode>();
    registry.register_builtin::<OrNode>();
    registry.register_builtin::<NotNode>();
    registry.register_builtin::<PassthroughNode>();
}
