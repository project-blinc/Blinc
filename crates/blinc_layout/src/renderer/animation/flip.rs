//! FLIP-style transitions on `RenderTree`.
//!
//! "First / Last / Invert / Play": when a subtree rebuild moves
//! existing elements, FLIP captures their positions before the
//! rebuild (`update_flip_bounds`), inverts the difference into a
//! `translate()` keyframe, and plays it back to zero so the user
//! sees a smooth transition instead of an instant jump.
//!
//! Animations are keyed by **stable element id** (not `LayoutNodeId`)
//! so they survive rebuilds that destroy + recreate the underlying
//! node. `apply_flip_animation_props` resolves id→node every frame
//! via `element_registry`; if the id is gone we drop the entry on
//! the next `tick`.
//!
//! Only triggers on elements with a CSS `transition` on `transform`
//! (or the `all` shorthand). Elements that have an
//! app-controlled transform (e.g. an actively-dragged item) are
//! skipped — FLIP only animates passive reflow.

use std::collections::HashSet;

use crate::tree::LayoutNodeId;

use super::super::RenderTree;

impl RenderTree {
    /// Update persistent element bounds for FLIP tracking.
    ///
    /// Called after every `compute_layout()` to record current absolute positions
    /// by element string ID. This data survives across subtree rebuilds since it's
    /// keyed by stable string IDs, not volatile LayoutNodeIds.
    pub fn update_flip_bounds(&mut self) {
        self.flip_previous_bounds.clear();
        for (node_id, _render_node) in &self.render_nodes {
            if let Some(element_id) = self.element_registry.get_id(*node_id) {
                if let Some(bounds) = self.layout_tree.get_absolute_bounds(*node_id) {
                    self.flip_previous_bounds.insert(element_id, bounds);
                }
            }
        }
    }

    /// Apply FLIP transitions for elements that moved during subtree rebuild.
    ///
    /// Called AFTER layout recomputation (in windowed.rs, after compute_layout()).
    /// Compares new layout positions to snapshots taken before the rebuild.
    /// For elements that moved AND have a CSS `transition` on `transform`,
    /// creates a CSS transition from `translate(dx, dy)` to `translate(0, 0)`.
    pub fn apply_flip_transitions(&mut self) {
        tracing::trace!(
            "FLIP: apply_flip_transitions called, flip_previous_bounds has {} entries",
            self.flip_previous_bounds.len()
        );
        if self.flip_previous_bounds.is_empty() {
            return;
        }

        let stylesheet = self.stylesheet.clone();

        // Collect elements that moved: compare previous bounds with current absolute bounds
        let mut moved: Vec<(String, f32, f32, crate::tree::LayoutNodeId)> = Vec::new();

        for (node_id, render_node) in &self.render_nodes {
            // Skip elements that already have a transform set by the app (e.g. the dragged item).
            // FLIP should only animate elements whose position changed passively due to reflow,
            // not elements being actively positioned via Transform::translate().
            if render_node.props.transform.is_some() {
                continue;
            }

            let Some(element_id) = self.element_registry.get_id(*node_id) else {
                continue;
            };
            let Some(old_bounds) = self.flip_previous_bounds.get(&element_id) else {
                continue;
            };
            let Some(new_bounds) = self.layout_tree.get_absolute_bounds(*node_id) else {
                continue;
            };

            let dx = old_bounds.x - new_bounds.x;
            let dy = old_bounds.y - new_bounds.y;

            if dx.abs() < 1.0 && dy.abs() < 1.0 {
                continue;
            }

            tracing::trace!("FLIP: '{}' moved: delta=({:.1},{:.1})", element_id, dx, dy);
            moved.push((element_id, dx, dy, *node_id));
        }

        if moved.is_empty() {
            return;
        }

        tracing::debug!("FLIP: {} elements moved, creating transitions", moved.len());

        for (element_id, dx, dy, new_node_id) in &moved {
            // Find CSS transition spec for "transform" (CssTransitionSet::get also matches "all")
            // Check: 1) ID-based style, 2) class-based complex selector rules
            let transition = stylesheet.as_ref().and_then(|ss| {
                // Check by element ID first
                ss.get(element_id)
                    .and_then(|style| style.transition.as_ref())
                    .and_then(|ts| ts.get("transform"))
                    .or_else(|| {
                        // Check class-based complex selector rules
                        let empty = std::collections::HashSet::new();
                        for rule in ss.complex_rules() {
                            if rule.0.has_state() {
                                continue;
                            }
                            if self.complex_selector_matches(
                                &rule.0,
                                *new_node_id,
                                &empty,
                                &empty,
                                None,
                            ) {
                                if let Some(ts) = rule.1.transition.as_ref() {
                                    if let Some(t) = ts.get("transform") {
                                        return Some(t);
                                    }
                                }
                            }
                        }
                        None
                    })
            });

            let Some(transition) = transition else {
                tracing::trace!(
                    "FLIP: '{}' has no CSS transition for 'transform'",
                    element_id
                );
                continue;
            };

            let duration_ms = transition.duration_ms;
            let delay_ms = transition.delay_ms;
            let easing = transition.timing.to_easing();

            if duration_ms == 0 {
                continue;
            }

            tracing::debug!(
                "FLIP: '{}' translate({:.1}, {:.1}) → (0,0) over {}ms",
                element_id,
                dx,
                dy,
                duration_ms
            );

            // Create a CSS transition from translate(dx, dy) to translate(0, 0)
            use blinc_animation::{FillMode, KeyframeProperties, MultiKeyframeAnimation};

            let from = KeyframeProperties {
                translate_x: Some(*dx),
                translate_y: Some(*dy),
                ..Default::default()
            };

            let to = KeyframeProperties {
                translate_x: Some(0.0),
                translate_y: Some(0.0),
                ..Default::default()
            };

            let anim = MultiKeyframeAnimation::new(duration_ms)
                .keyframe(0.0, from, easing)
                .keyframe(1.0, to, easing)
                .delay(delay_ms)
                .fill_mode(FillMode::Forwards);

            // Store in flip_animations keyed by string ID (survives subtree rebuilds).
            // Unlike css_anim_store.transitions which are keyed by LayoutNodeId,
            // these persist when nodes are recreated because we resolve to the
            // current LayoutNodeId at apply time via element_registry.
            self.flip_animations.insert(
                element_id.clone(),
                crate::render_state::ActiveCssAnimation::new(anim),
            );
        }
    }

    /// Tick all active FLIP animations by `dt_ms` milliseconds.
    /// Removes completed animations. Returns `true` if any are still playing.
    pub fn tick_flip_animations(&mut self, dt_ms: f32) -> bool {
        if self.flip_animations.is_empty() {
            return false;
        }
        let dt_ms = dt_ms.min(crate::render_state::MAX_ANIMATION_STEP_MS);
        let mut any_playing = false;
        for anim in self.flip_animations.values_mut() {
            if anim.tick(dt_ms) {
                any_playing = true;
            }
        }
        // Remove completed animations
        self.flip_animations.retain(|_, a| a.is_playing);
        any_playing
    }

    /// Apply current FLIP animation values to render props.
    /// Resolves string element IDs → LayoutNodeIds via element_registry.
    pub fn apply_flip_animation_props(&mut self) {
        if self.flip_animations.is_empty() {
            return;
        }
        // Collect data first to avoid borrow conflict with render_nodes
        let props_data: Vec<(
            crate::tree::LayoutNodeId,
            blinc_animation::KeyframeProperties,
        )> = self
            .flip_animations
            .iter()
            .filter_map(|(element_id, anim)| {
                let node_id = self.element_registry.get(element_id)?;
                Some((node_id, anim.current_properties.clone()))
            })
            .collect();

        for (node_id, anim_props) in props_data {
            if let Some(render_node) = self.render_nodes.get_mut(&node_id) {
                Self::apply_keyframe_props_to_render(&mut render_node.props, &anim_props);
            }
        }
    }

    /// Check if any FLIP animations are currently active.
    pub fn has_active_flip_animations(&self) -> bool {
        !self.flip_animations.is_empty()
    }

    /// Visibility-gated counterpart of `has_active_flip_animations`.
    /// FLIP entries are keyed by `element_id: String` (they survive
    /// subtree rebuilds), so resolve to the current `LayoutNodeId`
    /// through the element registry before checking visibility.
    /// Element ids without a current binding are skipped — if the
    /// node is gone, the animation can't be on screen anyway.
    pub fn has_active_visible_flip_animations(&self, painted: &HashSet<LayoutNodeId>) -> bool {
        self.flip_animations.keys().any(|element_id| {
            self.element_registry
                .get(element_id)
                .is_some_and(|nid| painted.contains(&nid))
        })
    }
}
