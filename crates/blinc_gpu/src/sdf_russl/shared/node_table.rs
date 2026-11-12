//! The animated node table: a row for each node that moves, holding the delta the GPU applies to
//! the primitives that follow it, so the primitives stay at their settled pose and an animation
//! changes a row, not the batch.
//!
//! A primitive names its node in `node_slot` (`slot + 1`, zero for none). A row's `link` names its
//! parent the same way, and a primitive takes its node's delta and then each parent's, up to four.
//!
//! The delta is the CSS composition about a pivot: scale, then rotate, then translate. It changes
//! what the CPU patcher in `apply_binding_deltas` changes: the bounds, the corner radius, the
//! rotation and the local affine, and the alpha of the fill, the gradient end, the border, the
//! shadow and the side colours. It leaves the border width, the shadow's offset and blur, the
//! clip and the gradient geometry as they were, as that patcher does.

russl::russl! {
    use super::primitive::Primitive;

    pub struct NodeRow {
        // translate x, translate y (device px), scale x, scale y
        pub motion: Vec4,
        // pivot x, pivot y (device px), rotation (radians), opacity
        pub pivot: Vec4,
        // parent slot + 1, zero for none; the rest reserved
        pub link: Vec4,
    }

    // `p_in` as it is drawn under the delta of `node`.
    pub fn apply_node(p_in: Primitive, node: NodeRow) -> Primitive {
        let mut p = p_in;
        let sx = node.motion.z;
        let sy = node.motion.w;
        let px = node.pivot.x;
        let py = node.pivot.y;
        let sin_d = sin(node.pivot.z);
        let cos_d = cos(node.pivot.z);
        let opacity = node.pivot.w;

        let w = p.bounds.z;
        let h = p.bounds.w;
        let cx = p.bounds.x + w * 0.5;
        let cy = p.bounds.y + h * 0.5;
        // The centre scaled about the pivot, turned about it, then moved.
        let rx = (cx - px) * sx;
        let ry = (cy - py) * sy;
        let ncx = px + rx * cos_d - ry * sin_d + node.motion.x;
        let ncy = py + rx * sin_d + ry * cos_d + node.motion.y;
        let nw = w * sx;
        let nh = h * sy;
        p.bounds = Vec4::new(ncx - nw * 0.5, ncy - nh * 0.5, nw, nh);
        p.corner_radius = p.corner_radius * sqrt(abs(sx * sy));

        // The turn composes onto the rotation, working in (sin, cos) to avoid an atan2, and onto
        // the affine by multiplying on the right.
        let sin_a = p.rotation.x;
        let cos_a = p.rotation.y;
        p.rotation.x = sin_a * cos_d + cos_a * sin_d;
        p.rotation.y = cos_a * cos_d - sin_a * sin_d;
        let a = p.local_affine.x;
        let b = p.local_affine.y;
        let c = p.local_affine.z;
        let d = p.local_affine.w;
        p.local_affine = Vec4::new(
            a * cos_d + c * sin_d,
            b * cos_d + d * sin_d,
            -a * sin_d + c * cos_d,
            -b * sin_d + d * cos_d
        );

        p.color.w = p.color.w * opacity;
        p.color2.w = p.color2.w * opacity;
        p.border_color.w = p.border_color.w * opacity;
        p.shadow_color.w = p.shadow_color.w * opacity;
        p.side_color_top.w = p.side_color_top.w * opacity;
        p.side_color_right.w = p.side_color_right.w * opacity;
        p.side_color_bottom.w = p.side_color_bottom.w * opacity;
        p.side_color_left.w = p.side_color_left.w * opacity;
        return p;
    }
}
