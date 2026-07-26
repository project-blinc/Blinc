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
//! let btn_state = ctx.use_fsm(ButtonState::Idle);
//! button(btn_state, "Click me")
//!     .on_click(|_| println!("Clicked!"))
//!     .bg_color(Color::RED)
//!     .hover_color(Color::GREEN)
//!
//! // Button with custom child content
//! let save_btn_state = ctx.use_fsm(ButtonState::Idle);
//! button_with(save_btn_state, |_state| {
//!     div().flex_row().gap(8.0)
//!         .child(svg_icon("save"))
//!         .child(text("Save"))
//! })
//! .on_click(|_| save())
//! ```

use std::sync::{Arc, Mutex};

use blinc_core::Color;
use blinc_core::reactive::SignalId;
use blinc_theme::{ColorToken, ThemeState};

use crate::css_parser::{ElementState, active_stylesheet};
use crate::div::{Div, ElementBuilder, div};
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
    /// CSS class names for stylesheet matching
    pub css_classes: Vec<std::sync::Arc<str>>,
}

impl Default for ButtonConfig {
    fn default() -> Self {
        let theme = ThemeState::get();
        Self {
            label: None,
            text_color: theme.color(ColorToken::TextInverse),
            text_size: 16.0,
            bg_color: theme.color(ColorToken::Primary),
            hover_color: theme.color(ColorToken::PrimaryHover),
            pressed_color: theme.color(ColorToken::PrimaryActive),
            disabled_color: theme.color(ColorToken::InputBgDisabled),
            disabled: false,
            css_classes: Vec::new(),
        }
    }
}

type ClickHandler = Arc<dyn Fn(&crate::event_handler::EventContext) + Send + Sync>;
type StateCallback = Arc<dyn Fn(ButtonState, &mut Div) + Send + Sync>;

/// Button widget - wraps `Stateful<ButtonState>`
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
    /// The state handle should be created via `ctx.use_fsm()` for persistence
    /// across rebuilds. The label text color can be customized with `.text_color()`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let btn_state = ctx.use_fsm(ButtonState::Idle);
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
    /// The state handle should be created via `ctx.use_fsm()` for persistence
    /// across rebuilds. The content builder receives the current button state, allowing
    /// state-dependent content rendering (e.g., different icons for pressed state).
    ///
    /// Note: When using custom content, `text_color()` has no effect.
    /// Style your content directly within the content builder.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let btn_state = ctx.use_fsm(ButtonState::Idle);
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

    /// Access the shared config (allows external content builders to read CSS-resolved values)
    pub fn config_arc(&self) -> Arc<Mutex<ButtonConfig>> {
        Arc::clone(&self.config)
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

    pub fn border(mut self, width: f32, color: Color) -> Self {
        self.inner = self.inner.border(width, color);
        self
    }

    pub fn border_color(mut self, color: Color) -> Self {
        self.inner = self.inner.border_color(color);
        self
    }

    pub fn border_width(mut self, width: f32) -> Self {
        self.inner = self.inner.border_width(width);
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

    /// Set the element ID for CSS selector targeting
    pub fn id(mut self, id: &str) -> Self {
        self.inner = self.inner.id(id);
        self
    }

    /// Add a CSS class for selector matching
    pub fn class(mut self, class: &str) -> Self {
        self.config
            .lock()
            .unwrap()
            .css_classes
            .push(blinc_core::intern::intern(class));
        self.inner = self.inner.class(class);
        self
    }
}

/// Create a button with a text label and context-managed state
///
/// The state handle should be created via `ctx.use_fsm()` for persistence
/// across rebuilds. This is the most common button constructor. For buttons with
/// custom content (icons, multiple elements, etc.), use `button_with()`.
///
/// # Example
/// ```ignore
/// let btn_state = ctx.use_fsm(ButtonState::Idle);
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
/// The state handle should be created via `ctx.use_fsm()` for persistence
/// across rebuilds. The content builder receives the current button state, allowing
/// state-dependent content (e.g., different icons for pressed state).
///
/// # Example
/// ```ignore
/// // Icon button
/// let trash_btn = ctx.use_fsm(ButtonState::Idle);
/// button_with(trash_btn, |_state| {
///     div().child(svg_icon("trash"))
/// })
/// .on_click(|_| delete_item())
///
/// // Button with icon and text
/// let save_btn = ctx.use_fsm(ButtonState::Idle);
/// button_with(save_btn, |_state| {
///     div().flex_row().gap(8.0)
///         .child(svg_icon("save"))
///         .child(text("Save").color(Color::WHITE))
/// })
/// .on_click(|_| save())
///
/// // State-aware button (e.g., loading spinner when pressed)
/// let submit_btn = ctx.use_fsm(ButtonState::Idle);
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

impl Button {
    /// Register the state callback on the inner Stateful's shared state.
    ///
    /// Called from both `build()` and `render_props()` so that when a fresh
    /// `Button` is constructed mid-rebuild (e.g. cn::button created inside
    /// an outer Stateful's `on_state` callback, then visited only via
    /// `render_props()` on the visual-rebuild path — which never calls
    /// `build()`), the new Button's inner Div still gets populated.
    ///
    /// Pre-fix: registration lived only in `build()`. On the visual-rebuild
    /// path the new `cn::button` builder's `render_props()` → Stateful's
    /// `render_props()` → `ensure_callback_invoked()` would short-circuit
    /// because either (a) the shared `state_callback` was None on a brand
    /// new shared-state slot, or (b) `needs_visual_update` was false (the
    /// previous callback run had reset it). Either way the new Stateful's
    /// fresh inner Div stayed empty — no bg, no label — and the visual
    /// rebuild then copied that empty render_props onto the existing
    /// layout node, washing out the trigger button to page-bg white.
    ///
    /// Idempotent: re-registering the callback overwrites the previous
    /// Arc and re-bumps `needs_visual_update`; `ensure_callback_invoked`
    /// runs the callback exactly once after a flip to true and then
    /// resets the flag.
    fn register_state_callback(&self) {
        let config_for_state = Arc::clone(&self.config);
        let custom_callback = self.custom_state_callback.clone();
        let css_element_id = self.inner.element_id().map(|s| s.to_string());

        self.inner.ensure_state_handlers_registered();

        let shared_state = self.inner.shared_state();
        let mut shared = shared_state.lock().unwrap();
        shared.state_callback = Some(Arc::new(move |state: &ButtonState, container: &mut Div| {
            tracing::debug!("Button on_state callback fired, state={:?}", state);
            let mut cfg = config_for_state.lock().unwrap();

            apply_css_overrides_button(&mut cfg, css_element_id.as_deref(), state, container);

            let bg = match state {
                ButtonState::Idle => cfg.bg_color,
                ButtonState::Hovered => cfg.hover_color,
                ButtonState::Pressed => cfg.pressed_color,
                ButtonState::Disabled => cfg.disabled_color,
            };

            let mut update = div().bg(bg);

            if let Some(ref callback) = custom_callback {
                drop(cfg);
                callback(*state, &mut update);
            } else if let Some(ref label) = cfg.label {
                update = update.child(text(label).size(cfg.text_size).color(cfg.text_color));
                drop(cfg);
            } else {
                drop(cfg);
            }

            container.merge(update);
        }));
        shared.base_render_props = Some(self.inner.inner_render_props());
        shared.base_style = self.inner.inner_layout_style();
        shared.needs_visual_update = true;
    }
}

impl ElementBuilder for Button {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        tracing::debug!("Button::build called");
        self.register_state_callback();
        // Build the inner Stateful — it will apply the callback we just set
        self.inner.build(tree)
    }

    fn render_props(&self) -> RenderProps {
        // Mirror the setup done in `build()` so that visual-rebuild paths
        // (which call `render_props()` but never `build()` on the new
        // builder instance) still get a fully-populated inner Div. See
        // `register_state_callback` for the full rationale — symptom was
        // cn::dropdown_menu_custom / cn::popover triggers losing their
        // background colour on hover because the outer wrapper Stateful's
        // visual rebuild snapshotted an unset cn::button into the layout
        // node.
        self.register_state_callback();
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        // Same mirror as `render_props`: the state callback lives on this
        // Button until `register_state_callback` installs it on the inner
        // Stateful, so without this the Stateful has no callback yet and
        // reports no children.
        //
        // A parent's refresh hashes its rebuilt content through
        // `children_builders`. Seeing a childless button meant the hash
        // did not change when only the label did, so the refresh took the
        // visual-only path and never re-measured -- a
        // `dropdown_menu_custom` trigger kept its previous width when its
        // label changed.
        self.register_state_callback();
        self.inner.children_builders()
    }

    fn element_type_id(&self) -> crate::div::ElementTypeId {
        crate::div::ElementTypeId::Div
    }

    fn semantic_type_name(&self) -> Option<&'static str> {
        Some("button")
    }

    fn element_id(&self) -> Option<&str> {
        self.inner.element_id()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.inner.element_classes()
    }

    fn event_handlers(&self) -> Option<&crate::event_handler::EventHandlers> {
        // Delegate to the inner Stateful which has the cached event handlers
        self.inner.event_handlers()
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.inner.layout_style()
    }
}

/// Apply CSS overrides from active stylesheet to button config
fn apply_css_overrides_button(
    cfg: &mut ButtonConfig,
    css_element_id: Option<&str>,
    state: &ButtonState,
    container: &mut Div,
) {
    let stylesheet = match active_stylesheet() {
        Some(s) => s,
        None => return,
    };

    // Helper to apply a single ElementStyle to the button config + container
    let apply = |cfg: &mut ButtonConfig,
                 container: &mut Div,
                 style: &crate::element_style::ElementStyle,
                 is_state_specific: bool| {
        if let Some(blinc_core::Brush::Solid(c)) = style.background.as_ref() {
            if is_state_specific {
                match state {
                    ButtonState::Idle => cfg.bg_color = *c,
                    ButtonState::Hovered => cfg.hover_color = *c,
                    ButtonState::Pressed => cfg.pressed_color = *c,
                    ButtonState::Disabled => cfg.disabled_color = *c,
                }
            } else {
                cfg.bg_color = *c;
            }
        }
        if let Some(c) = style.text_color {
            cfg.text_color = c;
        }
        if let Some(fs) = style.font_size {
            cfg.text_size = fs;
        }
        // Corner radius — NOT applied here. Border-radius is handled by the
        // renderer's apply_stylesheet_base_styles(), which correctly evaluates
        // hierarchical selectors (e.g. `.sidebar .cn-button--secondary`).
        // Applying it here from simple class styles would overwrite higher-specificity
        // hierarchical CSS on every state change.
        //
        // Border
        if let Some(bw) = style.border_width {
            if let Some(bc) = style.border_color {
                container.set_border(bw, bc);
            }
        } else if let Some(bc) = style.border_color {
            // border-color only (keep existing width)
            container.border_color = Some(bc);
        }
    };

    let element_state = match state {
        ButtonState::Hovered => Some(ElementState::Hover),
        ButtonState::Pressed => Some(ElementState::Active),
        ButtonState::Disabled => Some(ElementState::Disabled),
        ButtonState::Idle => None,
    };

    // 1. Resolve by class (lowest priority)
    let classes = cfg.css_classes.clone();
    for class in &classes {
        if let Some(base) = stylesheet.get_class(class) {
            apply(cfg, container, base, false);
        }
        if let Some(s) = element_state {
            if let Some(state_style) = stylesheet.get_class_with_state(class, s) {
                apply(cfg, container, state_style, true);
            }
        }
    }

    // 2. Resolve by element ID (higher priority)
    if let Some(id) = css_element_id {
        if let Some(base) = stylesheet.get(id) {
            apply(cfg, container, base, false);
        }
        if let Some(s) = element_state {
            if let Some(state_style) = stylesheet.get_with_state(id, s) {
                apply(cfg, container, state_style, true);
            }
        }
    }
}
