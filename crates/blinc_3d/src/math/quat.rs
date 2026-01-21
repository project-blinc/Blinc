//! Quaternion for 3D rotations

use super::extensions::Mat4Ext;
use blinc_core::{Mat4, Vec3};

/// Quaternion for representing 3D rotations
///
/// Quaternions avoid gimbal lock and interpolate smoothly.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct Quat {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Default for Quat {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Quat {
    /// Identity quaternion (no rotation)
    pub const IDENTITY: Quat = Quat {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        w: 1.0,
    };

    /// Create a new quaternion
    pub fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }

    /// Create from Euler angles (in radians)
    ///
    /// Order: XYZ (roll, pitch, yaw)
    pub fn from_euler(x: f32, y: f32, z: f32) -> Self {
        let (sx, cx) = (x * 0.5).sin_cos();
        let (sy, cy) = (y * 0.5).sin_cos();
        let (sz, cz) = (z * 0.5).sin_cos();

        Self {
            x: sx * cy * cz - cx * sy * sz,
            y: cx * sy * cz + sx * cy * sz,
            z: cx * cy * sz - sx * sy * cz,
            w: cx * cy * cz + sx * sy * sz,
        }
    }

    /// Create from axis-angle representation
    pub fn from_axis_angle(axis: Vec3, angle: f32) -> Self {
        let half_angle = angle * 0.5;
        let s = half_angle.sin();
        let len = (axis.x * axis.x + axis.y * axis.y + axis.z * axis.z).sqrt();

        if len < 1e-6 {
            return Self::IDENTITY;
        }

        let inv_len = 1.0 / len;
        Self {
            x: axis.x * inv_len * s,
            y: axis.y * inv_len * s,
            z: axis.z * inv_len * s,
            w: half_angle.cos(),
        }
    }

    /// Create rotation to look at target from position
    pub fn look_at(forward: Vec3, up: Vec3) -> Self {
        let f = Self::normalize_vec3(forward);
        let r = Self::normalize_vec3(Self::cross(up, f));
        let u = Self::cross(f, r);

        let trace = r.x + u.y + f.z;

        if trace > 0.0 {
            let s = 0.5 / (trace + 1.0).sqrt();
            Self {
                w: 0.25 / s,
                x: (u.z - f.y) * s,
                y: (f.x - r.z) * s,
                z: (r.y - u.x) * s,
            }
        } else if r.x > u.y && r.x > f.z {
            let s = 2.0 * (1.0 + r.x - u.y - f.z).sqrt();
            Self {
                w: (u.z - f.y) / s,
                x: 0.25 * s,
                y: (u.x + r.y) / s,
                z: (f.x + r.z) / s,
            }
        } else if u.y > f.z {
            let s = 2.0 * (1.0 + u.y - r.x - f.z).sqrt();
            Self {
                w: (f.x - r.z) / s,
                x: (u.x + r.y) / s,
                y: 0.25 * s,
                z: (f.y + u.z) / s,
            }
        } else {
            let s = 2.0 * (1.0 + f.z - r.x - u.y).sqrt();
            Self {
                w: (r.y - u.x) / s,
                x: (f.x + r.z) / s,
                y: (f.y + u.z) / s,
                z: 0.25 * s,
            }
        }
    }

    /// Normalize the quaternion
    pub fn normalize(&self) -> Self {
        let len = (self.x * self.x + self.y * self.y + self.z * self.z + self.w * self.w).sqrt();
        if len < 1e-6 {
            return Self::IDENTITY;
        }
        let inv_len = 1.0 / len;
        Self {
            x: self.x * inv_len,
            y: self.y * inv_len,
            z: self.z * inv_len,
            w: self.w * inv_len,
        }
    }

    /// Get the conjugate (inverse for unit quaternions)
    pub fn conjugate(&self) -> Self {
        Self {
            x: -self.x,
            y: -self.y,
            z: -self.z,
            w: self.w,
        }
    }

    /// Dot product of two quaternions
    pub fn dot(&self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z + self.w * other.w
    }

    /// Instance method for slerp
    pub fn slerp(&self, other: Self, t: f32) -> Self {
        Self::slerp_static(self, &other, t)
    }

    /// Multiply two quaternions
    pub fn mul(&self, other: &Self) -> Self {
        Self {
            x: self.w * other.x + self.x * other.w + self.y * other.z - self.z * other.y,
            y: self.w * other.y - self.x * other.z + self.y * other.w + self.z * other.x,
            z: self.w * other.z + self.x * other.y - self.y * other.x + self.z * other.w,
            w: self.w * other.w - self.x * other.x - self.y * other.y - self.z * other.z,
        }
    }

    /// Rotate a vector by this quaternion
    pub fn rotate_vec3(&self, v: Vec3) -> Vec3 {
        let qv = Self::new(v.x, v.y, v.z, 0.0);
        let result = self.mul(&qv).mul(&self.conjugate());
        Vec3::new(result.x, result.y, result.z)
    }

    /// Spherical linear interpolation (static version)
    pub fn slerp_static(a: &Quat, b: &Quat, t: f32) -> Quat {
        let mut cos_half_theta = a.x * b.x + a.y * b.y + a.z * b.z + a.w * b.w;

        // If negative dot, negate one quaternion to take shorter path
        let mut b = *b;
        if cos_half_theta < 0.0 {
            b = Self::new(-b.x, -b.y, -b.z, -b.w);
            cos_half_theta = -cos_half_theta;
        }

        // If quaternions are close, use linear interpolation
        if cos_half_theta > 0.9995 {
            return Self::new(
                a.x + t * (b.x - a.x),
                a.y + t * (b.y - a.y),
                a.z + t * (b.z - a.z),
                a.w + t * (b.w - a.w),
            )
            .normalize();
        }

        let half_theta = cos_half_theta.acos();
        let sin_half_theta = (1.0 - cos_half_theta * cos_half_theta).sqrt();

        let ratio_a = ((1.0 - t) * half_theta).sin() / sin_half_theta;
        let ratio_b = (t * half_theta).sin() / sin_half_theta;

        Self::new(
            a.x * ratio_a + b.x * ratio_b,
            a.y * ratio_a + b.y * ratio_b,
            a.z * ratio_a + b.z * ratio_b,
            a.w * ratio_a + b.w * ratio_b,
        )
    }

    /// Convert to a 4x4 rotation matrix
    pub fn to_mat4(&self) -> Mat4 {
        let x2 = self.x + self.x;
        let y2 = self.y + self.y;
        let z2 = self.z + self.z;

        let xx = self.x * x2;
        let xy = self.x * y2;
        let xz = self.x * z2;
        let yy = self.y * y2;
        let yz = self.y * z2;
        let zz = self.z * z2;
        let wx = self.w * x2;
        let wy = self.w * y2;
        let wz = self.w * z2;

        <Mat4 as Mat4Ext>::from_cols_array(&[
            1.0 - (yy + zz), xy + wz, xz - wy, 0.0,
            xy - wz, 1.0 - (xx + zz), yz + wx, 0.0,
            xz + wy, yz - wx, 1.0 - (xx + yy), 0.0,
            0.0, 0.0, 0.0, 1.0,
        ])
    }

    /// Convert to Euler angles (radians)
    pub fn to_euler(&self) -> (f32, f32, f32) {
        let sinr_cosp = 2.0 * (self.w * self.x + self.y * self.z);
        let cosr_cosp = 1.0 - 2.0 * (self.x * self.x + self.y * self.y);
        let roll = sinr_cosp.atan2(cosr_cosp);

        let sinp = 2.0 * (self.w * self.y - self.z * self.x);
        let pitch = if sinp.abs() >= 1.0 {
            std::f32::consts::FRAC_PI_2.copysign(sinp)
        } else {
            sinp.asin()
        };

        let siny_cosp = 2.0 * (self.w * self.z + self.x * self.y);
        let cosy_cosp = 1.0 - 2.0 * (self.y * self.y + self.z * self.z);
        let yaw = siny_cosp.atan2(cosy_cosp);

        (roll, pitch, yaw)
    }

    // Helper functions
    fn normalize_vec3(v: Vec3) -> Vec3 {
        let len = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
        if len < 1e-6 {
            Vec3::new(0.0, 0.0, 1.0)
        } else {
            Vec3::new(v.x / len, v.y / len, v.z / len)
        }
    }

    fn cross(a: Vec3, b: Vec3) -> Vec3 {
        Vec3::new(
            a.y * b.z - a.z * b.y,
            a.z * b.x - a.x * b.z,
            a.x * b.y - a.y * b.x,
        )
    }
}

impl std::ops::Mul for Quat {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        Quat::mul(&self, &rhs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    #[test]
    fn test_identity() {
        let q = Quat::IDENTITY;
        let v = Vec3::new(1.0, 2.0, 3.0);
        let rotated = q.rotate_vec3(v);

        assert!((rotated.x - v.x).abs() < 1e-5);
        assert!((rotated.y - v.y).abs() < 1e-5);
        assert!((rotated.z - v.z).abs() < 1e-5);
    }

    #[test]
    fn test_from_axis_angle() {
        // Rotate 90 degrees around Y axis
        let q = Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), PI / 2.0);
        let v = Vec3::new(1.0, 0.0, 0.0);
        let rotated = q.rotate_vec3(v);

        assert!((rotated.x - 0.0).abs() < 1e-5);
        assert!((rotated.y - 0.0).abs() < 1e-5);
        assert!((rotated.z - (-1.0)).abs() < 1e-5);
    }

    #[test]
    fn test_slerp() {
        // Test slerp with 90 degrees (avoids 180-degree floating-point edge case)
        let a = Quat::IDENTITY;
        let b = Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), PI / 2.0);

        let mid = Quat::slerp_static(&a, &b, 0.5);
        let v = Vec3::new(1.0, 0.0, 0.0);
        let rotated = mid.rotate_vec3(v);

        // Should be rotated 45 degrees: x ≈ 0.707, z ≈ -0.707
        let expected = (PI / 4.0).cos();  // ≈ 0.707
        assert!((rotated.x - expected).abs() < 1e-4, "x: {}, expected: {}", rotated.x, expected);
        assert!((rotated.z - (-expected)).abs() < 1e-4, "z: {}, expected: {}", rotated.z, -expected);
    }
}
