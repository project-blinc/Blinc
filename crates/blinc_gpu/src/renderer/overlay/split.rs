//! Drawing a batch in parts, so that content drawn apart from it can go between them.
use super::{
    Cursor, GpuRenderer, Node, PrimitiveBatch, glass_backdrop, named, nodes, resolved_batch,
};
use std::ops::Range;

/// Where an item of a batch sits in its primitives: the range it draws, empty for an item that
/// draws no primitives of the batch (an image, a mesh), placed where it was painted.
fn span(node: &Node, at: usize) -> Range<usize> {
    match node {
        Node::Run(start, end) => start.primitive..end.primitive,
        Node::Layer(_, children) => {
            let mut range: Option<Range<usize>> = None;
            let mut cursor = at;
            for child in children {
                let r = span(child, cursor);
                cursor = r.end;
                range = Some(match range {
                    None => r,
                    Some(range) => range.start.min(r.start)..range.end.max(r.end),
                });
            }
            range.unwrap_or(at..at)
        }
        _ => at..at,
    }
}

/// The items of `items` that draw within `part`, a run cut where `part` ends inside it. `None`
/// when a boundary of `part` falls inside a group, or inside a run that has path geometry.
fn select(items: &[Node], part: &Range<usize>, total: usize) -> Option<Vec<Node>> {
    let mut out = Vec::new();
    let mut cursor = 0;
    for node in items {
        let r = span(node, cursor);
        cursor = r.end;
        // The last part also takes what was painted after the last primitive.
        let inside =
            |i: usize| part.start <= i && (i < part.end || (i == total && part.end == total));
        if r.is_empty() {
            // An item that draws no primitives goes with what was painted after it.
            if inside(r.start) {
                out.push(clone_node(node)?);
            }
            continue;
        }
        if r.end <= part.start || r.start >= part.end {
            continue;
        }
        if part.start <= r.start && r.end <= part.end {
            out.push(clone_node(node)?);
            continue;
        }
        let Node::Run(start, end) = node else {
            return None;
        };
        if start.vertex != end.vertex || start.index != end.index {
            return None;
        }
        let cut = |primitive: usize| Cursor {
            primitive,
            vertex: start.vertex,
            index: start.index,
        };
        out.push(Node::Run(
            cut(start.primitive.max(part.start)),
            cut(end.primitive.min(part.end)),
        ));
    }
    Some(out)
}

fn clone_node(node: &Node) -> Option<Node> {
    Some(match node {
        Node::Run(start, end) => Node::Run(*start, *end),
        Node::Image(index) => Node::Image(*index),
        Node::Layer(config, children) => Node::Layer(
            config.clone(),
            children.iter().map(clone_node).collect::<Option<_>>()?,
        ),
        Node::Glass(glass) => Node::Glass(glass.clone()),
        Node::Meshes(scene) => Node::Meshes(scene.clone()),
        Node::GpuPass(pass) => Node::GpuPass(pass.clone()),
        // A named sample reads a layer that another part may draw.
        Node::Sample(_) => return None,
    })
}

impl GpuRenderer {
    /// Whether `batch` can be drawn in parts divided at each of `at` (ascending primitive
    /// indices) by [`Self::render_overlay_part`]: no division falls inside a group or a run of
    /// paths, and nothing in it reads another part (a named sample, a glass backdrop).
    pub fn overlay_divides_at(&self, batch: &PrimitiveBatch, at: &[usize]) -> bool {
        let batch = resolved_batch(batch);
        // Unordered groups take another path when drawn whole.
        if batch.has_background_glass()
            || (batch.has_layer_commands() && !batch.requires_ordered_rendering())
        {
            return false;
        }
        let items = nodes(&batch);
        if glass_backdrop::nested_frame(&items).is_some() {
            return false;
        }
        let mut start = 0;
        at.iter()
            .copied()
            .chain(std::iter::once(batch.primitives.len()))
            .all(|end| {
                let part = start..end;
                start = end;
                select(&items, &part, batch.primitives.len()).is_some()
            })
    }

    /// Draws the primitives of `batch` in `part`, and the items painted among them, as
    /// `render_overlay_msaa` at one sample draws the whole batch. Draws nothing when `part`
    /// does not divide the batch; [`Self::overlay_divides_at`] says whether it does.
    pub fn render_overlay_part(
        &mut self,
        target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
        part: Range<usize>,
    ) {
        let batch = resolved_batch(batch);
        let batch = self.with_node_rows_applied(batch.as_ref());
        let batch = batch.as_ref();
        let items = nodes(batch);
        let Some(items) = select(&items, &part, batch.primitives.len()) else {
            return;
        };
        self.layer_texture_cache.evict_oversized();
        let mut outputs = named::Outputs::new(&items);
        self.draw_overlay_nodes(target, batch, &items, [0.0; 2], 1, &mut outputs);
        for (_, output) in outputs.textures {
            self.layer_texture_cache.release(output.texture);
        }
        self.update_aux_data_buffer(batch);
        self.uploads.write(
            &self.device,
            &self.queue,
            &self.buffers.uniforms,
            0,
            bytemuck::bytes_of(&crate::primitives::Uniforms::new([
                self.viewport_size.0 as f32,
                self.viewport_size.1 as f32,
            ])),
        );
    }
}
