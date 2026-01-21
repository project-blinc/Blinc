//! SDF primitives

use blinc_core::Vec3;

/// SDF primitive shapes
#[derive(Clone, Debug)]
pub enum SdfPrimitive {
    /// Sphere with radius
    Sphere { radius: f32 },

    /// Box with half extents
    Box { half_extents: Vec3 },

    /// Torus with major and minor radii
    Torus {
        major_radius: f32,
        minor_radius: f32,
    },

    /// Cylinder with height and radius
    Cylinder { height: f32, radius: f32 },

    /// Infinite plane with normal and offset
    Plane { normal: Vec3, offset: f32 },

    /// Capsule (line segment with radius)
    Capsule {
        start: Vec3,
        end: Vec3,
        radius: f32,
    },

    /// Cone with height and base radius
    Cone { height: f32, radius: f32 },

    /// Rounded box
    RoundedBox {
        half_extents: Vec3,
        radius: f32,
    },

    /// Ellipsoid with radii
    Ellipsoid { radii: Vec3 },

    /// Triangular prism
    TriPrism { width: f32, height: f32 },

    /// Hexagonal prism
    HexPrism { width: f32, height: f32 },

    /// Octahedron
    Octahedron { size: f32 },

    /// Pyramid with base size and height
    Pyramid { base: f32, height: f32 },
}

impl SdfPrimitive {
    /// Get the WGSL function name for this primitive
    pub fn wgsl_function(&self) -> &'static str {
        match self {
            SdfPrimitive::Sphere { .. } => "sdf_sphere",
            SdfPrimitive::Box { .. } => "sdf_box",
            SdfPrimitive::Torus { .. } => "sdf_torus",
            SdfPrimitive::Cylinder { .. } => "sdf_cylinder",
            SdfPrimitive::Plane { .. } => "sdf_plane",
            SdfPrimitive::Capsule { .. } => "sdf_capsule",
            SdfPrimitive::Cone { .. } => "sdf_cone",
            SdfPrimitive::RoundedBox { .. } => "sdf_rounded_box",
            SdfPrimitive::Ellipsoid { .. } => "sdf_ellipsoid",
            SdfPrimitive::TriPrism { .. } => "sdf_tri_prism",
            SdfPrimitive::HexPrism { .. } => "sdf_hex_prism",
            SdfPrimitive::Octahedron { .. } => "sdf_octahedron",
            SdfPrimitive::Pyramid { .. } => "sdf_pyramid",
        }
    }

    /// Generate WGSL code for the SDF primitive evaluation
    pub fn to_wgsl(&self, point_var: &str) -> String {
        match self {
            SdfPrimitive::Sphere { radius } => {
                format!("sdf_sphere({}, {})", point_var, radius)
            }
            SdfPrimitive::Box { half_extents } => {
                format!(
                    "sdf_box({}, vec3<f32>({}, {}, {}))",
                    point_var, half_extents.x, half_extents.y, half_extents.z
                )
            }
            SdfPrimitive::Torus {
                major_radius,
                minor_radius,
            } => {
                format!(
                    "sdf_torus({}, vec2<f32>({}, {}))",
                    point_var, major_radius, minor_radius
                )
            }
            SdfPrimitive::Cylinder { height, radius } => {
                format!("sdf_cylinder({}, {}, {})", point_var, height, radius)
            }
            SdfPrimitive::Plane { normal, offset } => {
                format!(
                    "sdf_plane({}, vec3<f32>({}, {}, {}), {})",
                    point_var, normal.x, normal.y, normal.z, offset
                )
            }
            SdfPrimitive::Capsule { start, end, radius } => {
                format!(
                    "sdf_capsule({}, vec3<f32>({}, {}, {}), vec3<f32>({}, {}, {}), {})",
                    point_var, start.x, start.y, start.z, end.x, end.y, end.z, radius
                )
            }
            SdfPrimitive::Cone { height, radius } => {
                // For cone, we need to calculate the angle
                let angle = (radius / height).atan();
                format!(
                    "sdf_cone({}, vec2<f32>({}, {}), {})",
                    point_var,
                    angle.sin(),
                    angle.cos(),
                    height
                )
            }
            SdfPrimitive::RoundedBox { half_extents, radius } => {
                format!(
                    "sdf_rounded_box({}, vec3<f32>({}, {}, {}), {})",
                    point_var, half_extents.x, half_extents.y, half_extents.z, radius
                )
            }
            SdfPrimitive::Ellipsoid { radii } => {
                format!(
                    "sdf_ellipsoid({}, vec3<f32>({}, {}, {}))",
                    point_var, radii.x, radii.y, radii.z
                )
            }
            SdfPrimitive::TriPrism { width, height } => {
                format!(
                    "sdf_tri_prism({}, vec2<f32>({}, {}))",
                    point_var, width, height
                )
            }
            SdfPrimitive::HexPrism { width, height } => {
                format!(
                    "sdf_hex_prism({}, vec2<f32>({}, {}))",
                    point_var, width, height
                )
            }
            SdfPrimitive::Octahedron { size } => {
                format!("sdf_octahedron({}, {})", point_var, size)
            }
            SdfPrimitive::Pyramid { base, height } => {
                format!("sdf_pyramid({}, {}, {})", point_var, base, height)
            }
        }
    }

    /// Get WGSL function definitions for all primitives
    pub fn all_wgsl_definitions() -> &'static str {
        r#"
// SDF Primitive Functions

fn sdf_sphere(p: vec3<f32>, r: f32) -> f32 {
    return length(p) - r;
}

fn sdf_box(p: vec3<f32>, b: vec3<f32>) -> f32 {
    let q = abs(p) - b;
    return length(max(q, vec3<f32>(0.0))) + min(max(q.x, max(q.y, q.z)), 0.0);
}

fn sdf_torus(p: vec3<f32>, t: vec2<f32>) -> f32 {
    let q = vec2<f32>(length(p.xz) - t.x, p.y);
    return length(q) - t.y;
}

fn sdf_cylinder(p: vec3<f32>, h: f32, r: f32) -> f32 {
    let d = abs(vec2<f32>(length(p.xz), p.y)) - vec2<f32>(r, h);
    return min(max(d.x, d.y), 0.0) + length(max(d, vec2<f32>(0.0)));
}

fn sdf_plane(p: vec3<f32>, n: vec3<f32>, h: f32) -> f32 {
    return dot(p, n) + h;
}

fn sdf_capsule(p: vec3<f32>, a: vec3<f32>, b: vec3<f32>, r: f32) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return length(pa - ba * h) - r;
}

fn sdf_cone(p: vec3<f32>, c: vec2<f32>, h: f32) -> f32 {
    let q = h * vec2<f32>(c.x / c.y, -1.0);
    let w = vec2<f32>(length(p.xz), p.y);
    let a = w - q * clamp(dot(w, q) / dot(q, q), 0.0, 1.0);
    let b = w - q * vec2<f32>(clamp(w.x / q.x, 0.0, 1.0), 1.0);
    let k = sign(q.y);
    let d = min(dot(a, a), dot(b, b));
    let s = max(k * (w.x * q.y - w.y * q.x), k * (w.y - q.y));
    return sqrt(d) * sign(s);
}

fn sdf_rounded_box(p: vec3<f32>, b: vec3<f32>, r: f32) -> f32 {
    let q = abs(p) - b + r;
    return length(max(q, vec3<f32>(0.0))) + min(max(q.x, max(q.y, q.z)), 0.0) - r;
}

fn sdf_ellipsoid(p: vec3<f32>, r: vec3<f32>) -> f32 {
    let k0 = length(p / r);
    let k1 = length(p / (r * r));
    return k0 * (k0 - 1.0) / k1;
}

fn sdf_tri_prism(p: vec3<f32>, h: vec2<f32>) -> f32 {
    let q = abs(p);
    return max(q.z - h.y, max(q.x * 0.866025 + p.y * 0.5, -p.y) - h.x * 0.5);
}

fn sdf_hex_prism(p: vec3<f32>, h: vec2<f32>) -> f32 {
    let k = vec3<f32>(-0.8660254, 0.5, 0.57735);
    var q = abs(p);
    q = vec3<f32>(q.x - 2.0 * min(dot(k.xy, q.xy), 0.0) * k.x,
                  q.y - 2.0 * min(dot(k.xy, q.xy), 0.0) * k.y,
                  q.z);
    let d = vec2<f32>(
        length(q.xy - vec2<f32>(clamp(q.x, -k.z * h.x, k.z * h.x), h.x)) * sign(q.y - h.x),
        q.z - h.y
    );
    return min(max(d.x, d.y), 0.0) + length(max(d, vec2<f32>(0.0)));
}

fn sdf_octahedron(p: vec3<f32>, s: f32) -> f32 {
    let q = abs(p);
    return (q.x + q.y + q.z - s) * 0.57735027;
}

fn sdf_pyramid(p: vec3<f32>, base: f32, h: f32) -> f32 {
    let m2 = h * h + 0.25;
    var q = vec3<f32>(abs(p.x), p.y, abs(p.z));
    if (q.z > q.x) {
        q = vec3<f32>(q.z, q.y, q.x);
    }
    q = vec3<f32>(q.x - 0.5 * base, q.y, q.z - 0.5 * base);

    let a = vec3<f32>(q.z, h * q.y - 0.5 * q.x, h * q.x + 0.5 * q.y);
    let b = vec3<f32>(q.x, q.y, q.z);

    let s = max(-a.x, 0.0);
    let t = clamp((a.y - 0.5 * a.x) / (m2 + 0.25), 0.0, 1.0);

    let d1 = a.y - h;
    let d2 = a.y - h * t;

    if (min(d1, d2) > 0.0) {
        return sqrt(min(d1 * d1 + s * s, d2 * d2 + (a.x + s) * (a.x + s)));
    }
    return -min(d1, d2);
}
"#
    }
}
