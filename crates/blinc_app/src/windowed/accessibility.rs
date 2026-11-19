use blinc_layout::{
    accessibility::{
        AccessibilityAction, AccessibilityActions, AccessibilityLive, AccessibilityNode,
        AccessibilityRole,
    },
    tree::StableNodeId,
};
use blinc_platform_desktop::accesskit::{
    self as ak, Action, ActionData, Node, NodeId, Role, TreeId, TreeInfo, TreeUpdate,
};
use std::collections::HashMap;

const ROOT: NodeId = NodeId(0);

#[derive(Default)]
pub(crate) struct AccessibilityCache {
    generation: u64,
    nodes: HashMap<NodeId, Node>,
    focus: Option<NodeId>,
}

impl AccessibilityCache {
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn update(
        &mut self,
        semantics: &[AccessibilityNode],
        generation: u64,
        title: &str,
        scale: f64,
        size: (u32, u32),
    ) -> Option<TreeUpdate> {
        let full = self.generation != generation || self.nodes.is_empty();
        let mut children: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
        for n in semantics {
            children
                .entry(n.parent.map_or(ROOT, |id| NodeId(id.to_raw())))
                .or_default()
                .push(NodeId(n.id.to_raw()));
        }
        let mut current = HashMap::with_capacity(semantics.len() + 1);
        let mut root = Node::new(Role::Window);
        root.set_label(title);
        root.set_bounds(ak::Rect::new(0.0, 0.0, size.0 as f64, size.1 as f64));
        root.set_children(children.remove(&ROOT).unwrap_or_default());
        current.insert(ROOT, root);
        let mut focus = ROOT;
        let atomic_messages: std::collections::HashSet<_> = semantics
            .iter()
            .filter(|n| {
                matches!(n.role, AccessibilityRole::Status | AccessibilityRole::Alert)
                    && n.live.atomic
                    && n.live.root == Some(n.id)
            })
            .map(|n| n.id)
            .collect();
        for semantic in semantics {
            let id = NodeId(semantic.id.to_raw());
            if semantic.focused {
                focus = id;
            }
            let mut node = native_node(semantic, scale);
            if semantic.live.atomic
                && semantic
                    .live
                    .root
                    .is_some_and(|id| id != semantic.id && atomic_messages.contains(&id))
            {
                node.set_live(ak::Live::Off);
            }
            if !semantic.live.atomic
                && matches!(
                    semantic.role,
                    AccessibilityRole::Status | AccessibilityRole::Alert
                )
                && semantic.live.root == Some(semantic.id)
                && children.contains_key(&id)
            {
                node.set_live(ak::Live::Off);
            }
            if let Some(children) = children.remove(&id) {
                node.set_children(children);
            }
            current.insert(id, node);
        }
        let updated: Vec<_> = current
            .iter()
            .filter(|(id, node)| full || self.nodes.get(id) != Some(node))
            .map(|(&id, node)| (id, node.clone()))
            .collect();
        let changed = !updated.is_empty() || self.focus != Some(focus);
        self.nodes = current;
        self.focus = Some(focus);
        self.generation = generation;
        changed.then(|| TreeUpdate {
            nodes: updated,
            tree: full.then(|| TreeInfo::new(ROOT)),
            tree_id: TreeId::ROOT,
            focus,
        })
    }
}

fn native_role(role: AccessibilityRole, secure: bool) -> Role {
    use AccessibilityRole as R;
    match role {
        R::Custom(_) => Role::Section,
        R::Text => Role::Label,
        R::Heading => Role::Heading,
        R::Button => Role::Button,
        R::Link => Role::Link,
        R::TextField if secure => Role::PasswordInput,
        R::TextField => Role::TextInput,
        R::Checkbox => Role::CheckBox,
        R::Switch => Role::Switch,
        R::Radio => Role::RadioButton,
        R::Slider => Role::Slider,
        R::Tab => Role::Tab,
        R::MenuItem => Role::MenuItem,
        R::MenuItemCheckbox => Role::MenuItemCheckBox,
        R::MenuItemRadio => Role::MenuItemRadio,
        R::MenuBar => Role::MenuBar,
        R::Menu => Role::Menu,
        R::Separator => Role::Splitter,
        R::Tree => Role::Tree,
        R::TreeItem => Role::TreeItem,
        R::Toolbar => Role::Toolbar,
        R::Tooltip => Role::Tooltip,
        R::Dialog => Role::Dialog,
        R::AlertDialog => Role::AlertDialog,
        R::Status => Role::Status,
        R::Alert => Role::Alert,
        R::Region => Role::Region,
        R::Image => Role::Image,
    }
}

fn native_node(n: &AccessibilityNode, scale: f64) -> Node {
    let mut node = Node::new(native_role(n.role, n.secure));
    if let AccessibilityRole::Custom(name) = n.role {
        node.set_role_description(name);
    }
    if !n.label.is_empty() {
        if n.role == AccessibilityRole::Text {
            node.set_value(n.label.as_str());
        } else {
            node.set_label(n.label.as_str());
        }
    }
    if !n.value.is_empty() {
        node.set_value(n.value.as_str());
        if n.role == AccessibilityRole::Slider
            && let Ok(value) = n.value.parse::<f64>()
            && value.is_finite()
        {
            node.set_numeric_value(value);
        }
    }
    if !n.hint.is_empty() {
        node.set_description(n.hint.as_str());
    }
    if !n.placeholder.is_empty() {
        node.set_placeholder(n.placeholder.as_str());
    }
    if let Some(id) = &n.identifier {
        node.set_author_id(id.as_str());
    }
    let b = n.bounds;
    node.set_bounds(ak::Rect::new(
        b.x as f64 * scale,
        b.y as f64 * scale,
        (b.x + b.width) as f64 * scale,
        (b.y + b.height) as f64 * scale,
    ));
    if n.disabled {
        node.set_disabled();
    }
    if n.selected || matches!(n.role, AccessibilityRole::Tab | AccessibilityRole::TreeItem) {
        node.set_selected(n.selected);
    }
    if let Some(checked) = n.checked {
        node.set_toggled(if checked {
            ak::Toggled::True
        } else {
            ak::Toggled::False
        });
    }
    if let Some(expanded) = n.expanded {
        node.set_expanded(expanded);
    }
    node.set_live(
        match if n.live.busy {
            AccessibilityLive::Off
        } else {
            n.live.priority
        } {
            AccessibilityLive::Off => ak::Live::Off,
            AccessibilityLive::Polite => ak::Live::Polite,
            AccessibilityLive::Assertive => ak::Live::Assertive,
        },
    );
    if n.live.atomic {
        node.set_live_atomic();
    }
    if n.live.busy {
        node.set_busy();
    }
    if n.modal {
        node.set_modal();
    }
    if !n.disabled {
        if n.actions.contains(AccessibilityActions::FOCUS) {
            node.add_action(Action::Focus);
        }
        if n.actions.contains(AccessibilityActions::ACTIVATE) {
            node.add_action(Action::Click);
        }
        if n.actions.contains(AccessibilityActions::SET_VALUE) {
            node.add_action(Action::SetValue);
        }
        if n.actions.contains(AccessibilityActions::INCREMENT) {
            node.add_action(Action::Increment);
        }
        if n.actions.contains(AccessibilityActions::DECREMENT) {
            node.add_action(Action::Decrement);
        }
    }
    node
}

fn action(request: &ak::ActionRequest) -> Option<(StableNodeId, AccessibilityAction)> {
    if request.target_tree != TreeId::ROOT || request.target_node == ROOT {
        return None;
    }
    let action = match request.action {
        Action::Click => AccessibilityAction::Activate,
        Action::Focus => AccessibilityAction::Focus,
        Action::Increment => AccessibilityAction::Increment,
        Action::Decrement => AccessibilityAction::Decrement,
        Action::SetValue => match &request.data {
            Some(ActionData::Value(value)) => AccessibilityAction::SetValue(value.to_string()),
            Some(ActionData::NumericValue(value)) if value.is_finite() => {
                AccessibilityAction::SetValue(value.to_string())
            }
            _ => return None,
        },
        _ => return None,
    };
    Some((StableNodeId::from_raw(request.target_node.0), action))
}

impl<F, E> super::event_loop::DesktopLoop<F, E>
where
    F: FnMut(&mut super::WindowedContext) -> E + 'static,
    E: crate::ElementBuilder + 'static,
{
    pub(super) fn dispatch(
        &mut self,
        event: blinc_platform::Event,
        window: &blinc_platform_desktop::DesktopWindow,
    ) -> blinc_platform::ControlFlow {
        use blinc_platform::Window;
        let frame = matches!(event, blinc_platform::Event::Frame(_));
        let request = window.take_accessibility_request();
        let requested = request.is_some();
        if let Some(request) = request {
            let ws = if self.primary_wid == Some(window.id()) {
                Some(&mut self.ws)
            } else {
                self.secondary_windows.get_mut(&window.id())
            };
            if let Some(ws) = ws {
                if !request.active {
                    ws.accessibility.reset();
                }
                if let (true, Some(tree), Some(ctx)) =
                    (request.active, &mut ws.render_tree, &mut ws.ctx)
                {
                    for request in &request.actions {
                        if let Some((id, action)) = action(request) {
                            tree.perform_accessibility_action_with_router(
                                &mut ctx.event_router,
                                id,
                                action,
                            );
                        }
                    }
                }
            }
            self.frame_dirty
                .store(true, std::sync::atomic::Ordering::Release);
            window.request_redraw();
        }
        let flow = if frame && window.is_occluded() {
            blinc_platform::ControlFlow::Continue
        } else {
            self.handle(event, window)
        };
        if (frame || requested) && window.accessibility_active() {
            let ws = if self.primary_wid == Some(window.id()) {
                Some(&mut self.ws)
            } else {
                self.secondary_windows.get_mut(&window.id())
            };
            if let Some(ws) = ws
                && let (Some(tree), Some(ctx)) = (&mut ws.render_tree, &mut ws.ctx)
            {
                tree.sync_keyboard_focus(&mut ctx.event_router);
                let nodes = tree.accessibility_nodes();
                if let Some(update) = ws.accessibility.update(
                    &nodes,
                    window.accessibility_generation(),
                    &window.winit_window().title(),
                    window.scale_factor(),
                    window.size(),
                ) {
                    window.update_accessibility(|| update);
                }
            }
        }
        flow
    }
}

#[cfg(test)]
mod tests;
