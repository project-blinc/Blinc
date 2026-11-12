//! The 3D signed distance functions, boolean operators and ray helpers the raymarching shader
//! uses.

russl::russl! {
    pub fn op_intersect(d1: f32, d2: f32) -> f32 { return max(d1, d2); }

    pub fn op_smooth_intersect(d1: f32, d2: f32, k: f32) -> f32 {
        let h = clamp(0.5 - 0.5 * (d2 - d1) / k, 0.0, 1.0);
        return mix(d2, d1, h) + k * h * (1.0 - h);
    }

    pub fn op_smooth_subtract(d1: f32, d2: f32, k: f32) -> f32 {
        let h = clamp(0.5 - 0.5 * (d2 + d1) / k, 0.0, 1.0);
        return mix(d1, -d2, h) + k * h * (1.0 - h);
    }

    pub fn op_smooth_union(d1: f32, d2: f32, k: f32) -> f32 {
        let h = clamp(0.5 + 0.5 * (d2 - d1) / k, 0.0, 1.0);
        return mix(d2, d1, h) - k * h * (1.0 - h);
    }

    pub fn op_subtract(d1: f32, d2: f32) -> f32 { return max(d1, -d2); }

    pub fn op_union(d1: f32, d2: f32) -> f32 { return min(d1, d2); }

    pub fn apply_boolean_op(d_accum: f32, d_new: f32, op_type: u32, blend: f32) -> f32 {
        let mut r: f32;
        match op_type {
            0u32 => { r = op_union(d_accum, d_new); }
            1u32 => { r = op_subtract(d_accum, d_new); }
            2u32 => { r = op_intersect(d_accum, d_new); }
            3u32 => { r = op_smooth_union(d_accum, d_new, max(blend, 0.001)); }
            4u32 => { r = op_smooth_subtract(d_accum, d_new, max(blend, 0.001)); }
            5u32 => { r = op_smooth_intersect(d_accum, d_new, max(blend, 0.001)); }
            _ => { r = op_union(d_accum, d_new); }
        }
        return r;
    }

    pub fn ray_aabb_intersect(ro: Vec3, rd: Vec3, half: Vec3) -> Vec2 {
        let inv_rd = Vec3::new(
            if abs(rd.x) < 1e-8 { 1e10 } else { 1.0 / rd.x },
            if abs(rd.y) < 1e-8 { 1e10 } else { 1.0 / rd.y },
            if abs(rd.z) < 1e-8 { 1e10 } else { 1.0 / rd.z },
        );
        let t1 = (-half - ro) * inv_rd;
        let t2 = (half - ro) * inv_rd;
        let tmin = min(t1, t2);
        let tmax = max(t1, t2);
        let t_enter = max(max(tmin.x, tmin.y), tmin.z);
        let t_exit = min(min(tmax.x, tmax.y), tmax.z);
        return Vec2::new(t_enter, t_exit);
    }

    pub fn rotate_x_inv(p: Vec3, s: f32, c: f32) -> Vec3 {
        return Vec3::new(p.x, c * p.y + s * p.z, -s * p.y + c * p.z);
    }

    pub fn rotate_y_inv(p: Vec3, s: f32, c: f32) -> Vec3 {
        return Vec3::new(c * p.x - s * p.z, p.y, s * p.x + c * p.z);
    }

    pub fn rotate_z_inv(p: Vec3, s: f32, c: f32) -> Vec3 {
        return Vec3::new(c * p.x + s * p.y, -s * p.x + c * p.y, p.z);
    }

    pub fn sd_box_3d(p: Vec3, half_ext: Vec3, r: f32) -> f32 {
        let q = abs(p) - half_ext + Vec3::splat(r);
        return length(max(q, Vec3::splat(0.0))) + min(max(q.x, max(q.y, q.z)), 0.0) - r;
    }

    pub fn sd_capsule_3d(p: Vec3, h: f32, r: f32) -> f32 {
        let py = p.y - clamp(p.y, -h, h);
        return length(Vec3::new(p.x, py, p.z)) - r;
    }

    pub fn sd_cylinder_3d(p: Vec3, h: f32, r: f32) -> f32 {
        let d = Vec2::new(length(p.xz) - r, abs(p.y) - h);
        return min(max(d.x, d.y), 0.0) + length(max(d, Vec2::splat(0.0)));
    }

    pub fn sd_sphere_3d(p: Vec3, r: f32) -> f32 {
        return length(p) - r;
    }

    pub fn sd_torus_3d(p: Vec3, major_r: f32, minor_r: f32) -> f32 {
        let q = Vec2::new(length(p.xz) - major_r, p.y);
        return length(q) - minor_r;
    }

    pub fn sdf_3d_eval(p: Vec3, shape_type: u32, half_ext: Vec3, corner_r: f32) -> f32 {
        // Use X-Y dimensions for shape sizing (not Z/depth which may be smaller)
        let min_xy = min(half_ext.x, half_ext.y);
        let mut r: f32 = 1e10;
        match shape_type {
            1u32 => { r = sd_box_3d(p, half_ext, corner_r); }
            2u32 => { r = sd_sphere_3d(p, min_xy); }
            3u32 => { r = sd_cylinder_3d(p, half_ext.y, half_ext.x); }
            4u32 => {
                // Torus: minor + major = min_xy so outer edge fills element
                let minor = min(min_xy / 3.0, half_ext.y);
                let major = min_xy - minor;
                r = sd_torus_3d(p, major, minor);
            }
            5u32 => {
                // Capsule: inscribe in X-Y bounding box
                let rad = min(half_ext.x, half_ext.y * 0.5);
                let h = max(half_ext.y - rad, 0.0);
                r = sd_capsule_3d(p, h, rad);
            }
            _ => {}
        }
        return r;
    }
}
