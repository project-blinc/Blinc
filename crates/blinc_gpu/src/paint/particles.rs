//! GPU particle system drawing.

use super::GpuPaintContext;
use blinc_core::{
    ParticleBlendMode, ParticleEmitterShape, ParticleForce, ParticleSystemData, Rect,
};

impl GpuPaintContext<'_> {
    pub(super) fn draw_particles_impl(&mut self, rect: Rect, particle_data: &ParticleSystemData) {
        use crate::particles::{GpuEmitter, GpuForce};
        use crate::primitives::ParticleViewport3D;

        // Transform the rect to screen coordinates
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

        // Skip if system is not playing
        if !particle_data.playing {
            return;
        }

        // Convert emitter shape to GPU format
        let (shape_type, shape_params) = match &particle_data.emitter {
            ParticleEmitterShape::Point => (0u32, [0.0f32; 4]),
            ParticleEmitterShape::Sphere { radius } => (1u32, [*radius, 0.0, 0.0, 0.0]),
            ParticleEmitterShape::Hemisphere { radius } => (2u32, [*radius, 0.0, 0.0, 0.0]),
            ParticleEmitterShape::Cone { angle, radius } => (3u32, [*angle, *radius, 0.0, 0.0]),
            ParticleEmitterShape::Box { half_extents } => {
                (4u32, [half_extents.x, half_extents.y, half_extents.z, 0.0])
            }
            ParticleEmitterShape::Circle { radius } => (5u32, [*radius, 0.0, 0.0, 0.0]),
        };

        // Create GPU emitter
        let emitter = GpuEmitter {
            position_shape: [
                particle_data.emitter_position.x,
                particle_data.emitter_position.y,
                particle_data.emitter_position.z,
                shape_type as f32,
            ],
            shape_params,
            direction_randomness: [
                particle_data.direction.x,
                particle_data.direction.y,
                particle_data.direction.z,
                particle_data.direction_randomness,
            ],
            emission_config: [
                particle_data.emission_rate,
                particle_data.burst_count, // burst count for one-shot effects
                0.0,                       // spawn accumulated (deprecated)
                particle_data.gravity_scale,
            ],
            lifetime_speed: [
                particle_data.lifetime.0,
                particle_data.lifetime.1,
                particle_data.start_speed.0,
                particle_data.start_speed.1,
            ],
            size_config: [
                particle_data.start_size.0,
                particle_data.start_size.1,
                particle_data.end_size.0,
                particle_data.end_size.1,
            ],
            start_color: [
                particle_data.start_color.r,
                particle_data.start_color.g,
                particle_data.start_color.b,
                particle_data.start_color.a,
            ],
            mid_color: [
                particle_data.mid_color.r,
                particle_data.mid_color.g,
                particle_data.mid_color.b,
                particle_data.mid_color.a,
            ],
            end_color: [
                particle_data.end_color.r,
                particle_data.end_color.g,
                particle_data.end_color.b,
                particle_data.end_color.a,
            ],
        };

        // Convert forces to GPU format
        let forces: Vec<GpuForce> = particle_data
            .forces
            .iter()
            .map(|force| match force {
                ParticleForce::Gravity(dir) => GpuForce {
                    type_strength: [0.0, 1.0, 0.0, 0.0],
                    direction_params: [dir.x, dir.y, dir.z, 0.0],
                },
                ParticleForce::Wind {
                    direction,
                    strength,
                    turbulence,
                } => GpuForce {
                    type_strength: [1.0, *strength, 0.0, 0.0],
                    direction_params: [direction.x, direction.y, direction.z, *turbulence],
                },
                ParticleForce::Vortex {
                    axis,
                    center: _,
                    strength,
                } => GpuForce {
                    type_strength: [2.0, *strength, 0.0, 0.0],
                    direction_params: [axis.x, axis.y, axis.z, 0.0],
                },
                ParticleForce::Drag(coefficient) => GpuForce {
                    type_strength: [3.0, *coefficient, 0.0, 0.0],
                    direction_params: [0.0, 0.0, 0.0, 0.0],
                },
                ParticleForce::Turbulence {
                    strength,
                    frequency,
                } => GpuForce {
                    type_strength: [4.0, *strength, 0.0, 0.0],
                    direction_params: [0.0, 0.0, 0.0, *frequency],
                },
                ParticleForce::Attractor { position, strength } => GpuForce {
                    type_strength: [5.0, *strength, 0.0, 0.0],
                    direction_params: [position.x, position.y, position.z, 0.0],
                },
            })
            .collect();

        // Determine blend mode
        let blend_mode = match particle_data.blend_mode {
            ParticleBlendMode::Alpha => 0,
            ParticleBlendMode::Additive => 1,
            ParticleBlendMode::Multiply => 2,
        };

        // Create and push the particle viewport
        let viewport = ParticleViewport3D {
            emitter,
            forces,
            max_particles: particle_data.max_particles,
            bounds: [clipped_x, clipped_y, clipped_w, clipped_h],
            camera_pos: [
                particle_data.camera_pos.x,
                particle_data.camera_pos.y,
                particle_data.camera_pos.z,
            ],
            camera_target: [
                particle_data.camera_pos.x + particle_data.camera_dir.x,
                particle_data.camera_pos.y + particle_data.camera_dir.y,
                particle_data.camera_pos.z + particle_data.camera_dir.z,
            ],
            camera_up: [
                particle_data.camera_up.x,
                particle_data.camera_up.y,
                particle_data.camera_up.z,
            ],
            fov: 0.8, // Default FOV
            time: particle_data.time,
            delta_time: particle_data.delta_time,
            blend_mode,
            playing: particle_data.playing,
        };

        self.batch.push_particle_viewport(viewport);
    }
}
