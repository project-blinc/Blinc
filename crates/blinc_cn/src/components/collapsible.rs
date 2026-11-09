//! Collapsible component for expandable/collapsible content sections
//!
//! A primitive that shows and hides content, the foundation for the accordion
//! and other expand/collapse patterns.
//!
//! The content is built the first time it opens, and kept: closing does not
//! tear it down, so it keeps its state and eases shut. It carries
//! `data-state="open"` or `"closed"`, and the
//! `.cn-collapsible-content[data-state="closed"]` rule lays it out at no
//! height; the change of size eases over the theme's normal duration.
//! Restyle it with rules of your own.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
//!     let is_open = ctx.use_signal(false);
//!
//!     div().flex_col().gap(8.0).children([
//!         cn::button("Toggle").on_click(move |_| is_open.set(!is_open.get())),
//!         cn::collapsible(is_open).content(|| {
//!             div().p(16.0).child(text("This content expands and collapses"))
//!         }),
//!     ])
//! }
//! ```

use blinc_core::reactive::{Computed, ReactiveGraph, Signal, computed};
use blinc_layout::div::ElementTypeId;
use blinc_layout::element::RenderProps;
use blinc_layout::prelude::*;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_layout::visual_animation::VisualAnimationConfig;
use blinc_theme::ThemeState;

/// Chevron down SVG icon
const CHEVRON_DOWN_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6"/></svg>"#;

/// Collapsible content wrapper with animated expand/collapse
pub struct Collapsible {
    /// The fully-built inner element
    inner: Div,
}

impl Collapsible {
    /// Add a CSS class for selector matching
    pub fn class(mut self, name: impl AsRef<str>) -> Self {
        self.inner = self.inner.class(name);
        self
    }

    /// Set the element ID for CSS selector matching
    pub fn id(mut self, id: &str) -> Self {
        self.inner = self.inner.id(id);
        self
    }
}

impl ElementBuilder for Collapsible {
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

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.inner.layout_style()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.inner.element_classes()
    }
}

/// Content builder, cloneable and re-callable.
type ContentBuilderFn = std::sync::Arc<dyn Fn() -> Div + Send + Sync>;

/// `data-state` as the open and closed pair of attributes.
fn open_and_closed(is_open: Signal<bool>) -> (Computed<bool>, Computed<bool>) {
    (
        computed(move |g: &ReactiveGraph| g.get(is_open).unwrap_or(false)),
        computed(move |g: &ReactiveGraph| !g.get(is_open).unwrap_or(false)),
    )
}

/// True from the first time `is_open` holds. Content shown on it is built when its section
/// first opens and kept after, for the close animation and the content's own state.
pub(crate) fn opened_once(is_open: Signal<bool>) -> Computed<bool> {
    let opened = std::sync::atomic::AtomicBool::new(false);
    computed(move |g: &ReactiveGraph| {
        if g.get(is_open).unwrap_or(false) {
            opened.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        opened.load(std::sync::atomic::Ordering::Relaxed)
    })
}

/// The folding part: the content, built when it first opens, at no height while closed by
/// rule, its size change eased by a token-timed layout animation.
fn fold_body<E: ElementBuilder + 'static>(
    content: impl Fn() -> E + 'static,
    is_open: Signal<bool>,
) -> Div {
    let (open, closed) = open_and_closed(is_open);
    div()
        .class("cn-collapsible-content")
        .attr_when("data-state", "open", &open)
        .attr_when("data-state", "closed", &closed)
        .w_full()
        .flex_col()
        .overflow_clip()
        .animate_bounds(
            VisualAnimationConfig::height()
                .with_key(format!("cn-collapsible-{}", is_open.id().to_raw()))
                .clip_to_animated()
                .theme_timed(),
        )
        .show(opened_once(is_open), content)
}

/// Builder for a collapsible bound to an open signal.
pub struct CollapsibleBuilder {
    is_open: Signal<bool>,
    content: Option<ContentBuilderFn>,
    /// Cached built Collapsible - built lazily on first access
    built: std::cell::OnceCell<Collapsible>,
}

impl CollapsibleBuilder {
    /// A collapsible that opens and closes with `is_open`.
    pub fn new(is_open: Signal<bool>) -> Self {
        Self {
            is_open,
            content: None,
            built: std::cell::OnceCell::new(),
        }
    }

    /// Get or build the inner Collapsible
    fn get_or_build(&self) -> &Collapsible {
        ::blinc_layout::build_once::build_once(&self.built, || {
            let content = self.content.clone();
            let body = move || content.as_ref().map_or_else(div, |f| f());
            Collapsible {
                inner: div()
                    .class("cn-collapsible")
                    .w_full()
                    .flex_col()
                    .child(fold_body(body, self.is_open)),
            }
        })
    }

    /// Set the content, built the first time the section opens.
    pub fn content<F>(mut self, content: F) -> Self
    where
        F: Fn() -> Div + Send + Sync + 'static,
    {
        self.content = Some(std::sync::Arc::new(content));
        self
    }

    /// Toggle the collapsible state
    pub fn toggle(&self) {
        let current = self.is_open.try_get().unwrap_or(false);
        self.is_open.set(!current);
    }

    /// Set the open state
    pub fn set_open(&self, open: bool) {
        self.is_open.set(open);
    }
}

impl ElementBuilder for CollapsibleBuilder {
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

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.get_or_build().layout_style()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.get_or_build().element_classes()
    }
}

/// A header that opens and closes a collapsible. It carries the same
/// `data-state` as the content, so its chevron turns and its fill follow
/// rules.
pub struct CollapsibleTrigger {
    inner: Div,
}

impl CollapsibleTrigger {
    /// A trigger labelled `label` that toggles `is_open`.
    pub fn new(label: impl Into<String>, is_open: Signal<bool>) -> Self {
        let (open, closed) = open_and_closed(is_open);
        let inner = div()
            .class("cn-collapsible-trigger")
            .class("tabindex=0")
            .attr_when("data-state", "open", &open)
            .attr_when("data-state", "closed", &closed)
            .flex_row()
            .w_full()
            .justify_between()
            .items_center()
            .p(3.0)
            .child(text(label.into()).size(ThemeState::get().typography().text_sm))
            .child(
                div()
                    .class("cn-chevron")
                    .child(svg(CHEVRON_DOWN_SVG).size(16.0, 16.0)),
            )
            .on_click(move |_| is_open.set(!is_open.try_get().unwrap_or(false)));
        Self { inner }
    }
}

impl ElementBuilder for CollapsibleTrigger {
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

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.inner.layout_style()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.inner.element_classes()
    }

    fn element_id(&self) -> Option<&str> {
        ElementBuilder::element_id(&self.inner)
    }

    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(&self.inner)
    }
}

/// Create a collapsible content wrapper that opens and closes with
/// `is_open`.
///
/// # Example
///
/// ```ignore
/// let is_open = ctx.use_signal(false);
///
/// cn::collapsible(is_open)
///     .content(|| div().p(16.0).child(text("This content can be hidden")))
/// ```
pub fn collapsible(is_open: Signal<bool>) -> CollapsibleBuilder {
    CollapsibleBuilder::new(is_open)
}

/// A bordered section: a trigger labelled `trigger_label` over content that
/// opens and closes with `is_open`.
///
/// # Example
///
/// ```ignore
/// let is_open = ctx.use_signal(false);
///
/// cn::collapsible_section("What is Blinc?", is_open, || {
///     div().p(16.0).child(text("Blinc is a Rust UI framework..."))
/// })
/// ```
pub fn collapsible_section<F, E>(
    trigger_label: impl Into<String>,
    is_open: Signal<bool>,
    content: F,
) -> Div
where
    F: Fn() -> E + 'static,
    E: ElementBuilder + 'static,
{
    div()
        .class("cn-collapsible-section")
        .flex_col()
        .w_full()
        .overflow_clip()
        .child(CollapsibleTrigger::new(trigger_label, is_open))
        .child(fold_body(content, is_open))
}
