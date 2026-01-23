//! Value and constant nodes for the node graph.

use super::BuiltinNode;
use crate::nodegraph::{Node, OnTrigger};
use blinc_core::{Color, Vec3};

/// Outputs a constant float value.
pub struct ConstantFloatNode;

impl BuiltinNode for ConstantFloatNode {
    fn type_id() -> &'static str {
        "builtin.constant_float"
    }

    fn display_name() -> &'static str {
        "Constant (Float)"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<f32>("value")
            .with_output::<f32>("out")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let value: f32 = ctx.input_or("value", 0.0);
            ctx.output("out", value);
        })
    }
}

/// Outputs a constant Vec3 value.
pub struct ConstantVec3Node;

impl BuiltinNode for ConstantVec3Node {
    fn type_id() -> &'static str {
        "builtin.constant_vec3"
    }

    fn display_name() -> &'static str {
        "Constant (Vec3)"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<f32>("x")
            .with_input::<f32>("y")
            .with_input::<f32>("z")
            .with_output::<Vec3>("out")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let x: f32 = ctx.input_or("x", 0.0);
            let y: f32 = ctx.input_or("y", 0.0);
            let z: f32 = ctx.input_or("z", 0.0);
            ctx.output("out", Vec3::new(x, y, z));
        })
    }
}

/// Outputs a constant Color value.
pub struct ConstantColorNode;

impl BuiltinNode for ConstantColorNode {
    fn type_id() -> &'static str {
        "builtin.constant_color"
    }

    fn display_name() -> &'static str {
        "Constant (Color)"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<f32>("r")
            .with_input::<f32>("g")
            .with_input::<f32>("b")
            .with_input::<f32>("a")
            .with_output::<Color>("out")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let r: f32 = ctx.input_or("r", 1.0);
            let g: f32 = ctx.input_or("g", 1.0);
            let b: f32 = ctx.input_or("b", 1.0);
            let a: f32 = ctx.input_or("a", 1.0);
            ctx.output("out", Color::rgba(r, g, b, a));
        })
    }
}

/// Outputs elapsed time since start.
pub struct TimeNode;

impl BuiltinNode for TimeNode {
    fn type_id() -> &'static str {
        "builtin.time"
    }

    fn display_name() -> &'static str {
        "Time"
    }

    fn create_node() -> Node {
        Node::new()
            .with_output::<f32>("elapsed")
            .with_output::<f32>("delta")
            .with_output::<f32>("frame")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            ctx.output("elapsed", ctx.elapsed_time());
            ctx.output("delta", ctx.delta_time());
            ctx.output("frame", ctx.frame() as f32);
        })
    }
}

/// Splits a Vec3 into its components.
pub struct SplitVec3Node;

impl BuiltinNode for SplitVec3Node {
    fn type_id() -> &'static str {
        "builtin.split_vec3"
    }

    fn display_name() -> &'static str {
        "Split Vec3"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<Vec3>("vector")
            .with_output::<f32>("x")
            .with_output::<f32>("y")
            .with_output::<f32>("z")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let v: Vec3 = ctx.input_or("vector", Vec3::ZERO);
            ctx.output("x", v.x);
            ctx.output("y", v.y);
            ctx.output("z", v.z);
        })
    }
}

/// Combines components into a Vec3.
pub struct CombineVec3Node;

impl BuiltinNode for CombineVec3Node {
    fn type_id() -> &'static str {
        "builtin.combine_vec3"
    }

    fn display_name() -> &'static str {
        "Combine Vec3"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<f32>("x")
            .with_input::<f32>("y")
            .with_input::<f32>("z")
            .with_output::<Vec3>("vector")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let x: f32 = ctx.input_or("x", 0.0);
            let y: f32 = ctx.input_or("y", 0.0);
            let z: f32 = ctx.input_or("z", 0.0);
            ctx.output("vector", Vec3::new(x, y, z));
        })
    }
}

/// Computes the length of a Vec3.
pub struct Vec3LengthNode;

impl BuiltinNode for Vec3LengthNode {
    fn type_id() -> &'static str {
        "builtin.vec3_length"
    }

    fn display_name() -> &'static str {
        "Vec3 Length"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<Vec3>("vector")
            .with_output::<f32>("length")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let v: Vec3 = ctx.input_or("vector", Vec3::ZERO);
            ctx.output("length", v.length());
        })
    }
}

/// Normalizes a Vec3.
pub struct Vec3NormalizeNode;

impl BuiltinNode for Vec3NormalizeNode {
    fn type_id() -> &'static str {
        "builtin.vec3_normalize"
    }

    fn display_name() -> &'static str {
        "Vec3 Normalize"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<Vec3>("vector")
            .with_output::<Vec3>("normalized")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let v: Vec3 = ctx.input_or("vector", Vec3::new(1.0, 0.0, 0.0));
            ctx.output("normalized", v.normalize());
        })
    }
}

/// Computes the dot product of two Vec3s.
pub struct Vec3DotNode;

impl BuiltinNode for Vec3DotNode {
    fn type_id() -> &'static str {
        "builtin.vec3_dot"
    }

    fn display_name() -> &'static str {
        "Vec3 Dot"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<Vec3>("a")
            .with_input::<Vec3>("b")
            .with_output::<f32>("dot")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let a: Vec3 = ctx.input_or("a", Vec3::ZERO);
            let b: Vec3 = ctx.input_or("b", Vec3::ZERO);
            ctx.output("dot", a.dot(b));
        })
    }
}

/// Computes the cross product of two Vec3s.
pub struct Vec3CrossNode;

impl BuiltinNode for Vec3CrossNode {
    fn type_id() -> &'static str {
        "builtin.vec3_cross"
    }

    fn display_name() -> &'static str {
        "Vec3 Cross"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<Vec3>("a")
            .with_input::<Vec3>("b")
            .with_output::<Vec3>("cross")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let a: Vec3 = ctx.input_or("a", Vec3::new(1.0, 0.0, 0.0));
            let b: Vec3 = ctx.input_or("b", Vec3::new(0.0, 1.0, 0.0));
            ctx.output("cross", a.cross(b));
        })
    }
}

/// Adds two Vec3s.
pub struct Vec3AddNode;

impl BuiltinNode for Vec3AddNode {
    fn type_id() -> &'static str {
        "builtin.vec3_add"
    }

    fn display_name() -> &'static str {
        "Vec3 Add"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<Vec3>("a")
            .with_input::<Vec3>("b")
            .with_output::<Vec3>("result")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let a: Vec3 = ctx.input_or("a", Vec3::ZERO);
            let b: Vec3 = ctx.input_or("b", Vec3::ZERO);
            ctx.output("result", Vec3::new(a.x + b.x, a.y + b.y, a.z + b.z));
        })
    }
}

/// Multiplies a Vec3 by a scalar.
pub struct Vec3ScaleNode;

impl BuiltinNode for Vec3ScaleNode {
    fn type_id() -> &'static str {
        "builtin.vec3_scale"
    }

    fn display_name() -> &'static str {
        "Vec3 Scale"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<Vec3>("vector")
            .with_input::<f32>("scalar")
            .with_output::<Vec3>("result")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let v: Vec3 = ctx.input_or("vector", Vec3::ZERO);
            let s: f32 = ctx.input_or("scalar", 1.0);
            ctx.output("result", Vec3::new(v.x * s, v.y * s, v.z * s));
        })
    }
}

/// Linear interpolation between two Vec3s.
pub struct Vec3LerpNode;

impl BuiltinNode for Vec3LerpNode {
    fn type_id() -> &'static str {
        "builtin.vec3_lerp"
    }

    fn display_name() -> &'static str {
        "Vec3 Lerp"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<Vec3>("a")
            .with_input::<Vec3>("b")
            .with_input::<f32>("t")
            .with_output::<Vec3>("result")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let a: Vec3 = ctx.input_or("a", Vec3::ZERO);
            let b: Vec3 = ctx.input_or("b", Vec3::ONE);
            let t: f32 = ctx.input_or("t", 0.5).clamp(0.0, 1.0);
            // Manual lerp: a + (b - a) * t
            ctx.output("result", Vec3::new(
                a.x + (b.x - a.x) * t,
                a.y + (b.y - a.y) * t,
                a.z + (b.z - a.z) * t,
            ));
        })
    }
}

/// Compares two values and outputs the result.
pub struct CompareNode;

impl BuiltinNode for CompareNode {
    fn type_id() -> &'static str {
        "builtin.compare"
    }

    fn display_name() -> &'static str {
        "Compare"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<f32>("a")
            .with_input::<f32>("b")
            .with_output::<bool>("equal")
            .with_output::<bool>("greater")
            .with_output::<bool>("less")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let a: f32 = ctx.input_or("a", 0.0);
            let b: f32 = ctx.input_or("b", 0.0);
            ctx.output("equal", (a - b).abs() < f32::EPSILON);
            ctx.output("greater", a > b);
            ctx.output("less", a < b);
        })
    }
}

/// Selects between two values based on a condition.
pub struct SelectNode;

impl BuiltinNode for SelectNode {
    fn type_id() -> &'static str {
        "builtin.select"
    }

    fn display_name() -> &'static str {
        "Select"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<bool>("condition")
            .with_input::<f32>("true_value")
            .with_input::<f32>("false_value")
            .with_output::<f32>("result")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let condition: bool = ctx.input_or("condition", false);
            let true_value: f32 = ctx.input_or("true_value", 1.0);
            let false_value: f32 = ctx.input_or("false_value", 0.0);
            ctx.output("result", if condition { true_value } else { false_value });
        })
    }
}

/// Boolean AND operation.
pub struct AndNode;

impl BuiltinNode for AndNode {
    fn type_id() -> &'static str {
        "builtin.and"
    }

    fn display_name() -> &'static str {
        "And"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<bool>("a")
            .with_input::<bool>("b")
            .with_output::<bool>("result")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let a: bool = ctx.input_or("a", false);
            let b: bool = ctx.input_or("b", false);
            ctx.output("result", a && b);
        })
    }
}

/// Boolean OR operation.
pub struct OrNode;

impl BuiltinNode for OrNode {
    fn type_id() -> &'static str {
        "builtin.or"
    }

    fn display_name() -> &'static str {
        "Or"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<bool>("a")
            .with_input::<bool>("b")
            .with_output::<bool>("result")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let a: bool = ctx.input_or("a", false);
            let b: bool = ctx.input_or("b", false);
            ctx.output("result", a || b);
        })
    }
}

/// Boolean NOT operation.
pub struct NotNode;

impl BuiltinNode for NotNode {
    fn type_id() -> &'static str {
        "builtin.not"
    }

    fn display_name() -> &'static str {
        "Not"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<bool>("value")
            .with_output::<bool>("result")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let value: bool = ctx.input_or("value", false);
            ctx.output("result", !value);
        })
    }
}

/// Passes input through (useful for debugging or organization).
pub struct PassthroughNode;

impl BuiltinNode for PassthroughNode {
    fn type_id() -> &'static str {
        "builtin.passthrough"
    }

    fn display_name() -> &'static str {
        "Passthrough"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<f32>("value")
            .with_output::<f32>("out")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            if let Some(value) = ctx.input_raw("value") {
                ctx.output_raw("out", value.clone());
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::World;
    use crate::nodegraph::builtin::BuiltinNode;

    #[test]
    fn test_time_node() {
        let mut world = World::new();
        let entity = TimeNode::spawn(&mut world);

        let node = world.get::<Node>(entity).unwrap();
        assert_eq!(node.outputs().count(), 3);
    }

    #[test]
    fn test_split_combine_vec3() {
        let mut world = World::new();

        let split = SplitVec3Node::spawn(&mut world);
        let combine = CombineVec3Node::spawn(&mut world);

        let split_node = world.get::<Node>(split).unwrap();
        let combine_node = world.get::<Node>(combine).unwrap();

        assert_eq!(split_node.inputs().count(), 1);
        assert_eq!(split_node.outputs().count(), 3);
        assert_eq!(combine_node.inputs().count(), 3);
        assert_eq!(combine_node.outputs().count(), 1);
    }
}


