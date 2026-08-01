//! Accordion component for expandable content sections
//!
//! A set of vertically stacked collapsible sections. Supports single-open
//! (only one section open at a time) or multi-open modes.
//!
//! Uses global animation scheduler - no context needed.
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

use blinc_animation::{AnimatedValue, SpringConfig};
use blinc_core::context_state::BlincContextState;
use blinc_core::{SignalId, State, use_state_keyed};
use blinc_layout::div::ElementTypeId;
use blinc_layout::element::{CursorStyle, RenderProps};
// LayoutAnimationConfig is no longer used - using new VisualAnimationConfig system
use blinc_layout::InstanceKey;
use blinc_layout::motion::{SharedAnimatedValue, motion};
use blinc_layout::prelude::*;
use blinc_layout::render_state::get_global_scheduler;
use blinc_layout::stateful::Stateful;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_theme::{ColorToken, RadiusToken, ThemeState};
use std::cell::OnceCell;
use std::sync::{Arc, Mutex};

/// Chevron down SVG icon
const CHEVRON_DOWN_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6"/></svg>"#;

/// Chevron up SVG icon (for when section is open)
const CHEVRON_UP_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m18 15-6-6-6 6"/></svg>"#;

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
    inner: Stateful<()>,
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
    /// Caller-owned open state. `None` means the accordion keeps its
    /// own, keyed by instance and section so it survives rebuilds.
    state: Option<State<bool>>,
}

/// Runtime state for an accordion item (created during build)
#[derive(Clone)]
struct AccordionItemState {
    key: String,
    is_open: State<bool>,
    opacity_anim: SharedAnimatedValue,
}

/// Builder for creating Accordion components with fluent API
pub struct AccordionBuilder {
    instance_key: InstanceKey,
    mode: AccordionMode,
    spring_config: SpringConfig,
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
    /// Uses global animation scheduler - no context needed.
    pub fn new() -> Self {
        Self {
            instance_key: InstanceKey::new("accordion"),
            mode: AccordionMode::Single,
            spring_config: SpringConfig::snappy(),
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

    /// Set custom spring configuration for animations
    pub fn spring(mut self, config: SpringConfig) -> Self {
        self.spring_config = config;
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
    /// The accordion reads and writes that `State` instead of its own,
    /// so a section folds when the state is written from outside and
    /// the state reflects a click on the header. Use this to drive a
    /// section from a signal; [`item`](Self::item) keeps the state
    /// internal, which is what you want when nothing outside cares.
    ///
    /// [`default_open`](Self::default_open) does not apply to a section
    /// added this way: the state already carries the initial value.
    pub fn item_with_state<F>(
        mut self,
        key: impl Into<String>,
        label: impl Into<String>,
        state: State<bool>,
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
        let theme = ThemeState::get();

        // Get scheduler from global
        let scheduler = get_global_scheduler()
            .expect("Animation scheduler not initialized - call this after app starts");

        // Collect all signal IDs for the container's deps
        let mut all_signal_ids: Vec<SignalId> = Vec::new();

        // Build combined item data with runtime state - no mutex needed, just Vec
        let mut items_with_state: Vec<(AccordionItem, AccordionItemState)> = Vec::new();

        // `item()` namespaces each key to this instance, so the caller's
        // `default_open` key has to be namespaced the same way before it
        // can match anything.
        let initial_open_key = self
            .initial_open
            .as_deref()
            .map(|key| self.instance_key.derive(key));

        for item in &self.items {
            let is_initially_open = initial_open_key.as_ref() == Some(&item.key);

            // A caller-owned state carries its own initial value; only
            // an internal one needs seeding from `default_open`.
            let is_open: State<bool> = match &item.state {
                Some(state) => state.clone(),
                None => {
                    let state_key = format!("{}_{}_open", self.instance_key.get(), item.key);
                    BlincContextState::get().use_state_keyed(&state_key, || is_initially_open)
                }
            };

            // Collect signal ID for container deps
            all_signal_ids.push(is_open.signal_id());

            // Get actual current state (may differ from initial if persisted)
            let actual_is_open = is_open.get();
            let actual_opacity = if actual_is_open { 1.0 } else { 0.0 };

            // Create opacity animation
            let opacity_anim: SharedAnimatedValue = Arc::new(Mutex::new(AnimatedValue::new(
                scheduler.clone(),
                actual_opacity,
                self.spring_config,
            )));

            let item_state = AccordionItemState {
                key: item.key.clone(),
                is_open,
                opacity_anim,
            };

            items_with_state.push((item.clone(), item_state));
        }

        // Theme colors
        let text_primary = theme.color(ColorToken::TextPrimary);
        let text_secondary = theme.color(ColorToken::TextSecondary);
        let border_color = theme.color(ColorToken::Border);
        let radius = theme.radius(RadiusToken::Lg);

        let key_for_container = format!("{}_container", self.instance_key.get());
        let container_state_handle = blinc_layout::stateful::use_fsm_keyed(&key_for_container, ());

        // Clone for use inside the closure
        let anim_key_for_container = key_for_container.clone();

        let item_count = items_with_state.len();
        let mode = self.mode;

        // Clone all states for single-mode closing (needed in on_click)
        let all_item_states: Vec<AccordionItemState> =
            items_with_state.iter().map(|(_, s)| s.clone()).collect();

        // Build the entire accordion as a single Stateful that reacts to ALL item open states
        let accordion_stateful = Stateful::with_shared_state(container_state_handle)
            .deps(&all_signal_ids)
            .on_state(move |_state: &(), container: &mut Div| {
                // Outer container animates height so border follows children expansion
                let mut content = div()
                    .class("cn-accordion")
                    .flex_col()
                    .w_full()
                    .flex_shrink()
                    .rounded(radius)
                    .shadow_md()
                    .bg(theme.color(ColorToken::SurfaceElevated))
                    .border(1.5, border_color)
                    .overflow_clip()
                    .animate_bounds(
                        blinc_layout::visual_animation::VisualAnimationConfig::height()
                            .with_key(&anim_key_for_container)
                            .clip_to_animated()
                            .snappy(),
                    );

                for (index, (item, item_state)) in items_with_state.iter().enumerate() {
                    let is_open = item_state.is_open.clone();
                    let opacity_anim = item_state.opacity_anim.clone();
                    let item_key = item_state.key.clone();

                    let content_fn = item.content.clone();
                    let label = item.label.clone();

                    // Clones for on_click closure
                    let is_open_for_click = is_open.clone();
                    let opacity_anim_for_click = opacity_anim.clone();
                    let all_states_for_click = all_item_states.clone();
                    let key_for_click = item_key.clone();

                    let section_is_open = is_open.get();

                    // Build trigger - stateless div since container rebuilds on state change
                    let chevron_svg = if section_is_open {
                        CHEVRON_UP_SVG
                    } else {
                        CHEVRON_DOWN_SVG
                    };

                    let mut trigger = div()
                        .class("cn-accordion-trigger")
                        .flex_row()
                        .w_full()
                        // Builder padding fallback MUST match the
                        // `.cn-accordion-trigger` CSS (space-4 / space-3 =
                        // 16px / 12px). Every other cn menu primitive sets
                        // this fallback (see cn_styles.rs's note); the
                        // accordion trigger omitted it, so on a rebuild that
                        // doesn't re-apply the class layout — and on the
                        // initial default-open render — the row collapsed to
                        // zero padding + height. The value equals the CSS so
                        // there is never a shift when the class does apply.
                        .py(4.0)
                        .px(3.0)
                        .justify_between()
                        .items_center()
                        .cursor(CursorStyle::Pointer)
                        .child(
                            text(&label)
                                .size(14.0)
                                .weight(blinc_layout::div::FontWeight::Medium)
                                .color(text_primary)
                                .pointer_events_none(),
                        )
                        .child(svg(chevron_svg).size(16.0, 16.0).color(text_secondary))
                        .on_click(move |_| {
                            let current = is_open_for_click.get();
                            let new_state = !current;

                            // In single mode, close all other sections first
                            if mode == AccordionMode::Single && new_state {
                                for state in &all_states_for_click {
                                    if state.key != key_for_click && state.is_open.get() {
                                        state.is_open.set(false);
                                        state.opacity_anim.lock().unwrap().set_target(0.0);
                                    }
                                }
                            }

                            // Toggle this section
                            is_open_for_click.set(new_state);

                            let target_opacity = if new_state { 1.0 } else { 0.0 };
                            opacity_anim_for_click
                                .lock()
                                .unwrap()
                                .set_target(target_opacity);
                        });

                    // Note: No position animation on trigger - it doesn't move relative to item_div.
                    // The item_div itself animates position within the container.

                    // Structure: item_wrapper contains trigger (always visible) + collapsible content
                    // Only the collapsible content area animates, keeping trigger always visible
                    let anim_key = format!("accordion-content-{}", item_key);

                    // Build the collapsible content area with FLIP-style visual animation
                    // ALWAYS render content - during collapse, the content is visible while
                    // the animated height shrinks. overflow_clip() hides overflow during animation.
                    //
                    // Using animate_bounds() (new system) instead of animate_layout() (old system):
                    // - Layout runs once with final positions (taffy is read-only)
                    // - Animation tracks visual offsets from layout position
                    // - Parent offsets propagate to children hierarchically
                    // Content animates HEIGHT only - position is handled by parent item_div
                    // clip_to_animated ensures content clips to shrinking bounds during collapse
                    //
                    // IMPORTANT: Content must ALWAYS be rendered for collapse animation to work.
                    // If content is conditionally removed, there's nothing to show during collapse.
                    // The content is always present but clipped by overflow_clip and animate_bounds.
                    // Padding is only applied when open to avoid gaps when collapsed.
                    let collapsible_content = div()
                        .class("cn-accordion-content")
                        .flex_col()
                        .w_full()
                        .bg(theme.color(ColorToken::Surface))
                        .border_top(1.0, border_color)
                        .overflow_clip()
                        .animate_bounds(
                            blinc_layout::visual_animation::VisualAnimationConfig::height()
                                .with_key(&anim_key)
                                .clip_to_animated()
                                .snappy(),
                        )
                        // Always render content so collapse animation has something to clip
                        .child(content_fn())
                        // Only add padding when open (padding adds to size even with h(0))
                        .when(section_is_open, |d| d.py(2.0).px(1.0))
                        // Set explicit height to 0 when collapsed - content is clipped
                        .when(!section_is_open, |d| d.w_full().h(0.0).px(1.0));

                    // Item wrapper: trigger (always visible) + collapsible content
                    // Both item_div and separators animate position for fluid transitions
                    let item_div_key = format!("accordion-item-{}", item_key);
                    let item_div = div()
                        .flex_col()
                        .w_full()
                        .animate_bounds(
                            blinc_layout::visual_animation::VisualAnimationConfig::position()
                                .with_key(&item_div_key)
                                .snappy(),
                        )
                        .child(trigger)
                        .child(collapsible_content);

                    content = content.child(item_div);

                    // Add separator between items (not after last)
                    // Separator also animates position for fluid transitions
                    if index < item_count - 1 {
                        let separator_key = format!("accordion-sep-{}", item_key);
                        content = content.child(
                            div().w_full().h(1.0).bg(border_color).animate_bounds(
                                blinc_layout::visual_animation::VisualAnimationConfig::position()
                                    .with_key(&separator_key)
                                    .snappy(),
                            ),
                        );
                    }
                }

                container.merge(content);
            });

        let mut inner = accordion_stateful;
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
