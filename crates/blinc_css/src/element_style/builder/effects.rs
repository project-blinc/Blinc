//! Animation, transition, and filter.

use crate::element_style::*;

use crate::parser::{CssAnimation, CssTransitionSet};

impl ElementStyle {
    // =========================================================================
    // Transition
    // =========================================================================

    /// Set CSS transition configuration
    pub fn transition(mut self, t: CssTransitionSet) -> Self {
        self.transition = Some(t);
        self
    }

    // =========================================================================
    // Filter
    // =========================================================================

    /// Set CSS filter
    pub fn filter(mut self, f: CssFilter) -> Self {
        self.filter = Some(f);
        self
    }

    // =========================================================================
    // Animation
    // =========================================================================

    /// Set CSS animation
    pub fn animation(mut self, animation: CssAnimation) -> Self {
        self.animation = Some(animation);
        self
    }

    /// Set animation by name (requires stylesheet lookup later)
    pub fn animation_name(mut self, name: impl Into<String>) -> Self {
        let mut anim = self.animation.take().unwrap_or_default();
        anim.name = name.into();
        self.animation = Some(anim);
        self
    }

    /// Set animation duration in milliseconds
    pub fn animation_duration(mut self, duration_ms: u32) -> Self {
        let mut anim = self.animation.take().unwrap_or_default();
        anim.duration_ms = duration_ms;
        self.animation = Some(anim);
        self
    }
}
