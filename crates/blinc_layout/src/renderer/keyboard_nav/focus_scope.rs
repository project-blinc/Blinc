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
        }) && (self
            .accessibility_semantics
            .get(&node)
            .is_some_and(|semantics| {
                semantics.actions.is_some_and(|actions| {
                    actions.contains(crate::accessibility::AccessibilityActions::FOCUS)
                })
            })
            || matches!(
                self.element_registry()
                    .attribute(node, "tabindex")
                    .as_deref(),
                Some("0" | "text" | "-1")
            ))
            && self.focus_enabled(node)
    }

    pub(super) fn focus_enabled(&self, node: LayoutNodeId) -> bool {
        std::iter::once(node)
            .chain(self.layout_tree.ancestors(node))
            .all(|n| {
                self.element_registry().attribute(n, "disabled").is_none()
                    && self
                        .get_render_node(n)
                        .is_some_and(|node| node.props.visible)
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

    pub(crate) fn move_focus(&mut self, router: &mut EventRouter, node: LayoutNodeId) {
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

    fn inside_focus_scope(&self, node: LayoutNodeId, root: LayoutNodeId) -> bool {
        node == root || self.layout_tree.ancestors(node).contains(&root)
    }

    fn owns_focus_target(&self, root: LayoutNodeId, node: LayoutNodeId) -> bool {
        if node == root {
            return true;
        }
        self.layout_tree
            .ancestors(node)
            .into_iter()
            .find(|&ancestor| {
                self.element_registry()
                    .attribute(ancestor, "data-focus-scope")
                    .is_some()
            })
            == Some(root)
    }

    fn resolve_focus_target(&self, node: LayoutNodeId) -> Option<LayoutNodeId> {
        if self.is_focusable(node) {
            return Some(node);
        }
        if !self.focus_enabled(node) {
            return None;
        }
        let mut controls = self
            .focus_descendants(node)
            .into_iter()
            .skip(1)
            .filter(|&child| self.is_focusable(child));
        let target = controls.next()?;
        controls.next().is_none().then_some(target)
    }

    fn public_focus_id(&self, node: LayoutNodeId) -> Option<String> {
        let registry = self.element_registry();
        registry.get_id(node).or_else(|| {
            self.layout_tree
                .ancestors(node)
                .into_iter()
                .find_map(|parent| {
                    let id = registry.get_id(parent)?;
                    (self.resolve_focus_target(parent) == Some(node)).then_some(id)
                })
        })
    }

    fn initial_focus_target(
        &self,
        root: LayoutNodeId,
        nodes: &[LayoutNodeId],
    ) -> Option<LayoutNodeId> {
        let valid = |node| self.is_focusable(node) && self.owns_focus_target(root, node);
        let named = nodes.iter().find_map(|&node| {
            self.owns_focus_target(root, node)
                .then(|| {
                    self.element_registry()
                        .attribute(node, "data-initial-focus")
                })
                .flatten()
        });
        named
            .and_then(|id| self.query_by_id(&id))
            .and_then(|node| self.resolve_focus_target(node))
            .filter(|&node| valid(node))
            .or_else(|| {
                nodes.iter().copied().find_map(|node| {
                    self.element_registry().attribute(node, "autofocus")?;
                    self.resolve_focus_target(node)
                        .filter(|&target| valid(target))
                })
            })
    }

    /// Reconcile overlay focus after queued tree updates and before styling or input.
    pub fn sync_keyboard_focus(&mut self, router: &mut EventRouter) {
        let context = blinc_core::context_state::BlincContextState::try_get();
        let request = context.and_then(|ctx| ctx.take_focus_request());
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
        let active_root = live.last().map(|(_, root)| *root);
        let requested_target = request
            .as_ref()
            .and_then(|id| id.as_deref())
            .and_then(|id| self.query_by_id(id))
            .and_then(|node| self.resolve_focus_target(node))
            .filter(|&node| active_root.is_none_or(|root| self.inside_focus_scope(node, root)));
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
            let preferred = requested_target
                .filter(|&node| self.owns_focus_target(root, node))
                .or_else(|| self.initial_focus_target(root, &nodes));
            let target = preferred
                .or_else(|| {
                    if router.keyboard_focus() || (modal && menu.is_none()) {
                        {
                            let candidates: Vec<_> = nodes
                                .iter()
                                .copied()
                                .filter(|&n| {
                                    self.is_focusable(n)
                                        && n != root
                                        && Some(n) != menu
                                        && self.owns_focus_target(root, n)
                                })
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
                })
                .or(menu)
                .or_else(|| modal.then_some(root));
            router.menu_last = false;
            if let Some(target) = target {
                self.move_focus(router, target);
            }
        }
        if let Some(target) = requested_target {
            if router.focused() != Some(target) {
                self.move_focus(router, target);
            }
        } else if matches!(request, Some(None)) {
            crate::widgets::text_input::blur_all_text_inputs();
            router.set_focus(None);
        }
        if let Some(node) = router.focused()
            && !self.is_focusable(node)
        {
            router.set_focus(None);
        }
        router.focus_stable = router.focused().and_then(|n| self.stable_id(n));
        self.accessibility_focus = router.focus_stable;
        router.publish_focus();
        if let Some(context) = context
            && (request.is_some() || router.context_focus != router.focus_stable)
        {
            let requested_id = requested_target
                .filter(|&target| router.focused() == Some(target))
                .and_then(|_| request.as_ref().and_then(|id| id.clone()));
            context.publish_focus(
                requested_id
                    .or_else(|| router.focused().and_then(|node| self.public_focus_id(node))),
            );
            router.context_focus = router.focus_stable;
        }
        if let Some(shift) = router.pending_tab.take() {
            self.handle_navigation_key(router, KEY_TAB, shift);
        }
    }
}
