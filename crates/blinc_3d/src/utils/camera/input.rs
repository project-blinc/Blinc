//! Camera input handling

use blinc_core::Vec2;

/// Input state for camera controllers
#[derive(Clone, Debug, Default)]
pub struct CameraInput {
    /// Mouse movement delta this frame
    pub mouse_delta: Vec2,
    /// Scroll wheel delta (positive = zoom in)
    pub scroll_delta: f32,
    /// Currently pressed camera control keys
    pub keys: CameraKeys,
    /// Whether primary mouse button is pressed (left click)
    pub primary_pressed: bool,
    /// Whether secondary mouse button is pressed (right click)
    pub secondary_pressed: bool,
    /// Whether middle mouse button is pressed
    pub middle_pressed: bool,
}

impl CameraInput {
    /// Create empty input state
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if any movement keys are pressed
    pub fn has_movement(&self) -> bool {
        self.keys.forward || self.keys.backward || self.keys.left || self.keys.right
            || self.keys.up || self.keys.down
    }

    /// Get movement direction as normalized vector (in camera space)
    pub fn movement_direction(&self) -> blinc_core::Vec3 {
        let mut dir = blinc_core::Vec3::ZERO;

        if self.keys.forward {
            dir.z -= 1.0;
        }
        if self.keys.backward {
            dir.z += 1.0;
        }
        if self.keys.left {
            dir.x -= 1.0;
        }
        if self.keys.right {
            dir.x += 1.0;
        }
        if self.keys.up {
            dir.y += 1.0;
        }
        if self.keys.down {
            dir.y -= 1.0;
        }

        let len_sq = dir.x * dir.x + dir.y * dir.y + dir.z * dir.z;
        if len_sq > 0.0 {
            let len = len_sq.sqrt();
            dir.x /= len;
            dir.y /= len;
            dir.z /= len;
        }

        dir
    }
}

/// Camera control key states
#[derive(Clone, Debug, Default)]
pub struct CameraKeys {
    /// Move forward (typically W)
    pub forward: bool,
    /// Move backward (typically S)
    pub backward: bool,
    /// Strafe left (typically A)
    pub left: bool,
    /// Strafe right (typically D)
    pub right: bool,
    /// Move up (typically Space or E)
    pub up: bool,
    /// Move down (typically Ctrl or Q)
    pub down: bool,
    /// Sprint modifier (typically Shift)
    pub sprint: bool,
    /// Slow modifier (typically Alt)
    pub slow: bool,
}

impl CameraKeys {
    /// Create with no keys pressed
    pub fn new() -> Self {
        Self::default()
    }

    /// Get speed multiplier based on modifiers
    pub fn speed_multiplier(&self, sprint_mult: f32, slow_mult: f32) -> f32 {
        if self.sprint {
            sprint_mult
        } else if self.slow {
            slow_mult
        } else {
            1.0
        }
    }
}
