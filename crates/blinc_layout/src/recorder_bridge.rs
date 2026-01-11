//! Bridge module for recorder integration.
//!
//! This module provides helpers for sending event data to blinc_recorder
//! via BlincContextState callbacks, avoiding circular dependencies.

use blinc_core::BlincContextState;
use std::any::Any;

/// Mouse button for recorder events
#[derive(Clone, Copy, Debug)]
pub enum RecorderMouseButton {
    Left,
    Right,
    Middle,
    Other(u8),
}

impl From<crate::event_router::MouseButton> for RecorderMouseButton {
    fn from(btn: crate::event_router::MouseButton) -> Self {
        match btn {
            crate::event_router::MouseButton::Left => RecorderMouseButton::Left,
            crate::event_router::MouseButton::Right => RecorderMouseButton::Right,
            crate::event_router::MouseButton::Middle => RecorderMouseButton::Middle,
            crate::event_router::MouseButton::Back => RecorderMouseButton::Other(3),
            crate::event_router::MouseButton::Forward => RecorderMouseButton::Other(4),
            crate::event_router::MouseButton::Other(n) => RecorderMouseButton::Other(n as u8),
        }
    }
}

/// Event data sent to recorder
#[derive(Clone, Debug)]
pub enum RecorderEventData {
    MouseDown {
        x: f32,
        y: f32,
        button: RecorderMouseButton,
        target_element: Option<String>,
    },
    MouseUp {
        x: f32,
        y: f32,
        button: RecorderMouseButton,
        target_element: Option<String>,
    },
    MouseMove {
        x: f32,
        y: f32,
        hover_element: Option<String>,
    },
    Click {
        x: f32,
        y: f32,
        button: RecorderMouseButton,
        target_element: Option<String>,
    },
    KeyDown {
        key_code: u32,
        focused_element: Option<String>,
    },
    KeyUp {
        key_code: u32,
        focused_element: Option<String>,
    },
    TextInput {
        text: String,
        focused_element: Option<String>,
    },
    Scroll {
        x: f32,
        y: f32,
        delta_x: f32,
        delta_y: f32,
        target_element: Option<String>,
    },
    FocusChange {
        from: Option<String>,
        to: Option<String>,
    },
    HoverEnter {
        element_id: String,
        x: f32,
        y: f32,
    },
    HoverLeave {
        element_id: String,
        x: f32,
        y: f32,
    },
}

/// Send an event to the recorder if recording is enabled.
///
/// This is a no-op if BlincContextState is not initialized or no recorder callback is set.
pub fn record_event(event: RecorderEventData) {
    if let Some(ctx) = BlincContextState::try_get() {
        if ctx.is_recording_events() {
            ctx.record_event(Box::new(event) as Box<dyn Any + Send>);
        }
    }
}

/// Check if event recording is currently enabled.
pub fn is_recording() -> bool {
    BlincContextState::try_get()
        .map(|ctx| ctx.is_recording_events())
        .unwrap_or(false)
}

/// Check if snapshot recording is currently enabled.
pub fn is_recording_snapshots() -> bool {
    BlincContextState::try_get()
        .map(|ctx| ctx.is_recording_snapshots())
        .unwrap_or(false)
}

// ============================================================================
// Tree Snapshot Types
// ============================================================================

/// Rectangle for snapshot bounds.
#[derive(Clone, Debug)]
pub struct SnapshotRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl SnapshotRect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

/// Visual properties for element snapshots.
#[derive(Clone, Debug, Default)]
pub struct SnapshotVisualProps {
    pub background_color: Option<[f32; 4]>,
    pub border_color: Option<[f32; 4]>,
    pub border_width: Option<f32>,
    pub border_radius: Option<f32>,
    pub opacity: Option<f32>,
}

/// Snapshot of a single element.
#[derive(Clone, Debug)]
pub struct ElementSnapshotData {
    pub id: String,
    pub element_type: String,
    pub bounds: SnapshotRect,
    pub is_visible: bool,
    pub is_focused: bool,
    pub is_hovered: bool,
    pub is_interactive: bool,
    pub children: Vec<String>,
    pub parent: Option<String>,
    pub visual_props: Option<SnapshotVisualProps>,
    pub text_content: Option<String>,
}

/// Complete tree snapshot.
#[derive(Clone, Debug)]
pub struct TreeSnapshotData {
    pub elements: std::collections::HashMap<String, ElementSnapshotData>,
    pub root_id: Option<String>,
    pub focused_element: Option<String>,
    pub hovered_element: Option<String>,
    pub window_size: (u32, u32),
    pub scale_factor: f64,
}

impl TreeSnapshotData {
    /// Create an empty tree snapshot.
    pub fn new(window_size: (u32, u32), scale_factor: f64) -> Self {
        Self {
            elements: std::collections::HashMap::new(),
            root_id: None,
            focused_element: None,
            hovered_element: None,
            window_size,
            scale_factor,
        }
    }
}

/// Send a tree snapshot to the recorder if recording is enabled.
///
/// This is a no-op if BlincContextState is not initialized or no recorder callback is set.
pub fn record_snapshot(snapshot: TreeSnapshotData) {
    if let Some(ctx) = BlincContextState::try_get() {
        if ctx.is_recording_snapshots() {
            ctx.record_snapshot(Box::new(snapshot) as Box<dyn Any + Send>);
        }
    }
}

/// Capture a tree snapshot from a RenderTree.
///
/// This walks the render tree and captures the current state of all elements.
/// The snapshot can then be sent to the recorder via `record_snapshot`.
pub fn capture_tree_snapshot(
    tree: &crate::renderer::RenderTree,
    focused_node: Option<crate::tree::LayoutNodeId>,
    hovered_nodes: &std::collections::HashSet<crate::tree::LayoutNodeId>,
    window_width: u32,
    window_height: u32,
) -> TreeSnapshotData {
    let scale_factor = tree.scale_factor() as f64;
    let mut snapshot = TreeSnapshotData::new((window_width, window_height), scale_factor);

    if let Some(root) = tree.root() {
        snapshot.root_id = Some(format!("{:?}", root));
        capture_node_recursive(
            tree,
            root,
            None,
            focused_node,
            hovered_nodes,
            &mut snapshot,
        );
    }

    snapshot.focused_element = focused_node.map(|n| format!("{:?}", n));

    snapshot
}

/// Recursively capture a node and its children.
fn capture_node_recursive(
    tree: &crate::renderer::RenderTree,
    node: crate::tree::LayoutNodeId,
    parent: Option<crate::tree::LayoutNodeId>,
    focused_node: Option<crate::tree::LayoutNodeId>,
    hovered_nodes: &std::collections::HashSet<crate::tree::LayoutNodeId>,
    snapshot: &mut TreeSnapshotData,
) {
    let node_id_str = format!("{:?}", node);

    // Get bounds
    let bounds = tree
        .layout()
        .get_bounds(node, (0.0, 0.0))
        .map(|b| SnapshotRect::new(b.x, b.y, b.width, b.height))
        .unwrap_or_else(|| SnapshotRect::new(0.0, 0.0, 0.0, 0.0));

    // Get render node for element type and visual props
    let render_node = tree.get_render_node(node);
    let element_type = render_node
        .map(|n| match &n.element_type {
            crate::renderer::ElementType::Div => "Div".to_string(),
            crate::renderer::ElementType::Text(t) => format!("Text({})", t.content.len()),
            crate::renderer::ElementType::StyledText(_) => "StyledText".to_string(),
            crate::renderer::ElementType::Svg(_) => "Svg".to_string(),
            crate::renderer::ElementType::Image(_) => "Image".to_string(),
            crate::renderer::ElementType::Canvas(_) => "Canvas".to_string(),
        })
        .unwrap_or_else(|| "Unknown".to_string());

    // Extract visual props
    let visual_props = render_node.map(|n| {
        let props = &n.props;
        SnapshotVisualProps {
            background_color: props.background.as_ref().and_then(|brush| {
                if let blinc_core::Brush::Solid(c) = brush {
                    Some(c.to_array())
                } else {
                    None
                }
            }),
            border_color: props.border_color.map(|c| c.to_array()),
            border_width: if props.border_width > 0.0 {
                Some(props.border_width)
            } else {
                None
            },
            border_radius: Some(props.border_radius.top_left),
            opacity: Some(props.opacity),
        }
    });

    // Extract text content if available
    let text_content = render_node.and_then(|n| match &n.element_type {
        crate::renderer::ElementType::Text(t) => Some(t.content.clone()),
        _ => None,
    });

    // Get children
    let children = tree.layout().children(node);
    let child_ids: Vec<String> = children.iter().map(|c| format!("{:?}", c)).collect();

    // Simplified check - a more thorough check would need handler registry access
    let is_interactive = render_node.is_some();

    let elem = ElementSnapshotData {
        id: node_id_str.clone(),
        element_type,
        bounds,
        is_visible: true, // Could check opacity/display later
        is_focused: focused_node == Some(node),
        is_hovered: hovered_nodes.contains(&node),
        is_interactive,
        children: child_ids,
        parent: parent.map(|p| format!("{:?}", p)),
        visual_props,
        text_content,
    };

    snapshot.elements.insert(node_id_str, elem);

    // Update hovered element in snapshot if this node is hovered
    if hovered_nodes.contains(&node) {
        snapshot.hovered_element = Some(format!("{:?}", node));
    }

    // Recurse into children
    for child in children {
        capture_node_recursive(tree, child, Some(node), focused_node, hovered_nodes, snapshot);
    }
}
