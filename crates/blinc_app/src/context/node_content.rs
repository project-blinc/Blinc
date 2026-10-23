//! Collect the content channels of a node using its resolved inherited state.
use super::{
    Arc, ElementType, FlowElement, FontWeight, ImageElement, LayoutNodeId, RenderContext,
    RenderLayer, RenderTree, SvgElement, TextAlign, TextElement, Transform3DLayerInfo,
    effective_single_clip, text_clip,
};
use blinc_layout::ElementBounds;

/// Resolved state shared by the content channels of one collected node.
pub(super) struct NodeContent<'a> {
    pub(super) tree: &'a RenderTree,
    pub(super) node: LayoutNodeId,
    pub(super) bounds: ElementBounds,
    pub(super) inside_glass: bool,
    pub(super) is_glass: bool,
    pub(super) abs_x: f32,
    pub(super) abs_y: f32,
    pub(super) effective_motion_scale: (f32, f32),
    pub(super) effective_motion_translate: (f32, f32),
    pub(super) effective_motion_scale_center: Option<(f32, f32)>,
    pub(super) node_css_affine: Option<[f32; 6]>,
    pub(super) scale: f32,
    pub(super) current_clip: Option<[f32; 4]>,
    pub(super) current_scroll_clip: Option<[f32; 4]>,
    pub(super) current_clip_radius: Option<[f32; 4]>,
    pub(super) effective_text_color: Option<[f32; 4]>,
    pub(super) effective_motion_opacity: f32,
    pub(super) inherited_css_opacity: f32,
    pub(super) effective_composite_layer_opacity: f32,
    pub(super) nearest_composite_ancestor: Option<blinc_layout::tree::StableNodeId>,
    pub(super) text_path: &'a Option<Arc<text_clip::CollectedTextPath>>,
    pub(super) inside_3d_layer: &'a Option<Transform3DLayerInfo>,
    pub(super) children_inside_foreground: bool,
    pub(super) inside_motion_subtree: bool,
    pub(super) parent_node: Option<LayoutNodeId>,
    pub(super) z_layer: &'a u32,
}

impl RenderContext {
    pub(super) fn collect_node_content(
        &self,
        state: NodeContent<'_>,
        texts: &mut Vec<TextElement>,
        svgs: &mut Vec<SvgElement>,
        images: &mut Vec<ImageElement>,
        flows: &mut Vec<FlowElement>,
    ) {
        let NodeContent {
            tree,
            node,
            bounds,
            inside_glass,
            is_glass,
            abs_x,
            abs_y,
            effective_motion_scale,
            effective_motion_translate,
            effective_motion_scale_center,
            node_css_affine,
            scale,
            current_clip,
            current_scroll_clip,
            current_clip_radius,
            effective_text_color,
            effective_motion_opacity,
            inherited_css_opacity,
            effective_composite_layer_opacity,
            nearest_composite_ancestor,
            text_path,
            inside_3d_layer,
            children_inside_foreground,
            inside_motion_subtree,
            parent_node,
            z_layer,
        } = state;
        if let Some(render_node) = tree.get_render_node(node) {
            // Determine effective layer: children inside glass render in Foreground
            let effective_layer = if inside_glass && !is_glass {
                RenderLayer::Foreground
            } else if is_glass {
                RenderLayer::Glass
            } else {
                render_node.props.layer
            };

            match &render_node.element_type {
                ElementType::Text(text_data) => {
                    // Apply DPI before motion, matching primitive emission.
                    let base_x = abs_x * scale;
                    let base_y = abs_y * scale;
                    let base_width = bounds.width * scale;
                    let base_height = bounds.height * scale;

                    // Keep glyphs at their base rasterization size and apply motion through
                    // the affine.
                    let has_motion_scale = (effective_motion_scale.0 - 1.0).abs() > 1e-6
                        || (effective_motion_scale.1 - 1.0).abs() > 1e-6;
                    let has_motion_translate = effective_motion_translate.0.abs() > 1e-6
                        || effective_motion_translate.1.abs() > 1e-6;
                    let motion_affine = if has_motion_scale || has_motion_translate {
                        let (sx, sy) = effective_motion_scale;
                        let (tx, ty) = effective_motion_translate;
                        let (cx, cy) = effective_motion_scale_center.unwrap_or((0.0, 0.0));
                        // Scale around (cx, cy) plus translate (tx, ty) — all in layout coords.
                        Some([sx, 0.0, 0.0, sy, cx * (1.0 - sx) + tx, cy * (1.0 - sy) + ty])
                    } else {
                        None
                    };

                    // Compose motion_affine ∘ node_css_affine (motion is the outer transform).
                    let text_affine = match (motion_affine, node_css_affine) {
                        (Some(m), Some(c)) => {
                            let [ma, mb, mc, md, mtx, mty] = m;
                            let [ca, cb, cc, cd, ctx, cty] = c;
                            Some([
                                ma * ca + mc * cb,
                                mb * ca + md * cb,
                                ma * cc + mc * cd,
                                mb * cc + md * cd,
                                ma * ctx + mc * cty + mtx,
                                mb * ctx + md * cty + mty,
                            ])
                        }
                        (Some(m), None) => Some(m),
                        (None, c) => c,
                    };

                    // Keep glyph layout at base size; apply motion through the affine.
                    let scaled_x = base_x;
                    let scaled_y = base_y;
                    let scaled_width = base_width;
                    let scaled_height = base_height;

                    // Use CSS-overridden font size if available (from stylesheet/animation/transition)
                    let base_font_size = render_node.props.font_size.unwrap_or(text_data.font_size);
                    // Rasterize at base size only — motion scale is in text_affine.
                    let scaled_font_size = base_font_size * scale;
                    let scaled_measured_width = text_data.measured_width * scale;

                    // Intersect primary clip with scroll clip — text only supports
                    // a single clip rect so we must merge both boundaries.
                    let effective_clip = effective_single_clip(current_clip, current_scroll_clip);
                    let scaled_clip = effective_clip
                        .map(|[cx, cy, cw, ch]| [cx * scale, cy * scale, cw * scale, ch * scale]);

                    // Trace nontrivial motion values.
                    if effective_motion_translate.0.abs() > 0.1
                        || effective_motion_translate.1.abs() > 0.1
                        || (effective_motion_scale.0 - 1.0).abs() > 0.01
                        || (effective_motion_scale.1 - 1.0).abs() > 0.01
                    {
                        tracing::trace!(
                            "Text '{}': motion_translate=({:.1}, {:.1}), motion_scale=({:.2}, {:.2}), base=({:.1}, {:.1}), final=({:.1}, {:.1})",
                            text_data.content,
                            effective_motion_translate.0,
                            effective_motion_translate.1,
                            effective_motion_scale.0,
                            effective_motion_scale.1,
                            base_x,
                            base_y,
                            scaled_x,
                            scaled_y,
                        );
                    }
                    tracing::trace!(
                        "Text '{}': abs=({:.1}, {:.1}), size=({:.1}x{:.1}), font={:.1}, align={:?}, v_align={:?}, z_layer={}",
                        text_data.content,
                        scaled_x,
                        scaled_y,
                        scaled_width,
                        scaled_height,
                        scaled_font_size,
                        text_data.align,
                        text_data.v_align,
                        *z_layer
                    );

                    // Apply text-overflow: ellipsis truncation if needed.
                    // Check both text_data.wrap (set at build time) and render_node.props.white_space
                    // (set by CSS after build). CSS white-space: nowrap overrides the builder wrap setting.
                    let is_nowrap = !text_data.wrap
                        || matches!(
                            render_node.props.white_space,
                            Some(blinc_layout::element_style::WhiteSpace::Nowrap)
                                | Some(blinc_layout::element_style::WhiteSpace::Pre)
                        );
                    let content = if is_nowrap
                        && matches!(
                            render_node.props.text_overflow,
                            Some(blinc_layout::element_style::TextOverflow::Ellipsis)
                        )
                        && scaled_measured_width > scaled_width
                        && scaled_width > 0.0
                    {
                        // Measure with the same options used for layout
                        let mut options = blinc_layout::text_measure::TextLayoutOptions::new();
                        options.font_name = text_data.font_family.name.clone();
                        options.generic_font = text_data.font_family.generic;
                        options.font_weight =
                            match render_node.props.font_weight.unwrap_or(text_data.weight) {
                                FontWeight::Bold => 700,
                                FontWeight::Normal => 400,
                                FontWeight::Light => 300,
                                _ => 400,
                            };
                        options.letter_spacing = render_node
                            .props
                            .letter_spacing
                            .unwrap_or(text_data.letter_spacing);

                        // Measure "..." to know reserved width
                        let ellipsis = "\u{2026}";
                        let ellipsis_w = blinc_layout::text_measure::measure_text_with_options(
                            ellipsis,
                            scaled_font_size / scale,
                            &options,
                        )
                        .width
                            * scale;
                        let target_width = scaled_width - ellipsis_w;

                        if target_width > 0.0 {
                            // Binary search for the right truncation point
                            let chars: Vec<char> = text_data.content.chars().collect();
                            let mut lo = 0usize;
                            let mut hi = chars.len();
                            while lo < hi {
                                #[allow(clippy::manual_div_ceil)]
                                let mid = (lo + hi + 1) / 2;
                                let sub: String = chars[..mid].iter().collect();
                                let w = blinc_layout::text_measure::measure_text_with_options(
                                    &sub,
                                    scaled_font_size / scale,
                                    &options,
                                )
                                .width
                                    * scale;
                                if w <= target_width {
                                    lo = mid;
                                } else {
                                    hi = mid - 1;
                                }
                            }
                            let truncated: String = chars[..lo].iter().collect();
                            format!("{}{}", truncated.trim_end(), ellipsis)
                        } else {
                            ellipsis.to_string()
                        }
                    } else {
                        text_data.content.clone()
                    };

                    texts.push(TextElement {
                        content,
                        path_clip: text_path.clone(),
                        x: scaled_x,
                        y: scaled_y,
                        width: scaled_width,
                        height: scaled_height,
                        font_size: scaled_font_size,
                        color: effective_text_color.unwrap_or(text_data.color),
                        align: text_data.align,
                        weight: render_node.props.font_weight.unwrap_or(text_data.weight),
                        italic: render_node
                            .props
                            .font_style
                            .map_or(text_data.italic, |fs| fs.is_italic()),
                        v_align: text_data.v_align,
                        clip_bounds: scaled_clip,
                        // Apply promoted opacity at dispatch; keep cached glyphs at base alpha.
                        motion_opacity: effective_motion_opacity
                            * render_node.props.opacity
                            * inherited_css_opacity
                            * if nearest_composite_ancestor.is_some() {
                                1.0
                            } else {
                                effective_composite_layer_opacity
                            },
                        wrap: !is_nowrap && text_data.wrap,
                        line_height: text_data.line_height,
                        measured_width: scaled_measured_width,
                        font_family: text_data.font_family.clone(),
                        word_spacing: text_data.word_spacing,
                        letter_spacing: render_node
                            .props
                            .letter_spacing
                            .unwrap_or(text_data.letter_spacing),
                        z_index: *z_layer,
                        ascender: text_data.ascender * scale,
                        half_leading: text_data.half_leading * scale,
                        strikethrough: render_node.props.text_decoration.map_or(
                            text_data.strikethrough,
                            |td| {
                                matches!(
                                    td,
                                    blinc_layout::element_style::TextDecoration::LineThrough
                                )
                            },
                        ),
                        underline: render_node.props.text_decoration.map_or(
                            text_data.underline,
                            |td| {
                                matches!(td, blinc_layout::element_style::TextDecoration::Underline)
                            },
                        ),
                        decoration_color: render_node.props.text_decoration_color,
                        decoration_thickness: render_node.props.text_decoration_thickness,
                        css_affine: text_affine,
                        text_shadow: render_node.props.text_shadow,
                        transform_3d_layer: inside_3d_layer.clone(),
                        is_foreground: children_inside_foreground,
                        in_motion_subtree: inside_motion_subtree,
                        composite_ancestor: nearest_composite_ancestor,
                    });
                }
                ElementType::Svg(svg_data) => {
                    // Apply DPI scale factor FIRST to match shape rendering order
                    let base_x = abs_x * scale;
                    let base_y = abs_y * scale;
                    let base_width = bounds.width * scale;
                    let base_height = bounds.height * scale;

                    // Scale motion translate by DPI factor
                    let scaled_motion_tx = effective_motion_translate.0 * scale;
                    let scaled_motion_ty = effective_motion_translate.1 * scale;

                    // Apply motion around its inherited scale center.
                    let (scaled_x, scaled_y, scaled_width, scaled_height) =
                        if let Some((motion_center_x, motion_center_y)) =
                            effective_motion_scale_center
                        {
                            let motion_center_x_scaled = motion_center_x * scale;
                            let motion_center_y_scaled = motion_center_y * scale;

                            let rel_x = base_x - motion_center_x_scaled;
                            let rel_y = base_y - motion_center_y_scaled;

                            let scaled_rel_x = rel_x * effective_motion_scale.0;
                            let scaled_rel_y = rel_y * effective_motion_scale.1;
                            let scaled_w = base_width * effective_motion_scale.0;
                            let scaled_h = base_height * effective_motion_scale.1;

                            let final_x = motion_center_x_scaled + scaled_rel_x + scaled_motion_tx;
                            let final_y = motion_center_y_scaled + scaled_rel_y + scaled_motion_ty;

                            (final_x, final_y, scaled_w, scaled_h)
                        } else {
                            let final_x = base_x + scaled_motion_tx;
                            let final_y = base_y + scaled_motion_ty;
                            (final_x, final_y, base_width, base_height)
                        };

                    // Intersect primary clip with scroll clip — text/SVG only support
                    // a single clip rect so we must merge both boundaries.
                    let effective_clip = effective_single_clip(current_clip, current_scroll_clip);
                    let scaled_clip = effective_clip
                        .map(|[cx, cy, cw, ch]| [cx * scale, cy * scale, cw * scale, ch * scale]);

                    // Tint resolves `currentColor` references in SVG source.
                    // CSS fill/stroke are explicit overrides injected as SVG attributes.
                    // Both can coexist: tint handles currentColor, CSS handles specifics.
                    svgs.push(SvgElement {
                        source: svg_data.source.clone(),
                        x: scaled_x,
                        y: scaled_y,
                        width: scaled_width,
                        height: scaled_height,
                        // Resolve currentColor from the effective inherited text color.
                        tint: svg_data.tint.or_else(|| {
                            effective_text_color
                                .map(|c| blinc_core::Color::rgba(c[0], c[1], c[2], c[3]))
                        }),
                        fill: render_node
                            .props
                            .fill
                            .map(|c| blinc_core::Color::rgba(c[0], c[1], c[2], c[3]))
                            .or(svg_data.fill),
                        stroke: render_node
                            .props
                            .stroke
                            .map(|c| blinc_core::Color::rgba(c[0], c[1], c[2], c[3]))
                            .or(svg_data.stroke),
                        stroke_width: render_node.props.stroke_width.or(svg_data.stroke_width),
                        stroke_dasharray: render_node.props.stroke_dasharray.clone(),
                        stroke_dashoffset: render_node.props.stroke_dashoffset,
                        svg_path_data: render_node.props.svg_path_data.clone(),
                        clip_bounds: scaled_clip,
                        // Same base-alpha contract as TextElement:
                        // composite factor applied live at dispatch.
                        motion_opacity: effective_motion_opacity
                            * render_node.props.opacity
                            * inherited_css_opacity
                            * if nearest_composite_ancestor.is_some() {
                                1.0
                            } else {
                                effective_composite_layer_opacity
                            },
                        css_affine: node_css_affine,
                        tag_overrides: render_node.props.svg_tag_styles.clone(),
                        transform_3d_layer: inside_3d_layer.clone(),
                        in_motion_subtree: inside_motion_subtree,
                        z_layer: *z_layer,
                        composite_ancestor: nearest_composite_ancestor,
                    });
                }
                ElementType::Image(image_data) => {
                    // Apply DPI scale factor to image positions and sizes
                    let scaled_clip = current_clip
                        .map(|[cx, cy, cw, ch]| [cx * scale, cy * scale, cw * scale, ch * scale]);

                    // Scale clip radius by DPI factor (radius values are in layout coordinates)
                    let scaled_clip_radius = current_clip_radius
                        .map(|[tl, tr, br, bl]| [tl * scale, tr * scale, br * scale, bl * scale])
                        .unwrap_or([0.0; 4]);

                    // Scale scroll clip by DPI factor
                    let scaled_scroll_clip = current_scroll_clip
                        .map(|[cx, cy, cw, ch]| [cx * scale, cy * scale, cw * scale, ch * scale]);

                    // Look up parent render props for CSS property inheritance.
                    // Images render via a separate pipeline and don't inherit parent CSS
                    // properties automatically — we must propagate them explicitly.
                    let parent_props = parent_node
                        .and_then(|pid| tree.get_render_node(pid))
                        .map(|pn| &pn.props);

                    // Opacity: own CSS opacity * inherited CSS opacity chain * builder * motion
                    // * composite-layer opacity (so images inside a composite-promoted
                    // subtree fade in lockstep with the bake's blit).
                    let own_css_opacity = render_node.props.opacity;
                    let final_opacity = image_data.opacity
                        * own_css_opacity
                        * inherited_css_opacity
                        * effective_motion_opacity
                        * effective_composite_layer_opacity;

                    // Border-radius: prefer own CSS, then builder.
                    // Parent clip (now at content-box) handles corner rounding.
                    let own_br = render_node.props.border_radius.top_left;
                    let final_border_radius = if own_br > 0.0 {
                        own_br * scale
                    } else {
                        image_data.border_radius * scale
                    };

                    // Border: use image's own CSS border (parent border renders via SDF,
                    // visible because clip now insets by border-width)
                    let border_width = render_node.props.border_width * scale;
                    let border_color = render_node
                        .props
                        .border_color
                        .unwrap_or(blinc_core::Color::TRANSPARENT);

                    // Shadow: use image's own (parent shadow renders via SDF).
                    // ImageElement holds a single shadow — pick the topmost
                    // layer of any compound stack.
                    let shadow = render_node.props.shadow.first().copied();

                    // Filter: prefer own, fall back to parent
                    let own_filter = &render_node.props.filter;
                    let parent_filter = parent_props.and_then(|p| p.filter.as_ref());
                    let effective_filter = own_filter.as_ref().or(parent_filter);
                    let filter_a = effective_filter
                        .map(|f| Self::css_filter_to_arrays(f).0)
                        .unwrap_or([0.0, 0.0, 0.0, 0.0]);
                    let filter_b = effective_filter
                        .map(|f| Self::css_filter_to_arrays(f).1)
                        .unwrap_or([1.0, 1.0, 1.0, 0.0]);

                    // object-fit / object-position: CSS overrides builder values
                    let final_object_fit = render_node
                        .props
                        .object_fit
                        .unwrap_or(image_data.object_fit);
                    let final_object_position = render_node
                        .props
                        .object_position
                        .unwrap_or(image_data.object_position);

                    // CSS overrides for lazy loading properties
                    let final_loading_strategy = render_node
                        .props
                        .loading_strategy
                        .unwrap_or(image_data.loading_strategy);
                    let final_placeholder_type = render_node
                        .props
                        .placeholder_type
                        .unwrap_or(image_data.placeholder_type);
                    let final_placeholder_color = render_node
                        .props
                        .placeholder_color
                        .unwrap_or(image_data.placeholder_color);
                    let final_placeholder_image = render_node
                        .props
                        .placeholder_image
                        .clone()
                        .or_else(|| image_data.placeholder_image.clone());
                    let final_fade_duration = render_node
                        .props
                        .fade_duration_ms
                        .unwrap_or(image_data.fade_duration_ms);

                    // Mask: prefer own, fall back to parent
                    let own_mask = render_node.props.mask_image.as_ref();
                    let parent_mask = parent_props.and_then(|p| p.mask_image.as_ref());
                    let effective_mask = own_mask.or(parent_mask);
                    let (mask_params, mask_info) = Self::mask_image_to_arrays(effective_mask);

                    images.push(ImageElement {
                        source: image_data.source.clone(),
                        x: abs_x * scale,
                        y: abs_y * scale,
                        width: bounds.width * scale,
                        height: bounds.height * scale,
                        object_fit: final_object_fit,
                        object_position: final_object_position,
                        opacity: final_opacity,
                        border_radius: final_border_radius,
                        tint: image_data.tint,
                        clip_bounds: scaled_clip,
                        clip_radius: scaled_clip_radius,
                        layer: effective_layer,
                        loading_strategy: final_loading_strategy,
                        placeholder_type: final_placeholder_type,
                        placeholder_color: final_placeholder_color,
                        placeholder_image: final_placeholder_image,
                        fade_duration_ms: final_fade_duration,
                        z_index: *z_layer,
                        border_width,
                        border_color,
                        css_affine: node_css_affine,
                        shadow,
                        filter_a,
                        filter_b,
                        scroll_clip: scaled_scroll_clip,
                        mask_params,
                        mask_info,
                        transform_3d_layer: inside_3d_layer.clone(),
                    });
                }
                // Canvas elements are rendered inline during tree traversal (in render_layer)
                ElementType::Canvas(_) => {}
                ElementType::Div => {
                    // Check if this div has a background image brush
                    if let Some(blinc_core::Brush::Image(ref img_brush)) =
                        render_node.props.background
                    {
                        let scaled_clip = current_clip.map(|[cx, cy, cw, ch]| {
                            [cx * scale, cy * scale, cw * scale, ch * scale]
                        });
                        let scaled_clip_radius = current_clip_radius
                            .map(|[tl, tr, br, bl]| {
                                [tl * scale, tr * scale, br * scale, bl * scale]
                            })
                            .unwrap_or([0.0; 4]);
                        let scaled_scroll_clip_bg = current_scroll_clip.map(|[cx, cy, cw, ch]| {
                            [cx * scale, cy * scale, cw * scale, ch * scale]
                        });

                        images.push(ImageElement {
                            source: img_brush.source.clone(),
                            x: abs_x * scale,
                            y: abs_y * scale,
                            width: bounds.width * scale,
                            height: bounds.height * scale,
                            object_fit: match img_brush.fit {
                                blinc_core::ImageFit::Cover => 0,
                                blinc_core::ImageFit::Contain => 1,
                                blinc_core::ImageFit::Fill => 2,
                                blinc_core::ImageFit::Tile => 0,
                            },
                            object_position: [img_brush.position.x, img_brush.position.y],
                            opacity: img_brush.opacity
                                * render_node.props.opacity
                                * inherited_css_opacity
                                * effective_motion_opacity
                                * effective_composite_layer_opacity,
                            border_radius: render_node.props.border_radius.top_left * scale,
                            tint: [
                                img_brush.tint.r,
                                img_brush.tint.g,
                                img_brush.tint.b,
                                img_brush.tint.a,
                            ],
                            clip_bounds: scaled_clip,
                            clip_radius: scaled_clip_radius,
                            layer: effective_layer,
                            loading_strategy: 0, // Eager
                            placeholder_type: 0, // None
                            placeholder_color: [0.0; 4],
                            placeholder_image: None,
                            fade_duration_ms: 0,
                            z_index: *z_layer,
                            border_width: 0.0,
                            border_color: blinc_core::Color::TRANSPARENT,
                            css_affine: node_css_affine,
                            shadow: render_node.props.shadow.first().copied(),
                            filter_a: render_node
                                .props
                                .filter
                                .as_ref()
                                .map(|f| Self::css_filter_to_arrays(f).0)
                                .unwrap_or([0.0, 0.0, 0.0, 0.0]),
                            filter_b: render_node
                                .props
                                .filter
                                .as_ref()
                                .map(|f| Self::css_filter_to_arrays(f).1)
                                .unwrap_or([1.0, 1.0, 1.0, 0.0]),
                            scroll_clip: scaled_scroll_clip_bg,
                            mask_params: {
                                let (mp, _) = Self::mask_image_to_arrays(
                                    render_node.props.mask_image.as_ref(),
                                );
                                mp
                            },
                            mask_info: {
                                let (_, mi) = Self::mask_image_to_arrays(
                                    render_node.props.mask_image.as_ref(),
                                );
                                mi
                            },
                            transform_3d_layer: inside_3d_layer.clone(),
                        });
                    }
                }
                // StyledText: render text with inline styling using multiple TextElements
                ElementType::StyledText(styled_data) => {
                    // Apply DPI scale factor first
                    let base_x = abs_x * scale;
                    let base_y = abs_y * scale;
                    let base_width = bounds.width * scale;
                    let base_height = bounds.height * scale;

                    // Scale motion translate by DPI factor
                    let scaled_motion_tx = effective_motion_translate.0 * scale;
                    let scaled_motion_ty = effective_motion_translate.1 * scale;

                    // Apply motion around its inherited scale center.
                    let (scaled_x, scaled_y, scaled_width, scaled_height) =
                        if let Some((motion_center_x, motion_center_y)) =
                            effective_motion_scale_center
                        {
                            let motion_center_x_scaled = motion_center_x * scale;
                            let motion_center_y_scaled = motion_center_y * scale;

                            let rel_x = base_x - motion_center_x_scaled;
                            let rel_y = base_y - motion_center_y_scaled;

                            let scaled_rel_x = rel_x * effective_motion_scale.0;
                            let scaled_rel_y = rel_y * effective_motion_scale.1;
                            let scaled_w = base_width * effective_motion_scale.0;
                            let scaled_h = base_height * effective_motion_scale.1;

                            let final_x = motion_center_x_scaled + scaled_rel_x + scaled_motion_tx;
                            let final_y = motion_center_y_scaled + scaled_rel_y + scaled_motion_ty;

                            (final_x, final_y, scaled_w, scaled_h)
                        } else {
                            let final_x = base_x + scaled_motion_tx;
                            let final_y = base_y + scaled_motion_ty;
                            (final_x, final_y, base_width, base_height)
                        };

                    // Use CSS-overridden font size if available (from stylesheet/animation/transition)
                    let base_styled_font_size =
                        render_node.props.font_size.unwrap_or(styled_data.font_size);
                    let scaled_font_size = base_styled_font_size * effective_motion_scale.1 * scale;
                    // Intersect primary clip with scroll clip for styled text
                    let effective_clip = effective_single_clip(current_clip, current_scroll_clip);
                    let scaled_clip = effective_clip
                        .map(|[cx, cy, cw, ch]| [cx * scale, cy * scale, cw * scale, ch * scale]);

                    // Build non-overlapping segments from potentially overlapping spans
                    // This handles nested tags like <span color="red"><b>text</b></span>
                    let content = &styled_data.content;
                    let content_len = content.len();

                    // Get default styles from element config
                    let default_bold = styled_data.weight == FontWeight::Bold;
                    let default_italic = styled_data.italic;

                    // Collect all boundary positions where style might change
                    let mut boundaries: Vec<usize> = vec![0, content_len];
                    for span in &styled_data.spans {
                        if span.start < content_len {
                            boundaries.push(span.start);
                        }
                        if span.end <= content_len {
                            boundaries.push(span.end);
                        }
                    }
                    boundaries.sort();
                    boundaries.dedup();

                    // Build segments between boundaries
                    #[allow(clippy::type_complexity)]
                    let mut segments: Vec<(
                        usize,
                        usize,
                        [f32; 4],
                        bool,
                        bool,
                        bool,
                        bool,
                    )> = Vec::new();

                    for window in boundaries.windows(2) {
                        let seg_start = window[0];
                        let seg_end = window[1];
                        if seg_start >= seg_end {
                            continue;
                        }

                        // Determine style for this segment by merging all overlapping spans
                        let mut color: Option<[f32; 4]> = None;
                        let mut bold = default_bold;
                        let mut italic = default_italic;
                        let mut underline = false;
                        let mut strikethrough = false;

                        for span in &styled_data.spans {
                            // Check if span overlaps this segment
                            if span.start <= seg_start && span.end >= seg_end {
                                // This span covers this segment - merge styles
                                if span.bold {
                                    bold = true;
                                }
                                if span.italic {
                                    italic = true;
                                }
                                if span.underline {
                                    underline = true;
                                }
                                if span.strikethrough {
                                    strikethrough = true;
                                }
                                // Use color if span has explicit color (not transparent)
                                if span.color[3] > 0.0 {
                                    color = Some(span.color);
                                }
                            }
                        }

                        // CSS text_color override takes precedence over span colors
                        let default_color = render_node
                            .props
                            .text_color
                            .unwrap_or(styled_data.default_color);
                        let final_color = color.unwrap_or(default_color);
                        segments.push((
                            seg_start,
                            seg_end,
                            final_color,
                            bold,
                            italic,
                            underline,
                            strikethrough,
                        ));
                    }

                    // Use consistent ascender from element for baseline alignment
                    let scaled_ascender = styled_data.ascender * scale;
                    let scaled_half_leading = styled_data.half_leading * scale;

                    // Where the text wraps at this node's laid-out width.
                    // Segments are placed within a line, so a segment
                    // straddling a wrap point is drawn as two runs rather
                    // than one that overflows.
                    let mut wrap_options = blinc_layout::text_measure::TextLayoutOptions::new();
                    wrap_options.font_name = styled_data.font_family.name.clone();
                    wrap_options.generic_font = styled_data.font_family.generic;
                    wrap_options.font_weight = styled_data.weight.weight();
                    wrap_options.italic = styled_data.italic;
                    wrap_options.line_height = styled_data.line_height;
                    wrap_options.max_width = (bounds.width > 0.0).then_some(bounds.width);
                    let lines = blinc_layout::text_measure::text_line_spans(
                        content,
                        styled_data.font_size,
                        &wrap_options,
                    );
                    // Measured, not `font_size * line_height`: the layout
                    // sized this node from font metrics, and a run placed
                    // on a different step would drift further off with
                    // every line.
                    let scaled_line_height = blinc_layout::text_measure::line_height_px(
                        styled_data.font_size,
                        &wrap_options,
                    ) * scale
                        * effective_motion_scale.1;

                    // Calculate x offsets for each segment and push as TextElements
                    for (row, line) in lines.iter().enumerate() {
                        let line_y = scaled_y + row as f32 * scaled_line_height;
                        let mut x_offset = 0.0f32;

                        for &(start, end, color, bold, italic, underline, strikethrough) in
                            &segments
                        {
                            // Clip the segment to the line it is being drawn on.
                            let start = start.max(line.start);
                            let end = end.min(line.end).min(content.len());
                            if start >= end
                                || !content.is_char_boundary(start)
                                || !content.is_char_boundary(end)
                            {
                                continue;
                            }
                            let segment_text = &content[start..end];
                            if segment_text.is_empty() {
                                continue;
                            }

                            // Measure segment width for positioning
                            let mut options = blinc_layout::text_measure::TextLayoutOptions::new();
                            options.font_name = styled_data.font_family.name.clone();
                            options.generic_font = styled_data.font_family.generic;
                            options.font_weight = if bold { 700 } else { 400 };
                            options.italic = italic;
                            let metrics = blinc_layout::text_measure::measure_text_with_options(
                                segment_text,
                                styled_data.font_size,
                                &options,
                            );
                            // Apply both DPI scale and motion scale to segment width
                            let segment_width = metrics.width * scale * effective_motion_scale.0;

                            texts.push(TextElement {
                                content: segment_text.to_string(),
                                path_clip: text_path.clone(),
                                x: scaled_x + x_offset,
                                y: line_y,
                                width: segment_width,
                                // One line keeps the node's full height so
                                // existing vertical centring is untouched;
                                // wrapped text bands each run to its own line.
                                height: if lines.len() > 1 {
                                    scaled_line_height
                                } else {
                                    scaled_height
                                },
                                font_size: scaled_font_size,
                                color,
                                align: TextAlign::Left, // Always left-align segments
                                weight: if bold {
                                    FontWeight::Bold
                                } else {
                                    FontWeight::Normal
                                },
                                italic,
                                v_align: styled_data.v_align,
                                clip_bounds: scaled_clip,
                                // Base-alpha contract: composite factor
                                // applied live at dispatch.
                                motion_opacity: effective_motion_opacity
                                    * render_node.props.opacity
                                    * inherited_css_opacity
                                    * if nearest_composite_ancestor.is_some() {
                                        1.0
                                    } else {
                                        effective_composite_layer_opacity
                                    },
                                wrap: false, // Don't wrap individual segments
                                line_height: styled_data.line_height,
                                measured_width: segment_width,
                                font_family: styled_data.font_family.clone(),
                                word_spacing: 0.0,
                                letter_spacing: render_node.props.letter_spacing.unwrap_or(0.0),
                                z_index: *z_layer,
                                ascender: scaled_ascender * effective_motion_scale.1, // Scale ascender with motion
                                half_leading: scaled_half_leading * effective_motion_scale.1,
                                strikethrough,
                                underline,
                                decoration_color: render_node.props.text_decoration_color,
                                decoration_thickness: render_node.props.text_decoration_thickness,
                                css_affine: node_css_affine,
                                text_shadow: render_node.props.text_shadow,
                                transform_3d_layer: inside_3d_layer.clone(),
                                is_foreground: children_inside_foreground,
                                in_motion_subtree: inside_motion_subtree,
                                composite_ancestor: nearest_composite_ancestor,
                            });

                            x_offset += segment_width;
                        }
                    }
                }
            }

            // Collect flow element if this node has a @flow shader reference.
            // Flow elements render via custom GPU pipelines instead of (or on top of) the SDF path.
            if let Some(ref flow_name) = render_node.props.flow {
                flows.push(FlowElement {
                    flow_name: flow_name.clone(),
                    flow_graph: render_node.props.flow_graph.clone(),
                    x: abs_x * scale,
                    y: abs_y * scale,
                    width: bounds.width * scale,
                    height: bounds.height * scale,
                    z_index: *z_layer,
                    corner_radius: render_node.props.border_radius.top_left * scale,
                });
            }
        }
    }
}
