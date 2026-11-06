use super::{ElementType, RenderTree};
use crate::accessibility::{AccessibilityAction, AccessibilityNode, AccessibilityRole};
use crate::{
    element::ElementBounds,
    tree::{LayoutNodeId, StableNodeId},
};

impl RenderTree {
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
        let mut pending = vec![root];
        while let Some(node) = pending.pop() {
            if self.accessibility_hidden(node) {
                continue;
            }
            let Some(bounds) = self.accessibility_bounds(node) else {
                continue;
            };
            if let Some(role) = self.accessibility_role(node) {
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
                    .unwrap_or_else(|| self.accessibility_text(node));
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
                        identifier: self.element_registry.get_id(node),
                        role,
                        label,
                        value,
                        hint: attr("aria-description").unwrap_or_default(),
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
                        focused: live.as_ref().is_some_and(|v| v.focused),
                        selection: live.as_ref().and_then(|v| v.selection),
                    });
                }
                continue;
            }
            pending.extend(self.layout_tree.children(node).into_iter().rev());
        }
        result
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
            Some("menuitem" | "menuitemcheckbox" | "menuitemradio") => Some(MenuItem),
            Some("heading" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6") => Some(Heading),
            Some("img")
                if self
                    .element_registry
                    .attribute(node, "aria-label")
                    .is_some() =>
            {
                Some(Image)
            }
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
        while let Some(n) = pending.pop() {
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
                .is_some_and(|n| n.props.opacity <= 0.0)
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
            let clip = self.get_absolute_bounds(ancestor)?;
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
