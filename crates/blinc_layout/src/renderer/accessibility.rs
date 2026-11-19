use super::{ElementType, RenderTree};
use crate::accessibility::{
    AccessibilityAction, AccessibilityActions, AccessibilityLive, AccessibilityLiveRegion,
    AccessibilityNode, AccessibilityRole,
};
use crate::{
    element::ElementBounds,
    tree::{LayoutNodeId, StableNodeId},
};

impl RenderTree {
    pub(crate) fn register_accessibility<E: crate::div::ElementBuilder + ?Sized>(
        &mut self,
        node: LayoutNodeId,
        element: &E,
    ) {
        if let Some(binding) = element.accessibility_binding() {
            self.accessibility_bindings.insert(node, binding);
        } else {
            self.accessibility_bindings.remove(&node);
        }
        if let Some(semantics) = element.accessibility_semantics() {
            self.accessibility_semantics.insert(node, semantics);
        } else {
            self.accessibility_semantics.remove(&node);
        }
    }
    /// Accessible elements in reading order, excluding hidden and clipped content.
    pub fn accessibility_nodes(&self) -> Vec<AccessibilityNode> {
        let mut result = Vec::new();
        let Some(root) = self
            .active_focus_scopes()
            .last()
            .map(|(_, node)| *node)
            .or_else(|| self.root())
        else {
            return result;
        };
        let mut pending = vec![(root, None, AccessibilityLiveRegion::default())];
        while let Some((node, parent, inherited_live)) = pending.pop() {
            if self.accessibility_hidden(node) {
                continue;
            }
            let bounds = self.accessibility_bounds(node);
            if bounds.is_none()
                && self.layout_tree.get_style(node).is_some_and(|style| {
                    style.overflow.x != taffy::Overflow::Visible
                        && style.overflow.y != taffy::Overflow::Visible
                })
            {
                continue;
            }
            let role = self.accessibility_role(node);
            let live_region = self.accessibility_live_region(node, role, inherited_live);
            let mut child_parent = parent;
            if let (Some(role), Some(bounds)) = (role, bounds) {
                let attr = |name| self.element_registry.attribute(node, name);
                let live = self.accessibility_bindings.get(&node).map(|b| (b.read)());
                let label = attr("aria-label")
                    .or_else(|| {
                        attr("aria-labelledby").map(|ids| {
                            ids.split_whitespace()
                                .filter_map(|id| self.query_by_id(id))
                                .map(|n| self.accessibility_label_text(n, true))
                                .collect::<Vec<_>>()
                                .join(" ")
                        })
                    })
                    .or_else(|| live.as_ref().and_then(|v| v.label.clone()))
                    .unwrap_or_else(|| {
                        if self.accessibility_has_children(node, role)
                            && !matches!(
                                role,
                                AccessibilityRole::TreeItem
                                    | AccessibilityRole::Status
                                    | AccessibilityRole::Alert
                            )
                        {
                            String::new()
                        } else {
                            self.accessibility_text(node)
                        }
                    });
                let checked = attr("aria-checked").map(|v| v == "true").or_else(|| {
                    matches!(
                        role,
                        AccessibilityRole::Checkbox
                            | AccessibilityRole::Switch
                            | AccessibilityRole::Radio
                    )
                    .then(|| {
                        self.accessibility_descendants(node)
                            .iter()
                            .any(|&n| self.element_registry.attribute(n, "checked").is_some())
                    })
                });
                let value = live
                    .as_ref()
                    .map(|v| v.value.clone())
                    .or_else(|| attr("aria-valuetext"))
                    .or_else(|| attr("aria-valuenow"))
                    .unwrap_or_default();
                if let Some(id) = self.stable_id(node) {
                    result.push(AccessibilityNode {
                        id,
                        parent,
                        identifier: self.element_registry.get_id(node),
                        role,
                        label,
                        value,
                        hint: attr("aria-description")
                            .or_else(|| {
                                attr("aria-describedby").map(|ids| {
                                    ids.split_whitespace()
                                        .filter_map(|id| self.query_by_id(id))
                                        .map(|n| self.accessibility_label_text(n, true))
                                        .collect::<Vec<_>>()
                                        .join(" ")
                                })
                            })
                            .unwrap_or_default(),
                        placeholder: live
                            .as_ref()
                            .map(|v| v.placeholder.clone())
                            .unwrap_or_default(),
                        bounds,
                        disabled: std::iter::once(node)
                            .chain(self.layout_tree.ancestors(node))
                            .any(|n| {
                                self.element_registry.attribute(n, "disabled").is_some()
                                    || self
                                        .element_registry
                                        .attribute(n, "aria-disabled")
                                        .as_deref()
                                        == Some("true")
                            }),
                        selected: attr("aria-selected").as_deref() == Some("true")
                            || matches!(attr("data-selected").as_deref(), Some("" | "true")),
                        checked,
                        expanded: attr("aria-expanded").map(|v| v == "true"),
                        secure: live.as_ref().is_some_and(|v| v.secure),
                        focused: live.as_ref().is_some_and(|v| v.focused)
                            || self.accessibility_focus == Some(id),
                        focusable: self.is_focusable(node),
                        modal: attr("aria-modal").as_deref() == Some("true"),
                        selection: live.as_ref().and_then(|v| v.selection),
                        live: live_region,
                        actions: self.accessibility_actions(node, role),
                    });
                    child_parent = Some(id);
                }
                if !self.accessibility_has_children(node, role) {
                    continue;
                }
            }
            pending.extend(
                self.layout_tree
                    .children(node)
                    .into_iter()
                    .rev()
                    .map(|child| (child, child_parent, live_region)),
            );
        }
        result
    }

    fn accessibility_has_children(&self, node: LayoutNodeId, role: AccessibilityRole) -> bool {
        self.accessibility_semantics
            .get(&node)
            .map_or_else(|| role.has_children(), |s| s.children)
    }

    fn accessibility_actions(
        &self,
        node: LayoutNodeId,
        role: AccessibilityRole,
    ) -> AccessibilityActions {
        if let Some(actions) = self
            .accessibility_semantics
            .get(&node)
            .and_then(|s| s.actions)
        {
            return actions;
        }
        use AccessibilityActions as A;
        let actions = match role {
            AccessibilityRole::Button
            | AccessibilityRole::Link
            | AccessibilityRole::Checkbox
            | AccessibilityRole::Switch
            | AccessibilityRole::Radio
            | AccessibilityRole::Tab
            | AccessibilityRole::MenuItem
            | AccessibilityRole::MenuItemCheckbox
            | AccessibilityRole::MenuItemRadio
            | AccessibilityRole::TreeItem => A::ACTIVATE,
            AccessibilityRole::TextField => A::SET_VALUE,
            AccessibilityRole::Slider => A::INCREMENT | A::DECREMENT,
            _ => A::NONE,
        };
        if self.is_focusable(node) {
            actions | A::FOCUS
        } else {
            actions
        }
    }

    fn accessibility_live_region(
        &self,
        node: LayoutNodeId,
        role: Option<AccessibilityRole>,
        inherited: AccessibilityLiveRegion,
    ) -> AccessibilityLiveRegion {
        let attr = |name| self.element_registry.attribute(node, name);
        let declared = self.accessibility_semantics.get(&node).and_then(|s| s.live);
        let priority = match attr("aria-live").as_deref() {
            Some("off") => AccessibilityLive::Off,
            Some("polite") => AccessibilityLive::Polite,
            Some("assertive") => AccessibilityLive::Assertive,
            _ => {
                if let Some(live) = declared {
                    live.priority
                } else {
                    match role {
                        Some(AccessibilityRole::Status) => AccessibilityLive::Polite,
                        Some(AccessibilityRole::Alert) => AccessibilityLive::Assertive,
                        _ => inherited.priority,
                    }
                }
            }
        };
        let atomic = attr("aria-atomic")
            .map(|value| value == "true")
            .unwrap_or_else(|| {
                declared.map_or_else(
                    || {
                        matches!(
                            role,
                            Some(AccessibilityRole::Status | AccessibilityRole::Alert)
                        ) || inherited.atomic
                    },
                    |live| live.atomic,
                )
            });
        let busy = attr("aria-busy")
            .map(|value| value == "true")
            .unwrap_or(inherited.busy);
        AccessibilityLiveRegion {
            root: if declared.is_some()
                || attr("aria-live").is_some()
                || matches!(
                    role,
                    Some(AccessibilityRole::Status | AccessibilityRole::Alert)
                ) {
                self.stable_id(node)
            } else {
                inherited.root
            },
            priority,
            atomic,
            busy,
        }
    }

    /// Route an assistive request through the same focus scope as keyboard navigation.
    pub fn perform_accessibility_action_with_router(
        &mut self,
        router: &mut crate::event_router::EventRouter,
        id: StableNodeId,
        action: AccessibilityAction,
    ) -> bool {
        self.sync_keyboard_focus(router);
        let enabled = self
            .accessibility_nodes()
            .iter()
            .any(|n| n.id == id && !n.disabled && n.actions.supports(&action));
        if !enabled {
            return false;
        }
        if action == AccessibilityAction::Focus {
            let Some(node) = self.layout_id(id).filter(|&node| self.is_focusable(node)) else {
                return false;
            };
            self.move_focus(router, node);
            if self.element_registry.attribute(node, "tabindex").as_deref() != Some("text")
                && let Some(binding) = self.accessibility_bindings.get(&node)
            {
                (binding.action)(&action);
            }
            self.sync_keyboard_focus(router);
            crate::stateful::request_redraw();
            return true;
        }
        let handled = self.perform_accessibility_action(id, action);
        self.sync_keyboard_focus(router);
        handled
    }

    /// Apply an assistive action only to a currently exposed, enabled element.
    pub fn perform_accessibility_action(
        &mut self,
        id: StableNodeId,
        action: AccessibilityAction,
    ) -> bool {
        let Some(semantic) = self
            .accessibility_nodes()
            .into_iter()
            .find(|n| n.id == id && !n.disabled)
        else {
            return false;
        };
        let Some(node) = self.layout_id(id) else {
            return false;
        };
        self.scroll_into_view(node);
        if let Some(binding) = self.accessibility_bindings.get(&node)
            && (binding.action)(&action)
        {
            return true;
        }
        if matches!(
            action,
            AccessibilityAction::Increment | AccessibilityAction::Decrement
        ) && semantic.role == AccessibilityRole::Slider
        {
            let key = if action == AccessibilityAction::Increment {
                38
            } else {
                40
            };
            self.dispatch_key_event(
                node,
                blinc_core::events::event_types::KEY_DOWN,
                key,
                false,
                false,
                false,
                false,
            );
            return true;
        }
        if action != AccessibilityAction::Activate {
            return false;
        }
        if matches!(
            semantic.role,
            AccessibilityRole::Text | AccessibilityRole::Heading | AccessibilityRole::Image
        ) {
            return false;
        }
        if !self
            .handler_registry
            .has_handler(id, blinc_core::events::event_types::POINTER_DOWN)
            && !self
                .handler_registry
                .has_handler(id, blinc_core::events::event_types::POINTER_UP)
        {
            return false;
        }
        let b = semantic.bounds;
        for event in [
            blinc_core::events::event_types::POINTER_DOWN,
            blinc_core::events::event_types::POINTER_UP,
        ] {
            self.dispatch_event_full(
                node,
                event,
                b.x + b.width / 2.0,
                b.y + b.height / 2.0,
                b.width / 2.0,
                b.height / 2.0,
                b.x,
                b.y,
                b.width,
                b.height,
                0.0,
                0.0,
                1.0,
                false,
                false,
                false,
                false,
            );
        }
        true
    }

    fn accessibility_role(&self, node: LayoutNodeId) -> Option<AccessibilityRole> {
        use AccessibilityRole::*;
        if let Some(semantics) = self.accessibility_semantics.get(&node) {
            return Some(semantics.role);
        }
        let role = self.element_registry.attribute(node, "role");
        let tag = role
            .as_deref()
            .or_else(|| self.element_registry.get_element_type(node));
        match tag {
            Some("none" | "presentation") => None,
            Some("button") => Some(Button),
            Some("a" | "link") => Some(Link),
            Some("input" | "textarea" | "textbox") => Some(TextField),
            Some("checkbox") => Some(Checkbox),
            Some("switch") => Some(Switch),
            Some("radio") => Some(Radio),
            Some("slider" | "spinbutton") => Some(Slider),
            Some("tab") => Some(Tab),
            Some("menuitem") => Some(MenuItem),
            Some("menuitemcheckbox") => Some(MenuItemCheckbox),
            Some("menuitemradio") => Some(MenuItemRadio),
            Some("menubar") => Some(MenuBar),
            Some("menu") => Some(Menu),
            Some("separator") => Some(Separator),
            Some("tree") => Some(Tree),
            Some("treeitem") => Some(TreeItem),
            Some("toolbar") => Some(Toolbar),
            Some("tooltip") => Some(Tooltip),
            Some("dialog") => Some(Dialog),
            Some("alertdialog") => Some(AlertDialog),
            Some("status") => Some(Status),
            Some("alert") => Some(Alert),
            Some("region") => Some(Region),
            Some("group") => Some(Custom("group")),
            Some("heading" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6") => Some(Heading),
            Some("img")
                if self
                    .element_registry
                    .attribute(node, "aria-label")
                    .is_some() =>
            {
                Some(Image)
            }
            _ if self.element_registry.attribute(node, "aria-live").is_some() => Some(Region),
            _ if matches!(
                self.element_registry.attribute(node, "tabindex").as_deref(),
                Some("0")
            ) =>
            {
                Some(Button)
            }
            _ if self.get_render_node(node).is_some_and(|n| {
                matches!(
                    n.element_type,
                    ElementType::Text(_) | ElementType::StyledText(_)
                )
            }) =>
            {
                Some(Text)
            }
            _ => None,
        }
    }

    fn accessibility_descendants(&self, node: LayoutNodeId) -> Vec<LayoutNodeId> {
        let mut result = Vec::new();
        let mut pending = vec![node];
        while let Some(n) = pending.pop() {
            result.push(n);
            pending.extend(self.layout_tree.children(n).into_iter().rev());
        }
        result
    }

    fn accessibility_text(&self, node: LayoutNodeId) -> String {
        self.accessibility_label_text(node, false)
    }

    fn accessibility_label_text(&self, node: LayoutNodeId, include_hidden: bool) -> String {
        let mut words = Vec::new();
        let mut pending = vec![node];
        let tree_item = self.accessibility_role(node) == Some(AccessibilityRole::TreeItem);
        while let Some(n) = pending.pop() {
            if tree_item
                && n != node
                && self.accessibility_role(n) == Some(AccessibilityRole::TreeItem)
            {
                continue;
            }
            if !include_hidden && self.accessibility_hidden(n) {
                continue;
            }
            if let Some(render) = self.get_render_node(n) {
                let text = match &render.element_type {
                    ElementType::Text(t) => Some(t.content.as_str()),
                    ElementType::StyledText(t) => Some(t.content.as_str()),
                    _ => None,
                };
                if let Some(text) = text.filter(|t| !t.trim().is_empty()) {
                    words.push(text);
                }
            }
            pending.extend(self.layout_tree.children(n).into_iter().rev());
        }
        words.join(" ")
    }

    fn accessibility_hidden(&self, node: LayoutNodeId) -> bool {
        self.element_registry
            .attribute(node, "aria-hidden")
            .as_deref()
            == Some("true")
            || self.element_registry.attribute(node, "hidden").is_some()
            || self
                .element_registry
                .attribute(node, "data-focus-active")
                .as_deref()
                == Some("false")
            || self
                .get_render_node(node)
                .is_some_and(|n| !n.props.visible || n.props.opacity <= 0.0)
            || self
                .layout_tree
                .get_style(node)
                .is_some_and(|s| s.display == taffy::Display::None)
    }

    fn accessibility_bounds(&self, node: LayoutNodeId) -> Option<ElementBounds> {
        let mut b = self.get_absolute_bounds(node)?;
        for ancestor in self.layout_tree.ancestors(node) {
            if self.accessibility_hidden(ancestor) {
                return None;
            }
            let style = self.layout_tree.get_style(ancestor)?;
            let mut clip = self.get_absolute_bounds(ancestor)?;
            if Some(ancestor) == self.root() {
                if style.overflow.x == taffy::Overflow::Visible
                    && self.accessibility_viewport.0 > 0.0
                {
                    clip.x = 0.0;
                    clip.width = self.accessibility_viewport.0;
                }
                if style.overflow.y == taffy::Overflow::Visible
                    && self.accessibility_viewport.1 > 0.0
                {
                    clip.y = 0.0;
                    clip.height = self.accessibility_viewport.1;
                }
            }
            if Some(ancestor) == self.root() || style.overflow.x != taffy::Overflow::Visible {
                let right = (b.x + b.width).min(clip.x + clip.width);
                b.x = b.x.max(clip.x);
                b.width = right - b.x;
            }
            if Some(ancestor) == self.root() || style.overflow.y != taffy::Overflow::Visible {
                let bottom = (b.y + b.height).min(clip.y + clip.height);
                b.y = b.y.max(clip.y);
                b.height = bottom - b.y;
            }
        }
        (b.x.is_finite()
            && b.y.is_finite()
            && b.width.is_finite()
            && b.height.is_finite()
            && b.width > 0.0
            && b.height > 0.0)
            .then_some(b)
    }
}
