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
//! fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
//!     let is_open = ctx.use_state_keyed("dialog_open", || false);
//!
//!     div()
//!         .child(
//!             cn::button("Open Dialog")
//!                 .on_click({
//!                     let is_open = is_open.clone();
//!                     move |_| is_open.set(true)
//!                 })
//!         )
//!         .child(
//!             cn::dialog(&is_open)
//!                 .title("Confirm Action")
//!                 .description("Are you sure you want to proceed?")
//!                 .footer(
//!                     div().flex_row().gap(2.0)
//!                         .child(cn::button("Cancel").variant(ButtonVariant::Outline))
//!                         .child(cn::button("Confirm"))
//!                 )
//!         )
//! }
//!
//! // Alert dialog (simpler API)
//! cn::alert_dialog(&is_open)
//!     .title("Error")
//!     .description("Something went wrong.")
//!     .confirm_text("OK")
//!     .on_confirm(|| { /* handle confirm */ })
//! ```

use std::cell::OnceCell;
use std::sync::Arc;

use blinc_core::context_state::BlincContextState;
use blinc_core::{Color, State};
use blinc_layout::div::ElementTypeId;
use blinc_layout::element::RenderProps;
use blinc_layout::overlay_state::get_overlay_manager;
use blinc_layout::prelude::*;
use blinc_layout::stateful::Stateful;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
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
    fn max_width(&self) -> f32 {
        match self {
            DialogSize::Small => 400.0,
            DialogSize::Medium => 500.0,
            DialogSize::Large => 600.0,
            DialogSize::Full => 800.0,
        }
    }
}

/// Dialog component
///
/// A modal dialog that appears centered on screen with a backdrop.
/// Uses state-driven reactivity for open/close behavior.
pub struct Dialog {
    /// The fully-built inner element (empty placeholder - actual dialog is in overlay)
    inner: Div,
}

impl Dialog {
    /// Create from a full configuration
    fn from_config(config: DialogConfig) -> Self {
        let theme = ThemeState::get();
        let open_state = config.open_state.clone();

        // Create internal state for overlay handle
        let handle_key = format!(
            "_dialog_handle_{}",
            config.open_state.signal_id().to_raw()
        );
        let overlay_handle_state: State<Option<u64>> =
            BlincContextState::get().use_state_keyed(&handle_key, || None);

        let is_open = open_state.get();

        // Check if we need to show/hide the dialog
        if is_open {
            // Check if overlay is already shown
            if overlay_handle_state.get().is_none() {
                // Show the dialog overlay
                let handle = show_dialog_overlay(&config, &open_state, &overlay_handle_state);
                overlay_handle_state.set(Some(handle.id()));
            }
        } else {
            // Close overlay if it exists
            if let Some(handle_id) = overlay_handle_state.get() {
                let mgr = get_overlay_manager();
                mgr.close(OverlayHandle::from_raw(handle_id));
                overlay_handle_state.set(None);
            }
        }

        // The actual dialog renders nothing in the main tree - it's all in the overlay
        // But we use Stateful to ensure re-render when open state changes
        let placeholder = Stateful::<()>::new(())
            .deps(&[open_state.signal_id()])
            .w(0.0)
            .h(0.0)
            .on_state(|_, container| {
                // Empty placeholder
                container.merge(div().w(0.0).h(0.0));
            });

        Self {
            inner: div().child(placeholder),
        }
    }
}

impl ElementBuilder for Dialog {
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
}

/// Show the dialog overlay
fn show_dialog_overlay(
    config: &DialogConfig,
    open_state: &State<bool>,
    overlay_handle_state: &State<Option<u64>>,
) -> OverlayHandle {
    let theme = ThemeState::get();
    let bg = theme.color(ColorToken::Surface);
    let border = theme.color(ColorToken::Border);
    let text_primary = theme.color(ColorToken::TextPrimary);
    let text_secondary = theme.color(ColorToken::TextSecondary);
    let radius = theme.radius(RadiusToken::Lg);
    let spacing = theme.spacing_value(SpacingToken::Space4);

    let title = config.title.clone();
    let description = config.description.clone();
    let content = config.content.clone();
    let footer = config.footer.clone();
    let max_width = config.size.max_width();
    let on_close = config.on_close.clone();

    let open_state_for_close = open_state.clone();
    let handle_state_for_close = overlay_handle_state.clone();

    let mgr = get_overlay_manager();
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
                &open_state_for_close,
                &handle_state_for_close,
                &on_close,
            )
        })
        .show()
}

/// Build the dialog content div
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
    spacing: f32,
    open_state: &State<bool>,
    overlay_handle_state: &State<Option<u64>>,
    on_close: &Option<Arc<dyn Fn() + Send + Sync>>,
) -> Div {
    let handle_state_for_ready = overlay_handle_state.clone();

    let mut dialog = div()
        .flex_col()
        .w(max_width)
        .bg(bg)
        .border(1.0, border)
        .rounded(radius)
        .shadow_xl()
        .overflow_clip()
        .p(spacing)
        .gap(spacing)
        .on_ready(move |bounds| {
            // Report actual content size to overlay manager
            if let Some(handle_id) = handle_state_for_ready.get() {
                let mgr = get_overlay_manager();
                mgr.set_content_size(
                    OverlayHandle::from_raw(handle_id),
                    bounds.width,
                    bounds.height,
                );
            }
        });

    // Header section (title + description)
    if title.is_some() || description.is_some() {
        let mut header = div().flex_col().gap(spacing / 2.0);

        if let Some(ref title_text) = title {
            header = header.child(
                text(title_text)
                    .size(18.0)
                    .weight(FontWeight::SemiBold)
                    .color(text_primary),
            );
        }

        if let Some(ref desc_text) = description {
            header = header.child(
                text(desc_text)
                    .size(14.0)
                    .color(text_secondary),
            );
        }

        dialog = dialog.child(header);
    }

    // Custom content section
    if let Some(ref content_fn) = content {
        dialog = dialog.child(content_fn());
    }

    // Footer section
    if let Some(ref footer_fn) = footer {
        dialog = dialog.child(
            div()
                .flex_row()
                .justify_end()
                .gap(spacing / 2.0)
                .pt(spacing / 2.0)
                .child(footer_fn()),
        );
    }

    dialog
}

/// Internal configuration for building a Dialog
#[derive(Clone)]
struct DialogConfig {
    open_state: State<bool>,
    title: Option<String>,
    description: Option<String>,
    content: Option<Arc<dyn Fn() -> Div + Send + Sync>>,
    footer: Option<Arc<dyn Fn() -> Div + Send + Sync>>,
    size: DialogSize,
    on_close: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl DialogConfig {
    fn new(open_state: State<bool>) -> Self {
        Self {
            open_state,
            title: None,
            description: None,
            content: None,
            footer: None,
            size: DialogSize::default(),
            on_close: None,
        }
    }
}

/// Builder for creating Dialog components with fluent API
pub struct DialogBuilder {
    config: DialogConfig,
    /// Cached built Dialog - built lazily on first access
    built: OnceCell<Dialog>,
}

impl DialogBuilder {
    /// Create a new dialog builder with open state
    pub fn new(open_state: &State<bool>) -> Self {
        Self {
            config: DialogConfig::new(open_state.clone()),
            built: OnceCell::new(),
        }
    }

    /// Get or build the inner Dialog
    fn get_or_build(&self) -> &Dialog {
        self.built
            .get_or_init(|| Dialog::from_config(self.config.clone()))
    }

    /// Set the dialog title
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.config.title = Some(title.into());
        self
    }

    /// Set the dialog description
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.config.description = Some(description.into());
        self
    }

    /// Set custom content
    pub fn content<F>(mut self, f: F) -> Self
    where
        F: Fn() -> Div + Send + Sync + 'static,
    {
        self.config.content = Some(Arc::new(f));
        self
    }

    /// Set the footer content (typically buttons)
    pub fn footer<F>(mut self, f: F) -> Self
    where
        F: Fn() -> Div + Send + Sync + 'static,
    {
        self.config.footer = Some(Arc::new(f));
        self
    }

    /// Set the dialog size
    pub fn size(mut self, size: DialogSize) -> Self {
        self.config.size = size;
        self
    }

    /// Set callback when dialog is closed
    pub fn on_close<F>(mut self, f: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.config.on_close = Some(Arc::new(f));
        self
    }
}

impl ElementBuilder for DialogBuilder {
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
}

/// Create a dialog with open state
///
/// # Example
///
/// ```ignore
/// let is_open = ctx.use_state_keyed("dialog", || false);
///
/// cn::dialog(&is_open)
///     .title("Confirm")
///     .description("Are you sure?")
///     .footer(|| {
///         div().flex_row().gap(2.0)
///             .child(cn::button("Cancel"))
///             .child(cn::button("OK"))
///     })
/// ```
pub fn dialog(open_state: &State<bool>) -> DialogBuilder {
    DialogBuilder::new(open_state)
}

// =============================================================================
// Alert Dialog - Simpler API for confirmation dialogs
// =============================================================================

/// Builder for alert dialogs (simpler API for confirmations)
pub struct AlertDialogBuilder {
    open_state: State<bool>,
    title: Option<String>,
    description: Option<String>,
    confirm_text: String,
    cancel_text: Option<String>,
    on_confirm: Option<Arc<dyn Fn() + Send + Sync>>,
    on_cancel: Option<Arc<dyn Fn() + Send + Sync>>,
    size: DialogSize,
    destructive: bool,
}

impl AlertDialogBuilder {
    /// Create a new alert dialog builder
    pub fn new(open_state: &State<bool>) -> Self {
        Self {
            open_state: open_state.clone(),
            title: None,
            description: None,
            confirm_text: "OK".to_string(),
            cancel_text: None,
            on_confirm: None,
            on_cancel: None,
            size: DialogSize::Small,
            destructive: false,
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

    /// Set the confirm button text
    pub fn confirm_text(mut self, text: impl Into<String>) -> Self {
        self.confirm_text = text.into();
        self
    }

    /// Set the cancel button text (if None, no cancel button is shown)
    pub fn cancel_text(mut self, text: impl Into<String>) -> Self {
        self.cancel_text = Some(text.into());
        self
    }

    /// Set callback when confirm is clicked
    pub fn on_confirm<F>(mut self, f: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.on_confirm = Some(Arc::new(f));
        self
    }

    /// Set callback when cancel is clicked
    pub fn on_cancel<F>(mut self, f: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.on_cancel = Some(Arc::new(f));
        self
    }

    /// Make this a destructive action (red confirm button)
    pub fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }

    /// Set the dialog size
    pub fn size(mut self, size: DialogSize) -> Self {
        self.size = size;
        self
    }

    /// Build the alert dialog
    pub fn build_dialog(self) -> DialogBuilder {
        let open_state = self.open_state.clone();
        let on_confirm = self.on_confirm.clone();
        let on_cancel = self.on_cancel.clone();
        let confirm_text = self.confirm_text.clone();
        let cancel_text = self.cancel_text.clone();
        let destructive = self.destructive;

        let open_for_confirm = open_state.clone();
        let open_for_cancel = open_state.clone();

        let mut builder = dialog(&open_state).size(self.size);

        if let Some(title) = self.title {
            builder = builder.title(title);
        }

        if let Some(description) = self.description {
            builder = builder.description(description);
        }

        builder = builder.footer(move || {
            let mut footer = div().flex_row().justify_end().gap(2.0);

            // Cancel button (if text provided)
            if let Some(ref cancel) = cancel_text {
                let open_for_cancel = open_for_cancel.clone();
                let on_cancel = on_cancel.clone();
                footer = footer.child(
                    button(cancel)
                        .variant(ButtonVariant::Outline)
                        .on_click(move |_| {
                            open_for_cancel.set(false);
                            if let Some(ref cb) = on_cancel {
                                cb();
                            }
                        }),
                );
            }

            // Confirm button
            let open_for_confirm = open_for_confirm.clone();
            let on_confirm = on_confirm.clone();
            let variant = if destructive {
                ButtonVariant::Destructive
            } else {
                ButtonVariant::Primary
            };

            footer = footer.child(
                button(&confirm_text)
                    .variant(variant)
                    .on_click(move |_| {
                        open_for_confirm.set(false);
                        if let Some(ref cb) = on_confirm {
                            cb();
                        }
                    }),
            );

            footer
        });

        builder
    }
}

impl ElementBuilder for AlertDialogBuilder {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        // Clone self to build the dialog
        let builder = AlertDialogBuilder {
            open_state: self.open_state.clone(),
            title: self.title.clone(),
            description: self.description.clone(),
            confirm_text: self.confirm_text.clone(),
            cancel_text: self.cancel_text.clone(),
            on_confirm: self.on_confirm.clone(),
            on_cancel: self.on_cancel.clone(),
            size: self.size,
            destructive: self.destructive,
        };
        builder.build_dialog().build(tree)
    }

    fn render_props(&self) -> RenderProps {
        RenderProps::default()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        &[]
    }

    fn element_type_id(&self) -> ElementTypeId {
        ElementTypeId::Div
    }
}

/// Create an alert dialog (simpler API for confirmations)
///
/// # Example
///
/// ```ignore
/// let is_open = ctx.use_state_keyed("confirm", || false);
///
/// cn::alert_dialog(&is_open)
///     .title("Delete Item?")
///     .description("This action cannot be undone.")
///     .confirm_text("Delete")
///     .cancel_text("Cancel")
///     .destructive()
///     .on_confirm(|| { /* delete item */ })
/// ```
pub fn alert_dialog(open_state: &State<bool>) -> AlertDialogBuilder {
    AlertDialogBuilder::new(open_state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dialog_sizes() {
        assert_eq!(DialogSize::Small.max_width(), 400.0);
        assert_eq!(DialogSize::Medium.max_width(), 500.0);
        assert_eq!(DialogSize::Large.max_width(), 600.0);
        assert_eq!(DialogSize::Full.max_width(), 800.0);
    }
}
