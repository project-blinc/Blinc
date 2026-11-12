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

/// An affine on device pixel positions, `[a, b, c, d, tx, ty]`: `x' = a x + c y + tx` and
/// `y' = b x + d y + ty`.
pub type Affine = [f32; 6];

/// `first` and then `second`.
pub fn then(first: Affine, second: Affine) -> Affine {
    let [a1, b1, c1, d1, tx1, ty1] = first;
    let [a2, b2, c2, d2, tx2, ty2] = second;
    [
        a2 * a1 + c2 * b1,
        b2 * a1 + d2 * b1,
        a2 * c1 + c2 * d1,
        b2 * c1 + d2 * d1,
        a2 * tx1 + c2 * ty1 + tx2,
        b2 * tx1 + d2 * ty1 + ty2,
    ]
}

/// What the chain of rows from a slot amounts to for something that is not a primitive.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Chain {
    /// Where the chain puts a point of the painted pose.
    pub affine: Affine,
    /// The one scale lengths (a border, a corner radius) grow by.
    pub scale: f32,
    /// The opacity the rows multiply in.
    pub opacity: f32,
}

impl NodeRow {
    /// This row as an affine on device pixels: scale about the pivot, turn, move.
    pub fn affine(&self) -> Affine {
        let [tx, ty, sx, sy] = self.motion;
        let [px, py, turn, _] = self.pivot;
        let (sin, cos) = turn.sin_cos();
        let (a, b, c, d) = (cos * sx, sin * sx, -sin * sy, cos * sy);
        [
            a,
            b,
            c,
            d,
            px + tx - (a * px + c * py),
            py + ty - (b * px + d * py),
        ]
    }
}

/// The chain of rows from `slot`, each row and then its parent's, as the shaders follow it.
/// The identity when a row is missing.
pub fn chain(rows: &[NodeRow], slot: u32) -> Chain {
    let mut out = Chain {
        affine: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        scale: 1.0,
        opacity: 1.0,
    };
    let mut next = Some(slot);
    for _ in 0..NODE_CHAIN_DEPTH {
        let Some(row) = next.and_then(|s| rows.get(s as usize)) else {
            break;
        };
        out.affine = then(out.affine, row.affine());
        out.scale *= (row.motion[2] * row.motion[3]).abs().sqrt();
        out.opacity *= row.pivot[3];
        next = row.parent();
    }
    out
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
    fn a_chain_as_an_affine_puts_a_point_where_the_rows_put_a_rect() {
        let rows = [
            NodeRow::new([7.0, -3.0], [1.5, 1.5], [30.0, 20.0], 0.4, 0.5, Some(1)),
            NodeRow::new([-5.0, 9.0], [0.8, 0.8], [80.0, 50.0], -0.2, 0.5, None),
        ];
        let rect = [10.0, 20.0, 40.0, 20.0];
        let (cx, cy) = (rect[0] + rect[2] * 0.5, rect[1] + rect[3] * 0.5);
        let c = chain(&rows, 0);
        let [a, b, cc, d, tx, ty] = c.affine;
        let by_affine = (a * cx + cc * cy + tx, b * cx + d * cy + ty);
        let drawn = drawn_bounds(&rows, 0, rect);
        let by_rows = (drawn[0] + drawn[2] * 0.5, drawn[1] + drawn[3] * 0.5);
        assert!((by_affine.0 - by_rows.0).abs() < 1e-3 && (by_affine.1 - by_rows.1).abs() < 1e-3);
        assert!((c.scale - 1.2).abs() < 1e-5);
        assert!((c.opacity - 0.25).abs() < 1e-6);
        // No such row is no change.
        assert_eq!(chain(&rows, 9).affine, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    }

    #[test]
    fn then_applies_the_first_affine_first() {
        let move_then_double = then(
            [1.0, 0.0, 0.0, 1.0, 5.0, 0.0],
            [2.0, 0.0, 0.0, 2.0, 0.0, 0.0],
        );
        assert_eq!(move_then_double, [2.0, 0.0, 0.0, 2.0, 10.0, 0.0]);
    }

    #[test]
    fn slot_zero_is_a_parent_and_none_is_not() {
        let root = NodeRow::new([0.0; 2], [1.0; 2], [0.0; 2], 0.0, 1.0, None);
        let child = NodeRow::new([0.0; 2], [1.0; 2], [0.0; 2], 0.0, 1.0, Some(0));
        assert_eq!(root.parent(), None);
        assert_eq!(child.parent(), Some(0));
    }
}
