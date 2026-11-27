//! Layout-side stylesheet application: write CSS layout properties
//! into taffy `Style`s before `compute_layout()`.
//!
//! Three driver methods:
//!
//! - `apply_stylesheet_layout_overrides` — writes only layout
//!   properties (size, padding/margin/gap, flex, alignment, overflow,
//!   position, inset) into taffy. Called before each
//!   `compute_layout()`. Walks complex non-state rules in
//!   ascending-specificity order, then `#id` rules last so they win,
//!   then collects scroll physics for any node that ended up with
//!   `overflow: scroll`.
//! - `apply_all_stylesheet_styles` — fast path used during the full
//!   rebuild flow that needs both visual + layout: walks the rule set
//!   once, building the inverted class index a single time, applying
//!   visual props inline and queueing layout overrides for a single
//!   trailing pass.
//!
//! Scroll physics for nodes whose CSS makes them scroll is in `scroll_physics`.

use taffy::Overflow;
use taffy::prelude::*;

use crate::tree::LayoutNodeId;

use super::super::{ElementType, RenderTree};
use crate::tree::NodeSet;

impl RenderTree {
    /// Apply layout properties from the stylesheet to taffy nodes.
    ///
    /// This must be called after tree build and before compute_layout().
    /// It iterates all registered element IDs and applies layout overrides
    /// (width, height, padding, margin, gap, flex-direction, alignment, etc.)
    /// from the stylesheet to the corresponding taffy nodes.
    pub fn apply_stylesheet_layout_overrides(&mut self) {
        self.layout_restyle.clear();
        self.apply_stylesheet_layout_overrides_filtered(None);
    }

    /// Subtree-scoped variant of [`Self::apply_stylesheet_layout_overrides`].
    ///
    /// The stateful subtree rebuild path re-applies VISUAL base styles via
    /// `apply_stylesheet_base_styles_for_subtree` but historically skipped
    /// LAYOUT overrides, so a rebuilt node (fresh `LayoutNodeId`) lost any
    /// class-driven padding / size / flex: an opened accordion trigger
    /// collapsed to zero padding, a hovered menubar submenu row shifted.
    /// Re-applying layout overrides for just the rebuilt subtree restores
    /// them without a whole-tree pass.
    pub(crate) fn apply_stylesheet_layout_overrides_for_subtree(
        &mut self,
        parent_id: LayoutNodeId,
    ) {
        let mut subtree_nodes = Vec::new();
        self.collect_subtree_ids(parent_id, &mut subtree_nodes);
        if subtree_nodes.is_empty() {
            return;
        }
        let subtree_set: NodeSet = subtree_nodes.into_iter().collect();
        self.apply_stylesheet_layout_overrides_filtered(Some(&subtree_set));
    }

    pub(super) fn apply_stylesheet_layout_overrides_filtered(
        &mut self,
        subtree_filter: Option<&NodeSet>,
    ) {
        use crate::element_style::{
            StyleAlign, StyleDisplay, StyleFlexDirection, StyleJustify, StyleOverflow,
            StylePosition,
        };

        let stylesheet = match &self.stylesheet {
            Some(s) => s.clone(),
            None => return,
        };

        // The rules of a class that follows a signal sit in the node's layer for it, over the
        // snapshot folded below. Matched with the class on, they would be folded into the
        // snapshot and stay when the class goes, so the rules are matched without such classes.
        let withheld = self.withhold_dynamic_classes();

        // Collect (node_id, style) pairs that have layout overrides.
        // Apply complex rules (class/type selectors) FIRST — lower specificity.
        // Then apply simple ID rules LAST — higher specificity overrides.
        let mut overrides: Vec<(LayoutNodeId, &crate::element_style::ElementStyle)> = Vec::new();

        // Complex rules: class, type, and combinator selectors. A node takes the rules that match
        // it, in ascending specificity.
        let complex_rules = stylesheet.complex_rules();
        let layout_rules = stylesheet.stateless_layout_rules();
        if layout_rules.contains(&true) {
            let no_state = crate::tree::NodeSet::default();
            let nodes: Vec<LayoutNodeId> = match subtree_filter {
                Some(set) => set.iter().copied().collect(),
                None => self.render_nodes.keys().copied().collect(),
            };
            for node_id in nodes {
                let mut matched: Vec<usize> = self
                    .candidate_rules_for(&stylesheet, node_id)
                    .into_iter()
                    .filter(|i| layout_rules[*i])
                    .filter(|i| {
                        self.complex_selector_matches(
                            &complex_rules[*i].0,
                            node_id,
                            &no_state,
                            &no_state,
                            None,
                        )
                    })
                    .collect();
                matched.sort_by_key(|&i| {
                    (
                        Self::rule_order(&complex_rules[i].0, &complex_rules[i].1),
                        i,
                    )
                });
                for i in matched {
                    overrides.push((node_id, &complex_rules[i].1));
                }
            }
        }

        // Simple ID rules: highest specificity, applied last
        let ids: Vec<(LayoutNodeId, String)> = match subtree_filter {
            Some(set) => set
                .iter()
                .filter_map(|&node| Some((node, self.element_registry.get_id(node)?)))
                .collect(),
            None => self
                .element_registry
                .all_ids()
                .into_iter()
                .filter_map(|id| Some((self.element_registry.get(&id)?, id)))
                .collect(),
        };
        for (node_id, id) in &ids {
            if let Some(style) = stylesheet.get(id)
                && style.has_layout_props()
            {
                overrides.push((*node_id, style));
            }
        }
        self.restore_classes(withheld);

        // A node that took up its bound classes or a state rule before this pass holds a
        // snapshot of itself without them, and the rules applied here belong to that snapshot:
        // fold them in, then lay the classes and states over it again below.
        let mut snapshotted: Vec<LayoutNodeId> = Vec::new();
        for (node_id, es) in &overrides {
            if let Some(base) = self.base_taffy_styles.get_mut(node_id) {
                Self::apply_element_style_to_taffy(base, es);
                snapshotted.push(*node_id);
            }
        }

        // One read and one write per node, however many rules it takes. The sort is stable, so
        // each node keeps its rules in the order above.
        overrides.sort_by_key(|(node_id, _)| *node_id);
        let mut pending: Option<(LayoutNodeId, Style)> = None;
        for (node_id, es) in overrides {
            if let Some((node, _)) = &pending
                && *node != node_id
                && let Some((node, style)) = pending.take()
            {
                self.layout_tree.set_style(node, style);
            }
            if pending.is_none() {
                let Some(style) = self.layout_tree.get_style(node_id) else {
                    continue;
                };
                pending = Some((node_id, style));
            }
            let Some((_, style)) = pending.as_mut() else {
                continue;
            };

            // Sizing
            if let Some(w) = es.width {
                style.size.width = match w {
                    crate::element_style::StyleDimension::Length(px) => Dimension::length(px),
                    crate::element_style::StyleDimension::Percent(p) => Dimension::percent(p),
                    crate::element_style::StyleDimension::Auto => Dimension::auto(),
                };
                if matches!(w, crate::element_style::StyleDimension::Auto) {
                    style.flex_basis = Dimension::auto();
                    style.flex_grow = 0.0;
                    style.flex_shrink = 0.0;
                }
            }
            if let Some(h) = es.height {
                style.size.height = match h {
                    crate::element_style::StyleDimension::Length(px) => Dimension::length(px),
                    crate::element_style::StyleDimension::Percent(p) => Dimension::percent(p),
                    crate::element_style::StyleDimension::Auto => Dimension::auto(),
                };
                if matches!(h, crate::element_style::StyleDimension::Auto) {
                    style.flex_basis = Dimension::auto();
                    style.flex_grow = 0.0;
                    style.flex_shrink = 0.0;
                }
            }

            // Display & flex direction
            if let Some(display) = es.display {
                style.display = match display {
                    StyleDisplay::Flex => Display::Flex,
                    StyleDisplay::Block => Display::Block,
                    StyleDisplay::Grid => Display::Grid,
                    StyleDisplay::None => Display::None,
                };
            }
            if let Some(dir) = es.flex_direction {
                style.flex_direction = match dir {
                    StyleFlexDirection::Row => FlexDirection::Row,
                    StyleFlexDirection::Column => FlexDirection::Column,
                    StyleFlexDirection::RowReverse => FlexDirection::RowReverse,
                    StyleFlexDirection::ColumnReverse => FlexDirection::ColumnReverse,
                };
            }
            if let Some(wrap) = es.flex_wrap {
                style.flex_wrap = if wrap {
                    FlexWrap::Wrap
                } else {
                    FlexWrap::NoWrap
                };
            }
            if let Some(grow) = es.flex_grow {
                style.flex_grow = grow;
            }
            if let Some(shrink) = es.flex_shrink {
                style.flex_shrink = shrink;
            }

            // Alignment
            if let Some(align) = es.align_items {
                style.align_items = Some(match align {
                    StyleAlign::Start => AlignItems::START,
                    StyleAlign::Center => AlignItems::CENTER,
                    StyleAlign::End => AlignItems::END,
                    StyleAlign::Stretch => AlignItems::STRETCH,
                    StyleAlign::Baseline => AlignItems::BASELINE,
                });
            }
            if let Some(justify) = es.justify_content {
                style.justify_content = Some(match justify {
                    StyleJustify::Start => JustifyContent::START,
                    StyleJustify::Center => JustifyContent::CENTER,
                    StyleJustify::End => JustifyContent::END,
                    StyleJustify::SpaceBetween => JustifyContent::SPACE_BETWEEN,
                    StyleJustify::SpaceAround => JustifyContent::SPACE_AROUND,
                    StyleJustify::SpaceEvenly => JustifyContent::SPACE_EVENLY,
                });
            }
            if let Some(align) = es.align_self {
                style.align_self = Some(match align {
                    StyleAlign::Start => AlignSelf::START,
                    StyleAlign::Center => AlignSelf::CENTER,
                    StyleAlign::End => AlignSelf::END,
                    StyleAlign::Stretch => AlignSelf::STRETCH,
                    StyleAlign::Baseline => AlignSelf::BASELINE,
                });
                // Written by an author, so it outranks the parent even if
                // `w_fit` had already marked this node's `align_self` as
                // incidental. A widget that is `w_fit` inside still takes
                // an `align-self` rule from CSS.
                self.layout_tree.clear_incidental_align_self(node_id);
            }

            // Spacing
            if let Some(gap) = es.gap {
                style.gap = taffy::Size {
                    width: LengthPercentage::length(gap),
                    height: LengthPercentage::length(gap),
                };
            }
            crate::renderer::stylesheet::apply_extra_layout(es, style);

            // Overflow (shorthand sets both axes)
            if let Some(overflow) = es.overflow {
                let val = match overflow {
                    StyleOverflow::Visible => Overflow::Visible,
                    StyleOverflow::Clip => Overflow::Clip,
                    StyleOverflow::Scroll => Overflow::Scroll,
                };
                style.overflow.x = val;
                style.overflow.y = val;
            }
            // Per-axis overflow overrides
            if let Some(ox) = es.overflow_x {
                style.overflow.x = match ox {
                    StyleOverflow::Visible => Overflow::Visible,
                    StyleOverflow::Clip => Overflow::Clip,
                    StyleOverflow::Scroll => Overflow::Scroll,
                };
            }
            if let Some(oy) = es.overflow_y {
                style.overflow.y = match oy {
                    StyleOverflow::Visible => Overflow::Visible,
                    StyleOverflow::Clip => Overflow::Clip,
                    StyleOverflow::Scroll => Overflow::Scroll,
                };
            }

            // Visibility: hidden collapses element from layout
            if let Some(vis) = es.visibility {
                use crate::element_style::StyleVisibility;
                match vis {
                    StyleVisibility::Hidden => style.display = Display::None,
                    StyleVisibility::Visible => {
                        if style.display == Display::None {
                            style.display = Display::Flex;
                        }
                    }
                }
            }

            // Position
            if let Some(pos) = es.position {
                style.position = match pos {
                    StylePosition::Static | StylePosition::Relative | StylePosition::Sticky => {
                        taffy::Position::Relative
                    }
                    StylePosition::Absolute | StylePosition::Fixed => taffy::Position::Absolute,
                };
            }

            // Inset (top, right, bottom, left)
            // For sticky elements, inset values are scroll-lock thresholds, not layout offsets.
            // They go into RenderProps instead (via apply_element_style_to_props).
            let is_sticky = es.position == Some(StylePosition::Sticky);
            if !is_sticky {
                if let Some(top) = es.top {
                    style.inset.top = LengthPercentageAuto::length(top);
                }
                if let Some(right) = es.right {
                    style.inset.right = LengthPercentageAuto::length(right);
                }
                if let Some(bottom) = es.bottom {
                    style.inset.bottom = LengthPercentageAuto::length(bottom);
                }
                if let Some(left) = es.left {
                    style.inset.left = LengthPercentageAuto::length(left);
                }
            }
        }
        if let Some((node, style)) = pending {
            self.layout_tree.set_style(node, style);
        }
        snapshotted.sort_unstable();
        snapshotted.dedup();
        for node in snapshotted {
            self.refold_node(node);
        }

        // Only the nodes this pass restyled can have newly taken `overflow: scroll`.
        self.auto_create_css_scroll_physics_in(subtree_filter);
    }

    /// Combined application of both CSS visual (base) styles and layout overrides.
    ///
    /// This is an optimized alternative to calling `apply_stylesheet_base_styles()`
    /// followed by `apply_stylesheet_layout_overrides()` separately. It:
    /// - Builds the class-to-nodes inverted index **once** (not twice)
    /// - Sorts and filters complex rules **once** (not twice)
    /// - Iterates complex rules **once**, applying both visual and layout styles
    ///
    /// Use this in the full rebuild path where both operations are needed together.
    pub fn apply_all_stylesheet_styles(&mut self) {
        use crate::element_style::{
            StyleAlign, StyleDisplay, StyleFlexDirection, StyleJustify, StyleOverflow,
            StylePosition,
        };

        let stylesheet = match &self.stylesheet {
            Some(s) => s.clone(),
            None => return,
        };

        // Collect layout overrides to apply after the visual pass
        let mut layout_overrides: Vec<(LayoutNodeId, crate::element_style::ElementStyle)> =
            Vec::new();

        // =====================================================================
        // Complex rules: class, type, and combinator selectors
        // Build inverted class index ONCE for both visual + layout
        // =====================================================================
        let complex_rules = stylesheet.complex_rules();
        if !complex_rules.is_empty() {
            // Each node takes the rules that match it, in ascending specificity: the visual
            // ones at once, the layout ones after.
            let nodes: Vec<LayoutNodeId> = self.render_nodes.keys().copied().collect();
            for node_id in nodes {
                let mut matched = self.matching_base_rules(node_id, &stylesheet);
                matched.sort_by_key(|&i| {
                    (
                        Self::rule_order(&complex_rules[i].0, &complex_rules[i].1),
                        i,
                    )
                });
                for i in matched {
                    let style = &complex_rules[i].1;
                    if let Some(mut render_node) = self.render_nodes.get_mut(&node_id) {
                        Self::apply_element_style_to_props(&mut render_node.props, style);
                    }
                    if style.has_layout_props() {
                        layout_overrides.push((node_id, style.clone()));
                    }
                }
            }

            // NOTE: Do NOT eagerly save base_styles here. This function runs for
            // the entire tree during layout recomputation, at which point hovered
            // nodes may still carry hover styles. Saving contaminated props as
            // "base" causes hover to stick (trailing artifacts, blinking).
            // Eager base_styles saves belong in apply_stylesheet_base_styles() and
            // apply_stylesheet_base_styles_for_subtree() which run when props are clean.
        }

        // =====================================================================
        // Simple ID rules: highest specificity, applied last (overrides class selectors)
        // =====================================================================
        let registered_ids: Vec<(String, LayoutNodeId)> = self
            .element_registry
            .all_ids()
            .into_iter()
            .filter_map(|id| self.element_registry.get(&id).map(|node_id| (id, node_id)))
            .collect();

        for (element_id, node_id) in &registered_ids {
            if let Some(base_style) = stylesheet.get(element_id) {
                // Apply visual styles
                if let Some(mut render_node) = self.render_nodes.get_mut(node_id) {
                    Self::apply_element_style_to_props(&mut render_node.props, base_style);
                }
                // Collect layout overrides
                if base_style.has_layout_props() {
                    layout_overrides.push((*node_id, base_style.clone()));
                }
            }
        }

        // =====================================================================
        // Post-pass: sync text-align into baked TextData
        // =====================================================================
        for render_node in self.render_nodes.values_mut() {
            if let Some(ta) = render_node.props.text_align
                && let ElementType::Text(ref mut text_data) = render_node.element_type
            {
                text_data.align = ta;
            }
        }

        // =====================================================================
        // Update Stateful base_render_props with CSS-applied values
        // =====================================================================
        for (&node_id, render_node) in &self.render_nodes {
            let _tree_scope = self.layout_tree.enter_scope();
            if crate::stateful::has_stateful_base_updater(node_id) {
                crate::stateful::update_stateful_base_props(node_id, render_node.props.clone());
            }
        }

        // =====================================================================
        // Apply base (non-state) SVG tag-name rules to SVG nodes
        // =====================================================================
        let svg_tag_rules = stylesheet.svg_tag_rules();
        if !svg_tag_rules.is_empty() {
            let svg_nodes: Vec<LayoutNodeId> = self
                .render_nodes
                .keys()
                .copied()
                .filter(|&nid| self.element_registry.get_element_type(nid) == Some("svg"))
                .collect();

            for &svg_node in &svg_nodes {
                let mut tag_styles: std::collections::HashMap<String, crate::element::SvgTagStyle> =
                    std::collections::HashMap::new();
                for &(tag_name, ancestor_segments, style) in &svg_tag_rules {
                    let has_state = ancestor_segments.iter().any(|(c, _)| c.has_state());
                    if has_state {
                        continue;
                    }
                    let matches = if ancestor_segments.is_empty() {
                        true
                    } else {
                        let last_idx = ancestor_segments.len() - 1;
                        let (last_compound, _) = &ancestor_segments[last_idx];
                        // Layout-pass style application is state-agnostic
                        // — :hover / :has() inside selectors should NOT
                        // match here. Pass empty sets / None for the
                        // interaction state.
                        let empty: NodeSet = std::collections::HashSet::default();
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
                if !tag_styles.is_empty()
                    && let Some(mut render_node) = self.render_nodes.get_mut(&svg_node)
                {
                    render_node.props.svg_tag_styles = tag_styles;
                }
            }
        }

        // =====================================================================
        // Post-pass: propagate inherited text properties from parent to child
        // =====================================================================
        let all_node_ids: Vec<LayoutNodeId> = self.render_nodes.keys().copied().collect();
        for node_id in all_node_ids {
            let parent_id = match self.element_registry.get_parent(node_id) {
                Some(id) => id,
                None => continue,
            };
            let parent_text_props = self.render_nodes.get(&parent_id).map(|n| {
                (
                    n.props.text_decoration,
                    n.props.text_decoration_color,
                    n.props.text_decoration_thickness,
                    n.props.white_space,
                    n.props.text_overflow,
                    n.props.text_align,
                    n.props.fill,
                    n.props.stroke,
                    n.props.stroke_width,
                    n.props.font_style,
                )
            });
            if let Some((td, td_color, td_thick, ws, to, ta, fill, stroke, stroke_w, fstyle)) =
                parent_text_props
                && let Some(mut node) = self.render_nodes.get_mut(&node_id)
            {
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
                // text_color is NOT copied here either; see
                // `propagate_inherited_text`.
                if node.props.text_align.is_none()
                    && let Some(ta) = ta
                {
                    node.props.text_align = Some(ta);
                    if let ElementType::Text(ref mut text_data) = node.element_type {
                        text_data.align = ta;
                    }
                }
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

        // =====================================================================
        // Apply collected layout overrides to taffy styles
        // =====================================================================
        for (node_id, es) in layout_overrides {
            let Some(mut style) = self.layout_tree.get_style(node_id) else {
                continue;
            };

            // Sizing
            if let Some(w) = es.width {
                style.size.width = match w {
                    crate::element_style::StyleDimension::Length(px) => Dimension::length(px),
                    crate::element_style::StyleDimension::Percent(p) => Dimension::percent(p),
                    crate::element_style::StyleDimension::Auto => Dimension::auto(),
                };
                if matches!(w, crate::element_style::StyleDimension::Auto) {
                    style.flex_basis = Dimension::auto();
                    style.flex_grow = 0.0;
                    style.flex_shrink = 0.0;
                }
            }
            if let Some(h) = es.height {
                style.size.height = match h {
                    crate::element_style::StyleDimension::Length(px) => Dimension::length(px),
                    crate::element_style::StyleDimension::Percent(p) => Dimension::percent(p),
                    crate::element_style::StyleDimension::Auto => Dimension::auto(),
                };
                if matches!(h, crate::element_style::StyleDimension::Auto) {
                    style.flex_basis = Dimension::auto();
                    style.flex_grow = 0.0;
                    style.flex_shrink = 0.0;
                }
            }

            // Display & flex direction
            if let Some(display) = es.display {
                style.display = match display {
                    StyleDisplay::Flex => Display::Flex,
                    StyleDisplay::Block => Display::Block,
                    StyleDisplay::Grid => Display::Grid,
                    StyleDisplay::None => Display::None,
                };
            }
            if let Some(dir) = es.flex_direction {
                style.flex_direction = match dir {
                    StyleFlexDirection::Row => FlexDirection::Row,
                    StyleFlexDirection::Column => FlexDirection::Column,
                    StyleFlexDirection::RowReverse => FlexDirection::RowReverse,
                    StyleFlexDirection::ColumnReverse => FlexDirection::ColumnReverse,
                };
            }
            if let Some(wrap) = es.flex_wrap {
                style.flex_wrap = if wrap {
                    FlexWrap::Wrap
                } else {
                    FlexWrap::NoWrap
                };
            }
            if let Some(grow) = es.flex_grow {
                style.flex_grow = grow;
            }
            if let Some(shrink) = es.flex_shrink {
                style.flex_shrink = shrink;
            }

            // Alignment
            if let Some(align) = es.align_items {
                style.align_items = Some(match align {
                    StyleAlign::Start => AlignItems::START,
                    StyleAlign::Center => AlignItems::CENTER,
                    StyleAlign::End => AlignItems::END,
                    StyleAlign::Stretch => AlignItems::STRETCH,
                    StyleAlign::Baseline => AlignItems::BASELINE,
                });
            }
            if let Some(justify) = es.justify_content {
                style.justify_content = Some(match justify {
                    StyleJustify::Start => JustifyContent::START,
                    StyleJustify::Center => JustifyContent::CENTER,
                    StyleJustify::End => JustifyContent::END,
                    StyleJustify::SpaceBetween => JustifyContent::SPACE_BETWEEN,
                    StyleJustify::SpaceAround => JustifyContent::SPACE_AROUND,
                    StyleJustify::SpaceEvenly => JustifyContent::SPACE_EVENLY,
                });
            }
            if let Some(align) = es.align_self {
                style.align_self = Some(match align {
                    StyleAlign::Start => AlignSelf::START,
                    StyleAlign::Center => AlignSelf::CENTER,
                    StyleAlign::End => AlignSelf::END,
                    StyleAlign::Stretch => AlignSelf::STRETCH,
                    StyleAlign::Baseline => AlignSelf::BASELINE,
                });
                // Written by an author, so it outranks the parent even if
                // `w_fit` had already marked this node's `align_self` as
                // incidental. A widget that is `w_fit` inside still takes
                // an `align-self` rule from CSS.
                self.layout_tree.clear_incidental_align_self(node_id);
            }

            // Spacing
            if let Some(gap) = es.gap {
                style.gap = taffy::Size {
                    width: LengthPercentage::length(gap),
                    height: LengthPercentage::length(gap),
                };
            }
            crate::renderer::stylesheet::apply_extra_layout(&es, &mut style);

            // Overflow
            if let Some(overflow) = es.overflow {
                let val = match overflow {
                    StyleOverflow::Visible => Overflow::Visible,
                    StyleOverflow::Clip => Overflow::Clip,
                    StyleOverflow::Scroll => Overflow::Scroll,
                };
                style.overflow.x = val;
                style.overflow.y = val;
            }
            if let Some(ox) = es.overflow_x {
                style.overflow.x = match ox {
                    StyleOverflow::Visible => Overflow::Visible,
                    StyleOverflow::Clip => Overflow::Clip,
                    StyleOverflow::Scroll => Overflow::Scroll,
                };
            }
            if let Some(oy) = es.overflow_y {
                style.overflow.y = match oy {
                    StyleOverflow::Visible => Overflow::Visible,
                    StyleOverflow::Clip => Overflow::Clip,
                    StyleOverflow::Scroll => Overflow::Scroll,
                };
            }

            // Visibility
            if let Some(vis) = es.visibility {
                use crate::element_style::StyleVisibility;
                match vis {
                    StyleVisibility::Hidden => style.display = Display::None,
                    StyleVisibility::Visible => {
                        if style.display == Display::None {
                            style.display = Display::Flex;
                        }
                    }
                }
            }

            // Position
            if let Some(pos) = es.position {
                style.position = match pos {
                    StylePosition::Static | StylePosition::Relative | StylePosition::Sticky => {
                        taffy::Position::Relative
                    }
                    StylePosition::Absolute | StylePosition::Fixed => taffy::Position::Absolute,
                };
            }

            // Inset
            let is_sticky = es.position == Some(StylePosition::Sticky);
            if !is_sticky {
                if let Some(top) = es.top {
                    style.inset.top = LengthPercentageAuto::length(top);
                }
                if let Some(right) = es.right {
                    style.inset.right = LengthPercentageAuto::length(right);
                }
                if let Some(bottom) = es.bottom {
                    style.inset.bottom = LengthPercentageAuto::length(bottom);
                }
                if let Some(left) = es.left {
                    style.inset.left = LengthPercentageAuto::length(left);
                }
            }

            self.layout_tree.set_style(node_id, style);
        }

        // Auto-create scroll physics for any nodes with overflow: scroll
        self.auto_create_css_scroll_physics();

        // Classes that follow a signal take the rules they match now.
        self.sync_dynamic_classes(None);
    }
}
