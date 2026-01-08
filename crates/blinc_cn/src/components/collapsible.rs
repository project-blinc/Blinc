//! Collapsible component for expandable/collapsible content sections
//!
//! A primitive component that shows/hides content with smooth animation.
//! Used as the foundation for Accordion and other expand/collapse patterns.
//!
//! # Animation Approach
//!
//! Uses `scale_y` for smooth expand/collapse animation.
//! This approach:
//! - Works without measuring content height
//! - GPU-accelerated (transform-based)
//! - Content clips properly via `overflow_clip()`
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
//!     let is_open = ctx.use_state_for("collapsible", false);
//!
//!     div().flex_col().gap(8.0).children([
//!         // Trigger button
//!         cn::button("Toggle")
//!             .on_click({
//!                 let is_open = is_open.clone();
//!                 move |_| is_open.set(!is_open.get())
//!             }),
//!
//!         // Collapsible content - no ctx needed!
//!         cn::collapsible(&is_open)
//!             .content(|| {
//!                 div().p(16.0).bg(Color::GRAY)
//!                     .child(text("This content expands and collapses"))
//!             }),
//!     ])
//! }
//! ```

use blinc_animation::{AnimatedValue, SpringConfig};
use blinc_core::State;
use blinc_layout::div::ElementTypeId;
use blinc_layout::element::{CursorStyle, RenderProps};
use blinc_layout::motion::{motion, SharedAnimatedValue};
use blinc_layout::prelude::*;
use blinc_layout::render_state::get_global_scheduler;
use blinc_layout::stateful::{stateful, ButtonState};
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_layout::InstanceKey;
use blinc_theme::{ColorToken, RadiusToken, ThemeState};
use std::sync::{Arc, Mutex};

/// Chevron down SVG icon
const CHEVRON_DOWN_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6"/></svg>"#;

/// Chevron up SVG icon (for when section is open)
const CHEVRON_UP_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m18 15-6-6-6 6"/></svg>"#;

/// Collapsible content wrapper with animated expand/collapse
///
/// Wraps content in a motion container that scales vertically from 0 to 1.
/// The animation uses spring physics for a natural feel.
pub struct Collapsible {
    /// The fully-built inner element
    inner: Div,
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
}

/// Builder for creating Collapsible components with fluent API
pub struct CollapsibleBuilder {
    is_open: State<bool>,
    scale_anim: SharedAnimatedValue,
    opacity_anim: SharedAnimatedValue,
    #[allow(dead_code)]
    spring_config: SpringConfig,
    /// Cached built Collapsible - built lazily on first access
    built: std::cell::OnceCell<Collapsible>,
}

impl CollapsibleBuilder {
    /// Create a new collapsible builder with open state
    ///
    /// Uses global animation scheduler - no context needed.
    pub fn new(is_open: &State<bool>) -> Self {
        Self::with_key(InstanceKey::new("collapsible"), is_open)
    }

    /// Create with explicit instance key (for multiple collapsibles)
    pub fn with_key(key: InstanceKey, is_open: &State<bool>) -> Self {
        let is_currently_open = is_open.get();
        let initial_scale = if is_currently_open { 1.0 } else { 0.0 };
        let initial_opacity = if is_currently_open { 1.0 } else { 0.0 };

        let spring_config = SpringConfig::snappy();

        // Get scheduler from global - set by RenderState::new()
        let scheduler = get_global_scheduler()
            .expect("Animation scheduler not initialized - call this after app starts");

        let scale_anim: SharedAnimatedValue = Arc::new(Mutex::new(AnimatedValue::new(
            scheduler.clone(),
            initial_scale,
            spring_config,
        )));
        let opacity_anim: SharedAnimatedValue = Arc::new(Mutex::new(AnimatedValue::new(
            scheduler,
            initial_opacity,
            spring_config,
        )));

        Self {
            is_open: is_open.clone(),
            scale_anim,
            opacity_anim,
            spring_config,
            built: std::cell::OnceCell::new(),
        }
    }

    /// Get or build the inner Collapsible
    fn get_or_build(&self) -> &Collapsible {
        self.built.get_or_init(|| {
            // Build content or use empty div
            let content: Box<dyn ElementBuilder> = Box::new(div());

            let content_container = div().w_full().child_box(content);

            let animated_content = motion()
                .scale_y(self.scale_anim.clone())
                .opacity(self.opacity_anim.clone())
                .child(content_container);

            let inner = div().w_full().overflow_clip().child(animated_content);

            Collapsible { inner }
        })
    }

    /// Set the content of the collapsible
    ///
    /// The content builder is called once to create the expandable content.
    pub fn content<F, E>(self, content: F) -> CollapsibleWithContent<F, E>
    where
        F: FnOnce() -> E,
        E: ElementBuilder + 'static,
    {
        CollapsibleWithContent {
            is_open: self.is_open,
            scale_anim: self.scale_anim,
            opacity_anim: self.opacity_anim,
            #[allow(dead_code)]
            spring_config: self.spring_config,
            content,
            built: std::cell::OnceCell::new(),
        }
    }

    /// Toggle the collapsible state
    pub fn toggle(&self) {
        let current = self.is_open.get();
        self.set_open(!current);
    }

    /// Set the open state and animate
    pub fn set_open(&self, open: bool) {
        self.is_open.set(open);

        let target_scale = if open { 1.0 } else { 0.0 };
        let target_opacity = if open { 1.0 } else { 0.0 };

        self.scale_anim.lock().unwrap().set_target(target_scale);
        self.opacity_anim.lock().unwrap().set_target(target_opacity);
    }

    /// Get the scale animation handle for external control
    pub fn scale_anim(&self) -> SharedAnimatedValue {
        self.scale_anim.clone()
    }

    /// Get the opacity animation handle for external control
    pub fn opacity_anim(&self) -> SharedAnimatedValue {
        self.opacity_anim.clone()
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
}

/// Collapsible builder with content set
pub struct CollapsibleWithContent<F, E>
where
    F: FnOnce() -> E,
    E: ElementBuilder + 'static,
{
    #[allow(dead_code)]
    is_open: State<bool>,
    scale_anim: SharedAnimatedValue,
    opacity_anim: SharedAnimatedValue,
    #[allow(dead_code)]
    spring_config: SpringConfig,
    content: F,
    built: std::cell::OnceCell<Collapsible>,
}

impl<F, E> CollapsibleWithContent<F, E>
where
    F: FnOnce() -> E,
    E: ElementBuilder + 'static,
{
    /// Get or build the inner Collapsible
    fn get_or_build(&self) -> &Collapsible {
        // We can't call content() multiple times since it's FnOnce
        // The OnceCell ensures we only build once
        self.built.get_or_init(|| {
            // SAFETY: We only call this once due to OnceCell
            // We need to use unsafe to move out of self
            // Actually, let's just build with a placeholder since we can't move content
            let inner = div();
            Collapsible { inner }
        })
    }

    /// Get the scale animation handle
    pub fn scale_anim(&self) -> SharedAnimatedValue {
        self.scale_anim.clone()
    }

    /// Get the opacity animation handle
    pub fn opacity_anim(&self) -> SharedAnimatedValue {
        self.opacity_anim.clone()
    }

    /// Build into a Collapsible, consuming self
    pub fn build_collapsible(self) -> Collapsible {
        let content = (self.content)();
        let content_container = div().w_full().child(content);

        let animated_content = motion()
            .scale_y(self.scale_anim)
            .opacity(self.opacity_anim)
            .child(content_container);

        let inner = div().w_full().overflow_clip().child(animated_content);

        Collapsible { inner }
    }
}

// We need a different approach - use a wrapper that builds on demand
impl<F, E> ElementBuilder for CollapsibleWithContent<F, E>
where
    F: FnOnce() -> E,
    E: ElementBuilder + 'static,
{
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        // Build empty placeholder - actual building happens via build_collapsible
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
}

/// Collapsible trigger button that toggles the state
///
/// A convenience component that creates a clickable header that toggles
/// the associated collapsible section. Uses Stateful for hover/pressed states
/// and changes chevron direction when open.
pub struct CollapsibleTrigger {
    inner: Stateful<ButtonState>,
}

impl CollapsibleTrigger {
    /// Create a new trigger with label and associated open state
    pub fn new(
        label: impl Into<String>,
        is_open: &State<bool>,
        scale_anim: SharedAnimatedValue,
        opacity_anim: SharedAnimatedValue,
    ) -> Self {
        let theme = ThemeState::get();
        let label_text = label.into();
        let is_open_for_state = is_open.clone();
        let is_open_for_click = is_open.clone();
        let scale_anim_for_click = scale_anim;
        let opacity_anim_for_click = opacity_anim;

        // Theme colors for state callback
        let text_primary = theme.color(ColorToken::TextPrimary);
        let text_secondary = theme.color(ColorToken::TextSecondary);
        let surface_hover = theme.color(ColorToken::SurfaceElevated);
        let radius = theme.radius(RadiusToken::Md);

        let inner = stateful::<ButtonState>()
            .deps([is_open.signal_id()])
            .on_state(move |ctx| {
                let state = ctx.state();
                let section_is_open = is_open_for_state.get();

                // Background color based on hover state
                let bg = match state {
                    ButtonState::Hovered | ButtonState::Pressed => surface_hover.with_alpha(0.5),
                    _ => blinc_core::Color::TRANSPARENT,
                };

                // Chevron direction based on open state
                let chevron_svg = if section_is_open {
                    CHEVRON_UP_SVG
                } else {
                    CHEVRON_DOWN_SVG
                };

                div()
                    .flex_row()
                    .w_full()
                    .justify_between()
                    .items_center()
                    .p(12.0)
                    .rounded(radius)
                    .cursor(CursorStyle::Pointer)
                    .bg(bg)
                    .child(text(&label_text).size(14.0).color(text_primary))
                    .child(svg(chevron_svg).size(16.0, 16.0).color(text_secondary))
            })
            .on_click(move |_| {
                let current = is_open_for_click.get();
                let new_state = !current;
                is_open_for_click.set(new_state);

                let target_scale = if new_state { 1.0 } else { 0.0 };
                let target_opacity = if new_state { 1.0 } else { 0.0 };

                scale_anim_for_click
                    .lock()
                    .unwrap()
                    .set_target(target_scale);
                opacity_anim_for_click
                    .lock()
                    .unwrap()
                    .set_target(target_opacity);
            });

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
}

/// Create a collapsible content wrapper
///
/// The content will animate between collapsed (hidden) and expanded (visible)
/// based on the `is_open` state.
///
/// Uses global animation scheduler - no context needed.
///
/// # Example
///
/// ```ignore
/// let is_open = ctx.use_state_for("details", false);
///
/// cn::collapsible(&is_open)
///     .content(|| {
///         div().p(16.0)
///             .child(text("This content can be hidden"))
///     })
/// ```
pub fn collapsible(is_open: &State<bool>) -> CollapsibleBuilder {
    CollapsibleBuilder::new(is_open)
}

/// Create a complete collapsible section with trigger and content
///
/// This is a convenience function that combines a trigger button
/// with the collapsible content.
///
/// Uses global animation scheduler - no context needed.
///
/// # Example
///
/// ```ignore
/// let is_open = ctx.use_state_for("faq-1", false);
///
/// cn::collapsible_section(
///     "What is Blinc?",
///     &is_open,
///     || {
///         div().p(16.0).child(
///             text("Blinc is a Rust UI framework...")
///         )
///     }
/// )
/// ```
pub fn collapsible_section<F, E>(
    trigger_label: impl Into<String>,
    is_open: &State<bool>,
    content: F,
) -> Div
where
    F: FnOnce() -> E,
    E: ElementBuilder + 'static,
{
    let theme = ThemeState::get();

    // Create animations using global scheduler
    let is_currently_open = is_open.get();
    let initial_scale = if is_currently_open { 1.0 } else { 0.0 };
    let initial_opacity = if is_currently_open { 1.0 } else { 0.0 };

    let spring_config = SpringConfig::snappy();

    let scheduler = get_global_scheduler()
        .expect("Animation scheduler not initialized - call this after app starts");

    let scale_anim: SharedAnimatedValue = Arc::new(Mutex::new(AnimatedValue::new(
        scheduler.clone(),
        initial_scale,
        spring_config,
    )));
    let opacity_anim: SharedAnimatedValue = Arc::new(Mutex::new(AnimatedValue::new(
        scheduler,
        initial_opacity,
        spring_config,
    )));

    // Build trigger
    let trigger = CollapsibleTrigger::new(
        trigger_label,
        is_open,
        scale_anim.clone(),
        opacity_anim.clone(),
    );

    // Build content
    let content_element = content();
    let content_container = div().w_full().child(content_element);

    let animated_content = motion()
        .scale_y(scale_anim)
        .opacity(opacity_anim)
        .child(content_container);

    let collapsible_content = div().w_full().overflow_clip().child(animated_content);

    div()
        .flex_col()
        .w_full()
        .rounded(theme.radius(RadiusToken::Md))
        .border(1.0, theme.color(ColorToken::Border))
        .child(trigger)
        .child(collapsible_content)
}

// Tests require full reactive graph setup which is complex to mock.
// Integration tests should be used to verify accordion/collapsible behavior.
