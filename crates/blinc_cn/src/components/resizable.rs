//! Resizable panel component with drag handles
//!
//! A set of components for creating resizable panel layouts.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! // Horizontal resizable group
//! resizable_group()
//!     .direction(ResizeDirection::Horizontal)
//!     .panel(
//!         resizable_panel()
//!             .default_size(200.0)
//!             .min_size(100.0)
//!             .max_size(400.0)
//!             .child(sidebar_content())
//!     )
//!     .panel(
//!         resizable_panel()
//!             .flex_grow()
//!             .child(main_content())
//!     )
//!     .panel(
//!         resizable_panel()
//!             .default_size(250.0)
//!             .min_size(150.0)
//!             .child(inspector_content())
//!     )
//!
//! // Vertical resizable group
//! resizable_group()
//!     .direction(ResizeDirection::Vertical)
//!     .panel(
//!         resizable_panel()
//!             .flex_grow()
//!             .child(main_area())
//!     )
//!     .panel(
//!         resizable_panel()
//!             .default_size(150.0)
//!             .min_size(80.0)
//!             .child(timeline())
//!     )
//! ```

use std::cell::OnceCell;
use std::sync::Arc;

use blinc_core::State;
use blinc_core::context_state::BlincContextState;
use blinc_layout::InstanceKey;
use blinc_layout::div::{Div, ElementBuilder};
use blinc_layout::element::{CursorStyle, RenderProps};
use blinc_layout::prelude::*;
use blinc_layout::stateful::{NoState, stateful_with_key};
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_theme::{ColorToken, ThemeState};

/// Direction of resize operation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResizeDirection {
    /// Panels arranged horizontally, handles resize width
    #[default]
    Horizontal,
    /// Panels arranged vertically, handles resize height
    Vertical,
}

/// Configuration for a resize handle
#[derive(Clone)]
pub struct ResizeHandleConfig {
    /// Handle thickness in pixels
    pub thickness: f32,
    /// Hit area extends beyond visible handle
    pub hit_area_padding: f32,
    /// Whether handle is currently being dragged
    pub show_active: bool,
}

impl Default for ResizeHandleConfig {
    fn default() -> Self {
        Self {
            thickness: 4.0,
            hit_area_padding: 4.0,
            show_active: true,
        }
    }
}

/// Panel size specification
#[derive(Debug, Clone, Copy)]
pub enum PanelSize {
    /// Fixed size in pixels
    Fixed(f32),
    /// Flexible size that grows/shrinks
    Flex(f32),
}

impl Default for PanelSize {
    fn default() -> Self {
        PanelSize::Flex(1.0)
    }
}

/// Configuration for a resizable panel
struct ResizablePanelConfig {
    /// Default/initial size
    default_size: Option<f32>,
    /// Minimum allowed size
    min_size: f32,
    /// Maximum allowed size (None = unlimited)
    max_size: Option<f32>,
    /// Whether panel grows to fill available space
    is_flex: bool,
    /// Flex grow factor when is_flex is true
    flex_grow: f32,
    /// Panel content
    content: Option<Box<dyn ElementBuilder>>,
    /// Unique ID for this panel
    id: String,
}

impl Default for ResizablePanelConfig {
    fn default() -> Self {
        Self {
            default_size: None,
            min_size: 50.0,
            max_size: None,
            is_flex: false,
            flex_grow: 1.0,
            content: None,
            id: String::new(),
        }
    }
}

/// Builder for a resizable panel
pub struct ResizablePanelBuilder {
    config: ResizablePanelConfig,
}

impl ResizablePanelBuilder {
    /// Create a new resizable panel builder
    pub fn new() -> Self {
        Self {
            config: ResizablePanelConfig::default(),
        }
    }

    /// Set the panel ID (used for state persistence)
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.config.id = id.into();
        self
    }

    /// Set the default/initial size in pixels
    pub fn default_size(mut self, size: f32) -> Self {
        self.config.default_size = Some(size);
        self.config.is_flex = false;
        self
    }

    /// Set minimum size constraint
    pub fn min_size(mut self, size: f32) -> Self {
        self.config.min_size = size;
        self
    }

    /// Set maximum size constraint
    pub fn max_size(mut self, size: f32) -> Self {
        self.config.max_size = Some(size);
        self
    }

    /// Make this panel flexible (grows/shrinks to fill space)
    pub fn flex_grow(mut self) -> Self {
        self.config.is_flex = true;
        self.config.flex_grow = 1.0;
        self
    }

    /// Set flex grow factor (only applies when is_flex)
    pub fn flex(mut self, factor: f32) -> Self {
        self.config.is_flex = true;
        self.config.flex_grow = factor;
        self
    }

    /// Set the panel content
    pub fn child(mut self, content: impl ElementBuilder + 'static) -> Self {
        self.config.content = Some(Box::new(content));
        self
    }

    /// Build the config (internal use)
    fn build_config(self) -> ResizablePanelConfig {
        self.config
    }
}

impl Default for ResizablePanelBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Panel constraints extracted from config
#[derive(Clone, Copy)]
struct PanelConstraints {
    min_size: f32,
    max_size: Option<f32>,
}

/// Configuration for the resizable group
#[allow(clippy::type_complexity)]
struct ResizableGroupConfig {
    /// Direction of resize (horizontal or vertical)
    direction: ResizeDirection,
    /// Handle configuration
    handle: ResizeHandleConfig,
    /// Panel configurations
    panels: Vec<ResizablePanelConfig>,
    /// Unique key for state persistence
    key: String,
    /// Callback when sizes change
    on_resize: Option<Arc<dyn Fn(&[f32]) + Send + Sync>>,
    /// User-added CSS classes
    classes: Vec<std::sync::Arc<str>>,
    /// User-set element ID
    user_id: Option<String>,
}

impl Default for ResizableGroupConfig {
    fn default() -> Self {
        Self {
            direction: ResizeDirection::Horizontal,
            handle: ResizeHandleConfig::default(),
            panels: Vec::new(),
            key: String::new(),
            on_resize: None,
            classes: Vec::new(),
            user_id: None,
        }
    }
}

/// The built resizable group component
pub struct ResizableGroup {
    inner: Div,
}

impl ResizableGroup {
    fn from_config(config: ResizableGroupConfig) -> Self {
        let ctx = BlincContextState::get();

        let direction = config.direction;
        let handle_config = config.handle;
        let key = if config.key.is_empty() {
            InstanceKey::new("resizable").get().to_string()
        } else {
            config.key
        };

        // Extract constraints before consuming panels
        let constraints: Vec<PanelConstraints> = config
            .panels
            .iter()
            .map(|p| PanelConstraints {
                min_size: p.min_size,
                max_size: p.max_size,
            })
            .collect();

        // Create persisted state for each panel's size
        let panel_sizes: Vec<State<f32>> = config
            .panels
            .iter()
            .enumerate()
            .map(|(i, panel)| {
                let panel_key = if panel.id.is_empty() {
                    format!("{}_panel_{}", key, i)
                } else {
                    format!("{}_{}", key, panel.id)
                };
                let default = panel.default_size.unwrap_or(200.0);
                ctx.use_state_keyed(&panel_key, || default)
            })
            .collect();

        // Create drag state
        let drag_index = ctx.use_state_keyed(&format!("{}_drag_idx", key), || -1i32);
        let drag_start_pos = ctx.use_state_keyed(&format!("{}_drag_start", key), || 0.0f32);
        let drag_start_sizes = ctx.use_state_keyed(&format!("{}_drag_sizes", key), Vec::<f32>::new);

        // Build container
        let mut container = div().w_full().h_full();

        match direction {
            ResizeDirection::Horizontal => {
                container = container.flex_row();
            }
            ResizeDirection::Vertical => {
                container = container.flex_col();
            }
        }

        // Build panels with handles between them
        let panel_count = config.panels.len();
        for (i, panel_config) in config.panels.into_iter().enumerate() {
            let size_state = panel_sizes[i].clone();
            let is_flex = panel_config.is_flex;
            let flex_grow_factor = panel_config.flex_grow;
            let min_size = panel_config.min_size;
            let max_size = panel_config.max_size;

            // Build panel wrapper
            let panel_wrapper: Div;

            if is_flex {
                // Flex panel - grows to fill available space
                let mut wrapper = div().overflow_clip().flex_grow_value(flex_grow_factor);
                match direction {
                    ResizeDirection::Horizontal => {
                        wrapper = wrapper.h_full();
                        if min_size > 0.0 {
                            wrapper = wrapper.min_w(min_size);
                        }
                        if let Some(max) = max_size {
                            wrapper = wrapper.max_w(max);
                        }
                    }
                    ResizeDirection::Vertical => {
                        wrapper = wrapper.w_full();
                        if min_size > 0.0 {
                            wrapper = wrapper.min_h(min_size);
                        }
                        if let Some(max) = max_size {
                            wrapper = wrapper.max_h(max);
                        }
                    }
                }

                // Add content directly for flex panels
                if let Some(content) = panel_config.content {
                    wrapper = wrapper.child_box(content);
                }
                panel_wrapper = wrapper;
            } else {
                // Fixed size panel - use stateful to react to size changes
                let panel_key = format!("{}_panel_wrapper_{}", key, i);
                let size_for_stateful = size_state.clone();

                // Create a stateful wrapper that adjusts its size based on state
                let sized_wrapper = stateful_with_key::<NoState>(&panel_key)
                    .deps([size_state.signal_id()])
                    .on_state(move |_ctx| {
                        let current_size = size_for_stateful.get();
                        let mut sizing = div().overflow_clip();

                        match direction {
                            ResizeDirection::Horizontal => {
                                sizing = sizing.w(current_size).h_full();
                                if min_size > 0.0 {
                                    sizing = sizing.min_w(min_size);
                                }
                                if let Some(max) = max_size {
                                    sizing = sizing.max_w(max);
                                }
                            }
                            ResizeDirection::Vertical => {
                                sizing = sizing.h(current_size).w_full();
                                if min_size > 0.0 {
                                    sizing = sizing.min_h(min_size);
                                }
                                if let Some(max) = max_size {
                                    sizing = sizing.max_h(max);
                                }
                            }
                        }

                        sizing
                    });

                // For fixed panels, wrap content with the sized stateful container
                if let Some(content) = panel_config.content {
                    // Create outer container with sized_wrapper that handles sizing
                    // and content as sibling - the sized wrapper dictates size
                    let mut outer = div().overflow_clip();
                    match direction {
                        ResizeDirection::Horizontal => {
                            outer = outer.h_full();
                        }
                        ResizeDirection::Vertical => {
                            outer = outer.w_full();
                        }
                    }
                    // Use position relative/absolute pattern for content overlay
                    outer = outer.relative().child(sized_wrapper).child(
                        div()
                            .absolute()
                            .left(0.0)
                            .top(0.0)
                            .right(0.0)
                            .bottom(0.0)
                            .overflow_clip()
                            .child_box(content),
                    );
                    panel_wrapper = outer;
                } else {
                    panel_wrapper = div().overflow_clip().child(sized_wrapper);
                }
            }

            container = container.child(panel_wrapper);

            // Add resize handle after each panel except the last
            if i < panel_count - 1 {
                let left_constraints = constraints[i];
                let right_constraints = constraints[i + 1];

                let handle = Self::build_handle(
                    i,
                    direction,
                    &handle_config,
                    &key,
                    &panel_sizes,
                    left_constraints,
                    right_constraints,
                    drag_index.clone(),
                    drag_start_pos.clone(),
                    drag_start_sizes.clone(),
                );
                container = container.child(handle);
            }
        }

        // Apply user classes and id
        for c in &config.classes {
            container = container.class(c);
        }
        if let Some(ref id) = config.user_id {
            container = container.id(id);
        }

        Self { inner: container }
    }

    #[allow(clippy::too_many_arguments)]
    fn build_handle(
        index: usize,
        direction: ResizeDirection,
        handle_config: &ResizeHandleConfig,
        key: &str,
        panel_sizes: &[State<f32>],
        left_constraints: PanelConstraints,
        right_constraints: PanelConstraints,
        drag_index: State<i32>,
        drag_start_pos: State<f32>,
        drag_start_sizes: State<Vec<f32>>,
    ) -> Div {
        let theme = ThemeState::get();
        let border_color = theme.color(ColorToken::Border);
        let primary_color = theme.color(ColorToken::Primary);

        let thickness = handle_config.thickness;
        let hit_padding = handle_config.hit_area_padding;
        let total_hit_area = thickness + hit_padding * 2.0;

        // Clones for closures
        let drag_index_for_down = drag_index.clone();
        let drag_index_for_drag = drag_index.clone();
        let drag_index_for_end = drag_index.clone();
        let drag_index_for_visual = drag_index.clone();

        let drag_start_pos_for_down = drag_start_pos.clone();
        let drag_start_pos_for_drag = drag_start_pos.clone();

        let drag_start_sizes_for_down = drag_start_sizes.clone();
        let drag_start_sizes_for_drag = drag_start_sizes.clone();

        // Clone panel sizes for drag handler
        let panel_sizes_for_down: Vec<State<f32>> = panel_sizes.to_vec();
        let panel_sizes_for_drag: Vec<State<f32>> = panel_sizes.to_vec();

        // Extract constraints
        let left_min = left_constraints.min_size;
        let left_max = left_constraints.max_size;
        let right_min = right_constraints.min_size;
        let right_max = right_constraints.max_size;

        let handle_key = format!("{}_handle_{}", key, index);
        let idx = index;

        // Use stateful to show visual feedback when dragging
        let handle = stateful_with_key::<NoState>(&handle_key)
            .deps([drag_index.signal_id()])
            .on_state(move |_ctx| {
                let is_dragging = drag_index_for_visual.get() == idx as i32;

                let mut handle_visual = div();

                match direction {
                    ResizeDirection::Horizontal => {
                        handle_visual = handle_visual
                            .w(thickness)
                            .h_full()
                            .cursor(CursorStyle::ResizeEW);
                    }
                    ResizeDirection::Vertical => {
                        handle_visual = handle_visual
                            .w_full()
                            .h(thickness)
                            .cursor(CursorStyle::ResizeNS);
                    }
                }

                if is_dragging {
                    handle_visual = handle_visual.bg(primary_color);
                } else {
                    handle_visual = handle_visual.bg(border_color);
                }

                // Wrap in hit area container
                let mut hit_area = div()
                    .class("cn-resizable-handle")
                    .items_center()
                    .justify_center();

                match direction {
                    ResizeDirection::Horizontal => {
                        hit_area = hit_area
                            .w(total_hit_area)
                            .h_full()
                            .cursor(CursorStyle::ResizeEW);
                    }
                    ResizeDirection::Vertical => {
                        hit_area = hit_area
                            .w_full()
                            .h(total_hit_area)
                            .cursor(CursorStyle::ResizeNS);
                    }
                }

                hit_area.child(handle_visual)
            })
            .on_mouse_down(move |event| {
                // Start drag
                drag_index_for_down.set(idx as i32);

                // Store start position
                let pos = match direction {
                    ResizeDirection::Horizontal => event.mouse_x,
                    ResizeDirection::Vertical => event.mouse_y,
                };
                drag_start_pos_for_down.set(pos);

                // Store current sizes
                let sizes: Vec<f32> = panel_sizes_for_down.iter().map(|s| s.get()).collect();
                drag_start_sizes_for_down.set(sizes);
            })
            .on_drag(move |event| {
                let current_idx = drag_index_for_drag.get();
                if current_idx < 0 {
                    return;
                }

                let pos = match direction {
                    ResizeDirection::Horizontal => event.mouse_x,
                    ResizeDirection::Vertical => event.mouse_y,
                };

                let start_pos = drag_start_pos_for_drag.get();
                let delta = pos - start_pos;

                let start_sizes = drag_start_sizes_for_drag.get();
                if start_sizes.len() <= idx + 1 {
                    return;
                }

                // Calculate new sizes
                let left_start = start_sizes[idx];
                let right_start = start_sizes[idx + 1];

                let mut new_left = left_start + delta;
                let mut new_right = right_start - delta;

                // Apply constraints
                new_left = new_left.max(left_min);
                if let Some(max) = left_max {
                    new_left = new_left.min(max);
                }

                new_right = new_right.max(right_min);
                if let Some(max) = right_max {
                    new_right = new_right.min(max);
                }

                // Ensure total stays constant
                let total = left_start + right_start;
                if new_left + new_right > total {
                    // Adjust to fit
                    let overflow = (new_left + new_right) - total;
                    if delta > 0.0 {
                        new_right = (new_right - overflow).max(right_min);
                        new_left = total - new_right;
                    } else {
                        new_left = (new_left - overflow).max(left_min);
                        new_right = total - new_left;
                    }
                }

                // Update sizes
                panel_sizes_for_drag[idx].set(new_left);
                panel_sizes_for_drag[idx + 1].set(new_right);
            })
            .on_drag_end(move |_event| {
                drag_index_for_end.set(-1);
            });

        div().child(handle)
    }
}

impl ElementBuilder for ResizableGroup {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.inner.build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.inner.children_builders()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.inner.element_classes()
    }

    fn element_id(&self) -> Option<&str> {
        self.inner.element_id()
    }
}

/// Builder for resizable group
pub struct ResizableGroupBuilder {
    config: ResizableGroupConfig,
    #[allow(dead_code)]
    key: InstanceKey,
    built: OnceCell<ResizableGroup>,
}

impl ResizableGroupBuilder {
    /// Create a new resizable group builder
    #[track_caller]
    pub fn new() -> Self {
        Self {
            config: ResizableGroupConfig::default(),
            key: InstanceKey::new("resizable_group"),
            built: OnceCell::new(),
        }
    }

    /// Set the resize direction
    pub fn direction(mut self, direction: ResizeDirection) -> Self {
        self.config.direction = direction;
        self
    }

    /// Set horizontal direction (default)
    pub fn horizontal(mut self) -> Self {
        self.config.direction = ResizeDirection::Horizontal;
        self
    }

    /// Set vertical direction
    pub fn vertical(mut self) -> Self {
        self.config.direction = ResizeDirection::Vertical;
        self
    }

    /// Configure handle appearance
    pub fn handle_thickness(mut self, thickness: f32) -> Self {
        self.config.handle.thickness = thickness;
        self
    }

    /// Set a unique key for state persistence
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.config.key = key.into();
        self
    }

    /// Add a panel to the group
    pub fn panel(mut self, panel: ResizablePanelBuilder) -> Self {
        self.config.panels.push(panel.build_config());
        self
    }

    /// Set callback when panel sizes change
    pub fn on_resize<F>(mut self, callback: F) -> Self
    where
        F: Fn(&[f32]) + Send + Sync + 'static,
    {
        self.config.on_resize = Some(Arc::new(callback));
        self
    }

    /// Add a CSS class for selector matching
    pub fn class(mut self, name: impl AsRef<str>) -> Self {
        self.config
            .classes
            .push(blinc_core::intern::intern(name.as_ref()));
        self
    }

    /// Set the element ID for CSS selector matching
    pub fn id(mut self, id: &str) -> Self {
        self.config.user_id = Some(id.to_string());
        self
    }

    fn get_or_build(&self) -> &ResizableGroup {
        ::blinc_layout::build_once::build_once(&self.built, || {
            // We need to take ownership of config, but can't mutate self
            // This is a limitation - we'll clone what we can
            let config = ResizableGroupConfig {
                direction: self.config.direction,
                handle: self.config.handle.clone(),
                panels: Vec::new(), // Empty - panels already consumed
                key: self.config.key.clone(),
                on_resize: self.config.on_resize.clone(),
                classes: self.config.classes.clone(),
                user_id: self.config.user_id.clone(),
            };
            ResizableGroup::from_config(config)
        })
    }

    /// Build and consume the builder, returning the group
    pub fn build(self) -> ResizableGroup {
        ResizableGroup::from_config(self.config)
    }
}

impl Default for ResizableGroupBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ElementBuilder for ResizableGroupBuilder {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        // For ElementBuilder impl, we need to build without consuming
        // This means panels won't work through this path - use build() instead
        self.get_or_build().inner.build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.get_or_build().inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().inner.children_builders()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.get_or_build().inner.element_classes()
    }

    fn element_id(&self) -> Option<&str> {
        self.get_or_build().inner.element_id()
    }
}

/// Create a new resizable group builder
#[track_caller]
pub fn resizable_group() -> ResizableGroupBuilder {
    ResizableGroupBuilder::new()
}

/// Create a new resizable panel builder
pub fn resizable_panel() -> ResizablePanelBuilder {
    ResizablePanelBuilder::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resize_direction_default() {
        assert_eq!(ResizeDirection::default(), ResizeDirection::Horizontal);
    }

    #[test]
    fn test_panel_builder_defaults() {
        let config = resizable_panel().build_config();
        assert_eq!(config.min_size, 50.0);
        assert_eq!(config.max_size, None);
        assert!(!config.is_flex);
    }

    #[test]
    fn test_panel_builder_with_size() {
        let config = resizable_panel()
            .default_size(300.0)
            .min_size(100.0)
            .max_size(500.0)
            .build_config();

        assert_eq!(config.default_size, Some(300.0));
        assert_eq!(config.min_size, 100.0);
        assert_eq!(config.max_size, Some(500.0));
    }

    #[test]
    fn test_panel_builder_flex() {
        let config = resizable_panel().flex_grow().build_config();
        assert!(config.is_flex);
        assert_eq!(config.flex_grow, 1.0);
    }
}
