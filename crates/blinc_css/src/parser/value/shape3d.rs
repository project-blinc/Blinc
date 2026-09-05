//! 3D SDF shape and boolean-operation keywords.
//!
//! These lower to floats because the shader selects a shape and an
//! operation by numeric discriminant.

/// Check if a shape-3d value is valid
pub(crate) fn is_valid_shape_3d(value: &str) -> bool {
    matches!(
        value.trim().to_lowercase().as_str(),
        "none" | "box" | "sphere" | "cylinder" | "torus" | "capsule" | "group"
    )
}

/// Convert a shape-3d string to float encoding for the GPU
/// (0=none, 1=box, 2=sphere, 3=cylinder, 4=torus, 5=capsule, 6=group)
pub fn shape_3d_to_float(value: &str) -> f32 {
    match value.trim().to_lowercase().as_str() {
        "box" => 1.0,
        "sphere" => 2.0,
        "cylinder" => 3.0,
        "torus" => 4.0,
        "capsule" => 5.0,
        "group" => 6.0,
        _ => 0.0,
    }
}

pub(crate) fn is_valid_op_3d(value: &str) -> bool {
    matches!(
        value.trim().to_lowercase().as_str(),
        "union"
            | "subtract"
            | "intersect"
            | "smooth-union"
            | "smooth-subtract"
            | "smooth-intersect"
    )
}

/// Convert a 3d-op string to float encoding for the GPU
/// (0=union, 1=subtract, 2=intersect, 3=smooth-union, 4=smooth-subtract, 5=smooth-intersect)
pub fn op_3d_to_float(value: &str) -> f32 {
    match value.trim().to_lowercase().as_str() {
        "union" => 0.0,
        "subtract" => 1.0,
        "intersect" => 2.0,
        "smooth-union" => 3.0,
        "smooth-subtract" => 4.0,
        "smooth-intersect" => 5.0,
        _ => 0.0,
    }
}
