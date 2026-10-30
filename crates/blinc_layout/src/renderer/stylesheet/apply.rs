//! `ElementStyle` → `RenderProps` / `taffy::Style` translation,
//! plus `KeyframeProperties` snapshots and `clip-path` resolution.
//!
//! Three concerns:
//!
//! - **Style application**: `apply_element_style_to_props` and
//!   `apply_element_style_to_taffy` copy the matching half of an
//!   `ElementStyle` into the per-node `RenderProps` (visual) / taffy
//!   `Style` (layout). Called from the base-styles pass, the state
//!   styles pass, and the complex-selector pass — anywhere a
//!   `(node, style)` pair needs to be projected onto live state.
//! - **Snapshots**: `snapshot_keyframe_properties` and
//!   `snapshot_before_keyframe_properties` capture a node's full
//!   visual + layout state into a `KeyframeProperties` for transition
//!   detection. The "before" variant overlays values from any active
//!   transition so the QR-decompose roundtrip of the transform matrix
//!   doesn't introduce spurious diffs.
//! - **Clip resolution**: `resolve_clip_path` turns a CSS `ClipPath`
//!   (circle / ellipse / inset / rect / xywh / polygon / path) into a
//!   concrete `ClipShape` given element bounds. `clip_length_to_percent`
//!   is a small helper shared with the CSS animation interpolator.

use blinc_core::{BlurQuality, ClipShape, LayerEffect, Point, Rect, Vec2};

use crate::element::RenderProps;
use crate::style_tier::{StyleProp, StyleTier};
use crate::tree::LayoutNodeId;

use super::super::RenderTree;

impl RenderTree {
    /// Apply ElementStyle properties to RenderProps
    pub(crate) fn apply_element_style_to_props(
        props: &mut RenderProps,
        style: &crate::element_style::ElementStyle,
    ) {
        if let Some(ref bg) = style.background {
            props
                .tiers
                .mark(StyleProp::Background, StyleTier::Stylesheet);
            props.background = Some(bg.clone());
        }
        if let Some(ref cr) = style.corner_radius {
            props
                .tiers
                .mark(StyleProp::BorderRadius, StyleTier::Stylesheet);
            props.border_radius = *cr;
            props.border_radius_explicit = true;
        }
        if let Some(cs) = style.corner_shape {
            props
                .tiers
                .mark(StyleProp::CornerShape, StyleTier::Stylesheet);
            props.corner_shape = cs;
        }
        if let Some(fade) = style.overflow_fade {
            props
                .tiers
                .mark(StyleProp::OverflowFade, StyleTier::Stylesheet);
            props.overflow_fade = fade;
        }
        if !style.inner_shadow.is_empty() {
            props
                .tiers
                .mark(StyleProp::InnerShadow, StyleTier::Stylesheet);
            props.inner_shadow = style.inner_shadow.clone();
        }
        if !style.shadow.is_empty() {
            props.tiers.mark(StyleProp::Shadow, StyleTier::Stylesheet);
            props.shadow = style.shadow.clone();
        }
        if let Some(ref transform) = style.transform {
            props
                .tiers
                .mark(StyleProp::Transform, StyleTier::Stylesheet);
            props.transform = Some(transform.clone());
        }
        if let Some(opacity) = style.opacity {
            props.tiers.mark(StyleProp::Opacity, StyleTier::Stylesheet);
            props.opacity = opacity;
        }
        if let Some(ref render_layer) = style.render_layer {
            props.tiers.mark(StyleProp::Layer, StyleTier::Stylesheet);
            props.layer = *render_layer;
        }
        if let Some(ref material) = style.material {
            props.tiers.mark(StyleProp::Material, StyleTier::Stylesheet);
            props.material = Some(material.clone());
        }
        // 3D transform properties
        if let Some(rx) = style.rotate_x {
            props.tiers.mark(StyleProp::Rotate3d, StyleTier::Stylesheet);
            props.rotate_x = Some(rx);
        }
        if let Some(ry) = style.rotate_y {
            props.tiers.mark(StyleProp::Rotate3d, StyleTier::Stylesheet);
            props.rotate_y = Some(ry);
        }
        if let Some(p) = style.perspective {
            props
                .tiers
                .mark(StyleProp::Perspective, StyleTier::Stylesheet);
            props.perspective = Some(p);
        }
        if let Some(ref s) = style.shape_3d {
            props.tiers.mark(StyleProp::Shape3d, StyleTier::Stylesheet);
            props.shape_3d = Some(crate::css_parser::shape_3d_to_float(s));
        }
        if let Some(d) = style.depth {
            props.tiers.mark(StyleProp::Depth, StyleTier::Stylesheet);
            props.depth = Some(d);
        }
        if let Some(dir) = style.light_direction {
            props.tiers.mark(StyleProp::Light3d, StyleTier::Stylesheet);
            props.light_direction = Some(dir);
        }
        if let Some(v) = style.light_intensity {
            props.tiers.mark(StyleProp::Light3d, StyleTier::Stylesheet);
            props.light_intensity = Some(v);
        }
        if let Some(v) = style.ambient {
            props.tiers.mark(StyleProp::Light3d, StyleTier::Stylesheet);
            props.ambient = Some(v);
        }
        if let Some(v) = style.specular {
            props.tiers.mark(StyleProp::Light3d, StyleTier::Stylesheet);
            props.specular = Some(v);
        }
        if let Some(v) = style.translate_z {
            props
                .tiers
                .mark(StyleProp::Translate3d, StyleTier::Stylesheet);
            props.translate_z = Some(v);
        }
        if let Some(ref op) = style.op_3d {
            props.tiers.mark(StyleProp::Op3d, StyleTier::Stylesheet);
            props.op_3d = Some(crate::css_parser::op_3d_to_float(op));
        }
        if let Some(v) = style.blend_3d {
            props.tiers.mark(StyleProp::Blend3d, StyleTier::Stylesheet);
            props.blend_3d = Some(v);
        }
        if let Some(ref cp) = style.clip_path {
            props.tiers.mark(StyleProp::ClipPath, StyleTier::Stylesheet);
            props.clip_path = Some(cp.clone());
        }
        if let Some(filter) = style.filter {
            props.tiers.mark(StyleProp::Filter, StyleTier::Stylesheet);
            props.filter = Some(filter);
            // Convert blur filter to LayerEffect for GPU processing
            if filter.blur > 0.0 {
                // Remove any existing blur effect before adding updated one
                props
                    .layer_effects
                    .retain(|e| !matches!(e, LayerEffect::Blur { .. }));
                props.layer_effects.push(LayerEffect::Blur {
                    radius: filter.blur,
                    quality: BlurQuality::Medium,
                });
            } else {
                props
                    .layer_effects
                    .retain(|e| !matches!(e, LayerEffect::Blur { .. }));
            }
            // Convert drop-shadow filter to LayerEffect
            if let Some(ds) = &filter.drop_shadow {
                props
                    .layer_effects
                    .retain(|e| !matches!(e, LayerEffect::DropShadow { .. }));
                props.layer_effects.push(LayerEffect::DropShadow {
                    offset_x: ds.offset_x,
                    offset_y: ds.offset_y,
                    blur: ds.blur,
                    spread: ds.spread,
                    color: ds.color,
                });
            } else {
                props
                    .layer_effects
                    .retain(|e| !matches!(e, LayerEffect::DropShadow { .. }));
            }
        }
        // Mask image → LayerEffect (URL only; gradients handled per-primitive)
        if let Some(blinc_core::MaskImage::Url(ref url)) = style.mask_image {
            props
                .tiers
                .mark(StyleProp::MaskImage, StyleTier::Stylesheet);
            props
                .layer_effects
                .retain(|e| !matches!(e, LayerEffect::MaskImage { .. }));
            props.layer_effects.push(LayerEffect::MaskImage {
                image_url: url.clone(),
                mask_mode: style.mask_mode.clone().unwrap_or_default(),
            });
        }
        // Text color
        if let Some(c) = &style.text_color {
            props
                .tiers
                .mark(StyleProp::TextColor, StyleTier::Stylesheet);
            props.text_color = Some([c.r, c.g, c.b, c.a]);
        }
        // Text shadow
        if let Some(ts) = &style.text_shadow {
            props
                .tiers
                .mark(StyleProp::TextShadow, StyleTier::Stylesheet);
            props.text_shadow = Some(*ts);
        }
        // Font size
        if let Some(fs) = style.font_size {
            props.tiers.mark(StyleProp::FontSize, StyleTier::Stylesheet);
            props.font_size = Some(fs);
        }
        // Font weight
        if let Some(fw) = style.font_weight {
            props
                .tiers
                .mark(StyleProp::FontWeight, StyleTier::Stylesheet);
            props.font_weight = Some(fw);
        }
        // Font style
        if let Some(fs) = style.font_style {
            props
                .tiers
                .mark(StyleProp::FontStyle, StyleTier::Stylesheet);
            props.font_style = Some(fs);
        }
        // Text decoration
        if let Some(td) = style.text_decoration {
            props
                .tiers
                .mark(StyleProp::TextDecoration, StyleTier::Stylesheet);
            props.text_decoration = Some(td);
        }
        // Line height
        if let Some(lh) = style.line_height {
            props
                .tiers
                .mark(StyleProp::LineHeight, StyleTier::Stylesheet);
            props.line_height = Some(lh);
        }
        // Text align
        if let Some(ta) = style.text_align {
            props
                .tiers
                .mark(StyleProp::TextAlign, StyleTier::Stylesheet);
            props.text_align = Some(ta);
        }
        // Letter spacing
        if let Some(ls) = style.letter_spacing {
            props
                .tiers
                .mark(StyleProp::LetterSpacing, StyleTier::Stylesheet);
            props.letter_spacing = Some(ls);
        }
        // SVG properties
        if let Some(fill) = style.fill {
            props.tiers.mark(StyleProp::Fill, StyleTier::Stylesheet);
            props.fill = Some([fill.r, fill.g, fill.b, fill.a]);
        }
        if let Some(stroke) = style.stroke {
            props.tiers.mark(StyleProp::Stroke, StyleTier::Stylesheet);
            props.stroke = Some([stroke.r, stroke.g, stroke.b, stroke.a]);
        }
        if let Some(sw) = style.stroke_width {
            props
                .tiers
                .mark(StyleProp::StrokeWidth, StyleTier::Stylesheet);
            props.stroke_width = Some(sw);
        }
        if let Some(ref da) = style.stroke_dasharray {
            props
                .tiers
                .mark(StyleProp::StrokeDasharray, StyleTier::Stylesheet);
            props.stroke_dasharray = Some(da.clone());
        }
        if let Some(offset) = style.stroke_dashoffset {
            props
                .tiers
                .mark(StyleProp::StrokeDashoffset, StyleTier::Stylesheet);
            props.stroke_dashoffset = Some(offset);
        }
        if let Some(ref path_data) = style.svg_path_data {
            props
                .tiers
                .mark(StyleProp::SvgPathData, StyleTier::Stylesheet);
            props.svg_path_data = Some(path_data.clone());
        }
        // Transform origin
        if let Some(to) = style.transform_origin {
            props
                .tiers
                .mark(StyleProp::TransformOrigin, StyleTier::Stylesheet);
            props.transform_origin = Some(to);
        }
        // Border
        if let Some(bw) = style.border_width {
            props
                .tiers
                .mark(StyleProp::BorderWidth, StyleTier::Stylesheet);
            props.border_width = bw;
        }
        if let Some(bc) = style.border_color {
            props
                .tiers
                .mark(StyleProp::BorderColor, StyleTier::Stylesheet);
            props.border_color = Some(bc);
        }
        // Outline
        if let Some(ow) = style.outline_width {
            props
                .tiers
                .mark(StyleProp::OutlineWidth, StyleTier::Stylesheet);
            props.outline_width = ow;
        }
        if let Some(ref oc) = style.outline_color {
            props
                .tiers
                .mark(StyleProp::OutlineColor, StyleTier::Stylesheet);
            props.outline_color = Some(*oc);
        }
        if let Some(offset) = style.outline_offset {
            props
                .tiers
                .mark(StyleProp::OutlineOffset, StyleTier::Stylesheet);
            props.outline_offset = offset;
        }
        // Skew (composed into existing transform as Affine2D)
        if style.skew_x.is_some() || style.skew_y.is_some() {
            use blinc_core::Affine2D;
            let mut skew_affine = Affine2D::IDENTITY;
            if let Some(sx) = style.skew_x {
                skew_affine = skew_affine.then(&Affine2D::skew_x(sx.to_radians()));
            }
            if let Some(sy) = style.skew_y {
                skew_affine = skew_affine.then(&Affine2D::skew_y(sy.to_radians()));
            }
            // Compose with existing transform or set new
            if let Some(blinc_core::Transform::Affine2D(existing)) = &props.transform {
                props.transform =
                    Some(blinc_core::Transform::Affine2D(existing.then(&skew_affine)));
            } else {
                props.transform = Some(blinc_core::Transform::Affine2D(skew_affine));
            }
        }
        // Position flags (fixed/sticky are rendering concerns, not layout)
        if let Some(pos) = style.position {
            use crate::element_style::StylePosition;
            match pos {
                StylePosition::Fixed => {
                    props.is_fixed = true;
                    props.is_sticky = false;
                }
                StylePosition::Sticky => {
                    props.is_fixed = false;
                    props.is_sticky = true;
                    props.sticky_top = style.top;
                    props.sticky_bottom = style.bottom;
                }
                _ => {
                    props.is_fixed = false;
                    props.is_sticky = false;
                }
            }
        }
        // Numeric z-index for stacking order
        if let Some(z) = style.z_index {
            props.z_index = z;
        }
        // Overflow → clips_content (Clip or Scroll means clip children)
        if let Some(overflow) = style.overflow {
            use crate::element_style::StyleOverflow;
            if matches!(overflow, StyleOverflow::Clip | StyleOverflow::Scroll) {
                props.clips_content = true;
            }
        }
        if let Some(ox) = style.overflow_x {
            use crate::element_style::StyleOverflow;
            if matches!(ox, StyleOverflow::Clip | StyleOverflow::Scroll) {
                props.clips_content = true;
            }
        }
        if let Some(oy) = style.overflow_y {
            use crate::element_style::StyleOverflow;
            if matches!(oy, StyleOverflow::Clip | StyleOverflow::Scroll) {
                props.clips_content = true;
            }
        }
        // Visibility
        if let Some(vis) = style.visibility {
            use crate::element_style::StyleVisibility;
            props.visible = matches!(vis, StyleVisibility::Visible);
        }
        // Image properties
        if let Some(of) = style.object_fit {
            props.object_fit = Some(of);
        }
        if let Some(op) = style.object_position {
            props.object_position = Some(op);
        }
        if let Some(ls) = style.loading_strategy {
            props.loading_strategy = Some(ls);
        }
        if let Some(pt) = style.image_placeholder_type {
            props.placeholder_type = Some(pt);
        }
        if let Some(pc) = style.image_placeholder_color {
            props.placeholder_color = Some(pc);
        }
        if let Some(ref pi) = style.image_placeholder_image {
            props.placeholder_image = Some(pi.clone());
        }
        if let Some(fd) = style.fade_duration_ms {
            props.fade_duration_ms = Some(fd);
        }
        // Pointer events
        if let Some(pe) = style.pointer_events {
            props.pointer_events_none = matches!(pe, blinc_core::PointerEvents::None);
        }
        // Cursor
        if let Some(cursor) = style.cursor {
            props.cursor = Some(cursor);
        }
        // Mix blend mode
        if let Some(mode) = style.mix_blend_mode {
            props.mix_blend_mode = Some(mode);
        }
        // Text decoration enhancements
        if let Some(c) = style.text_decoration_color {
            props.text_decoration_color = Some([c.r, c.g, c.b, c.a]);
        }
        if let Some(t) = style.text_decoration_thickness {
            props.text_decoration_thickness = Some(t);
        }
        // Text overflow
        if let Some(to) = style.text_overflow {
            props.text_overflow = Some(to);
        }
        if let Some(ws) = style.white_space {
            props.white_space = Some(ws);
        }
        if let Some(ref mask) = style.mask_image {
            props.mask_image = Some(mask.clone());
        }
        if let Some(ref mode) = style.mask_mode {
            props.mask_mode = Some(mode.clone());
        }
        // @flow shader
        if let Some(ref flow_name) = style.flow {
            props.flow = Some(flow_name.clone());
        }
    }

    /// Apply layout properties from an ElementStyle to a taffy Style.
    ///
    /// This mirrors `apply_element_style_to_props` but for layout properties
    /// that live in the taffy layout system rather than RenderProps.
    pub(crate) fn apply_element_style_to_taffy(
        taffy_style: &mut taffy::Style,
        es: &crate::element_style::ElementStyle,
    ) {
        use taffy::prelude::*;

        if let Some(w) = es.width {
            taffy_style.size.width = match w {
                crate::element_style::StyleDimension::Length(px) => Dimension::length(px),
                crate::element_style::StyleDimension::Percent(p) => Dimension::percent(p),
                crate::element_style::StyleDimension::Auto => Dimension::auto(),
            };
            if matches!(w, crate::element_style::StyleDimension::Auto) {
                taffy_style.flex_basis = Dimension::auto();
                taffy_style.flex_grow = 0.0;
                taffy_style.flex_shrink = 0.0;
            }
        }
        if let Some(h) = es.height {
            taffy_style.size.height = match h {
                crate::element_style::StyleDimension::Length(px) => Dimension::length(px),
                crate::element_style::StyleDimension::Percent(p) => Dimension::percent(p),
                crate::element_style::StyleDimension::Auto => Dimension::auto(),
            };
            if matches!(h, crate::element_style::StyleDimension::Auto) {
                taffy_style.flex_basis = Dimension::auto();
                taffy_style.flex_grow = 0.0;
                taffy_style.flex_shrink = 0.0;
            }
        }
        if let Some(v) = es.min_width {
            taffy_style.min_size.width = LengthPercentageAuto::length(v);
        }
        if let Some(v) = es.max_width {
            taffy_style.max_size.width = LengthPercentageAuto::length(v);
        }
        if let Some(v) = es.min_height {
            taffy_style.min_size.height = LengthPercentageAuto::length(v);
        }
        if let Some(v) = es.max_height {
            taffy_style.max_size.height = LengthPercentageAuto::length(v);
        }
        if let Some(ref p) = es.padding {
            taffy_style.padding = taffy::geometry::Rect {
                top: LengthPercentage::length(p.top),
                right: LengthPercentage::length(p.right),
                bottom: LengthPercentage::length(p.bottom),
                left: LengthPercentage::length(p.left),
            };
        }
        if let Some(ref m) = es.margin {
            taffy_style.margin = taffy::geometry::Rect {
                top: LengthPercentageAuto::length(m.top),
                right: LengthPercentageAuto::length(m.right),
                bottom: LengthPercentageAuto::length(m.bottom),
                left: LengthPercentageAuto::length(m.left),
            };
        }
        if let Some(g) = es.gap {
            taffy_style.gap = taffy::geometry::Size {
                width: LengthPercentage::length(g),
                height: LengthPercentage::length(g),
            };
        }
        if let Some(v) = es.flex_grow {
            taffy_style.flex_grow = v;
        }
        if let Some(v) = es.flex_shrink {
            taffy_style.flex_shrink = v;
        }
        if let Some(pos) = es.position {
            use crate::element_style::StylePosition;
            taffy_style.position = match pos {
                StylePosition::Static | StylePosition::Relative | StylePosition::Sticky => {
                    taffy::Position::Relative
                }
                StylePosition::Absolute | StylePosition::Fixed => taffy::Position::Absolute,
            };
        }
        if let Some(v) = es.top {
            taffy_style.inset.top = LengthPercentageAuto::length(v);
        }
        if let Some(v) = es.right {
            taffy_style.inset.right = LengthPercentageAuto::length(v);
        }
        if let Some(v) = es.bottom {
            taffy_style.inset.bottom = LengthPercentageAuto::length(v);
        }
        if let Some(v) = es.left {
            taffy_style.inset.left = LengthPercentageAuto::length(v);
        }
        // visibility: hidden collapses element from layout (Blinc is always flex)
        if let Some(vis) = es.visibility {
            use crate::element_style::StyleVisibility;
            match vis {
                StyleVisibility::Hidden => taffy_style.display = Display::None,
                StyleVisibility::Visible => {
                    if taffy_style.display == Display::None {
                        taffy_style.display = Display::Flex;
                    }
                }
            }
        }
    }

    /// Snapshot all animatable properties (visual + layout) for a node.
    ///
    /// Combines visual properties from RenderProps with layout properties
    /// from the taffy Style to create a complete KeyframeProperties snapshot
    /// suitable for transition detection.
    pub(crate) fn snapshot_keyframe_properties(
        &self,
        node_id: LayoutNodeId,
    ) -> Option<blinc_animation::KeyframeProperties> {
        let render_node = self.render_nodes.get(&node_id)?;
        let mut kp = Self::render_props_to_keyframe_properties(&render_node.props);

        // Also extract layout properties from taffy style
        if let Some(style) = self.layout_tree.get_style(node_id) {
            if let taffy::style::ExpandedDimension::Length(w) = style.size.width.expand() {
                kp.width = Some(w);
            }
            if let taffy::style::ExpandedDimension::Length(h) = style.size.height.expand() {
                kp.height = Some(h);
            }
            // Extract padding
            let pt = Self::taffy_lp_to_f32(&style.padding.top);
            let pr = Self::taffy_lp_to_f32(&style.padding.right);
            let pb = Self::taffy_lp_to_f32(&style.padding.bottom);
            let pl = Self::taffy_lp_to_f32(&style.padding.left);
            if pt.is_some() || pr.is_some() || pb.is_some() || pl.is_some() {
                kp.padding = Some([
                    pt.unwrap_or(0.0),
                    pr.unwrap_or(0.0),
                    pb.unwrap_or(0.0),
                    pl.unwrap_or(0.0),
                ]);
            }
            // Extract margin
            let mt = Self::taffy_lpa_to_f32(&style.margin.top);
            let mr = Self::taffy_lpa_to_f32(&style.margin.right);
            let mb = Self::taffy_lpa_to_f32(&style.margin.bottom);
            let ml = Self::taffy_lpa_to_f32(&style.margin.left);
            if mt.is_some() || mr.is_some() || mb.is_some() || ml.is_some() {
                kp.margin = Some([
                    mt.unwrap_or(0.0),
                    mr.unwrap_or(0.0),
                    mb.unwrap_or(0.0),
                    ml.unwrap_or(0.0),
                ]);
            }
            // Extract gap
            if let taffy::style::ExpandedLengthPercentage::Length(g) = style.gap.width.expand() {
                kp.gap = Some(g);
            }
            // Extract min/max constraints
            if let taffy::style::ExpandedLengthPercentageAuto::Length(v) =
                style.min_size.width.expand()
            {
                kp.min_width = Some(v);
            }
            if let taffy::style::ExpandedLengthPercentageAuto::Length(v) =
                style.max_size.width.expand()
            {
                kp.max_width = Some(v);
            }
            if let taffy::style::ExpandedLengthPercentageAuto::Length(v) =
                style.min_size.height.expand()
            {
                kp.min_height = Some(v);
            }
            if let taffy::style::ExpandedLengthPercentageAuto::Length(v) =
                style.max_size.height.expand()
            {
                kp.max_height = Some(v);
            }
            // Extract flex grow/shrink
            if style.flex_grow != 0.0 {
                kp.flex_grow = Some(style.flex_grow);
            }
            if style.flex_shrink != 1.0 {
                kp.flex_shrink = Some(style.flex_shrink);
            }
            // Extract inset (top/right/bottom/left)
            if let Some(v) = Self::taffy_lpa_to_f32(&style.inset.top) {
                kp.inset_top = Some(v);
            }
            if let Some(v) = Self::taffy_lpa_to_f32(&style.inset.right) {
                kp.inset_right = Some(v);
            }
            if let Some(v) = Self::taffy_lpa_to_f32(&style.inset.bottom) {
                kp.inset_bottom = Some(v);
            }
            if let Some(v) = Self::taffy_lpa_to_f32(&style.inset.left) {
                kp.inset_left = Some(v);
            }
        }

        Some(kp)
    }

    /// Snapshot keyframe properties for transition "before" state.
    ///
    /// When an active transition exists, overlays the transition's current
    /// interpolated values for transform fields. This avoids QR decomposition
    /// drift: the affine→decompose roundtrip introduces tiny floating-point
    /// errors that would cause spurious property changes and defeat the
    /// same-target guard in `detect_and_start_transitions`.
    pub(crate) fn snapshot_before_keyframe_properties(
        &self,
        node_id: LayoutNodeId,
    ) -> Option<blinc_animation::KeyframeProperties> {
        self.snapshot_keyframe_properties(node_id).map(|mut kp| {
            let stable_id = self.stable_id(node_id);
            let store = self.css_anim_store.lock().unwrap();
            if let Some(active) = stable_id.and_then(|sid| store.transitions.get(&sid)) {
                let cp = &active.current_properties;
                if cp.rotate.is_some() {
                    kp.rotate = cp.rotate;
                }
                if cp.scale_x.is_some() {
                    kp.scale_x = cp.scale_x;
                }
                if cp.scale_y.is_some() {
                    kp.scale_y = cp.scale_y;
                }
                if cp.translate_x.is_some() {
                    kp.translate_x = cp.translate_x;
                }
                if cp.translate_y.is_some() {
                    kp.translate_y = cp.translate_y;
                }
                if cp.skew_x.is_some() {
                    kp.skew_x = cp.skew_x;
                }
                if cp.skew_y.is_some() {
                    kp.skew_y = cp.skew_y;
                }
                if cp.transform_origin.is_some() {
                    kp.transform_origin = cp.transform_origin;
                }
                // Overlay gradient fields from active transition to avoid
                // snapshot mismatch (render props may lag 1 frame behind)
                if cp.gradient_start_color.is_some() {
                    kp.gradient_start_color = cp.gradient_start_color;
                }
                if cp.gradient_end_color.is_some() {
                    kp.gradient_end_color = cp.gradient_end_color;
                }
                if cp.gradient_angle.is_some() {
                    kp.gradient_angle = cp.gradient_angle;
                }
                if cp.background_color.is_some() {
                    kp.background_color = cp.background_color;
                }
                // Mask gradient
                if cp.mask_gradient.is_some() {
                    kp.mask_gradient = cp.mask_gradient;
                }
            }
            kp
        })
    }

    fn taffy_lp_to_f32(lp: &taffy::LengthPercentage) -> Option<f32> {
        match lp.expand() {
            taffy::style::ExpandedLengthPercentage::Length(v) => Some(v),
            _ => None,
        }
    }

    fn taffy_lpa_to_f32(lpa: &taffy::LengthPercentageAuto) -> Option<f32> {
        match lpa.expand() {
            taffy::style::ExpandedLengthPercentageAuto::Length(v) => Some(v),
            _ => None,
        }
    }

    /// Resolve a CSS clip in its element-local frame.
    pub(crate) fn resolve_clip_path(
        clip_path: &blinc_core::ClipPath,
        bounds: &crate::element::ElementBounds,
    ) -> Option<ClipShape> {
        use blinc_core::{ClipLength, ClipPath};

        let w = bounds.width;
        let h = bounds.height;

        fn contour(mut vertices: impl ExactSizeIterator<Item = (f32, f32)>) -> Option<ClipShape> {
            if vertices.len() < 3 {
                return None;
            }
            let (x, y) = vertices.next()?;
            let mut path = blinc_core::Path::new().move_to(x, y);
            for (x, y) in vertices {
                path = path.line_to(x, y);
            }
            Some(ClipShape::Path(path.close()))
        }

        match clip_path {
            ClipPath::Circle { radius, center } => {
                let cx = center.0.resolve(w);
                let cy = center.1.resolve(h);
                let r = radius
                    .map(|r| r.resolve(w.min(h)))
                    .unwrap_or_else(|| w.min(h) * 0.5);
                Some(ClipShape::circle(Point::new(cx, cy), r))
            }
            ClipPath::Ellipse { rx, ry, center } => {
                let cx = center.0.resolve(w);
                let cy = center.1.resolve(h);
                let rx_val = rx.map(|r| r.resolve(w)).unwrap_or(w * 0.5);
                let ry_val = ry.map(|r| r.resolve(h)).unwrap_or(h * 0.5);
                Some(ClipShape::ellipse(
                    Point::new(cx, cy),
                    Vec2::new(rx_val, ry_val),
                ))
            }
            ClipPath::Inset {
                top,
                right,
                bottom,
                left,
                round,
            } => {
                let t = top.resolve(h);
                let r = right.resolve(w);
                let b = bottom.resolve(h);
                let l = left.resolve(w);
                let rect = Rect::new(l, t, w - l - r, h - t - b);
                if let Some(radius) = round {
                    Some(ClipShape::rounded_rect(rect, *radius))
                } else {
                    Some(ClipShape::rect(rect))
                }
            }
            ClipPath::Rect {
                top,
                right,
                bottom,
                left,
                round,
            } => {
                // CSS rect() uses absolute edge positions
                let t = top.resolve(h);
                let r = right.resolve(w);
                let b = bottom.resolve(h);
                let l = left.resolve(w);
                let rect = Rect::new(l, t, r - l, b - t);
                if let Some(radius) = round {
                    Some(ClipShape::rounded_rect(rect, *radius))
                } else {
                    Some(ClipShape::rect(rect))
                }
            }
            ClipPath::Xywh {
                x,
                y,
                w: cw,
                h: ch,
                round,
            } => {
                let rx = x.resolve(w);
                let ry = y.resolve(h);
                let rw = cw.resolve(w);
                let rh = ch.resolve(h);
                let rect = Rect::new(rx, ry, rw, rh);
                if let Some(radius) = round {
                    Some(ClipShape::rounded_rect(rect, *radius))
                } else {
                    Some(ClipShape::rect(rect))
                }
            }
            ClipPath::Polygon { points } => {
                contour(points.iter().map(|(px, py)| (px.resolve(w), py.resolve(h))))
            }
            ClipPath::Path { vertices } => contour(vertices.iter().copied()),
        }
    }

    /// Helper: convert ClipLength to percent value
    pub(crate) fn clip_length_to_percent(len: &blinc_core::ClipLength) -> f32 {
        match len {
            blinc_core::ClipLength::Percent(p) => *p,
            blinc_core::ClipLength::Px(px) => *px,
        }
    }
}
