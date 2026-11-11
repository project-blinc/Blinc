//! What every SDF pipeline shares: the viewport uniforms and the `Primitive` the CPU uploads.

russl::russl! {
    pub struct Uniforms {
        pub viewport_size: Vec2,
        pub _padding: Vec2,
    }

    pub struct Primitive {
        // Bounds (x, y, width, height)
        pub bounds: Vec4,
        // Corner radii (top-left, top-right, bottom-right, bottom-left)
        pub corner_radius: Vec4,
        // Fill color (or gradient start color)
        pub color: Vec4,
        // Gradient end color (for gradients)
        pub color2: Vec4,
        // Border (width, 0, 0, 0)
        pub border: Vec4,
        // Border color
        pub border_color: Vec4,
        // Shadow (offset_x, offset_y, blur, spread)
        pub shadow: Vec4,
        // Shadow color
        pub shadow_color: Vec4,
        // Clip bounds (x, y, width, height) for rect clips, (cx, cy, rx, ry) for circle/ellipse
        pub clip_bounds: Vec4,
        // Clip corner radii (for rounded rect) or (radius_x, radius_y, 0, 0) for ellipse
        pub clip_radius: Vec4,
        // Clip corner shape (superellipse n per corner) for the rounded
        // rect clip — n=1.0 = round (default), 2.0 = squircle, 0.0 = bevel,
        // -1.0 = scoop. Lets overflow:clip on a squircle parent follow
        // the same curve as the parent fill instead of a circular cut.
        pub clip_corner_shape: Vec4,
        // Gradient parameters: linear (x1, y1, x2, y2), radial (cx, cy, r, 0) in user space
        pub gradient_params: Vec4,
        // Rotation (sin_rz, cos_rz, sin_ry, cos_ry) - for rotated SDF evaluation
        pub rotation: Vec4,
        // Local 2x2 affine (a, b, c, d) - normalized (DPI removed).
        // Maps local rect space → screen space. Supports rotation, scale, AND skew.
        // Identity = (1, 0, 0, 1).
        pub local_affine: Vec4,
        // Perspective (sin_rx, cos_rx, perspective_d, shape_3d_type)
        // shape_3d_type: 0=none, 1=box, 2=sphere, 3=cylinder, 4=torus, 5=capsule, 6=group
        pub perspective: Vec4,
        // SDF 3D params (depth, ambient, specular_power, translate_z)
        pub sdf_3d: Vec4,
        // Light params (dir_x, dir_y, dir_z, intensity)
        pub light: Vec4,
        // CSS filter A (grayscale, invert, sepia, hue_rotate_rad)
        pub filter_a: Vec4,
        // CSS filter B (brightness, contrast, saturate, 0)
        pub filter_b: Vec4,
        // Mask gradient params: linear=(x1,y1,x2,y2), radial=(cx,cy,r,0) in OBB (0-1) space
        pub mask_params: Vec4,
        // Mask info: (mask_type, start_alpha, end_alpha, 0)
        // mask_type: 0=none, 1=linear, 2=radial
        pub mask_info: Vec4,
        // Corner shape (superellipse n parameter per corner)
        // n=1.0 = round (default), n=0.0 = bevel, n=2.0 = squircle, n=-1.0 = scoop
        pub corner_shape: Vec4,
        // Overflow fade distances (top, right, bottom, left) in pixels
        pub clip_fade: Vec4,
        // Type info (primitive_type, fill_type, clip_type, 0)
        pub type_info: UVec4,
        // Border colour of each side when the sides differ; alphas that sum to
        // zero mean unset, and the border is `border_color` all round.
        pub side_color_top: Vec4,
        pub side_color_right: Vec4,
        pub side_color_bottom: Vec4,
        pub side_color_left: Vec4,
        // A gradient's stop table in AUX_DATA: one more than its offset, zero for none.
        pub gradient_table: Vec4,
        // A clip in the primitive's own frame, its width zero for none.
        pub local_clip_bounds: Vec4,
        pub local_clip_radius: Vec4,
        // Separate screen fade rectangle; zero width uses clip_bounds.
        pub clip_fade_bounds: Vec4,
        // ABI notch scale and shadow mode; zero scale keeps native shading.
        pub notch_info: Vec4,
        // One more than the AUX_DATA header offset for an ABI shape clip, zero for none.
        pub clip_shape: Vec4,
    }
}
