//! The image shader: texture sampling with UV cropping, tint, rounded corners, a border, opacity,
//! two clips, an ABI clip and mapping, CSS filters and a mask gradient. The instance comes from a
//! vertex buffer. `TIER` 0 reads the auxiliary data from a storage buffer, 2 from a data texture;
//! the image bind group has the auxiliary data at slot 3, which the specialisations set.

russl::russl! {
    use super::access::UNIFORMS;
    use super::aux::aux;
    use super::clip::abi_shape_clip_alpha;
    use super::common::sd_shaped_rect;

    #[pipeline] const TIER: u32 = 0;
    #[pipeline] const AUX_SLOT: u32 = 5;

    #[group(0)] #[binding(1)] static IMAGE_TEXTURE: Texture2D<f32>;
    #[group(0)] #[binding(2)] static IMAGE_SAMPLER: Sampler;

    struct VertexOutput {
        #[builtin(position)] position: Vec4,
        #[location(0)] uv: Vec2,
        #[location(1)] tint: Vec4,
        #[location(2)] local_pos: Vec2,
        #[location(3)] rect_size: Vec2,
        // (border_radius, opacity, border_width, packed border colour as f32 bits)
        #[location(4)] params: Vec4,
        #[location(5)] world_pos: Vec2,
        #[location(6)] clip_bounds: Vec4,
        #[location(7)] clip_radius: Vec4,
        #[location(8)] filter_a: Vec4,
        #[location(9)] filter_b: Vec4,
        #[location(10)] clip2_bounds: Vec4,
        #[location(11)] mask_params: Vec4,
        #[location(12)] mask_info: Vec4,
        #[location(13)] #[flat] abi_clip: Vec3,
    }

    #[vertex]
    fn vs_main(
        #[builtin(vertex_index)] vertex_index: u32,
        // Destination rectangle (x, y, width, height) in screen pixels
        #[location(0)] dst_rect: Vec4,
        // Source UV rectangle (u_min, v_min, u_max, v_max)
        #[location(1)] src_uv: Vec4,
        // Tint colour (RGBA)
        #[location(2)] tint: Vec4,
        // (border_radius, opacity, border_width, packed_border_color)
        #[location(3)] params: Vec4,
        // Clip bounds (x, y, width, height); a very negative x means no clip
        #[location(4)] clip_bounds: Vec4,
        // Clip corner radii (top-left, top-right, bottom-right, bottom-left)
        #[location(5)] clip_radius: Vec4,
        // CSS filter A (grayscale, invert, sepia, hue_rotate_rad)
        #[location(6)] filter_a: Vec4,
        // CSS filter B (brightness, contrast, saturate, unused)
        #[location(7)] filter_b: Vec4,
        // 2x2 affine transform (a, b, c, d) applied around the quad centre; identity is (1, 0, 0, 1)
        #[location(8)] transform: Vec4,
        // Secondary clip bounds (x, y, width, height): a sharp rect for a scroll boundary
        #[location(9)] clip2_bounds: Vec4,
        // Mask gradient: linear (x1, y1, x2, y2), radial (cx, cy, r, 0), in the image's own 0..1 space
        #[location(10)] mask_params: Vec4,
        // Mask info: (mask_type, start_alpha, end_alpha, 0); type 0 none, 1 linear, 2 radial
        #[location(11)] mask_info: Vec4,
        // ABI metadata: clip header pointer, alpha-mask flag, mapping header pointer, reserved
        #[location(12)] abi_clip: Vec4,
    ) -> VertexOutput {
        // Two triangles over the quad. A match rather than an indexed array: the PowerVR Vulkan
        // driver compiles indexing a constant array wrongly (see `sdf_core`).
        let mut local_pos: Vec2;
        match vertex_index {
            0u32 => { local_pos = Vec2::new(0.0, 0.0); }
            1u32 => { local_pos = Vec2::new(1.0, 0.0); }
            2u32 => { local_pos = Vec2::new(1.0, 1.0); }
            3u32 => { local_pos = Vec2::new(1.0, 1.0); }
            4u32 => { local_pos = Vec2::new(0.0, 1.0); }
            _ => { local_pos = Vec2::new(0.0, 0.0); }
        }

        let mut x = dst_rect.x + local_pos.x * dst_rect.z;
        let mut y = dst_rect.y + local_pos.y * dst_rect.w;

        // The transform turns the quad about its centre, when it is not the identity.
        let ta = transform.x;
        let tb = transform.y;
        let tc = transform.z;
        let td = transform.w;
        if ta != 1.0 || tb != 0.0 || tc != 0.0 || td != 1.0 {
            let cx = dst_rect.x + dst_rect.z * 0.5;
            let cy = dst_rect.y + dst_rect.w * 0.5;
            let dx = x - cx;
            let dy = y - cy;
            x = cx + ta * dx + tc * dy;
            y = cy + tb * dx + td * dy;
        }

        let ndc_x = (x / UNIFORMS.viewport_size.x) * 2.0 - 1.0;
        let ndc_y = 1.0 - (y / UNIFORMS.viewport_size.y) * 2.0;

        let uv = Vec2::new(
            mix(src_uv.x, src_uv.z, local_pos.x),
            mix(src_uv.y, src_uv.w, local_pos.y),
        );

        let mut output: VertexOutput;
        output.position = Vec4::new(ndc_x, ndc_y, 0.0, 1.0);
        output.uv = uv;
        output.tint = tint;
        output.local_pos = local_pos * Vec2::new(dst_rect.z, dst_rect.w);
        output.rect_size = Vec2::new(dst_rect.z, dst_rect.w);
        output.params = params;
        output.world_pos = Vec2::new(x, y);
        output.clip_bounds = clip_bounds;
        output.clip_radius = clip_radius;
        output.filter_a = filter_a;
        output.filter_b = filter_b;
        output.clip2_bounds = clip2_bounds;
        output.mask_params = mask_params;
        output.mask_info = mask_info;
        output.abi_clip = abi_clip.xyz;

        return output;
    }

    // Signed distance to a rounded rect with one radius.
    fn rounded_rect_sdf(pos: Vec2, size: Vec2, radius: f32) -> f32 {
        let half_size = size * 0.5;
        let center_pos = pos - half_size;
        let r = min(radius, min(half_size.x, half_size.y));
        let q = abs(center_pos) - half_size + Vec2::splat(r);
        return min(max(q.x, q.y), 0.0) + length(max(q, Vec2::splat(0.0))) - r;
    }

    // Signed distance to a rounded rect with a radius per corner (top-left, top-right,
    // bottom-right, bottom-left).
    fn rounded_rect_sdf_corners(p: Vec2, origin: Vec2, size: Vec2, radius: Vec4) -> f32 {
        let half_size = size * 0.5;
        let center = origin + half_size;
        let rel = p - center;
        let q = abs(rel) - half_size;

        let mut r: f32;
        if rel.y < 0.0 {
            if rel.x > 0.0 {
                r = radius.y;
            } else {
                r = radius.x;
            }
        } else {
            if rel.x > 0.0 {
                r = radius.z;
            } else {
                r = radius.w;
            }
        }

        r = min(r, min(half_size.x, half_size.y));
        let q_adjusted = q + Vec2::splat(r);
        return length(max(q_adjusted, Vec2::splat(0.0))) + min(max(q_adjusted.x, q_adjusted.y), 0.0) - r;
    }

    // Alpha of a rounded rect clip, 1 inside. The edge is a fixed 0.75 px either side: a
    // derivative width cannot be used after the discards in `fs_main`.
    fn calculate_clip_alpha(p: Vec2, clip_bounds: Vec4, clip_radius: Vec4) -> f32 {
        if clip_bounds.x < -9000.0 {
            return 1.0;
        }
        let clip_d = rounded_rect_sdf_corners(p, clip_bounds.xy, clip_bounds.zw, clip_radius);
        let aa_width = 0.75;
        return 1.0 - smoothstep(-aa_width, aa_width, clip_d);
    }

    // CSS filters on a colour: filter_a = (grayscale, invert, sepia, hue_rotate_rad),
    // filter_b = (brightness, contrast, saturate, 0), in sRGB space.
    fn apply_image_filter(color: Vec4, fa: Vec4, fb: Vec4) -> Vec4 {
        let mut rgb = color.xyz;

        let grayscale = fa.x;
        if grayscale > 0.0 {
            let lum = dot(rgb, Vec3::new(0.2126, 0.7152, 0.0722));
            rgb = mix(rgb, Vec3::new(lum, lum, lum), grayscale);
        }

        let sepia = fa.z;
        if sepia > 0.0 {
            let sepia_r = dot(rgb, Vec3::new(0.393, 0.769, 0.189));
            let sepia_g = dot(rgb, Vec3::new(0.349, 0.686, 0.168));
            let sepia_b = dot(rgb, Vec3::new(0.272, 0.534, 0.131));
            rgb = mix(rgb, Vec3::new(sepia_r, sepia_g, sepia_b), sepia);
        }

        let invert = fa.y;
        if invert > 0.0 {
            rgb = mix(rgb, Vec3::splat(1.0) - rgb, invert);
        }

        let hue_rad = fa.w;
        if abs(hue_rad) > 0.001 {
            let cos_h = cos(hue_rad);
            let sin_h = sin(hue_rad);
            let w = Vec3::new(0.2126, 0.7152, 0.0722);
            let r = Vec3::new(
                cos_h + (1.0 - cos_h) * w.x,
                (1.0 - cos_h) * w.x * w.y - sin_h * w.z,
                (1.0 - cos_h) * w.x * w.z + sin_h * w.y
            );
            let g = Vec3::new(
                (1.0 - cos_h) * w.x * w.y + sin_h * w.z,
                cos_h + (1.0 - cos_h) * w.y,
                (1.0 - cos_h) * w.y * w.z - sin_h * w.x
            );
            let b = Vec3::new(
                (1.0 - cos_h) * w.x * w.z - sin_h * w.y,
                (1.0 - cos_h) * w.y * w.z + sin_h * w.x,
                cos_h + (1.0 - cos_h) * w.z
            );
            rgb = Vec3::new(dot(rgb, r), dot(rgb, g), dot(rgb, b));
        }

        let brightness = fb.x;
        rgb = rgb * brightness;

        let contrast = fb.y;
        rgb = (rgb - Vec3::splat(0.5)) * contrast + Vec3::splat(0.5);

        let saturation = fb.z;
        if abs(saturation - 1.0) > 0.001 {
            let lum = dot(rgb, Vec3::new(0.2126, 0.7152, 0.0722));
            rgb = mix(Vec3::new(lum, lum, lum), rgb, saturation);
        }

        let clamped = clamp(rgb, Vec3::splat(0.0), Vec3::splat(1.0));
        return Vec4::new(clamped.x, clamped.y, clamped.z, color.w);
    }

    #[fragment]
    #[location(0)]
    fn fs_main(input: VertexOutput) -> Vec4 {
        // Both clips first: a pixel outside either is discarded.
        let mut clip_alpha = calculate_clip_alpha(input.world_pos, input.clip_bounds, input.clip_radius);
        if input.abi_clip.x > 0.5 {
            let base = (input.abi_clip.x as u32) - 1u32;
            let info = aux(base + 4u32);
            if input.clip_bounds.x >= -9000.0 {
                let mut corners = Vec4::splat(info.x);
                if info.z > 0.5 { corners = aux((info.z as u32) - 1u32); }
                let distance = sd_shaped_rect(input.world_pos, input.clip_bounds.xy, input.clip_bounds.zw, input.clip_radius, corners);
                clip_alpha = 1.0 - smoothstep(-0.75, 0.75, distance);
            }
            let local = aux(base);
            if local.z > 0.0 {
                let distance = sd_shaped_rect(input.local_pos, local.xy, local.zw, aux(base + 1u32), Vec4::splat(info.x));
                clip_alpha = clip_alpha * (1.0 - smoothstep(-info.y, info.y, distance));
            }
            let bounds = aux(base + 2u32);
            let fade = aux(base + 3u32);
            if fade.x > 0.0 { clip_alpha = clip_alpha * clamp((input.world_pos.y - bounds.y) / fade.x, 0.0, 1.0); }
            if fade.y > 0.0 { clip_alpha = clip_alpha * clamp((bounds.x + bounds.z - input.world_pos.x) / fade.y, 0.0, 1.0); }
            if fade.z > 0.0 { clip_alpha = clip_alpha * clamp((bounds.y + bounds.w - input.world_pos.y) / fade.z, 0.0, 1.0); }
            if fade.w > 0.0 { clip_alpha = clip_alpha * clamp((input.world_pos.x - bounds.x) / fade.w, 0.0, 1.0); }
            clip_alpha = clip_alpha * abi_shape_clip_alpha(input.world_pos, info.w);
        }
        if clip_alpha < 0.001 {
            discard!();
        }
        // The secondary clip: a sharp rect, no corner radius.
        let clip2_alpha = calculate_clip_alpha(input.world_pos, input.clip2_bounds, Vec4::splat(0.0));
        if clip2_alpha < 0.001 {
            discard!();
        }

        let border_radius = input.params.x;
        let opacity = input.params.y;
        let border_width = input.params.z;

        // Level 0 explicitly: the discards above make the control flow non-uniform, where an
        // implicit-derivative sample is not allowed. The texture has no mipmaps.
        let mut uv = input.uv;
        if input.abi_clip.z > 0.5 {
            let base = (input.abi_clip.z as u32) - 1u32;
            let mapping = aux(base);
            let info = aux(base + 1u32);
            let repeat = info.x as u32;
            let mut at = (input.local_pos - mapping.xy) / mapping.zw;
            if (repeat & 1u32) != 0u32 {
                at.x = fract(at.x);
            } else {
                clip_alpha = clip_alpha * (1.0 - smoothstep(-info.y, info.y, max(mapping.x - input.local_pos.x, input.local_pos.x - mapping.x - mapping.z)));
            }
            if (repeat & 2u32) != 0u32 {
                at.y = fract(at.y);
            } else {
                clip_alpha = clip_alpha * (1.0 - smoothstep(-info.y, info.y, max(mapping.y - input.local_pos.y, input.local_pos.y - mapping.y - mapping.w)));
            }
            let half_texel = Vec2::splat(0.5) / textureDimensions(IMAGE_TEXTURE).as_vec2();
            uv = clamp(at, half_texel, Vec2::splat(1.0) - half_texel);
        }
        let mut color = textureSampleLevel(IMAGE_TEXTURE, IMAGE_SAMPLER, uv, 0.0);

        // The tint multiplies the texel, or for an alpha mask replaces its colour.
        if input.abi_clip.y > 0.5 {
            color = Vec4::new(input.tint.x, input.tint.y, input.tint.z, color.w * input.tint.w);
        } else {
            color = color * input.tint;
        }

        let fa = input.filter_a;
        let fb = input.filter_b;
        if fa.x != 0.0 || fa.y != 0.0 || fa.z != 0.0 || abs(fa.w) > 0.001 || fb.x != 1.0 || fb.y != 1.0 || fb.z != 1.0 {
            color = apply_image_filter(color, fa, fb);
        }

        color.w = color.w * opacity;

        // Rounded corners, and a border that fades in from the inside.
        if border_radius > 0.0 || border_width > 0.0 {
            let sdf = rounded_rect_sdf(input.local_pos, input.rect_size, border_radius);
            let outer_aa = 1.0 - smoothstep(-1.0, 1.0, sdf);

            if border_width > 0.0 {
                // The border colour is packed in `params.w`, 8 bits a channel.
                let packed = input.params.w.to_bits();
                let bc = Vec4::new(
                    ((packed >> 24u32) & 0xFFu32) as f32 / 255.0,
                    ((packed >> 16u32) & 0xFFu32) as f32 / 255.0,
                    ((packed >> 8u32) & 0xFFu32) as f32 / 255.0,
                    (packed & 0xFFu32) as f32 / 255.0,
                );
                let border_factor = smoothstep(-border_width - 1.0, -border_width + 1.0, sdf);
                color = Vec4::new(
                    mix(color.x, bc.x, border_factor),
                    mix(color.y, bc.y, border_factor),
                    mix(color.z, bc.z, border_factor),
                    mix(color.w, bc.w, border_factor),
                );
            }

            color.w = color.w * outer_aa;
        }

        color.w = color.w * clip_alpha * clip2_alpha;

        // Mask gradient: linear or radial in the image's own space.
        let mask_type = input.mask_info.x;
        if mask_type > 0.5 {
            let mask_uv = input.local_pos / max(input.rect_size, Vec2::splat(0.001));
            let mut mask_t: f32;
            if mask_type < 1.5 {
                let m_start = input.mask_params.xy;
                let m_end = input.mask_params.zw;
                let m_dir = m_end - m_start;
                let m_len_sq = dot(m_dir, m_dir);
                if m_len_sq > 0.0001 {
                    mask_t = clamp(dot(mask_uv - m_start, m_dir) / m_len_sq, 0.0, 1.0);
                } else {
                    mask_t = 0.0;
                }
            } else {
                let m_center = input.mask_params.xy;
                let m_radius = input.mask_params.z;
                mask_t = clamp(length(mask_uv - m_center) / max(m_radius, 0.001), 0.0, 1.0);
            }
            let mask_alpha = mix(input.mask_info.y, input.mask_info.z, mask_t);
            color = Vec4::new(color.x * mask_alpha, color.y * mask_alpha, color.z * mask_alpha, color.w * mask_alpha);
        }

        // Premultiplied alpha, for the blend state One, OneMinusSrcAlpha.
        return Vec4::new(color.x * color.w, color.y * color.w, color.z * color.w, color.w);
    }
}
