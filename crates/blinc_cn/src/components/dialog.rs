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

use blinc_core::Color;
use blinc_layout::InstanceKey;
use blinc_layout::overlay_state::overlay_stack;
use blinc_layout::prelude::*;
use blinc_layout::widgets::overlay_stack::{OverlayBuilder, OverlayHandle};
use blinc_theme::{ColorToken, RadiusToken, SpacingToken, ThemeState};

use super::button::{ButtonVariant, button};
use crate::reactive_props::clone_reactive;
use blinc_layout::binding::{IntoReactive, Reactive};

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
    title: Option<Reactive<String>>,
    description: Option<Reactive<String>>,
    content: Option<Arc<dyn Fn() -> Div + Send + Sync>>,
    footer: Option<Arc<dyn Fn() -> Div + Send + Sync>>,
    size: DialogSize,
    confirm_text: Reactive<String>,
    cancel_text: Reactive<String>,
    on_confirm: Option<Arc<dyn Fn() + Send + Sync>>,
    on_cancel: Option<Arc<dyn Fn() + Send + Sync>>,
    on_close: Option<Arc<dyn Fn() + Send + Sync>>,
    confirm_destructive: bool,
    show_cancel: bool,
    /// User-added CSS classes
    classes: Vec<std::sync::Arc<str>>,
    /// User-set element ID
    user_id: Option<String>,
    /// Unique key for motion animation
    key: InstanceKey,
}

impl DialogBuilder {
    /// Create a new dialog builder
    #[track_caller]
    pub fn new() -> Self {
        Self {
            title: None,
            description: None,
            content: None,
            footer: None,
            size: DialogSize::Medium,
            confirm_text: Reactive::Const("Confirm".to_string()),
            cancel_text: Reactive::Const("Cancel".to_string()),
            on_confirm: None,
            on_cancel: None,
            on_close: None,
            confirm_destructive: false,
            show_cancel: true,
            classes: Vec::new(),
            user_id: None,
            key: InstanceKey::new("dialog"),
        }
    }

    /// Set the dialog title. A signal here replaces the text in place.
    pub fn title(mut self, title: impl IntoReactive<String>) -> Self {
        self.title = Some(title.into_reactive());
        self
    }

    /// Set the dialog description. A signal here replaces the text in place.
    pub fn description(mut self, description: impl IntoReactive<String>) -> Self {
        self.description = Some(description.into_reactive());
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
    pub fn confirm_text(mut self, text: impl IntoReactive<String>) -> Self {
        self.confirm_text = text.into_reactive();
        self
    }

    /// Set the cancel button text
    pub fn cancel_text(mut self, text: impl IntoReactive<String>) -> Self {
        self.cancel_text = text.into_reactive();
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

    /// Fired when the dialog closes, however it was dismissed.
    ///
    /// `on_confirm` and `on_cancel` are the BUTTONS. A backdrop click or
    /// an Escape closes the dialog without either firing, so a caller
    /// binding a signal to `open` had no way to learn it closed: the
    /// signal stayed set and the dialog returned on the next mount.
    pub fn on_close<F>(mut self, callback: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.on_close = Some(Arc::new(callback));
        self
    }

    /// Add a CSS class for selector matching
    pub fn class(mut self, name: impl AsRef<str>) -> Self {
        self.classes.push(blinc_core::intern::intern(name.as_ref()));
        self
    }

    /// Set the element ID for CSS selector matching
    pub fn id(mut self, id: &str) -> Self {
        self.user_id = Some(id.to_string());
        self
    }

    /// Show the dialog
    ///
    /// This creates a modal overlay with the dialog content.
    /// The dialog can be dismissed by clicking the backdrop, pressing Escape,
    /// or clicking the Cancel/Confirm buttons.
    pub fn show(self) -> OverlayHandle {
        let theme = ThemeState::get();
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
        let on_close = self.on_close;
        let confirm_destructive = self.confirm_destructive;
        let show_cancel = self.show_cancel;
        let classes = self.classes;
        let user_id = self.user_id;

        // Pre-allocate the handle id so the confirm/cancel buttons can capture
        // it before show() is called (same pattern as popover). Buttons close
        // the dialog by calling `OverlayHandle::close()` on this captured id
        // instead of `close_top()` so they target THIS dialog even if a newer
        // one was pushed on top.
        let next_handle_id = overlay_stack()
            .lock()
            .ok()
            .map(|s| s.peek_next_handle_id())
            .unwrap_or(0);
        let dialog_handle = OverlayHandle::from_raw(next_handle_id);

        // Every dismissal route, not just the two buttons.
        let on_close_any = on_close.clone();
        let handle = OverlayBuilder::dialog()
            .on_close(move |_reason| {
                if let Some(ref cb) = on_close_any {
                    cb();
                }
            })
            // Dialog defaults (DismissRules::default_for(Dialog)):
            // on_escape=true, on_click_outside=true (backdrop click), backdrop=Some.
            // Rises in and out by the `.cn-dialog-motion` rules.
            .motion_class("cn-dialog-motion")
            .content(move || {
                let mut content_div = build_dialog_content(
                    &title,
                    &description,
                    &content,
                    &footer,
                    max_width,
                    spacing,
                    &confirm_text,
                    &cancel_text,
                    &on_confirm,
                    &on_cancel,
                    confirm_destructive,
                    show_cancel,
                    dialog_handle,
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
#[track_caller]
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
    pub fn title(mut self, title: impl IntoReactive<String>) -> Self {
        self.inner = self.inner.title(title);
        self
    }

    /// Set the dialog description
    pub fn description(mut self, description: impl IntoReactive<String>) -> Self {
        self.inner = self.inner.description(description);
        self
    }

    /// Set the confirm button text (default: "OK")
    pub fn confirm_text(mut self, text: impl IntoReactive<String>) -> Self {
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
#[track_caller]
pub fn alert_dialog() -> AlertDialogBuilder {
    AlertDialogBuilder::new()
}

/// Build the dialog content. Enter animation is delegated to the CSS
/// `@keyframes cn-dialog-enter` rule on `.cn-dialog` (see cn_styles.rs);
/// the wrapping `motion_derived` lives at the `OverlayStack` layer.
#[allow(clippy::too_many_arguments)]
fn build_dialog_content(
    title: &Option<Reactive<String>>,
    description: &Option<Reactive<String>>,
    content: &Option<Arc<dyn Fn() -> Div + Send + Sync>>,
    footer: &Option<Arc<dyn Fn() -> Div + Send + Sync>>,
    max_width: f32,
    _spacing: f32,
    confirm_text: &Reactive<String>,
    cancel_text: &Reactive<String>,
    on_confirm: &Option<Arc<dyn Fn() + Send + Sync>>,
    on_cancel: &Option<Arc<dyn Fn() + Send + Sync>>,
    confirm_destructive: bool,
    show_cancel: bool,
    handle: OverlayHandle,
) -> Div {
    // Use theme spacing tokens via helper methods (.p_6(), .gap_2(), .m_4(), etc.)
    let theme = ThemeState::get();

    let mut inner_content = div().w_full().flex_col();

    // Header section
    if title.is_some() || description.is_some() {
        let mut header = div().w_full().flex_col().gap_2(); // 8px gap from theme

        // Their colours are `.cn-dialog-title` and `.cn-dialog-description`.
        if let Some(title_text) = title {
            header = header.child(
                blinc_layout::typography::heading(3, clone_reactive(title_text))
                    .class("cn-dialog-title"),
            );
        }

        if let Some(desc_text) = description {
            header = header.child(
                blinc_layout::text::Text::bound(clone_reactive(desc_text))
                    .class("cn-dialog-description")
                    .size(theme.typography().text_sm),
            );
        }

        inner_content = inner_content.child(header);
    }

    // Custom content
    if let Some(content_fn) = content {
        inner_content = inner_content.child(
            div()
                .w_full()
                .mt(theme.spacing().space_2)
                .child(content_fn()),
        ); // 16px margin from theme
    }

    // Footer - either custom or default buttons
    let footer_content = if let Some(footer_fn) = footer {
        footer_fn()
    } else {
        // Default footer with buttons
        let mut footer_div = div().w_full().flex_row().gap_2().justify_end(); // 8px gap from theme

        if show_cancel {
            let on_cancel = on_cancel.clone();
            footer_div = footer_div.child(
                button(clone_reactive(cancel_text))
                    .variant(ButtonVariant::Outline)
                    .on_click(move |_| {
                        if let Some(ref cb) = on_cancel {
                            cb();
                        }
                        handle.close();
                    }),
            );
        }

        let on_confirm = on_confirm.clone();
        footer_div = footer_div.child(
            button(clone_reactive(confirm_text))
                .variant(if confirm_destructive {
                    ButtonVariant::Destructive
                } else {
                    ButtonVariant::Primary
                })
                .on_click(move |_| {
                    if let Some(ref cb) = on_confirm {
                        cb();
                    }
                    handle.close();
                }),
        );

        footer_div
    };

    inner_content = inner_content.child(
        div()
            .w_full()
            .mt(theme.spacing().space_2)
            .child(footer_content),
    ); // 16px margin from theme

    // The panel: fill, border, radius and shadow are `.cn-dialog` rules.
    div()
        .class("cn-dialog")
        .min_w(300.0)
        .max_w(max_width)
        .flex_col()
        .child(inner_content)
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

        let read = |r: &Option<Reactive<String>>| r.as_ref().map(crate::reactive_props::current);
        assert_eq!(read(&builder.title).as_deref(), Some("Test"));
        assert_eq!(read(&builder.description).as_deref(), Some("Description"));
        assert_eq!(crate::reactive_props::current(&builder.confirm_text), "OK");
    }

    #[test]
    fn test_alert_dialog_builder() {
        let builder = alert_dialog().title("Alert").confirm_text("Got it");

        let title = builder
            .inner
            .title
            .as_ref()
            .map(crate::reactive_props::current);
        assert_eq!(title.as_deref(), Some("Alert"));
        assert_eq!(
            crate::reactive_props::current(&builder.inner.confirm_text),
            "Got it"
        );
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
