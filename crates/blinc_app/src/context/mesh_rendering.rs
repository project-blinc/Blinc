//! Dispatch independent canvas GPU output.
use super::{GpuRenderer, PendingMesh};

/// Dispatch independently queued mesh scenes through the shared GPU implementation.
pub(super) fn dispatch_pending_meshes(
    renderer: &mut GpuRenderer,
    target: &wgpu::TextureView,
    width: u32,
    height: u32,
    meshes: &[PendingMesh],
) {
    renderer.render_pending_meshes(target, width, height, meshes);
}

/// Dispatch captured user passes within nonempty, target-clamped viewports.
pub(super) fn dispatch_pending_gpu_passes(
    renderer: &mut GpuRenderer,
    target: &wgpu::TextureView,
    width: u32,
    height: u32,
    scale_factor: f32,
    passes: &[blinc_gpu::paint::PendingGpuPass],
) {
    if passes.is_empty() {
        return;
    }
    let device = renderer.device_arc();
    let queue = renderer.queue_arc();
    let format = renderer.surface_format();
    for pending in passes {
        // Clamp user-pass viewports to the current target and skip empty areas before
        // dispatch.
        let clamped_viewport = match pending.viewport {
            Some(r) => {
                let c = clamp_viewport(r, width, height);
                if c[2] < 1.0 || c[3] < 1.0 {
                    continue;
                }
                Some(c)
            }
            None => None,
        };
        let ctx = blinc_gpu::custom_pass::RenderPassContext {
            device: &device,
            queue: &queue,
            target,
            viewport_width: width,
            viewport_height: height,
            texture_format: format,
            scale_factor: scale_factor as f64,
            view_proj: None,
            inv_view_proj: None,
            camera_pos: None,
            viewport: clamped_viewport,
        };
        pending
            .pass
            .initialize_and_render(&device, &queue, format, &ctx);
    }
}

/// Clamp a viewport rect to a render-target extent. `[x, y, w, h]` in
/// physical pixels — both origin and size adjusted so `x + w ≤ tw`
/// and `y + h ≤ th`. Returns a zero-size rect (still inside the
/// target) when the input is fully outside.
fn clamp_viewport(rect: [f32; 4], target_w: u32, target_h: u32) -> [f32; 4] {
    let [x, y, w, h] = rect;
    let tw = target_w as f32;
    let th = target_h as f32;
    let x = x.max(0.0).min(tw);
    let y = y.max(0.0).min(th);
    let w = w.max(0.0).min((tw - x).max(0.0));
    let h = h.max(0.0).min((th - y).max(0.0));
    [x, y, w, h]
}
