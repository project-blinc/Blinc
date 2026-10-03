//! CSS keyframe animation + transition machinery on `RenderTree`.
//!
//! Three responsibilities:
//!
//! - **Lifecycle**: `start_css_animation_for_element`,
//!   `start_css_animation_for_state`, `stop_css_animation`,
//!   `start_all_css_animations`. Each starts a `MultiKeyframeAnimation`
//!   from the stylesheet's `animation:` declaration and registers it
//!   in the shared `CssAnimationStore`.
//! - **Per-frame application**: `apply_all_css_animation_props`,
//!   `apply_all_css_transition_props`, `apply_css_animation_to_props`,
//!   `apply_keyframe_props_to_render`. These advance the time-driven
//!   value into the target node's `RenderProps`.
//! - **Transition detection**: `detect_and_start_transitions`,
//!   `render_props_to_keyframe_properties`,
//!   `remove_completed_transitions`. Compares before/after snapshots
//!   to start a `transition:`-driven `ActiveCssAnimation`.
//!
//! Transition snapshots are taken in `mod.rs` via
//! `snapshot_keyframe_properties` / `snapshot_before_keyframe_properties`
//! before / after the state-style apply pass. Those still live in
//! `mod.rs` because the snapshot is part of the stylesheet flow,
//! not the CSS-anim flow itself; they'd move with `stylesheet.rs`.

use blinc_core::{BlurQuality, LayerEffect, Shadow};

use crate::css_parser::ElementState;
use crate::element::{GlassMaterial, Material, RenderLayer, RenderProps};
use crate::tree::LayoutNodeId;

use super::super::RenderTree;

impl RenderTree {
    pub fn start_css_animation_for_element(&mut self, node_id: LayoutNodeId) -> bool {
        let stylesheet = match &self.stylesheet {
            Some(s) => s.clone(),
            None => return false,
        };

        let element_id = match self.element_registry.get_id(node_id) {
            Some(id) => id,
            None => return false,
        };

        // Resolve the full keyframe animation
        if let Some(animation) = stylesheet.resolve_keyframe_animation(&element_id) {
            let stable_id = self.stable_id_or_warn(node_id);
            self.css_anim_store.lock().unwrap().animations.insert(
                stable_id,
                crate::render_state::ActiveCssAnimation::new(animation),
            );
            return true;
        }

        false
    }

    /// Start a CSS keyframe animation for a node driven by a class rule.
    ///
    /// `start_css_animation_for_element` only looks up `#id` rules,
    /// so class-only animations like `.cn-popover-content {
    /// animation: cn-popover-enter ... }` never started even though
    /// the rule existed in the stylesheet. This helper bridges the
    /// gap: given a `node_id` whose class has an `animation` rule,
    /// it resolves the keyframes block and registers the
    /// `ActiveCssAnimation` against the node's stable id.
    pub fn start_css_animation_for_class(&mut self, node_id: LayoutNodeId, class: &str) -> bool {
        let stylesheet = match &self.stylesheet {
            Some(s) => s.clone(),
            None => return false,
        };
        let Some(style) = stylesheet.get_class(class) else {
            return false;
        };
        let Some(anim_config) = style.animation.as_ref() else {
            return false;
        };
        let Some(keyframes) = stylesheet.get_keyframes(&anim_config.name) else {
            return false;
        };

        let mut animation = keyframes
            .to_multi_keyframe_animation(anim_config.duration_ms, anim_config.timing.to_easing());
        let iterations = if anim_config.iteration_count == 0 {
            -1
        } else {
            anim_config.iteration_count as i32
        };
        animation.set_iterations(iterations);
        animation.set_delay(anim_config.delay_ms);
        animation.set_direction(anim_config.direction.to_play_direction());
        animation.set_fill_mode(anim_config.fill_mode.to_fill_mode());
        if anim_config.direction.starts_reversed() {
            animation.set_reversed(true);
        }

        let stable_id = self.stable_id_or_warn(node_id);
        self.css_anim_store.lock().unwrap().animations.insert(
            stable_id,
            crate::render_state::ActiveCssAnimation::new(animation),
        );
        true
    }

    /// Start a CSS keyframe animation for a node with a specific state
    ///
    /// Used when element state changes (hover, active, etc.) and the state
    /// has an animation defined.
    pub fn start_css_animation_for_state(
        &mut self,
        node_id: LayoutNodeId,
        state: ElementState,
    ) -> bool {
        let stylesheet = match &self.stylesheet {
            Some(s) => s.clone(),
            None => return false,
        };

        let element_id = match self.element_registry.get_id(node_id) {
            Some(id) => id,
            None => return false,
        };

        // Resolve animation for the specific state
        if let Some(animation) =
            stylesheet.resolve_keyframe_animation_with_state(&element_id, state)
        {
            let stable_id = self.stable_id_or_warn(node_id);
            self.css_anim_store.lock().unwrap().animations.insert(
                stable_id,
                crate::render_state::ActiveCssAnimation::new(animation),
            );
            return true;
        }

        false
    }

    /// Apply current CSS animation values to a node's render props
    ///
    /// Call this during rendering to apply animated values.
    pub fn apply_css_animation_to_props(&self, node_id: LayoutNodeId, props: &mut RenderProps) {
        let Some(stable_id) = self.stable_id(node_id) else {
            return;
        };
        let store = self.css_anim_store.lock().unwrap();
        if let Some(active_anim) = store.animations.get(&stable_id) {
            let anim_props = &active_anim.current_properties;

            // Apply animated opacity
            if let Some(opacity) = anim_props.opacity {
                props.opacity = opacity;
            }

            // Apply animated transform components
            let has_transform = anim_props.translate_x.is_some()
                || anim_props.translate_y.is_some()
                || anim_props.scale_x.is_some()
                || anim_props.scale_y.is_some()
                || anim_props.rotate.is_some();

            if has_transform {
                use blinc_core::{Affine2D, Transform};

                // Build transform by composing: scale -> rotate -> translate
                let mut affine = Affine2D::IDENTITY;

                // Apply scale
                if let (Some(sx), Some(sy)) = (anim_props.scale_x, anim_props.scale_y) {
                    affine = affine.then(&Affine2D::scale(sx, sy));
                } else if let Some(sx) = anim_props.scale_x {
                    affine = affine.then(&Affine2D::scale(sx, 1.0));
                } else if let Some(sy) = anim_props.scale_y {
                    affine = affine.then(&Affine2D::scale(1.0, sy));
                }

                // Apply rotation (in degrees)
                if let Some(rotate) = anim_props.rotate {
                    affine = affine.then(&Affine2D::rotation(rotate.to_radians()));
                }

                // Apply translation
                if let (Some(tx), Some(ty)) = (anim_props.translate_x, anim_props.translate_y) {
                    affine = affine.then(&Affine2D::translation(tx, ty));
                } else if let Some(tx) = anim_props.translate_x {
                    affine = affine.then(&Affine2D::translation(tx, 0.0));
                } else if let Some(ty) = anim_props.translate_y {
                    affine = affine.then(&Affine2D::translation(0.0, ty));
                }

                props.transform = Some(Transform::Affine2D(affine));
            }
        }
    }

    /// Get current CSS animation properties for a node (if any)
    pub fn get_css_animation_properties(
        &self,
        node_id: LayoutNodeId,
    ) -> Option<blinc_animation::KeyframeProperties> {
        let stable_id = self.stable_id(node_id)?;
        let store = self.css_anim_store.lock().unwrap();
        store
            .animations
            .get(&stable_id)
            .map(|a| a.current_properties.clone())
    }

    /// Check if a node has an active CSS animation
    pub fn has_css_animation(&self, node_id: LayoutNodeId) -> bool {
        let Some(stable_id) = self.stable_id(node_id) else {
            return false;
        };
        let store = self.css_anim_store.lock().unwrap();
        store
            .animations
            .get(&stable_id)
            .map(|a| a.is_playing)
            .unwrap_or(false)
    }

    /// Stop CSS animation for a node
    pub fn stop_css_animation(&mut self, node_id: LayoutNodeId) {
        let Some(stable_id) = self.stable_id(node_id) else {
            return;
        };
        self.css_anim_store
            .lock()
            .unwrap()
            .animations
            .remove(&stable_id);
    }

    /// Check if there are no active CSS animations
    pub fn css_animations_empty(&self) -> bool {
        self.css_anim_store.lock().unwrap().animations.is_empty()
    }

    /// Check if the CSS animation store has any active work (animations or transitions)
    pub fn css_has_active(&self) -> bool {
        let store = self.css_anim_store.lock().unwrap();
        store.has_active_animations() || store.has_active_transitions()
    }

    /// Whether every currently-playing CSS animation / transition on
    /// a *visible* node is composite-promotable per
    /// [`blinc_animation::KeyframeProperties::is_composite_promotable`].
    ///
    /// Used by the windowed frame loop's fast-path gate: when this is
    /// true and there's no other dynamic activity, the walker can be
    /// skipped — the cached layer textures already hold the subtree
    /// pixels at base state, and the per-frame composite step
    /// (`composite_css_layers_overlay`) blits them with the current
    /// translate / scale / opacity applied at compose time.
    ///
    /// **Viewport gating**: animations on nodes that didn't paint
    /// during the most recent walker pass (i.e. were scrolled
    /// off-screen or otherwise culled) are ignored. The CSS animation
    /// store keeps ticking them — their `current_properties` keep
    /// advancing and `apply_all_css_animation_props` keeps mutating
    /// `render_node.props` — but their pixel output isn't on the
    /// surface this frame, so they don't block the fast path. The
    /// moment one scrolls back into view, the scroll input
    /// invalidates the cache (see `windowed::Event::Input` ->
    /// `app.invalidate_render_cache_tagged("had_scroll")`), forcing
    /// a slow paint that re-walks the now-visible node.
    ///
    /// Returns `true` when no visible animation is playing (vacuously
    /// satisfied) so callers can use it as a single predicate without
    /// also checking `css_has_active`.
    pub fn css_active_all_composite_promotable(&self) -> bool {
        let painted = self.painted_node_ids.borrow();
        let store = self.css_anim_store.lock().unwrap();
        for (stable, anim) in store.animations.iter() {
            if !anim.is_playing {
                continue;
            }
            let Some(layout) = self.stable_to_layout.get(stable).copied() else {
                continue;
            };
            if !painted.contains(&layout) {
                continue;
            }
            // Layout-affecting animations on a *visible* node force
            // the slow path: `apply_animated_layout_props` will write
            // to taffy and `compute_layout` will shift bounds the
            // cache hasn't captured. Off-screen layout animations
            // (e.g. styling_demo's `grow-shrink` / `constraint-pulse`
            // ticking in unrelated sections) are tolerated — their
            // bounds shift affects only their own off-screen flex
            // container, and a scroll input invalidates the cache
            // before the user can see the discrepancy.
            if Self::keyframe_props_affect_layout(&anim.current_properties) {
                return false;
            }
            if !anim.current_properties.is_composite_promotable() {
                return false;
            }
        }
        for (stable, trans) in store.transitions.iter() {
            if !trans.is_playing {
                continue;
            }
            let Some(layout) = self.stable_to_layout.get(stable).copied() else {
                continue;
            };
            if !painted.contains(&layout) {
                continue;
            }
            if Self::keyframe_props_affect_layout(&trans.current_properties) {
                return false;
            }
            if !trans.current_properties.is_composite_promotable() {
                return false;
            }
        }
        true
    }

    /// True iff the keyframe properties touch any taffy style that
    /// would force a `compute_layout` (and therefore potentially
    /// reshuffle visible nodes via flex / grid). Mirrors the
    /// `has_layout_props` check inside `apply_animated_layout_props`.
    fn keyframe_props_affect_layout(p: &blinc_animation::KeyframeProperties) -> bool {
        p.width.is_some()
            || p.height.is_some()
            || p.padding.is_some()
            || p.margin.is_some()
            || p.gap.is_some()
            || p.min_width.is_some()
            || p.max_width.is_some()
            || p.min_height.is_some()
            || p.max_height.is_some()
            || p.flex_grow.is_some()
            || p.flex_shrink.is_some()
            || p.inset_top.is_some()
            || p.inset_right.is_some()
            || p.inset_bottom.is_some()
            || p.inset_left.is_some()
    }

    /// Start CSS animations for all registered elements that have animations defined
    ///
    /// Scans all registered element IDs, checks the stylesheet for animation properties,
    /// and starts `ActiveCssAnimation` instances for each matching element.
    pub fn start_all_css_animations(&mut self) {
        if self.stylesheet.is_none() {
            return;
        }

        let registered_ids: Vec<(String, LayoutNodeId)> = self
            .element_registry
            .all_ids()
            .into_iter()
            .filter_map(|id| self.element_registry.get(&id).map(|node_id| (id, node_id)))
            .collect();

        for (element_id, node_id) in &registered_ids {
            // Skip if already has an active animation (lock briefly, release before calling start)
            let Some(stable_id) = self.stable_id(*node_id) else {
                continue;
            };
            let already_has = self
                .css_anim_store
                .lock()
                .unwrap()
                .animations
                .contains_key(&stable_id);
            if already_has {
                continue;
            }
            let started = self.start_css_animation_for_element(*node_id);
            if started {
                tracing::debug!(
                    "CSS animation started for element '{}' (node={:?})",
                    element_id,
                    node_id
                );
            }
        }

        // Class-based animations: iterate the registry's class index and
        // start any class style that carries an `animation:` rule.
        // Without this, `.cn-popover-content { animation: ... }` and the
        // other cn overlay/toast keyframes would never fire on nodes
        // that don't also have a matching `#id` rule. Skip nodes that
        // already have an animation (id-based path wins, matching CSS
        // specificity).
        let class_to_nodes = self.element_registry.class_to_nodes_index();
        for (class, node_ids) in &class_to_nodes {
            // Cheap pre-check: the stylesheet has no class rule for this
            // class name — skip the inner loop entirely.
            let has_animation = self
                .stylesheet
                .as_ref()
                .and_then(|s| s.get_class(class))
                .and_then(|st| st.animation.as_ref())
                .is_some();
            if !has_animation {
                continue;
            }
            for &node_id in node_ids {
                let Some(stable_id) = self.stable_id(node_id) else {
                    continue;
                };
                let already_has = self
                    .css_anim_store
                    .lock()
                    .unwrap()
                    .animations
                    .contains_key(&stable_id);
                if already_has {
                    continue;
                }
                let started = self.start_css_animation_for_class(node_id, class);
                if started {
                    tracing::debug!(
                        "CSS animation started for class '.{}' (node={:?})",
                        class,
                        node_id
                    );
                }
            }
        }
    }

    /// Apply all active CSS animation values to their respective render props
    ///
    /// This mutates render props in-place with current animation values (opacity, transform).
    /// The background thread ticks animations; this reads the latest values and applies them.
    pub fn apply_all_css_animation_props(&mut self) {
        // Collect animation data (stable-keyed) under the lock, then
        // release before resolving back to layout ids for the
        // render_nodes write. Filter on `is_playing`: settled
        // animations sit in the store forever (the same-target
        // guard keeps them) but their `current_properties` no longer
        // change frame-to-frame, so re-applying them every frame is
        // wasted work — and the heavy `KeyframeProperties::clone`
        // (dozens of `Option<...>` fields) was running for every
        // ever-played animation on `styling_demo` at idle.
        // `apply_stylesheet_state_styles` resets base props to base
        // before this runs, so skipping settled animations here means
        // the element shows the base style — which matches the CSS
        // default (no `animation-fill-mode: forwards`).
        let anim_data: Vec<(
            crate::tree::StableNodeId,
            blinc_animation::KeyframeProperties,
        )> = {
            let store = self.css_anim_store.lock().unwrap();
            store
                .animations
                .iter()
                .filter(|(_, a)| a.is_playing)
                .map(|(sid, a)| (*sid, a.current_properties.clone()))
                .collect()
        };
        for (stable_id, anim_props) in anim_data {
            let Some(node_id) = self.layout_id(stable_id) else {
                continue;
            };
            // Composite-promotable animations (only opacity / 2D
            // transform) skip the per-frame property apply — the
            // composite-layer path rasterizes the subtree at BASE
            // state into a `LayerTexture` and applies the animated
            // values at composite time via
            // `blit_tight_texture_to_target` (dest_pos / dest_size /
            // opacity). Writing animated values onto
            // `render_node.props` here would bake them into the
            // texture and double-apply at composite time. Mixed
            // animations (any non-promotable property) keep the
            // existing apply because `is_composite_promotable`
            // returns false for them.
            if anim_props.is_composite_promotable() {
                continue;
            }
            if let Some(render_node) = self.render_nodes.get_mut(&node_id) {
                Self::apply_keyframe_props_to_render(&mut render_node.props, &anim_props);
            }
        }
    }

    /// Apply animated layout properties from CSS animations/transitions to taffy styles
    ///
    /// Returns true if any layout properties were modified (requiring layout recomputation).
    pub fn apply_animated_layout_props(&mut self) -> bool {
        use taffy::prelude::*;
        let mut needs_layout = false;

        // Collect all stable ids with ACTIVE animations or transitions
        // that have layout properties. Settled entries stay in the
        // store for the same-target guard but their values don't
        // change frame-to-frame, and `apply_animated_layout_props` is
        // the most expensive of the three per-frame applies — it
        // potentially triggers `compute_layout()` whenever `changed`
        // flips true, which would happen every frame for any settled
        // size / padding / margin / inset value reapplied on top of
        // the base style. Filter on `is_playing` to skip them.
        let anim_nodes: Vec<(
            crate::tree::StableNodeId,
            blinc_animation::KeyframeProperties,
        )> = {
            let store = self.css_anim_store.lock().unwrap();
            store
                .animations
                .iter()
                .filter(|(_, anim)| anim.is_playing)
                .map(|(sid, anim)| (*sid, anim.current_properties.clone()))
                .chain(
                    store
                        .transitions
                        .iter()
                        .filter(|(_, anim)| anim.is_playing)
                        .map(|(sid, anim)| (*sid, anim.current_properties.clone())),
                )
                .collect()
        };

        for (stable_id, anim_props) in anim_nodes {
            let Some(node_id) = self.layout_id(stable_id) else {
                continue;
            };
            let has_layout_props = anim_props.width.is_some()
                || anim_props.height.is_some()
                || anim_props.padding.is_some()
                || anim_props.margin.is_some()
                || anim_props.gap.is_some()
                || anim_props.min_width.is_some()
                || anim_props.max_width.is_some()
                || anim_props.min_height.is_some()
                || anim_props.max_height.is_some()
                || anim_props.flex_grow.is_some()
                || anim_props.flex_shrink.is_some()
                || anim_props.inset_top.is_some()
                || anim_props.inset_right.is_some()
                || anim_props.inset_bottom.is_some()
                || anim_props.inset_left.is_some();

            if !has_layout_props {
                continue;
            }

            if let Some(mut style) = self.layout_tree.get_style(node_id) {
                let mut changed = false;

                if let Some(w) = anim_props.width {
                    style.size.width = Dimension::Length(w);
                    changed = true;
                }
                if let Some(h) = anim_props.height {
                    style.size.height = Dimension::Length(h);
                    changed = true;
                }
                if let Some(v) = anim_props.min_width {
                    style.min_size.width = Dimension::Length(v);
                    changed = true;
                }
                if let Some(v) = anim_props.max_width {
                    style.max_size.width = Dimension::Length(v);
                    changed = true;
                }
                if let Some(v) = anim_props.min_height {
                    style.min_size.height = Dimension::Length(v);
                    changed = true;
                }
                if let Some(v) = anim_props.max_height {
                    style.max_size.height = Dimension::Length(v);
                    changed = true;
                }
                if let Some([top, right, bottom, left]) = anim_props.padding {
                    style.padding = taffy::geometry::Rect {
                        top: LengthPercentage::Length(top),
                        right: LengthPercentage::Length(right),
                        bottom: LengthPercentage::Length(bottom),
                        left: LengthPercentage::Length(left),
                    };
                    changed = true;
                }
                if let Some([top, right, bottom, left]) = anim_props.margin {
                    style.margin = taffy::geometry::Rect {
                        top: LengthPercentageAuto::Length(top),
                        right: LengthPercentageAuto::Length(right),
                        bottom: LengthPercentageAuto::Length(bottom),
                        left: LengthPercentageAuto::Length(left),
                    };
                    changed = true;
                }
                if let Some(g) = anim_props.gap {
                    style.gap = taffy::geometry::Size {
                        width: LengthPercentage::Length(g),
                        height: LengthPercentage::Length(g),
                    };
                    changed = true;
                }
                if let Some(v) = anim_props.flex_grow {
                    style.flex_grow = v;
                    changed = true;
                }
                if let Some(v) = anim_props.flex_shrink {
                    style.flex_shrink = v;
                    changed = true;
                }
                if anim_props.inset_top.is_some()
                    || anim_props.inset_right.is_some()
                    || anim_props.inset_bottom.is_some()
                    || anim_props.inset_left.is_some()
                {
                    if let Some(v) = anim_props.inset_top {
                        style.inset.top = LengthPercentageAuto::Length(v);
                    }
                    if let Some(v) = anim_props.inset_right {
                        style.inset.right = LengthPercentageAuto::Length(v);
                    }
                    if let Some(v) = anim_props.inset_bottom {
                        style.inset.bottom = LengthPercentageAuto::Length(v);
                    }
                    if let Some(v) = anim_props.inset_left {
                        style.inset.left = LengthPercentageAuto::Length(v);
                    }
                    changed = true;
                }

                if changed {
                    self.layout_tree.set_style(node_id, style);
                    needs_layout = true;
                }
            }
        }

        needs_layout
    }

    // =========================================================================
    // CSS Transition Methods
    // =========================================================================

    /// Detect property changes and start transitions where applicable
    ///
    /// Compares before/after keyframe properties, finds changed properties that
    /// have a matching `transition:` spec, and creates a 2-keyframe animation
    /// for each transitioning property group.
    pub(crate) fn detect_and_start_transitions(
        &mut self,
        node_id: LayoutNodeId,
        before: &blinc_animation::KeyframeProperties,
        after: &blinc_animation::KeyframeProperties,
        transition_set: &crate::css_parser::CssTransitionSet,
    ) {
        use blinc_animation::{
            FillMode, KeyframeProperties, MultiKeyframe, MultiKeyframeAnimation,
        };

        // Resolve once at the top so every `store.transitions[&_]`
        // access below uses the stable id (the store migrated to
        // stable keys in Phase 5).
        let stable_id = self.stable_id_or_warn(node_id);

        // Lock the shared store for the duration of this method
        let mut store = self.css_anim_store.lock().unwrap();

        // Build "from" properties: only include fields that differ AND have a transition spec
        let mut from = KeyframeProperties::default();
        let mut to = KeyframeProperties::default();
        let mut has_any = false;
        let mut duration_ms: u32 = 300;
        let mut delay_ms: u32 = 0;
        let mut easing = blinc_animation::Easing::EaseInOut;

        // Helper macro: check if a property changed and is covered by transition
        macro_rules! check_transition {
            ($field:ident, $prop_name:expr) => {
                if before.$field != after.$field {
                    if let Some(t) = transition_set.get($prop_name) {
                        // If a transition is already active, use current interpolated value as "from"
                        let from_val = if let Some(active) = store.transitions.get(&stable_id) {
                            if active.current_properties.$field.is_some() {
                                active.current_properties.$field.clone()
                            } else {
                                before.$field.clone()
                            }
                        } else {
                            before.$field.clone()
                        };
                        from.$field = from_val;
                        to.$field = after.$field.clone();
                        duration_ms = t.duration_ms;
                        delay_ms = t.delay_ms;
                        easing = t.timing.to_easing();
                        has_any = true;
                    }
                }
            };
            // Variant with an identity default — used for filter properties where
            // None means "use default" (e.g. brightness=1.0) rather than "not set"
            ($field:ident, $prop_name:expr, default $def:expr) => {
                if before.$field != after.$field {
                    if let Some(t) = transition_set.get($prop_name) {
                        let from_val = if let Some(active) = store.transitions.get(&stable_id) {
                            if active.current_properties.$field.is_some() {
                                active.current_properties.$field.clone()
                            } else {
                                Some(before.$field.unwrap_or($def))
                            }
                        } else {
                            Some(before.$field.unwrap_or($def))
                        };
                        from.$field = from_val;
                        to.$field = Some(after.$field.unwrap_or($def));
                        duration_ms = t.duration_ms;
                        delay_ms = t.delay_ms;
                        easing = t.timing.to_easing();
                        has_any = true;
                    }
                }
            };
        }

        check_transition!(opacity, "opacity");
        check_transition!(background_color, "background");
        check_transition!(gradient_start_color, "background");
        check_transition!(gradient_end_color, "background");
        check_transition!(gradient_angle, "background");
        check_transition!(text_color, "color");
        check_transition!(text_shadow_params, "text-shadow");
        check_transition!(text_shadow_color, "text-shadow");
        check_transition!(font_size, "font-size");
        check_transition!(border_color, "border-color");
        check_transition!(border_width, "border-width");
        check_transition!(outline_color, "outline-color");
        check_transition!(outline_width, "outline-width");
        check_transition!(outline_offset, "outline-offset");
        check_transition!(corner_radius, "border-radius");
        check_transition!(corner_shape, "corner-shape");
        check_transition!(overflow_fade, "overflow-fade");
        check_transition!(shadow_params, "box-shadow");
        check_transition!(shadow_color, "box-shadow");
        check_transition!(shadow_stack, "box-shadow");
        check_transition!(clip_inset, "clip-path");
        check_transition!(clip_circle_radius, "clip-path");
        check_transition!(clip_ellipse_radii, "clip-path");
        check_transition!(translate_x, "transform", default 0.0);
        check_transition!(translate_y, "transform", default 0.0);
        check_transition!(scale_x, "transform", default 1.0);
        check_transition!(scale_y, "transform", default 1.0);
        check_transition!(rotate, "transform", default 0.0);
        check_transition!(rotate_x, "rotate-x", default 0.0);
        check_transition!(rotate_y, "rotate-y", default 0.0);
        check_transition!(perspective, "perspective");
        check_transition!(depth, "depth");
        check_transition!(translate_z, "translate-z", default 0.0);
        check_transition!(light_intensity, "light-intensity");
        check_transition!(ambient, "ambient");
        check_transition!(specular, "specular");
        check_transition!(light_direction, "light-direction");
        check_transition!(filter_grayscale, "filter", default 0.0);
        check_transition!(filter_invert, "filter", default 0.0);
        check_transition!(filter_sepia, "filter", default 0.0);
        check_transition!(filter_brightness, "filter", default 1.0);
        check_transition!(filter_contrast, "filter", default 1.0);
        check_transition!(filter_saturate, "filter", default 1.0);
        check_transition!(filter_hue_rotate, "filter", default 0.0);
        check_transition!(filter_blur, "filter", default 0.0);

        // Backdrop filter
        check_transition!(backdrop_blur, "backdrop-filter", default 0.0);
        check_transition!(backdrop_saturation, "backdrop-filter", default 1.0);
        check_transition!(backdrop_brightness, "backdrop-filter", default 1.0);

        // Layout properties (require layout recomputation when transitioning)
        check_transition!(width, "width");
        check_transition!(height, "height");
        check_transition!(padding, "padding");
        check_transition!(margin, "margin");
        check_transition!(gap, "gap");
        check_transition!(min_width, "min-width");
        check_transition!(max_width, "max-width");
        check_transition!(min_height, "min-height");
        check_transition!(max_height, "max-height");
        check_transition!(flex_grow, "flex-grow");
        check_transition!(flex_shrink, "flex-shrink");
        check_transition!(inset_top, "top");
        check_transition!(inset_right, "right");
        check_transition!(inset_bottom, "bottom");
        check_transition!(inset_left, "left");
        check_transition!(z_index, "z-index");
        check_transition!(skew_x, "transform", default 0.0);
        check_transition!(skew_y, "transform", default 0.0);
        check_transition!(transform_origin, "transform-origin");

        // Mask gradient
        check_transition!(mask_gradient, "mask-image");

        // SVG properties
        check_transition!(svg_fill, "fill");
        check_transition!(svg_stroke, "stroke");
        check_transition!(svg_stroke_width, "stroke-width");
        check_transition!(svg_stroke_dashoffset, "stroke-dashoffset");

        if has_any && duration_ms > 0 {
            // If a transition already exists heading to the same target, let it continue
            // rather than restarting with a fresh duration each frame
            if let Some(existing) = store.transitions.get(&stable_id) {
                if let Some(last_kf) = existing.animation.last_keyframe() {
                    if last_kf.properties == to {
                        return; // Same target — let existing transition finish
                    }
                }
            }
            let anim = MultiKeyframeAnimation::new(duration_ms)
                .keyframe(0.0, from, easing)
                .keyframe(1.0, to, easing)
                .delay(delay_ms)
                .fill_mode(FillMode::Forwards);
            store.transitions.insert(
                stable_id,
                crate::render_state::ActiveCssAnimation::new(anim),
            );
        }
    }

    /// Apply all active CSS transition values to their respective render props
    ///
    /// The background thread ticks transitions; this reads the latest values and applies them.
    pub fn apply_all_css_transition_props(&mut self) {
        // Collect transition data (stable-keyed) under the lock,
        // then release before resolving back to layout ids. Filter
        // on `is_playing`: settled transitions stay in the store for
        // the same-target restart guard, but their final value also
        // matches the post-transition base/state-style value that
        // `apply_stylesheet_state_styles` already set this frame.
        // Re-applying them every frame is wasted work + a per-entry
        // `KeyframeProperties::clone` (large struct of Options) for
        // each ever-hovered widget on cn_demo / styling_demo, which
        // accumulated linearly with interaction history.
        let trans_data: Vec<(
            crate::tree::StableNodeId,
            blinc_animation::KeyframeProperties,
        )> = {
            let store = self.css_anim_store.lock().unwrap();
            store
                .transitions
                .iter()
                .filter(|(_, a)| a.is_playing)
                .map(|(sid, a)| (*sid, a.current_properties.clone()))
                .collect()
        };
        for (stable_id, anim_props) in trans_data {
            let Some(node_id) = self.layout_id(stable_id) else {
                continue;
            };
            // See `apply_all_css_animation_props` above — composite-
            // promotable transitions skip the apply for the same
            // reason (texture rasterizes at base, composite applies
            // animated values).
            if anim_props.is_composite_promotable() {
                continue;
            }
            if let Some(render_node) = self.render_nodes.get_mut(&node_id) {
                Self::apply_keyframe_props_to_render(&mut render_node.props, &anim_props);
            }
        }
    }

    /// Check if there are no active CSS transitions
    pub fn css_transitions_empty(&self) -> bool {
        self.css_anim_store.lock().unwrap().transitions.is_empty()
    }

    /// Remove completed transitions from the store.
    /// Must be called AFTER `apply_all_css_transition_props()`.
    pub fn remove_completed_transitions(&mut self) {
        self.css_anim_store
            .lock()
            .unwrap()
            .remove_completed_transitions();
    }

    /// Rebuild gradient stops, interpolating the first and last stop colors
    /// while preserving the positions of any intermediate stops.
    fn rebuild_two_stop_gradient(
        existing_stops: &[blinc_core::GradientStop],
        start_color: blinc_core::Color,
        end_color: blinc_core::Color,
    ) -> Vec<blinc_core::GradientStop> {
        if existing_stops.len() <= 2 {
            vec![
                blinc_core::GradientStop::new(0.0, start_color),
                blinc_core::GradientStop::new(1.0, end_color),
            ]
        } else {
            // Keep intermediate stops but lerp their colors proportionally
            let mut new_stops = Vec::with_capacity(existing_stops.len());
            for stop in existing_stops {
                let t = stop.offset;
                let c = blinc_core::Color::rgba(
                    start_color.r + (end_color.r - start_color.r) * t,
                    start_color.g + (end_color.g - start_color.g) * t,
                    start_color.b + (end_color.b - start_color.b) * t,
                    start_color.a + (end_color.a - start_color.a) * t,
                );
                new_stops.push(blinc_core::GradientStop::new(stop.offset, c));
            }
            new_stops
        }
    }

    /// Apply keyframe animation properties to render props
    pub(crate) fn apply_keyframe_props_to_render(
        props: &mut RenderProps,
        anim_props: &blinc_animation::KeyframeProperties,
    ) {
        if let Some(opacity) = anim_props.opacity {
            props.opacity = opacity;
        }

        let has_transform = anim_props.translate_x.is_some()
            || anim_props.translate_y.is_some()
            || anim_props.scale_x.is_some()
            || anim_props.scale_y.is_some()
            || anim_props.rotate.is_some()
            || anim_props.skew_x.is_some()
            || anim_props.skew_y.is_some();

        if has_transform {
            use blinc_core::{Affine2D, Transform};

            let mut affine = Affine2D::IDENTITY;

            if let (Some(sx), Some(sy)) = (anim_props.scale_x, anim_props.scale_y) {
                affine = affine.then(&Affine2D::scale(sx, sy));
            } else if let Some(sx) = anim_props.scale_x {
                affine = affine.then(&Affine2D::scale(sx, 1.0));
            } else if let Some(sy) = anim_props.scale_y {
                affine = affine.then(&Affine2D::scale(1.0, sy));
            }

            if let Some(rotate) = anim_props.rotate {
                affine = affine.then(&Affine2D::rotation(rotate.to_radians()));
            }

            // Skew
            if let Some(skx) = anim_props.skew_x {
                affine = affine.then(&Affine2D::skew_x(skx.to_radians()));
            }
            if let Some(sky) = anim_props.skew_y {
                affine = affine.then(&Affine2D::skew_y(sky.to_radians()));
            }

            if let (Some(tx), Some(ty)) = (anim_props.translate_x, anim_props.translate_y) {
                affine = affine.then(&Affine2D::translation(tx, ty));
            } else if let Some(tx) = anim_props.translate_x {
                affine = affine.then(&Affine2D::translation(tx, 0.0));
            } else if let Some(ty) = anim_props.translate_y {
                affine = affine.then(&Affine2D::translation(0.0, ty));
            }

            props.transform = Some(Transform::Affine2D(affine));
        }

        // 3D animation properties
        if let Some(rx) = anim_props.rotate_x {
            props.rotate_x = Some(rx);
        }
        if let Some(ry) = anim_props.rotate_y {
            props.rotate_y = Some(ry);
        }
        if let Some(p) = anim_props.perspective {
            props.perspective = Some(p);
        }
        if let Some(d) = anim_props.depth {
            props.depth = Some(d);
        }
        if let Some(tz) = anim_props.translate_z {
            props.translate_z = Some(tz);
        }
        if let Some(b) = anim_props.blend_3d {
            props.blend_3d = Some(b);
        }

        // Clip-path animation
        if let Some(inset) = &anim_props.clip_inset {
            use blinc_core::{ClipLength, ClipPath};
            props.clip_path = Some(ClipPath::Inset {
                top: ClipLength::Percent(inset[0]),
                right: ClipLength::Percent(inset[1]),
                bottom: ClipLength::Percent(inset[2]),
                left: ClipLength::Percent(inset[3]),
                round: None,
            });
        }
        if let Some(r) = anim_props.clip_circle_radius {
            use blinc_core::{ClipLength, ClipPath};
            props.clip_path = Some(ClipPath::Circle {
                radius: Some(ClipLength::Percent(r)),
                center: (ClipLength::Percent(50.0), ClipLength::Percent(50.0)),
            });
        }
        if let Some(radii) = &anim_props.clip_ellipse_radii {
            use blinc_core::{ClipLength, ClipPath};
            props.clip_path = Some(ClipPath::Ellipse {
                rx: Some(ClipLength::Percent(radii[0])),
                ry: Some(ClipLength::Percent(radii[1])),
                center: (ClipLength::Percent(50.0), ClipLength::Percent(50.0)),
            });
        }

        // Background color (solid)
        if let Some([r, g, b, a]) = anim_props.background_color {
            props.background = Some(blinc_core::Brush::Solid(blinc_core::Color::rgba(
                r, g, b, a,
            )));
        }

        // Gradient color stops animation
        if anim_props.gradient_start_color.is_some() || anim_props.gradient_end_color.is_some() {
            // Derive fallback colors from existing gradient stops to avoid
            // flashing to black/white when only one color is in the transition
            let (existing_start, existing_end) = match &props.background {
                Some(blinc_core::Brush::Gradient(g)) => {
                    let stops = g.stops();
                    let s = stops
                        .first()
                        .map(|s| [s.color.r, s.color.g, s.color.b, s.color.a]);
                    let e = stops
                        .last()
                        .map(|s| [s.color.r, s.color.g, s.color.b, s.color.a]);
                    (
                        s.unwrap_or([0.0, 0.0, 0.0, 1.0]),
                        e.unwrap_or([0.0, 0.0, 0.0, 1.0]),
                    )
                }
                _ => ([0.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.0, 1.0]),
            };
            let start_color = anim_props.gradient_start_color.unwrap_or(existing_start);
            let end_color = anim_props.gradient_end_color.unwrap_or(existing_end);
            let sc = blinc_core::Color::rgba(
                start_color[0],
                start_color[1],
                start_color[2],
                start_color[3],
            );
            let ec =
                blinc_core::Color::rgba(end_color[0], end_color[1], end_color[2], end_color[3]);

            // Reconstruct gradient preserving the existing type if possible
            match &props.background {
                Some(blinc_core::Brush::Gradient(existing)) => {
                    let new_gradient = match existing {
                        blinc_core::Gradient::Linear {
                            start,
                            end,
                            stops,
                            space,
                            spread,
                        } => {
                            let (start_pt, end_pt) = if let Some(angle) = anim_props.gradient_angle
                            {
                                crate::css_parser::angle_to_gradient_points(angle)
                            } else {
                                (*start, *end)
                            };
                            let new_stops = Self::rebuild_two_stop_gradient(stops, sc, ec);
                            blinc_core::Gradient::Linear {
                                start: start_pt,
                                end: end_pt,
                                stops: new_stops,
                                space: *space,
                                spread: *spread,
                            }
                        }
                        blinc_core::Gradient::Radial {
                            center,
                            radius,
                            focal,
                            stops,
                            space,
                            spread,
                        } => {
                            let new_stops = Self::rebuild_two_stop_gradient(stops, sc, ec);
                            blinc_core::Gradient::Radial {
                                center: *center,
                                radius: *radius,
                                focal: *focal,
                                stops: new_stops,
                                space: *space,
                                spread: *spread,
                            }
                        }
                        blinc_core::Gradient::Conic {
                            center,
                            start_angle,
                            stops,
                            space,
                        } => {
                            let new_stops = Self::rebuild_two_stop_gradient(stops, sc, ec);
                            blinc_core::Gradient::Conic {
                                center: *center,
                                start_angle: *start_angle,
                                stops: new_stops,
                                space: *space,
                            }
                        }
                    };
                    props.background = Some(blinc_core::Brush::Gradient(new_gradient));
                }
                _ => {
                    // No existing gradient — create a linear from the animated angle
                    let angle = anim_props.gradient_angle.unwrap_or(180.0);
                    let (start_pt, end_pt) = crate::css_parser::angle_to_gradient_points(angle);
                    props.background =
                        Some(blinc_core::Brush::Gradient(blinc_core::Gradient::Linear {
                            start: start_pt,
                            end: end_pt,
                            stops: vec![
                                blinc_core::GradientStop::new(0.0, sc),
                                blinc_core::GradientStop::new(1.0, ec),
                            ],
                            space: blinc_core::GradientSpace::ObjectBoundingBox,
                            spread: blinc_core::GradientSpread::Pad,
                        }));
                }
            }
        }

        // Text color
        if let Some(tc) = anim_props.text_color {
            props.text_color = Some(tc);
        }

        // Text shadow
        if let Some([ox, oy, blur, spread]) = anim_props.text_shadow_params {
            let color = anim_props
                .text_shadow_color
                .map(|[r, g, b, a]| blinc_core::Color::rgba(r, g, b, a))
                .or_else(|| props.text_shadow.as_ref().map(|s| s.color))
                .unwrap_or(blinc_core::Color::rgba(0.0, 0.0, 0.0, 0.5));
            props.text_shadow = Some(Shadow {
                offset_x: ox,
                offset_y: oy,
                blur,
                spread,
                color,
            });
        } else if let Some([r, g, b, a]) = anim_props.text_shadow_color {
            if let Some(ts) = &mut props.text_shadow {
                ts.color = blinc_core::Color::rgba(r, g, b, a);
            }
        }

        // Font size
        if let Some(fs) = anim_props.font_size {
            props.font_size = Some(fs);
        }

        // Corner radius
        if let Some([tl, tr, br, bl]) = anim_props.corner_radius {
            props.border_radius = blinc_core::CornerRadius {
                top_left: tl,
                top_right: tr,
                bottom_right: br,
                bottom_left: bl,
            };
        }

        // Corner shape (superellipse)
        if let Some([tl, tr, br, bl]) = anim_props.corner_shape {
            props.corner_shape = blinc_core::CornerShape::new(tl, tr, br, bl);
        }

        // Overflow fade
        if let Some([t, r, b, l]) = anim_props.overflow_fade {
            props.overflow_fade = blinc_core::OverflowFade::new(t, r, b, l);
        }

        // Border
        if let Some(bw) = anim_props.border_width {
            props.border_width = bw;
        }
        if let Some([r, g, b, a]) = anim_props.border_color {
            props.border_color = Some(blinc_core::Color::rgba(r, g, b, a));
        }

        // Outline
        if let Some(ow) = anim_props.outline_width {
            props.outline_width = ow;
        }
        if let Some([r, g, b, a]) = anim_props.outline_color {
            props.outline_color = Some(blinc_core::Color::rgba(r, g, b, a));
        }
        if let Some(offset) = anim_props.outline_offset {
            props.outline_offset = offset;
        }

        // Shadow — prefer the full compound stack when the snapshot
        // carried one; fall back to the single-layer fields for
        // animations that originated from `shadow_params/shadow_color`
        // alone (older snapshots, programmatic Spring/Tween drivers
        // that don't populate the stack).
        if let Some(ref stack) = anim_props.shadow_stack {
            props.shadow = stack
                .iter()
                .map(|l| blinc_core::Shadow {
                    offset_x: l[0],
                    offset_y: l[1],
                    blur: l[2],
                    spread: l[3],
                    color: blinc_core::Color::rgba(l[4], l[5], l[6], l[7]),
                })
                .collect();
        } else if let Some([ox, oy, blur, spread]) = anim_props.shadow_params {
            let color = if let Some([r, g, b, a]) = anim_props.shadow_color {
                blinc_core::Color::rgba(r, g, b, a)
            } else if let Some(existing) = props.shadow.first() {
                existing.color
            } else {
                blinc_core::Color::rgba(0.0, 0.0, 0.0, 0.5)
            };
            let layer = blinc_core::Shadow {
                offset_x: ox,
                offset_y: oy,
                blur,
                spread,
                color,
            };
            if let Some(first) = props.shadow.first_mut() {
                *first = layer;
            } else {
                props.shadow = vec![layer];
            }
        } else if let Some([r, g, b, a]) = anim_props.shadow_color {
            if let Some(first) = props.shadow.first_mut() {
                first.color = blinc_core::Color::rgba(r, g, b, a);
            }
        }

        // 3D lighting
        if let Some(li) = anim_props.light_intensity {
            props.light_intensity = Some(li);
        }
        if let Some(a) = anim_props.ambient {
            props.ambient = Some(a);
        }
        if let Some(s) = anim_props.specular {
            props.specular = Some(s);
        }
        if let Some(ld) = anim_props.light_direction {
            props.light_direction = Some(ld);
        }

        // CSS filter properties
        let has_filter = anim_props.filter_grayscale.is_some()
            || anim_props.filter_invert.is_some()
            || anim_props.filter_sepia.is_some()
            || anim_props.filter_brightness.is_some()
            || anim_props.filter_contrast.is_some()
            || anim_props.filter_saturate.is_some()
            || anim_props.filter_hue_rotate.is_some()
            || anim_props.filter_blur.is_some();
        if has_filter {
            let existing = props.filter.unwrap_or_default();
            let blur = anim_props.filter_blur.unwrap_or(existing.blur);
            props.filter = Some(crate::element_style::CssFilter {
                grayscale: anim_props.filter_grayscale.unwrap_or(existing.grayscale),
                invert: anim_props.filter_invert.unwrap_or(existing.invert),
                sepia: anim_props.filter_sepia.unwrap_or(existing.sepia),
                hue_rotate: anim_props.filter_hue_rotate.unwrap_or(existing.hue_rotate),
                brightness: anim_props.filter_brightness.unwrap_or(existing.brightness),
                contrast: anim_props.filter_contrast.unwrap_or(existing.contrast),
                saturate: anim_props.filter_saturate.unwrap_or(existing.saturate),
                blur,
                drop_shadow: existing.drop_shadow,
            });
            // Update LayerEffect for blur
            if blur > 0.0 {
                props
                    .layer_effects
                    .retain(|e| !matches!(e, LayerEffect::Blur { .. }));
                props.layer_effects.push(LayerEffect::Blur {
                    radius: blur,
                    quality: BlurQuality::Medium,
                });
            } else {
                props
                    .layer_effects
                    .retain(|e| !matches!(e, LayerEffect::Blur { .. }));
            }
        }

        // Backdrop filter (glass material)
        let has_backdrop = anim_props.backdrop_blur.is_some()
            || anim_props.backdrop_saturation.is_some()
            || anim_props.backdrop_brightness.is_some();
        if has_backdrop {
            let existing = match &props.material {
                Some(Material::Glass(g)) => g.clone(),
                _ => GlassMaterial {
                    blur: 0.0,
                    tint: blinc_core::Color::rgba(1.0, 1.0, 1.0, 0.1),
                    saturation: 1.0,
                    brightness: 1.0,
                    noise: 0.0,
                    border_thickness: 0.0,
                    shadow: None,
                    simple: true,
                },
            };
            let mut glass = existing;
            if let Some(b) = anim_props.backdrop_blur {
                glass.blur = b;
            }
            if let Some(s) = anim_props.backdrop_saturation {
                glass.saturation = s;
            }
            if let Some(br) = anim_props.backdrop_brightness {
                glass.brightness = br;
            }
            props.material = Some(Material::Glass(glass));
            props.layer = RenderLayer::Glass;
        }

        // z-index (round from f32 to i32)
        if let Some(z) = anim_props.z_index {
            props.z_index = z.round() as i32;
        }

        // Transform origin
        if let Some(to) = anim_props.transform_origin {
            props.transform_origin = Some(to);
        }

        // Mask gradient: reconstruct MaskImage::Gradient from combined [f32; 8]
        if let Some(mg) = anim_props.mask_gradient {
            let mask_type = mg[0];
            let start_alpha = mg[1];
            let end_alpha = mg[2];
            let gradient = if mask_type < 1.5 {
                blinc_core::Gradient::Linear {
                    start: blinc_core::Point::new(mg[4], mg[5]),
                    end: blinc_core::Point::new(mg[6], mg[7]),
                    stops: vec![
                        blinc_core::GradientStop::new(
                            0.0,
                            blinc_core::Color::rgba(0.0, 0.0, 0.0, start_alpha),
                        ),
                        blinc_core::GradientStop::new(
                            1.0,
                            blinc_core::Color::rgba(0.0, 0.0, 0.0, end_alpha),
                        ),
                    ],
                    space: blinc_core::GradientSpace::ObjectBoundingBox,
                    spread: blinc_core::GradientSpread::Pad,
                }
            } else {
                blinc_core::Gradient::Radial {
                    center: blinc_core::Point::new(mg[4], mg[5]),
                    radius: mg[6],
                    focal: None,
                    stops: vec![
                        blinc_core::GradientStop::new(
                            0.0,
                            blinc_core::Color::rgba(0.0, 0.0, 0.0, start_alpha),
                        ),
                        blinc_core::GradientStop::new(
                            1.0,
                            blinc_core::Color::rgba(0.0, 0.0, 0.0, end_alpha),
                        ),
                    ],
                    space: blinc_core::GradientSpace::ObjectBoundingBox,
                    spread: blinc_core::GradientSpread::Pad,
                }
            };
            props.mask_image = Some(blinc_core::MaskImage::Gradient(gradient));
        }

        // SVG properties
        if let Some([r, g, b, a]) = anim_props.svg_fill {
            props.fill = Some([r, g, b, a]);
        }
        if let Some([r, g, b, a]) = anim_props.svg_stroke {
            props.stroke = Some([r, g, b, a]);
        }
        if let Some(sw) = anim_props.svg_stroke_width {
            props.stroke_width = Some(sw);
        }
        if let Some(offset) = anim_props.svg_stroke_dashoffset {
            props.stroke_dashoffset = Some(offset);
        }
        if let Some(ref path_data) = anim_props.svg_path_data {
            props.svg_path_data = Some(path_data.clone());
        }
    }

    /// Extract animatable properties from RenderProps into KeyframeProperties
    ///
    /// This is the reverse of `apply_keyframe_props_to_render()` — used to snapshot
    /// the current visual state before/after a state change for transition detection.
    pub(crate) fn render_props_to_keyframe_properties(
        props: &RenderProps,
    ) -> blinc_animation::KeyframeProperties {
        use blinc_animation::KeyframeProperties;

        let mut kp = KeyframeProperties {
            opacity: Some(props.opacity),
            ..Default::default()
        };

        // Extract transform components via QR decomposition
        // Decomposes the 2x2 portion [a,c; b,d] into scale, rotation, and skew.
        if let Some(blinc_core::Transform::Affine2D(affine)) = &props.transform {
            let [a, b, c, d, tx, ty] = affine.elements;
            kp.translate_x = Some(tx);
            kp.translate_y = Some(ty);

            // QR decomposition of the 2x2 matrix:
            // Step 1: scale_x = length of first column
            let scale_x = (a * a + b * b).sqrt().max(1e-6);
            // Step 2: rotation from normalized first column
            let rotation = b.atan2(a);
            kp.rotate = Some(rotation.to_degrees());
            // Step 3: XY shear = dot(normalized_col0, col1)
            let skew_xy = (a * c + b * d) / (scale_x * scale_x);
            // Step 4: scale_y = det / scale_x (preserves sign)
            let det = a * d - b * c;
            let scale_y = det / scale_x;

            kp.scale_x = Some(scale_x);
            kp.scale_y = Some(scale_y);

            // Only set skew if non-trivial
            if skew_xy.abs() > 0.0001 {
                kp.skew_x = Some(skew_xy.atan().to_degrees());
            }
        } else {
            kp.translate_x = Some(0.0);
            kp.translate_y = Some(0.0);
            kp.scale_x = Some(1.0);
            kp.scale_y = Some(1.0);
            kp.rotate = Some(0.0);
        }

        // 3D
        kp.rotate_x = props.rotate_x;
        kp.rotate_y = props.rotate_y;
        kp.perspective = props.perspective;
        kp.depth = props.depth;
        kp.translate_z = props.translate_z;
        kp.blend_3d = props.blend_3d;

        // Clip-path
        match &props.clip_path {
            Some(blinc_core::ClipPath::Inset {
                top,
                right,
                bottom,
                left,
                ..
            }) => {
                kp.clip_inset = Some([
                    Self::clip_length_to_percent(top),
                    Self::clip_length_to_percent(right),
                    Self::clip_length_to_percent(bottom),
                    Self::clip_length_to_percent(left),
                ]);
            }
            Some(blinc_core::ClipPath::Circle {
                radius: Some(r), ..
            }) => {
                kp.clip_circle_radius = Some(Self::clip_length_to_percent(r));
            }
            Some(blinc_core::ClipPath::Ellipse {
                rx: Some(rx),
                ry: Some(ry),
                ..
            }) => {
                kp.clip_ellipse_radii = Some([
                    Self::clip_length_to_percent(rx),
                    Self::clip_length_to_percent(ry),
                ]);
            }
            _ => {}
        }

        // Background color (solid or gradient)
        match &props.background {
            Some(blinc_core::Brush::Solid(c)) => {
                kp.background_color = Some([c.r, c.g, c.b, c.a]);
            }
            Some(blinc_core::Brush::Gradient(gradient)) => {
                let stops = gradient.stops();
                if let Some(first) = stops.first() {
                    kp.gradient_start_color =
                        Some([first.color.r, first.color.g, first.color.b, first.color.a]);
                }
                if let Some(last) = stops.last() {
                    kp.gradient_end_color =
                        Some([last.color.r, last.color.g, last.color.b, last.color.a]);
                }
                if let blinc_core::Gradient::Linear { start, end, .. } = gradient {
                    kp.gradient_angle =
                        Some(crate::css_parser::gradient_points_to_angle(*start, *end));
                }
            }
            _ => {}
        }

        // Text color
        if let Some(tc) = &props.text_color {
            kp.text_color = Some(*tc);
        }

        // Text shadow
        if let Some(ts) = &props.text_shadow {
            kp.text_shadow_params = Some([ts.offset_x, ts.offset_y, ts.blur, ts.spread]);
            kp.text_shadow_color = Some([ts.color.r, ts.color.g, ts.color.b, ts.color.a]);
        }

        // Font size
        kp.font_size = props.font_size;

        // Corner radius
        let cr = &props.border_radius;
        kp.corner_radius = Some([cr.top_left, cr.top_right, cr.bottom_right, cr.bottom_left]);

        // Corner shape (superellipse) — always snapshot for transitions
        kp.corner_shape = Some(props.corner_shape.to_array());

        // Overflow fade — always snapshot for transitions
        kp.overflow_fade = Some(props.overflow_fade.to_array());

        // Border
        kp.border_width = Some(props.border_width);
        if let Some(bc) = &props.border_color {
            kp.border_color = Some([bc.r, bc.g, bc.b, bc.a]);
        }

        // Outline
        kp.outline_width = Some(props.outline_width);
        if let Some(oc) = &props.outline_color {
            kp.outline_color = Some([oc.r, oc.g, oc.b, oc.a]);
        }
        kp.outline_offset = Some(props.outline_offset);

        // Shadow — snapshot the whole compound stack into
        // `shadow_stack` and mirror the first layer into the
        // single-layer fast-path fields (the composite-promoted
        // CssAnimPaintMeta only samples layer 0). Empty Vec is the
        // canonical "no shadow" marker, distinct from `None` (which
        // means "don't animate this property").
        if let Some(first) = props.shadow.first() {
            kp.shadow_params = Some([first.offset_x, first.offset_y, first.blur, first.spread]);
            kp.shadow_color = Some([first.color.r, first.color.g, first.color.b, first.color.a]);
        }
        kp.shadow_stack = Some(
            props
                .shadow
                .iter()
                .map(|s| {
                    [
                        s.offset_x, s.offset_y, s.blur, s.spread, s.color.r, s.color.g, s.color.b,
                        s.color.a,
                    ]
                })
                .collect(),
        );

        // 3D lighting
        kp.light_intensity = props.light_intensity;
        kp.ambient = props.ambient;
        kp.specular = props.specular;
        kp.light_direction = props.light_direction;

        // CSS filter properties
        if let Some(f) = &props.filter {
            kp.filter_grayscale = Some(f.grayscale);
            kp.filter_invert = Some(f.invert);
            kp.filter_sepia = Some(f.sepia);
            kp.filter_brightness = Some(f.brightness);
            kp.filter_contrast = Some(f.contrast);
            kp.filter_saturate = Some(f.saturate);
            kp.filter_hue_rotate = Some(f.hue_rotate);
            kp.filter_blur = Some(f.blur);
        }

        // Backdrop filter (glass material)
        if let Some(Material::Glass(glass)) = &props.material {
            kp.backdrop_blur = Some(glass.blur);
            kp.backdrop_saturation = Some(glass.saturation);
            kp.backdrop_brightness = Some(glass.brightness);
        }

        // z-index (as f32 for smooth interpolation)
        if props.z_index != 0 {
            kp.z_index = Some(props.z_index as f32);
        }

        // Transform origin
        kp.transform_origin = props.transform_origin;

        // Mask gradient (combined [mask_type, start_alpha, end_alpha, 0, p0, p1, p2, p3])
        if let Some(blinc_core::MaskImage::Gradient(ref gradient)) = props.mask_image {
            let lum = matches!(props.mask_mode, Some(blinc_core::MaskMode::Luminance));
            kp.mask_gradient = Some(match gradient {
                blinc_core::Gradient::Linear {
                    start, end, stops, ..
                } => {
                    let (sa, ea) = Self::extract_mask_alphas(stops, lum);
                    [1.0, sa, ea, 0.0, start.x, start.y, end.x, end.y]
                }
                blinc_core::Gradient::Radial {
                    center,
                    radius,
                    stops,
                    ..
                } => {
                    let (sa, ea) = Self::extract_mask_alphas(stops, lum);
                    [2.0, sa, ea, 0.0, center.x, center.y, *radius, 0.0]
                }
                blinc_core::Gradient::Conic { center, stops, .. } => {
                    let (sa, ea) = Self::extract_mask_alphas(stops, lum);
                    [2.0, sa, ea, 0.0, center.x, center.y, 0.5, 0.0]
                }
            });
        }

        // SVG properties
        if let Some(fill) = &props.fill {
            kp.svg_fill = Some(*fill);
        }
        if let Some(stroke) = &props.stroke {
            kp.svg_stroke = Some(*stroke);
        }
        if let Some(sw) = props.stroke_width {
            kp.svg_stroke_width = Some(sw);
        }
        if let Some(offset) = props.stroke_dashoffset {
            kp.svg_stroke_dashoffset = Some(offset);
        }
        if let Some(ref path_data) = props.svg_path_data {
            kp.svg_path_data = Some(path_data.clone());
        }

        kp
    }
}
