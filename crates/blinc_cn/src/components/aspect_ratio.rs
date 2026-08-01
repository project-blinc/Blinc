//! Aspect Ratio component - container that maintains a specific aspect ratio
//!
//! A container element that maintains a specific width-to-height ratio regardless
//! of the content inside. Useful for images, videos, cards, and responsive layouts.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! // 16:9 video container
//! cn::aspect_ratio(16.0 / 9.0)
//!     .w(640.0)
//!     .child(video_element)
//!
//! // Square container
//! cn::aspect_ratio_square()
//!     .w(200.0)
//!     .child(profile_image)
//!
//! // 4:3 photo container
//! cn::aspect_ratio_4_3()
//!     .w(400.0)
//!     .child(photo)
//!
//! // Using preset ratios
//! cn::aspect_ratio(AspectRatioPreset::Widescreen.ratio())
//!     .w(800.0)
//!     .child(content)
//! ```

use std::cell::{OnceCell, RefCell};

use blinc_core::Color;
use blinc_layout::element::RenderProps;
use blinc_layout::prelude::*;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};

/// Common aspect ratio presets
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AspectRatioPreset {
    /// 1:1 square ratio
    Square,
    /// 16:9 widescreen (common for videos)
    Widescreen,
    /// 4:3 traditional (old TV/photos)
    Traditional,
    /// 21:9 ultrawide (cinema)
    Ultrawide,
    /// 3:2 classic photography
    Photo,
    /// 2:3 portrait photography
    Portrait,
    /// 9:16 vertical video (stories, reels)
    Vertical,
    /// 3:4 portrait traditional
    PortraitTraditional,
}

impl AspectRatioPreset {
    /// Get the numeric ratio (width / height)
    pub fn ratio(&self) -> f32 {
        match self {
            AspectRatioPreset::Square => 1.0,
            AspectRatioPreset::Widescreen => 16.0 / 9.0,
            AspectRatioPreset::Traditional => 4.0 / 3.0,
            AspectRatioPreset::Ultrawide => 21.0 / 9.0,
            AspectRatioPreset::Photo => 3.0 / 2.0,
            AspectRatioPreset::Portrait => 2.0 / 3.0,
            AspectRatioPreset::Vertical => 9.0 / 16.0,
            AspectRatioPreset::PortraitTraditional => 3.0 / 4.0,
        }
    }
}

/// Configuration for aspect ratio container
struct AspectRatioConfig {
    /// The aspect ratio (width / height)
    ratio: f32,
    /// Fixed width (height will be calculated)
    width: Option<f32>,
    /// Fixed height (width will be calculated)
    height: Option<f32>,
    /// Background color
    background: Option<Color>,
    /// Corner radius
    corner_radius: Option<f32>,
    /// Content child
    content: Option<Box<dyn ElementBuilder>>,
}

impl Default for AspectRatioConfig {
    fn default() -> Self {
        Self {
            ratio: 1.0, // Square by default
            width: None,
            height: None,
            background: None,
            corner_radius: None,
            content: None,
        }
    }
}

/// Built aspect ratio container
struct BuiltAspectRatio {
    inner: Div,
}

impl BuiltAspectRatio {
    fn from_config(config: AspectRatioConfig) -> Self {
        // Calculate dimensions based on what's provided
        let (final_width, final_height) = match (config.width, config.height) {
            // Both provided - use width and ignore height (width takes priority)
            (Some(w), Some(_)) => (w, w / config.ratio),
            // Only width provided - calculate height
            (Some(w), None) => (w, w / config.ratio),
            // Only height provided - calculate width
            (None, Some(h)) => (h * config.ratio, h),
            // Neither provided - use default width
            (None, None) => {
                let default_width = 200.0;
                (default_width, default_width / config.ratio)
            }
        };

        // Build outer container with fixed dimensions
        let mut container = div()
            .class("cn-aspect-ratio")
            .w(final_width)
            .h(final_height)
            .overflow_clip();

        // Apply background if set
        if let Some(bg) = config.background {
            container = container.bg(bg);
        }

        // Apply corner radius if set
        if let Some(radius) = config.corner_radius {
            container = container.rounded(radius);
        }

        // Add content if provided
        if let Some(content) = config.content {
            // Content wrapper that fills the container with absolute positioning
            let content_wrapper = div()
                .absolute()
                .left(0.0)
                .top(0.0)
                .right(0.0)
                .bottom(0.0)
                .overflow_clip()
                .child_box(content);

            // Position the container as relative for absolute positioning to work
            container = container.relative().child(content_wrapper);
        }

        Self { inner: container }
    }
}

/// Aspect ratio container component
pub struct AspectRatio {
    inner: Div,
}

impl AspectRatio {
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

impl ElementBuilder for AspectRatio {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.inner.build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.inner.children_builders()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.inner.element_classes()
    }
}

/// Builder for aspect ratio container
pub struct AspectRatioBuilder {
    config: RefCell<AspectRatioConfig>,
    built: OnceCell<AspectRatio>,
}

impl AspectRatioBuilder {
    /// Create a new aspect ratio builder with the given ratio
    pub fn new(ratio: f32) -> Self {
        Self {
            config: RefCell::new(AspectRatioConfig {
                ratio: ratio.max(0.01), // Prevent zero/negative ratios
                ..Default::default()
            }),
            built: OnceCell::new(),
        }
    }

    /// Create from a preset ratio
    pub fn from_preset(preset: AspectRatioPreset) -> Self {
        Self::new(preset.ratio())
    }

    fn get_or_build(&self) -> &AspectRatio {
        ::blinc_layout::build_once::build_once(&self.built, || {
            // Take ownership of config, replacing with default
            let config = self.config.take();
            let built = BuiltAspectRatio::from_config(config);
            AspectRatio { inner: built.inner }
        })
    }

    /// Set the aspect ratio (width / height)
    pub fn ratio(self, ratio: f32) -> Self {
        self.config.borrow_mut().ratio = ratio.max(0.01);
        self
    }

    /// Set width (height will be calculated from ratio)
    pub fn w(self, width: f32) -> Self {
        self.config.borrow_mut().width = Some(width);
        self
    }

    /// Set height (width will be calculated from ratio)
    pub fn h(self, height: f32) -> Self {
        self.config.borrow_mut().height = Some(height);
        self
    }

    /// Set background color
    pub fn bg(self, color: impl Into<Color>) -> Self {
        self.config.borrow_mut().background = Some(color.into());
        self
    }

    /// Set corner radius
    pub fn rounded(self, radius: f32) -> Self {
        self.config.borrow_mut().corner_radius = Some(radius);
        self
    }

    /// Set the child content
    pub fn child(self, content: impl ElementBuilder + 'static) -> Self {
        self.config.borrow_mut().content = Some(Box::new(content));
        self
    }

    /// Build the final AspectRatio component
    pub fn build_final(self) -> AspectRatio {
        let config = self.config.into_inner();
        let built = BuiltAspectRatio::from_config(config);
        AspectRatio { inner: built.inner }
    }
}

impl ElementBuilder for AspectRatioBuilder {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.get_or_build().build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.get_or_build().render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().children_builders()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.get_or_build().element_classes()
    }
}

/// Create an aspect ratio container with the given ratio
///
/// The ratio is width / height. For example:
/// - 16:9 ratio = 16.0 / 9.0 ≈ 1.78
/// - 4:3 ratio = 4.0 / 3.0 ≈ 1.33
/// - 1:1 ratio = 1.0 (square)
///
/// # Example
///
/// ```ignore
/// use blinc_cn::prelude::*;
///
/// // 16:9 video container
/// cn::aspect_ratio(16.0 / 9.0)
///     .w(640.0)
///     .child(video)
///
/// // Square container
/// cn::aspect_ratio(1.0)
///     .w(200.0)
///     .child(avatar)
/// ```
pub fn aspect_ratio(ratio: f32) -> AspectRatioBuilder {
    AspectRatioBuilder::new(ratio)
}

/// Create a square (1:1) aspect ratio container
///
/// # Example
///
/// ```ignore
/// cn::aspect_ratio_square()
///     .w(200.0)
///     .child(avatar_image)
/// ```
pub fn aspect_ratio_square() -> AspectRatioBuilder {
    AspectRatioBuilder::from_preset(AspectRatioPreset::Square)
}

/// Create a 16:9 widescreen aspect ratio container
///
/// Common for videos and modern displays.
///
/// # Example
///
/// ```ignore
/// cn::aspect_ratio_16_9()
///     .w(640.0)
///     .child(video_player)
/// ```
pub fn aspect_ratio_16_9() -> AspectRatioBuilder {
    AspectRatioBuilder::from_preset(AspectRatioPreset::Widescreen)
}

/// Create a 4:3 traditional aspect ratio container
///
/// Common for older photos and displays.
///
/// # Example
///
/// ```ignore
/// cn::aspect_ratio_4_3()
///     .w(400.0)
///     .child(photo)
/// ```
pub fn aspect_ratio_4_3() -> AspectRatioBuilder {
    AspectRatioBuilder::from_preset(AspectRatioPreset::Traditional)
}

/// Create a 21:9 ultrawide aspect ratio container
///
/// Common for cinematic content.
///
/// # Example
///
/// ```ignore
/// cn::aspect_ratio_21_9()
///     .w(840.0)
///     .child(cinema_frame)
/// ```
pub fn aspect_ratio_21_9() -> AspectRatioBuilder {
    AspectRatioBuilder::from_preset(AspectRatioPreset::Ultrawide)
}

/// Create a 9:16 vertical aspect ratio container
///
/// Common for mobile content like stories and reels.
///
/// # Example
///
/// ```ignore
/// cn::aspect_ratio_9_16()
///     .w(360.0)
///     .child(story_content)
/// ```
pub fn aspect_ratio_9_16() -> AspectRatioBuilder {
    AspectRatioBuilder::from_preset(AspectRatioPreset::Vertical)
}

#[cfg(test)]
mod tests {
    use super::*;
    use blinc_theme::ThemeState;

    fn init_theme() {
        let _ = ThemeState::try_get().unwrap_or_else(|| {
            ThemeState::init_default();
            ThemeState::get()
        });
    }

    #[test]
    fn test_aspect_ratio_presets() {
        assert_eq!(AspectRatioPreset::Square.ratio(), 1.0);
        assert!((AspectRatioPreset::Widescreen.ratio() - 16.0 / 9.0).abs() < 0.001);
        assert!((AspectRatioPreset::Traditional.ratio() - 4.0 / 3.0).abs() < 0.001);
        assert!((AspectRatioPreset::Vertical.ratio() - 9.0 / 16.0).abs() < 0.001);
    }

    #[test]
    fn test_aspect_ratio_dimensions_from_width() {
        init_theme();

        // 16:9 ratio with 640px width should give ~360px height
        let ratio: f32 = 16.0 / 9.0;
        let width: f32 = 640.0;
        let expected_height = width / ratio;
        assert!((expected_height - 360.0).abs() < 0.1);
    }

    #[test]
    fn test_aspect_ratio_dimensions_from_height() {
        init_theme();

        // 16:9 ratio with 360px height should give ~640px width
        let ratio: f32 = 16.0 / 9.0;
        let height: f32 = 360.0;
        let expected_width = height * ratio;
        assert!((expected_width - 640.0).abs() < 0.1);
    }

    #[test]
    fn test_aspect_ratio_builder() {
        init_theme();

        let builder = aspect_ratio(4.0 / 3.0).w(400.0).rounded(8.0);

        let config = builder.config.borrow();
        assert!((config.ratio - 4.0 / 3.0).abs() < 0.001);
        assert_eq!(config.width, Some(400.0));
        assert_eq!(config.corner_radius, Some(8.0));
    }

    #[test]
    fn test_square_aspect_ratio() {
        init_theme();

        let builder = aspect_ratio_square().w(200.0);

        let config = builder.config.borrow();
        assert_eq!(config.ratio, 1.0);
        assert_eq!(config.width, Some(200.0));
    }
}
