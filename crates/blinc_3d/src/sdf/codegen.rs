//! SDF WGSL code generation

use super::{operations::SdfDomainOp, SdfNode, SdfNodeContent, SdfOp, SdfPrimitive, SdfScene};

/// SDF code generator
pub struct SdfCodegen;

impl SdfCodegen {
    /// Generate complete WGSL shader for an SDF scene
    pub fn generate(scene: &SdfScene) -> String {
        let mut code = String::new();

        // Add primitive function definitions
        code.push_str(SdfPrimitive::all_wgsl_definitions());
        code.push_str("\n");

        // Add operation function definitions
        code.push_str(SdfOp::all_wgsl_definitions());
        code.push_str("\n");

        // Add domain operation function definitions
        code.push_str(SdfDomainOp::all_wgsl_definitions());
        code.push_str("\n");

        // Generate the scene mapping function
        code.push_str("// Scene mapping function\n");
        code.push_str("fn map_scene(p: vec3<f32>) -> f32 {\n");

        if let Some(root) = scene.root() {
            let (result_var, node_code) = Self::generate_node(root, "p", 0);
            code.push_str(&node_code);
            code.push_str(&format!("    return {};\n", result_var));
        } else {
            code.push_str("    return 1000.0; // Empty scene\n");
        }

        code.push_str("}\n");

        code
    }

    /// Generate code for a single SDF node
    fn generate_node(node: &SdfNode, point_var: &str, depth: usize) -> (String, String) {
        let indent = "    ".repeat(depth + 1);
        let mut code = String::new();

        // Apply transform if non-identity
        let transformed_point = if node.transform.position != blinc_core::Vec3::ZERO
            || node.transform.rotation != blinc_core::Vec3::ZERO
            || node.transform.scale != blinc_core::Vec3::ONE
        {
            let p_var = format!("p_{}", node.id);

            // Start with the input point
            let mut current = point_var.to_string();

            // Apply translation
            if node.transform.position != blinc_core::Vec3::ZERO {
                current = format!(
                    "{} - vec3<f32>({}, {}, {})",
                    current,
                    node.transform.position.x,
                    node.transform.position.y,
                    node.transform.position.z
                );
            }

            // Apply rotations (ZYX order for proper Euler angles)
            if node.transform.rotation.z != 0.0 {
                current = format!("op_rotate_z({}, {})", current, node.transform.rotation.z);
            }
            if node.transform.rotation.y != 0.0 {
                current = format!("op_rotate_y({}, {})", current, node.transform.rotation.y);
            }
            if node.transform.rotation.x != 0.0 {
                current = format!("op_rotate_x({}, {})", current, node.transform.rotation.x);
            }

            // Apply scale
            if node.transform.scale != blinc_core::Vec3::ONE {
                if node.transform.scale.x == node.transform.scale.y
                    && node.transform.scale.y == node.transform.scale.z
                {
                    // Uniform scale
                    current = format!("{} / {}", current, node.transform.scale.x);
                } else {
                    // Non-uniform scale (approximation)
                    current = format!(
                        "{} / vec3<f32>({}, {}, {})",
                        current,
                        node.transform.scale.x,
                        node.transform.scale.y,
                        node.transform.scale.z
                    );
                }
            }

            code.push_str(&format!("{}let {} = {};\n", indent, p_var, current));
            p_var
        } else {
            point_var.to_string()
        };

        // Generate the SDF evaluation
        let result_var = format!("d_{}", node.id);

        match &node.content {
            SdfNodeContent::Primitive(primitive) => {
                let sdf_call = primitive.to_wgsl(&transformed_point);

                // Apply scale correction if non-uniform scale was used
                let scale_factor = if node.transform.scale != blinc_core::Vec3::ONE {
                    if node.transform.scale.x == node.transform.scale.y
                        && node.transform.scale.y == node.transform.scale.z
                    {
                        format!(" * {}", node.transform.scale.x)
                    } else {
                        // Use minimum scale for conservative distance
                        let min_scale = node
                            .transform
                            .scale
                            .x
                            .min(node.transform.scale.y)
                            .min(node.transform.scale.z);
                        format!(" * {}", min_scale)
                    }
                } else {
                    String::new()
                };

                code.push_str(&format!(
                    "{}let {} = {}{};\n",
                    indent, result_var, sdf_call, scale_factor
                ));
            }
            SdfNodeContent::Operation { op, left, right } => {
                // Generate left subtree
                let (left_var, left_code) =
                    Self::generate_node(left, &transformed_point, depth + 1);
                code.push_str(&left_code);

                // Generate right subtree
                let (right_var, right_code) =
                    Self::generate_node(right, &transformed_point, depth + 1);
                code.push_str(&right_code);

                // Combine with operation
                let op_call = op.to_wgsl(&left_var, &right_var);
                code.push_str(&format!("{}let {} = {};\n", indent, result_var, op_call));
            }
        }

        (result_var, code)
    }

    /// Generate a complete WGSL shader file with the scene
    pub fn generate_full_shader(scene: &SdfScene) -> String {
        let mut shader = String::new();

        // Uniforms
        shader.push_str(
            r#"
struct SdfUniform {
    camera_pos: vec4<f32>,
    camera_dir: vec4<f32>,
    camera_up: vec4<f32>,
    camera_right: vec4<f32>,
    resolution: vec2<f32>,
    time: f32,
    fov: f32,
    max_steps: u32,
    max_distance: f32,
    epsilon: f32,
    _padding: f32,
}

@group(0) @binding(0) var<uniform> sdf: SdfUniform;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    let x = f32(i32(vertex_index) - 1);
    let y = f32(i32(vertex_index & 1u) * 2 - 1);
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>(x, -y) * 0.5 + 0.5;
    return out;
}

"#,
        );

        // Scene-specific code
        shader.push_str(&Self::generate(scene));

        // Raymarching and lighting
        shader.push_str(
            r#"
fn raymarch(ro: vec3<f32>, rd: vec3<f32>) -> f32 {
    var t = 0.0;
    for (var i = 0u; i < sdf.max_steps; i++) {
        let p = ro + rd * t;
        let d = map_scene(p);
        if (d < sdf.epsilon) { return t; }
        if (t > sdf.max_distance) { break; }
        t += d;
    }
    return -1.0;
}

fn calc_normal(p: vec3<f32>) -> vec3<f32> {
    let e = vec2<f32>(sdf.epsilon, 0.0);
    return normalize(vec3<f32>(
        map_scene(p + e.xyy) - map_scene(p - e.xyy),
        map_scene(p + e.yxy) - map_scene(p - e.yxy),
        map_scene(p + e.yyx) - map_scene(p - e.yyx)
    ));
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let aspect = sdf.resolution.x / sdf.resolution.y;
    let uv = (in.uv * 2.0 - 1.0) * vec2<f32>(aspect, 1.0);

    let ro = sdf.camera_pos.xyz;
    let rd = normalize(
        sdf.camera_dir.xyz +
        sdf.camera_right.xyz * uv.x * tan(sdf.fov * 0.5) +
        sdf.camera_up.xyz * uv.y * tan(sdf.fov * 0.5)
    );

    let t = raymarch(ro, rd);

    if (t < 0.0) {
        let sky = mix(vec3<f32>(0.5, 0.7, 1.0), vec3<f32>(0.1, 0.2, 0.4), rd.y * 0.5 + 0.5);
        return vec4<f32>(sky, 1.0);
    }

    let p = ro + rd * t;
    let n = calc_normal(p);
    let light_dir = normalize(vec3<f32>(1.0, 1.0, 1.0));
    let diff = max(dot(n, light_dir), 0.0);
    let color = vec3<f32>(0.8) * (0.2 + 0.8 * diff);

    return vec4<f32>(pow(color, vec3<f32>(1.0 / 2.2)), 1.0);
}
"#,
        );

        shader
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blinc_core::Vec3;

    #[test]
    fn test_simple_sphere() {
        let mut scene = SdfScene::new();
        scene.set_root(SdfScene::sphere(1.0));

        let code = scene.to_wgsl();
        assert!(code.contains("sdf_sphere"));
        assert!(code.contains("map_scene"));
    }

    #[test]
    fn test_union() {
        let mut scene = SdfScene::new();
        let sphere = SdfScene::sphere(1.0).at(Vec3::new(-1.0, 0.0, 0.0));
        let cube = SdfScene::cube(1.0).at(Vec3::new(1.0, 0.0, 0.0));
        scene.set_root(SdfScene::union(sphere, cube));

        let code = scene.to_wgsl();
        assert!(code.contains("sdf_sphere"));
        assert!(code.contains("sdf_box"));
        assert!(code.contains("op_union"));
    }

    #[test]
    fn test_smooth_union() {
        let mut scene = SdfScene::new();
        let sphere = SdfScene::sphere(1.0);
        let cube = SdfScene::cube(1.0);
        scene.set_root(SdfScene::smooth_union(sphere, cube, 0.5));

        let code = scene.to_wgsl();
        assert!(code.contains("op_smooth_union"));
    }
}
