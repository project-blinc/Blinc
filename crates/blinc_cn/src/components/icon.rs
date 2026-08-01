//! Icon component for Lucide icons
//!
//! Renders Lucide icons from blinc_icons with theming support.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//! use blinc_icons::icons;
//!
//! // Basic icon
//! cn::icon(icons::CHECK)
//!
//! // Sized icon
//! cn::icon(icons::ARROW_RIGHT).size(IconSize::Large)
//!
//! // Colored icon
//! cn::icon(icons::SETTINGS).color(ColorToken::Primary)
//!
//! // Custom size in pixels
//! cn::icon(icons::SEARCH).size_px(32.0)
//! ```

use std::cell::OnceCell;
use std::ops::{Deref, DerefMut};

use blinc_core::Color;
use blinc_layout::div::{Div, ElementBuilder, ElementTypeId};
use blinc_layout::element::RenderProps;
use blinc_layout::prelude::*;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_theme::{ColorToken, ThemeState};

/// Icon size presets
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum IconSize {
    /// 12px
    ExtraSmall,
    /// 16px
    Small,
    /// 20px
    #[default]
    Medium,
    /// 24px (native Lucide size)
    Large,
    /// 32px
    ExtraLarge,
}

impl IconSize {
    /// Get the pixel size for this preset
    pub fn pixels(&self) -> f32 {
        match self {
            IconSize::ExtraSmall => 12.0,
            IconSize::Small => 16.0,
            IconSize::Medium => 20.0,
            IconSize::Large => 24.0,
            IconSize::ExtraLarge => 32.0,
        }
    }
}

/// Icon component for Lucide icons
///
/// Implements `Deref` to `Div` for full customization.
pub struct Icon {
    inner: Div,
}

impl Icon {
    /// Build from configuration
    fn from_config(config: &IconConfig) -> Self {
        let theme = ThemeState::get();
        let size = config.size_px.unwrap_or_else(|| config.size.pixels());
        let stroke_width = config.stroke_width.unwrap_or(blinc_icons::STROKE_WIDTH);

        let color = config
            .color
            .or_else(|| config.color_token.map(|t| theme.color(t)))
            .unwrap_or_else(|| theme.color(ColorToken::TextPrimary));

        // Generate SVG string
        let svg_str = blinc_icons::to_svg_with_stroke(config.path_data, size, stroke_width);

        let inner = div()
            .class("cn-icon")
            .w(size)
            .h(size)
            .flex()
            .items_center()
            .justify_center()
            .child(svg(&svg_str).size(size, size).color(color));

        Icon { inner }
    }

    /// Add a CSS class for selector matching
    pub fn class(mut self, name: impl AsRef<str>) -> Self {
        self.inner = self.inner.class(name);
        self
    }

    /// Set the element ID for CSS selector matching
    pub fn id(mut self, id: &str) -> Self {
        self.inner = self.inner.id(id);
        self
    }
}

impl ElementBuilder for Icon {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.inner.build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.inner.children_builders()
    }

    fn element_type_id(&self) -> ElementTypeId {
        self.inner.element_type_id()
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.inner.layout_style()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.inner.element_classes()
    }
}

impl Deref for Icon {
    type Target = Div;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for Icon {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

/// Internal configuration for IconBuilder
#[derive(Clone, Copy)]
struct IconConfig {
    path_data: &'static str,
    size: IconSize,
    size_px: Option<f32>,
    color: Option<Color>,
    color_token: Option<ColorToken>,
    stroke_width: Option<f32>,
}

/// Builder for Icon component
///
/// Uses lazy initialization via `OnceCell` to build the Icon on first access.
pub struct IconBuilder {
    config: IconConfig,
    built: OnceCell<Icon>,
}

impl IconBuilder {
    /// Create a new icon builder from path data
    ///
    /// # Arguments
    /// * `path_data` - SVG inner content from `blinc_icons::icons::*`
    pub fn new(path_data: &'static str) -> Self {
        Self {
            config: IconConfig {
                path_data,
                size: IconSize::default(),
                size_px: None,
                color: None,
                color_token: None,
                stroke_width: None,
            },
            built: OnceCell::new(),
        }
    }

    /// Get or lazily build the Icon
    fn get_or_build(&self) -> &Icon {
        ::blinc_layout::build_once::build_once(&self.built, || Icon::from_config(&self.config))
    }

    /// Set icon size preset
    pub fn size(mut self, size: IconSize) -> Self {
        self.config.size = size;
        self
    }

    /// Set icon size in pixels (overrides preset)
    pub fn size_px(mut self, px: f32) -> Self {
        self.config.size_px = Some(px);
        self
    }

    /// Set color from theme token
    pub fn color(mut self, token: ColorToken) -> Self {
        self.config.color_token = Some(token);
        self
    }

    /// Set color directly
    pub fn color_value(mut self, color: Color) -> Self {
        self.config.color = Some(color);
        self
    }

    /// Set stroke width (default is 2.0)
    pub fn stroke_width(mut self, width: f32) -> Self {
        self.config.stroke_width = Some(width);
        self
    }

    /// Build the icon (for explicit building if needed)
    pub fn build(self) -> Icon {
        Icon::from_config(&self.config)
    }
}

impl ElementBuilder for IconBuilder {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.get_or_build().build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.get_or_build().render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().children_builders()
    }

    fn element_type_id(&self) -> ElementTypeId {
        self.get_or_build().element_type_id()
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.get_or_build().layout_style()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.get_or_build().element_classes()
    }
}

/// Create an icon from Lucide path data
///
/// # Example
///
/// ```ignore
/// use blinc_cn::prelude::*;
/// use blinc_icons::icons;
///
/// cn::icon(icons::CHECK)
///     .size(IconSize::Large)
///     .color(ColorToken::Primary)
/// ```
pub fn icon(path_data: &'static str) -> IconBuilder {
    IconBuilder::new(path_data)
}
