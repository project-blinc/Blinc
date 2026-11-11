//! Draining and resetting the recorded batch, and replaying recorded draw commands.

use super::{AuxBatch, GpuPaintContext, PendingGpuPass, PendingMesh};
use crate::primitives::PrimitiveBatch;
use blinc_core::{Affine2D, BlendMode, DrawCommand, DrawContext, Size};

impl GpuPaintContext<'_> {
    /// Take the accumulated batch for rendering
    pub fn take_batch(&mut self) -> PrimitiveBatch {
        self.forget_clip_offsets(AuxBatch::Static);
        self.batch.resolve_cross_stream_groups();
        self.rejected_cross_stream_groups.clear();
        self.overlay_ancestor_ids.clear();
        std::mem::take(&mut self.batch)
    }

    /// Drain ungrouped mesh instances. Native group scenes stay in the batch commands.
    pub fn take_pending_meshes(&mut self) -> Vec<PendingMesh> {
        std::mem::take(&mut self.pending_meshes)
    }

    /// Drain ungrouped canvas callbacks. Grouped callbacks stay in the batch commands.
    pub fn take_pending_gpu_passes(&mut self) -> Vec<PendingGpuPass> {
        std::mem::take(&mut self.pending_gpu_passes)
    }

    /// Get a reference to the current batch
    pub fn batch(&self) -> &PrimitiveBatch {
        &self.batch
    }

    /// Get a mutable reference to the current batch
    pub fn batch_mut(&mut self) -> &mut PrimitiveBatch {
        &mut self.batch
    }

    /// Clear the batch
    pub fn clear(&mut self) {
        self.batch.clear();
        self.transform_stack = vec![Affine2D::IDENTITY];
        self.opacity_stack = vec![1.0];
        self.blend_mode_stack = vec![BlendMode::Normal];
        self.clip_stack.clear();
        self.layer_stack.clear();
        self.rejected_cross_stream_groups.clear();
        self.overlay_ancestor_ids.clear();
        self.cross_stream_static_depth = 0;
        self.overlay_layer_scope = None;
        self.overlay_visit_sequence = 0;
        self.overlay_root_pending = None;
        self.static_motion_depth = None;
        self.is_3d = false;
        self.camera = None;
    }

    /// Snapshot the current transform-stack depth. Pair with
    /// [`Self::restore_transform_stack`] to GUARANTEE the stack
    /// returns to this depth at the end of a paint section, even
    /// when an inner code path leaks a push (early return, conditional
    /// pop with mutated gate, panic-recover, etc.). Without this,
    /// continuous_redraw frames that re-run the walker compound any
    /// single-call leak — observed as `current_affine` doubling per
    /// frame (`a`/`d` going 2 → 4 → 8 → 16) and the canvas appearing
    /// to zoom out exponentially.
    pub fn transform_stack_depth(&self) -> usize {
        self.transform_stack.len()
    }

    /// Truncate the transform stack back to the snapshot depth
    /// returned by [`Self::transform_stack_depth`]. Safe to call when
    /// the stack is already at-or-below that depth (no-op then).
    pub fn restore_transform_stack(&mut self, depth: usize) {
        if self.transform_stack.len() > depth {
            self.transform_stack.truncate(depth);
        }
    }

    /// Resize the viewport
    pub fn resize(&mut self, width: f32, height: f32) {
        self.viewport = Size::new(width, height);
    }

    /// Execute a list of recorded draw commands
    pub fn execute_commands(&mut self, commands: &[DrawCommand]) {
        for cmd in commands {
            self.execute_command(cmd);
        }
    }

    /// Execute a single draw command
    pub fn execute_command(&mut self, cmd: &DrawCommand) {
        match cmd {
            DrawCommand::PushTransform(t) => self.push_transform(t.clone()),
            DrawCommand::PopTransform => self.pop_transform(),
            DrawCommand::PushClip(shape) => self.push_clip(shape.clone()),
            DrawCommand::PopClip => self.pop_clip(),
            DrawCommand::PushOpacity(o) => self.push_opacity(*o),
            DrawCommand::PopOpacity => self.pop_opacity(),
            DrawCommand::PushBlendMode(m) => self.push_blend_mode(*m),
            DrawCommand::PopBlendMode => self.pop_blend_mode(),
            DrawCommand::FillPath { path, brush } => self.fill_path(path, brush.clone()),
            DrawCommand::StrokePath {
                path,
                stroke,
                brush,
            } => self.stroke_path(path, stroke, brush.clone()),
            DrawCommand::FillRect {
                rect,
                corner_radius,
                brush,
            } => self.fill_rect(*rect, *corner_radius, brush.clone()),
            DrawCommand::StrokeRect {
                rect,
                corner_radius,
                stroke,
                brush,
            } => self.stroke_rect(*rect, *corner_radius, stroke, brush.clone()),
            DrawCommand::FillCircle {
                center,
                radius,
                brush,
            } => self.fill_circle(*center, *radius, brush.clone()),
            DrawCommand::StrokeCircle {
                center,
                radius,
                stroke,
                brush,
            } => self.stroke_circle(*center, *radius, stroke, brush.clone()),
            DrawCommand::DrawText {
                text,
                origin,
                style,
            } => self.draw_text(text, *origin, style),
            DrawCommand::DrawImage {
                image,
                rect,
                options,
            } => self.draw_image(*image, *rect, options),
            DrawCommand::DrawShadow {
                rect,
                corner_radius,
                shadow,
            } => self.draw_shadow(*rect, *corner_radius, *shadow),
            DrawCommand::DrawInnerShadow {
                rect,
                corner_radius,
                shadow,
            } => self.draw_inner_shadow(*rect, *corner_radius, *shadow),
            DrawCommand::DrawCircleShadow {
                center,
                radius,
                shadow,
            } => self.draw_circle_shadow(*center, *radius, *shadow),
            DrawCommand::DrawCircleInnerShadow {
                center,
                radius,
                shadow,
            } => self.draw_circle_inner_shadow(*center, *radius, *shadow),
            DrawCommand::SetCamera(camera) => self.set_camera(camera),
            DrawCommand::DrawMesh {
                mesh,
                material,
                transform,
            } => self.draw_mesh(*mesh, *material, *transform),
            DrawCommand::DrawMeshInstanced { mesh, instances } => {
                self.draw_mesh_instanced(*mesh, instances)
            }
            DrawCommand::AddLight(light) => self.add_light(light.clone()),
            DrawCommand::SetEnvironment(env) => self.set_environment(env),
            DrawCommand::PushLayer(config) => self.push_layer(config.clone()),
            DrawCommand::PopLayer => self.pop_layer(),
            DrawCommand::SampleLayer {
                id,
                source_rect,
                dest_rect,
            } => self.sample_layer(*id, *source_rect, *dest_rect),
        }
    }
}
