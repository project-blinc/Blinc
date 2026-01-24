//! CPU evaluation for SDF primitives and operations
//!
//! This module provides CPU-based evaluation of signed distance fields
//! for raymarching and other CPU-side operations.

use super::{SdfNode, SdfNodeContent, SdfOp, SdfPrimitive, SdfScene, SdfTransform};
use blinc_core::{Color, Vec3};

/// Signed distance result with material information
#[derive(Clone, Debug)]
pub struct SdfHit {
    /// Signed distance to the surface
    pub distance: f32,
    /// Material color at this point
    pub color: Color,
}

impl SdfHit {
    pub fn new(distance: f32, color: Color) -> Self {
        Self { distance, color }
    }
}

// =============================================================================
// Primitive Distance Functions
// =============================================================================

impl SdfPrimitive {
    /// Evaluate the signed distance at a point
    pub fn evaluate(&self, p: Vec3) -> f32 {
        match self {
            SdfPrimitive::Sphere { radius } => sdf_sphere(p, *radius),
            SdfPrimitive::Box { half_extents } => sdf_box(p, *half_extents),
            SdfPrimitive::Torus {
                major_radius,
                minor_radius,
            } => sdf_torus(p, *major_radius, *minor_radius),
            SdfPrimitive::Cylinder { height, radius } => sdf_cylinder(p, *height, *radius),
            SdfPrimitive::Plane { normal, offset } => sdf_plane(p, *normal, *offset),
            SdfPrimitive::Capsule { start, end, radius } => sdf_capsule(p, *start, *end, *radius),
            SdfPrimitive::Cone { height, radius } => sdf_cone(p, *height, *radius),
            SdfPrimitive::RoundedBox { half_extents, radius } => {
                sdf_rounded_box(p, *half_extents, *radius)
            }
            SdfPrimitive::Ellipsoid { radii } => sdf_ellipsoid(p, *radii),
            SdfPrimitive::TriPrism { width, height } => sdf_tri_prism(p, *width, *height),
            SdfPrimitive::HexPrism { width, height } => sdf_hex_prism(p, *width, *height),
            SdfPrimitive::Octahedron { size } => sdf_octahedron(p, *size),
            SdfPrimitive::Pyramid { base, height } => sdf_pyramid(p, *base, *height),
        }
    }
}

/// Sphere distance function
fn sdf_sphere(p: Vec3, r: f32) -> f32 {
    p.length() - r
}

/// Box distance function
fn sdf_box(p: Vec3, b: Vec3) -> f32 {
    let q = Vec3::new(p.x.abs() - b.x, p.y.abs() - b.y, p.z.abs() - b.z);
    let q_max = Vec3::new(q.x.max(0.0), q.y.max(0.0), q.z.max(0.0));
    q_max.length() + q.x.max(q.y.max(q.z)).min(0.0)
}

/// Torus distance function
fn sdf_torus(p: Vec3, major_r: f32, minor_r: f32) -> f32 {
    let q_x = (p.x * p.x + p.z * p.z).sqrt() - major_r;
    let q = Vec3::new(q_x, p.y, 0.0);
    (q.x * q.x + q.y * q.y).sqrt() - minor_r
}

/// Cylinder distance function (centered, height along Y axis)
fn sdf_cylinder(p: Vec3, h: f32, r: f32) -> f32 {
    let d_x = (p.x * p.x + p.z * p.z).sqrt() - r;
    let d_y = p.y.abs() - h;
    let d = Vec3::new(d_x.abs(), d_y.abs(), 0.0);
    d_x.max(d_y).min(0.0) + Vec3::new(d_x.max(0.0), d_y.max(0.0), 0.0).length()
}

/// Plane distance function
fn sdf_plane(p: Vec3, n: Vec3, h: f32) -> f32 {
    p.x * n.x + p.y * n.y + p.z * n.z + h
}

/// Capsule distance function
fn sdf_capsule(p: Vec3, a: Vec3, b: Vec3, r: f32) -> f32 {
    let pa = Vec3::new(p.x - a.x, p.y - a.y, p.z - a.z);
    let ba = Vec3::new(b.x - a.x, b.y - a.y, b.z - a.z);
    let h = (pa.x * ba.x + pa.y * ba.y + pa.z * ba.z) / (ba.x * ba.x + ba.y * ba.y + ba.z * ba.z);
    let h = h.clamp(0.0, 1.0);
    let closest = Vec3::new(pa.x - ba.x * h, pa.y - ba.y * h, pa.z - ba.z * h);
    closest.length() - r
}

/// Cone distance function
fn sdf_cone(p: Vec3, height: f32, radius: f32) -> f32 {
    let angle = (radius / height).atan();
    let c = Vec3::new(angle.sin(), angle.cos(), 0.0);

    let q = Vec3::new(height * c.x / c.y, -height, 0.0);
    let w = Vec3::new((p.x * p.x + p.z * p.z).sqrt(), p.y, 0.0);

    let dot_wq = w.x * q.x + w.y * q.y;
    let dot_qq = q.x * q.x + q.y * q.y;
    let t = (dot_wq / dot_qq).clamp(0.0, 1.0);

    let a = Vec3::new(w.x - q.x * t, w.y - q.y * t, 0.0);

    let t2 = (w.x / q.x).clamp(0.0, 1.0);
    let b = Vec3::new(w.x - q.x * t2, w.y - q.y, 0.0);

    let k = q.y.signum();
    let d = (a.x * a.x + a.y * a.y).min(b.x * b.x + b.y * b.y);
    let s = (k * (w.x * q.y - w.y * q.x)).max(k * (w.y - q.y));
    d.sqrt() * s.signum()
}

/// Rounded box distance function
fn sdf_rounded_box(p: Vec3, b: Vec3, r: f32) -> f32 {
    let q = Vec3::new(p.x.abs() - b.x + r, p.y.abs() - b.y + r, p.z.abs() - b.z + r);
    let q_max = Vec3::new(q.x.max(0.0), q.y.max(0.0), q.z.max(0.0));
    q_max.length() + q.x.max(q.y.max(q.z)).min(0.0) - r
}

/// Ellipsoid distance function (approximate)
fn sdf_ellipsoid(p: Vec3, r: Vec3) -> f32 {
    let p_div_r = Vec3::new(p.x / r.x, p.y / r.y, p.z / r.z);
    let p_div_r2 = Vec3::new(
        p.x / (r.x * r.x),
        p.y / (r.y * r.y),
        p.z / (r.z * r.z),
    );
    let k0 = p_div_r.length();
    let k1 = p_div_r2.length();
    if k1 > 0.0 {
        k0 * (k0 - 1.0) / k1
    } else {
        0.0
    }
}

/// Triangular prism distance function
fn sdf_tri_prism(p: Vec3, width: f32, height: f32) -> f32 {
    let q = Vec3::new(p.x.abs(), p.y.abs(), p.z.abs());
    (q.z - height).max((q.x * 0.866025 + p.y * 0.5).max(-p.y) - width * 0.5)
}

/// Hexagonal prism distance function
fn sdf_hex_prism(p: Vec3, width: f32, height: f32) -> f32 {
    let k = Vec3::new(-0.8660254, 0.5, 0.57735);
    let q = Vec3::new(p.x.abs(), p.y.abs(), p.z.abs());

    let dot_kxy_qxy = k.x * q.x + k.y * q.y;
    let factor = 2.0 * dot_kxy_qxy.min(0.0);
    let q = Vec3::new(q.x - factor * k.x, q.y - factor * k.y, q.z);

    let clamped = q.x.clamp(-k.z * width, k.z * width);
    let diff = Vec3::new(q.x - clamped, q.y - width, 0.0);
    let d_x = diff.length() * (q.y - width).signum();
    let d_y = q.z - height;

    d_x.max(d_y).min(0.0) + Vec3::new(d_x.max(0.0), d_y.max(0.0), 0.0).length()
}

/// Octahedron distance function
fn sdf_octahedron(p: Vec3, s: f32) -> f32 {
    let q = Vec3::new(p.x.abs(), p.y.abs(), p.z.abs());
    (q.x + q.y + q.z - s) * 0.57735027
}

/// Pyramid distance function
fn sdf_pyramid(p: Vec3, base: f32, h: f32) -> f32 {
    let m2 = h * h + 0.25;
    let mut q = Vec3::new(p.x.abs(), p.y, p.z.abs());

    if q.z > q.x {
        q = Vec3::new(q.z, q.y, q.x);
    }
    q = Vec3::new(q.x - 0.5 * base, q.y, q.z - 0.5 * base);

    let a_y = h * q.y - 0.5 * q.x;
    let a_x = h * q.x + 0.5 * q.y;

    let s = (-q.z).max(0.0);
    let t = ((a_y - 0.5 * q.z) / (m2 + 0.25)).clamp(0.0, 1.0);

    let d1 = a_y - h;
    let d2 = a_y - h * t;

    if d1.min(d2) > 0.0 {
        let v1 = d1 * d1 + s * s;
        let v2 = d2 * d2 + (q.z + s) * (q.z + s);
        v1.min(v2).sqrt()
    } else {
        -d1.min(d2)
    }
}

// =============================================================================
// Boolean Operations
// =============================================================================

impl SdfOp {
    /// Apply the boolean operation to two distance values
    pub fn apply(&self, d1: f32, d2: f32) -> f32 {
        match self {
            SdfOp::Union => d1.min(d2),
            SdfOp::Subtract => (-d2).max(d1),
            SdfOp::Intersect => d1.max(d2),
            SdfOp::SmoothUnion { k } => smooth_union(d1, d2, *k),
            SdfOp::SmoothSubtract { k } => smooth_subtract(d1, d2, *k),
            SdfOp::SmoothIntersect { k } => smooth_intersect(d1, d2, *k),
        }
    }

    /// Apply the operation and blend colors based on distances
    pub fn apply_with_color(
        &self,
        d1: f32,
        c1: Color,
        d2: f32,
        c2: Color,
    ) -> (f32, Color) {
        match self {
            SdfOp::Union => {
                if d1 < d2 {
                    (d1, c1)
                } else {
                    (d2, c2)
                }
            }
            SdfOp::Subtract => {
                // Subtracting d2 from d1 - use d1's color
                ((-d2).max(d1), c1)
            }
            SdfOp::Intersect => {
                if d1 > d2 {
                    (d1, c1)
                } else {
                    (d2, c2)
                }
            }
            SdfOp::SmoothUnion { k } => {
                let h = (0.5 + 0.5 * (d2 - d1) / k).clamp(0.0, 1.0);
                let dist = lerp(d2, d1, h) - k * h * (1.0 - h);
                let color = lerp_color(c2, c1, h);
                (dist, color)
            }
            SdfOp::SmoothSubtract { k } => {
                let h = (0.5 - 0.5 * (d2 + d1) / k).clamp(0.0, 1.0);
                let dist = lerp(d2, -d1, h) + k * h * (1.0 - h);
                // Keep the primary shape's color
                (dist, c1)
            }
            SdfOp::SmoothIntersect { k } => {
                let h = (0.5 - 0.5 * (d2 - d1) / k).clamp(0.0, 1.0);
                let dist = lerp(d2, d1, h) + k * h * (1.0 - h);
                let color = lerp_color(c2, c1, h);
                (dist, color)
            }
        }
    }
}

fn smooth_union(d1: f32, d2: f32, k: f32) -> f32 {
    let h = (0.5 + 0.5 * (d2 - d1) / k).clamp(0.0, 1.0);
    lerp(d2, d1, h) - k * h * (1.0 - h)
}

fn smooth_subtract(d1: f32, d2: f32, k: f32) -> f32 {
    let h = (0.5 - 0.5 * (d2 + d1) / k).clamp(0.0, 1.0);
    lerp(d2, -d1, h) + k * h * (1.0 - h)
}

fn smooth_intersect(d1: f32, d2: f32, k: f32) -> f32 {
    let h = (0.5 - 0.5 * (d2 - d1) / k).clamp(0.0, 1.0);
    lerp(d2, d1, h) + k * h * (1.0 - h)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    Color::rgba(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}

// =============================================================================
// Transform Application
// =============================================================================

impl SdfTransform {
    /// Transform a point from world space to local SDF space
    pub fn apply_inverse(&self, p: Vec3) -> Vec3 {
        // Apply inverse transform: translate, then rotate (ZYX order), then scale
        let mut result = Vec3::new(
            p.x - self.position.x,
            p.y - self.position.y,
            p.z - self.position.z,
        );

        // Apply inverse rotations (reverse order: Z, Y, X)
        if self.rotation.z != 0.0 {
            result = rotate_z(result, -self.rotation.z);
        }
        if self.rotation.y != 0.0 {
            result = rotate_y(result, -self.rotation.y);
        }
        if self.rotation.x != 0.0 {
            result = rotate_x(result, -self.rotation.x);
        }

        // Apply inverse scale
        Vec3::new(
            result.x / self.scale.x,
            result.y / self.scale.y,
            result.z / self.scale.z,
        )
    }

    /// Get the uniform scale factor for distance correction
    pub fn scale_factor(&self) -> f32 {
        // For non-uniform scale, use the minimum component
        self.scale.x.min(self.scale.y.min(self.scale.z))
    }
}

fn rotate_x(p: Vec3, a: f32) -> Vec3 {
    let c = a.cos();
    let s = a.sin();
    Vec3::new(p.x, c * p.y - s * p.z, s * p.y + c * p.z)
}

fn rotate_y(p: Vec3, a: f32) -> Vec3 {
    let c = a.cos();
    let s = a.sin();
    Vec3::new(c * p.x + s * p.z, p.y, -s * p.x + c * p.z)
}

fn rotate_z(p: Vec3, a: f32) -> Vec3 {
    let c = a.cos();
    let s = a.sin();
    Vec3::new(c * p.x - s * p.y, s * p.x + c * p.y, p.z)
}

// =============================================================================
// Node and Scene Evaluation
// =============================================================================

impl SdfNode {
    /// Evaluate the signed distance at a point
    pub fn evaluate(&self, p: Vec3) -> f32 {
        // Transform point to local space
        let local_p = self.transform.apply_inverse(p);
        let scale = self.transform.scale_factor();

        let dist = match &self.content {
            SdfNodeContent::Primitive(prim) => prim.evaluate(local_p),
            SdfNodeContent::Operation { op, left, right } => {
                let d1 = left.evaluate(local_p);
                let d2 = right.evaluate(local_p);
                op.apply(d1, d2)
            }
        };

        // Correct distance for scale
        dist * scale
    }

    /// Evaluate and return distance with material color
    pub fn evaluate_with_color(&self, p: Vec3) -> SdfHit {
        // Transform point to local space
        let local_p = self.transform.apply_inverse(p);
        let scale = self.transform.scale_factor();

        let (dist, color) = match &self.content {
            SdfNodeContent::Primitive(prim) => (prim.evaluate(local_p), self.material.color),
            SdfNodeContent::Operation { op, left, right } => {
                let hit1 = left.evaluate_with_color(local_p);
                let hit2 = right.evaluate_with_color(local_p);
                op.apply_with_color(hit1.distance, hit1.color, hit2.distance, hit2.color)
            }
        };

        SdfHit::new(dist * scale, color)
    }
}

impl SdfScene {
    /// Evaluate the signed distance at a point
    pub fn evaluate(&self, p: Vec3) -> f32 {
        match &self.root {
            Some(node) => node.evaluate(p),
            None => f32::MAX,
        }
    }

    /// Evaluate and return distance with material color
    pub fn evaluate_with_color(&self, p: Vec3) -> SdfHit {
        match &self.root {
            Some(node) => node.evaluate_with_color(p),
            None => SdfHit::new(f32::MAX, Color::BLACK),
        }
    }

    /// Calculate surface normal at a point using gradient estimation
    pub fn normal(&self, p: Vec3, epsilon: f32) -> Vec3 {
        let ex = Vec3::new(epsilon, 0.0, 0.0);
        let ey = Vec3::new(0.0, epsilon, 0.0);
        let ez = Vec3::new(0.0, 0.0, epsilon);

        let nx = self.evaluate(Vec3::new(p.x + epsilon, p.y, p.z))
            - self.evaluate(Vec3::new(p.x - epsilon, p.y, p.z));
        let ny = self.evaluate(Vec3::new(p.x, p.y + epsilon, p.z))
            - self.evaluate(Vec3::new(p.x, p.y - epsilon, p.z));
        let nz = self.evaluate(Vec3::new(p.x, p.y, p.z + epsilon))
            - self.evaluate(Vec3::new(p.x, p.y, p.z - epsilon));

        let n = Vec3::new(nx, ny, nz);
        let len = n.length();
        if len > 0.0 {
            Vec3::new(n.x / len, n.y / len, n.z / len)
        } else {
            Vec3::new(0.0, 1.0, 0.0)
        }
    }

    /// Raymarch into the scene
    /// Returns (hit_distance, iterations) or None if no hit
    pub fn raymarch(
        &self,
        ray_origin: Vec3,
        ray_dir: Vec3,
        max_steps: u32,
        max_distance: f32,
        epsilon: f32,
    ) -> Option<(f32, u32)> {
        let mut t = 0.0;

        for i in 0..max_steps {
            let p = Vec3::new(
                ray_origin.x + ray_dir.x * t,
                ray_origin.y + ray_dir.y * t,
                ray_origin.z + ray_dir.z * t,
            );

            let d = self.evaluate(p);

            if d < epsilon {
                return Some((t, i));
            }

            if t > max_distance {
                break;
            }

            t += d;
        }

        None
    }

    /// Raymarch and return hit information with color
    pub fn raymarch_hit(
        &self,
        ray_origin: Vec3,
        ray_dir: Vec3,
        max_steps: u32,
        max_distance: f32,
        epsilon: f32,
    ) -> Option<RaymarchHit> {
        let mut t = 0.0;

        for _ in 0..max_steps {
            let p = Vec3::new(
                ray_origin.x + ray_dir.x * t,
                ray_origin.y + ray_dir.y * t,
                ray_origin.z + ray_dir.z * t,
            );

            let hit = self.evaluate_with_color(p);

            if hit.distance < epsilon {
                let normal = self.normal(p, epsilon * 10.0);
                return Some(RaymarchHit {
                    position: p,
                    normal,
                    distance: t,
                    color: hit.color,
                });
            }

            if t > max_distance {
                break;
            }

            t += hit.distance;
        }

        None
    }
}

/// Result of a raymarch hit
#[derive(Clone, Debug)]
pub struct RaymarchHit {
    /// Hit position in world space
    pub position: Vec3,
    /// Surface normal at hit point
    pub normal: Vec3,
    /// Distance along ray
    pub distance: f32,
    /// Material color at hit point
    pub color: Color,
}
