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
