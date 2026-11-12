//! Moving the primitives of motion-bound nodes with rows of the GPU node table, not by patching
//! them.
//!
//! After a full paint each motion binding owns a range of the cached dynamic batch, baked at the
//! binding's values of that moment. [`NodeRows::apply`] gives the primitives of a range the slot
//! of their innermost binding and writes one row per binding holding the change since the bake,
//! so a frame changes rows and leaves the batch alone. Anything the rows cannot express is
//! declined, and the caller patches the batch on the CPU as before.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};

use blinc_gpu::node_table::{NODE_CHAIN_DEPTH, NODE_TABLE_CAPACITY, NodeRow};
use blinc_gpu::primitives::LayerCommand;
use blinc_gpu::{GpuRenderer, PrimitiveBatch};
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

fn next_epoch() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

impl NodeRows {
    /// The row for a binding that went from `slot.baseline` to `now`. `None` when the change is
    /// not a scale, turn and move about the baseline pivot: a non-uniform scale does not commute
    /// with a turn, and a scale from nothing has no ratio.
    fn row(slot: &Slot, now: &Pose, dpi: f32) -> Option<NodeRow> {
        let was = &slot.baseline;
        // A group's opacity is a layer command, not a row.
        let layered = slot.layer.is_some();
        let turn = now.rotation - was.rotation;
        let uniform = |s: (f32, f32)| (s.0 - s.1).abs() <= EPS;
        if turn.abs() > EPS && !(uniform(was.scale) && uniform(now.scale)) {
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
            slot.parent,
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
        let ranges: Vec<_> = bindings
            .iter()
            .map(|(node, meta)| (*node, meta.primitive_range.clone(), ancestors(*node).len()))
            .collect();
        let nested = nest(&ranges)?;
        let mut slots: Vec<Slot> = Vec::with_capacity(nested.len());
        for (node, range, parent) in nested {
            if range.end > batch.primitives.len() {
                return None;
            }
            let meta = bindings.get(&node)?;
            // A parent is an ancestor in the tree, not only a range that happens to hold this one.
            if let Some(p) = parent
                && !ancestors(node).contains(&slots[p as usize].node)
            {
                return None;
            }
            let baseline = Pose::of_meta(meta);
            // A clip of the node's own should move with it, and one of an ancestor should not; a
            // row cannot tell which this is.
            if batch.primitives[range.clone()]
                .iter()
                .any(|p| p.type_info[2] != 0)
            {
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
            // A turned ancestor puts this node's own moves on turned axes, which a row works in
            // the world's.
            let mut up = parent;
            while let Some(p) = up {
                let ancestor: &Slot = &slots[p as usize];
                if ancestor.baseline.rotation.abs() > EPS {
                    return None;
                }
                up = ancestor.parent;
            }
            slots.push(Slot {
                node,
                range,
                parent,
                baseline,
                centre: meta.centre,
                layer: meta.layer_push_index,
            });
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
            && bindings
                .values()
                .filter(|m| !m.primitive_range.is_empty())
                .count()
                == self.slots.len()
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
        bindings: &mut HashMap<LayoutNodeId, CompositeBindingMeta>,
    ) {
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

    /// Bring the rows up to date with the bindings, giving the batch its slots first if it has
    /// none.
    pub(super) fn apply(
        &mut self,
        renderer: &GpuRenderer,
        batch: &mut PrimitiveBatch,
        bindings: &mut HashMap<LayoutNodeId, CompositeBindingMeta>,
        motion: &HashMap<LayoutNodeId, MotionBindings>,
        ancestors: &dyn Fn(LayoutNodeId) -> Vec<LayoutNodeId>,
        dpi: f32,
    ) -> Outcome {
        let given = self.describes(batch, bindings);
        if !given {
            self.release(batch, bindings);
            let Some(slots) = Self::plan(batch, bindings, ancestors) else {
                return Outcome::Declined;
            };
            self.slots = slots;
        }

        let mut rows = Vec::with_capacity(self.slots.len());
        for slot in &self.slots {
            let Some(b) = motion.get(&slot.node) else {
                return self.decline(given, batch, bindings);
            };
            let Some(row) = Self::row(slot, &Pose::of(b, slot.baseline.opacity), dpi) else {
                return self.decline(given, batch, bindings);
            };
            rows.push(row);
        }

        if !given {
            let epoch = next_epoch();
            for (i, slot) in self.slots.iter().enumerate() {
                for p in &mut batch.primitives[slot.range.clone()] {
                    p.set_node_slot(Some(i as u32));
                }
            }
            batch.node_slots_epoch = epoch;
            self.epoch = epoch;
            self.rows.clear();
        }
        if rows != self.rows {
            if !renderer.set_node_rows(&rows) {
                return self.decline(true, batch, bindings);
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
        for (node, meta) in bindings.iter_mut() {
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
        bindings: &mut HashMap<LayoutNodeId, CompositeBindingMeta>,
    ) -> Outcome {
        if slots_given {
            self.release(batch, bindings);
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
        let row = NodeRows::row(&slot(was), &now, 2.0).unwrap();
        assert_eq!(row.motion, [8.0, -6.0, 1.5, 1.5]);
        assert_eq!(row.pivot, [100.0, 80.0, 0.25, 2.0]);
        assert_eq!(row.parent(), None);
    }

    #[test]
    fn an_unchanged_binding_has_the_row_that_changes_nothing() {
        let row = NodeRows::row(&slot(REST), &REST, 1.0).unwrap();
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
        assert!(NodeRows::row(&slot(REST), &stretched, 1.0).is_some());
        assert!(NodeRows::row(&slot(REST), &turned, 1.0).is_none());
        // A scale from nothing has no ratio, nor has a fade from nothing.
        let nothing = Pose {
            scale: (0.0, 0.0),
            ..REST
        };
        assert!(NodeRows::row(&slot(nothing), &REST, 1.0).is_none());
        let invisible = Pose {
            opacity: 0.0,
            ..REST
        };
        assert!(NodeRows::row(&slot(invisible), &REST, 1.0).is_none());
        assert!(NodeRows::row(&slot(invisible), &invisible, 1.0).is_some());
    }
}
