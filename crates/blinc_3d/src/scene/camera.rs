//! Camera components

use crate::ecs::Component;
use crate::math::Mat4Ext;
use crate::scene::Object3D;
use blinc_core::Mat4;

/// Perspective camera for 3D rendering
///
/// Uses a frustum-based projection with field of view.
#[derive(Clone, Debug)]
pub struct PerspectiveCamera {
    /// Field of view in radians (vertical)
    pub fov: f32,
    /// Aspect ratio (width / height)
    pub aspect: f32,
    /// Near clipping plane distance
    pub near: f32,
    /// Far clipping plane distance
    pub far: f32,
    /// Zoom factor (1.0 = normal)
    pub zoom: f32,
}

impl Default for PerspectiveCamera {
    fn default() -> Self {
        Self::new(std::f32::consts::FRAC_PI_4, 16.0 / 9.0, 0.1, 1000.0)
    }
}

impl Component for PerspectiveCamera {}

impl PerspectiveCamera {
    /// Create a new perspective camera
    ///
    /// # Arguments
    /// * `fov` - Field of view in radians (vertical)
    /// * `aspect` - Aspect ratio (width / height)
    /// * `near` - Near clipping plane
    /// * `far` - Far clipping plane
    pub fn new(fov: f32, aspect: f32, near: f32, far: f32) -> Self {
        Self {
            fov,
            aspect,
            near,
            far,
            zoom: 1.0,
        }
    }

    /// Set aspect ratio
    pub fn with_aspect(mut self, aspect: f32) -> Self {
        self.aspect = aspect;
        self
    }

    /// Set zoom factor
    pub fn with_zoom(mut self, zoom: f32) -> Self {
        self.zoom = zoom;
        self
    }

    /// Get effective field of view (accounting for zoom)
    pub fn effective_fov(&self) -> f32 {
        2.0 * ((self.fov / 2.0).tan() / self.zoom).atan()
    }

    /// Compute projection matrix
    pub fn projection_matrix(&self) -> Mat4 {
        let fov = self.effective_fov();
        <Mat4 as Mat4Ext>::perspective_rh(fov, self.aspect, self.near, self.far)
    }

    /// Compute view matrix from transform
    pub fn view_matrix(&self, transform: &Object3D) -> Mat4 {
        let eye = transform.position;
        let forward = transform.forward();
        let target = blinc_core::Vec3::new(
            eye.x + forward.x,
            eye.y + forward.y,
            eye.z + forward.z,
        );
        let up = transform.up();
        <Mat4 as Mat4Ext>::look_at_rh(eye, target, up)
    }

    /// Convert to blinc_core Camera for rendering
    pub fn to_core_camera(&self, transform: &Object3D, aspect: f32) -> blinc_core::Camera {
        let forward = transform.forward();
        let target = blinc_core::Vec3::new(
            transform.position.x + forward.x,
            transform.position.y + forward.y,
            transform.position.z + forward.z,
        );
        blinc_core::Camera {
            position: transform.position,
            target,
            up: blinc_core::Vec3::UP,
            projection: blinc_core::CameraProjection::Perspective {
                fov_y: self.effective_fov(),
                aspect,
                near: self.near,
                far: self.far,
            },
        }
    }

    /// Convert to SDF camera vectors for raymarching
    ///
    /// Returns (camera_pos, camera_dir, camera_up, camera_right, fov)
    pub fn to_sdf_camera_vectors(
        &self,
        transform: &Object3D,
    ) -> (blinc_core::Vec3, blinc_core::Vec3, blinc_core::Vec3, blinc_core::Vec3, f32) {
        let camera_pos = transform.position;
        let camera_dir = transform.forward().normalize();
        let up = transform.up();

        // Calculate right vector (cross product of direction and up)
        let right = blinc_core::Vec3::new(
            camera_dir.z * up.y - camera_dir.y * up.z,
            camera_dir.x * up.z - camera_dir.z * up.x,
            camera_dir.y * up.x - camera_dir.x * up.y,
        );
        let right_len = (right.x * right.x + right.y * right.y + right.z * right.z).sqrt();
        let camera_right = blinc_core::Vec3::new(
            right.x / right_len,
            right.y / right_len,
            right.z / right_len,
        );

        // Recalculate up (cross product of right and direction)
        let camera_up = blinc_core::Vec3::new(
            camera_right.y * camera_dir.z - camera_right.z * camera_dir.y,
            camera_right.z * camera_dir.x - camera_right.x * camera_dir.z,
            camera_right.x * camera_dir.y - camera_right.y * camera_dir.x,
        );

        (camera_pos, camera_dir, camera_up, camera_right, self.effective_fov())
    }

    /// Project a 3D world point to 2D screen coordinates
    ///
    /// Returns `Some((screen_x, screen_y, depth))` if the point is in front of the camera,
    /// or `None` if the point is behind the camera.
    ///
    /// # Arguments
    /// * `world_point` - The 3D point to project
    /// * `camera_pos` - Camera position in world space
    /// * `camera_target` - Point the camera is looking at
    /// * `viewport_width` - Width of the viewport in pixels
    /// * `viewport_height` - Height of the viewport in pixels
    ///
    /// # Returns
    /// * `screen_x` - X coordinate in viewport (0 = left edge)
    /// * `screen_y` - Y coordinate in viewport (0 = top edge)
    /// * `depth` - Distance from camera (for depth sorting/size scaling)
    pub fn project_to_screen(
        &self,
        world_point: blinc_core::Vec3,
        camera_pos: blinc_core::Vec3,
        camera_target: blinc_core::Vec3,
        viewport_width: f32,
        viewport_height: f32,
    ) -> Option<(f32, f32, f32)> {
        project_point_to_screen(
            world_point,
            camera_pos,
            camera_target,
            self.effective_fov(),
            viewport_width,
            viewport_height,
        )
    }
}

/// Project a 3D point to 2D screen coordinates
///
/// Standalone function for projecting world coordinates to screen space.
/// Useful when you don't have a full camera component.
///
/// # Arguments
/// * `world_point` - The 3D point to project
/// * `camera_pos` - Camera position in world space
/// * `camera_target` - Point the camera is looking at
/// * `fov` - Field of view in radians
/// * `viewport_width` - Width of the viewport in pixels
/// * `viewport_height` - Height of the viewport in pixels
///
/// # Returns
/// `Some((screen_x, screen_y, depth))` if point is visible, `None` if behind camera
pub fn project_point_to_screen(
    world_point: blinc_core::Vec3,
    camera_pos: blinc_core::Vec3,
    camera_target: blinc_core::Vec3,
    fov: f32,
    viewport_width: f32,
    viewport_height: f32,
) -> Option<(f32, f32, f32)> {
    // Helper to normalize a vector
    fn normalize(x: f32, y: f32, z: f32) -> (f32, f32, f32) {
        let len = (x * x + y * y + z * z).sqrt();
        if len > 0.0001 {
            (x / len, y / len, z / len)
        } else {
            (0.0, 1.0, 0.0)
        }
    }

    // Calculate camera basis vectors
    let fx = camera_target.x - camera_pos.x;
    let fy = camera_target.y - camera_pos.y;
    let fz = camera_target.z - camera_pos.z;
    let (fx, fy, fz) = normalize(fx, fy, fz);

    // Right = Up x Forward (where Up is (0, 1, 0))
    // right = (0, 1, 0) x (fx, fy, fz) = (1*fz - 0*fy, 0*fx - 0*fz, 0*fy - 1*fx)
    let rx = -fz;
    let ry = 0.0;
    let rz = fx;
    let (rx, ry, rz) = normalize(rx, ry, rz);

    // Up = Forward x Right
    let ux = fy * rz - fz * ry;
    let uy = fz * rx - fx * rz;
    let uz = fx * ry - fy * rx;

    // Vector from camera to point
    let tpx = world_point.x - camera_pos.x;
    let tpy = world_point.y - camera_pos.y;
    let tpz = world_point.z - camera_pos.z;

    // Transform to camera space (dot products)
    let cam_x = tpx * rx + tpy * ry + tpz * rz;
    let cam_y = tpx * ux + tpy * uy + tpz * uz;
    let cam_z = tpx * fx + tpy * fy + tpz * fz;

    // Check if point is in front of camera
    if cam_z <= 0.1 {
        return None;
    }

    // Project to normalized device coordinates
    let aspect = viewport_width / viewport_height;
    let scale = 1.0 / (fov * 0.5).tan();

    let ndc_x = (cam_x / cam_z) * scale / aspect;
    let ndc_y = (cam_y / cam_z) * scale;

    // Convert to pixel coordinates (origin at top-left)
    let screen_x = viewport_width * 0.5 + ndc_x * viewport_width * 0.5;
    let screen_y = viewport_height * 0.5 - ndc_y * viewport_height * 0.5;

    Some((screen_x, screen_y, cam_z))
}

/// Orthographic camera for 2D-like 3D rendering
///
/// Uses parallel projection (no perspective).
#[derive(Clone, Debug)]
pub struct OrthographicCamera {
    /// Left edge of frustum
    pub left: f32,
    /// Right edge of frustum
    pub right: f32,
    /// Top edge of frustum
    pub top: f32,
    /// Bottom edge of frustum
    pub bottom: f32,
    /// Near clipping plane
    pub near: f32,
    /// Far clipping plane
    pub far: f32,
    /// Zoom factor (1.0 = normal)
    pub zoom: f32,
}

impl Default for OrthographicCamera {
    fn default() -> Self {
        Self::new(10.0, 16.0 / 9.0, 0.1, 1000.0)
    }
}

impl Component for OrthographicCamera {}

impl OrthographicCamera {
    /// Create a new orthographic camera
    ///
    /// # Arguments
    /// * `frustum_size` - Half-height of the view frustum
    /// * `aspect` - Aspect ratio (width / height)
    /// * `near` - Near clipping plane
    /// * `far` - Far clipping plane
    pub fn new(frustum_size: f32, aspect: f32, near: f32, far: f32) -> Self {
        let half_height = frustum_size;
        let half_width = half_height * aspect;
        Self {
            left: -half_width,
            right: half_width,
            top: half_height,
            bottom: -half_height,
            near,
            far,
            zoom: 1.0,
        }
    }

    /// Create from explicit bounds
    pub fn from_bounds(left: f32, right: f32, bottom: f32, top: f32, near: f32, far: f32) -> Self {
        Self {
            left,
            right,
            top,
            bottom,
            near,
            far,
            zoom: 1.0,
        }
    }

    /// Set zoom factor
    pub fn with_zoom(mut self, zoom: f32) -> Self {
        self.zoom = zoom;
        self
    }

    /// Update frustum for new aspect ratio
    pub fn set_aspect(&mut self, aspect: f32) {
        let height = self.top - self.bottom;
        let half_width = (height / 2.0) * aspect;
        self.left = -half_width;
        self.right = half_width;
    }

    /// Compute projection matrix
    pub fn projection_matrix(&self) -> Mat4 {
        let left = self.left / self.zoom;
        let right = self.right / self.zoom;
        let bottom = self.bottom / self.zoom;
        let top = self.top / self.zoom;
        <Mat4 as Mat4Ext>::orthographic_rh(left, right, bottom, top, self.near, self.far)
    }

    /// Compute view matrix from transform
    pub fn view_matrix(&self, transform: &Object3D) -> Mat4 {
        let eye = transform.position;
        let forward = transform.forward();
        let target = blinc_core::Vec3::new(
            eye.x + forward.x,
            eye.y + forward.y,
            eye.z + forward.z,
        );
        let up = transform.up();
        <Mat4 as Mat4Ext>::look_at_rh(eye, target, up)
    }

    /// Convert to blinc_core Camera for rendering
    pub fn to_core_camera(&self, transform: &Object3D) -> blinc_core::Camera {
        let forward = transform.forward();
        let target = blinc_core::Vec3::new(
            transform.position.x + forward.x,
            transform.position.y + forward.y,
            transform.position.z + forward.z,
        );
        blinc_core::Camera {
            position: transform.position,
            target,
            up: blinc_core::Vec3::UP,
            projection: blinc_core::CameraProjection::Orthographic {
                left: self.left / self.zoom,
                right: self.right / self.zoom,
                bottom: self.bottom / self.zoom,
                top: self.top / self.zoom,
                near: self.near,
                far: self.far,
            },
        }
    }
}
