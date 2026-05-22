//! FlowGraph → WGSL code generation
//!
//! Compiles `@flow` DAG blocks from CSS into executable WGSL shader modules.
//! Supports both fragment (rendering) and compute (simulation) targets.
//!
//! The codegen pipeline:
//! 1. Collect which helper functions are needed (SDF, noise, etc.)
//! 2. Emit uniform/binding declarations
//! 3. Emit only the used helper functions
//! 4. Emit vertex shader (for fragment flows)
//! 5. Emit entry point with node computations in topological order

use std::collections::HashSet;
use std::fmt::Write;

use blinc_core::{
    FlowError, FlowExpr, FlowFunc, FlowGraph, FlowInput, FlowInputSource, FlowNode, FlowOutput,
    FlowOutputTarget, FlowTarget, FlowType,
};

// ==========================================================================
// Public API
// ==========================================================================

/// Compile a validated FlowGraph into a complete WGSL module string.
///
/// The graph MUST be validated (via `graph.validate()`) before calling this.
/// Returns an error if code generation fails (e.g., unsupported constructs).
pub fn flow_to_wgsl(graph: &FlowGraph) -> Result<String, FlowError> {
    let mut ctx = CodegenContext::new(graph);
    ctx.collect_used_helpers();
    ctx.generate()
}

/// Check if a flow graph uses the `sample_scene()` function,
/// which requires binding a scene texture to the pipeline.
pub fn flow_needs_scene_texture(graph: &FlowGraph) -> bool {
    fn expr_uses_scene(expr: &FlowExpr) -> bool {
        match expr {
            FlowExpr::Call { func, args } => {
                if *func == FlowFunc::SampleScene {
                    return true;
                }
                args.iter().any(expr_uses_scene)
            }
            FlowExpr::Add(a, b)
            | FlowExpr::Sub(a, b)
            | FlowExpr::Mul(a, b)
            | FlowExpr::Div(a, b)
            | FlowExpr::Vec2(a, b) => expr_uses_scene(a) || expr_uses_scene(b),
            FlowExpr::Vec3(a, b, c) => {
                expr_uses_scene(a) || expr_uses_scene(b) || expr_uses_scene(c)
            }
            FlowExpr::Vec4(a, b, c, d) => {
                expr_uses_scene(a) || expr_uses_scene(b) || expr_uses_scene(c) || expr_uses_scene(d)
            }
            FlowExpr::Neg(a) | FlowExpr::Swizzle(a, _) => expr_uses_scene(a),
            _ => false,
        }
    }

    for node in &graph.nodes {
        if expr_uses_scene(&node.expr) {
            return true;
        }
    }
    for output in &graph.outputs {
        if let Some(expr) = &output.expr {
            if expr_uses_scene(expr) {
                return true;
            }
        }
    }
    false
}

// ==========================================================================
// Type Mapping
// ==========================================================================

fn flow_type_to_wgsl(ty: FlowType) -> &'static str {
    match ty {
        FlowType::Float => "f32",
        FlowType::Vec2 => "vec2<f32>",
        FlowType::Vec3 => "vec3<f32>",
        FlowType::Vec4 => "vec4<f32>",
        FlowType::Mat4 => "mat4x4<f32>",
    }
}

// ==========================================================================
// Codegen Context
// ==========================================================================

/// Tracks state during WGSL code generation
struct CodegenContext<'a> {
    graph: &'a FlowGraph,
    /// Which helper function categories are needed
    need_sdf: bool,
    need_sdf_ops: bool,
    need_noise: bool,
    need_lighting: bool,
    need_color: bool,
    need_simulation: bool,
    need_scene: bool,
    /// Individual SDF primitives needed
    used_sdf_prims: HashSet<&'static str>,
    /// Individual SDF ops needed
    used_sdf_ops: HashSet<&'static str>,
    /// Whether any buffer inputs/outputs exist
    has_buffer_io: bool,
    /// Tracks storage buffer bindings by name
    buffer_bindings: Vec<(String, FlowType, bool)>, // (name, type, is_readwrite)
}

impl<'a> CodegenContext<'a> {
    fn new(graph: &'a FlowGraph) -> Self {
        Self {
            graph,
            need_sdf: false,
            need_sdf_ops: false,
            need_noise: false,
            need_lighting: false,
            need_color: false,
            need_simulation: false,
            need_scene: false,
            used_sdf_prims: HashSet::new(),
            used_sdf_ops: HashSet::new(),
            has_buffer_io: false,
            buffer_bindings: Vec::new(),
        }
    }

    /// Walk the graph and determine which helper functions are needed
    fn collect_used_helpers(&mut self) {
        for node in &self.graph.nodes {
            self.scan_expr_helpers(&node.expr);
        }
        for output in &self.graph.outputs {
            if let Some(expr) = &output.expr {
                self.scan_expr_helpers(expr);
            }
        }

        // Check for buffer I/O
        for input in &self.graph.inputs {
            if matches!(input.source, FlowInputSource::Buffer { .. }) {
                self.has_buffer_io = true;
                if let FlowInputSource::Buffer { name, ty } = &input.source {
                    self.buffer_bindings.push((name.clone(), *ty, false)); // read-only
                }
            }
        }
        for output in &self.graph.outputs {
            if let FlowOutputTarget::Buffer { name } = &output.target {
                self.has_buffer_io = true;
                // Check if already exists as input (read-write) or new (write-only)
                let existing = self.buffer_bindings.iter_mut().find(|(n, _, _)| n == name);
                if let Some(entry) = existing {
                    entry.2 = true; // promote to read-write
                } else {
                    self.buffer_bindings
                        .push((name.clone(), FlowType::Vec4, true));
                }
            }
        }
    }

    fn scan_expr_helpers(&mut self, expr: &FlowExpr) {
        match expr {
            FlowExpr::Call { func, args } => {
                self.mark_func_needed(*func);
                for arg in args {
                    self.scan_expr_helpers(arg);
                }
            }
            FlowExpr::Add(a, b)
            | FlowExpr::Sub(a, b)
            | FlowExpr::Mul(a, b)
            | FlowExpr::Div(a, b)
            | FlowExpr::Vec2(a, b) => {
                self.scan_expr_helpers(a);
                self.scan_expr_helpers(b);
            }
            FlowExpr::Vec3(a, b, c) => {
                self.scan_expr_helpers(a);
                self.scan_expr_helpers(b);
                self.scan_expr_helpers(c);
            }
            FlowExpr::Vec4(a, b, c, d) => {
                self.scan_expr_helpers(a);
                self.scan_expr_helpers(b);
                self.scan_expr_helpers(c);
                self.scan_expr_helpers(d);
            }
            FlowExpr::Neg(a) | FlowExpr::Swizzle(a, _) => self.scan_expr_helpers(a),
            _ => {}
        }
    }

    fn mark_func_needed(&mut self, func: FlowFunc) {
        match func {
            FlowFunc::SdfBox => {
                self.need_sdf = true;
                self.used_sdf_prims.insert("sdf_box");
            }
            FlowFunc::SdfCircle => {
                self.need_sdf = true;
                self.used_sdf_prims.insert("sdf_circle");
            }
            FlowFunc::SdfEllipse => {
                self.need_sdf = true;
                self.used_sdf_prims.insert("sdf_ellipse");
            }
            FlowFunc::SdfRoundRect => {
                self.need_sdf = true;
                self.used_sdf_prims.insert("sdf_round_rect");
            }
            FlowFunc::SdfUnion => {
                self.need_sdf_ops = true;
                self.used_sdf_ops.insert("sdf_union");
            }
            FlowFunc::SdfIntersect => {
                self.need_sdf_ops = true;
                self.used_sdf_ops.insert("sdf_intersect");
            }
            FlowFunc::SdfSubtract => {
                self.need_sdf_ops = true;
                self.used_sdf_ops.insert("sdf_subtract");
            }
            FlowFunc::SdfSmoothUnion => {
                self.need_sdf_ops = true;
                self.used_sdf_ops.insert("sdf_smooth_union");
            }
            FlowFunc::SdfSmoothIntersect => {
                self.need_sdf_ops = true;
                self.used_sdf_ops.insert("sdf_smooth_intersect");
            }
            FlowFunc::SdfSmoothSubtract => {
                self.need_sdf_ops = true;
                self.used_sdf_ops.insert("sdf_smooth_subtract");
            }
            FlowFunc::Perlin
            | FlowFunc::Simplex
            | FlowFunc::Worley
            | FlowFunc::WorleyGrad
            | FlowFunc::Fbm
            | FlowFunc::FbmEx
            | FlowFunc::Checkerboard => {
                self.need_noise = true;
            }
            FlowFunc::Phong | FlowFunc::BlinnPhong => {
                self.need_lighting = true;
            }
            FlowFunc::SampleScene => {
                self.need_scene = true;
            }
            FlowFunc::SpringEval | FlowFunc::WaveStep | FlowFunc::FluidStep => {
                self.need_simulation = true;
            }
            _ => {}
        }
    }

    // ======================================================================
    // Full module generation
    // ======================================================================

    fn generate(&self) -> Result<String, FlowError> {
        let mut out = String::with_capacity(4096);
        let _ = writeln!(out, "// Auto-generated by Blinc @flow codegen");
        let _ = writeln!(out, "// Flow: {}", self.graph.name);
        let _ = writeln!(out);

        // Uniforms struct
        self.emit_uniforms(&mut out);

        // Bindings
        let _ = self.emit_bindings(&mut out);

        // Helper functions (only those needed)
        self.emit_helpers(&mut out);

        // Entry point(s)
        match self.graph.target {
            FlowTarget::Fragment => self.emit_fragment_shader(&mut out)?,
            FlowTarget::Compute => self.emit_compute_shader(&mut out)?,
            FlowTarget::Vertex => self.emit_vertex_3d_shader(&mut out)?,
            FlowTarget::Material => self.emit_material_shader(&mut out)?,
        }

        Ok(out)
    }

    // ======================================================================
    // Uniforms
    // ======================================================================

    fn emit_uniforms(&self, out: &mut String) {
        let _ = writeln!(out, "struct FlowUniforms {{");
        let _ = writeln!(out, "    viewport_size: vec2<f32>,");
        let _ = writeln!(out, "    time: f32,");
        let _ = writeln!(out, "    frame_index: f32,");
        let _ = writeln!(out, "    element_bounds: vec4<f32>,");
        let _ = writeln!(out, "    pointer: vec2<f32>,");
        let _ = writeln!(out, "    corner_radius: f32,");
        let _ = writeln!(out, "    pressure: f32,");

        // Add CSS property inputs as uniforms
        for input in &self.graph.inputs {
            if let FlowInputSource::CssProperty(prop) = &input.source {
                let ty = input.ty.unwrap_or(FlowType::Float);
                let wgsl_ty = flow_type_to_wgsl(ty);
                let safe_name = sanitize_name(prop);
                let _ = writeln!(out, "    css_{}: {},", safe_name, wgsl_ty);
            } else if let FlowInputSource::EnvVar(var) = &input.source {
                let ty = input.ty.unwrap_or(FlowType::Float);
                let wgsl_ty = flow_type_to_wgsl(ty);
                let safe_name = sanitize_name(var);
                let _ = writeln!(out, "    env_{}: {},", safe_name, wgsl_ty);
            }
        }

        let _ = writeln!(out, "}};");
        let _ = writeln!(out);
    }

    // ======================================================================
    // Bindings
    // ======================================================================

    /// Returns the next available binding index after all bindings are emitted
    fn emit_bindings(&self, out: &mut String) -> u32 {
        // group(0) binding(0) = uniforms
        let _ = writeln!(out, "@group(0) @binding(0) var<uniform> u: FlowUniforms;");

        // Storage buffers start at binding(1)
        let mut next_binding = 1u32;
        for (i, (name, _ty, is_rw)) in self.buffer_bindings.iter().enumerate() {
            let binding = i as u32 + 1;
            let access = if *is_rw { "read_write" } else { "read" };
            let safe_name = sanitize_name(name);
            let _ = writeln!(
                out,
                "@group(0) @binding({}) var<storage, {}> buf_{}: array<vec4<f32>>;",
                binding, access, safe_name
            );
            next_binding = binding + 1;
        }

        // Scene texture + sampler (for sample_scene())
        if self.need_scene {
            let _ = writeln!(
                out,
                "@group(0) @binding({}) var scene_tex: texture_2d<f32>;",
                next_binding
            );
            next_binding += 1;
            let _ = writeln!(
                out,
                "@group(0) @binding({}) var scene_sampler: sampler;",
                next_binding
            );
            next_binding += 1;
        }

        let _ = writeln!(out);
        next_binding
    }

    // ======================================================================
    // Helper Functions
    // ======================================================================

    fn emit_helpers(&self, out: &mut String) {
        if self.need_sdf {
            self.emit_sdf_helpers(out);
        }
        if self.need_sdf_ops {
            self.emit_sdf_ops(out);
        }
        if self.need_noise {
            self.emit_noise_helpers(out);
        }
        if self.need_lighting {
            self.emit_lighting_helpers(out);
        }
        if self.need_simulation {
            self.emit_simulation_helpers(out);
        }
    }

    fn emit_sdf_helpers(&self, out: &mut String) {
        let _ = writeln!(out, "// ---- SDF Primitives ----");

        if self.used_sdf_prims.contains("sdf_box") {
            let _ = writeln!(
                out,
                "fn flow_sdf_box(p: vec2<f32>, half_size: vec2<f32>) -> f32 {{"
            );
            let _ = writeln!(out, "    let d = abs(p) - half_size;");
            let _ = writeln!(
                out,
                "    return length(max(d, vec2<f32>(0.0))) + min(max(d.x, d.y), 0.0);"
            );
            let _ = writeln!(out, "}}");
            let _ = writeln!(out);
        }

        if self.used_sdf_prims.contains("sdf_circle") {
            let _ = writeln!(out, "fn flow_sdf_circle(p: vec2<f32>, r: f32) -> f32 {{");
            let _ = writeln!(out, "    return length(p) - r;");
            let _ = writeln!(out, "}}");
            let _ = writeln!(out);
        }

        if self.used_sdf_prims.contains("sdf_ellipse") {
            let _ = writeln!(
                out,
                "fn flow_sdf_ellipse(p: vec2<f32>, ab: vec2<f32>) -> f32 {{"
            );
            let _ = writeln!(out, "    let p2 = p * p;");
            let _ = writeln!(out, "    let ab2 = ab * ab;");
            let _ = writeln!(
                out,
                "    return (p2.x / ab2.x + p2.y / ab2.y - 1.0) * min(ab.x, ab.y);"
            );
            let _ = writeln!(out, "}}");
            let _ = writeln!(out);
        }

        if self.used_sdf_prims.contains("sdf_round_rect") {
            let _ = writeln!(
                out,
                "fn flow_sdf_round_rect(p: vec2<f32>, half_size: vec2<f32>, r: f32) -> f32 {{"
            );
            let _ = writeln!(out, "    let d = abs(p) - half_size + vec2<f32>(r);");
            let _ = writeln!(
                out,
                "    return length(max(d, vec2<f32>(0.0))) + min(max(d.x, d.y), 0.0) - r;"
            );
            let _ = writeln!(out, "}}");
            let _ = writeln!(out);
        }
    }

    fn emit_sdf_ops(&self, out: &mut String) {
        let _ = writeln!(out, "// ---- SDF Combinators ----");

        if self.used_sdf_ops.contains("sdf_union") {
            let _ = writeln!(out, "fn flow_sdf_union(a: f32, b: f32) -> f32 {{");
            let _ = writeln!(out, "    return min(a, b);");
            let _ = writeln!(out, "}}");
            let _ = writeln!(out);
        }

        if self.used_sdf_ops.contains("sdf_intersect") {
            let _ = writeln!(out, "fn flow_sdf_intersect(a: f32, b: f32) -> f32 {{");
            let _ = writeln!(out, "    return max(a, b);");
            let _ = writeln!(out, "}}");
            let _ = writeln!(out);
        }

        if self.used_sdf_ops.contains("sdf_subtract") {
            let _ = writeln!(out, "fn flow_sdf_subtract(a: f32, b: f32) -> f32 {{");
            let _ = writeln!(out, "    return max(a, -b);");
            let _ = writeln!(out, "}}");
            let _ = writeln!(out);
        }

        if self.used_sdf_ops.contains("sdf_smooth_union") {
            let _ = writeln!(
                out,
                "fn flow_sdf_smooth_union(a: f32, b: f32, k: f32) -> f32 {{"
            );
            let _ = writeln!(out, "    let h = clamp(0.5 + 0.5 * (b - a) / k, 0.0, 1.0);");
            let _ = writeln!(out, "    return mix(b, a, h) - k * h * (1.0 - h);");
            let _ = writeln!(out, "}}");
            let _ = writeln!(out);
        }

        if self.used_sdf_ops.contains("sdf_smooth_intersect") {
            let _ = writeln!(
                out,
                "fn flow_sdf_smooth_intersect(a: f32, b: f32, k: f32) -> f32 {{"
            );
            let _ = writeln!(out, "    let h = clamp(0.5 - 0.5 * (b - a) / k, 0.0, 1.0);");
            let _ = writeln!(out, "    return mix(b, a, h) + k * h * (1.0 - h);");
            let _ = writeln!(out, "}}");
            let _ = writeln!(out);
        }

        if self.used_sdf_ops.contains("sdf_smooth_subtract") {
            let _ = writeln!(
                out,
                "fn flow_sdf_smooth_subtract(a: f32, b: f32, k: f32) -> f32 {{"
            );
            let _ = writeln!(
                out,
                "    let h = clamp(0.5 - 0.5 * (-b - a) / k, 0.0, 1.0);"
            );
            let _ = writeln!(out, "    return mix(-b, a, h) + k * h * (1.0 - h);");
            let _ = writeln!(out, "}}");
            let _ = writeln!(out);
        }
    }

    fn emit_noise_helpers(&self, out: &mut String) {
        let _ = writeln!(out, "// ---- Noise Functions ----");

        // Hash functions used by all noise types
        let _ = writeln!(out, "fn flow_hash21(p: vec2<f32>) -> f32 {{");
        let _ = writeln!(
            out,
            "    var p3 = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);"
        );
        let _ = writeln!(
            out,
            "    p3 = p3 + vec3<f32>(dot(p3, vec3<f32>(p3.y + 33.33, p3.z + 33.33, p3.x + 33.33)));"
        );
        let _ = writeln!(out, "    return fract((p3.x + p3.y) * p3.z);");
        let _ = writeln!(out, "}}");
        let _ = writeln!(out);

        let _ = writeln!(out, "fn flow_hash22(p: vec2<f32>) -> vec2<f32> {{");
        let _ = writeln!(
            out,
            "    let n = vec3<f32>(dot(p, vec2<f32>(127.1, 311.7)), dot(p, vec2<f32>(269.5, 183.3)), 0.0);"
        );
        let _ = writeln!(
            out,
            "    return fract(sin(n.xy) * 43758.5453) * 2.0 - vec2<f32>(1.0);"
        );
        let _ = writeln!(out, "}}");
        let _ = writeln!(out);

        // Value noise (used as perlin base)
        let _ = writeln!(out, "fn flow_noise2d(p: vec2<f32>) -> f32 {{");
        let _ = writeln!(out, "    let i = floor(p);");
        let _ = writeln!(out, "    let f = fract(p);");
        let _ = writeln!(out, "    let u = f * f * (3.0 - 2.0 * f);");
        let _ = writeln!(out, "    return mix(");
        let _ = writeln!(
            out,
            "        mix(flow_hash21(i), flow_hash21(i + vec2<f32>(1.0, 0.0)), u.x),"
        );
        let _ = writeln!(
            out,
            "        mix(flow_hash21(i + vec2<f32>(0.0, 1.0)), flow_hash21(i + vec2<f32>(1.0, 1.0)), u.x),"
        );
        let _ = writeln!(out, "        u.y");
        let _ = writeln!(out, "    );");
        let _ = writeln!(out, "}}");
        let _ = writeln!(out);

        // FBM
        let _ = writeln!(out, "fn flow_fbm(p: vec2<f32>, octaves: i32) -> f32 {{");
        let _ = writeln!(out, "    var value = 0.0;");
        let _ = writeln!(out, "    var amplitude = 0.5;");
        let _ = writeln!(out, "    var pos = p;");
        let _ = writeln!(out, "    for (var i = 0; i < octaves; i = i + 1) {{");
        let _ = writeln!(
            out,
            "        value = value + amplitude * flow_noise2d(pos);"
        );
        let _ = writeln!(out, "        pos = pos * 2.0;");
        let _ = writeln!(out, "        amplitude = amplitude * 0.5;");
        let _ = writeln!(out, "    }}");
        let _ = writeln!(out, "    return value;");
        let _ = writeln!(out, "}}");
        let _ = writeln!(out);

        // Worley (cellular) noise
        let _ = writeln!(out, "fn flow_worley(p: vec2<f32>) -> f32 {{");
        let _ = writeln!(out, "    let i = floor(p);");
        let _ = writeln!(out, "    let f = fract(p);");
        let _ = writeln!(out, "    var min_dist = 1.0;");
        let _ = writeln!(out, "    for (var y = -1; y <= 1; y = y + 1) {{");
        let _ = writeln!(out, "        for (var x = -1; x <= 1; x = x + 1) {{");
        let _ = writeln!(out, "            let neighbor = vec2<f32>(f32(x), f32(y));");
        let _ = writeln!(
            out,
            "            let point = flow_hash22(i + neighbor) * 0.5 + vec2<f32>(0.5);"
        );
        let _ = writeln!(
            out,
            "            min_dist = min(min_dist, length(neighbor + point - f));"
        );
        let _ = writeln!(out, "        }}");
        let _ = writeln!(out, "    }}");
        let _ = writeln!(out, "    return min_dist;");
        let _ = writeln!(out, "}}");
        let _ = writeln!(out);

        // Worley with analytic gradient — returns vec3(distance, grad_x, grad_y)
        // Gradient is the direction away from the closest cell center (unit length).
        // Single pass replaces 5 separate worley() calls for finite-difference gradients.
        let _ = writeln!(out, "fn flow_worley_grad(p: vec2<f32>) -> vec3<f32> {{");
        let _ = writeln!(out, "    let i = floor(p);");
        let _ = writeln!(out, "    let f = fract(p);");
        let _ = writeln!(out, "    var min_dist = 1.0;");
        let _ = writeln!(out, "    var closest_delta = vec2<f32>(0.0);");
        let _ = writeln!(out, "    for (var y = -1; y <= 1; y = y + 1) {{");
        let _ = writeln!(out, "        for (var x = -1; x <= 1; x = x + 1) {{");
        let _ = writeln!(out, "            let neighbor = vec2<f32>(f32(x), f32(y));");
        let _ = writeln!(
            out,
            "            let point = flow_hash22(i + neighbor) * 0.5 + vec2<f32>(0.5);"
        );
        let _ = writeln!(out, "            let delta = neighbor + point - f;");
        let _ = writeln!(out, "            let d = length(delta);");
        let _ = writeln!(out, "            if (d < min_dist) {{");
        let _ = writeln!(out, "                min_dist = d;");
        let _ = writeln!(out, "                closest_delta = delta;");
        let _ = writeln!(out, "            }}");
        let _ = writeln!(out, "        }}");
        let _ = writeln!(out, "    }}");
        let _ = writeln!(
            out,
            "    let grad = -closest_delta / max(min_dist, 0.0001);"
        );
        let _ = writeln!(out, "    return vec3<f32>(min_dist, grad.x, grad.y);");
        let _ = writeln!(out, "}}");
        let _ = writeln!(out);

        // Extended FBM with configurable persistence (roughness)
        let _ = writeln!(
            out,
            "fn flow_fbm_ex(p: vec2<f32>, octaves: i32, persistence: f32) -> f32 {{"
        );
        let _ = writeln!(out, "    var value = 0.0;");
        let _ = writeln!(out, "    var amplitude = 0.5;");
        let _ = writeln!(out, "    var pos = p;");
        let _ = writeln!(out, "    for (var i = 0; i < octaves; i = i + 1) {{");
        let _ = writeln!(
            out,
            "        value = value + amplitude * flow_noise2d(pos);"
        );
        let _ = writeln!(out, "        pos = pos * 2.0;");
        let _ = writeln!(out, "        amplitude = amplitude * persistence;");
        let _ = writeln!(out, "    }}");
        let _ = writeln!(out, "    return value;");
        let _ = writeln!(out, "}}");
        let _ = writeln!(out);

        // Checkerboard pattern
        let _ = writeln!(
            out,
            "fn flow_checkerboard(p: vec2<f32>, scale: f32) -> f32 {{"
        );
        let _ = writeln!(out, "    let c = floor(p * scale) % vec2<f32>(2.0);");
        let _ = writeln!(out, "    return abs(c.x + c.y - 1.0);");
        let _ = writeln!(out, "}}");
        let _ = writeln!(out);
    }

    fn emit_lighting_helpers(&self, out: &mut String) {
        let _ = writeln!(out, "// ---- Lighting ----");

        let _ = writeln!(
            out,
            "fn flow_phong(normal: vec3<f32>, light_dir: vec3<f32>, view_dir: vec3<f32>) -> vec4<f32> {{"
        );
        let _ = writeln!(out, "    let ambient = 0.1;");
        let _ = writeln!(out, "    let diff = max(dot(normal, light_dir), 0.0);");
        let _ = writeln!(out, "    let reflect_dir = reflect(-light_dir, normal);");
        let _ = writeln!(
            out,
            "    let spec = pow(max(dot(view_dir, reflect_dir), 0.0), 32.0);"
        );
        let _ = writeln!(out, "    let intensity = ambient + diff + spec * 0.5;");
        let _ = writeln!(out, "    return vec4<f32>(vec3<f32>(intensity), 1.0);");
        let _ = writeln!(out, "}}");
        let _ = writeln!(out);

        let _ = writeln!(
            out,
            "fn flow_blinn_phong(normal: vec3<f32>, light_dir: vec3<f32>, view_dir: vec3<f32>) -> vec4<f32> {{"
        );
        let _ = writeln!(out, "    let ambient = 0.1;");
        let _ = writeln!(out, "    let diff = max(dot(normal, light_dir), 0.0);");
        let _ = writeln!(out, "    let half_dir = normalize(light_dir + view_dir);");
        let _ = writeln!(
            out,
            "    let spec = pow(max(dot(normal, half_dir), 0.0), 64.0);"
        );
        let _ = writeln!(out, "    let intensity = ambient + diff + spec * 0.5;");
        let _ = writeln!(out, "    return vec4<f32>(vec3<f32>(intensity), 1.0);");
        let _ = writeln!(out, "}}");
        let _ = writeln!(out);
    }

    fn emit_simulation_helpers(&self, out: &mut String) {
        let _ = writeln!(out, "// ---- Simulation Helpers ----");

        let _ = writeln!(
            out,
            "fn flow_spring_eval(pos: f32, vel: f32, target: f32, stiffness: f32, damping: f32) -> vec2<f32> {{"
        );
        let _ = writeln!(
            out,
            "    let force = -stiffness * (pos - target) - damping * vel;"
        );
        let _ = writeln!(
            out,
            "    return vec2<f32>(pos + vel * 0.016, vel + force * 0.016);"
        );
        let _ = writeln!(out, "}}");
        let _ = writeln!(out);

        let _ = writeln!(
            out,
            "fn flow_wave_step(height: f32, velocity: f32, neighbors_avg: f32, damping: f32) -> vec2<f32> {{"
        );
        let _ = writeln!(out, "    let accel = (neighbors_avg - height) * 2.0;");
        let _ = writeln!(out, "    let new_vel = (velocity + accel) * damping;");
        let _ = writeln!(
            out,
            "    return vec2<f32>(height + new_vel * 0.016, new_vel);"
        );
        let _ = writeln!(out, "}}");
        let _ = writeln!(out);
    }

    // ======================================================================
    // Fragment Shader
    // ======================================================================

    fn emit_fragment_shader(&self, out: &mut String) -> Result<(), FlowError> {
        // Vertex output struct
        let _ = writeln!(out, "struct VertexOutput {{");
        let _ = writeln!(out, "    @builtin(position) position: vec4<f32>,");
        let _ = writeln!(out, "    @location(0) uv: vec2<f32>,");
        let _ = writeln!(out, "}};");
        let _ = writeln!(out);

        // Fullscreen quad vertex shader
        let _ = writeln!(out, "@vertex");
        let _ = writeln!(
            out,
            "fn vs_main(@builtin(vertex_index) vi: u32) -> VertexOutput {{"
        );
        let _ = writeln!(out, "    var pos = array<vec2<f32>, 4>(");
        let _ = writeln!(out, "        vec2<f32>(-1.0, -1.0),");
        let _ = writeln!(out, "        vec2<f32>( 1.0, -1.0),");
        let _ = writeln!(out, "        vec2<f32>(-1.0,  1.0),");
        let _ = writeln!(out, "        vec2<f32>( 1.0,  1.0),");
        let _ = writeln!(out, "    );");
        let _ = writeln!(out, "    var uvs = array<vec2<f32>, 4>(");
        let _ = writeln!(out, "        vec2<f32>(0.0, 1.0),");
        let _ = writeln!(out, "        vec2<f32>(1.0, 1.0),");
        let _ = writeln!(out, "        vec2<f32>(0.0, 0.0),");
        let _ = writeln!(out, "        vec2<f32>(1.0, 0.0),");
        let _ = writeln!(out, "    );");
        let _ = writeln!(out, "    // Triangle strip indices: 0,1,2, 2,1,3");
        let _ = writeln!(out, "    let idx = array<u32, 6>(0u, 1u, 2u, 2u, 1u, 3u);");
        let _ = writeln!(out, "    let i = idx[vi];");
        let _ = writeln!(out, "    var output: VertexOutput;");
        let _ = writeln!(out, "    output.position = vec4<f32>(pos[i], 0.0, 1.0);");
        let _ = writeln!(out, "    output.uv = uvs[i];");
        let _ = writeln!(out, "    return output;");
        let _ = writeln!(out, "}}");
        let _ = writeln!(out);

        // Fragment entry point
        let _ = writeln!(out, "@fragment");
        let _ = writeln!(
            out,
            "fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {{"
        );

        // Bind builtin inputs
        self.emit_input_bindings(out, "    ");

        // Emit node computations in topological order
        self.emit_node_computations(out, "    ")?;

        // Emit output
        self.emit_fragment_output(out, "    ")?;

        let _ = writeln!(out, "}}");

        Ok(())
    }

    // ======================================================================
    // Compute Shader
    // ======================================================================

    fn emit_compute_shader(&self, out: &mut String) -> Result<(), FlowError> {
        let wg_size = self.graph.workgroup_size.unwrap_or(64);

        let _ = writeln!(out, "@compute @workgroup_size({})", wg_size);
        let _ = writeln!(
            out,
            "fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {{"
        );
        let _ = writeln!(out, "    let idx = gid.x;");

        // Bind builtin inputs
        self.emit_input_bindings(out, "    ");

        // Emit node computations in topological order
        self.emit_node_computations(out, "    ")?;

        // Emit buffer writes
        self.emit_compute_output(out, "    ")?;

        let _ = writeln!(out, "}}");

        Ok(())
    }

    // ======================================================================
    // 3D Vertex Shader
    // ======================================================================

    fn emit_vertex_3d_shader(&self, out: &mut String) -> Result<(), FlowError> {
        // Vertex input struct (matches blinc_core::Vertex layout)
        let _ = writeln!(out, "struct VertexInput3D {{");
        let _ = writeln!(out, "    @location(0) position: vec3<f32>,");
        let _ = writeln!(out, "    @location(1) normal: vec3<f32>,");
        let _ = writeln!(out, "    @location(2) uv: vec2<f32>,");
        let _ = writeln!(out, "    @location(3) color: vec4<f32>,");
        let _ = writeln!(out, "    @location(4) tangent: vec4<f32>,");
        let _ = writeln!(out, "    @location(5) joints: vec4<u32>,");
        let _ = writeln!(out, "    @location(6) weights: vec4<f32>,");
        let _ = writeln!(out, "}}");
        let _ = writeln!(out);

        // Vertex output struct
        let _ = writeln!(out, "struct VertexOutput3D {{");
        let _ = writeln!(out, "    @builtin(position) clip_position: vec4<f32>,");
        let _ = writeln!(out, "    @location(0) world_normal: vec3<f32>,");
        let _ = writeln!(out, "    @location(1) world_pos: vec3<f32>,");
        let _ = writeln!(out, "    @location(2) uv: vec2<f32>,");
        let _ = writeln!(out, "    @location(3) vertex_color: vec4<f32>,");
        let _ = writeln!(out, "    @location(4) world_tangent: vec3<f32>,");
        let _ = writeln!(out, "    @location(5) tangent_handedness: f32,");
        let _ = writeln!(out, "}}");
        let _ = writeln!(out);

        // Entry point
        let _ = writeln!(out, "@vertex");
        let _ = writeln!(
            out,
            "fn vs_main(in_vert: VertexInput3D) -> VertexOutput3D {{"
        );

        // Bind builtins
        self.emit_input_bindings(out, "    ");

        // Node computations
        self.emit_node_computations(out, "    ")?;

        // Build output
        let _ = writeln!(out, "    var out: VertexOutput3D;");

        // Position output is required
        for output in &self.graph.outputs {
            let expr_str = if let Some(ref e) = output.expr {
                expr_to_wgsl(e)?
            } else {
                output.name.clone()
            };
            match &output.target {
                FlowOutputTarget::Position => {
                    let _ = writeln!(out, "    out.clip_position = {};", expr_str);
                }
                FlowOutputTarget::WorldNormalOut => {
                    let _ = writeln!(out, "    out.world_normal = {};", expr_str);
                }
                FlowOutputTarget::WorldPositionOut => {
                    let _ = writeln!(out, "    out.world_pos = {};", expr_str);
                }
                _ => {}
            }
        }

        // Defaults for non-specified outputs
        let _ = writeln!(out, "    out.uv = in_vert.uv;");
        let _ = writeln!(out, "    out.vertex_color = in_vert.color;");
        let _ = writeln!(
            out,
            "    out.world_tangent = normalize(mat3x3(u.model[0].xyz, u.model[1].xyz, u.model[2].xyz) * in_vert.tangent.xyz);"
        );
        let _ = writeln!(out, "    out.tangent_handedness = in_vert.tangent.w;");

        let _ = writeln!(out, "    return out;");
        let _ = writeln!(out, "}}");

        Ok(())
    }

    // ======================================================================
    // 3D Material Shader
    // ======================================================================

    fn emit_material_shader(&self, out: &mut String) -> Result<(), FlowError> {
        // Fragment input (matches VertexOutput3D)
        let _ = writeln!(out, "struct FragmentInput3D {{");
        let _ = writeln!(out, "    @builtin(position) frag_coord: vec4<f32>,");
        let _ = writeln!(out, "    @location(0) world_normal: vec3<f32>,");
        let _ = writeln!(out, "    @location(1) world_pos: vec3<f32>,");
        let _ = writeln!(out, "    @location(2) uv: vec2<f32>,");
        let _ = writeln!(out, "    @location(3) vertex_color: vec4<f32>,");
        let _ = writeln!(out, "    @location(4) world_tangent: vec3<f32>,");
        let _ = writeln!(out, "    @location(5) tangent_handedness: f32,");
        let _ = writeln!(out, "}}");
        let _ = writeln!(out);

        // Material output struct
        let _ = writeln!(out, "struct MaterialOutput {{");
        let _ = writeln!(out, "    albedo: vec4<f32>,");
        let _ = writeln!(out, "    metallic: f32,");
        let _ = writeln!(out, "    roughness: f32,");
        let _ = writeln!(out, "    emissive: vec3<f32>,");
        let _ = writeln!(out, "    normal: vec3<f32>,");
        let _ = writeln!(out, "}}");
        let _ = writeln!(out);

        // Entry point — outputs final color after PBR evaluation
        let _ = writeln!(out, "@fragment");
        let _ = writeln!(
            out,
            "fn fs_main(in_frag: FragmentInput3D) -> @location(0) vec4<f32> {{"
        );

        // Bind builtins
        self.emit_input_bindings(out, "    ");

        // Node computations
        self.emit_node_computations(out, "    ")?;

        // Collect material outputs
        let _ = writeln!(out, "    var mat_out: MaterialOutput;");
        let _ = writeln!(out, "    mat_out.albedo = in_frag.vertex_color;");
        let _ = writeln!(out, "    mat_out.metallic = 0.0;");
        let _ = writeln!(out, "    mat_out.roughness = 0.5;");
        let _ = writeln!(out, "    mat_out.emissive = vec3<f32>(0.0);");
        let _ = writeln!(out, "    mat_out.normal = normalize(in_frag.world_normal);");

        for output in &self.graph.outputs {
            let expr_str = if let Some(ref e) = output.expr {
                expr_to_wgsl(e)?
            } else {
                output.name.clone()
            };
            match &output.target {
                FlowOutputTarget::Albedo => {
                    let _ = writeln!(out, "    mat_out.albedo = {};", expr_str);
                }
                FlowOutputTarget::Metallic => {
                    let _ = writeln!(out, "    mat_out.metallic = {};", expr_str);
                }
                FlowOutputTarget::Roughness => {
                    let _ = writeln!(out, "    mat_out.roughness = {};", expr_str);
                }
                FlowOutputTarget::Emissive => {
                    let _ = writeln!(out, "    mat_out.emissive = {};", expr_str);
                }
                FlowOutputTarget::SurfaceNormal => {
                    let _ = writeln!(out, "    mat_out.normal = {};", expr_str);
                }
                _ => {}
            }
        }

        // Inline PBR evaluation using the material outputs
        let _ = writeln!(out, "    // PBR shading");
        let _ = writeln!(out, "    let N = mat_out.normal;");
        let _ = writeln!(out, "    let L = normalize(-u.light_dir);");
        let _ = writeln!(
            out,
            "    let V = normalize(u.camera_pos - in_frag.world_pos);"
        );
        let _ = writeln!(out, "    let H = normalize(L + V);");
        let _ = writeln!(out, "    let NdotL = max(dot(N, L), 0.0);");
        let _ = writeln!(
            out,
            "    let diffuse = mat_out.albedo.rgb * NdotL * u.light_intensity;"
        );
        let _ = writeln!(out, "    let NdotH = max(dot(N, H), 0.0);");
        let _ = writeln!(
            out,
            "    let spec_power = (1.0 - max(mat_out.roughness, 0.04)) * 128.0;"
        );
        let _ = writeln!(
            out,
            "    let specular = pow(NdotH, spec_power) * u.light_intensity * mat_out.metallic;"
        );
        let _ = writeln!(
            out,
            "    let F0 = mix(vec3<f32>(0.04), mat_out.albedo.rgb, mat_out.metallic);"
        );
        let _ = writeln!(out, "    let VdotH = max(dot(V, H), 0.0);");
        let _ = writeln!(
            out,
            "    let fresnel = F0 + (vec3<f32>(1.0) - F0) * pow(1.0 - VdotH, 5.0);"
        );
        let _ = writeln!(out, "    let ambient = mat_out.albedo.rgb * 0.15;");
        let _ = writeln!(
            out,
            "    let final_color = ambient + diffuse + fresnel * specular + mat_out.emissive;"
        );
        let _ = writeln!(out, "    return vec4(final_color, mat_out.albedo.a);");
        let _ = writeln!(out, "}}");

        Ok(())
    }

    // ======================================================================
    // Input Bindings
    // ======================================================================

    fn emit_input_bindings(&self, out: &mut String, indent: &str) {
        for input in &self.graph.inputs {
            let name = &input.name;
            let ty = input.ty.unwrap_or(FlowType::Float);
            let wgsl_ty = flow_type_to_wgsl(ty);

            match &input.source {
                FlowInputSource::Builtin(builtin) => {
                    let value = match builtin {
                        blinc_core::BuiltinVar::Uv => "in.uv".to_string(),
                        blinc_core::BuiltinVar::Time => "u.time".to_string(),
                        blinc_core::BuiltinVar::Resolution => "u.element_bounds.zw".to_string(),
                        blinc_core::BuiltinVar::Sdf => {
                            // The element's SDF at the fragment position
                            "0.0".to_string() // placeholder — bound at runtime
                        }
                        blinc_core::BuiltinVar::FrameIndex => "u.frame_index".to_string(),
                        blinc_core::BuiltinVar::Pointer => "u.pointer".to_string(),
                        blinc_core::BuiltinVar::Pressure => "u.pressure".to_string(),
                        // 3D vertex builtins
                        blinc_core::BuiltinVar::VertexPosition => "in_vert.position".to_string(),
                        blinc_core::BuiltinVar::VertexNormal => "in_vert.normal".to_string(),
                        blinc_core::BuiltinVar::VertexTangent => "in_vert.tangent".to_string(),
                        blinc_core::BuiltinVar::VertexColor => "in_vert.color".to_string(),
                        blinc_core::BuiltinVar::VertexJoints => "in_vert.joints".to_string(),
                        blinc_core::BuiltinVar::VertexWeights => "in_vert.weights".to_string(),
                        blinc_core::BuiltinVar::VertexIndex => "f32(vertex_index)".to_string(),
                        // 3D material/fragment builtins
                        blinc_core::BuiltinVar::WorldPosition => "in_frag.world_pos".to_string(),
                        blinc_core::BuiltinVar::WorldNormal => "in_frag.world_normal".to_string(),
                        blinc_core::BuiltinVar::WorldTangent => "in_frag.world_tangent".to_string(),
                        blinc_core::BuiltinVar::TangentHandedness => {
                            "in_frag.tangent_handedness".to_string()
                        }
                        blinc_core::BuiltinVar::CameraPosition => "u.camera_pos".to_string(),
                        blinc_core::BuiltinVar::LightDirection => "u.light_dir".to_string(),
                        blinc_core::BuiltinVar::LightIntensity => "u.light_intensity".to_string(),
                        // Matrices
                        blinc_core::BuiltinVar::ModelMatrix => "u.model".to_string(),
                        blinc_core::BuiltinVar::ViewProjectionMatrix => "u.view_proj".to_string(),
                    };
                    let _ = writeln!(out, "{}let {}: {} = {};", indent, name, wgsl_ty, value);
                }
                FlowInputSource::Buffer { name: buf_name, .. } => {
                    let safe_buf = sanitize_name(buf_name);
                    if self.graph.target == FlowTarget::Compute {
                        let _ = writeln!(
                            out,
                            "{}let {}: {} = buf_{}[idx];",
                            indent, name, wgsl_ty, safe_buf
                        );
                    } else {
                        let _ = writeln!(
                            out,
                            "{}let {}: {} = buf_{}[0u];",
                            indent, name, wgsl_ty, safe_buf
                        );
                    }
                }
                FlowInputSource::CssProperty(prop) => {
                    let safe = sanitize_name(prop);
                    let _ = writeln!(out, "{}let {}: {} = u.css_{};", indent, name, wgsl_ty, safe);
                }
                FlowInputSource::EnvVar(var) => {
                    let safe = sanitize_name(var);
                    let _ = writeln!(out, "{}let {}: {} = u.env_{};", indent, name, wgsl_ty, safe);
                }
                FlowInputSource::Auto => {
                    // Auto-resolved: default to 0/identity
                    let zero = match ty {
                        FlowType::Float => "0.0",
                        FlowType::Vec2 => "vec2<f32>(0.0)",
                        FlowType::Vec3 => "vec3<f32>(0.0)",
                        FlowType::Vec4 => "vec4<f32>(0.0)",
                        FlowType::Mat4 => "mat4x4<f32>(1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0)",
                    };
                    let _ = writeln!(out, "{}let {}: {} = {};", indent, name, wgsl_ty, zero);
                }
            }
        }
    }

    // ======================================================================
    // Node Computations
    // ======================================================================

    fn emit_node_computations(&self, out: &mut String, indent: &str) -> Result<(), FlowError> {
        for &idx in &self.graph.topo_order {
            let node = &self.graph.nodes[idx];
            let ty = node.inferred_type.unwrap_or(FlowType::Float);
            let wgsl_ty = flow_type_to_wgsl(ty);
            let expr_str = expr_to_wgsl(&node.expr)?;
            let _ = writeln!(
                out,
                "{}let {}: {} = {};",
                indent, node.name, wgsl_ty, expr_str
            );
        }
        Ok(())
    }

    // ======================================================================
    // Output
    // ======================================================================

    fn emit_fragment_output(&self, out: &mut String, indent: &str) -> Result<(), FlowError> {
        // Find the color output
        let color_output = self
            .graph
            .outputs
            .iter()
            .find(|o| o.target == FlowOutputTarget::Color);

        // Get the color expression string
        let color_expr = if let Some(co) = color_output {
            if let Some(expr) = &co.expr {
                expr_to_wgsl(expr)?
            } else {
                co.name.clone()
            }
        } else if let Some(first) = self.graph.outputs.first() {
            if let Some(expr) = &first.expr {
                expr_to_wgsl(expr)?
            } else {
                first.name.clone()
            }
        } else {
            "vec4<f32>(1.0, 0.0, 1.0, 1.0)".to_string()
        };

        // Apply SDF rounded-rect clipping when corner_radius > 0
        let _ = writeln!(out, "{}var flow_out = {};", indent, color_expr);
        let _ = writeln!(out, "{}if (u.corner_radius > 0.0) {{", indent);
        let _ = writeln!(out, "{}    let el_size = u.element_bounds.zw;", indent);
        let _ = writeln!(out, "{}    let half = el_size * 0.5;", indent);
        let _ = writeln!(
            out,
            "{}    let p = (in.uv - vec2<f32>(0.5, 0.5)) * el_size;",
            indent
        );
        let _ = writeln!(
            out,
            "{}    let r = min(u.corner_radius, min(half.x, half.y));",
            indent
        );
        let _ = writeln!(
            out,
            "{}    let q = abs(p) - half + vec2<f32>(r, r);",
            indent
        );
        let _ = writeln!(
            out,
            "{}    let d = length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - r;",
            indent
        );
        let _ = writeln!(out, "{}    let aa = fwidth(d) * 0.75;", indent);
        let _ = writeln!(
            out,
            "{}    flow_out.w = flow_out.w * smoothstep(aa, -aa, d);",
            indent
        );
        let _ = writeln!(out, "{}}}", indent);
        let _ = writeln!(out, "{}return flow_out;", indent);

        Ok(())
    }

    fn emit_compute_output(&self, out: &mut String, indent: &str) -> Result<(), FlowError> {
        for output in &self.graph.outputs {
            if let FlowOutputTarget::Buffer { name } = &output.target {
                let safe_buf = sanitize_name(name);
                if let Some(expr) = &output.expr {
                    let expr_str = expr_to_wgsl(expr)?;
                    let _ = writeln!(out, "{}buf_{}[idx] = {};", indent, safe_buf, expr_str);
                } else {
                    let _ = writeln!(out, "{}buf_{}[idx] = {};", indent, safe_buf, output.name);
                }
            }
        }
        Ok(())
    }
}

// ==========================================================================
// Expression → WGSL
// ==========================================================================

/// Convert a FlowExpr AST to a WGSL expression string
fn expr_to_wgsl(expr: &FlowExpr) -> Result<String, FlowError> {
    match expr {
        FlowExpr::Float(v) => {
            if v.fract() == 0.0 && !v.is_nan() && !v.is_infinite() {
                Ok(format!("{:.1}", v))
            } else {
                // Enough precision to round-trip
                Ok(format!("{}", v))
            }
        }

        FlowExpr::Vec2(a, b) => {
            let a = expr_to_wgsl(a)?;
            let b = expr_to_wgsl(b)?;
            Ok(format!("vec2<f32>({}, {})", a, b))
        }
        FlowExpr::Vec3(a, b, c) => {
            let a = expr_to_wgsl(a)?;
            let b = expr_to_wgsl(b)?;
            let c = expr_to_wgsl(c)?;
            Ok(format!("vec3<f32>({}, {}, {})", a, b, c))
        }
        FlowExpr::Vec4(a, b, c, d) => {
            let a = expr_to_wgsl(a)?;
            let b = expr_to_wgsl(b)?;
            let c = expr_to_wgsl(c)?;
            let d = expr_to_wgsl(d)?;
            Ok(format!("vec4<f32>({}, {}, {}, {})", a, b, c, d))
        }
        FlowExpr::Color(r, g, b, a) => Ok(format!(
            "vec4<f32>({}, {}, {}, {})",
            format_float(*r),
            format_float(*g),
            format_float(*b),
            format_float(*a)
        )),

        FlowExpr::Ref(name) => Ok(name.clone()),

        FlowExpr::Swizzle(inner, components) => {
            let inner_wgsl = expr_to_wgsl(inner)?;
            Ok(format!("{}.{}", inner_wgsl, components))
        }

        FlowExpr::Add(a, b) => {
            let a = expr_to_wgsl(a)?;
            let b = expr_to_wgsl(b)?;
            Ok(format!("({} + {})", a, b))
        }
        FlowExpr::Sub(a, b) => {
            let a = expr_to_wgsl(a)?;
            let b = expr_to_wgsl(b)?;
            Ok(format!("({} - {})", a, b))
        }
        FlowExpr::Mul(a, b) => {
            let a = expr_to_wgsl(a)?;
            let b = expr_to_wgsl(b)?;
            Ok(format!("({} * {})", a, b))
        }
        FlowExpr::Div(a, b) => {
            let a = expr_to_wgsl(a)?;
            let b = expr_to_wgsl(b)?;
            Ok(format!("({} / {})", a, b))
        }
        FlowExpr::Neg(a) => {
            let a = expr_to_wgsl(a)?;
            Ok(format!("(-{})", a))
        }

        FlowExpr::Call { func, args } => {
            let args_wgsl: Vec<String> = args
                .iter()
                .map(expr_to_wgsl)
                .collect::<Result<Vec<_>, _>>()?;
            func_to_wgsl(*func, &args_wgsl)
        }
    }
}

/// Map a FlowFunc call to its WGSL representation
fn func_to_wgsl(func: FlowFunc, args: &[String]) -> Result<String, FlowError> {
    Ok(match func {
        // 1:1 WGSL builtins
        FlowFunc::Sin => format!("sin({})", args[0]),
        FlowFunc::Cos => format!("cos({})", args[0]),
        FlowFunc::Tan => format!("tan({})", args[0]),
        FlowFunc::Abs => format!("abs({})", args[0]),
        FlowFunc::Floor => format!("floor({})", args[0]),
        FlowFunc::Ceil => format!("ceil({})", args[0]),
        FlowFunc::Fract => format!("fract({})", args[0]),
        FlowFunc::Sqrt => format!("sqrt({})", args[0]),
        FlowFunc::Exp => format!("exp({})", args[0]),
        FlowFunc::Log => format!("log({})", args[0]),
        FlowFunc::Sign => format!("sign({})", args[0]),
        FlowFunc::Length => format!("length({})", args[0]),
        FlowFunc::Normalize => format!("normalize({})", args[0]),

        FlowFunc::Pow => format!("pow({}, {})", args[0], args[1]),
        FlowFunc::Atan2 => format!("atan2({}, {})", args[0], args[1]),
        FlowFunc::Mod => format!("({} % {})", args[0], args[1]),
        FlowFunc::Min => format!("min({}, {})", args[0], args[1]),
        FlowFunc::Max => format!("max({}, {})", args[0], args[1]),
        FlowFunc::Step => format!("step({}, {})", args[0], args[1]),
        FlowFunc::Distance => format!("distance({}, {})", args[0], args[1]),
        FlowFunc::Dot => format!("dot({}, {})", args[0], args[1]),
        FlowFunc::Reflect => format!("reflect({}, {})", args[0], args[1]),

        FlowFunc::Clamp => format!("clamp({}, {}, {})", args[0], args[1], args[2]),
        FlowFunc::Mix => format!("mix({}, {}, {})", args[0], args[1], args[2]),
        FlowFunc::Smoothstep => {
            format!("smoothstep({}, {}, {})", args[0], args[1], args[2])
        }
        FlowFunc::Cross => format!("cross({}, {})", args[0], args[1]),

        // SDF primitives → custom helper functions
        FlowFunc::SdfBox => format!("flow_sdf_box({}, {})", args[0], args[1]),
        FlowFunc::SdfCircle => format!("flow_sdf_circle({}, {})", args[0], args[1]),
        FlowFunc::SdfEllipse => format!("flow_sdf_ellipse({}, {})", args[0], args[1]),
        FlowFunc::SdfRoundRect => {
            format!("flow_sdf_round_rect({}, {}, {})", args[0], args[1], args[2])
        }

        // SDF combinators
        FlowFunc::SdfUnion => format!("flow_sdf_union({}, {})", args[0], args[1]),
        FlowFunc::SdfIntersect => format!("flow_sdf_intersect({}, {})", args[0], args[1]),
        FlowFunc::SdfSubtract => format!("flow_sdf_subtract({}, {})", args[0], args[1]),
        FlowFunc::SdfSmoothUnion => {
            format!(
                "flow_sdf_smooth_union({}, {}, {})",
                args[0], args[1], args[2]
            )
        }
        FlowFunc::SdfSmoothIntersect => {
            format!(
                "flow_sdf_smooth_intersect({}, {}, {})",
                args[0], args[1], args[2]
            )
        }
        FlowFunc::SdfSmoothSubtract => {
            format!(
                "flow_sdf_smooth_subtract({}, {}, {})",
                args[0], args[1], args[2]
            )
        }

        // Element SDF
        FlowFunc::Sdf => format!("({})", args[0]), // pass-through; the input IS the sdf value

        // Texture / Buffer
        FlowFunc::Sobel => {
            // Sobel from a buffer read — simplified
            format!("normalize(vec3<f32>({}, {}, 1.0))", args[0], args[1])
        }
        FlowFunc::BufferRead => {
            if args.len() >= 2 {
                format!("buf_{}[u32({})]", sanitize_name(&args[0]), args[1])
            } else {
                format!("buf_{}[0u]", sanitize_name(&args[0]))
            }
        }

        // Lighting
        FlowFunc::Phong => format!("flow_phong({}, {}, {})", args[0], args[1], args[2]),
        FlowFunc::BlinnPhong => {
            format!("flow_blinn_phong({}, {}, {})", args[0], args[1], args[2])
        }

        // Noise
        FlowFunc::Perlin | FlowFunc::Simplex => {
            if args.len() >= 2 {
                format!("flow_noise2d({} * {})", args[0], args[1])
            } else {
                format!("flow_noise2d({})", args[0])
            }
        }
        FlowFunc::Worley => {
            if args.len() >= 2 {
                format!("flow_worley({} * {})", args[0], args[1])
            } else {
                format!("flow_worley({})", args[0])
            }
        }
        FlowFunc::WorleyGrad => format!("flow_worley_grad({})", args[0]),
        FlowFunc::Fbm => {
            let octaves = if args.len() >= 2 {
                args[1].clone()
            } else {
                "4".to_string()
            };
            format!("flow_fbm({}, i32({}))", args[0], wgsl_to_int(&octaves))
        }
        FlowFunc::FbmEx => {
            format!(
                "flow_fbm_ex({}, i32({}), {})",
                args[0],
                wgsl_to_int(&args[1]),
                args[2]
            )
        }
        FlowFunc::Checkerboard => {
            let scale = if args.len() >= 2 {
                args[1].clone()
            } else {
                "8.0".to_string()
            };
            format!("flow_checkerboard({}, {})", args[0], scale)
        }

        // Simulation helpers
        FlowFunc::SpringEval => {
            let stiffness = if args.len() >= 4 {
                args[3].clone()
            } else {
                "100.0".to_string()
            };
            let damping = if args.len() >= 5 {
                args[4].clone()
            } else {
                "10.0".to_string()
            };
            format!(
                "flow_spring_eval({}, {}, {}, {}, {})",
                args[0], args[1], args[2], stiffness, damping
            )
        }
        FlowFunc::WaveStep => {
            let damping = if args.len() >= 4 {
                args[3].clone()
            } else {
                "0.99".to_string()
            };
            format!(
                "flow_wave_step({}, {}, {}, {})",
                args[0], args[1], args[2], damping
            )
        }
        FlowFunc::FluidStep => {
            // Simplified fluid step
            format!(
                "flow_wave_step({}, {}, {}, 0.995)",
                args[0],
                args[1],
                args.get(2).map(|s| s.as_str()).unwrap_or("0.0")
            )
        }

        // Scene sampling — map element-local UV to framebuffer UV
        // The scene texture is a copy of the full framebuffer, so we need:
        //   framebuffer_uv = (element_bounds.xy + local_uv * element_bounds.zw) / viewport_size
        FlowFunc::SampleScene => {
            format!(
                "textureSample(scene_tex, scene_sampler, (u.element_bounds.xy + ({}) * u.element_bounds.zw) / u.viewport_size)",
                args[0]
            )
        }

        // Matrix operations
        FlowFunc::Mat4MulVec4 => format!("({} * {})", args[0], args[1]),
        FlowFunc::Mat4Mul => format!("({} * {})", args[0], args[1]),
        FlowFunc::Mat4Inverse => {
            // WGSL doesn't have built-in mat4 inverse; emit a helper
            format!("flow_mat4_inverse({})", args[0])
        }
        FlowFunc::Mat4Transpose => format!("transpose({})", args[0]),
        FlowFunc::TransformNormal => {
            format!(
                "normalize(mat3x3({}[0].xyz, {}[1].xyz, {}[2].xyz) * {})",
                args[0], args[0], args[0], args[1]
            )
        }
        FlowFunc::TranslationMatrix => {
            format!("mat4x4<f32>(1.0,0.0,0.0,0.0, 0.0,1.0,0.0,0.0, 0.0,0.0,1.0,0.0, ({}).x,({}).y,({}).z,1.0)",
                args[0], args[0], args[0])
        }
        FlowFunc::RotationMatrix => {
            // axis (vec3), angle (float) — use Rodrigues' rotation
            format!("flow_rotation_matrix({}, {})", args[0], args[1])
        }
        FlowFunc::ScaleMatrix => {
            format!("mat4x4<f32>(({}).x,0.0,0.0,0.0, 0.0,({}).y,0.0,0.0, 0.0,0.0,({}).z,0.0, 0.0,0.0,0.0,1.0)",
                args[0], args[0], args[0])
        }
        FlowFunc::PerspectiveMatrix => {
            format!(
                "flow_perspective({}, {}, {}, {})",
                args[0], args[1], args[2], args[3]
            )
        }
        FlowFunc::LookAtMatrix => {
            format!("flow_look_at({}, {}, {})", args[0], args[1], args[2])
        }
        FlowFunc::SampleTexture => {
            format!("textureSample(base_texture, base_sampler, {})", args[1])
        }
    })
}

// ==========================================================================
// Utilities
// ==========================================================================

/// Sanitize a name for use as a WGSL identifier
fn sanitize_name(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Format a float for WGSL (ensure decimal point present)
fn format_float(v: f32) -> String {
    if v.fract() == 0.0 && !v.is_nan() && !v.is_infinite() {
        format!("{:.1}", v)
    } else {
        format!("{}", v)
    }
}

/// Coerce a WGSL expression string to integer form for i32 parameters.
/// Strips trailing ".0" from float literals so `i32(6)` is emitted instead of `i32(6.0)`.
fn wgsl_to_int(s: &str) -> String {
    s.strip_suffix(".0").unwrap_or(s).to_string()
}

// ==========================================================================
// Tests
// ==========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use blinc_core::{
        BuiltinVar, FlowExpr, FlowFunc, FlowGraph, FlowInput, FlowInputSource, FlowNode,
        FlowOutput, FlowOutputTarget, FlowTarget, FlowType,
    };

    /// Create the canonical ripple-effect flow graph for testing
    fn make_ripple_flow() -> FlowGraph {
        let mut graph = FlowGraph::new("ripple");
        graph.target = FlowTarget::Fragment;

        graph.inputs.push(FlowInput {
            name: "uv".to_string(),
            source: FlowInputSource::Builtin(BuiltinVar::Uv),
            ty: Some(FlowType::Vec2),
        });
        graph.inputs.push(FlowInput {
            name: "time".to_string(),
            source: FlowInputSource::Builtin(BuiltinVar::Time),
            ty: Some(FlowType::Float),
        });

        // node dist = distance(uv, vec2(0.5, 0.5))
        graph.nodes.push(FlowNode {
            name: "dist".to_string(),
            expr: FlowExpr::Call {
                func: FlowFunc::Distance,
                args: vec![
                    FlowExpr::Ref("uv".to_string()),
                    FlowExpr::Vec2(
                        Box::new(FlowExpr::Float(0.5)),
                        Box::new(FlowExpr::Float(0.5)),
                    ),
                ],
            },
            inferred_type: None,
        });

        // node wave = sin(dist * 20.0 - time * 4.0)
        graph.nodes.push(FlowNode {
            name: "wave".to_string(),
            expr: FlowExpr::Call {
                func: FlowFunc::Sin,
                args: vec![FlowExpr::Sub(
                    Box::new(FlowExpr::Mul(
                        Box::new(FlowExpr::Ref("dist".to_string())),
                        Box::new(FlowExpr::Float(20.0)),
                    )),
                    Box::new(FlowExpr::Mul(
                        Box::new(FlowExpr::Ref("time".to_string())),
                        Box::new(FlowExpr::Float(4.0)),
                    )),
                )],
            },
            inferred_type: None,
        });

        // output color = vec4(wave, wave, wave, 1.0)
        graph.outputs.push(FlowOutput {
            name: "color".to_string(),
            target: FlowOutputTarget::Color,
            expr: Some(FlowExpr::Vec4(
                Box::new(FlowExpr::Ref("wave".to_string())),
                Box::new(FlowExpr::Ref("wave".to_string())),
                Box::new(FlowExpr::Ref("wave".to_string())),
                Box::new(FlowExpr::Float(1.0)),
            )),
        });

        graph
    }

    #[test]
    #[allow(clippy::approx_constant)]
    fn test_expr_to_wgsl_literal() {
        assert_eq!(expr_to_wgsl(&FlowExpr::Float(1.0)).unwrap(), "1.0");
        assert_eq!(expr_to_wgsl(&FlowExpr::Float(3.14_f32)).unwrap(), "3.14");
        assert_eq!(expr_to_wgsl(&FlowExpr::Float(0.0)).unwrap(), "0.0");
    }

    #[test]
    fn test_expr_to_wgsl_vec() {
        let expr = FlowExpr::Vec2(
            Box::new(FlowExpr::Float(1.0)),
            Box::new(FlowExpr::Float(2.0)),
        );
        assert_eq!(expr_to_wgsl(&expr).unwrap(), "vec2<f32>(1.0, 2.0)");
    }

    #[test]
    fn test_expr_to_wgsl_color() {
        let expr = FlowExpr::Color(1.0, 0.0, 0.5, 1.0);
        assert_eq!(
            expr_to_wgsl(&expr).unwrap(),
            "vec4<f32>(1.0, 0.0, 0.5, 1.0)"
        );
    }

    #[test]
    fn test_expr_to_wgsl_arithmetic() {
        let expr = FlowExpr::Add(
            Box::new(FlowExpr::Ref("a".to_string())),
            Box::new(FlowExpr::Mul(
                Box::new(FlowExpr::Ref("b".to_string())),
                Box::new(FlowExpr::Float(2.0)),
            )),
        );
        assert_eq!(expr_to_wgsl(&expr).unwrap(), "(a + (b * 2.0))");
    }

    #[test]
    fn test_expr_to_wgsl_neg() {
        let expr = FlowExpr::Neg(Box::new(FlowExpr::Ref("x".to_string())));
        assert_eq!(expr_to_wgsl(&expr).unwrap(), "(-x)");
    }

    #[test]
    fn test_expr_to_wgsl_func_call() {
        let expr = FlowExpr::Call {
            func: FlowFunc::Sin,
            args: vec![FlowExpr::Ref("x".to_string())],
        };
        assert_eq!(expr_to_wgsl(&expr).unwrap(), "sin(x)");
    }

    #[test]
    fn test_expr_to_wgsl_distance() {
        let expr = FlowExpr::Call {
            func: FlowFunc::Distance,
            args: vec![
                FlowExpr::Ref("uv".to_string()),
                FlowExpr::Vec2(
                    Box::new(FlowExpr::Float(0.5)),
                    Box::new(FlowExpr::Float(0.5)),
                ),
            ],
        };
        assert_eq!(
            expr_to_wgsl(&expr).unwrap(),
            "distance(uv, vec2<f32>(0.5, 0.5))"
        );
    }

    #[test]
    fn test_expr_to_wgsl_smoothstep() {
        let expr = FlowExpr::Call {
            func: FlowFunc::Smoothstep,
            args: vec![
                FlowExpr::Float(0.0),
                FlowExpr::Float(1.0),
                FlowExpr::Ref("t".to_string()),
            ],
        };
        assert_eq!(expr_to_wgsl(&expr).unwrap(), "smoothstep(0.0, 1.0, t)");
    }

    #[test]
    fn test_expr_to_wgsl_sdf_box() {
        let expr = FlowExpr::Call {
            func: FlowFunc::SdfBox,
            args: vec![
                FlowExpr::Ref("p".to_string()),
                FlowExpr::Vec2(
                    Box::new(FlowExpr::Float(0.3)),
                    Box::new(FlowExpr::Float(0.2)),
                ),
            ],
        };
        assert_eq!(
            expr_to_wgsl(&expr).unwrap(),
            "flow_sdf_box(p, vec2<f32>(0.3, 0.2))"
        );
    }

    #[test]
    fn test_ripple_flow_codegen() {
        let mut graph = make_ripple_flow();
        graph.validate(None).unwrap();

        let wgsl = flow_to_wgsl(&graph).unwrap();

        // Should contain uniforms struct
        assert!(wgsl.contains("struct FlowUniforms"));
        assert!(wgsl.contains("viewport_size: vec2<f32>"));
        assert!(wgsl.contains("time: f32"));

        // Should have binding
        assert!(wgsl.contains("@group(0) @binding(0)"));

        // Should have vertex shader
        assert!(wgsl.contains("@vertex"));
        assert!(wgsl.contains("fn vs_main"));

        // Should have fragment shader
        assert!(wgsl.contains("@fragment"));
        assert!(wgsl.contains("fn fs_main"));

        // Should contain node computations
        assert!(wgsl.contains("let dist: f32 = distance(uv, vec2<f32>(0.5, 0.5))"));
        assert!(wgsl.contains("let wave: f32 = sin(((dist * 20.0) - (time * 4.0)))"));

        // Should contain output (wrapped in SDF clipping var)
        assert!(wgsl.contains("var flow_out = vec4<f32>(wave, wave, wave, 1.0)"));
        assert!(wgsl.contains("return flow_out"));

        // Should NOT contain unused helpers
        assert!(!wgsl.contains("flow_sdf_box"));
        assert!(!wgsl.contains("flow_noise2d"));
        assert!(!wgsl.contains("flow_phong"));
    }

    #[test]
    fn test_compute_flow_codegen() {
        let mut graph = FlowGraph::new("particle_update");
        graph.target = FlowTarget::Compute;
        graph.workgroup_size = Some(256);

        graph.inputs.push(FlowInput {
            name: "pos".to_string(),
            source: FlowInputSource::Buffer {
                name: "positions".to_string(),
                ty: FlowType::Vec4,
            },
            ty: Some(FlowType::Vec4),
        });
        graph.inputs.push(FlowInput {
            name: "time".to_string(),
            source: FlowInputSource::Builtin(BuiltinVar::Time),
            ty: Some(FlowType::Float),
        });

        // node new_pos = pos + vec4(0.0, -0.01, 0.0, 0.0)
        graph.nodes.push(FlowNode {
            name: "new_pos".to_string(),
            expr: FlowExpr::Add(
                Box::new(FlowExpr::Ref("pos".to_string())),
                Box::new(FlowExpr::Vec4(
                    Box::new(FlowExpr::Float(0.0)),
                    Box::new(FlowExpr::Neg(Box::new(FlowExpr::Float(0.01)))),
                    Box::new(FlowExpr::Float(0.0)),
                    Box::new(FlowExpr::Float(0.0)),
                )),
            ),
            inferred_type: None,
        });

        graph.outputs.push(FlowOutput {
            name: "positions".to_string(),
            target: FlowOutputTarget::Buffer {
                name: "positions".to_string(),
            },
            expr: Some(FlowExpr::Ref("new_pos".to_string())),
        });

        graph.validate(None).unwrap();
        let wgsl = flow_to_wgsl(&graph).unwrap();

        // Should have compute entry
        assert!(wgsl.contains("@compute @workgroup_size(256)"));
        assert!(wgsl.contains("fn cs_main"));
        assert!(wgsl.contains("let idx = gid.x"));

        // Should have buffer binding (read_write since same buffer used for input+output)
        assert!(wgsl.contains("var<storage, read_write> buf_positions"));

        // Should read from buffer
        assert!(wgsl.contains("let pos: vec4<f32> = buf_positions[idx]"));

        // Should write to buffer
        assert!(wgsl.contains("buf_positions[idx] = new_pos"));

        // Should NOT have vertex/fragment
        assert!(!wgsl.contains("@vertex"));
        assert!(!wgsl.contains("@fragment"));
    }

    #[test]
    fn test_sdf_helpers_emitted_only_when_used() {
        let mut graph = FlowGraph::new("sdf_test");
        graph.target = FlowTarget::Fragment;

        graph.inputs.push(FlowInput {
            name: "uv".to_string(),
            source: FlowInputSource::Builtin(BuiltinVar::Uv),
            ty: Some(FlowType::Vec2),
        });

        // Use sdf_box and sdf_smooth_union
        graph.nodes.push(FlowNode {
            name: "box1".to_string(),
            expr: FlowExpr::Call {
                func: FlowFunc::SdfBox,
                args: vec![
                    FlowExpr::Ref("uv".to_string()),
                    FlowExpr::Vec2(
                        Box::new(FlowExpr::Float(0.3)),
                        Box::new(FlowExpr::Float(0.3)),
                    ),
                ],
            },
            inferred_type: None,
        });

        graph.nodes.push(FlowNode {
            name: "box2".to_string(),
            expr: FlowExpr::Call {
                func: FlowFunc::SdfBox,
                args: vec![
                    FlowExpr::Sub(
                        Box::new(FlowExpr::Ref("uv".to_string())),
                        Box::new(FlowExpr::Vec2(
                            Box::new(FlowExpr::Float(0.2)),
                            Box::new(FlowExpr::Float(0.2)),
                        )),
                    ),
                    FlowExpr::Vec2(
                        Box::new(FlowExpr::Float(0.2)),
                        Box::new(FlowExpr::Float(0.2)),
                    ),
                ],
            },
            inferred_type: None,
        });

        graph.nodes.push(FlowNode {
            name: "combined".to_string(),
            expr: FlowExpr::Call {
                func: FlowFunc::SdfSmoothUnion,
                args: vec![
                    FlowExpr::Ref("box1".to_string()),
                    FlowExpr::Ref("box2".to_string()),
                    FlowExpr::Float(0.1),
                ],
            },
            inferred_type: None,
        });

        graph.outputs.push(FlowOutput {
            name: "color".to_string(),
            target: FlowOutputTarget::Color,
            expr: Some(FlowExpr::Vec4(
                Box::new(FlowExpr::Ref("combined".to_string())),
                Box::new(FlowExpr::Ref("combined".to_string())),
                Box::new(FlowExpr::Ref("combined".to_string())),
                Box::new(FlowExpr::Float(1.0)),
            )),
        });

        graph.validate(None).unwrap();
        let wgsl = flow_to_wgsl(&graph).unwrap();

        // Should have sdf_box helper
        assert!(wgsl.contains("fn flow_sdf_box"));
        // Should have smooth_union helper
        assert!(wgsl.contains("fn flow_sdf_smooth_union"));
        // Should NOT have unused SDF helpers
        assert!(!wgsl.contains("fn flow_sdf_circle"));
        assert!(!wgsl.contains("fn flow_sdf_ellipse"));
        assert!(!wgsl.contains("fn flow_sdf_intersect"));
        // Should NOT have noise/lighting
        assert!(!wgsl.contains("flow_noise2d"));
        assert!(!wgsl.contains("flow_phong"));
    }

    #[test]
    fn test_noise_flow_codegen() {
        let mut graph = FlowGraph::new("noise_test");
        graph.target = FlowTarget::Fragment;

        graph.inputs.push(FlowInput {
            name: "uv".to_string(),
            source: FlowInputSource::Builtin(BuiltinVar::Uv),
            ty: Some(FlowType::Vec2),
        });
        graph.inputs.push(FlowInput {
            name: "time".to_string(),
            source: FlowInputSource::Builtin(BuiltinVar::Time),
            ty: Some(FlowType::Float),
        });

        graph.nodes.push(FlowNode {
            name: "n".to_string(),
            expr: FlowExpr::Call {
                func: FlowFunc::Fbm,
                args: vec![FlowExpr::Ref("uv".to_string()), FlowExpr::Float(6.0)],
            },
            inferred_type: None,
        });

        graph.outputs.push(FlowOutput {
            name: "color".to_string(),
            target: FlowOutputTarget::Color,
            expr: Some(FlowExpr::Vec4(
                Box::new(FlowExpr::Ref("n".to_string())),
                Box::new(FlowExpr::Ref("n".to_string())),
                Box::new(FlowExpr::Ref("n".to_string())),
                Box::new(FlowExpr::Float(1.0)),
            )),
        });

        graph.validate(None).unwrap();
        let wgsl = flow_to_wgsl(&graph).unwrap();

        // Should have noise helpers
        assert!(wgsl.contains("fn flow_hash21"));
        assert!(wgsl.contains("fn flow_noise2d"));
        assert!(wgsl.contains("fn flow_fbm"));

        // FBM call should use integer octaves (6.0 → 6 via wgsl_to_int)
        assert!(wgsl.contains("flow_fbm(uv, i32(6))"));
    }

    #[test]
    fn test_css_and_env_inputs() {
        let mut graph = FlowGraph::new("dynamic");
        graph.target = FlowTarget::Fragment;

        graph.inputs.push(FlowInput {
            name: "uv".to_string(),
            source: FlowInputSource::Builtin(BuiltinVar::Uv),
            ty: Some(FlowType::Vec2),
        });
        graph.inputs.push(FlowInput {
            name: "opacity_val".to_string(),
            source: FlowInputSource::CssProperty("opacity".to_string()),
            ty: Some(FlowType::Float),
        });
        graph.inputs.push(FlowInput {
            name: "ptr_x".to_string(),
            source: FlowInputSource::EnvVar("pointer-x".to_string()),
            ty: Some(FlowType::Float),
        });

        graph.nodes.push(FlowNode {
            name: "brightness".to_string(),
            expr: FlowExpr::Mul(
                Box::new(FlowExpr::Ref("opacity_val".to_string())),
                Box::new(FlowExpr::Ref("ptr_x".to_string())),
            ),
            inferred_type: None,
        });

        graph.outputs.push(FlowOutput {
            name: "color".to_string(),
            target: FlowOutputTarget::Color,
            expr: Some(FlowExpr::Vec4(
                Box::new(FlowExpr::Ref("brightness".to_string())),
                Box::new(FlowExpr::Ref("brightness".to_string())),
                Box::new(FlowExpr::Ref("brightness".to_string())),
                Box::new(FlowExpr::Float(1.0)),
            )),
        });

        graph.validate(None).unwrap();
        let wgsl = flow_to_wgsl(&graph).unwrap();

        // Should have CSS property uniform
        assert!(wgsl.contains("css_opacity: f32"));
        // Should have env var uniform
        assert!(wgsl.contains("env_pointer_x: f32"));

        // Should bind from uniforms
        assert!(wgsl.contains("let opacity_val: f32 = u.css_opacity"));
        assert!(wgsl.contains("let ptr_x: f32 = u.env_pointer_x"));
    }

    #[test]
    fn test_sanitize_name() {
        assert_eq!(sanitize_name("pointer-x"), "pointer_x");
        assert_eq!(sanitize_name("some.property"), "some_property");
        assert_eq!(sanitize_name("valid_name"), "valid_name");
        assert_eq!(sanitize_name("a b c"), "a_b_c");
    }

    #[test]
    #[allow(clippy::approx_constant)]
    fn test_format_float() {
        assert_eq!(format_float(1.0), "1.0");
        assert_eq!(format_float(0.0), "0.0");
        assert_eq!(format_float(3.14_f32), "3.14");
        assert_eq!(format_float(100.0), "100.0");
    }
}
