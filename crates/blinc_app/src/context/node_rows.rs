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

use blinc_gpu::node_table::{NODE_CHAIN_DEPTH, NODE_TABLE_CAPACITY, NodeRow};
use blinc_gpu::primitives::LayerCommand;
use blinc_gpu::{GpuPrimitive, GpuRenderer, PrimitiveBatch};

use super::cached_text::TextBatch;
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
    range: Range<usize>,
    /// The slot of the nearest binding whose range holds this one's.
    parent: Option<u32>,
    /// The pose the primitives of the range were drawn at when the slot was given.
    baseline: Pose,
    /// The world-space pivot of the baseline, in logical pixels.
    centre: (f32, f32),
    /// The layer command whose opacity is this node's, patched on the CPU as a value of its own;
    /// the row then leaves opacity alone.
    layer: Option<usize>,
    /// The clip in force when the node began painting.
    ambient_clip: Option<[f32; 4]>,
    /// Whether a primitive's clip moves with this node's row, which cannot turn a clip.
    clip_moves: bool,
}

/// Whether a binding is the dynamic batch's to move: its primitives are in that batch, or it has
/// none and moves only text.
fn owned(meta: &CompositeBindingMeta) -> bool {
    meta.dynamic_batch || meta.primitive_range.is_empty()
}

/// The rows of one assignment of slots to a batch.
#[derive(Default)]
pub(super) struct NodeRows {
    /// The `node_slots_epoch` of the batch these slots were given to, zero for none.
    epoch: u64,
    slots: Vec<Slot>,
    /// The rows last written.
    rows: Vec<NodeRow>,
}

/// Ranges in the order slots are given, each with the slot of its parent.
type Nested<K> = Vec<(K, Range<usize>, Option<u32>)>;

/// Each range's parent among `ranges`, in the order the slots are given (outer before inner).
/// Equal ranges nest by depth in the tree, the shallower outside. Declines ranges that cannot be
/// nested: two equal ones at the same depth, two that overlap without one holding the other, or a
/// chain longer than the shaders follow. Empty ranges have no slot and are left out.
fn nest<K: Copy>(ranges: &[(K, Range<usize>, usize)]) -> Option<Nested<K>> {
    let mut order: Vec<&(K, Range<usize>, usize)> =
        ranges.iter().filter(|(_, r, _)| !r.is_empty()).collect();
    order.sort_by(|a, b| {
        a.1.start
            .cmp(&b.1.start)
            .then(b.1.end.cmp(&a.1.end))
            .then(a.2.cmp(&b.2))
    });
    let mut out: Nested<K> = Vec::with_capacity(order.len());
    let mut depths: Vec<usize> = Vec::with_capacity(order.len());
    // The slots whose ranges hold the one being placed, outermost first.
    let mut open: Vec<u32> = Vec::new();
    for (key, range, depth) in order {
        while let Some(&top) = open.last() {
            if out[top as usize].1.end <= range.start {
                open.pop();
            } else {
                break;
            }
        }
        if let Some(&top) = open.last() {
            let outer = &out[top as usize].1;
            if range.end > outer.end || (outer == range && *depth <= depths[top as usize]) {
                return None;
            }
        }
        if open.len() >= NODE_CHAIN_DEPTH {
            return None;
        }
        let slot = out.len() as u32;
        out.push((*key, range.clone(), open.last().copied()));
        depths.push(*depth);
        open.push(slot);
    }
    (out.len() <= NODE_TABLE_CAPACITY).then_some(out)
}

/// Whether `clip` (x, y, width, height) is the clip that was in force at `ambient`.
fn same_clip(clip: [f32; 4], ambient: Option<[f32; 4]>) -> bool {
    ambient.is_some_and(|a| clip.iter().zip(a).all(|(c, a)| (c - a).abs() < 0.5))
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
    /// them.
    fn plan(
        batch: &PrimitiveBatch,
        bindings: &HashMap<LayoutNodeId, CompositeBindingMeta>,
        ancestors: &dyn Fn(LayoutNodeId) -> Vec<LayoutNodeId>,
    ) -> Option<Vec<Slot>> {
        // Anything ordered across streams keeps its place by primitive index, and what a row
        // moves is found by it.
        if !batch.cross_stream_groups.captures.is_empty()
            || batch.has_foreground_content()
            || batch.has_foreground_layers()
            || batch.has_ordered_content()
        {
            return None;
        }
        // A plain group is drawn wherever its primitives are. One with effects, a blend mode or a
        // 3D transform sizes itself by what is under it, which a row changes.
        for entry in &batch.layer_commands {
            match &entry.command {
                LayerCommand::Push { config } => {
                    if !config.effects.is_empty()
                        || config.blend_mode != blinc_core::BlendMode::Normal
                        || config.transform_3d.is_some()
                    {
                        return None;
                    }
                }
                LayerCommand::Pop => {}
                _ => return None,
            }
        }
        // Bindings whose primitives are in the static batch are not this batch's to move.
        let ranges: Vec<_> = bindings
            .iter()
            .filter(|(_, meta)| owned(meta))
            .map(|(node, meta)| (*node, meta.primitive_range.clone(), ancestors(*node).len()))
            .collect();
        // Nodes with primitives come first, outer before inner, so an inner range's slot is
        // given to a primitive last. The nodes with none, which move only text, follow.
        let mut order: Vec<(LayoutNodeId, Range<usize>)> = nest(&ranges)?
            .into_iter()
            .map(|(node, range, _)| (node, range))
            .collect();
        let mut bare: Vec<(LayoutNodeId, Range<usize>, usize)> = ranges
            .iter()
            .filter(|(_, r, _)| r.is_empty())
            .map(|(n, r, d)| (*n, r.clone(), *d))
            .collect();
        bare.sort_by_key(|(_, _, depth)| *depth);
        order.extend(bare.into_iter().map(|(n, r, _)| (n, r)));
        if order.len() * 2 > NODE_TABLE_CAPACITY {
            return None;
        }
        let index: HashMap<LayoutNodeId, u32> = order
            .iter()
            .enumerate()
            .map(|(i, (node, _))| (*node, i as u32))
            .collect();

        let mut slots: Vec<Slot> = Vec::with_capacity(order.len());
        for (node, range) in order {
            if range.end > batch.primitives.len() {
                return None;
            }
            let meta = bindings.get(&node)?;
            // The nearest bound ancestor in the tree. For a node with primitives it is also the
            // nearest range that holds this one.
            let parent = ancestors(node)
                .into_iter()
                .find_map(|a| index.get(&a).copied());
            let held_by = if range.is_empty() {
                parent
            } else {
                slots
                    .iter()
                    .enumerate()
                    .rev()
                    .find(|(_, s)| {
                        !s.range.is_empty()
                            && s.range.start <= range.start
                            && s.range.end >= range.end
                    })
                    .map(|(i, _)| i as u32)
            };
            if parent != held_by {
                return None;
            }
            if let Some(index) = meta.layer_push_index
                && !matches!(
                    batch.layer_commands.get(index).map(|e| &e.command),
                    Some(LayerCommand::Push { .. })
                )
            {
                return None;
            }
            slots.push(Slot {
                node,
                range,
                parent,
                baseline: Pose::of_meta(meta),
                centre: meta.centre,
                layer: meta.layer_push_index,
                ambient_clip: meta.ambient_clip,
                clip_moves: false,
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
    ) -> bool {
        self.epoch != 0
            && self.epoch == batch.node_slots_epoch
            && bindings.values().filter(|m| owned(m)).count() == self.slots.len()
            && self.slots.iter().all(|s| {
                bindings
                    .get(&s.node)
                    .is_some_and(|m| m.primitive_range == s.range)
            })
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
            for slot in &self.slots {
                if let Some(prims) = batch.primitives.get_mut(slot.range.clone()) {
                    for p in prims {
                        p.set_node_slot(None);
                    }
                }
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
    /// began. `Ok(None)` for no clip, or one that is wholly an ancestor's and stays. `Err` for a
    /// clip that must move and is not a rectangle, which a row cannot move.
    fn clip_from(&mut self, slot: u32, prim: &GpuPrimitive) -> Result<Option<u32>, ()> {
        if prim.type_info[2] == 0 {
            return Ok(None);
        }
        let mut depth = 0u32;
        let mut up = Some(slot);
        while let Some(s) = up {
            let node = &self.slots[s as usize];
            if !same_clip(prim.clip_bounds, node.ambient_clip) {
                // Introduced inside this node's subtree: this row and every one above it move it.
                if prim.type_info[2] != 1 {
                    return Err(());
                }
                let mut moving = Some(s);
                while let Some(m) = moving {
                    self.slots[m as usize].clip_moves = true;
                    moving = self.slots[m as usize].parent;
                }
                return Ok(Some(depth));
            }
            depth += 1;
            up = node.parent;
        }
        Ok(None)
    }

    /// Say, on each primitive that has a clip, which rows move it. `false` when one cannot.
    fn follow_clips(&mut self, prims: &mut [GpuPrimitive]) -> bool {
        for p in prims {
            let Some(row) = p.node_slot() else { continue };
            match self.clip_from(row / 2, p) {
                Ok(from) => p.set_node_clip_from(from),
                Err(()) => return false,
            }
        }
        true
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
        for (range, node, _, _) in &text.owners {
            let Some(slot) = self.owner(*node, ancestors) else {
                return false;
            };
            for i in range.clone() {
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
        bindings: &mut HashMap<LayoutNodeId, CompositeBindingMeta>,
        motion: &HashMap<LayoutNodeId, MotionBindings>,
        ancestors: &dyn Fn(LayoutNodeId) -> Vec<LayoutNodeId>,
        dpi: f32,
        adopt: bool,
    ) -> Outcome {
        let has_text = text
            .as_ref()
            .is_some_and(|t| !t.primitives.is_empty() || !t.glyphs.is_empty());
        let given = self.describes(batch, bindings)
            && text
                .as_ref()
                .is_none_or(|t| !has_text || t.node_slots_epoch == self.epoch);
        if !given {
            self.release(batch, text.as_deref_mut(), bindings);
            if has_text && !adopt {
                return Outcome::Declined;
            }
            let Some(slots) = Self::plan(batch, bindings, ancestors) else {
                return Outcome::Declined;
            };
            self.slots = slots;
            // Give the batch and the glyphs their slots, and say which clips move with them.
            let epoch = next_epoch();
            for (i, slot) in self.slots.iter().enumerate() {
                for p in &mut batch.primitives[slot.range.clone()] {
                    p.set_node_slot(Some(i as u32 * 2));
                }
            }
            batch.node_slots_epoch = epoch;
            self.epoch = epoch;
            self.rows.clear();
            let mut primitives = std::mem::take(&mut batch.primitives);
            let followed = self.follow_clips(&mut primitives);
            batch.primitives = primitives;
            if !followed {
                return self.decline(true, batch, text, bindings);
            }
            if has_text && let Some(text) = text.as_deref_mut() {
                if !self.give_text_slots(text, ancestors) {
                    return self.decline(true, batch, Some(text), bindings);
                }
                text.node_slots_epoch = epoch;
                let mut primitives = std::mem::take(&mut text.primitives);
                let followed = self.follow_clips(&mut primitives);
                text.primitives = primitives;
                if !followed {
                    return self.decline(true, batch, Some(text), bindings);
                }
            }
        }

        let mut rows = Vec::with_capacity(self.slots.len() * 2);
        for slot in &self.slots {
            let Some(b) = motion.get(&slot.node) else {
                return self.decline(true, batch, text, bindings);
            };
            let now = Pose::of(b, slot.baseline.opacity);
            let (Some(batch_row), Some(text_row)) = (
                Self::row(slot, &now, dpi, false),
                Self::row(slot, &now, dpi, true),
            ) else {
                return self.decline(true, batch, text, bindings);
            };
            rows.push(batch_row);
            rows.push(text_row);
        }
        if rows != self.rows {
            if !renderer.set_node_rows(&rows) {
                return self.decline(true, batch, text, bindings);
            }
            self.rows = rows;
        }

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

    fn parents(ranges: &[Range<usize>]) -> Option<Vec<(usize, Range<usize>, Option<u32>)>> {
        let keyed: Vec<_> = ranges
            .iter()
            .cloned()
            .enumerate()
            .map(|(i, r)| (i, r, 0))
            .collect();
        nest(&keyed)
    }

    #[test]
    fn disjoint_ranges_have_no_parents() {
        let out = parents(&[0..3, 3..5, 7..9]).unwrap();
        assert!(out.iter().all(|(_, _, p)| p.is_none()));
    }

    #[test]
    fn a_range_inside_another_names_it_as_its_parent() {
        // Given out of order: the outer slot comes first whatever the map order.
        let out = parents(&[2..4, 0..10, 5..8, 6..7]).unwrap();
        let by_range = |r: Range<usize>| out.iter().position(|(_, x, _)| *x == r).unwrap() as u32;
        let parent_of = |r: Range<usize>| out.iter().find(|(_, x, _)| *x == r).unwrap().2;
        assert_eq!(by_range(0..10), 0);
        assert_eq!(parent_of(0..10), None);
        assert_eq!(parent_of(2..4), Some(by_range(0..10)));
        assert_eq!(parent_of(5..8), Some(by_range(0..10)));
        assert_eq!(parent_of(6..7), Some(by_range(5..8)));
    }

    #[test]
    fn empty_ranges_are_left_out() {
        let out = parents(&[4..4, 0..2]).unwrap();
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn equal_ranges_nest_by_depth_and_overlapping_ones_are_declined() {
        // The same range at the same depth cannot be told apart.
        assert!(parents(&[0..4, 0..4]).is_none());
        assert!(parents(&[0..4, 2..6]).is_none());
        let by_depth = nest(&[(0, 0..4, 3), (1, 0..4, 1), (2, 0..4, 2)]).unwrap();
        let keys: Vec<_> = by_depth.iter().map(|(k, _, p)| (*k, *p)).collect();
        assert_eq!(keys, [(1, None), (2, Some(0)), (0, Some(1))]);
    }

    #[test]
    fn a_chain_longer_than_the_shaders_follow_is_declined() {
        let deep: Vec<_> = (0..=NODE_CHAIN_DEPTH).map(|i| i..20 - i).collect();
        assert!(parents(&deep).is_none());
        assert!(parents(&deep[..NODE_CHAIN_DEPTH]).is_some());
    }

    fn slot(baseline: Pose) -> Slot {
        Slot {
            node: LayoutNodeId::default(),
            range: 0..1,
            parent: None,
            baseline,
            centre: (50.0, 40.0),
            layer: None,
            ambient_clip: None,
            clip_moves: false,
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
            centre: (10.0, 10.0),
            ambient_clip: None,
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

    #[test]
    fn a_node_with_no_primitives_gets_a_slot_under_its_bound_ancestor() {
        // The outer node holds both primitives, the inner one has none and moves only text.
        let bindings = HashMap::from([(id(1), meta(0..2)), (id(2), meta(0..0))]);
        let ancestors = tree_of(&[(1, &[]), (2, &[1])]);
        let slots = NodeRows::plan(&batch_of(2), &bindings, &ancestors).expect("a plan");
        assert_eq!(slots.len(), 2);
        assert_eq!(slots[0].node, id(1));
        assert_eq!(slots[1].node, id(2));
        assert_eq!(slots[1].parent, Some(0));
        assert!(slots[1].range.is_empty());
    }

    #[test]
    fn a_range_that_is_not_an_ancestors_is_declined() {
        // Node 2's range sits inside node 1's, but node 1 is not above it in the tree.
        let bindings = HashMap::from([(id(1), meta(0..4)), (id(2), meta(1..3))]);
        let ancestors = tree_of(&[(1, &[]), (2, &[])]);
        assert!(NodeRows::plan(&batch_of(4), &bindings, &ancestors).is_none());
    }

    #[test]
    fn glyphs_take_the_text_row_of_the_nearest_bound_node() {
        let bindings = HashMap::from([(id(1), meta(0..2)), (id(2), meta(2..4))]);
        let ancestors = tree_of(&[(1, &[]), (2, &[]), (3, &[2]), (4, &[1])]);
        let mut rows = NodeRows::default();
        rows.slots = NodeRows::plan(&batch_of(4), &bindings, &ancestors).unwrap();
        let slot_of = |n: u64| rows.slots.iter().position(|s| s.node == id(n)).unwrap() as u32;

        let mut text = TextBatch::default();
        for _ in 0..3 {
            text.primitives
                .push(blinc_gpu::GpuPrimitive::rect(0.0, 0.0, 2.0, 2.0));
        }
        // Run 0..2 belongs to node 3 (under 2), run 2..3 to node 4 (under 1).
        text.owners = vec![(0..2, id(3), 0, 1.0), (2..3, id(4), 1, 1.0)];
        assert!(rows.give_text_slots(&mut text, &ancestors));
        assert_eq!(text.primitives[0].node_slot(), Some(slot_of(2) * 2 + 1));
        assert_eq!(text.primitives[1].node_slot(), Some(slot_of(2) * 2 + 1));
        assert_eq!(text.primitives[2].node_slot(), Some(slot_of(1) * 2 + 1));
    }

    #[test]
    fn glyphs_with_no_bound_owner_are_left_alone() {
        let bindings = HashMap::from([(id(1), meta(0..2))]);
        let ancestors = tree_of(&[(1, &[]), (5, &[])]);
        let mut rows = NodeRows::default();
        rows.slots = NodeRows::plan(&batch_of(2), &bindings, &ancestors).unwrap();
        let mut text = TextBatch::default();
        text.primitives
            .push(blinc_gpu::GpuPrimitive::rect(0.0, 0.0, 2.0, 2.0));
        text.owners = vec![(0..1, id(5), 0, 1.0)];
        assert!(!rows.give_text_slots(&mut text, &ancestors));
        assert_eq!(text.primitives[0].node_slot(), None);
    }
}
