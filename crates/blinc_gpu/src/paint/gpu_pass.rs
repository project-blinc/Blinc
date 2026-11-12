//! Canvas GPU callbacks and flow shaders, and their state in a represented group's command stream.
use super::{AuxBatch, GpuPaintContext, GroupGpuPass, PendingFlow, PendingGpuPass};
use crate::primitives::{DynamicImage, LayerCommand};
use blinc_core::{DrawContext, Rect, layer::Affine2D};

impl GpuPaintContext<'_> {
    pub(super) fn gpu_group_owner(&self) -> Option<(AuxBatch, bool)> {
        let owner = self.layer_command_batch();
        let foreground = self.emit_foreground();
        self.layer_stack
            .iter()
            .any(|state| {
                state.command_batch == owner
                    && (state.foreground == foreground || state.cross_stream.is_some())
            })
            .then_some((owner, foreground))
    }

    pub(super) fn group_gpu_paint(&mut self) -> Option<DynamicImage> {
        self.transform_stack.push(Affine2D::IDENTITY);
        let paint = self.image_geometry(Rect::new(
            0.0,
            0.0,
            self.viewport.width,
            self.viewport.height,
        ));
        self.transform_stack.pop();
        paint
    }
    /// Set the display scale passed to canvas GPU callbacks. Defaults to one.
    pub fn set_gpu_pass_scale_factor(&mut self, scale_factor: f64) {
        self.gpu_pass_scale_factor = scale_factor;
    }

    pub(super) fn capture_group_gpu_pass(
        &mut self,
        mut pending: PendingGpuPass,
        viewport_from_clip: bool,
    ) -> bool {
        let Some((owner, foreground)) = self.gpu_group_owner() else {
            return false;
        };
        // A path-only clip has no finite legacy AABB. Its captured image rows
        // enforce the contour after the callback edits the full frame.
        if viewport_from_clip {
            pending.viewport = pending
                .viewport
                .filter(|rect| rect.iter().all(|v| v.is_finite()));
        }
        // Callback coordinates are already physical; clips retain their push-time frame.
        let paint = self.group_gpu_paint();
        let Some(paint) = paint else {
            return true;
        };
        let command = LayerCommand::GpuPass(Box::new(GroupGpuPass {
            pending,
            paint,
            frame_size: (self.viewport.width as u32, self.viewport.height as u32),
            scale_factor: self.gpu_pass_scale_factor,
        }));
        let batch = self.layer_command_batch_mut(owner);
        if foreground {
            batch.push_foreground_layer_command(command);
        } else {
            batch.push_layer_command(command);
        }
        true
    }

    pub(super) fn run_gpu_pass_impl(
        &mut self,
        hook: &dyn blinc_core::draw::GpuPassHook,
        viewport: Option<blinc_core::Rect>,
    ) {
        self.last_group_mesh = None;
        // Downcast through the canonical wrapper type. Any other
        // `GpuPassHook` impl is treated as an opaque no-op — the only
        // sanctioned construction route is `blinc_gpu::GpuPass::new`.
        let Some(pass) = hook.as_any().downcast_ref::<crate::custom_pass::GpuPass>() else {
            return;
        };
        // Resolve the viewport rect in physical pixels. Explicit
        // `Some(rect)` from the caller wins (caller is usually the
        // canvas closure handing through `bounds.rect()`). Otherwise
        // fall back to the current clip-stack AABB, which gives a
        // sensible default when the wrapping widget has already pushed
        // a clip for its layout bounds. `None` at both layers means
        // "render to the full frame target".
        let pixel_viewport = match viewport {
            Some(rect) => {
                let tl = self.transform_point(rect.origin);
                let br = self.transform_point(blinc_core::Point::new(
                    rect.origin.x + rect.size.width,
                    rect.origin.y + rect.size.height,
                ));
                Some([tl.x, tl.y, (br.x - tl.x).abs(), (br.y - tl.y).abs()])
            }
            None => self.current_clip_aabb(),
        };
        let pending = PendingGpuPass {
            pass: pass.clone(),
            viewport: pixel_viewport,
            flow: None,
        };
        if !self.capture_group_gpu_pass(pending.clone(), viewport.is_none()) {
            self.pending_gpu_passes.push(pending);
        }
    }

    pub(super) fn draw_shader_impl(
        &mut self,
        shader: &std::sync::Arc<blinc_core::FlowShader>,
        rect: blinc_core::Rect,
        inputs: &blinc_core::ShaderInputs,
    ) {
        self.last_group_mesh = None;
        let tl = self.transform_point(rect.origin);
        let br = self.transform_point(blinc_core::Point::new(
            rect.origin.x + rect.size.width,
            rect.origin.y + rect.size.height,
        ));
        let (left, top) = (tl.x.min(br.x), tl.y.min(br.y));
        let (width, height) = ((br.x - tl.x).abs(), (br.y - tl.y).abs());
        let mut visible = [left, top, width, height];
        // The clip leaves part of the bounds visible; the shader still maps `uv` over all of them.
        if let Some([cx, cy, cw, ch]) = self.current_clip_aabb() {
            let (x0, y0) = (left.max(cx), top.max(cy));
            let (x1, y1) = ((left + width).min(cx + cw), (top + height).min(cy + ch));
            visible = [x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0)];
        }
        if visible[2] < 1.0 || visible[3] < 1.0 {
            return;
        }
        let scale = if rect.size.width > f32::EPSILON {
            width / rect.size.width
        } else {
            1.0
        };
        let pending = PendingGpuPass {
            pass: crate::custom_pass::GpuPass::none(),
            viewport: Some(visible),
            flow: Some(PendingFlow {
                shader: shader.clone(),
                bounds: [left, top, width, height],
                inputs: *inputs,
                corner_radius: inputs.corner_radius * scale,
            }),
        };
        if !self.capture_group_gpu_pass(pending.clone(), false) {
            self.pending_gpu_passes.push(pending);
        }
    }
}
