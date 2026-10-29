//! Retain callback state in its represented group's command stream.
use super::{GpuPaintContext, GroupGpuPass, PendingGpuPass};
use crate::primitives::LayerCommand;
use blinc_core::{Rect, layer::Affine2D};

impl GpuPaintContext<'_> {
    /// Set the display scale passed to canvas GPU callbacks. Defaults to one.
    pub fn set_gpu_pass_scale_factor(&mut self, scale_factor: f64) {
        self.gpu_pass_scale_factor = scale_factor;
    }

    pub(super) fn capture_group_gpu_pass(
        &mut self,
        mut pending: PendingGpuPass,
        viewport_from_clip: bool,
    ) -> bool {
        let owner = self.layer_command_batch();
        let foreground = self.emit_foreground();
        if !self.layer_stack.iter().any(|state| {
            state.command_batch == owner
                && (state.foreground == foreground || state.cross_stream.is_some())
        }) {
            return false;
        }
        // A path-only clip has no finite legacy AABB. Its captured image rows
        // enforce the contour after the callback edits the full frame.
        if viewport_from_clip {
            pending.viewport = pending
                .viewport
                .filter(|rect| rect.iter().all(|v| v.is_finite()));
        }
        // Callback coordinates are already physical; clips retain their push-time frame.
        self.transform_stack.push(Affine2D::IDENTITY);
        let paint = self.image_geometry(Rect::new(
            0.0,
            0.0,
            self.viewport.width,
            self.viewport.height,
        ));
        self.transform_stack.pop();
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
}
