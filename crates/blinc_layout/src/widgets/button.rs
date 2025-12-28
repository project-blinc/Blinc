//! Ready-to-use Button widget
//!
//! A button with built-in hover, press, and disabled states using FSM-driven
//! state management via `Stateful<ButtonState>`. This provides efficient
//! incremental prop updates without tree rebuilds.
//!
//! # Example
//!
//! ```ignore
//! // Simple text button - state persists across rebuilds
//! let btn_state = ctx.use_state_for("my_button", ButtonState::Idle);
//! button(btn_state, "Click me")
//!     .on_click(|_| println!("Clicked!"))
//!     .bg_color(Color::RED)
//!     .hover_color(Color::GREEN)
//!
//! // Button with custom child content
//! let save_btn_state = ctx.use_state_for("save_btn", ButtonState::Idle);
//! button_with(save_btn_state, |_state| {
//!     div().flex_row().gap(8.0)
//!         .child(svg_icon("save"))
//!         .child(text("Save"))
//! })
//! .on_click(|_| save())
//! ```

use std::sync::{Arc, Mutex};

use blinc_core::reactive::SignalId;
use blinc_core::Color;

use crate::div::{div, Div, ElementBuilder};
use crate::element::RenderProps;
use crate::stateful::{ButtonState, SharedState, Stateful};
use crate::text::text;
use crate::tree::{LayoutNodeId, LayoutTree};

/// Button visual states (re-exported from stateful)
pub use crate::stateful::ButtonState as ButtonVisualState;

/// Button-specific configuration (colors)
#[derive(Clone)]
pub struct ButtonConfig {
    pub label: Option<String>,
    pub text_color: Color,
    pub text_size: f32,
    pub bg_color: Color,
    pub hover_color: Color,
    pub pressed_color: Color,
    pub disabled_color: Color,
    pub disabled: bool,
}

impl Default for ButtonConfig {
    fn default() -> Self {
        Self {
            label: None,
            text_color: Color::WHITE,
            text_size: 16.0,
            bg_color: Color::rgba(0.2, 0.5, 0.9, 1.0),
            hover_color: Color::rgba(0.3, 0.6, 1.0, 1.0),
            pressed_color: Color::rgba(0.15, 0.4, 0.8, 1.0),
            disabled_color: Color::rgba(0.3, 0.3, 0.35, 0.5),
            disabled: false,
        }
    }
}

type ClickHandler = Arc<dyn Fn(&crate::event_handler::EventContext) + Send + Sync>;
type StateCallback = Arc<dyn Fn(ButtonState, &mut Div) + Send + Sync>;

/// Button widget - wraps Stateful<ButtonState>
///
/// Buttons can have custom content via `button_with()` or use the simple
/// `button("Label")` constructor for text-only buttons.
pub struct Button {
    inner: Stateful<ButtonState>,
    config: Arc<Mutex<ButtonConfig>>,
    click_handler: Option<ClickHandler>,
    custom_state_callback: Option<StateCallback>,
    extra_deps: Vec<SignalId>,
}

impl Button {
    /// Create a button with a text label and externally-managed state
    ///
    /// The state handle should be created via `ctx.use_state_for()` for persistence
    /// across rebuilds. The label text color can be customized with `.text_color()`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let btn_state = ctx.use_state_for("my_button", ButtonState::Idle);
    /// Button::new(btn_state, "Click me")
    ///     .on_click(|_| println!("Clicked!"))
    /// ```
    pub fn new(state: SharedState<ButtonState>, label: impl Into<String>) -> Self {
        let config = Arc::new(Mutex::new(ButtonConfig {
            label: Some(label.into()),
            ..Default::default()
        }));

        // Create the inner Stateful - we'll apply bg color dynamically in build()
        // Don't use on_state callback for content since config changes after construction
        let inner = Stateful::with_shared_state(state);

        Self {
            inner,
            config,
            click_handler: None,
            custom_state_callback: None,
            extra_deps: Vec::new(),
        }
    }

    /// Create a button with custom content and externally-managed state
    ///
    /// The state handle should be created via `ctx.use_state_for()` for persistence
    /// across rebuilds. The content builder receives the current button state, allowing
    /// state-dependent content rendering (e.g., different icons for pressed state).
    ///
    /// Note: When using custom content, `text_color()` has no effect.
    /// Style your content directly within the content builder.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let btn_state = ctx.use_state_for("icon_btn", ButtonState::Idle);
    /// Button::with_content(btn_state, |state| {
    ///     div().child(text("Click me").color(Color::WHITE))
    /// })
    /// .on_click(|_| println!("Clicked!"))
    /// ```
    pub fn with_content<F>(state: SharedState<ButtonState>, content_builder: F) -> Self
    where
        F: Fn(ButtonState) -> Div + Send + Sync + 'static,
    {
        let config = Arc::new(Mutex::new(ButtonConfig::default()));

        // Store content builder in config for use in build()
        // We use a custom_state_callback to hold the content builder
        let content_builder = Arc::new(content_builder);

        // Create the inner Stateful
        let inner = Stateful::with_shared_state(state);

        Self {
            inner,
            config,
            click_handler: None,
            custom_state_callback: Some(Arc::new({
                let content_builder = Arc::clone(&content_builder);
                move |state, container: &mut Div| {
                    let content = content_builder(state);
                    container.merge(div().child(content));
                }
            })),
            extra_deps: Vec::new(),
        }
    }

    // Button-specific methods
    pub fn bg_color(self, color: impl Into<Color>) -> Self {
        self.config.lock().unwrap().bg_color = color.into();
        self
    }

    pub fn hover_color(self, color: impl Into<Color>) -> Self {
        self.config.lock().unwrap().hover_color = color.into();
        self
    }

    pub fn pressed_color(self, color: impl Into<Color>) -> Self {
        self.config.lock().unwrap().pressed_color = color.into();
        self
    }

    /// Set text color for simple text buttons created with `button("Label")`
    ///
    /// Note: This has no effect on buttons created with `button_with()`.
    /// For custom content buttons, style the text directly in your content builder.
    pub fn text_color(self, color: impl Into<Color>) -> Self {
        self.config.lock().unwrap().text_color = color.into();
        self
    }

    /// Set text size for simple text buttons created with `button("Label")`
    ///
    /// Note: This has no effect on buttons created with `button_with()`.
    pub fn text_size(self, size: f32) -> Self {
        self.config.lock().unwrap().text_size = size;
        self
    }

    pub fn disabled(self, disabled: bool) -> Self {
        self.config.lock().unwrap().disabled = disabled;
        // If disabling, also update the state
        if disabled {
            self.inner.set_state(ButtonState::Disabled);
        }
        self
    }

    pub fn on_click<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        // Register click handler on the inner Stateful
        let handler = Arc::new(handler);
        let handler_clone = Arc::clone(&handler);
        self.inner = self.inner.on_click(move |ctx| handler_clone(ctx));
        self.click_handler = Some(handler);
        self
    }

    pub fn on_state<F>(mut self, callback: F) -> Self
    where
        F: Fn(ButtonState, &mut Div) + Send + Sync + 'static,
    {
        self.custom_state_callback = Some(Arc::new(callback));
        self
    }

    pub fn deps(mut self, signal_ids: &[SignalId]) -> Self {
        self.extra_deps = signal_ids.to_vec();
        self.inner = self.inner.deps(&self.extra_deps);
        self
    }

    // Forward ALL Stateful layout methods to inner

    pub fn px(mut self, v: f32) -> Self {
        self.inner = self.inner.px(v);
        self
    }

    pub fn py(mut self, v: f32) -> Self {
        self.inner = self.inner.py(v);
        self
    }

    pub fn p(mut self, v: f32) -> Self {
        self.inner = self.inner.p(v);
        self
    }

    pub fn pt(mut self, v: f32) -> Self {
        self.inner = self.inner.pt(v);
        self
    }

    pub fn pb(mut self, v: f32) -> Self {
        self.inner = self.inner.pb(v);
        self
    }

    pub fn pl(mut self, v: f32) -> Self {
        self.inner = self.inner.pl(v);
        self
    }

    pub fn pr(mut self, v: f32) -> Self {
        self.inner = self.inner.pr(v);
        self
    }

    pub fn rounded(mut self, v: f32) -> Self {
        self.inner = self.inner.rounded(v);
        self
    }

    pub fn w(mut self, v: f32) -> Self {
        self.inner = self.inner.w(v);
        self
    }

    pub fn h(mut self, v: f32) -> Self {
        self.inner = self.inner.h(v);
        self
    }

    pub fn w_full(mut self) -> Self {
        self.inner = self.inner.w_full();
        self
    }

    pub fn h_full(mut self) -> Self {
        self.inner = self.inner.h_full();
        self
    }

    pub fn w_fit(mut self) -> Self {
        self.inner = self.inner.w_fit();
        self
    }

    pub fn h_fit(mut self) -> Self {
        self.inner = self.inner.h_fit();
        self
    }

    pub fn mt(mut self, v: f32) -> Self {
        self.inner = self.inner.mt(v);
        self
    }

    pub fn mb(mut self, v: f32) -> Self {
        self.inner = self.inner.mb(v);
        self
    }

    pub fn ml(mut self, v: f32) -> Self {
        self.inner = self.inner.ml(v);
        self
    }

    pub fn mr(mut self, v: f32) -> Self {
        self.inner = self.inner.mr(v);
        self
    }

    pub fn mx(mut self, v: f32) -> Self {
        self.inner = self.inner.mx(v);
        self
    }

    pub fn my(mut self, v: f32) -> Self {
        self.inner = self.inner.my(v);
        self
    }

    pub fn m(mut self, v: f32) -> Self {
        self.inner = self.inner.m(v);
        self
    }

    pub fn gap(mut self, v: f32) -> Self {
        self.inner = self.inner.gap(v);
        self
    }

    pub fn items_center(mut self) -> Self {
        self.inner = self.inner.items_center();
        self
    }

    pub fn items_start(mut self) -> Self {
        self.inner = self.inner.items_start();
        self
    }

    pub fn items_end(mut self) -> Self {
        self.inner = self.inner.items_end();
        self
    }

    pub fn justify_center(mut self) -> Self {
        self.inner = self.inner.justify_center();
        self
    }

    pub fn justify_start(mut self) -> Self {
        self.inner = self.inner.justify_start();
        self
    }

    pub fn justify_end(mut self) -> Self {
        self.inner = self.inner.justify_end();
        self
    }

    pub fn justify_between(mut self) -> Self {
        self.inner = self.inner.justify_between();
        self
    }

    pub fn flex_row(mut self) -> Self {
        self.inner = self.inner.flex_row();
        self
    }

    pub fn flex_col(mut self) -> Self {
        self.inner = self.inner.flex_col();
        self
    }

    pub fn flex_grow(mut self) -> Self {
        self.inner = self.inner.flex_grow();
        self
    }

    pub fn flex_shrink(mut self) -> Self {
        self.inner = self.inner.flex_shrink();
        self
    }

    pub fn flex_shrink_0(mut self) -> Self {
        self.inner = self.inner.flex_shrink_0();
        self
    }

    pub fn shadow_sm(mut self) -> Self {
        self.inner = self.inner.shadow_sm();
        self
    }

    pub fn shadow_md(mut self) -> Self {
        self.inner = self.inner.shadow_md();
        self
    }

    pub fn shadow_lg(mut self) -> Self {
        self.inner = self.inner.shadow_lg();
        self
    }

    pub fn shadow_xl(mut self) -> Self {
        self.inner = self.inner.shadow_xl();
        self
    }

    pub fn opacity(mut self, v: f32) -> Self {
        self.inner = self.inner.opacity(v);
        self
    }
}

/// Create a button with a text label and context-managed state
///
/// The state handle should be created via `ctx.use_state_for()` for persistence
/// across rebuilds. This is the most common button constructor. For buttons with
/// custom content (icons, multiple elements, etc.), use `button_with()`.
///
/// # Example
/// ```ignore
/// let btn_state = ctx.use_state_for("save_btn", ButtonState::Idle);
/// button(btn_state, "Save")
///     .on_click(|_| save_data())
///     .bg_color(Color::GREEN)
/// ```
pub fn button(state: SharedState<ButtonState>, label: impl Into<String>) -> Button {
    Button::new(state, label)
        .px(12.0)
        .py(6.0)
        .rounded(8.0)
        .items_center()
        .justify_center()
}

/// Create a button with custom content and context-managed state
///
/// The state handle should be created via `ctx.use_state_for()` for persistence
/// across rebuilds. The content builder receives the current button state, allowing
/// state-dependent content (e.g., different icons for pressed state).
///
/// # Example
/// ```ignore
/// // Icon button
/// let trash_btn = ctx.use_state_for("trash_btn", ButtonState::Idle);
/// button_with(trash_btn, |_state| {
///     div().child(svg_icon("trash"))
/// })
/// .on_click(|_| delete_item())
///
/// // Button with icon and text
/// let save_btn = ctx.use_state_for("save_btn", ButtonState::Idle);
/// button_with(save_btn, |_state| {
///     div().flex_row().gap(8.0)
///         .child(svg_icon("save"))
///         .child(text("Save").color(Color::WHITE))
/// })
/// .on_click(|_| save())
///
/// // State-aware button (e.g., loading spinner when pressed)
/// let submit_btn = ctx.use_state_for("submit_btn", ButtonState::Idle);
/// button_with(submit_btn, |state| {
///     if matches!(state, ButtonState::Pressed) {
///         div().child(spinner())
///     } else {
///         div().child(text("Submit").color(Color::WHITE))
///     }
/// })
/// ```
pub fn button_with<F>(state: SharedState<ButtonState>, content_builder: F) -> Button
where
    F: Fn(ButtonState) -> Div + Send + Sync + 'static,
{
    Button::with_content(state, content_builder)
        .px(12.0)
        .py(6.0)
        .rounded(8.0)
        .items_center()
        .justify_center()
}


impl ElementBuilder for Button {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        tracing::info!("Button::build called");
        // Capture current config values for the on_state callback
        let config_for_state = Arc::clone(&self.config);
        let custom_callback = self.custom_state_callback.clone();

        // Ensure state transition handlers (hover, press) are registered
        // This is needed because Button bypasses Stateful::on_state() and sets
        // the callback directly, so register_state_handlers() is never called.
        self.inner.ensure_state_handlers_registered();

        // Register on_state callback with current config
        // This will be applied by Stateful::build() when it sees needs_visual_update
        {
            let shared_state = self.inner.shared_state();
            let mut shared = shared_state.lock().unwrap();
            shared.state_callback = Some(Arc::new(move |state: &ButtonState, container: &mut Div| {
                tracing::info!("Button on_state callback fired, state={:?}", state);
                let cfg = config_for_state.lock().unwrap();
                let bg = match state {
                    ButtonState::Idle => cfg.bg_color,
                    ButtonState::Hovered => cfg.hover_color,
                    ButtonState::Pressed => cfg.pressed_color,
                    ButtonState::Disabled => cfg.disabled_color,
                };

                // Apply background color and content
                let mut update = div().bg(bg);

                // Add content based on whether we have custom content or label
                if let Some(ref callback) = custom_callback {
                    callback(*state, &mut update);
                } else if let Some(ref label) = cfg.label {
                    tracing::info!("Button adding label child: {}", label);
                    update = update.child(text(label).size(cfg.text_size).color(cfg.text_color));
                }

                let update_children = update.children.len();
                tracing::info!("Button update div has {} children before merge", update_children);
                drop(cfg);
                container.merge(update);
                let container_children = container.children.len();
                tracing::info!("Button container has {} children after merge", container_children);
            }));
            shared.base_render_props = Some(self.inner.inner_render_props());
            shared.needs_visual_update = true;
        }

        // Build the inner Stateful - it will apply the callback we just set
        self.inner.build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        // Delegate to the inner Stateful which has the cached children
        self.inner.children_builders()
    }

    fn element_type_id(&self) -> crate::div::ElementTypeId {
        crate::div::ElementTypeId::Div
    }

    fn event_handlers(&self) -> Option<&crate::event_handler::EventHandlers> {
        // Delegate to the inner Stateful which has the cached event handlers
        self.inner.event_handlers()
    }
}
