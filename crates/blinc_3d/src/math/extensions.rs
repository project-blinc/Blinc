//! Math extension methods for blinc_core types
//!
//! Provides additional methods needed for 3D graphics that aren't in blinc_core.

use blinc_core::{Mat4, Vec3};

/// Extension trait for Vec3
pub trait Vec3Ext {
    /// Create a Vec3 with all components set to the same value
    fn splat(v: f32) -> Vec3;
    /// Get the maximum component
    fn max_element(&self) -> f32;
    /// Get the minimum component
    fn min_element(&self) -> f32;
    /// Linear interpolation
    fn lerp(&self, other: Vec3, t: f32) -> Vec3;
    /// Component-wise absolute value
    fn abs(&self) -> Vec3;
    /// Component-wise maximum
    fn max(&self, other: Vec3) -> Vec3;
    /// Component-wise minimum
    fn min(&self, other: Vec3) -> Vec3;
}

impl Vec3Ext for Vec3 {
    fn splat(v: f32) -> Vec3 {
        Vec3::new(v, v, v)
    }

    fn max_element(&self) -> f32 {
        self.x.max(self.y).max(self.z)
    }

    fn min_element(&self) -> f32 {
        self.x.min(self.y).min(self.z)
    }

    fn lerp(&self, other: Vec3, t: f32) -> Vec3 {
        Vec3::new(
            self.x + (other.x - self.x) * t,
            self.y + (other.y - self.y) * t,
            self.z + (other.z - self.z) * t,
        )
    }

    fn abs(&self) -> Vec3 {
        Vec3::new(self.x.abs(), self.y.abs(), self.z.abs())
    }

    fn max(&self, other: Vec3) -> Vec3 {
        Vec3::new(
            self.x.max(other.x),
            self.y.max(other.y),
            self.z.max(other.z),
        )
    }

    fn min(&self, other: Vec3) -> Vec3 {
        Vec3::new(
            self.x.min(other.x),
            self.y.min(other.y),
            self.z.min(other.z),
        )
    }
}

/// Extension trait for Mat4
pub trait Mat4Ext {
    /// Create a translation matrix
    fn from_translation(v: Vec3) -> Mat4;
    /// Create a scale matrix
    fn from_scale(v: Vec3) -> Mat4;
    /// Create a rotation matrix around the X axis
    fn from_rotation_x(angle: f32) -> Mat4;
    /// Create a rotation matrix around the Y axis
    fn from_rotation_y(angle: f32) -> Mat4;
    /// Create a rotation matrix around the Z axis
    fn from_rotation_z(angle: f32) -> Mat4;
    /// Get column as array
    fn col(&self, idx: usize) -> [f32; 4];
    /// Get row as array
    fn row(&self, idx: usize) -> [f32; 4];
    /// Convert to column-major array
    fn to_cols_array(&self) -> [f32; 16];
    /// Convert to 2D array format
    fn to_cols_array_2d(&self) -> [[f32; 4]; 4];
    /// Create from column-major array
    fn from_cols_array(arr: &[f32; 16]) -> Mat4;
    /// Calculate the inverse (simple implementation)
    fn inverse(&self) -> Mat4;
    /// Transpose the matrix
    fn transpose(&self) -> Mat4;
    /// Transform a point (Vec3)
    fn transform_point(&self, p: Vec3) -> Vec3;
    /// Transform a direction (Vec3, ignores translation)
    fn transform_vector(&self, v: Vec3) -> Vec3;
    /// Create a perspective projection matrix (right-handed, depth 0 to 1)
    fn perspective_rh(fov_y: f32, aspect: f32, near: f32, far: f32) -> Mat4;
    /// Create a look-at view matrix (right-handed)
    fn look_at_rh(eye: Vec3, target: Vec3, up: Vec3) -> Mat4;
    /// Create an orthographic projection matrix (right-handed, depth 0 to 1)
    fn orthographic_rh(left: f32, right: f32, bottom: f32, top: f32, near: f32, far: f32) -> Mat4;
}

impl Mat4Ext for Mat4 {
    fn from_translation(v: Vec3) -> Mat4 {
        Mat4::translation(v.x, v.y, v.z)
    }

    fn from_scale(v: Vec3) -> Mat4 {
        Mat4::scale(v.x, v.y, v.z)
    }

    fn from_rotation_x(angle: f32) -> Mat4 {
        let c = angle.cos();
        let s = angle.sin();
        Mat4 {
            cols: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, c, s, 0.0],
                [0.0, -s, c, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    fn from_rotation_y(angle: f32) -> Mat4 {
        Mat4::rotation_y(angle)
    }

    fn from_rotation_z(angle: f32) -> Mat4 {
        let c = angle.cos();
        let s = angle.sin();
        Mat4 {
            cols: [
                [c, s, 0.0, 0.0],
                [-s, c, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    fn col(&self, idx: usize) -> [f32; 4] {
        self.cols[idx]
    }

    fn row(&self, idx: usize) -> [f32; 4] {
        [
            self.cols[0][idx],
            self.cols[1][idx],
            self.cols[2][idx],
            self.cols[3][idx],
        ]
    }

    fn to_cols_array(&self) -> [f32; 16] {
        [
            self.cols[0][0], self.cols[0][1], self.cols[0][2], self.cols[0][3],
            self.cols[1][0], self.cols[1][1], self.cols[1][2], self.cols[1][3],
            self.cols[2][0], self.cols[2][1], self.cols[2][2], self.cols[2][3],
            self.cols[3][0], self.cols[3][1], self.cols[3][2], self.cols[3][3],
        ]
    }

    fn to_cols_array_2d(&self) -> [[f32; 4]; 4] {
        self.cols
    }

    fn from_cols_array(arr: &[f32; 16]) -> Mat4 {
        Mat4 {
            cols: [
                [arr[0], arr[1], arr[2], arr[3]],
                [arr[4], arr[5], arr[6], arr[7]],
                [arr[8], arr[9], arr[10], arr[11]],
                [arr[12], arr[13], arr[14], arr[15]],
            ],
        }
    }

    fn inverse(&self) -> Mat4 {
        // Simple 4x4 matrix inverse using cofactor expansion
        let m = &self.cols;

        let a2323 = m[2][2] * m[3][3] - m[2][3] * m[3][2];
        let a1323 = m[2][1] * m[3][3] - m[2][3] * m[3][1];
        let a1223 = m[2][1] * m[3][2] - m[2][2] * m[3][1];
        let a0323 = m[2][0] * m[3][3] - m[2][3] * m[3][0];
        let a0223 = m[2][0] * m[3][2] - m[2][2] * m[3][0];
        let a0123 = m[2][0] * m[3][1] - m[2][1] * m[3][0];
        let a2313 = m[1][2] * m[3][3] - m[1][3] * m[3][2];
        let a1313 = m[1][1] * m[3][3] - m[1][3] * m[3][1];
        let a1213 = m[1][1] * m[3][2] - m[1][2] * m[3][1];
        let a2312 = m[1][2] * m[2][3] - m[1][3] * m[2][2];
        let a1312 = m[1][1] * m[2][3] - m[1][3] * m[2][1];
        let a1212 = m[1][1] * m[2][2] - m[1][2] * m[2][1];
        let a0313 = m[1][0] * m[3][3] - m[1][3] * m[3][0];
        let a0213 = m[1][0] * m[3][2] - m[1][2] * m[3][0];
        let a0312 = m[1][0] * m[2][3] - m[1][3] * m[2][0];
        let a0212 = m[1][0] * m[2][2] - m[1][2] * m[2][0];
        let a0113 = m[1][0] * m[3][1] - m[1][1] * m[3][0];
        let a0112 = m[1][0] * m[2][1] - m[1][1] * m[2][0];

        let det = m[0][0] * (m[1][1] * a2323 - m[1][2] * a1323 + m[1][3] * a1223)
            - m[0][1] * (m[1][0] * a2323 - m[1][2] * a0323 + m[1][3] * a0223)
            + m[0][2] * (m[1][0] * a1323 - m[1][1] * a0323 + m[1][3] * a0123)
            - m[0][3] * (m[1][0] * a1223 - m[1][1] * a0223 + m[1][2] * a0123);

        if det.abs() < 1e-10 {
            return Mat4::IDENTITY;
        }

        let inv_det = 1.0 / det;

        Mat4 {
            cols: [
                [
                    inv_det * (m[1][1] * a2323 - m[1][2] * a1323 + m[1][3] * a1223),
                    inv_det * -(m[0][1] * a2323 - m[0][2] * a1323 + m[0][3] * a1223),
                    inv_det * (m[0][1] * a2313 - m[0][2] * a1313 + m[0][3] * a1213),
                    inv_det * -(m[0][1] * a2312 - m[0][2] * a1312 + m[0][3] * a1212),
                ],
                [
                    inv_det * -(m[1][0] * a2323 - m[1][2] * a0323 + m[1][3] * a0223),
                    inv_det * (m[0][0] * a2323 - m[0][2] * a0323 + m[0][3] * a0223),
                    inv_det * -(m[0][0] * a2313 - m[0][2] * a0313 + m[0][3] * a0213),
                    inv_det * (m[0][0] * a2312 - m[0][2] * a0312 + m[0][3] * a0212),
                ],
                [
                    inv_det * (m[1][0] * a1323 - m[1][1] * a0323 + m[1][3] * a0123),
                    inv_det * -(m[0][0] * a1323 - m[0][1] * a0323 + m[0][3] * a0123),
                    inv_det * (m[0][0] * a1313 - m[0][1] * a0313 + m[0][3] * a0113),
                    inv_det * -(m[0][0] * a1312 - m[0][1] * a0312 + m[0][3] * a0112),
                ],
                [
                    inv_det * -(m[1][0] * a1223 - m[1][1] * a0223 + m[1][2] * a0123),
                    inv_det * (m[0][0] * a1223 - m[0][1] * a0223 + m[0][2] * a0123),
                    inv_det * -(m[0][0] * a1213 - m[0][1] * a0213 + m[0][2] * a0113),
                    inv_det * (m[0][0] * a1212 - m[0][1] * a0212 + m[0][2] * a0112),
                ],
            ],
        }
    }

    fn transpose(&self) -> Mat4 {
        Mat4 {
            cols: [
                [self.cols[0][0], self.cols[1][0], self.cols[2][0], self.cols[3][0]],
                [self.cols[0][1], self.cols[1][1], self.cols[2][1], self.cols[3][1]],
                [self.cols[0][2], self.cols[1][2], self.cols[2][2], self.cols[3][2]],
                [self.cols[0][3], self.cols[1][3], self.cols[2][3], self.cols[3][3]],
            ],
        }
    }

    fn transform_point(&self, p: Vec3) -> Vec3 {
        let x = self.cols[0][0] * p.x + self.cols[1][0] * p.y + self.cols[2][0] * p.z + self.cols[3][0];
        let y = self.cols[0][1] * p.x + self.cols[1][1] * p.y + self.cols[2][1] * p.z + self.cols[3][1];
        let z = self.cols[0][2] * p.x + self.cols[1][2] * p.y + self.cols[2][2] * p.z + self.cols[3][2];
        Vec3::new(x, y, z)
    }

    fn transform_vector(&self, v: Vec3) -> Vec3 {
        let x = self.cols[0][0] * v.x + self.cols[1][0] * v.y + self.cols[2][0] * v.z;
        let y = self.cols[0][1] * v.x + self.cols[1][1] * v.y + self.cols[2][1] * v.z;
        let z = self.cols[0][2] * v.x + self.cols[1][2] * v.y + self.cols[2][2] * v.z;
        Vec3::new(x, y, z)
    }

    fn perspective_rh(fov_y: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
        let h = 1.0 / (fov_y * 0.5).tan();
        let w = h / aspect;
        let r = far / (near - far);

        Mat4 {
            cols: [
                [w, 0.0, 0.0, 0.0],
                [0.0, h, 0.0, 0.0],
                [0.0, 0.0, r, -1.0],
                [0.0, 0.0, near * r, 0.0],
            ],
        }
    }

    fn look_at_rh(eye: Vec3, target: Vec3, up: Vec3) -> Mat4 {
        let f = Vec3::new(
            target.x - eye.x,
            target.y - eye.y,
            target.z - eye.z,
        );
        let f_len = (f.x * f.x + f.y * f.y + f.z * f.z).sqrt();
        let f = Vec3::new(f.x / f_len, f.y / f_len, f.z / f_len);

        // Cross product: up x f
        let s = Vec3::new(
            up.y * f.z - up.z * f.y,
            up.z * f.x - up.x * f.z,
            up.x * f.y - up.y * f.x,
        );
        let s_len = (s.x * s.x + s.y * s.y + s.z * s.z).sqrt();
        let s = if s_len > 0.0 {
            Vec3::new(s.x / s_len, s.y / s_len, s.z / s_len)
        } else {
            Vec3::new(1.0, 0.0, 0.0)
        };

        // Cross product: f x s
        let u = Vec3::new(
            f.y * s.z - f.z * s.y,
            f.z * s.x - f.x * s.z,
            f.x * s.y - f.y * s.x,
        );

        Mat4 {
            cols: [
                [s.x, u.x, -f.x, 0.0],
                [s.y, u.y, -f.y, 0.0],
                [s.z, u.z, -f.z, 0.0],
                [
                    -(s.x * eye.x + s.y * eye.y + s.z * eye.z),
                    -(u.x * eye.x + u.y * eye.y + u.z * eye.z),
                    f.x * eye.x + f.y * eye.y + f.z * eye.z,
                    1.0,
                ],
            ],
        }
    }

    fn orthographic_rh(left: f32, right: f32, bottom: f32, top: f32, near: f32, far: f32) -> Mat4 {
        let rml = right - left;
        let tmb = top - bottom;
        let fmn = far - near;

        Mat4 {
            cols: [
                [2.0 / rml, 0.0, 0.0, 0.0],
                [0.0, 2.0 / tmb, 0.0, 0.0],
                [0.0, 0.0, -1.0 / fmn, 0.0],
                [
                    -(right + left) / rml,
                    -(top + bottom) / tmb,
                    -near / fmn,
                    1.0,
                ],
            ],
        }
    }
}

/// Multiply operator for Mat4 (returns new matrix)
pub fn mat4_mul(a: &Mat4, b: &Mat4) -> Mat4 {
    a.mul(b)
}
