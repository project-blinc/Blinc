//! Pointer-space configuration.
//!
//! Describes how an element's `env(pointer-x)` and `env(pointer-y)` readings
//! are normalized: which coordinate space they sample, where the origin
//! sits, the output range, and how heavily the reading is smoothed. The live
//! tracking state that consumes this config stays in the layout crate.

/// Coordinate space for pointer tracking
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PointerSpace {
    /// Relative to the element itself
    #[default]
    SelfSpace,
    /// Relative to the parent element
    Parent,
    /// Relative to the viewport
    Viewport,
}

/// Origin point for coordinate normalization
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PointerOrigin {
    /// (0,0) at center, range extends symmetrically
    #[default]
    Center,
    /// (0,0) at top-left corner
    TopLeft,
    /// (0,0) at bottom-left (Y-up, like shader coordinates)
    BottomLeft,
}

/// Configuration for pointer tracking on an element
#[derive(Clone, Debug, PartialEq)]
pub struct PointerSpaceConfig {
    /// Coordinate space
    pub space: PointerSpace,
    /// Origin point
    pub origin: PointerOrigin,
    /// Output range (min, max) — default (-1.0, 1.0)
    pub range: (f32, f32),
    /// Smoothing time constant in seconds (0 = no smoothing)
    pub smoothing: f32,
}

impl Default for PointerSpaceConfig {
    fn default() -> Self {
        Self {
            space: PointerSpace::SelfSpace,
            origin: PointerOrigin::Center,
            range: (-1.0, 1.0),
            smoothing: 0.0,
        }
    }
}
