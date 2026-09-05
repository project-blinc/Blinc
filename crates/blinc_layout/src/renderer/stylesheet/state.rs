//! State-driven stylesheet application: per-node and per-frame.
//!
//! Three driver methods:
//!
//! - `apply_state_styles` — single-node entry point. Resets the node
//!   to its `base_styles` snapshot, re-applies the base CSS rule, and
//!   layers `:hover` / `:active` / `:focus` rules in precedence order.
//!   Captures a before-snapshot when a `transition:` exists so the
//!   diff against the after-snapshot can start CSS transitions.
//!   Mirrors the same flow into the taffy `Style` for layout
//!   transitions.
//! - `apply_stylesheet_state_styles` — per-frame fan-out. For every
//!   registered element, reads its current `is_hovered` / `is_pressed`
//!   / `is_focused` from the `EventRouter`, calls `apply_state_styles`,
//!   and starts/stops the matching `:hover`-scoped CSS animation. Then
//!   delegates to `apply_complex_selector_styles` and
//!   `apply_svg_tag_styles` for class- and SVG-tag rules.
//! - `apply_pointer_styles` — evaluates `calc(env(pointer-x), ...)`
//!   dynamic properties using the live `PointerQueryState`. Resets the
//!   transform back to base before reapplying transform-derived
//!   dynamics so successive frames don't compound.
//!
//! `css_has_visible_transitions` lives here too because the "any
//! state still in flight?" check is what gates the frame-loop redraw
//! after this pass writes new transitions.

use std::borrow::Cow;
use std::collections::HashSet;

use crate::css_parser::{ElementState, Stylesheet};
use crate::dynamic_style::DynamicPropertyExt;
use crate::element_style::ElementStyle;
use crate::tree::LayoutNodeId;

use super::super::RenderTree;

impl RenderTree {
    /// Resolve the base `#id { ... }` style for a node — Phase 5.2 of
    /// the unified property channel ([[project-reactive-architecture-v2]]).
    ///
    /// Prefers the pre-resolved [`StateStyleTable`] when populated AND
    /// fresh against the current tree's `build_generation`; falls back
    /// to the stylesheet rule walk otherwise. The borrow case is
    /// zero-cost (just a reference into the supplied `stylesheet`);
    /// the table-hit case clones the stored [`ElementStyle`] out from
    /// behind the table's `RefCell` borrow so the caller can release
    /// the guard and freely mutate `render_nodes` afterwards.
    ///
    /// Phase 5.2 wires the consumer-side: the table is always empty
    /// at this point so behaviour is exactly preserved. Phase 5.3
    /// wires the build trigger (stylesheet-bind / class-set change)
    /// and the win lands.
    fn resolve_base_style<'a>(
        &self,
        node_id: LayoutNodeId,
        element_id: &str,
        stylesheet: &'a Stylesheet,
    ) -> Option<Cow<'a, ElementStyle>> {
        if let Some(stable) = self.stable_id(node_id) {
            let table = self.state_style_table.borrow();
            // Phase 5.3 — populated-table check is sufficient. The
            // `build_generation` stamp on the table is kept as
            // debugging metadata, but freshness now relies on the
            // stable-id contract: entries for surviving nodes stay
            // correct across structural rebuilds because their
            // `StableNodeId` is preserved; missing entries (newly
            // added nodes) fall through to the rule walk below;
            // orphaned entries (removed nodes) are never queried.
            // The table is rebuilt on `set_stylesheet*` — the only
            // event that invalidates content.
            if table.is_populated() {
                return table.get_base(stable).cloned().map(Cow::Owned);
            }
        }
        stylesheet.get(element_id).map(Cow::Borrowed)
    }

    /// Stateful sibling of [`Self::resolve_base_style`]. Looks up
    /// `#id:state { ... }` cascades in the table first, falls back to
    /// `stylesheet.get_with_state`.
    fn resolve_state_style<'a>(
        &self,
        node_id: LayoutNodeId,
        element_id: &str,
        state: ElementState,
        stylesheet: &'a Stylesheet,
    ) -> Option<Cow<'a, ElementStyle>> {
        if let Some(stable) = self.stable_id(node_id) {
            let table = self.state_style_table.borrow();
            // Phase 5.3 — populated-table check is sufficient. The
            // `build_generation` stamp on the table is kept as
            // debugging metadata, but freshness now relies on the
            // stable-id contract: entries for surviving nodes stay
            // correct across structural rebuilds because their
            // `StableNodeId` is preserved; missing entries (newly
            // added nodes) fall through to the rule walk below;
            // orphaned entries (removed nodes) are never queried.
            // The table is rebuilt on `set_stylesheet*` — the only
            // event that invalidates content.
            if table.is_populated() {
                return table.get_state(stable, state).cloned().map(Cow::Owned);
            }
        }
        stylesheet
            .get_with_state(element_id, state)
            .map(Cow::Borrowed)
    }

    /// Apply state-specific styles from the stylesheet to a node.
    ///
    /// This is called when a node's interaction state changes (hover, pressed, focused).
    /// It looks up the node's string ID and applies any matching state styles.
    ///
    /// # Arguments
    /// * `node_id` - The node whose state changed
    /// * `hovered` - Whether the node is currently hovered
    /// * `pressed` - Whether the node is currently pressed
    /// * `focused` - Whether the node is currently focused
    ///
    /// # Returns
    /// `true` if styles were applied, `false` if no stylesheet or no matching styles
    pub fn apply_state_styles(
        &mut self,
        node_id: LayoutNodeId,
        hovered: bool,
        pressed: bool,
        focused: bool,
    ) -> bool {
        // Early return if no stylesheet
        let stylesheet = match &self.stylesheet {
            Some(s) => s.clone(),
            None => return false,
        };

        // Look up the node's string ID from the registry
        let element_id = match self.element_registry.get_id(node_id) {
            Some(id) => id,
            None => return false, // Node has no ID, can't apply stylesheet styles
        };

        // Get or store base style for this node
        if !self.base_styles.contains_key(&node_id) {
            if let Some(render_node) = self.render_nodes.get(&node_id) {
                self.base_styles.insert(node_id, render_node.props.clone());
            }
        }
        // Get or store base taffy style for layout transition support
        if !self.base_taffy_styles.contains_key(&node_id) {
            if let Some(style) = self.layout_tree.get_style(node_id) {
                self.base_taffy_styles.insert(node_id, style);
            }
        }

        // Start with base style
        let base_props = match self.base_styles.get(&node_id) {
            Some(props) => props.clone(),
            None => return false,
        };

        // Check if this element has transitions defined. Phase 5.2:
        // routed through `resolve_base_style` so the lookup hits the
        // pre-resolved table when populated (P5.3 wires the build);
        // today the table is always empty and the call falls through
        // to `stylesheet.get` for behaviour-preserving migration.
        let transition_set = self
            .resolve_base_style(node_id, &element_id, &stylesheet)
            .and_then(|s| s.transition.clone());

        // Pre-resolve every state-style cascade BEFORE taking the
        // `&mut` borrow on render_nodes below. Each Cow either holds
        // a borrow into `stylesheet` (lifetime independent of self)
        // or an owned clone from the state-style table (no borrow on
        // self), so they coexist freely with `self.render_nodes.get_mut`.
        let base_lookup = self.resolve_base_style(node_id, &element_id, &stylesheet);
        let hover_lookup = if hovered {
            self.resolve_state_style(node_id, &element_id, ElementState::Hover, &stylesheet)
        } else {
            None
        };
        let active_lookup = if pressed {
            self.resolve_state_style(node_id, &element_id, ElementState::Active, &stylesheet)
        } else {
            None
        };
        let focus_lookup = if focused {
            self.resolve_state_style(node_id, &element_id, ElementState::Focus, &stylesheet)
        } else {
            None
        };

        // Snapshot before-props for transition detection (visual + layout).
        // Uses snapshot_before_keyframe_properties to avoid QR decomposition
        // drift on transform fields when an active transition exists.
        let before_kp = if transition_set.is_some() {
            self.snapshot_before_keyframe_properties(node_id)
        } else {
            None
        };

        // Apply state-specific styles in order of precedence
        let mut applied = false;
        let render_node = match self.render_nodes.get_mut(&node_id) {
            Some(node) => node,
            None => return false,
        };

        // Reset to base style first
        render_node.props = base_props;

        // Reset taffy style to base before applying state layout overrides
        if let Some(base_taffy) = self.base_taffy_styles.get(&node_id) {
            self.layout_tree.set_style(node_id, base_taffy.clone());
        }

        // Apply base stylesheet style (if any)
        if let Some(base_style) = base_lookup.as_deref() {
            Self::apply_element_style_to_props(&mut render_node.props, base_style);
            if base_style.has_layout_props() {
                if let Some(mut taffy_style) = self.layout_tree.get_style(node_id) {
                    Self::apply_element_style_to_taffy(&mut taffy_style, base_style);
                    self.layout_tree.set_style(node_id, taffy_style);
                }
            }
            applied = true;
        }

        // Apply hover style
        if let Some(hover_style) = hover_lookup.as_deref() {
            Self::apply_element_style_to_props(&mut render_node.props, hover_style);
            if hover_style.has_layout_props() {
                if let Some(mut taffy_style) = self.layout_tree.get_style(node_id) {
                    Self::apply_element_style_to_taffy(&mut taffy_style, hover_style);
                    self.layout_tree.set_style(node_id, taffy_style);
                }
            }
            applied = true;
        }

        // Apply active/pressed style (takes precedence over hover)
        if let Some(active_style) = active_lookup.as_deref() {
            Self::apply_element_style_to_props(&mut render_node.props, active_style);
            if active_style.has_layout_props() {
                if let Some(mut taffy_style) = self.layout_tree.get_style(node_id) {
                    Self::apply_element_style_to_taffy(&mut taffy_style, active_style);
                    self.layout_tree.set_style(node_id, taffy_style);
                }
            }
            applied = true;
        }

        // Apply focus style
        if let Some(focus_style) = focus_lookup.as_deref() {
            Self::apply_element_style_to_props(&mut render_node.props, focus_style);
            if focus_style.has_layout_props() {
                if let Some(mut taffy_style) = self.layout_tree.get_style(node_id) {
                    Self::apply_element_style_to_taffy(&mut taffy_style, focus_style);
                    self.layout_tree.set_style(node_id, taffy_style);
                }
            }
            applied = true;
        }

        // Detect and start transitions for changed properties (visual + layout)
        if let (Some(before_kp), Some(transition_set)) = (before_kp, transition_set) {
            if let Some(after_kp) = self.snapshot_keyframe_properties(node_id) {
                self.detect_and_start_transitions(node_id, &before_kp, &after_kp, &transition_set);
            }
        }

        applied
    }

    /// Visibility-gated counterpart of `css_transitions_empty`.
    /// Returns `true` when there's at least one transition whose
    /// target node was painted in the most recent frame, i.e. the
    /// chain should keep firing for it.
    ///
    /// `painted` is the `LayoutNodeId` set the paint walker
    /// produced; the store is keyed by `StableNodeId` (Phase 5),
    /// so we translate via `layout_to_stable` before the
    /// containment check.
    pub fn css_has_visible_transitions(&self, painted: &HashSet<LayoutNodeId>) -> bool {
        let painted_stable: HashSet<crate::tree::StableNodeId> =
            painted.iter().filter_map(|n| self.stable_id(*n)).collect();
        let store = self.css_anim_store.lock().unwrap();
        // Match `has_active_transitions`: only PLAYING transitions
        // count as a redraw signal. Completed transitions stay in
        // the map for same-target restart prevention but should
        // not pin the chain.
        store
            .transitions
            .iter()
            .any(|(s, t)| t.is_playing && painted_stable.contains(s))
    }

    /// Apply stylesheet state styles based on EventRouter state.
    ///
    /// This should be called after mouse events to update styles for nodes
    /// whose interaction state has changed. It applies `:hover`, `:active`,
    /// and `:focus` styles from the stylesheet.
    ///
    /// # Arguments
    /// * `router` - The event router containing current interaction state
    ///
    /// # Returns
    /// `true` if any styles were applied, `false` otherwise
    pub fn apply_stylesheet_state_styles(
        &mut self,
        router: &crate::event_router::EventRouter,
    ) -> bool {
        // Early return if no stylesheet
        if self.stylesheet.is_none() {
            return false;
        }

        // State-style apply can change `props.cursor` via `:hover`/
        // `:active`/`:focus` rules, so the bare-mouse-move cache may
        // be stale. Invalidate eagerly — the cache recomputes on the
        // next read.
        self.invalidate_mouse_move_pipeline_cache();

        let mut any_applied = false;

        // Get all registered element IDs and their node IDs
        let registered_ids: Vec<(String, LayoutNodeId)> = self
            .element_registry
            .all_ids()
            .into_iter()
            .filter_map(|id| self.element_registry.get(&id).map(|node_id| (id, node_id)))
            .collect();

        // Apply state styles for each registered element
        for (element_id, node_id) in registered_ids {
            // Check if this element has any state styles in the stylesheet
            if !self.has_state_styles(node_id) {
                continue;
            }

            // Get current interaction state from router. Both
            // `is_hovered` and `is_pressed` are keyed by stable id
            // (they survive rebuilds — see
            // `gotcha_event_router_layout_id_staleness`); resolve
            // the layout id to a stable id before asking.
            let stable = self.stable_id(node_id);
            let hovered = stable.is_some_and(|s| router.is_hovered(s));
            let pressed = stable.is_some_and(|s| router.is_pressed(s));
            let focused = router.is_focused(node_id);

            // Apply state styles
            if self.apply_state_styles(node_id, hovered, pressed, focused) {
                any_applied = true;
                tracing::trace!(
                    "Applied stylesheet state styles to #{}: hovered={}, pressed={}, focused={}",
                    element_id,
                    hovered,
                    pressed,
                    focused
                );
            }

            // Trigger/stop hover CSS animations. Phase 5.2 routes the
            // "does this state have an animation?" probe through the
            // pre-resolved table when populated; today the table is
            // always empty, so the call falls back to the stylesheet
            // rule walk — behaviour preserved.
            let stylesheet = self.stylesheet.as_ref().unwrap().clone();
            if hovered && !self.hover_css_animations.contains(&node_id) {
                let has_hover_anim = self
                    .resolve_state_style(node_id, &element_id, ElementState::Hover, &stylesheet)
                    .as_deref()
                    .is_some_and(|s| s.animation.is_some());
                if has_hover_anim {
                    self.start_css_animation_for_state(node_id, ElementState::Hover);
                    self.hover_css_animations.insert(node_id);
                    any_applied = true;
                }
            } else if !hovered && self.hover_css_animations.remove(&node_id) {
                // Hover left — remove hover animation if no base animation exists
                let base_has_anim = self
                    .resolve_base_style(node_id, &element_id, &stylesheet)
                    .as_deref()
                    .is_some_and(|s| s.animation.is_some());
                if base_has_anim {
                    self.start_css_animation_for_element(node_id);
                } else if let Some(stable) = self.stable_id(node_id) {
                    self.css_anim_store
                        .lock()
                        .unwrap()
                        .animations
                        .remove(&stable);
                }
                any_applied = true;
            }
        }

        // Apply complex selector rules (class selectors, descendant/child combinators, etc.)
        if self.apply_complex_selector_styles(router) {
            any_applied = true;
        }

        // Apply SVG tag-name CSS rules (e.g., `path { fill: red; }`)
        if self.apply_svg_tag_styles(router) {
            any_applied = true;
        }

        any_applied
    }

    /// Evaluate dynamic `calc(env(...))` properties for pointer-tracked elements.
    ///
    /// Called per-frame after `apply_stylesheet_state_styles()` and before rendering.
    /// For each element in the pointer query, collects dynamic properties from the
    /// active stylesheet entries (base + hover/active/focus) and evaluates them with
    /// the current pointer state, writing results directly to RenderProps.
    pub fn apply_pointer_styles(
        &mut self,
        pointer_query: &crate::pointer_query::PointerQueryState,
        router: &crate::event_router::EventRouter,
    ) {
        let stylesheet = match &self.stylesheet {
            Some(s) => s.clone(),
            None => return,
        };

        for (element_id, pointer_state) in pointer_query.iter() {
            let node_id = match self.element_registry.get(element_id) {
                Some(id) => id,
                None => continue,
            };

            // Build CalcContext with pointer env vars
            let mut ctx = crate::calc::CalcContext::default();
            for name in &[
                "pointer-x",
                "pointer-y",
                "pointer-vx",
                "pointer-vy",
                "pointer-speed",
                "pointer-distance",
                "pointer-angle",
                "pointer-inside",
                "pointer-active",
                "pointer-pressure",
                "pointer-touch-count",
                "pointer-hover-duration",
            ] {
                if let Some(val) = pointer_state.resolve_env(name) {
                    ctx.env_vars.insert(name.to_string(), val);
                }
            }

            // Collect dynamic properties from applicable stylesheet entries
            // (base first, then state overrides in precedence order)
            let mut dynamic_props: Vec<&crate::element_style::DynamicProperty> = Vec::new();

            // Base style
            if let Some(base) = stylesheet.get(element_id) {
                if let Some(ref dps) = base.dynamic_properties {
                    dynamic_props.extend(dps.iter());
                }
            }

            // Hover state (overrides base) — router stores hover by
            // stable id (survives rebuilds); resolve the layout id
            // first.
            if self
                .stable_id(node_id)
                .is_some_and(|s| router.is_hovered(s))
            {
                if let Some(hover) = stylesheet.get_with_state(element_id, ElementState::Hover) {
                    if let Some(ref dps) = hover.dynamic_properties {
                        dynamic_props.extend(dps.iter());
                    }
                }
            }

            // Active/pressed state (overrides hover) — router stores
            // pressed by stable id; resolve the layout id first.
            let pressed_here = self
                .stable_id(node_id)
                .map(|s| router.is_pressed(s))
                .unwrap_or(false);
            if pressed_here {
                if let Some(active) = stylesheet.get_with_state(element_id, ElementState::Active) {
                    if let Some(ref dps) = active.dynamic_properties {
                        dynamic_props.extend(dps.iter());
                    }
                }
            }

            // Focus state
            if router.is_focused(node_id) {
                if let Some(focus) = stylesheet.get_with_state(element_id, ElementState::Focus) {
                    if let Some(ref dps) = focus.dynamic_properties {
                        dynamic_props.extend(dps.iter());
                    }
                }
            }

            if dynamic_props.is_empty() {
                continue;
            }

            // Evaluate and apply to RenderProps
            if let Some(render_node) = self.render_nodes.get_mut(&node_id) {
                // If any dynamic properties are transform-related (SkewX, SkewY, Rotate, etc.),
                // reset props.transform to its base value first to prevent frame-compounding.
                // Without this, compose_affine() would accumulate onto the previous frame's
                // dynamic transform, causing exponential growth.
                let has_transform_dynamics = dynamic_props.iter().any(|dp| dp.is_transform());
                if has_transform_dynamics {
                    let base_transform = self
                        .base_styles
                        .get(&node_id)
                        .and_then(|base| base.transform.clone());
                    render_node.props.transform = base_transform;
                }

                for dp in &dynamic_props {
                    dp.apply(&mut render_node.props, &ctx);
                }
            }
        }
    }
}

#[cfg(test)]
mod p5_2_table_consumer_tests {
    //! Phase 5.2 of the unified property channel
    //! ([[project-reactive-architecture-v2]]):
    //! `apply_state_styles` must consult `StateStyleTable` when it's
    //! populated and fresh against `build_generation`, otherwise fall
    //! back to the rule-walk via `stylesheet.get` / `.get_with_state`.
    //!
    //! These tests discriminate the two paths by building the table
    //! from one stylesheet and binding a DIFFERENT stylesheet to the
    //! tree: the table-hit path applies the table's value, the
    //! fallback path applies the bound stylesheet's value.
    use super::*;
    use crate::div::div;
    use crate::renderer::RenderTree;
    use crate::state_style_table::StateStyleTable;

    fn parse(css: &str) -> Stylesheet {
        Stylesheet::parse_with_errors(css).stylesheet
    }

    fn build_btn_tree(initial_opacity: f32) -> RenderTree {
        let ui = div().id("btn").opacity(initial_opacity);
        RenderTree::from_element(&ui)
    }

    #[test]
    fn fallback_path_runs_when_table_empty() {
        // Empty table → resolve_base_style walks the bound stylesheet.
        let mut tree = build_btn_tree(1.0);
        tree.set_stylesheet(parse("#btn { opacity: 0.5; }"));
        let root = tree.root().unwrap();

        assert!(tree.apply_state_styles(root, false, false, false));
        let props = &tree.render_nodes.get(&root).unwrap().props;
        assert!(
            (props.opacity - 0.5).abs() < 0.001,
            "fallback path must produce the bound stylesheet's opacity, got {}",
            props.opacity
        );
    }

    #[test]
    fn table_path_wins_when_populated_and_fresh() {
        // Bound stylesheet says 0.5, table says 0.25. Table should win.
        let mut tree = build_btn_tree(1.0);
        tree.set_stylesheet(parse("#btn { opacity: 0.5; }"));
        let root = tree.root().unwrap();

        // Build the table from a different stylesheet — same id, distinct value.
        let table_source = parse("#btn { opacity: 0.25; }");
        let table = StateStyleTable::build(
            &table_source,
            std::iter::once(("btn".to_string(), tree.stable_id(root).unwrap())),
            tree.build_generation(),
        );
        assert!(table.is_populated(), "table must be populated");
        *tree.state_style_table.borrow_mut() = table;

        assert!(tree.apply_state_styles(root, false, false, false));
        let props = &tree.render_nodes.get(&root).unwrap().props;
        assert!(
            (props.opacity - 0.25).abs() < 0.001,
            "table path must produce the table's opacity, got {}",
            props.opacity
        );
    }

    #[test]
    fn set_stylesheet_rebinds_clear_stale_table_entries() {
        // Phase 5.3 contract: `set_stylesheet` rebuilds the table
        // against the new rules, so a follow-up `apply_state_styles`
        // never observes stale entries from the previous stylesheet.
        let mut tree = build_btn_tree(1.0);

        // First bind: stylesheet says opacity 0.5.
        tree.set_stylesheet(parse("#btn { opacity: 0.5; }"));
        let root = tree.root().unwrap();
        assert!(tree.apply_state_styles(root, false, false, false));
        assert!(
            (tree.render_nodes.get(&root).unwrap().props.opacity - 0.5).abs() < 0.001,
            "first bind must produce 0.5"
        );

        // Rebind: stylesheet now says 0.75. The table rebuild fired
        // inside `set_stylesheet` so the next apply must observe the
        // new value — no manual cache flush needed.
        tree.set_stylesheet(parse("#btn { opacity: 0.75; }"));
        assert!(tree.apply_state_styles(root, false, false, false));
        let after = tree.render_nodes.get(&root).unwrap().props.opacity;
        assert!(
            (after - 0.75).abs() < 0.001,
            "second bind must produce 0.75 (table rebuilt against new rules), got {}",
            after
        );
    }

    #[test]
    fn set_stylesheet_auto_populates_table() {
        // Phase 5.3 contract: simply binding a stylesheet to a tree
        // with registered ids triggers a table build. No manual
        // install required.
        let mut tree = build_btn_tree(1.0);
        assert!(
            !tree.state_style_table.borrow().is_populated(),
            "pre-bind: table empty"
        );

        tree.set_stylesheet(parse(
            "
            #btn { opacity: 0.5; }
            #btn:hover { opacity: 0.3; }
            ",
        ));

        let table = tree.state_style_table.borrow();
        assert!(
            table.is_populated(),
            "post-bind: table must be populated automatically"
        );
        // Should have one base + one hover entry.
        assert_eq!(table.base_entry_count(), 1);
        assert_eq!(table.state_entry_count(), 1);
    }

    #[test]
    fn set_stylesheet_arc_skips_rebuild_for_same_pointer() {
        // Phase 5.3 hot-path guard: the windowed runner re-binds the
        // cached stylesheet Arc on every incremental-update frame.
        // Without the Arc::ptr_eq guard inside `set_stylesheet_arc`
        // the entire ~500-rule cn cascade rebuilds per frame and the
        // P5.3 win flips to a regression (observed +30% p95 on
        // cn_demo before this guard landed).
        //
        // We don't have a direct hook to count rebuilds, but we can
        // mutate the table's contents after binding once and then
        // re-bind with the SAME Arc; if the guard works, our mutation
        // survives.
        use std::sync::Arc;

        let mut tree = build_btn_tree(1.0);
        let stylesheet = Arc::new(parse("#btn { opacity: 0.5; }"));
        tree.set_stylesheet_arc(stylesheet.clone());

        // Wipe the table so we can detect a rebuild firing.
        tree.state_style_table.borrow_mut().clear();
        assert!(!tree.state_style_table.borrow().is_populated());

        // Re-bind the SAME Arc — guard must short-circuit so the
        // empty table is preserved (no rebuild fires).
        tree.set_stylesheet_arc(stylesheet.clone());
        assert!(
            !tree.state_style_table.borrow().is_populated(),
            "same-Arc re-bind must NOT trigger rebuild (would be a hot-path regression)"
        );

        // Re-bind with a FRESH Arc → guard fails → rebuild fires.
        let fresh = Arc::new(parse("#btn { opacity: 0.5; }"));
        assert!(!Arc::ptr_eq(&stylesheet, &fresh));
        tree.set_stylesheet_arc(fresh);
        assert!(
            tree.state_style_table.borrow().is_populated(),
            "fresh-Arc bind must rebuild the table"
        );
    }

    #[test]
    fn auto_populated_table_makes_apply_state_styles_consult_it() {
        // Combined wiring test: with no manual table install, the
        // pre-resolved cascade must be consulted by apply_state_styles
        // after set_stylesheet. This is the headline P5.3 behaviour
        // — the table goes from "never populated" to "populated on
        // every cn_demo session".
        let mut tree = build_btn_tree(1.0);
        tree.set_stylesheet(parse(
            "
            #btn { opacity: 0.5; }
            #btn:hover { opacity: 0.3; }
            ",
        ));
        let root = tree.root().unwrap();

        // The discriminator here is indirect: if the table was NOT
        // consulted, this still passes because the bound stylesheet
        // produces the same values. Direct table-consultation
        // discrimination is covered by
        // `table_path_wins_when_populated_and_fresh`; this test
        // ensures the end-to-end wiring stays glued.
        assert!(tree.apply_state_styles(root, true, false, false));
        let opacity = tree.render_nodes.get(&root).unwrap().props.opacity;
        assert!(
            (opacity - 0.3).abs() < 0.001,
            "hover style must apply, got {}",
            opacity
        );
    }

    #[test]
    fn table_path_resolves_hover_state() {
        // Bound stylesheet: only base. Table: base + hover. Apply
        // with hovered=true and confirm the hover style was layered
        // — proving resolve_state_style consulted the table.
        let mut tree = build_btn_tree(1.0);
        tree.set_stylesheet(parse("#btn { opacity: 0.9; }"));
        let root = tree.root().unwrap();

        let table_source = parse(
            "
            #btn { opacity: 0.9; }
            #btn:hover { opacity: 0.25; }
            ",
        );
        let table = StateStyleTable::build(
            &table_source,
            std::iter::once(("btn".to_string(), tree.stable_id(root).unwrap())),
            tree.build_generation(),
        );
        *tree.state_style_table.borrow_mut() = table;

        assert!(tree.apply_state_styles(root, true, false, false));
        let props = &tree.render_nodes.get(&root).unwrap().props;
        assert!(
            (props.opacity - 0.25).abs() < 0.001,
            "hover-state table entry must override base, got {}",
            props.opacity
        );
    }
}
