//! Moving the primitives of motion-bound nodes with rows of the GPU node table, not by patching
//! them.
//!
//! After a full paint each motion binding owns a range of the cached dynamic batch, baked at the
//! binding's values of that moment. [`NodeRows::apply`] gives the primitives of a range the slot
//! of their innermost binding and writes one row per binding holding the change since the bake,
//! so a frame changes rows and leaves the batch alone. Anything the rows cannot express is
//! declined, and the caller patches the batch on the CPU as before.
//!
//! A node has two rows. Row `2 * slot` is for its batch primitives, whose group opacity is a
//! layer command and so is left out. Row `2 * slot + 1` is for the glyphs of the text under it,
//! which draw outside any layer and take the opacity as an alpha ratio. Each row's parent is the
//! same kind of row of the node's nearest bound ancestor.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};

use blinc_gpu::node_table::{NODE_CHAIN_DEPTH, NODE_TABLE_CAPACITY, NodeRow, chain, then};
use blinc_gpu::primitives::LayerCommand;
use blinc_gpu::{GpuPrimitive, GpuRenderer, PrimitiveBatch};

use super::cached_text::TextBatch;
use super::{ImageElement, SvgElement};
use blinc_layout::motion::MotionBindings;
use blinc_layout::renderer::CompositeBindingMeta;
use blinc_layout::tree::LayoutNodeId;

const EPS: f32 = 1e-4;

/// What a frame of [`NodeRows::apply`] did.
pub(super) enum Outcome {
    /// The rows are current. `active` is whether a binding is still moving.
    Applied { active: bool },
    /// The rows cannot draw these bindings; the batch is as it was painted.
    Declined,
}

/// A binding's values: what the primitives of its range were baked at, or what it holds now.
#[derive(Clone, Copy, Debug)]
struct Pose {
    translate: (f32, f32),
    scale: (f32, f32),
    rotation: f32,
    opacity: f32,
}

impl Pose {
    /// The values the CPU patcher reads from a binding. An opacity the binding does not drive
    /// stays at `opacity`.
    fn of(bindings: &MotionBindings, opacity: f32) -> Pose {
        let read = |v: &Option<blinc_animation::SharedAnimatedValue>, default: f32| {
            v.as_ref()
                .and_then(|v| v.lock().ok().map(|g| g.get()))
                .unwrap_or(default)
        };
        let scale = read(&bindings.scale, 1.0);
        Pose {
            translate: (
                read(&bindings.translate_x, 0.0),
                read(&bindings.translate_y, 0.0),
            ),
            scale: (
                read(&bindings.scale_x, 1.0) * scale,
                read(&bindings.scale_y, 1.0) * scale,
            ),
            rotation: bindings
                .get_rotation()
                .map(|deg| deg.to_radians())
                .unwrap_or(0.0),
            opacity: read(&bindings.opacity, opacity),
        }
    }

    fn of_meta(meta: &CompositeBindingMeta) -> Pose {
        Pose {
            translate: meta.last_translate,
            scale: meta.last_scale,
            rotation: meta.last_rotation_rad,
            opacity: meta.last_opacity,
        }
    }

    fn differs(&self, other: &Pose) -> bool {
        (self.translate.0 - other.translate.0).abs() > f32::EPSILON
            || (self.translate.1 - other.translate.1).abs() > f32::EPSILON
            || (self.scale.0 - other.scale.0).abs() > f32::EPSILON
            || (self.scale.1 - other.scale.1).abs() > f32::EPSILON
            || (self.rotation - other.rotation).abs() > f32::EPSILON
            || (self.opacity - other.opacity).abs() > f32::EPSILON
    }
}

/// A binding with a row.
struct Slot {
    node: LayoutNodeId,
    /// The ordinal the walker gave the node, which its primitives carry.
    ordinal: u32,
    /// The slot of the nearest bound ancestor.
    parent: Option<u32>,
    /// The pose the node's primitives were drawn at when the slot was given.
    baseline: Pose,
    /// The world-space pivot of the baseline, in logical pixels.
    centre: (f32, f32),
    /// The layer command whose opacity is this node's, patched on the CPU as a value of its own;
    /// the row then leaves opacity alone.
    layer: Option<usize>,
    /// The clip in force when the node began painting.
    ambient_clip: Option<[f32; 4]>,
    /// The path clip pointer in force when the node began painting, zero for none.
    ambient_path: f32,
    /// Whether a primitive's clip moves with this node's row, which cannot turn a clip.
    clip_moves: bool,
    /// How far the node may move from where it was painted, in logical pixels: a scroll container
    /// has content painted only so far past its viewport.
    limit: Option<f32>,
}

/// Whether a binding is the dynamic batch's to move: its primitives, or its text alone, are in
/// that batch.
fn owned(meta: &CompositeBindingMeta) -> bool {
    meta.dynamic_batch
}

/// The rows of one assignment of slots to a batch.
#[derive(Default)]
pub(super) struct NodeRows {
    /// The `node_slots_epoch` of the batch these slots were given to, zero for none.
    epoch: u64,
    slots: Vec<Slot>,
    /// The rows last written.
    rows: Vec<NodeRow>,
    /// Whether the rows are applied on the CPU, because the renderer has no table.
    cpu: bool,
}

/// The ordinal of the nearest bound node at or above `node`, zero for none.
pub(super) fn ordinal_of(tree: &blinc_layout::RenderTree, node: LayoutNodeId) -> u32 {
    let ordinals = tree.binding_ordinals();
    std::iter::once(node)
        .chain(tree.layout_tree.ancestors(node))
        .find_map(|n| ordinals.get(&n).copied())
        .unwrap_or(0)
}

/// The index of the one `Push` in `batch` that was pushed with `config`, `None` when there is
/// not exactly one. A group that has an id is found by it: equal cards in a grid have the same
/// position, size and opacity.
fn find_group(batch: &PrimitiveBatch, config: &blinc_core::LayerConfig) -> Option<usize> {
    if let Some(id) = config.id {
        let mut pushes = batch
            .layer_commands
            .iter()
            .enumerate()
            .filter(|(_, entry)| matches!(&entry.command, LayerCommand::Push { config: c } if c.id == Some(id)));
        let first = pushes.next()?.0;
        return pushes.next().is_none().then_some(first);
    }
    let mut found = None;
    for (i, entry) in batch.layer_commands.iter().enumerate() {
        let LayerCommand::Push { config: c } = &entry.command else {
            continue;
        };
        let same = c.position == config.position
            && c.size == config.size
            && c.blend_mode == config.blend_mode
            && c.effects.len() == config.effects.len()
            && c.transform_3d.is_some() == config.transform_3d.is_some()
            && (c.opacity - config.opacity).abs() < 1e-4;
        if same {
            if found.is_some() {
                return None;
            }
            found = Some(i);
        }
    }
    found
}

/// Whether `clip` (x, y, width, height) is the clip that was in force at `ambient`. The unbounded
/// rect a primitive carries for no clip is the clip of no ambient one.
fn same_clip(clip: [f32; 4], ambient: Option<[f32; 4]>) -> bool {
    match ambient {
        Some(a) => clip.iter().zip(a).all(|(c, a)| (c - a).abs() < 0.5),
        None => clip[2] >= 50000.0 && clip[3] >= 50000.0,
    }
}

/// What [`NodeRows::resolve_batch`] took out of a batch, to put back after it is drawn.
pub(super) struct ResolvedBatch {
    primitives: Vec<GpuPrimitive>,
    dynamic_images: Vec<blinc_gpu::primitives::DynamicImage>,
    aux_len: usize,
}

impl ResolvedBatch {
    /// Put back what was taken out of `batch`.
    pub(super) fn restore(self, batch: &mut PrimitiveBatch) {
        batch.primitives = self.primitives;
        batch.dynamic_images = self.dynamic_images;
        batch.aux_data.truncate(self.aux_len);
    }
}

/// How a primitive's clip follows the rows.
enum Follow {
    /// It is wholly an ancestor's and stays.
    Stays,
    /// Introduced at this depth of the chain, with nothing around it that stays: it moves whole.
    Moves(u32),
    /// Introduced at this depth and cut by a clip around it that stays.
    Cut(u32),
}

fn next_epoch() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

impl NodeRows {
    /// The row for a binding that went from `slot.baseline` to `now`. `None` when the change is
    /// not a scale, turn and move about the baseline pivot: a non-uniform scale does not commute
    /// with a turn, and a scale from nothing has no ratio.
    ///
    /// `text` asks for the row of the glyphs, not of the batch primitives.
    fn row(slot: &Slot, now: &Pose, dpi: f32, text: bool) -> Option<NodeRow> {
        let was = &slot.baseline;
        // Past the content that was painted there is nothing to move into view.
        if let Some(limit) = slot.limit
            && ((now.translate.0 - was.translate.0).abs() > limit
                || (now.translate.1 - was.translate.1).abs() > limit)
        {
            return None;
        }
        // A group's opacity is a layer command, not a row; glyphs are outside the layer.
        let layered = slot.layer.is_some() && !text;
        let turn = now.rotation - was.rotation;
        let uniform = |s: (f32, f32)| (s.0 - s.1).abs() <= EPS;
        if turn.abs() > EPS && !(uniform(was.scale) && uniform(now.scale)) {
            return None;
        }
        // A clip is a rectangle in the window's axes, which a turn would have to rotate.
        if turn.abs() > EPS && slot.clip_moves {
            return None;
        }
        if was.scale.0.abs() < EPS || was.scale.1.abs() < EPS {
            return None;
        }
        let opacity = if layered {
            1.0
        } else if was.opacity.abs() < EPS {
            if (now.opacity - was.opacity).abs() > EPS {
                return None;
            }
            1.0
        } else {
            now.opacity / was.opacity
        };
        Some(NodeRow::new(
            [
                (now.translate.0 - was.translate.0) * dpi,
                (now.translate.1 - was.translate.1) * dpi,
            ],
            [now.scale.0 / was.scale.0, now.scale.1 / was.scale.1],
            [slot.centre.0 * dpi, slot.centre.1 * dpi],
            turn,
            opacity,
            slot.parent.map(|p| p * 2 + u32::from(text)),
        ))
    }

    /// Slots for the bindings of `batch` as they stand now, or `None` when the rows cannot draw
    /// them. A node's primitives are found by the ordinal they carry, so the batch may have been
    /// reordered, or have had text inserted, since they were painted.
    fn plan(
        batch: &PrimitiveBatch,
        bindings: &HashMap<LayoutNodeId, CompositeBindingMeta>,
        ordinals: &HashMap<LayoutNodeId, u32>,
        ancestors: &dyn Fn(LayoutNodeId) -> Vec<LayoutNodeId>,
    ) -> Option<Vec<Slot>> {
        if !batch.cross_stream_groups.captures.is_empty()
            || batch.has_foreground_content()
            || batch.has_foreground_layers()
            || batch.has_ordered_content_besides_images()
        {
            return None;
        }
        // A group sizes itself by where its primitives are drawn, which the renderer works out
        // through the rows, so effects do not matter. A blend mode or a 3D transform is not
        // checked yet.
        for entry in &batch.layer_commands {
            match &entry.command {
                LayerCommand::Push { config } => {
                    if config.blend_mode != blinc_core::BlendMode::Normal
                        || config.transform_3d.is_some()
                    {
                        return None;
                    }
                }
                LayerCommand::Pop => {}
                _ => return None,
            }
        }
        // Bindings whose primitives are in the static batch are not this batch's to move. The rest
        // are given slots in walk order, which puts a node after the ones it is inside.
        let mut nodes: Vec<LayoutNodeId> = bindings
            .iter()
            .filter(|(_, meta)| owned(meta))
            .map(|(node, _)| *node)
            .collect();
        nodes.sort_by_key(|node| ordinals.get(node).copied().unwrap_or(u32::MAX));
        if nodes.len() * 2 > NODE_TABLE_CAPACITY {
            return None;
        }
        let index: HashMap<LayoutNodeId, u32> = nodes
            .iter()
            .enumerate()
            .map(|(i, node)| (*node, i as u32))
            .collect();

        let mut slots: Vec<Slot> = Vec::with_capacity(nodes.len());
        for node in nodes {
            let ordinal = *ordinals.get(&node)?;
            let meta = bindings.get(&node)?;
            // The nearest bound ancestor in the tree.
            let parent = ancestors(node)
                .into_iter()
                .find_map(|a| index.get(&a).copied());
            // The command that holds this node's group opacity, found by the configuration it was
            // pushed with: its index may have moved since.
            let layer = match &meta.layer_config {
                Some(config) => Some(find_group(batch, config)?),
                None => None,
            };
            slots.push(Slot {
                node,
                ordinal,
                parent,
                baseline: Pose::of_meta(meta),
                centre: meta.centre,
                layer,
                ambient_clip: meta.ambient_clip,
                ambient_path: meta.ambient_path,
                clip_moves: false,
                limit: meta.scroll_margin,
            });
        }
        // The shaders follow a chain of NODE_CHAIN_DEPTH rows, and a turned ancestor puts a node's
        // own moves on turned axes, which a row works in the world's.
        for slot in &slots {
            let mut length = 1;
            let mut up = slot.parent;
            while let Some(p) = up {
                let ancestor = &slots[p as usize];
                length += 1;
                if length > NODE_CHAIN_DEPTH || ancestor.baseline.rotation.abs() > EPS {
                    return None;
                }
                up = ancestor.parent;
            }
        }
        Some(slots)
    }

    /// Whether `self` still describes `batch` and `bindings`.
    fn describes(
        &self,
        batch: &PrimitiveBatch,
        bindings: &HashMap<LayoutNodeId, CompositeBindingMeta>,
        ordinals: &HashMap<LayoutNodeId, u32>,
    ) -> bool {
        self.epoch != 0
            && self.epoch == batch.node_slots_epoch
            && bindings.values().filter(|m| owned(m)).count() == self.slots.len()
            && self
                .slots
                .iter()
                .all(|s| ordinals.get(&s.node) == Some(&s.ordinal))
    }

    /// Take the slots off `batch` and put the bindings' recorded values back to the baseline the
    /// primitives were left at, so the CPU patcher can take over from there.
    pub(super) fn release(
        &mut self,
        batch: &mut PrimitiveBatch,
        text: Option<&mut TextBatch>,
        bindings: &mut HashMap<LayoutNodeId, CompositeBindingMeta>,
    ) {
        if let Some(text) = text
            && text.node_slots_epoch != 0
            && text.node_slots_epoch == self.epoch
        {
            for p in &mut text.primitives {
                p.set_node_slot(None);
            }
            text.node_slots_epoch = 0;
        }
        if self.epoch != 0 && self.epoch == batch.node_slots_epoch {
            for p in &mut batch.primitives {
                p.set_node_slot(None);
            }
            for image in &mut batch.dynamic_images {
                image.node_slot = None;
                image.shape_from = None;
            }
            for slot in &self.slots {
                if let Some(meta) = bindings.get_mut(&slot.node) {
                    meta.last_translate = slot.baseline.translate;
                    meta.last_scale = slot.baseline.scale;
                    meta.last_rotation_rad = slot.baseline.rotation;
                    meta.last_opacity = slot.baseline.opacity;
                }
            }
            batch.node_slots_epoch = 0;
        }
        self.epoch = 0;
        self.cpu = false;
        self.slots.clear();
        self.rows.clear();
    }

    /// Whether the binding of `meta` has moved from the pose it was painted at.
    pub(super) fn moved(
        &self,
        meta: &CompositeBindingMeta,
        binding: Option<&MotionBindings>,
    ) -> bool {
        if owned(meta) {
            return false;
        }
        match binding {
            Some(b) => Pose::of(b, meta.last_opacity).differs(&Pose::of_meta(meta)),
            None => true,
        }
    }

    /// Where in the chain of rows from `slot` the clip of `prim` begins to follow them: the first
    /// node whose own painting introduced a clip it does not share with the clip in force when it
    /// began. `Err` for a clip that must move and is not a rectangle, which a row cannot move.
    fn clip_from(&self, slot: u32, prim: &GpuPrimitive) -> Result<Follow, ()> {
        if prim.type_info[2] == 0 {
            return Ok(Follow::Stays);
        }
        let mut depth = 0u32;
        let mut up = Some(slot);
        while let Some(s) = up {
            let node = &self.slots[s as usize];
            if !same_clip(prim.clip_bounds, node.ambient_clip) {
                if prim.type_info[2] != 1 {
                    return Err(());
                }
                return Ok(match node.ambient_clip {
                    Some(_) => Follow::Cut(depth),
                    None => Follow::Moves(depth),
                });
            }
            depth += 1;
            up = node.parent;
        }
        Ok(Follow::Stays)
    }

    /// Mark the row at `depth` in the chain from `slot`, and every row above it, as moving a clip
    /// rect, which a turn cannot move.
    fn clip_rect_moves(&mut self, slot: u32, depth: u32) {
        let mut moving = Some(slot);
        for _ in 0..depth {
            moving = moving.and_then(|m| self.slots[m as usize].parent);
        }
        while let Some(m) = moving {
            self.slots[m as usize].clip_moves = true;
            moving = self.slots[m as usize].parent;
        }
    }

    /// [`clip_from`](Self::clip_from) for a clip that is moved whole, as an image's primary clip
    /// is: `None` when it stays, the depth it moves from otherwise.
    fn clip_moves_from(&mut self, slot: u32, prim: &GpuPrimitive) -> Result<Option<u32>, ()> {
        match self.clip_from(slot, prim)? {
            Follow::Stays => Ok(None),
            Follow::Moves(depth) | Follow::Cut(depth) => {
                self.clip_rect_moves(slot, depth);
                Ok(Some(depth))
            }
        }
    }

    /// Where in the chain of rows from `slot` the path clip of `prim` begins to follow them, found
    /// as [`clip_from`](Self::clip_from) finds the rectangle's. A path is taken back through
    /// the rows where it is tested, so it may follow a turn.
    fn path_from(&self, slot: u32, prim: &GpuPrimitive) -> Option<u32> {
        let pointer = prim.clip_shape[0];
        if pointer < 0.5 {
            return None;
        }
        let mut depth = 0u32;
        let mut up = Some(slot);
        while let Some(s) = up {
            let node = &self.slots[s as usize];
            if (node.ambient_path - pointer).abs() > 0.5 {
                return Some(depth);
            }
            depth += 1;
            up = node.parent;
        }
        None
    }

    /// Say, on each primitive that has a clip, which rows move it. `false` when one cannot, or
    /// when a path must move and `paths` says nothing here can take it back.
    fn follow_clips(&mut self, prims: &mut [GpuPrimitive], paths: bool) -> bool {
        for p in prims {
            let Some(row) = p.node_slot() else { continue };
            // A rect cut by a clip that stays cannot be moved whole; the painter makes the part
            // that moves a shape clip, so a cut rect here is one it could not.
            match self.clip_from(row / 2, p) {
                Ok(Follow::Stays) => p.set_node_clip_from(None),
                Ok(Follow::Moves(depth)) => {
                    self.clip_rect_moves(row / 2, depth);
                    p.set_node_clip_from(Some(depth));
                }
                Ok(Follow::Cut(_)) | Err(()) => return false,
            }
            let path = self.path_from(row / 2, p);
            if path.is_some() && !paths {
                return false;
            }
            p.set_node_path_from(path);
        }
        true
    }

    /// The row, in the table, of the glyphs owned by the nearest bound node at or above `node`.
    pub(super) fn text_row(
        &self,
        node: LayoutNodeId,
        ancestors: &dyn Fn(LayoutNodeId) -> Vec<LayoutNodeId>,
    ) -> Option<u32> {
        self.owner(node, ancestors).map(|slot| slot * 2 + 1)
    }

    /// The rows last written to the table.
    pub(super) fn rows(&self) -> &[NodeRow] {
        &self.rows
    }

    /// Whether the rows are applied by [`resolve_primitives`](Self::resolve_primitives) on the
    /// CPU, because the renderer has no table to read them from.
    pub(super) fn cpu_backend(&self) -> bool {
        self.cpu && self.epoch != 0
    }

    /// The assignment of slots the rows belong to.
    pub(super) fn epoch(&self) -> u64 {
        self.epoch
    }

    /// `prims` as they are drawn under the rows now, each slotted one moved and its slot cleared,
    /// for a renderer with no table.
    pub(super) fn resolve_primitives(&self, prims: &[GpuPrimitive]) -> Vec<GpuPrimitive> {
        prims
            .iter()
            .map(|p| {
                if p.node_slot().is_some() {
                    blinc_gpu::node_table::resolve_primitive(&self.rows, p)
                } else {
                    *p
                }
            })
            .collect()
    }

    /// `batch`'s primitives as [`resolve_primitives`](Self::resolve_primitives) gives them, with
    /// each path clip that follows the rows given its own shape rows, appended to the batch's aux
    /// data. Returns the primitives that were there and the aux length to cut back to.
    pub(super) fn resolve_batch(&self, batch: &mut PrimitiveBatch) -> ResolvedBatch {
        let aux_len = batch.aux_data.len();
        let mut prims = batch.primitives.clone();
        for p in &mut prims {
            blinc_gpu::node_table::follow_path_clip(&self.rows, p, &mut batch.aux_data);
        }
        let resolved = self.resolve_primitives(&prims);
        let placed: Vec<_> = batch
            .dynamic_images
            .iter()
            .map(|image| blinc_gpu::node_table::place_dynamic_image(&self.rows, image))
            .collect();
        ResolvedBatch {
            primitives: std::mem::replace(&mut batch.primitives, resolved),
            dynamic_images: std::mem::replace(&mut batch.dynamic_images, placed),
            aux_len,
        }
    }

    /// Whether the chain of rows from `row` changes nothing.
    pub(super) fn chain_is_rest(&self, row: u32) -> bool {
        let mut next = Some(row);
        while let Some(r) = next.and_then(|r| self.rows.get(r as usize)) {
            let [tx, ty, sx, sy] = r.motion;
            let [_, _, turn, opacity] = r.pivot;
            let rest = tx.abs() < 1e-3
                && ty.abs() < 1e-3
                && (sx - 1.0).abs() < 1e-4
                && (sy - 1.0).abs() < 1e-4
                && turn.abs() < 1e-4
                && (opacity - 1.0).abs() < 1e-4;
            if !rest {
                return false;
            }
            next = r.parent();
        }
        true
    }

    /// The one scale the chain of rows from `row` amounts to, or `None` if one of them turns or
    /// stretches.
    pub(super) fn uniform_scale(&self, row: u32) -> Option<f32> {
        let mut scale = 1.0;
        let mut next = Some(row);
        while let Some(r) = next.and_then(|r| self.rows.get(r as usize)) {
            let [_, _, sx, sy] = r.motion;
            if (sx - sy).abs() > 1e-3 || r.pivot[2].abs() > 1e-4 {
                return None;
            }
            scale *= sx;
            next = r.parent();
        }
        Some(scale)
    }

    /// The slot of the nearest bound node at or above `node`, if any.
    fn owner(
        &self,
        node: LayoutNodeId,
        ancestors: &dyn Fn(LayoutNodeId) -> Vec<LayoutNodeId>,
    ) -> Option<u32> {
        let slot_of = |n: LayoutNodeId| self.slots.iter().position(|s| s.node == n);
        slot_of(node)
            .or_else(|| ancestors(node).into_iter().find_map(slot_of))
            .map(|i| i as u32)
    }

    /// Give the glyphs of `text` the text row of the node that owns each run. `false`, with the
    /// glyphs as they were, when a run has no owner or has a clip, or the batch is not glyph
    /// primitives alone.
    fn give_text_slots(
        &self,
        text: &mut TextBatch,
        ancestors: &dyn Fn(LayoutNodeId) -> Vec<LayoutNodeId>,
    ) -> bool {
        if !text.glyphs.is_empty() {
            return false;
        }
        let mut owned = vec![None; text.primitives.len()];
        for run in &text.owners {
            let Some(slot) = self.owner(run.node, ancestors) else {
                return false;
            };
            for i in run.range.clone() {
                if let Some(o) = owned.get_mut(i) {
                    *o = Some(slot * 2 + 1);
                }
            }
        }
        if owned.iter().any(Option::is_none) {
            return false;
        }
        for (p, row) in text.primitives.iter_mut().zip(owned) {
            p.set_node_slot(row);
        }
        true
    }

    /// Give each ordered image of `batch` the row of the node it was painted under. Its clip rect
    /// must stay; a shape clip in its rows that is the inner clips of the scroll container it is
    /// in follows from that container's row. `false` when one cannot be moved.
    fn give_dynamic_image_slots(
        &mut self,
        batch: &mut PrimitiveBatch,
        by_ordinal: &HashMap<u32, u32>,
    ) -> bool {
        let mut given = Vec::new();
        for (index, image) in batch.dynamic_images.iter().enumerate() {
            if image.paint_order.is_none() {
                continue;
            }
            let Some(&slot) = by_ordinal.get(&image.node_ordinal) else {
                continue;
            };
            let [x, y, w, h] = image.clip_bounds;
            let proxy = GpuPrimitive::rect(image.dest.x(), image.dest.y(), 1.0, 1.0)
                .with_clip_rect(x, y, w, h);
            if x > -9999.0 && !matches!(self.clip_from(slot, &proxy), Ok(Follow::Stays)) {
                return false;
            }
            // The shape, when there is one: its info row's type is at its pointer.
            let shape = image
                .clip_aux
                .get(4)
                .map(|row| row[3] as usize)
                .filter(|&pointer| pointer >= 1)
                .and_then(|pointer| image.clip_aux.get(pointer))
                .map(|info| info[2]);
            let from = match shape {
                None => None,
                Some(kind) if (kind - 4.0).abs() < 0.5 => {
                    let mut depth = 0u32;
                    let mut up = Some(slot);
                    let mut found = None;
                    while let Some(s) = up {
                        if self.slots[s as usize].limit.is_some() {
                            found = Some(depth);
                            break;
                        }
                        depth += 1;
                        up = self.slots[s as usize].parent;
                    }
                    let Some(depth) = found else { return false };
                    Some(depth)
                }
                Some(_) => return false,
            };
            given.push((index, slot * 2, from));
        }
        for (index, row, from) in given {
            batch.dynamic_images[index].node_slot = Some(row);
            batch.dynamic_images[index].shape_from = from;
        }
        true
    }

    /// A primitive standing for an image's clip, so the clip is followed as a primitive's is.
    fn clip_proxy(image: &ImageElement) -> GpuPrimitive {
        let proxy = GpuPrimitive::rect(image.x, image.y, image.width, image.height);
        match image.clip_bounds {
            Some([x, y, w, h]) => {
                let [tl, tr, br, bl] = image.clip_radius;
                proxy.with_clip_rounded_rect(x, y, w, h, tl, tr, br, bl)
            }
            None => proxy,
        }
    }

    /// Give each image the text row of the node that owns it, and say from which row its clip
    /// moves. `false`, with the images as they were, when one has no owner or is drawn in a way
    /// the affine does not carry: a shadow, or a placeholder while its image is not loaded,
    /// which are drawn at the painted rect. The scroll clip is an ancestor's and stays.
    fn give_image_slots(
        &mut self,
        images: &mut [ImageElement],
        ancestors: &dyn Fn(LayoutNodeId) -> Vec<LayoutNodeId>,
        loaded: &dyn Fn(&str) -> bool,
    ) -> bool {
        let mut given = Vec::with_capacity(images.len());
        for image in images.iter() {
            if image.shadow.is_some() || (image.placeholder_type != 0 && !loaded(&image.source)) {
                return false;
            }
            let Some(slot) = self.owner(image.node, ancestors) else {
                return false;
            };
            let Ok(from) = self.clip_moves_from(slot, &Self::clip_proxy(image)) else {
                return false;
            };
            // The scroll clip stays where it was painted; one introduced in the moved content (a
            // nested scroll container) would have to move in part.
            if let Some([x, y, w, h]) = image.scroll_clip
                && !matches!(
                    self.clip_from(
                        slot,
                        &GpuPrimitive::rect(x, y, w, h).with_clip_rect(x, y, w, h)
                    ),
                    Ok(Follow::Stays)
                )
            {
                return false;
            }
            given.push((slot * 2 + 1, from));
        }
        for (image, (row, from)) in images.iter_mut().zip(given) {
            image.row = Some(row);
            image.clip_from = from;
        }
        true
    }

    /// A primitive standing for an SVG's clip, so the clip is followed as a primitive's is.
    fn svg_clip_proxy(svg: &SvgElement) -> GpuPrimitive {
        let proxy = GpuPrimitive::rect(svg.x, svg.y, svg.width, svg.height);
        match svg.clip_bounds {
            Some([x, y, w, h]) => proxy.with_clip_rect(x, y, w, h),
            None => proxy,
        }
    }

    /// Give each SVG the text row of the node that owns it, and say from which row its clip
    /// moves. `false`, with the SVGs as they were, when one has no owner, follows a text path or
    /// is drawn in a 3D layer, which the affine does not carry.
    fn give_svg_slots(
        &mut self,
        svgs: &mut [SvgElement],
        ancestors: &dyn Fn(LayoutNodeId) -> Vec<LayoutNodeId>,
    ) -> bool {
        let mut given = Vec::with_capacity(svgs.len());
        for svg in svgs.iter() {
            if svg.path_clip.is_some() || svg.transform_3d_layer.is_some() {
                return false;
            }
            let Some(slot) = self.owner(svg.node, ancestors) else {
                return false;
            };
            // An SVG with a scroll clip of its own moves its clip whole, as an image does, and its
            // scroll clip must stay; one with a single clip cannot move it in part.
            let from = match self.clip_from(slot, &Self::svg_clip_proxy(svg)) {
                Ok(Follow::Stays) => None,
                Ok(Follow::Moves(depth)) => {
                    self.clip_rect_moves(slot, depth);
                    Some(depth)
                }
                Ok(Follow::Cut(depth)) if svg.scroll_clip.is_some() => {
                    self.clip_rect_moves(slot, depth);
                    Some(depth)
                }
                Ok(Follow::Cut(_)) | Err(()) => return false,
            };
            if let Some([x, y, w, h]) = svg.scroll_clip
                && !matches!(
                    self.clip_from(
                        slot,
                        &GpuPrimitive::rect(x, y, w, h).with_clip_rect(x, y, w, h)
                    ),
                    Ok(Follow::Stays)
                )
            {
                return false;
            }
            given.push((slot * 2 + 1, from));
        }
        for (svg, (row, from)) in svgs.iter_mut().zip(given) {
            svg.row = Some(row);
            svg.clip_from = from;
        }
        true
    }

    /// `svgs` as they are drawn under the rows now. SVGs the table has not taken are returned as
    /// they are.
    pub(super) fn place_svgs(&self, svgs: &[SvgElement], scale: f32) -> Vec<SvgElement> {
        svgs.iter()
            .map(|svg| match svg.row {
                Some(row) if !self.chain_is_rest(row) => self.place_svg(svg, row, scale),
                _ => svg.clone(),
            })
            .collect()
    }

    /// `svg` under the chain of rows from `row`: faded by it, and moved, scaled and turned, as
    /// [`Self::place_image`] places an image. The SVG is rasterized at the size its rect has, so a
    /// scale folded into the rect is drawn sharp.
    fn place_svg(&self, svg: &SvgElement, row: u32, scale: f32) -> SvgElement {
        let moved = chain(&self.rows, row);
        let [a, b, c, d, _, _] = moved.affine;
        let (sx, sy) = (a.hypot(b), c.hypot(d));
        let mut out = svg.clone();
        out.motion_opacity *= moved.opacity;
        if sx < 1e-6 || sy < 1e-6 {
            return out;
        }
        let own = svg.css_affine.map_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0], |m| {
            [m[0], m[1], m[2], m[3], m[4] * scale, m[5] * scale]
        });
        let diagonal = own[1].abs() < 1e-6 && own[2].abs() < 1e-6 && (own[0] - own[3]).abs() < 1e-6;
        let folds = (sx - sy).abs() < 1e-4 * sx.max(sy) || diagonal;
        let all = if folds {
            let turn = [a / sx, b / sx, c / sy, d / sy, 0.0, 0.0];
            let m = then(own, turn);
            let (w, h) = (svg.width * sx, svg.height * sy);
            let (cx, cy) = (svg.x + svg.width * 0.5, svg.y + svg.height * 0.5);
            let go = then(own, moved.affine);
            let (dx, dy) = (
                go[0] * cx + go[2] * cy + go[4],
                go[1] * cx + go[3] * cy + go[5],
            );
            out.x = cx - w * 0.5;
            out.y = cy - h * 0.5;
            out.width = w;
            out.height = h;
            [
                m[0],
                m[1],
                m[2],
                m[3],
                dx - (m[0] * cx + m[2] * cy),
                dy - (m[1] * cx + m[3] * cy),
            ]
        } else {
            then(own, moved.affine)
        };
        let identity = (all[0] - 1.0).abs() < 1e-6
            && all[1].abs() < 1e-6
            && all[2].abs() < 1e-6
            && (all[3] - 1.0).abs() < 1e-6;
        if identity && svg.css_affine.is_none() {
            // A plain move: the rect carries it, as a full paint draws an SVG with no transform.
            out.x += all[4];
            out.y += all[5];
        } else {
            out.css_affine = Some([
                all[0],
                all[1],
                all[2],
                all[3],
                all[4] / scale,
                all[5] / scale,
            ]);
        }
        if svg.clip_bounds.is_some() {
            let mut proxy = Self::svg_clip_proxy(svg);
            proxy.set_node_slot(Some(row));
            proxy.set_node_clip_from(svg.clip_from);
            let proxy = blinc_gpu::node_table::resolve_primitive(&self.rows, &proxy);
            out.clip_bounds = Some(proxy.clip_bounds);
        }
        out
    }

    /// `images` as they are drawn under the rows now. Images the table has not taken are returned
    /// as they are.
    pub(super) fn place_images(&self, images: &[ImageElement], scale: f32) -> Vec<ImageElement> {
        images
            .iter()
            .map(|image| match image.row {
                Some(row) if !self.chain_is_rest(row) => self.place_image(image, row, scale),
                _ => image.clone(),
            })
            .collect()
    }

    /// `image` under the chain of rows from `row`: faded by it, and moved, scaled and turned.
    ///
    /// A full paint folds a motion scale into the image's rect and turns it with an affine, which
    /// is what this does too where the order of scaling and turning does not matter, so a frame
    /// of rows is the frame a full paint makes. Where it does (a stretch under an image that has
    /// a transform of its own), the chain goes through the affine whole, which the pipeline draws
    /// with the same geometry and edges a little different.
    ///
    /// The clip stays where an ancestor put it and moves where the node did, then is carried back
    /// through the affine the pipeline applies to a clip, so that it lands where it should.
    fn place_image(&self, image: &ImageElement, row: u32, scale: f32) -> ImageElement {
        let moved = chain(&self.rows, row);
        let [a, b, c, d, tx, ty] = moved.affine;
        let (sx, sy) = (a.hypot(b), c.hypot(d));
        let mut out = image.clone();
        out.opacity *= moved.opacity;
        if sx < 1e-6 || sy < 1e-6 {
            return out;
        }
        // The image's own transform, in device pixels.
        let own = image
            .css_affine
            .map_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0], |m| {
                [m[0], m[1], m[2], m[3], m[4] * scale, m[5] * scale]
            });
        let diagonal = own[1].abs() < 1e-6 && own[2].abs() < 1e-6 && (own[0] - own[3]).abs() < 1e-6;
        let folds = (sx - sy).abs() < 1e-4 * sx.max(sy) || diagonal;
        let all = if folds {
            // The turn alone goes in the affine; the scale goes in the rect, about its centre.
            let turn = [a / sx, b / sx, c / sy, d / sy, 0.0, 0.0];
            let m = then(own, turn);
            let (w, h) = (image.width * sx, image.height * sy);
            let (cx, cy) = (image.x + image.width * 0.5, image.y + image.height * 0.5);
            // Where the centre goes under the image's own transform and then the rows'.
            let go = then(own, moved.affine);
            let (dx, dy) = (
                go[0] * cx + go[2] * cy + go[4],
                go[1] * cx + go[3] * cy + go[5],
            );
            out.x = cx - w * 0.5;
            out.y = cy - h * 0.5;
            out.width = w;
            out.height = h;
            let k = (sx * sy).sqrt();
            out.border_radius *= k;
            out.border_width *= k;
            [
                m[0],
                m[1],
                m[2],
                m[3],
                dx - (m[0] * cx + m[2] * cy),
                dy - (m[1] * cx + m[3] * cy),
            ]
        } else {
            then(own, moved.affine)
        };
        out.css_affine = Some([
            all[0],
            all[1],
            all[2],
            all[3],
            all[4] / scale,
            all[5] / scale,
        ]);
        if image.clip_bounds.is_some() {
            let mut proxy = Self::clip_proxy(image);
            proxy.set_node_slot(Some(row));
            proxy.set_node_clip_from(image.clip_from);
            let proxy = blinc_gpu::node_table::resolve_primitive(&self.rows, &proxy);
            let [fx, fy, fw, fh] = proxy.clip_bounds;
            let [ma, mb, mc, md, mtx, mty] = all;
            let det = ma * md - mb * mc;
            let s = det.abs().sqrt().max(1e-6);
            let (cx, cy) = (fx + fw * 0.5 - mtx, fy + fh * 0.5 - mty);
            let (ix, iy) = if det.abs() > 1e-9 {
                ((md * cx - mc * cy) / det, (ma * cy - mb * cx) / det)
            } else {
                (fx + fw * 0.5, fy + fh * 0.5)
            };
            let (iw, ih) = (fw / s, fh / s);
            out.clip_bounds = Some([ix - iw * 0.5, iy - ih * 0.5, iw, ih]);
            out.clip_radius = proxy.clip_radius.map(|r| r / s);
        }
        out
    }

    /// Bring the rows up to date with the bindings, giving the batch and the glyphs their slots
    /// first if they have none. Slots are given only when `adopt` says the batch and the glyphs
    /// were both just painted, at the same pose; a glyph batch baked at some other pose can only
    /// be left alone, and then the rows are declined.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply(
        &mut self,
        renderer: &GpuRenderer,
        batch: &mut PrimitiveBatch,
        mut text: Option<&mut TextBatch>,
        mut images: Option<&mut Vec<ImageElement>>,
        mut svgs: Option<&mut Vec<SvgElement>>,
        bindings: &mut HashMap<LayoutNodeId, CompositeBindingMeta>,
        motion: &HashMap<LayoutNodeId, MotionBindings>,
        ordinals: &HashMap<LayoutNodeId, u32>,
        ancestors: &dyn Fn(LayoutNodeId) -> Vec<LayoutNodeId>,
        loaded: &dyn Fn(&str) -> bool,
        scroll: &dyn Fn(LayoutNodeId) -> Option<(f32, f32)>,
        dpi: f32,
        adopt: bool,
    ) -> Outcome {
        let has_text = text
            .as_ref()
            .is_some_and(|t| !t.primitives.is_empty() || !t.glyphs.is_empty());
        let has_images = images.as_ref().is_some_and(|v| !v.is_empty())
            || svgs.as_ref().is_some_and(|v| !v.is_empty());
        let given = self.describes(batch, bindings, ordinals)
            && text
                .as_ref()
                .is_none_or(|t| !has_text || t.node_slots_epoch == self.epoch)
            && images
                .as_ref()
                .is_none_or(|v| v.iter().all(|i| i.row.is_some()))
            && svgs
                .as_ref()
                .is_none_or(|v| v.iter().all(|s| s.row.is_some()));
        if !given {
            self.release(batch, text.as_deref_mut(), bindings);
            if (has_text || has_images) && !adopt {
                return Outcome::Declined;
            }
            let Some(slots) = Self::plan(batch, bindings, ordinals, ancestors) else {
                return Outcome::Declined;
            };
            self.slots = slots;
            // Give the batch and the glyphs their slots, and say which clips move with them.
            let epoch = next_epoch();
            let by_ordinal: HashMap<u32, u32> = self
                .slots
                .iter()
                .enumerate()
                .map(|(i, slot)| (slot.ordinal, i as u32))
                .collect();
            for p in &mut batch.primitives {
                if let Some(slot) = by_ordinal.get(&p.node_ordinal()) {
                    p.set_node_slot(Some(slot * 2));
                }
            }
            batch.node_slots_epoch = epoch;
            self.epoch = epoch;
            self.rows.clear();
            let mut primitives = std::mem::take(&mut batch.primitives);
            let followed = self.follow_clips(&mut primitives, true);
            batch.primitives = primitives;
            if !followed || !self.give_dynamic_image_slots(batch, &by_ordinal) {
                return self.decline(true, batch, text, bindings);
            }
            if has_text && let Some(text) = text.as_deref_mut() {
                if !self.give_text_slots(text, ancestors) {
                    return self.decline(true, batch, Some(text), bindings);
                }
                text.node_slots_epoch = epoch;
                let mut primitives = std::mem::take(&mut text.primitives);
                let followed = self.follow_clips(&mut primitives, true);
                text.primitives = primitives;
                if !followed {
                    return self.decline(true, batch, Some(text), bindings);
                }
            }
            if let Some(images) = images.as_mut()
                && !self.give_image_slots(images, ancestors, loaded)
            {
                return self.decline(true, batch, text, bindings);
            }
            if let Some(svgs) = svgs.as_mut()
                && !self.give_svg_slots(svgs, ancestors)
            {
                return self.decline(true, batch, text, bindings);
            }
        }

        let mut rows = Vec::with_capacity(self.slots.len() * 2);
        for slot in &self.slots {
            // A node's pose is its bound values, or for a scroll container its offset.
            let now = match motion.get(&slot.node) {
                Some(b) => Pose::of(b, slot.baseline.opacity),
                None => match scroll(slot.node) {
                    Some(offset) => Pose {
                        translate: offset,
                        scale: (1.0, 1.0),
                        rotation: 0.0,
                        opacity: slot.baseline.opacity,
                    },
                    None => return self.decline(true, batch, text, bindings),
                },
            };
            let (Some(batch_row), Some(text_row)) = (
                Self::row(slot, &now, dpi, false),
                Self::row(slot, &now, dpi, true),
            ) else {
                return self.decline(true, batch, text, bindings);
            };
            rows.push(batch_row);
            rows.push(text_row);
        }
        let on_gpu = renderer.node_table_enabled();
        if rows != self.rows {
            if on_gpu && !renderer.set_node_rows(&rows) {
                return self.decline(true, batch, text, bindings);
            }
            self.rows = rows;
        }
        self.cpu = !on_gpu;

        // A group's opacity is the value itself, set on its command.
        for slot in &self.slots {
            let Some(index) = slot.layer else { continue };
            let Some(b) = motion.get(&slot.node) else {
                continue;
            };
            let opacity = Pose::of(b, slot.baseline.opacity).opacity;
            if let Some(LayerCommand::Push { config }) =
                batch.layer_commands.get_mut(index).map(|e| &mut e.command)
            {
                config.opacity = opacity;
            }
        }

        // Record each binding's values as the patcher does, so a settled binding reads as settled
        // and the redraw signal clears.
        let mut active = false;
        for (node, meta) in bindings.iter_mut().filter(|(_, m)| owned(m)) {
            let Some(b) = motion.get(node) else { continue };
            active |= b.is_any_animating();
            let now = Pose::of(b, meta.last_opacity);
            if now.differs(&Pose::of_meta(meta)) {
                active = true;
                meta.last_translate = now.translate;
                meta.last_scale = now.scale;
                meta.last_rotation_rad = now.rotation;
                meta.last_opacity = now.opacity;
            }
        }
        Outcome::Applied { active }
    }

    fn decline(
        &mut self,
        slots_given: bool,
        batch: &mut PrimitiveBatch,
        text: Option<&mut TextBatch>,
        bindings: &mut HashMap<LayoutNodeId, CompositeBindingMeta>,
    ) -> Outcome {
        if slots_given {
            self.release(batch, text, bindings);
        } else {
            self.slots.clear();
        }
        Outcome::Declined
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(baseline: Pose) -> Slot {
        Slot {
            node: LayoutNodeId::default(),
            ordinal: 1,
            parent: None,
            baseline,
            centre: (50.0, 40.0),
            layer: None,
            ambient_clip: None,
            ambient_path: 0.0,
            clip_moves: false,
            limit: None,
        }
    }

    const REST: Pose = Pose {
        translate: (0.0, 0.0),
        scale: (1.0, 1.0),
        rotation: 0.0,
        opacity: 1.0,
    };

    #[test]
    fn a_row_holds_the_change_since_the_bake_in_device_pixels() {
        let was = Pose {
            translate: (10.0, 0.0),
            scale: (2.0, 2.0),
            rotation: 0.5,
            opacity: 0.5,
        };
        let now = Pose {
            translate: (14.0, -3.0),
            scale: (3.0, 3.0),
            rotation: 0.75,
            opacity: 1.0,
        };
        let row = NodeRows::row(&slot(was), &now, 2.0, false).unwrap();
        assert_eq!(row.motion, [8.0, -6.0, 1.5, 1.5]);
        assert_eq!(row.pivot, [100.0, 80.0, 0.25, 2.0]);
        assert_eq!(row.parent(), None);
    }

    #[test]
    fn an_unchanged_binding_has_the_row_that_changes_nothing() {
        let row = NodeRows::row(&slot(REST), &REST, 1.0, false).unwrap();
        assert_eq!(row.motion, NodeRow::REST.motion);
        assert_eq!(row.pivot[2..], NodeRow::REST.pivot[2..]);
    }

    #[test]
    fn what_a_row_cannot_say_is_declined() {
        let stretched = Pose {
            scale: (2.0, 1.0),
            ..REST
        };
        let turned = Pose {
            rotation: 0.3,
            ..stretched
        };
        // A stretch alone is a row; a stretch with a turn is not, since the two do not commute.
        assert!(NodeRows::row(&slot(REST), &stretched, 1.0, false).is_some());
        assert!(NodeRows::row(&slot(REST), &turned, 1.0, false).is_none());
        // A scale from nothing has no ratio, nor has a fade from nothing.
        let nothing = Pose {
            scale: (0.0, 0.0),
            ..REST
        };
        assert!(NodeRows::row(&slot(nothing), &REST, 1.0, false).is_none());
        let invisible = Pose {
            opacity: 0.0,
            ..REST
        };
        assert!(NodeRows::row(&slot(invisible), &REST, 1.0, false).is_none());
        assert!(NodeRows::row(&slot(invisible), &invisible, 1.0, false).is_some());
    }

    fn run(range: Range<usize>, node: LayoutNodeId) -> super::super::cached_text::TextRun {
        super::super::cached_text::TextRun {
            range,
            node,
            element: 0,
            k: 1.0,
            snapped: true,
            baked: None,
        }
    }

    fn id(n: u64) -> LayoutNodeId {
        slotmap::KeyData::from_ffi((1 << 32) | n).into()
    }

    fn meta(range: Range<usize>) -> CompositeBindingMeta {
        CompositeBindingMeta {
            dynamic_batch: true,
            primitive_range: range,
            last_translate: (0.0, 0.0),
            last_scale: (1.0, 1.0),
            last_rotation_rad: 0.0,
            last_opacity: 1.0,
            layer_push_index: None,
            layer_config: None,
            centre: (10.0, 10.0),
            ambient_clip: None,
            ambient_path: 0.0,
            scroll_margin: None,
            last_screen_aabb: None,
        }
    }

    fn batch_of(prims: usize) -> PrimitiveBatch {
        let mut batch = PrimitiveBatch::new();
        for i in 0..prims {
            batch.push(blinc_gpu::GpuPrimitive::rect(i as f32, 0.0, 4.0, 4.0));
        }
        batch
    }

    /// A tree of `parents`: node n has the ancestors listed for it, nearest first.
    fn tree_of(parents: &[(u64, &[u64])]) -> impl Fn(LayoutNodeId) -> Vec<LayoutNodeId> {
        let table: HashMap<LayoutNodeId, Vec<LayoutNodeId>> = parents
            .iter()
            .map(|(n, up)| (id(*n), up.iter().map(|a| id(*a)).collect()))
            .collect();
        move |node| table.get(&node).cloned().unwrap_or_default()
    }

    /// A batch of `prims` rects, each stamped with the ordinal in `stamps` at its index.
    fn stamped(stamps: &[u32]) -> PrimitiveBatch {
        let mut batch = batch_of(stamps.len());
        for (p, ordinal) in batch.primitives.iter_mut().zip(stamps) {
            p.set_node_ordinal(*ordinal);
        }
        batch
    }

    fn ordinals(pairs: &[(u64, u32)]) -> HashMap<LayoutNodeId, u32> {
        pairs.iter().map(|(n, o)| (id(*n), *o)).collect()
    }

    #[test]
    fn a_node_with_no_primitives_gets_a_slot_under_its_bound_ancestor() {
        // The outer node holds both primitives, the inner one has none and moves only text.
        let bindings = HashMap::from([(id(1), meta(0..2)), (id(2), meta(0..0))]);
        let ancestors = tree_of(&[(1, &[]), (2, &[1])]);
        let slots = NodeRows::plan(
            &stamped(&[1, 1]),
            &bindings,
            &ordinals(&[(1, 1), (2, 2)]),
            &ancestors,
        )
        .expect("a plan");
        assert_eq!(slots.len(), 2);
        assert_eq!(slots[0].node, id(1));
        assert_eq!(slots[1].node, id(2));
        assert_eq!(slots[1].parent, Some(0));
    }

    #[test]
    fn slots_follow_the_stamps_not_the_order_of_the_primitives() {
        // Inner primitives sit between the outer's, and a primitive of the outer comes after:
        // a range could not say which is which, the stamps can.
        let batch = stamped(&[1, 2, 2, 1]);
        let bindings = HashMap::from([(id(1), meta(0..4)), (id(2), meta(1..3))]);
        let ancestors = tree_of(&[(1, &[]), (2, &[1])]);
        let slots = NodeRows::plan(&batch, &bindings, &ordinals(&[(1, 1), (2, 2)]), &ancestors)
            .expect("a plan");
        assert_eq!(slots[1].parent, Some(0));
        assert_eq!(slots[0].ordinal, 1);
        assert_eq!(slots[1].ordinal, 2);
    }

    fn group(opacity: f32, x: f32) -> blinc_core::LayerConfig {
        blinc_core::LayerConfig {
            position: Some(blinc_core::Point::new(x, 0.0)),
            size: Some(blinc_core::Size::new(10.0, 10.0)),
            opacity,
            ..Default::default()
        }
    }

    fn with_groups(
        mut batch: PrimitiveBatch,
        configs: &[blinc_core::LayerConfig],
    ) -> PrimitiveBatch {
        for config in configs {
            batch.push_layer_command(LayerCommand::Push {
                config: config.clone(),
            });
            batch.push_layer_command(LayerCommand::Pop);
        }
        batch
    }

    #[test]
    fn a_group_is_found_by_what_it_was_pushed_with_and_only_when_that_is_one_group() {
        let a = group(0.5, 0.0);
        let b = group(0.5, 20.0);
        let batch = with_groups(batch_of(1), &[a.clone(), b.clone()]);
        assert_eq!(find_group(&batch, &b), Some(2));
        assert_eq!(find_group(&batch, &group(0.5, 40.0)), None);
        // Two groups pushed alike cannot be told apart.
        let twins = with_groups(batch_of(1), &[a.clone(), a.clone()]);
        assert_eq!(find_group(&twins, &a), None);
    }

    #[test]
    fn a_bound_group_takes_its_slot_and_the_row_leaves_its_opacity_alone() {
        let config = group(0.5, 0.0);
        let mut bound = meta(0..1);
        bound.layer_config = Some(config.clone());
        let bindings = HashMap::from([(id(1), bound)]);
        let batch = with_groups(stamped(&[1]), &[config]);
        let slots = NodeRows::plan(
            &batch,
            &bindings,
            &ordinals(&[(1, 1)]),
            &tree_of(&[(1, &[])]),
        )
        .expect("a plan");
        assert_eq!(slots[0].layer, Some(0));
        let now = Pose {
            translate: (0.0, 0.0),
            scale: (1.0, 1.0),
            rotation: 0.0,
            opacity: 0.25,
        };
        let row = NodeRows::row(&slots[0], &now, 1.0, false).unwrap();
        assert_eq!(row.pivot[3], 1.0, "the group's opacity is its command's");
        let glyph_row = NodeRows::row(&slots[0], &now, 1.0, true).unwrap();
        assert_eq!(glyph_row.pivot[3], 0.25, "glyphs are outside the group");
    }

    #[test]
    fn a_blend_mode_or_a_3d_group_is_declined() {
        let bindings = HashMap::from([(id(1), meta(0..1))]);
        let ancestors = tree_of(&[(1, &[])]);
        let plan = |config: blinc_core::LayerConfig| {
            let batch = with_groups(stamped(&[1]), &[config]);
            NodeRows::plan(&batch, &bindings, &ordinals(&[(1, 1)]), &ancestors)
        };
        assert!(plan(group(1.0, 0.0)).is_some());
        let mut blended = group(1.0, 0.0);
        blended.blend_mode = blinc_core::BlendMode::Multiply;
        assert!(plan(blended).is_none());
    }

    #[test]
    fn a_binding_in_the_static_batch_is_not_the_tables() {
        let mut in_static = meta(0..1);
        in_static.dynamic_batch = false;
        let bindings = HashMap::from([(id(1), meta(0..1)), (id(2), in_static.clone())]);
        let slots = NodeRows::plan(
            &stamped(&[1]),
            &bindings,
            &ordinals(&[(1, 1), (2, 2)]),
            &tree_of(&[(1, &[]), (2, &[])]),
        )
        .expect("a plan");
        assert_eq!(slots.len(), 1);
        assert_eq!(slots[0].node, id(1));
        let rows = NodeRows::default();
        // Settled where it was painted it is left be; a change asks for a repaint.
        assert!(!rows.moved(&in_static, Some(&MotionBindings::default())));
        assert!(rows.moved(&in_static, None));
    }

    #[test]
    fn a_chain_longer_than_the_shaders_follow_is_declined() {
        let nodes: Vec<u64> = (1..=(NODE_CHAIN_DEPTH as u64 + 1)).collect();
        let bindings: HashMap<_, _> = nodes.iter().map(|n| (id(*n), meta(0..1))).collect();
        let pairs: Vec<(u64, u32)> = nodes.iter().map(|n| (*n, *n as u32)).collect();
        let tree: Vec<(u64, Vec<u64>)> = nodes
            .iter()
            .map(|n| (*n, (1..*n).rev().collect()))
            .collect();
        let tree_ref: Vec<(u64, &[u64])> = tree.iter().map(|(n, up)| (*n, up.as_slice())).collect();
        let ancestors = tree_of(&tree_ref);
        let batch = stamped(&[1]);
        assert!(NodeRows::plan(&batch, &bindings, &ordinals(&pairs), &ancestors).is_none());
        let shallow: HashMap<_, _> = nodes[..NODE_CHAIN_DEPTH]
            .iter()
            .map(|n| (id(*n), meta(0..1)))
            .collect();
        assert!(NodeRows::plan(&batch, &shallow, &ordinals(&pairs), &ancestors).is_some());
    }

    #[test]
    fn glyphs_take_the_text_row_of_the_nearest_bound_node() {
        let bindings = HashMap::from([(id(1), meta(0..2)), (id(2), meta(2..4))]);
        let ancestors = tree_of(&[(1, &[]), (2, &[]), (3, &[2]), (4, &[1])]);
        let rows = NodeRows {
            slots: NodeRows::plan(
                &stamped(&[1, 1, 2, 2]),
                &bindings,
                &ordinals(&[(1, 1), (2, 2)]),
                &ancestors,
            )
            .unwrap(),
            ..Default::default()
        };
        let slot_of = |n: u64| rows.slots.iter().position(|s| s.node == id(n)).unwrap() as u32;

        let mut text = TextBatch::default();
        for _ in 0..3 {
            text.primitives
                .push(blinc_gpu::GpuPrimitive::rect(0.0, 0.0, 2.0, 2.0));
        }
        // Run 0..2 belongs to node 3 (under 2), run 2..3 to node 4 (under 1).
        text.owners = vec![run(0..2, id(3)), run(2..3, id(4))];
        assert!(rows.give_text_slots(&mut text, &ancestors));
        assert_eq!(text.primitives[0].node_slot(), Some(slot_of(2) * 2 + 1));
        assert_eq!(text.primitives[1].node_slot(), Some(slot_of(2) * 2 + 1));
        assert_eq!(text.primitives[2].node_slot(), Some(slot_of(1) * 2 + 1));
    }

    #[test]
    fn glyphs_with_no_bound_owner_are_left_alone() {
        let bindings = HashMap::from([(id(1), meta(0..2))]);
        let ancestors = tree_of(&[(1, &[]), (5, &[])]);
        let rows = NodeRows {
            slots: NodeRows::plan(
                &stamped(&[1, 1]),
                &bindings,
                &ordinals(&[(1, 1)]),
                &ancestors,
            )
            .unwrap(),
            ..Default::default()
        };
        let mut text = TextBatch::default();
        text.primitives
            .push(blinc_gpu::GpuPrimitive::rect(0.0, 0.0, 2.0, 2.0));
        text.owners = vec![run(0..1, id(5))];
        assert!(!rows.give_text_slots(&mut text, &ancestors));
        assert_eq!(text.primitives[0].node_slot(), None);
    }
}
