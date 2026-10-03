//! Text-property inheritance + `TextData` materialisation.
//!
//! Two `pub(crate)` helpers on `RenderTree`:
//!
//! - `inherit_text_props_from_parent` — fills any `None` text-related
//!   field on `props` (text-decoration, white-space, text-overflow,
//!   color, text-align, plus SVG fill/stroke) with the parent
//!   `RenderNode`'s value. Mirrors CSS inheritance for the subset of
//!   properties text rendering actually needs.
//! - `build_text_data` — folds CSS overrides from `RenderProps`
//!   (text-decoration → underline/strikethrough flags, white-space →
//!   wrap flag, text-align) into a `TextData` ready for the layout
//!   tree. Called by every `collect_render_props*` path that lands on
//!   a text node.

use crate::element::RenderProps;
use crate::tree::LayoutNodeId;

use super::super::{RenderTree, TextData};

impl RenderTree {
    /// Inherit CSS text properties from the parent RenderNode.
    ///
    /// CSS text properties like text-decoration, white-space, text-overflow, etc.
    /// need to cascade from parent divs to child text elements. Without this,
    /// CSS like `.my-class { text-decoration: underline; }` on a parent div
    /// wouldn't affect the child text node.
    pub(crate) fn inherit_text_props_from_parent(
        &self,
        props: &mut RenderProps,
        node_id: LayoutNodeId,
    ) {
        let parent_id = match self.element_registry.get_parent(node_id) {
            Some(id) => id,
            None => return,
        };
        let parent_props = match self.render_nodes.get(&parent_id) {
            Some(node) => &node.props,
            None => return,
        };

        // text-decoration (CSS spec: not inherited, but decorations paint across inline content)
        if props.text_decoration.is_none() {
            if let Some(td) = parent_props.text_decoration {
                props.text_decoration = Some(td);
            }
        }
        if props.text_decoration_color.is_none() {
            if let Some(c) = parent_props.text_decoration_color {
                props.text_decoration_color = Some(c);
            }
        }
        if props.text_decoration_thickness.is_none() {
            if let Some(t) = parent_props.text_decoration_thickness {
                props.text_decoration_thickness = Some(t);
            }
        }
        // white-space (CSS spec: inherited)
        if props.white_space.is_none() {
            if let Some(ws) = parent_props.white_space {
                props.white_space = Some(ws);
            }
        }
        // text-overflow (CSS spec: not inherited, but child text must know)
        if props.text_overflow.is_none() {
            if let Some(to) = parent_props.text_overflow {
                props.text_overflow = Some(to);
            }
        }
        // color (CSS spec: inherited) is NOT copied: the paint walk
        // resolves it as it descends, so `props.text_color` stays what this
        // node set for itself and an ancestor's later change is picked up.
        // text-align (CSS spec: inherited)
        if props.text_align.is_none() {
            if let Some(ta) = parent_props.text_align {
                props.text_align = Some(ta);
            }
        }
        // SVG fill (CSS spec: inherited in SVG)
        if props.fill.is_none() {
            if let Some(f) = parent_props.fill {
                props.fill = Some(f);
            }
        }
        // SVG stroke (CSS spec: inherited in SVG)
        if props.stroke.is_none() {
            if let Some(s) = parent_props.stroke {
                props.stroke = Some(s);
            }
        }
        // SVG stroke-width (CSS spec: inherited in SVG)
        if props.stroke_width.is_none() {
            if let Some(sw) = parent_props.stroke_width {
                props.stroke_width = Some(sw);
            }
        }
    }

    /// Build TextData from TextRenderInfo, applying CSS overrides from RenderProps
    pub(crate) fn build_text_data(
        info: crate::div::TextRenderInfo,
        props: &RenderProps,
    ) -> TextData {
        let mut strikethrough = info.strikethrough;
        let mut underline = info.underline;
        let mut wrap = info.wrap;
        // CSS text-decoration overrides builder values
        if let Some(td) = props.text_decoration {
            use crate::element_style::TextDecoration;
            match td {
                TextDecoration::Underline => underline = true,
                TextDecoration::LineThrough => strikethrough = true,
                TextDecoration::None => {
                    underline = false;
                    strikethrough = false;
                }
            }
        }
        // CSS white-space overrides wrap
        if let Some(ws) = props.white_space {
            use crate::element_style::WhiteSpace;
            match ws {
                WhiteSpace::Nowrap | WhiteSpace::Pre => wrap = false,
                WhiteSpace::Normal | WhiteSpace::PreWrap => wrap = true,
            }
        }
        // CSS text-align overrides builder value
        let align = props.text_align.unwrap_or(info.align);
        TextData {
            content: info.content,
            font_size: info.font_size,
            color: info.color,
            align,
            weight: info.weight,
            italic: info.italic,
            v_align: info.v_align,
            wrap,
            line_height: info.line_height,
            measured_width: info.measured_width,
            font_family: info.font_family,
            word_spacing: info.word_spacing,
            letter_spacing: info.letter_spacing,
            ascender: info.ascender,
            strikethrough,
            underline,
        }
    }
}
