//! Ready-to-use Checkbox widget
//!
//! A checkbox with built-in toggle, hover states, and CSS styling support.
//! Uses `Stateful<ButtonState>` internally for automatic hover/pressed detection.
//! Uses reactive `State<bool>` for checked state, connected via `.deps()` so
//! the visual updates immediately on click.
//! Follows the lazy `OnceCell` initialization pattern from `blinc_cn`.
//!
//! # Example
//!
//! ```ignore
//! let checked = ctx.use_state_keyed("my_checkbox", || false);
//! div().child(
//!     checkbox(&checked)
//!         .label("Remember me")
//!         .on_change(|checked| println!("Checked: {}", checked))
//! )
//! ```

use std::sync::Arc;

use blinc_core::reactive::{ReactiveGraph, computed};
use blinc_core::{Color, State};
use blinc_theme::{ColorToken, ThemeState};

use crate::css_parser::{ElementState, Stylesheet, active_stylesheet};
use crate::div::{ElementBuilder, div};
use crate::element::RenderProps;
use crate::element_style::ElementStyle;
use crate::interaction::Interaction;
use crate::key::InstanceKey;
use crate::svg::svg;
use crate::text::text;
use crate::tree::{LayoutNodeId, LayoutTree};

/// Checkbox configuration
///
/// Color fields are `Option<Color>` — when `None`, theme defaults are resolved
/// fresh inside the `on_state` callback so the widget reacts to theme changes.
#[derive(Clone)]
pub struct CheckboxConfig {
    /// Reactive checked state signal
    checked: State<bool>,
    /// Whether disabled
    pub disabled: bool,
    /// Label text (optional)
    pub label: Option<String>,
    /// Box size
    pub size: f32,
    /// Gap between box and label
    pub gap: f32,
    /// Unchecked background color (None = transparent)
    pub unchecked_bg: Option<Color>,
    /// Checked background color (None = theme Primary)
    pub checked_bg: Option<Color>,
    /// Hover tint amount
    pub hover_tint: f32,
    /// Check mark color (None = theme TextInverse)
    pub check_color: Option<Color>,
    /// Label color (None = theme TextPrimary)
    pub label_color: Option<Color>,
    /// Label font size
    pub label_font_size: f32,
    /// Corner radius
    pub corner_radius: f32,
    /// Border color (None = theme BorderSecondary)
    pub border_color: Option<Color>,
    /// Border width
    pub border_width: f32,
    /// Disabled opacity
    pub disabled_opacity: f32,
    /// CSS element ID for stylesheet matching
    pub css_element_id: Option<String>,
    /// Change handler
    pub on_change: Option<Arc<dyn Fn(bool) + Send + Sync>>,
}

impl CheckboxConfig {
    fn new(checked: State<bool>) -> Self {
        Self {
            checked,
            disabled: false,
            label: None,
            size: 20.0,
            gap: 8.0,
            unchecked_bg: None,
            checked_bg: None,
            hover_tint: 0.1,
            check_color: None,
            label_color: None,
            label_font_size: 14.0,
            corner_radius: 4.0,
            border_color: None,
            border_width: 2.0,
            disabled_opacity: 0.5,
            css_element_id: None,
            on_change: None,
        }
    }
}

/// Resolved checkbox colors — theme defaults filled in for any `None` config values
struct ResolvedColors {
    unchecked_bg: Color,
    checked_bg: Color,
    check_color: Color,
    label_color: Color,
    border_color: Color,
}

impl ResolvedColors {
    fn from_config(config: &CheckboxConfig, theme: &ThemeState) -> Self {
        Self {
            unchecked_bg: config.unchecked_bg.unwrap_or(Color::TRANSPARENT),
            checked_bg: config
                .checked_bg
                .unwrap_or_else(|| theme.color(ColorToken::Primary)),
            check_color: config
                .check_color
                .unwrap_or_else(|| theme.color(ColorToken::TextInverse)),
            label_color: config
                .label_color
                .unwrap_or_else(|| theme.color(ColorToken::TextPrimary)),
            border_color: config
                .border_color
                .unwrap_or_else(|| theme.color(ColorToken::BorderSecondary)),
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

/// Apply CSS stylesheet overrides to resolved colors and config
fn apply_css_overrides_checkbox(
    colors: &mut ResolvedColors,
    cfg: &mut CheckboxConfig,
    stylesheet: &Stylesheet,
    element_id: &str,
    is_checked: bool,
    is_hovered: bool,
    is_disabled: bool,
) {
    // 1. Base style
    if let Some(base) = stylesheet.get(element_id) {
        apply_style_to_checkbox(colors, cfg, base, is_checked);
    }
    // 2. :checked (if checked)
    if is_checked {
        if let Some(s) = stylesheet.get_with_state(element_id, ElementState::Checked) {
            apply_style_to_checkbox(colors, cfg, s, is_checked);
        }
    }
    // 3. :hover (layered after :checked)
    if is_hovered {
        if let Some(s) = stylesheet.get_with_state(element_id, ElementState::Hover) {
            apply_style_to_checkbox(colors, cfg, s, is_checked);
        }
    }
    // 4. :disabled (highest priority)
    if is_disabled {
        if let Some(s) = stylesheet.get_with_state(element_id, ElementState::Disabled) {
            apply_style_to_checkbox(colors, cfg, s, is_checked);
        }
    }
}

/// Apply an ElementStyle to resolved colors and config
fn apply_style_to_checkbox(
    colors: &mut ResolvedColors,
    cfg: &mut CheckboxConfig,
    style: &ElementStyle,
    is_checked: bool,
) {
    if let Some(blinc_core::Brush::Solid(color)) = style.background {
        if is_checked {
            colors.checked_bg = color;
        } else {
            colors.unchecked_bg = color;
        }
    }
    if let Some(color) = style.border_color {
        colors.border_color = color;
    }
    if let Some(w) = style.border_width {
        cfg.border_width = w;
    }
    if let Some(cr) = style.corner_radius {
        cfg.corner_radius = cr.top_left;
    }
    if let Some(opacity) = style.opacity {
        cfg.disabled_opacity = opacity;
    }
    if let Some(color) = style.accent_color {
        colors.check_color = color;
    }
    if let Some(color) = style.text_color {
        colors.label_color = color;
    }
    if let Some(size) = style.font_size {
        cfg.label_font_size = size;
    }
}

/// The fully-built checkbox component (Div containing stateful checkbox + optional label)
pub struct Checkbox {
    inner: crate::div::Div,
}

/// What a checkbox draws in one state: the part the state can change.
#[derive(Clone)]
struct Look {
    bg: Color,
    border_color: Color,
    check_color: Color,
    /// Config after stylesheet overrides, for the parts fixed at build.
    cfg: CheckboxConfig,
}

impl Look {
    fn resolve(config: &CheckboxConfig, is_checked: bool, is_hovered: bool) -> Self {
        let is_disabled = config.disabled;

        // Query theme fresh each time for reactive theme support
        let theme = ThemeState::get();
        let mut colors = ResolvedColors::from_config(config, theme);
        let mut cfg = config.clone();

        // Apply CSS overrides if element has an ID
        if let Some(ref element_id) = config.css_element_id {
            if let Some(stylesheet) = active_stylesheet() {
                apply_css_overrides_checkbox(
                    &mut colors,
                    &mut cfg,
                    &stylesheet,
                    element_id,
                    is_checked,
                    is_hovered,
                    is_disabled,
                );
            }
        }

        let bg = if is_checked {
            if is_hovered && !is_disabled {
                lighten(colors.checked_bg, cfg.hover_tint)
            } else {
                colors.checked_bg
            }
        } else if is_hovered && !is_disabled {
            lighten(colors.unchecked_bg, cfg.hover_tint)
        } else {
            colors.unchecked_bg
        };

        Self {
            bg,
            border_color: colors.border_color,
            check_color: colors.check_color,
            cfg,
        }
    }
}

impl Checkbox {
    /// Build from a full configuration
    fn with_config(instance_key: &InstanceKey, config: CheckboxConfig) -> Self {
        let checked_state = config.checked.clone();
        let checked_for_click = config.checked.clone();
        let on_change = config.on_change.clone();
        let disabled = config.disabled;

        // Derive a unique key from the instance key
        let key = instance_key.get().to_string();
        let interaction = Interaction::keyed(&key);

        let label_text = config.label.clone();
        let gap = config.gap;

        // The part fixed at build: sizes and shapes cannot follow a state
        // without a layout, so they are resolved once, as the box stands.
        let first = Look::resolve(&config, checked_state.get(), false);
        let cfg = &first.cfg;
        let icon_size = cfg.size * 0.7;

        // What a state changes is bound: each is a computed over the checked
        // signal and the element's hover, so a change patches the node
        // instead of rebuilding it.
        let (checked, hovered) = (checked_state.signal(), interaction.hovered().signal());
        let follow = |pick: fn(&Look) -> Color| {
            let config = config.clone();
            computed(move |g: &ReactiveGraph| {
                pick(&Look::resolve(
                    &config,
                    g.get(checked).unwrap_or(false),
                    g.get(hovered).unwrap_or(false),
                ))
            })
        };
        let bg = follow(|l| l.bg);
        let border_color = follow(|l| l.border_color);
        let check_color = follow(|l| l.check_color);

        // The mark is always there and shown while checked, so toggling
        // changes what is visible and nothing is rebuilt.
        let checkmark_svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{icon_size}" height="{icon_size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6 9 17l-5-5"/></svg>"#
        );
        let mut checkbox = div()
            .w(cfg.size)
            .h(cfg.size)
            .bg(&bg)
            .rounded(cfg.corner_radius)
            .border_width(cfg.border_width)
            .border_color(&border_color)
            .items_center()
            .justify_center()
            .track(&interaction)
            .child(
                div()
                    .w(icon_size)
                    .h(icon_size)
                    .visible(&checked_state)
                    .child(
                        svg(&checkmark_svg)
                            .size(icon_size, icon_size)
                            .color(&check_color)
                            .internal(),
                    ),
            );
        if disabled {
            checkbox = checkbox.opacity(cfg.disabled_opacity);
        }

        // Add click handler — toggles the reactive signal
        let checkbox = checkbox.on_click(move |_| {
            if disabled {
                return;
            }
            let current = checked_for_click.get();
            checked_for_click.set(!current);

            if let Some(ref handler) = on_change {
                handler(!current);
            }
        });

        // Wrap in container with label support
        let inner = if let Some(ref label) = label_text {
            let theme = ThemeState::get();
            let label_color = if disabled {
                theme.color(ColorToken::TextTertiary)
            } else {
                theme.color(ColorToken::TextPrimary)
            };

            div()
                .flex_row()
                .gap(gap)
                .items_center()
                .cursor_pointer()
                .child(checkbox)
                .child(text(label).size(14.0).color(label_color))
        } else {
            div()
                .flex_row()
                .gap(gap)
                .items_center()
                .cursor_pointer()
                .child(checkbox)
        };

        Self { inner }
    }
}

impl ElementBuilder for Checkbox {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.inner.build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.inner.children_builders()
    }

    fn element_type_id(&self) -> crate::div::ElementTypeId {
        self.inner.element_type_id()
    }

    fn semantic_type_name(&self) -> Option<&'static str> {
        Some("checkbox")
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.inner.layout_style()
    }

    fn event_handlers(&self) -> Option<&crate::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(&self.inner)
    }

    fn element_id(&self) -> Option<&str> {
        self.inner.element_id()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.inner.element_classes()
    }
}

/// Builder for creating Checkbox components with fluent API.
/// Config is accumulated via builder methods; the actual Checkbox is built lazily
/// via `OnceCell::get_or_init()` on first access (build/render_props/children).
pub struct CheckboxBuilder {
    key: InstanceKey,
    config: CheckboxConfig,
    /// Cached built Checkbox — built lazily on first access
    built: std::cell::OnceCell<Checkbox>,
}

impl CheckboxBuilder {
    /// Create a new checkbox builder with reactive checked state
    #[track_caller]
    pub fn new(checked: &State<bool>) -> Self {
        Self {
            key: InstanceKey::new("checkbox"),
            config: CheckboxConfig::new(checked.clone()),
            built: std::cell::OnceCell::new(),
        }
    }

    /// Get or build the inner Checkbox
    fn get_or_build(&self) -> &Checkbox {
        self.built
            .get_or_init(|| Checkbox::with_config(&self.key, self.config.clone()))
    }

    /// Set the CSS element ID for stylesheet matching
    pub fn id(mut self, id: &str) -> Self {
        self.config.css_element_id = Some(id.to_string());
        self
    }

    /// Set label
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.config.label = Some(label.into());
        self
    }

    /// Set checkbox box size
    pub fn checkbox_size(mut self, size: f32) -> Self {
        self.config.size = size;
        self
    }

    /// Set checked background color
    pub fn checked_bg(mut self, color: impl Into<Color>) -> Self {
        self.config.checked_bg = Some(color.into());
        self
    }

    /// Set unchecked background color
    pub fn unchecked_bg(mut self, color: impl Into<Color>) -> Self {
        self.config.unchecked_bg = Some(color.into());
        self
    }

    /// Set check mark color
    pub fn check_color(mut self, color: impl Into<Color>) -> Self {
        self.config.check_color = Some(color.into());
        self
    }

    /// Set label color
    pub fn label_color(mut self, color: impl Into<Color>) -> Self {
        self.config.label_color = Some(color.into());
        self
    }

    /// Set label font size
    pub fn label_font_size(mut self, size: f32) -> Self {
        self.config.label_font_size = size;
        self
    }

    /// Set corner radius
    pub fn rounded(mut self, radius: f32) -> Self {
        self.config.corner_radius = radius;
        self
    }

    /// Set border color
    pub fn border_color(mut self, color: impl Into<Color>) -> Self {
        self.config.border_color = Some(color.into());
        self
    }

    /// Set border width
    pub fn border_width(mut self, width: f32) -> Self {
        self.config.border_width = width;
        self
    }

    /// Set disabled state
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.config.disabled = disabled;
        self
    }

    /// Set change handler
    pub fn on_change<F>(mut self, handler: F) -> Self
    where
        F: Fn(bool) + Send + Sync + 'static,
    {
        self.config.on_change = Some(Arc::new(handler));
        self
    }
}

impl ElementBuilder for CheckboxBuilder {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.get_or_build().build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.get_or_build().render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().children_builders()
    }

    fn element_type_id(&self) -> crate::div::ElementTypeId {
        self.get_or_build().element_type_id()
    }

    fn semantic_type_name(&self) -> Option<&'static str> {
        Some("checkbox")
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.get_or_build().layout_style()
    }

    fn event_handlers(&self) -> Option<&crate::event_handler::EventHandlers> {
        self.get_or_build().event_handlers()
    }

    fn element_id(&self) -> Option<&str> {
        self.get_or_build().element_id()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.get_or_build().element_classes()
    }
}

/// Create a checkbox with reactive checked state
///
/// The checkbox uses `Stateful<ButtonState>` internally for hover detection,
/// and connects to the `State<bool>` signal via `.deps()` for immediate
/// visual updates on toggle.
///
/// Use `.id("name")` to enable CSS styling via `:hover`, `:checked`, `:disabled`.
///
/// # Example
///
/// ```ignore
/// let checked = ctx.use_state_keyed("remember_me", || false);
/// checkbox(&checked)
///     .label("Remember me")
///     .on_change(|checked| println!("Checked: {}", checked))
/// ```
#[track_caller]
pub fn checkbox(checked: &State<bool>) -> CheckboxBuilder {
    CheckboxBuilder::new(checked)
}

/// Create a checkbox with label and reactive state
#[track_caller]
pub fn checkbox_labeled(checked: &State<bool>, label: impl Into<String>) -> CheckboxBuilder {
    CheckboxBuilder::new(checked).label(label)
}
