//! Math operation nodes for the node graph.

use super::BuiltinNode;
use crate::nodegraph::{Node, OnTrigger};
use blinc_core::{Mat4, Vec2, Vec3};

/// Adds two float values.
pub struct AddNode;

impl BuiltinNode for AddNode {
    fn type_id() -> &'static str {
        "builtin.add"
    }

    fn display_name() -> &'static str {
        "Add"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<f32>("a")
            .with_input::<f32>("b")
            .with_output::<f32>("result")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let a: f32 = ctx.input_or("a", 0.0);
            let b: f32 = ctx.input_or("b", 0.0);
            ctx.output("result", a + b);
        })
    }
}

/// Subtracts two float values (a - b).
pub struct SubtractNode;

impl BuiltinNode for SubtractNode {
    fn type_id() -> &'static str {
        "builtin.subtract"
    }

    fn display_name() -> &'static str {
        "Subtract"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<f32>("a")
            .with_input::<f32>("b")
            .with_output::<f32>("result")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let a: f32 = ctx.input_or("a", 0.0);
            let b: f32 = ctx.input_or("b", 0.0);
            ctx.output("result", a - b);
        })
    }
}

/// Multiplies two float values.
pub struct MultiplyNode;

impl BuiltinNode for MultiplyNode {
    fn type_id() -> &'static str {
        "builtin.multiply"
    }

    fn display_name() -> &'static str {
        "Multiply"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<f32>("a")
            .with_input::<f32>("b")
            .with_output::<f32>("result")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let a: f32 = ctx.input_or("a", 1.0);
            let b: f32 = ctx.input_or("b", 1.0);
            ctx.output("result", a * b);
        })
    }
}

/// Divides two float values (a / b).
pub struct DivideNode;

impl BuiltinNode for DivideNode {
    fn type_id() -> &'static str {
        "builtin.divide"
    }

    fn display_name() -> &'static str {
        "Divide"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<f32>("a")
            .with_input::<f32>("b")
            .with_output::<f32>("result")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let a: f32 = ctx.input_or("a", 0.0);
            let b: f32 = ctx.input_or("b", 1.0);
            ctx.output("result", if b != 0.0 { a / b } else { 0.0 });
        })
    }
}

/// Linear interpolation between two values.
pub struct LerpNode;

impl BuiltinNode for LerpNode {
    fn type_id() -> &'static str {
        "builtin.lerp"
    }

    fn display_name() -> &'static str {
        "Lerp"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<f32>("a")
            .with_input::<f32>("b")
            .with_input::<f32>("t")
            .with_output::<f32>("result")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let a: f32 = ctx.input_or("a", 0.0);
            let b: f32 = ctx.input_or("b", 1.0);
            let t: f32 = ctx.input_or("t", 0.5);
            ctx.output("result", a + (b - a) * t.clamp(0.0, 1.0));
        })
    }
}

/// Clamps a value between min and max.
pub struct ClampNode;

impl BuiltinNode for ClampNode {
    fn type_id() -> &'static str {
        "builtin.clamp"
    }

    fn display_name() -> &'static str {
        "Clamp"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<f32>("value")
            .with_input::<f32>("min")
            .with_input::<f32>("max")
            .with_output::<f32>("result")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let value: f32 = ctx.input_or("value", 0.0);
            let min: f32 = ctx.input_or("min", 0.0);
            let max: f32 = ctx.input_or("max", 1.0);
            ctx.output("result", value.clamp(min, max));
        })
    }
}

/// Remaps a value from one range to another.
pub struct RemapNode;

impl BuiltinNode for RemapNode {
    fn type_id() -> &'static str {
        "builtin.remap"
    }

    fn display_name() -> &'static str {
        "Remap"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<f32>("value")
            .with_input::<f32>("in_min")
            .with_input::<f32>("in_max")
            .with_input::<f32>("out_min")
            .with_input::<f32>("out_max")
            .with_output::<f32>("result")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let value: f32 = ctx.input_or("value", 0.0);
            let in_min: f32 = ctx.input_or("in_min", 0.0);
            let in_max: f32 = ctx.input_or("in_max", 1.0);
            let out_min: f32 = ctx.input_or("out_min", 0.0);
            let out_max: f32 = ctx.input_or("out_max", 1.0);

            let in_range = in_max - in_min;
            if in_range.abs() < f32::EPSILON {
                ctx.output("result", out_min);
            } else {
                let t = (value - in_min) / in_range;
                ctx.output("result", out_min + t * (out_max - out_min));
            }
        })
    }
}

/// Computes sine of an angle (in radians).
pub struct SinNode;

impl BuiltinNode for SinNode {
    fn type_id() -> &'static str {
        "builtin.sin"
    }

    fn display_name() -> &'static str {
        "Sin"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<f32>("angle")
            .with_output::<f32>("result")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let angle: f32 = ctx.input_or("angle", 0.0);
            ctx.output("result", angle.sin());
        })
    }
}

/// Computes cosine of an angle (in radians).
pub struct CosNode;

impl BuiltinNode for CosNode {
    fn type_id() -> &'static str {
        "builtin.cos"
    }

    fn display_name() -> &'static str {
        "Cos"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<f32>("angle")
            .with_output::<f32>("result")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let angle: f32 = ctx.input_or("angle", 0.0);
            ctx.output("result", angle.cos());
        })
    }
}

/// Computes the absolute value.
pub struct AbsNode;

impl BuiltinNode for AbsNode {
    fn type_id() -> &'static str {
        "builtin.abs"
    }

    fn display_name() -> &'static str {
        "Abs"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<f32>("value")
            .with_output::<f32>("result")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let value: f32 = ctx.input_or("value", 0.0);
            ctx.output("result", value.abs());
        })
    }
}

/// Computes the power (base^exponent).
pub struct PowerNode;

impl BuiltinNode for PowerNode {
    fn type_id() -> &'static str {
        "builtin.power"
    }

    fn display_name() -> &'static str {
        "Power"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<f32>("base")
            .with_input::<f32>("exponent")
            .with_output::<f32>("result")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let base: f32 = ctx.input_or("base", 1.0);
            let exponent: f32 = ctx.input_or("exponent", 1.0);
            ctx.output("result", base.powf(exponent));
        })
    }
}

/// Computes the square root.
pub struct SqrtNode;

impl BuiltinNode for SqrtNode {
    fn type_id() -> &'static str {
        "builtin.sqrt"
    }

    fn display_name() -> &'static str {
        "Sqrt"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<f32>("value")
            .with_output::<f32>("result")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let value: f32 = ctx.input_or("value", 0.0);
            ctx.output("result", if value >= 0.0 { value.sqrt() } else { 0.0 });
        })
    }
}

/// Returns the minimum of two values.
pub struct MinNode;

impl BuiltinNode for MinNode {
    fn type_id() -> &'static str {
        "builtin.min"
    }

    fn display_name() -> &'static str {
        "Min"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<f32>("a")
            .with_input::<f32>("b")
            .with_output::<f32>("result")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let a: f32 = ctx.input_or("a", 0.0);
            let b: f32 = ctx.input_or("b", 0.0);
            ctx.output("result", a.min(b));
        })
    }
}

/// Returns the maximum of two values.
pub struct MaxNode;

impl BuiltinNode for MaxNode {
    fn type_id() -> &'static str {
        "builtin.max"
    }

    fn display_name() -> &'static str {
        "Max"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<f32>("a")
            .with_input::<f32>("b")
            .with_output::<f32>("result")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let a: f32 = ctx.input_or("a", 0.0);
            let b: f32 = ctx.input_or("b", 0.0);
            ctx.output("result", a.max(b));
        })
    }
}

/// Computes floor of a value.
pub struct FloorNode;

impl BuiltinNode for FloorNode {
    fn type_id() -> &'static str {
        "builtin.floor"
    }

    fn display_name() -> &'static str {
        "Floor"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<f32>("value")
            .with_output::<f32>("result")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let value: f32 = ctx.input_or("value", 0.0);
            ctx.output("result", value.floor());
        })
    }
}

/// Computes ceil of a value.
pub struct CeilNode;

impl BuiltinNode for CeilNode {
    fn type_id() -> &'static str {
        "builtin.ceil"
    }

    fn display_name() -> &'static str {
        "Ceil"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<f32>("value")
            .with_output::<f32>("result")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let value: f32 = ctx.input_or("value", 0.0);
            ctx.output("result", value.ceil());
        })
    }
}

/// Rounds a value to the nearest integer.
pub struct RoundNode;

impl BuiltinNode for RoundNode {
    fn type_id() -> &'static str {
        "builtin.round"
    }

    fn display_name() -> &'static str {
        "Round"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<f32>("value")
            .with_output::<f32>("result")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let value: f32 = ctx.input_or("value", 0.0);
            ctx.output("result", value.round());
        })
    }
}

/// Negates a value.
pub struct NegateNode;

impl BuiltinNode for NegateNode {
    fn type_id() -> &'static str {
        "builtin.negate"
    }

    fn display_name() -> &'static str {
        "Negate"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<f32>("value")
            .with_output::<f32>("result")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let value: f32 = ctx.input_or("value", 0.0);
            ctx.output("result", -value);
        })
    }
}

/// Computes modulo (remainder after division).
pub struct ModuloNode;

impl BuiltinNode for ModuloNode {
    fn type_id() -> &'static str {
        "builtin.modulo"
    }

    fn display_name() -> &'static str {
        "Modulo"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<f32>("a")
            .with_input::<f32>("b")
            .with_output::<f32>("result")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let a: f32 = ctx.input_or("a", 0.0);
            let b: f32 = ctx.input_or("b", 1.0);
            ctx.output("result", if b != 0.0 { a % b } else { 0.0 });
        })
    }
}

// ============================================================================
// Vec2 Operations
// ============================================================================

/// Adds two Vec2 values.
pub struct Vec2AddNode;

impl BuiltinNode for Vec2AddNode {
    fn type_id() -> &'static str {
        "builtin.vec2_add"
    }

    fn display_name() -> &'static str {
        "Vec2 Add"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<Vec2>("a")
            .with_input::<Vec2>("b")
            .with_output::<Vec2>("result")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let a: Vec2 = ctx.input_or("a", Vec2::ZERO);
            let b: Vec2 = ctx.input_or("b", Vec2::ZERO);
            ctx.output("result", Vec2::new(a.x + b.x, a.y + b.y));
        })
    }
}

/// Subtracts two Vec2 values (a - b).
pub struct Vec2SubtractNode;

impl BuiltinNode for Vec2SubtractNode {
    fn type_id() -> &'static str {
        "builtin.vec2_subtract"
    }

    fn display_name() -> &'static str {
        "Vec2 Subtract"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<Vec2>("a")
            .with_input::<Vec2>("b")
            .with_output::<Vec2>("result")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let a: Vec2 = ctx.input_or("a", Vec2::ZERO);
            let b: Vec2 = ctx.input_or("b", Vec2::ZERO);
            ctx.output("result", Vec2::new(a.x - b.x, a.y - b.y));
        })
    }
}

/// Multiplies a Vec2 by a scalar.
pub struct Vec2ScaleNode;

impl BuiltinNode for Vec2ScaleNode {
    fn type_id() -> &'static str {
        "builtin.vec2_scale"
    }

    fn display_name() -> &'static str {
        "Vec2 Scale"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<Vec2>("vector")
            .with_input::<f32>("scalar")
            .with_output::<Vec2>("result")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let v: Vec2 = ctx.input_or("vector", Vec2::ZERO);
            let s: f32 = ctx.input_or("scalar", 1.0);
            ctx.output("result", Vec2::new(v.x * s, v.y * s));
        })
    }
}

/// Computes the length of a Vec2.
pub struct Vec2LengthNode;

impl BuiltinNode for Vec2LengthNode {
    fn type_id() -> &'static str {
        "builtin.vec2_length"
    }

    fn display_name() -> &'static str {
        "Vec2 Length"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<Vec2>("vector")
            .with_output::<f32>("length")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let v: Vec2 = ctx.input_or("vector", Vec2::ZERO);
            ctx.output("length", v.length());
        })
    }
}

/// Normalizes a Vec2.
pub struct Vec2NormalizeNode;

impl BuiltinNode for Vec2NormalizeNode {
    fn type_id() -> &'static str {
        "builtin.vec2_normalize"
    }

    fn display_name() -> &'static str {
        "Vec2 Normalize"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<Vec2>("vector")
            .with_output::<Vec2>("normalized")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let v: Vec2 = ctx.input_or("vector", Vec2::new(1.0, 0.0));
            ctx.output("normalized", v.normalize());
        })
    }
}

/// Computes the dot product of two Vec2s.
pub struct Vec2DotNode;

impl BuiltinNode for Vec2DotNode {
    fn type_id() -> &'static str {
        "builtin.vec2_dot"
    }

    fn display_name() -> &'static str {
        "Vec2 Dot"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<Vec2>("a")
            .with_input::<Vec2>("b")
            .with_output::<f32>("dot")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let a: Vec2 = ctx.input_or("a", Vec2::ZERO);
            let b: Vec2 = ctx.input_or("b", Vec2::ZERO);
            ctx.output("dot", a.x * b.x + a.y * b.y);
        })
    }
}

/// Linear interpolation between two Vec2s.
pub struct Vec2LerpNode;

impl BuiltinNode for Vec2LerpNode {
    fn type_id() -> &'static str {
        "builtin.vec2_lerp"
    }

    fn display_name() -> &'static str {
        "Vec2 Lerp"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<Vec2>("a")
            .with_input::<Vec2>("b")
            .with_input::<f32>("t")
            .with_output::<Vec2>("result")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let a: Vec2 = ctx.input_or("a", Vec2::ZERO);
            let b: Vec2 = ctx.input_or("b", Vec2::ONE);
            let t: f32 = ctx.input_or("t", 0.5).clamp(0.0, 1.0);
            ctx.output("result", Vec2::new(
                a.x + (b.x - a.x) * t,
                a.y + (b.y - a.y) * t,
            ));
        })
    }
}

/// Computes the distance between two Vec2 points.
pub struct Vec2DistanceNode;

impl BuiltinNode for Vec2DistanceNode {
    fn type_id() -> &'static str {
        "builtin.vec2_distance"
    }

    fn display_name() -> &'static str {
        "Vec2 Distance"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<Vec2>("a")
            .with_input::<Vec2>("b")
            .with_output::<f32>("distance")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let a: Vec2 = ctx.input_or("a", Vec2::ZERO);
            let b: Vec2 = ctx.input_or("b", Vec2::ZERO);
            let dx = b.x - a.x;
            let dy = b.y - a.y;
            ctx.output("distance", (dx * dx + dy * dy).sqrt());
        })
    }
}

// ============================================================================
// Additional Vec3 Operations
// ============================================================================

/// Subtracts two Vec3 values (a - b).
pub struct Vec3SubtractNode;

impl BuiltinNode for Vec3SubtractNode {
    fn type_id() -> &'static str {
        "builtin.vec3_subtract"
    }

    fn display_name() -> &'static str {
        "Vec3 Subtract"
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
            ctx.output("result", Vec3::new(a.x - b.x, a.y - b.y, a.z - b.z));
        })
    }
}

/// Component-wise multiplication of two Vec3s.
pub struct Vec3MultiplyNode;

impl BuiltinNode for Vec3MultiplyNode {
    fn type_id() -> &'static str {
        "builtin.vec3_multiply"
    }

    fn display_name() -> &'static str {
        "Vec3 Multiply"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<Vec3>("a")
            .with_input::<Vec3>("b")
            .with_output::<Vec3>("result")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let a: Vec3 = ctx.input_or("a", Vec3::ONE);
            let b: Vec3 = ctx.input_or("b", Vec3::ONE);
            ctx.output("result", Vec3::new(a.x * b.x, a.y * b.y, a.z * b.z));
        })
    }
}

/// Computes the distance between two Vec3 points.
pub struct Vec3DistanceNode;

impl BuiltinNode for Vec3DistanceNode {
    fn type_id() -> &'static str {
        "builtin.vec3_distance"
    }

    fn display_name() -> &'static str {
        "Vec3 Distance"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<Vec3>("a")
            .with_input::<Vec3>("b")
            .with_output::<f32>("distance")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let a: Vec3 = ctx.input_or("a", Vec3::ZERO);
            let b: Vec3 = ctx.input_or("b", Vec3::ZERO);
            let dx = b.x - a.x;
            let dy = b.y - a.y;
            let dz = b.z - a.z;
            ctx.output("distance", (dx * dx + dy * dy + dz * dz).sqrt());
        })
    }
}

/// Negates a Vec3.
pub struct Vec3NegateNode;

impl BuiltinNode for Vec3NegateNode {
    fn type_id() -> &'static str {
        "builtin.vec3_negate"
    }

    fn display_name() -> &'static str {
        "Vec3 Negate"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<Vec3>("vector")
            .with_output::<Vec3>("result")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let v: Vec3 = ctx.input_or("vector", Vec3::ZERO);
            ctx.output("result", Vec3::new(-v.x, -v.y, -v.z));
        })
    }
}

/// Reflects a Vec3 around a normal.
pub struct Vec3ReflectNode;

impl BuiltinNode for Vec3ReflectNode {
    fn type_id() -> &'static str {
        "builtin.vec3_reflect"
    }

    fn display_name() -> &'static str {
        "Vec3 Reflect"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<Vec3>("vector")
            .with_input::<Vec3>("normal")
            .with_output::<Vec3>("result")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let v: Vec3 = ctx.input_or("vector", Vec3::ZERO);
            let n: Vec3 = ctx.input_or("normal", Vec3::UP);
            // reflect = v - 2 * dot(v, n) * n
            let dot = v.x * n.x + v.y * n.y + v.z * n.z;
            let factor = 2.0 * dot;
            ctx.output("result", Vec3::new(
                v.x - factor * n.x,
                v.y - factor * n.y,
                v.z - factor * n.z,
            ));
        })
    }
}

/// Projects a Vec3 onto another Vec3.
pub struct Vec3ProjectNode;

impl BuiltinNode for Vec3ProjectNode {
    fn type_id() -> &'static str {
        "builtin.vec3_project"
    }

    fn display_name() -> &'static str {
        "Vec3 Project"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<Vec3>("vector")
            .with_input::<Vec3>("onto")
            .with_output::<Vec3>("result")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let v: Vec3 = ctx.input_or("vector", Vec3::ZERO);
            let onto: Vec3 = ctx.input_or("onto", Vec3::new(1.0, 0.0, 0.0));
            // project = onto * (dot(v, onto) / dot(onto, onto))
            let onto_dot = onto.x * onto.x + onto.y * onto.y + onto.z * onto.z;
            if onto_dot < f32::EPSILON {
                ctx.output("result", Vec3::ZERO);
            } else {
                let v_dot_onto = v.x * onto.x + v.y * onto.y + v.z * onto.z;
                let factor = v_dot_onto / onto_dot;
                ctx.output("result", Vec3::new(
                    onto.x * factor,
                    onto.y * factor,
                    onto.z * factor,
                ));
            }
        })
    }
}

// ============================================================================
// Mat4 Operations
// ============================================================================

/// Creates an identity Mat4.
pub struct Mat4IdentityNode;

impl BuiltinNode for Mat4IdentityNode {
    fn type_id() -> &'static str {
        "builtin.mat4_identity"
    }

    fn display_name() -> &'static str {
        "Mat4 Identity"
    }

    fn create_node() -> Node {
        Node::new()
            .with_output::<Mat4>("matrix")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            ctx.output("matrix", Mat4::IDENTITY);
        })
    }
}

/// Creates a translation Mat4.
pub struct Mat4TranslationNode;

impl BuiltinNode for Mat4TranslationNode {
    fn type_id() -> &'static str {
        "builtin.mat4_translation"
    }

    fn display_name() -> &'static str {
        "Mat4 Translation"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<Vec3>("translation")
            .with_output::<Mat4>("matrix")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let t: Vec3 = ctx.input_or("translation", Vec3::ZERO);
            ctx.output("matrix", Mat4::translation(t.x, t.y, t.z));
        })
    }
}

/// Creates a scale Mat4.
pub struct Mat4ScaleNode;

impl BuiltinNode for Mat4ScaleNode {
    fn type_id() -> &'static str {
        "builtin.mat4_scale"
    }

    fn display_name() -> &'static str {
        "Mat4 Scale"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<Vec3>("scale")
            .with_output::<Mat4>("matrix")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let s: Vec3 = ctx.input_or("scale", Vec3::ONE);
            ctx.output("matrix", Mat4::scale(s.x, s.y, s.z));
        })
    }
}

/// Creates a Y-axis rotation Mat4.
pub struct Mat4RotationYNode;

impl BuiltinNode for Mat4RotationYNode {
    fn type_id() -> &'static str {
        "builtin.mat4_rotation_y"
    }

    fn display_name() -> &'static str {
        "Mat4 Rotation Y"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<f32>("angle")
            .with_output::<Mat4>("matrix")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let angle: f32 = ctx.input_or("angle", 0.0);
            ctx.output("matrix", Mat4::rotation_y(angle));
        })
    }
}

/// Creates an X-axis rotation Mat4.
pub struct Mat4RotationXNode;

impl BuiltinNode for Mat4RotationXNode {
    fn type_id() -> &'static str {
        "builtin.mat4_rotation_x"
    }

    fn display_name() -> &'static str {
        "Mat4 Rotation X"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<f32>("angle")
            .with_output::<Mat4>("matrix")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let angle: f32 = ctx.input_or("angle", 0.0);
            let c = angle.cos();
            let s = angle.sin();
            let matrix = Mat4 {
                cols: [
                    [1.0, 0.0, 0.0, 0.0],
                    [0.0, c, s, 0.0],
                    [0.0, -s, c, 0.0],
                    [0.0, 0.0, 0.0, 1.0],
                ],
            };
            ctx.output("matrix", matrix);
        })
    }
}

/// Creates a Z-axis rotation Mat4.
pub struct Mat4RotationZNode;

impl BuiltinNode for Mat4RotationZNode {
    fn type_id() -> &'static str {
        "builtin.mat4_rotation_z"
    }

    fn display_name() -> &'static str {
        "Mat4 Rotation Z"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<f32>("angle")
            .with_output::<Mat4>("matrix")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let angle: f32 = ctx.input_or("angle", 0.0);
            let c = angle.cos();
            let s = angle.sin();
            let matrix = Mat4 {
                cols: [
                    [c, s, 0.0, 0.0],
                    [-s, c, 0.0, 0.0],
                    [0.0, 0.0, 1.0, 0.0],
                    [0.0, 0.0, 0.0, 1.0],
                ],
            };
            ctx.output("matrix", matrix);
        })
    }
}

/// Multiplies two Mat4 matrices.
pub struct Mat4MultiplyNode;

impl BuiltinNode for Mat4MultiplyNode {
    fn type_id() -> &'static str {
        "builtin.mat4_multiply"
    }

    fn display_name() -> &'static str {
        "Mat4 Multiply"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<Mat4>("a")
            .with_input::<Mat4>("b")
            .with_output::<Mat4>("result")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let a: Mat4 = ctx.input_or("a", Mat4::IDENTITY);
            let b: Mat4 = ctx.input_or("b", Mat4::IDENTITY);
            ctx.output("result", a.mul(&b));
        })
    }
}

/// Transforms a point by a Mat4.
pub struct Mat4TransformPointNode;

impl BuiltinNode for Mat4TransformPointNode {
    fn type_id() -> &'static str {
        "builtin.mat4_transform_point"
    }

    fn display_name() -> &'static str {
        "Mat4 Transform Point"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<Mat4>("matrix")
            .with_input::<Vec3>("point")
            .with_output::<Vec3>("result")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let m: Mat4 = ctx.input_or("matrix", Mat4::IDENTITY);
            let p: Vec3 = ctx.input_or("point", Vec3::ZERO);
            // Transform point (w=1)
            let x = m.cols[0][0] * p.x + m.cols[1][0] * p.y + m.cols[2][0] * p.z + m.cols[3][0];
            let y = m.cols[0][1] * p.x + m.cols[1][1] * p.y + m.cols[2][1] * p.z + m.cols[3][1];
            let z = m.cols[0][2] * p.x + m.cols[1][2] * p.y + m.cols[2][2] * p.z + m.cols[3][2];
            let w = m.cols[0][3] * p.x + m.cols[1][3] * p.y + m.cols[2][3] * p.z + m.cols[3][3];
            if w.abs() > f32::EPSILON {
                ctx.output("result", Vec3::new(x / w, y / w, z / w));
            } else {
                ctx.output("result", Vec3::new(x, y, z));
            }
        })
    }
}

/// Transforms a direction by a Mat4 (ignores translation).
pub struct Mat4TransformDirectionNode;

impl BuiltinNode for Mat4TransformDirectionNode {
    fn type_id() -> &'static str {
        "builtin.mat4_transform_direction"
    }

    fn display_name() -> &'static str {
        "Mat4 Transform Dir"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<Mat4>("matrix")
            .with_input::<Vec3>("direction")
            .with_output::<Vec3>("result")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let m: Mat4 = ctx.input_or("matrix", Mat4::IDENTITY);
            let d: Vec3 = ctx.input_or("direction", Vec3::FORWARD);
            // Transform direction (w=0, ignores translation)
            let x = m.cols[0][0] * d.x + m.cols[1][0] * d.y + m.cols[2][0] * d.z;
            let y = m.cols[0][1] * d.x + m.cols[1][1] * d.y + m.cols[2][1] * d.z;
            let z = m.cols[0][2] * d.x + m.cols[1][2] * d.y + m.cols[2][2] * d.z;
            ctx.output("result", Vec3::new(x, y, z));
        })
    }
}

/// Composes a TRS (Translation, Rotation, Scale) Mat4.
pub struct Mat4ComposeNode;

impl BuiltinNode for Mat4ComposeNode {
    fn type_id() -> &'static str {
        "builtin.mat4_compose"
    }

    fn display_name() -> &'static str {
        "Mat4 Compose TRS"
    }

    fn create_node() -> Node {
        Node::new()
            .with_input::<Vec3>("translation")
            .with_input::<Vec3>("rotation")
            .with_input::<Vec3>("scale")
            .with_output::<Mat4>("matrix")
    }

    fn create_trigger() -> OnTrigger {
        OnTrigger::run(|ctx| {
            let t: Vec3 = ctx.input_or("translation", Vec3::ZERO);
            let r: Vec3 = ctx.input_or("rotation", Vec3::ZERO); // Euler angles (x, y, z)
            let s: Vec3 = ctx.input_or("scale", Vec3::ONE);

            // Build rotation matrices
            let (sx, cx) = (r.x.sin(), r.x.cos());
            let (sy, cy) = (r.y.sin(), r.y.cos());
            let (sz, cz) = (r.z.sin(), r.z.cos());

            // Combined rotation: Rz * Ry * Rx (in that order)
            // With scale applied
            let matrix = Mat4 {
                cols: [
                    [
                        s.x * (cy * cz),
                        s.x * (cy * sz),
                        s.x * (-sy),
                        0.0,
                    ],
                    [
                        s.y * (sx * sy * cz - cx * sz),
                        s.y * (sx * sy * sz + cx * cz),
                        s.y * (sx * cy),
                        0.0,
                    ],
                    [
                        s.z * (cx * sy * cz + sx * sz),
                        s.z * (cx * sy * sz - sx * cz),
                        s.z * (cx * cy),
                        0.0,
                    ],
                    [t.x, t.y, t.z, 1.0],
                ],
            };
            ctx.output("matrix", matrix);
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::World;
    use crate::nodegraph::builtin::BuiltinNode;

    #[test]
    fn test_add_node() {
        let mut world = World::new();
        let entity = AddNode::spawn(&mut world);

        let node = world.get::<Node>(entity).unwrap();
        assert_eq!(node.inputs().count(), 2);
        assert_eq!(node.outputs().count(), 1);
    }

    #[test]
    fn test_lerp_node() {
        let mut world = World::new();
        let entity = LerpNode::spawn(&mut world);

        let node = world.get::<Node>(entity).unwrap();
        assert_eq!(node.inputs().count(), 3);
        assert!(node.port("t").is_some());
    }
}

