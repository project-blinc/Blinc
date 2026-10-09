//! Ready-to-use Button widget
//!
//! A button that restyles itself on hover, press and disabled without
//! rebuilding: the pointer state is a set of signals ([`Interaction`]) and
//! the fill and text colour are computeds over them, bound to the node.
//!
//! # Example
//!
//! ```ignore
//! // Simple text button
//! button("Click me")
//!     .on_click(|_| println!("Clicked!"))
//!     .bg_color(Color::RED)
//!     .hover_color(Color::GREEN)
//!
//! // Custom content. The builder is called once, with the signals the
//! // content can bind to, and returns the one element the button holds.
//! button_with(|look| {
//!     div().flex_row().gap(8.0)
//!         .child(svg_icon("save"))
//!         .child(text("Save").color(look.text_color()))
//! })
//! .on_click(|_| save())
//! ```

use std::cell::{OnceCell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use blinc_core::Color;
use blinc_core::reactive::{Computed, ReactiveGraph, Signal, State, computed};
use blinc_theme::{ColorToken, ThemeState};

use crate::binding::{IntoReactive, Reactive};
use crate::css_parser::{ElementState, active_stylesheet};
use crate::div::{Div, ElementBuilder, div};
use crate::element::RenderProps;
use crate::interaction::Interaction;
use crate::key::InstanceKey;
use crate::stateful::ButtonState;
use crate::text::Text;
use crate::tree::{LayoutNodeId, LayoutTree};

/// Button visual states (re-exported from stateful)
pub use crate::stateful::ButtonState as ButtonVisualState;

/// Button-specific configuration (colors)
#[derive(Clone)]
pub struct ButtonConfig {
    pub text_color: Color,
    pub text_size: f32,
    pub bg_color: Color,
    pub hover_color: Color,
    pub pressed_color: Color,
    pub disabled_color: Color,
    /// Text colour while disabled; `text_color` when `None`.
    pub disabled_text_color: Option<Color>,
    /// Border width and colour while disabled, in place of the border the
    /// button has otherwise.
    pub disabled_border: Option<(f32, Color)>,
    /// Take the shadow away while disabled.
    pub flat_when_disabled: bool,
    /// CSS class names for stylesheet matching
    pub css_classes: Vec<std::sync::Arc<str>>,
    /// The border the element has when it is not disabled: set when it is
    /// built, from what the fluent calls gave it.
    base_border: Option<(f32, Color)>,
}

impl Default for ButtonConfig {
    fn default() -> Self {
        let theme = ThemeState::get();
        Self {
            text_color: theme.color(ColorToken::TextInverse),
            text_size: 16.0,
            bg_color: theme.color(ColorToken::Primary),
            hover_color: theme.color(ColorToken::PrimaryHover),
            pressed_color: theme.color(ColorToken::PrimaryActive),
            disabled_color: theme.color(ColorToken::InputBgDisabled),
            disabled_text_color: None,
            disabled_border: None,
            flat_when_disabled: false,
            css_classes: Vec::new(),
            base_border: None,
        }
    }
}

type ClickHandler = Arc<dyn Fn(&crate::event_handler::EventContext) + Send + Sync>;
type ContentBuilder = Box<dyn Fn(&ButtonLook) -> Box<dyn ElementBuilder>>;
/// One fluent call, kept to be replayed on the element when it is built.
type Op = Rc<dyn Fn(Div) -> Div>;

/// What a button's content can follow: the text colour its state resolves
/// to, and the pointer signals.
pub struct ButtonLook {
    text_color: Computed<Color>,
    interaction: Interaction,
}

impl ButtonLook {
    /// The colour the button's text has in its current state, after
    /// stylesheet overrides. Pass it to `.color(..)` of a text or an svg.
    pub fn text_color(&self) -> &Computed<Color> {
        &self.text_color
    }

    /// Hovered, pressed and focused signals of the button.
    pub fn interaction(&self) -> &Interaction {
        &self.interaction
    }
}

/// How a button is disabled: for ever, by a signal, or by a computed.
enum Disabled {
    Const(bool),
    Signal(State<bool>),
    Computed(Computed<bool>),
}

impl Disabled {
    fn from(source: &Reactive<bool>) -> Self {
        match source {
            Reactive::Const(v) => Disabled::Const(*v),
            Reactive::Bound(state) => Disabled::Signal(state.clone()),
            Reactive::Computed(computed) => Disabled::Computed(computed.clone()),
        }
    }

    /// Read inside a computed, through the graph it is given.
    fn in_graph(&self, g: &ReactiveGraph) -> bool {
        match self {
            Disabled::Const(v) => *v,
            Disabled::Signal(state) => g.get(state.signal()).unwrap_or(false),
            Disabled::Computed(computed) => computed.try_get().unwrap_or(false),
        }
    }

    fn now(&self) -> bool {
        match self {
            Disabled::Const(v) => *v,
            Disabled::Signal(state) => state.try_get().unwrap_or(false),
            Disabled::Computed(computed) => computed.try_get().unwrap_or(false),
        }
    }
}

/// The border a stylesheet rule gives a button.
#[derive(Clone, Copy, Default)]
struct BorderOverride {
    width: Option<f32>,
    color: Option<Color>,
}

/// What a button draws in one state.
#[derive(Clone)]
struct Look {
    bg: Color,
    text_color: Color,
    text_size: f32,
    /// The border it has, after what a stylesheet gives it.
    border_width: f32,
    border_color: Option<Color>,
    disabled: bool,
    /// Whether a stylesheet rule gave it a border colour in this state.
    border_from_css: bool,
}

impl Look {
    fn resolve(config: &ButtonConfig, css_id: Option<&str>, state: ButtonState) -> Self {
        let mut cfg = config.clone();
        if state == ButtonState::Disabled {
            if let Some(color) = cfg.disabled_text_color {
                cfg.text_color = color;
            }
        }
        let mut border = BorderOverride::default();
        apply_css_overrides_button(&mut cfg, css_id, &state, &mut border);
        let base = if state == ButtonState::Disabled {
            cfg.disabled_border.or(cfg.base_border)
        } else {
            cfg.base_border
        };
        let bg = match state {
            ButtonState::Idle => cfg.bg_color,
            ButtonState::Hovered => cfg.hover_color,
            ButtonState::Pressed => cfg.pressed_color,
            ButtonState::Disabled => cfg.disabled_color,
        };
        Self {
            bg,
            text_color: cfg.text_color,
            text_size: cfg.text_size,
            border_width: border.width.or(base.map(|b| b.0)).unwrap_or(0.0),
            border_color: border.color.or(base.map(|b| b.1)),
            disabled: state == ButtonState::Disabled,
            border_from_css: border.color.is_some(),
        }
    }
}

fn visual_state(disabled: bool, hovered: bool, pressed: bool) -> ButtonState {
    if disabled {
        ButtonState::Disabled
    } else if pressed {
        ButtonState::Pressed
    } else if hovered {
        ButtonState::Hovered
    } else {
        ButtonState::Idle
    }
}

/// What a button resolves its look from.
struct LookSource {
    config: ButtonConfig,
    css_id: Option<String>,
    disabled: Disabled,
    hovered: Signal<bool>,
    pressed: Signal<bool>,
}

/// A value taken from the look, kept current by the signals it is resolved from.
fn follow_look<T: Clone + Send + 'static>(
    source: &Arc<LookSource>,
    pick: impl Fn(&Look) -> T + Send + 'static,
) -> Computed<T> {
    let source = Arc::clone(source);
    computed(move |g: &ReactiveGraph| {
        let state = visual_state(
            source.disabled.in_graph(g),
            g.get(source.hovered).unwrap_or(false),
            g.get(source.pressed).unwrap_or(false),
        );
        pick(&Look::resolve(
            &source.config,
            source.css_id.as_deref(),
            state,
        ))
    })
}

/// Button widget
///
/// Buttons can have custom content via `button_with()` or use the simple
/// `button("Label")` constructor for text-only buttons.
///
/// The element is made when it is first asked for, so every fluent call made
/// before that counts, in any order.
pub struct Button {
    key: InstanceKey,
    config: ButtonConfig,
    disabled: Reactive<bool>,
    label: Option<Reactive<String>>,
    content: Option<ContentBuilder>,
    click_handler: Option<ClickHandler>,
    ops: Vec<Op>,
    css_id: Option<String>,
    built: OnceCell<Div>,
}

impl Button {
    /// Create a button with a text label.
    ///
    /// The label can follow a signal; it is patched in place.
    #[track_caller]
    pub fn new(label: impl IntoReactive<String>) -> Self {
        let mut button = Self::empty();
        button.label = Some(label.into_reactive());
        button
    }

    /// Create a button holding the element `content` returns.
    ///
    /// `content` is called once, when the button is built, with the signals
    /// the content can bind to. It does not run again when the pointer
    /// state changes: bind what should follow it.
    ///
    /// Note: When using custom content, `text_color()` only reaches content
    /// that binds [`ButtonLook::text_color`].
    #[track_caller]
    pub fn with_content<F, E>(content: F) -> Self
    where
        F: Fn(&ButtonLook) -> E + 'static,
        E: ElementBuilder + 'static,
    {
        let mut button = Self::empty();
        button.content = Some(Box::new(move |look| {
            Box::new(content(look)) as Box<dyn ElementBuilder>
        }));
        button
    }

    #[track_caller]
    fn empty() -> Self {
        Self {
            key: InstanceKey::new("button"),
            config: ButtonConfig::default(),
            disabled: Reactive::Const(false),
            label: None,
            content: None,
            click_handler: None,
            ops: Vec::new(),
            css_id: None,
            built: OnceCell::new(),
        }
    }

    /// Name the button's pointer state, so it persists across rebuilds
    /// under a key of your choosing instead of one made from where it was
    /// created. Needed for buttons made in a loop from one place.
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = InstanceKey::explicit(key);
        self
    }

    // Button-specific methods
    pub fn bg_color(mut self, color: impl Into<Color>) -> Self {
        self.config.bg_color = color.into();
        self
    }

    pub fn hover_color(mut self, color: impl Into<Color>) -> Self {
        self.config.hover_color = color.into();
        self
    }

    pub fn pressed_color(mut self, color: impl Into<Color>) -> Self {
        self.config.pressed_color = color.into();
        self
    }

    /// Fill while disabled.
    pub fn disabled_color(mut self, color: impl Into<Color>) -> Self {
        self.config.disabled_color = color.into();
        self
    }

    /// Text colour while disabled; `text_color` when not set.
    pub fn disabled_text_color(mut self, color: impl Into<Color>) -> Self {
        self.config.disabled_text_color = Some(color.into());
        self
    }

    /// A border of this width and colour while disabled, in place of the
    /// button's own.
    pub fn disabled_border(mut self, width: f32, color: impl Into<Color>) -> Self {
        self.config.disabled_border = Some((width, color.into()));
        self
    }

    /// Take the button's shadow away while it is disabled.
    pub fn flat_when_disabled(mut self, flat: bool) -> Self {
        self.config.flat_when_disabled = flat;
        self
    }

    /// Set text color for simple text buttons created with `button("Label")`
    ///
    /// For custom content, bind [`ButtonLook::text_color`] in the content.
    pub fn text_color(mut self, color: impl Into<Color>) -> Self {
        self.config.text_color = color.into();
        self
    }

    /// Set text size for simple text buttons created with `button("Label")`
    ///
    /// Note: This has no effect on buttons created with `button_with()`.
    pub fn text_size(mut self, size: f32) -> Self {
        self.config.text_size = size;
        self
    }

    /// Disable the button, for good or while a signal holds. A disabled
    /// button has its disabled fill and does not click.
    pub fn disabled(mut self, disabled: impl IntoReactive<bool>) -> Self {
        self.disabled = disabled.into_reactive();
        self
    }

    pub fn on_click<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.click_handler = Some(Arc::new(handler));
        self
    }

    /// Set the element ID for CSS selector targeting
    pub fn id(mut self, id: &str) -> Self {
        self.css_id = Some(id.to_string());
        let id = id.to_string();
        self.ops.push(Rc::new(move |d| d.id(id.clone())));
        self
    }

    /// Add a CSS class for selector matching
    pub fn class(mut self, class: &str) -> Self {
        self.config
            .css_classes
            .push(blinc_core::intern::intern(class));
        let class = class.to_string();
        self.ops.push(Rc::new(move |d| d.class(&class)));
        self
    }

    /// The element: the recorded fluent calls replayed on a fresh `Div`,
    /// with the fill, the content and the pointer wiring added.
    fn body(&self) -> &Div {
        crate::build_once::build_once(&self.built, || self.make())
    }

    fn make(&self) -> Div {
        let chassis = self.ops.iter().fold(div(), |d, op| op(d));
        let interaction = Interaction::keyed(self.key.get());
        let disabled = Disabled::from(&self.disabled);
        let own = chassis.render_props();
        // What only a disabled button has counts only if this one can be.
        let can_be_disabled = !matches!(self.disabled, Reactive::Const(false));
        let mut config = self.config.clone();
        if !can_be_disabled {
            config.disabled_border = None;
            config.flat_when_disabled = false;
        }
        config.base_border = own.border_color.map(|c| (own.border_width, c));
        let first = Look::resolve(
            &config,
            self.css_id.as_deref(),
            visual_state(disabled.now(), false, false),
        );
        let has_border = config.base_border.is_some()
            || config.disabled_border.is_some()
            || first.border_from_css;
        let flat = config.flat_when_disabled;
        let base_shadow = own.shadow;
        let source = Arc::new(LookSource {
            config,
            css_id: self.css_id.clone(),
            disabled,
            hovered: interaction.hovered().signal(),
            pressed: interaction.pressed().signal(),
        });

        let bg = follow_look(&source, |l| l.bg);
        let text_color = follow_look(&source, |l| l.text_color);

        let mut body = chassis.bg(&bg).track(&interaction);

        // The border and the shadow can change with the state, so they follow
        // it. A button with no border in any state gets none.
        if has_border {
            body = body
                .border_width(follow_look(&source, |l| l.border_width))
                .border_color(follow_look(&source, |l| {
                    l.border_color.unwrap_or(Color::TRANSPARENT)
                }));
        }
        if flat {
            body = body.shadows(follow_look(&source, move |l| {
                if l.disabled {
                    Vec::new()
                } else {
                    base_shadow.clone()
                }
            }));
        }

        let look = ButtonLook {
            text_color: text_color.clone(),
            interaction,
        };
        if let Some(content) = &self.content {
            body = body.child_box(content(&look));
        } else if let Some(label) = &self.label {
            body = body.child(
                Text::bound(clone_reactive(label))
                    .size(first.text_size)
                    .color(&text_color),
            );
        }

        if let Some(handler) = self.click_handler.clone() {
            let gate = Arc::clone(&source);
            body = body.on_click(move |ctx| {
                if !gate.disabled.now() {
                    handler(ctx);
                }
            });
        }
        body
    }
}

fn clone_reactive<T: Clone>(r: &Reactive<T>) -> Reactive<T> {
    match r {
        Reactive::Const(v) => Reactive::Const(v.clone()),
        Reactive::Bound(state) => Reactive::Bound(state.clone()),
        Reactive::Computed(computed) => Reactive::Computed(computed.clone()),
    }
}

/// Forward fluent layout calls to the element: each is recorded and replayed
/// when the button is built.
macro_rules! forward {
    ($($name:ident($($arg:ident: $ty:ty),*);)*) => {
        impl Button {
            $(
                pub fn $name(mut self, $($arg: $ty),*) -> Self {
                    self.ops.push(Rc::new(move |d: Div| d.$name($($arg),*)));
                    self
                }
            )*
        }
    };
}

forward! {
    px(v: f32);
    py(v: f32);
    p(v: f32);
    pt(v: f32);
    pb(v: f32);
    pl(v: f32);
    pr(v: f32);
    rounded(v: f32);
    border(width: f32, color: Color);
    border_color(color: Color);
    border_width(width: f32);
    w(v: f32);
    h(v: f32);
    w_full();
    h_full();
    w_fit();
    h_fit();
    mt(v: f32);
    mb(v: f32);
    ml(v: f32);
    mr(v: f32);
    mx(v: f32);
    my(v: f32);
    m(v: f32);
    gap(v: f32);
    items_center();
    items_start();
    items_end();
    justify_center();
    justify_start();
    justify_end();
    justify_between();
    flex_row();
    flex_col();
    flex_grow();
    flex_shrink();
    flex_shrink_0();
    shadow_sm();
    shadow_md();
    shadow_lg();
    shadow_xl();
    opacity(v: f32);
}

/// Create a button with a text label
///
/// This is the most common button constructor. For buttons with custom
/// content (icons, multiple elements, etc.), use `button_with()`.
///
/// # Example
/// ```ignore
/// button("Save")
///     .on_click(|_| save_data())
///     .bg_color(Color::GREEN)
/// ```
#[track_caller]
pub fn button(label: impl IntoReactive<String>) -> Button {
    Button::new(label)
        .px(12.0)
        .py(6.0)
        .rounded(8.0)
        .items_center()
        .justify_center()
}

/// Create a button holding the element `content` returns
///
/// `content` is called once, when the button is built; bind what should
/// follow the pointer or the disabled state through the [`ButtonLook`] it is
/// given.
///
/// # Example
/// ```ignore
/// // Icon button
/// button_with(|_| div().child(svg_icon("trash")))
///     .on_click(|_| delete_item())
///
/// // Button with icon and text
/// button_with(|look| {
///     div().flex_row().gap(8.0)
///         .child(svg_icon("save"))
///         .child(text("Save").color(look.text_color()))
/// })
/// .on_click(|_| save())
/// ```
#[track_caller]
pub fn button_with<F, E>(content: F) -> Button
where
    F: Fn(&ButtonLook) -> E + 'static,
    E: ElementBuilder + 'static,
{
    Button::with_content(content)
        .px(12.0)
        .py(6.0)
        .rounded(8.0)
        .items_center()
        .justify_center()
}

impl ElementBuilder for Button {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.body().build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.body().render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.body().children_builders()
    }

    fn element_type_id(&self) -> crate::div::ElementTypeId {
        crate::div::ElementTypeId::Div
    }

    fn semantic_type_name(&self) -> Option<&'static str> {
        Some("button")
    }

    fn element_id(&self) -> Option<&str> {
        self.css_id.as_deref()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        &self.config.css_classes
    }

    fn event_handlers(&self) -> Option<&crate::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(self.body())
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.body().layout_style()
    }
}

/// Apply CSS overrides from active stylesheet to button config
fn apply_css_overrides_button(
    cfg: &mut ButtonConfig,
    css_element_id: Option<&str>,
    state: &ButtonState,
    border: &mut BorderOverride,
) {
    let stylesheet = match active_stylesheet() {
        Some(s) => s,
        None => return,
    };

    // Helper to apply a single ElementStyle to the button config + container
    let apply = |cfg: &mut ButtonConfig,
                 border: &mut BorderOverride,
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
                border.width = Some(bw);
                border.color = Some(bc);
            }
        } else if let Some(bc) = style.border_color {
            // border-color only (keep existing width)
            border.color = Some(bc);
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
            apply(cfg, border, base, false);
        }
        if let Some(s) = element_state {
            if let Some(state_style) = stylesheet.get_class_with_state(class, s) {
                apply(cfg, border, state_style, true);
            }
        }
    }

    // 2. Resolve by element ID (higher priority)
    if let Some(id) = css_element_id {
        if let Some(base) = stylesheet.get(id) {
            apply(cfg, border, base, false);
        }
        if let Some(s) = element_state {
            if let Some(state_style) = stylesheet.get_with_state(id, s) {
                apply(cfg, border, state_style, true);
            }
        }
    }
}
