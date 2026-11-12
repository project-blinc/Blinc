//! 3D content: mesh data, billboards, 3D viewports and SDF viewports.

use super::{GpuPaintContext, PendingMesh};
use crate::primitives::{Sdf3DUniform, Viewport3D};
use blinc_core::{BillboardFacing, Camera, DrawContext, Mat4, MeshData, Rect, Sdf3DViewport, Size};

impl GpuPaintContext<'_> {
    pub(super) fn draw_mesh_data_impl(&mut self, mesh: std::sync::Arc<MeshData>, transform: Mat4) {
        let camera = self.camera.clone().unwrap_or_default();

        let viewport = self.mesh_viewport_bounds.map(|(lw, lh)| {
            let tl = self.transform_point(blinc_core::Point::new(0.0, 0.0));
            let br = self.transform_point(blinc_core::Point::new(lw, lh));
            [tl.x, tl.y, (br.x - tl.x).abs(), (br.y - tl.y).abs()]
        });
        // The canvas viewport remains active for every mesh in the scene.
        let pending = PendingMesh {
            mesh,
            transform,
            camera,
            lights: self.lights.clone(),
            viewport,
            env_cubemap: self.pending_env.clone(),
        };
        if !self.capture_group_mesh(pending.clone()) {
            self.pending_meshes.push(pending);
        }
    }

    pub(super) fn billboard_draw_impl(
        &mut self,
        _size: Size,
        _transform: Mat4,
        _facing: BillboardFacing,
        f: &mut dyn FnMut(&mut dyn DrawContext),
    ) {
        // For now, just execute the 2D content without the billboard transform
        // Real implementation would require 3D projection
        f(self);
    }

    pub(super) fn viewport_3d_draw_impl(
        &mut self,
        _rect: Rect,
        camera: &Camera,
        f: &mut dyn FnMut(&mut dyn DrawContext),
    ) {
        // Set up 3D context
        let was_3d = self.is_3d;
        let old_camera = self.camera.take();
        self.set_camera(camera);

        // Execute 3D drawing
        f(self);

        // Restore 2D context
        self.is_3d = was_3d;
        self.camera = old_camera;
    }

    pub(super) fn draw_sdf_viewport_impl(&mut self, rect: Rect, viewport: &Sdf3DViewport) {
        // Transform the rect to screen coordinates (like fill_rect does)
        let transformed = self.transform_rect(rect);

        // Get current clip bounds and intersect with the viewport
        let (clip_bounds, _, _, _) = self.get_clip_data();
        let clip_min_x = clip_bounds[0];
        let clip_min_y = clip_bounds[1];
        let clip_max_x = clip_bounds[0] + clip_bounds[2];
        let clip_max_y = clip_bounds[1] + clip_bounds[3];

        // Original viewport bounds
        let orig_x = transformed.x();
        let orig_y = transformed.y();
        let orig_w = transformed.width();
        let orig_h = transformed.height();

        // Intersect viewport with clip region
        let clipped_x = orig_x.max(clip_min_x);
        let clipped_y = orig_y.max(clip_min_y);
        let clipped_right = (orig_x + orig_w).min(clip_max_x);
        let clipped_bottom = (orig_y + orig_h).min(clip_max_y);
        let clipped_w = (clipped_right - clipped_x).max(0.0);
        let clipped_h = (clipped_bottom - clipped_y).max(0.0);

        // Skip if viewport is fully clipped
        if clipped_w <= 0.0 || clipped_h <= 0.0 {
            return;
        }

        // Calculate UV offset and scale for clipped viewports
        let uv_offset_x = if orig_w > 0.0 {
            (clipped_x - orig_x) / orig_w
        } else {
            0.0
        };
        let uv_offset_y = if orig_h > 0.0 {
            (clipped_y - orig_y) / orig_h
        } else {
            0.0
        };
        let uv_scale_x = if orig_w > 0.0 {
            clipped_w / orig_w
        } else {
            1.0
        };
        let uv_scale_y = if orig_h > 0.0 {
            clipped_h / orig_h
        } else {
            1.0
        };

        // Create the uniform data for the shader
        // Must match the WGSL SdfUniform struct layout exactly
        let uniforms = Sdf3DUniform {
            camera_pos: [
                viewport.camera_pos.x,
                viewport.camera_pos.y,
                viewport.camera_pos.z,
                1.0,
            ],
            camera_dir: [
                viewport.camera_dir.x,
                viewport.camera_dir.y,
                viewport.camera_dir.z,
                0.0,
            ],
            camera_up: [
                viewport.camera_up.x,
                viewport.camera_up.y,
                viewport.camera_up.z,
                0.0,
            ],
            camera_right: [
                viewport.camera_right.x,
                viewport.camera_right.y,
                viewport.camera_right.z,
                0.0,
            ],
            // Use ORIGINAL resolution for correct aspect ratio calculation
            resolution: [orig_w, orig_h],
            time: viewport.time,
            fov: viewport.fov,
            max_steps: viewport.max_steps,
            max_distance: viewport.max_distance,
            epsilon: viewport.epsilon,
            _padding: 0.0,
            uv_offset: [uv_offset_x, uv_offset_y],
            uv_scale: [uv_scale_x, uv_scale_y],
        };

        // Create and push the 3D viewport with CLIPPED bounds
        let viewport_3d = Viewport3D {
            shader_wgsl: viewport.shader_wgsl.clone(),
            uniforms,
            bounds: [clipped_x, clipped_y, clipped_w, clipped_h],
            lights: viewport.lights.clone(),
        };

        self.batch.push_viewport_3d(viewport_3d);
    }
}
