//! Ordered native layer composition over an existing target.
use super::{GpuRenderer, LayerTexture, PrimitiveBatch};
use crate::primitives::{ClipType, FillType, LayerCommand, PathBatch, PrimitiveType};
use blinc_core::{BlendMode, LayerConfig, LayerEffect};
use std::borrow::Cow;
use std::collections::BTreeSet;

mod alpha;
mod glass;
mod glass_backdrop;
mod gpu_pass;
mod named;
pub(super) use alpha::AlphaPass;

#[cfg(test)]
mod tests;

fn resolved_batch(batch: &PrimitiveBatch) -> Cow<'_, PrimitiveBatch> {
    if batch.cross_stream_groups.captures.is_empty() {
        Cow::Borrowed(batch)
    } else {
        let mut batch = batch.clone();
        batch.resolve_cross_stream_groups();
        Cow::Owned(batch)
    }
}

fn active_effects(effects: &[LayerEffect]) -> Cow<'_, [LayerEffect]> {
    let noop = |effect: &&LayerEffect| {
        matches!(effect, LayerEffect::Blur {radius,..} if *radius<=0.0)
            || matches!(effect, LayerEffect::Glow {opacity,..} if *opacity<=0.0)
    };
    if effects.iter().any(|effect| noop(&effect)) {
        Cow::Owned(
            effects
                .iter()
                .filter(|effect| !noop(effect))
                .cloned()
                .collect(),
        )
    } else {
        Cow::Borrowed(effects)
    }
}

#[derive(Clone, Copy, Default)]
struct Cursor {
    primitive: usize,
    vertex: usize,
    index: usize,
}

enum Node {
    Glass(Box<crate::paint::GroupGlass>),
    Run(Cursor, Cursor),
    Image(usize),
    Layer(LayerConfig, Vec<Node>),
    Sample(Box<named::Sample>),
    GpuPass(Box<crate::paint::GroupGpuPass>),
    Meshes(Box<crate::paint::GroupMeshes>),
}

fn nodes(batch: &PrimitiveBatch) -> Vec<Node> {
    let mut stack = vec![Vec::new()];
    let mut configs = Vec::new();
    let mut cursor = Cursor::default();
    let end = Cursor {
        primitive: batch.primitives.len(),
        vertex: batch.paths.vertices.len(),
        index: batch.paths.indices.len(),
    };
    let run = |out: &mut Vec<Node>, start: Cursor, end: Cursor| {
        if end.primitive > start.primitive || end.index > start.index {
            out.push(Node::Run(start, end));
        }
    };
    let mut images = batch
        .dynamic_images
        .iter()
        .enumerate()
        .filter_map(|(index, image)| image.paint_order.map(|order| (index, order)))
        .peekable();
    for command_index in 0..=batch.layer_commands.len() {
        while images.peek().is_some_and(|(_, order)| {
            order.layer_command_count <= command_index
                || command_index == batch.layer_commands.len()
        }) {
            let (index, order) = images.next().unwrap();
            let next = Cursor {
                primitive: order
                    .primitive_index
                    .min(end.primitive)
                    .max(cursor.primitive),
                vertex: order.path_vertex_index.min(end.vertex).max(cursor.vertex),
                index: order.path_index_count.min(end.index).max(cursor.index),
            };
            run(stack.last_mut().unwrap(), cursor, next);
            stack.last_mut().unwrap().push(Node::Image(index));
            cursor = next;
        }
        let Some(entry) = batch.layer_commands.get(command_index) else {
            break;
        };
        let next = Cursor {
            primitive: entry
                .primitive_index
                .min(end.primitive)
                .max(cursor.primitive),
            vertex: entry.path_vertex_index.min(end.vertex).max(cursor.vertex),
            index: entry.path_index_count.min(end.index).max(cursor.index),
        };
        run(stack.last_mut().unwrap(), cursor, next);
        cursor = next;
        match &entry.command {
            LayerCommand::Glass(glass) => {
                stack.last_mut().unwrap().push(Node::Glass(glass.clone()))
            }
            LayerCommand::Meshes(scene) => {
                stack.last_mut().unwrap().push(Node::Meshes(scene.clone()))
            }
            LayerCommand::GpuPass(pass) => {
                stack.last_mut().unwrap().push(Node::GpuPass(pass.clone()))
            }
            LayerCommand::Push { config } => {
                configs.push(config.clone());
                stack.push(Vec::new());
            }
            LayerCommand::Pop if stack.len() > 1 => {
                let children = stack.pop().unwrap();
                stack
                    .last_mut()
                    .unwrap()
                    .push(Node::Layer(configs.pop().unwrap(), children));
            }
            LayerCommand::Sample {
                id,
                source,
                dest,
                paint,
            } => {
                stack
                    .last_mut()
                    .unwrap()
                    .push(Node::Sample(Box::new(named::Sample::new(
                        *id,
                        *source,
                        *dest,
                        paint.as_deref(),
                    ))));
            }
            _ => {}
        }
    }
    run(stack.last_mut().unwrap(), cursor, end);
    while stack.len() > 1 {
        let children = stack.pop().unwrap();
        stack
            .last_mut()
            .unwrap()
            .push(Node::Layer(configs.pop().unwrap(), children));
    }
    stack.pop().unwrap()
}

#[derive(Clone, Copy)]
struct Bounds {
    min: [f32; 2],
    max: [f32; 2],
}

impl Bounds {
    fn union(self, other: Self) -> Self {
        Self {
            min: [self.min[0].min(other.min[0]), self.min[1].min(other.min[1])],
            max: [self.max[0].max(other.max[0]), self.max[1].max(other.max[1])],
        }
    }
    fn expand(self, e: [f32; 4]) -> Self {
        Self {
            min: [self.min[0] - e[0], self.min[1] - e[1]],
            max: [self.max[0] + e[2], self.max[1] + e[3]],
        }
    }
}

fn expansion(config: &LayerConfig) -> [f32; 4] {
    config.effects.iter().fold([0.0; 4], |mut total, effect| {
        let (l, t, r, b) = GpuRenderer::calculate_effect_expansion(std::slice::from_ref(effect));
        for (sum, value) in total.iter_mut().zip([l, t, r, b]) {
            *sum += value.max(0.0);
        }
        total
    })
}

fn bounds(
    items: &[Node],
    batch: &PrimitiveBatch,
    viewport: Bounds,
    wanted: &std::collections::HashSet<blinc_core::LayerId>,
) -> Option<Bounds> {
    items
        .iter()
        .filter_map(|node| match node {
            Node::Glass(glass) => {
                let rect = glass.paint.screen_bounds();
                Some(Bounds {
                    min: [rect.x(), rect.y()],
                    max: [rect.x() + rect.width(), rect.y() + rect.height()],
                })
            }
            Node::Meshes(scene) => Some(Bounds {
                min: [0.0; 2],
                max: [scene.frame_size.0 as f32, scene.frame_size.1 as f32],
            }),
            Node::GpuPass(pass) => Some(Bounds {
                min: [0.0; 2],
                max: [pass.frame_size.0 as f32, pass.frame_size.1 as f32],
            }),
            Node::Layer(config, children) => {
                let bounds = if config.id.is_some_and(|id| wanted.contains(&id)) {
                    named::frame(config, viewport)
                } else {
                    bounds(children, batch, viewport, wanted)
                };
                bounds.map(|b| b.expand(expansion(config)))
            }
            Node::Sample(sample) => {
                let image = &sample.paint;
                if !crate::primitives::DynamicImage::valid_geometry(image.dest, image.transform) {
                    return None;
                }
                let rect = image.screen_bounds();
                Some(Bounds {
                    min: [rect.x(), rect.y()],
                    max: [rect.x() + rect.width(), rect.y() + rect.height()],
                })
            }
            Node::Image(index) => {
                let image = &batch.dynamic_images[*index];
                if !crate::primitives::DynamicImage::valid_geometry(image.dest, image.transform) {
                    return None;
                }
                let rect = image.screen_bounds();
                Some(Bounds {
                    min: [rect.x(), rect.y()],
                    max: [rect.x() + rect.width(), rect.y() + rect.height()],
                })
            }
            Node::Run(start, end) => {
                let mut result: Option<Bounds> = None;
                let mut add = |b| result = Some(result.map_or(b, |a| a.union(b)));
                for p in &batch.primitives[start.primitive..end.primitive] {
                    if p.perspective[2] > 0.0 || p.perspective[3] > 0.0 {
                        add(viewport);
                        continue;
                    }
                    let [x, y, w, h] = p.bounds;
                    let [a, b, c, d] = p.local_affine;
                    let mut corners = Bounds {
                        min: [f32::INFINITY; 2],
                        max: [f32::NEG_INFINITY; 2],
                    };
                    for (u, v) in [
                        (-w / 2.0, -h / 2.0),
                        (w / 2.0, -h / 2.0),
                        (-w / 2.0, h / 2.0),
                        (w / 2.0, h / 2.0),
                    ] {
                        let point = [x + w / 2.0 + a * u + c * v, y + h / 2.0 + b * u + d * v];
                        for (i, value) in point.into_iter().enumerate() {
                            corners.min[i] = corners.min[i].min(value);
                            corners.max[i] = corners.max[i].max(value);
                        }
                    }
                    let shadow = if (3..=6).contains(&p.type_info[0]) {
                        p.shadow[2].max(0.0) * 3.0
                            + p.shadow[3].abs()
                            + p.shadow[0].abs().max(p.shadow[1].abs())
                    } else {
                        0.0
                    };
                    add(corners.expand([shadow + 1.0; 4]));
                }
                for v in &batch.paths.vertices[start.vertex..end.vertex] {
                    add(Bounds {
                        min: v.position,
                        max: v.position,
                    });
                }
                result
            }
        })
        .reduce(Bounds::union)
}

fn has_clip(b: [f32; 4]) -> bool {
    b[0] > -5000.0 && b[2] < 90000.0
}

fn common_clip(items: &[Node], batch: &PrimitiveBatch) -> Option<([f32; 4], [f32; 4])> {
    let mut first = None;
    let mut matching = true;
    let mut add = |clip| {
        if let Some(previous) = first {
            matching &= previous == clip;
        } else {
            first = Some(clip);
        }
    };
    for node in items {
        match node {
            Node::Glass(glass) => {
                let image = &glass.paint;
                let shaped = image.clip_aux.get(4).is_some_and(|info| info[2] > 0.5);
                add(has_clip(image.clip_bounds).then_some((
                    image.clip_bounds,
                    if shaped { [0.0; 4] } else { image.clip_radius },
                )));
            }
            Node::Meshes(scene) => {
                let image = &scene.paint;
                let shaped = image.clip_aux.get(4).is_some_and(|info| info[2] > 0.5);
                add(has_clip(image.clip_bounds).then_some((
                    image.clip_bounds,
                    if shaped { [0.0; 4] } else { image.clip_radius },
                )));
            }
            Node::GpuPass(pass) => {
                let image = &pass.paint;
                let shaped = image.clip_aux.get(4).is_some_and(|info| info[2] > 0.5);
                add(has_clip(image.clip_bounds).then_some((
                    image.clip_bounds,
                    if shaped { [0.0; 4] } else { image.clip_radius },
                )));
            }
            Node::Layer(_, children) => add(common_clip(children, batch)),
            Node::Sample(sample) => {
                let image = &sample.paint;
                let shaped = image.clip_aux.get(4).is_some_and(|info| info[2] > 0.5);
                add(has_clip(image.clip_bounds).then_some((
                    image.clip_bounds,
                    if shaped { [0.0; 4] } else { image.clip_radius },
                )));
            }
            Node::Image(index) => {
                let image = &batch.dynamic_images[*index];
                let shaped = image.clip_aux.get(4).is_some_and(|info| info[2] > 0.5);
                add(has_clip(image.clip_bounds).then_some((
                    image.clip_bounds,
                    if shaped { [0.0; 4] } else { image.clip_radius },
                )));
            }

            Node::Run(start, end) => {
                for p in &batch.primitives[start.primitive..end.primitive] {
                    add(has_clip(p.clip_bounds).then_some((
                        p.clip_bounds,
                        if p.type_info[2] == ClipType::Rect as u32 {
                            p.clip_radius
                        } else {
                            [0.0; 4]
                        },
                    )));
                }
                for v in &batch.paths.vertices[start.vertex..end.vertex] {
                    add(has_clip(v.clip_bounds).then_some((
                        v.clip_bounds,
                        if v.clip_type == 1 {
                            v.clip_radius
                        } else {
                            [0.0; 4]
                        },
                    )));
                }
            }
        }
    }
    if matching { first.flatten() } else { None }
}

pub(super) fn translate_clip_frames(
    aux: &mut [[f32; 4]],
    pointers: impl Iterator<Item = u32>,
    origin: [f32; 2],
) -> bool {
    let mut frames = BTreeSet::new();
    let mut changed = false;
    for pointer in pointers.filter(|p| *p > 0) {
        let at = pointer as usize - 1;
        if at + 1 < aux.len() && frames.insert(at) {
            let [a, b, c, d] = aux[at];
            aux[at + 1][0] += a * origin[0] + c * origin[1];
            aux[at + 1][1] += b * origin[0] + d * origin[1];
            changed = true;
        }
    }
    changed
}

fn translated(
    batch: &PrimitiveBatch,
    start: Cursor,
    end: Cursor,
    origin: [f32; 2],
) -> PrimitiveBatch {
    let [dx, dy] = origin.map(|v| -v);
    let mut out = PrimitiveBatch::new();
    out.aux_data.clone_from(&batch.aux_data);
    let style = &batch.paths;
    out.paths = PathBatch {
        vertices: Vec::new(),
        indices: Vec::new(),
        clip_bounds: style.clip_bounds,
        clip_radius: style.clip_radius,
        clip_type: style.clip_type,
        use_gradient_texture: style.use_gradient_texture,
        gradient_stops: style.gradient_stops.clone(),
        use_image_texture: style.use_image_texture,
        image_source: style.image_source.clone(),
        image_uv_bounds: style.image_uv_bounds,
        use_glass_effect: style.use_glass_effect,
        glass_params: style.glass_params,
        glass_tint: style.glass_tint,
    };
    if has_clip(out.paths.clip_bounds) {
        out.paths.clip_bounds[0] += dx;
        out.paths.clip_bounds[1] += dy;
    }
    if matches!(out.paths.clip_type, 2 | 3) {
        out.paths.clip_radius[0] += dx;
        out.paths.clip_radius[1] += dy;
    }
    let mut meshes = BTreeSet::new();
    for p in &batch.primitives[start.primitive..end.primitive] {
        let mut p = *p;
        p.bounds[0] += dx;
        p.bounds[1] += dy;
        if has_clip(p.clip_bounds) {
            p.clip_bounds[0] += dx;
            p.clip_bounds[1] += dy;
        }
        if matches!(p.type_info[2], 2 | 3) {
            p.clip_radius[0] += dx;
            p.clip_radius[1] += dy;
        }
        if p.clip_fade_bounds[2] > 0.0 {
            p.clip_fade_bounds[0] += dx;
            p.clip_fade_bounds[1] += dy;
        }
        if p.type_info[0] != PrimitiveType::Text as u32 {
            if p.type_info[1] == FillType::LinearGradient as u32 {
                p.gradient_params[2] += dx;
                p.gradient_params[3] += dy;
            }
            if p.type_info[1] != FillType::Solid as u32 {
                p.gradient_params[0] += dx;
                p.gradient_params[1] += dy;
            }
        }
        if p.type_info[0] == PrimitiveType::Mesh as u32 {
            let at = p.border[2] as usize;
            if at + 1 < out.aux_data.len() && meshes.insert(at) {
                for pair in [0, 2] {
                    out.aux_data[at][pair] += dx;
                    out.aux_data[at][pair + 1] += dy;
                }
                out.aux_data[at + 1][0] += dx;
                out.aux_data[at + 1][1] += dy;
            }
        }
        out.primitives.push(p);
    }
    translate_clip_frames(
        &mut out.aux_data,
        batch.primitives[start.primitive..end.primitive]
            .iter()
            .map(|p| p.clip_shape[0] as u32)
            .chain(
                batch.paths.vertices[start.vertex..end.vertex]
                    .iter()
                    .map(|v| v.clip_shape),
            ),
        origin,
    );
    for v in &batch.paths.vertices[start.vertex..end.vertex] {
        let mut v = *v;
        v.position[0] += dx;
        v.position[1] += dy;
        if has_clip(v.clip_bounds) {
            v.clip_bounds[0] += dx;
            v.clip_bounds[1] += dy;
        }
        if matches!(v.clip_type, 2 | 3) {
            v.clip_radius[0] += dx;
            v.clip_radius[1] += dy;
        }
        // UVs and path gradient geometry share the unchanged path-local frame.
        out.paths.vertices.push(v);
    }
    out.paths.indices.extend(
        batch.paths.indices[start.index..end.index]
            .iter()
            .map(|i| i.saturating_sub(start.vertex as u32)),
    );
    out
}

impl GpuRenderer {
    /// Nested groups must composite into their parent, not as independent ranges.
    pub(super) fn has_nested_layers(batch: &PrimitiveBatch) -> bool {
        let mut depth = 0_u32;
        batch
            .layer_commands
            .iter()
            .any(|entry| match &entry.command {
                LayerCommand::Push { .. } => {
                    depth += 1;
                    depth > 1
                }
                LayerCommand::Pop => {
                    depth = depth.saturating_sub(1);
                    false
                }
                LayerCommand::Glass(_)
                | LayerCommand::Meshes(_)
                | LayerCommand::GpuPass(_)
                | LayerCommand::Sample { .. }
                | LayerCommand::DeferredText { .. }
                | LayerCommand::DeferredSvg { .. }
                | LayerCommand::DeferredImage { .. } => false,
            })
    }

    pub(crate) fn composite_subtree_bounds(
        batch: &PrimitiveBatch,
        viewport: [f32; 2],
    ) -> Option<[f32; 4]> {
        let batch = resolved_batch(batch);
        let batch = batch.as_ref();
        let items = nodes(batch);
        let outputs = named::Outputs::new(&items);
        bounds(
            &items,
            batch,
            Bounds {
                min: [0.0; 2],
                max: viewport,
            },
            &outputs.wanted,
        )
        .map(|b| [b.min[0], b.min[1], b.max[0] - b.min[0], b.max[1] - b.min[1]])
    }

    pub(super) fn render_composite_subtree(
        &mut self,
        batch: &PrimitiveBatch,
        origin: (f32, f32),
        size: (f32, f32),
    ) -> (LayerTexture, (u32, u32)) {
        self.render_composite_subtree_with_backdrop(batch, origin, size, None)
    }

    fn render_composite_subtree_with_backdrop(
        &mut self,
        batch: &PrimitiveBatch,
        origin: (f32, f32),
        size: (f32, f32),
        backdrop: Option<&wgpu::TextureView>,
    ) -> (LayerTexture, (u32, u32)) {
        let batch = resolved_batch(batch);
        let batch = batch.as_ref();
        let extent = (size.0.ceil().max(1.0) as u32, size.1.ceil().max(1.0) as u32);
        self.layer_texture_cache.evict_oversized();
        let target = self
            .layer_texture_cache
            .acquire_exact(&self.device, extent, false);
        let viewport = self.viewport_size;
        self.viewport_size = extent;
        self.render_with_clear_simple(&target.view, &PrimitiveBatch::new(), [0.0; 4]);
        let items = nodes(batch);
        let mut outputs = named::Outputs::new(&items);
        outputs.local_glass_frame = glass_backdrop::frame(&items);
        outputs.glass_backdrop = backdrop.cloned();
        outputs.prepared_glass_backdrop = backdrop.cloned();
        self.draw_overlay_nodes(
            &target.view,
            batch,
            &items,
            [origin.0, origin.1],
            1,
            &mut outputs,
        );
        for (_, output) in outputs.textures {
            self.layer_texture_cache.release(output.texture);
        }
        let straight = self
            .layer_texture_cache
            .acquire_exact(&self.device, extent, false);
        self.ensure_overlay_alpha();
        self.overlay_alpha.as_ref().unwrap().straighten(
            &self.device,
            &self.queue,
            &self.bind_group_layouts.composite,
            &self.path_image_sampler,
            &target,
            &straight,
        );
        self.layer_texture_cache.release(target);
        self.viewport_size = viewport;
        self.update_aux_data_buffer(batch);
        self.queue.write_buffer(
            &self.buffers.uniforms,
            0,
            bytemuck::bytes_of(&crate::primitives::Uniforms::new([
                viewport.0 as f32,
                viewport.1 as f32,
            ])),
        );
        (straight, extent)
    }

    pub(super) fn ensure_overlay_alpha(&mut self) {
        if self.overlay_alpha.is_none() {
            self.overlay_alpha = Some(AlphaPass::new(
                &self.device,
                self.texture_format,
                &self.bind_group_layouts.composite,
            ));
        }
    }
    pub(super) fn render_layered_overlay_msaa(
        &mut self,
        target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
        samples: u32,
    ) {
        self.render_layered_overlay_with_backdrop(target, batch, samples, None);
    }

    pub(super) fn render_layered_overlay_with_backdrop(
        &mut self,
        target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
        samples: u32,
        backdrop: Option<&wgpu::TextureView>,
    ) {
        let batch = resolved_batch(batch);
        let batch = batch.as_ref();
        let items = nodes(batch);
        if (backdrop.is_none() && batch.has_background_glass())
            || (backdrop.is_some() && glass_backdrop::nested_frame(&items).is_some())
        {
            let size = self.viewport_size;
            let (texture, _) = self.render_composite_subtree_with_backdrop(
                batch,
                (0.0, 0.0),
                (size.0 as f32, size.1 as f32),
                backdrop,
            );
            self.render_images(
                target,
                &texture.view,
                &[crate::GpuImageInstance::new(
                    0.0,
                    0.0,
                    size.0 as f32,
                    size.1 as f32,
                )],
            );
            self.layer_texture_cache.release(texture);
            return;
        }
        self.layer_texture_cache.evict_oversized();
        let mut outputs = named::Outputs::new(&items);
        outputs.glass_backdrop = backdrop.cloned();
        self.draw_overlay_nodes(target, batch, &items, [0.0; 2], samples, &mut outputs);
        for (_, output) in outputs.textures {
            self.layer_texture_cache.release(output.texture);
        }
        self.update_aux_data_buffer(batch);
        self.queue.write_buffer(
            &self.buffers.uniforms,
            0,
            bytemuck::bytes_of(&crate::primitives::Uniforms::new([
                self.viewport_size.0 as f32,
                self.viewport_size.1 as f32,
            ])),
        );
    }

    fn draw_overlay_nodes(
        &mut self,
        target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
        items: &[Node],
        origin: [f32; 2],
        samples: u32,
        outputs: &mut named::Outputs,
    ) {
        let size = self.viewport_size;
        let viewport = Bounds {
            min: origin,
            max: [origin[0] + size.0 as f32, origin[1] + size.1 as f32],
        };
        for node in items {
            match node {
                Node::Glass(glass) => {
                    if let Some(frame) = outputs
                        .local_glass_frame
                        .filter(|_| outputs.prepared_glass_backdrop.is_none() || glass.nested)
                    {
                        let backdrop = self.capture_glass_backdrop(
                            target,
                            origin,
                            frame,
                            outputs.glass_backdrop.as_ref(),
                        );
                        self.draw_group_glass(target, &backdrop.view, glass, origin);
                        self.layer_texture_cache.release(backdrop);
                    } else if let Some(backdrop) = outputs
                        .prepared_glass_backdrop
                        .as_ref()
                        .or(outputs.glass_backdrop.as_ref())
                    {
                        self.draw_group_glass(target, backdrop, glass, origin);
                    }
                }
                Node::Meshes(scene) => self.draw_group_meshes(target, scene, origin),
                Node::GpuPass(pass) => self.draw_group_gpu_pass(target, pass, origin),
                Node::Sample(sample) => self.draw_named_sample(target, sample, origin, outputs),
                Node::Image(index) => {
                    self.render_dynamic_image_at(target, &batch.dynamic_images[*index], origin)
                }
                Node::Run(start, end) => {
                    let run = translated(batch, *start, *end, origin);
                    self.render_overlay_msaa_flat(target, &run, samples);
                }
                Node::Layer(config, children) => {
                    let named_id = config.id.filter(|id| outputs.wanted.contains(id));
                    if config.opacity <= 0.0 {
                        if let Some(old) = named_id.and_then(|id| outputs.textures.remove(&id)) {
                            self.layer_texture_cache.release(old.texture);
                        }
                        continue;
                    }
                    let effects = active_effects(&config.effects);
                    if effects.is_empty()
                        && config.opacity == 1.0
                        && config.blend_mode == BlendMode::Normal
                        && config.transform_3d.is_none()
                        && named_id.is_none()
                        && !gpu_pass::has_external_gpu_draws(children)
                    {
                        self.draw_overlay_nodes(target, batch, children, origin, samples, outputs);
                        continue;
                    }
                    // Keep primitive-only perspective layers on the native bake path.
                    if config.transform_3d.is_some()
                        && named_id.is_none()
                        && let [Node::Run(start, end)] = children.as_slice()
                        && start.index == end.index
                    {
                        let mut native = PrimitiveBatch::new();
                        native.push_layer_command(LayerCommand::Push {
                            config: config.clone(),
                        });
                        native.merge(translated(batch, *start, *end, origin));
                        native.push_layer_command(LayerCommand::Pop);
                        self.render_overlay_with_layer_effects(target, &native);
                        continue;
                    }
                    let content_bounds = if named_id.is_some() {
                        named::frame(config, viewport)
                    } else {
                        bounds(children, batch, viewport, &outputs.wanted)
                    };
                    let Some(b) = content_bounds else {
                        continue;
                    };
                    let e = expansion(config);
                    let b = b.expand(e);
                    if named_id.is_none()
                        && (b.max[0] <= viewport.min[0]
                            || b.max[1] <= viewport.min[1]
                            || b.min[0] >= viewport.max[0]
                            || b.min[1] >= viewport.max[1])
                    {
                        continue;
                    }
                    // Retain off-target inputs that a blur or shifted shadow can read.
                    let margin = e.into_iter().fold(0.0, f32::max).ceil();
                    let min = if named_id.is_some() {
                        b.min.map(f32::floor)
                    } else {
                        [
                            b.min[0].max(viewport.min[0] - margin).floor(),
                            b.min[1].max(viewport.min[1] - margin).floor(),
                        ]
                    };
                    let max = if named_id.is_some() {
                        b.max.map(f32::ceil)
                    } else {
                        [
                            b.max[0].min(viewport.max[0] + margin).ceil(),
                            b.max[1].min(viewport.max[1] + margin).ceil(),
                        ]
                    };
                    let tight = (
                        (max[0] - min[0]).max(1.0) as u32,
                        (max[1] - min[1]).max(1.0) as u32,
                    );
                    let limit = self.device.limits().max_texture_dimension_2d;
                    if tight.0 > limit || tight.1 > limit {
                        continue;
                    }
                    let backdrop = outputs
                        .local_glass_frame
                        .filter(|_| glass_backdrop::frame(children).is_some())
                        .map(|frame| {
                            self.capture_glass_backdrop(
                                target,
                                origin,
                                frame,
                                outputs.glass_backdrop.as_ref(),
                            )
                        });
                    let previous = backdrop
                        .as_ref()
                        .map(|backdrop| outputs.glass_backdrop.replace(backdrop.view.clone()));
                    let texture =
                        self.layer_texture_cache
                            .acquire_exact(&self.device, tight, false);
                    // Use the acquired extent as viewport; source pixels stay one-to-one.
                    self.viewport_size = texture.size;
                    self.render_with_clear_simple(&texture.view, &PrimitiveBatch::new(), [0.0; 4]);
                    self.draw_overlay_nodes(&texture.view, batch, children, min, samples, outputs);
                    if let Some(previous) = previous {
                        outputs.glass_backdrop = previous;
                    }
                    if let Some(backdrop) = backdrop {
                        self.layer_texture_cache.release(backdrop);
                    }
                    self.viewport_size = size;
                    let straight =
                        self.layer_texture_cache
                            .acquire_exact(&self.device, texture.size, false);
                    self.ensure_overlay_alpha();
                    self.overlay_alpha.as_ref().unwrap().straighten(
                        &self.device,
                        &self.queue,
                        &self.bind_group_layouts.composite,
                        &self.path_image_sampler,
                        &texture,
                        &straight,
                    );
                    let effected: Option<LayerTexture> = (!effects.is_empty())
                        .then(|| self.apply_layer_effects(&straight, &effects));
                    let source = effected.as_ref().unwrap_or(&straight);
                    let clip = common_clip(children, batch).map(|(mut bounds, radius)| {
                        bounds[0] -= origin[0];
                        bounds[1] -= origin[1];
                        (bounds, radius)
                    });
                    self.blit_tight_texture_to_target(
                        &source.view,
                        source.size,
                        target,
                        (min[0] - origin[0], min[1] - origin[1]),
                        (tight.0 as f32, tight.1 as f32),
                        config.opacity,
                        config.blend_mode,
                        clip,
                        config.transform_3d,
                        0.0,
                    );
                    let output = if let Some(effected) = effected {
                        self.layer_texture_cache.release(straight);
                        effected
                    } else {
                        straight
                    };
                    if let Some(id) = named_id {
                        let local_origin = config.position.map_or(origin, |p| [p.x, p.y]);
                        if let Some(old) = outputs.textures.insert(
                            id,
                            named::Output {
                                texture: output,
                                origin: min,
                                local_origin,
                                opacity: config.opacity,
                            },
                        ) {
                            self.layer_texture_cache.release(old.texture);
                        }
                    } else {
                        self.layer_texture_cache.release(output);
                    }
                    self.layer_texture_cache.release(texture);
                }
            }
        }
    }
}
