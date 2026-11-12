//! The animated node table: one row for each node that moves, which the SDF shaders apply to the
//! primitives that name it (see `sdf_russl::shared::node_table` for the maths).
//!
//! A primitive follows a node through [`GpuPrimitive::set_node_slot`](crate::primitives::GpuPrimitive::set_node_slot);
//! a row's `link` names its parent the same way. The table is a storage buffer and exists only on
//! devices whose vertex stage can read one.

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
    fn slot_zero_is_a_parent_and_none_is_not() {
        let root = NodeRow::new([0.0; 2], [1.0; 2], [0.0; 2], 0.0, 1.0, None);
        let child = NodeRow::new([0.0; 2], [1.0; 2], [0.0; 2], 0.0, 1.0, Some(0));
        assert_eq!(root.parent(), None);
        assert_eq!(child.parent(), Some(0));
    }
}
