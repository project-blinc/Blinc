//! Tree View component for hierarchical data display
//!
//! A selectable tree whose items open and close with an eased height. Each
//! item is a row (a chevron or the space of one, an optional icon, the label)
//! and, for an item with children, a group of child items indented beside a
//! guide line.
//!
//! Rows carry `data-selected`, `data-diff` when the node has a diff status,
//! and `data-focus-visible` while the keys are on them. Items with children
//! and their groups carry `data-state="open"` or `"closed"`, and a closed
//! group is laid out at no height. Their look is the `.cn-tree*` rules; the
//! height change eases over the theme's normal duration and the rows below
//! move with it.
//!
//! Click a row to select it, and to open or close it. Once the tree has been
//! clicked, Up and Down move through the visible rows, Right opens a row or
//! enters its children, Left closes it or moves to its parent, Home and End
//! reach the first and last rows, and Enter or Space selects.
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
//!                     n.child("main", "main.rs", |c| c)
//!                         .child("lib", "lib.rs", |c| c)
//!                 })
//!                 .child("cargo", "Cargo.toml", |c| c)
//!         })
//!         .on_select(|key| println!("Selected: {}", key))
//! }
//! ```

use blinc_core::context_state::BlincContextState;
use blinc_core::reactive::{ReactiveGraph, Signal, computed};
use blinc_layout::InstanceKey;
use blinc_layout::div::ElementTypeId;
use blinc_layout::element::RenderProps;
use blinc_layout::prelude::*;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_layout::visual_animation::VisualAnimationConfig;
use blinc_theme::ThemeState;
use std::cell::OnceCell;
use std::collections::HashMap;
use std::sync::Arc;

use super::number_gauge::GaugeFocus;

/// The chevron of an item with children; it turns to point down while open.
const CHEVRON_RIGHT_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m9 18 6-6-6-6"/></svg>"#;

/// How far a child group sits in from its item, in px.
const INDENT: f32 = 14.0;

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

impl TreeNodeDiff {
    /// The `data-diff` value the rules colour the row by.
    fn attr(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Added => Some("added"),
            Self::Removed => Some("removed"),
            Self::Modified => Some("modified"),
        }
    }
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

    /// Set a leading icon (SVG markup drawn in the row's icon colour)
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
    inner: Div,
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

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.inner.element_classes()
    }

    fn element_id(&self) -> Option<&str> {
        self.inner.element_id()
    }

    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(&self.inner)
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
    indent: f32,
    guides: bool,
    /// User-added CSS classes
    classes: Vec<std::sync::Arc<str>>,
    /// User-set element ID
    user_id: Option<String>,
    built: OnceCell<TreeView>,
}

impl TreeViewBuilder {
    /// Create a new tree view builder
    #[track_caller]
    pub fn new() -> Self {
        Self {
            instance_key: InstanceKey::new("tree_view"),
            nodes: Vec::new(),
            selected_key: None,
            on_select: None,
            indent: INDENT,
            guides: true,
            classes: Vec::new(),
            user_id: None,
            built: OnceCell::new(),
        }
    }

    fn get_or_build(&self) -> &TreeView {
        ::blinc_layout::build_once::build_once(&self.built, || self.build_component())
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

    /// How far each child group sits in from its item, in px (default 14).
    pub fn indent(mut self, px: f32) -> Self {
        self.indent = px;
        self
    }

    /// Show guide lines beside child groups (the default).
    pub fn with_guides(self) -> Self {
        self.guides(true)
    }

    /// Whether to show guide lines beside child groups.
    pub fn guides(mut self, on: bool) -> Self {
        self.guides = on;
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

    /// Build the tree view component
    fn build_component(&self) -> TreeView {
        let instance = self.instance_key.get().to_string();
        let ctx = BlincContextState::get();
        let initial = self.selected_key.clone();
        let selected = ctx.use_signal_keyed(&format!("{instance}_selected"), || initial);
        let cursor: Signal<Option<String>> =
            ctx.use_signal_keyed(&format!("{instance}_cursor"), || None);

        let mut nodes = HashMap::new();
        for node in &self.nodes {
            index(node, None, &instance, &mut nodes);
        }
        let roots: Vec<String> = self.nodes.iter().map(|n| n.key.clone()).collect();

        let tree_id = self
            .user_id
            .clone()
            .unwrap_or_else(|| format!("cn-tree-{instance}"));
        let parts = Parts {
            nodes: Arc::new(nodes),
            selected,
            cursor,
            focus: GaugeFocus::new(&instance, &tree_id),
            on_select: self.on_select.clone(),
            indent: self.indent,
            font_size: ThemeState::get().typography().text_sm,
        };

        let mut tree = div()
            .class("cn-tree")
            .id(&tree_id)
            .attr_when("data-guides", "", self.guides)
            .flex_col()
            .flex_shrink_0()
            .gap(0.25);
        for c in &self.classes {
            tree = tree.class(c);
        }
        for node in &self.nodes {
            tree = tree.child(item(node, &parts));
        }
        let keys = parts;
        let tree = tree.on_key_down(move |ctx| {
            if keys.focus.focused.get() && keys.key(&roots, ctx.key_code) {
                keys.focus.key();
            }
        });

        TreeView { inner: tree }
    }
}

/// What a node is to the keyboard: its open signal, its parent and children.
struct NodeInfo {
    expanded: Signal<bool>,
    parent: Option<String>,
    children: Vec<String>,
}

fn index(
    node: &TreeNodeConfig,
    parent: Option<&str>,
    instance: &str,
    out: &mut HashMap<String, NodeInfo>,
) {
    let open = node.initially_expanded;
    let expanded = BlincContextState::get()
        .use_signal_keyed(&format!("{instance}_{}_expanded", node.key), || open);
    out.insert(
        node.key.clone(),
        NodeInfo {
            expanded,
            parent: parent.map(str::to_string),
            children: node.children.iter().map(|c| c.key.clone()).collect(),
        },
    );
    for child in &node.children {
        index(child, Some(&node.key), instance, out);
    }
}

/// What every item of one tree shares.
#[derive(Clone)]
struct Parts {
    nodes: Arc<HashMap<String, NodeInfo>>,
    selected: Signal<Option<String>>,
    /// The row the keys are on.
    cursor: Signal<Option<String>>,
    focus: GaugeFocus,
    on_select: Option<SelectCallback>,
    indent: f32,
    font_size: f32,
}

impl Parts {
    fn select(&self, key: &str) {
        self.selected.set(Some(key.to_string()));
        if let Some(cb) = &self.on_select {
            cb(key);
        }
    }

    fn is_open(&self, key: &str) -> bool {
        self.nodes
            .get(key)
            .is_some_and(|n| n.expanded.try_get().unwrap_or(false))
    }

    /// The rows shown, in order: an item's children only while it is open.
    fn shown(&self, roots: &[String]) -> Vec<String> {
        fn walk(parts: &Parts, keys: &[String], out: &mut Vec<String>) {
            for key in keys {
                out.push(key.clone());
                if parts.is_open(key) {
                    walk(parts, &parts.nodes[key].children, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(self, roots, &mut out);
        out
    }

    /// A key while the tree has focus; true when it did something.
    fn key(&self, roots: &[String], code: u32) -> bool {
        const ENTER: u32 = 13;
        const SPACE: u32 = 32;
        const END: u32 = 35;
        const HOME: u32 = 36;
        const LEFT: u32 = 37;
        const UP: u32 = 38;
        const RIGHT: u32 = 39;
        const DOWN: u32 = 40;

        if !matches!(code, ENTER | SPACE | END | HOME | LEFT | UP | RIGHT | DOWN) {
            return false;
        }
        let rows = self.shown(roots);
        if rows.is_empty() {
            return false;
        }
        // The first key lands on the selection, or the first row.
        let current = self
            .cursor
            .try_get()
            .flatten()
            .or_else(|| self.selected.try_get().flatten());
        let Some(at) = current.and_then(|k| rows.iter().position(|r| *r == k)) else {
            self.cursor.set(Some(rows[0].clone()));
            return true;
        };
        let here = &rows[at];
        let node = &self.nodes[here];
        let branch = !node.children.is_empty();
        let next = match code {
            DOWN => rows.get(at + 1).cloned(),
            UP => at.checked_sub(1).map(|i| rows[i].clone()),
            HOME => rows.first().cloned(),
            END => rows.last().cloned(),
            RIGHT if branch && self.is_open(here) => node.children.first().cloned(),
            RIGHT if branch => {
                node.expanded.set(true);
                None
            }
            LEFT if branch && self.is_open(here) => {
                node.expanded.set(false);
                None
            }
            LEFT => node.parent.clone(),
            ENTER | SPACE => {
                self.select(here);
                None
            }
            _ => None,
        };
        if let Some(next) = next {
            self.cursor.set(Some(next));
        }
        true
    }
}

/// One item: its row and, with children, its group.
fn item(node: &TreeNodeConfig, parts: &Parts) -> Div {
    let key = node.key.clone();
    let branch = !node.children.is_empty();
    let expanded = parts.nodes[&key].expanded;
    let open = computed(move |g: &ReactiveGraph| g.get(expanded).unwrap_or(false));
    let closed = computed(move |g: &ReactiveGraph| !g.get(expanded).unwrap_or(false));

    let selected = parts.selected;
    let is_selected = {
        let key = key.clone();
        computed(move |g: &ReactiveGraph| g.get(selected).flatten().as_deref() == Some(&*key))
    };
    let focus_here = {
        let key = key.clone();
        let (cursor, focused, keyed) = (parts.cursor, parts.focus.focused, parts.focus.keyed);
        computed(move |g: &ReactiveGraph| {
            g.get(focused).unwrap_or(false)
                && g.get(keyed).unwrap_or(false)
                && g.get(cursor).flatten().as_deref() == Some(&*key)
        })
    };

    let mut row = div()
        .class("cn-tree-row")
        .attr_when("data-selected", "", &is_selected)
        .attr_when("data-focus-visible", "", &focus_here)
        .flex_row()
        .items_center()
        .flex_shrink_0()
        .h(28.0)
        .px(2.0)
        .gap(1.5);
    if let Some(diff) = node.diff.attr() {
        row = row.attr("data-diff", diff);
    }
    row = row.child(if branch {
        div()
            .class("cn-tree-chevron")
            .flex_shrink_0()
            .child(svg(CHEVRON_RIGHT_SVG).size(14.0, 14.0))
            .pointer_events_none()
    } else {
        div().class("cn-tree-chevron-space").flex_shrink_0().w(14.0)
    });
    if let Some(icon) = &node.icon {
        row = row.child(
            div()
                .class("cn-tree-icon")
                .flex_shrink_0()
                .child(svg(icon).size(16.0, 16.0))
                .pointer_events_none(),
        );
    }
    row = row.child(
        div()
            .class("cn-tree-label")
            .flex_row()
            .items_center()
            .flex_shrink_0()
            .child(
                text(&node.label)
                    .size(parts.font_size)
                    .no_wrap()
                    .no_cursor()
                    .pointer_events_none(),
            ),
    );
    let on_click = parts.clone();
    let row = row.on_click(move |_| {
        on_click.focus.press();
        on_click.cursor.set(Some(key.clone()));
        on_click.select(&key);
        if branch {
            expanded.set(!expanded.try_get().unwrap_or(false));
        }
    });

    // The rows below an opening group move with it.
    let mut shell = div()
        .class("cn-tree-item")
        .flex_col()
        .flex_shrink_0()
        .gap(0.25)
        .animate_bounds(
            VisualAnimationConfig::position()
                .with_key(format!("cn-tree-item-{}", node.key))
                .theme_timed(),
        )
        .child(row);
    if branch {
        shell =
            shell
                .attr_when("data-state", "open", &open)
                .attr_when("data-state", "closed", &closed);
        // Laid out once built, so there is a height to ease to and from; a
        // closed group is at no height by rule.
        let mut group = div()
            .class("cn-tree-group")
            .attr_when("data-state", "open", &open)
            .attr_when("data-state", "closed", &closed)
            .flex_col()
            .flex_shrink_0()
            .gap(0.25)
            .relative()
            .ml(parts.indent / 4.0)
            .pl(7.0 / 4.0)
            .overflow_clip()
            .animate_bounds(
                VisualAnimationConfig::height()
                    .with_key(format!("cn-tree-group-{}", node.key))
                    .clip_to_animated()
                    .theme_timed(),
            )
            .child(
                div()
                    .class("cn-tree-guide")
                    .absolute()
                    .left(0.0)
                    .top(0.0)
                    .bottom(0.0)
                    .w(1.0),
            );
        // The rows below a group are built when it first opens and kept.
        let (children, rows_parts) = (node.children.clone(), parts.clone());
        group = group.show(super::collapsible::opened_once(expanded), move || {
            children
                .iter()
                .fold(div().flex_col().gap(0.25), |rows, child| {
                    rows.child(item(child, &rows_parts))
                })
        });
        shell = shell.child(group);
    }
    shell
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

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.get_or_build().element_classes()
    }

    fn element_id(&self) -> Option<&str> {
        self.get_or_build().element_id()
    }

    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        self.get_or_build().event_handlers()
    }
}

/// Create a tree view component
///
/// # Example
///
/// ```ignore
/// cn::tree_view()
///     .node("root", "Project", |n| {
///         n.expanded()
///             .child("src", "src/", |n| n.child("main", "main.rs", |c| c))
///     })
///     .on_select(|key| println!("Selected: {}", key))
/// ```
#[track_caller]
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
