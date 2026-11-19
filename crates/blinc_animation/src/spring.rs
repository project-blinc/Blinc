//! Spring physics animation
//!
//! RK4-integrated spring physics for smooth, natural animations.
//! Supports preset configurations and custom spring parameters.

/// Configuration for a spring animation
#[derive(Clone, Copy, Debug)]
pub struct SpringConfig {
    pub stiffness: f32,
    pub damping: f32,
    pub mass: f32,
}

impl SpringConfig {
    /// Create a new spring configuration
    pub fn new(stiffness: f32, damping: f32, mass: f32) -> Self {
        Self {
            stiffness,
            damping,
            mass,
        }
    }

    /// A gentle, slow spring (good for page transitions)
    pub fn gentle() -> Self {
        Self {
            stiffness: 120.0,
            damping: 14.0,
            mass: 1.0,
        }
    }

    /// A wobbly spring with overshoot (good for playful UI)
    pub fn wobbly() -> Self {
        Self {
            stiffness: 180.0,
            damping: 12.0,
            mass: 1.0,
        }
    }

    /// A stiff, snappy spring (good for buttons)
    pub fn stiff() -> Self {
        Self {
            stiffness: 400.0,
            damping: 30.0,
            mass: 1.0,
        }
    }

    /// A very stiff spring with minimal oscillation (good for quick responses)
    pub fn snappy() -> Self {
        Self {
            stiffness: 600.0,
            damping: 40.0,
            mass: 1.0,
        }
    }

    /// A slow spring with no overshoot (critically damped)
    pub fn molasses() -> Self {
        Self {
            stiffness: 100.0,
            damping: 20.0,
            mass: 1.0,
        }
    }

    /// Calculate critical damping for this spring's stiffness and mass
    pub fn critical_damping(&self) -> f32 {
        2.0 * (self.stiffness * self.mass).sqrt()
    }

    /// Check if the spring is underdamped (will oscillate)
    pub fn is_underdamped(&self) -> bool {
        self.damping < self.critical_damping()
    }

    /// Check if the spring is critically damped (no oscillation, fastest settling)
    pub fn is_critically_damped(&self) -> bool {
        (self.damping - self.critical_damping()).abs() < 0.01
    }

    /// Check if the spring is overdamped (slow settling, no oscillation)
    pub fn is_overdamped(&self) -> bool {
        self.damping > self.critical_damping()
    }
}

impl Default for SpringConfig {
    fn default() -> Self {
        Self::stiff()
    }
}

/// A spring-based animator
#[derive(Clone, Copy, Debug)]
pub struct Spring {
    config: SpringConfig,
    value: f32,
    velocity: f32,
    target: f32,
    /// When true, the spring is paused — step() is a no-op
    paused: bool,
}

impl Spring {
    pub fn new(config: SpringConfig, initial: f32) -> Self {
        Self {
            config,
            value: initial,
            velocity: 0.0,
            target: initial,
            paused: false,
        }
    }

    pub fn value(&self) -> f32 {
        self.value
    }

    pub fn velocity(&self) -> f32 {
        self.velocity
    }

    pub fn target(&self) -> f32 {
        self.target
    }

    /// This spring's motion from now on as data: from its value and velocity at `now_ms` towards
    /// its target, in closed form. It settles when the descriptor says, not on the thresholds
    /// [`Self::is_settled`] uses.
    pub fn to_descriptor(&self, now_ms: f64) -> crate::descriptor::AnimationDescriptor {
        crate::descriptor::AnimationDescriptor::spring(
            now_ms,
            self.value,
            self.target,
            self.velocity,
            self.config,
        )
    }

    pub fn set_target(&mut self, target: f32) {
        self.target = target;
    }

    #[cfg(test)]
    pub(crate) fn velocity_for_test(&mut self, velocity: f32) {
        self.velocity = velocity;
    }

    /// Check if the spring has settled (within epsilon of target with minimal velocity)
    pub fn is_settled(&self) -> bool {
        const EPSILON: f32 = 0.01;
        const VELOCITY_EPSILON: f32 = 0.1;

        self.paused
            || ((self.value - self.target).abs() < EPSILON
                && self.velocity.abs() < VELOCITY_EPSILON)
    }

    /// Pause the spring — step() becomes a no-op, preserving current state
    pub fn pause(&mut self) {
        self.paused = true;
    }

    /// Resume the spring from where it was paused
    pub fn resume(&mut self) {
        self.paused = false;
    }

    /// Step the spring simulation using RK4 integration.
    ///
    /// Large `dt` values are split into fixed-size substeps to keep the
    /// integrator numerically stable on stiff springs. RK4's stability
    /// on an undamped oscillator requires `dt * omega < ~2.78`; for our
    /// stiffer scroll-rebound spring (`stiffness = 3000`,
    /// `omega ≈ 55 rad/s`), a 16 ms dt from a 60 Hz `requestAnimationFrame`
    /// loop was in the overshoot regime and produced a visible
    /// multi-cycle oscillation on top of a supposedly
    /// critically-damped spring. Substepping at ~8 ms keeps every
    /// preset stable without needing preset-specific tuning.
    pub fn step(&mut self, dt: f32) {
        if self.paused {
            return;
        }
        if self.is_settled() {
            self.value = self.target;
            self.velocity = 0.0;
            return;
        }

        const MAX_SUBSTEP: f32 = 1.0 / 120.0;
        let substeps = (dt / MAX_SUBSTEP).ceil().max(1.0) as u32;
        let h = dt / substeps as f32;
        for _ in 0..substeps {
            self.rk4_step(h);
        }
    }

    fn rk4_step(&mut self, dt: f32) {
        let k1_v = self.acceleration(self.value, self.velocity);
        let k1_x = self.velocity;

        let k2_v = self.acceleration(
            self.value + k1_x * dt * 0.5,
            self.velocity + k1_v * dt * 0.5,
        );
        let k2_x = self.velocity + k1_v * dt * 0.5;

        let k3_v = self.acceleration(
            self.value + k2_x * dt * 0.5,
            self.velocity + k2_v * dt * 0.5,
        );
        let k3_x = self.velocity + k2_v * dt * 0.5;

        let k4_v = self.acceleration(self.value + k3_x * dt, self.velocity + k3_v * dt);
        let k4_x = self.velocity + k3_v * dt;

        self.velocity += (k1_v + 2.0 * k2_v + 2.0 * k3_v + k4_v) * dt / 6.0;
        self.value += (k1_x + 2.0 * k2_x + 2.0 * k3_x + k4_x) * dt / 6.0;
    }

    fn acceleration(&self, x: f32, v: f32) -> f32 {
        let spring_force = -self.config.stiffness * (x - self.target);
        let damping_force = -self.config.damping * v;
        (spring_force + damping_force) / self.config.mass
    }
}

// =============================================================================
// ZRTL Plugin Exports
// =============================================================================

#[cfg(feature = "zrtl-plugin")]
mod ffi {
    #[unsafe(no_mangle)]
    pub extern "C" fn blinc_spring_create(
        _stiffness: f32,
        _damping: f32,
        _mass: f32,
        _initial: f32,
    ) -> *mut std::ffi::c_void {
        // TODO: Implement
        std::ptr::null_mut()
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn blinc_spring_set_target(_handle: *mut std::ffi::c_void, _target: f32) {
        // TODO: Implement
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn blinc_spring_value(_handle: *mut std::ffi::c_void) -> f32 {
        // TODO: Implement
        0.0
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn blinc_spring_velocity(_handle: *mut std::ffi::c_void) -> f32 {
        // TODO: Implement
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spring_settles_to_target() {
        let mut spring = Spring::new(SpringConfig::stiff(), 0.0);
        spring.set_target(100.0);

        // Simulate for 2 seconds at 60fps
        for _ in 0..120 {
            spring.step(1.0 / 60.0);
        }

        assert!(spring.is_settled());
        assert!((spring.value() - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_spring_inherits_velocity() {
        let mut spring = Spring::new(SpringConfig::wobbly(), 0.0);
        spring.set_target(100.0);

        // Let it get some velocity
        for _ in 0..10 {
            spring.step(1.0 / 60.0);
        }

        let velocity = spring.velocity();
        assert!(velocity > 0.0);

        // Change target mid-flight - velocity should continue
        spring.set_target(50.0);
        assert_eq!(spring.velocity(), velocity);
    }

    #[test]
    fn test_spring_presets() {
        // Test that presets are underdamped (will oscillate) for snappy feel
        assert!(SpringConfig::wobbly().is_underdamped());
        assert!(SpringConfig::gentle().is_underdamped());

        // Stiff spring should still be slightly underdamped for natural feel
        let stiff = SpringConfig::stiff();
        assert!(stiff.is_underdamped());
    }

    #[test]
    fn test_spring_rk4_stability() {
        // Test that RK4 integration remains stable even with large time steps
        let mut spring = Spring::new(SpringConfig::stiff(), 0.0);
        spring.set_target(1000.0);

        // Large time step that might cause instability with Euler integration
        for _ in 0..100 {
            spring.step(0.1);
            // Value should never exceed target too much (stability check)
            assert!(spring.value() < 2000.0);
            assert!(spring.value() > -500.0);
        }
    }

    #[test]
    fn test_spring_different_mass() {
        // Test with heavier mass - should still settle, just slower
        let config = SpringConfig::new(400.0, 25.0, 2.0);
        let mut spring = Spring::new(config, 0.0);
        spring.set_target(100.0);

        // Heavier mass needs more time to settle
        for _ in 0..240 {
            spring.step(1.0 / 60.0);
        }

        assert!(spring.value().is_finite());
        assert!(spring.is_settled());
    }
}
