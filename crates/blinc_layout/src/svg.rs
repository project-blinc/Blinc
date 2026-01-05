//! SVG element builder
//!
//! Provides a builder for SVG elements that participate in layout:
//! ```rust
//! use blinc_layout::prelude::*;
//! use blinc_core::Color;
//!
//! let icon = svg("<svg></svg>")
//!     .size(32.0, 32.0)
//!     .color(Color::WHITE);
//! ```

use blinc_core::{Color, Shadow, Transform};
use taffy::prelude::*;

use crate::div::{ElementBuilder, ElementTypeId, SvgRenderInfo};
use crate::element::{RenderLayer, RenderProps};
use crate::tree::{LayoutNodeId, LayoutTree};

/// An SVG element builder
pub struct Svg {
    /// The SVG source string
    source: String,
    /// Width in pixels
    width: f32,
    /// Height in pixels
    height: f32,
    /// Optional tint color (replaces fill/stroke colors)
    tint: Option<Color>,
    /// Taffy style for layout
    style: Style,
    /// Render layer
    render_layer: RenderLayer,
    /// Drop shadow
    shadow: Option<Shadow>,
    /// Transform
    transform: Option<Transform>,
}

impl Svg {
    /// Create a new SVG element from source string
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            width: 24.0,
            height: 24.0,
            tint: None,
            style: Style {
                size: taffy::Size {
                    width: Dimension::Length(24.0),
                    height: Dimension::Length(24.0),
                },
                ..Default::default()
            },
            render_layer: RenderLayer::default(),
            shadow: None,
            transform: None,
        }
    }

    /// Set the size (width and height)
    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.width = width;
        self.height = height;
        self.style.size.width = Dimension::Length(width);
        self.style.size.height = Dimension::Length(height);
        self
    }

    /// Set square size (width = height)
    pub fn square(mut self, size: f32) -> Self {
        self.width = size;
        self.height = size;
        self.style.size.width = Dimension::Length(size);
        self.style.size.height = Dimension::Length(size);
        self
    }

    /// Set tint color (replaces SVG fill/stroke colors)
    pub fn tint(mut self, color: Color) -> Self {
        self.tint = Some(color);
        self
    }

    /// Alias for tint - set the color
    pub fn color(self, color: Color) -> Self {
        self.tint(color)
    }

    /// Set the render layer
    pub fn layer(mut self, layer: RenderLayer) -> Self {
        self.render_layer = layer;
        self
    }

    /// Render in foreground (on top of glass)
    pub fn foreground(self) -> Self {
        self.layer(RenderLayer::Foreground)
    }

    /// Get the SVG source
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Get the width
    pub fn width(&self) -> f32 {
        self.width
    }

    /// Get the height
    pub fn height(&self) -> f32 {
        self.height
    }

    /// Get the tint color
    pub fn tint_color(&self) -> Option<Color> {
        self.tint
    }

    // =========================================================================
    // Layout properties (delegate to style)
    // =========================================================================

    /// Set margin on all sides (in 4px units)
    pub fn m(mut self, units: f32) -> Self {
        let px = LengthPercentageAuto::Length(units * 4.0);
        self.style.margin = Rect {
            left: px,
            right: px,
            top: px,
            bottom: px,
        };
        self
    }

    /// Set horizontal margin (in 4px units)
    pub fn mx(mut self, units: f32) -> Self {
        let px = LengthPercentageAuto::Length(units * 4.0);
        self.style.margin.left = px;
        self.style.margin.right = px;
        self
    }

    /// Set vertical margin (in 4px units)
    pub fn my(mut self, units: f32) -> Self {
        let px = LengthPercentageAuto::Length(units * 4.0);
        self.style.margin.top = px;
        self.style.margin.bottom = px;
        self
    }

    /// Set left margin (in 4px units)
    pub fn ml(mut self, units: f32) -> Self {
        self.style.margin.left = LengthPercentageAuto::Length(units * 4.0);
        self
    }

    /// Set right margin (in 4px units)
    pub fn mr(mut self, units: f32) -> Self {
        self.style.margin.right = LengthPercentageAuto::Length(units * 4.0);
        self
    }

    /// Set top margin (in 4px units)
    pub fn mt(mut self, units: f32) -> Self {
        self.style.margin.top = LengthPercentageAuto::Length(units * 4.0);
        self
    }

    /// Set bottom margin (in 4px units)
    pub fn mb(mut self, units: f32) -> Self {
        self.style.margin.bottom = LengthPercentageAuto::Length(units * 4.0);
        self
    }

    /// Set flex-grow
    pub fn flex_grow(mut self) -> Self {
        self.style.flex_grow = 1.0;
        self
    }

    /// Set flex-shrink to 1 (element will shrink if needed)
    pub fn flex_shrink(mut self) -> Self {
        self.style.flex_shrink = 1.0;
        self
    }

    /// Set flex-shrink to 0 (element won't shrink)
    pub fn flex_shrink_0(mut self) -> Self {
        self.style.flex_shrink = 0.0;
        self
    }

    /// Align self center
    pub fn self_center(mut self) -> Self {
        self.style.align_self = Some(AlignSelf::Center);
        self
    }

    // =========================================================================
    // Shadow
    // =========================================================================

    /// Apply a drop shadow to this SVG
    pub fn shadow(mut self, shadow: Shadow) -> Self {
        self.shadow = Some(shadow);
        self
    }

    /// Apply a drop shadow with the given parameters
    pub fn shadow_params(self, offset_x: f32, offset_y: f32, blur: f32, color: Color) -> Self {
        self.shadow(Shadow::new(offset_x, offset_y, blur, color))
    }

    // =========================================================================
    // Transform
    // =========================================================================

    /// Apply a transform to this SVG
    pub fn transform(mut self, transform: Transform) -> Self {
        self.transform = Some(transform);
        self
    }

    /// Translate this SVG by the given x and y offset
    pub fn translate(self, x: f32, y: f32) -> Self {
        self.transform(Transform::translate(x, y))
    }

    /// Scale this SVG uniformly
    pub fn scale(self, factor: f32) -> Self {
        self.transform(Transform::scale(factor, factor))
    }

    /// Rotate this SVG by the given angle in radians
    pub fn rotate(self, angle: f32) -> Self {
        self.transform(Transform::rotate(angle))
    }
}

impl ElementBuilder for Svg {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        tree.create_node(self.style.clone())
    }

    fn render_props(&self) -> RenderProps {
        RenderProps {
            background: None,
            border_radius: Default::default(),
            border_color: None,
            border_width: 0.0,
            border_sides: Default::default(),
            layer: self.render_layer,
            material: None,
            node_id: None,
            shadow: self.shadow,
            transform: self.transform.clone(),
            opacity: 1.0,
            clips_content: false,
            motion: None,
            motion_stable_id: None,
            motion_should_replay: false,
            is_stack_layer: false,
            pointer_events_none: false,
            cursor: None,
        }
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        &[] // SVG has no children
    }

    fn element_type_id(&self) -> ElementTypeId {
        ElementTypeId::Svg
    }

    fn svg_render_info(&self) -> Option<SvgRenderInfo> {
        Some(SvgRenderInfo {
            source: self.source.clone(),
            tint: self.tint,
        })
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        Some(&self.style)
    }
}

/// Convenience function to create a new SVG element
pub fn svg(source: impl Into<String>) -> Svg {
    Svg::new(source)
}

/// SVG element with render data for the renderer
#[derive(Clone)]
pub struct SvgRenderData {
    /// The SVG source string
    pub source: String,
    /// Width in pixels
    pub width: f32,
    /// Height in pixels
    pub height: f32,
    /// Optional tint color
    pub tint: Option<[f32; 4]>,
}

impl Svg {
    /// Get render data for this SVG element
    pub fn render_data(&self) -> SvgRenderData {
        SvgRenderData {
            source: self.source.clone(),
            width: self.width,
            height: self.height,
            tint: self.tint.map(|c| [c.r, c.g, c.b, c.a]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_svg_builder() {
        let s = svg("<svg></svg>").size(48.0, 48.0).tint(Color::WHITE);

        assert_eq!(s.width(), 48.0);
        assert_eq!(s.height(), 48.0);
        assert!(s.tint_color().is_some());
    }

    #[test]
    fn test_svg_build() {
        let s = svg("<svg></svg>");

        let mut tree = LayoutTree::new();
        let _node = s.build(&mut tree);

        assert_eq!(tree.len(), 1);
    }
}
