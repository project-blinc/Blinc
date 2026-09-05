//! CSS filter functions, and the theme shadow lookup.
//!
//! `CssFilter` holds one field per filter function at its identity value,
//! so an unset filter costs nothing to apply.

use blinc_core::Shadow;

/// CSS filter functions applied to an element
///
/// Each field corresponds to a CSS filter function.
/// Default/identity values: grayscale=0, invert=0, sepia=0, hue_rotate=0,
/// brightness=1, contrast=1, saturate=1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CssFilter {
    /// Grayscale amount (0.0 = none, 1.0 = full grayscale)
    pub grayscale: f32,
    /// Invert amount (0.0 = none, 1.0 = fully inverted)
    pub invert: f32,
    /// Sepia amount (0.0 = none, 1.0 = full sepia)
    pub sepia: f32,
    /// Hue rotation in degrees
    pub hue_rotate: f32,
    /// Brightness multiplier (1.0 = normal)
    pub brightness: f32,
    /// Contrast multiplier (1.0 = normal)
    pub contrast: f32,
    /// Saturation multiplier (1.0 = normal)
    pub saturate: f32,
    /// Blur radius in pixels (0.0 = no blur)
    pub blur: f32,
    /// Drop shadow (offset, blur, color) — rendered as LayerEffect
    pub drop_shadow: Option<Shadow>,
}

impl Default for CssFilter {
    fn default() -> Self {
        Self {
            grayscale: 0.0,
            invert: 0.0,
            sepia: 0.0,
            hue_rotate: 0.0,
            brightness: 1.0,
            contrast: 1.0,
            saturate: 1.0,
            blur: 0.0,
            drop_shadow: None,
        }
    }
}

impl CssFilter {
    /// Returns true if all filter values are at identity (no effect)
    pub fn is_identity(&self) -> bool {
        self.grayscale == 0.0
            && self.invert == 0.0
            && self.sepia == 0.0
            && self.hue_rotate == 0.0
            && self.brightness == 1.0
            && self.contrast == 1.0
            && self.saturate == 1.0
            && self.blur == 0.0
            && self.drop_shadow.is_none()
    }
}
