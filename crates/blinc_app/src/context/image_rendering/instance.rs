//! Image fit, shader styling and legacy clip preparation.
use super::{GpuImageInstance, ImageElement, RenderContext};
use blinc_image::{ObjectFit, ObjectPosition, calculate_fit_rects, src_rect_to_uv};

impl RenderContext {
    pub(in crate::context) fn image_instance(
        image: &ImageElement,
        width: u32,
        height: u32,
        fade_factor: f32,
        scale_factor: f32,
    ) -> GpuImageInstance {
        let object_fit = match image.object_fit {
            0 => ObjectFit::Cover,
            1 => ObjectFit::Contain,
            2 => ObjectFit::Fill,
            3 => ObjectFit::ScaleDown,
            4 => ObjectFit::None,
            _ => ObjectFit::Cover,
        };

        let object_position =
            ObjectPosition::new(image.object_position[0], image.object_position[1]);

        let (src_rect, dst_rect) = calculate_fit_rects(
            width,
            height,
            image.width,
            image.height,
            object_fit,
            object_position,
        );

        let src_uv = src_rect_to_uv(src_rect, width, height);

        let base_x = image.x + dst_rect[0];
        let base_y = image.y + dst_rect[1];
        let base_w = dst_rect[2];
        let base_h = dst_rect[3];

        let (draw_x, draw_y, draw_w, draw_h, ta, tb, tc, td) =
            if let Some(affine) = image.css_affine {
                Self::decompose_image_affine(base_x, base_y, base_w, base_h, affine, scale_factor)
            } else {
                (base_x, base_y, base_w, base_h, 1.0, 0.0, 0.0, 1.0)
            };

        let effective_clip = image.clip_bounds.map(|clip| {
            if let Some(affine) = image.css_affine {
                Self::transform_clip_by_affine(clip, image.clip_radius, affine, scale_factor)
            } else {
                (clip, image.clip_radius)
            }
        });

        let mut instance = GpuImageInstance::new(draw_x, draw_y, draw_w, draw_h)
            .with_src_uv(src_uv[0], src_uv[1], src_uv[2], src_uv[3])
            .with_tint(image.tint[0], image.tint[1], image.tint[2], image.tint[3])
            .with_border_radius(image.border_radius)
            .with_opacity(image.opacity * fade_factor)
            .with_transform(ta, tb, tc, td)
            .with_filter(image.filter_a, image.filter_b);

        if image.border_width > 0.0 {
            instance = instance.with_image_border(
                image.border_width,
                image.border_color.r,
                image.border_color.g,
                image.border_color.b,
                image.border_color.a,
            );
        }

        if image.mask_info[0] > 0.5 {
            instance.mask_params = image.mask_params;
            instance.mask_info = image.mask_info;
        }

        if let Some((clip, clip_r)) = effective_clip {
            instance = instance.with_clip_rounded_rect_corners(
                clip[0], clip[1], clip[2], clip[3], clip_r[0], clip_r[1], clip_r[2], clip_r[3],
            );
        }
        if let Some(sc) = image.scroll_clip {
            instance = instance.with_clip2_rect(sc[0], sc[1], sc[2], sc[3]);
        }

        instance
    }
}
