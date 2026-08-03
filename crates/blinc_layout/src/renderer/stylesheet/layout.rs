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
//! - `auto_create_css_scroll_physics` — scans every node and creates
//!   `ScrollPhysics` + handler-registry entries for nodes whose
//!   `overflow` is `scroll` but that don't already have a `scroll()`
//!   widget or `overflow_scroll()` builder behind them.

use std::sync::{Arc, Mutex};

use taffy::Overflow;
use taffy::prelude::*;

use crate::tree::LayoutNodeId;

use super::super::{ElementType, RenderTree};

impl RenderTree {
    /// Apply layout properties from the stylesheet to taffy nodes.
    ///
    /// This must be called after tree build and before compute_layout().
    /// It iterates all registered element IDs and applies layout overrides
    /// (width, height, padding, margin, gap, flex-direction, alignment, etc.)
    /// from the stylesheet to the corresponding taffy nodes.
    pub fn apply_stylesheet_layout_overrides(&mut self) {
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
        let subtree_set: std::collections::HashSet<LayoutNodeId> =
            subtree_nodes.into_iter().collect();
        self.apply_stylesheet_layout_overrides_filtered(Some(&subtree_set));
    }

    fn apply_stylesheet_layout_overrides_filtered(
        &mut self,
        subtree_filter: Option<&std::collections::HashSet<LayoutNodeId>>,
    ) {
        use crate::element_style::{
            SpacingRect, StyleAlign, StyleDisplay, StyleFlexDirection, StyleJustify, StyleOverflow,
            StylePosition,
        };

        let stylesheet = match &self.stylesheet {
            Some(s) => s.clone(),
            None => return,
        };

        // Collect (node_id, style) pairs that have layout overrides.
        // Apply complex rules (class/type selectors) FIRST — lower specificity.
        // Then apply simple ID rules LAST — higher specificity overrides.
        let mut overrides: Vec<(LayoutNodeId, crate::element_style::ElementStyle)> = Vec::new();

        // Complex rules: class, type, and combinator selectors
        let complex_rules = stylesheet.complex_rules();
        if !complex_rules.is_empty() {
            let empty_set = std::collections::HashSet::new();

            let mut base_rules: Vec<&(
                crate::css_parser::ComplexSelector,
                crate::element_style::ElementStyle,
            )> = complex_rules
                .iter()
                .filter(|(selector, _)| !selector.has_state())
                .collect();
            base_rules.sort_by_key(|(selector, _)| Self::selector_specificity(selector));

            // Build inverted class index once for O(1) lookups
            let class_to_nodes = self.element_registry.class_to_nodes_index();

            for (selector, style) in base_rules {
                if !style.has_layout_props() {
                    continue;
                }

                // Fast path: simple `.class` selectors use inverted index
                if let Some(class_name) = selector.simple_class_name() {
                    if let Some(node_ids) = class_to_nodes.get(class_name) {
                        for &node_id in node_ids {
                            if subtree_filter.is_none_or(|s| s.contains(&node_id)) {
                                overrides.push((node_id, style.clone()));
                            }
                        }
                    }
                    continue;
                }

                // Slow path: complex selectors need full matching
                let all_node_ids: Vec<LayoutNodeId> = self.render_nodes.keys().copied().collect();
                for &node_id in &all_node_ids {
                    if subtree_filter.is_none_or(|s| s.contains(&node_id))
                        && self.complex_selector_matches(
                            selector, node_id, &empty_set, &empty_set, None,
                        )
                    {
                        overrides.push((node_id, style.clone()));
                    }
                }
            }
        }

        // Simple ID rules: highest specificity, applied last
        let all_ids = self.element_registry.all_ids();
        for id in &all_ids {
            if let Some(node_id) = self.element_registry.get(id) {
                if let Some(style) = stylesheet.get(id) {
                    if style.has_layout_props()
                        && subtree_filter.is_none_or(|s| s.contains(&node_id))
                    {
                        overrides.push((node_id, style.clone()));
                    }
                }
            }
        }

        for (node_id, es) in overrides {
            let Some(mut style) = self.layout_tree.get_style(node_id) else {
                continue;
            };

            // Sizing
            if let Some(w) = es.width {
                style.size.width = match w {
                    crate::element_style::StyleDimension::Length(px) => Dimension::Length(px),
                    crate::element_style::StyleDimension::Percent(p) => Dimension::Percent(p),
                    crate::element_style::StyleDimension::Auto => Dimension::Auto,
                };
                if matches!(w, crate::element_style::StyleDimension::Auto) {
                    style.flex_basis = Dimension::Auto;
                    style.flex_grow = 0.0;
                    style.flex_shrink = 0.0;
                }
            }
            if let Some(h) = es.height {
                style.size.height = match h {
                    crate::element_style::StyleDimension::Length(px) => Dimension::Length(px),
                    crate::element_style::StyleDimension::Percent(p) => Dimension::Percent(p),
                    crate::element_style::StyleDimension::Auto => Dimension::Auto,
                };
                if matches!(h, crate::element_style::StyleDimension::Auto) {
                    style.flex_basis = Dimension::Auto;
                    style.flex_grow = 0.0;
                    style.flex_shrink = 0.0;
                }
            }
            if let Some(w) = es.min_width {
                style.min_size.width = Dimension::Length(w);
            }
            if let Some(h) = es.min_height {
                style.min_size.height = Dimension::Length(h);
            }
            if let Some(w) = es.max_width {
                style.max_size.width = Dimension::Length(w);
            }
            if let Some(h) = es.max_height {
                style.max_size.height = Dimension::Length(h);
            }

            // Display & flex direction
            if let Some(display) = es.display {
                style.display = match display {
                    StyleDisplay::Flex => Display::Flex,
                    StyleDisplay::Block => Display::Block,
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
                    StyleAlign::Start => AlignItems::Start,
                    StyleAlign::Center => AlignItems::Center,
                    StyleAlign::End => AlignItems::End,
                    StyleAlign::Stretch => AlignItems::Stretch,
                    StyleAlign::Baseline => AlignItems::Baseline,
                });
            }
            if let Some(justify) = es.justify_content {
                style.justify_content = Some(match justify {
                    StyleJustify::Start => JustifyContent::Start,
                    StyleJustify::Center => JustifyContent::Center,
                    StyleJustify::End => JustifyContent::End,
                    StyleJustify::SpaceBetween => JustifyContent::SpaceBetween,
                    StyleJustify::SpaceAround => JustifyContent::SpaceAround,
                    StyleJustify::SpaceEvenly => JustifyContent::SpaceEvenly,
                });
            }
            if let Some(align) = es.align_self {
                style.align_self = Some(match align {
                    StyleAlign::Start => AlignSelf::Start,
                    StyleAlign::Center => AlignSelf::Center,
                    StyleAlign::End => AlignSelf::End,
                    StyleAlign::Stretch => AlignSelf::Stretch,
                    StyleAlign::Baseline => AlignSelf::Baseline,
                });
            }

            // Spacing
            if let Some(SpacingRect {
                top,
                right,
                bottom,
                left,
            }) = es.padding
            {
                style.padding.top = LengthPercentage::Length(top);
                style.padding.right = LengthPercentage::Length(right);
                style.padding.bottom = LengthPercentage::Length(bottom);
                style.padding.left = LengthPercentage::Length(left);
            }
            if let Some(SpacingRect {
                top,
                right,
                bottom,
                left,
            }) = es.margin
            {
                style.margin.top = LengthPercentageAuto::Length(top);
                style.margin.right = LengthPercentageAuto::Length(right);
                style.margin.bottom = LengthPercentageAuto::Length(bottom);
                style.margin.left = LengthPercentageAuto::Length(left);
            }
            if let Some(gap) = es.gap {
                style.gap = taffy::Size {
                    width: LengthPercentage::Length(gap),
                    height: LengthPercentage::Length(gap),
                };
            }

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
                    style.inset.top = LengthPercentageAuto::Length(top);
                }
                if let Some(right) = es.right {
                    style.inset.right = LengthPercentageAuto::Length(right);
                }
                if let Some(bottom) = es.bottom {
                    style.inset.bottom = LengthPercentageAuto::Length(bottom);
                }
                if let Some(left) = es.left {
                    style.inset.left = LengthPercentageAuto::Length(left);
                }
            }

            self.layout_tree.set_style(node_id, style);
        }

        // Auto-create scroll physics for any nodes with overflow: scroll
        self.auto_create_css_scroll_physics();
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
            SpacingRect, StyleAlign, StyleDisplay, StyleFlexDirection, StyleJustify, StyleOverflow,
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
            let empty_set = std::collections::HashSet::new();

            // Collect non-state rules and sort by specificity (ascending)
            let mut base_rules: Vec<&(
                crate::css_parser::ComplexSelector,
                crate::element_style::ElementStyle,
            )> = complex_rules
                .iter()
                .filter(|(selector, _)| !selector.has_state())
                .collect();
            base_rules.sort_by_key(|(selector, _)| Self::selector_specificity(selector));

            // Build inverted class index ONCE (single lock acquisition)
            let class_to_nodes = self.element_registry.class_to_nodes_index();

            for (selector, style) in base_rules {
                let has_layout = style.has_layout_props();

                // Fast path: simple `.class` selectors use inverted index
                if let Some(class_name) = selector.simple_class_name() {
                    if let Some(node_ids) = class_to_nodes.get(class_name) {
                        for &node_id in node_ids {
                            // Apply visual styles
                            if let Some(render_node) = self.render_nodes.get_mut(&node_id) {
                                Self::apply_element_style_to_props(&mut render_node.props, style);
                            }
                            // Collect layout overrides
                            if has_layout {
                                layout_overrides.push((node_id, style.clone()));
                            }
                        }
                    }
                    continue;
                }

                // Slow path: complex selectors (combinators, structural pseudos)
                let all_node_ids: Vec<LayoutNodeId> = self.render_nodes.keys().copied().collect();
                for &node_id in &all_node_ids {
                    if self
                        .complex_selector_matches(selector, node_id, &empty_set, &empty_set, None)
                    {
                        // Apply visual styles
                        if let Some(render_node) = self.render_nodes.get_mut(&node_id) {
                            Self::apply_element_style_to_props(&mut render_node.props, style);
                        }
                        // Collect layout overrides
                        if has_layout {
                            layout_overrides.push((node_id, style.clone()));
                        }
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
                if let Some(render_node) = self.render_nodes.get_mut(node_id) {
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
            if let Some(ta) = render_node.props.text_align {
                if let ElementType::Text(ref mut text_data) = render_node.element_type {
                    text_data.align = ta;
                }
            }
        }

        // =====================================================================
        // Update Stateful base_render_props with CSS-applied values
        // =====================================================================
        for (&node_id, render_node) in &self.render_nodes {
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
                            if let ElementType::Text(ref mut text_data) = node.element_type {
                                text_data.align = ta;
                            }
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
                    crate::element_style::StyleDimension::Length(px) => Dimension::Length(px),
                    crate::element_style::StyleDimension::Percent(p) => Dimension::Percent(p),
                    crate::element_style::StyleDimension::Auto => Dimension::Auto,
                };
                if matches!(w, crate::element_style::StyleDimension::Auto) {
                    style.flex_basis = Dimension::Auto;
                    style.flex_grow = 0.0;
                    style.flex_shrink = 0.0;
                }
            }
            if let Some(h) = es.height {
                style.size.height = match h {
                    crate::element_style::StyleDimension::Length(px) => Dimension::Length(px),
                    crate::element_style::StyleDimension::Percent(p) => Dimension::Percent(p),
                    crate::element_style::StyleDimension::Auto => Dimension::Auto,
                };
                if matches!(h, crate::element_style::StyleDimension::Auto) {
                    style.flex_basis = Dimension::Auto;
                    style.flex_grow = 0.0;
                    style.flex_shrink = 0.0;
                }
            }
            if let Some(w) = es.min_width {
                style.min_size.width = Dimension::Length(w);
            }
            if let Some(h) = es.min_height {
                style.min_size.height = Dimension::Length(h);
            }
            if let Some(w) = es.max_width {
                style.max_size.width = Dimension::Length(w);
            }
            if let Some(h) = es.max_height {
                style.max_size.height = Dimension::Length(h);
            }

            // Display & flex direction
            if let Some(display) = es.display {
                style.display = match display {
                    StyleDisplay::Flex => Display::Flex,
                    StyleDisplay::Block => Display::Block,
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
                    StyleAlign::Start => AlignItems::Start,
                    StyleAlign::Center => AlignItems::Center,
                    StyleAlign::End => AlignItems::End,
                    StyleAlign::Stretch => AlignItems::Stretch,
                    StyleAlign::Baseline => AlignItems::Baseline,
                });
            }
            if let Some(justify) = es.justify_content {
                style.justify_content = Some(match justify {
                    StyleJustify::Start => JustifyContent::Start,
                    StyleJustify::Center => JustifyContent::Center,
                    StyleJustify::End => JustifyContent::End,
                    StyleJustify::SpaceBetween => JustifyContent::SpaceBetween,
                    StyleJustify::SpaceAround => JustifyContent::SpaceAround,
                    StyleJustify::SpaceEvenly => JustifyContent::SpaceEvenly,
                });
            }
            if let Some(align) = es.align_self {
                style.align_self = Some(match align {
                    StyleAlign::Start => AlignSelf::Start,
                    StyleAlign::Center => AlignSelf::Center,
                    StyleAlign::End => AlignSelf::End,
                    StyleAlign::Stretch => AlignSelf::Stretch,
                    StyleAlign::Baseline => AlignSelf::Baseline,
                });
            }

            // Spacing
            if let Some(SpacingRect {
                top,
                right,
                bottom,
                left,
            }) = es.padding
            {
                style.padding.top = LengthPercentage::Length(top);
                style.padding.right = LengthPercentage::Length(right);
                style.padding.bottom = LengthPercentage::Length(bottom);
                style.padding.left = LengthPercentage::Length(left);
            }
            if let Some(SpacingRect {
                top,
                right,
                bottom,
                left,
            }) = es.margin
            {
                style.margin.top = LengthPercentageAuto::Length(top);
                style.margin.right = LengthPercentageAuto::Length(right);
                style.margin.bottom = LengthPercentageAuto::Length(bottom);
                style.margin.left = LengthPercentageAuto::Length(left);
            }
            if let Some(gap) = es.gap {
                style.gap = taffy::Size {
                    width: LengthPercentage::Length(gap),
                    height: LengthPercentage::Length(gap),
                };
            }

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
                    style.inset.top = LengthPercentageAuto::Length(top);
                }
                if let Some(right) = es.right {
                    style.inset.right = LengthPercentageAuto::Length(right);
                }
                if let Some(bottom) = es.bottom {
                    style.inset.bottom = LengthPercentageAuto::Length(bottom);
                }
                if let Some(left) = es.left {
                    style.inset.left = LengthPercentageAuto::Length(left);
                }
            }

            self.layout_tree.set_style(node_id, style);
        }

        // Auto-create scroll physics for any nodes with overflow: scroll
        self.auto_create_css_scroll_physics();
    }

    /// Auto-create scroll physics for nodes with `overflow: scroll` set via CSS.
    ///
    /// Scans all layout nodes and creates scroll physics + event handlers
    /// for any node that has `overflow: scroll` on either axis but doesn't
    /// already have scroll physics registered (e.g., from the `scroll()` widget
    /// or the `overflow_scroll()` Div builder).
    pub fn auto_create_css_scroll_physics(&mut self) {
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
        let mut stack: Vec<LayoutNodeId> = self.root().into_iter().collect();

        while let Some(node_id) = stack.pop() {
            stack.extend(self.layout_tree.children(node_id));
            // Skip if already has scroll physics (from Scroll widget or Div builder)
            if self.scroll_physics.contains_key(&node_id) {
                continue;
            }
            let Some(style) = self.layout_tree.get_style(node_id) else {
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
