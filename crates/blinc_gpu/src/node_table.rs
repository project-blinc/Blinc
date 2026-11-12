//! The animated node table: one row for each node that moves, which the SDF shaders apply to the
//! primitives that name it (see `sdf_russl::shared::node_table` for the maths).
//!
//! A primitive follows a node through [`GpuPrimitive::set_node_slot`](crate::primitives::GpuPrimitive::set_node_slot);
//! a row's `link` names its parent the same way. The table is a storage buffer and exists only on
//! devices whose vertex stage can read one.

use crate::primitives::GpuPrimitive;

/// Rows the table holds. A frame needing more leaves the table unused.
pub const NODE_TABLE_CAPACITY: usize = 1024;

/// The longest parent chain the shaders follow.
pub const NODE_CHAIN_DEPTH: usize = 4;

/// A node's row as the shader reads it.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct NodeRow {
    /// Translate x and y (device px), scale x and y.
    pub motion: [f32; 4],
    /// Pivot x and y (device px), rotation (radians), opacity.
    pub pivot: [f32; 4],
    /// Parent slot + 1, zero for none; the rest is reserved.
    pub link: [f32; 4],
}

impl NodeRow {
    /// The row that changes nothing.
    pub const REST: NodeRow = NodeRow {
        motion: [0.0, 0.0, 1.0, 1.0],
        pivot: [0.0, 0.0, 0.0, 1.0],
        link: [0.0; 4],
    };

    /// A row applying `translate`, `scale` and `rotation` about `pivot`, and `opacity`, under
    /// the node in slot `parent`.
    pub fn new(
        translate: [f32; 2],
        scale: [f32; 2],
        pivot: [f32; 2],
        rotation: f32,
        opacity: f32,
        parent: Option<u32>,
    ) -> Self {
        NodeRow {
            motion: [translate[0], translate[1], scale[0], scale[1]],
            pivot: [pivot[0], pivot[1], rotation, opacity],
            link: [parent.map_or(0.0, |slot| (slot + 1) as f32), 0.0, 0.0, 0.0],
        }
    }

    /// The slot of the parent node, if there is one.
    pub fn parent(&self) -> Option<u32> {
        let link = self.link[0] as u32;
        link.checked_sub(1)
    }
}

impl NodeRow {
    /// Where the rect `b` (x, y, width, height) is drawn under this row: scaled about the pivot,
    /// turned, moved. A turned rect is widened to the square that holds it at any angle, so the
    /// result always covers what the shader draws.
    pub fn apply_to_rect(&self, b: [f32; 4]) -> [f32; 4] {
        let [w, h] = [b[2], b[3]];
        let (cx, cy) = (b[0] + w * 0.5, b[1] + h * 0.5);
        let [tx, ty, sx, sy] = self.motion;
        let [px, py, turn, _] = self.pivot;
        let (rx, ry) = ((cx - px) * sx, (cy - py) * sy);
        let (sin, cos) = turn.sin_cos();
        let ncx = px + rx * cos - ry * sin + tx;
        let ncy = py + rx * sin + ry * cos + ty;
        let (nw, nh) = (w * sx.abs(), h * sy.abs());
        let (hw, hh) = if turn == 0.0 {
            (nw * 0.5, nh * 0.5)
        } else {
            let half_diagonal = (nw * nw + nh * nh).sqrt() * 0.5;
            (half_diagonal, half_diagonal)
        };
        [ncx - hw, ncy - hh, hw * 2.0, hh * 2.0]
    }
}

impl NodeRow {
    /// `p` as it is drawn under this row: the CPU twin of the shaders' `apply_node`, for the
    /// passes that move primitives into another frame of reference before they draw them.
    pub fn apply_to_primitive(&self, mut p: GpuPrimitive, depth: u32) -> GpuPrimitive {
        let [tx, ty, sx, sy] = self.motion;
        let [px, py, turn, opacity] = self.pivot;
        let (sin_d, cos_d) = turn.sin_cos();
        let (w, h) = (p.bounds[2], p.bounds[3]);
        let (cx, cy) = (p.bounds[0] + w * 0.5, p.bounds[1] + h * 0.5);
        let (rx, ry) = ((cx - px) * sx, (cy - py) * sy);
        let ncx = px + rx * cos_d - ry * sin_d + tx;
        let ncy = py + rx * sin_d + ry * cos_d + ty;
        let (nw, nh) = (w * sx, h * sy);
        p.bounds = [ncx - nw * 0.5, ncy - nh * 0.5, nw, nh];
        let k = (sx * sy).abs().sqrt();
        for length in &mut p.corner_radius {
            *length *= k;
        }
        if p.node_clip_from().is_some_and(|from| depth >= from) {
            let [cx, cy, cw, ch] = p.clip_bounds;
            let (ccx, ccy) = (cx + cw * 0.5, cy + ch * 0.5);
            let (ncx, ncy) = (px + (ccx - px) * sx + tx, py + (ccy - py) * sy + ty);
            p.clip_bounds = [ncx - cw * sx * 0.5, ncy - ch * sy * 0.5, cw * sx, ch * sy];
            for length in &mut p.clip_radius {
                *length *= k;
            }
        }
        // A mesh triangle keeps a triangle count and an offset in its border and has no shadow.
        if p.type_info[0] != 9 {
            for length in p.border.iter_mut().chain(&mut p.shadow) {
                *length *= k;
            }
        }
        let (sin_a, cos_a) = (p.rotation[0], p.rotation[1]);
        p.rotation[0] = sin_a * cos_d + cos_a * sin_d;
        p.rotation[1] = cos_a * cos_d - sin_a * sin_d;
        let [a, b, c, d] = p.local_affine;
        p.local_affine = [
            a * cos_d + c * sin_d,
            b * cos_d + d * sin_d,
            -a * sin_d + c * cos_d,
            -b * sin_d + d * cos_d,
        ];
        for colour in [
            &mut p.color,
            &mut p.color2,
            &mut p.border_color,
            &mut p.shadow_color,
            &mut p.side_color_top,
            &mut p.side_color_right,
            &mut p.side_color_bottom,
            &mut p.side_color_left,
        ] {
            colour[3] *= opacity;
        }
        p
    }
}

/// `p` as it is drawn under its node's row and then each parent's, with the slot cleared so
/// nothing applies it again. `p` as it is when it names no row or the row is missing.
pub fn resolve_primitive(rows: &[NodeRow], p: &GpuPrimitive) -> GpuPrimitive {
    let mut out = *p;
    let mut next = p.node_slot();
    for depth in 0..NODE_CHAIN_DEPTH as u32 {
        let Some(row) = next.and_then(|s| rows.get(s as usize)) else {
            break;
        };
        out = row.apply_to_primitive(out, depth);
        next = row.parent();
    }
    out.set_node_slot(None);
    out
}

/// Where a primitive with bounds `b` and slot `slot` is drawn under `rows`: the rect after its
/// node's row and then each parent's, as the shaders apply them. The rect as given when a row
/// is missing.
pub fn drawn_bounds(rows: &[NodeRow], slot: u32, b: [f32; 4]) -> [f32; 4] {
    let mut b = b;
    let mut next = Some(slot);
    for _ in 0..NODE_CHAIN_DEPTH {
        let Some(row) = next.and_then(|s| rows.get(s as usize)) else {
            break;
        };
        b = row.apply_to_rect(b);
        next = row.parent();
    }
    b
}

impl Default for NodeRow {
    fn default() -> Self {
        Self::REST
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_is_three_vec4() {
        assert_eq!(std::mem::size_of::<NodeRow>(), 48);
    }

    #[test]
    fn a_rect_is_drawn_where_its_rows_put_it() {
        let rect = [10.0, 20.0, 40.0, 20.0];
        let moved = NodeRow::new([5.0, -3.0], [1.0; 2], [0.0; 2], 0.0, 1.0, None);
        assert_eq!(moved.apply_to_rect(rect), [15.0, 17.0, 40.0, 20.0]);
        // Doubled about its own centre it grows around it.
        let grown = NodeRow::new([0.0; 2], [2.0; 2], [30.0, 30.0], 0.0, 1.0, None);
        assert_eq!(grown.apply_to_rect(rect), [-10.0, 10.0, 80.0, 40.0]);
        // A turned rect is covered at any angle.
        let turned = NodeRow::new([0.0; 2], [1.0; 2], [30.0, 30.0], 0.7, 1.0, None);
        let b = turned.apply_to_rect(rect);
        assert!(b[2] >= 40.0 && b[3] >= 20.0);
        // A parent's row applies to what the child's left.
        let rows = [
            NodeRow::new([5.0, 0.0], [1.0; 2], [0.0; 2], 0.0, 1.0, Some(1)),
            NodeRow::new([0.0, 7.0], [1.0; 2], [0.0; 2], 0.0, 1.0, None),
        ];
        assert_eq!(drawn_bounds(&rows, 0, rect), [15.0, 27.0, 40.0, 20.0]);
        assert_eq!(drawn_bounds(&rows, 9, rect), rect, "no such row");
    }

    #[test]
    fn slot_zero_is_a_parent_and_none_is_not() {
        let root = NodeRow::new([0.0; 2], [1.0; 2], [0.0; 2], 0.0, 1.0, None);
        let child = NodeRow::new([0.0; 2], [1.0; 2], [0.0; 2], 0.0, 1.0, Some(0));
        assert_eq!(root.parent(), None);
        assert_eq!(child.parent(), Some(0));
    }
}
