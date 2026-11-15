//! The animated node table: a row for each node that moves, holding the delta the GPU applies to
//! the primitives that follow it, so the primitives stay at their settled pose and an animation
//! changes a row, not the batch.
//!
//! A primitive names its node in `node_slot` (`slot + 1`, zero for none). A row's `link` names its
//! parent the same way, and a primitive takes its node's delta and then each parent's, up to four.
//!
//! The delta is the CSS composition about a pivot: scale, then rotate, then translate. It changes
//! the bounds, the corner radius, the border widths, the shadow's offset, blur and spread, the
//! rotation and the local affine, and the alpha of the fill, the gradient end, the border, the
//! shadow and the side colours. Lengths scale by the geometric mean of the two axes. The clip and
//! the gradient geometry stay as they were, except that a clip a primitive says follows the rows
//! (`node_slot.y`) is scaled and moved with them.

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

    // Whether the delta of `node` scales what it moves by at least half a hundredth, the step a
    // glyph's raster size is rounded to.
    pub fn scales_node(node: NodeRow) -> bool {
        return abs(node.motion.z - 1.0) >= 0.005 || abs(node.motion.w - 1.0) >= 0.005;
    }

    // A glyph under a scaling row is put on a whole device pixel, the corner of its quad (which is
    // drawn scaled about its centre by its own affine), so a bitmap drawn at its own size is
    // sampled texel for texel as static text is. A glyph that is turned is left as it is.
    pub fn snap_glyph(p_in: Primitive) -> Primitive {
        let mut p = p_in;
        if p.type_info.x == 7u32 && abs(p.rotation.x) < 0.0001 && abs(p.local_affine.y) < 0.0001 && abs(p.local_affine.z) < 0.0001 {
            let w = p.bounds.z;
            let h = p.bounds.w;
            let left = p.bounds.x + w * 0.5 - w * p.local_affine.x * 0.5;
            let top = p.bounds.y + h * 0.5 - h * p.local_affine.w * 0.5;
            let dx = floor(left - 0.001 + 0.5) - left;
            let dy = floor(top - 0.001 + 0.5) - top;
            p.bounds = Vec4::new(p.bounds.x + dx, p.bounds.y + dy, w, h);
        }
        return p;
    }

    // `p_in` as it is drawn under the delta of `node`.
    // `depth` is the row's place in the primitive's chain, zero for the node's own.
    pub fn apply_node(p_in: Primitive, node: NodeRow, depth: u32) -> Primitive {
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
        let k = sqrt(abs(sx * sy));
        p.corner_radius = p.corner_radius * k;
        // A clip of the node's own, or of an ancestor that moves, is moved by the rows from its
        // depth up: scaled and moved like the bounds, without the turn.
        if p.node_slot.y > 0.5 && depth + 1u32 >= (p.node_slot.y as u32) {
            let cw = p.clip_bounds.z;
            let ch = p.clip_bounds.w;
            let ccx = p.clip_bounds.x + cw * 0.5;
            let ccy = p.clip_bounds.y + ch * 0.5;
            let ncx2 = px + (ccx - px) * sx + node.motion.x;
            let ncy2 = py + (ccy - py) * sy + node.motion.y;
            p.clip_bounds = Vec4::new(ncx2 - cw * sx * 0.5, ncy2 - ch * sy * 0.5, cw * sx, ch * sy);
            p.clip_radius = p.clip_radius * k;
        }
        // A mesh triangle keeps a triangle count and an offset in its border and has no shadow.
        if p.type_info.x != 9u32 {
            p.border = p.border * k;
            p.shadow = p.shadow * k;
        }

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
