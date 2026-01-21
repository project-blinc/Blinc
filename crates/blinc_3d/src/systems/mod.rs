//! Built-in systems
//!
//! Provides common systems for transform propagation, visibility culling,
//! and animation synchronization.

mod animation;
mod transform;
mod visibility;

pub use animation::{
    AnimationSyncSystem, ColorAnimation, DeltaTime, Easing, FloatAnimation, FrameCount,
    Interpolate, QuatAnimation, QuatKeyframe, SphericalInterpolate, TotalTime, TypedKeyframe,
    TypedKeyframeAnimation, Vec3Animation,
};
pub use transform::TransformSystem;
pub use visibility::VisibilitySystem;
