//! The SDF core shader: rects, circles and ellipses, with borders, shadows, gradients,
//! masks, CSS filters, clips and CSS perspective. The `FEAT_*` pipeline constants switch the
//! optional blocks; `simple` turns off the ones a plain solid shape never uses. `TIER` builds it
//! for the other device tiers.

russl::russl! {
    use super::common::{sd_circle, sd_ellipse, sd_shaped_rect, quarter_ellipse_sdf, erf, apply_css_filter, border_side_color};
    use super::access::{UNIFORMS, GLYPH_ATLAS, GLYPH_SAMPLER, COLOR_GLYPH_ATLAS, prim};
    use super::aux::aux;
    use super::clip::{calculate_clip_alpha, calculate_polygon_clip_alpha, gradient_table_color, abi_shape_clip_alpha};

    // The device tier: 0 reads storage buffers and draws mesh triangles through the vertex stage;
    // 1 and 2 have no storage buffer in the vertex stage (instance fields come from a vertex
    // buffer) and 2 reads data textures; both test the mesh triangle per fragment instead.
    #[pipeline] const TIER: u32 = 0;

    // ============================================================================
    // Blinc SDF Core Shader — Rect, Circle, Ellipse
    // ============================================================================
    // Handles prim_type 0 (Rect), 1 (Circle), 2 (Ellipse) with full
    // anti-aliasing, borders, shadows-behind-shape, gradients, mask
    // gradients, CSS filters, clip paths, and CSS perspective.

    struct VertexOutput {
        #[builtin(position)] position: Vec4,
        #[location(0)] uv: Vec2,
        #[location(1)] #[flat] instance_index: u32,
        #if TIER == 0
        // Per-vertex barycentric weights; PRIM_MESH vertex stage emits
        // (1,0,0), (0,1,0), (0,0,1) for triangle corners 0/1/2 so the
        // hardware-interpolated value at any fragment tells us how far
        // that fragment is from each of the triangle's three edges. The
        // fragment shader then feathers alpha at silhouette edges only.
        #[location(2)] mesh_bary: Vec3,
        #endif
    }


    // Enum values used throughout this shader. They're inlined as
    // literals at every use site rather than declared as `const` at
    // module scope so naga's WGSL→MSL backend doesn't emit orphaned
    // `constant uint NAME = <value>;` declarations that trip the Metal
    // shader compiler's `-Wunused-const-variable` pass at runtime.
    // naga constant-folds every reference, so the const symbols would
    // have no surviving users in the generated MSL source.
    //
    // Primitive types (field: type_info.x, variable: prim_type)
    //   0u32 = RECT                 1u32 = CIRCLE              2u32 = ELLIPSE
    //
    // Fill types (field: type_info.y, variable: fill_type)
    //   0u32 = SOLID                1u32 = LINEAR_GRADIENT     2u32 = RADIAL_GRADIENT
    //
    // Clip types (field: type_info.z, variable: clip_type)
    //   0u32 = NONE                 1u32 = RECT                2u32 = CIRCLE
    //   3u32 = ELLIPSE              4u32 = POLYGON


    // Feature switches, set per pipeline. The default pipeline keeps every feature; the simple
    // pipeline turns off the ones a plain solid shape never uses, so the compiler does not size
    // the shader's registers for them. Which PRIMITIVES take it is `GpuPrimitive::is_simple_core`.
    #[pipeline] const FEAT_GRADIENT: bool = true;
    #[pipeline] const FEAT_BORDER: bool = true;
    #[pipeline] const FEAT_MASK: bool = true;
    #[pipeline] const FEAT_FILTER: bool = true;
    #[pipeline] const FEAT_SHADOW: bool = true;
    #[pipeline] const FEAT_3D: bool = true;
    #[pipeline] const FEAT_CLIP_SHAPE: bool = true;


    // ============================================================================
    // Vertex Shader
    // ============================================================================

    #[vertex]
    fn vs_main(
        #[builtin(vertex_index)] vertex_index: u32,
        #[builtin(instance_index)] instance_index: u32,
        #if TIER >= 1
            #[location(0)] instance_bounds: Vec4,
            #[location(1)] instance_shadow: Vec4,
            #[location(2)] instance_rotation: Vec4,
            #[location(3)] instance_perspective: Vec4,
            #[location(4)] instance_sdf_3d: Vec4,
            #[location(5)] instance_local_affine: Vec4,
        #endif
    ) -> VertexOutput {
        let mut out: VertexOutput;

        #if TIER == 0
            let p = prim(instance_index);
            let instance_bounds = p.bounds;
            let instance_shadow = p.shadow;
            let instance_rotation = p.rotation;
            let instance_perspective = p.perspective;
            let instance_sdf_3d = p.sdf_3d;
            let instance_local_affine = p.local_affine;
        #endif

        // Expand bounds for shadow blur
        let blur_expand = instance_shadow.z * 3.0 + abs(instance_shadow.x) + abs(instance_shadow.y);

        // Check for rotation, skew, and 3D transforms.
        let sin_rz = instance_rotation.x;
        let cos_rz = instance_rotation.y;
        let sin_ry = instance_rotation.z;
        let cos_ry = instance_rotation.w;
        let sin_rx = instance_perspective.x;
        let cos_rx = instance_perspective.y;
        let persp_d = instance_perspective.z;
        let la = instance_local_affine; // [a, b, c, d] of normalized 2x2 affine
        let has_3d = abs(sin_ry) > 0.0001 || abs(sin_rx) > 0.0001 || persp_d > 0.001;
        // Check if local_affine is non-identity (rotation, skew, or non-uniform scale)
        let has_local_affine = abs(la.x - 1.0) > 0.0001 || abs(la.y) > 0.0001
                            || abs(la.z) > 0.0001 || abs(la.w - 1.0) > 0.0001;

        let mut bounds: Vec4;
        if has_3d {
            // 3D perspective: project all 8 corners of the 3D bounding box to find AABB
            let ctr = instance_bounds.xy + instance_bounds.zw * 0.5;
            let half = instance_bounds.zw * 0.5;
            let half_d = instance_sdf_3d.x * 0.5; // half-depth
            let corners3d = [
                Vec3::new(-half.x, -half.y, -half_d),
                Vec3::new( half.x, -half.y, -half_d),
                Vec3::new( half.x,  half.y, -half_d),
                Vec3::new(-half.x,  half.y, -half_d),
                Vec3::new(-half.x, -half.y,  half_d),
                Vec3::new( half.x, -half.y,  half_d),
                Vec3::new( half.x,  half.y,  half_d),
                Vec3::new(-half.x,  half.y,  half_d),
            ];
            let mut min_p = Vec2::splat(1e10);
            let mut max_p = Vec2::splat(-1e10);
            let pd = if persp_d > 0.001 { persp_d } else { 800.0 };
            for i in 0u32..8u32 {
                let c = corners3d[i];
                // Apply rotateZ
                let rz_x = c.x * cos_rz - c.y * sin_rz;
                let rz_y = c.x * sin_rz + c.y * cos_rz;
                let rz_z = c.z;
                // Apply rotateX (tilt Y/Z)
                let rx_y = rz_y * cos_rx - rz_z * sin_rx;
                let rx_z = rz_y * sin_rx + rz_z * cos_rx;
                // Apply rotateY (tilt X/Z)
                let ry_x = rz_x * cos_ry + rx_z * sin_ry;
                let ry_z = -rz_x * sin_ry + rx_z * cos_ry;
                // Perspective divide
                let w = 1.0 - ry_z / pd;
                let proj = Vec2::new(ry_x, rx_y) / max(w, 0.001);
                min_p = min(min_p, proj);
                max_p = max(max_p, proj);
            }
            min_p -= Vec2::splat(blur_expand + 2.0);
            max_p += Vec2::splat(blur_expand + 2.0);
            bounds = Vec4::new(ctr + min_p, max_p - min_p);
        } else if has_local_affine {
            // General 2D affine (rotation, skew, non-uniform scale):
            // Transform the 4 corners of the local rect by the local_affine to find AABB
            let center = instance_bounds.xy + instance_bounds.zw * 0.5;
            let hw = instance_bounds.z * 0.5;
            let hh = instance_bounds.w * 0.5;
            // Transform corners: la * (±hw, ±hh)
            // new_x = la.x * cx + la.z * cy, new_y = la.y * cx + la.w * cy
            let c0x = la.x * hw + la.z * hh;
            let c0y = la.y * hw + la.w * hh;
            let c1x = -la.x * hw + la.z * hh;
            let c1y = -la.y * hw + la.w * hh;
            let aabb_hw = max(abs(c0x), abs(c1x)) + blur_expand;
            let aabb_hh = max(abs(c0y), abs(c1y)) + blur_expand;
            bounds = Vec4::new(center.x - aabb_hw, center.y - aabb_hh, aabb_hw * 2.0, aabb_hh * 2.0);
        } else {
            bounds = Vec4::new(
                instance_bounds.x - blur_expand,
                instance_bounds.y - blur_expand,
                instance_bounds.z + blur_expand * 2.0,
                instance_bounds.w + blur_expand * 2.0
            );
        }

        // Generate quad vertices (two triangles split along / diagonal)
        // 0--1
        // |\ |
        // | \|
        // 3--2
        // Triangle 1: 0 → 1 → 3 (TL → TR → BL) - upper-left triangle
        // Triangle 2: 1 → 2 → 3 (TR → BR → BL) - lower-right triangle
        // Shared edge: 1-3 (top-right to bottom-left = / diagonal)
        //
        // PowerVR Vulkan codegen bug workaround: dynamic indexing into a
        // `let [literal]` produces an `OpConstantComposite` +
        // `OpAccessChain` pattern that the Pixel 10 Pro / Tensor G5
        // PowerVR driver compiles incorrectly — vertex_index 0..2 work
        // but 3..5 silently produce degenerate output, collapsing the
        // second triangle to a point and leaving every primitive a
        // half-quad. Replacing the array-literal indexing with an explicit
        // `match` forces naga to emit `OpSwitch`, which the driver
        // handles correctly. Confirmed on Android 16 / driver 25.1@6794074.
        let mut uv: Vec2;
        match vertex_index {
            0u32 => { uv = Vec2::new(0.0, 0.0); } // 0 - top-left
            1u32 => { uv = Vec2::new(1.0, 0.0); } // 1 - top-right
            2u32 => { uv = Vec2::new(0.0, 1.0); } // 3 - bottom-left
            3u32 => { uv = Vec2::new(1.0, 0.0); } // 1 - top-right
            4u32 => { uv = Vec2::new(1.0, 1.0); } // 2 - bottom-right
            _ => { uv = Vec2::new(0.0, 1.0); } // 3 - bottom-left
        }
        let mut pos = Vec2::new(
            bounds.x + uv.x * bounds.z,
            bounds.y + uv.y * bounds.w
        );

        #if TIER == 0
        // PRIM_MESH replaces the quad geometry with the triangle's
        // three corners pulled from AUX_DATA. Vertex indices 0..2
        // emit the real triangle; 3..5 collapse onto vertex 0 so
        // the second triangle of the underlying 6-vertex draw
        // degenerates to zero area and the rasteriser skips it.
        // Hardware rasterises the real triangle edge-to-edge, which
        // is the point of the SDF-stream rework: the fragment shader
        // no longer does a per-pixel triangle walk.
        //
        // `mesh_bary` emits (1,0,0), (0,1,0), (0,0,1) at the three
        // corners; hardware interpolates linearly across the
        // triangle so the fragment's `mesh_bary.x` == 0 exactly on
        // edge v1→v2, `mesh_bary.y` == 0 on edge v2→v0, and
        // `mesh_bary.z` == 0 on edge v0→v1. Paired with the per-edge
        // silhouette flags in `prim.corner_radius`, the fragment
        // shader can feather alpha at outer edges only and leave
        // interior tessellation seams fully opaque.
        let mut mesh_bary = Vec3::splat(0.0);
        if p.type_info.x == 9u32 {
            let aux_off = ((p.border.z) as u32);
            let pack0 = aux(aux_off);
            let pack1 = aux(aux_off + 1u32);
            match vertex_index {
                0u32 => { pos = pack0.xy; mesh_bary = Vec3::new(1.0, 0.0, 0.0); }
                1u32 => { pos = pack0.zw; mesh_bary = Vec3::new(0.0, 1.0, 0.0); }
                2u32 => { pos = pack1.xy; mesh_bary = Vec3::new(0.0, 0.0, 1.0); }
                _ => { pos = pack0.xy; mesh_bary = Vec3::new(1.0, 0.0, 0.0); }
            }
        }
        out.mesh_bary = mesh_bary;
        #endif

        // Convert to clip space (-1 to 1)
        let clip_pos = Vec2::new(
            (pos.x / UNIFORMS.viewport_size.x) * 2.0 - 1.0,
            1.0 - (pos.y / UNIFORMS.viewport_size.y) * 2.0
        );

        out.position = Vec4::new(clip_pos, 0.0, 1.0);
        out.uv = pos; // Pass world position for SDF calculation
        out.instance_index = instance_index;

        return out;
    }


    // ============================================================================
    // Fragment Shader
    // ============================================================================
    #[fragment]
    #[location(0)]
    fn fs_main(input: VertexOutput) -> Vec4 {
        let prim = prim(input.instance_index);
        let p = input.uv;

        #if TIER == 0
        // Mesh-triangle barycentric derivatives, read only by `prim_type == 9` below. WGSL
        // requires derivatives in uniform control flow, so they are taken here, before any
        // `discard!()` or early return that depends on the non-uniform `prim_type` or
        // `clip_alpha`. See https://www.w3.org/TR/WGSL/#uniformity.
        let mb_uniform = input.mesh_bary;
        let mb_fw_uniform = fwidth(mb_uniform);
        #endif

        let prim_type = prim.type_info.x;
        let fill_type = prim.type_info.y;
        let clip_type = prim.type_info.z;

        // Early type filter — discard PRIMITIVES handled by other split pipelines.
        // Allow prim_type 7 (text) — transformed text glyphs are rendered through
        // the SDF pipeline with css_affine, not the separate text pipeline.
        // Allow prim_type 9 (mesh triangle) — tessellated path fills routed
        // through the SDF stream so their submission order interleaves with
        // text in the same draw dispatch.
        if prim_type > 2u32 && prim_type != 7u32 && prim_type != 9u32 { discard!(); }

        // Early clip test - discard if completely outside clip region (screen space).
        // For CLIP_POLYGON the scissor-AABB portion runs here; the polygon's
        // winding-number shape test is deferred to after `sp` is computed
        // below so the polygon vertices (stored in element-local coords) can
        // be tested in the same frame the SDF evaluates — which means clips
        // rotate with motion-binding / CSS rotation automatically.
        let mut clip_alpha = calculate_clip_alpha(p, prim.clip_bounds, prim.clip_radius, prim.clip_corner_shape, clip_type, prim.clip_fade, prim.clip_fade_bounds);
        if FEAT_CLIP_SHAPE {
            clip_alpha = clip_alpha * abi_shape_clip_alpha(p, prim.clip_shape.x);
        }
        if clip_alpha < 0.001 {
            discard!();
        }

        let origin = prim.bounds.xy;
        let size = prim.bounds.zw;
        let center = origin + size * 0.5;

        // Extract rotation and perspective parameters
        let sin_rz = prim.rotation.x;
        let cos_rz = prim.rotation.y;
        let sin_ry = prim.rotation.z;
        let cos_ry = prim.rotation.w;
        let sin_rx = prim.perspective.x;
        let cos_rx = prim.perspective.y;
        let persp_d = prim.perspective.z;

        let has_3d = abs(sin_ry) > 0.0001 || abs(sin_rx) > 0.0001 || persp_d > 0.001;

        // ── Perspective Unprojection (flat elements with 3D perspective) ──
        let mut sp = p;
        if FEAT_3D && has_3d {
            let pd = if persp_d > 0.001 { persp_d } else { 800.0 };
            let rel = p - center;

            // Inverse homography: map screen point back to element local coords
            let safe_cos_ry = max(abs(cos_ry), 0.0001) * sign(cos_ry + 0.0001);
            let safe_cos_rx = max(abs(cos_rx), 0.0001) * sign(cos_rx + 0.0001);
            let tan_ry = sin_ry / safe_cos_ry;
            let tan_rx = sin_rx / safe_cos_rx;

            let u = rel.x * cos_rz / safe_cos_ry + rel.y * (-cos_rz * tan_rx * tan_ry + sin_rz / safe_cos_rx);
            let v = rel.x * (-sin_rz) / safe_cos_ry + rel.y * (sin_rz * tan_rx * tan_ry + cos_rz / safe_cos_rx);
            let w = 1.0 - rel.x * tan_ry / pd + rel.y * tan_rx / (pd * safe_cos_ry);

            let safe_w = max(abs(w), 0.001) * sign(w + 0.001);
            sp = Vec2::new(u / safe_w, v / safe_w) + center;
        } else {
            // 2D affine (rotation, skew, non-uniform scale) via inverse local_affine
            let la = prim.local_affine;
            let is_identity = abs(la.x - 1.0) < 0.0001 && abs(la.y) < 0.0001
                           && abs(la.z) < 0.0001 && abs(la.w - 1.0) < 0.0001;
            if !is_identity {
                let rel = p - center;
                // Compute inverse of 2x2 [a,b; c,d]: inv = [d,-b; -c,a] / det
                let det = la.x * la.w - la.y * la.z;
                let inv_det = if det >= 0.0 { 1.0 } else { -1.0 } / max(abs(det), 0.0001);
                let inv_a = la.w * inv_det;
                let inv_b = -la.y * inv_det;
                let inv_c = -la.z * inv_det;
                let inv_d = la.x * inv_det;
                sp = Vec2::new(inv_a * rel.x + inv_c * rel.y, inv_b * rel.x + inv_d * rel.y) + center;
            }
        }

        // CLIP_POLYGON shape test in element-local coords. Polygon vertices
        // are stored in AUX_DATA as supplied by the user (0..size range); the
        // walker no longer pre-transforms them to screen space. Using
        // `sp - prim.bounds.xy` puts the sample point in the same frame, so
        // any rotation (CSS, motion-binding timeline, motion-binding rotation
        // updated via apply_binding_deltas) naturally rotates the polygon.
        if FEAT_CLIP_SHAPE && clip_type == 4u32 {
            let vertex_count = ((prim.clip_radius.z) as u32);
            let aux_offset = ((prim.clip_radius.w) as u32);
            let local_p = sp - prim.bounds.xy;
            let shape_alpha = calculate_polygon_clip_alpha(local_p, vertex_count, aux_offset, prim.clip_radius.x > 0.5);
            clip_alpha = clip_alpha * shape_alpha;
            if clip_alpha < 0.001 {
                discard!();
            }
        }

        // A clip in the primitive's own frame. `sp` has been taken back through any
        // transform, so a turned parent clips its children to its turned, rounded box.
        if FEAT_CLIP_SHAPE && prim.local_clip_bounds.z > 0.0 {
            let local_d = sd_shaped_rect(
                sp - origin,
                prim.local_clip_bounds.xy,
                prim.local_clip_bounds.zw,
                prim.local_clip_radius,
                prim.clip_corner_shape
            );
            clip_alpha = clip_alpha * (1.0 - smoothstep(-0.75, 0.75, local_d));
            if clip_alpha < 0.001 {
                discard!();
            }
        }

        let mut result = Vec4::splat(0.0);

        // Calculate shadow first (rendered behind)
        if FEAT_SHADOW && (prim.shadow.z > 0.0 || prim.shadow.w != 0.0) {
            let shadow_offset = prim.shadow.xy;
            let blur = prim.shadow.z;
            let spread = prim.shadow.w;

            let shadow_origin = origin + shadow_offset - Vec2::splat(spread);
            let shadow_size = size + Vec2::splat(spread * 2.0);

            // Adjust corner radii for spread (expand corners proportionally)
            let shadow_radii = prim.corner_radius + Vec4::splat(spread);

            let mut shadow_sdf_dist: f32;
            shadow_sdf_dist = sd_shaped_rect(sp, shadow_origin, shadow_size, shadow_radii, prim.corner_shape);
            let mut shadow_alpha: f32;
            if blur < 0.001 {
                shadow_alpha = if shadow_sdf_dist < 0.0 { 1.0 } else { 0.0 };
            } else {
                let sigma_d = 0.5 * sqrt(2.0) * blur;
                shadow_alpha = 0.5 * (1.0 + erf(-shadow_sdf_dist / sigma_d));
            }

            let shadow_color = prim.shadow_color * shadow_alpha;

            // Premultiply and blend
            result = shadow_color;
        }

        // Calculate main shape SDF
        let mut d: f32;
        match prim_type {
            0u32 /* PRIM_RECT */ => {
                d = sd_shaped_rect(sp, origin, size, prim.corner_radius, prim.corner_shape);
            }
            1u32 /* PRIM_CIRCLE */ => {
                let radius = min(size.x, size.y) * 0.5;
                d = sd_circle(sp, center, radius);
            }
            2u32 /* PRIM_ELLIPSE */ => {
                d = sd_ellipse(sp, center, size * 0.5);
            }
            9u32 /* PRIM_MESH */ => {
                // Hardware already clipped this fragment to the
                // triangle; `d` just needs to read as "inside" for the
                // downstream smoothstep. Silhouette AA is folded in
                // below, after `fill_alpha` is computed — see the
                // comment on `fill_alpha` and the `mesh_bary`
                // smoothstep block.
                #if TIER == 0
                d = -1.0;
                #else
                // The vertex stage cannot read the triangle here, so the quad covers its
                // bounding box and each fragment tests whether it is inside.
                let aux_off = ((prim.border.z) as u32);
                let pack0 = aux(aux_off);
                let pack1 = aux(aux_off + 1u32);
                let v0 = pack0.xy;
                let v1 = pack0.zw;
                let v2 = pack1.xy;
                let s0 = (v1.x - v0.x) * (sp.y - v0.y) - (v1.y - v0.y) * (sp.x - v0.x);
                let s1 = (v2.x - v1.x) * (sp.y - v1.y) - (v2.y - v1.y) * (sp.x - v1.x);
                let s2 = (v0.x - v2.x) * (sp.y - v2.y) - (v0.y - v2.y) * (sp.x - v2.x);
                let inside = (s0 >= 0.0 && s1 >= 0.0 && s2 >= 0.0)
                          || (s0 <= 0.0 && s1 <= 0.0 && s2 <= 0.0);
                d = if inside { -1.0 } else { 1.0 };
                #endif
            }
            7u32 /* PRIM_TEXT */ => {
                // `gradient_params` holds atlas PIXEL coords for text
                // glyphs (px_min, py_min, px_max, py_max), not pre-
                // divided UVs. Compute UVs against the CURRENT atlas
                // dimensions via `textureDimensions()` so atlas growth
                // (which doubles dims to fit new glyphs) automatically
                // produces correct UVs without needing a CPU-side
                // primitive rewrite. Pre-fix the CPU baked UVs at
                // prepare_text time; any later atlas growth made those
                // UVs reference the wrong sub-rect of the resized
                // texture → garbled glyphs after the first zoom.
                let pixel_bounds = prim.gradient_params;
                let is_color = fill_type >= 1u32 && fill_type <= 3u32;
                let mut atlas_size: Vec2;
                if is_color {
                    atlas_size = textureDimensions(COLOR_GLYPH_ATLAS).as_vec2();
                } else {
                    atlas_size = textureDimensions(GLYPH_ATLAS).as_vec2();
                }
                let uv_bounds = pixel_bounds / Vec4::new(
                    atlas_size.x, atlas_size.y, atlas_size.x, atlas_size.y
                );
                let local_uv = (sp - origin) / size;
                // A transformed AABB must not sample adjacent glyphs in the atlas.
                if fill_type >= 2u32 && (any(local_uv < Vec2::splat(0.0)) || any(local_uv >= Vec2::splat(1.0))) {
                    discard!();
                }
                let atlas_uv = uv_bounds.xy + local_uv * (uv_bounds.zw - uv_bounds.xy);

                let mut text_result: Vec4;
                if is_color {
                    text_result = textureSampleLevel(COLOR_GLYPH_ATLAS, GLYPH_SAMPLER, atlas_uv, 0.0);
                    if fill_type == 2u32 {
                        text_result = Vec4::new(prim.color.rgb, prim.color.a * text_result.a);
                    } else if fill_type == 3u32 {
                        text_result.a *= prim.color.a;
                    }
                } else {
                    let coverage = textureSampleLevel(GLYPH_ATLAS, GLYPH_SAMPLER, atlas_uv, 0.0).r;
                    let gamma_coverage = pow(coverage, 0.7);
                    text_result = Vec4::new(prim.color.rgb, prim.color.a * gamma_coverage);
                }

                text_result.a *= clip_alpha;

                // Rect-edge AA from `clip_bounds`. Only apply when the
                // primitive actually has a rect clip — when `clip_type`
                // is None the bounds are padding / stale metadata and
                // this smoothstep would discard every fragment for a
                // degenerate (zero-width or zero-height) rectangle that
                // the layout pipeline can produce mid-frame (e.g. a
                // scroll container whose inner clip hasn't resolved a
                // dimension yet). `calculate_clip_alpha` above already
                // returned 1.0 in that case, so this extra edge fade
                // being 1.0 too matches expectation.
                if clip_type == 1u32 {
                    let edge_aa = 1.0;
                    let clip_edge_alpha = smoothstep(0.0, edge_aa, min(
                        min(p.x - prim.clip_bounds.x, prim.clip_bounds.x + prim.clip_bounds.z - p.x),
                        min(p.y - prim.clip_bounds.y, prim.clip_bounds.y + prim.clip_bounds.w - p.y)
                    ));
                    text_result.a *= clip_edge_alpha;
                }

                return text_result;
            }
            _ => {
                d = sd_shaped_rect(sp, origin, size, prim.corner_radius, prim.corner_shape);
            }
        }

        // Anti-aliasing: smooth transition at edge over ~1 pixel total.
        //
        // `aa_width = 0.5` means the `smoothstep` range is `[-0.5, 0.5]` —
        // one pixel wide, centered on the shape boundary. The fragment
        // shader is evaluated at pixel CENTERS (integer + 0.5), so for an
        // axis-aligned edge at integer y, pixels at `y ± 0.5` end up at
        // `d = ∓0.5`, which `smoothstep(-0.5, 0.5, ±0.5)` saturates to 1
        // and 0 respectively. That matters for two reasons:
        //
        //   1. **No seams between adjacent same-colored elements.** When
        //      two fills meet at an integer-pixel line (e.g. menu bar
        //      bottom + notch dropdown top), both sides of the boundary
        //      now reach full alpha at their own pixel-center and don't
        //      double-composite with partial alpha — the old wider AA
        //      (`d_fw * 0.75 ≈ 1.06`) left them at `alpha ≈ 0.83`, which
        //      let ~14% of the background bleed through at the shared
        //      edge as a hairline seam.
        //
        //   2. **Pixel-accurate rect bounds.** A rect with integer pixel
        //      bounds now covers exactly the right pixels with alpha 1,
        //      matching what a non-AA rasterizer would produce. Previously
        //      the AA zone extended half a pixel into each neighboring
        //      row/column.
        //
        // The trade-off is that diagonal edges lose a fraction of a pixel
        // of AA softness — they still transition over ~1 pixel in the
        // gradient direction but the transition is a touch tighter than
        // the old 2.12-pixel zone. Subpixel accuracy is preserved because
        // the SDF is Euclidean (|∇d| = 1) so the 1-pixel smoothstep range
        // matches the true distance-to-edge for every orientation.
        let aa_width = 0.5;
        let mut fill_alpha = 1.0 - smoothstep(-aa_width, aa_width, d);

        #if TIER == 0
        // Mesh primitive silhouette AA. `mesh_bary` is (1,0,0) at v0,
        // (0,1,0) at v1, (0,0,1) at v2 — so the component opposite
        // each edge drops to 0 on that edge. `prim.corner_radius`
        // carries the per-edge silhouette flag (1.0 = silhouette edge,
        // 0.0 = interior tessellation seam) in the order
        //   [0] = edge v0→v1  (bary.z drops to 0 here)
        //   [1] = edge v1→v2  (bary.x drops)
        //   [2] = edge v2→v0  (bary.y drops)
        //
        // Convert each barycentric to pixel-distance-to-edge via
        // `bary / fwidth(bary)` — this is the signed distance from the
        // fragment centre to the edge measured in screen-space pixels
        // (positive inside, 0 on the edge). We then use
        // `saturate(d_px + 0.5)` — a linear ramp centred on the edge
        // such that a pixel whose centre sits exactly on the edge gets
        // 50 % coverage, a pixel half a pixel inside is fully opaque,
        // and anything further inside stays at 1.0. This is closer to
        // true geometric coverage than `smoothstep(0, fw, bary)`, which
        // fades INWARD from the edge (shrinking triangles by half a
        // pixel) and gives dithered-looking edges because the alpha at
        // boundary pixels depends on exactly how close each pixel's
        // centre is to the edge.
        //
        // Interior (non-silhouette) edges have flag = 0, so `mix` keeps
        // the coverage at 1.0 — tessellation seams stay fully opaque.
        // Combining via `min()` gives distance-to-nearest-silhouette-edge
        // semantics, avoiding the double-darkening that `a * b * c` causes
        // at vertices where two silhouette edges meet.
        //
        // `fwidth` runs in uniform control flow here (computed
        // unconditionally before any prim_type branch) to satisfy WGSL's
        // derivative-uniformity rule.
        // `UNIFORMS._padding.x` is the MSAA-active flag: 1.0 when the
        // render pass is writing into a multisampled target, 0.0 on
        // single-sampled paths. When hardware MSAA is doing the silhouette
        // coverage, stacking the shader's per-edge barycentric fade on
        // top under-fills partially-covered pixels — each rendered sample
        // carries a faded-down value before the hardware resolver
        // averages them, so a 2/4-covered pixel that should resolve to
        // 0.5 alpha comes out closer to 0.05. Skip the fade in that case;
        // keep it on single-sampled paths where it's the only AA we have.
        //
        // `fwidth` still runs in uniform control flow so the derivative
        // queries stay valid regardless of which branch the fragment
        // actually takes.
        let mb = mb_uniform;
        let mb_fw = mb_fw_uniform;
        let mesh_msaa_active = UNIFORMS._padding.x > 0.5;
        if prim.type_info.x == 9u32 {
            if mesh_msaa_active {
                fill_alpha = 1.0;
            } else {
                let flags = prim.corner_radius.xyz;
                let d_x = mb.x / max(mb_fw.x, 1e-5);
                let d_y = mb.y / max(mb_fw.y, 1e-5);
                let d_z = mb.z / max(mb_fw.z, 1e-5);
                let a_x = saturate(d_x + 0.5);
                let a_y = saturate(d_y + 0.5);
                let a_z = saturate(d_z + 0.5);
                let m_x = mix(1.0, a_x, flags.y);
                let m_y = mix(1.0, a_y, flags.z);
                let m_z = mix(1.0, a_z, flags.x);
                fill_alpha = min(m_x, min(m_y, m_z));
            }
        }
        #endif

        if fill_alpha < 0.001 {
            return result;
        }

        // Determine fill color
        let mut fill_color: Vec4;
        // PRIM_MESH with `type_info.w == 1u32` carries per-vertex colours
        // packed into `AUX_DATA[offset+2..+5]` right after the triangle
        // positions. Barycentric interpolation reproduces authored
        // multi-stop gradients across the triangle without needing a
        // gradient atlas — fidelity tracks tessellation density. Honour
        // this before the fill_type match so the 2-stop `mix` path
        // stays the precise per-fragment default.
        if prim.type_info.x == 9u32 && prim.type_info.w == 1u32 {
            let aux_off = ((prim.border.z) as u32);
            let c0 = aux(aux_off + 2u32);
            let c1 = aux(aux_off + 3u32);
            let c2 = aux(aux_off + 4u32);
            #if TIER == 0
            let mb = input.mesh_bary;
            fill_color = c0 * mb.x + c1 * mb.y + c2 * mb.z;
            #else
            // Barycentric weights from the fragment's position in the triangle.
            let pack0 = aux(aux_off);
            let pack1 = aux(aux_off + 1u32);
            let v0 = pack0.xy;
            let v1 = pack0.zw;
            let v2 = pack1.xy;
            let denom = (v1.y - v2.y) * (v0.x - v2.x) + (v2.x - v1.x) * (v0.y - v2.y);
            let safe_denom = if denom.abs() < 1e-6 { 1e-6 } else { denom };
            let w0 = ((v1.y - v2.y) * (sp.x - v2.x) + (v2.x - v1.x) * (sp.y - v2.y)) / safe_denom;
            let w1 = ((v2.y - v0.y) * (sp.x - v2.x) + (v0.x - v2.x) * (sp.y - v2.y)) / safe_denom;
            let w2 = 1.0 - w0 - w1;
            fill_color = c0 * w0 + c1 * w1 + c2 * w2;
            #endif
        } else {
            match if FEAT_GRADIENT { fill_type } else { 0u32 } {
                0u32 /* FILL_SOLID */ => {
                    fill_color = prim.color;
                }
                1u32 /* FILL_LINEAR_GRADIENT */ => {
                    // Linear gradient using gradient_params (x1, y1, x2, y2) in user space
                    let g_start = prim.gradient_params.xy;
                    let g_end = prim.gradient_params.zw;
                    let g_dir = g_end - g_start;
                    let g_len_sq = dot(g_dir, g_dir);

                    let mut t: f32;
                    if (g_len_sq > 0.0001) {
                        // Project current position onto gradient line
                        let proj = sp - g_start;
                        t = dot(proj, g_dir) / g_len_sq;
                    } else {
                        t = 0.0;
                    }
                    if prim.gradient_table.x > 0.5 {
                        fill_color = gradient_table_color(t, ((prim.gradient_table.x) as u32) - 1u32);
                    } else {
                        fill_color = mix(prim.color, prim.color2, clamp(t, 0.0, 1.0));
                    }
                }
                2u32 /* FILL_RADIAL_GRADIENT */ => {
                    // Radial gradient using gradient_params (cx, cy, radius, 0) in user space
                    let g_center = prim.gradient_params.xy;
                    let g_radius = prim.gradient_params.z;

                    let dist = length(sp - g_center);
                    let t = dist / max(g_radius, 0.001);
                    if prim.gradient_table.x > 0.5 {
                        fill_color = gradient_table_color(t, ((prim.gradient_table.x) as u32) - 1u32);
                    } else {
                        fill_color = mix(prim.color, prim.color2, clamp(t, 0.0, 1.0));
                    }
                }
                3u32 /* FILL_CONIC_GRADIENT */ => {
                    // Conic gradient using gradient_params (cx, cy, start angle, 0) in user space
                    let delta = sp - prim.gradient_params.xy;
                    let t = fract((atan2(delta.x, -delta.y) - prim.gradient_params.z) / 6.283185307179586);
                    if prim.gradient_table.x > 0.5 {
                        fill_color = gradient_table_color(t, ((prim.gradient_table.x) as u32) - 1u32);
                    } else {
                        fill_color = mix(prim.color, prim.color2, t);
                    }
                }
                _ => {
                    fill_color = prim.color;
                }
            }
        }

        // Handle border with proper inner corner radii (GPUI-style approach)
        // The border is the ring between the outer shape edge and an inner shape
        // For asymmetric borders, inner corners become elliptical, not circular
        // prim.border = [top, right, bottom, left] for per-side borders, or [uniform, 0, 0, 0] for uniform
        let border_top = prim.border.x;
        let border_right = prim.border.y;
        let border_bottom = prim.border.z;
        let border_left = prim.border.w;

        // Check if any border is present (using max of all sides).
        // PRIM_MESH reuses `border[1]` for triangle count and `border[2]`
        // for the AUX_DATA offset — non-zero values that would otherwise
        // trigger the border stroke below and paint `border_color`
        // (default opaque black) over the whole fill. Mesh PRIMITIVES
        // have no border to render, so short-circuit past this block for
        // them.
        let max_border = max(max(border_top, border_right), max(border_bottom, border_left));
        if FEAT_BORDER && max_border > 0.0 && prim_type != 9u32 {
            // For uniform border (legacy: only .x set), use it for all sides
            let bt = if border_right > 0.0 || border_bottom > 0.0 || border_left > 0.0 { border_top } else { border_top };
            let br = if border_right > 0.0 || border_bottom > 0.0 || border_left > 0.0 { border_right } else { border_top };
            let bb = if border_right > 0.0 || border_bottom > 0.0 || border_left > 0.0 { border_bottom } else { border_top };
            let bl = if border_right > 0.0 || border_bottom > 0.0 || border_left > 0.0 { border_left } else { border_top };

            let half_size = size * 0.5;
            let rel = sp - center;  // Position relative to center (signed, in unrotated space)

            // Use the same AA width as the outer edge smoothstep so the
            // border's inner transition matches the fill's outer transition
            // pixel-for-pixel. Currently 0.5 (tight 1-pixel AA — see the
            // longer rationale above `aa_width`).
            let border_aa = aa_width;

            // Select corner radius and corner shape based on quadrant
            let mut corner_radius: f32;
            let mut corner_n: f32;
            if rel.y < 0.0 {
                if rel.x > 0.0 { corner_radius = prim.corner_radius.y; corner_n = prim.corner_shape.y; }  // top-right
                else { corner_radius = prim.corner_radius.x; corner_n = prim.corner_shape.x; }           // top-left
            } else {
                if rel.x > 0.0 { corner_radius = prim.corner_radius.z; corner_n = prim.corner_shape.z; }  // bottom-right
                else { corner_radius = prim.corner_radius.w; corner_n = prim.corner_shape.w; }           // bottom-left
            }
            // Clamp radius to half the minimum dimension (CSS spec)
            corner_radius = min(corner_radius, min(half_size.x, half_size.y));

            // Select border widths for nearest edges based on quadrant (GPUI approach)
            let border = Vec2::new(
                if rel.x < 0.0 { bl } else { br },  // horizontal: left or right
                if rel.y < 0.0 { bt } else { bb }   // vertical: top or bottom
            );

            // Handle zero-width borders (treat as negative for AA purposes)
            let reduced_border = Vec2::new(
                if border.x == 0.0 { -border_aa } else { border.x },
                if border.y == 0.0 { -border_aa } else { border.y }
            );

            // Calculate position relative to corner
            let corner_to_point = abs(rel) - half_size;
            let corner_center_to_point = corner_to_point + corner_radius;

            // Determine if we're near a rounded corner
            let is_near_rounded_corner = corner_center_to_point.x >= 0.0 && corner_center_to_point.y >= 0.0;

            // Inner straight border edge
            let straight_border_inner = corner_to_point + reduced_border;

            // Check if we're clearly inside the inner area (not near border)
            let is_within_inner_straight = straight_border_inner.x < -border_aa &&
                                           straight_border_inner.y < -border_aa;

            // Fast path: clearly inside inner area, not near rounded corner.
            // A concave corner's edges lie inside the straight-inner box and
            // outside the corner square, which is exactly what the fast path
            // skips -- so a scoop's arc ends and a notch's step faces drew no
            // border at all. Send them to the SDF branch.
            let is_concave_corner = corner_n < 0.0;

            if is_within_inner_straight && !is_near_rounded_corner && !is_concave_corner {
                // No border here, keep fill_color as-is
            } else {
                // Calculate inner SDF based on context
                let mut inner_sdf: f32;

                if is_concave_corner {
                    // The asymmetric branch below insets the corner RADIUS,
                    // which only means something for a convex corner. Now
                    // that `d` is a true distance, offsetting it is exact for
                    // a uniform width; with differing widths take the smaller
                    // so the border never exceeds what was asked for.
                    inner_sdf = -(d + min(reduced_border.x, reduced_border.y));
                } else if abs(reduced_border.x - reduced_border.y) < 0.001 {
                    // Uniform border — use exact SDF offset of the outer shape distance.
                    // For rounded rects (Minkowski sum of box + circle), inset by a constant
                    // produces another rounded rect. This is branchless, handles both straight
                    // edges and corners, and is guaranteed continuous with the outer SDF.
                    inner_sdf = -(d + reduced_border.x);
                } else {
                    // Asymmetric borders — inner corners become elliptical
                    if corner_center_to_point.x <= 0.0 || corner_center_to_point.y <= 0.0 {
                        // Not in corner region — straight edge distance
                        inner_sdf = -max(straight_border_inner.x, straight_border_inner.y);
                    } else if abs(corner_n - 1.0) < 0.01 {
                        // Round corner — elliptical inner corner (GPUI approach)
                        let ellipse_radii = max(Vec2::splat(0.0), Vec2::splat(corner_radius) - reduced_border);
                        inner_sdf = quarter_ellipse_sdf(corner_center_to_point, ellipse_radii);
                    } else {
                        // Superellipse with per-axis reduced radii
                        let inner_radii = max(Vec2::splat(0.0), Vec2::splat(corner_radius) - reduced_border);
                        let p_exp = pow(2.0, min(abs(corner_n), 5.0));
                        let min_inner_r = min(inner_radii.x, inner_radii.y);
                        if min_inner_r < 0.001 {
                            inner_sdf = -length(max(Vec2::splat(0.0), corner_center_to_point));
                        } else {
                            let inner_t = corner_center_to_point / inner_radii;
                            let inner_se = pow(max(inner_t.x, 0.0), p_exp) + pow(max(inner_t.y, 0.0), p_exp);
                            let inner_r_scale = sqrt(inner_radii.x * inner_radii.y);
                            inner_sdf = -((pow(inner_se, 1.0 / p_exp) - 1.0) * inner_r_scale);
                        }
                    }
                }

                // Match the main fill's tight 1-pixel AA (see `aa_width`
                // above) so the inner border edge shares the same
                // transition width as the outer shape edge. Wider AA here
                // would create a visible gap between the border fill and
                // the main fill at pixel boundaries.
                let inner_aa = aa_width;
                let border_blend = smoothstep(-inner_aa, inner_aa, -inner_sdf);

                // Composite the border ON TOP of the fill using src-over alpha
                // compositing, NOT a four-component mix. The previous mix() pulled
                // the border colour's alpha into fill_color.a — so when the border
                // is semi-transparent (e.g. ColorToken::Border at 10 % alpha) over
                // an opaque body the border ring became ~90 % transparent and the
                // page bg leaked through as a dark "extra outline stroke" tracing
                // the rounded corner.
                //
                // Standard "over" in straight-alpha form:
                //   out_a   = src_a + dst_a * (1 - src_a)
                //   out_rgb = (src_rgb * src_a + dst_rgb * dst_a * (1 - src_a)) / out_a
                // Both branches matter:
                //   * opaque fill + semi-transparent border  → out_a stays 1.0,
                //     out_rgb = border * border_a + fill * (1 - border_a).
                //     Body stays opaque, corner ring tinted by border colour.
                //   * transparent fill (e.g. outline button) + semi-transparent
                //     border → out_a = border_a, out_rgb = border_rgb. The border
                //     renders at its own colour and alpha, exactly as before the
                //     bug fix. The premultiplied form without the /out_a division
                //     would have squared the alpha here (10 % × 10 % = 1 % effective)
                //     and made the outline button's border invisible.
                let border_t = border_blend * step(0.001, fill_alpha);
                let mut border_rgba = prim.border_color;
                if prim.side_color_top.a + prim.side_color_right.a
                    + prim.side_color_bottom.a + prim.side_color_left.a > 0.0 {
                    border_rgba = border_side_color(
                        rel, half_size, Vec4::new(bt, br, bb, bl),
                        prim.side_color_top, prim.side_color_right,
                        prim.side_color_bottom, prim.side_color_left
                    );
                }
                let border_a = border_rgba.a * border_t;
                let out_a = border_a + fill_color.a * (1.0 - border_a);
                let premult_rgb = border_rgba.rgb * border_a
                    + fill_color.rgb * fill_color.a * (1.0 - border_a);
                let new_rgb = if out_a < 0.0001 { Vec3::splat(0.0) } else { premult_rgb / max(out_a, 0.0001) };
                fill_color = Vec4::new(new_rgb, out_a);
            }
        }

        // Apply clip alpha to shadow
        result.a *= clip_alpha;

        // Mask shadow strictly outside the shape boundary
        // Use the same aa_width as fill_alpha to prevent gaps at corners
        // The shadow should render only where d > 0 (outside the shape)
        if result.a > 0.0 {
            // Use matching AA width to ensure shadow and fill meet seamlessly
            let shadow_mask = smoothstep(-aa_width, aa_width, d);
            result.a *= shadow_mask;
        }

        // Blend fill over shadow at FULL opacity first (fill fully covers shadow)
        // This ensures no shadow bleeds through the shape regardless of edge AA
        let full_fill = Vec4::new(fill_color.rgb, fill_color.a * clip_alpha);
        result = full_fill + result * (1.0 - full_fill.a);

        // NOW apply outer edge anti-aliasing to the combined result
        // This gives smooth edges against the background without shadow bleed
        result.a *= fill_alpha;

        // Apply mask gradient (mask-image: linear-gradient / radial-gradient)
        // mask_info.x: 0=none, 1=linear, 2=radial
        // mask_params are in OBB (0-1) space relative to element bounds
        let mask_type = prim.mask_info.x;
        if FEAT_MASK && mask_type > 0.5 {
            // Compute normalized UV relative to element bounds
            let mask_uv = (sp - origin) / max(size, Vec2::splat(0.001));
            let mut mask_t: f32;
            if mask_type < 1.5 {
                // Linear mask gradient
                let m_start = prim.mask_params.xy;
                let m_end = prim.mask_params.zw;
                let m_dir = m_end - m_start;
                let m_len_sq = dot(m_dir, m_dir);
                if m_len_sq > 0.0001 {
                    mask_t = clamp(dot(mask_uv - m_start, m_dir) / m_len_sq, 0.0, 1.0);
                } else {
                    mask_t = 0.0;
                }
            } else {
                // Radial mask gradient
                let m_center = prim.mask_params.xy;
                let m_radius = prim.mask_params.z;
                mask_t = clamp(length(mask_uv - m_center) / max(m_radius, 0.001), 0.0, 1.0);
            }
            let mask_alpha = mix(prim.mask_info.y, prim.mask_info.z, mask_t);
            result = Vec4::new(result.rgb * mask_alpha, result.a * mask_alpha);
        }

        // Apply CSS filters (grayscale, invert, sepia, hue-rotate, brightness, contrast, saturate)
        // Skip if all identity (filter_a all zero, filter_b = (1,1,1,0))
        let fa = prim.filter_a;
        let fb = prim.filter_b;
        if FEAT_FILTER && (fa.x != 0.0 || fa.y != 0.0 || fa.z != 0.0 || abs(fa.w) > 0.001 || fb.x != 1.0 || fb.y != 1.0 || fb.z != 1.0) {
            result = apply_css_filter(result, fa, fb);
        }

        return result;
    }
}
