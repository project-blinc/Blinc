//! Sheet component for slide-in panels
//!
//! A themed panel that slides in from an edge of the screen.
//! Uses the overlay system for proper layering and dismissal.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! // Basic sheet from the right
//! cn::sheet()
//!     .title("Settings")
//!     .description("Configure your preferences")
//!     .content(|| {
//!         div().flex_col().gap_4()
//!             .child(cn::input().placeholder("Name"))
//!             .child(cn::input().placeholder("Email"))
//!     })
//!     .show();
//!
//! // Sheet from the left
//! cn::sheet()
//!     .side(SheetSide::Left)
//!     .title("Navigation")
//!     .show();
//!
//! // Bottom sheet (mobile-style)
//! cn::sheet()
//!     .side(SheetSide::Bottom)
//!     .title("Share")
//!     .show();
//! ```

use std::sync::Arc;

use blinc_animation::{AnimationPreset, MultiKeyframeAnimation};
use blinc_core::Color;
use blinc_layout::InstanceKey;
use blinc_layout::overlay_state::overlay_stack;
use blinc_layout::prelude::*;
use blinc_layout::widgets::overlay::EdgeSide;
use blinc_layout::widgets::overlay_stack::{OverlayBuilder, OverlayHandle};
use blinc_theme::{ColorToken, RadiusToken, ThemeState};

/// Sheet side variants - which edge the sheet slides from
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SheetSide {
    /// Slide in from the left edge
    Left,
    /// Slide in from the right edge (default)
    #[default]
    Right,
    /// Slide in from the top edge
    Top,
    /// Slide in from the bottom edge
    Bottom,
}

/// Sheet size variants
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SheetSize {
    /// Small sheet (320px for left/right, 200px for top/bottom)
    Small,
    /// Medium sheet (400px for left/right, 300px for top/bottom)
    #[default]
    Medium,
    /// Large sheet (540px for left/right, 400px for top/bottom)
    Large,
    /// Full screen (100% viewport)
    Full,
}

impl SheetSize {
    /// Get the size in pixels for horizontal sheets (left/right)
    pub fn width(&self) -> f32 {
        match self {
            SheetSize::Small => 320.0,
            SheetSize::Medium => 400.0,
            SheetSize::Large => 540.0,
            SheetSize::Full => f32::MAX, // Will be clamped to viewport
        }
    }

    /// Get the size in pixels for vertical sheets (top/bottom)
    pub fn height(&self) -> f32 {
        match self {
            SheetSize::Small => 200.0,
            SheetSize::Medium => 300.0,
            SheetSize::Large => 400.0,
            SheetSize::Full => f32::MAX, // Will be clamped to viewport
        }
    }
}

/// Builder for creating and showing sheets
pub struct SheetBuilder {
    side: SheetSide,
    size: SheetSize,
    title: Option<String>,
    description: Option<String>,
    content: Option<Arc<dyn Fn() -> Div + Send + Sync>>,
    footer: Option<Arc<dyn Fn() -> Div + Send + Sync>>,
    show_close: bool,
    on_close: Option<Arc<dyn Fn() + Send + Sync>>,
    /// Custom enter animation
    enter_animation: Option<MultiKeyframeAnimation>,
    /// Custom exit animation
    exit_animation: Option<MultiKeyframeAnimation>,
    /// Animation duration in ms
    animation_duration: u32,
    /// Unique key for motion animation
    key: InstanceKey,
}

impl SheetBuilder {
    /// Create a new sheet builder
    #[track_caller]
    pub fn new() -> Self {
        Self {
            side: SheetSide::Right,
            size: SheetSize::Medium,
            title: None,
            description: None,
            content: None,
            footer: None,
            show_close: true,
            on_close: None,
            enter_animation: None,
            exit_animation: None,
            animation_duration: 300,
            key: InstanceKey::new("sheet"),
        }
    }

    /// Set which side the sheet slides from
    pub fn side(mut self, side: SheetSide) -> Self {
        self.side = side;
        self
    }

    /// Set the sheet size
    pub fn size(mut self, size: SheetSize) -> Self {
        self.size = size;
        self
    }

    /// Set the sheet title
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Set the sheet description
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set custom content for the sheet body
    pub fn content<F>(mut self, content: F) -> Self
    where
        F: Fn() -> Div + Send + Sync + 'static,
    {
        self.content = Some(Arc::new(content));
        self
    }

    /// Set custom footer content
    pub fn footer<F>(mut self, footer: F) -> Self
    where
        F: Fn() -> Div + Send + Sync + 'static,
    {
        self.footer = Some(Arc::new(footer));
        self
    }

    /// Show or hide the close button
    pub fn show_close(mut self, show: bool) -> Self {
        self.show_close = show;
        self
    }

    /// Set the callback for when the sheet is closed
    pub fn on_close<F>(mut self, callback: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.on_close = Some(Arc::new(callback));
        self
    }

    /// Set custom enter animation
    pub fn enter_animation(mut self, animation: MultiKeyframeAnimation) -> Self {
        self.enter_animation = Some(animation);
        self
    }

    /// Set custom exit animation
    pub fn exit_animation(mut self, animation: MultiKeyframeAnimation) -> Self {
        self.exit_animation = Some(animation);
        self
    }

    /// Set animation duration in milliseconds
    pub fn animation_duration(mut self, duration_ms: u32) -> Self {
        self.animation_duration = duration_ms;
        self
    }

    /// Show the sheet
    pub fn show(self) -> OverlayHandle {
        let theme = ThemeState::get();
        let bg = theme.color(ColorToken::Surface);
        let border = theme.color(ColorToken::Border);
        let text_primary = theme.color(ColorToken::TextPrimary);
        let text_secondary = theme.color(ColorToken::TextSecondary);

        let side = self.side;
        let size = self.size;
        let title = self.title;
        let description = self.description;
        let content = self.content;
        let footer = self.footer;
        let show_close = self.show_close;
        let on_close = self.on_close;

        let edge_side = match side {
            SheetSide::Left => EdgeSide::Left,
            SheetSide::Right => EdgeSide::Right,
            SheetSide::Top => EdgeSide::Top,
            SheetSide::Bottom => EdgeSide::Bottom,
        };

        // Size override: position_wrapper pins the panel flush to the edge,
        // stretching the perpendicular axis to viewport. Pass the size that
        // matches the slide axis; the other axis is ignored.
        let (panel_w, panel_h) = match side {
            SheetSide::Left | SheetSide::Right => (size.width(), 0.0),
            SheetSide::Top | SheetSide::Bottom => (0.0, size.height()),
        };

        // Slide in / out from the entry edge. Distance matches the panel's
        // axis dimension so it fully slides off-screen.
        let slide_distance = match side {
            SheetSide::Left | SheetSide::Right => size.width(),
            SheetSide::Top | SheetSide::Bottom => size.height(),
        };
        // Theme's `ease_sheet` slot drives the slide curve so sheets
        // read as "substantial drawer surface" — long-tail decelerate
        // on enter, mirrored ease-in on exit. User overrides apply on
        // top.
        let sheet_easing = theme.animations().ease_sheet.to_animation_easing();
        let (in_dx, in_dy, out_dx, out_dy) = match side {
            SheetSide::Left => (-slide_distance, 0.0, -slide_distance, 0.0),
            SheetSide::Right => (slide_distance, 0.0, slide_distance, 0.0),
            SheetSide::Top => (0.0, -slide_distance, 0.0, -slide_distance),
            SheetSide::Bottom => (0.0, slide_distance, 0.0, slide_distance),
        };
        let enter_animation = self
            .enter_animation
            .clone()
            .unwrap_or_else(|| AnimationPreset::slide_in_with(300, in_dx, in_dy, sheet_easing));
        let exit_animation = self
            .exit_animation
            .clone()
            .unwrap_or_else(|| AnimationPreset::slide_out_with(225, out_dx, out_dy, sheet_easing));

        let next_handle_id = overlay_stack()
            .lock()
            .ok()
            .map(|s| s.peek_next_handle_id())
            .unwrap_or(0);
        let sheet_handle = OverlayHandle::from_raw(next_handle_id);

        // Every dismissal route, not just the close button. Backdrop
        // and Escape are handled inside the overlay stack, so a caller
        // that only wired the button's callback saw its bound signal
        // stay set — the overlay went away and returned on the next
        // mount, because nothing had told the signal it closed.
        let on_close_any = on_close.clone();
        let handle = OverlayBuilder::modal()
            .on_close(move |_reason| {
                if let Some(ref cb) = on_close_any {
                    cb();
                }
            })
            // Modal defaults: ESC, click-outside (backdrop dismiss), backdrop=Some.
            .edge(edge_side)
            .size(panel_w, panel_h)
            .motion_enter(enter_animation)
            .motion_exit(exit_animation)
            .content(move || {
                build_sheet_content(
                    side,
                    size,
                    &title,
                    &description,
                    &content,
                    &footer,
                    show_close,
                    &on_close,
                    bg,
                    border,
                    text_primary,
                    text_secondary,
                    sheet_handle,
                )
            })
            .show();

        debug_assert_eq!(
            handle.raw(),
            next_handle_id,
            "peek_next_handle_id was stale — concurrent push?"
        );

        handle
    }
}

impl Default for SheetBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a new sheet builder
///
/// # Example
///
/// ```ignore
/// cn::sheet()
///     .side(SheetSide::Right)
///     .title("Settings")
///     .content(|| div().child(text("Content here")))
///     .show();
/// ```
#[track_caller]
pub fn sheet() -> SheetBuilder {
    SheetBuilder::new()
}

/// Build the sheet content. Enter animation is delegated to the CSS
/// `@keyframes cn-sheet-enter-{left,right,top,bottom}` rules on `.cn-sheet`.
#[allow(clippy::too_many_arguments)]
fn build_sheet_content(
    side: SheetSide,
    size: SheetSize,
    title: &Option<String>,
    description: &Option<String>,
    content: &Option<Arc<dyn Fn() -> Div + Send + Sync>>,
    footer: &Option<Arc<dyn Fn() -> Div + Send + Sync>>,
    show_close: bool,
    on_close: &Option<Arc<dyn Fn() + Send + Sync>>,
    bg: Color,
    border: Color,
    text_primary: Color,
    text_secondary: Color,
    handle: OverlayHandle,
) -> Div {
    let theme = ThemeState::get();
    let radius = theme.radius(RadiusToken::Lg);

    // Calculate sheet dimensions based on side
    let (sheet_w, sheet_h, border_radius) = match side {
        SheetSide::Left => {
            let w = if size == SheetSize::Full {
                f32::MAX
            } else {
                size.width()
            };
            (Some(w), None, (0.0, radius, radius, 0.0)) // Top-right, bottom-right rounded
        }
        SheetSide::Right => {
            let w = if size == SheetSize::Full {
                f32::MAX
            } else {
                size.width()
            };
            (Some(w), None, (radius, 0.0, 0.0, radius)) // Top-left, bottom-left rounded
        }
        SheetSide::Top => {
            let h = if size == SheetSize::Full {
                f32::MAX
            } else {
                size.height()
            };
            (None, Some(h), (0.0, 0.0, radius, radius)) // Bottom-left, bottom-right rounded
        }
        SheetSide::Bottom => {
            let h = if size == SheetSize::Full {
                f32::MAX
            } else {
                size.height()
            };
            (None, Some(h), (radius, radius, 0.0, 0.0)) // Top-left, top-right rounded
        }
    };

    let side_class = match side {
        SheetSide::Left => "cn-sheet--left",
        SheetSide::Right => "cn-sheet--right",
        SheetSide::Top => "cn-sheet--top",
        SheetSide::Bottom => "cn-sheet--bottom",
    };

    let mut sheet = div()
        .class("cn-sheet")
        .class(side_class)
        .bg(bg)
        .border(1.0, border)
        .shadow_xl()
        .flex_col()
        .overflow_clip();

    // Apply dimensions
    match side {
        SheetSide::Left | SheetSide::Right => {
            sheet = sheet.h_full();
            if let Some(w) = sheet_w {
                sheet = sheet.w(w).max_w(w);
            }
        }
        SheetSide::Top | SheetSide::Bottom => {
            sheet = sheet.w_full();
            if let Some(h) = sheet_h {
                sheet = sheet.h(h).max_h(h);
            }
        }
    }

    // Apply rounded corners based on side
    let (tl, tr, br, bl) = border_radius;
    sheet = sheet.rounded_corners(tl, tr, br, bl);

    // Header section
    let mut header = div()
        .w_full()
        .flex_row()
        .items_center()
        .justify_between()
        .p_4();

    let mut header_text = div().flex_col().gap_1();

    if let Some(title_text) = title {
        header_text = header_text.child(
            text(title_text)
                .size(theme.typography().text_lg)
                .color(text_primary)
                .medium(),
        );
    }

    if let Some(desc_text) = description {
        header_text = header_text.child(
            text(desc_text)
                .size(theme.typography().text_sm)
                .color(text_secondary),
        );
    }

    header = header.child(header_text);

    // Close button
    if show_close {
        let close_icon = r#"<svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="18" x2="6" y1="6" y2="18"/><line x1="6" x2="18" y1="6" y2="18"/></svg>"#;

        let on_close_clone = on_close.clone();
        header = header.child(
            div()
                .w(32.0)
                .h(32.0)
                .items_center()
                .rounded(theme.radius(RadiusToken::Sm))
                .cursor_pointer()
                .on_click(move |_| {
                    if let Some(ref cb) = on_close_clone {
                        cb();
                    }
                    handle.close();
                })
                .child(svg(close_icon).size(18.0, 18.0).color(text_secondary)),
        );
    }

    sheet = sheet.child(header);

    // Separator under header
    sheet = sheet.child(div().w_full().h(1.0).bg(border));

    // Content section (scrollable)
    if let Some(content_fn) = content {
        let content_div = div()
            .flex_1()
            .w_full()
            .p_4()
            .overflow_scroll()
            .child(content_fn());
        sheet = sheet.child(content_div);
    }

    // Footer section
    if let Some(footer_fn) = footer {
        sheet = sheet.child(div().w_full().h(1.0).bg(border)); // Separator
        sheet = sheet.child(div().w_full().p_4().child(footer_fn()));
    }

    sheet
}

/// Convenience function for a left-side sheet
#[track_caller]
pub fn sheet_left() -> SheetBuilder {
    sheet().side(SheetSide::Left)
}

/// Convenience function for a right-side sheet
#[track_caller]
pub fn sheet_right() -> SheetBuilder {
    sheet().side(SheetSide::Right)
}

/// Convenience function for a top sheet
#[track_caller]
pub fn sheet_top() -> SheetBuilder {
    sheet().side(SheetSide::Top)
}

/// Convenience function for a bottom sheet (mobile-style)
#[track_caller]
pub fn sheet_bottom() -> SheetBuilder {
    sheet().side(SheetSide::Bottom)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sheet_builder() {
        let builder = sheet()
            .side(SheetSide::Left)
            .size(SheetSize::Large)
            .title("Test");

        assert_eq!(builder.side, SheetSide::Left);
        assert_eq!(builder.size, SheetSize::Large);
        assert_eq!(builder.title, Some("Test".to_string()));
    }

    #[test]
    fn test_sheet_sizes() {
        assert_eq!(SheetSize::Small.width(), 320.0);
        assert_eq!(SheetSize::Medium.width(), 400.0);
        assert_eq!(SheetSize::Large.width(), 540.0);

        assert_eq!(SheetSize::Small.height(), 200.0);
        assert_eq!(SheetSize::Medium.height(), 300.0);
        assert_eq!(SheetSize::Large.height(), 400.0);
    }

    #[test]
    fn test_sheet_sides() {
        assert_eq!(SheetSide::default(), SheetSide::Right);
    }
}
