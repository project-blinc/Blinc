use super::*;

impl RenderTree {
    pub(super) fn roving_group(&self, node: LayoutNodeId) -> Option<LayoutNodeId> {
        std::iter::once(node)
            .chain(self.layout_tree.ancestors(node).into_iter().rev())
            .find(|&n| {
                matches!(
                    self.element_registry().attribute(n, "role").as_deref(),
                    Some("menu" | "menubar")
                )
            })
    }

    pub(super) fn roving_items(&self, group: LayoutNodeId) -> Vec<LayoutNodeId> {
        if !matches!(
            self.element_registry().attribute(group, "role").as_deref(),
            Some("menu" | "menubar")
        ) {
            return Vec::new();
        }
        self.focus_descendants(group)
            .into_iter()
            .filter(|&n| {
                n != group
                    && self.element_registry().attribute(n, "role").as_deref() == Some("menuitem")
                    && self.roving_group(n) == Some(group)
                    && self.is_focusable(n)
            })
            .collect()
    }

    pub(super) fn dismiss_focus_scope(&self, router: &EventRouter) -> bool {
        let Some(scope) = router.focus_scopes.last() else {
            return false;
        };
        if let Some(id) = scope
            .key
            .strip_prefix("stack-")
            .and_then(|s| s.parse::<u64>().ok())
        {
            if let Ok(mut stack) = crate::overlay_state::overlay_stack().lock() {
                let dismiss = stack
                    .iter_bottom_up()
                    .any(|e| e.handle.raw() == id && e.dismiss.on_escape);
                if dismiss {
                    stack.close_with_reason(
                        crate::widgets::overlay_stack::OverlayHandle::from_raw(id),
                        crate::widgets::overlay_stack::CloseReason::Escape,
                    );
                }
            }
            return true;
        }
        if let Some(id) = scope
            .key
            .strip_prefix("legacy-")
            .and_then(|s| s.parse::<u64>().ok())
        {
            if let Some(ctx) = crate::overlay_state::OverlayContext::try_get() {
                let manager = ctx.overlay_manager();
                let mut inner = manager.lock().unwrap();
                let dismiss = inner
                    .overlays_sorted()
                    .iter()
                    .any(|o| o.handle.id() == id && o.config.dismiss_on_escape);
                if dismiss {
                    inner.close(crate::widgets::overlay::OverlayHandle::from_raw(id));
                }
            }
            return true;
        }
        false
    }

    fn close_focus_scope(&self, key: &str) {
        if let Some(id) = key.strip_prefix("stack-").and_then(|s| s.parse().ok()) {
            crate::widgets::overlay_stack::OverlayHandle::from_raw(id).close();
        } else if let Some(id) = key.strip_prefix("legacy-").and_then(|s| s.parse().ok())
            && let Some(ctx) = crate::overlay_state::OverlayContext::try_get()
        {
            ctx.overlay_manager()
                .lock()
                .unwrap()
                .close(crate::widgets::overlay::OverlayHandle::from_raw(id));
        }
    }

    pub(super) fn handle_roving_key(
        &mut self,
        router: &mut EventRouter,
        key: u32,
        shift: bool,
    ) -> bool {
        let Some(node) = router.focused() else {
            return false;
        };
        if (key == 93 || (key == 121 && shift))
            && self
                .element_registry()
                .attribute(node, "data-context-menu")
                .is_some()
        {
            crate::event_handler::set_current_mouse_button(2);
            self.click_node(node);
            crate::event_handler::set_current_mouse_button(0);
            return true;
        }
        let group = self.roving_group(node);
        let bar = group.is_some_and(|g| {
            self.element_registry().attribute(g, "role").as_deref() == Some("menubar")
        });
        if matches!(key, 38 | 40)
            && (group.is_none() || bar)
            && self
                .element_registry()
                .attribute(node, "aria-haspopup")
                .as_deref()
                == Some("menu")
        {
            router.menu_last = key == 38;
            self.click_node(node);
            return true;
        }
        let Some(group) = group else {
            return false;
        };
        if !bar
            && key == KEY_TAB
            && let Some(scope) = router.focus_scopes.iter().rev().find(|s| {
                s.return_to
                    .and_then(|id| self.layout_id(id))
                    .is_some_and(|n| {
                        self.roving_group(n).is_none_or(|g| {
                            self.element_registry().attribute(g, "role").as_deref() != Some("menu")
                        })
                    })
            })
        {
            self.close_focus_scope(&scope.key);
            router.pending_tab = Some(shift);
            return true;
        }
        let items = self.roving_items(group);
        if items.is_empty() {
            return matches!(key, 35..=40);
        }
        let position = items.iter().position(|&n| n == node);
        let movement = if bar {
            matches!(key, 35 | 36 | 37 | 39)
        } else {
            matches!(key, 35 | 36 | 38 | 40)
        };
        if movement {
            let next = match key {
                36 => 0,
                35 => items.len() - 1,
                37 | 38 => {
                    position.map_or(items.len() - 1, |i| (i + items.len() - 1) % items.len())
                }
                _ => position.map_or(0, |i| (i + 1) % items.len()),
            };
            self.move_focus(router, items[next]);
            return true;
        }
        if !bar
            && key == 39
            && self
                .element_registry()
                .attribute(node, "aria-haspopup")
                .as_deref()
                == Some("menu")
        {
            self.click_node(node);
            return true;
        }
        if !bar && matches!(key, 37 | 39) {
            // A child menu returns to its row; a root menu switches menubar triggers.
            let trigger = router
                .focus_scopes
                .last()
                .and_then(|scope| scope.return_to)
                .and_then(|id| self.layout_id(id));
            if let Some(trigger) = trigger
                && let Some(parent) = self.roving_group(trigger)
            {
                if self.element_registry().attribute(parent, "role").as_deref() == Some("menubar") {
                    let triggers = self.roving_items(parent);
                    if let Some(at) = triggers.iter().position(|&n| n == trigger) {
                        let next = if key == 37 {
                            (at + triggers.len() - 1) % triggers.len()
                        } else {
                            (at + 1) % triggers.len()
                        };
                        self.move_focus(router, triggers[next]);
                        self.click_node(triggers[next]);
                        return true;
                    }
                } else if key == 37 {
                    if let Some(scope) = router.focus_scopes.last() {
                        self.close_focus_scope(&scope.key);
                    }
                    return true;
                }
            }
        }
        false
    }
}
