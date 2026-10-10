//! Tabs component for tabbed navigation
//!
//! A themed tabbed interface component using state-driven reactivity.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
//!     let active_tab = ctx.use_state_keyed("active_tab", || "account".to_string());
//!
//!     // Simple text labels
//!     cn::tabs(&active_tab)
//!         .tab("account", "Account", || {
//!             div().child(text("Account settings here"))
//!         })
//!         .tab("password", "Password", || {
//!             div().child(text("Password settings here"))
//!         })
//!         .tab("notifications", "Notifications", || {
//!             div().child(text("Notification preferences here"))
//!         })
//! }
//!
//! // Using TabMenuItem for custom tab triggers with icons
//! cn::tabs(&active_tab)
//!     .tab_item(
//!         cn::tab_item("account")
//!             .icon(account_icon_svg)
//!             .label("Account"),
//!         || div().child(text("Account settings"))
//!     )
//!     .tab_item(
//!         cn::tab_item("settings")
//!             .icon(settings_icon_svg)
//!             .label("Settings")
//!             .badge("3"),  // Show notification badge
//!         || div().child(text("Settings panel"))
//!     )
//!
//! // Disabled tabs
//! cn::tabs(&active_tab)
//!     .tab_item(
//!         cn::tab_item("active").label("Active Tab"),
//!         || div()
//!     )
//!     .tab_item(
//!         cn::tab_item("disabled").label("Disabled").disabled(),
//!         || div()
//!     )
//!
//! // With size variant
//! cn::tabs(&active_tab)
//!     .size(TabsSize::Large)
//!     .tab("tab1", "Tab 1", || div())
//!
//! // With default tab
//! cn::tabs(&active_tab)
//!     .default_value("password")
//!     .tab("account", "Account", || div())
//!     .tab("password", "Password", || div())
//! ```

use std::cell::OnceCell;
use std::sync::Arc;

use blinc_animation::{AnimationPreset, MultiKeyframeAnimation};
use blinc_core::reactive::{ReactiveGraph, computed, signal};
use blinc_core::{Color, State};
use blinc_layout::div::ElementTypeId;
// For query_motion to trigger suspended animations
use blinc_layout::element::{CursorStyle, RenderProps};
use blinc_layout::motion::motion_derived;
use blinc_layout::prelude::*;
use blinc_layout::region::Row;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_theme::{ColorScheme, ColorToken, RadiusToken, ThemeState};

use blinc_layout::selector::query_motion;
use blinc_layout::{InstanceKey, Interaction};

/// Tabs size variants
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TabsSize {
    /// Small tabs (height: 32px, text: 13px)
    Small,
    /// Medium tabs (height: 40px, text: 14px)
    #[default]
    Medium,
    /// Large tabs (height: 48px, text: 16px)
    Large,
}

impl TabsSize {
    /// Get the height for the tab list
    fn height(&self) -> f32 {
        match self {
            TabsSize::Small => 32.0,
            TabsSize::Medium => 40.0,
            TabsSize::Large => 48.0,
        }
    }

    /// Get the font size
    fn font_size(&self) -> f32 {
        match self {
            TabsSize::Small => 13.0,
            TabsSize::Medium => 14.0,
            TabsSize::Large => 16.0,
        }
    }

    /// Get the horizontal padding
    fn padding_x(&self) -> f32 {
        match self {
            TabsSize::Small => 12.0,
            TabsSize::Medium => 16.0,
            TabsSize::Large => 20.0,
        }
    }

    /// Get icon size
    fn icon_size(&self) -> f32 {
        match self {
            TabsSize::Small => 14.0,
            TabsSize::Medium => 16.0,
            TabsSize::Large => 18.0,
        }
    }

    /// Get badge font size
    fn badge_font_size(&self) -> f32 {
        match self {
            TabsSize::Small => 10.0,
            TabsSize::Medium => 11.0,
            TabsSize::Large => 12.0,
        }
    }
}

/// Builder for customizing individual tab triggers
///
/// Allows setting icons, badges, custom content, and disabled state for tabs.
#[derive(Clone)]
pub struct TabMenuItem {
    /// The value (stored in state when selected)
    value: String,
    /// Optional text label
    label: Option<String>,
    /// Optional icon SVG string
    icon: Option<String>,
    /// Optional badge text (e.g., notification count)
    badge: Option<String>,
    /// Whether this tab is disabled
    disabled: bool,
    /// Custom content builder (overrides label/icon if set)
    custom_content: Option<Arc<dyn Fn(bool) -> Div + Send + Sync>>,
}

impl std::fmt::Debug for TabMenuItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TabMenuItem")
            .field("value", &self.value)
            .field("label", &self.label)
            .field("icon", &self.icon.is_some())
            .field("badge", &self.badge)
            .field("disabled", &self.disabled)
            .field("custom_content", &self.custom_content.is_some())
            .finish()
    }
}

impl TabMenuItem {
    /// Create a new tab menu item with a value
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: None,
            icon: None,
            badge: None,
            disabled: false,
            custom_content: None,
        }
    }

    /// Set the text label
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Set an icon (SVG string)
    pub fn icon(mut self, svg: impl Into<String>) -> Self {
        self.icon = Some(svg.into());
        self
    }

    /// Set a badge (e.g., notification count)
    pub fn badge(mut self, badge: impl Into<String>) -> Self {
        self.badge = Some(badge.into());
        self
    }

    /// Mark this tab as disabled
    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }

    /// Set custom content builder
    ///
    /// The callback receives a boolean indicating if the tab is active.
    /// This overrides the default label/icon rendering.
    pub fn content<F>(mut self, builder: F) -> Self
    where
        F: Fn(bool) -> Div + Send + Sync + 'static,
    {
        self.custom_content = Some(Arc::new(builder));
        self
    }

    /// Get the value
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Check if disabled
    pub fn is_disabled(&self) -> bool {
        self.disabled
    }
}

/// Create a new tab menu item builder
///
/// # Example
///
/// ```ignore
/// cn::tab_item("settings")
///     .icon(settings_icon)
///     .label("Settings")
///     .badge("2")
/// ```
pub fn tab_item(value: impl Into<String>) -> TabMenuItem {
    TabMenuItem::new(value)
}

/// Content builder for tab panels
pub type TabContentFn = Arc<dyn Fn() -> Div + Send + Sync>;

/// A single tab item (internal representation)
#[derive(Clone)]
struct TabItem {
    /// The tab menu item configuration
    menu_item: TabMenuItem,
    /// Content builder for the tab panel
    content: TabContentFn,
}

impl std::fmt::Debug for TabItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TabItem")
            .field("menu_item", &self.menu_item)
            .finish()
    }
}

/// Content transition preset for tab switching
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TabsTransition {
    /// No animation
    None,
    /// Fade in/out (default)
    #[default]
    Fade,
    /// Slide from left
    SlideLeft,
    /// Slide from right
    SlideRight,
    /// Slide up
    SlideUp,
    /// Slide down
    SlideDown,
}

impl TabsTransition {
    /// Get enter animation for this transition
    fn enter_animation(&self) -> Option<MultiKeyframeAnimation> {
        match self {
            TabsTransition::None => None,
            TabsTransition::Fade => Some(AnimationPreset::fade_in(250)),
            TabsTransition::SlideLeft => Some(AnimationPreset::slide_in_left(250, 20.0)),
            TabsTransition::SlideRight => Some(AnimationPreset::slide_in_right(250, 50.0)),
            TabsTransition::SlideUp => Some(AnimationPreset::slide_in_top(250, 20.0)),
            TabsTransition::SlideDown => Some(AnimationPreset::slide_in_bottom(250, 20.0)),
        }
    }

    /// Get exit animation for this transition
    fn exit_animation(&self) -> Option<MultiKeyframeAnimation> {
        match self {
            TabsTransition::None => None,
            TabsTransition::Fade => Some(AnimationPreset::fade_out(200)),
            TabsTransition::SlideLeft => Some(AnimationPreset::slide_out_left(200, 20.0)),
            TabsTransition::SlideRight => Some(AnimationPreset::slide_out_right(200, 25.0)),
            TabsTransition::SlideUp => Some(AnimationPreset::slide_out_top(200, 20.0)),
            TabsTransition::SlideDown => Some(AnimationPreset::slide_out_bottom(200, 20.0)),
        }
    }
}

/// Configuration for the tabs component
#[derive(Clone)]
#[allow(clippy::type_complexity)]
struct TabsConfig {
    state: State<String>,
    tabs: Vec<TabItem>,
    size: TabsSize,
    default_value: Option<String>,
    on_change: Option<Arc<dyn Fn(&str) + Send + Sync>>,
    transition: TabsTransition,
}

impl std::fmt::Debug for TabsConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TabsConfig")
            .field("tabs", &self.tabs)
            .field("size", &self.size)
            .field("default_value", &self.default_value)
            .finish()
    }
}

/// The built tabs component
pub struct Tabs {
    inner: Div,
}

impl std::fmt::Debug for Tabs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tabs").finish()
    }
}

/// Builder for tabs component
pub struct TabsBuilder {
    key: InstanceKey,
    config: TabsConfig,
    /// User-added CSS classes
    classes: Vec<std::sync::Arc<str>>,
    /// User-set element ID
    user_id: Option<String>,
    built: OnceCell<Tabs>,
}

impl std::fmt::Debug for TabsBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TabsBuilder")
            .field("config", &self.config)
            .finish()
    }
}

impl TabsBuilder {
    /// Create a new tabs builder with state
    #[track_caller]
    pub fn new(state: &State<String>) -> Self {
        Self {
            key: InstanceKey::new("tabs"),
            config: TabsConfig {
                state: state.clone(),
                tabs: Vec::new(),
                size: TabsSize::default(),
                default_value: None,
                on_change: None,
                transition: TabsTransition::default(),
            },
            classes: Vec::new(),
            user_id: None,
            built: OnceCell::new(),
        }
    }

    /// Create a tabs builder with an explicit key
    pub fn with_key(key: impl Into<String>, state: &State<String>) -> Self {
        Self {
            key: InstanceKey::explicit(key),
            config: TabsConfig {
                state: state.clone(),
                tabs: Vec::new(),
                size: TabsSize::default(),
                default_value: None,
                on_change: None,
                transition: TabsTransition::default(),
            },
            classes: Vec::new(),
            user_id: None,
            built: OnceCell::new(),
        }
    }

    /// Add a tab with value, label, and content (simple API)
    pub fn tab<F>(mut self, value: impl Into<String>, label: impl Into<String>, content: F) -> Self
    where
        F: Fn() -> Div + Send + Sync + 'static,
    {
        let value_str = value.into();
        let label_str = label.into();
        self.config.tabs.push(TabItem {
            menu_item: TabMenuItem::new(value_str).label(label_str),
            content: Arc::new(content),
        });
        self
    }

    /// Add a tab with a TabMenuItem for custom configuration
    ///
    /// # Example
    ///
    /// ```ignore
    /// cn::tabs(&state)
    ///     .tab_item(
    ///         cn::tab_item("settings")
    ///             .icon(settings_svg)
    ///             .label("Settings")
    ///             .badge("3"),
    ///         || div().child(text("Settings content"))
    ///     )
    /// ```
    pub fn tab_item<F>(mut self, item: TabMenuItem, content: F) -> Self
    where
        F: Fn() -> Div + Send + Sync + 'static,
    {
        self.config.tabs.push(TabItem {
            menu_item: item,
            content: Arc::new(content),
        });
        self
    }

    /// Add a disabled tab (simple API)
    pub fn tab_disabled<F>(
        mut self,
        value: impl Into<String>,
        label: impl Into<String>,
        content: F,
    ) -> Self
    where
        F: Fn() -> Div + Send + Sync + 'static,
    {
        let value_str = value.into();
        let label_str = label.into();
        self.config.tabs.push(TabItem {
            menu_item: TabMenuItem::new(value_str).label(label_str).disabled(),
            content: Arc::new(content),
        });
        self
    }

    /// Set the tabs size
    pub fn size(mut self, size: TabsSize) -> Self {
        self.config.size = size;
        self
    }

    /// Set the default value (will be set on first render if state is empty)
    pub fn default_value(mut self, value: impl Into<String>) -> Self {
        self.config.default_value = Some(value.into());
        self
    }

    /// Set the change callback
    pub fn on_change<F>(mut self, callback: F) -> Self
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        self.config.on_change = Some(Arc::new(callback));
        self
    }

    /// Set the content transition animation
    ///
    /// # Example
    ///
    /// ```ignore
    /// cn::tabs(&state)
    ///     .transition(TabsTransition::SlideLeft)
    ///     .tab("a", "Tab A", || div())
    /// ```
    pub fn transition(mut self, transition: TabsTransition) -> Self {
        self.config.transition = transition;
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

    /// Get or build the component
    fn get_or_build(&self) -> &Tabs {
        ::blinc_layout::build_once::build_once(&self.built, || self.build_component())
    }

    /// Build the tabs component
    fn build_component(&self) -> Tabs {
        let theme = ThemeState::get();
        let config = &self.config;

        // Get current value from state - use State<T>::get() directly
        let current_value = config.state.get();

        // If current value is empty and we have a default, use it
        if current_value.is_empty() {
            if let Some(ref default) = config.default_value {
                config.state.set(default.clone());
            } else if let Some(first_tab) = config.tabs.first() {
                let first_enabled = config
                    .tabs
                    .iter()
                    .find(|t| !t.menu_item.is_disabled())
                    .map(|t| t.menu_item.value().to_string())
                    .unwrap_or_else(|| first_tab.menu_item.value().to_string());
                config.state.set(first_enabled);
            }
        }

        // In 4px units, which is what `mt` takes. The token is already
        // pixels, so passing it straight through spaced the panel four
        // times further from the strip than the token asks for.
        let content_margin = theme.spacing().space_1 / 4.0;
        let size = config.size;

        // ========================================
        // Container 1: Tab Button Area
        // ========================================
        let tabs_for_buttons = config.tabs.clone();
        let state_for_buttons = config.state.clone();
        let on_change = config.on_change.clone();
        let transition = config.transition;

        let trigger_key = self.key.derive("tab_triggers");
        // Create motion base key for triggering animations from tab buttons (already a String)
        let motion_base_key_str = self.key.derive("motion");

        // The strip is built once. Each trigger follows the selected tab, so
        // a change patches the triggers instead of rebuilding the strip.
        let mut tab_button_area = div()
            .class("cn-tabs-list")
            .w_full()
            .flex_row()
            .items_center();

        for tab in tabs_for_buttons.iter() {
            let value = tab.menu_item.value();

            // Build motion key for this tab's content
            let tab_motion_key = if transition != TabsTransition::None {
                Some(format!("{}:{}", motion_base_key_str, value))
            } else {
                None
            };

            tab_button_area = tab_button_area.child(build_tab_trigger(
                &trigger_key,
                &tab.menu_item,
                size,
                state_for_buttons.clone(),
                on_change.clone(),
                tab_motion_key,
            ));
        }

        // ========================================
        // Container 2: Tab Content Area
        // ========================================
        // The selected tab's panel is the one row. With a transition it stays
        // mounted, out of flow, while its exit animation plays; the strip
        // above is not rebuilt either way.
        let tab_content_area = {
            let tabs_for_value = config.tabs.clone();
            let tabs_for_panel = config.tabs.clone();
            let selected = config.state.signal();
            let transition = config.transition;
            let motion_base_key = self.key.derive("motion");
            // The selected value, if a tab has it: the one row.
            let shown = computed(move |g: &ReactiveGraph| {
                let value = g.get(selected).unwrap_or_default();
                tabs_for_value
                    .iter()
                    .any(|t| t.menu_item.value() == value)
                    .then_some(value)
                    .into_iter()
                    .collect::<Vec<String>>()
            });

            div()
                .w_full()
                .mt(content_margin)
                .flex_grow()
                .relative()
                .for_each(
                    &shown,
                    |value: &String| value.clone(),
                    move |value: String| {
                        let tab = tabs_for_panel
                            .iter()
                            .find(|t| t.menu_item.value() == value)
                            .expect("a panel is built for a tab that exists");
                        let content = (tab.content)();
                        if transition == TabsTransition::None {
                            return Row::new(div().w_full().flex_grow().child(content));
                        }

                        // The key the strip's triggers start the enter
                        // animation with.
                        let tab_motion_key = format!("{}:{}", motion_base_key, value);
                        let mut m = motion_derived(&tab_motion_key);
                        if let Some(enter) = transition.enter_animation() {
                            m = m.enter_animation(enter);
                        }
                        if let Some(exit) = transition.exit_animation() {
                            m = m.exit_animation(exit);
                        }

                        let leaving = signal(false);
                        let panel = div()
                            .class_when("cn-tabs-panel--leaving", leaving)
                            .w_full()
                            .flex_grow()
                            .child(m.child(content));
                        let exit_key = format!("motion:{}:child:0", tab_motion_key);
                        let done_key = exit_key.clone();
                        Row::new(panel).on_leave(
                            move || {
                                leaving.set(true);
                                query_motion(&exit_key).exit();
                            },
                            move || !query_motion(&done_key).is_animating(),
                        )
                    },
                )
        };

        // Combine both containers
        let mut container = div()
            .w_full()
            .flex_grow()
            .flex_col()
            .child(tab_button_area)
            .child(tab_content_area);

        // Apply user classes and id
        for c in &self.classes {
            container = container.class(c);
        }
        if let Some(ref id) = self.user_id {
            container = container.id(id);
        }

        Tabs { inner: container }
    }
}

/// Build a tab trigger. Its colour, weight and active class follow the
/// selected tab and the pointer, in place.
#[allow(clippy::type_complexity)]
fn build_tab_trigger(
    trigger_key: &str,
    menu_item: &TabMenuItem,
    size: TabsSize,
    tab_state: State<String>,
    on_change: Option<Arc<dyn Fn(&str) + Send + Sync>>,
    motion_key: Option<String>,
) -> Div {
    let theme = ThemeState::get();
    let text_primary = theme.color(ColorToken::TextPrimary);
    let text_secondary = theme.color(ColorToken::TextSecondary);
    let value = menu_item.value.clone();
    let disabled = menu_item.disabled;

    let icon_svg = menu_item.icon.clone();
    let label_text = menu_item.label.clone();
    let badge_text = menu_item.badge.clone();

    let interaction = Interaction::keyed(&format!("{}:{}", trigger_key, value));
    let (selected, hovered) = (tab_state.signal(), interaction.hovered().signal());
    // Each reads the selected tab itself, so each is woken by it.
    let is_selected = move |g: &ReactiveGraph, selected_value: &str| {
        g.get(selected).unwrap_or_default() == selected_value
    };
    // The active class is for a tab that can be used.
    let is_active = {
        let value = value.clone();
        computed(move |g: &ReactiveGraph| !disabled && is_selected(g, &value))
    };
    let text_color = {
        let value = value.clone();
        computed(move |g: &ReactiveGraph| {
            if disabled {
                text_secondary.with_alpha(0.5)
            } else if is_selected(g, &value) {
                text_primary
            } else if g.get(hovered).unwrap_or(false) {
                text_primary.with_alpha(0.8)
            } else {
                text_secondary
            }
        })
    };
    let weight = {
        let value = value.clone();
        computed(move |g: &ReactiveGraph| {
            if is_selected(g, &value) {
                FontWeight::Medium
            } else {
                FontWeight::Normal
            }
        })
    };

    // Build content
    // `gap_px`, not `gap`: a spacing token is already pixels, while
    // `gap` takes 4px units and would space these four times apart.
    let mut content = div()
        .flex_row()
        .items_center()
        .gap_px(theme.spacing().space_2);

    // Add icon if present
    if let Some(ref icon) = icon_svg {
        content = content.child(
            svg(icon)
                .size(size.icon_size(), size.icon_size())
                .color(&text_color),
        );
    }

    // Add label if present
    if let Some(ref label) = label_text {
        content = content.child(
            text(label)
                .size(size.font_size())
                .color(&text_color)
                .weight(&weight)
                .no_cursor(),
        );
    }

    // Add badge if present. The shared widget rather than a pill of
    // its own, so a count in a tab reads like a count anywhere else.
    if let Some(ref badge_label) = badge_text {
        content = content.child(
            crate::components::badge::badge(badge_label)
                .variant(crate::components::badge::BadgeVariant::Default),
        );
    }

    // Determine size CSS class for trigger
    let trigger_size_class = match size {
        TabsSize::Small => "cn-tabs-trigger--sm",
        TabsSize::Medium => "cn-tabs-trigger--md",
        TabsSize::Large => "cn-tabs-trigger--lg",
    };

    let mut trigger = div()
        .class("cn-tabs-trigger")
        .class(trigger_size_class)
        .class_when("cn-tabs-trigger--active", &is_active)
        .flex_row()
        .items_center()
        .justify_center()
        .cursor(if disabled {
            CursorStyle::Default
        } else {
            CursorStyle::Pointer
        })
        .track(&interaction)
        .child(content);

    // Add disabled class
    if disabled {
        trigger = trigger.class("cn-tabs-trigger--disabled");
    }

    // Clicking the tab that is already selected does nothing.
    if !disabled {
        let value_for_click = value.clone();
        trigger = trigger.on_click(move |_| {
            if tab_state.get() == value_for_click {
                return;
            }
            // Start the motion animation for the new tab content
            if let Some(ref mk) = motion_key {
                let full_motion_key = format!("motion:{}:child:0", mk);
                query_motion(&full_motion_key).start();
            }

            tab_state.set(value_for_click.clone());
            if let Some(ref cb) = on_change {
                cb(&value_for_click);
            }
        });
    }

    trigger
}

impl ElementBuilder for TabsBuilder {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.get_or_build().inner.build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.get_or_build().inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().inner.children_builders()
    }

    fn element_type_id(&self) -> ElementTypeId {
        self.get_or_build().inner.element_type_id()
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.get_or_build().inner.layout_style()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.get_or_build().inner.element_classes()
    }

    fn element_id(&self) -> Option<&str> {
        self.get_or_build().inner.element_id()
    }
}

impl std::ops::Deref for TabsBuilder {
    type Target = Div;

    fn deref(&self) -> &Self::Target {
        &self.get_or_build().inner
    }
}

/// Create a new tabs component
///
/// # Example
///
/// ```ignore
/// let tab_state = ctx.use_state_keyed("tabs", || "tab1".to_string());
///
/// cn::tabs(&tab_state)
///     .tab("tab1", "First Tab", || div().child(text("Content 1")))
///     .tab("tab2", "Second Tab", || div().child(text("Content 2")))
/// ```
#[track_caller]
pub fn tabs(state: &State<String>) -> TabsBuilder {
    TabsBuilder::new(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tabs_size() {
        assert_eq!(TabsSize::Small.height(), 32.0);
        assert_eq!(TabsSize::Medium.height(), 40.0);
        assert_eq!(TabsSize::Large.height(), 48.0);
    }
}
