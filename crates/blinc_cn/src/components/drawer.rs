//! Drawer component for navigation panels
//!
//! A themed navigation drawer that slides in from the left or right edge.
//! Optimized for navigation menus with a simpler API than Sheet.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! // Basic navigation drawer
//! cn::drawer()
//!     .title("Menu")
//!     .child(cn::button("Home").variant(ButtonVariant::Ghost))
//!     .child(cn::button("Profile").variant(ButtonVariant::Ghost))
//!     .child(cn::button("Settings").variant(ButtonVariant::Ghost))
//!     .show();
//!
//! // Drawer from the right
//! cn::drawer()
//!     .side(DrawerSide::Right)
//!     .title("Notifications")
//!     .show();
//!
//! // Drawer with header and footer
//! cn::drawer()
//!     .header(|| {
//!         div().flex_row().gap_2()
//!             .child(avatar("JD"))
//!             .child(text("John Doe"))
//!     })
//!     .child(navigation_items())
//!     .footer(|| cn::button("Logout").variant(ButtonVariant::Destructive))
//!     .show();
//! ```

use std::sync::Arc;

use blinc_animation::AnimationPreset;
use blinc_core::Color;
use blinc_layout::InstanceKey;
use blinc_layout::overlay_state::overlay_stack;
use blinc_layout::prelude::*;
use blinc_layout::widgets::overlay::EdgeSide;
use blinc_layout::widgets::overlay_stack::{OverlayBuilder, OverlayHandle};
use blinc_theme::{ColorToken, RadiusToken, ThemeState};

/// Drawer side variants
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DrawerSide {
    /// Slide in from the left edge (default, standard for navigation)
    #[default]
    Left,
    /// Slide in from the right edge
    Right,
}

/// Drawer size variants
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DrawerSize {
    /// Narrow drawer (240px)
    Narrow,
    /// Medium drawer (280px)
    #[default]
    Medium,
    /// Wide drawer (320px)
    Wide,
}

impl DrawerSize {
    /// Get the width in pixels
    pub fn width(&self) -> f32 {
        match self {
            DrawerSize::Narrow => 240.0,
            DrawerSize::Medium => 280.0,
            DrawerSize::Wide => 320.0,
        }
    }
}

/// Builder for creating and showing drawers
pub struct DrawerBuilder {
    side: DrawerSide,
    size: DrawerSize,
    title: Option<String>,
    header: Option<Arc<dyn Fn() -> Div + Send + Sync>>,
    children: Vec<Arc<dyn Fn() -> Div + Send + Sync>>,
    footer: Option<Arc<dyn Fn() -> Div + Send + Sync>>,
    show_close: bool,
    on_close: Option<Arc<dyn Fn() + Send + Sync>>,
    /// Animation duration in ms
    animation_duration: u32,
    /// User-added CSS classes
    classes: Vec<std::sync::Arc<str>>,
    /// User-set element ID
    user_id: Option<String>,
    /// Unique key for motion animation
    key: InstanceKey,
}

impl DrawerBuilder {
    /// Create a new drawer builder
    #[track_caller]
    pub fn new() -> Self {
        Self {
            side: DrawerSide::Left,
            size: DrawerSize::Medium,
            title: None,
            header: None,
            children: Vec::new(),
            footer: None,
            show_close: true,
            on_close: None,
            animation_duration: 250,
            key: InstanceKey::new("drawer"),
            classes: Vec::new(),
            user_id: None,
        }
    }

    /// Set which side the drawer slides from
    pub fn side(mut self, side: DrawerSide) -> Self {
        self.side = side;
        self
    }

    /// Set the drawer size
    pub fn size(mut self, size: DrawerSize) -> Self {
        self.size = size;
        self
    }

    /// Set the drawer title (shown in header)
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Set custom header content (replaces title)
    pub fn header<F>(mut self, header: F) -> Self
    where
        F: Fn() -> Div + Send + Sync + 'static,
    {
        self.header = Some(Arc::new(header));
        self
    }

    /// Add a child element to the drawer body
    pub fn child<F>(mut self, child: F) -> Self
    where
        F: Fn() -> Div + Send + Sync + 'static,
    {
        self.children.push(Arc::new(child));
        self
    }

    /// Add a child element builder directly (for Button, etc.)
    pub fn child_builder<B: ElementBuilder + Clone + Send + Sync + 'static>(
        mut self,
        builder: B,
    ) -> Self {
        self.children
            .push(Arc::new(move || div().child(builder.clone())));
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

    /// Set the callback for when the drawer is closed
    pub fn on_close<F>(mut self, callback: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.on_close = Some(Arc::new(callback));
        self
    }

    /// Set animation duration in milliseconds
    pub fn animation_duration(mut self, duration_ms: u32) -> Self {
        self.animation_duration = duration_ms;
        self
    }

    /// Add a CSS class for selector matching
    pub fn class(mut self, name: impl AsRef<str>) -> Self {
        self.classes.push(blinc_core::intern::intern(name.as_ref()));
        self
    }

    /// Set the element ID for CSS selector matching
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.user_id = Some(id.into());
        self
    }

    /// Show the drawer
    pub fn show(self) -> OverlayHandle {
        let theme = ThemeState::get();
        let bg = theme.color(ColorToken::Surface);
        let border = theme.color(ColorToken::Border);
        let text_primary = theme.color(ColorToken::TextPrimary);
        let text_secondary = theme.color(ColorToken::TextSecondary);

        let side = self.side;
        let size = self.size;
        let title = self.title;
        let header = self.header;
        let children = self.children;
        let footer = self.footer;
        let show_close = self.show_close;
        let on_close = self.on_close;
        let classes = self.classes;
        let user_id = self.user_id;

        let edge_side = match side {
            DrawerSide::Left => EdgeSide::Left,
            DrawerSide::Right => EdgeSide::Right,
        };

        // Drawer panel size: width is fixed, height fills viewport (position_wrapper
        // stretches the perpendicular axis to viewport for Edge positions).
        let drawer_width = size.width();

        // Slide from the edge the drawer is anchored to. Theme's
        // `ease_sheet` shapes the slide so the drawer reads as a heavy
        // surface rather than a generic ease-out.
        let sheet_easing = theme.animations().ease_sheet.to_animation_easing();
        let (in_dx, out_dx) = match side {
            DrawerSide::Left => (-drawer_width, -drawer_width),
            DrawerSide::Right => (drawer_width, drawer_width),
        };
        let enter_animation =
            AnimationPreset::slide_in_with(self.animation_duration, in_dx, 0.0, sheet_easing);
        let exit_animation = AnimationPreset::slide_out_with(
            (self.animation_duration as f32 * 0.7) as u32,
            out_dx,
            0.0,
            sheet_easing,
        );

        // Pre-allocate the handle id so the close button can capture it
        // (same pattern as dialog/popover — `handle.close()` instead of
        // `close_top()`).
        let next_handle_id = overlay_stack()
            .lock()
            .ok()
            .map(|s| s.peek_next_handle_id())
            .unwrap_or(0);
        let drawer_handle = OverlayHandle::from_raw(next_handle_id);

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
            .size(drawer_width, 0.0) // height ignored — position_wrapper uses viewport.1
            .motion_enter(enter_animation)
            .motion_exit(exit_animation)
            .content(move || {
                let mut content_div = build_drawer_content(
                    side,
                    size,
                    &title,
                    &header,
                    &children,
                    &footer,
                    show_close,
                    &on_close,
                    bg,
                    border,
                    text_primary,
                    text_secondary,
                    drawer_handle,
                );
                for c in &classes {
                    content_div = content_div.class(c);
                }
                if let Some(ref id) = user_id {
                    content_div = content_div.id(id);
                }
                content_div
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

impl Default for DrawerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a new drawer builder
///
/// # Example
///
/// ```ignore
/// cn::drawer()
///     .title("Navigation")
///     .child(|| cn::button("Home").variant(ButtonVariant::Ghost))
///     .child(|| cn::button("Settings").variant(ButtonVariant::Ghost))
///     .show();
/// ```
#[track_caller]
pub fn drawer() -> DrawerBuilder {
    DrawerBuilder::new()
}

/// Build the drawer content. Enter animation is delegated to the CSS
/// `@keyframes cn-drawer-enter-{left,right,top,bottom}` rules on `.cn-drawer`
/// (see cn_styles.rs); side modifier classes pick the right keyframe.
#[allow(clippy::too_many_arguments)]
fn build_drawer_content(
    side: DrawerSide,
    size: DrawerSize,
    title: &Option<String>,
    header: &Option<Arc<dyn Fn() -> Div + Send + Sync>>,
    children: &[Arc<dyn Fn() -> Div + Send + Sync>],
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

    // Determine rounded corners based on side
    let border_radius = match side {
        DrawerSide::Left => (0.0, radius, radius, 0.0), // Right corners rounded
        DrawerSide::Right => (radius, 0.0, 0.0, radius), // Left corners rounded
    };

    let side_class = match side {
        DrawerSide::Left => "cn-drawer--left",
        DrawerSide::Right => "cn-drawer--right",
    };

    let mut drawer = div()
        .class("cn-drawer")
        .class(side_class)
        .w(size.width())
        .h_full()
        .bg(bg)
        .border(1.0, border)
        .shadow_xl()
        .flex_col()
        .overflow_clip();

    // Apply rounded corners
    let (tl, tr, br, bl) = border_radius;
    drawer = drawer.rounded_corners(tl, tr, br, bl);

    // Header section
    let has_header = header.is_some() || title.is_some() || show_close;
    if has_header {
        // padding from CSS: .cn-drawer-header { padding: 16px; }
        let mut header_div = div()
            .class("cn-drawer-header")
            .w_full()
            .flex_row()
            .items_center()
            .justify_between();

        if let Some(header_fn) = header {
            header_div = header_div.child(header_fn());
        } else if let Some(title_text) = title {
            header_div = header_div.child(
                text(title_text)
                    .size(theme.typography().text_lg)
                    .color(text_primary)
                    .semibold(),
            );
        } else {
            // Empty spacer for alignment when only close button
            header_div = header_div.child(div());
        }

        // Close button
        if show_close {
            let close_icon = r#"<svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="18" x2="6" y1="6" y2="18"/><line x1="6" x2="18" y1="6" y2="18"/></svg>"#;

            let on_close_clone = on_close.clone();
            header_div = header_div.child(
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

        drawer = drawer.child(header_div);

        // Separator under header
        drawer = drawer.child(div().w_full().h(1.0).bg(border));
    }

    // Body section with children (scrollable)
    if !children.is_empty() {
        let mut body = div()
            .flex_1()
            .w_full()
            .flex_col()
            .gap_1()
            .p_2()
            .overflow_scroll();

        for child_fn in children {
            body = body.child(child_fn());
        }

        drawer = drawer.child(body);
    }

    // Footer section
    if let Some(footer_fn) = footer {
        // Push footer to bottom with spacer if no children
        if children.is_empty() {
            drawer = drawer.child(div().flex_1());
        }

        drawer = drawer.child(div().w_full().h(1.0).bg(border)); // Separator
        // padding from CSS: .cn-drawer-footer { padding: 16px; }
        drawer = drawer.child(div().class("cn-drawer-footer").w_full().child(footer_fn()));
    }

    drawer
}

/// Convenience function for a left-side drawer (navigation)
#[track_caller]
pub fn drawer_left() -> DrawerBuilder {
    drawer().side(DrawerSide::Left)
}

/// Convenience function for a right-side drawer
#[track_caller]
pub fn drawer_right() -> DrawerBuilder {
    drawer().side(DrawerSide::Right)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_drawer_builder() {
        let builder = drawer()
            .side(DrawerSide::Right)
            .size(DrawerSize::Wide)
            .title("Test");

        assert_eq!(builder.side, DrawerSide::Right);
        assert_eq!(builder.size, DrawerSize::Wide);
        assert_eq!(builder.title, Some("Test".to_string()));
    }

    #[test]
    fn test_drawer_sizes() {
        assert_eq!(DrawerSize::Narrow.width(), 240.0);
        assert_eq!(DrawerSize::Medium.width(), 280.0);
        assert_eq!(DrawerSize::Wide.width(), 320.0);
    }

    #[test]
    fn test_drawer_sides() {
        assert_eq!(DrawerSide::default(), DrawerSide::Left);
    }
}
