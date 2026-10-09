//! CSS selector matching engine on `RenderTree`.
//!
//! Three concerns live here:
//!
//! - **Matching**: `compound_matches` evaluates a single
//!   `CompoundSelector` against one node (type / id / class / state /
//!   `:not(...)` / `:is(...)` / structural pseudo-classes).
//!   `complex_selector_matches` walks a chain of compounds joined by
//!   combinators (`>`, descendant, `+`, `~`) right-to-left through the
//!   ancestor / sibling tree.
//! - **Specificity**: `selector_specificity` returns the standard
//!   `(ids, classes, types)` tuple used to break ties when several
//!   rules match the same node.
//! - **Per-frame application**: `apply_complex_selector_styles` resets
//!   nodes that left their state since last frame, re-applies the
//!   matching state rules, and starts CSS transitions when the
//!   `before → after` snapshot diff intersects a `transition:` spec.
//!   `apply_svg_tag_styles` is the SVG-tag-rule sibling pass
//!   (`#chart circle { ... }`).
//!
//! All methods are visible only inside the renderer crate via
//! `pub(crate)` because the stylesheet flow in `mod.rs` (and the FLIP
//! reflow path in `animation/flip.rs`) drives them — they are not part
//! of the public `RenderTree` surface.

use std::collections::{HashMap, HashSet};

use crate::css_parser::{
    Combinator, ComplexSelector, CompoundSelector, ElementState, SelectorPart, StructuralPseudo,
};
use crate::tree::LayoutNodeId;

use super::super::RenderTree;

impl RenderTree {
    pub fn has_state_styles(&self, node_id: LayoutNodeId) -> bool {
        let stylesheet = match &self.stylesheet {
            Some(s) => s,
            None => return false,
        };

        let element_id = match self.element_registry.get_id(node_id) {
            Some(id) => id,
            None => return false,
        };

        // Check if any simple state styles exist
        if stylesheet.contains_with_state(&element_id, ElementState::Hover)
            || stylesheet.contains_with_state(&element_id, ElementState::Active)
            || stylesheet.contains_with_state(&element_id, ElementState::Focus)
            || stylesheet.contains_with_state(&element_id, ElementState::Disabled)
        {
            return true;
        }

        // Check if any complex rules with state exist that could affect this node
        stylesheet.has_complex_state_rules()
    }

    /// Check if a compound selector matches a given node
    pub(crate) fn compound_matches(
        &self,
        compound: &CompoundSelector,
        node_id: LayoutNodeId,
        hovered_nodes: &HashSet<LayoutNodeId>,
        pressed_nodes: &HashSet<LayoutNodeId>,
        focused_node: Option<LayoutNodeId>,
    ) -> bool {
        let hovered = hovered_nodes.contains(&node_id);
        let pressed = pressed_nodes.contains(&node_id);
        let focused = focused_node == Some(node_id);
        for part in &compound.parts {
            match part {
                SelectorPart::Type(type_name) => {
                    let node_type = self.element_registry.get_element_type(node_id);
                    if node_type != Some(type_name.as_str()) {
                        return false;
                    }
                }
                SelectorPart::Id(id) => {
                    let node_id_str = self.element_registry.get_id(node_id);
                    if node_id_str.as_deref() != Some(id.as_str()) {
                        return false;
                    }
                }
                SelectorPart::Class(class) => {
                    if !self.element_registry.has_class(node_id, class) {
                        return false;
                    }
                }
                SelectorPart::State(state) => {
                    let matches = match state {
                        ElementState::Hover => hovered,
                        ElementState::Active => pressed,
                        ElementState::Focus => focused,
                        ElementState::Disabled => false, // TODO: track disabled state
                        ElementState::Checked => false, // checked state managed by widget callbacks
                    };
                    if !matches {
                        return false;
                    }
                }
                SelectorPart::Universal => {
                    // Universal selector matches everything — continue
                }
                SelectorPart::Not(inner_compound) => {
                    // :not(selector) — matches if the inner compound does NOT match
                    if self.compound_matches(
                        inner_compound,
                        node_id,
                        hovered_nodes,
                        pressed_nodes,
                        focused_node,
                    ) {
                        return false;
                    }
                }
                SelectorPart::Is(selectors) => {
                    // :is(sel1, sel2, ...) — matches if ANY inner selector matches
                    let any_match = selectors.iter().any(|s| {
                        self.compound_matches(
                            s,
                            node_id,
                            hovered_nodes,
                            pressed_nodes,
                            focused_node,
                        )
                    });
                    if !any_match {
                        return false;
                    }
                }
                SelectorPart::Has(inner_selectors) => {
                    // :has(complex, ...) — relational pseudo. Subject
                    // matches if at least one inner relative selector
                    // matches against the subject's descendants.
                    //
                    // For each inner ComplexSelector, walk the subject's
                    // descendants and call complex_selector_matches with
                    // each descendant as the target. Per CSS spec the
                    // subject itself is NOT in the candidate set.
                    //
                    // Single-compound inners (`:has(.x:hover)`) hit the
                    // common case the menu flicker fix relies on; the
                    // descendant walk is `get_descendants` (DFS pre-
                    // order) on the registry.
                    let descendants = self.element_registry.get_descendants(node_id);
                    let any_inner_matches = inner_selectors.iter().any(|inner| {
                        descendants.iter().any(|&d| {
                            self.complex_selector_matches(
                                inner,
                                d,
                                hovered_nodes,
                                pressed_nodes,
                                focused_node,
                            )
                        })
                    });
                    if !any_inner_matches {
                        return false;
                    }
                }
                SelectorPart::PseudoClass(pseudo) => {
                    let matches = match pseudo {
                        StructuralPseudo::FirstChild => {
                            self.element_registry.get_child_index(node_id) == Some(0)
                        }
                        StructuralPseudo::LastChild => {
                            let index = self.element_registry.get_child_index(node_id);
                            let count = self.element_registry.get_sibling_count(node_id);
                            match (index, count) {
                                (Some(i), Some(c)) if c > 0 => i == c - 1,
                                _ => false,
                            }
                        }
                        StructuralPseudo::NthChild(n) => {
                            // nth-child is 1-based in CSS
                            self.element_registry.get_child_index(node_id)
                                == Some(n.saturating_sub(1))
                        }
                        StructuralPseudo::NthLastChild(n) => {
                            // nth-last-child is 1-based from the end
                            let index = self.element_registry.get_child_index(node_id);
                            let count = self.element_registry.get_sibling_count(node_id);
                            match (index, count) {
                                (Some(i), Some(c)) if c > 0 => i == c - n,
                                _ => false,
                            }
                        }
                        StructuralPseudo::OnlyChild => {
                            self.element_registry.get_sibling_count(node_id) == Some(1)
                        }
                        StructuralPseudo::Empty => !self.element_registry.has_children(node_id),
                        StructuralPseudo::Root => self.element_registry.is_root(node_id),
                        // *-of-type: In Blinc all elements are Div, so equivalent to *-child
                        StructuralPseudo::FirstOfType => {
                            self.element_registry.get_child_index(node_id) == Some(0)
                        }
                        StructuralPseudo::LastOfType => {
                            let index = self.element_registry.get_child_index(node_id);
                            let count = self.element_registry.get_sibling_count(node_id);
                            match (index, count) {
                                (Some(i), Some(c)) if c > 0 => i == c - 1,
                                _ => false,
                            }
                        }
                        StructuralPseudo::NthOfType(n) => {
                            self.element_registry.get_child_index(node_id)
                                == Some(n.saturating_sub(1))
                        }
                        StructuralPseudo::NthLastOfType(n) => {
                            let index = self.element_registry.get_child_index(node_id);
                            let count = self.element_registry.get_sibling_count(node_id);
                            match (index, count) {
                                (Some(i), Some(c)) if c > 0 => i == c - n,
                                _ => false,
                            }
                        }
                        StructuralPseudo::OnlyOfType => {
                            self.element_registry.get_sibling_count(node_id) == Some(1)
                        }
                    };
                    if !matches {
                        return false;
                    }
                }
                SelectorPart::PseudoElement(_) => {
                    // Pseudo-elements like ::placeholder are handled at the rule storage level
                    // (stored with key "id::placeholder"), not during runtime matching.
                    // If we encounter one here, it doesn't match element nodes.
                    return false;
                }
            }
        }
        true
    }

    /// Calculate CSS specificity for a complex selector as (ids, classes, types).
    /// Used for sorting: lower specificity rules apply first, higher ones override.
    pub(crate) fn selector_specificity(selector: &ComplexSelector) -> (u32, u32, u32) {
        let (mut ids, mut classes, mut types) = (0u32, 0u32, 0u32);
        for (compound, _) in &selector.segments {
            for part in &compound.parts {
                match part {
                    SelectorPart::Id(_) => ids += 1,
                    SelectorPart::Class(_)
                    | SelectorPart::State(_)
                    | SelectorPart::PseudoClass(_)
                    | SelectorPart::Not(_)
                    | SelectorPart::Is(_)
                    | SelectorPart::Has(_) => classes += 1,
                    SelectorPart::Type(_) => types += 1,
                    SelectorPart::Universal | SelectorPart::PseudoElement(_) => {}
                }
            }
        }
        (ids, classes, types)
    }

    /// Check if a complex selector matches a given target node.
    ///
    /// Walks the selector chain right-to-left (target element first),
    /// checking combinators against the ancestor / sibling chain.
    pub(crate) fn complex_selector_matches(
        &self,
        selector: &ComplexSelector,
        target_node: LayoutNodeId,
        hovered_nodes: &HashSet<LayoutNodeId>,
        pressed_nodes: &HashSet<LayoutNodeId>,
        focused_node: Option<LayoutNodeId>,
    ) -> bool {
        if selector.segments.is_empty() {
            return false;
        }

        // Start from the rightmost segment (the target)
        let segments = &selector.segments;
        let (target_compound, _) = &segments[segments.len() - 1];

        if !self.compound_matches(
            target_compound,
            target_node,
            hovered_nodes,
            pressed_nodes,
            focused_node,
        ) {
            return false;
        }

        // Walk backwards through remaining segments (ancestors)
        if segments.len() == 1 {
            return true; // Simple selector, already matched
        }

        let mut current_node = target_node;

        // Walk from right-to-left through the selector segments (skip the last which is the target)
        for i in (0..segments.len() - 1).rev() {
            let (compound, combinator) = &segments[i];
            let combinator = combinator.unwrap_or(Combinator::Descendant);

            match combinator {
                Combinator::Child => {
                    // Must match the immediate parent
                    let parent = match self.element_registry.get_parent(current_node) {
                        Some(p) => p,
                        None => return false,
                    };
                    if !self.compound_matches(
                        compound,
                        parent,
                        hovered_nodes,
                        pressed_nodes,
                        focused_node,
                    ) {
                        return false;
                    }
                    current_node = parent;
                }
                Combinator::Descendant => {
                    // Walk up ancestors until a match is found
                    let ancestors = self.element_registry.ancestors(current_node);
                    let mut found = false;
                    for ancestor in &ancestors {
                        if self.compound_matches(
                            compound,
                            *ancestor,
                            hovered_nodes,
                            pressed_nodes,
                            focused_node,
                        ) {
                            current_node = *ancestor;
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        return false;
                    }
                }
                Combinator::AdjacentSibling => {
                    // Must match the immediately preceding sibling
                    let prev_sibling =
                        match self.element_registry.get_previous_sibling(current_node) {
                            Some(s) => s,
                            None => return false,
                        };
                    if !self.compound_matches(
                        compound,
                        prev_sibling,
                        hovered_nodes,
                        pressed_nodes,
                        focused_node,
                    ) {
                        return false;
                    }
                    current_node = prev_sibling;
                }
                Combinator::GeneralSibling => {
                    // Walk preceding siblings until a match is found
                    let preceding = self.element_registry.get_preceding_siblings(current_node);
                    let mut found = false;
                    for sibling in preceding.iter().rev() {
                        if self.compound_matches(
                            compound,
                            *sibling,
                            hovered_nodes,
                            pressed_nodes,
                            focused_node,
                        ) {
                            current_node = *sibling;
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        return false;
                    }
                }
            }
        }

        true
    }

    /// Apply complex selector rules that match the current state.
    ///
    /// Returns true if any styles were applied.
    pub(crate) fn apply_complex_selector_styles(
        &mut self,
        router: &crate::event_router::EventRouter,
    ) -> bool {
        let stylesheet = match &self.stylesheet {
            Some(s) => s.clone(),
            None => return false,
        };

        let complex_rules = stylesheet.complex_rules();
        if complex_rules.is_empty() {
            return false;
        }

        // Collect current interaction state into sets for efficient lookup
        // `router.hovered_nodes()` is now stable-id-keyed (so a
        // subtree rebuild between mouse-moves doesn't fake an
        // unhover); resolve back to live layout ids for the
        // `contains(layout_id)` checks below.
        let hovered_nodes: HashSet<LayoutNodeId> = router
            .hovered_nodes()
            .filter_map(|s| self.layout_id(s))
            .collect();
        let pressed_nodes: HashSet<LayoutNodeId> =
            router.pressed_target(self).into_iter().collect();
        // Use the router's focused node directly. Previously we walked
        // `element_registry.all_ids()` and checked `router.is_focused()`
        // for each — but that only finds elements with a string `id`.
        // Class-only widgets (e.g. cn::input, which attaches `.cn-input`
        // but no id) were invisible to this lookup, so `.cn-input:focus`
        // never matched in `complex_selector_matches`. The router stores
        // focus by `LayoutNodeId`, so just ask it directly.
        let focused_node: Option<LayoutNodeId> = router.focused();

        // Collect ALL render node IDs (not just those with IDs — .class selectors can match any node)
        let all_node_ids: Vec<LayoutNodeId> = self.render_nodes.keys().copied().collect();

        // Reset previously state-affected nodes to their base props so that
        // styles from rules that no longer match (e.g. :hover ended) are removed
        let prev_affected: HashSet<LayoutNodeId> = std::mem::take(&mut self.complex_state_affected);

        // Snapshot BEFORE the reset for prev_affected nodes — captures the actual
        // visual state (e.g. hover values after a transition completed).  This way
        // when the same hover state is re-applied, before == after → no spurious
        // re-transition.
        let mut pre_reset_snapshots: HashMap<LayoutNodeId, blinc_animation::KeyframeProperties> =
            HashMap::new();
        for &node_id in &prev_affected {
            if let Some(kp) = self.snapshot_before_keyframe_properties(node_id) {
                pre_reset_snapshots.insert(node_id, kp);
            }
        }

        for &node_id in &prev_affected {
            self.state_layers.remove(&node_id);
            if let Some(base) = self.base_styles.get(&node_id) {
                if let Some(render_node) = self.render_nodes.get_mut(&node_id) {
                    render_node.props = base.clone();
                }
            }
            // Reset taffy layout to base for nodes leaving state
            if let Some(base_taffy) = self.base_taffy_styles.get(&node_id) {
                self.layout_tree.set_style(node_id, base_taffy.clone());
            }
        }

        // Re-apply all base (non-state) complex rules for reset nodes,
        // sorted by specificity so type < class < id.
        {
            let mut base_rules: Vec<&(ComplexSelector, crate::element_style::ElementStyle)> =
                complex_rules
                    .iter()
                    .filter(|(selector, _)| !selector.has_state())
                    .collect();
            base_rules.sort_by_key(|(selector, _)| Self::selector_specificity(selector));

            for (selector, style) in &base_rules {
                for &node_id in &prev_affected {
                    if self.complex_selector_matches(
                        selector,
                        node_id,
                        &hovered_nodes,
                        &pressed_nodes,
                        focused_node,
                    ) {
                        if let Some(render_node) = self.render_nodes.get_mut(&node_id) {
                            Self::apply_element_style_to_props(&mut render_node.props, style);
                        }
                    }
                }
            }
        }

        // Re-apply simple ID rules on top (highest specificity)
        if let Some(stylesheet) = &self.stylesheet {
            let stylesheet = stylesheet.clone();
            for &node_id in &prev_affected {
                if let Some(element_id) = self.element_registry.get_id(node_id) {
                    if let Some(base_style) = stylesheet.get(&element_id) {
                        if let Some(render_node) = self.render_nodes.get_mut(&node_id) {
                            Self::apply_element_style_to_props(&mut render_node.props, base_style);
                        }
                    }
                }
            }
        }

        // If any prev_affected nodes were reset, styles changed
        let mut any_applied = !prev_affected.is_empty();

        for (selector, style) in complex_rules {
            // Base (non-state) rules are already applied:
            //   - At tree build time by apply_stylesheet_base_styles()
            //   - For prev_affected (leaving-state) nodes in the reset section above
            // Only state-dependent rules (:hover, :active, :focus, etc.) need per-frame matching.
            let is_state_rule = selector.has_state();
            if !is_state_rule {
                continue;
            }

            for &node_id in &all_node_ids {
                if self.complex_selector_matches(
                    selector,
                    node_id,
                    &hovered_nodes,
                    &pressed_nodes,
                    focused_node,
                ) {
                    // Save base styles for nodes affected by state rules (for future reset)
                    if is_state_rule {
                        if !self.base_styles.contains_key(&node_id) {
                            if let Some(render_node) = self.render_nodes.get(&node_id) {
                                self.base_styles.insert(node_id, render_node.props.clone());
                            }
                        }
                        if !self.base_taffy_styles.contains_key(&node_id) {
                            if let Some(taffy_style) = self.layout_tree.get_style(node_id) {
                                self.base_taffy_styles.insert(node_id, taffy_style);
                            }
                        }
                        self.complex_state_affected.insert(node_id);
                    }

                    // Check for transition support — look in both ID-based and class-based rules
                    let transition_set = {
                        // First try by element ID
                        let by_id = self.element_registry.get_id(node_id).and_then(|eid| {
                            stylesheet.get(&eid).and_then(|s| s.transition.clone())
                        });
                        // If not found, check if any matching base complex rule has a transition
                        by_id.or_else(|| {
                            complex_rules.iter().find_map(|(sel, sty)| {
                                if !sel.has_state()
                                    && self.complex_selector_matches(
                                        sel,
                                        node_id,
                                        &hovered_nodes,
                                        &pressed_nodes,
                                        focused_node,
                                    )
                                {
                                    sty.transition.clone()
                                } else {
                                    None
                                }
                            })
                        })
                    };

                    // For nodes already in prev_affected (sustaining a state from
                    // last frame), skip transition detection — the existing transition
                    // is already in progress or completed. Re-detecting every frame
                    // can cause spurious restarts due to snapshot mismatches between
                    // overlaid and non-overlaid properties.
                    let is_sustaining = prev_affected.contains(&node_id);

                    let before_kp = if !is_sustaining && transition_set.is_some() {
                        self.snapshot_before_keyframe_properties(node_id)
                    } else {
                        None
                    };

                    if let Some(render_node) = self.render_nodes.get_mut(&node_id) {
                        Self::apply_element_style_to_props(&mut render_node.props, style);
                        self.state_layers
                            .entry(node_id)
                            .or_default()
                            .push(style.clone());
                        any_applied = true;
                    }
                    // Apply layout changes from complex selector styles
                    if style.has_layout_props() {
                        if let Some(mut taffy_style) = self.layout_tree.get_style(node_id) {
                            Self::apply_element_style_to_taffy(&mut taffy_style, style);
                            self.layout_tree.set_style(node_id, taffy_style);
                        }
                    }

                    // Detect transitions only for newly entering state (not sustaining)
                    if let (Some(before_kp), Some(transition_set)) = (before_kp, transition_set) {
                        if let Some(after_kp) = self.snapshot_keyframe_properties(node_id) {
                            self.detect_and_start_transitions(
                                node_id,
                                &before_kp,
                                &after_kp,
                                &transition_set,
                            );
                        }
                    }
                }
            }
        }

        // Detect reverse transitions for nodes that WERE state-affected but
        // are no longer matched (e.g. mouse left a :hover element).  These nodes
        // were reset to base above but the transition detection inside the main
        // loop only runs for *matching* rules, so we handle the leave case here.
        for &node_id in &prev_affected {
            if self.complex_state_affected.contains(&node_id) {
                continue; // still matched — already handled above
            }
            // Look up transition set for this node
            let transition_set = {
                let by_id = self
                    .element_registry
                    .get_id(node_id)
                    .and_then(|eid| stylesheet.get(&eid).and_then(|s| s.transition.clone()));
                by_id.or_else(|| {
                    complex_rules.iter().find_map(|(sel, sty)| {
                        if !sel.has_state()
                            && self.complex_selector_matches(
                                sel,
                                node_id,
                                &hovered_nodes,
                                &pressed_nodes,
                                focused_node,
                            )
                        {
                            sty.transition.clone()
                        } else {
                            None
                        }
                    })
                })
            };
            if let Some(transition_set) = transition_set {
                if let Some(before_kp) = pre_reset_snapshots.get(&node_id) {
                    if let Some(after_kp) = self.snapshot_keyframe_properties(node_id) {
                        self.detect_and_start_transitions(
                            node_id,
                            before_kp,
                            &after_kp,
                            &transition_set,
                        );
                    }
                }
            }
        }

        any_applied
    }

    /// Restricted variant of [`Self::apply_complex_selector_styles`]
    /// that runs only against `subtree_nodes` (the freshly-rebuilt
    /// node set produced by `process_pending_subtree_rebuilds`).
    ///
    /// Driven from inside `apply_stylesheet_base_styles_for_subtree`
    /// so that every subtree rebuild lands base styles AND the
    /// matching state styles in a single pass. Phase 4's
    /// fingerprint-gated `apply_stylesheet_state_styles` would
    /// otherwise skip on animation-tick frames (focus / hover
    /// unchanged frame-to-frame), letting the just-clobbered base
    /// stand until a router-state change. Symptom this avoids:
    /// cn::input / cn::textarea opened in a cn::popover whose
    /// stateful refresh chain runs every animation tick — the
    /// rebuild's base pass writes `.cn-input { background: idle }`
    /// over the focused bg, and without this local state pass the
    /// next paint shows an unfocused popup until mouse-move.
    ///
    /// No `prev_affected` reset path — the caller is rebuilding the
    /// subtree from a fresh temp_div so there's no stale state to
    /// roll back. Just save `base_styles` (so future Phase 4 resets
    /// behave correctly) and overlay matching `:hover` / `:active` /
    /// `:focus` rules.
    pub(crate) fn apply_state_styles_for_subtree(
        &mut self,
        subtree_nodes: &[LayoutNodeId],
        router: &crate::event_router::EventRouter,
    ) -> bool {
        if subtree_nodes.is_empty() {
            return false;
        }
        let stylesheet = match &self.stylesheet {
            Some(s) => s.clone(),
            None => return false,
        };
        let complex_rules = stylesheet.complex_rules();
        if complex_rules.is_empty() {
            return false;
        }
        // `router.hovered_nodes()` is now stable-id-keyed (so a
        // subtree rebuild between mouse-moves doesn't fake an
        // unhover); resolve back to live layout ids for the
        // `contains(layout_id)` checks below.
        let hovered_nodes: HashSet<LayoutNodeId> = router
            .hovered_nodes()
            .filter_map(|s| self.layout_id(s))
            .collect();
        let pressed_nodes: HashSet<LayoutNodeId> =
            router.pressed_target(self).into_iter().collect();
        let focused_node: Option<LayoutNodeId> = router.focused();

        let mut any_applied = false;
        for (selector, style) in complex_rules {
            if !selector.has_state() {
                continue;
            }
            for &node_id in subtree_nodes {
                if !self.complex_selector_matches(
                    selector,
                    node_id,
                    &hovered_nodes,
                    &pressed_nodes,
                    focused_node,
                ) {
                    continue;
                }
                // Save base so Phase 4's reset path (which keys on
                // `complex_state_affected`) returns to the correct
                // baseline when the state later clears.
                if !self.base_styles.contains_key(&node_id) {
                    if let Some(render_node) = self.render_nodes.get(&node_id) {
                        self.base_styles.insert(node_id, render_node.props.clone());
                    }
                }
                if !self.base_taffy_styles.contains_key(&node_id) {
                    if let Some(taffy_style) = self.layout_tree.get_style(node_id) {
                        self.base_taffy_styles.insert(node_id, taffy_style);
                    }
                }
                self.complex_state_affected.insert(node_id);
                if let Some(render_node) = self.render_nodes.get_mut(&node_id) {
                    Self::apply_element_style_to_props(&mut render_node.props, style);
                    self.state_layers
                        .entry(node_id)
                        .or_default()
                        .push(style.clone());
                    any_applied = true;
                }
            }
        }
        any_applied
    }

    /// Apply SVG tag-name CSS rules (e.g., `path { fill: red; }`, `#my-svg circle { stroke: blue; }`).
    ///
    /// For each complex rule targeting an SVG tag name, finds SVG layout nodes whose
    /// ancestor chain matches the remaining selector segments and stores per-tag
    /// style overrides on those nodes' RenderProps.
    pub(crate) fn apply_svg_tag_styles(
        &mut self,
        router: &crate::event_router::EventRouter,
    ) -> bool {
        let stylesheet = match &self.stylesheet {
            Some(s) => s.clone(),
            None => return false,
        };

        let svg_tag_rules = stylesheet.svg_tag_rules();
        if svg_tag_rules.is_empty() {
            return false;
        }

        // Collect interaction state
        // `router.hovered_nodes()` is now stable-id-keyed (so a
        // subtree rebuild between mouse-moves doesn't fake an
        // unhover); resolve back to live layout ids for the
        // `contains(layout_id)` checks below.
        let hovered_nodes: HashSet<LayoutNodeId> = router
            .hovered_nodes()
            .filter_map(|s| self.layout_id(s))
            .collect();
        let pressed_nodes: HashSet<LayoutNodeId> =
            router.pressed_target(self).into_iter().collect();
        let focused_node: Option<LayoutNodeId> = {
            let mut focused = None;
            for id in self.element_registry.all_ids() {
                if let Some(nid) = self.element_registry.get(&id) {
                    if router.is_focused(nid) {
                        focused = Some(nid);
                        break;
                    }
                }
            }
            focused
        };

        // Find all SVG layout nodes
        let svg_nodes: Vec<LayoutNodeId> = self
            .render_nodes
            .keys()
            .copied()
            .filter(|&nid| self.element_registry.get_element_type(nid) == Some("svg"))
            .collect();

        if svg_nodes.is_empty() {
            return false;
        }

        let mut any_changed = false;

        for &svg_node in &svg_nodes {
            let mut tag_styles: HashMap<String, crate::element::SvgTagStyle> = HashMap::new();

            for &(tag_name, ancestor_segments, style) in &svg_tag_rules {
                // Check if ancestor segments match the SVG node
                let matches = if ancestor_segments.is_empty() {
                    // Bare tag selector (e.g., `path { ... }`) — matches all SVGs
                    true
                } else {
                    // Build a temporary ComplexSelector with ancestor segments + SVG node as target
                    // The last ancestor segment targets the SVG node itself
                    let last_idx = ancestor_segments.len() - 1;
                    let (last_compound, _) = &ancestor_segments[last_idx];

                    if !self.compound_matches(
                        last_compound,
                        svg_node,
                        &hovered_nodes,
                        &pressed_nodes,
                        focused_node,
                    ) {
                        false
                    } else if ancestor_segments.len() == 1 {
                        // Only one ancestor segment and it matched the SVG node
                        true
                    } else {
                        // Walk remaining ancestor segments up the tree
                        let mut current_node = svg_node;
                        let mut all_matched = true;
                        for i in (0..last_idx).rev() {
                            let (compound, combinator) = &ancestor_segments[i];
                            let combinator = combinator.unwrap_or(Combinator::Descendant);
                            match combinator {
                                Combinator::Child => {
                                    let parent =
                                        match self.element_registry.get_parent(current_node) {
                                            Some(p) => p,
                                            None => {
                                                all_matched = false;
                                                break;
                                            }
                                        };
                                    if !self.compound_matches(
                                        compound,
                                        parent,
                                        &hovered_nodes,
                                        &pressed_nodes,
                                        focused_node,
                                    ) {
                                        all_matched = false;
                                        break;
                                    }
                                    current_node = parent;
                                }
                                Combinator::Descendant => {
                                    let ancestors = self.element_registry.ancestors(current_node);
                                    let mut found = false;
                                    for ancestor in &ancestors {
                                        if self.compound_matches(
                                            compound,
                                            *ancestor,
                                            &hovered_nodes,
                                            &pressed_nodes,
                                            focused_node,
                                        ) {
                                            current_node = *ancestor;
                                            found = true;
                                            break;
                                        }
                                    }
                                    if !found {
                                        all_matched = false;
                                        break;
                                    }
                                }
                                _ => {
                                    all_matched = false;
                                    break;
                                }
                            }
                        }
                        all_matched
                    }
                };

                if matches {
                    // Extract SVG-relevant properties from ElementStyle into SvgTagStyle
                    let entry = tag_styles.entry(tag_name.to_string()).or_default();
                    if let Some(fill) = style.fill {
                        entry.fill = Some([fill.r, fill.g, fill.b, fill.a]);
                    }
                    if let Some(stroke) = style.stroke {
                        entry.stroke = Some([stroke.r, stroke.g, stroke.b, stroke.a]);
                    }
                    if let Some(sw) = style.stroke_width {
                        entry.stroke_width = Some(sw);
                    }
                    if let Some(ref da) = style.stroke_dasharray {
                        entry.stroke_dasharray = Some(da.clone());
                    }
                    if let Some(offset) = style.stroke_dashoffset {
                        entry.stroke_dashoffset = Some(offset);
                    }
                    if let Some(opacity) = style.opacity {
                        entry.opacity = Some(opacity);
                    }
                }
            }

            // Apply tag styles to render props
            if !tag_styles.is_empty() {
                if let Some(render_node) = self.render_nodes.get_mut(&svg_node) {
                    if render_node.props.svg_tag_styles != tag_styles {
                        render_node.props.svg_tag_styles = tag_styles;
                        any_changed = true;
                    }
                }
            } else if let Some(render_node) = self.render_nodes.get_mut(&svg_node) {
                if !render_node.props.svg_tag_styles.is_empty() {
                    render_node.props.svg_tag_styles.clear();
                    any_changed = true;
                }
            }
        }

        any_changed
    }
}
