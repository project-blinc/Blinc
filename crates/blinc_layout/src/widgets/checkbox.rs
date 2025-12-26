//! Ready-to-use Checkbox widget
//!
//! A checkbox with built-in toggle, hover states, and animations.
//! Works directly in the fluent API without needing `.build()`.
//! Inherits ALL Div methods for full layout control via Deref.
//!
//! # Example
//!
//! ```ignore
//! div().child(
//!     checkbox(&state)
//!         .label("Remember me")
//!         .on_change(|checked| println!("Checked: {}", checked))
//!         .rounded(8.0)
//!         .shadow_sm()
//! )
//! ```

use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex};

use blinc_core::Color;

use crate::div::{div, Div, ElementBuilder};
use crate::element::RenderProps;
use crate::text::text;
use crate::tree::{LayoutNodeId, LayoutTree};

/// Checkbox state
#[derive(Debug, Clone, Default)]
pub struct CheckboxState {
    /// Whether checked
    pub checked: bool,
    /// Whether hovered
    pub hovered: bool,
    /// Whether disabled
    pub disabled: bool,
}

/// Shared checkbox state handle
pub type SharedCheckboxState = Arc<Mutex<CheckboxState>>;

/// Create a shared checkbox state
pub fn checkbox_state(checked: bool) -> SharedCheckboxState {
    Arc::new(Mutex::new(CheckboxState {
        checked,
        hovered: false,
        disabled: false,
    }))
}

/// Checkbox configuration
#[derive(Clone)]
pub struct CheckboxConfig {
    /// Label text (optional)
    pub label: Option<String>,
    /// Box size
    pub size: f32,
    /// Gap between box and label
    pub gap: f32,
    /// Unchecked background color
    pub unchecked_bg: Color,
    /// Checked background color
    pub checked_bg: Color,
    /// Hover tint
    pub hover_tint: f32,
    /// Check mark color
    pub check_color: Color,
    /// Label color
    pub label_color: Color,
    /// Label font size
    pub label_font_size: f32,
    /// Corner radius
    pub corner_radius: f32,
    /// Border color
    pub border_color: Color,
    /// Border width
    pub border_width: f32,
    /// Disabled opacity
    pub disabled_opacity: f32,
}

impl Default for CheckboxConfig {
    fn default() -> Self {
        Self {
            label: None,
            size: 20.0,
            gap: 8.0,
            unchecked_bg: Color::rgba(0.15, 0.15, 0.2, 1.0),
            checked_bg: Color::rgba(0.2, 0.5, 0.9, 1.0),
            hover_tint: 0.1,
            check_color: Color::WHITE,
            label_color: Color::rgba(0.9, 0.9, 0.9, 1.0),
            label_font_size: 14.0,
            corner_radius: 4.0,
            border_color: Color::rgba(0.3, 0.3, 0.35, 1.0),
            border_width: 1.0,
            disabled_opacity: 0.5,
        }
    }
}

/// Helper to lighten a color
fn lighten(color: Color, amount: f32) -> Color {
    Color::rgba(
        (color.r + amount).min(1.0),
        (color.g + amount).min(1.0),
        (color.b + amount).min(1.0),
        color.a,
    )
}

/// Ready-to-use checkbox widget
///
/// Inherits all Div methods via Deref, so you have full layout control.
///
/// Usage: `checkbox(&state).label("Remember me").on_change(|checked| ...)`
pub struct Checkbox {
    /// Inner div - ALL Div methods are available via Deref
    inner: Div,
    /// Checkbox state
    state: SharedCheckboxState,
    /// Checkbox configuration
    config: CheckboxConfig,
    /// Change handler
    on_change: Option<Arc<dyn Fn(bool) + Send + Sync>>,
}

// Deref to Div gives Checkbox ALL Div methods for reading
impl Deref for Checkbox {
    type Target = Div;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for Checkbox {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl Checkbox {
    /// Create a new checkbox
    pub fn new(state: &SharedCheckboxState) -> Self {
        let config = CheckboxConfig::default();

        // Build initial visual
        let inner = Self::build_visual(&config, state);

        Self {
            inner,
            state: Arc::clone(state),
            config,
            on_change: None,
        }
    }

    /// Create with label
    pub fn with_label(state: &SharedCheckboxState, label: impl Into<String>) -> Self {
        let config = CheckboxConfig {
            label: Some(label.into()),
            ..Default::default()
        };

        let inner = Self::build_visual(&config, state);

        Self {
            inner,
            state: Arc::clone(state),
            config,
            on_change: None,
        }
    }

    /// Build the visual representation from config and state
    fn build_visual(config: &CheckboxConfig, state: &SharedCheckboxState) -> Div {
        let state_guard = state.lock().unwrap();
        let is_checked = state_guard.checked;
        let is_hovered = state_guard.hovered;
        let is_disabled = state_guard.disabled;
        drop(state_guard);

        // Calculate background color
        let bg = if is_checked {
            if is_hovered && !is_disabled {
                lighten(config.checked_bg, config.hover_tint)
            } else {
                config.checked_bg
            }
        } else if is_hovered && !is_disabled {
            lighten(config.unchecked_bg, config.hover_tint)
        } else {
            config.unchecked_bg
        };

        // Build the checkbox box
        let mut checkbox_box = div()
            .w(config.size)
            .h(config.size)
            .bg(bg)
            .rounded(config.corner_radius)
            .items_center()
            .justify_center();

        // Add checkmark if checked
        if is_checked {
            checkbox_box =
                checkbox_box.child(text("✓").size(config.size * 0.7).color(config.check_color));
        }

        // Apply disabled opacity
        if is_disabled {
            checkbox_box = checkbox_box.opacity(config.disabled_opacity);
        }

        // Build final element
        let mut container = div()
            .flex_row()
            .gap(config.gap)
            .items_center()
            .child(checkbox_box);

        // Add label if present
        if let Some(ref label) = config.label {
            let label_color = if is_disabled {
                Color::rgba(
                    config.label_color.r * config.disabled_opacity,
                    config.label_color.g * config.disabled_opacity,
                    config.label_color.b * config.disabled_opacity,
                    config.label_color.a * config.disabled_opacity,
                )
            } else {
                config.label_color
            };

            container =
                container.child(text(label).size(config.label_font_size).color(label_color));
        }

        container
    }

    /// Rebuild visual after config change
    fn rebuild_visual(&mut self) {
        self.inner = Self::build_visual(&self.config, &self.state);
    }

    /// Set label
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.config.label = Some(label.into());
        self.rebuild_visual();
        self
    }

    /// Set checkbox box size
    pub fn checkbox_size(mut self, size: f32) -> Self {
        self.config.size = size;
        self.rebuild_visual();
        self
    }

    /// Set checked background color
    pub fn checked_bg(mut self, color: impl Into<Color>) -> Self {
        self.config.checked_bg = color.into();
        self.rebuild_visual();
        self
    }

    /// Set unchecked background color
    pub fn unchecked_bg(mut self, color: impl Into<Color>) -> Self {
        self.config.unchecked_bg = color.into();
        self.rebuild_visual();
        self
    }

    /// Set check mark color
    pub fn check_color(mut self, color: impl Into<Color>) -> Self {
        self.config.check_color = color.into();
        self.rebuild_visual();
        self
    }

    /// Set label color
    pub fn label_color(mut self, color: impl Into<Color>) -> Self {
        self.config.label_color = color.into();
        self.rebuild_visual();
        self
    }

    /// Set label font size
    pub fn label_font_size(mut self, size: f32) -> Self {
        self.config.label_font_size = size;
        self.rebuild_visual();
        self
    }

    /// Set disabled state
    pub fn disabled(mut self, disabled: bool) -> Self {
        if let Ok(mut s) = self.state.lock() {
            s.disabled = disabled;
        }
        self.rebuild_visual();
        self
    }

    /// Set change handler
    pub fn on_change<F>(mut self, handler: F) -> Self
    where
        F: Fn(bool) + Send + Sync + 'static,
    {
        self.on_change = Some(Arc::new(handler));
        self
    }

    // =========================================================================
    // Builder methods that return Self (shadow Div methods for fluent API)
    // =========================================================================

    pub fn w(mut self, px: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).w(px);
        self
    }

    pub fn h(mut self, px: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).h(px);
        self
    }

    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).size(w, h);
        self
    }

    pub fn square(mut self, size: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).square(size);
        self
    }

    pub fn w_full(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).w_full();
        self
    }

    pub fn h_full(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).h_full();
        self
    }

    pub fn w_fit(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).w_fit();
        self
    }

    pub fn h_fit(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).h_fit();
        self
    }

    pub fn p(mut self, px: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).p(px);
        self
    }

    pub fn px(mut self, px: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).px(px);
        self
    }

    pub fn py(mut self, px: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).py(px);
        self
    }

    pub fn m(mut self, px: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).m(px);
        self
    }

    pub fn mx(mut self, px: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).mx(px);
        self
    }

    pub fn my(mut self, px: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).my(px);
        self
    }

    pub fn gap(mut self, px: f32) -> Self {
        self.config.gap = px;
        self.inner = std::mem::take(&mut self.inner).gap(px);
        self
    }

    pub fn flex_row(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).flex_row();
        self
    }

    pub fn flex_col(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).flex_col();
        self
    }

    pub fn flex_grow(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).flex_grow();
        self
    }

    pub fn items_center(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).items_center();
        self
    }

    pub fn items_start(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).items_start();
        self
    }

    pub fn items_end(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).items_end();
        self
    }

    pub fn justify_center(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).justify_center();
        self
    }

    pub fn justify_start(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).justify_start();
        self
    }

    pub fn justify_end(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).justify_end();
        self
    }

    pub fn justify_between(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).justify_between();
        self
    }

    pub fn bg(mut self, color: impl Into<blinc_core::Brush>) -> Self {
        self.inner = std::mem::take(&mut self.inner).background(color);
        self
    }

    pub fn rounded(mut self, radius: f32) -> Self {
        self.config.corner_radius = radius;
        self.inner = std::mem::take(&mut self.inner).rounded(radius);
        self
    }

    pub fn shadow(mut self, shadow: blinc_core::Shadow) -> Self {
        self.inner = std::mem::take(&mut self.inner).shadow(shadow);
        self
    }

    pub fn shadow_sm(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).shadow_sm();
        self
    }

    pub fn shadow_md(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).shadow_md();
        self
    }

    pub fn shadow_lg(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).shadow_lg();
        self
    }

    pub fn transform(mut self, transform: blinc_core::Transform) -> Self {
        self.inner = std::mem::take(&mut self.inner).transform(transform);
        self
    }

    pub fn opacity(mut self, opacity: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).opacity(opacity);
        self
    }

    pub fn overflow_clip(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).overflow_clip();
        self
    }

    pub fn overflow_visible(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).overflow_visible();
        self
    }

    pub fn child(mut self, child: impl ElementBuilder + 'static) -> Self {
        self.inner = std::mem::take(&mut self.inner).child(child);
        self
    }

    pub fn children<I>(mut self, children: I) -> Self
    where
        I: IntoIterator,
        I::Item: ElementBuilder + 'static,
    {
        self.inner = std::mem::take(&mut self.inner).children(children);
        self
    }

    // Event handlers
    pub fn on_click<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::mem::take(&mut self.inner).on_click(handler);
        self
    }

    pub fn on_hover_enter<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::mem::take(&mut self.inner).on_hover_enter(handler);
        self
    }

    pub fn on_hover_leave<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::mem::take(&mut self.inner).on_hover_leave(handler);
        self
    }

    pub fn on_mouse_down<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::mem::take(&mut self.inner).on_mouse_down(handler);
        self
    }

    pub fn on_mouse_up<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::mem::take(&mut self.inner).on_mouse_up(handler);
        self
    }

    pub fn on_focus<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::mem::take(&mut self.inner).on_focus(handler);
        self
    }

    pub fn on_blur<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::mem::take(&mut self.inner).on_blur(handler);
        self
    }

    pub fn on_key_down<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::mem::take(&mut self.inner).on_key_down(handler);
        self
    }

    pub fn on_key_up<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::mem::take(&mut self.inner).on_key_up(handler);
        self
    }

    pub fn on_scroll<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::mem::take(&mut self.inner).on_scroll(handler);
        self
    }
}

/// Create a checkbox with shared state
///
/// The checkbox inherits ALL Div methods, so you have full layout control.
///
/// # Example
///
/// ```ignore
/// let state = checkbox_state(false);
/// checkbox(&state)
///     .label("Remember me")
///     .on_change(|checked| println!("Checked: {}", checked))
///     .rounded(8.0)
///     .shadow_sm()
/// ```
pub fn checkbox(state: &SharedCheckboxState) -> Checkbox {
    Checkbox::new(state)
}

/// Create a checkbox with label and state
pub fn checkbox_labeled(state: &SharedCheckboxState, label: impl Into<String>) -> Checkbox {
    Checkbox::with_label(state, label)
}

impl ElementBuilder for Checkbox {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        // Rebuild visual with current state before building
        let is_disabled = self.state.lock().map(|s| s.disabled).unwrap_or(false);

        let mut visual = Self::build_visual(&self.config, &self.state);

        // Add click handler if not disabled
        if !is_disabled {
            let state_clone = Arc::clone(&self.state);
            let on_change = self.on_change.clone();

            visual = visual.on_click(move |_| {
                if let Ok(mut s) = state_clone.lock() {
                    s.checked = !s.checked;
                    let checked = s.checked;
                    drop(s);

                    if let Some(ref handler) = on_change {
                        handler(checked);
                    }
                }
            });
        }

        visual.build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.inner.children_builders()
    }

    fn element_type_id(&self) -> crate::div::ElementTypeId {
        crate::div::ElementTypeId::Div
    }

    fn event_handlers(&self) -> Option<&crate::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(&self.inner)
    }
}
