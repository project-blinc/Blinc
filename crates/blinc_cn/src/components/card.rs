//! Card component for content containers
//!
//! A styled container with shadow and border for grouping related content.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! // Simple card with content
//! cn::card()
//!     .child(text("Card content"))
//!
//! // Card with structured content using CardHeader and CardFooter
//! cn::card()
//!     .child(cn::card_header().title("Card Title").description("Description"))
//!     .child(text("Main content goes here"))
//!     .child(cn::card_footer().child(cn::button("Action")))
//!
//! // Card with custom styling (via Deref to Div)
//! cn::card()
//!     .shadow_lg()  // Larger shadow
//!     .p(32.0)      // Custom padding
//!     .child(text("Custom styled card"))
//! ```

use std::ops::{Deref, DerefMut};

use blinc_layout::div::{Div, ElementBuilder, ElementTypeId};
use blinc_layout::prelude::*;
use blinc_theme::{ColorToken, RadiusToken, SpacingToken, ThemeState};

/// Card component for content containers
///
/// Implements `Deref` to `Div` for full customization.
pub struct Card {
    inner: Div,
}

impl Card {
    /// Create a new empty card
    pub fn new() -> Self {
        let theme = ThemeState::get();

        let bg = theme.color(ColorToken::Surface);
        let border_color = theme.color(ColorToken::Border);
        let radius = theme.radius(RadiusToken::Lg);
        let padding = theme.spacing_value(SpacingToken::Space6); // 24px
        let gap = theme.spacing_value(SpacingToken::Space4); // 16px

        let inner = div()
            .bg(bg)
            .border(1.0, border_color)
            .rounded(radius)
            .shadow_sm()
            .p_px(padding)
            .flex_col()
            .items_start() // Align content to start, not center
            .h_fit() // Don't stretch to fill parent height
            .gap_px(gap); // 16px gap between sections

        Self { inner }
    }

    /// Add content to the card body
    pub fn child(mut self, content: impl ElementBuilder + 'static) -> Self {
        self.inner = self.inner.child(content);
        self
    }

    // Forwarding methods for common Div operations

    /// Set width
    pub fn w(mut self, width: f32) -> Self {
        self.inner = self.inner.w(width);
        self
    }

    /// Set height
    pub fn h(mut self, height: f32) -> Self {
        self.inner = self.inner.h(height);
        self
    }

    /// Set full width
    pub fn w_full(mut self) -> Self {
        self.inner = self.inner.w_full();
        self
    }

    /// Set padding on all sides
    pub fn p(mut self, padding: f32) -> Self {
        self.inner = self.inner.p(padding);
        self
    }

    /// Set horizontal padding
    pub fn px(mut self, padding: f32) -> Self {
        self.inner = self.inner.px(padding);
        self
    }

    /// Set vertical padding
    pub fn py(mut self, padding: f32) -> Self {
        self.inner = self.inner.py(padding);
        self
    }

    /// Set margin on all sides
    pub fn m(mut self, margin: f32) -> Self {
        self.inner = self.inner.m(margin);
        self
    }

    /// Apply large shadow
    pub fn shadow_lg(mut self) -> Self {
        self.inner = self.inner.shadow_lg();
        self
    }

    /// Apply medium shadow
    pub fn shadow_md(mut self) -> Self {
        self.inner = self.inner.shadow_md();
        self
    }

    /// Set background color
    pub fn bg(mut self, color: blinc_core::Color) -> Self {
        self.inner = self.inner.bg(color);
        self
    }
}

impl Default for Card {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for Card {
    type Target = Div;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for Card {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl ElementBuilder for Card {
    fn build(&self, tree: &mut blinc_layout::tree::LayoutTree) -> blinc_layout::tree::LayoutNodeId {
        self.inner.build(tree)
    }

    fn render_props(&self) -> blinc_layout::element::RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.inner.children_builders()
    }

    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(&self.inner)
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        ElementBuilder::layout_style(&self.inner)
    }

    fn element_type_id(&self) -> ElementTypeId {
        ElementBuilder::element_type_id(&self.inner)
    }
}

/// Create an empty card
///
/// # Example
///
/// ```ignore
/// use blinc_cn::prelude::*;
///
/// cn::card()
///     .child(text("Content"))
/// ```
pub fn card() -> Card {
    Card::new()
}

// ============================================================================
// Card subcomponents for structured content
// ============================================================================

/// Card header section
pub struct CardHeader {
    inner: Div,
}

impl CardHeader {
    /// Create a new card header
    pub fn new() -> Self {
        let theme = ThemeState::get();
        let gap = theme.spacing_value(SpacingToken::Space1_5); // 6px
        let inner = div()
            .flex_col()
            .items_start()
            .w_full()
            .gap_px(gap);

        Self { inner }
    }

    /// Add a title
    pub fn title(mut self, title: impl Into<String>) -> Self {
        let theme = ThemeState::get();
        self.inner = self.inner.child(
            text(title)
                .size(18.0)
                .semibold()
                .color(theme.color(ColorToken::TextPrimary)),
        );
        self
    }

    /// Add a description
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        let theme = ThemeState::get();
        self.inner = self.inner.child(
            text(desc)
                .size(14.0)
                .color(theme.color(ColorToken::TextSecondary)),
        );
        self
    }
}

impl Default for CardHeader {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for CardHeader {
    type Target = Div;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for CardHeader {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl ElementBuilder for CardHeader {
    fn build(&self, tree: &mut blinc_layout::tree::LayoutTree) -> blinc_layout::tree::LayoutNodeId {
        self.inner.build(tree)
    }

    fn render_props(&self) -> blinc_layout::element::RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.inner.children_builders()
    }

    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(&self.inner)
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        ElementBuilder::layout_style(&self.inner)
    }

    fn element_type_id(&self) -> ElementTypeId {
        ElementBuilder::element_type_id(&self.inner)
    }
}

/// Create a card header
pub fn card_header() -> CardHeader {
    CardHeader::new()
}

/// Card content section - grows to fill available space
pub struct CardContent {
    inner: Div,
}

impl CardContent {
    /// Create a new card content section
    pub fn new() -> Self {
        let inner = div()
            .flex_col()
            .flex_1() // Grow to fill available space
            .w_full();

        Self { inner }
    }

    /// Add a child element
    pub fn child(mut self, content: impl ElementBuilder + 'static) -> Self {
        self.inner = self.inner.child(content);
        self
    }
}

impl Default for CardContent {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for CardContent {
    type Target = Div;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for CardContent {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl ElementBuilder for CardContent {
    fn build(&self, tree: &mut blinc_layout::tree::LayoutTree) -> blinc_layout::tree::LayoutNodeId {
        self.inner.build(tree)
    }

    fn render_props(&self) -> blinc_layout::element::RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.inner.children_builders()
    }

    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(&self.inner)
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        ElementBuilder::layout_style(&self.inner)
    }

    fn element_type_id(&self) -> ElementTypeId {
        ElementBuilder::element_type_id(&self.inner)
    }
}

/// Create a card content section
pub fn card_content() -> CardContent {
    CardContent::new()
}

/// Card footer section
pub struct CardFooter {
    inner: Div,
}

impl CardFooter {
    /// Create a new card footer
    pub fn new() -> Self {
        let theme = ThemeState::get();
        let gap = theme.spacing_value(SpacingToken::Space2); // 8px
        let inner = div()
            .flex_row()
            .w_full()
            .gap_px(gap)
            .justify_end();

        Self { inner }
    }

    /// Add a child element
    pub fn child(mut self, content: impl ElementBuilder + 'static) -> Self {
        self.inner = self.inner.child(content);
        self
    }
}

impl Default for CardFooter {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for CardFooter {
    type Target = Div;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for CardFooter {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl ElementBuilder for CardFooter {
    fn build(&self, tree: &mut blinc_layout::tree::LayoutTree) -> blinc_layout::tree::LayoutNodeId {
        self.inner.build(tree)
    }

    fn render_props(&self) -> blinc_layout::element::RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.inner.children_builders()
    }

    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(&self.inner)
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        ElementBuilder::layout_style(&self.inner)
    }

    fn element_type_id(&self) -> ElementTypeId {
        ElementBuilder::element_type_id(&self.inner)
    }
}

/// Create a card footer
pub fn card_footer() -> CardFooter {
    CardFooter::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_theme() {
        let _ = ThemeState::try_get().unwrap_or_else(|| {
            ThemeState::init_default();
            ThemeState::get()
        });
    }

    #[test]
    fn test_card_default() {
        init_theme();
        let _ = card();
    }

    #[test]
    fn test_card_with_content() {
        init_theme();
        let _ = card().child(text("Content"));
    }

    #[test]
    fn test_card_header() {
        init_theme();
        let _ = card_header().title("Title").description("Description");
    }

    #[test]
    fn test_card_footer() {
        init_theme();
        let _ = card_footer();
    }
}
