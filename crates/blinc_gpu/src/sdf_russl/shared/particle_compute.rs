//! The particle simulation, one thread for each particle: a live particle ages, feels the forces,
//! moves and changes colour and size over its life; a dead one may spawn from the emitter. The
//! random numbers are a PCG hash seeded from the particle's index, the time and a random seed.

russl::russl! {
    use super::particle_data::Particle;

    struct Emitter {
        // xyz position, w shape type
        position_shape: Vec4,
        // shape-specific parameters
        shape_params: Vec4,
        // xyz direction, w randomness
        direction_randomness: Vec4,
        // rate, burst, spawn accumulator, gravity scale
        emission_config: Vec4,
        // min life, max life, min speed, max speed
        lifetime_speed: Vec4,
        // start min, start max, end min, end max
        size_config: Vec4,
        // young particles
        start_color: Vec4,
        // middle age
        mid_color: Vec4,
        // old particles
        end_color: Vec4,
    }

    struct Force {
        // type, strength, 0, 0
        type_strength: Vec4,
        // xyz direction or position, w extra
        direction_params: Vec4,
    }

    struct SimUniforms {
        // dt, time, max_particles, active
        time_config: Vec4,
        // four random seeds
        random_seed: Vec4,
        // num_forces, 0, 0, 0
        force_config: Vec4,
    }

    #[group(0)] #[binding(0)] static PARTICLES: StorageMut<[Particle]>;
    #[group(0)] #[binding(1)] static EMITTER: Uniform<Emitter>;
    #[group(0)] #[binding(2)] static UNIFORMS: Uniform<SimUniforms>;
    #[group(0)] #[binding(3)] static FORCES: Storage<[Force]>;

    // The thread's random state: each thread has its own copy of a private variable.
    static mut SEED: u32 = 0u32;

    // The emitter shapes: 0 point, 1 sphere, 2 hemisphere, 3 cone, 4 box, 5 circle. The force
    // types: 0 gravity, 1 wind, 2 vortex, 3 drag, 4 turbulence, 5 attractor, 6 radial.

    // PCG random number generator.
    fn pcg_hash(input: u32) -> u32 {
        let state = input * 747796405u32 + 2891336453u32;
        let word = ((state >> ((state >> 28u32) + 4u32)) ^ state) * 277803737u32;
        return (word >> 22u32) ^ word;
    }

    fn random_float() -> f32 {
        SEED = pcg_hash(SEED);
        return (SEED as f32) / 4294967295.0;
    }

    fn random_range(min_val: f32, max_val: f32) -> f32 {
        return min_val + random_float() * (max_val - min_val);
    }

    fn random_unit_vector() -> Vec3 {
        let theta = random_float() * 6.283185;
        let z = random_range(-1.0, 1.0);
        let r = sqrt(1.0 - z * z);
        return Vec3::new(r * cos(theta), r * sin(theta), z);
    }

    // A spawn position by the emitter's shape.
    fn get_spawn_position() -> Vec3 {
        let shape = EMITTER.position_shape.w as u32;
        let base_pos = EMITTER.position_shape.xyz;
        let mut result = base_pos;

        match shape {
            1u32 => {
                let radius = EMITTER.shape_params.x;
                let dir = random_unit_vector();
                let r = radius * pow(random_float(), 1.0 / 3.0);
                result = base_pos + dir * r;
            }
            2u32 => {
                let radius = EMITTER.shape_params.x;
                let mut dir = random_unit_vector();
                // The upper hemisphere.
                dir.y = abs(dir.y);
                let r = radius * pow(random_float(), 1.0 / 3.0);
                result = base_pos + dir * r;
            }
            3u32 => {
                let angle = EMITTER.shape_params.x;
                let radius = EMITTER.shape_params.y;
                let theta = random_float() * 6.283185;
                let r = radius * sqrt(random_float());
                result = base_pos + Vec3::new(r * cos(theta), 0.0, r * sin(theta));
            }
            4u32 => {
                let half_extent = EMITTER.shape_params.xyz;
                result = base_pos + Vec3::new(
                    random_range(-half_extent.x, half_extent.x),
                    random_range(-half_extent.y, half_extent.y),
                    random_range(-half_extent.z, half_extent.z)
                );
            }
            5u32 => {
                let radius = EMITTER.shape_params.x;
                let theta = random_float() * 6.283185;
                let r = radius * sqrt(random_float());
                result = base_pos + Vec3::new(r * cos(theta), 0.0, r * sin(theta));
            }
            _ => {}
        }

        return result;
    }

    // A spawn velocity: the emitter's direction mixed with a random one, at a random speed.
    fn get_spawn_velocity() -> Vec3 {
        let base_dir = normalize(EMITTER.direction_randomness.xyz);
        let randomness = EMITTER.direction_randomness.w;

        let random_dir = random_unit_vector();
        let dir = normalize(mix(base_dir, random_dir, randomness));

        let speed = random_range(EMITTER.lifetime_speed.z, EMITTER.lifetime_speed.w);
        return dir * speed;
    }

    // A force applied to a velocity.
    fn apply_force(force: Force, pos: Vec3, vel: Vec3, dt: f32) -> Vec3 {
        let force_type = force.type_strength.x as u32;
        let strength = force.type_strength.y;
        let mut new_vel = vel;

        match force_type {
            0u32 => {
                new_vel = vel + force.direction_params.xyz * dt;
            }
            1u32 => {
                new_vel = vel + force.direction_params.xyz * strength * dt;
            }
            2u32 => {
                let axis = normalize(force.direction_params.xyz);
                let to_particle = pos;
                let tangent = cross(axis, to_particle);
                new_vel = vel + normalize(tangent) * strength * dt;
            }
            3u32 => {
                new_vel = vel * (1.0 - strength * dt);
            }
            4u32 => {
                // 3D turbulence with a phase for each particle. The velocity's magnitude seeds
                // it, so particles at one position still get their own turbulence.
                let freq = force.direction_params.w;
                let t = UNIFORMS.time_config.y;

                let vel_seed = dot(vel, Vec3::new(12.9898, 78.233, 37.719));
                let particle_offset = fract(sin(vel_seed) * 43758.5453) * 6.28318;

                // Position-based noise, with the phase.
                let p1 = pos * freq + Vec3::new(t * 1.3 + particle_offset, t * 0.7, t * 1.1);
                let p2 = pos * freq * 1.7 + Vec3::new(t * 0.9, t * 1.4 + particle_offset, t * 0.6);

                // Multi-octave noise.
                let n1 = sin(p1.x + particle_offset) * cos(p1.y) * sin(p1.z + t);
                let n2 = cos(p1.y * 1.3 + particle_offset) * sin(p1.z * 0.8) * cos(p1.x + t * 0.7);
                let n3 = sin(p1.z * 1.1) * cos(p1.x * 0.9 + particle_offset) * sin(p1.y + t * 1.2);

                // The second octave.
                let m1 = sin(p2.x + particle_offset * 0.7) * cos(p2.z) * 0.5;
                let m2 = cos(p2.y) * sin(p2.x + particle_offset * 1.3) * 0.5;
                let m3 = sin(p2.z + particle_offset * 0.5) * cos(p2.y) * 0.5;

                // More variation vertically, for dancing flames.
                let turb = Vec3::new(
                    n1 + m1,
                    (n2 + m2) * 0.5,
                    n3 + m3
                );

                new_vel = vel + turb * strength * dt;
            }
            5u32 => {
                let attractor_pos = force.direction_params.xyz;
                let to_attractor = attractor_pos - pos;
                let dist = max(length(to_attractor), 0.1);
                new_vel = vel + normalize(to_attractor) * strength / (dist * dist) * dt;
            }
            6u32 => {
                // A centring force toward the Y axis: a negative strength pulls in, a positive
                // one pushes out, and it grows with the distance.
                let center = force.direction_params.xyz;
                let to_center = Vec3::new(center.x - pos.x, 0.0, center.z - pos.z);
                let dist = max(length(to_center), 0.01);
                new_vel = vel + to_center * strength * dt;
            }
            _ => {}
        }

        return new_vel;
    }

    #[compute(workgroup_size(64))]
    fn cs_main(#[builtin(global_invocation_id)] global_id: UVec3) {
        let idx = global_id.x;
        let max_particles = UNIFORMS.time_config.z as u32;

        if idx >= max_particles {
            return;
        }

        let mut p = PARTICLES[idx];
        let dt = UNIFORMS.time_config.x;
        let time = UNIFORMS.time_config.y;

        // The seed comes from the particle's index and the time, and a random seed from the CPU.
        SEED = idx + (time * 1000.0) as u32;
        SEED = pcg_hash(SEED + (UNIFORMS.random_seed.x * 1000000.0) as u32);

        if p.position_life.w > 0.0 {
            // A live particle ages.
            p.position_life.w = p.position_life.w - dt;

            if p.position_life.w <= 0.0 {
                p.position_life.w = 0.0;
            } else {
                // The forces from the array. Gravity is one of them, with the emitter's scale.
                let mut vel = p.velocity_max_life.xyz;
                let num_forces = UNIFORMS.force_config.x as u32;
                let gravity_scale = EMITTER.emission_config.w;

                for i in 0u32..num_forces {
                    let force = FORCES[i];
                    if (force.type_strength.x as u32) == 0u32 {
                        let gravity_dir = force.direction_params.xyz * gravity_scale;
                        vel = vel + gravity_dir * dt;
                    } else {
                        vel = apply_force(force, p.position_life.xyz, vel, dt);
                    }
                }

                p.velocity_max_life = Vec4::new(vel.x, vel.y, vel.z, p.velocity_max_life.w);

                p.position_life = Vec4::new(
                    p.position_life.x + vel.x * dt,
                    p.position_life.y + vel.y * dt,
                    p.position_life.z + vel.z * dt,
                    p.position_life.w
                );

                // A three-colour gradient over the life: 1 is just born (start), 0.5 mid-life
                // (mid) and 0 dying (end).
                let life_ratio = p.position_life.w / p.velocity_max_life.w;
                if life_ratio > 0.5 {
                    let t = (life_ratio - 0.5) * 2.0;
                    p.color = mix(EMITTER.mid_color, EMITTER.start_color, t);
                } else {
                    let t = life_ratio * 2.0;
                    p.color = mix(EMITTER.end_color, EMITTER.mid_color, t);
                }

                let start_size = p.size_rotation.y;
                let end_size = p.size_rotation.z;
                p.size_rotation.x = mix(end_size, start_size, life_ratio);
            }
        } else {
            // A dead particle may spawn: the chance combines continuous emission and a burst.
            let emission_rate = EMITTER.emission_config.x;
            let burst_count = EMITTER.emission_config.y;

            let mut spawn_chance = 0.0;
            if emission_rate > 0.0 {
                spawn_chance = emission_rate * dt / (max_particles as f32);
            }
            if burst_count > 0.0 {
                spawn_chance = spawn_chance + burst_count / (max_particles as f32);
            }

            if random_float() < spawn_chance {
                let pos = get_spawn_position();
                let vel = get_spawn_velocity();
                let lifetime = random_range(EMITTER.lifetime_speed.x, EMITTER.lifetime_speed.y);
                let size = random_range(EMITTER.size_config.x, EMITTER.size_config.y);
                let end_size = random_range(EMITTER.size_config.z, EMITTER.size_config.w);

                p.position_life = Vec4::new(pos.x, pos.y, pos.z, lifetime);
                p.velocity_max_life = Vec4::new(vel.x, vel.y, vel.z, lifetime);
                p.color = EMITTER.start_color;
                p.size_rotation = Vec4::new(size, size, end_size, 0.0);
            }
        }

        PARTICLES[idx] = p;
    }
}
