//! Time of day system for dynamic sky cycles

use super::{Skybox, ProceduralSkybox, GradientSkybox};
use crate::ecs::{Component, System, SystemContext, SystemStage, World};
use blinc_core::{Color, Vec3};
use std::f32::consts::PI;

/// Time of day preset
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeOfDay {
    /// Early morning (5-7)
    Dawn,
    /// Morning (7-10)
    Morning,
    /// Midday (10-14)
    Noon,
    /// Afternoon (14-17)
    Afternoon,
    /// Evening (17-19)
    Dusk,
    /// Night (19-5)
    Night,
}

impl TimeOfDay {
    /// Get the approximate hour for this time of day
    pub fn hour(&self) -> f32 {
        match self {
            TimeOfDay::Dawn => 6.0,
            TimeOfDay::Morning => 8.5,
            TimeOfDay::Noon => 12.0,
            TimeOfDay::Afternoon => 15.5,
            TimeOfDay::Dusk => 18.0,
            TimeOfDay::Night => 22.0,
        }
    }

    /// Determine time of day from hour (0-24)
    pub fn from_hour(hour: f32) -> Self {
        let hour = hour % 24.0;
        if hour < 5.0 || hour >= 21.0 {
            TimeOfDay::Night
        } else if hour < 7.0 {
            TimeOfDay::Dawn
        } else if hour < 10.0 {
            TimeOfDay::Morning
        } else if hour < 14.0 {
            TimeOfDay::Noon
        } else if hour < 17.0 {
            TimeOfDay::Afternoon
        } else {
            TimeOfDay::Dusk
        }
    }

    /// Get a procedural skybox for this time
    pub fn to_procedural(&self) -> ProceduralSkybox {
        match self {
            TimeOfDay::Dawn => ProceduralSkybox::sunrise(),
            TimeOfDay::Morning => {
                let mut sky = ProceduralSkybox::new();
                sky.set_time_of_day(8.5);
                sky
            }
            TimeOfDay::Noon => ProceduralSkybox::midday(),
            TimeOfDay::Afternoon => {
                let mut sky = ProceduralSkybox::new();
                sky.set_time_of_day(15.5);
                sky
            }
            TimeOfDay::Dusk => ProceduralSkybox::sunset(),
            TimeOfDay::Night => {
                let mut sky = ProceduralSkybox::new();
                sky.set_time_of_day(22.0);
                sky.sun_intensity = 0.0;
                sky
            }
        }
    }

    /// Get a gradient skybox for this time
    pub fn to_gradient(&self) -> GradientSkybox {
        match self {
            TimeOfDay::Dawn => GradientSkybox::dawn(),
            TimeOfDay::Morning => GradientSkybox::clear_day(),
            TimeOfDay::Noon => GradientSkybox::clear_day(),
            TimeOfDay::Afternoon => GradientSkybox::clear_day(),
            TimeOfDay::Dusk => GradientSkybox::dusk(),
            TimeOfDay::Night => GradientSkybox::night(),
        }
    }

    /// Get a skybox for this time
    pub fn to_skybox(&self) -> Skybox {
        Skybox::Procedural(self.to_procedural())
    }
}

/// Day/night cycle configuration
#[derive(Clone, Debug)]
pub struct DayNightCycle {
    /// Current time in hours (0-24)
    pub current_hour: f32,
    /// Speed multiplier (1.0 = real-time, 60.0 = 1 minute = 1 hour)
    pub speed: f32,
    /// Whether cycle is paused
    pub paused: bool,
    /// Whether to use procedural or gradient sky
    pub use_procedural: bool,
}

impl DayNightCycle {
    /// Create a new day/night cycle starting at noon
    pub fn new() -> Self {
        Self {
            current_hour: 12.0,
            speed: 60.0, // 1 real minute = 1 game hour
            paused: false,
            use_procedural: true,
        }
    }

    /// Create starting at specific hour
    pub fn starting_at(hour: f32) -> Self {
        Self {
            current_hour: hour % 24.0,
            ..Self::new()
        }
    }

    /// Set speed (multiplier, e.g., 60 = 1 minute real = 1 hour game)
    pub fn with_speed(mut self, speed: f32) -> Self {
        self.speed = speed;
        self
    }

    /// Use gradient sky instead of procedural
    pub fn with_gradient(mut self) -> Self {
        self.use_procedural = false;
        self
    }

    /// Get current time of day
    pub fn time_of_day(&self) -> TimeOfDay {
        TimeOfDay::from_hour(self.current_hour)
    }

    /// Update cycle
    pub fn update(&mut self, dt: f32) {
        if !self.paused {
            // dt is in seconds, convert to hours based on speed
            self.current_hour += (dt / 3600.0) * self.speed;
            self.current_hour %= 24.0;
        }
    }

    /// Get current skybox
    pub fn current_skybox(&self) -> Skybox {
        if self.use_procedural {
            let mut sky = ProceduralSkybox::new();
            sky.set_time_of_day(self.current_hour);
            Skybox::Procedural(sky)
        } else {
            // Blend between gradient presets
            let tod = self.time_of_day();
            Skybox::Gradient(tod.to_gradient())
        }
    }

    /// Pause the cycle
    pub fn pause(&mut self) {
        self.paused = true;
    }

    /// Resume the cycle
    pub fn resume(&mut self) {
        self.paused = false;
    }

    /// Set time directly (0-24)
    pub fn set_hour(&mut self, hour: f32) {
        self.current_hour = hour % 24.0;
    }

    /// Check if it's currently daytime
    pub fn is_daytime(&self) -> bool {
        self.current_hour >= 6.0 && self.current_hour < 20.0
    }

    /// Get sun elevation (0 = horizon, PI/2 = zenith)
    pub fn sun_elevation(&self) -> f32 {
        if !self.is_daytime() {
            return 0.0;
        }
        let normalized = ((self.current_hour - 6.0) / 14.0).clamp(0.0, 1.0);
        (normalized * PI).sin() * (PI / 2.2)
    }
}

impl Default for DayNightCycle {
    fn default() -> Self {
        Self::new()
    }
}

impl Component for DayNightCycle {}

/// System that updates day/night cycle and skybox
pub struct TimeOfDaySystem;

impl System for TimeOfDaySystem {
    fn run(&mut self, ctx: &mut SystemContext) {
        let dt = ctx.delta_time;

        // Find all entities with day/night cycles and collect them first
        // (this ECS uses immutable queries, so we collect entities then mutate)
        let entities_to_update: Vec<_> = ctx.world
            .query::<(&DayNightCycle,)>()
            .iter()
            .map(|(entity, _)| entity)
            .collect();

        // Apply updates through get_mut
        for entity in entities_to_update {
            if let Some(cycle) = ctx.world.get_mut::<DayNightCycle>(entity) {
                cycle.update(dt);
            }
        }
    }

    fn name(&self) -> &'static str {
        "TimeOfDaySystem"
    }

    fn stage(&self) -> SystemStage {
        SystemStage::Update
    }

    fn priority(&self) -> i32 {
        -10 // Run before rendering systems
    }
}
