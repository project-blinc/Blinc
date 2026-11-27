# Changelog

All notable changes to `blinc_gpu` will be documented in this file.

## [Unreleased]

### Fixed
- A notch's shadow no longer drops out under the partly covered pixels of its edge, which drew a dotted line along a rounded corner.
- Inset box shadows paint as a browser paints them: a Gaussian blur with a deviation of half the blur radius, a hole moved by the offset and shrunk by the spread, smooth corners, and about half strength at the edge. The circle inner shadow follows. `draw_inner_shadow` takes the padding box.
- Coloured box shadows keep their colour. The falloff scaled the colour as well as its alpha, so they drew darker than a browser draws them.
- Circle shadows scale with the display and with transforms, as the circles that cast them do.

### Added
- `GpuRenderer::render_glass_layers` draws a batch's glass, and the glass nested in it, over what a target holds.
- The notch shader reads a peak's apex offset from the last lane of its edge modifier, which a peak did not use.
- `GpuRenderer::overlay_divides_at` says whether a batch can be drawn in parts divided at given primitive indices, and `render_overlay_part` draws one part as `render_overlay_msaa` at one sample draws the whole batch, so content drawn apart from the batch can go between the parts.
- `GpuRenderer::begin_frame_submission` and `end_frame_submission` bracket a frame: between them the renderer's passes record into one shared command encoder and the frame is submitted once, at the end. Devices without storage buffers keep submitting each pass. `GpuRenderer::queue` and `queue_arc` submit what the renderer has recorded before handing out the queue, so work submitted through it lands after the renderer's.
- `DynamicImage` records the motion-bound node it was painted under (`node_ordinal`), the row that moves it (`node_slot`) and the row its shape clip follows from (`shape_from`); `node_table::place_dynamic_image` draws it through the rows, and the layered overlay places ordered images that way. An image painted under inner clips carries them as its shape clip. `PrimitiveBatch::has_ordered_content_besides_images`.
- Shape clip type 4: a rounded rect with a radius and a corner shape for each corner, in five aux rows. Inside content that rows move (`DrawContext::begin_row_clips`), a rect or rounded-rect clip is an inner clip: the inner clips in force are drawn as one type-4 shape clip that follows the rows, and stay out of the clip rect, which keeps the clips around the content. `DrawContext::current_row_clip_aabb` is the clip without them.
- `GpuRenderer::render_image_set` draws a list of images, each with its own texture, in one pass and one submission.
- A path clip can follow the animated node table: `GpuPrimitive::set_node_path_from` says from which row of a primitive's chain it moves, and the SDF, notch and shadow shaders test it at the pixel taken back through those rows, its samples along screen axes. Rows that turn may move it. `node_table::follow_path_clip`, `chain_from` and `invert` do the same for a renderer that applies the rows itself, by moving a contour's points. `DrawContext::current_path_clip` gives the pointer a primitive emitted now would carry.
- A glyph under a node row that scales is put on a whole device pixel, by `node_table::resolve_primitive` on the CPU (`snap_glyph`, `NodeRow::scales`) and by the table shader, so a bitmap drawn at its own size is sampled texel for texel while its node scales. A glyph that is turned, and one under rows that only move, are left as they are.
- `Uniforms::new` builds the viewport uniforms stamped with `blinc_core::FrameClock`'s time, `with_msaa` sets the multisample flag and `time` reads the stamp. The shaders' `UNIFORMS.time` is the frame clock in seconds, in the lane that was padding; `GlassUniforms::time` follows the clock too.
- `GpuRenderer::release_idle_memory` frees the static layer, the overlay targets, the pooled layer textures, the flow scene copy and the mesh pass's window-sized targets, and maintains the device so wgpu destroys them. `LayerTextureCache::clear_pools` drops the pooled textures and keeps the named ones.
- `GpuRenderer::next_frame` starts a frame for the renderer's caches: the overlay passes' offscreen targets that no frame has drawn for 60 frames are freed, and the layer texture pool's idle clock moves, so a pool whose layered passes stopped is flushed as it is after 120 idle frames. A renderer never told keeps them.
- `GpuRenderer::with_offscreen_canvas` shares web surface/device setup with the DOM canvas constructor.
- `RendererConfig::node_table` (off by default; `BLINC_NODE_TABLE=1` turns it on) binds the animated node table at binding 6 of the SDF bind group and builds the core, shadow, 3D and notch pipelines, plain and multisampled, from the shaders that apply it. `GpuRenderer::set_node_rows` writes the rows (`node_table::NodeRow`, up to `NODE_TABLE_CAPACITY`), `node_table_enabled` reports whether the table is bound, and a primitive names its row with `set_node_slot`. A primitive with a slot draws through the full core shader, not the compact ones. A row scales border widths and shadow offset, blur and spread with the node, as a paint does. Only devices whose vertex stage reads storage buffers have the table; others ignore the setting. `NodeRow::apply_to_rect` and `apply_to_primitive`, `node_table::drawn_bounds` and `resolve_primitive` are the CPU twins of the shader's placement; the renderer culls and sizes layers by where a slotted primitive is drawn, and applies its rows on the CPU where it draws a layer or ordered group. `PrimitiveBatch::node_slots_epoch` says which assignment of slots a batch carries. A primitive's `node_clip_from` says from which row of its chain a clip moves with the rows (the clip of the node itself, or of an ancestor that moves); a clip it does not say follows stays where it is painted.
- `node_table::chain` is what the chain of rows from a slot amounts to for something that is not a primitive: an affine on device pixels (`NodeRow::affine` for one row, `then` to compose), the one scale lengths grow by and the opacity. The CPU twin for the app's images, which draw outside the batch.
- A primitive carries the ordinal of the bound node it was painted for (`GpuPrimitive::node_ordinal`, `set_node_ordinal`), and `GpuPaintContext` stamps it on what it emits while the walker has one set (`DrawContext::set_motion_node_ordinal`), so a batch that cross-stream resolution has reordered still says whose each primitive is. `GpuPaintContext::set_moving_groups_dynamic` keeps the shared groups of a motion subtree in the dynamic batch, as a caller that moves that batch without painting it again needs, instead of retaining them in the static batch at their paint anchor; a glass group is always retained.
- `PrimitiveBatch::has_ordered_content` reports the ordered draws a batch holds now (captured groups, ordered images, glass, layer samples, meshes, GPU passes, clipped paths). `has_ordered_draws` is that or the mark left by cross-stream resolution.

### Deprecated
- `GpuRenderer::update_time` does nothing: the renderer reads `blinc_core::FrameClock`, which the runners set each frame.

### Changed
- The `vulkan` feature (in the `desktop`, `android`, `fuchsia` and `harmony` presets) turns on wgpu's Vulkan backend for Windows, Linux and Android only. Before, it turned on Vulkan through MoltenVK on Apple targets too, which compiled `ash` and its helpers into every macOS build although the renderer only asks for Metal there.
- Workspace builds use `third_party/wgpu-hal`, wgpu-hal 26.0.5 whose Metal backend makes a command buffer only when a command needs one. Before, every render pass also cost an empty command buffer for its transitions. A crate that depends on `blinc_gpu` from crates.io still gets the published wgpu-hal.
- `BLINC_DEBUG_FILL_RECT`, `BLINC_MULTISTOP_VERTEX` and `BLINC_FILL_RULE_EVENODD` are read once per process instead of on each rect, gradient or path, so changing them while the app runs has no effect.
- The layer composite blits write their uniforms into one buffer the renderer keeps, ahead of each blit's pass, instead of creating a buffer for each blit.
- An opacity or blend group without effects or a named output is composited from the premultiplied colour it was drawn in, and its texture is cleared by the pass that draws its first run of primitives. It took four render passes (a clear, the draw, a conversion to straight colour and the composite) and now takes two. Opacity groups drawn through one atlas skip the same two passes. `LayerCompositeUniforms` gains `premultiplied`, 1 when the source holds premultiplied colour, and padding to 128 bytes, so a struct literal needs both fields.
- A glyph drawn as an SDF primitive is clipped once at a rect clip's edge. The text branch faded the edge a second time, so text touching its clip (an input's first letter at its content box) lost half of its first pixel column.
- An opacity or blend group without effects or a named output renders into a pooled texture at least its size, instead of one of exactly its size. A group sliding past the window's edge changes size every frame, and took two new textures each frame.
- The staging belt takes writes up to 8 MiB, in 1 MiB chunks; a larger write gets a chunk of its own that the belt keeps. A per-frame upload over 256 KiB (the text of a scrolled page) went through `Queue::write_buffer`, allocating each frame and submitting the frame's work early.
- Solid fill ownership survives native group rebasing in existing primitive metadata. Retained colour updates use the opacity baked into the fill, keeping group opacity separate without enlarging GPU buffers.
- A retained node row can request glyph placement on the pixel grid after its full translation chain, on both CPU and GPU paths. The row uses a reserved value and keeps its existing size.
- A run of primitives in a layered overlay drawn with one sample and no paths is drawn onto its target in one pass, where it was drawn into a cleared window-sized texture and blended over the target in a second pass. The result is the same composite.
- The renderer's buffer writes go through a reused staging belt instead of `Queue::write_buffer`, which allocated and freed a staging buffer per call, and a write of the bytes a range already holds from the renderer's last write is skipped. A frame at rest allocates nothing. `BLINC_DIRECT_UPLOADS=1` writes through the queue and `BLINC_SUBMIT_EACH=1` submits each pass on its own, for comparison; `BLINC_STRICT_UPLOADS=1` panics where a pass writes a buffer, or borrows the shared encoder, while another pass has it.
- `GpuRenderer::set_glyph_atlas` keeps the views it binds and rebuilds the SDF bind group only when they change; a view replaced in place by a grown atlas counts as a change.
- Consecutive opacity groups with no effect, blend mode or transform, each a run of primitives and at most 512 px a side, are drawn into one atlas and composited in one pass, where each took a clear, a draw, an alpha pass and a blit of its own. A group the viewport cuts, a card against the window's edge, is in the run: its cell is as large as the group and the composite keeps to the viewport, where it used to end the run and take passes of its own. The pixels are the same. `GpuRenderer::set_batch_opacity_layers(false)` draws them one at a time again.
- The opacity-group atlas also takes a group that sits alone in a layer that draws as its content, at full opacity with no effect and no named output, and primitives with a shape clip. Each cell draws through its own copy of the clip frame, moved with the cell. A calendar's two fading months are drawn in two passes instead of four. The pixels are the same.
- `GpuPrimitive` gains `node_slot`, a vec4 whose x is the animated node it follows as `slot + 1` (zero for none), so it is 560 bytes where it was 544 and a struct literal without `..` needs the field. `GpuGlyph::flags[2]` carries the same slot. The shaders read it in the data-texture tier's loader but nothing uses it yet, so output is unchanged.
- The renderer no longer builds the monolithic SDF pipelines or the MSAA text pipeline, which nothing drew with, nor the second copy of the core SDF shader they were built from.
- Plain rects and circles draw with a compact shader, `sdf_simple`: a solid fill, circular or convex superellipse corners, an optional uniform border and an optional rounded rect clip. The vertex stage passes what the fragment stage needs as flat values, so the fragment stage loads nothing. `GpuPrimitive::is_plain_core` picks the primitives and `SdfPipelineCategory::CorePlain` carries them; the output matches the core shader's to within 2/255 (`tests/plain_shader_parity.rs` in `blinc_recorder`). Tiers without storage buffers in the vertex stage keep the core pipelines.
- `sd_shaped_rect` returns early for a convex superellipse corner outside its corner square, with the values the general form returns there, so the interior and straight edges of a large rounded rect skip its `pow` calls.
- The main SDF pass clears to the colour of its first primitive when that is an opaque, square, unclipped rect covering the whole target (a page background), instead of drawing it: the same pixels without shading the screen once more. `GpuPrimitive::opaque_backdrop` decides.
- A rect, circle or ellipse that uses none of the optional core features (solid fill, no mask, CSS filter, inline shadow, 3D tilt or shape clip; a border, corner shape and rect or rounded clip are kept) draws with `sdf_core.wgsl` compiled without those blocks. The shader declares `override FEAT_*` switches and a second pipeline sets them off; `GpuPrimitive::is_simple_core` picks the primitives and `SdfPipelineCategory::CoreSimple` carries them, with draw order kept by contiguous runs. One source, two pipelines: the full shader sizes its registers for every feature on every fragment, which left phone GPUs well under their frame rate. The vertex-buffer and data-texture tiers have no switches and share the full pipeline. A `match` on `SdfPipelineCategory` needs the new arm.
- The core SDF shader (`sdf_core`) is written in russl (`src/sdf_russl`) and compiled when the crate builds; the full and simple core pipelines both come from it, and `sdf_core.wgsl` is gone. The simple pipeline is the same source with the `FEAT_*` switches fixed off when the crate builds, instead of wgpu processing overrides when the pipeline is created; its switches now apply to the vertex stage too. A test draws simple-core primitives with both pipelines and requires identical pixels; another checks that `GpuPrimitive` lays out field for field as the shader's `Primitive`, from the struct layout russl generates. The SDF shadow (`sdf_shadow`), 3D raymarching (`sdf_3d`) and notch (`sdf_notch`) shaders are written in russl too and their `.wgsl` files are gone; both share the `Primitive` and bind group in `sdf_russl::abi`. The core, shadow, 3D and notch shaders are one source for all three tiers (storage buffer, vertex buffer, data texture): a `TIER` pipeline constant and `#if` in the source choose how a primitive and the auxiliary data are read and what the vertex stage takes, and a one-line specialisation builds each variant (`src/sdf_russl`), so their `_vb` and `_dt` WGSL files are gone too. The compact `sdf_simple` shader is written in russl too, and `SDF_SIMPLE_SHADER` is its WGSL. The glyph shader (`text`) is one source for the storage and data texture tiers too, and `TEXT_SHADER` and `TEXT_DT_SHADER` are its WGSL. The image shader is written in russl too, for both tiers, and `IMAGE_SHADER` and `IMAGE_DT_SHADER` are its WGSL; the auxiliary data's bind group slot is a pipeline constant, `AUX_SLOT`. The path shader is written in russl too, for both tiers, and `PATH_SHADER` and `PATH_DT_SHADER` are its WGSL; a test checks that `PathUniforms` lays out as the shader's. The ABI backdrop composite is written in russl too (`display_list` builds its pipeline from it), with a golden per machine for its scene and CPU probes for the frosted path. The glass shaders are written in russl too: the liquid glass and the simple frosted glass, each one source for both tiers, sharing a `glass_data` module; `GLASS_SHADER`, `GLASS_DT_SHADER`, `SIMPLE_GLASS_SHADER` and `SIMPLE_GLASS_DT_SHADER` are their WGSL. The mesh shaders are written in russl too: the full PBR shader with storage buffers and the reduced one for devices without, sharing a `mesh_common` module (the material, the vertex format and the BRDF), each with its colour and transparency entries; `MESH_SHADER` and `MESH_DT_SHADER` are their WGSL. The comparison sampling a shadow map needs is new in russl 0.3.1. The particle shaders are written in russl too: the simulation, with its random state a private variable, and the billboard render pass for both tiers; `PARTICLE_COMPUTE_SHADER`, `PARTICLE_RENDER_SHADER` and `PARTICLE_RENDER_DT_SHADER` are their WGSL. Only `canvas.wgsl` and the custom pass templates remain WGSL, because user shader code is spliced into them at run time. `canvas.wgsl` is self-contained now, so the build script that composed WGSL helpers into shaders and the shared WGSL helper files are gone. `SDF_CORE_SHADER`, `SDF_SHADOW_SHADER`, `SDF_SHADOW_VB_SHADER`, `SDF_SHADOW_DT_SHADER`, `SDF_3D_SHADER`, `SDF_3D_VB_SHADER`, `SDF_3D_DT_SHADER` and `SDF_NOTCH_SHADER` are now the russl builds' WGSL. New dependency: `russl`.
- The shadow depth pass, the tonemap, bloom, skybox and OIT composite passes, the damage-rect clear and the backdrop blur's row pass, and the layer effect shaders (blur, glow, drop shadow, colour matrix, mask image, composite and layer composite) are written in russl (`src/shaders_russl`) and compiled when the crate builds; their `.wgsl` files and inline sources are gone. `CLEAR_QUAD_SHADER`, `BLUR_SHADER`, `GLOW_SHADER`, `DROP_SHADOW_SHADER`, `COLOR_MATRIX_SHADER`, `MASK_IMAGE_SHADER`, `COMPOSITE_SHADER` and `LAYER_COMPOSITE_SHADER` are the russl builds' WGSL.
- On iOS and Android the end-of-frame `GpuRenderer::poll` no longer waits for the GPU to go idle. The CPU encodes the next frame while the GPU draws the last, and the swapchain paces the loop. Desktop keeps the wait.

### Fixed
- A rounded clip keeps its corners inside another rounded clip with larger ones. Each corner took the largest radius of any clip, and lost its rounding when that clip's own corner was elsewhere.
- A shape with a clip of its own inside an overflow fade fades at the fading clip's edges, not at its own.
- A notch composites its border by the border colour's alpha, as a rect does. A translucent border drew darker and uneven on a notch than on a rect.
- A frame with more glyphs than `max_glyphs`, or more glass primitives than `max_glass_primitives`, is truncated with a warning instead of aborting on a buffer overrun, as primitives already were.
- Grayscale text drawn through a transform no longer samples the neighbouring glyphs in the atlas. The quad of a turned glyph is larger than the glyph, and only colour glyphs discarded what lay outside its rectangle, so rotated text drew stray fragments around its letters and its ink grew with the angle.
- The shadow shaders apply a primitive's local affine: the shadow of a turned or skewed element turns with it, offset included, and the quad covers the turned shadow. Shadows stayed axis-aligned under any rotation, CSS or bound.
- The flow scene copy and the mesh pass's window-sized depth, HDR, OIT and bloom targets are freed once no frame has used them for 60 frames, instead of being held for the life of the app; the next use makes them again. The mesh pipelines, shadow map and environment map stay.
- The overlay passes keep a target per viewport size instead of one shared slot, so a frame that draws the window and a layer of another size no longer rebuilds two window-sized textures each time it switches. A pass at one sample no longer allocates a multisampled texture it never draws into.
- `draw_shadow` emits nothing for a shadow whose colour has no alpha. The primitive painted nothing but the shader still ran over its whole blurred area, about a screen of fragments for each large card whose `box-shadow` was `none`.
- The WebGL2 SDF shaders (`sdf_core_dt` and `sdf_core_vb`) composite a border over the fill with source-over alpha, as `sdf_core` does. A semi-transparent border no longer leaks the page through the ring, which drew a dim stroke with a dotted line along rounded corners.
- Web renderer setup probes for a WebGPU adapter first and uses the WebGL2 backend when there is none. Browsers that expose `navigator.gpu` but return no adapter no longer fail with "No suitable GPU adapter found".

### Removed
- `SDF_SHADER`, the monolithic SDF shader. Nothing has drawn with it since the split pipelines replaced it; `SDF_CORE_SHADER`, `SDF_SHADOW_SHADER`, `SDF_3D_SHADER` and `SDF_NOTCH_SHADER` (and their `_VB` and `_DT` variants) are the shaders the renderer uses.

## [0.6.0] - 2026-11-04

### Added
- `GpuPaintContext::draw_shader` queues a flow draw as a canvas GPU pass (`PendingGpuPass::flow`), so it follows the clip, groups and overlays like other passes. `GpuRenderer::render_pending_flow` draws it and `set_flow_cursor` supplies the cursor for a shader whose pointer is not given; `flow_pipeline::app_clock_secs` is the clock for one whose time is not.

### Changed
- **Breaking:** `FlowPipelineCache::compile` takes a `blinc_core::FlowShader` (WGSL and bindings) and builds the pipeline from its WGSL. The WGSL generator moved to `blinc_flow`, which `blinc_gpu` re-exports as `flow_to_wgsl`; the `flow_codegen` module is gone.

### Added
- `SdfBuilder::line(from, to, width)` draws a capsule, a segment with round ends, as one SDF primitive (a rotated rounded rect), so it is antialiased exactly on any target, multisampled or not. It was a stub that drew nothing.

### Fixed
- `GpuPaintContext::ambient_clip_rounded` returns a zero-size clip when the ancestor clips do not overlap; it returned `None`, which means no clip.
- `render_flow` takes the element's visible clip. A flow shader is scissored to its bounds within the target and within that clip, so it stops at a scroll container or clip an ancestor sets. Its viewport is the whole element, so a flow partly off the target or clipped keeps its proportions; it was squeezed into the visible part.
- Retained nested glass samples enclosing prior paint through owned foreground bakes while independent first-level panels keep the prepared frame backdrop. `GroupGlass::nested` records the CPU sampling scope; GPU layouts and shaders are unchanged.
- Glass inside background isolation samples prior paint from pooled snapshots, including inherited translucent paint, images and GPU output. Background/backdrop dispatch shares the bake; ordinary foreground glass retains its prepared-backdrop path.
- Native glass groups retain their panel, clips and child content through opacity/effects and foreground replay. Tight pooled surfaces preserve world-space backdrop coordinates; eligible opaque draws retain the direct glass path.
- Nested native overlays paint after their late owner's ordinary foreground content in tree order across both streams. Intermediate native layer state follows each nested panel, while the enclosing overlay's opacity/effects remain joint.
- Inline portals carry supported native ancestor layers into their late dynamic scope, preserving opacity/effects and restoring ordinary sibling ownership afterward.
- Shared native groups inside overlay subtrees retain their dynamic owner, preserving late foreground stacking instead of moving to the static paint anchor.
- Canvas mesh scenes inside native visual groups retain paint order, effects, clips and opacity. Consecutive compatible instances share depth, shadows, transparency sorting and tone mapping; shapes, images and callbacks separate scene runs. Invisible or empty instances no longer consume batch boundaries, and independent camera/viewports render as separate scenes.
- Canvas GPU callbacks inside native visual groups keep paint order, group effects, clips and opacity. Callback edits preserve loaded content, transparent clears and full-frame physical coordinates. Glass backdrops share the visible background bake so callbacks execute once. Ungrouped callbacks retain their separate dispatch.
- Native named layers can be sampled in their ordered paint pass, retaining post-effect pixels, source crops, destination transforms/clips and group opacity. Outputs are released after the pass; missing or forward references draw nothing.
- Ordered composition preserves perspective layer geometry and edge filtering when neighboring collected groups become active.
- Native layer bakes use their allocated texture extent for raster coordinates and source UVs, keeping output independent of larger pooled textures.
- Image-mask effect uploads match the existing shader's 32-byte uniform footprint, avoiding validation failures when a mask texture is loaded.
- Batch merging retains background RGBA images and rebases their primitive, path and command cursors.
- Native shared opacity groups join background and foreground draws at their tree paint position, including nested layers, paths, raw images and clips. Batch drains, merges, foreground projection and borrowed ordered dispatch preserve ownership. Moving captures retain their static paint position and request repaint instead of patching rebased ranges.
- Native clear rendering sends nested groups through ordered composition so child groups render into their parent instead of blending independently onto the target.

### Added
- Document retained glass commands as renderer-produced CPU metadata, including first-level and nested backdrop scope.
- `LayerCommand::Glass`, `GroupGlass`, opt-in paint capture and backdrop-aware ordered/foreground dispatch. Exhaustive host command matches must handle the new variant. Native `GlassUniforms` is now 32 bytes: `clip_in_image` replaces `_padding`, with added `sample_size` and `origin`; all four native glass shaders use the matching frame layout. Primitive/image instance layouts are unchanged.
- Native `cross_stream::Capture` carries `overlay_order: Option<u32>` for overlay ownership ordering; raw Rust struct literals must provide `None` for ordinary groups. GPU primitive/instance and shader layouts are unchanged.
- `LayerCommand::Meshes` retains grouped scene snapshots; exhaustive host Rust command matches must handle it. `take_pending_meshes` drains only ungrouped instances, and `GpuRenderer::render_pending_meshes` provides shared scene dispatch. GPU primitive/instance layouts and shader sources are unchanged.
- `LayerCommand::GpuPass` retains grouped callbacks and their physical frame/clip metadata. Host Rust consumers with exhaustive command matches need to handle this variant; GPU primitive and instance layouts are unchanged.
- `LayerCommand::Sample` carries optional captured image geometry in `paint`; raw command producers can set it to `None`. Sample-only batches select ordered dispatch. Only names referenced by the pass retain their outputs.
- Native ordered images can retain shared texture views and shader styling, avoiding pixel copies and replay uploads. `DeferredImage` reserves collected content positions; `DynamicImage` gains optional retained state without changing GPU instance layouts.
- `DeferredSvg` commands and deferred-content insertion support ordered raw image payloads alongside native text, retaining opacity, clip rows and rebased image cursors.
- Opt-in deferred text commands resolve to prepared native glyph primitives and owned auxiliary rows inside shared groups. Captured node identities follow cache replay and merging and reset when their batch is cleared.
- Display-list layer masks support producer stop tables beyond 16 entries, including repeated gradients and hard stops. Nested masks follow cropped layer origins. Small masks retain inline uniforms; larger masks use a reusable float texture with no extra render pass or primitive fields. `LayerMask::stops` is a shared slice, and `LayerMask` and `LayerFx` are now `Clone` rather than `Copy`. Invalid mask table ranges and selectors report `Unsupported::LayerEffect`.
- Display-list colour glyph shadows use colour-atlas alpha in the producer tint, with hard and blurred shadows, clips and layer effects. Colour glyphs honour record opacity, and transformed glyphs stay within their atlas rectangles. Native glyph modes and primitive layouts remain unchanged. Invalid atlas selectors report `Unsupported::GlyphMode`.
- Display-list image mappings: cover, contain, fill, tile, none and scale-down with fractional positions, pixel offsets and independent repeat axes. `Painter::render_with_images` accepts intrinsic logical dimensions, legacy fit and alpha-mask resources; the existing texture resolver remains supported. Masks use the producer tint and opacity, and background image brushes share fitting, clip and layer handling. Mapping uses auxiliary rows without extending image instances or creating render textures.
- Display-list canvas slots: `Painter::set_canvas` registers synchronous GPU callbacks in paint order. Borrowed `CanvasFrame` exposes content size, affine transform, display scale, target origin and scissor; prepared pipelines share WGSL transform, clip and opacity helpers. Callbacks can retain buffers and cached textures, draw inside tight pooled layers and use their filters and group opacity. `missing_canvases` reports unregistered slots separately from images. No per-canvas render texture is required.
- Display-list backdrops in paint order: Gaussian blur, colour matrices, frosted grain and liquid-glass refraction, rim lighting, tint, borders and dispersion. Backdrops take transformed geometry, notches and screen/local/fade/element-frame clips. Each samples a snapshot of its current layer; layers containing backdrops retain the full sampling frame. `plan::Painter` uses an internal copyable frame and a raw final copy, so the caller's target needs only render-attachment usage. Shared Gaussian weights and distance helpers keep the native glass pipelines unchanged. Non-finite backdrop data and negative blur or read margins are reported as `Unsupported::BackdropEffect` before Gaussian weights are built.
- Display-list image clipping: clips in the image's own frame, fading screen rectangles, element-frame ellipses, rounded rectangles and polygons, including separate rings and fill rules, and shaped clip corners. `ImageDraw::aux` carries the clip data, `GpuImageInstance::abi_clip` addresses it, and layers translate screen rectangles and inverse frames. Zero retains native image clipping. `Record::image_with_aux` accepts polygon point tables, and `render_images_with_aux` uploads each draw's clip data. The image shader supports storage-buffer and data-texture payloads.
- Display-list clip paths in their element's frame: ellipses, rounded rectangles and polygons, including separate path rings and nonzero or even-odd fill. `GpuPrimitive::clip_shape` addresses an auxiliary payload containing the inverse frame and shape, followed by polygon points; zero keeps native clipping unchanged. Runs rebase that pointer and layer textures translate the inverse frame. Boxes, glyphs, shadows and notches take the clip. `Record::to_primitive` reports shapes needing auxiliary data, while `primitive`, `decode` and `plan` carry it.
- Display-list notches: signed concave or convex corners, top and bottom scoop, bulge, cut and peak edges, the producer's border and gradients, and outer and inset shadows. `GpuPrimitive::notch_info` selects ABI coordinates and shadow mode in the existing notch pipeline; zero keeps native notch shading. Gradient tables, conic fills and local clips also reach the notch shaders. Layer translation moves notch gradient geometry, including conic centres.
- Display-list fading clips on boxes, shadows and glyphs, with a separate `GpuPrimitive::clip_fade_bounds` screen rectangle so intersecting clips retain the producer's fade edges. A zero width keeps native primitives fading relative to `clip_bounds`. Layer content moves the fade rectangle into its texture frame.
- Clips in a primitive's own frame. `GpuPrimitive::local_clip_bounds` and `local_clip_radius` (a width of zero is none) are tested by `sdf_core` and its `vb` and `dt` variants against the sample point taken back through the primitive's transform, so a turned parent clips its children to its turned, rounded box and not to a bounding box on screen, and a rounded clip cut by a plain one is exact. `display_list` reads them for boxes and glyphs.
- `display_list::effects` draws a layer's mask: a linear, radial or conic gradient of alpha measured in the element's box, which the layer's alpha is multiplied by. The composite takes a target pixel back through the element's transform into fractions of its box and evaluates the stops (up to `MAX_MASK_STOPS`), folded by their spread. Every form the producer writes, two stops, a middle stop and a table, is read into that list; a mask with more stops than there is room for leaves its layer drawn flat and reported.
- Gradients with any number of stops, a spread (pad, repeat, reflect) and conic gradients on SDF primitives. `GpuPrimitive::gradient_table` names a stop table in `aux_data` (a header of the count and spread, the average colour, then a colour row and an offset row for each stop; zero means none, so two stops from 0 to 1 are drawn as before), `FillType::ConicGradient` is a fill of its own, and `sdf_core` and its `vb` and `dt` variants read the table, mixing stops in premultiplied colour. `display_list` reads all three of the producer's forms (a middle stop, other offsets, a table) and `plan::Run` carries the tables its primitives name.
- `display_list::effects` draws a layer's blur, colour filter and drop shadow as `blinc_abi` defines them: a Gaussian of a stated deviation in device pixels, an affine map of straight RGB, and the content's blurred alpha shifted and tinted, composited over the parent with straight alpha. Its weights are worked out once on the CPU and a blur wider than `FULL_RESOLUTION_DEVIATION` (8 px) is taken on the content shrunk by powers of two and drawn back at full size. Against a CPU Gaussian it stays within 2 levels of 255 up to that deviation and within 5 beyond it. `plan::Painter` draws a layer's content into a pooled texture that covers only the layer's bounds (and where its shadow is read from), moving the content into that frame, and keeps the pipelines, scratch textures and uniforms between frames; `plan::render` is a one-shot wrapper. `blinc_test_suite`'s `blur_cost` tests measure the blur against the Kawase blur `apply_blur` uses, and a layer drawn over the whole target against one over its element.
- `display_list::plan` nests a layer: the records from a layer's opening to its closing become an `Item::Layer`, drawn into a texture of exactly the target's size and composited with the layer's opacity, so what overlaps inside a faded group does not show through itself. `plan::render` takes the target's size for it. A layer that also blurs, filters its colour, casts a drop shadow or is masked is drawn flat and reported as `Unsupported::LayerEffect`.
- `display_list::plan` turns a display list into a `Plan` in paint order: runs of primitives with each image between them, since an image is a texture the consumer owns, drawn by the image pipeline in a pass of its own. `Record::image` reads an image record as a `GpuImageInstance` stretched over its box, with its opacity and rounded clip, and `plan::render` draws a plan over a target with `render_overlay` and `render_images`, finding each image by its slot. An image that is fitted, positioned or tiled is `Unsupported::ImageMapping`.
- `display_list` reads a glyph record as a `PrimitiveType::Text` primitive over its rect in the producer's glyph atlas, and `display_list::atlas::AtlasTexture` keeps that atlas on the GPU from the producer's revisioned updates, for `GpuRenderer::set_glyph_atlas`. A colour glyph's shape drawn in one colour (a text shadow's) is reported as `Unsupported::ColorGlyphShape`.
- Polygon clips honour a fill rule: `GpuPaintContext::set_clip_fill_rule(FillRule::EvenOdd)` before `push_clip` makes the clip even-odd, carried in `clip_radius.x` and read by `calculate_polygon_clip_alpha` in all twelve SDF shaders.
- `GpuPrimitive` carries a border colour for each side (`side_color_top`, `_right`, `_bottom`, `_left`); alphas that sum to zero mean unset, and the border is `border_color` all round, so existing paths are unchanged. `sdf_core` and its `vb` and `dt` variants colour the border by the side whose edge the point is nearest in widths, so two sides meet on the diagonal at a corner as CSS draws them.
- `display_list`: reads `blinc_abi` display lists. `records` checks a list's version and length, `Record` is a typed view of one record, and `Record::to_primitive` turns a box or shadow record into the `GpuPrimitive` the SDF pipelines draw, at a given display scale. A record that needs something a primitive cannot yet carry (a notch on an image or a layer mask of more than 16 stops) comes back as `Unsupported` naming it.
- Per-corner squircle / scoop / bevel support on `ClipShape::RoundedRect` propagated into the SDF clip primitive uniforms — all 12 SDF shader variants (`sdf_core` / `sdf_shadow` / `sdf_3d` / `sdf_notch` × `vb` / `dt` / msaa) now read and apply per-corner `n` when evaluating clip alpha.

### Fixed
- Static foreground groups retain native motion descendants, images and auxiliary rows with their owning commands. Nested motion scopes restore independent routing after the group exits.
- Native motion background draws stay in an open static ancestor group, retaining group opacity, command ranges and auxiliary ownership. Nested motion exits restore normal routing.
- Primitive overlay draws respect the pending damage scissor, including converted cached-text glyphs.
- Text-only primitive batches participate in the MSAA dispatch range instead of being skipped as empty.
- Prepared glyph batches retain path clip headers through merges, primitive conversion and explicit text-batch rendering. Storage and data-texture text shaders share the contour evaluator; glyph size stays 96 bytes, and direct text bindings follow current atlas and auxiliary resources.
- Standalone glass/blur primitives retain path clip headers and rectangular scissors, including circles, nested materials and merged batches. Native glass now dispatches its existing DT readers; backdrop and auxiliary bindings refresh per draw. Instance size remains 144 bytes.
- Native tessellated path brushes retain path clips through per-vertex contour pointers, owning batches and tight targets. Path auxiliary data uses storage or data textures, including MSAA and texture growth, without increasing vertex stride. `PathVertex::clip_shape` uses a former padding lane; `_padding` now has two lanes.
- Native SDF primitives, solid/gradient path meshes, shadows and canvas text retain path clips with push-time transforms, fill rules and rectangular scissors. Contours are shared per batch; merges and canvas collection rebase auxiliary references.
- Raw RGBA clips retain per-corner shapes and overflow fades, with fade edges anchored to their owning rectangle through inner scissors and tight bakes. Image clip headers carry optional corner rows, and grouped shaped clips avoid an extra circular crop. Scratch ancestor fades remain stripped; the image-instance GPU layout is unchanged.
- Raw RGBA draws support `ClipShape::Path` with the push-time affine, open and closed contours, Bézier curves, SVG arcs and nonzero/even-odd filling. Path clip rows are prepared lazily in physical pixels and retained through drains and tight bakes. Empty, invalid or over-budget paths cover no pixels; other native primitive kinds retain their existing path fallback.
- Raw RGBA images retain native circle, ellipse and polygon clips, including polygon fill rules and affine frames. Self-contained image clip rows survive foreground extraction and batch drains, and translate into tight group textures without changing the image-instance GPU layout. Native path clips remain unsupported.
- Raw RGBA draws retain rotation, shear, reflection and non-uniform scale through the existing image transform. Ordered group bounds use the transformed quad; invalid or near-singular geometry is omitted before allocation/upload. Image shaders apply every non-identity transform without a tolerance cutoff.
- Foreground raw RGBA shares the active foreground batch, local paint cursors and group opacity with shapes and paths. Foreground projection, merging and clearing retain image ownership; explicit legacy image drawing remains available.
- Foreground shape groups keep commands and path cursors in their active static or motion batch, and `render_foreground` composes them once after glass. Unrepresented ancestor opacity and separate raw RGBA fallback are retained.
- Background RGBA draws retain active-batch ownership and primitive/path/layer paint cursors. Native clear, overlay, MSAA, backdrop and tight-bake routes compose them with nested groups in order. `render_unordered_dynamic_images` filters records already drawn by full-batch methods; explicit `render_dynamic_images` still draws the supplied sequence. Foreground RGBA retains its separate opacity fallback. Glass hand-offs retain foreground and reuse native nested-glass dispatch; storage-free backends preserve ordinary background/foreground while omitting unsupported glass. Invalid or empty uploads are skipped.
- Composite bake regions include tessellated paths and nested effect margins in their screen-space bounds.
- Composite scratch groups retain their own commands and opacity. Subtree baking walks nested groups and paths into exact-size textures and returns straight alpha for the live compositor blit.
- Motion-background groups record Push/Pop commands and animation indices in their own batch. Their children retain only explicit or unrepresented ancestor opacity, so overlapping children fade as a group. Pop uses the saved command owner even after routing changes; composite, foreground and image opacity fallbacks remain in place.
- `GpuPaintContext` leaves background group opacity to layer composition rather than baking it into children too. Explicit opacity scopes still colour children, including inside nested or zero-opacity groups. Batch routing recognises opacity-only and 3D groups and excludes them from later z-layer overlays. Independently dispatched motion, composite, foreground and late image streams retain their effective opacity.
- Native MSAA overlays honour nested layer effects, group opacity and blend modes in paint order, including path-only layers. Premultiplied resolve colours are converted for native effects, preserving coverage through no-op boundaries; effect intermediates retain exact texture extents.
- Triangle mesh vertices, optional vertex colours and compound 3D shape descriptors follow primitive routing into motion and composite batches. Inherited polygon clips cache their payload in each batch that uses them and invalidate offsets when batches are drained.
- The no-storage-buffer primitive path sizes texture rows and uploads from `GpuPrimitive` and loads every field in the core, shadow, notch and 3D data-texture shaders. Layout guards check field columns, instance rows and integer bitcasts. This restores the data-texture payload used by WebGL2 without changing native storage-buffer or vertex-buffer rendering.
- `GpuRenderer::render_msaa` now uploads the auxiliary data buffer, so polygon clips and 3D groups draw through it. It drew them as if their vertices were all zero.
- Box shadows in `sdf_shadow` (all three variants): a spread-only shadow now draws (the quad grows by the spread), an offset-only shadow with no blur and no spread now draws, a spread grows a corner only as far as it is already round (a square box keeps square shadow corners), and an unblurred outer or inset edge is antialiased over a pixel. A zero-blur inset shadow is now a crisp ring instead of a soft one.
- `pre_warm_pipelines` previously issued `pass.draw(...)` against the Path pipeline (and SDF pipelines on hosts without `VERTEX_STORAGE`) without first calling `set_vertex_buffer(0, ...)`. Both declare a vertex-buffer layout, so a missing slot 0 triggered the Vulkan validation panic `requires vertex buffer 0 to be set` on Linux at startup and prevented apps from launching. Binds a single-vertex dummy buffer for the warm-up draws.

### Changed
- `LayerTextureCache::evict_oversized` now also drives idle-frame eviction. After 60 frames without an `acquire()` it drops one texture per pool (largest first); after 120 it flushes all four pools. Reclaims GPU memory held by glass / blur intermediates when the UI is sitting still.

## [0.5.1] - 2026-04-13

### Changed
- Split monolithic SDF shader (~2400 lines) into 4 specialized pipelines: core (Rect/Circle/Ellipse), shadow, 3D raymarching, notch
- Back-to-front draw order for split pipelines preserving z-order
- Monolithic SDF_SHADER no longer compiled (split pipelines only)
- Non-sRGB surface format on WebGL2 GL adapter (avoids double gamma)
- Surface COPY_SRC conditionally dropped when unsupported
- Glass rendering skipped on WebGL2 (DT shader exists, per-frame plumbing pending)
- Particle rendering skipped on WebGL2 (no compute shader support)

### Added
- VERTEX_STORAGE fallback: instance-stepped vertex buffers when adapter lacks vertex storage
- WebGL2 data texture fallback: primitive/glyph/aux data packed into Rgba32Float textures via textureLoad, replacing all storage buffer reads
- Data texture variants for all shader types: SDF (core/shadow/3d/notch), text, glass, simple glass, mesh, particle render
- Early prim_type discard guards in all split shaders
- Frame generation counter for skip-redundant GPU uploads
- 28 shader parse tests (up from 16)

## [0.5.0] - 2026-04-10

### Fixed

- All WGSL shaders: `textureSample` → `textureSampleLevel` in non-uniform control flow paths for WebGPU compliance (glass, simple glass, image, composite, drop shadow, glow, path pipeline)
- Image shader: `fwidth` → constant 0.75px AA width (WebGPU rejects derivative functions from non-uniform flow)
- `device_required_limits`: wasm32 uses adapter's reported limits directly (`supported.clone()`) instead of requesting defaults that Safari/Firefox can't satisfy
- VERTEX_STORAGE capability check: descriptive error on browsers without storage buffer vertex access
- Monospace font fallback list: added "JetBrains Mono" and "Fira Code" to `find_generic_font_id` named fallbacks
- `preload_generic_styles` on `with_canvas` path for web font family resolution
- `TextRenderingContext::invalidate_generic_font_cache()` clears negative cache entries after font loads

## [0.4.0] - 2026-04-05

### Added

#### 3D Mesh Rendering Pipeline
- `MeshPipeline` with lazy initialization and PBR WGSL shader (`mesh.wgsl`)
- `render_mesh_data()` — vertex/index buffer upload, PBR shading, optional base color texture
- Blinn-Phong specular + Schlick Fresnel + Lambertian diffuse lighting model

#### Shadow Mapping
- 2048x2048 Depth32Float shadow map texture
- Shadow depth pass pipeline (`shadow.wgsl`) with front-face culling and depth bias
- 4-tap PCF soft shadow sampling in main fragment shader
- Per-material `receives_shadows` / `casts_shadows` control
- `light_view_proj` parameter for directional shadow projection

#### Normal Mapping & Parallax Displacement
- Tangent-space normal mapping with configurable `normal_scale`
- TBN matrix construction from vertex tangent + bitangent
- Parallax occlusion mapping: 16-layer raymarching with interpolated relief
- Per-material `displacement_scale` for height map depth

#### Skeletal Animation (GPU Skinning)
- Storage buffer (binding 8) for joint matrices (max 256 joints)
- Vertex shader computes weighted skin matrix from 4 joint influences
- Joints/weights vertex attributes (locations 5, 6)

#### Custom Render Pass API
- `CustomRenderPass` trait with `PreRender` / `PostProcess` stages
- `RenderPassContext` provides device, queue, target, viewport, format, scale
- `CustomPassManager` for registration, stage execution, resize notification, label-based removal
- `GpuRenderer::register_custom_pass()`, `remove_custom_pass()`, `execute_custom_passes()`

#### Custom Bind Groups
- `BindGroupBuilder` — declarative bind group + layout creation
- Supports uniform buffers, storage buffers (read/write), textures, storage textures, samplers, comparison samplers

#### Compute Dispatch
- `ComputeDispatch` struct for single-call compute execution
- `create_compute_pipeline()` — WGSL source to ComputePipeline convenience
- `create_buffer()` — initialized buffer creation helper

#### Post-Processing Pipeline
- `PostProcessEffect` trait: input → output texture effect interface
- `PostProcessChain` — ping-pong effect chaining as `CustomRenderPass`
- Auto-manages intermediate textures, fullscreen blit pipeline, resize
- `create_fullscreen_pipeline()` — WGSL to fullscreen quad RenderPipeline

#### Render Region Culling
- AABB visibility test before GPU buffer upload
- Conservative expansion for shadows, borders, rotation (half-diagonal), local affine
- 3D perspective primitives always pass (complex projection)
- Applied in both `render_with_clear_simple` and `render_primitives_excluding`

#### GPU Memory Budget
- `GpuMemoryBudget` tracks texture memory across layer cache and mask images
- `RendererConfig.gpu_memory_budget` (default 128 MB, env var `BLINC_GPU_MEMORY_BUDGET_MB`)
- `enforce_memory_budget()` evicts largest pooled textures first, then mask image cache
- `LayerTextureCache::evict_to_budget()` — budget-aware pool eviction

#### Flow Shader 3D Codegen
- `emit_vertex_3d_shader()` — generates vertex shader with mesh vertex input struct
- `emit_material_shader()` — generates fragment shader with inline PBR evaluation
- 3D builtin bindings: vertex attributes, world-space interpolants, matrices, camera, light
- Matrix function WGSL emission: multiply, inverse, transpose, transform_normal, translation/rotation/scale/perspective/lookAt
- `FlowType::Mat4` support in `flow_type_to_wgsl()`
- `FlowTarget::Vertex` / `Material` pipeline creation with mesh vertex buffer layout

#### Dynamic Image Rendering
- `render_dynamic_images()` — per-frame RGBA texture upload via `GpuImage::from_rgba`
- `DynamicImage` in `PrimitiveBatch` for video frames, camera preview, procedural textures

### Changed
- `RendererConfig` uses `..RendererConfig::default()` in all construction sites for forward compatibility

## [0.1.13] - 2026-02-18

### Added

#### Flow Codegen

- Underscore variants for all `StepType` identifiers (e.g. `pattern_noise` alongside `pattern-noise`) for `flow!` macro compatibility

### Changed

- `GpuImageInstance` params layout: `params[2..3]` changed from sin_rot/cos_rot to border_width/packed_border_color; rotation now uses the `transform` field
- Image shader (`image.wgsl`) consolidated VertexOutput from 16 locations to 13 by passing combined `params` vec4

### Removed

- `GpuImageInstance::with_rotation_sincos()` — superseded by `with_transform()` for full 2x2 affine support

### Added

#### 3D SDF Raymarching Pipeline

- Per-element 3D shapes via `shape-3d: box | sphere | cylinder | torus | capsule | group`
- `depth` property for 3D extrusion depth
- `perspective` property for camera distance
- `rotate-x`, `rotate-y` for 3D axis rotation
- `translate-z` for Z-axis translation (closer/farther positioning)
- 32-step raymarching with analytical ray-AABB intersection
- Edge anti-aliasing via closest-approach distance tracking
- Blinn-Phong lighting with configurable `ambient`, `specular`, `light-direction`, `light-intensity`

#### 3D Boolean Operations

- `ShapeDesc` struct for per-shape descriptors in group composition (64 bytes, 4 vec4s)
- `MAX_GROUP_SHAPES` constant (16 shapes per group)
- Boolean SDF operations: `union`, `subtract`, `intersect`
- Smooth boolean operations: `smooth-union`, `smooth-subtract`, `smooth-intersect` with configurable blend radius
- `shape-3d: group` for collecting children into compound SDF via storage buffer

#### UV Mapping

- Automatic UV mapping of background (solid/gradient) onto 3D surface hit points
- Box: face-based projection (front/back, top/bottom, left/right)
- Sphere: spherical coordinate mapping
- Cylinder/torus/capsule: cylindrical coordinate mapping

#### GpuPrimitive Extensions

- `perspective[4]` field: `(sin_rx, cos_rx, persp_d, shape_type)`
- `sdf_3d[4]` field: `(depth, ambient, specular_power, translate_z)`
- `light[4]` field: `(dir_x, dir_y, dir_z, intensity)`
- `filter_a[4]` field: `(grayscale, invert, sepia, hue_rotate_rad)` for CSS filters
- `filter_b[4]` field: `(brightness, contrast, saturate, 0)` for CSS filters

#### Image/SVG Rotation Support

- `GpuImageInstance::with_rotation_sincos()` for applying rotation to image quads
- Image vertex shader (`image.wgsl`) rotates quad vertices around center using sin/cos parameters
- Fixed default `GpuImageInstance` params: `cos_rot` now defaults to 1.0 (was 0.0, collapsed all vertices)

#### Corner Radius Clamping

- Clamp `border-radius` to `min(half_width, half_height)` per CSS spec in:
  - SDF border rendering (fragment shader)
  - Glass shader `sd_rounded_rect`
  - Clip shader `sd_rounded_rect_clip`

#### CSS Filters (GPU)

- `apply_css_filter()` WGSL function: grayscale, invert, sepia, hue-rotate, brightness, contrast, saturate
- Identity-skip guard in fragment shader for zero-cost when no filter is active

#### Kawase Blur Pipeline

- Multi-pass Kawase blur shader with 3 modes: CSS filter (RGBA), shadow (alpha-only), passthrough
- Batched blur passes into single GPU command encoder (was per-pass submission)
- Pre-allocated 8 uniform buffers in `blur_uniforms_pool` (eliminates per-frame buffer creation)
- `apply_blur()` and `apply_shadow_blur()` convenience wrappers
- `calculate_effect_expansion()` for automatic layer texture sizing based on effect parameters
- DPI-scaled layer effects in `render_layer_with_motion`
- XLarge texture pooling in `LayerTextureCache` (>512px textures were previously dropped every frame)

#### Paint Context

- `set_3d_shape()` for configuring per-element 3D shape parameters
- `set_3d_translate_z()` for Z-axis offset
- `set_3d_group_raw()` for compound shape composition from raw float arrays
- 3D transient state management with `clear_3d()` reset
- `set_css_filter()` / `clear_css_filter()` for per-element CSS filter state

#### Glass Backdrop Blur

- Golden-angle spiral blur for simple glass shader (72 samples, 6 rings × 12) replacing 5×5 box blur
- CSS-spec-correct sigma: blur radius = standard deviation (was `radius × 0.5`, producing half-strength blur)
- Sampling extent increased to 2.5× sigma with linear ring spacing for proper Gaussian kernel coverage
- Consistent blur algorithm across both liquid glass (GLASS_SHADER) and simple glass (SIMPLE_GLASS_SHADER) pipelines

### Fixed

- Mix blend mode layers were invisible because `has_layer_effects()` didn't check `blend_mode != Normal`, causing the interleaved z-layer path to strip layer commands from the batch
- Z>0 overlay pass re-rendered blend-mode primitives without blend, overwriting correctly composited results
- Simple glass shader pixelation: replaced crude 25-sample box blur with 72-sample golden-angle spiral, eliminating visible grid artifacts
- Glass blur intensity too weak: corrected Gaussian sigma from `radius * 0.5` to `radius` per CSS spec (blur radius = standard deviation)
- `set_css_filter` and `clear_css_filter` now properly override the `DrawContext` trait (previously only defined as inherent methods, causing no-op dispatch via `&mut dyn DrawContext`)
- Clippy warnings across image.rs, particles.rs, path.rs, primitives.rs, renderer.rs, text.rs

## [0.1.1] - Initial Release

- Initial public release with GPU-accelerated 2D rendering pipeline
