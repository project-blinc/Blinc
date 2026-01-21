//! Joint constraints for physics simulation

use blinc_core::Vec3;

/// Joint constraint types
#[derive(Clone, Debug)]
pub enum Joint {
    /// Fixed joint - bodies move together
    Fixed {
        /// Local anchor on body A
        anchor_a: Vec3,
        /// Local anchor on body B
        anchor_b: Vec3,
    },
    /// Ball/spherical joint - allows rotation around a point
    Ball {
        /// Local anchor on body A
        anchor_a: Vec3,
        /// Local anchor on body B
        anchor_b: Vec3,
    },
    /// Revolute/hinge joint - rotation around one axis
    Revolute {
        /// Local anchor on body A
        anchor_a: Vec3,
        /// Local anchor on body B
        anchor_b: Vec3,
        /// Rotation axis (local to body A)
        axis: Vec3,
        /// Lower angle limit (radians, None = no limit)
        lower_limit: Option<f32>,
        /// Upper angle limit (radians, None = no limit)
        upper_limit: Option<f32>,
        /// Motor target velocity
        motor_velocity: Option<f32>,
        /// Motor max torque
        motor_max_torque: f32,
    },
    /// Prismatic/slider joint - translation along one axis
    Prismatic {
        /// Local anchor on body A
        anchor_a: Vec3,
        /// Local anchor on body B
        anchor_b: Vec3,
        /// Slide axis (local to body A)
        axis: Vec3,
        /// Lower position limit (None = no limit)
        lower_limit: Option<f32>,
        /// Upper position limit (None = no limit)
        upper_limit: Option<f32>,
        /// Motor target velocity
        motor_velocity: Option<f32>,
        /// Motor max force
        motor_max_force: f32,
    },
    /// Distance/spring joint - maintains distance between points
    Distance {
        /// Local anchor on body A
        anchor_a: Vec3,
        /// Local anchor on body B
        anchor_b: Vec3,
        /// Rest length (0 = compute from initial positions)
        rest_length: f32,
        /// Stiffness (spring constant)
        stiffness: f32,
        /// Damping
        damping: f32,
    },
    /// Rope joint - maximum distance constraint
    Rope {
        /// Local anchor on body A
        anchor_a: Vec3,
        /// Local anchor on body B
        anchor_b: Vec3,
        /// Maximum distance
        max_length: f32,
    },
    /// Generic 6-DOF joint
    Generic {
        /// Local frame on body A
        frame_a: JointFrame,
        /// Local frame on body B
        frame_b: JointFrame,
        /// Linear limits (min, max) for each axis
        linear_limits: [(f32, f32); 3],
        /// Angular limits (min, max) for each axis
        angular_limits: [(f32, f32); 3],
    },
}

impl Joint {
    /// Create a fixed joint
    pub fn fixed(anchor_a: Vec3, anchor_b: Vec3) -> Self {
        Self::Fixed { anchor_a, anchor_b }
    }

    /// Create a fixed joint at origin
    pub fn fixed_at_origin() -> Self {
        Self::Fixed {
            anchor_a: Vec3::ZERO,
            anchor_b: Vec3::ZERO,
        }
    }

    /// Create a ball joint
    pub fn ball(anchor_a: Vec3, anchor_b: Vec3) -> Self {
        Self::Ball { anchor_a, anchor_b }
    }

    /// Create a ball joint at origin
    pub fn ball_at_origin() -> Self {
        Self::Ball {
            anchor_a: Vec3::ZERO,
            anchor_b: Vec3::ZERO,
        }
    }

    /// Create a revolute (hinge) joint
    pub fn revolute(anchor_a: Vec3, anchor_b: Vec3, axis: Vec3) -> Self {
        Self::Revolute {
            anchor_a,
            anchor_b,
            axis,
            lower_limit: None,
            upper_limit: None,
            motor_velocity: None,
            motor_max_torque: 0.0,
        }
    }

    /// Create a hinge joint with limits
    pub fn hinge(anchor_a: Vec3, anchor_b: Vec3, axis: Vec3, lower: f32, upper: f32) -> Self {
        Self::Revolute {
            anchor_a,
            anchor_b,
            axis,
            lower_limit: Some(lower),
            upper_limit: Some(upper),
            motor_velocity: None,
            motor_max_torque: 0.0,
        }
    }

    /// Create a prismatic (slider) joint
    pub fn prismatic(anchor_a: Vec3, anchor_b: Vec3, axis: Vec3) -> Self {
        Self::Prismatic {
            anchor_a,
            anchor_b,
            axis,
            lower_limit: None,
            upper_limit: None,
            motor_velocity: None,
            motor_max_force: 0.0,
        }
    }

    /// Create a slider joint with limits
    pub fn slider(anchor_a: Vec3, anchor_b: Vec3, axis: Vec3, lower: f32, upper: f32) -> Self {
        Self::Prismatic {
            anchor_a,
            anchor_b,
            axis,
            lower_limit: Some(lower),
            upper_limit: Some(upper),
            motor_velocity: None,
            motor_max_force: 0.0,
        }
    }

    /// Create a spring joint
    pub fn spring(anchor_a: Vec3, anchor_b: Vec3, stiffness: f32, damping: f32) -> Self {
        Self::Distance {
            anchor_a,
            anchor_b,
            rest_length: 0.0, // Compute from initial positions
            stiffness,
            damping,
        }
    }

    /// Create a distance constraint
    pub fn distance(anchor_a: Vec3, anchor_b: Vec3, rest_length: f32) -> Self {
        Self::Distance {
            anchor_a,
            anchor_b,
            rest_length,
            stiffness: 1000.0,
            damping: 10.0,
        }
    }

    /// Create a rope joint
    pub fn rope(anchor_a: Vec3, anchor_b: Vec3, max_length: f32) -> Self {
        Self::Rope {
            anchor_a,
            anchor_b,
            max_length,
        }
    }

    // ========== Builder methods for Revolute ==========

    /// Add angular limits to revolute joint
    pub fn with_angular_limits(mut self, lower: f32, upper: f32) -> Self {
        if let Self::Revolute { lower_limit, upper_limit, .. } = &mut self {
            *lower_limit = Some(lower);
            *upper_limit = Some(upper);
        }
        self
    }

    /// Add motor to revolute joint
    pub fn with_motor(mut self, velocity: f32, max_torque: f32) -> Self {
        match &mut self {
            Self::Revolute { motor_velocity, motor_max_torque, .. } => {
                *motor_velocity = Some(velocity);
                *motor_max_torque = max_torque;
            }
            Self::Prismatic { motor_velocity, motor_max_force, .. } => {
                *motor_velocity = Some(velocity);
                *motor_max_force = max_torque;
            }
            _ => {}
        }
        self
    }

    // ========== Builder methods for Distance ==========

    /// Set spring stiffness
    pub fn with_stiffness(mut self, stiffness: f32) -> Self {
        if let Self::Distance { stiffness: s, .. } = &mut self {
            *s = stiffness;
        }
        self
    }

    /// Set spring damping
    pub fn with_damping(mut self, damping: f32) -> Self {
        if let Self::Distance { damping: d, .. } = &mut self {
            *d = damping;
        }
        self
    }

    // ========== Presets ==========

    /// Door hinge joint
    pub fn door_hinge(anchor: Vec3) -> Self {
        Self::Revolute {
            anchor_a: anchor,
            anchor_b: anchor,
            axis: Vec3::new(0.0, 1.0, 0.0), // Y-axis
            lower_limit: Some(0.0),
            upper_limit: Some(std::f32::consts::PI * 0.5), // 90 degrees
            motor_velocity: None,
            motor_max_torque: 0.0,
        }
    }

    /// Wheel axle joint
    pub fn wheel_axle(anchor: Vec3) -> Self {
        Self::Revolute {
            anchor_a: anchor,
            anchor_b: Vec3::ZERO,
            axis: Vec3::new(1.0, 0.0, 0.0), // X-axis (side-to-side)
            lower_limit: None,
            upper_limit: None,
            motor_velocity: None,
            motor_max_torque: 1000.0,
        }
    }

    /// Suspension spring
    pub fn suspension(anchor: Vec3, travel: f32) -> Self {
        Self::Prismatic {
            anchor_a: anchor,
            anchor_b: anchor,
            axis: Vec3::new(0.0, 1.0, 0.0),
            lower_limit: Some(-travel),
            upper_limit: Some(travel),
            motor_velocity: None,
            motor_max_force: 0.0,
        }
    }

    /// Ragdoll limb joint
    pub fn ragdoll_limb(anchor_a: Vec3, anchor_b: Vec3) -> Self {
        Self::Ball { anchor_a, anchor_b }
    }

    /// Chain link
    pub fn chain_link(length: f32) -> Self {
        Self::Distance {
            anchor_a: Vec3::new(0.0, -length / 2.0, 0.0),
            anchor_b: Vec3::new(0.0, length / 2.0, 0.0),
            rest_length: length,
            stiffness: 10000.0,
            damping: 100.0,
        }
    }
}

/// Joint frame (position and orientation)
#[derive(Clone, Debug, Default)]
pub struct JointFrame {
    /// Local position
    pub position: Vec3,
    /// Local rotation (euler angles)
    pub rotation: Vec3,
}

impl JointFrame {
    /// Create a joint frame at origin
    pub fn origin() -> Self {
        Self {
            position: Vec3::ZERO,
            rotation: Vec3::ZERO,
        }
    }

    /// Create a joint frame at position
    pub fn at(position: Vec3) -> Self {
        Self {
            position,
            rotation: Vec3::ZERO,
        }
    }

    /// Set rotation
    pub fn with_rotation(mut self, rotation: Vec3) -> Self {
        self.rotation = rotation;
        self
    }
}

/// Joint limit configuration
#[derive(Clone, Debug)]
pub struct JointLimits {
    /// Lower limit
    pub lower: f32,
    /// Upper limit
    pub upper: f32,
    /// Contact distance (soft limit zone)
    pub contact_distance: f32,
}

impl JointLimits {
    /// Create new limits
    pub fn new(lower: f32, upper: f32) -> Self {
        Self {
            lower,
            upper,
            contact_distance: 0.01,
        }
    }

    /// Create limits centered at zero
    pub fn symmetric(half_range: f32) -> Self {
        Self {
            lower: -half_range,
            upper: half_range,
            contact_distance: 0.01,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_joint_creation() {
        let fixed = Joint::fixed(Vec3::ZERO, Vec3::new(0.0, 1.0, 0.0));
        match fixed {
            Joint::Fixed { anchor_a, anchor_b } => {
                assert!((anchor_a.y - 0.0).abs() < 0.001);
                assert!((anchor_b.y - 1.0).abs() < 0.001);
            }
            _ => panic!("Expected fixed joint"),
        }
    }

    #[test]
    fn test_joint_presets() {
        let door = Joint::door_hinge(Vec3::ZERO);
        match door {
            Joint::Revolute { lower_limit, upper_limit, .. } => {
                assert!(lower_limit.is_some());
                assert!(upper_limit.is_some());
            }
            _ => panic!("Expected revolute joint"),
        }
    }
}
