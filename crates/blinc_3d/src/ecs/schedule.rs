//! System scheduling

use super::{BoxedSystem, System, SystemContext, SystemStage, World};
use smallvec::SmallVec;

/// System entry with metadata
struct SystemEntry {
    system: BoxedSystem,
    stage: SystemStage,
    priority: i32,
}

/// Schedule for running systems
pub struct Schedule {
    systems: Vec<SystemEntry>,
    sorted: bool,
}

impl Default for Schedule {
    fn default() -> Self {
        Self::new()
    }
}

impl Schedule {
    /// Create a new empty schedule
    pub fn new() -> Self {
        Self {
            systems: Vec::new(),
            sorted: true,
        }
    }

    /// Add a system to the schedule
    pub fn add_system<S: System + 'static>(&mut self, system: S) {
        let stage = system.stage();
        let priority = system.priority();
        self.systems.push(SystemEntry {
            system: Box::new(system),
            stage,
            priority,
        });
        self.sorted = false;
    }

    /// Add a boxed system to the schedule
    pub fn add_boxed_system(&mut self, system: BoxedSystem) {
        let stage = system.stage();
        let priority = system.priority();
        self.systems.push(SystemEntry {
            system,
            stage,
            priority,
        });
        self.sorted = false;
    }

    /// Sort systems by stage and priority
    fn sort_if_needed(&mut self) {
        if !self.sorted {
            self.systems.sort_by(|a, b| {
                match a.stage.cmp(&b.stage) {
                    std::cmp::Ordering::Equal => a.priority.cmp(&b.priority),
                    other => other,
                }
            });
            self.sorted = true;
        }
    }

    /// Run all systems
    pub fn run(&mut self, world: &mut World, delta_time: f32, elapsed_time: f32, frame: u64) {
        self.sort_if_needed();

        let mut ctx = SystemContext::new(world, delta_time, elapsed_time, frame);

        for entry in &mut self.systems {
            entry.system.run(&mut ctx);
        }
    }

    /// Run systems for a specific stage
    pub fn run_stage(
        &mut self,
        stage: SystemStage,
        world: &mut World,
        delta_time: f32,
        elapsed_time: f32,
        frame: u64,
    ) {
        self.sort_if_needed();

        let mut ctx = SystemContext::new(world, delta_time, elapsed_time, frame);

        for entry in &mut self.systems {
            if entry.stage == stage {
                entry.system.run(&mut ctx);
            }
        }
    }

    /// Get system count
    pub fn len(&self) -> usize {
        self.systems.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.systems.is_empty()
    }

    /// Get system names
    pub fn system_names(&self) -> Vec<&str> {
        self.systems.iter().map(|e| e.system.name()).collect()
    }
}

/// Builder for creating schedules
pub struct ScheduleBuilder {
    systems: SmallVec<[SystemEntry; 16]>,
}

impl Default for ScheduleBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ScheduleBuilder {
    /// Create a new schedule builder
    pub fn new() -> Self {
        Self {
            systems: SmallVec::new(),
        }
    }

    /// Add a system
    pub fn add<S: System + 'static>(mut self, system: S) -> Self {
        let stage = system.stage();
        let priority = system.priority();
        self.systems.push(SystemEntry {
            system: Box::new(system),
            stage,
            priority,
        });
        self
    }

    /// Build the schedule
    pub fn build(self) -> Schedule {
        let mut schedule = Schedule {
            systems: self.systems.into_vec(),
            sorted: false,
        };
        schedule.sort_if_needed();
        schedule
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestSystem {
        name: &'static str,
        stage: SystemStage,
        priority: i32,
        run_count: std::sync::Arc<std::sync::atomic::AtomicU32>,
    }

    impl System for TestSystem {
        fn run(&mut self, _ctx: &mut SystemContext) {
            self.run_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
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

    #[test]
    fn test_schedule_ordering() {
        use std::sync::atomic::AtomicU32;
        use std::sync::Arc;

        let mut schedule = Schedule::new();

        let count1 = Arc::new(AtomicU32::new(0));
        let count2 = Arc::new(AtomicU32::new(0));

        schedule.add_system(TestSystem {
            name: "System1",
            stage: SystemStage::Update,
            priority: 1,
            run_count: count1.clone(),
        });

        schedule.add_system(TestSystem {
            name: "System2",
            stage: SystemStage::PreUpdate,
            priority: 0,
            run_count: count2.clone(),
        });

        let mut world = World::new();
        schedule.run(&mut world, 0.016, 0.0, 0);

        // Both systems should run
        assert_eq!(count1.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(count2.load(std::sync::atomic::Ordering::SeqCst), 1);
    }
}
