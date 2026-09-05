//! Interaction: pointer events, cursor, blend mode, and masking.

use blinc_core::{BlendMode, Color, PointerEvents};

use crate::element_style::*;
use crate::material::CursorStyle;

impl ElementStyle {
    // =========================================================================
    // Interaction Properties
    // =========================================================================

    /// Set pointer-events behavior
    pub fn pointer_events(mut self, pe: PointerEvents) -> Self {
        self.pointer_events = Some(pe);
        self
    }

    /// Set pointer-events: none
    pub fn pointer_events_none(mut self) -> Self {
        self.pointer_events = Some(PointerEvents::None);
        self
    }

    /// Set cursor style
    pub fn cursor(mut self, cursor: CursorStyle) -> Self {
        self.cursor = Some(cursor);
        self
    }

    /// Set mix-blend-mode
    pub fn mix_blend_mode(mut self, mode: BlendMode) -> Self {
        self.mix_blend_mode = Some(mode);
        self
    }

    /// Set text-decoration-color
    pub fn text_decoration_color(mut self, color: Color) -> Self {
        self.text_decoration_color = Some(color);
        self
    }

    /// Set text-decoration-thickness
    pub fn text_decoration_thickness(mut self, thickness: f32) -> Self {
        self.text_decoration_thickness = Some(thickness);
        self
    }

    /// Set text-overflow
    pub fn text_overflow(mut self, overflow: TextOverflow) -> Self {
        self.text_overflow = Some(overflow);
        self
    }

    /// Set white-space
    pub fn white_space(mut self, ws: WhiteSpace) -> Self {
        self.white_space = Some(ws);
        self
    }

    /// Set mask-image URL
    pub fn mask_image(mut self, url: impl Into<String>) -> Self {
        self.mask_image = Some(blinc_core::MaskImage::Url(url.into()));
        self
    }

    /// Set mask-image gradient
    pub fn mask_gradient(mut self, gradient: blinc_core::Gradient) -> Self {
        self.mask_image = Some(blinc_core::MaskImage::Gradient(gradient));
        self
    }

    /// Set mask-mode
    pub fn mask_mode(mut self, mode: blinc_core::MaskMode) -> Self {
        self.mask_mode = Some(mode);
        self
    }

    /// Set @flow shader reference by name
    pub fn flow(mut self, name: impl Into<String>) -> Self {
        self.flow = Some(name.into());
        self
    }
}
