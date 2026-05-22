//! DAG-based flow language for declarative shader composition
//!
//! The `@flow` system provides a CSS-embedded dataflow graph language that
//! compiles to WGSL shaders. It supports both fragment (rendering) and
//! compute (simulation) targets, with compile-time cycle detection and
//! type inference.
//!
//! # Example
//!
//! ```css
//! @flow ripple-effect {
//!   target: fragment;
//!   input uv;
//!   input time;
//!   node dist = distance(uv, vec2(0.5, 0.5));
//!   node wave = sin(dist * 20.0 - time * 4.0);
//!   output color = vec4(wave, wave, wave, 1.0);
//! }
//! ```

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;

// ===========================================================================
// Flow Graph — the top-level DAG structure
// ===========================================================================

/// A parsed `@flow` block — the DAG intermediate representation
#[derive(Clone, Debug)]
pub struct FlowGraph {
    /// Name of the flow (referenced by CSS `flow: name`)
    pub name: String,
    /// Target pipeline: fragment shader or compute shader
    pub target: FlowTarget,
    /// Workgroup size for compute flows (default: 64)
    pub workgroup_size: Option<u32>,
    /// Input declarations (builtins, buffers, CSS properties)
    pub inputs: Vec<FlowInput>,
    /// Processing nodes (the DAG vertices)
    pub nodes: Vec<FlowNode>,
    /// Output declarations (what the flow produces)
    pub outputs: Vec<FlowOutput>,
    /// Cached topological order (node indices, set after validation)
    pub topo_order: Vec<usize>,
    /// Semantic step declarations (expanded to nodes during validation)
    pub steps: Vec<FlowStep>,
    /// Chain declarations (desugared to steps, then expanded to nodes)
    pub chains: Vec<FlowChain>,
    /// Composition imports (inlined during validation)
    pub uses: Vec<FlowUse>,
}

/// Target pipeline for a flow
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlowTarget {
    /// Generates fragment shader code for visual output
    Fragment,
    /// Generates compute shader code for simulation
    Compute,
    /// Vertex shader for 3D mesh rendering — transforms positions/normals.
    /// Receives per-vertex attributes (position, normal, uv) as builtins.
    Vertex,
    /// Material/surface shader for 3D mesh rendering — computes PBR outputs.
    /// Receives interpolated vertex outputs (world_pos, normal, uv) and
    /// produces albedo, metallic, roughness, emissive, normal as outputs.
    Material,
}

impl FlowGraph {
    /// Create a new empty flow graph
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            target: FlowTarget::Fragment,
            workgroup_size: None,
            inputs: Vec::new(),
            nodes: Vec::new(),
            outputs: Vec::new(),
            topo_order: Vec::new(),
            steps: Vec::new(),
            chains: Vec::new(),
            uses: Vec::new(),
        }
    }

    /// Look up a name (input or node) and return its index in the combined namespace
    fn resolve_name(&self, name: &str) -> Option<NameResolution> {
        // Check inputs first
        for (i, input) in self.inputs.iter().enumerate() {
            if input.name == name {
                return Some(NameResolution::Input(i));
            }
        }
        // Then check nodes
        for (i, node) in self.nodes.iter().enumerate() {
            if node.name == name {
                return Some(NameResolution::Node(i));
            }
        }
        None
    }
}

#[derive(Debug)]
enum NameResolution {
    Input(usize),
    Node(usize),
}

// ===========================================================================
// Inputs
// ===========================================================================

/// An input declaration in a flow graph
#[derive(Clone, Debug)]
pub struct FlowInput {
    /// Name of the input (used as reference in node expressions)
    pub name: String,
    /// Where this input comes from
    pub source: FlowInputSource,
    /// Inferred or declared type
    pub ty: Option<FlowType>,
}

/// Source of a flow input value
#[derive(Clone, Debug, PartialEq)]
pub enum FlowInputSource {
    /// Built-in variable (uv, time, resolution, etc.)
    Builtin(BuiltinVar),
    /// Read from a named GPU buffer
    Buffer { name: String, ty: FlowType },
    /// Bound from a CSS property value
    CssProperty(String),
    /// From an environment variable (e.g., pointer-x)
    EnvVar(String),
    /// Unresolved — will be determined during validation
    Auto,
}

/// Built-in variables available to flows
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuiltinVar {
    /// Element UV coordinates `[0,1]`
    Uv,
    /// Frame time in seconds
    Time,
    /// Element size in pixels
    Resolution,
    /// Current element's SDF value at the sample point
    Sdf,
    /// Frame index (integer)
    FrameIndex,
    /// Pointer position (`vec2`, from pointer query system)
    Pointer,
    /// Press intensity (0.0 released, 1.0 pressed). Renderer-driven,
    /// always populated for any `@flow` element — no `pointer-space`
    /// registration required.
    Pressure,

    // ── 3D builtins (available in Vertex and Material targets) ──
    /// Vertex position in model space (vec3) — Vertex target
    VertexPosition,
    /// Vertex normal in model space (vec3) — Vertex target
    VertexNormal,
    /// Vertex tangent (vec4: xyz = direction, w = handedness) — Vertex target
    VertexTangent,
    /// Vertex color (vec4) — Vertex target
    VertexColor,
    /// Joint indices (`vec4<u32>`) — Vertex target (skeletal)
    VertexJoints,
    /// Joint weights (vec4) — Vertex target (skeletal)
    VertexWeights,
    /// Instance/vertex index (float from u32) — Vertex/Compute target
    VertexIndex,

    /// World-space position (vec3) — Material target (interpolated from vertex)
    WorldPosition,
    /// World-space normal (vec3) — Material target (interpolated)
    WorldNormal,
    /// World-space tangent (vec3) — Material target (interpolated)
    WorldTangent,
    /// Tangent handedness (float) — Material target
    TangentHandedness,
    /// Camera position in world space (vec3) — Material target
    CameraPosition,
    /// Light direction (vec3) — Material target
    LightDirection,
    /// Light intensity (float) — Material target
    LightIntensity,

    /// Model matrix (mat4) — Vertex target
    ModelMatrix,
    /// View-projection matrix (mat4) — Vertex target
    ViewProjectionMatrix,
}

impl BuiltinVar {
    /// Parse a builtin variable name
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "uv" => Some(Self::Uv),
            "time" => Some(Self::Time),
            "resolution" => Some(Self::Resolution),
            "sdf" => Some(Self::Sdf),
            "frame-index" | "frame_index" => Some(Self::FrameIndex),
            "pointer" => Some(Self::Pointer),
            "pressure" => Some(Self::Pressure),
            // 3D vertex builtins
            "vertex_position" | "position" => Some(Self::VertexPosition),
            "vertex_normal" | "normal" => Some(Self::VertexNormal),
            "vertex_tangent" | "tangent" => Some(Self::VertexTangent),
            "vertex_color" => Some(Self::VertexColor),
            "vertex_joints" | "joints" => Some(Self::VertexJoints),
            "vertex_weights" | "weights" => Some(Self::VertexWeights),
            "vertex_index" => Some(Self::VertexIndex),
            // 3D material builtins
            "world_position" | "world_pos" => Some(Self::WorldPosition),
            "world_normal" => Some(Self::WorldNormal),
            "world_tangent" => Some(Self::WorldTangent),
            "tangent_handedness" => Some(Self::TangentHandedness),
            "camera_position" | "camera_pos" => Some(Self::CameraPosition),
            "light_direction" | "light_dir" => Some(Self::LightDirection),
            "light_intensity" => Some(Self::LightIntensity),
            // Matrices
            "model_matrix" | "model" => Some(Self::ModelMatrix),
            "view_proj" | "view_projection" => Some(Self::ViewProjectionMatrix),
            _ => None,
        }
    }

    /// Get the output type of this builtin
    pub fn output_type(&self) -> FlowType {
        match self {
            Self::Uv | Self::Resolution | Self::Pointer => FlowType::Vec2,
            Self::Time
            | Self::Sdf
            | Self::FrameIndex
            | Self::Pressure
            | Self::VertexIndex
            | Self::TangentHandedness
            | Self::LightIntensity => FlowType::Float,
            Self::VertexPosition
            | Self::VertexNormal
            | Self::WorldPosition
            | Self::WorldNormal
            | Self::WorldTangent
            | Self::CameraPosition
            | Self::LightDirection => FlowType::Vec3,
            Self::VertexTangent | Self::VertexColor | Self::VertexJoints | Self::VertexWeights => {
                FlowType::Vec4
            }
            Self::ModelMatrix | Self::ViewProjectionMatrix => FlowType::Mat4,
        }
    }
}

// ===========================================================================
// Nodes — DAG vertices
// ===========================================================================

/// A processing node in the flow graph
#[derive(Clone, Debug)]
pub struct FlowNode {
    /// Name of this node (used for references)
    pub name: String,
    /// The computation expression
    pub expr: FlowExpr,
    /// Inferred type (set during validation)
    pub inferred_type: Option<FlowType>,
}

// ===========================================================================
// Expressions — the computation graph
// ===========================================================================

/// A flow expression — the computation at each node
#[derive(Clone, Debug, PartialEq)]
pub enum FlowExpr {
    // Literals
    /// Scalar float literal
    Float(f32),
    /// 2-component vector constructor
    Vec2(Box<FlowExpr>, Box<FlowExpr>),
    /// 3-component vector constructor
    Vec3(Box<FlowExpr>, Box<FlowExpr>, Box<FlowExpr>),
    /// 4-component vector constructor
    Vec4(Box<FlowExpr>, Box<FlowExpr>, Box<FlowExpr>, Box<FlowExpr>),
    /// Color literal (#hex → RGBA)
    Color(f32, f32, f32, f32),

    // References (edges in the DAG)
    /// Reference to an input or earlier node by name
    Ref(String),

    // Arithmetic
    /// Addition: `a + b`
    Add(Box<FlowExpr>, Box<FlowExpr>),
    /// Subtraction: `a - b`
    Sub(Box<FlowExpr>, Box<FlowExpr>),
    /// Multiplication: `a * b`
    Mul(Box<FlowExpr>, Box<FlowExpr>),
    /// Division: `a / b`
    Div(Box<FlowExpr>, Box<FlowExpr>),
    /// Negation: `-a`
    Neg(Box<FlowExpr>),

    // Swizzle access
    /// Component access: `expr.xy`, `expr.rgb`, etc.
    Swizzle(Box<FlowExpr>, String),

    // Function calls (validated set — NOT arbitrary)
    /// Built-in function call
    Call { func: FlowFunc, args: Vec<FlowExpr> },
}

impl FlowExpr {
    /// Collect all `Ref` names in this expression tree
    pub fn collect_refs(&self, refs: &mut HashSet<String>) {
        match self {
            Self::Ref(name) => {
                refs.insert(name.clone());
            }
            Self::Float(_) | Self::Color(_, _, _, _) => {}
            Self::Vec2(a, b)
            | Self::Add(a, b)
            | Self::Sub(a, b)
            | Self::Mul(a, b)
            | Self::Div(a, b) => {
                a.collect_refs(refs);
                b.collect_refs(refs);
            }
            Self::Vec3(a, b, c) => {
                a.collect_refs(refs);
                b.collect_refs(refs);
                c.collect_refs(refs);
            }
            Self::Vec4(a, b, c, d) => {
                a.collect_refs(refs);
                b.collect_refs(refs);
                c.collect_refs(refs);
                d.collect_refs(refs);
            }
            Self::Neg(a) | Self::Swizzle(a, _) => a.collect_refs(refs),
            Self::Call { args, .. } => {
                for arg in args {
                    arg.collect_refs(refs);
                }
            }
        }
    }
}

// ===========================================================================
// Built-in functions
// ===========================================================================

/// Built-in function identifiers for the flow language
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlowFunc {
    // ── Math (scalar) ──
    Sin,
    Cos,
    Tan,
    Abs,
    Floor,
    Ceil,
    Fract,
    Sqrt,
    Pow,
    Atan2,
    Exp,
    Log,
    Sign,
    Mod,

    // ── Math (comparative) ──
    Min,
    Max,
    Clamp,
    Mix,
    Smoothstep,
    Step,

    // ── Vector ──
    Length,
    Distance,
    Dot,
    Cross,
    Normalize,
    Reflect,

    // ── SDF primitives ──
    SdfBox,
    SdfCircle,
    SdfEllipse,
    SdfRoundRect,

    // ── SDF combinators ──
    SdfUnion,
    SdfIntersect,
    SdfSubtract,
    SdfSmoothUnion,
    SdfSmoothIntersect,
    SdfSmoothSubtract,

    /// Current element's SDF at sample point
    Sdf,

    // ── Texture / buffer ──
    /// Sobel filter (compute normal from heightfield)
    Sobel,
    /// Read from a named buffer
    BufferRead,

    // ── Lighting ──
    Phong,
    BlinnPhong,

    // ── Noise ──
    Perlin,
    Simplex,
    Worley,
    /// Worley noise with analytic gradient — returns vec3(distance, grad_x, grad_y)
    /// in a single 3×3 grid pass. 5× faster than finite-difference gradient.
    WorleyGrad,
    /// Fractal Brownian Motion (layered noise)
    Fbm,
    /// Extended FBM with configurable persistence (roughness)
    FbmEx,
    /// Checkerboard pattern
    Checkerboard,

    // ── Simulation helpers ──
    SpringEval,
    WaveStep,
    FluidStep,

    // ── Scene sampling ──
    /// Sample the background/scene texture at given UV coordinates.
    /// Returns vec4 (RGBA). Enables refraction effects in flow shaders.
    SampleScene,

    // ── Matrix operations (for Vertex/Material targets) ──
    /// Matrix × vector multiply: mat4 * vec4 → vec4
    Mat4MulVec4,
    /// Matrix × matrix multiply: mat4 * mat4 → mat4
    Mat4Mul,
    /// Inverse of a mat4 → mat4
    Mat4Inverse,
    /// Transpose of a mat4 → mat4
    Mat4Transpose,
    /// Extract 3x3 normal matrix from model mat4, transform a vec3 normal
    TransformNormal,
    /// Construct a translation matrix from vec3
    TranslationMatrix,
    /// Construct a rotation matrix from axis (vec3) and angle (float)
    RotationMatrix,
    /// Construct a scale matrix from vec3
    ScaleMatrix,
    /// Construct a perspective projection: fov, aspect, near, far → mat4
    PerspectiveMatrix,
    /// Construct a lookAt view matrix: eye, target, up → mat4
    LookAtMatrix,

    // ── 3D texture sampling ──
    /// Sample a texture at UV coordinates: texture_id, uv → vec4
    SampleTexture,
}

impl FlowFunc {
    /// Parse a function name to a FlowFunc
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "sin" => Some(Self::Sin),
            "cos" => Some(Self::Cos),
            "tan" => Some(Self::Tan),
            "abs" => Some(Self::Abs),
            "floor" => Some(Self::Floor),
            "ceil" => Some(Self::Ceil),
            "fract" => Some(Self::Fract),
            "sqrt" => Some(Self::Sqrt),
            "pow" => Some(Self::Pow),
            "atan2" => Some(Self::Atan2),
            "exp" => Some(Self::Exp),
            "log" => Some(Self::Log),
            "sign" => Some(Self::Sign),
            "mod" => Some(Self::Mod),

            "min" => Some(Self::Min),
            "max" => Some(Self::Max),
            "clamp" => Some(Self::Clamp),
            "mix" => Some(Self::Mix),
            "smoothstep" => Some(Self::Smoothstep),
            "step" => Some(Self::Step),

            "length" => Some(Self::Length),
            "distance" => Some(Self::Distance),
            "dot" => Some(Self::Dot),
            "cross" => Some(Self::Cross),
            "normalize" => Some(Self::Normalize),
            "reflect" => Some(Self::Reflect),

            "sdf_box" | "sdf-box" => Some(Self::SdfBox),
            "sdf_circle" | "sdf-circle" => Some(Self::SdfCircle),
            "sdf_ellipse" | "sdf-ellipse" => Some(Self::SdfEllipse),
            "sdf_round_rect" | "sdf-round-rect" => Some(Self::SdfRoundRect),
            "sdf_union" | "sdf-union" => Some(Self::SdfUnion),
            "sdf_intersect" | "sdf-intersect" => Some(Self::SdfIntersect),
            "sdf_subtract" | "sdf-subtract" => Some(Self::SdfSubtract),
            "sdf_smooth_union" | "sdf-smooth-union" => Some(Self::SdfSmoothUnion),
            "sdf_smooth_intersect" | "sdf-smooth-intersect" => Some(Self::SdfSmoothIntersect),
            "sdf_smooth_subtract" | "sdf-smooth-subtract" => Some(Self::SdfSmoothSubtract),
            "sdf" => Some(Self::Sdf),

            "sobel" => Some(Self::Sobel),
            "buffer_read" | "buffer-read" => Some(Self::BufferRead),

            "phong" => Some(Self::Phong),
            "blinn_phong" | "blinn-phong" => Some(Self::BlinnPhong),

            "perlin" => Some(Self::Perlin),
            "simplex" => Some(Self::Simplex),
            "worley" => Some(Self::Worley),
            "worley_grad" | "worley-grad" => Some(Self::WorleyGrad),
            "fbm" => Some(Self::Fbm),
            "fbm_ex" | "fbm-ex" => Some(Self::FbmEx),
            "checkerboard" => Some(Self::Checkerboard),

            "spring_eval" | "spring-eval" => Some(Self::SpringEval),
            "wave_step" | "wave-step" => Some(Self::WaveStep),
            "fluid_step" | "fluid-step" => Some(Self::FluidStep),

            "sample_scene" | "sample-scene" => Some(Self::SampleScene),

            // Matrix operations
            "mat4_mul_vec4" | "mat4-mul-vec4" => Some(Self::Mat4MulVec4),
            "mat4_mul" | "mat4-mul" => Some(Self::Mat4Mul),
            "mat4_inverse" | "mat4-inverse" | "inverse" => Some(Self::Mat4Inverse),
            "mat4_transpose" | "mat4-transpose" | "transpose" => Some(Self::Mat4Transpose),
            "transform_normal" | "transform-normal" => Some(Self::TransformNormal),
            "translation_matrix" | "translation-matrix" => Some(Self::TranslationMatrix),
            "rotation_matrix" | "rotation-matrix" => Some(Self::RotationMatrix),
            "scale_matrix" | "scale-matrix" => Some(Self::ScaleMatrix),
            "perspective" | "perspective_matrix" | "perspective-matrix" => {
                Some(Self::PerspectiveMatrix)
            }
            "look_at" | "look-at" | "lookat" => Some(Self::LookAtMatrix),
            "sample_texture" | "sample-texture" => Some(Self::SampleTexture),

            _ => None,
        }
    }

    /// Get the expected argument count for this function
    pub fn arg_count(&self) -> (usize, usize) {
        // (min_args, max_args)
        match self {
            // 1-arg scalar functions
            Self::Sin
            | Self::Cos
            | Self::Tan
            | Self::Abs
            | Self::Floor
            | Self::Ceil
            | Self::Fract
            | Self::Sqrt
            | Self::Exp
            | Self::Log
            | Self::Sign
            | Self::Length
            | Self::Normalize
            | Self::Sdf => (1, 1),

            // 2-arg functions
            Self::Pow
            | Self::Atan2
            | Self::Mod
            | Self::Min
            | Self::Max
            | Self::Step
            | Self::Distance
            | Self::Dot
            | Self::Reflect
            | Self::Sobel
            | Self::SdfCircle
            | Self::SdfEllipse
            | Self::SdfUnion
            | Self::SdfIntersect
            | Self::SdfSubtract => (2, 2),

            // 3-arg functions
            Self::Clamp
            | Self::Mix
            | Self::Smoothstep
            | Self::Cross
            | Self::SdfBox
            | Self::SdfRoundRect
            | Self::SdfSmoothUnion
            | Self::SdfSmoothIntersect
            | Self::SdfSmoothSubtract
            | Self::Phong
            | Self::BlinnPhong
            | Self::Perlin
            | Self::Simplex
            | Self::Worley
            | Self::Fbm => (2, 4),

            // WorleyGrad: single arg (already-scaled position) → vec3
            Self::WorleyGrad => (1, 1),

            // FbmEx: (point, octaves, persistence) — 3 args
            Self::FbmEx => (3, 3),

            // Checkerboard: (point, scale) — 1-2 args
            Self::Checkerboard => (1, 2),

            // Scene sampling: sample_scene(uv)
            Self::SampleScene => (1, 1),

            // Variable-arg functions
            Self::BufferRead => (1, 3),
            Self::SpringEval | Self::WaveStep | Self::FluidStep => (2, 5),

            // Matrix operations
            Self::Mat4MulVec4 | Self::Mat4Mul | Self::TransformNormal => (2, 2),
            Self::Mat4Inverse | Self::Mat4Transpose => (1, 1),
            Self::TranslationMatrix | Self::ScaleMatrix => (1, 1),
            Self::RotationMatrix => (2, 2),    // axis, angle
            Self::PerspectiveMatrix => (4, 4), // fov, aspect, near, far
            Self::LookAtMatrix => (3, 3),      // eye, target, up
            Self::SampleTexture => (2, 2),     // texture_id, uv
        }
    }

    /// Get the return type given argument types
    pub fn return_type(&self, arg_types: &[FlowType]) -> Option<FlowType> {
        match self {
            // Always scalar (reduce vector to scalar, or scalar→scalar)
            Self::Length
            | Self::Distance
            | Self::Dot
            | Self::Sdf
            | Self::SdfBox
            | Self::SdfCircle
            | Self::SdfEllipse
            | Self::SdfRoundRect
            | Self::SdfUnion
            | Self::SdfIntersect
            | Self::SdfSubtract
            | Self::SdfSmoothUnion
            | Self::SdfSmoothIntersect
            | Self::SdfSmoothSubtract
            | Self::Step
            | Self::Atan2 => Some(FlowType::Float),

            // Component-wise: preserve input type (e.g. floor(vec2) → vec2)
            Self::Sin
            | Self::Cos
            | Self::Tan
            | Self::Abs
            | Self::Floor
            | Self::Ceil
            | Self::Fract
            | Self::Sqrt
            | Self::Exp
            | Self::Log
            | Self::Sign
            | Self::Pow
            | Self::Mod
            | Self::Min
            | Self::Max
            | Self::Clamp
            | Self::Mix
            | Self::Smoothstep
            | Self::Normalize
            | Self::Reflect => arg_types.first().cloned().or(Some(FlowType::Float)),

            // Vector operations
            Self::Cross => Some(FlowType::Vec3),

            // Noise → Float
            Self::Perlin
            | Self::Simplex
            | Self::Worley
            | Self::Fbm
            | Self::FbmEx
            | Self::Checkerboard => Some(FlowType::Float),

            // WorleyGrad → Vec3 (distance, grad_x, grad_y)
            Self::WorleyGrad => Some(FlowType::Vec3),

            // Sobel → Vec3 (normal)
            Self::Sobel => Some(FlowType::Vec3),

            // Lighting → Vec4 (color)
            Self::Phong | Self::BlinnPhong => Some(FlowType::Vec4),

            // Simulation → depends
            Self::SpringEval | Self::WaveStep | Self::FluidStep => {
                arg_types.first().cloned().or(Some(FlowType::Float))
            }

            Self::BufferRead | Self::SampleScene | Self::SampleTexture => Some(FlowType::Vec4),

            // Matrix operations
            Self::Mat4MulVec4 => Some(FlowType::Vec4),
            Self::Mat4Mul
            | Self::Mat4Inverse
            | Self::Mat4Transpose
            | Self::TranslationMatrix
            | Self::RotationMatrix
            | Self::ScaleMatrix
            | Self::PerspectiveMatrix
            | Self::LookAtMatrix => Some(FlowType::Mat4),
            Self::TransformNormal => Some(FlowType::Vec3),
        }
    }
}

// ===========================================================================
// Semantic Step Types — high-level, composable operations
// ===========================================================================

/// Semantic step types — named, parameterized operations that expand to
/// one or more `FlowNode` entries during validation. Names follow CSS-like
/// namespacing with human-readable parameters instead of GPU jargon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepType {
    // ── pattern-* — procedural texture generators ──
    /// Fractal/procedural noise (turbulence, smooth, cellular, crystal)
    PatternNoise,
    /// Animated concentric rings from a center point
    PatternRipple,
    /// Sinusoidal wave on a scalar input
    PatternWaves,
    /// Linear, radial, or angular gradient
    PatternGradient,
    /// Alternating grid/checkerboard
    PatternGrid,
    /// Multi-frequency sine interference
    PatternPlasma,
    /// Worley noise with built-in SDF masking and gradient computation
    PatternWorley,

    // ── transform-* — spatial distortions ──
    /// Distort UV space using a pattern or direction
    TransformWarp,
    /// Rotate pattern by angle
    TransformRotate,
    /// Scale pattern
    TransformScale,
    /// Repeat pattern N times
    TransformTile,
    /// Mirror at axis
    TransformMirror,
    /// Cartesian to polar coordinates
    TransformPolar,
    /// Animated wetness UV: aspect-corrected gravity scroll with offset
    TransformWet,

    // ── surface-* — 3D appearance from 2D patterns ──
    /// Compute normals from height + apply diffuse lighting
    SurfaceLight,
    /// Create 3D depth appearance from height field
    SurfaceDepth,
    /// Fresnel/rim glow effect
    SurfaceGlow,

    // ── effect-* — composite post-processing effects ──
    /// UV offset from Worley gradient for lens refraction
    EffectRefract,
    /// Noise-based UV jitter for frost/ice distortion
    EffectFrost,
    /// Hash-scatter specular highlights on masked areas
    EffectSpecular,
    /// Fog/haze composite with tint, density, highlights, and clear mask
    EffectFog,
    /// Directional specular highlights from surface normals (Worley gradients)
    EffectLight,

    // ── color-* — color mapping & manipulation ──
    /// Map scalar to color gradient via stops
    ColorRamp,
    /// Shift hue by amount
    ColorShift,
    /// Multiply by a color
    ColorTint,
    /// Invert value (1.0 - x)
    ColorInvert,

    // ── compose-* — combining two sources ──
    /// Blend two inputs with a blend mode (multiply, screen, overlay, add)
    ComposeBlend,
    /// Alpha mask one input by another
    ComposeMask,
    /// Stack with opacity
    ComposeLayer,

    // ── adjust-* — value curve shaping ──
    /// Distance-based fade
    AdjustFalloff,
    /// Remap value from one range to another
    AdjustRemap,
    /// Step/smooth threshold cutoff
    AdjustThreshold,
    /// Apply easing curve
    AdjustEase,
    /// Clamp to range
    AdjustClamp,
}

impl StepType {
    /// Parse a step type name (kebab-case)
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "pattern-noise" | "pattern_noise" => Some(Self::PatternNoise),
            "pattern-ripple" | "pattern_ripple" => Some(Self::PatternRipple),
            "pattern-waves" | "pattern_waves" => Some(Self::PatternWaves),
            "pattern-gradient" | "pattern_gradient" => Some(Self::PatternGradient),
            "pattern-grid" | "pattern_grid" => Some(Self::PatternGrid),
            "pattern-plasma" | "pattern_plasma" => Some(Self::PatternPlasma),
            "pattern-worley" | "pattern_worley" => Some(Self::PatternWorley),

            "transform-warp" | "transform_warp" => Some(Self::TransformWarp),
            "transform-rotate" | "transform_rotate" => Some(Self::TransformRotate),
            "transform-scale" | "transform_scale" => Some(Self::TransformScale),
            "transform-tile" | "transform_tile" => Some(Self::TransformTile),
            "transform-mirror" | "transform_mirror" => Some(Self::TransformMirror),
            "transform-polar" | "transform_polar" => Some(Self::TransformPolar),
            "transform-wet" | "transform_wet" => Some(Self::TransformWet),

            "surface-light" | "surface_light" => Some(Self::SurfaceLight),
            "surface-depth" | "surface_depth" => Some(Self::SurfaceDepth),
            "surface-glow" | "surface_glow" => Some(Self::SurfaceGlow),

            "effect-refract" | "effect_refract" => Some(Self::EffectRefract),
            "effect-frost" | "effect_frost" => Some(Self::EffectFrost),
            "effect-specular" | "effect_specular" => Some(Self::EffectSpecular),
            "effect-fog" | "effect_fog" => Some(Self::EffectFog),
            "effect-light" | "effect_light" => Some(Self::EffectLight),

            "color-ramp" | "color_ramp" => Some(Self::ColorRamp),
            "color-shift" | "color_shift" => Some(Self::ColorShift),
            "color-tint" | "color_tint" => Some(Self::ColorTint),
            "color-invert" | "color_invert" => Some(Self::ColorInvert),

            "compose-blend" | "compose_blend" => Some(Self::ComposeBlend),
            "compose-mask" | "compose_mask" => Some(Self::ComposeMask),
            "compose-layer" | "compose_layer" => Some(Self::ComposeLayer),

            "adjust-falloff" | "adjust_falloff" => Some(Self::AdjustFalloff),
            "adjust-remap" | "adjust_remap" => Some(Self::AdjustRemap),
            "adjust-threshold" | "adjust_threshold" => Some(Self::AdjustThreshold),
            "adjust-ease" | "adjust_ease" => Some(Self::AdjustEase),
            "adjust-clamp" | "adjust_clamp" => Some(Self::AdjustClamp),

            _ => None,
        }
    }

    /// Output type of this step type
    pub fn output_type(&self) -> FlowType {
        match self {
            // Pattern generators → float (scalar field)
            Self::PatternNoise
            | Self::PatternRipple
            | Self::PatternWaves
            | Self::PatternGrid
            | Self::PatternWorley => FlowType::Float,

            // Gradient/plasma → vec4 (color)
            Self::PatternGradient | Self::PatternPlasma => FlowType::Vec4,

            // Transforms → float (transformed scalar)
            Self::TransformWarp
            | Self::TransformRotate
            | Self::TransformScale
            | Self::TransformTile
            | Self::TransformMirror
            | Self::TransformPolar => FlowType::Float,
            // transform-wet → vec2 (UV coordinate)
            Self::TransformWet => FlowType::Vec2,

            // Surface steps → float (shading value) or vec4
            Self::SurfaceLight | Self::SurfaceDepth | Self::SurfaceGlow => FlowType::Float,

            // Color mapping → vec4
            Self::ColorRamp | Self::ColorShift | Self::ColorTint | Self::ColorInvert => {
                FlowType::Vec4
            }

            // Compositing → vec4
            Self::ComposeBlend | Self::ComposeMask | Self::ComposeLayer => FlowType::Vec4,

            // Effects → vec2 (UV offset) or vec4 (composited color) or float (scalar)
            Self::EffectRefract | Self::EffectFrost => FlowType::Vec2,
            Self::EffectSpecular | Self::EffectLight => FlowType::Float,
            Self::EffectFog => FlowType::Vec4,

            // Adjustments → float (value modifier)
            Self::AdjustFalloff
            | Self::AdjustRemap
            | Self::AdjustThreshold
            | Self::AdjustEase
            | Self::AdjustClamp => FlowType::Float,
        }
    }

    /// Required parameter names for this step type
    pub fn required_params(&self) -> &[&str] {
        match self {
            Self::PatternNoise => &[],
            Self::PatternRipple => &[],
            Self::PatternWaves => &["source"],
            Self::PatternGradient => &[],
            Self::PatternGrid => &[],
            Self::PatternPlasma => &[],
            Self::PatternWorley => &["scale"],

            Self::EffectRefract => &["sources"],
            Self::EffectFrost => &[],
            Self::EffectSpecular => &[],
            Self::EffectFog => &["source"],
            Self::EffectLight => &["sources"],

            Self::TransformWarp => &["source"],
            Self::TransformRotate => &["source"],
            Self::TransformScale => &["source"],
            Self::TransformTile => &["source"],
            Self::TransformMirror => &["source"],
            Self::TransformPolar => &[],
            Self::TransformWet => &[],

            Self::SurfaceLight => &["source"],
            Self::SurfaceDepth => &["source"],
            Self::SurfaceGlow => &["source"],

            Self::ColorRamp => &["source", "stops"],
            Self::ColorShift => &["source"],
            Self::ColorTint => &["source", "color"],
            Self::ColorInvert => &["source"],

            Self::ComposeBlend => &["a", "b"],
            Self::ComposeMask => &["source", "mask"],
            Self::ComposeLayer => &["a", "b"],

            Self::AdjustFalloff => &["source"],
            Self::AdjustRemap => &["source"],
            Self::AdjustThreshold => &["source"],
            Self::AdjustEase => &["source"],
            Self::AdjustClamp => &["source"],
        }
    }
}

impl fmt::Display for StepType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::PatternNoise => "pattern-noise",
            Self::PatternRipple => "pattern-ripple",
            Self::PatternWaves => "pattern-waves",
            Self::PatternGradient => "pattern-gradient",
            Self::PatternGrid => "pattern-grid",
            Self::PatternPlasma => "pattern-plasma",
            Self::PatternWorley => "pattern-worley",
            Self::EffectRefract => "effect-refract",
            Self::EffectFrost => "effect-frost",
            Self::EffectSpecular => "effect-specular",
            Self::EffectFog => "effect-fog",
            Self::EffectLight => "effect-light",
            Self::TransformWarp => "transform-warp",
            Self::TransformRotate => "transform-rotate",
            Self::TransformScale => "transform-scale",
            Self::TransformTile => "transform-tile",
            Self::TransformMirror => "transform-mirror",
            Self::TransformPolar => "transform-polar",
            Self::TransformWet => "transform-wet",
            Self::SurfaceLight => "surface-light",
            Self::SurfaceDepth => "surface-depth",
            Self::SurfaceGlow => "surface-glow",
            Self::ColorRamp => "color-ramp",
            Self::ColorShift => "color-shift",
            Self::ColorTint => "color-tint",
            Self::ColorInvert => "color-invert",
            Self::ComposeBlend => "compose-blend",
            Self::ComposeMask => "compose-mask",
            Self::ComposeLayer => "compose-layer",
            Self::AdjustFalloff => "adjust-falloff",
            Self::AdjustRemap => "adjust-remap",
            Self::AdjustThreshold => "adjust-threshold",
            Self::AdjustEase => "adjust-ease",
            Self::AdjustClamp => "adjust-clamp",
        };
        write!(f, "{}", name)
    }
}

// ===========================================================================
// Semantic Step, Chain, Use — high-level DAG constructs
// ===========================================================================

/// A parameter value within a step block
#[derive(Clone, Debug)]
pub enum StepParam {
    /// A flow expression (number, vector, reference, arithmetic)
    Expr(FlowExpr),
    /// Color stops for color-ramp: [(color_expr, position)]
    ColorStops(Vec<(FlowExpr, f32)>),
    /// A bare identifier (blend modes, curve names, style names)
    Ident(String),
    /// Integer value (e.g., detail/octaves count)
    Int(i32),
    /// Comma-separated identifiers (e.g., sources: drops1, drops2, streaks)
    IdentList(Vec<String>),
    /// Comma-separated floats (e.g., weights: 1.0, 0.5, 0.3)
    FloatList(Vec<f32>),
}

/// A semantic step declaration within a @flow block
#[derive(Clone, Debug)]
pub struct FlowStep {
    /// User-chosen name for this step
    pub name: String,
    /// The semantic operation type
    pub step_type: StepType,
    /// Named parameters
    pub params: HashMap<String, StepParam>,
}

/// A chain declaration — piped sequence of operations
#[derive(Clone, Debug)]
pub struct FlowChain {
    /// User-chosen name for the chain's final output
    pub name: String,
    /// Ordered sequence of pipe links
    pub links: Vec<ChainLink>,
}

/// A single link in a chain pipe
#[derive(Clone, Debug)]
pub struct ChainLink {
    /// The semantic operation type
    pub step_type: StepType,
    /// Named parameters (source is implicit from previous link)
    pub params: HashMap<String, StepParam>,
}

/// A `use` declaration referencing another @flow for composition
#[derive(Clone, Debug)]
pub struct FlowUse {
    /// Name of the flow being imported
    pub flow_name: String,
}

// ===========================================================================
// Types
// ===========================================================================

/// Flow data types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlowType {
    Float,
    Vec2,
    Vec3,
    Vec4,
    /// 4x4 matrix (for transforms, projections)
    Mat4,
}

impl FlowType {
    /// Number of components
    pub fn components(&self) -> usize {
        match self {
            Self::Float => 1,
            Self::Vec2 => 2,
            Self::Vec3 => 3,
            Self::Vec4 => 4,
            Self::Mat4 => 16,
        }
    }

    /// Can this type be broadcast with another?
    pub fn broadcast_with(&self, other: &Self) -> Option<Self> {
        if self == other {
            Some(*self)
        } else if *self == Self::Float {
            Some(*other)
        } else if *other == Self::Float {
            Some(*self)
        } else {
            None // incompatible types (e.g., Vec2 + Vec3)
        }
    }
}

impl fmt::Display for FlowType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Float => write!(f, "float"),
            Self::Vec2 => write!(f, "vec2"),
            Self::Vec3 => write!(f, "vec3"),
            Self::Vec4 => write!(f, "vec4"),
            Self::Mat4 => write!(f, "mat4x4<f32>"),
        }
    }
}

// ===========================================================================
// Outputs
// ===========================================================================

/// An output declaration in a flow graph
#[derive(Clone, Debug)]
pub struct FlowOutput {
    /// Name of the output
    pub name: String,
    /// Where this output goes
    pub target: FlowOutputTarget,
    /// Optional expression (if None, uses node with same name)
    pub expr: Option<FlowExpr>,
}

/// Target for a flow output
#[derive(Clone, Debug, PartialEq)]
pub enum FlowOutputTarget {
    /// Fragment shader output color
    Color,
    /// Fragment shader output alpha
    Alpha,
    /// Fragment SDF displacement
    Displacement,
    /// Write to a named compute buffer
    Buffer { name: String },
    /// Expose as a CSS variable (GPU → CPU readback)
    CssVar(String),

    // ── 3D vertex outputs ──
    /// Clip-space position (vec4) — Vertex target, required
    Position,
    /// World-space normal to pass to fragment/material (vec3) — Vertex target
    WorldNormalOut,
    /// World-space position to pass to fragment/material (vec3) — Vertex target
    WorldPositionOut,

    // ── 3D material outputs ──
    /// Base color / albedo (vec4 RGBA) — Material target
    Albedo,
    /// Metallic factor (float 0–1) — Material target
    Metallic,
    /// Roughness factor (float 0–1) — Material target
    Roughness,
    /// Emissive color (vec3 RGB) — Material target
    Emissive,
    /// Surface normal override (vec3 world-space) — Material target
    SurfaceNormal,
    /// Alpha override (float) — Material target
    AlphaOut,
}

// ===========================================================================
// Validation Errors
// ===========================================================================

/// Errors from flow graph validation
#[derive(Clone, Debug)]
pub enum FlowError {
    /// Cycle detected in the DAG
    CycleDetected {
        /// Names of nodes involved in the cycle
        nodes: Vec<String>,
    },
    /// Reference to an undefined name
    UndefinedReference {
        /// The node containing the reference
        in_node: String,
        /// The undefined name
        name: String,
    },
    /// Type mismatch in an operation
    TypeMismatch {
        /// The node where the error occurred
        in_node: String,
        /// Description of the mismatch
        message: String,
    },
    /// Wrong number of arguments to a function
    ArgCountMismatch {
        /// The node where the error occurred
        in_node: String,
        /// The function name
        func: String,
        /// Expected argument count range
        expected: (usize, usize),
        /// Actual argument count
        got: usize,
    },
    /// Missing required output for the target type
    MissingOutput {
        /// What's missing
        message: String,
    },
    /// Duplicate node name
    DuplicateName {
        /// The duplicated name
        name: String,
    },
    /// Unknown semantic step type
    UnknownStepType {
        /// The step name
        name: String,
        /// The unrecognized type string
        step_type: String,
    },
    /// Missing required parameter for a step
    MissingStepParam {
        /// The step name
        step_name: String,
        /// The missing parameter
        param: String,
    },
    /// Invalid parameter value for a step
    InvalidStepParam {
        /// The step name
        step_name: String,
        /// The parameter name
        param: String,
        /// Description of the issue
        message: String,
    },
    /// Referenced flow not found (for `use` declarations)
    FlowNotFound {
        /// The flow name
        name: String,
    },
    /// Circular flow composition (flow A uses B which uses A)
    CircularComposition {
        /// Chain of flow names forming the cycle
        chain: Vec<String>,
    },
    /// Invalid identifier for WGSL (e.g., starts with `__`, contains invalid chars)
    InvalidIdentifier { name: String, reason: String },
}

impl fmt::Display for FlowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CycleDetected { nodes } => {
                write!(f, "cycle detected in flow graph: {}", nodes.join(" → "))
            }
            Self::UndefinedReference { in_node, name } => {
                write!(f, "in node '{}': undefined reference '{}'", in_node, name)
            }
            Self::TypeMismatch { in_node, message } => {
                write!(f, "in node '{}': type mismatch: {}", in_node, message)
            }
            Self::ArgCountMismatch {
                in_node,
                func,
                expected,
                got,
            } => {
                write!(
                    f,
                    "in node '{}': function '{}' expects {}-{} args, got {}",
                    in_node, func, expected.0, expected.1, got
                )
            }
            Self::MissingOutput { message } => write!(f, "missing output: {}", message),
            Self::DuplicateName { name } => {
                write!(f, "duplicate name '{}' in flow graph", name)
            }
            Self::UnknownStepType { name, step_type } => {
                write!(f, "step '{}': unknown type '{}'", name, step_type)
            }
            Self::MissingStepParam { step_name, param } => {
                write!(
                    f,
                    "step '{}': missing required parameter '{}'",
                    step_name, param
                )
            }
            Self::InvalidStepParam {
                step_name,
                param,
                message,
            } => {
                write!(
                    f,
                    "step '{}': invalid parameter '{}': {}",
                    step_name, param, message
                )
            }
            Self::FlowNotFound { name } => {
                write!(f, "referenced flow '{}' not found", name)
            }
            Self::CircularComposition { chain } => {
                write!(f, "circular flow composition: {}", chain.join(" → "))
            }
            Self::InvalidIdentifier { name, reason } => {
                write!(f, "invalid identifier '{}': {}", name, reason)
            }
        }
    }
}

// ===========================================================================
// Validation — Cycle Detection & Type Inference
// ===========================================================================

// ===========================================================================
// Semantic Layer Expansion — steps/chains/uses → FlowNode/FlowExpr
// ===========================================================================

impl FlowGraph {
    /// Expand semantic constructs (steps, chains, uses) into raw FlowNode entries.
    /// This must run before validation (cycle detection, type inference, etc.).
    fn expand_semantic_layer(
        &mut self,
        flow_registry: Option<&HashMap<String, FlowGraph>>,
    ) -> Result<(), Vec<FlowError>> {
        let mut errors = Vec::new();

        // 1. Resolve `use` declarations (inline sub-graphs)
        self.resolve_uses(flow_registry, &mut errors);

        // 2. Desugar chains into steps
        self.desugar_chains();

        // 3. Auto-inject implicit inputs (uv, time) if steps need them
        if !self.steps.is_empty() {
            self.ensure_input("uv", BuiltinVar::Uv);
            self.ensure_input("time", BuiltinVar::Time);
        }

        // 4. Expand steps into nodes
        self.expand_steps(&mut errors);

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Ensure an input exists; if not, auto-inject it.
    fn ensure_input(&mut self, name: &str, builtin: BuiltinVar) {
        if !self.inputs.iter().any(|i| i.name == name) {
            self.inputs.push(FlowInput {
                name: name.to_string(),
                source: FlowInputSource::Builtin(builtin),
                ty: Some(builtin.output_type()),
            });
        }
    }

    /// Resolve `use` declarations by inlining referenced flow sub-graphs.
    fn resolve_uses(
        &mut self,
        flow_registry: Option<&HashMap<String, FlowGraph>>,
        errors: &mut Vec<FlowError>,
    ) {
        let uses = std::mem::take(&mut self.uses);
        let registry = match flow_registry {
            Some(r) => r,
            None => {
                for u in &uses {
                    errors.push(FlowError::FlowNotFound {
                        name: u.flow_name.clone(),
                    });
                }
                return;
            }
        };

        for flow_use in &uses {
            let referenced = match registry.get(&flow_use.flow_name) {
                Some(g) => g,
                None => {
                    errors.push(FlowError::FlowNotFound {
                        name: flow_use.flow_name.clone(),
                    });
                    continue;
                }
            };

            // Check for circular reference (the referenced flow shouldn't use us)
            if referenced.uses.iter().any(|u| u.flow_name == self.name) {
                errors.push(FlowError::CircularComposition {
                    chain: vec![self.name.clone(), flow_use.flow_name.clone()],
                });
                continue;
            }

            let prefix = format!("{}_", flow_use.flow_name.replace('-', "_"));

            // Copy inputs (skip duplicates)
            for input in &referenced.inputs {
                if !self.inputs.iter().any(|i| i.name == input.name) {
                    self.inputs.push(input.clone());
                }
            }

            // Copy nodes with prefixed names
            for node in &referenced.nodes {
                let prefixed_name = format!("{}{}", prefix, node.name);
                let prefixed_expr = prefix_refs_in_expr(&node.expr, &prefix, &referenced.inputs);
                self.nodes.push(FlowNode {
                    name: prefixed_name,
                    expr: prefixed_expr,
                    inferred_type: None,
                });
            }

            // Create synthetic nodes for referenced flow's outputs
            for output in &referenced.outputs {
                let output_name =
                    format!("{}_{}", flow_use.flow_name.replace('-', "_"), output.name);
                let expr = if let Some(ref e) = output.expr {
                    prefix_refs_in_expr(e, &prefix, &referenced.inputs)
                } else {
                    FlowExpr::Ref(format!("{}{}", prefix, output.name))
                };
                self.nodes.push(FlowNode {
                    name: output_name,
                    expr,
                    inferred_type: None,
                });
            }
        }
    }

    /// Desugar chains: each chain link becomes a FlowStep with `source` wired.
    fn desugar_chains(&mut self) {
        let chains = std::mem::take(&mut self.chains);
        for chain in chains {
            let link_count = chain.links.len();
            for (i, link) in chain.links.into_iter().enumerate() {
                let step_name = if i == link_count - 1 {
                    // Last link uses the chain's name directly
                    chain.name.clone()
                } else {
                    format!("_chain_{}_{}", chain.name, i)
                };

                let mut params = link.params;
                // Wire source from previous link (if not the first)
                if i > 0 {
                    let prev_name = if i == 1 {
                        format!("_chain_{}_{}", chain.name, 0)
                    } else {
                        format!("_chain_{}_{}", chain.name, i - 1)
                    };
                    params
                        .entry("source".to_string())
                        .or_insert(StepParam::Expr(FlowExpr::Ref(prev_name)));
                }

                self.steps.push(FlowStep {
                    name: step_name,
                    step_type: link.step_type,
                    params,
                });
            }
        }
    }

    /// Expand all steps into FlowNode entries.
    fn expand_steps(&mut self, errors: &mut Vec<FlowError>) {
        let steps = std::mem::take(&mut self.steps);
        for step in &steps {
            // Validate required params
            for &param in step.step_type.required_params() {
                if !step.params.contains_key(param) {
                    errors.push(FlowError::MissingStepParam {
                        step_name: step.name.clone(),
                        param: param.to_string(),
                    });
                }
            }

            match expand_step(step) {
                Ok(nodes) => self.nodes.extend(nodes),
                Err(e) => errors.push(e),
            }
        }
    }
}

/// Prefix all Ref names in an expression, skipping input names (they're shared).
fn prefix_refs_in_expr(expr: &FlowExpr, prefix: &str, inputs: &[FlowInput]) -> FlowExpr {
    let input_names: HashSet<&str> = inputs.iter().map(|i| i.name.as_str()).collect();
    prefix_refs_recursive(expr, prefix, &input_names)
}

fn prefix_refs_recursive(expr: &FlowExpr, prefix: &str, inputs: &HashSet<&str>) -> FlowExpr {
    match expr {
        FlowExpr::Ref(name) => {
            if inputs.contains(name.as_str()) {
                FlowExpr::Ref(name.clone())
            } else {
                FlowExpr::Ref(format!("{}{}", prefix, name))
            }
        }
        FlowExpr::Float(_) | FlowExpr::Color(_, _, _, _) => expr.clone(),
        FlowExpr::Vec2(a, b) => FlowExpr::Vec2(
            Box::new(prefix_refs_recursive(a, prefix, inputs)),
            Box::new(prefix_refs_recursive(b, prefix, inputs)),
        ),
        FlowExpr::Vec3(a, b, c) => FlowExpr::Vec3(
            Box::new(prefix_refs_recursive(a, prefix, inputs)),
            Box::new(prefix_refs_recursive(b, prefix, inputs)),
            Box::new(prefix_refs_recursive(c, prefix, inputs)),
        ),
        FlowExpr::Vec4(a, b, c, d) => FlowExpr::Vec4(
            Box::new(prefix_refs_recursive(a, prefix, inputs)),
            Box::new(prefix_refs_recursive(b, prefix, inputs)),
            Box::new(prefix_refs_recursive(c, prefix, inputs)),
            Box::new(prefix_refs_recursive(d, prefix, inputs)),
        ),
        FlowExpr::Add(a, b) => FlowExpr::Add(
            Box::new(prefix_refs_recursive(a, prefix, inputs)),
            Box::new(prefix_refs_recursive(b, prefix, inputs)),
        ),
        FlowExpr::Sub(a, b) => FlowExpr::Sub(
            Box::new(prefix_refs_recursive(a, prefix, inputs)),
            Box::new(prefix_refs_recursive(b, prefix, inputs)),
        ),
        FlowExpr::Mul(a, b) => FlowExpr::Mul(
            Box::new(prefix_refs_recursive(a, prefix, inputs)),
            Box::new(prefix_refs_recursive(b, prefix, inputs)),
        ),
        FlowExpr::Div(a, b) => FlowExpr::Div(
            Box::new(prefix_refs_recursive(a, prefix, inputs)),
            Box::new(prefix_refs_recursive(b, prefix, inputs)),
        ),
        FlowExpr::Neg(a) => FlowExpr::Neg(Box::new(prefix_refs_recursive(a, prefix, inputs))),
        FlowExpr::Swizzle(a, s) => FlowExpr::Swizzle(
            Box::new(prefix_refs_recursive(a, prefix, inputs)),
            s.clone(),
        ),
        FlowExpr::Call { func, args } => FlowExpr::Call {
            func: *func,
            args: args
                .iter()
                .map(|a| prefix_refs_recursive(a, prefix, inputs))
                .collect(),
        },
    }
}

// ===========================================================================
// Step Expansion Functions
// ===========================================================================

/// Helper: get an expression param or return a default
fn param_expr(params: &HashMap<String, StepParam>, key: &str, default: FlowExpr) -> FlowExpr {
    match params.get(key) {
        Some(StepParam::Expr(e)) => e.clone(),
        _ => default,
    }
}

/// Helper: get an ident param or return a default
fn param_ident(params: &HashMap<String, StepParam>, key: &str, default: &str) -> String {
    match params.get(key) {
        Some(StepParam::Ident(s)) => s.clone(),
        _ => default.to_string(),
    }
}

/// Helper: get an int param or return a default
fn param_int(params: &HashMap<String, StepParam>, key: &str, default: i32) -> i32 {
    match params.get(key) {
        Some(StepParam::Int(n)) => *n,
        Some(StepParam::Expr(FlowExpr::Float(f))) => *f as i32,
        _ => default,
    }
}

/// Expand a single step into FlowNode entries.
fn expand_step(step: &FlowStep) -> Result<Vec<FlowNode>, FlowError> {
    match step.step_type {
        StepType::PatternNoise => expand_pattern_noise(step),
        StepType::PatternRipple => expand_pattern_ripple(step),
        StepType::PatternWaves => expand_pattern_waves(step),
        StepType::AdjustFalloff => expand_adjust_falloff(step),
        StepType::ColorRamp => expand_color_ramp(step),
        StepType::TransformWarp => expand_transform_warp(step),
        StepType::ComposeBlend => expand_compose_blend(step),
        StepType::SurfaceLight => expand_surface_light(step),
        StepType::PatternWorley => expand_pattern_worley(step),
        StepType::EffectRefract => expand_effect_refract(step),
        StepType::EffectFrost => expand_effect_frost(step),
        StepType::EffectSpecular => expand_effect_specular(step),
        StepType::EffectFog => expand_effect_fog(step),
        StepType::EffectLight => expand_effect_light(step),
        StepType::TransformWet => expand_transform_wet(step),
        // Remaining types — return a placeholder identity node for now
        _ => Ok(vec![FlowNode {
            name: step.name.clone(),
            expr: param_expr(&step.params, "source", FlowExpr::Float(0.0)),
            inferred_type: None,
        }]),
    }
}

/// pattern-noise: FBM/perlin/worley noise
fn expand_pattern_noise(step: &FlowStep) -> Result<Vec<FlowNode>, FlowError> {
    let scale = param_expr(&step.params, "scale", FlowExpr::Float(4.0));
    let detail = param_int(&step.params, "detail", 4);
    let animation = param_expr(
        &step.params,
        "animation",
        FlowExpr::Mul(
            Box::new(FlowExpr::Ref("time".to_string())),
            Box::new(FlowExpr::Float(0.5)),
        ),
    );
    let _style = param_ident(&step.params, "style", "turbulence");

    let uv_name = format!("_s_{}_p", step.name);

    // node _s_{name}_p = uv * scale + vec2(animation, 0.0)
    let uv_node = FlowNode {
        name: uv_name.clone(),
        expr: FlowExpr::Add(
            Box::new(FlowExpr::Mul(
                Box::new(FlowExpr::Ref("uv".to_string())),
                Box::new(scale),
            )),
            Box::new(FlowExpr::Vec2(
                Box::new(animation),
                Box::new(FlowExpr::Float(0.0)),
            )),
        ),
        inferred_type: None,
    };

    // node {name} = fbm(uv_node, detail)
    let out_node = FlowNode {
        name: step.name.clone(),
        expr: FlowExpr::Call {
            func: FlowFunc::Fbm,
            args: vec![FlowExpr::Ref(uv_name), FlowExpr::Float(detail as f32)],
        },
        inferred_type: None,
    };

    Ok(vec![uv_node, out_node])
}

/// pattern-ripple: concentric rings from a center
fn expand_pattern_ripple(step: &FlowStep) -> Result<Vec<FlowNode>, FlowError> {
    let center = param_expr(
        &step.params,
        "center",
        FlowExpr::Vec2(
            Box::new(FlowExpr::Float(0.5)),
            Box::new(FlowExpr::Float(0.5)),
        ),
    );
    let density = param_expr(&step.params, "density", FlowExpr::Float(30.0));
    let speed = param_expr(&step.params, "speed", FlowExpr::Float(4.0));

    let dist_name = format!("_s_{}_d", step.name);

    // node _s_{name}_d = length(uv - center)
    let dist_node = FlowNode {
        name: dist_name.clone(),
        expr: FlowExpr::Call {
            func: FlowFunc::Length,
            args: vec![FlowExpr::Sub(
                Box::new(FlowExpr::Ref("uv".to_string())),
                Box::new(center),
            )],
        },
        inferred_type: None,
    };

    // node {name} = sin(dist * density - time * speed) * 0.5 + 0.5
    let out_node = FlowNode {
        name: step.name.clone(),
        expr: FlowExpr::Add(
            Box::new(FlowExpr::Mul(
                Box::new(FlowExpr::Call {
                    func: FlowFunc::Sin,
                    args: vec![FlowExpr::Sub(
                        Box::new(FlowExpr::Mul(
                            Box::new(FlowExpr::Ref(dist_name)),
                            Box::new(density),
                        )),
                        Box::new(FlowExpr::Mul(
                            Box::new(FlowExpr::Ref("time".to_string())),
                            Box::new(speed),
                        )),
                    )],
                }),
                Box::new(FlowExpr::Float(0.5)),
            )),
            Box::new(FlowExpr::Float(0.5)),
        ),
        inferred_type: None,
    };

    Ok(vec![dist_node, out_node])
}

/// pattern-waves: sin wave on a scalar source
fn expand_pattern_waves(step: &FlowStep) -> Result<Vec<FlowNode>, FlowError> {
    let source = param_expr(&step.params, "source", FlowExpr::Float(0.0));
    let density = param_expr(&step.params, "density", FlowExpr::Float(10.0));
    let speed = param_expr(&step.params, "speed", FlowExpr::Float(1.0));
    let offset = param_expr(&step.params, "offset", FlowExpr::Float(0.0));

    // node {name} = sin(source * density - time * speed + offset) * 0.5 + 0.5
    let out_node = FlowNode {
        name: step.name.clone(),
        expr: FlowExpr::Add(
            Box::new(FlowExpr::Mul(
                Box::new(FlowExpr::Call {
                    func: FlowFunc::Sin,
                    args: vec![FlowExpr::Add(
                        Box::new(FlowExpr::Sub(
                            Box::new(FlowExpr::Mul(Box::new(source), Box::new(density))),
                            Box::new(FlowExpr::Mul(
                                Box::new(FlowExpr::Ref("time".to_string())),
                                Box::new(speed),
                            )),
                        )),
                        Box::new(offset),
                    )],
                }),
                Box::new(FlowExpr::Float(0.5)),
            )),
            Box::new(FlowExpr::Float(0.5)),
        ),
        inferred_type: None,
    };

    Ok(vec![out_node])
}

/// adjust-falloff: distance-based fade
fn expand_adjust_falloff(step: &FlowStep) -> Result<Vec<FlowNode>, FlowError> {
    let source = param_expr(&step.params, "source", FlowExpr::Float(0.0));
    let radius = param_expr(&step.params, "radius", FlowExpr::Float(0.5));
    let curve = param_ident(&step.params, "curve", "smooth");

    let out_node = if curve == "linear" {
        // clamp(1.0 - source / radius, 0.0, 1.0)
        FlowNode {
            name: step.name.clone(),
            expr: FlowExpr::Call {
                func: FlowFunc::Clamp,
                args: vec![
                    FlowExpr::Sub(
                        Box::new(FlowExpr::Float(1.0)),
                        Box::new(FlowExpr::Div(Box::new(source), Box::new(radius))),
                    ),
                    FlowExpr::Float(0.0),
                    FlowExpr::Float(1.0),
                ],
            },
            inferred_type: None,
        }
    } else {
        // smoothstep(radius, 0.0, source)
        FlowNode {
            name: step.name.clone(),
            expr: FlowExpr::Call {
                func: FlowFunc::Smoothstep,
                args: vec![radius, FlowExpr::Float(0.0), source],
            },
            inferred_type: None,
        }
    };

    Ok(vec![out_node])
}

/// color-ramp: map scalar to color via stops
///
/// Optional `opacity` param (0.0–1.0) multiplies the final alpha channel.
fn expand_color_ramp(step: &FlowStep) -> Result<Vec<FlowNode>, FlowError> {
    let source = param_expr(&step.params, "source", FlowExpr::Float(0.0));
    let opacity = step.params.get("opacity");

    let stops = match step.params.get("stops") {
        Some(StepParam::ColorStops(stops)) if stops.len() >= 2 => stops,
        _ => {
            return Err(FlowError::InvalidStepParam {
                step_name: step.name.clone(),
                param: "stops".to_string(),
                message: "color-ramp requires at least 2 color stops".to_string(),
            });
        }
    };

    // When opacity is set, the ramp result goes to an intermediate name
    let has_opacity = opacity.is_some();

    let mut nodes = Vec::new();
    let mut prev_mix_name: Option<String> = None;

    for i in 0..stops.len() - 1 {
        let (ref color_a, pos_a) = stops[i];
        let (ref color_b, pos_b) = stops[i + 1];

        let t_name = format!("_s_{}_t{}{}", step.name, i, i + 1);
        let is_last = i == stops.len() - 2;
        let mix_name = if is_last && !has_opacity {
            step.name.clone()
        } else if is_last && has_opacity {
            format!("_s_{}_rgb", step.name)
        } else {
            format!("_s_{}_m{}{}", step.name, i, i + 1)
        };

        // node t = smoothstep(pos_a, pos_b, source)
        nodes.push(FlowNode {
            name: t_name.clone(),
            expr: FlowExpr::Call {
                func: FlowFunc::Smoothstep,
                args: vec![
                    FlowExpr::Float(pos_a),
                    FlowExpr::Float(pos_b),
                    source.clone(),
                ],
            },
            inferred_type: None,
        });

        // mix(prev_color_or_mix, color_b, t)
        let a_expr = if let Some(ref prev) = prev_mix_name {
            FlowExpr::Ref(prev.clone())
        } else {
            color_a.clone()
        };

        nodes.push(FlowNode {
            name: mix_name.clone(),
            expr: FlowExpr::Call {
                func: FlowFunc::Mix,
                args: vec![a_expr, color_b.clone(), FlowExpr::Ref(t_name)],
            },
            inferred_type: None,
        });

        prev_mix_name = Some(mix_name);
    }

    // Apply opacity: vec4(rgb.x, rgb.y, rgb.z, rgb.w * opacity)
    if let Some(opacity_param) = opacity {
        let opacity_expr = match opacity_param {
            StepParam::Expr(e) => e.clone(),
            _ => FlowExpr::Float(1.0),
        };
        let rgb_ref = prev_mix_name.unwrap_or_else(|| step.name.clone());
        nodes.push(FlowNode {
            name: step.name.clone(),
            expr: FlowExpr::Vec4(
                Box::new(FlowExpr::Swizzle(
                    Box::new(FlowExpr::Ref(rgb_ref.clone())),
                    "x".to_string(),
                )),
                Box::new(FlowExpr::Swizzle(
                    Box::new(FlowExpr::Ref(rgb_ref.clone())),
                    "y".to_string(),
                )),
                Box::new(FlowExpr::Swizzle(
                    Box::new(FlowExpr::Ref(rgb_ref.clone())),
                    "z".to_string(),
                )),
                Box::new(FlowExpr::Mul(
                    Box::new(FlowExpr::Swizzle(
                        Box::new(FlowExpr::Ref(rgb_ref)),
                        "w".to_string(),
                    )),
                    Box::new(opacity_expr),
                )),
            ),
            inferred_type: None,
        });
    }

    Ok(nodes)
}

/// transform-warp: distort UV space
fn expand_transform_warp(step: &FlowStep) -> Result<Vec<FlowNode>, FlowError> {
    let source = param_expr(&step.params, "source", FlowExpr::Float(0.0));
    let strength = param_expr(&step.params, "strength", FlowExpr::Float(0.1));
    let direction = param_expr(&step.params, "direction", FlowExpr::Float(0.0));

    let ca_name = format!("_s_{}_ca", step.name);
    let sa_name = format!("_s_{}_sa", step.name);
    let cx_name = format!("_s_{}_cx", step.name);
    let cy_name = format!("_s_{}_cy", step.name);
    let rx_name = format!("_s_{}_rx", step.name);
    let ry_name = format!("_s_{}_ry", step.name);

    let nodes = vec![
        // ca = cos(direction)
        FlowNode {
            name: ca_name.clone(),
            expr: FlowExpr::Call {
                func: FlowFunc::Cos,
                args: vec![direction.clone()],
            },
            inferred_type: None,
        },
        // sa = sin(direction)
        FlowNode {
            name: sa_name.clone(),
            expr: FlowExpr::Call {
                func: FlowFunc::Sin,
                args: vec![direction],
            },
            inferred_type: None,
        },
        // cx = uv.x - 0.5
        FlowNode {
            name: cx_name.clone(),
            expr: FlowExpr::Sub(
                Box::new(FlowExpr::Swizzle(
                    Box::new(FlowExpr::Ref("uv".to_string())),
                    "x".to_string(),
                )),
                Box::new(FlowExpr::Float(0.5)),
            ),
            inferred_type: None,
        },
        // cy = uv.y - 0.5
        FlowNode {
            name: cy_name.clone(),
            expr: FlowExpr::Sub(
                Box::new(FlowExpr::Swizzle(
                    Box::new(FlowExpr::Ref("uv".to_string())),
                    "y".to_string(),
                )),
                Box::new(FlowExpr::Float(0.5)),
            ),
            inferred_type: None,
        },
        // rx = cx * ca + cy * sa + 0.5
        FlowNode {
            name: rx_name.clone(),
            expr: FlowExpr::Add(
                Box::new(FlowExpr::Add(
                    Box::new(FlowExpr::Mul(
                        Box::new(FlowExpr::Ref(cx_name.clone())),
                        Box::new(FlowExpr::Ref(ca_name.clone())),
                    )),
                    Box::new(FlowExpr::Mul(
                        Box::new(FlowExpr::Ref(cy_name.clone())),
                        Box::new(FlowExpr::Ref(sa_name.clone())),
                    )),
                )),
                Box::new(FlowExpr::Float(0.5)),
            ),
            inferred_type: None,
        },
        // ry = cx * -sa + cy * ca + 0.5
        FlowNode {
            name: ry_name.clone(),
            expr: FlowExpr::Add(
                Box::new(FlowExpr::Add(
                    Box::new(FlowExpr::Mul(
                        Box::new(FlowExpr::Ref(cx_name)),
                        Box::new(FlowExpr::Neg(Box::new(FlowExpr::Ref(sa_name)))),
                    )),
                    Box::new(FlowExpr::Mul(
                        Box::new(FlowExpr::Ref(cy_name)),
                        Box::new(FlowExpr::Ref(ca_name)),
                    )),
                )),
                Box::new(FlowExpr::Float(0.5)),
            ),
            inferred_type: None,
        },
        // {name} = source * strength (warped scalar, referencing rotated UVs)
        FlowNode {
            name: step.name.clone(),
            expr: FlowExpr::Mul(Box::new(source), Box::new(strength)),
            inferred_type: None,
        },
    ];

    Ok(nodes)
}

/// compose-blend: blend two inputs with a mode
fn expand_compose_blend(step: &FlowStep) -> Result<Vec<FlowNode>, FlowError> {
    let a = param_expr(&step.params, "a", FlowExpr::Float(0.0));
    let b = param_expr(&step.params, "b", FlowExpr::Float(0.0));
    let mode = param_ident(&step.params, "mode", "multiply");

    let expr = match mode.as_str() {
        "screen" => {
            // 1 - (1 - a) * (1 - b) = a + b - a * b
            FlowExpr::Sub(
                Box::new(FlowExpr::Add(Box::new(a.clone()), Box::new(b.clone()))),
                Box::new(FlowExpr::Mul(Box::new(a), Box::new(b))),
            )
        }
        "add" => FlowExpr::Call {
            func: FlowFunc::Clamp,
            args: vec![
                FlowExpr::Add(Box::new(a), Box::new(b)),
                FlowExpr::Float(0.0),
                FlowExpr::Float(1.0),
            ],
        },
        _ => {
            // multiply (default)
            FlowExpr::Mul(Box::new(a), Box::new(b))
        }
    };

    Ok(vec![FlowNode {
        name: step.name.clone(),
        expr,
        inferred_type: None,
    }])
}

/// surface-light: finite-difference normals + diffuse lighting
fn expand_surface_light(step: &FlowStep) -> Result<Vec<FlowNode>, FlowError> {
    let source = param_expr(&step.params, "source", FlowExpr::Float(0.0));
    let direction = param_expr(
        &step.params,
        "direction",
        FlowExpr::Vec3(
            Box::new(FlowExpr::Float(0.3)),
            Box::new(FlowExpr::Float(-0.8)),
            Box::new(FlowExpr::Float(0.5)),
        ),
    );
    let softness = param_expr(&step.params, "softness", FlowExpr::Float(0.15));

    // For Phase 1, require source_dx and source_dy as explicit params
    let source_dx = param_expr(&step.params, "source_dx", source.clone());
    let source_dy = param_expr(&step.params, "source_dy", source.clone());
    let precision = param_expr(&step.params, "precision", FlowExpr::Float(0.005));

    let gx_name = format!("_s_{}_gx", step.name);
    let gy_name = format!("_s_{}_gy", step.name);
    let normal_name = format!("_s_{}_n", step.name);

    let nodes = vec![
        // gx = (source_dx - source) / precision
        FlowNode {
            name: gx_name.clone(),
            expr: FlowExpr::Div(
                Box::new(FlowExpr::Sub(Box::new(source_dx), Box::new(source.clone()))),
                Box::new(precision.clone()),
            ),
            inferred_type: None,
        },
        // gy = (source_dy - source) / precision
        FlowNode {
            name: gy_name.clone(),
            expr: FlowExpr::Div(
                Box::new(FlowExpr::Sub(Box::new(source_dy), Box::new(source))),
                Box::new(precision),
            ),
            inferred_type: None,
        },
        // n = normalize(vec3(-gx, -gy, 1.0))
        FlowNode {
            name: normal_name.clone(),
            expr: FlowExpr::Call {
                func: FlowFunc::Normalize,
                args: vec![FlowExpr::Vec3(
                    Box::new(FlowExpr::Neg(Box::new(FlowExpr::Ref(gx_name)))),
                    Box::new(FlowExpr::Neg(Box::new(FlowExpr::Ref(gy_name)))),
                    Box::new(FlowExpr::Float(1.0)),
                )],
            },
            inferred_type: None,
        },
        // {name} = clamp(dot(n, normalize(direction)), 0.0, 1.0) + softness
        FlowNode {
            name: step.name.clone(),
            expr: FlowExpr::Add(
                Box::new(FlowExpr::Call {
                    func: FlowFunc::Clamp,
                    args: vec![
                        FlowExpr::Call {
                            func: FlowFunc::Dot,
                            args: vec![
                                FlowExpr::Ref(normal_name),
                                FlowExpr::Call {
                                    func: FlowFunc::Normalize,
                                    args: vec![direction],
                                },
                            ],
                        },
                        FlowExpr::Float(0.0),
                        FlowExpr::Float(1.0),
                    ],
                }),
                Box::new(softness),
            ),
            inferred_type: None,
        },
    ];

    Ok(nodes)
}

/// pattern-worley: Worley noise with SDF masking and gradient computation
///
/// Emits `_s_{name}_sc` (scaled UV), `_s_{name}_eval` (raw Worley distance),
/// `_s_{name}_gx/gy` (finite-difference gradient for refraction),
/// and `{name}` (masked drop field).
fn expand_pattern_worley(step: &FlowStep) -> Result<Vec<FlowNode>, FlowError> {
    let uv_input = param_expr(&step.params, "uv", FlowExpr::Ref("uv".to_string()));
    let scale = param_expr(&step.params, "scale", FlowExpr::Float(10.0));
    let threshold = param_expr(&step.params, "threshold", FlowExpr::Float(0.05));
    let edge = param_expr(&step.params, "edge", FlowExpr::Float(0.005));
    let mask = param_expr(&step.params, "mask", FlowExpr::Float(1.0));

    let sc_name = format!("_s_{}_sc", step.name);
    let wg_name = format!("_s_{}_wg", step.name);
    let eval_name = format!("_s_{}_eval", step.name);
    let gx_name = format!("_s_{}_gx", step.name);
    let gy_name = format!("_s_{}_gy", step.name);

    let mut nodes = Vec::new();

    // _s_{name}_sc = uv_input  (the user-provided UV, already scaled if needed)
    nodes.push(FlowNode {
        name: sc_name.clone(),
        expr: uv_input,
        inferred_type: None,
    });

    // _s_{name}_wg = worley_grad(_s_{name}_sc * scale) → vec3(dist, grad_x, grad_y)
    // Single pass: replaces 5 separate worley() calls (1 eval + 4 finite-difference).
    nodes.push(FlowNode {
        name: wg_name.clone(),
        expr: FlowExpr::Call {
            func: FlowFunc::WorleyGrad,
            args: vec![FlowExpr::Mul(
                Box::new(FlowExpr::Ref(sc_name)),
                Box::new(scale),
            )],
        },
        inferred_type: None,
    });

    // _s_{name}_eval = _s_{name}_wg.x  (raw Worley distance)
    nodes.push(FlowNode {
        name: eval_name.clone(),
        expr: FlowExpr::Swizzle(Box::new(FlowExpr::Ref(wg_name.clone())), "x".to_string()),
        inferred_type: None,
    });

    // {name} = smoothstep(threshold, edge, _s_{name}_eval) * mask
    nodes.push(FlowNode {
        name: step.name.clone(),
        expr: FlowExpr::Mul(
            Box::new(FlowExpr::Call {
                func: FlowFunc::Smoothstep,
                args: vec![threshold, edge, FlowExpr::Ref(eval_name)],
            }),
            Box::new(mask),
        ),
        inferred_type: None,
    });

    // _s_{name}_gx = _s_{name}_wg.y  (analytic gradient x)
    nodes.push(FlowNode {
        name: gx_name,
        expr: FlowExpr::Swizzle(Box::new(FlowExpr::Ref(wg_name.clone())), "y".to_string()),
        inferred_type: None,
    });

    // _s_{name}_gy = _s_{name}_wg.z  (analytic gradient y)
    nodes.push(FlowNode {
        name: gy_name,
        expr: FlowExpr::Swizzle(Box::new(FlowExpr::Ref(wg_name)), "z".to_string()),
        inferred_type: None,
    });

    // Public gradient accessors: {name}_gx, {name}_gy
    // Lets user reference drop surface normals in manual nodes or other steps.
    nodes.push(FlowNode {
        name: format!("{}_gx", step.name),
        expr: FlowExpr::Ref(format!("_s_{}_gx", step.name)),
        inferred_type: None,
    });
    nodes.push(FlowNode {
        name: format!("{}_gy", step.name),
        expr: FlowExpr::Ref(format!("_s_{}_gy", step.name)),
        inferred_type: None,
    });

    Ok(nodes)
}

/// effect-refract: UV offset from Worley gradients for lens refraction
///
/// Reads `_s_{src}_gx/gy` and `_s_{src}_eval` from pattern-worley steps
/// via naming convention. Produces a vec2 UV offset.
fn expand_effect_refract(step: &FlowStep) -> Result<Vec<FlowNode>, FlowError> {
    let sources = match step.params.get("sources") {
        Some(StepParam::IdentList(list)) => list.clone(),
        // Allow a single ident too
        Some(StepParam::Ident(s)) => vec![s.clone()],
        _ => {
            return Err(FlowError::InvalidStepParam {
                step_name: step.name.clone(),
                param: "sources".to_string(),
                message: "effect-refract requires 'sources' as a comma-separated list of pattern-worley step names".to_string(),
            });
        }
    };

    let weights = match step.params.get("weights") {
        Some(StepParam::FloatList(list)) => list.clone(),
        Some(StepParam::Expr(FlowExpr::Float(f))) => vec![*f],
        _ => vec![1.0; sources.len()],
    };

    let strength = param_expr(&step.params, "strength", FlowExpr::Float(0.1));

    // Build sum: for each source, add gx * eval * strength * weight * mask
    let mut nodes = Vec::new();

    let ox_name = format!("_s_{}_ox", step.name);
    let oy_name = format!("_s_{}_oy", step.name);

    let mut ox_expr: Option<FlowExpr> = None;
    let mut oy_expr: Option<FlowExpr> = None;

    for (i, src) in sources.iter().enumerate() {
        let w = weights.get(i).copied().unwrap_or(1.0);
        let gx_ref = format!("_s_{}_gx", src);
        let gy_ref = format!("_s_{}_gy", src);

        // term_x = _s_{src}_gx * strength * weight * {src}
        // Gradients are already normalized (divided by 2*eps) in pattern-worley,
        // so strength directly controls UV offset magnitude.
        let term_x = FlowExpr::Mul(
            Box::new(FlowExpr::Mul(
                Box::new(FlowExpr::Mul(
                    Box::new(FlowExpr::Ref(gx_ref)),
                    Box::new(strength.clone()),
                )),
                Box::new(FlowExpr::Float(w)),
            )),
            Box::new(FlowExpr::Ref(src.clone())),
        );

        let term_y = FlowExpr::Mul(
            Box::new(FlowExpr::Mul(
                Box::new(FlowExpr::Mul(
                    Box::new(FlowExpr::Ref(gy_ref)),
                    Box::new(strength.clone()),
                )),
                Box::new(FlowExpr::Float(w)),
            )),
            Box::new(FlowExpr::Ref(src.clone())),
        );

        ox_expr = Some(match ox_expr {
            None => term_x,
            Some(prev) => FlowExpr::Add(Box::new(prev), Box::new(term_x)),
        });
        oy_expr = Some(match oy_expr {
            None => term_y,
            Some(prev) => FlowExpr::Add(Box::new(prev), Box::new(term_y)),
        });
    }

    let ox_final = ox_expr.unwrap_or(FlowExpr::Float(0.0));
    let oy_final = oy_expr.unwrap_or(FlowExpr::Float(0.0));

    nodes.push(FlowNode {
        name: ox_name.clone(),
        expr: ox_final,
        inferred_type: None,
    });
    nodes.push(FlowNode {
        name: oy_name.clone(),
        expr: oy_final,
        inferred_type: None,
    });

    // {name} = vec2(ox, oy)
    nodes.push(FlowNode {
        name: step.name.clone(),
        expr: FlowExpr::Vec2(
            Box::new(FlowExpr::Ref(ox_name)),
            Box::new(FlowExpr::Ref(oy_name)),
        ),
        inferred_type: None,
    });

    Ok(nodes)
}

/// effect-frost: noise-based UV jitter for frost/ice distortion
fn expand_effect_frost(step: &FlowStep) -> Result<Vec<FlowNode>, FlowError> {
    let strength = param_expr(&step.params, "strength", FlowExpr::Float(0.003));
    let mask = param_expr(&step.params, "mask", FlowExpr::Float(1.0));
    let scale = param_expr(&step.params, "scale", FlowExpr::Float(30.0));

    let nx_name = format!("_s_{}_nx", step.name);
    let ny_name = format!("_s_{}_ny", step.name);
    let nx_p = format!("_s_{}_nxp", step.name);
    let ny_p = format!("_s_{}_nyp", step.name);
    let s_name = format!("_s_{}_s", step.name);

    let nodes = vec![
        // Noise X: uv * scale
        FlowNode {
            name: nx_p.clone(),
            expr: FlowExpr::Mul(
                Box::new(FlowExpr::Ref("uv".to_string())),
                Box::new(scale.clone()),
            ),
            inferred_type: None,
        },
        FlowNode {
            name: nx_name.clone(),
            expr: FlowExpr::Call {
                func: FlowFunc::Fbm,
                args: vec![FlowExpr::Ref(nx_p), FlowExpr::Float(2.0)],
            },
            inferred_type: None,
        },
        // Noise Y: uv * scale + vec2(100, 0)
        FlowNode {
            name: ny_p.clone(),
            expr: FlowExpr::Add(
                Box::new(FlowExpr::Mul(
                    Box::new(FlowExpr::Ref("uv".to_string())),
                    Box::new(scale),
                )),
                Box::new(FlowExpr::Vec2(
                    Box::new(FlowExpr::Float(100.0)),
                    Box::new(FlowExpr::Float(0.0)),
                )),
            ),
            inferred_type: None,
        },
        FlowNode {
            name: ny_name.clone(),
            expr: FlowExpr::Call {
                func: FlowFunc::Fbm,
                args: vec![FlowExpr::Ref(ny_p), FlowExpr::Float(2.0)],
            },
            inferred_type: None,
        },
        // _s_{name}_s = strength * mask
        FlowNode {
            name: s_name.clone(),
            expr: FlowExpr::Mul(Box::new(strength), Box::new(mask)),
            inferred_type: None,
        },
        // {name} = vec2((nx - 0.5) * s, (ny - 0.5) * s)
        FlowNode {
            name: step.name.clone(),
            expr: FlowExpr::Vec2(
                Box::new(FlowExpr::Mul(
                    Box::new(FlowExpr::Sub(
                        Box::new(FlowExpr::Ref(nx_name)),
                        Box::new(FlowExpr::Float(0.5)),
                    )),
                    Box::new(FlowExpr::Ref(s_name.clone())),
                )),
                Box::new(FlowExpr::Mul(
                    Box::new(FlowExpr::Sub(
                        Box::new(FlowExpr::Ref(ny_name)),
                        Box::new(FlowExpr::Float(0.5)),
                    )),
                    Box::new(FlowExpr::Ref(s_name)),
                )),
            ),
            inferred_type: None,
        },
    ];

    Ok(nodes)
}

/// effect-specular: hash-scatter specular highlights on masked areas
#[allow(clippy::vec_init_then_push)]
fn expand_effect_specular(step: &FlowStep) -> Result<Vec<FlowNode>, FlowError> {
    let scale = param_expr(&step.params, "scale", FlowExpr::Float(20.0));
    let mask = param_expr(&step.params, "mask", FlowExpr::Float(1.0));
    let density = param_expr(&step.params, "density", FlowExpr::Float(0.5));
    let size = param_expr(&step.params, "size", FlowExpr::Float(0.025));

    let gs_name = format!("_s_{}_gs", step.name);
    let cs_name = format!("_s_{}_cs", step.name);
    let fs_name = format!("_s_{}_fs", step.name);
    let ha_name = format!("_s_{}_ha", step.name);
    let hb_name = format!("_s_{}_hb", step.name);
    let hc_name = format!("_s_{}_hc", step.name);
    let d_name = format!("_s_{}_d", step.name);

    let mut nodes = Vec::new();

    // gs = uv * scale
    nodes.push(FlowNode {
        name: gs_name.clone(),
        expr: FlowExpr::Mul(Box::new(FlowExpr::Ref("uv".to_string())), Box::new(scale)),
        inferred_type: None,
    });

    // cs = floor(gs)
    nodes.push(FlowNode {
        name: cs_name.clone(),
        expr: FlowExpr::Call {
            func: FlowFunc::Floor,
            args: vec![FlowExpr::Ref(gs_name.clone())],
        },
        inferred_type: None,
    });

    // fs = fract(gs) - vec2(0.5, 0.5)
    nodes.push(FlowNode {
        name: fs_name.clone(),
        expr: FlowExpr::Sub(
            Box::new(FlowExpr::Call {
                func: FlowFunc::Fract,
                args: vec![FlowExpr::Ref(gs_name)],
            }),
            Box::new(FlowExpr::Vec2(
                Box::new(FlowExpr::Float(0.5)),
                Box::new(FlowExpr::Float(0.5)),
            )),
        ),
        inferred_type: None,
    });

    // ha = fract(sin(dot(cs, vec2(127.1, 311.7))) * 43758.5)
    nodes.push(FlowNode {
        name: ha_name.clone(),
        expr: FlowExpr::Call {
            func: FlowFunc::Fract,
            args: vec![FlowExpr::Mul(
                Box::new(FlowExpr::Call {
                    func: FlowFunc::Sin,
                    args: vec![FlowExpr::Call {
                        func: FlowFunc::Dot,
                        args: vec![
                            FlowExpr::Ref(cs_name.clone()),
                            FlowExpr::Vec2(
                                Box::new(FlowExpr::Float(127.1)),
                                Box::new(FlowExpr::Float(311.7)),
                            ),
                        ],
                    }],
                }),
                Box::new(FlowExpr::Float(43758.5)),
            )],
        },
        inferred_type: None,
    });

    // hb = fract(sin(dot(cs, vec2(269.5, 183.3))) * 43758.5)
    nodes.push(FlowNode {
        name: hb_name.clone(),
        expr: FlowExpr::Call {
            func: FlowFunc::Fract,
            args: vec![FlowExpr::Mul(
                Box::new(FlowExpr::Call {
                    func: FlowFunc::Sin,
                    args: vec![FlowExpr::Call {
                        func: FlowFunc::Dot,
                        args: vec![
                            FlowExpr::Ref(cs_name.clone()),
                            FlowExpr::Vec2(
                                Box::new(FlowExpr::Float(269.5)),
                                Box::new(FlowExpr::Float(183.3)),
                            ),
                        ],
                    }],
                }),
                Box::new(FlowExpr::Float(43758.5)),
            )],
        },
        inferred_type: None,
    });

    // hc = fract(sin(dot(cs, vec2(97.3, 157.1))) * 43758.5)
    nodes.push(FlowNode {
        name: hc_name.clone(),
        expr: FlowExpr::Call {
            func: FlowFunc::Fract,
            args: vec![FlowExpr::Mul(
                Box::new(FlowExpr::Call {
                    func: FlowFunc::Sin,
                    args: vec![FlowExpr::Call {
                        func: FlowFunc::Dot,
                        args: vec![
                            FlowExpr::Ref(cs_name),
                            FlowExpr::Vec2(
                                Box::new(FlowExpr::Float(97.3)),
                                Box::new(FlowExpr::Float(157.1)),
                            ),
                        ],
                    }],
                }),
                Box::new(FlowExpr::Float(43758.5)),
            )],
        },
        inferred_type: None,
    });

    // d = fs - vec2(ha - 0.5, hb - 0.5) * 0.4
    nodes.push(FlowNode {
        name: d_name.clone(),
        expr: FlowExpr::Sub(
            Box::new(FlowExpr::Ref(fs_name)),
            Box::new(FlowExpr::Mul(
                Box::new(FlowExpr::Vec2(
                    Box::new(FlowExpr::Sub(
                        Box::new(FlowExpr::Ref(ha_name)),
                        Box::new(FlowExpr::Float(0.5)),
                    )),
                    Box::new(FlowExpr::Sub(
                        Box::new(FlowExpr::Ref(hb_name)),
                        Box::new(FlowExpr::Float(0.5)),
                    )),
                )),
                Box::new(FlowExpr::Float(0.4)),
            )),
        ),
        inferred_type: None,
    });

    // {name} = smoothstep(size, 0.0, length(d)) * step(density, hc) * mask
    nodes.push(FlowNode {
        name: step.name.clone(),
        expr: FlowExpr::Mul(
            Box::new(FlowExpr::Mul(
                Box::new(FlowExpr::Call {
                    func: FlowFunc::Smoothstep,
                    args: vec![
                        size,
                        FlowExpr::Float(0.0),
                        FlowExpr::Call {
                            func: FlowFunc::Length,
                            args: vec![FlowExpr::Ref(d_name)],
                        },
                    ],
                }),
                Box::new(FlowExpr::Call {
                    func: FlowFunc::Step,
                    args: vec![density, FlowExpr::Ref(hc_name)],
                }),
            )),
            Box::new(mask),
        ),
        inferred_type: None,
    });

    Ok(nodes)
}

/// effect-fog: fog/haze composite with tint, density, highlights, and clear mask
#[allow(clippy::vec_init_then_push)]
fn expand_effect_fog(step: &FlowStep) -> Result<Vec<FlowNode>, FlowError> {
    let source = param_expr(&step.params, "source", FlowExpr::Float(0.0));
    let fog_density = param_expr(&step.params, "fog_density", FlowExpr::Float(0.1));
    let tint_strength = param_expr(&step.params, "tint_strength", FlowExpr::Float(0.1));
    let clear_mask = param_expr(&step.params, "clear_mask", FlowExpr::Float(0.0));
    let highlights = param_expr(&step.params, "highlights", FlowExpr::Float(0.0));

    let tint_name = format!("_s_{}_tint", step.name);
    let r_name = format!("_s_{}_r", step.name);
    let g_name = format!("_s_{}_g", step.name);
    let b_name = format!("_s_{}_b", step.name);
    let a_name = format!("_s_{}_a", step.name);

    let mut nodes = Vec::new();

    // _s_{name}_tint = (1.0 - clear_mask) * tint_strength
    nodes.push(FlowNode {
        name: tint_name.clone(),
        expr: FlowExpr::Mul(
            Box::new(FlowExpr::Sub(
                Box::new(FlowExpr::Float(1.0)),
                Box::new(clear_mask.clone()),
            )),
            Box::new(tint_strength),
        ),
        inferred_type: None,
    });

    // r = source.x * (1.0 - tint) + tint + highlights
    nodes.push(FlowNode {
        name: r_name.clone(),
        expr: FlowExpr::Add(
            Box::new(FlowExpr::Add(
                Box::new(FlowExpr::Mul(
                    Box::new(FlowExpr::Swizzle(Box::new(source.clone()), "x".to_string())),
                    Box::new(FlowExpr::Sub(
                        Box::new(FlowExpr::Float(1.0)),
                        Box::new(FlowExpr::Ref(tint_name.clone())),
                    )),
                )),
                Box::new(FlowExpr::Ref(tint_name.clone())),
            )),
            Box::new(highlights.clone()),
        ),
        inferred_type: None,
    });

    // g = source.y * (1.0 - tint) + tint + highlights
    nodes.push(FlowNode {
        name: g_name.clone(),
        expr: FlowExpr::Add(
            Box::new(FlowExpr::Add(
                Box::new(FlowExpr::Mul(
                    Box::new(FlowExpr::Swizzle(Box::new(source.clone()), "y".to_string())),
                    Box::new(FlowExpr::Sub(
                        Box::new(FlowExpr::Float(1.0)),
                        Box::new(FlowExpr::Ref(tint_name.clone())),
                    )),
                )),
                Box::new(FlowExpr::Ref(tint_name.clone())),
            )),
            Box::new(highlights.clone()),
        ),
        inferred_type: None,
    });

    // b = source.z * (1.0 - tint) + tint + highlights
    nodes.push(FlowNode {
        name: b_name.clone(),
        expr: FlowExpr::Add(
            Box::new(FlowExpr::Add(
                Box::new(FlowExpr::Mul(
                    Box::new(FlowExpr::Swizzle(Box::new(source), "z".to_string())),
                    Box::new(FlowExpr::Sub(
                        Box::new(FlowExpr::Float(1.0)),
                        Box::new(FlowExpr::Ref(tint_name.clone())),
                    )),
                )),
                Box::new(FlowExpr::Ref(tint_name)),
            )),
            Box::new(highlights),
        ),
        inferred_type: None,
    });

    // a = mix(fog_density, 1.0, clear_mask)
    nodes.push(FlowNode {
        name: a_name.clone(),
        expr: FlowExpr::Call {
            func: FlowFunc::Mix,
            args: vec![fog_density, FlowExpr::Float(1.0), clear_mask],
        },
        inferred_type: None,
    });

    // {name} = vec4(r, g, b, a)
    nodes.push(FlowNode {
        name: step.name.clone(),
        expr: FlowExpr::Vec4(
            Box::new(FlowExpr::Ref(r_name)),
            Box::new(FlowExpr::Ref(g_name)),
            Box::new(FlowExpr::Ref(b_name)),
            Box::new(FlowExpr::Ref(a_name)),
        ),
        inferred_type: None,
    });

    Ok(nodes)
}

/// transform-wet: animated wetness UV with aspect correction, gravity scroll, and offset
///
/// Generates: `vec2(uv.x * aspect * x_scale, uv.y * y_scale - time * speed) + offset`
/// where `aspect = resolution.x / max(resolution.y, 1.0)`.
///
/// Params:
///   - speed: gravity scroll speed (default 0.02)
///   - offset: vec2 spatial offset (default vec2(0,0))
///   - x_scale: horizontal scale multiplier (default 1.0)
///   - y_scale: vertical scale multiplier (default 1.0)
///
/// Requires inputs: uv, time, resolution
#[allow(clippy::vec_init_then_push)]
fn expand_transform_wet(step: &FlowStep) -> Result<Vec<FlowNode>, FlowError> {
    let speed = param_expr(&step.params, "speed", FlowExpr::Float(0.02));
    let offset = param_expr(
        &step.params,
        "offset",
        FlowExpr::Vec2(
            Box::new(FlowExpr::Float(0.0)),
            Box::new(FlowExpr::Float(0.0)),
        ),
    );
    let x_scale = param_expr(&step.params, "x_scale", FlowExpr::Float(1.0));
    let y_scale = param_expr(&step.params, "y_scale", FlowExpr::Float(1.0));

    let aspect_name = format!("_s_{}_aspect", step.name);
    let scroll_name = format!("_s_{}_scroll", step.name);

    let mut nodes = Vec::new();

    // _s_{name}_aspect = resolution.x / max(resolution.y, 1.0)
    nodes.push(FlowNode {
        name: aspect_name.clone(),
        expr: FlowExpr::Div(
            Box::new(FlowExpr::Swizzle(
                Box::new(FlowExpr::Ref("resolution".to_string())),
                "x".to_string(),
            )),
            Box::new(FlowExpr::Call {
                func: FlowFunc::Max,
                args: vec![
                    FlowExpr::Swizzle(
                        Box::new(FlowExpr::Ref("resolution".to_string())),
                        "y".to_string(),
                    ),
                    FlowExpr::Float(1.0),
                ],
            }),
        ),
        inferred_type: None,
    });

    // _s_{name}_scroll = time * speed
    nodes.push(FlowNode {
        name: scroll_name.clone(),
        expr: FlowExpr::Mul(Box::new(FlowExpr::Ref("time".to_string())), Box::new(speed)),
        inferred_type: None,
    });

    // {name} = vec2(uv.x * aspect * x_scale, uv.y * y_scale - scroll) + offset
    nodes.push(FlowNode {
        name: step.name.clone(),
        expr: FlowExpr::Add(
            Box::new(FlowExpr::Vec2(
                Box::new(FlowExpr::Mul(
                    Box::new(FlowExpr::Mul(
                        Box::new(FlowExpr::Swizzle(
                            Box::new(FlowExpr::Ref("uv".to_string())),
                            "x".to_string(),
                        )),
                        Box::new(FlowExpr::Ref(aspect_name)),
                    )),
                    Box::new(x_scale),
                )),
                Box::new(FlowExpr::Sub(
                    Box::new(FlowExpr::Mul(
                        Box::new(FlowExpr::Swizzle(
                            Box::new(FlowExpr::Ref("uv".to_string())),
                            "y".to_string(),
                        )),
                        Box::new(y_scale),
                    )),
                    Box::new(FlowExpr::Ref(scroll_name)),
                )),
            )),
            Box::new(offset),
        ),
        inferred_type: None,
    });

    Ok(nodes)
}

/// effect-light: directional specular highlights from Worley surface normals
///
/// Reads `_s_{src}_gx/gy` from pattern-worley steps (same convention as effect-refract).
/// Computes `pow(max(dot(normal, light_dir), 0), shininess) * intensity * mask`.
///
/// Params:
///   - sources: ident-list of pattern-worley step names (required)
///   - weights: float-list of per-source weights (default all 1.0)
///   - angle: light direction angle in degrees, 0=right, 90=down (default 225 = upper-left)
///   - shininess: specular exponent (default 32.0)
///   - intensity: brightness multiplier (default 0.6)
///   - mask: mask expression (default 1.0)
fn expand_effect_light(step: &FlowStep) -> Result<Vec<FlowNode>, FlowError> {
    let sources = match step.params.get("sources") {
        Some(StepParam::IdentList(list)) => list.clone(),
        _ => {
            return Err(FlowError::MissingStepParam {
                step_name: step.name.clone(),
                param: "sources".to_string(),
            });
        }
    };
    let weights: Vec<f32> = match step.params.get("weights") {
        Some(StepParam::FloatList(list)) => list.clone(),
        _ => vec![1.0; sources.len()],
    };
    let angle_deg = match step.params.get("angle") {
        Some(StepParam::Expr(FlowExpr::Float(f))) => *f,
        _ => 225.0,
    };
    let shininess = param_expr(&step.params, "shininess", FlowExpr::Float(32.0));
    let intensity = param_expr(&step.params, "intensity", FlowExpr::Float(0.6));
    let mask = param_expr(&step.params, "mask", FlowExpr::Float(1.0));

    let angle_rad = angle_deg * std::f32::consts::PI / 180.0;
    let lx = angle_rad.cos();
    let ly = angle_rad.sin();

    let gx_sum_name = format!("_s_{}_gx_sum", step.name);
    let gy_sum_name = format!("_s_{}_gy_sum", step.name);
    let dot_name = format!("_s_{}_dot", step.name);
    let spec_name = format!("_s_{}_spec", step.name);

    let mut nodes = Vec::new();

    // Sum weighted gradients from all sources: gx_sum = sum(w_i * _s_{src}_gx)
    let mut gx_expr: Option<FlowExpr> = None;
    let mut gy_expr: Option<FlowExpr> = None;
    for (i, src) in sources.iter().enumerate() {
        let w = if i < weights.len() { weights[i] } else { 1.0 };
        let gx_ref = FlowExpr::Ref(format!("_s_{}_gx", src));
        let gy_ref = FlowExpr::Ref(format!("_s_{}_gy", src));
        let weighted_gx = FlowExpr::Mul(Box::new(gx_ref), Box::new(FlowExpr::Float(w)));
        let weighted_gy = FlowExpr::Mul(Box::new(gy_ref), Box::new(FlowExpr::Float(w)));
        gx_expr = Some(match gx_expr {
            None => weighted_gx,
            Some(prev) => FlowExpr::Add(Box::new(prev), Box::new(weighted_gx)),
        });
        gy_expr = Some(match gy_expr {
            None => weighted_gy,
            Some(prev) => FlowExpr::Add(Box::new(prev), Box::new(weighted_gy)),
        });
    }

    nodes.push(FlowNode {
        name: gx_sum_name.clone(),
        expr: gx_expr.unwrap_or(FlowExpr::Float(0.0)),
        inferred_type: None,
    });
    nodes.push(FlowNode {
        name: gy_sum_name.clone(),
        expr: gy_expr.unwrap_or(FlowExpr::Float(0.0)),
        inferred_type: None,
    });

    // dot = gx_sum * lx + gy_sum * ly
    nodes.push(FlowNode {
        name: dot_name.clone(),
        expr: FlowExpr::Add(
            Box::new(FlowExpr::Mul(
                Box::new(FlowExpr::Ref(gx_sum_name)),
                Box::new(FlowExpr::Float(lx)),
            )),
            Box::new(FlowExpr::Mul(
                Box::new(FlowExpr::Ref(gy_sum_name)),
                Box::new(FlowExpr::Float(ly)),
            )),
        ),
        inferred_type: None,
    });

    // spec = pow(max(dot, 0.0), shininess)
    nodes.push(FlowNode {
        name: spec_name.clone(),
        expr: FlowExpr::Call {
            func: FlowFunc::Pow,
            args: vec![
                FlowExpr::Call {
                    func: FlowFunc::Max,
                    args: vec![FlowExpr::Ref(dot_name), FlowExpr::Float(0.0)],
                },
                shininess,
            ],
        },
        inferred_type: None,
    });

    // {name} = spec * intensity * mask
    nodes.push(FlowNode {
        name: step.name.clone(),
        expr: FlowExpr::Mul(
            Box::new(FlowExpr::Mul(
                Box::new(FlowExpr::Ref(spec_name)),
                Box::new(intensity),
            )),
            Box::new(mask),
        ),
        inferred_type: None,
    });

    Ok(nodes)
}

// ===========================================================================
// Validation — Cycle Detection & Type Inference
// ===========================================================================

impl FlowGraph {
    /// Validate the flow graph: detect cycles, check types, resolve references.
    ///
    /// On success, populates `topo_order` with node indices in dependency order.
    /// On failure, returns all errors found.
    ///
    /// Pass a flow registry for `use` resolution. Pass `None` for standalone validation.
    pub fn validate(
        &mut self,
        flow_registry: Option<&HashMap<String, FlowGraph>>,
    ) -> Result<(), Vec<FlowError>> {
        // 0. Expand semantic layer (steps, chains, uses → nodes)
        if !self.steps.is_empty() || !self.chains.is_empty() || !self.uses.is_empty() {
            self.expand_semantic_layer(flow_registry)?;
        }

        let mut errors = Vec::new();

        // 1. Validate all identifiers are valid WGSL names
        self.check_wgsl_identifiers(&mut errors);

        // 2. Check for duplicate names
        self.check_duplicates(&mut errors);

        // 3. Check all references resolve
        self.check_references(&mut errors);

        // 4. Topological sort (cycle detection via Kahn's algorithm)
        match self.topological_sort() {
            Ok(order) => self.topo_order = order,
            Err(cycle_nodes) => {
                errors.push(FlowError::CycleDetected { nodes: cycle_nodes });
            }
        }

        // 5. Type inference (only if no cycles)
        if errors
            .iter()
            .all(|e| !matches!(e, FlowError::CycleDetected { .. }))
        {
            self.infer_types(&mut errors);
        }

        // 6. Validate function argument counts
        self.check_function_args(&mut errors);

        // 7. Validate outputs match target
        self.check_outputs(&mut errors);

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Convenience: validate without a flow registry (no `use` support).
    pub fn validate_standalone(&mut self) -> Result<(), Vec<FlowError>> {
        self.validate(None)
    }

    /// WGSL reserved keywords that cannot be used as identifiers.
    const WGSL_KEYWORDS: &'static [&'static str] = &[
        "alias",
        "break",
        "case",
        "const",
        "const_assert",
        "continue",
        "continuing",
        "default",
        "diagnostic",
        "discard",
        "else",
        "enable",
        "false",
        "fn",
        "for",
        "if",
        "let",
        "loop",
        "override",
        "return",
        "struct",
        "switch",
        "true",
        "var",
        "while",
        // Reserved words
        "NULL",
        "Self",
        "abstract",
        "active",
        "alignas",
        "alignof",
        "as",
        "asm",
        "asm_fragment",
        "async",
        "attribute",
        "auto",
        "await",
        "become",
        "binding_array",
        "cast",
        "catch",
        "class",
        "co_await",
        "co_return",
        "co_yield",
        "coherent",
        "column_major",
        "common",
        "compile",
        "compile_fragment",
        "concept",
        "const_cast",
        "consteval",
        "constexpr",
        "constinit",
        "crate",
        "debugger",
        "decltype",
        "delete",
        "demote",
        "demote_to_helper",
        "do",
        "dynamic_cast",
        "enum",
        "explicit",
        "export",
        "extends",
        "extern",
        "external",
        "fallthrough",
        "filter",
        "final",
        "finally",
        "friend",
        "from",
        "fxgroup",
        "get",
        "goto",
        "groupshared",
        "highp",
        "impl",
        "implements",
        "import",
        "in",
        "inline",
        "instanceof",
        "interface",
        "layout",
        "lowp",
        "macro",
        "match",
        "mediump",
        "meta",
        "mod",
        "module",
        "move",
        "mut",
        "mutable",
        "namespace",
        "new",
        "nil",
        "noexcept",
        "noinline",
        "nointerpolation",
        "noperspective",
        "null",
        "nullptr",
        "of",
        "operator",
        "package",
        "packoffset",
        "partition",
        "pass",
        "patch",
        "pixelfragment",
        "precise",
        "precision",
        "premerge",
        "priv",
        "protected",
        "pub",
        "public",
        "readonly",
        "ref",
        "regardless",
        "register",
        "reinterpret_cast",
        "require",
        "resource",
        "restrict",
        "self",
        "set",
        "shared",
        "sizeof",
        "smooth",
        "snorm",
        "static",
        "static_assert",
        "static_cast",
        "std",
        "subroutine",
        "super",
        "target",
        "template",
        "this",
        "thread_local",
        "throw",
        "trait",
        "try",
        "type",
        "typedef",
        "typeid",
        "typename",
        "typeof",
        "union",
        "unless",
        "unorm",
        "unsafe",
        "unsized",
        "use",
        "using",
        "varying",
        "virtual",
        "volatile",
        "wgsl",
        "with",
        "writeonly",
        "yield",
    ];

    fn check_wgsl_identifiers(&self, errors: &mut Vec<FlowError>) {
        let keywords: HashSet<&str> = Self::WGSL_KEYWORDS.iter().copied().collect();

        let check = |name: &str, errors: &mut Vec<FlowError>| {
            if name.starts_with("__") {
                errors.push(FlowError::InvalidIdentifier {
                    name: name.to_string(),
                    reason: "identifiers starting with '__' are reserved in WGSL".to_string(),
                });
            }
            if !name
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            {
                errors.push(FlowError::InvalidIdentifier {
                    name: name.to_string(),
                    reason: "must start with a letter or underscore".to_string(),
                });
            } else if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                errors.push(FlowError::InvalidIdentifier {
                    name: name.to_string(),
                    reason: "must contain only letters, digits, and underscores".to_string(),
                });
            }
            if keywords.contains(name) {
                errors.push(FlowError::InvalidIdentifier {
                    name: name.to_string(),
                    reason: format!("'{}' is a reserved WGSL keyword", name),
                });
            }
        };

        for input in &self.inputs {
            check(&input.name, errors);
        }
        for node in &self.nodes {
            check(&node.name, errors);
        }
        for output in &self.outputs {
            check(&output.name, errors);
        }
    }

    fn check_duplicates(&self, errors: &mut Vec<FlowError>) {
        let mut names = HashSet::new();
        for input in &self.inputs {
            if !names.insert(&input.name) {
                errors.push(FlowError::DuplicateName {
                    name: input.name.clone(),
                });
            }
        }
        for node in &self.nodes {
            if !names.insert(&node.name) {
                errors.push(FlowError::DuplicateName {
                    name: node.name.clone(),
                });
            }
        }
    }

    fn check_references(&self, errors: &mut Vec<FlowError>) {
        let mut all_names: HashSet<String> = HashSet::new();
        for input in &self.inputs {
            all_names.insert(input.name.clone());
        }
        for node in &self.nodes {
            all_names.insert(node.name.clone());
        }

        for node in &self.nodes {
            let mut refs = HashSet::new();
            node.expr.collect_refs(&mut refs);
            for r in &refs {
                if !all_names.contains(r) {
                    errors.push(FlowError::UndefinedReference {
                        in_node: node.name.clone(),
                        name: r.clone(),
                    });
                }
            }
        }

        // Check output expressions too
        for output in &self.outputs {
            if let Some(expr) = &output.expr {
                let mut refs = HashSet::new();
                expr.collect_refs(&mut refs);
                for r in &refs {
                    if !all_names.contains(r) {
                        errors.push(FlowError::UndefinedReference {
                            in_node: format!("output:{}", output.name),
                            name: r.clone(),
                        });
                    }
                }
            } else {
                // Bare output name — must reference an existing name
                if !all_names.contains(&output.name) {
                    errors.push(FlowError::UndefinedReference {
                        in_node: "output".to_string(),
                        name: output.name.clone(),
                    });
                }
            }
        }
    }

    /// Topological sort using Kahn's algorithm.
    /// Returns Ok(order) with node indices in dependency order,
    /// or Err(cycle_nodes) with names of nodes in the cycle.
    fn topological_sort(&self) -> Result<Vec<usize>, Vec<String>> {
        let n = self.nodes.len();
        if n == 0 {
            return Ok(Vec::new());
        }

        // Build name → node index map
        let mut node_index: HashMap<&str, usize> = HashMap::new();
        for (i, node) in self.nodes.iter().enumerate() {
            node_index.insert(&node.name, i);
        }

        // Build adjacency list and in-degree count
        // Edge: dependency → dependent (if node B references node A, edge A → B)
        let mut in_degree = vec![0usize; n];
        let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); n];

        for (i, node) in self.nodes.iter().enumerate() {
            let mut refs = HashSet::new();
            node.expr.collect_refs(&mut refs);
            for r in &refs {
                if let Some(&dep_idx) = node_index.get(r.as_str()) {
                    // dep_idx is a dependency of node i
                    dependents[dep_idx].push(i);
                    in_degree[i] += 1;
                }
                // References to inputs don't create node-to-node edges
            }
        }

        // Kahn's algorithm: start with nodes that have no node dependencies
        let mut queue: VecDeque<usize> = in_degree
            .iter()
            .enumerate()
            .filter(|(_, &deg)| deg == 0)
            .map(|(i, _)| i)
            .collect();

        let mut order = Vec::with_capacity(n);
        while let Some(idx) = queue.pop_front() {
            order.push(idx);
            for &dep in &dependents[idx] {
                in_degree[dep] -= 1;
                if in_degree[dep] == 0 {
                    queue.push_back(dep);
                }
            }
        }

        if order.len() == n {
            Ok(order)
        } else {
            // Cycle detected — collect names of unprocessed nodes
            let processed: HashSet<usize> = order.iter().copied().collect();
            let cycle_nodes: Vec<String> = (0..n)
                .filter(|i| !processed.contains(i))
                .map(|i| self.nodes[i].name.clone())
                .collect();
            Err(cycle_nodes)
        }
    }

    /// Infer types for all nodes in topological order
    fn infer_types(&mut self, errors: &mut Vec<FlowError>) {
        // Build type map from inputs
        let mut type_map: HashMap<String, FlowType> = HashMap::new();
        for input in &self.inputs {
            let ty = input.ty.unwrap_or_else(|| match &input.source {
                FlowInputSource::Builtin(b) => b.output_type(),
                FlowInputSource::Buffer { ty, .. } => *ty,
                FlowInputSource::CssProperty(_) | FlowInputSource::EnvVar(_) => FlowType::Float,
                FlowInputSource::Auto => FlowType::Float,
            });
            type_map.insert(input.name.clone(), ty);
        }

        // Process nodes in topological order
        for &idx in &self.topo_order {
            let node = &self.nodes[idx];
            match infer_expr_type(&node.expr, &type_map) {
                Ok(ty) => {
                    type_map.insert(node.name.clone(), ty);
                    // Store inferred type back
                    self.nodes[idx].inferred_type = Some(ty);
                }
                Err(msg) => {
                    errors.push(FlowError::TypeMismatch {
                        in_node: node.name.clone(),
                        message: msg,
                    });
                    // Default to Float so we can continue checking
                    type_map.insert(node.name.clone(), FlowType::Float);
                    self.nodes[idx].inferred_type = Some(FlowType::Float);
                }
            }
        }
    }

    fn check_function_args(&self, errors: &mut Vec<FlowError>) {
        for node in &self.nodes {
            check_args_recursive(&node.expr, &node.name, errors);
        }
    }

    fn check_outputs(&self, errors: &mut Vec<FlowError>) {
        match self.target {
            FlowTarget::Fragment => {
                // Fragment flows should have a color output
                let has_color = self
                    .outputs
                    .iter()
                    .any(|o| o.target == FlowOutputTarget::Color);
                if !has_color && self.outputs.is_empty() {
                    errors.push(FlowError::MissingOutput {
                        message: "fragment flow needs at least one output (e.g., 'output color')"
                            .to_string(),
                    });
                }
            }
            FlowTarget::Compute => {
                // Compute flows should have buffer outputs
                let has_buffer = self
                    .outputs
                    .iter()
                    .any(|o| matches!(o.target, FlowOutputTarget::Buffer { .. }));
                if !has_buffer && self.outputs.is_empty() {
                    errors.push(FlowError::MissingOutput {
                        message: "compute flow needs at least one buffer output".to_string(),
                    });
                }
            }
            FlowTarget::Vertex => {
                // Vertex flows must produce a position output
                let has_position = self
                    .outputs
                    .iter()
                    .any(|o| o.target == FlowOutputTarget::Position);
                if !has_position {
                    errors.push(FlowError::MissingOutput {
                        message: "vertex flow needs 'output position' (vec4 clip-space)"
                            .to_string(),
                    });
                }
            }
            FlowTarget::Material => {
                // Material flows should produce at least an albedo output
                let has_albedo = self
                    .outputs
                    .iter()
                    .any(|o| o.target == FlowOutputTarget::Albedo);
                if !has_albedo && self.outputs.is_empty() {
                    errors.push(FlowError::MissingOutput {
                        message: "material flow needs at least 'output albedo' (vec4 base color)"
                            .to_string(),
                    });
                }
            }
        }
    }
}

/// Infer the type of an expression given a type map of known names
fn infer_expr_type(
    expr: &FlowExpr,
    type_map: &HashMap<String, FlowType>,
) -> Result<FlowType, String> {
    match expr {
        FlowExpr::Float(_) => Ok(FlowType::Float),
        FlowExpr::Vec2(_, _) => Ok(FlowType::Vec2),
        FlowExpr::Vec3(_, _, _) => Ok(FlowType::Vec3),
        FlowExpr::Vec4(_, _, _, _) => Ok(FlowType::Vec4),
        FlowExpr::Color(_, _, _, _) => Ok(FlowType::Vec4),

        FlowExpr::Ref(name) => type_map
            .get(name)
            .copied()
            .ok_or_else(|| format!("undefined reference '{}'", name)),

        FlowExpr::Neg(a) => infer_expr_type(a, type_map),

        FlowExpr::Swizzle(_expr, components) => match components.len() {
            1 => Ok(FlowType::Float),
            2 => Ok(FlowType::Vec2),
            3 => Ok(FlowType::Vec3),
            4 => Ok(FlowType::Vec4),
            _ => Err(format!("invalid swizzle length: {}", components.len())),
        },

        FlowExpr::Add(a, b) | FlowExpr::Sub(a, b) | FlowExpr::Mul(a, b) | FlowExpr::Div(a, b) => {
            let ta = infer_expr_type(a, type_map)?;
            let tb = infer_expr_type(b, type_map)?;
            ta.broadcast_with(&tb)
                .ok_or_else(|| format!("cannot broadcast {} with {}", ta, tb))
        }

        FlowExpr::Call { func, args } => {
            let arg_types: Vec<FlowType> = args
                .iter()
                .map(|a| infer_expr_type(a, type_map))
                .collect::<Result<Vec<_>, _>>()?;
            func.return_type(&arg_types)
                .ok_or_else(|| format!("cannot determine return type for {:?}", func))
        }
    }
}

/// Recursively check function argument counts
fn check_args_recursive(expr: &FlowExpr, node_name: &str, errors: &mut Vec<FlowError>) {
    match expr {
        FlowExpr::Call { func, args } => {
            let (min, max) = func.arg_count();
            if args.len() < min || args.len() > max {
                errors.push(FlowError::ArgCountMismatch {
                    in_node: node_name.to_string(),
                    func: format!("{:?}", func),
                    expected: (min, max),
                    got: args.len(),
                });
            }
            for arg in args {
                check_args_recursive(arg, node_name, errors);
            }
        }
        FlowExpr::Add(a, b)
        | FlowExpr::Sub(a, b)
        | FlowExpr::Mul(a, b)
        | FlowExpr::Div(a, b)
        | FlowExpr::Vec2(a, b) => {
            check_args_recursive(a, node_name, errors);
            check_args_recursive(b, node_name, errors);
        }
        FlowExpr::Vec3(a, b, c) => {
            check_args_recursive(a, node_name, errors);
            check_args_recursive(b, node_name, errors);
            check_args_recursive(c, node_name, errors);
        }
        FlowExpr::Vec4(a, b, c, d) => {
            check_args_recursive(a, node_name, errors);
            check_args_recursive(b, node_name, errors);
            check_args_recursive(c, node_name, errors);
            check_args_recursive(d, node_name, errors);
        }
        FlowExpr::Neg(a) | FlowExpr::Swizzle(a, _) => check_args_recursive(a, node_name, errors),
        _ => {}
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

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

        // node falloff = smoothstep(0.6, 0.0, dist)
        graph.nodes.push(FlowNode {
            name: "falloff".to_string(),
            expr: FlowExpr::Call {
                func: FlowFunc::Smoothstep,
                args: vec![
                    FlowExpr::Float(0.6),
                    FlowExpr::Float(0.0),
                    FlowExpr::Ref("dist".to_string()),
                ],
            },
            inferred_type: None,
        });

        // node height = wave * falloff * 0.15
        graph.nodes.push(FlowNode {
            name: "height".to_string(),
            expr: FlowExpr::Mul(
                Box::new(FlowExpr::Mul(
                    Box::new(FlowExpr::Ref("wave".to_string())),
                    Box::new(FlowExpr::Ref("falloff".to_string())),
                )),
                Box::new(FlowExpr::Float(0.15)),
            ),
            inferred_type: None,
        });

        graph.outputs.push(FlowOutput {
            name: "color".to_string(),
            target: FlowOutputTarget::Color,
            expr: Some(FlowExpr::Vec4(
                Box::new(FlowExpr::Ref("height".to_string())),
                Box::new(FlowExpr::Ref("height".to_string())),
                Box::new(FlowExpr::Ref("height".to_string())),
                Box::new(FlowExpr::Float(1.0)),
            )),
        });

        graph
    }

    // -----------------------------------------------------------------------
    // Valid DAG
    // -----------------------------------------------------------------------

    #[test]
    fn test_valid_dag_validates() {
        let mut graph = make_ripple_flow();
        assert!(graph.validate(None).is_ok());
    }

    #[test]
    fn test_topo_order_correct() {
        let mut graph = make_ripple_flow();
        graph.validate(None).unwrap();
        // dist must come before wave, falloff, and height
        let dist_pos = graph.topo_order.iter().position(|&i| i == 0).unwrap();
        let wave_pos = graph.topo_order.iter().position(|&i| i == 1).unwrap();
        let falloff_pos = graph.topo_order.iter().position(|&i| i == 2).unwrap();
        let height_pos = graph.topo_order.iter().position(|&i| i == 3).unwrap();
        assert!(dist_pos < wave_pos);
        assert!(dist_pos < falloff_pos);
        assert!(wave_pos < height_pos);
        assert!(falloff_pos < height_pos);
    }

    #[test]
    fn test_type_inference() {
        let mut graph = make_ripple_flow();
        graph.validate(None).unwrap();

        assert_eq!(graph.nodes[0].inferred_type, Some(FlowType::Float)); // dist = distance(vec2, vec2)
        assert_eq!(graph.nodes[1].inferred_type, Some(FlowType::Float)); // wave = sin(float)
        assert_eq!(graph.nodes[2].inferred_type, Some(FlowType::Float)); // falloff = smoothstep(...)
        assert_eq!(graph.nodes[3].inferred_type, Some(FlowType::Float)); // height = float * float
    }

    // -----------------------------------------------------------------------
    // Cycle detection
    // -----------------------------------------------------------------------

    #[test]
    fn test_cycle_detected() {
        let mut graph = FlowGraph::new("cyclic");
        graph.target = FlowTarget::Fragment;

        graph.inputs.push(FlowInput {
            name: "x".to_string(),
            source: FlowInputSource::Builtin(BuiltinVar::Time),
            ty: Some(FlowType::Float),
        });

        // a depends on b, b depends on a → cycle
        graph.nodes.push(FlowNode {
            name: "a".to_string(),
            expr: FlowExpr::Add(
                Box::new(FlowExpr::Ref("b".to_string())),
                Box::new(FlowExpr::Float(1.0)),
            ),
            inferred_type: None,
        });
        graph.nodes.push(FlowNode {
            name: "b".to_string(),
            expr: FlowExpr::Add(
                Box::new(FlowExpr::Ref("a".to_string())),
                Box::new(FlowExpr::Float(1.0)),
            ),
            inferred_type: None,
        });

        graph.outputs.push(FlowOutput {
            name: "color".to_string(),
            target: FlowOutputTarget::Color,
            expr: Some(FlowExpr::Ref("a".to_string())),
        });

        let result = graph.validate(None);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| matches!(e, FlowError::CycleDetected { .. })));
    }

    #[test]
    fn test_self_reference_cycle() {
        let mut graph = FlowGraph::new("self-ref");
        graph.target = FlowTarget::Fragment;

        // node a = a + 1 → self-reference cycle
        graph.nodes.push(FlowNode {
            name: "a".to_string(),
            expr: FlowExpr::Add(
                Box::new(FlowExpr::Ref("a".to_string())),
                Box::new(FlowExpr::Float(1.0)),
            ),
            inferred_type: None,
        });

        graph.outputs.push(FlowOutput {
            name: "color".to_string(),
            target: FlowOutputTarget::Color,
            expr: Some(FlowExpr::Ref("a".to_string())),
        });

        let result = graph.validate(None);
        assert!(result.is_err());
    }

    #[test]
    fn test_three_node_cycle() {
        let mut graph = FlowGraph::new("triangle");
        graph.target = FlowTarget::Fragment;

        // a → b → c → a
        graph.nodes.push(FlowNode {
            name: "a".to_string(),
            expr: FlowExpr::Ref("c".to_string()),
            inferred_type: None,
        });
        graph.nodes.push(FlowNode {
            name: "b".to_string(),
            expr: FlowExpr::Ref("a".to_string()),
            inferred_type: None,
        });
        graph.nodes.push(FlowNode {
            name: "c".to_string(),
            expr: FlowExpr::Ref("b".to_string()),
            inferred_type: None,
        });

        graph.outputs.push(FlowOutput {
            name: "color".to_string(),
            target: FlowOutputTarget::Color,
            expr: Some(FlowExpr::Ref("a".to_string())),
        });

        let result = graph.validate(None);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        let cycle_err = errors
            .iter()
            .find(|e| matches!(e, FlowError::CycleDetected { .. }));
        assert!(cycle_err.is_some());
        if let FlowError::CycleDetected { nodes } = cycle_err.unwrap() {
            assert_eq!(nodes.len(), 3);
        }
    }

    // -----------------------------------------------------------------------
    // Error cases
    // -----------------------------------------------------------------------

    #[test]
    fn test_undefined_reference() {
        let mut graph = FlowGraph::new("bad-ref");
        graph.target = FlowTarget::Fragment;

        graph.nodes.push(FlowNode {
            name: "a".to_string(),
            expr: FlowExpr::Ref("nonexistent".to_string()),
            inferred_type: None,
        });

        graph.outputs.push(FlowOutput {
            name: "color".to_string(),
            target: FlowOutputTarget::Color,
            expr: Some(FlowExpr::Ref("a".to_string())),
        });

        let result = graph.validate(None);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| matches!(e, FlowError::UndefinedReference { .. })));
    }

    #[test]
    fn test_duplicate_name() {
        let mut graph = FlowGraph::new("dup");
        graph.target = FlowTarget::Fragment;

        graph.inputs.push(FlowInput {
            name: "x".to_string(),
            source: FlowInputSource::Auto,
            ty: Some(FlowType::Float),
        });
        graph.nodes.push(FlowNode {
            name: "x".to_string(), // conflicts with input
            expr: FlowExpr::Float(1.0),
            inferred_type: None,
        });

        graph.outputs.push(FlowOutput {
            name: "color".to_string(),
            target: FlowOutputTarget::Color,
            expr: Some(FlowExpr::Ref("x".to_string())),
        });

        let result = graph.validate(None);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| matches!(e, FlowError::DuplicateName { .. })));
    }

    #[test]
    fn test_type_broadcast() {
        // vec3 * float should broadcast to vec3
        let mut graph = FlowGraph::new("broadcast");
        graph.target = FlowTarget::Fragment;

        graph.inputs.push(FlowInput {
            name: "uv".to_string(),
            source: FlowInputSource::Builtin(BuiltinVar::Uv),
            ty: Some(FlowType::Vec2),
        });

        // node scaled = uv * 2.0 → should be Vec2
        graph.nodes.push(FlowNode {
            name: "scaled".to_string(),
            expr: FlowExpr::Mul(
                Box::new(FlowExpr::Ref("uv".to_string())),
                Box::new(FlowExpr::Float(2.0)),
            ),
            inferred_type: None,
        });

        graph.outputs.push(FlowOutput {
            name: "color".to_string(),
            target: FlowOutputTarget::Color,
            expr: Some(FlowExpr::Ref("scaled".to_string())),
        });

        graph.validate(None).unwrap();
        assert_eq!(graph.nodes[0].inferred_type, Some(FlowType::Vec2));
    }

    #[test]
    fn test_incompatible_types() {
        let mut graph = FlowGraph::new("bad-types");
        graph.target = FlowTarget::Fragment;

        graph.inputs.push(FlowInput {
            name: "a".to_string(),
            source: FlowInputSource::Auto,
            ty: Some(FlowType::Vec2),
        });
        graph.inputs.push(FlowInput {
            name: "b".to_string(),
            source: FlowInputSource::Auto,
            ty: Some(FlowType::Vec3),
        });

        // vec2 + vec3 → type error
        graph.nodes.push(FlowNode {
            name: "c".to_string(),
            expr: FlowExpr::Add(
                Box::new(FlowExpr::Ref("a".to_string())),
                Box::new(FlowExpr::Ref("b".to_string())),
            ),
            inferred_type: None,
        });

        graph.outputs.push(FlowOutput {
            name: "color".to_string(),
            target: FlowOutputTarget::Color,
            expr: Some(FlowExpr::Ref("c".to_string())),
        });

        let result = graph.validate(None);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| matches!(e, FlowError::TypeMismatch { .. })));
    }

    // -----------------------------------------------------------------------
    // Compute target
    // -----------------------------------------------------------------------

    #[test]
    fn test_compute_flow_needs_buffer_output() {
        let mut graph = FlowGraph::new("compute-no-output");
        graph.target = FlowTarget::Compute;
        graph.workgroup_size = Some(64);

        graph.inputs.push(FlowInput {
            name: "pos".to_string(),
            source: FlowInputSource::Buffer {
                name: "positions".to_string(),
                ty: FlowType::Vec4,
            },
            ty: Some(FlowType::Vec4),
        });

        graph.nodes.push(FlowNode {
            name: "new_pos".to_string(),
            expr: FlowExpr::Add(
                Box::new(FlowExpr::Ref("pos".to_string())),
                Box::new(FlowExpr::Float(1.0)),
            ),
            inferred_type: None,
        });

        // No output → should fail
        let result = graph.validate(None);
        assert!(result.is_err());
    }

    #[test]
    fn test_compute_flow_with_buffer_output() {
        let mut graph = FlowGraph::new("compute-ok");
        graph.target = FlowTarget::Compute;
        graph.workgroup_size = Some(64);

        graph.inputs.push(FlowInput {
            name: "pos".to_string(),
            source: FlowInputSource::Buffer {
                name: "positions".to_string(),
                ty: FlowType::Vec4,
            },
            ty: Some(FlowType::Vec4),
        });

        graph.nodes.push(FlowNode {
            name: "new_pos".to_string(),
            expr: FlowExpr::Add(
                Box::new(FlowExpr::Ref("pos".to_string())),
                Box::new(FlowExpr::Float(1.0)),
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

        assert!(graph.validate(None).is_ok());
    }

    // -----------------------------------------------------------------------
    // Empty graph
    // -----------------------------------------------------------------------

    #[test]
    fn test_empty_fragment_flow_errors() {
        let mut graph = FlowGraph::new("empty");
        graph.target = FlowTarget::Fragment;

        let result = graph.validate(None);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // FlowFunc parsing
    // -----------------------------------------------------------------------

    #[test]
    fn test_flowfunc_from_str() {
        assert_eq!(FlowFunc::parse("sin"), Some(FlowFunc::Sin));
        assert_eq!(FlowFunc::parse("distance"), Some(FlowFunc::Distance));
        assert_eq!(
            FlowFunc::parse("sdf-smooth-union"),
            Some(FlowFunc::SdfSmoothUnion)
        );
        assert_eq!(
            FlowFunc::parse("sdf_smooth_union"),
            Some(FlowFunc::SdfSmoothUnion)
        );
        assert_eq!(FlowFunc::parse("unknown"), None);
    }

    // -----------------------------------------------------------------------
    // FlowType broadcast
    // -----------------------------------------------------------------------

    #[test]
    fn test_type_broadcast_rules() {
        assert_eq!(
            FlowType::Float.broadcast_with(&FlowType::Float),
            Some(FlowType::Float)
        );
        assert_eq!(
            FlowType::Float.broadcast_with(&FlowType::Vec3),
            Some(FlowType::Vec3)
        );
        assert_eq!(
            FlowType::Vec3.broadcast_with(&FlowType::Float),
            Some(FlowType::Vec3)
        );
        assert_eq!(FlowType::Vec2.broadcast_with(&FlowType::Vec3), None);
        assert_eq!(
            FlowType::Vec4.broadcast_with(&FlowType::Vec4),
            Some(FlowType::Vec4)
        );
    }

    // ===================================================================
    // Semantic step expansion tests
    // ===================================================================

    #[test]
    fn test_step_type_from_str() {
        assert_eq!(
            StepType::parse("pattern-noise"),
            Some(StepType::PatternNoise)
        );
        assert_eq!(StepType::parse("color-ramp"), Some(StepType::ColorRamp));
        assert_eq!(
            StepType::parse("compose-blend"),
            Some(StepType::ComposeBlend)
        );
        assert_eq!(
            StepType::parse("transform-warp"),
            Some(StepType::TransformWarp)
        );
        assert_eq!(
            StepType::parse("surface-light"),
            Some(StepType::SurfaceLight)
        );
        assert_eq!(
            StepType::parse("adjust-falloff"),
            Some(StepType::AdjustFalloff)
        );
        assert_eq!(StepType::parse("unknown-step"), None);
    }

    #[test]
    fn test_expand_pattern_noise_produces_valid_nodes() {
        let mut graph = FlowGraph::new("test_noise");
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

        let mut params = HashMap::new();
        params.insert("scale".to_string(), StepParam::Expr(FlowExpr::Float(3.0)));
        params.insert("detail".to_string(), StepParam::Int(4));

        graph.steps.push(FlowStep {
            name: "n".to_string(),
            step_type: StepType::PatternNoise,
            params,
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

        // Expansion + validation should succeed
        graph.validate(None).unwrap();

        // Steps should be consumed, nodes should be expanded
        assert!(graph.steps.is_empty());
        assert!(graph.nodes.iter().any(|n| n.name == "n"));
        assert!(graph.nodes.iter().any(|n| n.name.contains("_s_n_")));
    }

    #[test]
    fn test_expand_pattern_ripple() {
        let mut graph = FlowGraph::new("test_ripple");
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

        let mut params = HashMap::new();
        params.insert(
            "center".to_string(),
            StepParam::Expr(FlowExpr::Vec2(
                Box::new(FlowExpr::Float(0.5)),
                Box::new(FlowExpr::Float(0.5)),
            )),
        );

        graph.steps.push(FlowStep {
            name: "r".to_string(),
            step_type: StepType::PatternRipple,
            params,
        });

        graph.outputs.push(FlowOutput {
            name: "color".to_string(),
            target: FlowOutputTarget::Color,
            expr: Some(FlowExpr::Vec4(
                Box::new(FlowExpr::Ref("r".to_string())),
                Box::new(FlowExpr::Ref("r".to_string())),
                Box::new(FlowExpr::Ref("r".to_string())),
                Box::new(FlowExpr::Float(1.0)),
            )),
        });

        graph.validate(None).unwrap();
        assert!(graph.steps.is_empty());
        assert!(graph.nodes.iter().any(|n| n.name == "r"));
    }

    #[test]
    fn test_expand_color_ramp() {
        let mut graph = FlowGraph::new("test_ramp");
        graph.inputs.push(FlowInput {
            name: "uv".to_string(),
            source: FlowInputSource::Builtin(BuiltinVar::Uv),
            ty: Some(FlowType::Vec2),
        });

        // A scalar source node
        graph.nodes.push(FlowNode {
            name: "val".to_string(),
            expr: FlowExpr::Swizzle(Box::new(FlowExpr::Ref("uv".to_string())), "x".to_string()),
            inferred_type: None,
        });

        let mut params = HashMap::new();
        params.insert(
            "source".to_string(),
            StepParam::Expr(FlowExpr::Ref("val".to_string())),
        );
        params.insert(
            "stops".to_string(),
            StepParam::ColorStops(vec![
                (FlowExpr::Color(0.0, 0.0, 0.0, 1.0), 0.0),
                (FlowExpr::Color(1.0, 1.0, 1.0, 1.0), 1.0),
            ]),
        );

        graph.steps.push(FlowStep {
            name: "c".to_string(),
            step_type: StepType::ColorRamp,
            params,
        });

        graph.outputs.push(FlowOutput {
            name: "color".to_string(),
            target: FlowOutputTarget::Color,
            expr: Some(FlowExpr::Ref("c".to_string())),
        });

        graph.validate(None).unwrap();
        assert!(graph.steps.is_empty());
        // Should have interpolation nodes
        assert!(graph.nodes.iter().any(|n| n.name == "c"));
        assert!(graph.nodes.iter().any(|n| n.name.contains("_s_c_")));
    }

    #[test]
    fn test_chain_desugaring() {
        let mut graph = FlowGraph::new("test_chain");
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

        let mut link1_params = HashMap::new();
        link1_params.insert(
            "center".to_string(),
            StepParam::Expr(FlowExpr::Vec2(
                Box::new(FlowExpr::Float(0.5)),
                Box::new(FlowExpr::Float(0.5)),
            )),
        );

        let mut link2_params = HashMap::new();
        link2_params.insert("radius".to_string(), StepParam::Expr(FlowExpr::Float(0.5)));

        graph.chains.push(FlowChain {
            name: "effect".to_string(),
            links: vec![
                ChainLink {
                    step_type: StepType::PatternRipple,
                    params: link1_params,
                },
                ChainLink {
                    step_type: StepType::AdjustFalloff,
                    params: link2_params,
                },
            ],
        });

        graph.outputs.push(FlowOutput {
            name: "color".to_string(),
            target: FlowOutputTarget::Color,
            expr: Some(FlowExpr::Vec4(
                Box::new(FlowExpr::Ref("effect".to_string())),
                Box::new(FlowExpr::Ref("effect".to_string())),
                Box::new(FlowExpr::Ref("effect".to_string())),
                Box::new(FlowExpr::Float(1.0)),
            )),
        });

        graph.validate(None).unwrap();
        assert!(graph.chains.is_empty());
        assert!(graph.steps.is_empty());
        // The chain's final output should be "effect"
        assert!(graph.nodes.iter().any(|n| n.name == "effect"));
    }

    #[test]
    fn test_use_composition() {
        // Create a base flow
        let mut base = FlowGraph::new("noise-base");
        base.inputs.push(FlowInput {
            name: "uv".to_string(),
            source: FlowInputSource::Builtin(BuiltinVar::Uv),
            ty: Some(FlowType::Vec2),
        });
        base.inputs.push(FlowInput {
            name: "time".to_string(),
            source: FlowInputSource::Builtin(BuiltinVar::Time),
            ty: Some(FlowType::Float),
        });
        base.nodes.push(FlowNode {
            name: "n".to_string(),
            expr: FlowExpr::Call {
                func: FlowFunc::Fbm,
                args: vec![FlowExpr::Ref("uv".to_string()), FlowExpr::Float(4.0)],
            },
            inferred_type: None,
        });
        base.outputs.push(FlowOutput {
            name: "value".to_string(),
            target: FlowOutputTarget::Color,
            expr: Some(FlowExpr::Ref("n".to_string())),
        });

        let mut registry = HashMap::new();
        registry.insert("noise-base".to_string(), base);

        // Create a derived flow that uses the base
        let mut graph = FlowGraph::new("colored");
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

        graph.uses.push(FlowUse {
            flow_name: "noise-base".to_string(),
        });

        // Hyphens become underscores in the prefix: noise-base → noise_base_
        graph.outputs.push(FlowOutput {
            name: "color".to_string(),
            target: FlowOutputTarget::Color,
            expr: Some(FlowExpr::Vec4(
                Box::new(FlowExpr::Ref("noise_base_n".to_string())),
                Box::new(FlowExpr::Ref("noise_base_n".to_string())),
                Box::new(FlowExpr::Ref("noise_base_n".to_string())),
                Box::new(FlowExpr::Float(1.0)),
            )),
        });

        graph.validate(Some(&registry)).unwrap();
        assert!(graph.uses.is_empty());
        // Inlined nodes should be prefixed: noise-base → noise_base_ prefix
        assert!(graph.nodes.iter().any(|n| n.name == "noise_base_n"));
    }

    #[test]
    fn test_missing_step_param_errors() {
        // color-ramp requires "source" and "stops" params
        let step = FlowStep {
            name: "bad".to_string(),
            step_type: StepType::ColorRamp,
            params: HashMap::new(), // missing required params
        };

        let mut graph = FlowGraph::new("test_bad");
        graph.inputs.push(FlowInput {
            name: "uv".to_string(),
            source: FlowInputSource::Builtin(BuiltinVar::Uv),
            ty: Some(FlowType::Vec2),
        });
        graph.steps.push(step);
        graph.outputs.push(FlowOutput {
            name: "color".to_string(),
            target: FlowOutputTarget::Color,
            expr: Some(FlowExpr::Float(1.0)),
        });

        let result = graph.validate(None);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(errs
            .iter()
            .any(|e| matches!(e, FlowError::MissingStepParam { .. })));
    }

    #[test]
    fn test_use_flow_not_found_errors() {
        let mut graph = FlowGraph::new("test_missing_use");
        graph.inputs.push(FlowInput {
            name: "uv".to_string(),
            source: FlowInputSource::Builtin(BuiltinVar::Uv),
            ty: Some(FlowType::Vec2),
        });
        graph.uses.push(FlowUse {
            flow_name: "nonexistent".to_string(),
        });
        graph.outputs.push(FlowOutput {
            name: "color".to_string(),
            target: FlowOutputTarget::Color,
            expr: Some(FlowExpr::Float(1.0)),
        });

        // With no registry
        let result = graph.validate(None);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(errs
            .iter()
            .any(|e| matches!(e, FlowError::FlowNotFound { .. })));
    }

    #[test]
    fn test_mixed_step_and_node() {
        let mut graph = FlowGraph::new("test_mixed");
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

        // Raw node
        graph.nodes.push(FlowNode {
            name: "speed".to_string(),
            expr: FlowExpr::Float(0.5),
            inferred_type: None,
        });

        // Semantic step referencing the raw node
        let mut params = HashMap::new();
        params.insert("scale".to_string(), StepParam::Expr(FlowExpr::Float(3.0)));
        params.insert(
            "animation".to_string(),
            StepParam::Expr(FlowExpr::Mul(
                Box::new(FlowExpr::Ref("time".to_string())),
                Box::new(FlowExpr::Ref("speed".to_string())),
            )),
        );

        graph.steps.push(FlowStep {
            name: "n".to_string(),
            step_type: StepType::PatternNoise,
            params,
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
        // Both the raw node and expanded step nodes should exist
        assert!(graph.nodes.iter().any(|n| n.name == "speed"));
        assert!(graph.nodes.iter().any(|n| n.name == "n"));
    }

    #[test]
    fn test_compose_blend_expansion() {
        let mut graph = FlowGraph::new("test_blend");
        graph.inputs.push(FlowInput {
            name: "uv".to_string(),
            source: FlowInputSource::Builtin(BuiltinVar::Uv),
            ty: Some(FlowType::Vec2),
        });

        // Two source values
        graph.nodes.push(FlowNode {
            name: "a".to_string(),
            expr: FlowExpr::Vec4(
                Box::new(FlowExpr::Float(1.0)),
                Box::new(FlowExpr::Float(0.0)),
                Box::new(FlowExpr::Float(0.0)),
                Box::new(FlowExpr::Float(1.0)),
            ),
            inferred_type: None,
        });
        graph.nodes.push(FlowNode {
            name: "b".to_string(),
            expr: FlowExpr::Vec4(
                Box::new(FlowExpr::Float(0.0)),
                Box::new(FlowExpr::Float(1.0)),
                Box::new(FlowExpr::Float(0.0)),
                Box::new(FlowExpr::Float(1.0)),
            ),
            inferred_type: None,
        });

        let mut params = HashMap::new();
        params.insert(
            "a".to_string(),
            StepParam::Expr(FlowExpr::Ref("a".to_string())),
        );
        params.insert(
            "b".to_string(),
            StepParam::Expr(FlowExpr::Ref("b".to_string())),
        );
        params.insert("mode".to_string(), StepParam::Ident("multiply".to_string()));

        graph.steps.push(FlowStep {
            name: "blended".to_string(),
            step_type: StepType::ComposeBlend,
            params,
        });

        graph.outputs.push(FlowOutput {
            name: "color".to_string(),
            target: FlowOutputTarget::Color,
            expr: Some(FlowExpr::Ref("blended".to_string())),
        });

        graph.validate(None).unwrap();
        assert!(graph.nodes.iter().any(|n| n.name == "blended"));
    }

    // ===================================================================
    // New semantic step expansion tests
    // ===================================================================

    #[test]
    fn test_step_expansion_pattern_worley() {
        let mut graph = FlowGraph::new("test_worley");
        graph.inputs.push(FlowInput {
            name: "uv".to_string(),
            source: FlowInputSource::Builtin(BuiltinVar::Uv),
            ty: Some(FlowType::Vec2),
        });

        let mut params = HashMap::new();
        params.insert("scale".to_string(), StepParam::Expr(FlowExpr::Float(18.0)));
        params.insert(
            "threshold".to_string(),
            StepParam::Expr(FlowExpr::Float(0.05)),
        );
        params.insert("mask".to_string(), StepParam::Expr(FlowExpr::Float(1.0)));

        graph.steps.push(FlowStep {
            name: "w".to_string(),
            step_type: StepType::PatternWorley,
            params,
        });

        graph.outputs.push(FlowOutput {
            name: "color".to_string(),
            target: FlowOutputTarget::Color,
            expr: Some(FlowExpr::Vec4(
                Box::new(FlowExpr::Ref("w".to_string())),
                Box::new(FlowExpr::Ref("w".to_string())),
                Box::new(FlowExpr::Ref("w".to_string())),
                Box::new(FlowExpr::Float(1.0)),
            )),
        });

        graph.validate(None).unwrap();
        assert!(graph.steps.is_empty());
        assert!(graph.nodes.iter().any(|n| n.name == "w"));
        assert!(graph.nodes.iter().any(|n| n.name == "_s_w_eval"));
        assert!(graph.nodes.iter().any(|n| n.name == "_s_w_gx"));
        assert!(graph.nodes.iter().any(|n| n.name == "_s_w_gy"));
        assert!(graph.nodes.iter().any(|n| n.name == "_s_w_sc"));
    }

    #[test]
    fn test_step_expansion_effect_refract() {
        let mut graph = FlowGraph::new("test_refract");
        graph.inputs.push(FlowInput {
            name: "uv".to_string(),
            source: FlowInputSource::Builtin(BuiltinVar::Uv),
            ty: Some(FlowType::Vec2),
        });

        // First add a pattern-worley step
        let mut wp = HashMap::new();
        wp.insert("scale".to_string(), StepParam::Expr(FlowExpr::Float(18.0)));
        graph.steps.push(FlowStep {
            name: "d1".to_string(),
            step_type: StepType::PatternWorley,
            params: wp,
        });

        // Then add effect-refract referencing it
        let mut rp = HashMap::new();
        rp.insert(
            "sources".to_string(),
            StepParam::IdentList(vec!["d1".to_string()]),
        );
        rp.insert(
            "strength".to_string(),
            StepParam::Expr(FlowExpr::Float(0.15)),
        );
        graph.steps.push(FlowStep {
            name: "r".to_string(),
            step_type: StepType::EffectRefract,
            params: rp,
        });

        graph.outputs.push(FlowOutput {
            name: "color".to_string(),
            target: FlowOutputTarget::Color,
            expr: Some(FlowExpr::Vec4(
                Box::new(FlowExpr::Swizzle(
                    Box::new(FlowExpr::Ref("r".to_string())),
                    "x".to_string(),
                )),
                Box::new(FlowExpr::Swizzle(
                    Box::new(FlowExpr::Ref("r".to_string())),
                    "y".to_string(),
                )),
                Box::new(FlowExpr::Float(0.0)),
                Box::new(FlowExpr::Float(1.0)),
            )),
        });

        graph.validate(None).unwrap();
        assert!(graph.nodes.iter().any(|n| n.name == "r"));
        assert!(graph.nodes.iter().any(|n| n.name == "_s_r_ox"));
        assert!(graph.nodes.iter().any(|n| n.name == "_s_r_oy"));
    }

    #[test]
    fn test_step_expansion_effect_frost() {
        let mut graph = FlowGraph::new("test_frost");
        graph.inputs.push(FlowInput {
            name: "uv".to_string(),
            source: FlowInputSource::Builtin(BuiltinVar::Uv),
            ty: Some(FlowType::Vec2),
        });

        let mut params = HashMap::new();
        params.insert(
            "strength".to_string(),
            StepParam::Expr(FlowExpr::Float(0.003)),
        );
        params.insert("scale".to_string(), StepParam::Expr(FlowExpr::Float(30.0)));

        graph.steps.push(FlowStep {
            name: "f".to_string(),
            step_type: StepType::EffectFrost,
            params,
        });

        graph.outputs.push(FlowOutput {
            name: "color".to_string(),
            target: FlowOutputTarget::Color,
            expr: Some(FlowExpr::Vec4(
                Box::new(FlowExpr::Swizzle(
                    Box::new(FlowExpr::Ref("f".to_string())),
                    "x".to_string(),
                )),
                Box::new(FlowExpr::Swizzle(
                    Box::new(FlowExpr::Ref("f".to_string())),
                    "y".to_string(),
                )),
                Box::new(FlowExpr::Float(0.0)),
                Box::new(FlowExpr::Float(1.0)),
            )),
        });

        graph.validate(None).unwrap();
        assert!(graph.steps.is_empty());
        assert!(graph.nodes.iter().any(|n| n.name == "f"));
        assert!(graph.nodes.iter().any(|n| n.name == "_s_f_nx"));
        assert!(graph.nodes.iter().any(|n| n.name == "_s_f_ny"));
    }

    #[test]
    fn test_step_expansion_effect_specular() {
        let mut graph = FlowGraph::new("test_spec");
        graph.inputs.push(FlowInput {
            name: "uv".to_string(),
            source: FlowInputSource::Builtin(BuiltinVar::Uv),
            ty: Some(FlowType::Vec2),
        });

        let mut params = HashMap::new();
        params.insert("scale".to_string(), StepParam::Expr(FlowExpr::Float(20.0)));
        params.insert("mask".to_string(), StepParam::Expr(FlowExpr::Float(1.0)));

        graph.steps.push(FlowStep {
            name: "s".to_string(),
            step_type: StepType::EffectSpecular,
            params,
        });

        graph.outputs.push(FlowOutput {
            name: "color".to_string(),
            target: FlowOutputTarget::Color,
            expr: Some(FlowExpr::Vec4(
                Box::new(FlowExpr::Ref("s".to_string())),
                Box::new(FlowExpr::Ref("s".to_string())),
                Box::new(FlowExpr::Ref("s".to_string())),
                Box::new(FlowExpr::Float(1.0)),
            )),
        });

        graph.validate(None).unwrap();
        assert!(graph.steps.is_empty());
        assert!(graph.nodes.iter().any(|n| n.name == "s"));
        assert!(graph.nodes.iter().any(|n| n.name == "_s_s_gs"));
        assert!(graph.nodes.iter().any(|n| n.name == "_s_s_ha"));
        assert!(graph.nodes.iter().any(|n| n.name == "_s_s_d"));
    }

    #[test]
    fn test_step_expansion_effect_fog() {
        let mut graph = FlowGraph::new("test_fog");
        graph.inputs.push(FlowInput {
            name: "uv".to_string(),
            source: FlowInputSource::Builtin(BuiltinVar::Uv),
            ty: Some(FlowType::Vec2),
        });

        // Source scene as a vec4 node
        graph.nodes.push(FlowNode {
            name: "sc".to_string(),
            expr: FlowExpr::Vec4(
                Box::new(FlowExpr::Float(0.5)),
                Box::new(FlowExpr::Float(0.5)),
                Box::new(FlowExpr::Float(0.5)),
                Box::new(FlowExpr::Float(1.0)),
            ),
            inferred_type: None,
        });

        let mut params = HashMap::new();
        params.insert(
            "source".to_string(),
            StepParam::Expr(FlowExpr::Ref("sc".to_string())),
        );
        params.insert(
            "fog_density".to_string(),
            StepParam::Expr(FlowExpr::Float(0.1)),
        );
        params.insert(
            "clear_mask".to_string(),
            StepParam::Expr(FlowExpr::Float(0.5)),
        );

        graph.steps.push(FlowStep {
            name: "fg".to_string(),
            step_type: StepType::EffectFog,
            params,
        });

        graph.outputs.push(FlowOutput {
            name: "color".to_string(),
            target: FlowOutputTarget::Color,
            expr: Some(FlowExpr::Ref("fg".to_string())),
        });

        graph.validate(None).unwrap();
        assert!(graph.steps.is_empty());
        assert!(graph.nodes.iter().any(|n| n.name == "fg"));
        assert!(graph.nodes.iter().any(|n| n.name == "_s_fg_tint"));
        assert!(graph.nodes.iter().any(|n| n.name == "_s_fg_r"));
        assert!(graph.nodes.iter().any(|n| n.name == "_s_fg_a"));
    }

    #[test]
    fn test_step_type_from_str_new_types() {
        assert_eq!(
            StepType::parse("pattern-worley"),
            Some(StepType::PatternWorley)
        );
        assert_eq!(
            StepType::parse("effect-refract"),
            Some(StepType::EffectRefract)
        );
        assert_eq!(StepType::parse("effect-frost"), Some(StepType::EffectFrost));
        assert_eq!(
            StepType::parse("effect-specular"),
            Some(StepType::EffectSpecular)
        );
        assert_eq!(StepType::parse("effect-fog"), Some(StepType::EffectFog));
    }
}
