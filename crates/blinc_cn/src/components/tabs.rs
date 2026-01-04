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
use std::sync::{Arc, Mutex};

use blinc_animation::{AnimationPreset, MultiKeyframeAnimation};
use blinc_core::{State, use_state_keyed};
use blinc_layout::div::ElementTypeId;
use blinc_layout::element::{CursorStyle, RenderProps};
use blinc_layout::motion::motion_derived;
use blinc_layout::prelude::*;
use blinc_layout::stateful::Stateful;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_theme::{ColorToken, RadiusToken, ThemeState};

use blinc_layout::InstanceKey;

use crate::button::use_button_state;

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
            TabsTransition::Fade => Some(AnimationPreset::fade_in(200)),
            TabsTransition::SlideLeft => Some(AnimationPreset::slide_in_left(200, 20.0)),
            TabsTransition::SlideRight => Some(AnimationPreset::slide_in_right(200, 20.0)),
            TabsTransition::SlideUp => Some(AnimationPreset::slide_in_top(200, 20.0)),
            TabsTransition::SlideDown => Some(AnimationPreset::slide_in_bottom(200, 20.0)),
        }
    }

    /// Get exit animation for this transition
    fn exit_animation(&self) -> Option<MultiKeyframeAnimation> {
        match self {
            TabsTransition::None => None,
            TabsTransition::Fade => Some(AnimationPreset::fade_out(150)),
            TabsTransition::SlideLeft => Some(AnimationPreset::slide_out_left(150, 20.0)),
            TabsTransition::SlideRight => Some(AnimationPreset::slide_out_right(150, 20.0)),
            TabsTransition::SlideUp => Some(AnimationPreset::slide_out_top(150, 20.0)),
            TabsTransition::SlideDown => Some(AnimationPreset::slide_out_bottom(150, 20.0)),
        }
    }
}

/// Configuration for the tabs component
#[derive(Clone)]
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

    /// Get or build the component
    fn get_or_build(&self) -> &Tabs {
        self.built.get_or_init(|| self.build_component())
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

        // Theme colors - use SecondaryHover for better contrast with text
        let tab_list_bg = theme.color(ColorToken::SecondaryHover);
        let radius = theme.radius(RadiusToken::Md);
        let content_margin = theme.spacing().space_1;
        let size = config.size;

        // ========================================
        // Container 1: Tab Button Area
        // ========================================
        let tabs_for_buttons = config.tabs.clone();
        let state_for_buttons = config.state.clone();
        let on_change = config.on_change.clone();

        let unique_state_component_key = use_button_state(self.key.get());

        let tab_button_area = Stateful::with_shared_state(unique_state_component_key)
            .deps(&[config.state.signal_id()])
            .h(size.height())
            .w_full()
            .bg(tab_list_bg)
            .rounded(radius)
            .p(4.0)
            .flex_row()
            .items_center()
            .gap(4.0)
            .on_state(move |_state, container: &mut Div| {
                let active_value = state_for_buttons.get();

                let mut buttons = div().w_fit().flex_row().items_center().gap(4.0);

                for tab in tabs_for_buttons.iter() {
                    let is_active = tab.menu_item.value() == active_value;

                    let tab_trigger = build_tab_trigger(
                        &tab.menu_item,
                        is_active,
                        size,
                        state_for_buttons.clone(),
                        on_change.clone(),
                    );

                    buttons = buttons.child(tab_trigger);
                }

                container.merge(buttons);
            });

        // ========================================
        // Container 2: Tab Content Area
        // ========================================
        let tabs_for_content = config.tabs.clone();
        let state_for_content = config.state.clone();
        let transition = config.transition;
        // Clone the base key for deriving motion keys inside on_state
        let motion_base_key = self.key.derive("motion");

        let content_area_state = use_shared_state(&self.key.derive("content_area"));
        let tab_content_area = Stateful::with_shared_state(content_area_state)
            .deps(&[config.state.signal_id()])
            .w_full()
            .flex_grow()
            .mt(content_margin)
            .on_state(move |_state: &(), container: &mut Div| {
                let active_value = state_for_content.get();

                // Find and render the active tab's content
                for tab in &tabs_for_content {
                    if tab.menu_item.value() == active_value {
                        let content = (tab.content)();
                        if transition != TabsTransition::None {
                           
                            let tab_motion_key = format!("{}:{}", motion_base_key, active_value);
                            let mut m = motion_derived(&tab_motion_key).replay();
                            if let Some(enter) = transition.enter_animation() {
                                m = m.enter_animation(enter);
                            }
                            // Wrap in div since merge() expects Div
                            container.merge(div().w_full().flex_grow().child(m.child(content)));
                        } else {
                            // Wrap content in div to ensure it fills parent
                            container.merge(div().w_full().flex_grow().child(content));
                        }
                        break;
                    }
                }
            });

        // Combine both containers
        Tabs {
            inner: div()
                .w_full()
                .flex_grow()
                .flex_col()
                .child(tab_button_area)
                .child(tab_content_area),
        }
    }
}

/// Build a simple tab trigger without nested Stateful (no hover effects)
fn build_tab_trigger(
    menu_item: &TabMenuItem,
    is_active: bool,
    size: TabsSize,
    tab_state: State<String>,
    on_change: Option<Arc<dyn Fn(&str) + Send + Sync>>,
) -> Div {
    let theme = ThemeState::get();
    let text_primary = theme.color(ColorToken::TextPrimary);
    let text_tertiary = theme.color(ColorToken::TextTertiary);
    let surface = theme.color(ColorToken::Surface);
    let radius = theme.radius(RadiusToken::Md);

    let value = menu_item.value.clone();
    let disabled = menu_item.disabled;

    // Calculate inner height (tab list height minus padding)
    let inner_height = size.height() - 8.0;

    // Determine colors based on state
    // Use TextPrimary for active, TextTertiary for inactive (better contrast on muted bg)
    let text_color = if disabled {
        text_tertiary.with_alpha(0.5)
    } else if is_active {
        text_primary
    } else {
        text_tertiary
    };

    let bg = if is_active && !disabled {
        surface
    } else {
        blinc_core::Color::TRANSPARENT
    };

    // Build content
    let mut content = div().flex_row().items_center().gap(theme.spacing().space_2);

    // Add icon if present
    if let Some(ref icon_svg) = menu_item.icon {
        content = content.child(
            svg(icon_svg)
                .size(size.icon_size(), size.icon_size())
                .color(text_color),
        );
    }

    // Add label if present
    if let Some(ref label) = menu_item.label {
        content = content.child(
            text(label)
                .size(size.font_size())
                .color(text_color)
                .weight(if is_active {
                    FontWeight::Medium
                } else {
                    FontWeight::Normal
                }).no_cursor(),
        );
    }

    // Add badge if present
    if let Some(ref badge_text) = menu_item.badge {
        let primary = theme.color(ColorToken::Primary);
        content = content.child(
            div()
                .px(theme.spacing().space_1_5)
                .py(1.0)
                .bg(primary)
                .rounded(theme.radius(RadiusToken::Full))
                .child(
                    text(badge_text)
                        .size(size.badge_font_size())
                        .color(theme.color(ColorToken::PrimaryActive))
                        .medium().no_cursor(),
                ),
        );
    }

    // Build the trigger div
    let mut trigger = div()
        .h(inner_height)
        .padding_x(Length::Px(size.padding_x()))
        .flex_row()
        .items_center()
        .justify_center()
        .rounded(radius)
        .bg(bg)
        .cursor(if disabled {
            CursorStyle::Default
        } else {
            CursorStyle::Pointer
        })
        .child(content);

    // Add shadow for active tab
    if is_active && !disabled {
        trigger = trigger.shadow_sm();
    }

    // Add click handler if not disabled and not active
    if !disabled && !is_active {
        let value_for_click = value.clone();
        trigger = trigger.on_click(move |_| {
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
