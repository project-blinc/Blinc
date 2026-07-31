//! Visual + layout animation overlays on `RenderTree`.
//!
//! Two related-but-distinct subsystems live here:
//!
//! - **Layout animations** (the older FLIP-style API): per-node spring
//!   animations from old → new layout bounds. State lives in
//!   `layout_animations` (keyed by `LayoutNodeId`) and
//!   `layout_animations_by_key` (keyed by stable string id, survives
//!   subtree rebuilds). Reads via `get_animated_bounds` /
//!   `is_layout_animating`.
//!
//! - **Visual animations** (the newer `animate_bounds` / `motion-resize`
//!   API): per-node visual offsets layered on top of taffy bounds
//!   without modifying the layout tree itself. The pre-computed
//!   bounds get cached in `animated_render_bounds` once per frame
//!   (`compute_animated_render_bounds` walks from root and applies
//!   parent inheritance) so the paint walker reads them in O(1).
//!
//! `get_render_bounds` is the unified accessor — it tries visual
//! first (with parent-relative coords), then layout-anim (absolute,
//! deprecated path), then falls back to plain taffy bounds.
//!
//! FLIP transitions on subtree rebuild are a separate cluster — see
//! `flip.rs` in this directory. CSS keyframes / transitions are also
//! distinct (different storage, different tick path).

use std::collections::HashSet;

use blinc_core::Rect;

use crate::element::ElementBounds;
use crate::tree::LayoutNodeId;
use crate::visual_animation::{AnimatedRenderBounds, VisualAnimation, VisualAnimationConfig};

use super::super::RenderTree;

impl RenderTree {
    /// Check if any layout animations are currently active
    pub fn has_active_layout_animations(&self) -> bool {
        self.layout_animations
            .values()
            .any(|state| state.is_animating())
            || self
                .layout_animations_by_key
                .values()
                .any(|state| state.is_animating())
    }

    /// Get animated bounds for a node if a layout animation is active
    ///
    /// Returns the current animated bounds, or None if no animation is active.
    /// Checks both node ID based and stable key based animations.
    pub fn get_animated_bounds(&self, node_id: LayoutNodeId) -> Option<ElementBounds> {
        // First check node ID based animations
        if let Some(state) = self.layout_animations.get(&node_id) {
            return Some(state.current_bounds());
        }

        // Check stable key based animations
        // Look up if this node has a config with a stable key
        if let Some(config) = self.layout_animation_configs.get(&node_id) {
            if let Some(ref stable_key) = config.stable_key {
                if let Some(state) = self.layout_animations_by_key.get(stable_key) {
                    return Some(state.current_bounds());
                }
            }
        }

        None
    }

    /// Get bounds for rendering, using animated bounds if available
    ///
    /// This returns the animated bounds if a layout animation is active,
    /// otherwise returns the layout bounds from taffy.
    pub fn get_render_bounds(
        &self,
        node_id: LayoutNodeId,
        parent_offset: (f32, f32),
    ) -> Option<ElementBounds> {
        // Check if this node has an ACTIVE visual animation
        // Apply animation offsets to layout bounds (keeps parent-relative coordinates)
        if let Some(key) = self
            .visual_animation_key_to_node
            .iter()
            .find(|&(_, &n)| n == node_id)
            .map(|(k, _)| k.clone())
        {
            if let Some(anim) = self.visual_animations.get(&key) {
                if anim.is_animating() {
                    // Get layout bounds (relative to parent)
                    let layout = self.layout_tree.get_bounds(node_id, parent_offset)?;
                    // Apply animation offsets to layout bounds
                    return Some(ElementBounds {
                        x: layout.x + anim.offset.get_x(),
                        y: layout.y + anim.offset.get_y(),
                        width: (layout.width + anim.size_delta.get_width()).max(0.0),
                        height: (layout.height + anim.size_delta.get_height()).max(0.0),
                    });
                }
            }
        }

        // Then try old layout animations (node ID based) - deprecated
        if let Some(anim_bounds) = self.layout_animations.get(&node_id) {
            let current = anim_bounds.current_bounds();
            return Some(ElementBounds {
                x: current.x + parent_offset.0,
                y: current.y + parent_offset.1,
                width: current.width,
                height: current.height,
            });
        }

        // Try stable key based animations - deprecated
        if let Some(config) = self.layout_animation_configs.get(&node_id) {
            if let Some(ref stable_key) = config.stable_key {
                if let Some(anim_state) = self.layout_animations_by_key.get(stable_key) {
                    let current = anim_state.current_bounds();
                    return Some(ElementBounds {
                        x: current.x + parent_offset.0,
                        y: current.y + parent_offset.1,
                        width: current.width,
                        height: current.height,
                    });
                }
            }
        }

        // Fall back to layout bounds
        self.layout_tree.get_bounds(node_id, parent_offset)
    }

    /// Check if a specific node has an active layout animation
    pub fn is_layout_animating(&self, node_id: LayoutNodeId) -> bool {
        // Check node ID based
        if self
            .layout_animations
            .get(&node_id)
            .map(|s| s.is_animating())
            .unwrap_or(false)
        {
            return true;
        }

        // Check stable key based
        if let Some(config) = self.layout_animation_configs.get(&node_id) {
            if let Some(ref stable_key) = config.stable_key {
                if self
                    .layout_animations_by_key
                    .get(stable_key)
                    .map(|s| s.is_animating())
                    .unwrap_or(false)
                {
                    return true;
                }
            }
        }

        false
    }

    /// Register a visual animation config for a node
    ///
    /// This associates a VisualAnimationConfig with a node for FLIP-style animations.
    /// The animation tracks visual offsets from layout bounds, never modifying taffy.
    pub fn register_visual_animation_config(
        &mut self,
        node_id: LayoutNodeId,
        config: VisualAnimationConfig,
    ) {
        let key = config
            .key
            .clone()
            .unwrap_or_else(|| format!("node_{:?}", node_id));

        tracing::trace!(
            "[VISUAL_ANIM] Registering config: node={:?}, key={}",
            node_id,
            key
        );

        self.visual_animation_configs.insert(key.clone(), config);
        // key→node direction ensures we always have the current node for each key
        // (overwrites any stale node_id from previous rebuild)
        self.visual_animation_key_to_node.insert(key, node_id);
    }

    /// Update visual animations for nodes with changed bounds
    ///
    /// This implements the FLIP technique:
    /// 1. Compare current layout bounds vs previous visual bounds
    /// 2. Calculate offset = previous - current (the "Invert" step)
    /// 3. Create animation that plays offset back to 0 (the "Play" step)
    ///
    /// Called after layout computation but before rendering.
    pub(crate) fn update_visual_animations(&mut self) {
        if !self.visual_animation_configs.is_empty() {
            tracing::trace!(
                "[VISUAL_ANIM] update_visual_animations: {} configs registered",
                self.visual_animation_configs.len()
            );
        }
        if self.visual_animation_configs.is_empty() {
            return;
        }

        // Get animation scheduler handle
        let scheduler_handle = if let Some(arc) = self.animations.upgrade() {
            arc.lock().unwrap().handle()
        } else if let Some(handle) = crate::render_state::get_global_scheduler() {
            handle
        } else {
            tracing::warn!("No animation scheduler available for visual animations");
            return;
        };

        // Process each registered config
        for (key, config) in &self.visual_animation_configs {
            // Get the current node ID for this key (directly from key→node map)
            let Some(&node_id) = self.visual_animation_key_to_node.get(key) else {
                continue;
            };

            // Get current layout bounds from taffy
            let Some(layout_bounds) = self.layout_tree.get_bounds(node_id, (0.0, 0.0)) else {
                continue;
            };

            // Get previous visual bounds (what was rendered last frame)
            let prev_visual = self.previous_visual_bounds.get(key).copied();

            // Check if we have an existing animation
            if let Some(existing_anim) = self.visual_animations.get_mut(key) {
                // Animation in progress - update target if layout changed
                if (existing_anim.to_bounds.width - layout_bounds.width).abs() > 0.5
                    || (existing_anim.to_bounds.height - layout_bounds.height).abs() > 0.5
                    || (existing_anim.to_bounds.x - layout_bounds.x).abs() > 0.5
                    || (existing_anim.to_bounds.y - layout_bounds.y).abs() > 0.5
                {
                    tracing::debug!(
                        "Visual animation: updating target for key='{}', to_bounds changed",
                        key
                    );
                    existing_anim.update_target(layout_bounds, scheduler_handle.clone());
                }

                // Store current visual bounds for next frame
                let current_visual = existing_anim.current_visual_bounds();
                self.previous_visual_bounds
                    .insert(key.clone(), current_visual);
            } else if let Some(prev) = prev_visual {
                // No animation yet - check if bounds changed significantly
                let bounds_changed = (prev.width - layout_bounds.width).abs() > config.threshold
                    || (prev.height - layout_bounds.height).abs() > config.threshold
                    || (prev.x - layout_bounds.x).abs() > config.threshold
                    || (prev.y - layout_bounds.y).abs() > config.threshold;

                if bounds_changed {
                    // Create new FLIP animation: from prev visual, to current layout
                    if let Some(anim) = VisualAnimation::from_bounds_change(
                        key.clone(),
                        prev,
                        layout_bounds,
                        config,
                        scheduler_handle.clone(),
                    ) {
                        tracing::debug!(
                            "Visual animation: created for key='{}', from={:?} to={:?}, direction={:?}",
                            key,
                            prev,
                            layout_bounds,
                            anim.direction
                        );
                        self.visual_animations.insert(key.clone(), anim);
                    }
                }

                // Store current visual bounds for next frame
                // (use layout since no animation is active)
                self.previous_visual_bounds
                    .insert(key.clone(), layout_bounds);
            } else {
                // First frame - just store current layout bounds
                self.previous_visual_bounds
                    .insert(key.clone(), layout_bounds);
            }
        }

        // Cleanup completed animations
        self.visual_animations.retain(|key, anim| {
            let is_animating = anim.is_animating();
            if !is_animating {
                tracing::debug!(
                    "Visual animation: cleaning up completed animation for key='{}'",
                    key
                );
            }
            is_animating
        });
    }

    /// Compute animated render bounds for all nodes
    ///
    /// This is the hierarchical computation phase:
    /// 1. Start from root with identity bounds
    /// 2. For each node, calculate its animated bounds accounting for:
    ///    - Own animation state (if any)
    ///    - Parent's animated offset (inherited)
    /// 3. Store pre-computed bounds for use during rendering
    ///
    /// Called after update_visual_animations().
    pub(crate) fn compute_animated_render_bounds(&mut self) {
        // Clear previous computation
        self.animated_render_bounds.clear();

        // Get root node
        let Some(root_id) = self.root else {
            return;
        };

        // Start recursive computation from root
        self.compute_bounds_recursive(
            root_id, 0.0, 0.0, // Parent offset starts at screen origin
        );
    }

    /// Recursively compute animated render bounds for a subtree
    fn compute_bounds_recursive(&mut self, node_id: LayoutNodeId, parent_x: f32, parent_y: f32) {
        // Get layout bounds relative to parent (from taffy)
        let Some(layout_bounds) = self.layout_tree.get_bounds(node_id, (0.0, 0.0)) else {
            return;
        };

        // Check if this node has an active visual animation
        // Find the key for this node (reverse lookup - O(n) but typically few animated nodes)
        let stable_key = self
            .visual_animation_key_to_node
            .iter()
            .find(|&(_, &n)| n == node_id)
            .map(|(k, _)| k.clone());
        let animation = stable_key
            .as_ref()
            .and_then(|k| self.visual_animations.get(k));
        let config = stable_key
            .as_ref()
            .and_then(|k| self.visual_animation_configs.get(k));

        // Calculate this node's animated bounds
        let animated_bounds = if let Some(anim) = animation {
            // Node has active animation - apply visual offset
            let dx = anim.offset.get_x();
            let dy = anim.offset.get_y();
            let dw = anim.size_delta.get_width();
            let dh = anim.size_delta.get_height();

            // Position: layout + parent offset + animation offset
            let x = parent_x + layout_bounds.x + dx;
            let y = parent_y + layout_bounds.y + dy;

            // Size: layout + animation delta
            let width = layout_bounds.width + dw;
            let height = layout_bounds.height + dh;

            // Determine clip rect based on animation direction and config
            let clip_rect = if let Some(cfg) = config {
                use crate::visual_animation::ClipBehavior;
                match cfg.clip_behavior {
                    ClipBehavior::ClipToAnimated => {
                        // Clip to animated (current) bounds
                        Some(Rect::new(0.0, 0.0, width.max(0.0), height.max(0.0)))
                    }
                    ClipBehavior::ClipToLayout => {
                        // Clip to layout (target) bounds
                        Some(Rect::new(
                            0.0,
                            0.0,
                            layout_bounds.width,
                            layout_bounds.height,
                        ))
                    }
                    ClipBehavior::NoClip => None,
                }
            } else {
                // Default: clip to animated bounds
                Some(Rect::new(0.0, 0.0, width.max(0.0), height.max(0.0)))
            };

            AnimatedRenderBounds {
                x,
                y,
                width,
                height,
                clip_rect,
            }
        } else {
            // No animation - use layout bounds + parent offset
            AnimatedRenderBounds {
                x: parent_x + layout_bounds.x,
                y: parent_y + layout_bounds.y,
                width: layout_bounds.width,
                height: layout_bounds.height,
                clip_rect: None,
            }
        };

        // Store computed bounds
        let child_parent_x = animated_bounds.x;
        let child_parent_y = animated_bounds.y;
        self.animated_render_bounds.insert(node_id, animated_bounds);

        // Recursively compute for children
        // Children use THIS node's animated position as their parent offset
        let children = self.layout_tree.children(node_id);
        for child_id in children {
            self.compute_bounds_recursive(child_id, child_parent_x, child_parent_y);
        }
    }

    /// Get visual animated render bounds for a node
    ///
    /// Returns pre-computed animated bounds if available, otherwise None.
    /// Use this during rendering to get hierarchically-correct animated positions.
    pub fn get_visual_render_bounds(&self, node_id: LayoutNodeId) -> Option<&AnimatedRenderBounds> {
        self.animated_render_bounds.get(&node_id)
    }

    /// Check if any visual animations are currently active
    pub fn has_active_visual_animations(&self) -> bool {
        self.visual_animations.values().any(|a| a.is_animating())
    }

    /// Visibility-gated counterpart of `has_active_visual_animations`.
    /// Only counts animations whose target node was painted in the
    /// most recent frame (i.e. the node id is present in `painted`).
    /// See `CssAnimationStore::has_visible_active` for the rationale.
    pub fn has_active_visible_visual_animations(&self, painted: &HashSet<LayoutNodeId>) -> bool {
        self.visual_animations.iter().any(|(key, anim)| {
            if !anim.is_animating() {
                return false;
            }
            self.visual_animation_key_to_node
                .get(key)
                .is_some_and(|n| painted.contains(n))
        })
    }

    /// Check if a specific node has an active visual animation
    pub fn is_visual_animating(&self, node_id: LayoutNodeId) -> bool {
        // Find the key for this node (reverse lookup)
        self.visual_animation_key_to_node
            .iter()
            .find(|&(_, &n)| n == node_id)
            .and_then(|(key, _)| self.visual_animations.get(key))
            .map(|a| a.is_animating())
            .unwrap_or(false)
    }

    /// Single source of truth for "is any animation system live this
    /// frame?".
    ///
    /// Returns `true` if any of the following has a mid-flight tick:
    ///
    /// - `MotionBindings` springs / always-playing rotation
    ///   timelines (`is_any_animating`)
    /// - `motion()` enter / exit FSM
    ///   (`render_state.has_active_motions()`)
    /// - Visual animations (`animate_bounds`)
    /// - Layout animations (`animate_layout`)
    /// - CSS keyframe animations
    /// - CSS property transitions
    /// - FLIP transitions
    ///
    /// Used by the compositor cache-invalidation gate and the
    /// `visible_anim_active` fast-path bookkeeping in `blinc_app`,
    /// plus the windowed-runner redraw chain. Every animation
    /// driver registers here so the three sites stay in lockstep —
    /// adding a new driver only requires extending this method,
    /// not chasing OR-chains in three different files.
    ///
    /// Visibility filtering is the caller's job; this returns the
    /// global "anything alive" answer. See
    /// [`Self::has_any_active_animation_visible`] for the
    /// painted-set-gated variant the windowed redraw chain uses.
    pub fn has_any_active_animation(
        &self,
        render_state: &crate::render_state::RenderState,
    ) -> bool {
        // MotionBindings (springs + always-playing rotation_timelines).
        if self.motion_bindings.values().any(|b| b.is_any_animating()) {
            return true;
        }
        // motion() FSM enter / exit.
        if render_state.has_active_motions() {
            return true;
        }
        // animate_bounds / animate_layout.
        if self.has_active_visual_animations() || self.has_active_layout_animations() {
            return true;
        }
        // FLIP transitions (animate_layout's modern counterpart).
        if self.has_active_flip_animations() {
            return true;
        }
        // CSS keyframe animations + CSS property transitions.
        if let Ok(store) = self.css_anim_store.lock() {
            if store.has_active_animations() || store.has_active_transitions() {
                return true;
            }
        }
        false
    }

    /// Visibility-gated counterpart of
    /// [`Self::has_any_active_animation`] — only counts animations
    /// whose owning node is in `painted` (the set the walker
    /// emitted this frame). Used by the windowed redraw chain so an
    /// off-screen keyframe doesn't keep request_redraw firing at
    /// vsync forever.
    ///
    /// `MotionBindings` are not visibility-filtered here — they're
    /// keyed by `LayoutNodeId` directly, and the walker's
    /// `painted_node_ids` already excludes off-screen ones, so
    /// `is_any_animating` only returns true for visible bindings in
    /// practice. Same posture as the pre-existing windowed gate.
    pub fn has_any_active_animation_visible(
        &self,
        render_state: &crate::render_state::RenderState,
        painted: &HashSet<LayoutNodeId>,
        painted_stable: &HashSet<crate::tree::StableNodeId>,
    ) -> bool {
        // Culling diagnostic. An animating binding that is NOT in the
        // painted set is one the visibility gate just saved us from; an
        // animating binding that IS painted legitimately drives frames.
        // If a scrolled-away spinner still shows up as painted, the
        // painted set is the thing that is wrong, not this predicate.
        {
            let animating = self
                .motion_bindings
                .iter()
                .filter(|(_, b)| b.is_any_animating())
                .count();
            // Rate-limited: this runs per frame, and one line per frame
            // at 60Hz buries everything else.
            static TICK: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            if animating > 0 && TICK.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % 120 == 0 {
                let visible = self
                    .motion_bindings
                    .iter()
                    .filter(|(n, b)| painted.contains(n) && b.is_any_animating())
                    .count();
                tracing::debug!(
                    animating,
                    visible,
                    painted = painted.len(),
                    unresolved_bounds = crate::renderer::paint::motion::UNRESOLVED_BOUNDS
                        .swap(0, std::sync::atomic::Ordering::Relaxed),
                    anim_on_screen_y = crate::renderer::paint::motion::LAST_ANIMATING_ON_SCREEN_Y
                        .load(std::sync::atomic::Ordering::Relaxed),
                    scroll_y = crate::renderer::paint::motion::LAST_ANIMATING_SCROLL_Y
                        .load(std::sync::atomic::Ordering::Relaxed),
                    // WHICH term keeps the gate true. The motion-binding
                    // term is painted-gated and reads false here, but
                    // `has_active_motions` and `has_active_layout_animations`
                    // are NOT gated by visibility at all — either one
                    // returning true pins the redraw chain regardless of
                    // what is on screen.
                    t_motions = render_state.has_active_motions(),
                    t_visual = self.has_active_visible_visual_animations(painted),
                    t_layout = self.has_active_layout_animations(),
                    t_flip = self.has_active_visible_flip_animations(painted),
                    t_css = self
                        .css_anim_store
                        .lock()
                        .map(|s| s.has_visible_active(painted_stable))
                        .unwrap_or(false),
                    "motion culling: animating bindings, of which painted"
                );
            }
        }

        if self
            .motion_bindings
            .iter()
            .any(|(n, b)| painted.contains(n) && b.is_any_animating())
        {
            return true;
        }
        if render_state.has_active_motions() {
            return true;
        }
        if self.has_active_visible_visual_animations(painted) {
            return true;
        }
        if self.has_active_layout_animations() {
            return true;
        }
        if self.has_active_visible_flip_animations(painted) {
            return true;
        }
        if let Ok(store) = self.css_anim_store.lock() {
            if store.has_visible_active(painted_stable) {
                return true;
            }
        }
        false
    }
}
