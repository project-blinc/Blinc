//! Ordered native layer composition over an existing target.
use super::{GpuRenderer, LayerTexture, PrimitiveBatch};
use crate::primitives::{ClipType, FillType, LayerCommand, PathBatch, PrimitiveType};
use blinc_core::{BlendMode, LayerConfig, LayerEffect};
use std::borrow::Cow;
use std::collections::BTreeSet;

mod alpha;
pub(super) use alpha::AlphaPass;

#[cfg(test)]
mod tests;

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
    Run(Cursor, Cursor),
    Image(usize),
    Layer(LayerConfig, Vec<Node>),
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

fn bounds(items: &[Node], batch: &PrimitiveBatch, viewport: Bounds) -> Option<Bounds> {
    items
        .iter()
        .filter_map(|node| match node {
            Node::Layer(config, children) => {
                bounds(children, batch, viewport).map(|b| b.expand(expansion(config)))
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
            Node::Layer(_, children) => add(common_clip(children, batch)),
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
    pub(crate) fn composite_subtree_bounds(
        batch: &PrimitiveBatch,
        viewport: [f32; 2],
    ) -> Option<[f32; 4]> {
        bounds(
            &nodes(batch),
            batch,
            Bounds {
                min: [0.0; 2],
                max: viewport,
            },
        )
        .map(|b| [b.min[0], b.min[1], b.max[0] - b.min[0], b.max[1] - b.min[1]])
    }

    pub(super) fn render_composite_subtree(
        &mut self,
        batch: &PrimitiveBatch,
        origin: (f32, f32),
        size: (f32, f32),
    ) -> (LayerTexture, (u32, u32)) {
        let extent = (size.0.ceil().max(1.0) as u32, size.1.ceil().max(1.0) as u32);
        self.layer_texture_cache.evict_oversized();
        let target = self
            .layer_texture_cache
            .acquire_exact(&self.device, extent, false);
        let viewport = self.viewport_size;
        self.viewport_size = extent;
        self.render_with_clear_simple(&target.view, &PrimitiveBatch::new(), [0.0; 4]);
        self.draw_overlay_nodes(&target.view, batch, &nodes(batch), [origin.0, origin.1], 1);
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
            bytemuck::bytes_of(&crate::primitives::Uniforms {
                viewport_size: [viewport.0 as f32, viewport.1 as f32],
                _padding: [0.0; 2],
            }),
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
        self.layer_texture_cache.evict_oversized();
        self.draw_overlay_nodes(target, batch, &nodes(batch), [0.0; 2], samples);
        self.update_aux_data_buffer(batch);
        self.queue.write_buffer(
            &self.buffers.uniforms,
            0,
            bytemuck::bytes_of(&crate::primitives::Uniforms {
                viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
                _padding: [0.0; 2],
            }),
        );
    }

    fn draw_overlay_nodes(
        &mut self,
        target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
        items: &[Node],
        origin: [f32; 2],
        samples: u32,
    ) {
        let size = self.viewport_size;
        let viewport = Bounds {
            min: origin,
            max: [origin[0] + size.0 as f32, origin[1] + size.1 as f32],
        };
        for node in items {
            match node {
                Node::Image(index) => {
                    self.render_dynamic_image_at(target, &batch.dynamic_images[*index], origin)
                }
                Node::Run(start, end) => {
                    let run = translated(batch, *start, *end, origin);
                    self.render_overlay_msaa_flat(target, &run, samples);
                }
                Node::Layer(config, children) => {
                    if config.opacity <= 0.0 {
                        continue;
                    }
                    let effects = active_effects(&config.effects);
                    if effects.is_empty()
                        && config.opacity == 1.0
                        && config.blend_mode == BlendMode::Normal
                        && config.transform_3d.is_none()
                    {
                        self.draw_overlay_nodes(target, batch, children, origin, samples);
                        continue;
                    }
                    let Some(b) = bounds(children, batch, viewport) else {
                        continue;
                    };
                    let e = expansion(config);
                    let b = b.expand(e);
                    if b.max[0] <= viewport.min[0]
                        || b.max[1] <= viewport.min[1]
                        || b.min[0] >= viewport.max[0]
                        || b.min[1] >= viewport.max[1]
                    {
                        continue;
                    }
                    // Retain off-target inputs that a blur or shifted shadow can read.
                    let margin = e.into_iter().fold(0.0, f32::max).ceil();
                    let min = [
                        b.min[0].max(viewport.min[0] - margin).floor(),
                        b.min[1].max(viewport.min[1] - margin).floor(),
                    ];
                    let max = [
                        b.max[0].min(viewport.max[0] + margin).ceil(),
                        b.max[1].min(viewport.max[1] + margin).ceil(),
                    ];
                    let tight = (
                        (max[0] - min[0]).max(1.0) as u32,
                        (max[1] - min[1]).max(1.0) as u32,
                    );
                    let texture =
                        self.layer_texture_cache
                            .acquire_exact(&self.device, tight, false);
                    // Use the acquired extent as viewport; source pixels stay one-to-one.
                    self.viewport_size = texture.size;
                    self.render_with_clear_simple(&texture.view, &PrimitiveBatch::new(), [0.0; 4]);
                    self.draw_overlay_nodes(&texture.view, batch, children, min, samples);
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
                    if let Some(effected) = effected {
                        self.layer_texture_cache.release(effected);
                    }
                    self.layer_texture_cache.release(straight);
                    self.layer_texture_cache.release(texture);
                }
            }
        }
    }
}
