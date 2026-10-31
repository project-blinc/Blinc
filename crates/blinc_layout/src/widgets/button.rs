//! Ready-to-use Button widget
//!
//! A button is an element that carries its state as selectors: `:hover`,
//! `:active` and `:disabled` match it like any other element, so how it looks
//! in each state, and how it eases between them, is stylesheet. The default
//! look is the `.blinc-button` rules of [`crate::ua_styles::UA_STYLES`]; a
//! theme or an application restyles it with its own rules.
//!
//! # Example
//!
//! ```ignore
//! // Simple text button
//! button("Click me").on_click(|_| println!("Clicked!"))
//!
//! // Restyled from a stylesheet
//! //   .danger { background: #c0392b; transition: background-color 150ms; }
//! //   .danger:hover { background: #e74c3c; }
//! button("Delete").class("danger")
//!
//! // Custom content. The builder is called once and returns the one element
//! // the button holds; text and icons inside take the button's `color`.
//! button_with(|_| {
//!     div().flex_row().gap(8.0)
//!         .child(svg_icon("save"))
//!         .child(text("Save"))
//! })
//! .on_click(|_| save())
//! ```

use std::cell::OnceCell;
use std::rc::Rc;
use std::sync::Arc;

use blinc_core::Color;
use blinc_core::reactive::{Computed, State};

use crate::binding::{IntoReactive, Reactive};
use crate::div::{Div, ElementBuilder, div};
use crate::element::RenderProps;
use crate::interaction::Interaction;
use crate::key::InstanceKey;
use crate::text::Text;
use crate::tree::{LayoutNodeId, LayoutTree};

/// Button visual states (re-exported from stateful)
pub use crate::stateful::ButtonState as ButtonVisualState;

/// The class whose rules in [`crate::ua_styles::UA_STYLES`] give `button()`
/// its default look.
pub const DEFAULT_CLASS: &str = "blinc-button";

type ClickHandler = Arc<dyn Fn(&crate::event_handler::EventContext) + Send + Sync>;
type ContentBuilder = Box<dyn Fn(&ButtonLook) -> Box<dyn ElementBuilder>>;
/// One fluent call, kept to be replayed on the element when it is built.
type Op = Rc<dyn Fn(Div) -> Div>;

/// What a button's content can follow: the pointer signals.
pub struct ButtonLook {
    interaction: Interaction,
}

impl ButtonLook {
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

    fn now(&self) -> bool {
        match self {
            Disabled::Const(v) => *v,
            Disabled::Signal(state) => state.try_get().unwrap_or(false),
            Disabled::Computed(computed) => computed.try_get().unwrap_or(false),
        }
    }
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
    disabled: Reactive<bool>,
    label: Option<Reactive<String>>,
    content: Option<ContentBuilder>,
    click_handler: Option<ClickHandler>,
    ops: Vec<Op>,
    text_color: Option<Color>,
    text_size: f32,
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
    /// `content` is called once, when the button is built. It does not run
    /// again when the pointer state changes: text and icons inside take the
    /// button's `color` as it changes.
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
            disabled: Reactive::Const(false),
            label: None,
            content: None,
            click_handler: None,
            ops: Vec::new(),
            text_color: None,
            text_size: 16.0,
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

    /// Apply an `ElementStyle` to the button's element as its default tier,
    /// below an application stylesheet and below any fluent call made after
    /// it. Replayed in call order with the other fluent calls.
    pub fn style(mut self, style: &crate::element_style::ElementStyle) -> Self {
        let style = style.clone();
        self.ops.push(Rc::new(move |d: Div| d.style(&style)));
        self
    }

    /// The fill, in every state: a fluent value wins over the default look's
    /// rules, so this fixes the fill. To restyle by state, write rules.
    pub fn bg_color(mut self, color: impl Into<Color>) -> Self {
        let color = color.into();
        self.ops.push(Rc::new(move |d: Div| d.bg(color)));
        self
    }

    /// Colour of the label of a button made with `button("Label")`. For custom
    /// content, give the content's own text a colour or set `color` in a rule.
    pub fn text_color(mut self, color: impl Into<Color>) -> Self {
        self.text_color = Some(color.into());
        self
    }

    /// Set text size for simple text buttons created with `button("Label")`
    ///
    /// Note: This has no effect on buttons created with `button_with()`.
    pub fn text_size(mut self, size: f32) -> Self {
        self.text_size = size;
        self
    }

    /// Disable the button, for good or while a signal holds. A disabled
    /// button matches `:disabled` and does not click.
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
        let class = class.to_string();
        self.ops.push(Rc::new(move |d| d.class(&class)));
        self
    }

    /// The element: the recorded fluent calls replayed on a fresh `Div`,
    /// with the content and the pointer wiring added.
    fn body(&self) -> &Div {
        crate::build_once::build_once(&self.built, || self.make())
    }

    fn make(&self) -> Div {
        // `:disabled` rules match while the button is disabled.
        let chassis = self.ops.iter().fold(div(), |d, op| op(d)).attr_when(
            "disabled",
            "",
            clone_reactive(&self.disabled),
        );
        let interaction = Interaction::keyed(self.key.get());
        let disabled = Disabled::from(&self.disabled);
        let mut body = chassis.track(&interaction);

        let look = ButtonLook { interaction };
        if let Some(content) = &self.content {
            body = body.child_box(content(&look));
        } else if let Some(label) = &self.label {
            let mut text = Text::bound(clone_reactive(label)).size(self.text_size);
            if let Some(color) = self.text_color {
                text = text.color(color);
            }
            body = body.child(text);
        }

        if let Some(handler) = self.click_handler.clone() {
            body = body.on_click(move |ctx| {
                if !disabled.now() {
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
        .class(DEFAULT_CLASS)
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
        .class(DEFAULT_CLASS)
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
        self.body().element_classes()
    }

    fn event_handlers(&self) -> Option<&crate::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(self.body())
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.body().layout_style()
    }
}
