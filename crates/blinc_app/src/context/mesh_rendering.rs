//! Dispatch canvas mesh/GPU passes and prepare camera/light matrices.
use super::{Arc, GpuPaintContext, GpuRenderer, PendingMesh};

// ─────────────────────────────────────────────────────────────────────────────
// 3D mesh dispatch
// ─────────────────────────────────────────────────────────────────────────────

/// Render captured meshes with shared shadow preparation and opaque-before-blend ordering.
pub(super) fn dispatch_pending_meshes(
    renderer: &mut GpuRenderer,
    target: &wgpu::TextureView,
    width: u32,
    height: u32,
    meshes: &[PendingMesh],
) {
    if meshes.is_empty() {
        return;
    }
    let aspect = if height > 0 {
        width as f32 / height as f32
    } else {
        1.0
    };

    // Upload each shared environment cubemap once, keyed by Arc identity.
    let mut last_env: Option<std::sync::Arc<blinc_core::layer::CubemapData>> = None;

    // Use the first scene camera and directional light to size the shared shadow frustum.
    let first = &meshes[0];
    let (light_dir, _light_intensity_0) = first_directional_light(&first.lights);
    let light_view_proj = compute_shadow_matrix(&first.camera, light_dir);

    // Render every shadow caster before the main passes sample the shadow map. Only the
    // first caster clears it.
    let mut shadow_index: usize = 0;
    for pending in meshes {
        if !pending.mesh.material.casts_shadows {
            continue;
        }
        let model = mat4_to_array(&pending.transform);
        renderer.render_mesh_shadow_pass(&pending.mesh, &model, &light_view_proj, shadow_index);
        shadow_index += 1;
    }
    // If no casters were drawn, skip shadow sampling entirely so the
    // main pass doesn't read a stale / uninitialized shadow map.
    let shadow_matrix: Option<&[f32; 16]> = if shadow_index > 0 {
        Some(&light_view_proj)
    } else {
        None
    };

    // Draw opaque and masked meshes before blended meshes so transparency sees complete
    // opaque depth. Stable sorting preserves submission order within each mode.
    let mut ordered: Vec<&PendingMesh> = meshes.iter().collect();
    ordered.sort_by_key(|p| {
        matches!(
            p.mesh.material.alpha_mode,
            blinc_core::draw::AlphaMode::Blend
        ) as u8
    });

    let batch_count = ordered.len();
    for (batch_index, &pending) in ordered.iter().enumerate() {
        // Upload the environment cubemap only when its `Arc` identity
        // changes. The renderer's texture is overwritten on each real
        // upload, so only the last distinct environment matters.
        if let Some(ref env) = pending.env_cubemap {
            let is_new = last_env
                .as_ref()
                .map(|prev| !std::sync::Arc::ptr_eq(prev, env))
                .unwrap_or(true);
            if is_new {
                renderer.upload_environment_cubemap(env);
                last_env = Some(env.clone());
            }
        }

        // Use the canvas viewport aspect when available so the
        // perspective projection matches the clipped region, not the
        // full frame. Falls back to the frame aspect for full-viewport
        // mesh draws (no canvas wrapper).
        let vp_aspect = pending
            .viewport
            .map(|[_, _, w, h]| if h > 0.0 { w / h } else { 1.0 })
            .unwrap_or(aspect);
        let view_proj = camera_view_proj(&pending.camera, vp_aspect);
        let _inv_view_proj = mat4_inverse_flat(&view_proj);
        let camera_pos = [
            pending.camera.position.x,
            pending.camera.position.y,
            pending.camera.position.z,
        ];
        let lights = collect_directional_lights(&pending.lights);
        let model = mat4_to_array(&pending.transform);

        // Clamp the viewport and skip empty areas before tonemap dispatch.
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
        renderer.render_mesh_data_batched(
            target,
            &pending.mesh,
            &model,
            &view_proj,
            camera_pos,
            &lights,
            shadow_matrix,
            clamped_viewport,
            batch_index,
            batch_count,
        );
    }
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

/// Build a directional-light view-projection matrix centered on the camera target. Scale
/// the orthographic frustum with camera distance.
fn compute_shadow_matrix(camera: &blinc_core::Camera, light_dir: [f32; 3]) -> [f32; 16] {
    let target = camera.target;
    let eye = camera.position;
    let dx = eye.x - target.x;
    let dy = eye.y - target.y;
    let dz = eye.z - target.z;
    let cam_dist = (dx * dx + dy * dy + dz * dz).sqrt().max(1.0);

    // Ortho frustum half-extent — ~1.0× camera distance covers the
    // full scene for typical framings (frame_camera multiplies scene
    // diagonal by 1.1, so camera distance ≈ scene diagonal).
    let half = cam_dist;

    // Light sits on the opposite side of the target from its direction,
    // far enough back that the near plane doesn't clip the scene.
    let ld = blinc_core::Vec3::new(light_dir[0], light_dir[1], light_dir[2]).normalize();
    let light_pos = blinc_core::Vec3::new(
        target.x - ld.x * cam_dist * 2.0,
        target.y - ld.y * cam_dist * 2.0,
        target.z - ld.z * cam_dist * 2.0,
    );

    // Pick an up-vector not parallel to the light direction.
    let up = if ld.y.abs() > 0.95 {
        blinc_core::Vec3::new(0.0, 0.0, 1.0)
    } else {
        blinc_core::Vec3::new(0.0, 1.0, 0.0)
    };

    let view = mat4_look_at(light_pos, target, up);
    let proj = mat4_orthographic_rh(-half, half, -half, half, 0.1, cam_dist * 4.0);
    mat4_mul_flat(&proj, &view)
}

/// Build a right-handed, Y-up camera view-projection matrix. Perspective cameras use the
/// current viewport aspect ratio.
fn camera_view_proj(camera: &blinc_core::Camera, frame_aspect: f32) -> [f32; 16] {
    let view = mat4_look_at(camera.position, camera.target, camera.up);
    let proj = match camera.projection {
        blinc_core::CameraProjection::Perspective {
            fov_y, near, far, ..
        } => mat4_perspective_rh(fov_y, frame_aspect, near, far),
        blinc_core::CameraProjection::Orthographic {
            left,
            right,
            bottom,
            top,
            near,
            far,
        } => mat4_orthographic_rh(left, right, bottom, top, near, far),
    };
    mat4_mul_flat(&proj, &view)
}

/// Extract the first `Light::Directional` from the snapshot, returning
/// a normalized direction vector and scalar intensity. Falls back to a
/// soft top-down fill if none is present so the demo never renders
/// pitch-black.
fn first_directional_light(lights: &[blinc_core::Light]) -> ([f32; 3], f32) {
    for light in lights {
        if let blinc_core::Light::Directional {
            direction,
            intensity,
            ..
        } = light
        {
            let d = direction.normalize();
            return ([d.x, d.y, d.z], *intensity);
        }
    }
    ([0.0, -1.0, 0.3], 0.8)
}

/// Collect every directional light for the mesh shader's multi-light
/// array. Capped at the shader's `MAX_DIR_LIGHTS`; if the scene has
/// no directional lights we hand back a single soft top-down default
/// so the mesh never renders pitch-black.
fn collect_directional_lights(lights: &[blinc_core::Light]) -> Vec<blinc_gpu::DirectionalLight> {
    let max = blinc_gpu::MAX_DIR_LIGHTS;
    let mut out: Vec<blinc_gpu::DirectionalLight> = Vec::with_capacity(max);
    for light in lights {
        if out.len() >= max {
            break;
        }
        if let blinc_core::Light::Directional {
            direction,
            color,
            intensity,
            ..
        } = light
        {
            let d = direction.normalize();
            out.push(blinc_gpu::DirectionalLight {
                direction: [d.x, d.y, d.z],
                intensity: *intensity,
                color: [color.r, color.g, color.b],
            });
        }
    }
    if out.is_empty() {
        out.push(blinc_gpu::DirectionalLight::DEFAULT);
    }
    out
}

/// Flatten a column-major `Mat4` to the `[f32; 16]` layout
/// `GpuRenderer::render_mesh_data` expects.
fn mat4_to_array(m: &blinc_core::Mat4) -> [f32; 16] {
    let mut out = [0.0f32; 16];
    for col in 0..4 {
        for row in 0..4 {
            out[col * 4 + row] = m.cols[col][row];
        }
    }
    out
}

/// Multiply two flat column-major 4×4 matrices (`a * b`), returning a
/// `[f32; 16]` in the same layout. Used to compose `proj * view` after
/// both are computed in `Mat4`/array form.
fn mat4_mul_flat(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut out = [0.0f32; 16];
    for col in 0..4 {
        for row in 0..4 {
            let mut s = 0.0;
            for k in 0..4 {
                s += a[k * 4 + row] * b[col * 4 + k];
            }
            out[col * 4 + row] = s;
        }
    }
    out
}

/// Right-handed look-at view matrix. Produces `[f32; 16]` directly
/// (column-major) for the downstream multiply.
fn mat4_look_at(
    eye: blinc_core::Vec3,
    target: blinc_core::Vec3,
    up: blinc_core::Vec3,
) -> [f32; 16] {
    let f = blinc_core::Vec3::new(target.x - eye.x, target.y - eye.y, target.z - eye.z).normalize();
    let r = f.cross(up).normalize();
    let u = r.cross(f);
    let tx = -(r.x * eye.x + r.y * eye.y + r.z * eye.z);
    let ty = -(u.x * eye.x + u.y * eye.y + u.z * eye.z);
    let tz = f.x * eye.x + f.y * eye.y + f.z * eye.z;
    // Column-major: col0 = [r.x, u.x, -f.x, 0], col1 = [r.y, u.y, -f.y, 0], ...
    [
        r.x, u.x, -f.x, 0.0, r.y, u.y, -f.y, 0.0, r.z, u.z, -f.z, 0.0, tx, ty, tz, 1.0,
    ]
}

/// Right-handed perspective projection. Maps view-space Z in `[-far, -near]`
/// to clip-space depth `[0, 1]` (wgpu convention). `fov_y` is radians.
fn mat4_perspective_rh(fov_y: f32, aspect: f32, near: f32, far: f32) -> [f32; 16] {
    let f = 1.0 / (fov_y * 0.5).tan();
    let nf = 1.0 / (near - far);
    [
        f / aspect,
        0.0,
        0.0,
        0.0,
        0.0,
        f,
        0.0,
        0.0,
        0.0,
        0.0,
        far * nf,
        -1.0,
        0.0,
        0.0,
        far * near * nf,
        0.0,
    ]
}

/// Right-handed orthographic projection. Uses the same clip-space
/// depth range `[0, 1]` as the perspective variant so the mesh shader
/// can stay agnostic of the projection choice.
fn mat4_orthographic_rh(
    left: f32,
    right: f32,
    bottom: f32,
    top: f32,
    near: f32,
    far: f32,
) -> [f32; 16] {
    let rl = 1.0 / (right - left);
    let tb = 1.0 / (top - bottom);
    let fnn = 1.0 / (far - near);
    [
        2.0 * rl,
        0.0,
        0.0,
        0.0,
        0.0,
        2.0 * tb,
        0.0,
        0.0,
        0.0,
        0.0,
        -fnn,
        0.0,
        -(right + left) * rl,
        -(top + bottom) * tb,
        -near * fnn,
        1.0,
    ]
}

/// Inverse of a column-major 4×4 matrix (GLU-style cofactor expansion).
fn mat4_inverse_flat(m: &[f32; 16]) -> [f32; 16] {
    let mut inv = [0.0f32; 16];
    inv[0] = m[5] * m[10] * m[15] - m[5] * m[11] * m[14] - m[9] * m[6] * m[15]
        + m[9] * m[7] * m[14]
        + m[13] * m[6] * m[11]
        - m[13] * m[7] * m[10];
    inv[4] = -m[4] * m[10] * m[15] + m[4] * m[11] * m[14] + m[8] * m[6] * m[15]
        - m[8] * m[7] * m[14]
        - m[12] * m[6] * m[11]
        + m[12] * m[7] * m[10];
    inv[8] = m[4] * m[9] * m[15] - m[4] * m[11] * m[13] - m[8] * m[5] * m[15]
        + m[8] * m[7] * m[13]
        + m[12] * m[5] * m[11]
        - m[12] * m[7] * m[9];
    inv[12] = -m[4] * m[9] * m[14] + m[4] * m[10] * m[13] + m[8] * m[5] * m[14]
        - m[8] * m[6] * m[13]
        - m[12] * m[5] * m[10]
        + m[12] * m[6] * m[9];
    inv[1] = -m[1] * m[10] * m[15] + m[1] * m[11] * m[14] + m[9] * m[2] * m[15]
        - m[9] * m[3] * m[14]
        - m[13] * m[2] * m[11]
        + m[13] * m[3] * m[10];
    inv[5] = m[0] * m[10] * m[15] - m[0] * m[11] * m[14] - m[8] * m[2] * m[15]
        + m[8] * m[3] * m[14]
        + m[12] * m[2] * m[11]
        - m[12] * m[3] * m[10];
    inv[9] = -m[0] * m[9] * m[15] + m[0] * m[11] * m[13] + m[8] * m[1] * m[15]
        - m[8] * m[3] * m[13]
        - m[12] * m[1] * m[11]
        + m[12] * m[3] * m[9];
    inv[13] = m[0] * m[9] * m[14] - m[0] * m[10] * m[13] - m[8] * m[1] * m[14]
        + m[8] * m[2] * m[13]
        + m[12] * m[1] * m[10]
        - m[12] * m[2] * m[9];
    inv[2] = m[1] * m[6] * m[15] - m[1] * m[7] * m[14] - m[5] * m[2] * m[15]
        + m[5] * m[3] * m[14]
        + m[13] * m[2] * m[7]
        - m[13] * m[3] * m[6];
    inv[6] = -m[0] * m[6] * m[15] + m[0] * m[7] * m[14] + m[4] * m[2] * m[15]
        - m[4] * m[3] * m[14]
        - m[12] * m[2] * m[7]
        + m[12] * m[3] * m[6];
    inv[10] = m[0] * m[5] * m[15] - m[0] * m[7] * m[13] - m[4] * m[1] * m[15]
        + m[4] * m[3] * m[13]
        + m[12] * m[1] * m[7]
        - m[12] * m[3] * m[5];
    inv[14] = -m[0] * m[5] * m[14] + m[0] * m[6] * m[13] + m[4] * m[1] * m[14]
        - m[4] * m[2] * m[13]
        - m[12] * m[1] * m[6]
        + m[12] * m[2] * m[5];
    inv[3] = -m[1] * m[6] * m[11] + m[1] * m[7] * m[10] + m[5] * m[2] * m[11]
        - m[5] * m[3] * m[10]
        - m[9] * m[2] * m[7]
        + m[9] * m[3] * m[6];
    inv[7] = m[0] * m[6] * m[11] - m[0] * m[7] * m[10] - m[4] * m[2] * m[11]
        + m[4] * m[3] * m[10]
        + m[8] * m[2] * m[7]
        - m[8] * m[3] * m[6];
    inv[11] = -m[0] * m[5] * m[11] + m[0] * m[7] * m[9] + m[4] * m[1] * m[11]
        - m[4] * m[3] * m[9]
        - m[8] * m[1] * m[7]
        + m[8] * m[3] * m[5];
    inv[15] = m[0] * m[5] * m[10] - m[0] * m[6] * m[9] - m[4] * m[1] * m[10]
        + m[4] * m[2] * m[9]
        + m[8] * m[1] * m[6]
        - m[8] * m[2] * m[5];
    let det = m[0] * inv[0] + m[1] * inv[4] + m[2] * inv[8] + m[3] * inv[12];
    if det.abs() < 1e-12 {
        return [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
    }
    let id = 1.0 / det;
    for v in &mut inv {
        *v *= id;
    }
    inv
}
