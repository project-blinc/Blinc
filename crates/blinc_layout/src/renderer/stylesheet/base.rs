//! Apply base (non-state) stylesheet styles to a tree or subtree.
//!
//! Two driver methods plus a small helper:
//!
//! - `apply_stylesheet_base_styles` — runs once after the stylesheet
//!   is set on a freshly built tree. Walks complex rules in
//!   ascending-specificity order (type < class < id-shaped chains),
//!   then applies simple `#id` rules last so they win, then handles
//!   SVG tag rules and propagates inherited text properties from
//!   parent to child.
//! - `apply_stylesheet_base_styles_for_subtree` — same flow but
//!   restricted to a subtree, called by `process_pending_subtree_rebuilds`
//!   so newly-built children pick up class- and id-based base styles
//!   that `collect_render_props_boxed` only resolves for `#id`.
//! - `collect_subtree_ids` — DFS into the layout tree to gather all
//!   descendant node ids; private to this file.
//!
//! Both passes also eagerly seed `base_styles` for nodes matching a
//! state-rule class so the lazy save inside
//! `apply_complex_selector_styles` doesn't capture
//! Stateful-rebuilt-into-hover props as the "base".

use std::collections::{HashMap, HashSet};

use crate::tree::LayoutNodeId;

use super::super::{ElementType, RenderTree};

/// The text properties a child takes from its ancestors when it says
/// nothing itself.
#[derive(Clone, Copy, Default)]
struct InheritedText {
    decoration: Option<crate::element_style::TextDecoration>,
    decoration_color: Option<[f32; 4]>,
    decoration_thickness: Option<f32>,
    white_space: Option<crate::element_style::WhiteSpace>,
    overflow: Option<crate::element_style::TextOverflow>,
    color: Option<[f32; 4]>,
    align: Option<crate::div::TextAlign>,
    font_style: Option<crate::element_style::FontStyle>,
}

impl InheritedText {
    fn from_node(node: &super::super::RenderNode) -> Self {
        Self {
            decoration: node.props.text_decoration,
            decoration_color: node.props.text_decoration_color,
            decoration_thickness: node.props.text_decoration_thickness,
            white_space: node.props.white_space,
            overflow: node.props.text_overflow,
            color: node.props.text_color,
            align: node.props.text_align,
            font_style: node.props.font_style,
        }
    }
}

impl RenderTree {
    /// Apply base stylesheet styles to all registered elements.
    ///
    /// This must be called after `set_stylesheet_arc()` when the stylesheet
    /// was set AFTER tree construction. During tree build, `collect_render_props`
    /// applies base styles if the stylesheet is already set. But when the stylesheet
    /// is set after `from_element_with_registry()`, the base styles (background,
    /// border-radius, opacity, etc.) are missing. This method fixes that by
    /// iterating all registered elements and applying their base CSS styles.
    pub fn apply_stylesheet_base_styles(&mut self) {
        let stylesheet = match &self.stylesheet {
            Some(s) => s.clone(),
            None => return,
        };

        // Base-style apply can change `props.cursor` on any node, so
        // invalidate the bare-mouse-move pipeline cache to force a
        // recompute on next read.
        self.invalidate_mouse_move_pipeline_cache();

        // CSS specificity order: type(0,0,1) < class(0,1,0) < id(1,0,0)
        // Apply complex base rules FIRST (lower specificity: type, class selectors)
        // sorted by ascending specificity so higher-specificity rules overwrite lower.
        // Then apply simple ID rules LAST (highest specificity, always override).
        let complex_rules = stylesheet.complex_rules();
        if !complex_rules.is_empty() {
            let empty_set = HashSet::new();

            // Collect non-state rules and sort by specificity (ascending)
            let mut base_rules: Vec<&(
                crate::css_parser::ComplexSelector,
                crate::element_style::ElementStyle,
            )> = complex_rules
                .iter()
                .filter(|(selector, _)| !selector.has_state())
                .collect();
            base_rules.sort_by_key(|(selector, _)| Self::selector_specificity(selector));

            // Build inverted class index once (single lock acquisition) for O(1) lookups
            let class_to_nodes = self.element_registry.class_to_nodes_index();

            for (selector, style) in base_rules {
                // Fast path: simple `.class` selectors use inverted index — O(matched_nodes)
                if let Some(class_name) = selector.simple_class_name() {
                    if let Some(node_ids) = class_to_nodes.get(class_name) {
                        for &node_id in node_ids {
                            if let Some(render_node) = self.render_nodes.get_mut(&node_id) {
                                Self::apply_element_style_to_props(&mut render_node.props, style);
                            }
                        }
                    }
                    continue;
                }

                // Slow path: complex selectors (combinators, structural pseudos) — O(all_nodes)
                let all_node_ids: Vec<LayoutNodeId> = self.render_nodes.keys().copied().collect();
                for &node_id in &all_node_ids {
                    if self
                        .complex_selector_matches(selector, node_id, &empty_set, &empty_set, None)
                    {
                        if let Some(render_node) = self.render_nodes.get_mut(&node_id) {
                            Self::apply_element_style_to_props(&mut render_node.props, style);
                        }
                    }
                }
            }

            // Eagerly save base_styles for nodes matching classes that also have
            // state rules (:hover, :active, :focus). This prevents the lazy save
            // in apply_complex_selector_styles() from capturing contaminated props
            // (e.g. inline hover backgrounds set by Stateful component rebuilds).
            // Only save if not already present — this function runs for the entire
            // tree and nodes outside a rebuild may still carry hover/active styles.
            let state_class_names: HashSet<&str> = complex_rules
                .iter()
                .filter(|(sel, _)| sel.has_state())
                .filter_map(|(sel, _)| sel.class_name_with_state())
                .collect();
            for class_name in &state_class_names {
                if let Some(node_ids) = class_to_nodes.get(*class_name) {
                    for &node_id in node_ids {
                        if !self.base_styles.contains_key(&node_id) {
                            if let Some(render_node) = self.render_nodes.get(&node_id) {
                                self.base_styles.insert(node_id, render_node.props.clone());
                            }
                        }
                    }
                }
            }
        }

        // Apply simple ID rules LAST — #id has highest specificity and overrides
        // type/class selectors applied above.
        let registered_ids: Vec<(String, LayoutNodeId)> = self
            .element_registry
            .all_ids()
            .into_iter()
            .filter_map(|id| self.element_registry.get(&id).map(|node_id| (id, node_id)))
            .collect();

        for (element_id, node_id) in &registered_ids {
            if let Some(base_style) = stylesheet.get(element_id) {
                if let Some(render_node) = self.render_nodes.get_mut(node_id) {
                    Self::apply_element_style_to_props(&mut render_node.props, base_style);
                }
            }
        }

        // Sync CSS text-align into baked TextData for text nodes.
        // text-align may have been set by CSS above but TextData was built before CSS.
        for render_node in self.render_nodes.values_mut() {
            if let Some(ta) = render_node.props.text_align {
                if let ElementType::Text(ref mut text_data) = render_node.element_type {
                    text_data.align = ta;
                }
            }
        }

        // Update Stateful base_render_props with CSS-applied values.
        // This ensures that state changes (hover, press) start from CSS-enhanced
        // base props, preserving CSS overrides like border-radius across state changes.
        for (&node_id, render_node) in &self.render_nodes {
            if crate::stateful::has_stateful_base_updater(node_id) {
                crate::stateful::update_stateful_base_props(node_id, render_node.props.clone());
            }
        }

        // Apply base (non-state) SVG tag-name rules to SVG nodes
        let svg_tag_rules = stylesheet.svg_tag_rules();
        if !svg_tag_rules.is_empty() {
            let svg_nodes: Vec<LayoutNodeId> = self
                .render_nodes
                .keys()
                .copied()
                .filter(|&nid| self.element_registry.get_element_type(nid) == Some("svg"))
                .collect();

            for &svg_node in &svg_nodes {
                let mut tag_styles: HashMap<String, crate::element::SvgTagStyle> = HashMap::new();
                for &(tag_name, ancestor_segments, style) in &svg_tag_rules {
                    // Skip state-dependent rules (handled by apply_svg_tag_styles)
                    let has_state = ancestor_segments.iter().any(|(c, _)| c.has_state());
                    if has_state {
                        continue;
                    }
                    let matches = if ancestor_segments.is_empty() {
                        true
                    } else {
                        // Check if ancestor segments match the SVG node's chain
                        let last_idx = ancestor_segments.len() - 1;
                        let (last_compound, _) = &ancestor_segments[last_idx];
                        // Base-style application is state-agnostic — no
                        // hover/press/focus interaction here. Pass empty
                        // sets so :hover / :has() inside don't accidentally
                        // match. `STATE` is reused across all branches so
                        // we don't repeat the empty-set construction.
                        let empty: std::collections::HashSet<crate::tree::LayoutNodeId> =
                            std::collections::HashSet::new();
                        if !self.compound_matches(last_compound, svg_node, &empty, &empty, None) {
                            false
                        } else if ancestor_segments.len() == 1 {
                            true
                        } else {
                            let mut current_node = svg_node;
                            let mut all_matched = true;
                            for i in (0..last_idx).rev() {
                                let (compound, combinator) = &ancestor_segments[i];
                                let combinator =
                                    combinator.unwrap_or(crate::css_parser::Combinator::Descendant);
                                match combinator {
                                    crate::css_parser::Combinator::Child => {
                                        match self.element_registry.get_parent(current_node) {
                                            Some(parent) => {
                                                if !self.compound_matches(
                                                    compound, parent, &empty, &empty, None,
                                                ) {
                                                    all_matched = false;
                                                    break;
                                                }
                                                current_node = parent;
                                            }
                                            None => {
                                                all_matched = false;
                                                break;
                                            }
                                        }
                                    }
                                    crate::css_parser::Combinator::Descendant => {
                                        let ancestors =
                                            self.element_registry.ancestors(current_node);
                                        let mut found = false;
                                        for ancestor in &ancestors {
                                            if self.compound_matches(
                                                compound, *ancestor, &empty, &empty, None,
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
                if !tag_styles.is_empty() {
                    if let Some(render_node) = self.render_nodes.get_mut(&svg_node) {
                        render_node.props.svg_tag_styles = tag_styles;
                    }
                }
            }
        }

        // Post-pass: propagate inherited text properties (text-decoration, white-space,
        // text-overflow, text-align) from parent to child nodes. This must run AFTER all
        // CSS styles are applied above, because during initial tree construction the
        // stylesheet wasn't set yet and inherit_text_props_from_parent found no parent values.
        // PARENT BEFORE CHILD. Each node copies from its immediate
        // parent, so a single pass carries an inherited value exactly
        // one level down. `render_nodes` is insertion-ordered, which is
        // parent-first for a tree built top-down -- but not after a
        // subtree rebuild re-inserts a parent behind children that
        // outlived it. Walking depth-first from the root makes the pass
        // converge at any depth regardless of insertion history. Nodes
        // outside the main tree (overlays) are appended afterwards, as
        // before.
        let mut all_node_ids: Vec<LayoutNodeId> = Vec::with_capacity(self.render_nodes.len());
        if let Some(root) = self.root {
            self.collect_subtree_ids(root, &mut all_node_ids);
        }
        let visited: HashSet<LayoutNodeId> = all_node_ids.iter().copied().collect();
        all_node_ids.extend(self.render_nodes.keys().filter(|id| !visited.contains(id)));

        for node_id in all_node_ids {
            let parent_id = match self.element_registry.get_parent(node_id) {
                Some(id) => id,
                None => continue,
            };
            // Read parent text props (need separate borrow)
            let parent_text_props = self.render_nodes.get(&parent_id).map(|n| {
                (
                    n.props.text_decoration,
                    n.props.text_decoration_color,
                    n.props.text_decoration_thickness,
                    n.props.white_space,
                    n.props.text_overflow,
                    n.props.text_color,
                    n.props.text_align,
                    n.props.fill,
                    n.props.stroke,
                    n.props.stroke_width,
                    n.props.font_style,
                )
            });
            if let Some((td, td_color, td_thick, ws, to, tc, ta, fill, stroke, stroke_w, fstyle)) =
                parent_text_props
            {
                if let Some(node) = self.render_nodes.get_mut(&node_id) {
                    if node.props.text_decoration.is_none() {
                        node.props.text_decoration = td;
                    }
                    if node.props.text_decoration_color.is_none() {
                        node.props.text_decoration_color = td_color;
                    }
                    if node.props.text_decoration_thickness.is_none() {
                        node.props.text_decoration_thickness = td_thick;
                    }
                    if node.props.white_space.is_none() {
                        node.props.white_space = ws;
                    }
                    if node.props.text_overflow.is_none() {
                        node.props.text_overflow = to;
                    }
                    if node.props.text_color.is_none() {
                        node.props.text_color = tc;
                    }
                    if node.props.text_align.is_none() {
                        if let Some(ta) = ta {
                            node.props.text_align = Some(ta);
                            // Also update baked TextData.align so rendering uses the
                            // inherited value (TextData is built before CSS post-pass)
                            if let ElementType::Text(ref mut text_data) = node.element_type {
                                text_data.align = ta;
                            }
                        }
                    }
                    // SVG fill/stroke (CSS spec: inherited in SVG)
                    if node.props.fill.is_none() {
                        node.props.fill = fill;
                    }
                    if node.props.stroke.is_none() {
                        node.props.stroke = stroke;
                    }
                    if node.props.stroke_width.is_none() {
                        node.props.stroke_width = stroke_w;
                    }
                    if node.props.font_style.is_none() {
                        node.props.font_style = fstyle;
                    }
                }
            }
        }
    }

    /// Apply CSS base styles (class and ID selectors) to a subtree after rebuild.
    ///
    /// Called after `process_pending_subtree_rebuilds` builds new child nodes.
    /// `collect_render_props_boxed` only applies `#id` styles inline; class-based
    /// selectors (`.sort-item`, `.grid-item`, etc.) are resolved by
    /// `apply_stylesheet_base_styles()` which only runs at full tree creation.
    /// This method fills that gap for incrementally rebuilt subtrees.
    pub(crate) fn apply_stylesheet_base_styles_for_subtree(
        &mut self,
        parent_id: LayoutNodeId,
        router: Option<&crate::event_router::EventRouter>,
    ) {
        let stylesheet = match &self.stylesheet {
            Some(s) => s.clone(),
            None => return,
        };

        // Collect all node IDs in the subtree (parent + descendants)
        let mut subtree_nodes = Vec::new();
        self.collect_subtree_ids(parent_id, &mut subtree_nodes);

        if subtree_nodes.is_empty() {
            return;
        }

        // Apply complex base rules (class selectors, combinators) — lower specificity first
        let complex_rules = stylesheet.complex_rules();
        if !complex_rules.is_empty() {
            let empty_set = HashSet::new();

            let mut base_rules: Vec<&(
                crate::css_parser::ComplexSelector,
                crate::element_style::ElementStyle,
            )> = complex_rules
                .iter()
                .filter(|(selector, _)| !selector.has_state())
                .collect();
            base_rules.sort_by_key(|(selector, _)| Self::selector_specificity(selector));

            // Build inverted class index for the subtree nodes
            let class_to_nodes = self.element_registry.class_to_nodes_index();
            // Filter to only subtree nodes for simple class lookups
            let subtree_set: HashSet<LayoutNodeId> = subtree_nodes.iter().copied().collect();

            for (selector, style) in base_rules {
                // Fast path: simple `.class` selectors use inverted index
                if let Some(class_name) = selector.simple_class_name() {
                    if let Some(node_ids) = class_to_nodes.get(class_name) {
                        for &node_id in node_ids {
                            if subtree_set.contains(&node_id) {
                                if let Some(render_node) = self.render_nodes.get_mut(&node_id) {
                                    Self::apply_element_style_to_props(
                                        &mut render_node.props,
                                        style,
                                    );
                                }
                            }
                        }
                    }
                    continue;
                }

                // Slow path: complex selectors need full matching
                for &node_id in &subtree_nodes {
                    if self
                        .complex_selector_matches(selector, node_id, &empty_set, &empty_set, None)
                    {
                        if let Some(render_node) = self.render_nodes.get_mut(&node_id) {
                            Self::apply_element_style_to_props(&mut render_node.props, style);
                        }
                    }
                }
            }

            // Eagerly save base_styles for subtree nodes matching classes that
            // also have :hover/:active/:focus state rules. This prevents the
            // lazy save in apply_complex_selector_styles() from capturing
            // contaminated props set by Stateful component rebuilds.
            let state_class_names: HashSet<&str> = complex_rules
                .iter()
                .filter(|(sel, _)| sel.has_state())
                .filter_map(|(sel, _)| sel.class_name_with_state())
                .collect();
            for class_name in &state_class_names {
                if let Some(node_ids) = class_to_nodes.get(*class_name) {
                    for &node_id in node_ids {
                        if subtree_set.contains(&node_id) {
                            if let Some(render_node) = self.render_nodes.get(&node_id) {
                                self.base_styles.insert(node_id, render_node.props.clone());
                            }
                        }
                    }
                }
            }
        }

        // Apply simple ID rules (highest specificity, overrides class selectors)
        for &node_id in &subtree_nodes {
            if let Some(element_id) = self.element_registry.get_id(node_id) {
                if let Some(base_style) = stylesheet.get(&element_id) {
                    if let Some(render_node) = self.render_nodes.get_mut(&node_id) {
                        Self::apply_element_style_to_props(&mut render_node.props, base_style);
                    }
                }
            }
        }

        // Update Stateful base_render_props for subtree nodes with CSS-applied values
        for &node_id in &subtree_nodes {
            if crate::stateful::has_stateful_base_updater(node_id) {
                if let Some(render_node) = self.render_nodes.get(&node_id) {
                    crate::stateful::update_stateful_base_props(node_id, render_node.props.clone());
                }
            }
        }

        // Propagate inherited text properties (color, text-decoration,
        // etc.) down the rebuilt subtree, seeded from what the subtree's
        // parent resolved to.
        //
        // Carried down rather than read per node from the element
        // registry: a rebuilt node's registry parent can name a node
        // that is not its layout parent, and the flat form then found
        // nothing to inherit and left the value unset. Text under a
        // rebuilt subtree fell back to black on a dark scheme, and only
        // after the swap that rebuilt it.
        let seed = self
            .element_registry
            .get_parent(parent_id)
            .and_then(|p| self.render_nodes.get(&p))
            .map(InheritedText::from_node)
            .unwrap_or_default();
        self.propagate_inherited_text(parent_id, seed);

        // Apply matching `:hover`, `:active`, `:focus` rules on top
        // of the base CSS we just wrote, restricted to the rebuilt
        // subtree. Without this, every subtree rebuild kind
        // (Structural / LayoutProps / Visual) silently rewinds
        // focused / hovered visuals on the rebuilt subtree because
        // the class-base rule (`.cn-input { background: idle }` etc)
        // overwrites whatever the rebuild's temp_div produced.
        // Phase 4's gated state-style pass is the usual restorer,
        // but it skips on frames where the router fingerprint hasn't
        // moved — exactly the case for animation-tick-driven
        // rebuilds (Stateful refresh while a spring / keyframe is
        // still running). The router is plumbed through
        // `process_pending_subtree_rebuilds`; callers without a live
        // router (web/mobile cold paths, tests) pass `None` and lose
        // the state-restore, which only matters when their rebuild
        // chain races an in-flight focused-state animation.
        if let Some(router) = router {
            self.apply_state_styles_for_subtree(&subtree_nodes, router);
        }
    }

    /// Collect all node IDs in a subtree (the node itself + all descendants).
    /// Fill a node's unset text properties from what it inherits, then
    /// carry the result to its layout children.
    fn propagate_inherited_text(&mut self, node_id: LayoutNodeId, inherited: InheritedText) {
        let carried = match self.render_nodes.get_mut(&node_id) {
            Some(node) => {
                let p = &mut node.props;
                p.text_decoration = p.text_decoration.or(inherited.decoration);
                p.text_decoration_color = p.text_decoration_color.or(inherited.decoration_color);
                p.text_decoration_thickness = p
                    .text_decoration_thickness
                    .or(inherited.decoration_thickness);
                p.white_space = p.white_space.or(inherited.white_space);
                p.text_overflow = p.text_overflow.or(inherited.overflow);
                p.text_color = p.text_color.or(inherited.color);
                p.font_style = p.font_style.or(inherited.font_style);
                if p.text_align.is_none()
                    && let Some(align) = inherited.align
                {
                    p.text_align = Some(align);
                    // TextData is baked before this pass, so the
                    // renderer reads the inherited alignment from there.
                    if let ElementType::Text(ref mut text_data) = node.element_type {
                        text_data.align = align;
                    }
                }
                InheritedText::from_node(node)
            }
            // A layout node with no render node inherits nothing of its
            // own, but must not break the chain to its children.
            None => inherited,
        };
        for child_id in self.layout_tree.children(node_id) {
            self.propagate_inherited_text(child_id, carried);
        }
    }

    pub(crate) fn collect_subtree_ids(&self, node_id: LayoutNodeId, out: &mut Vec<LayoutNodeId>) {
        out.push(node_id);
        for child_id in self.layout_tree.children(node_id) {
            self.collect_subtree_ids(child_id, out);
        }
    }
}

#[cfg(test)]
mod inherit_tests {
    use crate::css_parser::Stylesheet;
    use crate::div::div;
    use crate::renderer::RenderTree;
    use crate::text::text;

    /// An inherited colour must reach a deeply nested text node in ONE
    /// pass.
    ///
    /// The propagation copies from the immediate parent, so the pass
    /// only converges if parents are visited first. Insertion order
    /// happens to satisfy that for a freshly built tree -- this pins the
    /// invariant so a rebuild that re-inserts a parent behind its
    /// children cannot quietly cost a frame per level.
    #[test]
    fn inherited_text_color_reaches_a_deep_child_in_one_pass() {
        let host = div().class("tinted").child(
            // Two intermediate wrappers, as a Stateful-backed widget
            // introduces between the classed node and its label.
            div().child(div().child(text("hello"))),
        );
        let mut tree = RenderTree::from_element(&host);
        tree.set_stylesheet(Stylesheet::parse(".tinted { color: #ff0000 }").expect("css"));
        tree.apply_stylesheet_base_styles();

        // Deepest node is the text.
        let mut deepest = tree.root().expect("root");
        loop {
            let kids = tree.layout_tree.children(deepest);
            match kids.first() {
                Some(&c) => deepest = c,
                None => break,
            }
        }
        let color = tree
            .render_nodes
            .get(&deepest)
            .and_then(|n| n.props.text_color)
            .expect("the text node must have inherited a colour");
        assert_eq!(
            color[0], 1.0,
            "expected the red from `.tinted`, got {color:?}"
        );
    }
}
