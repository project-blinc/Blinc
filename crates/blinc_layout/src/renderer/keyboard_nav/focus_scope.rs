use super::*;
use crate::event_router::focus::FocusScope;
use crate::tree::StableNodeId;

impl RenderTree {
    pub(crate) fn is_focusable(&self, node: LayoutNodeId) -> bool {
        !self.layout_tree.ancestors(node).iter().any(|&n| {
            n != node
                && self
                    .element_registry()
                    .attribute(n, "data-focus-delegate")
                    .is_some()
        }) && matches!(
            self.element_registry()
                .attribute(node, "tabindex")
                .as_deref(),
            Some("0" | "text" | "-1")
        ) && self.focus_enabled(node)
    }

    pub(super) fn focus_enabled(&self, node: LayoutNodeId) -> bool {
        std::iter::once(node)
            .chain(self.layout_tree.ancestors(node))
            .all(|n| {
                self.element_registry().attribute(n, "disabled").is_none()
                    && self
                        .element_registry()
                        .attribute(n, "data-focus-scope")
                        .is_none_or(|key| crate::widgets::overlay_focus::is_active(&key))
                    && self
                        .element_registry()
                        .attribute(n, "data-focus-active")
                        .as_deref()
                        != Some("false")
            })
            && self
                .get_absolute_bounds(node)
                .is_some_and(|b| b.width > 0.0 && b.height > 0.0)
    }

    pub(super) fn focus_descendants(&self, root: LayoutNodeId) -> Vec<LayoutNodeId> {
        let mut nodes = Vec::new();
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            nodes.push(node);
            stack.extend(self.layout_tree.children(node).into_iter().rev());
        }
        nodes
    }

    pub(crate) fn active_focus_scopes(&self) -> Vec<(String, LayoutNodeId)> {
        [
            crate::widgets::overlay::OVERLAY_LAYER_ID,
            crate::widgets::overlay_stack::OVERLAY_STACK_LAYER_ID,
        ]
        .into_iter()
        .filter_map(|id| self.query_by_id(id))
        .flat_map(|root| self.focus_descendants(root))
        .filter_map(|node| {
            let registry = self.element_registry();
            let key = registry.attribute(node, "data-focus-scope")?;
            if !crate::widgets::overlay_focus::is_active(&key) {
                return None;
            }
            if registry.attribute(node, "data-focus-active").as_deref() == Some("false") {
                return None;
            }
            if registry.attribute(node, "data-focus-passive").is_some()
                && !self
                    .focus_descendants(node)
                    .iter()
                    .any(|&n| registry.attribute(n, "role").as_deref() == Some("menu"))
            {
                return None;
            }
            Some((key, node))
        })
        .collect()
    }

    pub(super) fn move_focus(&mut self, router: &mut EventRouter, node: LayoutNodeId) {
        let keyboard = router.keyboard_focus();
        crate::widgets::text_input::blur_all_text_inputs();
        self.scroll_into_view(node);
        router.set_focus_with_ancestors(Some(node), self.layout_tree.ancestors(node));
        if keyboard {
            router.focus_via_keyboard(node);
        }
        router.focus_stable = self.stable_id(node);
        if self
            .element_registry()
            .attribute(node, "tabindex")
            .as_deref()
            == Some("text")
        {
            self.click_node(node);
        }
    }

    /// Reconcile overlay focus after queued tree updates and before styling or input.
    pub fn sync_keyboard_focus(&mut self, router: &mut EventRouter) {
        if let Some(stable) = router.focus_stable {
            if let Some(node) = self.layout_id(stable) {
                if router.focused() != Some(node) {
                    let keyboard = router.keyboard_focus();
                    router.set_focus(Some(node));
                    if keyboard {
                        router.focus_via_keyboard(node);
                    }
                }
            } else {
                router.set_focus(None);
                router.focus_stable = Some(stable);
            }
        }
        let live = self.active_focus_scopes();
        let common = router
            .focus_scopes
            .iter()
            .zip(&live)
            .take_while(|(old, (key, _))| old.key == *key)
            .count();
        let mut return_to = None;
        let mut closing_members: Vec<StableNodeId> = Vec::new();
        while router.focus_scopes.len() > common {
            let scope = router.focus_scopes.pop().unwrap();
            return_to = scope.return_to;
            closing_members.extend(scope.members);
        }
        let focused = router
            .focus_stable
            .or_else(|| router.focused().and_then(|n| self.stable_id(n)));
        let outside = focused.is_some_and(|id| {
            !closing_members.contains(&id)
                && self.layout_id(id).is_some_and(|n| self.is_focusable(n))
        });
        if !closing_members.is_empty() && !outside {
            if let Some(node) = return_to
                .and_then(|id| self.layout_id(id))
                .filter(|&n| self.is_focusable(n))
            {
                self.move_focus(router, node);
            } else {
                router.set_focus(None);
            }
        }
        for (index, (key, root)) in live.into_iter().enumerate() {
            let nodes = self.focus_descendants(root);
            let members = nodes.iter().filter_map(|&n| self.stable_id(n)).collect();
            if index < common {
                router.focus_scopes[index].members = members;
                continue;
            }
            let return_to = router
                .focus_stable
                .or_else(|| router.focused().and_then(|n| self.stable_id(n)));
            router.focus_scopes.push(FocusScope {
                key,
                return_to,
                members,
            });
            let menu = nodes
                .iter()
                .copied()
                .find(|&n| self.element_registry().attribute(n, "role").as_deref() == Some("menu"));
            let modal = self
                .element_registry()
                .attribute(root, "data-focus-modal")
                .is_some();
            let target = if router.keyboard_focus() || (modal && menu.is_none()) {
                {
                    let candidates: Vec<_> = nodes
                        .iter()
                        .copied()
                        .filter(|&n| self.is_focusable(n) && n != root && Some(n) != menu)
                        .collect();
                    if router.menu_last {
                        candidates.last().copied()
                    } else {
                        candidates.first().copied()
                    }
                }
            } else {
                menu
            }
            .or(menu)
            .or_else(|| modal.then_some(root));
            router.menu_last = false;
            if let Some(target) = target {
                self.move_focus(router, target);
            }
        }
        if let Some(node) = router.focused()
            && !self.is_focusable(node)
        {
            router.set_focus(None);
        }
        router.focus_stable = router.focused().and_then(|n| self.stable_id(n));
        router.publish_focus();
        if let Some(shift) = router.pending_tab.take() {
            self.handle_navigation_key(router, KEY_TAB, shift);
        }
    }
}
