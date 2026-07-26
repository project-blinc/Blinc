//! Button component with shadcn-style variants
//!
//! A themed button component using CSS `:hover`/`:active` for visual feedback.
//! All styling is CSS-driven via `.cn-button` classes, making it fully overridable.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! // Primary button (default)
//! cn::button("Click me")
//!
//! // Destructive button
//! cn::button("Delete")
//!     .variant(ButtonVariant::Destructive)
//!
//! // Outline button with custom size
//! cn::button("Cancel")
//!     .variant(ButtonVariant::Outline)
//!     .size(ButtonSize::Small)
//!
//! // Button with click handler
//! cn::button("Submit")
//!     .on_click(|_| println!("Submitted!"))
//! ```

use crate::reactive_props::{clone_reactive, current, dep_signal};
use blinc_core::Color;
use blinc_layout::InstanceKey;
use blinc_layout::binding::{IntoReactive, Reactive};
use blinc_layout::div::ElementBuilder;
use blinc_layout::prelude::*;
use blinc_layout::stateful::{ButtonState, SharedState, use_fsm_keyed};
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_layout::widgets::button as layout_button;
use blinc_theme::{ColorToken, ThemeState};
use std::sync::Arc;

/// Button visual variants (like shadcn)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonVariant {
    /// Primary action button - filled with primary color
    #[default]
    Primary,
    /// Secondary action - muted background
    Secondary,
    /// Destructive action - red/danger styling
    Destructive,
    /// Outline button - border only, transparent background
    Outline,
    /// Ghost button - no background, minimal styling
    Ghost,
    /// Link button - appears as a link, no button styling
    Link,
}

impl ButtonVariant {
    /// Get the CSS class suffix for this variant
    fn css_class(&self) -> &'static str {
        match self {
            ButtonVariant::Primary => "cn-button--primary",
            ButtonVariant::Secondary => "cn-button--secondary",
            ButtonVariant::Destructive => "cn-button--destructive",
            ButtonVariant::Outline => "cn-button--outline",
            ButtonVariant::Ghost => "cn-button--ghost",
            ButtonVariant::Link => "cn-button--link",
        }
    }

    /// Get the background color for this variant and state.
    ///
    /// Used by components that still use `Stateful<ButtonState>` (dropdown menu, select).
    pub(crate) fn background(&self, theme: &ThemeState, state: ButtonState) -> Color {
        match (self, state) {
            // Disabled keeps full bg alpha — the `.opacity(0.5)` applied
            // at the button level (see `apply_css_overrides_button` callers)
            // already dims the whole element (bg + text + border). Stacking
            // a second 0.5 alpha on the bg made it 0.25 effective vs the
            // parent surface and the button visually disappeared in light mode.
            (_, ButtonState::Disabled) => self.base_background(theme),
            (ButtonVariant::Primary, ButtonState::Pressed) => {
                theme.color(ColorToken::PrimaryActive)
            }
            (ButtonVariant::Secondary, ButtonState::Pressed) => {
                theme.color(ColorToken::SecondaryActive)
            }
            (ButtonVariant::Destructive, ButtonState::Pressed) => {
                darken(theme.color(ColorToken::Error), 0.15)
            }
            (ButtonVariant::Outline | ButtonVariant::Ghost, ButtonState::Pressed) => {
                theme.color(ColorToken::TextPrimary).with_alpha(0.1)
            }
            (ButtonVariant::Link, ButtonState::Pressed) => Color::TRANSPARENT,
            (ButtonVariant::Primary, ButtonState::Hovered) => theme.color(ColorToken::PrimaryHover),
            (ButtonVariant::Secondary, ButtonState::Hovered) => {
                theme.color(ColorToken::SecondaryHover)
            }
            (ButtonVariant::Destructive, ButtonState::Hovered) => {
                darken(theme.color(ColorToken::Error), 0.1)
            }
            (ButtonVariant::Outline | ButtonVariant::Ghost, ButtonState::Hovered) => {
                theme.color(ColorToken::TextPrimary).with_alpha(0.05)
            }
            (ButtonVariant::Link, ButtonState::Hovered) => Color::TRANSPARENT,
            _ => self.base_background(theme),
        }
    }

    fn base_background(&self, theme: &ThemeState) -> Color {
        match self {
            ButtonVariant::Primary => theme.color(ColorToken::Primary),
            ButtonVariant::Secondary => theme.color(ColorToken::Secondary),
            ButtonVariant::Destructive => theme.color(ColorToken::Error),
            ButtonVariant::Outline | ButtonVariant::Ghost | ButtonVariant::Link => {
                Color::TRANSPARENT
            }
        }
    }

    /// Get the foreground (text) color for this variant
    pub(crate) fn foreground(&self, theme: &ThemeState) -> Color {
        match self {
            // Filled tonal variants — Secondary's bg is dark slate in light
            // mode / light gray in dark mode, so the inverse text token
            // (white / near-black) is the correct contrast partner.
            ButtonVariant::Primary | ButtonVariant::Destructive | ButtonVariant::Secondary => {
                theme.color(ColorToken::TextInverse)
            }
            ButtonVariant::Outline | ButtonVariant::Ghost => theme.color(ColorToken::TextPrimary),
            ButtonVariant::Link => theme.color(ColorToken::Primary),
        }
    }

    /// Get the border color for this variant (if any)
    pub(crate) fn border(&self, theme: &ThemeState) -> Option<Color> {
        match self {
            ButtonVariant::Outline => Some(theme.color(ColorToken::Border)),
            _ => None,
        }
    }
}

/// Helper to darken a color
fn darken(color: Color, amount: f32) -> Color {
    Color::rgba(
        (color.r * (1.0 - amount)).max(0.0),
        (color.g * (1.0 - amount)).max(0.0),
        (color.b * (1.0 - amount)).max(0.0),
        color.a,
    )
}

/// Button size variants
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum ButtonSize {
    /// Small button
    Small,
    /// Default size
    #[default]
    Medium,
    /// Large button
    Large,
    /// Icon-only button (square)
    Icon,
    /// Explicit dimensions in logical pixels — `(width, height)`.
    ///
    /// Use when the standard `Small` / `Medium` / `Large` ladder
    /// doesn't fit (a wide auth-flow CTA, a tight inline action, a
    /// settings row aligned to a specific column width). Padding /
    /// font-size / corner radius fall back to the `Medium` defaults
    /// — drop a per-instance `.id(...)` + CSS rule if you need to
    /// tweak those independently of the dimensions.
    Custom(f32, f32),
}

impl ButtonSize {
    /// Get the CSS class suffix for this size
    fn css_class(&self) -> &'static str {
        match self {
            ButtonSize::Small => "cn-button--sm",
            ButtonSize::Medium => "cn-button--md",
            ButtonSize::Large => "cn-button--lg",
            ButtonSize::Icon => "cn-button--icon",
            // `Custom` opts out of size-specific cascade rules — the
            // explicit dimensions are the source of truth. The
            // generic `.cn-button` class still applies for variant
            // colours / hover / etc.
            ButtonSize::Custom(_, _) => "cn-button--custom",
        }
    }

    /// Explicit width in logical pixels when the variant pins one
    /// (only `Custom`). Other variants are content-driven via
    /// `w_fit()` and return `None`.
    fn width(&self) -> Option<f32> {
        match self {
            ButtonSize::Custom(w, _) => Some(*w),
            _ => None,
        }
    }

    /// Get height
    fn height(&self) -> f32 {
        match self {
            ButtonSize::Small => 32.0,
            ButtonSize::Medium => 40.0,
            ButtonSize::Large => 44.0,
            ButtonSize::Icon => 40.0,
            ButtonSize::Custom(_, h) => *h,
        }
    }

    /// Get horizontal padding (raw pixels)
    fn padding_x(&self) -> f32 {
        match self {
            ButtonSize::Small => 12.0,
            ButtonSize::Medium => 16.0,
            ButtonSize::Large => 24.0,
            ButtonSize::Icon => 8.0,
            // Custom: fall back to Medium so a content-with-label
            // button has sensible breathing room; users can override
            // via `.cn-button--custom` CSS.
            ButtonSize::Custom(_, _) => 16.0,
        }
    }

    /// Get vertical padding (raw pixels)
    fn padding_y(&self) -> f32 {
        match self {
            ButtonSize::Small => 4.0,
            ButtonSize::Medium => 8.0,
            ButtonSize::Large => 12.0,
            ButtonSize::Icon => 8.0,
            ButtonSize::Custom(_, _) => 8.0,
        }
    }

    /// Get font size for text/icon sizing
    fn font_size(&self) -> f32 {
        match self {
            ButtonSize::Small => 13.0,
            ButtonSize::Medium => 14.0,
            ButtonSize::Large => 16.0,
            ButtonSize::Icon => 14.0,
            ButtonSize::Custom(_, _) => 14.0,
        }
    }

    /// Map this size to a [`RadiusToken`] so the active theme's
    /// radii ladder decides the corner reach. Tiny buttons get a
    /// crisp `Sm`, default-sized buttons get `Default`, large
    /// buttons step up to `Lg`. Picks up each theme's
    /// `RadiusTokens` automatically: Hybrid's `Sm=4, Default=8, Lg=14`,
    /// Restrained's `Sm=3, Default=6, Lg=10`, etc.
    fn radius_token(&self) -> blinc_theme::RadiusToken {
        use blinc_theme::RadiusToken;
        match self {
            ButtonSize::Small => RadiusToken::Sm,
            ButtonSize::Medium => RadiusToken::Default,
            ButtonSize::Large => RadiusToken::Lg,
            ButtonSize::Icon => RadiusToken::Default,
            ButtonSize::Custom(_, _) => RadiusToken::Default,
        }
    }
}

/// Icon position within the button
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IconPosition {
    /// Icon appears before the label (left in LTR)
    #[default]
    Start,
    /// Icon appears after the label (right in LTR)
    End,
}

/// Get or create a persistent `SharedState<ButtonState>` for the given key
///
/// Convenience wrapper around `use_fsm_keyed::<_, ButtonState>(key, default)`.
/// Used by dropdown menus, menubars, and navigation menus.
pub(crate) fn use_button_state(key: &str) -> SharedState<ButtonState> {
    use_fsm_keyed(key, ButtonState::default())
}

/// Reset a button state to Idle
///
/// Call this when an overlay closes to clear any lingering hover/pressed states.
pub(crate) fn reset_button_state(key: &str) {
    let state = use_button_state(key);
    let mut inner = state.lock().unwrap();
    inner.state = ButtonState::Idle;
}

/// Create a button with a label
///
/// Uses `#[track_caller]` with UUID to generate a unique instance key.
/// CSS handles all visual states (`:hover`, `:active`) automatically.
///
/// # Example
///
/// ```ignore
/// use blinc_cn::prelude::*;
///
/// cn::button("OK")
///     .variant(ButtonVariant::Primary)
///     .on_click(|_| println!("Confirmed!"))
///
/// // Safe in loops - each button gets unique state
/// for item in items {
///     cn::button(&item.name)
/// }
/// ```
#[track_caller]
pub fn button(label: impl IntoReactive<String>) -> ButtonBuilder {
    ButtonBuilder {
        key: InstanceKey::new("button"),
        config: ButtonConfig {
            label: label.into_reactive(),
            variant: ButtonVariant::default(),
            btn_size: ButtonSize::default(),
            disabled: Reactive::Const(false),
            icon: None,
            icon_position: IconPosition::default(),
            icon_size: None,
            text_color: None,
            on_click: None,
        },
        built: std::cell::OnceCell::new(),
    }
}

/// Internal configuration for ButtonBuilder
#[allow(clippy::type_complexity)]
struct ButtonConfig {
    /// Text content has no property-binding writer (`PropertyId::TextContent`
    /// is declared but unimplemented), so a bound label rebuilds the
    /// subtree via `deps()` rather than patching in place.
    label: Reactive<String>,
    variant: ButtonVariant,
    btn_size: ButtonSize,
    /// `disabled` gates the entire style branch (background, border,
    /// shadow, text colour) plus the FSM's starting state, so a change
    /// can't be patched as a single `RenderProps` write. `Bound` values
    /// register a `deps()` subscription that rebuilds the subtree.
    disabled: Reactive<bool>,
    icon: Option<String>,
    icon_position: IconPosition,
    icon_size: Option<f32>,
    text_color: Option<Color>,
    on_click: Option<Arc<dyn Fn(&blinc_layout::event_handler::EventContext) + Send + Sync>>,
}

impl Clone for ButtonConfig {
    fn clone(&self) -> Self {
        Self {
            label: clone_reactive(&self.label),
            variant: self.variant,
            btn_size: self.btn_size,
            disabled: clone_reactive(&self.disabled),
            icon: self.icon.clone(),
            icon_position: self.icon_position,
            icon_size: self.icon_size,
            text_color: self.text_color,
            on_click: self.on_click.clone(),
        }
    }
}

/// The built button element — wraps `blinc_layout::widgets::button::Button`
/// which provides `Stateful<ButtonState>` FSM for hover/press behavior.
pub struct Button {
    inner: layout_button::Button,
}

impl Button {
    /// Build from a config with the instance key
    fn from_config(instance_key: &str, config: ButtonConfig) -> Self {
        let theme = ThemeState::get();
        let font_size = config.btn_size.font_size();
        let variant = config.variant;
        let disabled = current(&config.disabled);
        let disabled_dep = dep_signal(&config.disabled);

        // Get persistent state for this button
        let state_key = format!("_cn_btn_{}", instance_key);
        let btn_state = use_button_state(&state_key);
        if disabled {
            let mut inner = btn_state.lock().unwrap();
            inner.state = ButtonState::Disabled;
        }

        // Variant colors for the layout button's FSM
        let bg = variant.base_background(theme);
        let hover_bg = variant.background(theme, ButtonState::Hovered);
        let pressed_bg = variant.background(theme, ButtonState::Pressed);

        // Content closure — returns ONLY the text/icon content.
        // The layout button handles bg, rounded, padding, etc.
        let label = current(&config.label);
        let label_dep = dep_signal(&config.label);
        let icon = config.icon.clone();
        let icon_position = config.icon_position;
        let custom_icon_size = config.icon_size;

        let btn_size = config.btn_size;
        let default_fg = config
            .text_color
            .unwrap_or_else(|| variant.foreground(theme));

        // Determine icon-only mode at construction time (needed for sizing)
        let is_icon_only = config.icon.is_some() && label.is_empty();
        let resolved_icon_size = config.icon_size.unwrap_or(font_size + 2.0);

        // Create button with empty content — we'll set up on_state below
        // to read CSS-resolved text_color for both label and icon.
        let mut btn = layout_button::Button::with_content(btn_state, |_state| div())
            .text_color(default_fg)
            .bg_color(bg)
            .hover_color(hover_bg)
            .pressed_color(pressed_bg)
            // Pull the corner radius from the active theme's
            // `RadiusTokens` so Universal HID variants etc. each get
            // their own corner-reach. CSS `.cn-button--{size}` rules
            // can still cascade to override per-size.
            .rounded(theme.radii().get(config.btn_size.radius_token()))
            .items_center()
            .justify_center()
            // CSS classes for user overrides
            .class("cn-button")
            .class(variant.css_class())
            .class(config.btn_size.css_class());

        // Icon-only: explicit square dimensions so items_center/justify_center
        // can center the icon. `flex_shrink_0` pins both axes — without it
        // a narrowing parent row would let taffy compress the width while
        // height held, collapsing the square into a vertical oval.
        // Custom: user-pinned (width, height) wins over the icon-square
        // and content-fit branches.
        // With-label: shrink-wrap to content.
        if let Some(explicit_w) = config.btn_size.width() {
            btn = btn
                .w(explicit_w)
                .h(config.btn_size.height())
                .flex_shrink_0();
        } else if is_icon_only {
            let pad = config.btn_size.padding_y();
            let dim = resolved_icon_size + pad * 2.0;
            btn = btn.w(dim).h(dim).flex_shrink_0();
        } else {
            btn = btn.w_fit();
        }

        // Capture config arc so the on_state callback can read CSS-resolved text_color
        let cfg_arc = btn.config_arc();
        btn = btn.on_state(move |_state, container| {
            // Read CSS-resolved text_color — apply_css_overrides_button has already run
            let fg = cfg_arc.lock().unwrap().text_color;

            if is_icon_only {
                // Icon-only: place SVG directly as child of the Stateful container.
                // The container's items_center + justify_center + explicit w/h
                // handles centering — no content wrapper needed.
                if let Some(ref icon_str) = icon {
                    let icon_size = custom_icon_size.unwrap_or(font_size + 2.0);
                    let svg_str = blinc_icons::to_svg(icon_str, icon_size);
                    let icon_svg = svg(&svg_str).size(icon_size, icon_size).color(fg);
                    container.merge(div().child(icon_svg));
                }
            } else {
                // With label: use content wrapper for flex_row layout
                let label_text = text(&label)
                    .size(font_size)
                    .color(fg)
                    .no_wrap()
                    .v_center()
                    .pointer_events_none()
                    .no_cursor();

                let pad_x = btn_size.padding_x();
                let pad_y = btn_size.padding_y();
                let mut content = div()
                    .flex_row()
                    .items_center()
                    .justify_center()
                    .gap_px(6.0)
                    .padding_x_px(pad_x)
                    .padding_y_px(pad_y)
                    .pointer_events_none();

                if let Some(ref icon_str) = icon {
                    let icon_size = custom_icon_size.unwrap_or(font_size + 2.0);
                    let svg_str = blinc_icons::to_svg(icon_str, icon_size);
                    let icon_svg = svg(&svg_str).size(icon_size, icon_size).color(fg);

                    match icon_position {
                        IconPosition::Start => {
                            content = content.child(icon_svg).child(label_text);
                        }
                        IconPosition::End => {
                            content = content.child(label_text).child(icon_svg);
                        }
                    }
                } else {
                    content = content.child(label_text);
                }

                container.merge(div().child(content));
            }
        });

        if disabled {
            // Filled tonal disabled treatment — matches the disabled
            // select / input look. Solid muted surface + muted text reads
            // as a button (still has identity) but clearly inert.
            // Opacity dimming on a saturated bg (e.g. Primary blue at 50%)
            // washed out to pale lavender against white, losing all contrast.
            // A thin BorderSecondary outline gives the button a sharper
            // silhouette against the page without re-introducing depth.
            let disabled_bg = theme.color(ColorToken::InputBgDisabled);
            let disabled_fg = theme.color(ColorToken::TextTertiary);
            let disabled_border = theme.color(ColorToken::BorderSecondary);
            btn = btn
                .class("cn-button--disabled")
                .bg_color(disabled_bg)
                .hover_color(disabled_bg)
                .pressed_color(disabled_bg)
                .text_color(disabled_fg)
                .border(1.0, disabled_border)
                .disabled(true);
        }

        // Shadow — disabled is intentionally flat (no shadow_md / shadow_sm)
        // so the inert tonal fill reads as non-interactive.
        if !disabled {
            if variant != ButtonVariant::Link && variant != ButtonVariant::Ghost {
                btn = btn.shadow_md();
            }
            if variant == ButtonVariant::Outline {
                btn = btn.shadow_sm();
            }
        }

        // Border for outline variant (skip when disabled — disabled already
        // applied its own BorderSecondary outline above).
        if !disabled {
            if let Some(border_color) = variant.border(theme) {
                btn = btn.border(1.0, border_color);
            }
        }

        // A bound `disabled` rebuilds the subtree: the value picks
        // different backgrounds, borders, shadows and FSM start state,
        // none of which a single property write can express.
        let deps: Vec<_> = [disabled_dep, label_dep].into_iter().flatten().collect();
        if !deps.is_empty() {
            btn = btn.deps(&deps);
        }

        // Click handler
        if let Some(handler) = config.on_click {
            btn = btn.on_click(move |ctx| handler(ctx));
        }

        Self { inner: btn }
    }

    /// Add a CSS class for selector matching
    pub fn class(mut self, name: impl AsRef<str>) -> Self {
        self.inner = self.inner.class(name.as_ref());
        self
    }

    /// Set the element ID for CSS selector matching
    pub fn id(mut self, id: &str) -> Self {
        self.inner = self.inner.id(id);
        self
    }
}

impl ElementBuilder for Button {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.inner.build(tree)
    }

    fn render_props(&self) -> blinc_layout::element::RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.inner.children_builders()
    }

    fn element_type_id(&self) -> blinc_layout::div::ElementTypeId {
        self.inner.element_type_id()
    }

    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        self.inner.event_handlers()
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.inner.layout_style()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.inner.element_classes()
    }

    fn element_id(&self) -> Option<&str> {
        self.inner.element_id()
    }
}

/// Button configuration for building buttons
pub struct ButtonBuilder {
    /// Unique instance key (UUID-based for loop/closure safety)
    key: InstanceKey,
    config: ButtonConfig,
    /// Cached built Button - built lazily on first access
    built: std::cell::OnceCell<Button>,
}

impl ButtonBuilder {
    /// Create a new button builder with explicit key
    ///
    /// For most use cases, prefer `button()` which auto-generates a unique key.
    /// Use this when you need a deterministic key for programmatic access.
    pub fn with_key(key: impl Into<String>, label: impl IntoReactive<String>) -> Self {
        Self {
            key: InstanceKey::explicit(key),
            config: ButtonConfig {
                label: label.into_reactive(),
                variant: ButtonVariant::default(),
                btn_size: ButtonSize::default(),
                disabled: Reactive::Const(false),
                icon: None,
                icon_position: IconPosition::Start,
                icon_size: None,
                text_color: None,
                on_click: None,
            },
            built: std::cell::OnceCell::new(),
        }
    }

    /// Get or build the inner Button
    fn get_or_build(&self) -> &Button {
        self.built
            .get_or_init(|| Button::from_config(self.key.get(), self.config.clone()))
    }

    /// Set the button variant
    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.config.variant = variant;
        self
    }

    /// Set the button size
    pub fn size(mut self, size: ButtonSize) -> Self {
        self.config.btn_size = size;
        self
    }

    /// Make the button disabled
    pub fn disabled(mut self, disabled: impl IntoReactive<bool>) -> Self {
        self.config.disabled = disabled.into_reactive();
        self
    }

    /// Set an icon for the button
    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.config.icon = Some(icon.into());
        self
    }

    /// Set the icon position
    pub fn icon_position(mut self, position: IconPosition) -> Self {
        self.config.icon_position = position;
        self
    }

    /// Set the icon size in pixels (overrides the default derived from font size)
    pub fn icon_size(mut self, size: f32) -> Self {
        self.config.icon_size = Some(size);
        self
    }

    /// Set the text/icon color (overrides variant default)
    pub fn color(mut self, color: impl Into<Color>) -> Self {
        self.config.text_color = Some(color.into());
        self
    }

    /// Set the click handler
    pub fn on_click<F>(mut self, handler: F) -> Self
    where
        F: Fn(&blinc_layout::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.config.on_click = Some(Arc::new(handler));
        self
    }

    /// Build the final Button component
    pub fn build_component(self) -> Button {
        Button::from_config(self.key.get(), self.config)
    }
}

impl ElementBuilder for ButtonBuilder {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.get_or_build().build(tree)
    }

    fn render_props(&self) -> blinc_layout::element::RenderProps {
        self.get_or_build().render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().children_builders()
    }

    fn element_type_id(&self) -> blinc_layout::div::ElementTypeId {
        self.get_or_build().element_type_id()
    }

    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        self.get_or_build().event_handlers()
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.get_or_build().layout_style()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.get_or_build().element_classes()
    }

    fn element_id(&self) -> Option<&str> {
        self.get_or_build().element_id()
    }
}

#[cfg(test)]
mod reactive_disabled_tests {
    use super::*;
    use crate::reactive_props::{current, dep_signal};
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};

    fn bound_state(v: bool) -> blinc_core::reactive::State<bool> {
        let graph = Arc::new(Mutex::new(blinc_core::reactive::ReactiveGraph::new()));
        let signal = graph.lock().unwrap().create_signal(v);
        blinc_core::reactive::State::new(signal, graph, Arc::new(AtomicBool::new(false)))
    }

    #[test]
    fn plain_bool_stays_const() {
        let b = button("Save").disabled(true);
        assert!(matches!(b.config.disabled, Reactive::Const(true)));
        // A constant has nothing to subscribe to, so no rebuild dep.
        assert!(dep_signal(&b.config.disabled).is_none());
    }

    #[test]
    fn bound_state_produces_rebuild_dep() {
        let state = bound_state(true);
        let b = button("Save").disabled(&state);
        assert!(
            matches!(b.config.disabled, Reactive::Bound(_)),
            "a State<bool> must reach the builder as Reactive::Bound"
        );
        assert!(
            dep_signal(&b.config.disabled).is_some(),
            "a bound disabled must yield a SignalId for deps()"
        );
    }

    #[test]
    fn plain_str_label_stays_const() {
        // `&str` / `&String` / `String` call sites must keep working
        // now that the constructor takes `impl IntoReactive<String>`.
        let b = button("Save");
        assert!(matches!(b.config.label, Reactive::Const(_)));
        assert!(dep_signal(&b.config.label).is_none());
        assert_eq!(current(&b.config.label), "Save");
    }

    #[test]
    fn bound_label_produces_rebuild_dep() {
        let graph = Arc::new(Mutex::new(blinc_core::reactive::ReactiveGraph::new()));
        let signal = graph.lock().unwrap().create_signal(String::from("Save"));
        let state =
            blinc_core::reactive::State::new(signal, graph, Arc::new(AtomicBool::new(false)));
        let b = button(&state);
        assert!(matches!(b.config.label, Reactive::Bound(_)));
        assert!(dep_signal(&b.config.label).is_some());
        assert_eq!(current(&b.config.label), "Save");
        state.set(String::from("Saving..."));
        assert_eq!(current(&b.config.label), "Saving...");
    }

    #[test]
    fn bound_value_reads_through_at_build_time() {
        let state = bound_state(true);
        let b = button("Save").disabled(&state);
        assert!(current(&b.config.disabled), "reads the signal's value");
        state.set(false);
        assert!(
            !current(&b.config.disabled),
            "a later set is visible to the next build"
        );
    }
}
