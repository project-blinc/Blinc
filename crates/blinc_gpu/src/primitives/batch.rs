//! The batch of primitives, paths and layer commands handed to the renderer.

use super::{
    ClipType, GpuGlassPrimitive, GpuGlyph, GpuPrimitive, LayerCommand, LayerCommandEntry,
    ParticleViewport3D, Viewport3D,
};

/// A batch of tessellated path geometry
#[derive(Clone, Default)]
pub struct PathBatch {
    /// Vertices for all paths in this batch
    pub vertices: Vec<crate::path::PathVertex>,
    /// Indices for all paths in this batch
    pub indices: Vec<u32>,
    /// Clip bounds for this batch: (x, y, width, height) or (cx, cy, rx, ry)
    pub clip_bounds: [f32; 4],
    /// Clip corner radii for this batch
    pub clip_radius: [f32; 4],
    /// Clip type for this batch: 0=none, 1=rect, 2=circle, 3=ellipse
    pub clip_type: u32,
    /// Whether to use gradient texture (for >2 stop gradients)
    pub use_gradient_texture: bool,
    /// Gradient stops for texture rasterization (when use_gradient_texture is true)
    pub gradient_stops: Option<Vec<blinc_core::GradientStop>>,
    /// Whether to use image texture
    pub use_image_texture: bool,
    /// Image source path for image brush (None if not using image)
    pub image_source: Option<String>,
    /// Image UV bounds: (u_min, v_min, u_max, v_max)
    pub image_uv_bounds: [f32; 4],
    /// Whether to use glass effect
    pub use_glass_effect: bool,
    /// Glass parameters: (blur_radius, saturation, tint_strength, opacity)
    pub glass_params: [f32; 4],
    /// Glass tint color (RGBA)
    pub glass_tint: [f32; 4],
}

/// Batch of GPU primitives for efficient rendering
#[derive(Clone)]
pub struct PrimitiveBatch {
    /// Paired stream captures, joined before native dispatch.
    pub cross_stream_groups: super::cross_stream::CrossStreamGroups,
    /// Background primitives (rendered before glass)
    pub primitives: Vec<GpuPrimitive>,
    /// Foreground primitives (rendered after glass)
    pub foreground_primitives: Vec<GpuPrimitive>,
    pub glass_primitives: Vec<GpuGlassPrimitive>,
    /// Nested glass primitives (glass inside another glass element, rendered in a second pass)
    pub nested_glass_primitives: Vec<GpuGlassPrimitive>,
    pub glyphs: Vec<GpuGlyph>,
    /// Tessellated path geometry
    pub paths: PathBatch,
    /// Foreground tessellated path geometry
    pub foreground_paths: PathBatch,
    /// Layer commands for offscreen rendering and composition
    pub layer_commands: Vec<LayerCommandEntry>,
    /// Foreground-local commands, composed after glass.
    pub foreground_layer_commands: Vec<LayerCommandEntry>,
    /// 3D viewports to render via raymarching
    pub viewports_3d: Vec<Viewport3D>,
    /// GPU particle viewports to render
    pub particle_viewports: Vec<ParticleViewport3D>,
    /// Auxiliary data buffer (`vec4<f32>` array) for variable-length per-primitive data.
    /// Used for 3D group shape descriptors and polygon clip vertices.
    pub aux_data: Vec<[f32; 4]>,
    /// Dynamic RGBA images to upload and render this frame
    /// (video frames, camera preview, procedural textures)
    pub dynamic_images: Vec<DynamicImage>,
    /// Foreground RGBA draws with foreground-local paint cursors.
    pub foreground_dynamic_images: Vec<DynamicImage>,
}

/// Paint cursor in the owning background or foreground stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImagePaintOrder {
    pub primitive_index: usize,
    pub path_vertex_index: usize,
    pub path_index_count: usize,
    pub layer_command_count: usize,
}

/// A sampled texture handle and its non-geometric image shader state.
#[derive(Clone, Debug)]
pub struct RetainedImage {
    pub view: wgpu::TextureView,
    pub style: crate::GpuImageInstance,
}

/// An ordered raw upload or retained texture draw.
#[derive(Clone, Debug)]
pub struct DynamicImage {
    /// Local paint cursor, or None for an explicitly separate legacy draw.
    pub paint_order: Option<ImagePaintOrder>,
    /// Retained texture and shader styling, or None for raw RGBA uploads.
    pub retained: Option<std::sync::Arc<RetainedImage>>,
    /// RGBA pixel data (4 bytes per pixel)
    pub data: Vec<u8>,
    /// Image width in pixels
    pub width: u32,
    /// Image height in pixels
    pub height: u32,
    /// Destination quad before its centered transform, in physical pixels.
    pub dest: blinc_core::Rect,
    /// Centered 2x2 affine applied to dest, matching GpuImageInstance.
    /// Identity is [1, 0, 0, 1].
    pub transform: [f32; 4],
    /// Opacity
    pub opacity: f32,
    /// Corner radius
    pub corner_radius: f32,
    /// Active rectangular clip in physical pixels as `[x, y, width, height]`.
    /// The no-clip sentinel is `[-10000, -10000, 100000, 100000]`.
    pub clip_bounds: [f32; 4],
    /// Per-corner clip radius `[tl, tr, br, bl]` from `get_clip_data`,
    /// so a rounded node-body clip rounds the image too.
    pub clip_radius: [f32; 4],
    /// Five image clip rows, optional shape/point rows and per-corner exponents.
    /// Coordinates are in physical pixels; pointers are relative to these rows.
    /// Empty for unfaded ordinary rectangular/rounded or absent clips.
    pub clip_aux: Vec<[f32; 4]>,
}

impl PrimitiveBatch {
    pub fn new() -> Self {
        Self {
            cross_stream_groups: super::cross_stream::CrossStreamGroups::default(),
            primitives: Vec::new(),
            foreground_primitives: Vec::new(),
            glass_primitives: Vec::new(),
            nested_glass_primitives: Vec::new(),
            glyphs: Vec::new(),
            paths: PathBatch::default(),
            foreground_paths: PathBatch::default(),
            layer_commands: Vec::new(),
            foreground_layer_commands: Vec::new(),
            viewports_3d: Vec::new(),
            particle_viewports: Vec::new(),
            aux_data: Vec::new(),
            dynamic_images: Vec::new(),
            foreground_dynamic_images: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.cross_stream_groups = super::cross_stream::CrossStreamGroups::default();
        self.primitives.clear();
        self.foreground_primitives.clear();
        self.glass_primitives.clear();
        self.nested_glass_primitives.clear();
        self.glyphs.clear();
        self.paths = PathBatch::default();
        self.foreground_paths = PathBatch::default();
        self.layer_commands.clear();
        self.foreground_layer_commands.clear();
        self.viewports_3d.clear();
        self.particle_viewports.clear();
        self.aux_data.clear();
        self.dynamic_images.clear();
        self.foreground_dynamic_images.clear();
    }

    /// Push a 3D viewport for SDF raymarching
    pub fn push_viewport_3d(&mut self, viewport: Viewport3D) {
        self.viewports_3d.push(viewport);
    }

    /// Check if there are any 3D viewports to render
    pub fn has_3d_viewports(&self) -> bool {
        !self.viewports_3d.is_empty()
    }

    /// Push a particle viewport for GPU particle rendering
    pub fn push_particle_viewport(&mut self, viewport: ParticleViewport3D) {
        self.particle_viewports.push(viewport);
    }

    /// Check if there are any particle viewports to render
    pub fn has_particle_viewports(&self) -> bool {
        !self.particle_viewports.is_empty()
    }

    /// Record a layer command at the current primitive index
    pub fn push_layer_command(&mut self, command: LayerCommand) {
        self.layer_commands.push(LayerCommandEntry {
            primitive_index: self.primitives.len(),
            foreground_primitive_index: self.foreground_primitives.len(),
            path_vertex_index: self.paths.vertices.len(),
            path_index_count: self.paths.indices.len(),
            foreground_path_vertex_index: self.foreground_paths.vertices.len(),
            foreground_path_index_count: self.foreground_paths.indices.len(),
            command,
        });
    }

    /// Check if there are any layer commands recorded
    pub fn has_layer_commands(&self) -> bool {
        !self.layer_commands.is_empty()
    }

    /// Images whose paint cursors are part of this batch's ordered stream.
    pub fn has_ordered_images(&self) -> bool {
        self.dynamic_images
            .iter()
            .any(|image| image.paint_order.is_some())
    }

    /// Callback state is opaque and must be executed again when its subtree is painted.
    pub fn has_gpu_passes(&self) -> bool {
        self.layer_commands
            .iter()
            .chain(&self.foreground_layer_commands)
            .any(|entry| matches!(entry.command, LayerCommand::GpuPass(_)))
    }

    /// Callbacks and mesh scenes require ordered execution and fresh scene output.
    pub fn has_external_gpu_draws(&self) -> bool {
        self.layer_commands
            .iter()
            .chain(&self.foreground_layer_commands)
            .any(|entry| {
                matches!(
                    entry.command,
                    LayerCommand::GpuPass(_) | LayerCommand::Meshes(_)
                )
            })
    }

    /// Retained glass commands need backdrop-aware ordered dispatch.
    pub fn has_grouped_glass(&self) -> bool {
        self.layer_commands
            .iter()
            .chain(&self.foreground_layer_commands)
            .any(|entry| matches!(entry.command, LayerCommand::Glass(_)))
    }

    /// Glass retained at a background isolation anchor needs ordered local sampling.
    pub fn has_background_glass(&self) -> bool {
        self.layer_commands
            .iter()
            .any(|entry| matches!(entry.command, LayerCommand::Glass(_)))
    }

    /// Whether splitting the background into primitive-only passes would lose content.
    pub fn requires_ordered_rendering(&self) -> bool {
        self.has_layer_effects() || self.has_ordered_draws()
    }

    /// Draws consumed together by the ordered renderer, including path-only groups.
    pub fn has_ordered_draws(&self) -> bool {
        self.cross_stream_groups.resolved || self.has_ordered_content()
    }

    /// Ordered draws the batch holds now: captured groups, ordered images, glass, layer
    /// samples, meshes, GPU passes and clipped paths. A batch that has only been through
    /// cross-stream resolution keeps that mark with none of these in it.
    pub fn has_ordered_content(&self) -> bool {
        !self.cross_stream_groups.captures.is_empty()
            || self.has_ordered_images()
            || self.layer_commands.iter().any(|entry| {
                matches!(
                    &entry.command,
                    LayerCommand::Glass(_)
                        | LayerCommand::Sample { .. }
                        | LayerCommand::Meshes(_)
                        | LayerCommand::GpuPass(_)
                )
            })
            || (!self.aux_data.is_empty() && self.paths.vertices.iter().any(|v| v.clip_shape > 0))
    }

    /// Whether a layer needs composition, including opacity-only and 3D groups.
    pub fn has_layer_effects(&self) -> bool {
        self.layer_commands.iter().any(|entry| {
            if let LayerCommand::Push { config } = &entry.command {
                !config.effects.is_empty()
                    || config.blend_mode != blinc_core::BlendMode::Normal
                    || config.transform_3d.is_some()
                    || config.opacity < 1.0
            } else {
                false
            }
        })
    }

    pub fn push(&mut self, primitive: GpuPrimitive) {
        self.primitives.push(primitive);
    }

    /// Push a primitive to the foreground layer (rendered after glass)
    pub fn push_foreground(&mut self, primitive: GpuPrimitive) {
        self.foreground_primitives.push(primitive);
    }

    pub fn push_glass(&mut self, glass: GpuGlassPrimitive) {
        self.glass_primitives.push(glass);
    }

    pub fn push_nested_glass(&mut self, glass: GpuGlassPrimitive) {
        self.nested_glass_primitives.push(glass);
    }

    pub fn push_glyph(&mut self, glyph: GpuGlyph) {
        self.glyphs.push(glyph);
    }

    /// Convert a glyph to a primitive and add it to the foreground primitives
    ///
    /// This enables unified rendering of text and SDF shapes in the same pass,
    /// ensuring transforms are applied consistently during animations.
    pub fn push_glyph_as_primitive(&mut self, glyph: GpuGlyph) {
        self.foreground_primitives
            .push(GpuPrimitive::from_glyph(&glyph));
    }

    /// Convert all accumulated glyphs to foreground primitives
    ///
    /// This should be called before rendering to enable unified text/SDF rendering.
    /// After calling this, the glyphs vector will be empty and all text will be
    /// rendered as SDF primitives.
    pub fn convert_glyphs_to_primitives(&mut self) {
        for glyph in self.glyphs.drain(..) {
            self.foreground_primitives
                .push(GpuPrimitive::from_glyph(&glyph));
        }
    }

    /// Get the combined primitives including converted glyphs for unified rendering
    ///
    /// Returns a vector of all foreground primitives plus glyphs converted to primitives.
    /// This is useful for unified rendering without modifying the batch state.
    pub fn get_unified_foreground_primitives(&self) -> Vec<GpuPrimitive> {
        let mut result = self.foreground_primitives.clone();
        for glyph in &self.glyphs {
            result.push(GpuPrimitive::from_glyph(glyph));
        }
        result
    }

    /// Add tessellated path geometry to the batch
    pub fn push_path(&mut self, tessellated: crate::path::TessellatedPath) {
        if tessellated.is_empty() {
            return;
        }
        // Offset indices by current vertex count
        let base_vertex = self.paths.vertices.len() as u32;
        self.paths.vertices.extend(tessellated.vertices);
        self.paths
            .indices
            .extend(tessellated.indices.iter().map(|i| i + base_vertex));
    }

    /// Add tessellated path geometry to the foreground batch
    pub fn push_foreground_path(&mut self, tessellated: crate::path::TessellatedPath) {
        if tessellated.is_empty() {
            return;
        }
        let base_vertex = self.foreground_paths.vertices.len() as u32;
        self.foreground_paths.vertices.extend(tessellated.vertices);
        self.foreground_paths
            .indices
            .extend(tessellated.indices.iter().map(|i| i + base_vertex));
    }

    /// Add tessellated path geometry with clip data to the batch
    pub fn push_path_with_clip(
        &mut self,
        tessellated: crate::path::TessellatedPath,
        clip_bounds: [f32; 4],
        clip_radius: [f32; 4],
        clip_type: ClipType,
    ) {
        if tessellated.is_empty() {
            return;
        }
        // Update clip data on the batch (last path's clip wins)
        self.paths.clip_bounds = clip_bounds;
        self.paths.clip_radius = clip_radius;
        self.paths.clip_type = clip_type as u32;

        // Offset indices by current vertex count
        let base_vertex = self.paths.vertices.len() as u32;
        self.paths.vertices.extend(tessellated.vertices);
        self.paths
            .indices
            .extend(tessellated.indices.iter().map(|i| i + base_vertex));
    }

    /// Add tessellated path geometry with clip data to the foreground batch
    pub fn push_foreground_path_with_clip(
        &mut self,
        tessellated: crate::path::TessellatedPath,
        clip_bounds: [f32; 4],
        clip_radius: [f32; 4],
        clip_type: ClipType,
    ) {
        if tessellated.is_empty() {
            return;
        }
        // Update clip data on the batch (last path's clip wins)
        self.foreground_paths.clip_bounds = clip_bounds;
        self.foreground_paths.clip_radius = clip_radius;
        self.foreground_paths.clip_type = clip_type as u32;

        let base_vertex = self.foreground_paths.vertices.len() as u32;
        self.foreground_paths.vertices.extend(tessellated.vertices);
        self.foreground_paths
            .indices
            .extend(tessellated.indices.iter().map(|i| i + base_vertex));
    }

    /// Add tessellated path geometry with clip data and brush info to the batch
    pub fn push_path_with_brush_info(
        &mut self,
        mut tessellated: crate::path::TessellatedPath,
        clip_bounds: [f32; 4],
        clip_radius: [f32; 4],
        clip_type: ClipType,
        brush_info: &crate::path::PathBrushInfo,
    ) {
        if tessellated.is_empty() {
            return;
        }

        // Stamp clip data onto each new vertex. Path primitives share a single
        // VBO + uniform buffer, so the legacy "batch-level" clip would get
        // clobbered every time a new path with a different clip was submitted —
        // making the LAST notch's clip apply to ALL paths in the batch.
        // Carrying clip per-vertex lets multiple canvases coexist in one draw.
        let clip_type_u = clip_type as u32;
        for v in &mut tessellated.vertices {
            v.clip_bounds = clip_bounds;
            v.clip_radius = clip_radius;
            v.clip_type = clip_type_u;
        }

        // Update batch-level clip too (kept for legacy uniform binding fallback,
        // though the shader now reads from the per-vertex attributes).
        self.paths.clip_bounds = clip_bounds;
        self.paths.clip_radius = clip_radius;
        self.paths.clip_type = clip_type_u;

        // Update brush metadata
        self.paths.use_gradient_texture = brush_info.needs_gradient_texture;
        self.paths.gradient_stops = brush_info.gradient_stops.clone();
        self.paths.use_image_texture =
            matches!(brush_info.brush_type, crate::path::PathBrushType::Image);
        self.paths.image_source = brush_info.image_source.clone();
        self.paths.image_uv_bounds = [0.0, 0.0, 1.0, 1.0]; // Default full UV range
        self.paths.use_glass_effect =
            matches!(brush_info.brush_type, crate::path::PathBrushType::Glass);
        self.paths.glass_params = brush_info.glass_params;
        self.paths.glass_tint = [
            brush_info.glass_tint.r,
            brush_info.glass_tint.g,
            brush_info.glass_tint.b,
            brush_info.glass_tint.a,
        ];

        // Offset indices by current vertex count
        let base_vertex = self.paths.vertices.len() as u32;
        self.paths.vertices.extend(tessellated.vertices);
        self.paths
            .indices
            .extend(tessellated.indices.iter().map(|i| i + base_vertex));
    }

    /// Add tessellated path geometry with clip data and brush info to the foreground batch
    pub fn push_foreground_path_with_brush_info(
        &mut self,
        mut tessellated: crate::path::TessellatedPath,
        clip_bounds: [f32; 4],
        clip_radius: [f32; 4],
        clip_type: ClipType,
        brush_info: &crate::path::PathBrushInfo,
    ) {
        if tessellated.is_empty() {
            return;
        }

        // Stamp clip data onto each new vertex (see push_path_with_brush_info
        // for the rationale — same shared-batch problem applies here).
        let clip_type_u = clip_type as u32;
        for v in &mut tessellated.vertices {
            v.clip_bounds = clip_bounds;
            v.clip_radius = clip_radius;
            v.clip_type = clip_type_u;
        }

        // Update clip data
        self.foreground_paths.clip_bounds = clip_bounds;
        self.foreground_paths.clip_radius = clip_radius;
        self.foreground_paths.clip_type = clip_type_u;

        // Update brush metadata
        self.foreground_paths.use_gradient_texture = brush_info.needs_gradient_texture;
        self.foreground_paths.gradient_stops = brush_info.gradient_stops.clone();
        self.foreground_paths.use_image_texture =
            matches!(brush_info.brush_type, crate::path::PathBrushType::Image);
        self.foreground_paths.image_source = brush_info.image_source.clone();
        self.foreground_paths.image_uv_bounds = [0.0, 0.0, 1.0, 1.0];
        self.foreground_paths.use_glass_effect =
            matches!(brush_info.brush_type, crate::path::PathBrushType::Glass);
        self.foreground_paths.glass_params = brush_info.glass_params;
        self.foreground_paths.glass_tint = [
            brush_info.glass_tint.r,
            brush_info.glass_tint.g,
            brush_info.glass_tint.b,
            brush_info.glass_tint.a,
        ];

        let base_vertex = self.foreground_paths.vertices.len() as u32;
        self.foreground_paths.vertices.extend(tessellated.vertices);
        self.foreground_paths
            .indices
            .extend(tessellated.indices.iter().map(|i| i + base_vertex));
    }

    pub fn is_empty(&self) -> bool {
        self.primitives.is_empty()
            && self.foreground_primitives.is_empty()
            && self.glass_primitives.is_empty()
            && self.glyphs.is_empty()
            && self.paths.vertices.is_empty()
            && self.foreground_paths.vertices.is_empty()
            && self.viewports_3d.is_empty()
            && self.dynamic_images.is_empty()
            && self.foreground_dynamic_images.is_empty()
            && !self
                .layer_commands
                .iter()
                .chain(&self.foreground_layer_commands)
                .any(|entry| {
                    matches!(
                        entry.command,
                        LayerCommand::Glass(_)
                            | LayerCommand::Sample { .. }
                            | LayerCommand::Meshes(_)
                            | LayerCommand::GpuPass(_)
                    )
                })
    }

    /// Check if the batch contains any tessellated path geometry
    pub fn has_paths(&self) -> bool {
        !self.paths.vertices.is_empty() || !self.foreground_paths.vertices.is_empty()
    }

    pub fn path_vertex_count(&self) -> usize {
        self.paths.vertices.len()
    }

    pub fn path_index_count(&self) -> usize {
        self.paths.indices.len()
    }

    pub fn foreground_path_vertex_count(&self) -> usize {
        self.foreground_paths.vertices.len()
    }

    pub fn foreground_path_index_count(&self) -> usize {
        self.foreground_paths.indices.len()
    }

    pub fn primitive_count(&self) -> usize {
        self.primitives.len()
    }

    pub fn foreground_primitive_count(&self) -> usize {
        self.foreground_primitives.len()
    }

    pub fn glass_count(&self) -> usize {
        self.glass_primitives.len()
    }

    pub fn glyph_count(&self) -> usize {
        self.glyphs.len()
    }

    /// Get the maximum z_layer used by primitives in this batch
    pub fn max_z_layer(&self) -> u32 {
        self.primitives
            .iter()
            .chain(self.foreground_primitives.iter())
            .map(|p| p.z_layer())
            .max()
            .unwrap_or(0)
    }

    /// Filter primitives by z_layer, returning a new batch with only matching primitives
    pub fn primitives_for_layer(&self, z_layer: u32) -> Vec<GpuPrimitive> {
        self.primitives
            .iter()
            .filter(|p| p.z_layer() == z_layer)
            .cloned()
            .collect()
    }

    /// Filter primitives by z_layer, excluding those in effect/blend layers
    pub fn primitives_for_layer_excluding_effects(
        &self,
        z_layer: u32,
        effect_indices: &std::collections::HashSet<usize>,
    ) -> Vec<GpuPrimitive> {
        self.primitives
            .iter()
            .enumerate()
            .filter(|(i, p)| p.z_layer() == z_layer && !effect_indices.contains(i))
            .map(|(_, p)| *p)
            .collect()
    }

    /// Primitive indices already drawn by layer composition, including opacity.
    pub fn effect_layer_indices(&self) -> std::collections::HashSet<usize> {
        let mut indices = std::collections::HashSet::new();
        let mut stack: Vec<(usize, &blinc_core::LayerConfig)> = Vec::new();
        for entry in &self.layer_commands {
            match &entry.command {
                LayerCommand::Push { config } => {
                    stack.push((entry.primitive_index, config));
                }
                LayerCommand::Pop => {
                    if let Some((start_idx, config)) = stack.pop()
                        && (!config.effects.is_empty()
                            || config.blend_mode != blinc_core::BlendMode::Normal
                            || config.transform_3d.is_some()
                            || config.opacity < 1.0)
                    {
                        for i in start_idx..entry.primitive_index {
                            indices.insert(i);
                        }
                    }
                }
                LayerCommand::Glass(_)
                | LayerCommand::Meshes(_)
                | LayerCommand::GpuPass(_)
                | LayerCommand::Sample { .. }
                | LayerCommand::DeferredText { .. }
                | LayerCommand::DeferredSvg { .. }
                | LayerCommand::DeferredImage { .. } => {}
            }
        }
        indices
    }

    /// Filter foreground primitives by z_layer
    pub fn foreground_primitives_for_layer(&self, z_layer: u32) -> Vec<GpuPrimitive> {
        self.foreground_primitives
            .iter()
            .filter(|p| p.z_layer() == z_layer)
            .cloned()
            .collect()
    }

    /// Check if any primitives use z_layer (i.e., max_z_layer > 0)
    pub fn has_z_layers(&self) -> bool {
        self.max_z_layer() > 0
    }

    /// Merge another batch into this one
    ///
    /// Useful for combining batches from different paint contexts.
    pub fn merge(&mut self, mut other: PrimitiveBatch) {
        self.resolve_cross_stream_groups();
        other.resolve_cross_stream_groups();
        self.cross_stream_groups.resolved |= other.cross_stream_groups.resolved;
        self.cross_stream_groups
            .text_nodes
            .extend(other.cross_stream_groups.text_nodes);
        self.cross_stream_groups
            .svg_nodes
            .extend(other.cross_stream_groups.svg_nodes);
        self.cross_stream_groups
            .image_nodes
            .extend(other.cross_stream_groups.image_nodes);
        let shift = self.aux_data.len() as f32;
        for glyph in &mut other.glyphs {
            if glyph.flags[1] > 0.0 {
                glyph.flags[1] += shift;
            }
        }
        for p in other
            .primitives
            .iter_mut()
            .chain(&mut other.foreground_primitives)
        {
            p.rebase_aux(shift);
        }
        for v in other
            .paths
            .vertices
            .iter_mut()
            .chain(&mut other.foreground_paths.vertices)
        {
            v.rebase_aux(shift as u32);
        }
        let bg_vertex_offset = self.paths.vertices.len();
        let bg_index_offset = self.paths.indices.len();
        for glass in other
            .glass_primitives
            .iter_mut()
            .chain(&mut other.nested_glass_primitives)
        {
            if glass.clip_radius[3] < 0.0 {
                glass.clip_radius[0] += shift;
            }
        }
        // Record the current primitive count for offsetting layer commands
        let primitive_offset = self.primitives.len();
        let fg_primitive_offset = self.foreground_primitives.len();
        let fg_vertex_offset = self.foreground_paths.vertices.len();
        let fg_index_offset = self.foreground_paths.indices.len();
        let fg_command_offset = self.foreground_layer_commands.len();
        let bg_command_offset = self.layer_commands.len();

        self.primitives.extend(other.primitives);
        self.foreground_primitives
            .extend(other.foreground_primitives);
        self.glass_primitives.extend(other.glass_primitives);
        self.nested_glass_primitives
            .extend(other.nested_glass_primitives);
        self.glyphs.extend(other.glyphs);

        // Merge paths with index offset
        let base_vertex = self.paths.vertices.len() as u32;
        self.paths.vertices.extend(other.paths.vertices);
        self.paths
            .indices
            .extend(other.paths.indices.iter().map(|i| i + base_vertex));

        // Merge foreground paths
        let fg_base_vertex = self.foreground_paths.vertices.len() as u32;
        self.foreground_paths
            .vertices
            .extend(other.foreground_paths.vertices);
        self.foreground_paths.indices.extend(
            other
                .foreground_paths
                .indices
                .iter()
                .map(|i| i + fg_base_vertex),
        );

        // Merge layer commands with offset primitive indices
        for mut entry in other.layer_commands {
            entry.primitive_index += primitive_offset;
            entry.path_vertex_index += bg_vertex_offset;
            entry.path_index_count += bg_index_offset;
            entry.foreground_primitive_index += fg_primitive_offset;
            entry.foreground_path_vertex_index += fg_vertex_offset;
            entry.foreground_path_index_count += fg_index_offset;
            self.layer_commands.push(entry);
        }

        for mut entry in other.foreground_layer_commands {
            entry.primitive_index += fg_primitive_offset;
            entry.path_vertex_index += fg_vertex_offset;
            entry.path_index_count += fg_index_offset;
            entry.foreground_primitive_index += fg_primitive_offset;
            entry.foreground_path_vertex_index += fg_vertex_offset;
            entry.foreground_path_index_count += fg_index_offset;
            self.foreground_layer_commands.push(entry);
        }

        for mut image in other.dynamic_images {
            if let Some(order) = image.paint_order.as_mut() {
                order.primitive_index += primitive_offset;
                order.path_vertex_index += bg_vertex_offset;
                order.path_index_count += bg_index_offset;
                order.layer_command_count += bg_command_offset;
            }
            self.dynamic_images.push(image);
        }

        for mut image in other.foreground_dynamic_images {
            if let Some(order) = image.paint_order.as_mut() {
                order.primitive_index += fg_primitive_offset;
                order.path_vertex_index += fg_vertex_offset;
                order.path_index_count += fg_index_offset;
                order.layer_command_count += fg_command_offset;
            }
            self.foreground_dynamic_images.push(image);
        }

        // Merge 3D viewports
        self.viewports_3d.extend(other.viewports_3d);

        // Primitive references were rebased before moving their rows.
        self.aux_data.extend(other.aux_data);
    }
}

impl Default for PrimitiveBatch {
    fn default() -> Self {
        Self::new()
    }
}
