//! Keep compatible consecutive mesh draws in one ordered scene.
use super::{GpuPaintContext, GroupMeshes, PendingMesh};
use crate::primitives::{DynamicImage, LayerCommand, PrimitiveBatch};
use blinc_core::{Camera, CameraProjection};

fn cursor(batch: &PrimitiveBatch) -> [usize; 8] {
    [
        batch.primitives.len(),
        batch.foreground_primitives.len(),
        batch.paths.indices.len(),
        batch.foreground_paths.indices.len(),
        batch.dynamic_images.len(),
        batch.foreground_dynamic_images.len(),
        batch.layer_commands.len(),
        batch.foreground_layer_commands.len(),
    ]
}

pub(crate) fn camera_signature(camera: &Camera) -> [f32; 16] {
    let projection = match camera.projection {
        CameraProjection::Perspective {
            fov_y,
            aspect,
            near,
            far,
        } => [0.0, fov_y, aspect, near, far, 0.0, 0.0],
        CameraProjection::Orthographic {
            left,
            right,
            bottom,
            top,
            near,
            far,
        } => [1.0, left, right, bottom, top, near, far],
    };
    [
        camera.position.x,
        camera.position.y,
        camera.position.z,
        camera.target.x,
        camera.target.y,
        camera.target.z,
        camera.up.x,
        camera.up.y,
        camera.up.z,
        projection[0],
        projection[1],
        projection[2],
        projection[3],
        projection[4],
        projection[5],
        projection[6],
    ]
}

fn same_coverage(a: &DynamicImage, b: &DynamicImage) -> bool {
    a.dest == b.dest
        && a.transform == b.transform
        && a.opacity == b.opacity
        && a.clip_bounds == b.clip_bounds
        && a.clip_radius == b.clip_radius
        && a.clip_aux == b.clip_aux
}

impl GpuPaintContext<'_> {
    pub(super) fn capture_group_mesh(&mut self, pending: PendingMesh) -> bool {
        let Some((owner, foreground)) = self.gpu_group_owner() else {
            self.last_group_mesh = None;
            return false;
        };
        let Some(paint) = self.group_gpu_paint() else {
            self.last_group_mesh = None;
            return true;
        };
        let frame_size = (self.viewport.width as u32, self.viewport.height as u32);
        let previous = self.last_group_mesh;
        let batch = self.layer_command_batch_mut(owner);
        let current = cursor(batch);
        if let Some((old_owner, old_foreground, index, saved)) = previous
            && old_owner == owner
            && old_foreground == foreground
            && saved == current
        {
            let commands = if foreground {
                &mut batch.foreground_layer_commands
            } else {
                &mut batch.layer_commands
            };
            if let Some(entry) = commands.get_mut(index)
                && let LayerCommand::Meshes(scene) = &mut entry.command
                && scene.frame_size == frame_size
                && same_coverage(&scene.paint, &paint)
                && scene.meshes.last().is_some_and(|last| {
                    last.viewport == pending.viewport
                        && camera_signature(&last.camera) == camera_signature(&pending.camera)
                })
            {
                scene.meshes.push(pending);
                return true;
            }
        }
        let command = LayerCommand::Meshes(Box::new(GroupMeshes {
            meshes: vec![pending],
            paint,
            frame_size,
        }));
        let index = if foreground {
            let index = batch.foreground_layer_commands.len();
            batch.push_foreground_layer_command(command);
            index
        } else {
            let index = batch.layer_commands.len();
            batch.push_layer_command(command);
            index
        };
        let saved = cursor(batch);
        self.last_group_mesh = Some((owner, foreground, index, saved));
        true
    }
}
