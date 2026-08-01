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
use blinc_layout::InstanceKey;
use blinc_layout::div::ElementTypeId;
use blinc_layout::element::{CursorStyle, RenderProps};
use blinc_layout::motion::{SharedAnimatedValue, motion};
use blinc_layout::prelude::*;
use blinc_layout::render_state::get_global_scheduler;
use blinc_layout::stateful::{ButtonState, stateful};
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
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

/// Builder for creating Collapsible components with fluent API
/// Content builder, cloneable and re-callable — the same shape
/// `cn::accordion` uses. A `Stateful` rebuilds its subtree, so content
/// has to be a recipe rather than an already-built element.
type ContentBuilderFn = std::sync::Arc<dyn Fn() -> Div + Send + Sync>;

pub struct CollapsibleBuilder {
    is_open: State<bool>,
    content: Option<ContentBuilderFn>,
    scale_anim: SharedAnimatedValue,
    opacity_anim: SharedAnimatedValue,
    #[allow(dead_code)]
    spring_config: SpringConfig,
    /// Cached built Collapsible - built lazily on first access
    built: std::cell::OnceCell<Collapsible>,
}

/// The folding body: content under a height animation, clipped.
///
/// Shared by both entry points so they cannot drift. Content is ALWAYS
/// rendered — the collapse animates down FROM the open bounds, so
/// removing it leaves nothing to measure or shrink — and the collapsed
/// branch re-asserts width while adding no vertical padding, which
/// would keep the element occupying space even at `h(0)`.
fn fold_body(content: impl ElementBuilder + 'static, anim_key: &str, open: bool) -> Div {
    div()
        .w_full()
        .flex_col()
        .overflow_clip()
        .animate_bounds(
            blinc_layout::visual_animation::VisualAnimationConfig::height()
                .with_key(anim_key)
                .clip_to_animated()
                .snappy(),
        )
        .child(content)
        .when(!open, |d| d.w_full().h(0.0))
}

impl CollapsibleBuilder {
    /// Create a new collapsible builder with open state
    ///
    /// Uses global animation scheduler - no context needed.
    /// `#[track_caller]` so `InstanceKey` is minted against the CALLER,
    /// not this line. Without it every collapsible in a program shares
    /// one key, since `InstanceKey::new` would see this function body
    /// as the location.
    #[track_caller]
    pub fn new(is_open: &State<bool>) -> Self {
        Self::with_key(InstanceKey::new("collapsible"), is_open)
    }

    /// Create with explicit instance key (for multiple collapsibles)
    pub fn with_key(key: InstanceKey, is_open: &State<bool>) -> Self {
        let is_currently_open = is_open.get();
        let initial_scale = if is_currently_open { 1.0 } else { 0.0 };
        let initial_opacity = if is_currently_open { 1.0 } else { 0.0 };

        let spring_config = SpringConfig::snappy();

        // Persisted, keyed on the bound signal. Fresh springs on every
        // build would leave the effect registered against the FIRST
        // build's, so a rebuilt section would sit at whatever the new
        // springs started at and never move — visible as content that
        // needs a second interaction to appear.
        //
        // `initial_*` applies on first build only; afterwards the live
        // value is returned wherever it is.
        let anim_key = is_open.signal_id().to_raw();
        let scale_anim = blinc_layout::stateful::persisted_animated_value(
            &format!("cn-collapsible:{anim_key}:scale"),
            initial_scale,
            spring_config,
        );
        let opacity_anim = blinc_layout::stateful::persisted_animated_value(
            &format!("cn-collapsible:{anim_key}:opacity"),
            initial_opacity,
            spring_config,
        );

        // Re-assert the target on every build, the same as `cn::switch`.
        // The persisted springs keep their value across a rebuild, and
        // `initial_*` above applies only on first mint, so without this
        // a section rebuilt while folded holds whatever the previous
        // build left and never moves to where the state now says it
        // belongs. A no-op when the spring already sits there, so a
        // plain mount still does not animate.
        {
            let target = if is_currently_open { 1.0 } else { 0.0 };
            scale_anim.lock().unwrap().set_target(target);
            opacity_anim.lock().unwrap().set_target(target);
        }

        crate::reactive_props::bind_bool_targets(
            crate::reactive_props::bool_binding::COLLAPSIBLE,
            is_open,
            vec![
                crate::reactive_props::BoolTarget::to(&scale_anim, 1.0),
                crate::reactive_props::BoolTarget::to(&opacity_anim, 1.0),
            ],
        );

        Self {
            is_open: is_open.clone(),
            content: None,
            scale_anim,
            opacity_anim,
            spring_config,
            built: std::cell::OnceCell::new(),
        }
    }

    /// Get or build the inner Collapsible
    fn get_or_build(&self) -> &Collapsible {
        // Built OUTSIDE the cell rather than in `get_or_init`.
        // `Stateful` runs its callback during construction, and that
        // path can reach back here — inside `get_or_init` that is a
        // "reentrant init" panic, whereas here the inner call simply
        // builds its own and loses the `set` race harmlessly.
        if let Some(built) = self.built.get() {
            return built;
        }
        let built = self.make();
        let _ = self.built.set(built);
        return self.built.get().expect("just set");
    }

    fn make(&self) -> Collapsible {
        {
            let anim_key = format!("cn-collapsible-{}", self.is_open.signal_id().to_raw());
            let is_open = self.is_open.clone();
            let content = self.content.clone();

            // Wrapped in a `Stateful` bound to the state. Open and shut
            // differ by an explicit zero height, and height is decided
            // when the element is BUILT, so without a rebuild the
            // section keeps whatever it was first built with and only
            // moves when something unrelated rebuilds it.
            let inner =
                blinc_layout::stateful::stateful_with_key::<()>(&format!("{anim_key}-container"))
                    .deps([self.is_open.signal_id()])
                    .on_state(move |_ctx| {
                        // Content is ALWAYS rendered: the collapse animates
                        // down FROM the open bounds, so removing it would
                        // leave the animation nothing to measure or shrink.
                        let body = match &content {
                            Some(f) => f(),
                            None => div(),
                        };
                        fold_body(body, &anim_key, is_open.get())
                    });

            Collapsible {
                inner: div().w_full().child(inner),
            }
        }
    }

    /// Set the content, as a builder called on every rebuild.
    pub fn content<F>(mut self, content: F) -> Self
    where
        F: Fn() -> Div + Send + Sync + 'static,
    {
        self.content = Some(std::sync::Arc::new(content));
        self
    }

    /// Set the content of the collapsible

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

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.get_or_build().element_classes()
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
        ::blinc_layout::build_once::build_once(&self.built, || {
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
        let is_open = self.is_open.get();

        // Height animation rather than `scale_y`, matching
        // `cn::accordion`. Scaling squashes the content and fades it as
        // a block; animating the bound reveals it, and the surrounding
        // layout follows because taffy sees the final size while the
        // animation tracks the visual offset from it.
        //
        // Content is ALWAYS rendered: a collapse animates from the open
        // bounds, so removing it would leave nothing to shrink. It is
        // `overflow_clip` plus an explicit zero height that hide it when
        // shut.
        let anim_key = format!("cn-collapsible-{}", self.is_open.signal_id().to_raw());
        let inner = div()
            .w_full()
            .flex_col()
            .overflow_clip()
            .animate_bounds(
                blinc_layout::visual_animation::VisualAnimationConfig::height()
                    .with_key(&anim_key)
                    .clip_to_animated()
                    .snappy(),
            )
            .child(content)
            .when(!is_open, |d| d.h(0.0));

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

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.get_or_build().element_classes()
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
                    .class("cn-collapsible-trigger")
                    .flex_row()
                    .w_full()
                    .justify_between()
                    .items_center()
                    .p(12.0)
                    .rounded(radius)
                    .cursor(CursorStyle::Pointer)
                    .bg(bg)
                    .child(
                        text(&label_text)
                            .size(theme.typography().text_sm)
                            .color(text_primary),
                    )
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

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.inner.element_classes()
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
#[track_caller]
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
