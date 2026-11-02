//! Toast component for temporary notifications
//!
//! A themed toast notification that appears in a corner and auto-dismisses.
//! Uses the overlay system for proper positioning and timing.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! // Simple toast
//! cn::toast("Operation completed successfully").show();
//!
//! // Toast with title and description
//! cn::toast("Saved")
//!     .description("Your changes have been saved.")
//!     .show();
//!
//! // Different variants
//! cn::toast("Error occurred")
//!     .variant(ToastVariant::Destructive)
//!     .description("Please try again later.")
//!     .show();
//!
//! // With custom duration
//! cn::toast("Quick message")
//!     .duration_ms(2000)
//!     .show();
//!
//! // With action button
//! cn::toast("File deleted")
//!     .action("Undo", || {
//!         println!("Undoing...");
//!     })
//!     .show();
//!
//! // In a specific corner
//! cn::toast("Top left toast")
//!     .corner(Corner::TopLeft)
//!     .show();
//!
//! // With custom body content
//! cn::toast("Download Complete")
//!     .body(|| {
//!         div().flex_col().gap_2()
//!             .child(text("Your file has been downloaded."))
//!             .child(
//!                 div().flex_row().gap_2()
//!                     .child(svg(file_icon).size(16.0, 16.0))
//!                     .child(text("report.pdf"))
//!             )
//!     })
//!     .show();
//!
//! // Fully custom toast content
//! cn::toast_custom(|| {
//!     div().flex_row().gap_3().p_4()
//!         .child(avatar("JD").size(40.0))
//!         .child(
//!             div().flex_col()
//!                 .child(text("John Doe").weight(600))
//!                 .child(text("Sent you a message"))
//!         )
//! })
//! .show();
//! ```

use std::sync::Arc;

use blinc_core::Color;
use blinc_layout::overlay_state::toast_tray;
use blinc_layout::prelude::*;
use blinc_layout::style;
use blinc_layout::widgets::overlay::Corner;
use blinc_layout::widgets::toast_tray::{ToastBuilder as TrayBuilder, ToastHandle};
use blinc_theme::{ColorToken, RadiusToken, ThemeState};

use super::button::{ButtonSize, ButtonVariant, button};

/// Toast visual variants
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToastVariant {
    /// Default toast styling
    #[default]
    Default,
    /// Success toast - green accent
    Success,
    /// Warning toast - yellow accent
    Warning,
    /// Destructive/error toast - red accent
    Destructive,
}

impl ToastVariant {
    /// Get the accent color for this variant
    fn accent_color(&self, theme: &ThemeState) -> Color {
        match self {
            ToastVariant::Default => theme.color(ColorToken::Primary),
            ToastVariant::Success => theme.color(ColorToken::Success),
            ToastVariant::Warning => theme.color(ColorToken::Warning),
            ToastVariant::Destructive => theme.color(ColorToken::Error),
        }
    }

    /// Get the icon SVG for this variant (optional)
    fn icon_svg(&self) -> Option<&'static str> {
        match self {
            ToastVariant::Default => None,
            ToastVariant::Success => Some(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 11.08V12a10 10 0 1 1-5.93-9.14"/><polyline points="22 4 12 14.01 9 11.01"/></svg>"#,
            ),
            ToastVariant::Warning => Some(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3Z"/><line x1="12" x2="12" y1="9" y2="13"/><line x1="12" x2="12.01" y1="17" y2="17"/></svg>"#,
            ),
            ToastVariant::Destructive => Some(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><line x1="15" x2="9" y1="9" y2="15"/><line x1="9" x2="15" y1="9" y2="15"/></svg>"#,
            ),
        }
    }
}

/// Content builder type for custom toast body
pub type ToastBodyFn = Arc<dyn Fn() -> Div + Send + Sync>;

/// Builder for creating and showing toast notifications
pub struct ToastBuilder {
    title: Option<String>,
    description: Option<String>,
    variant: ToastVariant,
    duration_ms: u32,
    corner: Corner,
    action_label: Option<String>,
    action_callback: Option<Arc<dyn Fn() + Send + Sync>>,
    show_close: bool,
    /// Custom body content (replaces title + description)
    body: Option<ToastBodyFn>,
    /// Fully custom content (replaces entire toast layout)
    custom_content: Option<ToastBodyFn>,
}

impl ToastBuilder {
    /// Create a new toast builder with a title
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: Some(title.into()),
            description: None,
            variant: ToastVariant::Default,
            duration_ms: 5000, // 5 seconds default
            corner: Corner::BottomRight,
            action_label: None,
            action_callback: None,
            show_close: true,
            body: None,
            custom_content: None,
        }
    }

    /// Create a toast builder with fully custom content
    pub fn custom<F>(content: F) -> Self
    where
        F: Fn() -> Div + Send + Sync + 'static,
    {
        Self {
            title: None,
            description: None,
            variant: ToastVariant::Default,
            duration_ms: 5000,
            corner: Corner::BottomRight,
            action_label: None,
            action_callback: None,
            show_close: false,
            body: None,
            custom_content: Some(Arc::new(content)),
        }
    }

    /// Set the toast description
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set custom body content (replaces description, keeps title)
    ///
    /// # Example
    ///
    /// ```ignore
    /// cn::toast("Download Complete")
    ///     .body(|| {
    ///         div().flex_row().gap_2()
    ///             .child(svg(file_icon).size(16.0, 16.0))
    ///             .child(text("report.pdf - 2.4 MB"))
    ///     })
    ///     .show();
    /// ```
    pub fn body<F>(mut self, content: F) -> Self
    where
        F: Fn() -> Div + Send + Sync + 'static,
    {
        self.body = Some(Arc::new(content));
        self
    }

    /// Set the toast variant
    pub fn variant(mut self, variant: ToastVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Set auto-dismiss duration in milliseconds
    pub fn duration_ms(mut self, ms: u32) -> Self {
        self.duration_ms = ms;
        self
    }

    /// Make the toast persistent (no auto-dismiss)
    pub fn persistent(mut self) -> Self {
        self.duration_ms = 0; // 0 = no auto-dismiss
        self
    }

    /// Set the corner position
    pub fn corner(mut self, corner: Corner) -> Self {
        self.corner = corner;
        self
    }

    /// Add an action button
    pub fn action<F>(mut self, label: impl Into<String>, callback: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.action_label = Some(label.into());
        self.action_callback = Some(Arc::new(callback));
        self
    }

    /// Show or hide the close button
    pub fn show_close(mut self, show: bool) -> Self {
        self.show_close = show;
        self
    }

    /// Show the toast
    #[track_caller]
    pub fn show(self) -> ToastHandle {
        let theme = ThemeState::get();
        let bg = theme.color(ColorToken::Surface);
        let border = theme.color(ColorToken::Border);
        let text_primary = theme.color(ColorToken::TextPrimary);
        let text_secondary = theme.color(ColorToken::TextSecondary);
        let radius = theme.radius(RadiusToken::Lg);

        let title = self.title;
        let description = self.description;
        let variant = self.variant;
        let accent_color = variant.accent_color(theme);
        let icon_svg = variant.icon_svg().map(|s| s.to_string());
        let action_label = self.action_label;
        let action_callback = self.action_callback;
        let show_close = self.show_close;
        let corner = self.corner;
        let duration_ms = self.duration_ms;
        let body = self.body;
        let custom_content = self.custom_content;

        // Pre-allocate handle id so action / close callbacks can capture the
        // dismiss target.
        let next_id = toast_tray()
            .lock()
            .ok()
            .map(|t| t.peek_next_handle_id())
            .unwrap_or(0);
        let toast_handle = ToastHandle::from_raw(next_id);

        // Slides in from its side by the `.cn-toast-motion-{side}` rules.
        let motion_class = match corner {
            Corner::TopLeft | Corner::BottomLeft => "cn-toast-motion-left",
            Corner::TopRight | Corner::BottomRight => "cn-toast-motion-right",
        };
        let mut builder = TrayBuilder::new()
            .corner(corner)
            .motion_class(motion_class)
            .dismiss_on_click(false) // dismiss via explicit close button / action
            .content(move || {
                build_toast_content(
                    &title,
                    &description,
                    &icon_svg,
                    accent_color,
                    variant,
                    bg,
                    border,
                    text_primary,
                    text_secondary,
                    radius,
                    &action_label,
                    &action_callback,
                    show_close,
                    &body,
                    &custom_content,
                    toast_handle,
                )
            });

        if duration_ms > 0 {
            builder = builder.auto_after_ms(duration_ms as u64);
        } else {
            // 0 in the legacy API meant "persistent". For the new tray that's
            // expressed as a very large auto_after_ms. Pick a 10-year sentinel
            // — toasts shouldn't realistically live that long.
            builder = builder.auto_after_ms(u64::MAX / 2);
        }

        builder.show()
    }
}

/// Build the toast content. Animation is delegated to CSS @keyframes
/// (cn-toast-enter — defined in cn_styles.rs). `toast_handle` is captured
/// so the action button and close button can dismiss this specific toast.
#[allow(clippy::too_many_arguments)]
fn build_toast_content(
    title: &Option<String>,
    description: &Option<String>,
    icon_svg: &Option<String>,
    accent_color: Color,
    variant: ToastVariant,
    bg: Color,
    border: Color,
    text_primary: Color,
    text_secondary: Color,
    radius: f32,
    action_label: &Option<String>,
    action_callback: &Option<Arc<dyn Fn() + Send + Sync>>,
    show_close: bool,
    body: &Option<ToastBodyFn>,
    custom_content: &Option<ToastBodyFn>,
    toast_handle: ToastHandle,
) -> Div {
    let theme = ThemeState::get();

    // If fully custom content is provided, use that directly
    if let Some(custom_fn) = custom_content {
        return custom_fn()
            .class("cn-toast")
            .w(360.0)
            .style(&style! { bg: bg, border: (1.0, border), rounded: radius })
            .shadow_lg();
    }

    let has_accent = icon_svg.is_some();

    let mut toast = div()
        .class("cn-toast")
        .w(360.0)
        .style(&style! { bg: bg, border: (1.0, border), rounded: radius })
        .shadow_lg();

    match variant {
        ToastVariant::Success => toast = toast.class("cn-toast--success"),
        ToastVariant::Warning => toast = toast.class("cn-toast--warning"),
        ToastVariant::Destructive => toast = toast.class("cn-toast--error"),
        ToastVariant::Default => toast = toast.class("cn-toast--info"),
    }

    if has_accent {
        toast = toast.border(1.0, accent_color);
    }

    let mut inner = div().w_full().flex_row().items_start().gap_3().p_4();

    if let Some(svg_str) = icon_svg {
        inner = inner.child(
            div()
                .flex_shrink_0()
                .child(svg(svg_str).size(20.0, 20.0).color(accent_color)),
        );
    }

    let mut content = div().flex_1().flex_col().gap_1();

    if let Some(title_text) = title {
        content = content.child(
            text(title_text)
                .size(theme.typography().text_sm)
                .color(text_primary)
                .medium(),
        );
    }

    if let Some(body_fn) = body {
        content = content.child(body_fn());
    } else if let Some(desc) = description {
        content = content.child(
            text(desc)
                .size(theme.typography().text_sm)
                .color(text_secondary),
        );
    }

    inner = inner.child(content);

    if let Some(label) = action_label {
        let callback = action_callback.clone();
        inner = inner.child(
            button(label)
                .variant(ButtonVariant::Outline)
                .size(ButtonSize::Small)
                .on_click(move |_| {
                    if let Some(ref cb) = callback {
                        cb();
                    }
                    toast_handle.dismiss();
                }),
        );
    }

    if show_close {
        let close_icon = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="18" x2="6" y1="6" y2="18"/><line x1="6" x2="18" y1="6" y2="18"/></svg>"#;

        inner = inner.child(
            div()
                .flex_shrink_0()
                .w(24.0)
                .h(24.0)
                .items_center()
                .style(&style! { rounded: theme.radius(RadiusToken::Sm) })
                .cursor_pointer()
                .on_click(move |_| {
                    toast_handle.dismiss();
                })
                .child(svg(close_icon).size(16.0, 16.0).color(text_secondary)),
        );
    }

    toast.child(inner)
}

/// Create a toast notification
///
/// # Example
///
/// ```ignore
/// cn::toast("Success!")
///     .description("Your file has been uploaded.")
///     .variant(ToastVariant::Success)
///     .show();
/// ```
pub fn toast(title: impl Into<String>) -> ToastBuilder {
    ToastBuilder::new(title)
}

/// Convenience function for success toasts
pub fn toast_success(title: impl Into<String>) -> ToastBuilder {
    ToastBuilder::new(title).variant(ToastVariant::Success)
}

/// Convenience function for warning toasts
pub fn toast_warning(title: impl Into<String>) -> ToastBuilder {
    ToastBuilder::new(title).variant(ToastVariant::Warning)
}

/// Convenience function for error toasts
pub fn toast_error(title: impl Into<String>) -> ToastBuilder {
    ToastBuilder::new(title).variant(ToastVariant::Destructive)
}

/// Create a toast with fully custom content
///
/// # Example
///
/// ```ignore
/// cn::toast_custom(|| {
///     div().flex_row().gap_3().p_4()
///         .child(avatar("JD").size(40.0))
///         .child(
///             div().flex_col()
///                 .child(text("John Doe").weight(600))
///                 .child(text("Sent you a message"))
///         )
/// })
/// .show();
/// ```
pub fn toast_custom<F>(content: F) -> ToastBuilder
where
    F: Fn() -> Div + Send + Sync + 'static,
{
    ToastBuilder::custom(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toast_builder() {
        let toast = ToastBuilder::new("Test");
        assert_eq!(toast.title, Some("Test".to_string()));
        assert_eq!(toast.duration_ms, 5000);
    }

    #[test]
    fn test_toast_with_description() {
        let toast = ToastBuilder::new("Title").description("Description");
        assert_eq!(toast.description, Some("Description".to_string()));
    }

    #[test]
    fn test_toast_persistent() {
        let toast = ToastBuilder::new("Persistent").persistent();
        assert_eq!(toast.duration_ms, 0);
    }

    #[test]
    fn test_toast_variants() {
        assert!(ToastVariant::Default.icon_svg().is_none());
        assert!(ToastVariant::Success.icon_svg().is_some());
        assert!(ToastVariant::Warning.icon_svg().is_some());
        assert!(ToastVariant::Destructive.icon_svg().is_some());
    }
}
