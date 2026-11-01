//! Accordion component for expandable content sections
//!
//! A set of vertically stacked collapsible sections. Supports single-open
//! (only one section open at a time) or multi-open modes.
//!
//! Each section's trigger and content carry `data-state="open"` or
//! `"closed"`. The `.cn-accordion-content[data-state="closed"]` rule lays a
//! closed section out at no height, and the change of size eases over the
//! theme's normal duration; the sections below move with it. Restyle it with
//! rules of your own.
//!
//! # Example - Single Open
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
//!     cn::accordion()
//!         .item("section-1", "What is Blinc?", || {
//!             div().p(16.0).child(text("Blinc is a Rust UI framework..."))
//!         })
//!         .item("section-2", "How do I use it?", || {
//!             div().p(16.0).child(text("Start by creating a window..."))
//!         })
//!         .item("section-3", "Is it fast?", || {
//!             div().p(16.0).child(text("Yes, very fast!"))
//!         })
//! }
//! ```
//!
//! # Multi-Open Mode
//!
//! ```ignore
//! cn::accordion()
//!     .multi_open()  // Allow multiple sections open at once
//!     .item("a", "First Section", || content_a())
//!     .item("b", "Second Section", || content_b())
//! ```

use blinc_core::context_state::BlincContextState;
use blinc_core::reactive::{ReactiveGraph, Signal, computed};
use blinc_layout::InstanceKey;
use blinc_layout::div::ElementTypeId;
use blinc_layout::element::RenderProps;
use blinc_layout::prelude::*;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_layout::visual_animation::VisualAnimationConfig;
use std::cell::OnceCell;
use std::sync::Arc;

/// Chevron down SVG icon
const CHEVRON_DOWN_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6"/></svg>"#;

/// Accordion mode - single or multi open
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AccordionMode {
    /// Only one section can be open at a time (default)
    #[default]
    Single,
    /// Multiple sections can be open simultaneously
    Multi,
}

/// Accordion component - multiple collapsible sections
pub struct Accordion {
    /// The fully-built inner element
    inner: Div,
}

impl ElementBuilder for Accordion {
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

    fn visual_animation_config(
        &self,
    ) -> Option<blinc_layout::visual_animation::VisualAnimationConfig> {
        self.inner.visual_animation_config()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.inner.element_classes()
    }

    fn element_id(&self) -> Option<&str> {
        self.inner.element_id()
    }
}

/// Content builder function type (cloneable via Arc)
type ContentBuilderFn = Arc<dyn Fn() -> Div + Send + Sync>;

/// Single accordion item - stores data, not built elements
#[derive(Clone)]
struct AccordionItem {
    key: String,
    label: String,
    content: ContentBuilderFn,
    /// Caller-owned open signal. `None` means the accordion keeps its
    /// own, keyed by instance and section so it survives rebuilds.
    state: Option<Signal<bool>>,
}

/// Builder for creating Accordion components with fluent API
pub struct AccordionBuilder {
    instance_key: InstanceKey,
    mode: AccordionMode,
    initial_open: Option<String>,
    /// Item definitions (not yet built)
    items: Vec<AccordionItem>,
    /// User-added CSS classes
    classes: Vec<std::sync::Arc<str>>,
    /// User-set element ID
    user_id: Option<String>,
    /// Cached built accordion
    built: OnceCell<Accordion>,
}

impl AccordionBuilder {
    /// Create a new accordion builder
    ///
    pub fn new() -> Self {
        Self {
            instance_key: InstanceKey::new("accordion"),
            mode: AccordionMode::Single,
            initial_open: None,
            items: Vec::new(),
            classes: Vec::new(),
            user_id: None,
            built: OnceCell::new(),
        }
    }

    /// Get or build the accordion
    fn get_or_build(&self) -> &Accordion {
        ::blinc_layout::build_once::build_once(&self.built, || self.build_component())
    }

    /// Set to multi-open mode (multiple sections can be open at once)
    pub fn multi_open(mut self) -> Self {
        self.mode = AccordionMode::Multi;
        self
    }

    /// Set the initially open section (by key)
    pub fn default_open(mut self, key: impl Into<String>) -> Self {
        self.initial_open = Some(key.into());
        self
    }

    /// Add an accordion item
    ///
    /// # Arguments
    /// * `key` - Unique identifier for this section
    /// * `label` - Text shown in the trigger header
    /// * `content` - Function that builds the content when expanded
    pub fn item<F>(mut self, key: impl Into<String>, label: impl Into<String>, content: F) -> Self
    where
        F: Fn() -> Div + Send + Sync + 'static,
    {
        // Store item data - don't build yet
        self.items.push(AccordionItem {
            key: self.instance_key.derive(&key.into()),
            label: label.into(),
            content: Arc::new(content),
            state: None,
        });
        self
    }

    /// Add a section whose open state the caller owns.
    ///
    /// The accordion reads and writes that signal instead of its own,
    /// so a section folds when the state is written from outside and
    /// the state reflects a click on the header. Use this to drive a
    /// section from a signal; [`item`](Self::item) keeps the state
    /// internal, which is what you want when nothing outside cares.
    ///
    /// [`default_open`](Self::default_open) does not apply to a section
    /// added this way: the signal already carries the initial value.
    pub fn item_with_state<F>(
        mut self,
        key: impl Into<String>,
        label: impl Into<String>,
        state: Signal<bool>,
        content: F,
    ) -> Self
    where
        F: Fn() -> Div + Send + Sync + 'static,
    {
        self.items.push(AccordionItem {
            key: self.instance_key.derive(&key.into()),
            label: label.into(),
            content: Arc::new(content),
            state: Some(state),
        });
        self
    }

    /// Add a CSS class for selector matching
    pub fn class(mut self, name: impl AsRef<str>) -> Self {
        self.classes.push(blinc_core::intern::intern(name.as_ref()));
        self
    }

    /// Set the element ID for CSS selector matching
    pub fn id(mut self, id: &str) -> Self {
        self.user_id = Some(id.to_string());
        self
    }

    /// Build the final Accordion component
    pub fn build_component(&self) -> Accordion {
        // `item()` namespaces each key to this instance, so the caller's
        // `default_open` key has to be namespaced the same way before it
        // can match anything.
        let initial_open_key = self
            .initial_open
            .as_deref()
            .map(|key| self.instance_key.derive(key));

        // A caller-owned signal carries its own initial value; only an
        // internal one is seeded from `default_open`.
        let sections: Vec<(AccordionItem, Signal<bool>)> = self
            .items
            .iter()
            .map(|item| {
                let is_open = item.state.unwrap_or_else(|| {
                    let open_now = initial_open_key.as_ref() == Some(&item.key);
                    BlincContextState::get().use_signal_keyed(
                        &format!("{}_{}_open", self.instance_key.get(), item.key),
                        || open_now,
                    )
                });
                (item.clone(), is_open)
            })
            .collect();
        let all_open: Vec<Signal<bool>> = sections.iter().map(|(_, s)| *s).collect();
        let mode = self.mode;
        let count = sections.len();

        // Built once. Each section's chevron, height and look follow its
        // `data-state` by rule, and the size and position changes ease over
        // the theme's normal duration.
        let mut content = div()
            .class("cn-accordion")
            .flex_col()
            .w_full()
            .flex_shrink()
            .overflow_clip()
            .animate_bounds(
                VisualAnimationConfig::height()
                    .with_key(format!("{}_container", self.instance_key.get()))
                    .clip_to_animated()
                    .theme_timed(),
            );

        for (index, (item, is_open)) in sections.into_iter().enumerate() {
            let open = computed(move |g: &ReactiveGraph| g.get(is_open).unwrap_or(false));
            let closed = computed(move |g: &ReactiveGraph| !g.get(is_open).unwrap_or(false));
            let all_open = all_open.clone();

            let trigger = div()
                .class("cn-accordion-trigger")
                .attr_when("data-state", "open", &open)
                .attr_when("data-state", "closed", &closed)
                .flex_row()
                .w_full()
                // Matches `.cn-accordion-trigger`'s padding, so nothing shifts
                // when the rule applies.
                .py(4.0)
                .px(3.0)
                .justify_between()
                .items_center()
                .child(
                    text(&item.label)
                        .size(14.0)
                        .weight(blinc_layout::div::FontWeight::Medium)
                        .pointer_events_none(),
                )
                .child(
                    div()
                        .class("cn-chevron")
                        .child(svg(CHEVRON_DOWN_SVG).size(16.0, 16.0)),
                )
                .on_click(move |_| {
                    let opening = !is_open.try_get().unwrap_or(false);
                    // In single mode, opening a section closes the others.
                    if mode == AccordionMode::Single && opening {
                        for other in &all_open {
                            if other.id() != is_open.id() {
                                other.set(false);
                            }
                        }
                    }
                    is_open.set(opening);
                });

            // The content is always laid out, so the height animation clips
            // it as it closes instead of it vanishing first. Its padding sits
            // inside, so a closed section has none.
            let section_content = div()
                .class("cn-accordion-content")
                .attr_when("data-state", "open", &open)
                .attr_when("data-state", "closed", &closed)
                .flex_col()
                .w_full()
                .overflow_clip()
                .animate_bounds(
                    VisualAnimationConfig::height()
                        .with_key(format!("accordion-content-{}", item.key))
                        .clip_to_animated()
                        .theme_timed(),
                )
                .child(
                    div()
                        .flex_col()
                        .w_full()
                        .py(2.0)
                        .px(1.0)
                        .child((item.content)()),
                );

            // Sections and separators below an opening one move with it.
            content = content.child(
                div()
                    .flex_col()
                    .w_full()
                    .animate_bounds(
                        VisualAnimationConfig::position()
                            .with_key(format!("accordion-item-{}", item.key))
                            .theme_timed(),
                    )
                    .child(trigger)
                    .child(section_content),
            );
            if index + 1 < count {
                content = content.child(
                    div()
                        .class("cn-accordion-separator")
                        .w_full()
                        .h(1.0)
                        .animate_bounds(
                            VisualAnimationConfig::position()
                                .with_key(format!("accordion-sep-{}", item.key))
                                .theme_timed(),
                        ),
                );
            }
        }

        let mut inner = content;
        for c in &self.classes {
            inner = inner.class(c);
        }
        if let Some(ref id) = self.user_id {
            inner = inner.id(id);
        }

        Accordion { inner }
    }
}

impl Default for AccordionBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ElementBuilder for AccordionBuilder {
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

    fn visual_animation_config(
        &self,
    ) -> Option<blinc_layout::visual_animation::VisualAnimationConfig> {
        self.get_or_build().visual_animation_config()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.get_or_build().element_classes()
    }

    fn element_id(&self) -> Option<&str> {
        self.get_or_build().element_id()
    }
}

/// Create an accordion
///
/// Uses global animation scheduler - no context needed.
///
/// # Example
///
/// ```ignore
/// cn::accordion()
///     .item("faq-1", "What is Blinc?", || {
///         div().p(16.0).child(text("A Rust UI framework"))
///     })
///     .item("faq-2", "Is it fast?", || {
///         div().p(16.0).child(text("Yes!"))
///     })
/// ```
pub fn accordion() -> AccordionBuilder {
    AccordionBuilder::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_accordion_mode_default() {
        assert_eq!(AccordionMode::default(), AccordionMode::Single);
    }
}
