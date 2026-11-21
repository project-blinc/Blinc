//! Scroll physics for nodes whose CSS gives them `overflow: scroll`.
//!
//! `auto_create_css_scroll_physics` creates `ScrollPhysics` and handler-registry
//! entries for such nodes that have no `scroll()` widget or `overflow_scroll()`
//! builder behind them. The layout pass runs it over the nodes it restyled.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use taffy::Overflow;

use crate::tree::LayoutNodeId;

use super::super::RenderTree;

impl RenderTree {
    /// Auto-create scroll physics for nodes with `overflow: scroll` set via CSS.
    ///
    /// Scans all layout nodes and creates scroll physics + event handlers
    /// for any node that has `overflow: scroll` on either axis but doesn't
    /// already have scroll physics registered (e.g., from the `scroll()` widget
    /// or the `overflow_scroll()` Div builder).
    pub fn auto_create_css_scroll_physics(&mut self) {
        self.auto_create_css_scroll_physics_in(None);
    }

    /// [`Self::auto_create_css_scroll_physics`] over `scope`, or the whole tree for `None`.
    pub(super) fn auto_create_css_scroll_physics_in(
        &mut self,
        scope: Option<&HashSet<LayoutNodeId>>,
    ) {
        use crate::scroll::{Scroll, ScrollConfig, ScrollDirection, ScrollPhysics};

        /// CSS-created physics, kept by STABLE id so they outlive the
        /// tree.
        ///
        /// `scroll_physics` is keyed by `LayoutNodeId`, which is
        /// reissued whenever the tree is rebuilt -- so a rebuild used to
        /// mint fresh physics here and the scroll position jumped back
        /// to the top. A `scroll()` widget does not suffer this because
        /// `use_scroll_ref_keyed` parks its state outside the tree; a
        /// CSS-declared scroller has no such anchor, so it gets one
        /// here.
        static CSS_SCROLL_PHYSICS: std::sync::Mutex<
            Option<
                std::collections::HashMap<
                    crate::tree::StableNodeId,
                    crate::scroll::SharedScrollPhysics,
                >,
            >,
        > = std::sync::Mutex::new(None);

        // Walk the whole tree, not `element_registry.all_ids()`: that
        // enumerates only elements carrying an explicit `id`, so a
        // class-only rule (`.page { overflow: scroll }`) was never
        // considered and CSS-driven scrolling silently worked for
        // `#id` selectors alone.
        let mut needs_physics: Vec<(LayoutNodeId, ScrollDirection)> = Vec::new();
        let nodes: Vec<LayoutNodeId> = match scope {
            Some(scope) => scope.iter().copied().collect(),
            None => {
                let mut nodes = Vec::new();
                let mut stack: Vec<LayoutNodeId> = self.root().into_iter().collect();
                while let Some(node_id) = stack.pop() {
                    stack.extend(self.layout_tree.children(node_id));
                    nodes.push(node_id);
                }
                nodes
            }
        };

        for node_id in nodes {
            // Skip if already has scroll physics (from Scroll widget or Div builder)
            if self.scroll_physics.contains_key(&node_id) {
                continue;
            }
            let Some(style) = self.layout_tree.style(node_id) else {
                continue;
            };
            let scroll_x = matches!(style.overflow.x, Overflow::Scroll);
            let scroll_y = matches!(style.overflow.y, Overflow::Scroll);
            if !scroll_x && !scroll_y {
                continue;
            }
            let direction = match (scroll_x, scroll_y) {
                (true, true) => ScrollDirection::Both,
                (true, false) => ScrollDirection::Horizontal,
                (false, true) => ScrollDirection::Vertical,
                _ => unreachable!(),
            };
            needs_physics.push((node_id, direction));
        }

        // Create physics and register handlers
        for (node_id, direction) in needs_physics {
            tracing::trace!(
                "auto_create_css_scroll_physics: creating for {:?} direction={:?}",
                node_id,
                direction
            );
            let config = ScrollConfig {
                direction,
                ..Default::default()
            };
            let stable_id = self.stable_id_or_warn(node_id);
            // Reuse this node's physics across rebuilds, so its offset
            // survives; only mint on first sight.
            let physics = {
                let mut cache = CSS_SCROLL_PHYSICS.lock().expect("poisoned");
                let cache = cache.get_or_insert_with(std::collections::HashMap::new);
                cache
                    .entry(stable_id)
                    .or_insert_with(|| Arc::new(Mutex::new(ScrollPhysics::new(config))))
                    .clone()
            };
            // Set animation scheduler for bounce springs
            if let Some(scheduler) = self.animations.upgrade() {
                physics.lock().unwrap().set_scheduler(&scheduler);
            }
            let handlers = Scroll::create_internal_handlers(Arc::clone(&physics));
            self.scroll_physics.insert(node_id, physics);
            self.handler_registry.register(stable_id, handlers);
        }
        self.apply_base_scrollbar_styles();
    }
}

#[cfg(test)]
mod css_scroll_tests {
    use crate::div::div;
    use crate::renderer::RenderTree;

    /// CSS-driven scrolling must work for a class selector, not only
    /// for `#id`.
    ///
    /// `auto_create_css_scroll_physics` used to enumerate
    /// `element_registry.all_ids()`, which lists only elements carrying
    /// an explicit `id`. A `.page { overflow: scroll }` rule therefore
    /// set taffy's overflow but never got scroll physics, so the
    /// element clipped its content and refused to scroll -- with no
    /// warning, and while `#page` on the same markup worked.
    #[test]
    fn class_selector_overflow_scroll_gets_physics() {
        let el = div()
            .class("page")
            .w(200.0)
            .h(100.0)
            .child(div().w(200.0).h(1000.0));
        let mut tree = RenderTree::from_element(&el);
        tree.set_stylesheet(
            crate::css_parser::Stylesheet::parse(".page { overflow: scroll }")
                .expect("stylesheet parses"),
        );
        tree.apply_stylesheet_layout_overrides();

        assert!(
            !tree.scroll_physics.is_empty(),
            "a class-selected overflow:scroll element must get scroll physics"
        );
    }
}
