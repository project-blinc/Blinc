//! Walk inherited transforms, clips and motion state before collecting node content.
use super::node_content::NodeContent;
use super::{
    ATLAS_GC_MAX_AGE, ATLAS_GC_PRESSURE, Arc, ImageElement, LayoutNodeId, RenderContext,
    RenderLayer, RenderTree, SvgElement, TextElement, Transform3DLayerInfo, merge_scroll_clip,
    text_clip,
};
use blinc_core::Affine2D;

/// The intersection of two rounded clips (x, y, width, height and corner radii tl, tr, br, bl):
/// a corner of the result keeps a radius only from a clip whose corner it is, so a sharp clip
/// cut inside a rounded one stays sharp.
fn intersect_rounded(a: [f32; 4], ra: [f32; 4], b: [f32; 4], rb: [f32; 4]) -> ([f32; 4], [f32; 4]) {
    let (x0, y0) = (a[0].max(b[0]), a[1].max(b[1]));
    let (x1, y1) = (
        (a[0] + a[2]).min(b[0] + b[2]),
        (a[1] + a[3]).min(b[1] + b[3]),
    );
    let near = |p: f32, q: f32| (p - q).abs() < 0.5;
    let corner = |c: [f32; 4], r: [f32; 4], i: usize| {
        let (cx, cy) = match i {
            0 => (c[0], c[1]),
            1 => (c[0] + c[2], c[1]),
            2 => (c[0] + c[2], c[1] + c[3]),
            _ => (c[0], c[1] + c[3]),
        };
        let (px, py) = match i {
            0 => (x0, y0),
            1 => (x1, y0),
            2 => (x1, y1),
            _ => (x0, y1),
        };
        if near(cx, px) && near(cy, py) {
            r[i]
        } else {
            0.0
        }
    };
    let radius = std::array::from_fn(|i| corner(a, ra, i).max(corner(b, rb, i)));
    ([x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0)], radius)
}

impl RenderContext {
    /// Collect text, SVG, and image elements from the render tree
    pub(super) fn collect_render_elements(
        &mut self,
        tree: &RenderTree,
    ) -> (Vec<TextElement>, Vec<SvgElement>, Vec<ImageElement>) {
        self.collect_render_elements_with_state(tree, None)
    }

    /// Collect text, SVG, and image elements with motion state
    pub(super) fn collect_render_elements_with_state(
        &mut self,
        tree: &RenderTree,
        render_state: Option<&blinc_layout::RenderState>,
    ) -> (Vec<TextElement>, Vec<SvgElement>, Vec<ImageElement>) {
        // One atlas epoch per walk, bumped before anything is drawn so
        // this walk's glyphs land in the new epoch. Reclaim first, while
        // nothing from this walk has been touched yet: a glyph the last
        // walk drew is one epoch old and safe.
        self.text_ctx.begin_atlas_epoch();
        if self.text_ctx.atlas_utilization() >= ATLAS_GC_PRESSURE
            && self.text_ctx.gc_atlas(ATLAS_GC_MAX_AGE)
        {
            // Reclaimed space is zeroed and reused, so any cached batch
            // holding baked PRIM_TEXT UVs into it would sample whatever
            // lands there next. This walk rebuilds them.
            self.renderer.invalidate_static_layer();
            self.overlay_invalidates_replay |= self.overlay_frame_active;
        }

        // Reuse scratch buffers - take them, clear, populate, and return
        // On next call they'll be reallocated if not returned
        let mut texts = std::mem::take(&mut self.scratch_texts);
        let mut svgs = std::mem::take(&mut self.scratch_svgs);
        let mut images = std::mem::take(&mut self.scratch_images);
        texts.clear();
        svgs.clear();
        images.clear();

        // Get the scale factor from the tree for DPI scaling
        let scale = tree.scale_factor();

        // Capture promoted-ancestor opacity for separately dispatched text, SVG and image
        // content.
        self.composite_patch_baselines.clear();
        let composite_layer_opacities: std::collections::HashMap<blinc_layout::LayoutNodeId, f32> = {
            let promo = tree.composite_promotion();
            let store = tree.css_anim_store();
            if let Ok(guard) = store.lock() {
                promo
                    .iter()
                    .filter_map(|&layout| {
                        let stable = tree.stable_id(layout)?;
                        let anim = guard
                            .animations
                            .get(&stable)
                            .or_else(|| guard.transitions.get(&stable))?;
                        if !anim.is_playing {
                            return None;
                        }
                        let opacity = anim.current_properties.opacity?;
                        // Save the promoted region's collect-time geometry for live text
                        // transforms.
                        if let Some(region) = tree.dynamic_regions().get(&layout) {
                            let p = &anim.current_properties;
                            self.composite_patch_baselines.insert(
                                stable,
                                (
                                    region.screen_aabb,
                                    (p.translate_x.unwrap_or(0.0), p.translate_y.unwrap_or(0.0)),
                                    (p.scale_x.unwrap_or(1.0), p.scale_y.unwrap_or(1.0)),
                                ),
                            );
                        }
                        Some((layout, opacity))
                    })
                    .collect()
            } else {
                std::collections::HashMap::new()
            }
        };

        if let Some(root) = tree.root() {
            let mut z_layer = 0u32;
            self.collect_elements_recursive(
                tree,
                root,
                (0.0, 0.0),
                false,      // inside_glass
                false,      // inside_foreground
                None,       // No initial clip bounds
                None,       // No initial clip radius
                1.0,        // Initial motion opacity
                (0.0, 0.0), // Initial motion translate offset
                (1.0, 1.0), // Initial motion scale
                None,       // No initial motion scale center
                render_state,
                scale,
                &mut z_layer,
                &mut texts,
                &mut svgs,
                &mut images,
                None,  // No initial CSS transform
                1.0,   // Initial inherited CSS opacity
                None,  // No inherited text colour at the root
                None,  // No parent node
                None,  // No initial scroll clip
                None,  // No 3D layer ancestor
                false, // No ancestor pending motion at root
                false, // No ancestor motion container at root
                &composite_layer_opacities,
                1.0,  // Initial composite-layer opacity (no ancestor promoted)
                None, // No composite-promoted ancestor at root
                None, // No inherited text path at root
                [0.0; 4],
                None, // No culling scroll container above the root
            );
        }

        // Sort texts by z_index (z_layer) to ensure correct rendering order with primitives
        texts.sort_by_key(|t| t.z_index);

        (texts, svgs, images)
    }

    #[allow(clippy::too_many_arguments, clippy::only_used_in_recursion)]
    pub(super) fn collect_elements_recursive(
        &self,
        tree: &RenderTree,
        node: LayoutNodeId,
        parent_offset: (f32, f32),
        inside_glass: bool,
        inside_foreground: bool,
        current_clip: Option<[f32; 4]>,
        current_clip_radius: Option<[f32; 4]>,
        inherited_motion_opacity: f32,
        inherited_motion_translate: (f32, f32),
        inherited_motion_scale: (f32, f32),
        // Center point for motion scale (in layout coordinates, before DPI scaling)
        // When a parent has motion scale, children should scale around the parent's center
        inherited_motion_scale_center: Option<(f32, f32)>,
        render_state: Option<&blinc_layout::RenderState>,
        scale: f32,
        z_layer: &mut u32,
        texts: &mut Vec<TextElement>,
        svgs: &mut Vec<SvgElement>,
        images: &mut Vec<ImageElement>,
        // Accumulated CSS transform from ancestors as a 6-element affine [a,b,c,d,tx,ty]
        // in layout coordinates. Maps pre-transform coords to post-transform visual coords.
        inherited_css_affine: Option<[f32; 6]>,
        // Accumulated CSS opacity from ancestors (compounds multiplicatively).
        // CSS `opacity` applies to the element and its entire visual subtree.
        inherited_css_opacity: f32,
        // Inherited text colour, resolved as the walk descends rather than
        // copied into props beforehand, so a subtree rebuilt after the
        // stylesheet pass still inherits. Mirrors `render_text_recursive`.
        inherited_text_color: Option<[f32; 4]>,
        // Parent node ID for inheriting non-cascading CSS props (border, shadow, filter)
        // to child images that render separately from the SDF pipeline.
        parent_node: Option<LayoutNodeId>,
        // Scroll container clip — sharp rect kept separate from the primary rounded clip.
        // This prevents corner radius morphing when a rounded element (card) is partially
        // scrolled past a sharp scroll boundary.
        current_scroll_clip: Option<[f32; 4]>,
        // 3D layer info if inside a perspective-transformed ancestor.
        // Text/SVGs/images inside 3D layers are rendered to offscreen textures
        // and blitted with the same perspective transform.
        inside_3d_layer: Option<Transform3DLayerInfo>,
        // Keep descendants during a pending opacity animation, including its zero endpoint.
        ancestor_pending_motion: bool,
        // Defer descendants of motion containers to the motion overlay streams.
        inside_motion_subtree: bool,
        // Live promoted-ancestor opacity for content dispatched separately from its baked
        // geometry.
        composite_layer_opacities: &std::collections::HashMap<blinc_layout::LayoutNodeId, f32>,
        // Inherited promoted opacity for images. Text and SVG use their promoted-ancestor
        // dispatch patch.
        inherited_composite_layer_opacity: f32,
        // Nearest composite-promoted ancestor with a playing CSS
        // animation, threaded down so text / SVG elements can record
        // it for the dispatch-time live patch.
        nearest_composite_ancestor: Option<blinc_layout::tree::StableNodeId>,
        inherited_text_path: Option<Arc<text_clip::CollectedTextPath>>,
        inherited_text_fade: [f32; 4],
        // The on-screen rect of the nearest culling scroll container, in the space of `bounds`;
        // `None` where culling is off.
        cull_view: Option<[f32; 4]>,
    ) {
        // Use animated bounds if this node has layout animation, otherwise use layout bounds
        // This ensures children are positioned correctly during layout animation transitions
        let Some(bounds) = tree.get_render_bounds(node, parent_offset) else {
            return;
        };

        let abs_x = bounds.x;
        let abs_y = bounds.y;

        // What this node set for itself wins; anything it did not set, it
        // inherits. This is what paints and what descends.
        let effective_text_color = tree
            .get_render_node(node)
            .and_then(|n| n.props.text_color)
            .or(inherited_text_color);

        // Get motion values for this node from RenderState (entry/exit animations)
        let motion_values = render_state.and_then(|rs| {
            // Try stable motion first, then node-based
            if let Some(render_node) = tree.get_render_node(node)
                && let Some(ref stable_key) = render_node.props.motion_stable_id
            {
                return rs.get_stable_motion_values(stable_key);
            }
            rs.get_motion_values(node)
        });

        // Translation is already included in the child offset; read only the remaining
        // binding transform here.
        let binding_scale = tree.get_motion_scale(node);
        let binding_opacity = tree.get_motion_opacity(node);
        let binding_rotation = tree
            .get_motion_rotation(node)
            .filter(|deg| deg.abs() > 1e-4);
        // The node's own binding translate; descendants are placed with it already applied.
        let own_translate = tree
            .get_motion_transform(node)
            .map(|t| match t {
                blinc_core::Transform::Affine2D(a) => (a.elements[4], a.elements[5]),
                _ => (0.0, 0.0),
            })
            .unwrap_or((0.0, 0.0));

        // Bindings and motion FSMs both make this node a motion container.
        let is_motion_container_node = tree.motion_bindings_map().contains_key(&node)
            || tree.is_scroll_row_node(node)
            || tree
                .get_render_node(node)
                .map(|n| n.props.motion.is_some() || n.props.is_overlay_root)
                .unwrap_or(false);
        let child_inside_motion_subtree = inside_motion_subtree || is_motion_container_node;

        // Calculate motion opacity for this node (combine both sources)
        let node_motion_opacity = motion_values
            .and_then(|m| m.opacity)
            .unwrap_or_else(|| binding_opacity.unwrap_or(1.0));

        // Get motion translate for this node from RenderState only
        // (binding translate is handled via new_offset in recursive calls)
        let node_motion_translate = motion_values
            .map(|m| m.resolved_translate())
            .unwrap_or((0.0, 0.0));

        // Get motion scale for this node from RenderState
        let node_motion_scale = motion_values
            .map(|m| m.resolved_scale())
            .unwrap_or((1.0, 1.0));

        // Combine with binding scale
        let binding_scale_values = binding_scale.unwrap_or((1.0, 1.0));

        // Combine with inherited values
        // NOTE: effective_motion_translate only includes RenderState motion values,
        // NOT binding transforms (which are already in the position via new_offset)
        let effective_motion_opacity = inherited_motion_opacity * node_motion_opacity;
        // Accumulate promoted opacity for separately dispatched descendants.
        let node_composite_layer_opacity =
            composite_layer_opacities.get(&node).copied().unwrap_or(1.0);
        let effective_composite_layer_opacity =
            inherited_composite_layer_opacity * node_composite_layer_opacity;
        // Record the nearest promoted ancestor for live text and SVG patches.
        let nearest_composite_ancestor = if composite_layer_opacities.contains_key(&node) {
            tree.stable_id(node).or(nearest_composite_ancestor)
        } else {
            nearest_composite_ancestor
        };
        let effective_motion_translate = (
            inherited_motion_translate.0 + node_motion_translate.0,
            inherited_motion_translate.1 + node_motion_translate.1,
        );
        // Scale compounds multiplicatively (including binding scale)
        let effective_motion_scale = (
            inherited_motion_scale.0 * node_motion_scale.0 * binding_scale_values.0,
            inherited_motion_scale.1 * node_motion_scale.1 * binding_scale_values.1,
        );

        // Determine the motion scale center for children
        // If this node has motion scale (from RenderState or binding), use its center as the scale center
        // Otherwise, inherit the parent's scale center
        // The same bound as the one that decides a text affine has a scale at all (1e-6): a scale
        // between the two used to scale text about the origin.
        let this_node_has_scale = (node_motion_scale.0 - 1.0).abs() > 1e-6
            || (node_motion_scale.1 - 1.0).abs() > 1e-6
            || (binding_scale_values.0 - 1.0).abs() > 1e-6
            || (binding_scale_values.1 - 1.0).abs() > 1e-6;

        let effective_motion_scale_center = if this_node_has_scale {
            // This node has motion scale - compute its center in absolute layout coordinates
            let center_x = abs_x + own_translate.0 + bounds.width / 2.0;
            let center_y = abs_y + own_translate.1 + bounds.height / 2.0;
            Some((center_x, center_y))
        } else {
            // No scale on this node - inherit the parent's scale center
            inherited_motion_scale_center
        };

        // Keep transparent nodes while motion can make them visible, matching primitive
        // emission.
        let has_pending_motion = tree
            .motion_bindings_map()
            .get(&node)
            .is_some_and(|b| b.is_any_animating())
            || render_state.is_some_and(|rs| {
                if let Some(render_node) = tree.get_render_node(node)
                    && let Some(ref stable_key) = render_node.props.motion_stable_id
                {
                    return rs.is_stable_motion_active(stable_key);
                }
                rs.is_motion_active(node)
            });
        // Propagate pending motion so zero-opacity descendants remain available for later
        // frames.
        let subtree_pending_motion = ancestor_pending_motion || has_pending_motion;
        if effective_motion_opacity <= 0.001 && !subtree_pending_motion {
            return;
        }

        // CSS visibility: hidden — skip rendering but preserve layout space
        if let Some(render_node) = tree.get_render_node(node)
            && (!render_node.props.visible || tree.layout_tree.is_display_none(node))
        {
            return;
        }

        // Determine if this node is a glass element
        let is_glass = tree
            .get_render_node(node)
            .map(|n| n.props.is_glass())
            .unwrap_or(false);

        // Track if children should be considered inside glass
        let children_inside_glass = inside_glass || is_glass;

        // Track if we're inside a foreground-layer element
        let is_foreground_node = tree
            .get_render_node(node)
            .map(|n| n.props.layer == RenderLayer::Foreground)
            .unwrap_or(false);
        let children_inside_foreground = inside_foreground || is_foreground_node;

        // Check if this node clips its children (e.g., scroll containers)
        let clips_content = tree
            .get_render_node(node)
            .map(|n| n.props.clips_content)
            .unwrap_or(false);

        // Check if this node has an active layout animation (also needs clipping)
        // Layout animations need to clip children to animated bounds
        let has_layout_animation = tree.is_layout_animating(node);

        // Check if this is a Stack layer - if so, increment z_layer for proper z-ordering
        let is_stack_layer = tree
            .get_render_node(node)
            .map(|n| n.props.is_stack_layer)
            .unwrap_or(false);
        if is_stack_layer {
            *z_layer += 1;
        }

        // Apply CSS z-index to z_layer for stacking order
        let saved_z_layer = *z_layer;
        let node_z_index = tree
            .get_render_node(node)
            .map(|n| n.props.z_index)
            .unwrap_or(0);
        if node_z_index > 0 {
            *z_layer = node_z_index as u32;
        }

        // Update clip bounds for children if this node clips (either via clips_content or layout animation)
        // When a node clips, we INTERSECT its bounds with any existing clip
        // This ensures nested clipping works correctly (inner clips can't expand outer clips)
        let should_clip = clips_content || has_layout_animation;
        let (child_clip, child_clip_radius, child_scroll_clip) = if should_clip {
            // For layout animation, use animated bounds for clipping
            // This ensures content is clipped to the animating size during transition
            let clip_bounds = if has_layout_animation {
                // Get animated bounds - these are the interpolated bounds during animation
                tree.get_render_bounds(node, parent_offset)
                    .map(|b| [b.x, b.y, b.width, b.height])
                    .unwrap_or([abs_x, abs_y, bounds.width, bounds.height])
            } else {
                [abs_x, abs_y, bounds.width, bounds.height]
            };
            // Inset clip by border-width only.  Per CSS spec, overflow clips
            // at the padding box (inside border, but padding area is visible).
            // Padding affects layout positioning, not clipping.
            let bw = tree
                .get_render_node(node)
                .map(|n| n.props.border_width)
                .unwrap_or(0.0);
            let this_clip = [
                clip_bounds[0] + bw,
                clip_bounds[1] + bw,
                (clip_bounds[2] - bw * 2.0).max(0.0),
                (clip_bounds[3] - bw * 2.0).max(0.0),
            ];

            // Extract border radius from this node for rounded clipping.
            // Inner corner radius = max(outer_radius − border_width, 0)
            let this_clip_radius = tree.get_render_node(node).map(|n| {
                let r = &n.props.border_radius;
                [
                    (r.top_left - bw).max(0.0),
                    (r.top_right - bw).max(0.0),
                    (r.bottom_right - bw).max(0.0),
                    (r.bottom_left - bw).max(0.0),
                ]
            });

            let this_has_radius = this_clip_radius
                .map(|r| r.iter().any(|&v| v > 0.5))
                .unwrap_or(false);

            // A sharp scroll container's clip is the scroll clip, kept apart from the primary
            // clip so a rounded card it cuts keeps its corners, and so content moved inside the
            // viewport is cut where the viewport stays: the outermost scroll container's, and that
            // of one whose content rows move. Any other clip, a nested scroll container's
            // included, goes into the primary clip, which moves with the content around it.
            if !this_has_radius
                && tree.is_scroll_container(node)
                && (current_scroll_clip.is_none() || tree.is_scroll_row_node(node))
            {
                (
                    current_clip,
                    current_clip_radius,
                    merge_scroll_clip(this_clip, current_scroll_clip),
                )
            } else if let Some(parent_clip) = current_clip {
                let (clip, radius) = intersect_rounded(
                    parent_clip,
                    current_clip_radius.unwrap_or([0.0; 4]),
                    this_clip,
                    this_clip_radius.unwrap_or([0.0; 4]),
                );
                (Some(clip), Some(radius), current_scroll_clip)
            } else {
                (Some(this_clip), this_clip_radius, current_scroll_clip)
            }
        } else {
            (current_clip, current_clip_radius, current_scroll_clip)
        };

        // Compose the node's transform (CSS transform, then binding rotation) before collecting
        // its own content. Descendants are placed with the node's binding translate applied, so
        // their copy pivots about the translated centre.
        let compose_transform = |shift: (f32, f32)| -> Option<[f32; 6]> {
            let (x, y) = (abs_x + shift.0, abs_y + shift.1);
            let css = tree
                .get_render_node(node)
                .and_then(|rn| css_transform_affine(&rn.props, x, y, bounds.width, bounds.height));
            let rotation = binding_rotation.map(|deg| {
                rotation_about(
                    deg.to_radians(),
                    x + bounds.width / 2.0,
                    y + bounds.height / 2.0,
                )
            });
            let own = match (rotation, css) {
                (Some(r), Some(c)) => Some(r.then(&c)),
                (r, c) => r.or(c),
            };
            match (inherited_css_affine, own) {
                (Some(parent), Some(own)) => {
                    Some(Affine2D { elements: parent }.then(&own).elements)
                }
                (parent, None) => parent,
                (None, Some(own)) => Some(own.elements),
            }
        };
        let node_css_affine = compose_transform((0.0, 0.0));
        let child_css_affine = if own_translate != (0.0, 0.0) {
            compose_transform(own_translate)
        } else {
            node_css_affine
        };

        let text_path = tree
            .get_render_node(node)
            .and_then(|n| n.props.clip_path.as_ref())
            .and_then(|clip| {
                text_clip::CollectedTextPath::capture(
                    clip,
                    [abs_x, abs_y, bounds.width, bounds.height],
                    node_css_affine,
                    effective_motion_scale,
                    effective_motion_translate,
                    effective_motion_scale_center,
                    scale,
                    if tree.get_render_node(node).is_some_and(|n| {
                        n.props.rotate_x.is_some()
                            || n.props.rotate_y.is_some()
                            || n.props.perspective.is_some()
                    }) {
                        Some(node)
                    } else {
                        inside_3d_layer.as_ref().map(|info| info.node_id)
                    },
                )
            })
            .or(inherited_text_path);

        self.collect_node_content(
            NodeContent {
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
                text_fade: inherited_text_fade,
                effective_text_color,
                effective_motion_opacity,
                inherited_css_opacity,
                effective_composite_layer_opacity,
                nearest_composite_ancestor,
                text_path: &text_path,
                inside_3d_layer: &inside_3d_layer,
                children_inside_foreground,
                inside_motion_subtree,
                parent_node,
                z_layer,
            },
            texts,
            svgs,
            images,
        );

        // Include scroll offset and motion offset when calculating child positions
        let scroll_offset = tree.get_scroll_offset(node);
        let new_offset = (
            abs_x + scroll_offset.0 + own_translate.0,
            abs_y + scroll_offset.1 + own_translate.1,
        );

        // Compute inherited CSS opacity for children: compound this node's CSS opacity
        // CSS `opacity` applies to the element AND its visual subtree
        let child_css_opacity = if let Some(rn) = tree.get_render_node(node) {
            inherited_css_opacity * rn.props.opacity
        } else {
            inherited_css_opacity
        };

        // Detect 3D layer: if this node has rotate-x/rotate-y/perspective,
        // create a Transform3DLayerInfo for children to inherit.
        let child_3d_layer = if let Some(rn) = tree.get_render_node(node) {
            let has_3d = rn.props.rotate_x.is_some()
                || rn.props.rotate_y.is_some()
                || rn.props.perspective.is_some();
            if has_3d {
                let rx = rn.props.rotate_x.unwrap_or(0.0).to_radians();
                let ry = rn.props.rotate_y.unwrap_or(0.0).to_radians();
                let d = rn.props.perspective.unwrap_or(800.0) * scale;
                Some(Transform3DLayerInfo {
                    node_id: node,
                    layer_bounds: [
                        abs_x * scale,
                        abs_y * scale,
                        bounds.width * scale,
                        bounds.height * scale,
                    ],
                    transform_3d: blinc_core::Transform3DParams {
                        sin_rx: rx.sin(),
                        cos_rx: rx.cos(),
                        sin_ry: ry.sin(),
                        cos_ry: ry.cos(),
                        perspective_d: d,
                    },
                    opacity: rn.props.opacity,
                })
            } else {
                inside_3d_layer.clone()
            }
        } else {
            inside_3d_layer.clone()
        };

        // Culling follows the paint walk: inside a culling scroll container, a child that draws
        // wholly outside its rect is skipped. Content placed by a CSS or motion transform or a
        // 3D layer is positioned apart from `bounds`, so culling stops there.
        let positioned_apart = node_css_affine.is_some()
            || child_3d_layer.is_some()
            || effective_motion_translate != (0.0, 0.0)
            || effective_motion_scale != (1.0, 1.0)
            || tree.moves_its_content(node);
        let children_cull_view = if tree.is_viewport_cull_scroll(node) && !positioned_apart {
            Some([abs_x, abs_y, bounds.width, bounds.height])
        } else if positioned_apart {
            None
        } else {
            cull_view
        };
        // Children sit at their layout position moved by `new_offset`'s scroll and motion.
        let cull_shift = tree
            .layout()
            .get_absolute_bounds(node)
            .map(|a| (new_offset.0 - a.x, new_offset.1 - a.y));
        // A clip with no area shows nothing beneath it (a closed collapsible, say).
        let clip_is_empty = should_clip && (bounds.width <= 0.0 || bounds.height <= 0.0);
        // A container on rows was painted as a window wider than its viewport, and is collected
        // for the same window.
        let previous_overscan = tree
            .scroll_row_overscan(node)
            .map(|overscan| tree.set_cull_overscan(overscan));
        let children = if clip_is_empty {
            Vec::new()
        } else if let (Some(view), Some(shift)) = (children_cull_view, cull_shift) {
            tree.visible_children(node, shift, view)
        } else {
            tree.layout().children(node)
        };
        for child_id in children {
            self.collect_elements_recursive(
                tree,
                child_id,
                new_offset,
                children_inside_glass,
                children_inside_foreground,
                child_clip,
                child_clip_radius,
                effective_motion_opacity,
                effective_motion_translate,
                effective_motion_scale,
                effective_motion_scale_center,
                render_state,
                scale,
                z_layer,
                texts,
                svgs,
                images,
                child_css_affine,
                child_css_opacity,
                effective_text_color,
                Some(node), // pass current node as parent for children
                child_scroll_clip,
                child_3d_layer.clone(),
                subtree_pending_motion,
                child_inside_motion_subtree,
                composite_layer_opacities,
                effective_composite_layer_opacity,
                nearest_composite_ancestor,
                text_path.clone(),
                tree.get_render_node(node)
                    .filter(|n| should_clip && !n.props.overflow_fade.is_none())
                    .map_or(inherited_text_fade, |n| n.props.overflow_fade.to_array()),
                children_cull_view,
            );
        }

        if let Some(previous) = previous_overscan {
            tree.set_cull_overscan(previous);
        }

        // Restore z_layer after this subtree
        if node_z_index > 0 {
            *z_layer = saved_z_layer;
        }
    }
}

/// The node's CSS transform as a layout-space affine about its transform origin, or `None`
/// when it is the identity.
fn css_transform_affine(
    props: &blinc_layout::element::RenderProps,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) -> Option<Affine2D> {
    let Some(blinc_core::Transform::Affine2D(affine)) = &props.transform else {
        return None;
    };
    let [a, b, c, d, tx, ty] = affine.elements;
    let identity = (a - 1.0).abs() < 0.0001
        && b.abs() < 0.0001
        && c.abs() < 0.0001
        && (d - 1.0).abs() < 0.0001
        && tx.abs() < 0.0001
        && ty.abs() < 0.0001;
    if identity {
        return None;
    }
    let (cx, cy) = match props.transform_origin {
        Some([ox_pct, oy_pct]) => (x + width * ox_pct / 100.0, y + height * oy_pct / 100.0),
        None => (x + width / 2.0, y + height / 2.0),
    };
    // T(center) * [a b c d tx ty] * T(-center)
    Some(Affine2D {
        elements: [
            a,
            b,
            c,
            d,
            cx * (1.0 - a) - cy * c + tx,
            cy * (1.0 - d) - cx * b + ty,
        ],
    })
}

/// Rotation by `radians` about `(cx, cy)`.
fn rotation_about(radians: f32, cx: f32, cy: f32) -> Affine2D {
    let (sin, cos) = radians.sin_cos();
    Affine2D {
        elements: [
            cos,
            sin,
            -sin,
            cos,
            cx * (1.0 - cos) + cy * sin,
            cy * (1.0 - cos) - cx * sin,
        ],
    }
}
