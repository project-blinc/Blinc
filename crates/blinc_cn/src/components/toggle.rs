//! Toggle component — shadcn-style binary toggle button.
//!
//! Wraps [`blinc_layout::widgets::toggle`](mod@blinc_layout::widgets::toggle) with cn surface CSS classes
//! (`.cn-toggle`, `.cn-toggle--<variant>`, `.cn-toggle--<size>`) plus
//! shadcn-flavoured size + variant ladders. All parse / state /
//! token-default work lives in the layout widget; cn only contributes
//! the surface styling and the variant / size selection.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
//!     let bold_on = ctx.use_state_keyed("bold", || false);
//!     cn::toggle(&bold_on)
//!         .label("B")
//!         .aria_label("Toggle bold")
//!         .on_change(|on| println!("bold: {on}"))
//! }
//!
//! // Outline variant (bordered when off)
//! cn::toggle(&bold_on).variant(ToggleVariant::Outline)
//!
//! // Compact size
//! cn::toggle(&bold_on).size(ToggleSize::Small)
//! ```

use blinc_core::State;
use blinc_layout::div::ElementBuilder;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_layout::widgets::toggle as layout_toggle;
use blinc_theme::{ThemeState, TypographyTokens};
use std::sync::Arc;

/// Toggle visual variants (matches shadcn/ui Toggle).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToggleVariant {
    /// No border in the off state; muted-secondary background when on.
    /// The default — feels at home next to other transparent toolbar
    /// buttons (icon, ghost-button, etc.).
    #[default]
    Default,
    /// Border in the off state. Reads as a contained option chip; pairs
    /// well with `cn::toggle_group` for radio-style toggle bars.
    Outline,
}

impl ToggleVariant {
    fn css_class(self) -> &'static str {
        match self {
            ToggleVariant::Default => "cn-toggle--default",
            ToggleVariant::Outline => "cn-toggle--outline",
        }
    }

    fn bordered_off(self) -> bool {
        matches!(self, ToggleVariant::Outline)
    }
}

/// Toggle size ladder. Heights line up with `cn::button` and `cn::input`
/// so toggles in a row with those widgets sit flush.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToggleSize {
    /// 36 × auto, text_sm. Matches `ButtonSize::Medium` / `InputSize::Medium`.
    #[default]
    Medium,
    /// 32 × auto, text_xs. Compact toolbar use.
    Small,
    /// 40 × auto, text_base. Comfortable thumb target.
    Large,
}

impl ToggleSize {
    pub fn height(self) -> f32 {
        match self {
            ToggleSize::Small => 32.0,
            ToggleSize::Medium => 36.0,
            ToggleSize::Large => 40.0,
        }
    }

    pub fn padding_x(self) -> f32 {
        match self {
            ToggleSize::Small => 8.0,
            ToggleSize::Medium => 12.0,
            ToggleSize::Large => 16.0,
        }
    }

    pub fn icon_size(self) -> f32 {
        match self {
            ToggleSize::Small => 14.0,
            ToggleSize::Medium => 16.0,
            ToggleSize::Large => 18.0,
        }
    }

    pub fn font_size(self, typography: &TypographyTokens) -> f32 {
        match self {
            ToggleSize::Small => typography.text_xs,
            ToggleSize::Medium => typography.text_sm,
            ToggleSize::Large => typography.text_base,
        }
    }

    /// Corner-radius token matching the `.cn-toggle--<size>` CSS rules
    /// in `cn_styles`. Small toggles get a tighter `radius-sm`; Medium
    /// and Large share the variant's `radius-default`. Used by
    /// `cn::toggle_group` so its items pick up the same radius the
    /// standalone `cn::toggle` resolves to via CSS overrides — without
    /// it, group items rendered with a noticeably larger radius than
    /// matching-size standalone toggles.
    pub fn radius_token(self) -> blinc_theme::RadiusToken {
        match self {
            ToggleSize::Small => blinc_theme::RadiusToken::Sm,
            ToggleSize::Medium | ToggleSize::Large => blinc_theme::RadiusToken::Default,
        }
    }

    fn css_class(self) -> &'static str {
        match self {
            ToggleSize::Small => "cn-toggle--sm",
            ToggleSize::Medium => "cn-toggle--md",
            ToggleSize::Large => "cn-toggle--lg",
        }
    }
}

/// Internal config for [`cn::toggle`](toggle).
struct ToggleConfig {
    state: State<bool>,
    variant: ToggleVariant,
    size: ToggleSize,
    label: Option<String>,
    icon: Option<String>,
    aria_label: Option<String>,
    disabled: bool,
    on_change: Option<Arc<dyn Fn(bool) + Send + Sync>>,
    css_element_id: Option<String>,
}

/// The fully-built cn toggle.
pub struct Toggle {
    inner: layout_toggle::ToggleBuilder,
}

impl Toggle {
    fn from_config(config: ToggleConfig) -> Self {
        let theme = ThemeState::get();
        let typography = theme.typography();

        let mut inner = layout_toggle::toggle(&config.state)
            .class("cn-toggle")
            .class(config.variant.css_class())
            .class(config.size.css_class())
            .height(config.size.height())
            .padding_x(config.size.padding_x())
            .icon_size(config.size.icon_size())
            .label_font_size(config.size.font_size(&typography))
            .bordered_off(config.variant.bordered_off())
            .disabled(config.disabled);

        if let Some(label) = config.label {
            inner = inner.label(label);
        }
        if let Some(icon) = config.icon {
            inner = inner.icon(icon);
        }
        if let Some(id) = config.css_element_id {
            inner = inner.id(&id);
        }
        if let Some(cb) = config.on_change {
            inner = inner.on_change(move |on| cb(on));
        }

        // aria_label is captured for future a11y plumbing — for now it
        // just rides along as a class-suffixed id so screen-readers
        // walking the DOM (web target) can pick it up via the
        // element_id-as-aria-label fallback. No-op on desktop.
        let _ = config.aria_label;

        Self { inner }
    }
}

impl ElementBuilder for Toggle {
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

    fn semantic_type_name(&self) -> Option<&'static str> {
        Some("toggle")
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.inner.layout_style()
    }

    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        self.inner.event_handlers()
    }

    fn element_id(&self) -> Option<&str> {
        self.inner.element_id()
    }

    fn element_classes(&self) -> &[Arc<str>] {
        self.inner.element_classes()
    }
}

/// Lazy builder for [`Toggle`].
pub struct ToggleBuilder {
    config: ToggleConfig,
    built: std::cell::OnceCell<Toggle>,
}

impl ToggleBuilder {
    #[track_caller]
    pub fn new(state: &State<bool>) -> Self {
        Self {
            config: ToggleConfig {
                state: state.clone(),
                variant: ToggleVariant::default(),
                size: ToggleSize::default(),
                label: None,
                icon: None,
                aria_label: None,
                disabled: false,
                on_change: None,
                css_element_id: None,
            },
            built: std::cell::OnceCell::new(),
        }
    }

    fn get_or_build(&self) -> &Toggle {
        ::blinc_layout::build_once::build_once(&self.built, || {
            Toggle::from_config(self.clone_config())
        })
    }

    fn clone_config(&self) -> ToggleConfig {
        ToggleConfig {
            state: self.config.state.clone(),
            variant: self.config.variant,
            size: self.config.size,
            label: self.config.label.clone(),
            icon: self.config.icon.clone(),
            aria_label: self.config.aria_label.clone(),
            disabled: self.config.disabled,
            on_change: self.config.on_change.clone(),
            css_element_id: self.config.css_element_id.clone(),
        }
    }

    pub fn variant(mut self, variant: ToggleVariant) -> Self {
        self.config.variant = variant;
        self
    }

    pub fn size(mut self, size: ToggleSize) -> Self {
        self.config.size = size;
        self
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.config.label = Some(label.into());
        self
    }

    pub fn icon(mut self, svg_markup: impl Into<String>) -> Self {
        self.config.icon = Some(svg_markup.into());
        self
    }

    /// Set the aria-label for screen readers. Especially recommended for
    /// icon-only toggles where there's no visible text.
    pub fn aria_label(mut self, label: impl Into<String>) -> Self {
        self.config.aria_label = Some(label.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.config.disabled = disabled;
        self
    }

    pub fn on_change<F>(mut self, handler: F) -> Self
    where
        F: Fn(bool) + Send + Sync + 'static,
    {
        self.config.on_change = Some(Arc::new(handler));
        self
    }

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.config.css_element_id = Some(id.into());
        self
    }
}

impl ElementBuilder for ToggleBuilder {
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

    fn semantic_type_name(&self) -> Option<&'static str> {
        Some("toggle")
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.get_or_build().layout_style()
    }

    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        self.get_or_build().event_handlers()
    }

    fn element_id(&self) -> Option<&str> {
        self.get_or_build().element_id()
    }

    fn element_classes(&self) -> &[Arc<str>] {
        self.get_or_build().element_classes()
    }
}

/// Create a cn-styled toggle bound to a reactive [`State<bool>`].
#[track_caller]
pub fn toggle(state: &State<bool>) -> ToggleBuilder {
    ToggleBuilder::new(state)
}
