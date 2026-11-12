//! What the particle shaders share: the particle as the simulation updates it and the render pass
//! draws it.

russl::russl! {
    pub struct Particle {
        // xyz position, w life remaining
        pub position_life: Vec4,
        // xyz velocity, w max lifetime
        pub velocity_max_life: Vec4,
        // rgba
        pub color: Vec4,
        // current, start, end, rotation
        pub size_rotation: Vec4,
    }
}
