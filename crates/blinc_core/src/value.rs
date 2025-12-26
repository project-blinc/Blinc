//! Dynamic value system for reactive rendering
//!
//! The `Value` trait abstracts over different sources of values:
//! - Static values (literals, constants)
//! - Signal values (from ReactiveGraph)
//! - Spring values (from AnimationScheduler)
//! - Derived values (computed from other values)
//!
//! This allows the RenderTree to store references to values rather than
//! the values themselves, enabling updates without tree rebuilds.

use std::marker::PhantomData;
use std::sync::Arc;

use crate::reactive::SignalId;

/// Context provided to value resolution at render time
pub struct ValueContext<'a> {
    /// Access to reactive graph for signal values
    pub reactive: &'a dyn ReactiveAccess,
    /// Access to animation scheduler for spring values
    pub animations: &'a dyn AnimationAccess,
}

/// Trait for accessing reactive signal values
pub trait ReactiveAccess {
    /// Get a signal value by ID, returns None if not found
    fn get_signal_value_raw(&self, id: u64) -> Option<Box<dyn std::any::Any + Send>>;
}

/// Trait for accessing animation values (springs, keyframes, timelines)
pub trait AnimationAccess {
    /// Get a spring's current value
    fn get_spring_value(&self, id: u64, generation: u32) -> Option<f32>;

    /// Get a keyframe animation's current value
    fn get_keyframe_value(&self, _id: u64) -> Option<f32> {
        None // Default implementation
    }

    /// Get a timeline's current value for a given property
    fn get_timeline_value(&self, _timeline_id: u64, _property: &str) -> Option<f32> {
        None // Default implementation
    }
}

/// A value that can be resolved at render time
///
/// This trait is object-safe and can be stored in RenderProps.
pub trait Value<T>: Send + Sync {
    /// Resolve the current value
    fn get(&self, ctx: &ValueContext) -> T;

    /// Check if this is a static value (never changes)
    fn is_static(&self) -> bool {
        false
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Static Value
// ─────────────────────────────────────────────────────────────────────────────

/// A static value that never changes
#[derive(Clone)]
pub struct Static<T>(pub T);

impl<T: Clone + Send + Sync> Value<T> for Static<T> {
    fn get(&self, _ctx: &ValueContext) -> T {
        self.0.clone()
    }

    fn is_static(&self) -> bool {
        true
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Signal Value
// ─────────────────────────────────────────────────────────────────────────────

/// A value backed by a reactive signal
pub struct SignalValue<T> {
    signal_id: u64,
    default: T,
    _marker: PhantomData<T>,
}

impl<T: Clone + Send + Sync + 'static> SignalValue<T> {
    pub fn new(signal_id: SignalId, default: T) -> Self {
        Self {
            signal_id: signal_id.to_raw(),
            default,
            _marker: PhantomData,
        }
    }
}

impl<T: Clone + Send + Sync + 'static> Value<T> for SignalValue<T> {
    fn get(&self, ctx: &ValueContext) -> T {
        ctx.reactive
            .get_signal_value_raw(self.signal_id)
            .and_then(|boxed| boxed.downcast::<T>().ok())
            .map(|b| *b)
            .unwrap_or_else(|| self.default.clone())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Spring Value
// ─────────────────────────────────────────────────────────────────────────────

/// A value backed by a spring animation
#[derive(Clone)]
pub struct SpringValue {
    spring_id: u64,
    generation: u32,
    default: f32,
}

impl SpringValue {
    pub fn new(spring_id: u64, generation: u32, default: f32) -> Self {
        Self {
            spring_id,
            generation,
            default,
        }
    }

    /// Create from raw parts (for use with SpringId)
    pub fn from_raw(id: u64, generation: u32, default: f32) -> Self {
        Self::new(id, generation, default)
    }
}

impl Value<f32> for SpringValue {
    fn get(&self, ctx: &ValueContext) -> f32 {
        ctx.animations
            .get_spring_value(self.spring_id, self.generation)
            .unwrap_or(self.default)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Derived Value
// ─────────────────────────────────────────────────────────────────────────────

/// A value computed from a function
pub struct Derived<T, F>
where
    F: Fn(&ValueContext) -> T + Send + Sync,
{
    compute: F,
    _marker: PhantomData<T>,
}

impl<T, F> Derived<T, F>
where
    F: Fn(&ValueContext) -> T + Send + Sync,
{
    pub fn new(compute: F) -> Self {
        Self {
            compute,
            _marker: PhantomData,
        }
    }
}

impl<T, F> Value<T> for Derived<T, F>
where
    T: Send + Sync,
    F: Fn(&ValueContext) -> T + Send + Sync,
{
    fn get(&self, ctx: &ValueContext) -> T {
        (self.compute)(ctx)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Boxed Value (for type erasure in RenderProps)
// ─────────────────────────────────────────────────────────────────────────────

/// Type-erased boxed value for storage in render props
pub type BoxedValue<T> = Arc<dyn Value<T>>;

/// Helper to create a boxed static value
pub fn static_value<T: Clone + Send + Sync + 'static>(value: T) -> BoxedValue<T> {
    Arc::new(Static(value))
}

/// Helper to create a boxed signal value
pub fn signal_value<T: Clone + Send + Sync + 'static>(
    signal_id: SignalId,
    default: T,
) -> BoxedValue<T> {
    Arc::new(SignalValue::new(signal_id, default))
}

/// Helper to create a boxed spring value
pub fn spring_value(spring_id: u64, generation: u32, default: f32) -> BoxedValue<f32> {
    Arc::new(SpringValue::from_raw(spring_id, generation, default))
}

/// Helper to create a boxed derived value
pub fn derived_value<T, F>(compute: F) -> BoxedValue<T>
where
    T: Send + Sync + 'static,
    F: Fn(&ValueContext) -> T + Send + Sync + 'static,
{
    Arc::new(Derived::new(compute))
}

// ─────────────────────────────────────────────────────────────────────────────
// DynValue enum for common cases
// ─────────────────────────────────────────────────────────────────────────────

/// Dynamic value that can be static, signal, or spring
///
/// This is more efficient than BoxedValue for common cases because it avoids
/// the indirection of Arc<dyn Value<T>>.
#[derive(Clone)]
pub enum DynValue<T: Clone + Send + Sync + 'static> {
    /// Static value that never changes
    Static(T),
    /// Value from a reactive signal
    Signal { id: u64, default: T },
}

impl<T: Clone + Send + Sync + 'static> DynValue<T> {
    /// Resolve the current value
    pub fn get(&self, ctx: &ValueContext) -> T {
        match self {
            DynValue::Static(v) => v.clone(),
            DynValue::Signal { id, default } => ctx
                .reactive
                .get_signal_value_raw(*id)
                .and_then(|boxed| boxed.downcast::<T>().ok())
                .map(|b| *b)
                .unwrap_or_else(|| default.clone()),
        }
    }

    /// Check if this is a static value
    pub fn is_static(&self) -> bool {
        matches!(self, DynValue::Static(_))
    }
}

impl<T: Clone + Send + Sync + 'static> From<T> for DynValue<T> {
    fn from(value: T) -> Self {
        DynValue::Static(value)
    }
}

/// Dynamic f32 value that can also be a spring
#[derive(Clone)]
pub enum DynFloat {
    /// Static value
    Static(f32),
    /// Value from a reactive signal
    Signal { id: u64, default: f32 },
    /// Value from a spring animation
    Spring {
        id: u64,
        generation: u32,
        default: f32,
    },
}

impl DynFloat {
    /// Resolve the current value
    pub fn get(&self, ctx: &ValueContext) -> f32 {
        match self {
            DynFloat::Static(v) => *v,
            DynFloat::Signal { id, default } => ctx
                .reactive
                .get_signal_value_raw(*id)
                .and_then(|boxed| boxed.downcast::<f32>().ok())
                .map(|b| *b)
                .unwrap_or(*default),
            DynFloat::Spring {
                id,
                generation,
                default,
            } => ctx
                .animations
                .get_spring_value(*id, *generation)
                .unwrap_or(*default),
        }
    }

    /// Check if this is a static value
    pub fn is_static(&self) -> bool {
        matches!(self, DynFloat::Static(_))
    }
}

impl From<f32> for DynFloat {
    fn from(value: f32) -> Self {
        DynFloat::Static(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockReactive;
    impl ReactiveAccess for MockReactive {
        fn get_signal_value_raw(&self, _id: u64) -> Option<Box<dyn std::any::Any + Send>> {
            None
        }
    }

    struct MockAnimations;
    impl AnimationAccess for MockAnimations {
        fn get_spring_value(&self, _id: u64, _gen: u32) -> Option<f32> {
            Some(42.0)
        }
    }

    #[test]
    fn test_static_value() {
        let reactive = MockReactive;
        let animations = MockAnimations;
        let ctx = ValueContext {
            reactive: &reactive,
            animations: &animations,
        };

        let value = Static(100.0f32);
        assert_eq!(value.get(&ctx), 100.0);
        assert!(value.is_static());
    }

    #[test]
    fn test_spring_value() {
        let reactive = MockReactive;
        let animations = MockAnimations;
        let ctx = ValueContext {
            reactive: &reactive,
            animations: &animations,
        };

        let value = SpringValue::new(1, 1, 0.0);
        assert_eq!(value.get(&ctx), 42.0); // Mock returns 42.0
    }

    #[test]
    fn test_dyn_float() {
        let reactive = MockReactive;
        let animations = MockAnimations;
        let ctx = ValueContext {
            reactive: &reactive,
            animations: &animations,
        };

        let static_val = DynFloat::Static(10.0);
        assert_eq!(static_val.get(&ctx), 10.0);

        let spring_val = DynFloat::Spring {
            id: 1,
            generation: 1,
            default: 0.0,
        };
        assert_eq!(spring_val.get(&ctx), 42.0);
    }
}
