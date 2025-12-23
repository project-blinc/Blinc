//! GPUI-style div builder with tailwind-style methods
//!
//! Provides a fluent builder API for creating layout elements:
//! ```rust
//! use blinc_layout::prelude::*;
//! use blinc_core::Color;
//!
//! let ui = div()
//!     .flex_row()
//!     .gap(4.0)
//!     .p(2.0)
//!     .bg(Color::RED)
//!     .child(text("Hello"));
//! ```

use blinc_core::{Brush, Color, CornerRadius, Shadow, Transform};
use taffy::prelude::*;

use crate::element::{
    GlassMaterial, Material, MetallicMaterial, RenderLayer, RenderProps, WoodMaterial,
};
use crate::tree::{LayoutNodeId, LayoutTree};

/// A div element builder with GPUI/Tailwind-style methods
pub struct Div {
    style: Style,
    children: Vec<Box<dyn ElementBuilder>>,
    background: Option<Brush>,
    border_radius: CornerRadius,
    render_layer: RenderLayer,
    material: Option<Material>,
    shadow: Option<Shadow>,
    transform: Option<Transform>,
}

impl Default for Div {
    fn default() -> Self {
        Self::new()
    }
}

impl Div {
    /// Create a new div element
    pub fn new() -> Self {
        Self {
            style: Style::default(),
            children: Vec::new(),
            background: None,
            border_radius: CornerRadius::default(),
            render_layer: RenderLayer::default(),
            material: None,
            shadow: None,
            transform: None,
        }
    }

    // =========================================================================
    // Display & Flex Direction
    // =========================================================================

    /// Set display to flex (default)
    pub fn flex(mut self) -> Self {
        self.style.display = Display::Flex;
        self
    }

    /// Set display to block
    pub fn block(mut self) -> Self {
        self.style.display = Display::Block;
        self
    }

    /// Set display to grid
    pub fn grid(mut self) -> Self {
        self.style.display = Display::Grid;
        self
    }

    /// Set display to none
    pub fn hidden(mut self) -> Self {
        self.style.display = Display::None;
        self
    }

    /// Set flex direction to row (horizontal)
    pub fn flex_row(mut self) -> Self {
        self.style.display = Display::Flex;
        self.style.flex_direction = FlexDirection::Row;
        self
    }

    /// Set flex direction to column (vertical)
    pub fn flex_col(mut self) -> Self {
        self.style.display = Display::Flex;
        self.style.flex_direction = FlexDirection::Column;
        self
    }

    /// Set flex direction to row-reverse
    pub fn flex_row_reverse(mut self) -> Self {
        self.style.display = Display::Flex;
        self.style.flex_direction = FlexDirection::RowReverse;
        self
    }

    /// Set flex direction to column-reverse
    pub fn flex_col_reverse(mut self) -> Self {
        self.style.display = Display::Flex;
        self.style.flex_direction = FlexDirection::ColumnReverse;
        self
    }

    // =========================================================================
    // Flex Properties
    // =========================================================================

    /// Set flex-grow to 1 (element will grow to fill space)
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

    /// Set flex-basis to auto
    pub fn flex_auto(mut self) -> Self {
        self.style.flex_grow = 1.0;
        self.style.flex_shrink = 1.0;
        self.style.flex_basis = Dimension::Auto;
        self
    }

    /// Set flex: 1 1 0% (grow, shrink, basis 0)
    pub fn flex_1(mut self) -> Self {
        self.style.flex_grow = 1.0;
        self.style.flex_shrink = 1.0;
        self.style.flex_basis = Dimension::Length(0.0);
        self
    }

    /// Allow wrapping
    pub fn flex_wrap(mut self) -> Self {
        self.style.flex_wrap = FlexWrap::Wrap;
        self
    }

    // =========================================================================
    // Alignment & Justification
    // =========================================================================

    /// Center items both horizontally and vertically
    pub fn items_center(mut self) -> Self {
        self.style.align_items = Some(AlignItems::Center);
        self
    }

    /// Align items to start
    pub fn items_start(mut self) -> Self {
        self.style.align_items = Some(AlignItems::Start);
        self
    }

    /// Align items to end
    pub fn items_end(mut self) -> Self {
        self.style.align_items = Some(AlignItems::End);
        self
    }

    /// Stretch items to fill (default)
    pub fn items_stretch(mut self) -> Self {
        self.style.align_items = Some(AlignItems::Stretch);
        self
    }

    /// Align items to baseline
    pub fn items_baseline(mut self) -> Self {
        self.style.align_items = Some(AlignItems::Baseline);
        self
    }

    /// Justify content to start
    pub fn justify_start(mut self) -> Self {
        self.style.justify_content = Some(JustifyContent::Start);
        self
    }

    /// Justify content to center
    pub fn justify_center(mut self) -> Self {
        self.style.justify_content = Some(JustifyContent::Center);
        self
    }

    /// Justify content to end
    pub fn justify_end(mut self) -> Self {
        self.style.justify_content = Some(JustifyContent::End);
        self
    }

    /// Space between items
    pub fn justify_between(mut self) -> Self {
        self.style.justify_content = Some(JustifyContent::SpaceBetween);
        self
    }

    /// Space around items
    pub fn justify_around(mut self) -> Self {
        self.style.justify_content = Some(JustifyContent::SpaceAround);
        self
    }

    /// Space evenly between items
    pub fn justify_evenly(mut self) -> Self {
        self.style.justify_content = Some(JustifyContent::SpaceEvenly);
        self
    }

    // =========================================================================
    // Sizing (pixel values)
    // =========================================================================

    /// Set width in pixels
    pub fn w(mut self, px: f32) -> Self {
        self.style.size.width = Dimension::Length(px);
        self
    }

    /// Set width to 100%
    pub fn w_full(mut self) -> Self {
        self.style.size.width = Dimension::Percent(1.0);
        self
    }

    /// Set width to auto
    pub fn w_auto(mut self) -> Self {
        self.style.size.width = Dimension::Auto;
        self
    }

    /// Set width to fit content (shrink-wrap to children)
    ///
    /// This sets width to auto with flex_basis auto and prevents flex growing/shrinking,
    /// so the element will size exactly to fit its content.
    pub fn w_fit(mut self) -> Self {
        self.style.size.width = Dimension::Auto;
        self.style.flex_basis = Dimension::Auto;
        self.style.flex_grow = 0.0;
        self.style.flex_shrink = 0.0;
        self
    }

    /// Set height in pixels
    pub fn h(mut self, px: f32) -> Self {
        self.style.size.height = Dimension::Length(px);
        self
    }

    /// Set height to 100%
    pub fn h_full(mut self) -> Self {
        self.style.size.height = Dimension::Percent(1.0);
        self
    }

    /// Set height to auto
    pub fn h_auto(mut self) -> Self {
        self.style.size.height = Dimension::Auto;
        self
    }

    /// Set height to fit content (shrink-wrap to children)
    ///
    /// This sets height to auto and prevents flex growing/shrinking, so the element
    /// will size exactly to fit its content.
    pub fn h_fit(mut self) -> Self {
        self.style.size.height = Dimension::Auto;
        self.style.flex_basis = Dimension::Auto;
        self.style.flex_grow = 0.0;
        self.style.flex_shrink = 0.0;
        self
    }

    /// Set both width and height to fit content
    ///
    /// This makes the element shrink-wrap to its content in both dimensions.
    pub fn size_fit(mut self) -> Self {
        self.style.size.width = Dimension::Auto;
        self.style.size.height = Dimension::Auto;
        self.style.flex_basis = Dimension::Auto;
        self.style.flex_grow = 0.0;
        self.style.flex_shrink = 0.0;
        self
    }

    /// Set both width and height in pixels
    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.style.size.width = Dimension::Length(w);
        self.style.size.height = Dimension::Length(h);
        self
    }

    /// Set square size (width and height equal)
    pub fn square(mut self, size: f32) -> Self {
        self.style.size.width = Dimension::Length(size);
        self.style.size.height = Dimension::Length(size);
        self
    }

    /// Set min-width in pixels
    pub fn min_w(mut self, px: f32) -> Self {
        self.style.min_size.width = Dimension::Length(px);
        self
    }

    /// Set min-height in pixels
    pub fn min_h(mut self, px: f32) -> Self {
        self.style.min_size.height = Dimension::Length(px);
        self
    }

    /// Set max-width in pixels
    pub fn max_w(mut self, px: f32) -> Self {
        self.style.max_size.width = Dimension::Length(px);
        self
    }

    /// Set max-height in pixels
    pub fn max_h(mut self, px: f32) -> Self {
        self.style.max_size.height = Dimension::Length(px);
        self
    }

    // =========================================================================
    // Spacing (4px base unit like Tailwind)
    // =========================================================================

    /// Set gap between children (in 4px units)
    /// gap(4) = 16px
    pub fn gap(mut self, units: f32) -> Self {
        let px = units * 4.0;
        self.style.gap = taffy::Size {
            width: LengthPercentage::Length(px),
            height: LengthPercentage::Length(px),
        };
        self
    }

    /// Set gap in pixels directly
    pub fn gap_px(mut self, px: f32) -> Self {
        self.style.gap = taffy::Size {
            width: LengthPercentage::Length(px),
            height: LengthPercentage::Length(px),
        };
        self
    }

    /// Set column gap (horizontal spacing between items)
    pub fn gap_x(mut self, units: f32) -> Self {
        self.style.gap.width = LengthPercentage::Length(units * 4.0);
        self
    }

    /// Set row gap (vertical spacing between items)
    pub fn gap_y(mut self, units: f32) -> Self {
        self.style.gap.height = LengthPercentage::Length(units * 4.0);
        self
    }

    /// Set padding on all sides (in 4px units)
    /// p(4) = 16px padding
    pub fn p(mut self, units: f32) -> Self {
        let px = LengthPercentage::Length(units * 4.0);
        self.style.padding = Rect {
            left: px,
            right: px,
            top: px,
            bottom: px,
        };
        self
    }

    /// Set padding in pixels
    pub fn p_px(mut self, px: f32) -> Self {
        let val = LengthPercentage::Length(px);
        self.style.padding = Rect {
            left: val,
            right: val,
            top: val,
            bottom: val,
        };
        self
    }

    /// Set horizontal padding (in 4px units)
    pub fn px(mut self, units: f32) -> Self {
        let px = LengthPercentage::Length(units * 4.0);
        self.style.padding.left = px;
        self.style.padding.right = px;
        self
    }

    /// Set vertical padding (in 4px units)
    pub fn py(mut self, units: f32) -> Self {
        let px = LengthPercentage::Length(units * 4.0);
        self.style.padding.top = px;
        self.style.padding.bottom = px;
        self
    }

    /// Set left padding (in 4px units)
    pub fn pl(mut self, units: f32) -> Self {
        self.style.padding.left = LengthPercentage::Length(units * 4.0);
        self
    }

    /// Set right padding (in 4px units)
    pub fn pr(mut self, units: f32) -> Self {
        self.style.padding.right = LengthPercentage::Length(units * 4.0);
        self
    }

    /// Set top padding (in 4px units)
    pub fn pt(mut self, units: f32) -> Self {
        self.style.padding.top = LengthPercentage::Length(units * 4.0);
        self
    }

    /// Set bottom padding (in 4px units)
    pub fn pb(mut self, units: f32) -> Self {
        self.style.padding.bottom = LengthPercentage::Length(units * 4.0);
        self
    }

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

    /// Set margin in pixels
    pub fn m_px(mut self, px: f32) -> Self {
        let val = LengthPercentageAuto::Length(px);
        self.style.margin = Rect {
            left: val,
            right: val,
            top: val,
            bottom: val,
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

    /// Set auto horizontal margin (centering)
    pub fn mx_auto(mut self) -> Self {
        self.style.margin.left = LengthPercentageAuto::Auto;
        self.style.margin.right = LengthPercentageAuto::Auto;
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

    // =========================================================================
    // Position
    // =========================================================================

    /// Set position to absolute
    pub fn absolute(mut self) -> Self {
        self.style.position = Position::Absolute;
        self
    }

    /// Set position to relative (default)
    pub fn relative(mut self) -> Self {
        self.style.position = Position::Relative;
        self
    }

    /// Set inset (position from all edges)
    pub fn inset(mut self, px: f32) -> Self {
        let val = LengthPercentageAuto::Length(px);
        self.style.inset = Rect {
            left: val,
            right: val,
            top: val,
            bottom: val,
        };
        self
    }

    /// Set top position
    pub fn top(mut self, px: f32) -> Self {
        self.style.inset.top = LengthPercentageAuto::Length(px);
        self
    }

    /// Set bottom position
    pub fn bottom(mut self, px: f32) -> Self {
        self.style.inset.bottom = LengthPercentageAuto::Length(px);
        self
    }

    /// Set left position
    pub fn left(mut self, px: f32) -> Self {
        self.style.inset.left = LengthPercentageAuto::Length(px);
        self
    }

    /// Set right position
    pub fn right(mut self, px: f32) -> Self {
        self.style.inset.right = LengthPercentageAuto::Length(px);
        self
    }

    // =========================================================================
    // Visual Properties
    // =========================================================================

    /// Set background color
    pub fn bg(mut self, color: Color) -> Self {
        self.background = Some(Brush::Solid(color));
        self
    }

    /// Set background brush (for gradients)
    pub fn background(mut self, brush: impl Into<Brush>) -> Self {
        self.background = Some(brush.into());
        self
    }

    /// Set corner radius (all corners)
    pub fn rounded(mut self, radius: f32) -> Self {
        self.border_radius = CornerRadius::uniform(radius);
        self
    }

    /// Set corner radius with full pill shape (radius = min(w,h)/2)
    pub fn rounded_full(mut self) -> Self {
        // Use a large value; actual pill shape depends on element size
        self.border_radius = CornerRadius::uniform(9999.0);
        self
    }

    /// Set individual corner radii
    pub fn rounded_corners(mut self, tl: f32, tr: f32, br: f32, bl: f32) -> Self {
        self.border_radius = CornerRadius::new(tl, tr, br, bl);
        self
    }

    // =========================================================================
    // Layer (for rendering order)
    // =========================================================================

    /// Set the render layer
    pub fn layer(mut self, layer: RenderLayer) -> Self {
        self.render_layer = layer;
        self
    }

    /// Render in foreground (on top of glass)
    pub fn foreground(self) -> Self {
        self.layer(RenderLayer::Foreground)
    }

    // =========================================================================
    // Material System
    // =========================================================================

    /// Apply a material to this element
    pub fn material(mut self, material: Material) -> Self {
        // Glass materials also set the render layer to Glass
        if matches!(material, Material::Glass(_)) {
            self.render_layer = RenderLayer::Glass;
        }
        self.material = Some(material);
        self
    }

    /// Apply a visual effect to this element
    ///
    /// Effects include glass (blur), metallic (reflection), wood (texture), etc.
    /// This is the general-purpose method for applying any material effect.
    ///
    /// Example:
    /// ```ignore
    /// // Glass effect
    /// div().effect(GlassMaterial::thick().tint_rgba(1.0, 0.9, 0.9, 0.5))
    ///
    /// // Metallic effect
    /// div().effect(MetallicMaterial::chrome())
    /// ```
    pub fn effect(self, effect: impl Into<Material>) -> Self {
        self.material(effect.into())
    }

    /// Apply glass material with default settings (shorthand for common case)
    ///
    /// Creates a frosted glass effect that blurs content behind the element.
    pub fn glass(self) -> Self {
        self.material(Material::Glass(GlassMaterial::new()))
    }

    /// Apply metallic material with default settings
    pub fn metallic(self) -> Self {
        self.material(Material::Metallic(MetallicMaterial::new()))
    }

    /// Apply chrome metallic preset
    pub fn chrome(self) -> Self {
        self.material(Material::Metallic(MetallicMaterial::chrome()))
    }

    /// Apply gold metallic preset
    pub fn gold(self) -> Self {
        self.material(Material::Metallic(MetallicMaterial::gold()))
    }

    /// Apply wood material with default settings
    pub fn wood(self) -> Self {
        self.material(Material::Wood(WoodMaterial::new()))
    }

    // =========================================================================
    // Shadow
    // =========================================================================

    /// Apply a drop shadow to this element
    pub fn shadow(mut self, shadow: Shadow) -> Self {
        self.shadow = Some(shadow);
        self
    }

    /// Apply a drop shadow with the given parameters
    pub fn shadow_params(
        self,
        offset_x: f32,
        offset_y: f32,
        blur: f32,
        color: Color,
    ) -> Self {
        self.shadow(Shadow::new(offset_x, offset_y, blur, color))
    }

    /// Apply a small drop shadow (2px offset, 4px blur)
    pub fn shadow_sm(self) -> Self {
        self.shadow(Shadow::new(0.0, 2.0, 4.0, Color::rgba(0.0, 0.0, 0.0, 0.1)))
    }

    /// Apply a medium drop shadow (4px offset, 8px blur)
    pub fn shadow_md(self) -> Self {
        self.shadow(Shadow::new(0.0, 4.0, 8.0, Color::rgba(0.0, 0.0, 0.0, 0.15)))
    }

    /// Apply a large drop shadow (8px offset, 16px blur)
    pub fn shadow_lg(self) -> Self {
        self.shadow(Shadow::new(0.0, 8.0, 16.0, Color::rgba(0.0, 0.0, 0.0, 0.2)))
    }

    /// Apply an extra large drop shadow (12px offset, 24px blur)
    pub fn shadow_xl(self) -> Self {
        self.shadow(Shadow::new(0.0, 12.0, 24.0, Color::rgba(0.0, 0.0, 0.0, 0.25)))
    }

    // =========================================================================
    // Transform
    // =========================================================================

    /// Apply a transform to this element
    pub fn transform(mut self, transform: Transform) -> Self {
        self.transform = Some(transform);
        self
    }

    /// Translate this element by the given x and y offset
    pub fn translate(self, x: f32, y: f32) -> Self {
        self.transform(Transform::translate(x, y))
    }

    /// Scale this element uniformly
    pub fn scale(self, factor: f32) -> Self {
        self.transform(Transform::scale(factor, factor))
    }

    /// Scale this element with different x and y factors
    pub fn scale_xy(self, sx: f32, sy: f32) -> Self {
        self.transform(Transform::scale(sx, sy))
    }

    /// Rotate this element by the given angle in radians
    pub fn rotate(self, angle: f32) -> Self {
        self.transform(Transform::rotate(angle))
    }

    /// Rotate this element by the given angle in degrees
    pub fn rotate_deg(self, degrees: f32) -> Self {
        self.rotate(degrees * std::f32::consts::PI / 180.0)
    }

    // =========================================================================
    // Children
    // =========================================================================

    /// Add a child element
    pub fn child(mut self, child: impl ElementBuilder + 'static) -> Self {
        self.children.push(Box::new(child));
        self
    }

    /// Add multiple children
    pub fn children<I>(mut self, children: I) -> Self
    where
        I: IntoIterator,
        I::Item: ElementBuilder + 'static,
    {
        for child in children {
            self.children.push(Box::new(child));
        }
        self
    }

    /// Get direct access to the taffy style for advanced configuration
    pub fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

/// Element type identifier for downcasting
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElementTypeId {
    Div,
    Text,
    Svg,
}

/// Text alignment options
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextAlign {
    /// Align text to the left (default)
    #[default]
    Left,
    /// Center text
    Center,
    /// Align text to the right
    Right,
}

/// Font weight options
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FontWeight {
    /// Thin (100)
    Thin,
    /// Extra Light (200)
    ExtraLight,
    /// Light (300)
    Light,
    /// Normal/Regular (400)
    #[default]
    Normal,
    /// Medium (500)
    Medium,
    /// Semi Bold (600)
    SemiBold,
    /// Bold (700)
    Bold,
    /// Extra Bold (800)
    ExtraBold,
    /// Black (900)
    Black,
}

impl FontWeight {
    /// Get the numeric weight value (100-900)
    pub fn weight(&self) -> u16 {
        match self {
            FontWeight::Thin => 100,
            FontWeight::ExtraLight => 200,
            FontWeight::Light => 300,
            FontWeight::Normal => 400,
            FontWeight::Medium => 500,
            FontWeight::SemiBold => 600,
            FontWeight::Bold => 700,
            FontWeight::ExtraBold => 800,
            FontWeight::Black => 900,
        }
    }
}

/// Text render data extracted from element
#[derive(Clone)]
pub struct TextRenderInfo {
    pub content: String,
    pub font_size: f32,
    pub color: [f32; 4],
    pub align: TextAlign,
    pub weight: FontWeight,
}

/// SVG render data extracted from element
#[derive(Clone)]
pub struct SvgRenderInfo {
    pub source: String,
    pub tint: Option<blinc_core::Color>,
}

/// Trait for types that can build into layout elements
pub trait ElementBuilder: Send {
    /// Build this element into a layout tree, returning the node ID
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId;

    /// Get the render properties for this element
    fn render_props(&self) -> RenderProps;

    /// Get children builders (for recursive traversal)
    fn children_builders(&self) -> &[Box<dyn ElementBuilder>];

    /// Get the element type identifier
    fn element_type_id(&self) -> ElementTypeId {
        ElementTypeId::Div
    }

    /// Get text render info if this is a text element
    fn text_render_info(&self) -> Option<TextRenderInfo> {
        None
    }

    /// Get SVG render info if this is an SVG element
    fn svg_render_info(&self) -> Option<SvgRenderInfo> {
        None
    }
}

impl ElementBuilder for Div {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        let node = tree.create_node(self.style.clone());

        // Build and add children
        for child in &self.children {
            let child_node = child.build(tree);
            tree.add_child(node, child_node);
        }

        node
    }

    fn render_props(&self) -> RenderProps {
        RenderProps {
            background: self.background.clone(),
            border_radius: self.border_radius,
            layer: self.render_layer,
            material: self.material.clone(),
            node_id: None,
            shadow: self.shadow,
            transform: self.transform.clone(),
        }
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        &self.children
    }
}

/// Convenience function to create a new div
pub fn div() -> Div {
    Div::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::RenderTree;

    #[test]
    fn test_div_builder() {
        let d = div().w(100.0).h(50.0).flex_row().gap(2.0).p(4.0);

        assert!(matches!(d.style.display, Display::Flex));
        assert!(matches!(d.style.flex_direction, FlexDirection::Row));
    }

    #[test]
    fn test_div_with_children() {
        let parent = div().flex_col().child(div().h(20.0)).child(div().h(30.0));

        assert_eq!(parent.children.len(), 2);
    }

    #[test]
    fn test_build_tree() {
        let ui = div().flex_col().child(div().h(20.0)).child(div().h(30.0));

        let mut tree = LayoutTree::new();
        let root = ui.build(&mut tree);

        assert_eq!(tree.len(), 3);
        assert_eq!(tree.children(root).len(), 2);
    }

    #[test]
    fn test_layout_flex_row_with_fixed_children() {
        // Three fixed-width children in a row
        let ui = div()
            .w(300.0)
            .h(100.0)
            .flex_row()
            .child(div().w(50.0).h(100.0))
            .child(div().w(100.0).h(100.0))
            .child(div().w(50.0).h(100.0));

        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(300.0, 100.0);

        let root = tree.root().unwrap();
        let children: Vec<_> = tree.layout_tree.children(root);

        // First child at x=0
        let first = tree
            .layout_tree
            .get_bounds(children[0], (0.0, 0.0))
            .unwrap();
        assert_eq!(first.x, 0.0);
        assert_eq!(first.width, 50.0);

        // Second child at x=50
        let second = tree
            .layout_tree
            .get_bounds(children[1], (0.0, 0.0))
            .unwrap();
        assert_eq!(second.x, 50.0);
        assert_eq!(second.width, 100.0);

        // Third child at x=150
        let third = tree
            .layout_tree
            .get_bounds(children[2], (0.0, 0.0))
            .unwrap();
        assert_eq!(third.x, 150.0);
        assert_eq!(third.width, 50.0);
    }

    #[test]
    fn test_layout_flex_col_with_gap() {
        // Column with gap between children (10px gap using gap_px)
        let ui = div()
            .w(100.0)
            .h(200.0)
            .flex_col()
            .gap_px(10.0) // 10px gap
            .child(div().w_full().h(40.0))
            .child(div().w_full().h(40.0))
            .child(div().w_full().h(40.0));

        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(100.0, 200.0);

        let root = tree.root().unwrap();
        let children: Vec<_> = tree.layout_tree.children(root);

        // First child at y=0
        let first = tree
            .layout_tree
            .get_bounds(children[0], (0.0, 0.0))
            .unwrap();
        assert_eq!(first.y, 0.0);
        assert_eq!(first.height, 40.0);

        // Second child at y=50 (40 + 10 gap)
        let second = tree
            .layout_tree
            .get_bounds(children[1], (0.0, 0.0))
            .unwrap();
        assert_eq!(second.y, 50.0);
        assert_eq!(second.height, 40.0);

        // Third child at y=100 (50 + 40 + 10 gap)
        let third = tree
            .layout_tree
            .get_bounds(children[2], (0.0, 0.0))
            .unwrap();
        assert_eq!(third.y, 100.0);
        assert_eq!(third.height, 40.0);
    }

    #[test]
    fn test_layout_flex_grow() {
        // One fixed child, one growing child
        let ui = div()
            .w(200.0)
            .h(100.0)
            .flex_row()
            .child(div().w(50.0).h(100.0))
            .child(div().flex_grow().h(100.0));

        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(200.0, 100.0);

        let root = tree.root().unwrap();
        let children: Vec<_> = tree.layout_tree.children(root);

        // Fixed child
        let fixed = tree
            .layout_tree
            .get_bounds(children[0], (0.0, 0.0))
            .unwrap();
        assert_eq!(fixed.width, 50.0);

        // Growing child should fill remaining space
        let growing = tree
            .layout_tree
            .get_bounds(children[1], (0.0, 0.0))
            .unwrap();
        assert_eq!(growing.x, 50.0);
        assert_eq!(growing.width, 150.0);
    }

    #[test]
    fn test_layout_padding() {
        // Container with padding
        let ui = div()
            .w(100.0)
            .h(100.0)
            .p(2.0) // 8px padding (2 * 4px base unit)
            .child(div().w_full().h_full());

        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(100.0, 100.0);

        let root = tree.root().unwrap();
        let children: Vec<_> = tree.layout_tree.children(root);

        // Child should be inset by padding
        let child = tree
            .layout_tree
            .get_bounds(children[0], (0.0, 0.0))
            .unwrap();
        assert_eq!(child.x, 8.0);
        assert_eq!(child.y, 8.0);
        assert_eq!(child.width, 84.0); // 100 - 8 - 8
        assert_eq!(child.height, 84.0);
    }

    #[test]
    fn test_layout_justify_between() {
        // Three children with space between
        let ui = div()
            .w(200.0)
            .h(50.0)
            .flex_row()
            .justify_between()
            .child(div().w(30.0).h(50.0))
            .child(div().w(30.0).h(50.0))
            .child(div().w(30.0).h(50.0));

        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(200.0, 50.0);

        let root = tree.root().unwrap();
        let children: Vec<_> = tree.layout_tree.children(root);

        // First at start
        let first = tree
            .layout_tree
            .get_bounds(children[0], (0.0, 0.0))
            .unwrap();
        assert_eq!(first.x, 0.0);

        // Last at end
        let third = tree
            .layout_tree
            .get_bounds(children[2], (0.0, 0.0))
            .unwrap();
        assert_eq!(third.x, 170.0); // 200 - 30

        // Middle should be centered between first and third
        let second = tree
            .layout_tree
            .get_bounds(children[1], (0.0, 0.0))
            .unwrap();
        assert_eq!(second.x, 85.0); // (170 - 0) / 2 - 30/2 + 30/2 = 85
    }

    #[test]
    fn test_nested_layout() {
        // Nested flex containers
        let ui = div()
            .w(200.0)
            .h(200.0)
            .flex_col()
            .child(
                div()
                    .w_full()
                    .h(50.0)
                    .flex_row()
                    .child(div().w(50.0).h(50.0))
                    .child(div().flex_grow().h(50.0)),
            )
            .child(div().w_full().flex_grow());

        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(200.0, 200.0);

        let root = tree.root().unwrap();
        let root_bounds = tree.get_bounds(root).unwrap();
        assert_eq!(root_bounds.width, 200.0);
        assert_eq!(root_bounds.height, 200.0);

        let root_children: Vec<_> = tree.layout_tree.children(root);

        // First row
        let row = tree
            .layout_tree
            .get_bounds(root_children[0], (0.0, 0.0))
            .unwrap();
        assert_eq!(row.height, 50.0);
        assert_eq!(row.width, 200.0);

        // Second element should fill remaining height
        let bottom = tree
            .layout_tree
            .get_bounds(root_children[1], (0.0, 0.0))
            .unwrap();
        assert_eq!(bottom.y, 50.0);
        assert_eq!(bottom.height, 150.0);
    }
}
