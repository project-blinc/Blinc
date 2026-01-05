//! Dialog component for modal dialogs
//!
//! A themed modal dialog that appears centered on screen with a backdrop.
//! Uses the overlay system for proper layering and dismissal.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! // Imperative API - show dialog from on_click handler
//! cn::button("Open Dialog")
//!     .on_click(|_| {
//!         cn::dialog()
//!             .title("Confirm Action")
//!             .description("Are you sure you want to proceed?")
//!             .on_confirm(|| {
//!                 tracing::info!("Confirmed!");
//!             })
//!             .show();
//!     })
//!
//! // Alert dialog (simpler API)
//! cn::alert_dialog()
//!     .title("Error")
//!     .description("Something went wrong.")
//!     .confirm_text("OK")
//!     .on_confirm(|| { /* handle confirm */ })
//!     .show()
//! ```

use std::sync::Arc;

use blinc_animation::{AnimationPreset, MultiKeyframeAnimation};
use blinc_core::Color;
use blinc_layout::motion::motion;
use blinc_layout::overlay_state::get_overlay_manager;
use blinc_layout::prelude::*;
use blinc_layout::widgets::overlay::{OverlayHandle, OverlayManagerExt};
use blinc_theme::{ColorToken, RadiusToken, SpacingToken, ThemeState};

use super::button::{button, ButtonVariant};

/// Dialog size variants
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DialogSize {
    /// Small dialog (max-width: 400px)
    Small,
    /// Medium dialog (max-width: 500px)
    #[default]
    Medium,
    /// Large dialog (max-width: 600px)
    Large,
    /// Full width dialog (max-width: 800px)
    Full,
}

impl DialogSize {
    /// Get the max width for this size
    pub fn max_width(&self) -> f32 {
        match self {
            DialogSize::Small => 400.0,
            DialogSize::Medium => 500.0,
            DialogSize::Large => 600.0,
            DialogSize::Full => 800.0,
        }
    }
}

/// Builder for creating and showing dialogs imperatively
///
/// Use `cn::dialog()` to create a builder, configure it, then call `.show()`.
pub struct DialogBuilder {
    title: Option<String>,
    description: Option<String>,
    content: Option<Arc<dyn Fn() -> Div + Send + Sync>>,
    footer: Option<Arc<dyn Fn() -> Div + Send + Sync>>,
    size: DialogSize,
    confirm_text: String,
    cancel_text: String,
    on_confirm: Option<Arc<dyn Fn() + Send + Sync>>,
    on_cancel: Option<Arc<dyn Fn() + Send + Sync>>,
    confirm_destructive: bool,
    show_cancel: bool,
    /// Custom enter animation (defaults to dialog_in)
    enter_animation: Option<MultiKeyframeAnimation>,
    /// Custom exit animation (defaults to dialog_out)
    exit_animation: Option<MultiKeyframeAnimation>,
    // key: InstanceKey,
}

impl DialogBuilder {
    /// Create a new dialog builder
    pub fn new() -> Self {
        Self {
            title: None,
            description: None,
            content: None,
            footer: None,
            size: DialogSize::Medium,
            confirm_text: "Confirm".to_string(),
            cancel_text: "Cancel".to_string(),
            on_confirm: None,
            on_cancel: None,
            confirm_destructive: false,
            show_cancel: true,
            enter_animation: None,
            exit_animation: None,
            // key: InstanceKey::new("dialog")
        }
    }

    /// Set the dialog title
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Set the dialog description
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set custom content for the dialog body
    pub fn content<F>(mut self, content: F) -> Self
    where
        F: Fn() -> Div + Send + Sync + 'static,
    {
        self.content = Some(Arc::new(content));
        self
    }

    /// Set custom footer content (replaces default buttons)
    pub fn footer<F>(mut self, footer: F) -> Self
    where
        F: Fn() -> Div + Send + Sync + 'static,
    {
        self.footer = Some(Arc::new(footer));
        self
    }

    /// Set the dialog size
    pub fn size(mut self, size: DialogSize) -> Self {
        self.size = size;
        self
    }

    /// Set the confirm button text
    pub fn confirm_text(mut self, text: impl Into<String>) -> Self {
        self.confirm_text = text.into();
        self
    }

    /// Set the cancel button text
    pub fn cancel_text(mut self, text: impl Into<String>) -> Self {
        self.cancel_text = text.into();
        self
    }

    /// Set whether the confirm button should be destructive (red)
    pub fn confirm_destructive(mut self, destructive: bool) -> Self {
        self.confirm_destructive = destructive;
        self
    }

    /// Hide the cancel button (for alert-style dialogs)
    pub fn hide_cancel(mut self) -> Self {
        self.show_cancel = false;
        self
    }

    /// Set the callback for when confirm is clicked
    pub fn on_confirm<F>(mut self, callback: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.on_confirm = Some(Arc::new(callback));
        self
    }

    /// Set the callback for when cancel is clicked
    pub fn on_cancel<F>(mut self, callback: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.on_cancel = Some(Arc::new(callback));
        self
    }

    /// Set a custom enter animation
    ///
    /// Overrides the default `AnimationPreset::grow_in(200)` animation.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use blinc_animation::AnimationPreset;
    ///
    /// cn::dialog()
    ///     .title("Custom Animation")
    ///     .enter_animation(AnimationPreset::bounce_in(300))
    ///     .show();
    ///
    /// // For more dramatic scale animation:
    /// cn::dialog()
    ///     .title("Scale Animation")
    ///     .enter_animation(AnimationPreset::dialog_in(200))
    ///     .exit_animation(AnimationPreset::dialog_out(150))
    ///     .show();
    /// ```
    pub fn enter_animation(mut self, animation: MultiKeyframeAnimation) -> Self {
        self.enter_animation = Some(animation);
        self
    }

    /// Set a custom exit animation
    ///
    /// Overrides the default `AnimationPreset::grow_out(150)` animation.
    pub fn exit_animation(mut self, animation: MultiKeyframeAnimation) -> Self {
        self.exit_animation = Some(animation);
        self
    }

    /// Show the dialog
    ///
    /// This creates a modal overlay with the dialog content.
    /// The dialog can be dismissed by clicking the backdrop, pressing Escape,
    /// or clicking the Cancel/Confirm buttons.
    pub fn show(self) -> OverlayHandle {
        let theme = ThemeState::get();
        let bg = theme.color(ColorToken::Surface);
        let border = theme.color(ColorToken::Border);
        let text_primary = theme.color(ColorToken::TextPrimary);
        let text_secondary = theme.color(ColorToken::TextSecondary);
        let radius = theme.radius(RadiusToken::Lg);
        let spacing = theme.spacing_value(SpacingToken::Space4);

        let title = self.title;
        let description = self.description;
        let content = self.content;
        let footer = self.footer;
        let max_width = self.size.max_width();
        let confirm_text = self.confirm_text;
        let cancel_text = self.cancel_text;
        let on_confirm = self.on_confirm;
        let on_cancel = self.on_cancel;
        let confirm_destructive = self.confirm_destructive;
        let show_cancel = self.show_cancel;
        // Use grow_in/grow_out by default - gentler scale (99% → 100%) to reduce text distortion
        let enter_animation = self
            .enter_animation
            .unwrap_or_else(|| AnimationPreset::grow_in(200));
        let exit_animation = self
            .exit_animation
            .unwrap_or_else(|| AnimationPreset::grow_out(150));

        let mgr = get_overlay_manager();
        // let overlay_key = self.key.derive("overlay_handle");

        mgr.modal()
            .dismiss_on_escape(true)
            .content(move || {
                build_dialog_content(
                    &title,
                    &description,
                    &content,
                    &footer,
                    max_width,
                    bg,
                    border,
                    text_primary,
                    text_secondary,
                    radius,
                    spacing,
                    &confirm_text,
                    &cancel_text,
                    &on_confirm,
                    &on_cancel,
                    confirm_destructive,
                    show_cancel,
                    &enter_animation,
                    &exit_animation,
                )
            })
            .show()
    }
}

impl Default for DialogBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a new dialog builder
///
/// # Example
///
/// ```ignore
/// cn::dialog()
///     .title("Edit Profile")
///     .description("Make changes to your profile.")
///     .on_confirm(|| { /* save */ })
///     .show();
/// ```
pub fn dialog() -> DialogBuilder {
    DialogBuilder::new()
}

/// Builder for alert dialogs (single button confirmation)
pub struct AlertDialogBuilder {
    inner: DialogBuilder,
}

impl AlertDialogBuilder {
    /// Create a new alert dialog builder
    pub fn new() -> Self {
        Self {
            inner: DialogBuilder::new().hide_cancel(),
        }
    }

    /// Set the dialog title
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.inner = self.inner.title(title);
        self
    }

    /// Set the dialog description
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.inner = self.inner.description(description);
        self
    }

    /// Set the confirm button text (default: "OK")
    pub fn confirm_text(mut self, text: impl Into<String>) -> Self {
        self.inner = self.inner.confirm_text(text);
        self
    }

    /// Set the callback for when confirm is clicked
    pub fn on_confirm<F>(mut self, callback: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.inner = self.inner.on_confirm(callback);
        self
    }

    /// Set the dialog size
    pub fn size(mut self, size: DialogSize) -> Self {
        self.inner = self.inner.size(size);
        self
    }

    /// Show the alert dialog
    pub fn show(self) -> OverlayHandle {
        self.inner.show()
    }
}

impl Default for AlertDialogBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a new alert dialog builder (single button confirmation)
///
/// # Example
///
/// ```ignore
/// cn::alert_dialog()
///     .title("Information")
///     .description("Operation completed.")
///     .confirm_text("OK")
///     .show();
/// ```
pub fn alert_dialog() -> AlertDialogBuilder {
    AlertDialogBuilder::new()
}

/// Build the dialog content wrapped in motion for animations
#[allow(clippy::too_many_arguments)]
fn build_dialog_content(
    title: &Option<String>,
    description: &Option<String>,
    content: &Option<Arc<dyn Fn() -> Div + Send + Sync>>,
    footer: &Option<Arc<dyn Fn() -> Div + Send + Sync>>,
    max_width: f32,
    bg: Color,
    border: Color,
    text_primary: Color,
    text_secondary: Color,
    radius: f32,
    _spacing: f32,
    confirm_text: &str,
    _cancel_text: &str,
    on_confirm: &Option<Arc<dyn Fn() + Send + Sync>>,
    on_cancel: &Option<Arc<dyn Fn() + Send + Sync>>,
    confirm_destructive: bool,
    show_cancel: bool,
    enter_animation: &MultiKeyframeAnimation,
    exit_animation: &MultiKeyframeAnimation,
) -> Div {
    // Use theme spacing tokens via helper methods (.p_6(), .gap_2(), .m_4(), etc.)
    let theme = ThemeState::get();

    // Build inner content that will have its own fade animation
    // This helps mask any visual distortion from the outer scale animation
    let mut inner_content = div().w_full().flex_col();

    // Header section
    if title.is_some() || description.is_some() {
        let mut header = div().w_full().flex_col().gap_2(); // 8px gap from theme

        if let Some(ref title_text) = title {
            header = header.child(h3(title_text).color(text_primary));
        }

        if let Some(ref desc_text) = description {
            header = header.child(
                text(desc_text)
                    .size(theme.typography().text_sm)
                    .color(text_secondary),
            );
        }

        inner_content = inner_content.child(header);
    }

    // Custom content
    if let Some(ref content_fn) = content {
        inner_content = inner_content.child(
            div()
                .w_full()
                .mt(theme.spacing().space_2)
                .child(content_fn()),
        ); // 16px margin from theme
    }

    // Footer - either custom or default buttons
    let footer_content = if let Some(ref footer_fn) = footer {
        footer_fn()
    } else {
        // Default footer with buttons
        let mut footer_div = div().w_full().flex_row().gap_2().justify_end(); // 8px gap from theme

        if show_cancel {
            let on_cancel = on_cancel.clone();
            footer_div =
                footer_div.child(button("Cancel").variant(ButtonVariant::Outline).on_click(
                    move |_| {
                        if let Some(ref cb) = on_cancel {
                            cb();
                        }
                        // Get fresh overlay manager to close
                        get_overlay_manager().close_top();
                    },
                ));
        }

        let on_confirm = on_confirm.clone();
        let confirm_text = confirm_text.to_string();
        footer_div = footer_div.child(
            button(&confirm_text)
                .variant(if confirm_destructive {
                    ButtonVariant::Destructive
                } else {
                    ButtonVariant::Primary
                })
                .on_click(move |_| {
                    if let Some(ref cb) = on_confirm {
                        cb();
                    }
                    // Get fresh overlay manager to close
                    get_overlay_manager().close_top();
                }),
        );

        footer_div
    };

    // Add footer to inner content
    inner_content = inner_content.child(
        div()
            .w_full()
            .mt(theme.spacing().space_2)
            .child(footer_content),
    ); // 16px margin from theme

    // Wrap inner content in a motion container with fade-in
    // This helps mask visual distortion from the outer scale animation
    let animated_inner = motion()
        .enter_animation(AnimationPreset::fade_in(150))
        .exit_animation(AnimationPreset::fade_out(100))
        .child(inner_content);

    // Build the dialog container (card styling)
    let dialog = div()
        .min_w(300.0)
        .max_w(max_width)
        .bg(bg)
        .border(1.0, border)
        .rounded(radius)
        .shadow_xl()
        .flex_col()
        .p_6() // 24px padding from theme
        .child(animated_inner);

    // Wrap dialog in outer motion container for scale+fade animations
    div().child(
        motion()
            .enter_animation(enter_animation.clone())
            .exit_animation(exit_animation.clone())
            .child(dialog)
            .replay(),
    )
}

// Keep the old types for backwards compatibility but mark as deprecated
#[doc(hidden)]
pub type Dialog = DialogBuilder;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dialog_builder() {
        let builder = dialog()
            .title("Test")
            .description("Description")
            .confirm_text("OK");

        assert_eq!(builder.title, Some("Test".to_string()));
        assert_eq!(builder.description, Some("Description".to_string()));
        assert_eq!(builder.confirm_text, "OK");
    }

    #[test]
    fn test_alert_dialog_builder() {
        let builder = alert_dialog().title("Alert").confirm_text("Got it");

        assert_eq!(builder.inner.title, Some("Alert".to_string()));
        assert_eq!(builder.inner.confirm_text, "Got it");
        assert!(!builder.inner.show_cancel);
    }

    #[test]
    fn test_dialog_sizes() {
        assert_eq!(DialogSize::Small.max_width(), 400.0);
        assert_eq!(DialogSize::Medium.max_width(), 500.0);
        assert_eq!(DialogSize::Large.max_width(), 600.0);
        assert_eq!(DialogSize::Full.max_width(), 800.0);
    }
}
