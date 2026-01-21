//! System trait and execution context

use super::World;

/// System execution stages
///
/// Systems are executed in order: PreUpdate → Update → PostUpdate → PreRender → Render
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SystemStage {
    /// Physics simulation, collision detection (runs first)
    PreUpdate = 0,
    /// Main game logic
    Update = 1,
    /// Animation and transform updates
    PostUpdate = 2,
    /// Camera and visibility culling
    PreRender = 3,
    /// Rendering commands generation (runs last)
    Render = 4,
}

impl Default for SystemStage {
    fn default() -> Self {
        Self::Update
    }
}

/// Context provided to systems each frame
pub struct SystemContext<'a> {
    /// The world containing all entities and components
    pub world: &'a mut World,
    /// Time since last frame in seconds
    pub delta_time: f32,
    /// Total elapsed time in seconds
    pub elapsed_time: f32,
    /// Current frame number
    pub frame: u64,
}

impl<'a> SystemContext<'a> {
    /// Create a new system context
    pub fn new(world: &'a mut World, delta_time: f32, elapsed_time: f32, frame: u64) -> Self {
        Self {
            world,
            delta_time,
            elapsed_time,
            frame,
        }
    }
}

/// System trait for update logic
///
/// Systems process entities with specific component combinations.
///
/// # Example
///
/// ```rust,ignore
/// use blinc_3d::ecs::{System, SystemContext};
///
/// struct GravitySystem {
///     gravity: f32,
/// }
///
/// impl System for GravitySystem {
///     fn run(&mut self, ctx: &mut SystemContext) {
///         for (entity, (velocity,)) in ctx.world.query_mut::<(&mut Velocity,)>() {
///             velocity.y -= self.gravity * ctx.delta_time;
///         }
///     }
///
///     fn name(&self) -> &'static str {
///         "GravitySystem"
///     }
/// }
/// ```
pub trait System: Send + Sync {
    /// Run the system
    fn run(&mut self, ctx: &mut SystemContext);

    /// System name for debugging
    fn name(&self) -> &'static str;

    /// System stage (when to run)
    fn stage(&self) -> SystemStage {
        SystemStage::Update
    }

    /// System priority within stage (lower runs first)
    fn priority(&self) -> i32 {
        0
    }
}

/// A boxed system for type erasure
pub type BoxedSystem = Box<dyn System>;

/// Function-based system wrapper
pub struct FnSystem<F>
where
    F: FnMut(&mut SystemContext) + Send + Sync,
{
    name: &'static str,
    stage: SystemStage,
    priority: i32,
    func: F,
}

impl<F> FnSystem<F>
where
    F: FnMut(&mut SystemContext) + Send + Sync,
{
    /// Create a new function-based system
    pub fn new(name: &'static str, func: F) -> Self {
        Self {
            name,
            stage: SystemStage::Update,
            priority: 0,
            func,
        }
    }

    /// Set the system stage
    pub fn with_stage(mut self, stage: SystemStage) -> Self {
        self.stage = stage;
        self
    }

    /// Set the system priority
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }
}

impl<F> System for FnSystem<F>
where
    F: FnMut(&mut SystemContext) + Send + Sync,
{
    fn run(&mut self, ctx: &mut SystemContext) {
        (self.func)(ctx);
    }

    fn name(&self) -> &'static str {
        self.name
    }

    fn stage(&self) -> SystemStage {
        self.stage
    }

    fn priority(&self) -> i32 {
        self.priority
    }
}

/// Create a function-based system
///
/// # Example
///
/// ```rust,ignore
/// let system = system("MySystem", |ctx| {
///     // System logic here
/// });
/// ```
pub fn system<F>(name: &'static str, func: F) -> FnSystem<F>
where
    F: FnMut(&mut SystemContext) + Send + Sync,
{
    FnSystem::new(name, func)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_stage_ordering() {
        assert!(SystemStage::PreUpdate < SystemStage::Update);
        assert!(SystemStage::Update < SystemStage::PostUpdate);
        assert!(SystemStage::PostUpdate < SystemStage::PreRender);
        assert!(SystemStage::PreRender < SystemStage::Render);
    }
}
