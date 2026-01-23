//! Blinc Animation System
//!
//! Spring physics, keyframe animations, and timeline orchestration.
//!
//! # Features
//!
//! - **Spring Physics**: RK4-integrated springs with stiffness, damping, mass
//! - **Keyframe Animations**: Timed sequences with easing functions
//! - **Multi-Property Keyframes**: Animate multiple properties simultaneously
//! - **Timelines**: Orchestrate multiple animations with offsets
//! - **Typed Animations**: Generic animations for Vec3, Color, and custom types
//! - **Interruptible**: Animations inherit velocity when interrupted
//! - **Animation Presets**: Common entry/exit animations
//! - **AnimationContext**: Platform-agnostic animation management trait

pub mod context;
pub mod easing;
pub mod keyframe;
pub mod presets;
pub mod scheduler;
pub mod spring;
pub mod timeline;
pub mod values;

pub use context::{
    AnimationContext, AnimationContextExt, SharedAnimatedTimeline, SharedAnimatedValue,
};
pub use easing::Easing;
pub use keyframe::{
    FillMode, Keyframe, KeyframeAnimation, KeyframePoint, KeyframeProperties, KeyframeTrack,
    KeyframeTrackBuilder, MultiKeyframe, MultiKeyframeAnimation, PlayDirection,
};
pub use presets::AnimationPreset;
pub use scheduler::{
    get_scheduler, is_scheduler_initialized, set_global_scheduler, try_get_scheduler,
    AnimatedKeyframe, AnimatedTimeline, AnimatedValue, AnimationScheduler, ConfigureResult,
    KeyframeId, SchedulerHandle, SpringId, TickCallback, TickCallbackId, TimelineId,
};
pub use spring::{Spring, SpringConfig};
pub use timeline::{StaggerBuilder, Timeline, TimelineEntryId};
pub use values::{
    ColorAnimation, ColorKeyframe, FloatAnimation, FloatKeyframe, Interpolate,
    SphericalInterpolate, TypedKeyframe, TypedKeyframeAnimation, Vec3Animation, Vec3Keyframe,
};
