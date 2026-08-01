//! Avatar component - user avatar with image, fallback initials, and status indicator
//!
//! A themed avatar component that displays user images with automatic fallback
//! to initials when no image is provided or when the image fails to load.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! // Avatar with image
//! cn::avatar()
//!     .src("https://example.com/avatar.jpg")
//!     .alt("John Doe")
//!
//! // Avatar with fallback initials
//! cn::avatar()
//!     .fallback("JD")
//!
//! // Avatar with status indicator
//! cn::avatar()
//!     .src("https://example.com/avatar.jpg")
//!     .status(AvatarStatus::Online)
//!
//! // Different sizes
//! cn::avatar()
//!     .size(AvatarSize::Large)
//!     .fallback("AB")
//!
//! // Square avatar
//! cn::avatar()
//!     .shape(AvatarShape::Square)
//!     .fallback("CD")
//! ```

use std::cell::{OnceCell, RefCell};

use blinc_core::Color;
use blinc_layout::element::RenderProps;
use blinc_layout::prelude::*;
use blinc_layout::text::Text as TextElement;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_theme::{ColorToken, RadiusToken, ThemeState};

/// Avatar size variants
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AvatarSize {
    /// Extra small - 24px
    ExtraSmall,
    /// Small - 32px
    Small,
    /// Medium - 40px (default)
    #[default]
    Medium,
    /// Large - 48px
    Large,
    /// Extra large - 64px
    ExtraLarge,
}

impl AvatarSize {
    /// Get the size in pixels
    pub fn pixels(&self) -> f32 {
        match self {
            AvatarSize::ExtraSmall => 24.0,
            AvatarSize::Small => 32.0,
            AvatarSize::Medium => 40.0,
            AvatarSize::Large => 48.0,
            AvatarSize::ExtraLarge => 64.0,
        }
    }

    /// Get the font size for fallback initials
    fn font_size(&self) -> f32 {
        match self {
            AvatarSize::ExtraSmall => 10.0,
            AvatarSize::Small => 12.0,
            AvatarSize::Medium => 14.0,
            AvatarSize::Large => 18.0,
            AvatarSize::ExtraLarge => 24.0,
        }
    }

    /// Get the status indicator size
    fn status_size(&self) -> f32 {
        match self {
            AvatarSize::ExtraSmall => 6.0,
            AvatarSize::Small => 8.0,
            AvatarSize::Medium => 10.0,
            AvatarSize::Large => 12.0,
            AvatarSize::ExtraLarge => 14.0,
        }
    }

    /// Get the status indicator offset from edge
    fn status_offset(&self) -> f32 {
        match self {
            AvatarSize::ExtraSmall => 0.0,
            AvatarSize::Small => 0.0,
            AvatarSize::Medium => 1.0,
            AvatarSize::Large => 2.0,
            AvatarSize::ExtraLarge => 3.0,
        }
    }
}

/// Avatar shape variants
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AvatarShape {
    /// Circular avatar (default)
    #[default]
    Circle,
    /// Rounded square avatar
    Square,
}

impl AvatarShape {
    /// Get the border radius for this shape
    fn border_radius(&self, size: f32, theme: &ThemeState) -> f32 {
        match self {
            AvatarShape::Circle => size / 2.0,
            AvatarShape::Square => theme.radius(RadiusToken::Md),
        }
    }
}

/// Avatar status indicator
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AvatarStatus {
    /// Online - green indicator
    Online,
    /// Offline - gray indicator
    Offline,
    /// Away - yellow indicator
    Away,
    /// Busy/Do not disturb - red indicator
    Busy,
}

impl AvatarStatus {
    /// Get the status indicator color
    fn color(&self, theme: &ThemeState) -> Color {
        match self {
            AvatarStatus::Online => theme.color(ColorToken::Success),
            AvatarStatus::Offline => theme.color(ColorToken::TextTertiary),
            AvatarStatus::Away => theme.color(ColorToken::Warning),
            AvatarStatus::Busy => theme.color(ColorToken::Error),
        }
    }
}

/// Configuration for avatar component
#[derive(Default)]
struct AvatarConfig {
    /// Image source URL
    src: Option<blinc_layout::binding::Reactive<String>>,
    /// Alt text for accessibility
    alt: Option<String>,
    /// Fallback text (initials) when no image
    fallback: Option<blinc_layout::binding::Reactive<String>>,
    /// Avatar size
    size: AvatarSize,
    /// Avatar shape
    shape: AvatarShape,
    /// Status indicator
    status: Option<AvatarStatus>,
    /// Custom background color for fallback
    fallback_bg: Option<Color>,
    /// Custom text color for fallback
    fallback_color: Option<Color>,
    /// Optional separator ring drawn on the avatar's own outer div
    /// (NOT a wrapper). Used by `AvatarGroupBuilder` to give each
    /// avatar a single-primitive border that anti-aliases cleanly,
    /// the same way the spinner track does — no nested wrapper, no
    /// co-located AA edges.
    ring: Option<(Color, f32)>,
    /// User-added CSS classes
    classes: Vec<std::sync::Arc<str>>,
    /// User-set element ID
    user_id: Option<String>,
}

/// Built avatar component
struct BuiltAvatar {
    inner: Box<dyn ElementBuilder>,
}

impl BuiltAvatar {
    fn from_config(config: AvatarConfig) -> Self {
        let theme = ThemeState::get();
        let size_px = config.size.pixels();
        let radius = config.shape.border_radius(size_px, theme);

        // Determine background and content
        let (background, content) = if let Some(ref src) = config.src {
            // Image avatar. An image source has no property writer
            // either, so a bound one rebuilds the same way text does.
            let node = crate::reactive_props::reactive_node(src, move |s| {
                Box::new(img(s).size(size_px, size_px).cover().rounded(radius))
            });
            (None, AvatarContent::Node(node))
        } else if let Some(ref fallback_text) = config.fallback {
            // Fallback initials
            let bg = config
                .fallback_bg
                .unwrap_or_else(|| theme.color(ColorToken::Surface));
            let fg = config
                .fallback_color
                .unwrap_or_else(|| theme.color(ColorToken::TextPrimary));

            // The style closure sets everything: rebuilt content is
            // created after the stylesheet pass, so it inherits nothing.
            let font_size = config.size.font_size();
            let initials = crate::reactive_props::reactive_text(fallback_text, move |t| {
                t.size(font_size)
                    .weight(FontWeight::Medium)
                    .color(fg)
                    .no_wrap()
            });

            (Some(bg), AvatarContent::Node(initials))
        } else {
            // Empty fallback - show placeholder
            let bg = config
                .fallback_bg
                .unwrap_or_else(|| theme.color(ColorToken::Surface));
            let fg = config
                .fallback_color
                .unwrap_or_else(|| theme.color(ColorToken::TextTertiary));

            // Default user icon placeholder
            let placeholder = text("?")
                .size(config.size.font_size())
                .weight(FontWeight::Medium)
                .color(fg)
                .no_wrap();

            (Some(bg), AvatarContent::Node(Box::new(placeholder)))
        };

        // Build the inner avatar container (with clipping for image/initials).
        // When `config.ring` is set (the AvatarGroup case), we add the
        // border directly to this div so the WHOLE avatar — bg, border,
        // rounded corners — emits as a SINGLE SDF primitive. That's the
        // same single-primitive recipe spinner_track / switch thumb /
        // slider thumb use, and it's why their edges are smooth: one
        // primitive means one AA gradient at the curve, no co-located
        // doubling against a wrapper.
        let mut inner = div()
            .class("cn-avatar")
            .w(size_px)
            .h(size_px)
            .rounded(radius)
            .overflow_clip()
            .flex_row()
            .items_center()
            .justify_center();

        if config.shape == AvatarShape::Square {
            inner = inner.class("cn-avatar--square");
        }

        // Apply background if needed (for fallback)
        if let Some(bg) = background {
            inner = inner.bg(bg);
        }

        // Apply the separator ring directly on this div. The renderer
        // merges uniform border + bg into a single primitive via
        // `fill_rect_with_per_side_border`, so the SDF AA gradient at
        // the rounded edge handles border + bg together — no second
        // primitive, no co-located AA.
        if let Some((color, width)) = config.ring {
            inner = inner.border(width, color);
        }

        // Add content
        match content {
            AvatarContent::Node(node) => {
                inner = inner.child_box(node);
            }
        }

        // Apply user classes and id
        for c in &config.classes {
            inner = inner.class(c);
        }
        if let Some(ref id) = config.user_id {
            inner = inner.id(id);
        }

        // If we have a status indicator, use foreground layer to render on top of images
        let container = if let Some(status) = config.status {
            let status_size = config.size.status_size();
            let status_offset = config.size.status_offset();
            // Status pip is a plain themed dot — colour pulls from
            // the theme's semantic tokens (Success / Warning / Error
            // / TextTertiary). No ring, no shadow: the renderer's
            // SDF corner pipeline already produces a clean anti-
            // aliased circle, and avoiding the extra border-stroke
            // outline keeps the pip uncluttered at small sizes.
            let status_color = status.color(theme);

            let status_indicator = div()
                .class("cn-avatar-status")
                .w(status_size)
                .h(status_size)
                .rounded_full()
                .bg(status_color)
                .absolute()
                .bottom(status_offset)
                .right(status_offset);

            Box::new(
                stack()
                    .w(size_px)
                    .h(size_px)
                    .overflow_visible()
                    .child(inner)
                    .child(status_indicator),
            ) as Box<dyn ElementBuilder>
        } else {
            Box::new(inner) as Box<dyn ElementBuilder>
        };

        Self { inner: container }
    }
}

/// Avatar content type
enum AvatarContent {
    /// Boxed rather than `Image` / `TextElement`: a bound `src` or
    /// `fallback` hands back a `Stateful` wrapper, not the leaf.
    Node(Box<dyn ElementBuilder>),
}

/// Avatar component
pub struct Avatar {
    inner: Box<dyn ElementBuilder>,
}

impl ElementBuilder for Avatar {
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

    fn element_id(&self) -> Option<&str> {
        self.inner.element_id()
    }
}

/// Builder for avatar component
pub struct AvatarBuilder {
    config: RefCell<AvatarConfig>,
    built: OnceCell<Avatar>,
}

impl AvatarBuilder {
    /// Create a new avatar builder
    pub fn new() -> Self {
        Self {
            config: RefCell::new(AvatarConfig::default()),
            built: OnceCell::new(),
        }
    }

    fn get_or_build(&self) -> &Avatar {
        ::blinc_layout::build_once::build_once(&self.built, || {
            let config = self.config.take();
            let built = BuiltAvatar::from_config(config);
            Avatar { inner: built.inner }
        })
    }

    /// Set the image source URL
    pub fn src(self, src: impl blinc_layout::binding::IntoReactive<String>) -> Self {
        self.config.borrow_mut().src = Some(src.into_reactive());
        self
    }

    /// Set alt text for accessibility
    pub fn alt(self, alt: impl Into<String>) -> Self {
        self.config.borrow_mut().alt = Some(alt.into());
        self
    }

    /// Set fallback text (initials) when no image
    pub fn fallback(self, text: impl blinc_layout::binding::IntoReactive<String>) -> Self {
        self.config.borrow_mut().fallback = Some(text.into_reactive());
        self
    }

    /// Set avatar size
    pub fn size(self, size: AvatarSize) -> Self {
        self.config.borrow_mut().size = size;
        self
    }

    /// Set avatar shape
    pub fn shape(self, shape: AvatarShape) -> Self {
        self.config.borrow_mut().shape = shape;
        self
    }

    /// Set status indicator
    pub fn status(self, status: AvatarStatus) -> Self {
        self.config.borrow_mut().status = Some(status);
        self
    }

    /// Set custom background color for fallback
    pub fn fallback_bg(self, color: impl Into<Color>) -> Self {
        self.config.borrow_mut().fallback_bg = Some(color.into());
        self
    }

    /// Set custom text color for fallback
    pub fn fallback_color(self, color: impl Into<Color>) -> Self {
        self.config.borrow_mut().fallback_color = Some(color.into());
        self
    }

    /// Add a separator ring directly on the avatar's own outer div
    /// (used by `AvatarGroupBuilder` to give each avatar a single-
    /// primitive border that anti-aliases cleanly). The border merges
    /// with the avatar's bg into one SDF primitive — same recipe the
    /// spinner track uses — so the edge stays smooth at any radius.
    pub fn ring(self, color: impl Into<Color>, width: f32) -> Self {
        self.config.borrow_mut().ring = Some((color.into(), width));
        self
    }

    /// Add a CSS class for selector matching
    pub fn class(self, name: impl AsRef<str>) -> Self {
        self.config
            .borrow_mut()
            .classes
            .push(blinc_core::intern::intern(name.as_ref()));
        self
    }

    /// Set the element ID for CSS selector matching
    pub fn id(self, id: &str) -> Self {
        self.config.borrow_mut().user_id = Some(id.to_string());
        self
    }

    // Convenience size methods

    /// Set size to extra small (24px)
    pub fn xs(self) -> Self {
        self.size(AvatarSize::ExtraSmall)
    }

    /// Set size to small (32px)
    pub fn sm(self) -> Self {
        self.size(AvatarSize::Small)
    }

    /// Set size to medium (40px) - default
    pub fn md(self) -> Self {
        self.size(AvatarSize::Medium)
    }

    /// Set size to large (48px)
    pub fn lg(self) -> Self {
        self.size(AvatarSize::Large)
    }

    /// Set size to extra large (64px)
    pub fn xl(self) -> Self {
        self.size(AvatarSize::ExtraLarge)
    }

    // Convenience shape methods

    /// Set shape to circle (default)
    pub fn circle(self) -> Self {
        self.shape(AvatarShape::Circle)
    }

    /// Set shape to rounded square
    pub fn square(self) -> Self {
        self.shape(AvatarShape::Square)
    }

    // Convenience status methods

    /// Set status to online
    pub fn online(self) -> Self {
        self.status(AvatarStatus::Online)
    }

    /// Set status to offline
    pub fn offline(self) -> Self {
        self.status(AvatarStatus::Offline)
    }

    /// Set status to away
    pub fn away(self) -> Self {
        self.status(AvatarStatus::Away)
    }

    /// Set status to busy
    pub fn busy(self) -> Self {
        self.status(AvatarStatus::Busy)
    }

    /// Build the final Avatar component
    pub fn build_final(self) -> Avatar {
        let config = self.config.into_inner();
        let built = BuiltAvatar::from_config(config);
        Avatar { inner: built.inner }
    }
}

impl Default for AvatarBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ElementBuilder for AvatarBuilder {
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

    fn element_id(&self) -> Option<&str> {
        self.get_or_build().element_id()
    }
}

/// Create a new avatar
///
/// # Example
///
/// ```ignore
/// use blinc_cn::prelude::*;
///
/// // Avatar with image
/// cn::avatar()
///     .src("https://example.com/avatar.jpg")
///     .alt("John Doe")
///
/// // Avatar with fallback initials
/// cn::avatar()
///     .fallback("JD")
///     .size(AvatarSize::Large)
///
/// // Avatar with status
/// cn::avatar()
///     .fallback("AB")
///     .online()
/// ```
pub fn avatar() -> AvatarBuilder {
    AvatarBuilder::new()
}

/// Create an avatar group for displaying multiple avatars
///
/// Avatars are stacked with a slight overlap.
///
/// # Example
///
/// ```ignore
/// use blinc_cn::prelude::*;
///
/// cn::avatar_group()
///     .child(cn::avatar().fallback("A"))
///     .child(cn::avatar().fallback("B"))
///     .child(cn::avatar().fallback("C"))
/// ```
pub fn avatar_group() -> AvatarGroupBuilder {
    AvatarGroupBuilder::new()
}

/// Configuration for avatar group
struct AvatarGroupConfig {
    /// Avatars in the group
    avatars: Vec<AvatarBuilder>,
    /// Size for all avatars
    size: AvatarSize,
    /// Maximum avatars to show (rest shows as +N)
    max: Option<usize>,
    /// Overlap amount in pixels
    overlap: f32,
    /// User-added CSS classes
    classes: Vec<std::sync::Arc<str>>,
    /// User-set element ID
    user_id: Option<String>,
}

impl Default for AvatarGroupConfig {
    fn default() -> Self {
        Self {
            avatars: Vec::new(),
            size: AvatarSize::default(),
            max: None,
            overlap: 8.0,
            classes: Vec::new(),
            user_id: None,
        }
    }
}

/// Built avatar group
struct BuiltAvatarGroup {
    inner: Div,
}

impl BuiltAvatarGroup {
    fn from_config(config: AvatarGroupConfig) -> Self {
        let theme = ThemeState::get();
        let size_px = config.size.pixels();
        let overlap = config.overlap;

        // Convert overlap pixels to margin units (1 unit = 4px)
        let overlap_units = -overlap / 4.0;

        let mut container = div().class("cn-avatar-group").flex_row().items_center();

        let total = config.avatars.len();
        let visible_count = config.max.unwrap_or(total).min(total);
        let remaining = total.saturating_sub(visible_count);

        // Single-primitive ring (no wrapper): each avatar gets its
        // border applied DIRECTLY on its own outer div via
        // `AvatarBuilder::ring(...)`. The renderer merges the
        // border + bg + rounded corner into a single SDF primitive
        // — same single-primitive recipe `cn::spinner`'s track
        // and `cn::switch` / `cn::slider`'s thumbs use — so each
        // avatar's edge is one smooth AA gradient. Avoids the
        // wrapper-vs-inner co-located AA that previously layered
        // two ~50 %-alpha gradients at the same curve and let parent
        // bg leak through as a faint "doubled" outline.
        // BorderSecondary is calibrated as a divider tone — visible
        // against Surface in BOTH schemes (light grey #D4D9E4 on
        // white, medium grey #3A4055 on dark Surface). Surface as
        // the ring colour disappeared in dark mode because the
        // avatar's own bg AND the typical card parent are both
        // Surface; nothing to contrast with.
        let ring_color = theme.color(ColorToken::BorderSecondary);

        // Ring width scales with avatar size — at small sizes a 2px
        // ring is half the visible stroke and the 1px AA at its outer
        // edge reads as a significant fraction of the perceived ring;
        // bumping to ~3-4px keeps a solid centre that the eye locks
        // onto and pushes the AA fringe into a smaller perceived
        // proportion of the stroke.
        let ring_width = match config.size {
            AvatarSize::ExtraSmall | AvatarSize::Small => 2.0,
            AvatarSize::Medium => 3.0,
            AvatarSize::Large | AvatarSize::ExtraLarge => 4.0,
        };
        for (i, avatar) in config.avatars.into_iter().take(visible_count).enumerate() {
            let prepared = avatar.ring(ring_color, ring_width);
            // Negative margin for overlap. Wrap in a layout-only div
            // (no bg/border/rounded — purely for the `ml` margin) so
            // it can't introduce a second primitive at the avatar's
            // outer edge.
            let positioned = if i > 0 {
                div().ml(overlap_units).child(prepared)
            } else {
                div().child(prepared)
            };
            container = container.child(positioned);
        }

        // Add "+N" indicator if there are remaining avatars. Same
        // single-primitive recipe as the avatars themselves: bg +
        // border + rounded on one div, no wrapper.
        if remaining > 0 {
            let remaining_indicator = div()
                .class("cn-avatar-group__more")
                .w(size_px)
                .h(size_px)
                .rounded(size_px / 2.0)
                .bg(theme.color(ColorToken::SurfaceElevated))
                .border(ring_width, ring_color)
                .flex_row()
                .items_center()
                .justify_center()
                .ml(overlap_units)
                .child(
                    text(format!("+{}", remaining))
                        .size(config.size.font_size())
                        .weight(FontWeight::Medium)
                        .color(theme.color(ColorToken::TextTertiary))
                        .no_wrap(),
                );

            container = container.child(remaining_indicator);
        }

        // Apply user classes and id
        for c in &config.classes {
            container = container.class(c);
        }
        if let Some(ref id) = config.user_id {
            container = container.id(id);
        }

        Self { inner: container }
    }
}

/// Avatar group component
pub struct AvatarGroup {
    inner: Div,
}

impl ElementBuilder for AvatarGroup {
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

    fn element_id(&self) -> Option<&str> {
        self.inner.element_id()
    }
}

/// Builder for avatar group
pub struct AvatarGroupBuilder {
    config: RefCell<AvatarGroupConfig>,
    built: OnceCell<AvatarGroup>,
}

impl AvatarGroupBuilder {
    /// Create a new avatar group builder
    pub fn new() -> Self {
        Self {
            config: RefCell::new(AvatarGroupConfig::default()),
            built: OnceCell::new(),
        }
    }

    fn get_or_build(&self) -> &AvatarGroup {
        ::blinc_layout::build_once::build_once(&self.built, || {
            let config = self.config.take();
            let built = BuiltAvatarGroup::from_config(config);
            AvatarGroup { inner: built.inner }
        })
    }

    /// Add an avatar to the group. Takes an `AvatarBuilder`
    /// specifically (not a generic ElementBuilder) so the group can
    /// call `.ring(...)` on each one before it materialises —
    /// applying the separator border to the avatar's own outer div
    /// instead of wrapping it. That keeps each avatar a single SDF
    /// primitive, which is what makes its edge AA cleanly (the
    /// same reason `cn::spinner`'s track and `cn::switch`'s thumb
    /// render smoothly).
    pub fn child(self, avatar: AvatarBuilder) -> Self {
        self.config.borrow_mut().avatars.push(avatar);
        self
    }

    /// Set the size for all avatars in the group
    pub fn size(self, size: AvatarSize) -> Self {
        self.config.borrow_mut().size = size;
        self
    }

    /// Set maximum number of visible avatars
    pub fn max(self, count: usize) -> Self {
        self.config.borrow_mut().max = Some(count);
        self
    }

    /// Set overlap amount in pixels
    pub fn overlap(self, pixels: f32) -> Self {
        self.config.borrow_mut().overlap = pixels;
        self
    }

    /// Add a CSS class for selector matching
    pub fn class(self, name: impl AsRef<str>) -> Self {
        self.config
            .borrow_mut()
            .classes
            .push(blinc_core::intern::intern(name.as_ref()));
        self
    }

    /// Set the element ID for CSS selector matching
    pub fn id(self, id: &str) -> Self {
        self.config.borrow_mut().user_id = Some(id.to_string());
        self
    }

    /// Build the final AvatarGroup component
    pub fn build_final(self) -> AvatarGroup {
        let config = self.config.into_inner();
        let built = BuiltAvatarGroup::from_config(config);
        AvatarGroup { inner: built.inner }
    }
}

impl Default for AvatarGroupBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ElementBuilder for AvatarGroupBuilder {
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

    fn element_id(&self) -> Option<&str> {
        self.get_or_build().element_id()
    }
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
    fn test_avatar_size_pixels() {
        assert_eq!(AvatarSize::ExtraSmall.pixels(), 24.0);
        assert_eq!(AvatarSize::Small.pixels(), 32.0);
        assert_eq!(AvatarSize::Medium.pixels(), 40.0);
        assert_eq!(AvatarSize::Large.pixels(), 48.0);
        assert_eq!(AvatarSize::ExtraLarge.pixels(), 64.0);
    }

    #[test]
    fn test_avatar_builder_config() {
        init_theme();

        let builder = avatar()
            .fallback("JD")
            .size(AvatarSize::Large)
            .shape(AvatarShape::Square);

        let config = builder.config.borrow();
        assert_eq!(
            config
                .fallback
                .as_ref()
                .map(crate::reactive_props::current::<String>),
            Some("JD".to_string())
        );
        assert_eq!(config.size, AvatarSize::Large);
        assert_eq!(config.shape, AvatarShape::Square);
    }

    #[test]
    fn test_avatar_with_status() {
        init_theme();

        let builder = avatar().fallback("AB").status(AvatarStatus::Online);

        let config = builder.config.borrow();
        assert_eq!(config.status, Some(AvatarStatus::Online));
    }

    #[test]
    fn test_avatar_convenience_methods() {
        init_theme();

        let builder = avatar().fallback("X").lg().square().busy();

        let config = builder.config.borrow();
        assert_eq!(config.size, AvatarSize::Large);
        assert_eq!(config.shape, AvatarShape::Square);
        assert_eq!(config.status, Some(AvatarStatus::Busy));
    }

    #[test]
    fn test_avatar_group_config() {
        init_theme();

        let builder = avatar_group()
            .child(avatar().fallback("A"))
            .child(avatar().fallback("B"))
            .max(3);

        let config = builder.config.borrow();
        assert_eq!(config.avatars.len(), 2);
        assert_eq!(config.max, Some(3));
    }
}
