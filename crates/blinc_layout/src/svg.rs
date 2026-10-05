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

use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use blinc_core::{Color, Shadow, Transform};
use taffy::prelude::*;

use crate::div::{ElementBuilder, ElementTypeId, SvgRenderInfo};
use crate::element::{RenderLayer, RenderProps};
use crate::tree::{LayoutNodeId, LayoutTree};

// ---------------------------------------------------------------------------
// SVG string interning
// ---------------------------------------------------------------------------

/// Maximum number of interned SVG source strings.
/// When exceeded, entries only held by the cache (strong_count == 1) are evicted.
const SVG_INTERN_CAP: usize = 512;

thread_local! {
    static SVG_INTERN: RefCell<HashMap<u64, Arc<str>>> =
        RefCell::new(HashMap::with_capacity(64));
}

/// Intern an SVG source string: if the same content was seen before, return the
/// cached `Arc<str>` and drop the input.  Otherwise store and return a new Arc.
fn intern_svg_source(s: String) -> Arc<str> {
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    let hash = hasher.finish();

    SVG_INTERN.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(cached) = cache.get(&hash) {
            return Arc::clone(cached);
        }
        // Evict dead entries when full (strong_count == 1 ⇒ only cache holds it)
        if cache.len() >= SVG_INTERN_CAP {
            cache.retain(|_, v| Arc::strong_count(v) > 1);
            if cache.len() >= SVG_INTERN_CAP {
                cache.clear();
            }
        }
        let arc: Arc<str> = Arc::from(s.as_str());
        cache.insert(hash, Arc::clone(&arc));
        arc
    })
}

/// An SVG element builder
pub struct Svg {
    /// The SVG source string (Arc for O(1) cloning through the render pipeline)
    source: Arc<str>,
    /// Width in pixels
    width: f32,
    /// Height in pixels
    height: f32,
    /// Optional tint color (replaces fill/stroke colors)
    tint: Option<Color>,
    /// Fill color override
    fill_color: Option<Color>,
    /// Stroke color override
    stroke_color: Option<Color>,
    /// Stroke width override
    stroke_w: Option<f32>,
    /// Taffy style for layout
    style: Style,
    /// Render layer
    render_layer: RenderLayer,
    /// Drop shadow stack
    shadow: Vec<Shadow>,
    /// Transform
    transform: Option<Transform>,
    /// Element ID for CSS selector targeting
    element_id: Option<String>,
    /// CSS class names for selector matching
    classes: Vec<std::sync::Arc<str>>,
    /// Internal SVGs (widget checkmarks, icons) don't match type selectors
    is_internal: bool,
}

impl Svg {
    /// Create a new SVG element from source string
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: intern_svg_source(source.into()),
            width: 24.0,
            height: 24.0,
            tint: None,
            fill_color: None,
            stroke_color: None,
            stroke_w: None,
            style: Style {
                size: taffy::Size {
                    width: Dimension::length(24.0),
                    height: Dimension::length(24.0),
                },
                ..Default::default()
            },
            render_layer: RenderLayer::default(),
            shadow: Vec::new(),
            transform: None,
            element_id: None,
            classes: Vec::new(),
            is_internal: false,
        }
    }

    /// Set the size (width and height)
    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.width = width;
        self.height = height;
        self.style.size.width = Dimension::length(width);
        self.style.size.height = Dimension::length(height);
        self
    }

    /// Set square size (width = height)
    pub fn square(mut self, size: f32) -> Self {
        self.width = size;
        self.height = size;
        self.style.size.width = Dimension::length(size);
        self.style.size.height = Dimension::length(size);
        self
    }

    /// Set tint color (replaces SVG fill/stroke colors)
    pub fn tint(mut self, color: Color) -> Self {
        self.tint = Some(color);
        self
    }

    /// Set the color (resolves `currentColor` in SVG)
    ///
    /// Maps to the CSS `color` property. During rasterization, any `currentColor`
    /// references in the SVG source are replaced with this value.
    pub fn color(self, color: Color) -> Self {
        self.tint(color)
    }

    /// Set the fill color (overrides SVG fill without affecting stroke)
    pub fn fill(mut self, color: Color) -> Self {
        self.fill_color = Some(color);
        self
    }

    /// Set the stroke color (overrides SVG stroke without affecting fill)
    pub fn stroke(mut self, color: Color) -> Self {
        self.stroke_color = Some(color);
        self
    }

    /// Set the stroke width in pixels
    pub fn stroke_width(mut self, width: f32) -> Self {
        self.stroke_w = Some(width);
        self
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

    /// Set the element ID for CSS selector targeting
    pub fn id(mut self, id: &str) -> Self {
        self.element_id = Some(id.to_string());
        self
    }

    /// Add a CSS class name for selector matching
    pub fn class(mut self, name: impl AsRef<str>) -> Self {
        self.classes.push(blinc_core::intern::intern(name.as_ref()));
        self
    }

    // =========================================================================
    // Layout properties (delegate to style)
    // =========================================================================

    /// Set margin on all sides (in 4px units)
    pub fn m(mut self, units: f32) -> Self {
        let px = LengthPercentageAuto::length(units * 4.0);
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
        let px = LengthPercentageAuto::length(units * 4.0);
        self.style.margin.left = px;
        self.style.margin.right = px;
        self
    }

    /// Set vertical margin (in 4px units)
    pub fn my(mut self, units: f32) -> Self {
        let px = LengthPercentageAuto::length(units * 4.0);
        self.style.margin.top = px;
        self.style.margin.bottom = px;
        self
    }

    /// Set left margin (in 4px units)
    pub fn ml(mut self, units: f32) -> Self {
        self.style.margin.left = LengthPercentageAuto::length(units * 4.0);
        self
    }

    /// Set right margin (in 4px units)
    pub fn mr(mut self, units: f32) -> Self {
        self.style.margin.right = LengthPercentageAuto::length(units * 4.0);
        self
    }

    /// Set top margin (in 4px units)
    pub fn mt(mut self, units: f32) -> Self {
        self.style.margin.top = LengthPercentageAuto::length(units * 4.0);
        self
    }

    /// Set bottom margin (in 4px units)
    pub fn mb(mut self, units: f32) -> Self {
        self.style.margin.bottom = LengthPercentageAuto::length(units * 4.0);
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
        self.style.align_self = Some(AlignSelf::CENTER);
        self
    }

    // =========================================================================
    // Shadow
    // =========================================================================

    /// Apply a single drop shadow to this SVG (replaces any existing stack).
    pub fn shadow(mut self, shadow: Shadow) -> Self {
        self.shadow = vec![shadow];
        self
    }

    /// Apply a compound drop shadow stack to this SVG.
    pub fn shadow_stack(mut self, shadows: Vec<Shadow>) -> Self {
        self.shadow = shadows;
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

    /// Mark as internal widget SVG (won't match `svg { }` type selectors)
    pub fn internal(mut self) -> Self {
        self.is_internal = true;
        self
    }
}

impl ElementBuilder for Svg {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        tree.create_node(self.style.clone())
    }

    #[allow(deprecated)]
    fn render_props(&self) -> RenderProps {
        RenderProps {
            layer: self.render_layer,
            shadow: self.shadow.clone(),
            transform: self.transform.clone(),
            ..Default::default()
        }
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        &[] // SVG has no children
    }

    fn element_type_id(&self) -> ElementTypeId {
        ElementTypeId::Svg
    }

    fn semantic_type_name(&self) -> Option<&'static str> {
        if self.is_internal { None } else { Some("svg") }
    }

    fn element_id(&self) -> Option<&str> {
        self.element_id.as_deref()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        &self.classes
    }

    fn svg_render_info(&self) -> Option<SvgRenderInfo> {
        Some(SvgRenderInfo {
            source: self.source.clone(),
            tint: self.tint,
            fill: self.fill_color,
            stroke: self.stroke_color,
            stroke_width: self.stroke_w,
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
    pub source: Arc<str>,
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
