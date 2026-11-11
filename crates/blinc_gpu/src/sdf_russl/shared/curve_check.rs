//! Evaluates descriptors on the GPU for the parity test against the shared vectors: one thread
//! for each case, writing the value and the velocity.

russl::russl! {
    use super::curve::eval_descriptor;

    struct Case {
        // from, to, duration_ms, initial velocity
        timing: Vec4,
        // curve parameters
        params: Vec4,
        // kind, n, jump, elapsed_ms
        shape: Vec4,
    }

    #[group(0)] #[binding(0)] static CASES: Storage<[Case]>;
    #[group(0)] #[binding(1)] static OUT: StorageMut<[Vec4]>;
    #[group(0)] #[binding(2)] static COUNT: Uniform<Vec4>;

    #[compute(workgroup_size(64))]
    fn cs_main(#[builtin(global_invocation_id)] global_id: UVec3) {
        let i = global_id.x;
        if i >= (COUNT.x as u32) {
            return;
        }
        let c = CASES[i];
        let r = eval_descriptor(
            c.timing.x, c.timing.y, c.timing.z, c.timing.w,
            c.shape.x as u32, c.params, c.shape.y as u32, c.shape.z as u32, c.shape.w
        );
        OUT[i] = Vec4::new(r.x, r.y, 0.0, 0.0);
    }
}
