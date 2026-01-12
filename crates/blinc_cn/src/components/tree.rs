//! Tree View component for hierarchical data display
//!
//! A recursive tree structure with expand/collapse, selection, and customizable rendering.
//! Designed for file explorers, element inspectors, and hierarchical data views.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
//!     cn::tree_view()
//!         .node("root", "Project", |n| {
//!             n.expanded()
//!                 .child("src", "src/", |n| {
//!                     n.child("main", "main.rs", |_| {})
//!                         .child("lib", "lib.rs", |_| {})
//!                 })
//!                 .child("cargo", "Cargo.toml", |_| {})
//!         })
//!         .on_select(|key| println!("Selected: {}", key))
//! }
//! ```

use blinc_animation::{AnimatedValue, SchedulerHandle, SpringConfig};
use blinc_core::context_state::BlincContextState;
use blinc_core::{Color, SignalId, State};
use blinc_layout::div::ElementTypeId;
use blinc_layout::element::{CursorStyle, RenderProps};
use blinc_layout::motion::{motion, SharedAnimatedValue};
use blinc_layout::prelude::*;
use blinc_layout::render_state::get_global_scheduler;
use blinc_layout::stateful::Stateful;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_layout::InstanceKey;
use blinc_theme::{ColorToken, RadiusToken, ThemeState};
use std::cell::OnceCell;
use std::sync::{Arc, Mutex};

/// Chevron right SVG icon (collapsed state)
const CHEVRON_RIGHT_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m9 18 6-6-6-6"/></svg>"#;

/// Chevron down SVG icon (expanded state)
const CHEVRON_DOWN_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6"/></svg>"#;

/// Diff status for tree nodes (used in debugger)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TreeNodeDiff {
    /// No change
    #[default]
    None,
    /// Node was added
    Added,
    /// Node was removed
    Removed,
    /// Node was modified
    Modified,
}

/// Configuration for a tree node
#[derive(Clone)]
pub struct TreeNodeConfig {
    /// Unique key for this node
    pub key: String,
    /// Display label
    pub label: String,
    /// Optional icon (SVG string)
    pub icon: Option<String>,
    /// Whether this node starts expanded
    pub initially_expanded: bool,
    /// Diff status for highlighting
    pub diff: TreeNodeDiff,
    /// Child nodes
    pub children: Vec<TreeNodeConfig>,
}

impl TreeNodeConfig {
    /// Create a new tree node configuration
    pub fn new(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            icon: None,
            initially_expanded: false,
            diff: TreeNodeDiff::None,
            children: Vec::new(),
        }
    }

    /// Set the node to be initially expanded
    pub fn expanded(mut self) -> Self {
        self.initially_expanded = true;
        self
    }

    /// Set a custom icon (SVG string)
    pub fn icon(mut self, svg: impl Into<String>) -> Self {
        self.icon = Some(svg.into());
        self
    }

    /// Set diff status for highlighting
    pub fn diff(mut self, status: TreeNodeDiff) -> Self {
        self.diff = status;
        self
    }

    /// Add a child node using builder pattern
    pub fn child<F>(mut self, key: impl Into<String>, label: impl Into<String>, builder: F) -> Self
    where
        F: FnOnce(TreeNodeConfig) -> TreeNodeConfig,
    {
        let child = TreeNodeConfig::new(key, label);
        self.children.push(builder(child));
        self
    }
}

/// Tree View component for hierarchical data
pub struct TreeView {
    inner: Stateful<()>,
}

impl ElementBuilder for TreeView {
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
}

/// Callback for selection events
type SelectCallback = Arc<dyn Fn(&str) + Send + Sync>;

/// Builder for creating TreeView components
pub struct TreeViewBuilder {
    instance_key: InstanceKey,
    nodes: Vec<TreeNodeConfig>,
    selected_key: Option<String>,
    on_select: Option<SelectCallback>,
    indent_size: f32,
    show_guides: bool,
    built: OnceCell<TreeView>,
}

impl TreeViewBuilder {
    /// Create a new tree view builder
    pub fn new() -> Self {
        Self {
            instance_key: InstanceKey::new("tree_view"),
            nodes: Vec::new(),
            selected_key: None,
            on_select: None,
            indent_size: 4.0,
            show_guides: false,
            built: OnceCell::new(),
        }
    }

    fn get_or_build(&self) -> &TreeView {
        self.built.get_or_init(|| self.build_component())
    }

    /// Add a root-level node
    pub fn node<F>(mut self, key: impl Into<String>, label: impl Into<String>, builder: F) -> Self
    where
        F: FnOnce(TreeNodeConfig) -> TreeNodeConfig,
    {
        let node = TreeNodeConfig::new(key, label);
        self.nodes.push(builder(node));
        self
    }

    /// Add a pre-configured node
    pub fn add_node(mut self, node: TreeNodeConfig) -> Self {
        self.nodes.push(node);
        self
    }

    /// Set the initially selected key
    pub fn selected(mut self, key: impl Into<String>) -> Self {
        self.selected_key = Some(key.into());
        self
    }

    /// Set selection callback
    pub fn on_select<F>(mut self, callback: F) -> Self
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        self.on_select = Some(Arc::new(callback));
        self
    }

    /// Set indent size per level (default: 16.0)
    pub fn indent(mut self, size: f32) -> Self {
        self.indent_size = size;
        self
    }

    /// Show tree guides/lines
    pub fn with_guides(mut self) -> Self {
        self.show_guides = true;
        self
    }

    /// Build the tree view component
    fn build_component(&self) -> TreeView {
        let theme = ThemeState::get();

        // Get scheduler for animations
        let scheduler = get_global_scheduler()
            .expect("Animation scheduler not initialized - call this after app starts");

        // Collect all expand states for reactivity
        let mut all_signal_ids: Vec<SignalId> = Vec::new();
        let mut expand_states: Vec<(String, State<bool>, SharedAnimatedValue)> = Vec::new();

        // Create expand state for each node recursively
        fn collect_states(
            node: &TreeNodeConfig,
            instance_key: &InstanceKey,
            scheduler: &SchedulerHandle,
            signal_ids: &mut Vec<SignalId>,
            states: &mut Vec<(String, State<bool>, SharedAnimatedValue)>,
            spring_config: SpringConfig,
        ) {
            let state_key = format!("{}_{}_expanded", instance_key.get(), node.key);
            let is_expanded: State<bool> =
                BlincContextState::get().use_state_keyed(&state_key, || node.initially_expanded);

            signal_ids.push(is_expanded.signal_id());

            let initial_value = if is_expanded.get() { 1.0 } else { 0.0 };
            let anim: SharedAnimatedValue = Arc::new(Mutex::new(AnimatedValue::new(
                scheduler.clone(),
                initial_value,
                spring_config,
            )));

            states.push((node.key.clone(), is_expanded, anim));

            for child in &node.children {
                collect_states(
                    child,
                    instance_key,
                    scheduler,
                    signal_ids,
                    states,
                    spring_config,
                );
            }
        }

        let spring_config = SpringConfig::snappy();
        for node in &self.nodes {
            collect_states(
                node,
                &self.instance_key,
                &scheduler,
                &mut all_signal_ids,
                &mut expand_states,
                spring_config,
            );
        }

        // Selection state
        let selected_state_key = format!("{}_selected", self.instance_key.get());
        let selected: State<Option<String>> = BlincContextState::get()
            .use_state_keyed(&selected_state_key, || self.selected_key.clone());
        all_signal_ids.push(selected.signal_id());

        // Clone data for closure
        let nodes = self.nodes.clone();
        let indent_size = self.indent_size;
        let show_guides = self.show_guides;
        let on_select = self.on_select.clone();
        let container_key = format!("{}_container", self.instance_key.get());

        let container_state = use_shared_state_with(&container_key, ());

        let text_primary = theme.color(ColorToken::TextPrimary);
        let text_secondary = theme.color(ColorToken::TextSecondary);
        let text_tertiary = theme.color(ColorToken::TextTertiary);
        let surface_hover = theme.color(ColorToken::SurfaceElevated);
        let primary = theme.color(ColorToken::Primary);
        let radius = theme.radius(RadiusToken::Sm);

        // Diff colors
        let diff_added = theme.color(ColorToken::Success);
        let diff_removed = theme.color(ColorToken::Error);
        let diff_modified = theme.color(ColorToken::Warning);

        let inner =
            Stateful::with_shared_state(container_state)
                .deps(&all_signal_ids)
                .on_state(move |_state: &(), container: &mut Div| {
                    let mut tree_container = div().flex_col().flex_shrink_0();

                    // Build tree recursively
                    fn build_node(
                        node: &TreeNodeConfig,
                        depth: usize,
                        indent_size: f32,
                        show_guides: bool,
                        expand_states: &[(String, State<bool>, SharedAnimatedValue)],
                        selected: &State<Option<String>>,
                        on_select: &Option<SelectCallback>,
                        text_primary: Color,
                        text_secondary: Color,
                        text_tertiary: Color,
                        _surface_hover: Color,
                        primary: Color,
                        radius: f32,
                        diff_added: Color,
                        diff_removed: Color,
                        diff_modified: Color,
                    ) -> Div {
                        let has_children = !node.children.is_empty();
                        let indent = depth as f32 * indent_size;

                        // Find this node's expand state
                        let expand_state = expand_states
                            .iter()
                            .find(|(k, _, _)| k == &node.key)
                            .map(|(_, s, a)| (s.clone(), a.clone()));

                        let is_expanded =
                            expand_state.as_ref().map(|(s, _)| s.get()).unwrap_or(false);

                        let is_selected = selected.get().as_ref() == Some(&node.key);

                        // Diff-based coloring
                        let label_color = match node.diff {
                            TreeNodeDiff::None => {
                                if is_selected {
                                    primary
                                } else {
                                    text_primary
                                }
                            }
                            TreeNodeDiff::Added => diff_added,
                            TreeNodeDiff::Removed => diff_removed,
                            TreeNodeDiff::Modified => diff_modified,
                        };

                        // Background for selected/hover
                        let bg = if is_selected {
                            primary.with_alpha(0.15)
                        } else {
                            Color::TRANSPARENT
                        };

                        // Build the node row
                        let node_key = node.key.clone();
                        let selected_for_click = selected.clone();
                        let on_select_for_click = on_select.clone();

                        // Expand/collapse handler
                        let expand_state_for_row = expand_state.clone();

                        let mut row = div()
                            .flex_row()
                            .items_center()
                            .flex_shrink_0()
                            .h(28.0)
                            .pl(indent + 1.0)
                            .pr(2.0)
                            .rounded(radius)
                            .bg(bg)
                            .cursor(CursorStyle::Pointer)
                            .on_click(move |_| {
                                // Update selection
                                selected_for_click.set(Some(node_key.clone()));

                                // Call callback
                                if let Some(cb) = &on_select_for_click {
                                    cb(&node_key);
                                }

                                // Also toggle expand if has children
                                if let Some((state, anim)) = &expand_state_for_row {
                                    let new_expanded = !state.get();
                                    state.set(new_expanded);
                                    let target = if new_expanded { 1.0 } else { 0.0 };
                                    anim.lock().unwrap().set_target(target);
                                }
                            });

                        // Expand/collapse chevron (if has children)
                        if has_children {
                            let chevron = if is_expanded {
                                CHEVRON_DOWN_SVG
                            } else {
                                CHEVRON_RIGHT_SVG
                            };

                            row = row.child(
                                div()
                                    .w(16.0)
                                    .h(16.0)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .mr(1.0)
                                    .flex_shrink_0()
                                    .child(svg(chevron).size(16.0, 16.0).color(text_secondary)),
                            );
                        } else {
                            // Spacer for alignment (matches chevron container width)
                            row = row.child(div().w(5.0).h(4.0).flex_shrink_0());
                        }

                        // Diff indicator icon (+/-/~)
                        match node.diff {
                            TreeNodeDiff::Added => {
                                row = row.child(
                                    div()
                                        .mr(1.0)
                                        .flex_shrink_0()
                                        .child(text("+").size(13.0).color(diff_added).no_wrap()),
                                );
                            }
                            TreeNodeDiff::Removed => {
                                row = row.child(
                                    div()
                                        .mr(1.0)
                                        .flex_shrink_0()
                                        .child(text("−").size(13.0).color(diff_removed).no_wrap()),
                                );
                            }
                            TreeNodeDiff::Modified => {
                                row =
                                    row.child(div().mr(1.0).flex_shrink_0().child(
                                        text("~").size(13.0).color(diff_modified).no_wrap(),
                                    ));
                            }
                            TreeNodeDiff::None => {}
                        }

                        // Optional custom icon
                        if let Some(icon_svg) = &node.icon {
                            row =
                                row.child(
                                    div().w(3.5).h(3.5).mr(1.5).flex_shrink_0().child(
                                        svg(icon_svg).size(14.0, 14.0).color(text_secondary),
                                    ),
                                );
                        }

                        // Label
                        row = row.child(
                            text(&node.label)
                                .size(13.0)
                                .color(label_color)
                                .no_wrap()
                                .pointer_events_none(),
                        );

                        // Build node container with optional children
                        let mut node_div = div().flex_col().flex_shrink_0().child(row);

                        // Children (if expanded)
                        if has_children && is_expanded {
                            let anim_key = format!("tree-children-{}", node.key);

                            let mut children_container = div()
                                .flex_col()
                                .flex_shrink_0()
                                .relative()
                                .overflow_clip()
                                .animate_bounds(
                                    blinc_layout::visual_animation::VisualAnimationConfig::height()
                                        .with_key(&anim_key)
                                        .clip_to_animated()
                                        .gentle(),
                                );

                            // Optional guide line - positioned at center of this node's chevron
                            if show_guides {
                                children_container = children_container.child(
                                    div()
                                        .absolute()
                                        .left((indent * 4.0) + 12.0)
                                        .top(0.0)
                                        .bottom(0.0)
                                        .w(1.0)
                                        .bg(text_tertiary.with_alpha(0.5)),
                                );
                            }

                            for child in &node.children {
                                children_container = children_container.child(build_node(
                                    child,
                                    depth + 1,
                                    indent_size,
                                    show_guides,
                                    expand_states,
                                    selected,
                                    on_select,
                                    text_primary,
                                    text_secondary,
                                    text_tertiary,
                                    _surface_hover,
                                    primary,
                                    radius,
                                    diff_added,
                                    diff_removed,
                                    diff_modified,
                                ));
                            }

                            node_div = node_div.child(children_container);
                        }

                        node_div
                    }

                    for node in &nodes {
                        tree_container = tree_container.child(build_node(
                            node,
                            0,
                            indent_size,
                            show_guides,
                            &expand_states,
                            &selected,
                            &on_select,
                            text_primary,
                            text_secondary,
                            text_tertiary,
                            surface_hover,
                            primary,
                            radius,
                            diff_added,
                            diff_removed,
                            diff_modified,
                        ));
                    }

                    container.merge(tree_container);
                });

        TreeView { inner }
    }
}

impl Default for TreeViewBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ElementBuilder for TreeViewBuilder {
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
}

/// Create a tree view component
///
/// Uses global animation scheduler - no context needed.
///
/// # Example
///
/// ```ignore
/// cn::tree_view()
///     .node("root", "Project", |n| {
///         n.expanded()
///             .child("src", "src/", |n| {
///                 n.child("main", "main.rs", |_| {})
///             })
///     })
///     .on_select(|key| println!("Selected: {}", key))
/// ```
pub fn tree_view() -> TreeViewBuilder {
    TreeViewBuilder::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tree_node_config() {
        let node = TreeNodeConfig::new("test", "Test Node")
            .expanded()
            .diff(TreeNodeDiff::Added)
            .child("child1", "Child 1", |n| n)
            .child("child2", "Child 2", |n| n.expanded());

        assert_eq!(node.key, "test");
        assert_eq!(node.label, "Test Node");
        assert!(node.initially_expanded);
        assert_eq!(node.diff, TreeNodeDiff::Added);
        assert_eq!(node.children.len(), 2);
    }

    #[test]
    fn test_tree_node_diff_default() {
        assert_eq!(TreeNodeDiff::default(), TreeNodeDiff::None);
    }
}
