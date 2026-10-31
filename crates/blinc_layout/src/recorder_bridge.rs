//! Bridge module for recorder integration.
//!
//! Sends what the layout sees (pointer and key events, the element tree) to
//! whatever recorder is installed on [`BlincContextState`], in the types
//! `blinc_core::recording` defines.

use blinc_core::BlincContextState;
use blinc_core::recording::{
    ElementSnapshot, FocusChangeEvent, HoverEvent, Key, KeyEvent, Modifiers, MouseEvent,
    MouseMoveEvent, Point, RecordedEvent, Rect, ScrollEvent, TextInputEvent, Timestamp,
    TreeSnapshot, VisualProps,
};

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
            ctx.record_event(event.into());
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

/// Check if update recording is currently enabled.
pub fn is_recording_updates() -> bool {
    BlincContextState::try_get()
        .map(|ctx| ctx.is_recording_updates())
        .unwrap_or(false)
}

/// Record an element update with category.
///
/// This is called when diff/stateful detects element changes.
pub fn record_update(element_id: &str, category: blinc_core::UpdateCategory) {
    if let Some(ctx) = BlincContextState::try_get() {
        if ctx.is_recording_updates() {
            ctx.record_update(element_id, category);
        }
    }
}

/// Convert ChangeCategory from diff module to UpdateCategory for recording.
pub fn change_category_to_update(
    change: &crate::diff::ChangeCategory,
) -> blinc_core::UpdateCategory {
    if change.children {
        blinc_core::UpdateCategory::Structural
    } else if change.layout {
        blinc_core::UpdateCategory::Layout
    } else {
        blinc_core::UpdateCategory::Visual
    }
}

impl From<RecorderMouseButton> for blinc_core::recording::MouseButton {
    fn from(button: RecorderMouseButton) -> Self {
        match button {
            RecorderMouseButton::Left => Self::Left,
            RecorderMouseButton::Right => Self::Right,
            RecorderMouseButton::Middle => Self::Middle,
            RecorderMouseButton::Other(n) => Self::Other(n),
        }
    }
}

impl From<RecorderEventData> for RecordedEvent {
    fn from(event: RecorderEventData) -> Self {
        let mouse = |x, y, button: RecorderMouseButton, target_element| MouseEvent {
            position: Point::new(x, y),
            button: button.into(),
            modifiers: Modifiers::none(),
            target_element,
        };
        match event {
            RecorderEventData::MouseDown {
                x,
                y,
                button,
                target_element,
            } => RecordedEvent::MouseDown(mouse(x, y, button, target_element)),
            RecorderEventData::MouseUp {
                x,
                y,
                button,
                target_element,
            } => RecordedEvent::MouseUp(mouse(x, y, button, target_element)),
            RecorderEventData::Click {
                x,
                y,
                button,
                target_element,
            } => RecordedEvent::Click(mouse(x, y, button, target_element)),
            RecorderEventData::MouseMove {
                x,
                y,
                hover_element,
            } => RecordedEvent::MouseMove(MouseMoveEvent {
                position: Point::new(x, y),
                hover_element,
            }),
            RecorderEventData::KeyDown {
                key_code,
                focused_element,
            } => RecordedEvent::KeyDown(KeyEvent {
                key: Key::Other(key_code),
                modifiers: Modifiers::none(),
                is_repeat: false,
                focused_element,
            }),
            RecorderEventData::KeyUp {
                key_code,
                focused_element,
            } => RecordedEvent::KeyUp(KeyEvent {
                key: Key::Other(key_code),
                modifiers: Modifiers::none(),
                is_repeat: false,
                focused_element,
            }),
            RecorderEventData::TextInput {
                text,
                focused_element,
            } => RecordedEvent::TextInput(TextInputEvent {
                text,
                focused_element,
            }),
            RecorderEventData::Scroll {
                x,
                y,
                delta_x,
                delta_y,
                target_element,
            } => RecordedEvent::Scroll(ScrollEvent {
                position: Point::new(x, y),
                delta_x,
                delta_y,
                target_element,
            }),
            RecorderEventData::FocusChange { from, to } => {
                RecordedEvent::FocusChange(FocusChangeEvent { from, to })
            }
            RecorderEventData::HoverEnter { element_id, x, y } => {
                RecordedEvent::HoverEnter(HoverEvent {
                    element_id,
                    position: Point::new(x, y),
                })
            }
            RecorderEventData::HoverLeave { element_id, x, y } => {
                RecordedEvent::HoverLeave(HoverEvent {
                    element_id,
                    position: Point::new(x, y),
                })
            }
        }
    }
}

/// Send a tree snapshot to the recorder if recording is enabled.
///
/// This is a no-op if BlincContextState is not initialized or no recorder callback is set.
pub fn record_snapshot(snapshot: TreeSnapshot) {
    if let Some(ctx) = BlincContextState::try_get() {
        if ctx.is_recording_snapshots() {
            ctx.record_snapshot(snapshot);
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
) -> TreeSnapshot {
    let scale_factor = tree.scale_factor() as f64;
    let mut snapshot = TreeSnapshot::new(
        Timestamp::from_micros(0),
        (window_width, window_height),
        scale_factor,
    );

    if let Some(root) = tree.root() {
        snapshot.root_id = Some(format!("{:?}", root));
        capture_node_recursive(tree, root, None, focused_node, hovered_nodes, &mut snapshot);
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
    snapshot: &mut TreeSnapshot,
) {
    let node_id_str = format!("{:?}", node);

    // Get bounds
    let bounds = tree
        .layout()
        .get_bounds(node, (0.0, 0.0))
        .map(|b| Rect::new(b.x, b.y, b.width, b.height))
        .unwrap_or_else(|| Rect::new(0.0, 0.0, 0.0, 0.0));

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
        VisualProps {
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
            ..Default::default()
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

    let elem = ElementSnapshot {
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
        capture_node_recursive(
            tree,
            child,
            Some(node),
            focused_node,
            hovered_nodes,
            snapshot,
        );
    }
}

/// Record the tree as it stands this frame, if a recorder wants snapshots.
///
/// Runners call this once per frame, after styles and layout are settled.
pub fn record_frame(
    tree: &crate::renderer::RenderTree,
    router: &crate::event_router::EventRouter,
    window_size: (u32, u32),
) {
    if !is_recording_snapshots() {
        return;
    }
    let hovered: std::collections::HashSet<_> = router
        .hovered_nodes()
        .filter_map(|stable| tree.layout_id(stable))
        .collect();
    record_snapshot(capture_tree_snapshot(
        tree,
        router.focused(),
        &hovered,
        window_size.0,
        window_size.1,
    ));
}
