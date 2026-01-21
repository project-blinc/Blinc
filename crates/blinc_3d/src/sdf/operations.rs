//! SDF boolean operations

/// SDF boolean operations
#[derive(Clone, Debug)]
pub enum SdfOp {
    /// Union (minimum)
    Union,
    /// Subtraction (maximum of negated)
    Subtract,
    /// Intersection (maximum)
    Intersect,
    /// Smooth union with blending factor
    SmoothUnion { k: f32 },
    /// Smooth subtraction with blending factor
    SmoothSubtract { k: f32 },
    /// Smooth intersection with blending factor
    SmoothIntersect { k: f32 },
}

impl SdfOp {
    /// Get the WGSL function name for this operation
    pub fn wgsl_function(&self) -> &'static str {
        match self {
            SdfOp::Union => "op_union",
            SdfOp::Subtract => "op_subtract",
            SdfOp::Intersect => "op_intersect",
            SdfOp::SmoothUnion { .. } => "op_smooth_union",
            SdfOp::SmoothSubtract { .. } => "op_smooth_subtract",
            SdfOp::SmoothIntersect { .. } => "op_smooth_intersect",
        }
    }

    /// Generate WGSL code for this operation
    pub fn to_wgsl(&self, left_var: &str, right_var: &str) -> String {
        match self {
            SdfOp::Union => {
                format!("op_union({}, {})", left_var, right_var)
            }
            SdfOp::Subtract => {
                format!("op_subtract({}, {})", left_var, right_var)
            }
            SdfOp::Intersect => {
                format!("op_intersect({}, {})", left_var, right_var)
            }
            SdfOp::SmoothUnion { k } => {
                format!("op_smooth_union({}, {}, {})", left_var, right_var, k)
            }
            SdfOp::SmoothSubtract { k } => {
                format!("op_smooth_subtract({}, {}, {})", left_var, right_var, k)
            }
            SdfOp::SmoothIntersect { k } => {
                format!("op_smooth_intersect({}, {}, {})", left_var, right_var, k)
            }
        }
    }

    /// Get WGSL function definitions for all operations
    pub fn all_wgsl_definitions() -> &'static str {
        r#"
// SDF Boolean Operations

fn op_union(d1: f32, d2: f32) -> f32 {
    return min(d1, d2);
}

fn op_subtract(d1: f32, d2: f32) -> f32 {
    return max(-d1, d2);
}

fn op_intersect(d1: f32, d2: f32) -> f32 {
    return max(d1, d2);
}

fn op_smooth_union(d1: f32, d2: f32, k: f32) -> f32 {
    let h = clamp(0.5 + 0.5 * (d2 - d1) / k, 0.0, 1.0);
    return mix(d2, d1, h) - k * h * (1.0 - h);
}

fn op_smooth_subtract(d1: f32, d2: f32, k: f32) -> f32 {
    let h = clamp(0.5 - 0.5 * (d2 + d1) / k, 0.0, 1.0);
    return mix(d2, -d1, h) + k * h * (1.0 - h);
}

fn op_smooth_intersect(d1: f32, d2: f32, k: f32) -> f32 {
    let h = clamp(0.5 - 0.5 * (d2 - d1) / k, 0.0, 1.0);
    return mix(d2, d1, h) + k * h * (1.0 - h);
}
"#
    }
}

/// SDF domain operations (space transformations)
#[derive(Clone, Debug)]
pub enum SdfDomainOp {
    /// Translate the space
    Translate { offset: blinc_core::Vec3 },
    /// Rotate around X axis
    RotateX { angle: f32 },
    /// Rotate around Y axis
    RotateY { angle: f32 },
    /// Rotate around Z axis
    RotateZ { angle: f32 },
    /// Uniform scale
    Scale { factor: f32 },
    /// Infinite repetition
    Repeat { cell_size: blinc_core::Vec3 },
    /// Limited repetition
    RepeatLimited {
        cell_size: f32,
        limit: blinc_core::Vec3,
    },
    /// Mirror across a plane
    Mirror { axis: MirrorAxis },
    /// Twist around Y axis
    Twist { amount: f32 },
    /// Bend around Y axis
    Bend { amount: f32 },
}

/// Mirror axis
#[derive(Clone, Copy, Debug)]
pub enum MirrorAxis {
    X,
    Y,
    Z,
}

impl SdfDomainOp {
    /// Generate WGSL code for this domain operation
    pub fn to_wgsl(&self, point_var: &str) -> String {
        match self {
            SdfDomainOp::Translate { offset } => {
                format!(
                    "{} - vec3<f32>({}, {}, {})",
                    point_var, offset.x, offset.y, offset.z
                )
            }
            SdfDomainOp::RotateX { angle } => {
                format!("op_rotate_x({}, {})", point_var, angle)
            }
            SdfDomainOp::RotateY { angle } => {
                format!("op_rotate_y({}, {})", point_var, angle)
            }
            SdfDomainOp::RotateZ { angle } => {
                format!("op_rotate_z({}, {})", point_var, angle)
            }
            SdfDomainOp::Scale { factor } => {
                format!("{} / {}", point_var, factor)
            }
            SdfDomainOp::Repeat { cell_size } => {
                format!(
                    "op_repeat({}, vec3<f32>({}, {}, {}))",
                    point_var, cell_size.x, cell_size.y, cell_size.z
                )
            }
            SdfDomainOp::RepeatLimited { cell_size, limit } => {
                format!(
                    "op_repeat_limited({}, {}, vec3<f32>({}, {}, {}))",
                    point_var, cell_size, limit.x, limit.y, limit.z
                )
            }
            SdfDomainOp::Mirror { axis } => match axis {
                MirrorAxis::X => format!("vec3<f32>(abs({}.x), {}.y, {}.z)", point_var, point_var, point_var),
                MirrorAxis::Y => format!("vec3<f32>({}.x, abs({}.y), {}.z)", point_var, point_var, point_var),
                MirrorAxis::Z => format!("vec3<f32>({}.x, {}.y, abs({}.z))", point_var, point_var, point_var),
            },
            SdfDomainOp::Twist { amount } => {
                format!("op_twist({}, {})", point_var, amount)
            }
            SdfDomainOp::Bend { amount } => {
                format!("op_bend({}, {})", point_var, amount)
            }
        }
    }

    /// Get WGSL function definitions for domain operations
    pub fn all_wgsl_definitions() -> &'static str {
        r#"
// SDF Domain Operations

fn op_rotate_x(p: vec3<f32>, a: f32) -> vec3<f32> {
    let c = cos(a);
    let s = sin(a);
    return vec3<f32>(p.x, c * p.y - s * p.z, s * p.y + c * p.z);
}

fn op_rotate_y(p: vec3<f32>, a: f32) -> vec3<f32> {
    let c = cos(a);
    let s = sin(a);
    return vec3<f32>(c * p.x + s * p.z, p.y, -s * p.x + c * p.z);
}

fn op_rotate_z(p: vec3<f32>, a: f32) -> vec3<f32> {
    let c = cos(a);
    let s = sin(a);
    return vec3<f32>(c * p.x - s * p.y, s * p.x + c * p.y, p.z);
}

fn op_repeat(p: vec3<f32>, c: vec3<f32>) -> vec3<f32> {
    return p - c * round(p / c);
}

fn op_repeat_limited(p: vec3<f32>, c: f32, l: vec3<f32>) -> vec3<f32> {
    return p - c * clamp(round(p / c), -l, l);
}

fn op_twist(p: vec3<f32>, k: f32) -> vec3<f32> {
    let c = cos(k * p.y);
    let s = sin(k * p.y);
    let m = mat2x2<f32>(c, -s, s, c);
    let q = m * p.xz;
    return vec3<f32>(q.x, p.y, q.y);
}

fn op_bend(p: vec3<f32>, k: f32) -> vec3<f32> {
    let c = cos(k * p.x);
    let s = sin(k * p.x);
    let m = mat2x2<f32>(c, -s, s, c);
    let q = m * p.xy;
    return vec3<f32>(q.x, q.y, p.z);
}
"#
    }
}
