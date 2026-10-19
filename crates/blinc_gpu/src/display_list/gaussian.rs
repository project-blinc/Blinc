//! Gaussian tap pairs shared by layer effects and backdrops.

/// The pairs of a Gaussian's taps: where each fetch falls from the pixel, and
/// its weight, the weights summing to one. Each fetch lies between two texels
/// and stands for both, so the pairs are the Gaussian over whole texels.
pub(super) fn taps(deviation: f32) -> Vec<[f32; 2]> {
    let sigma = deviation.max(1e-4);
    let reach = (sigma * 3.0).ceil();
    let mut pairs = Vec::new();
    let mut i = -reach;
    while i <= reach {
        let w0 = (-(i * i) / (2.0 * sigma * sigma)).exp();
        let w1 = (-((i + 1.0) * (i + 1.0)) / (2.0 * sigma * sigma)).exp();
        let w = w0 + w1;
        pairs.push([i + w1 / w.max(1e-30), w]);
        i += 2.0;
    }
    let total: f32 = pairs.iter().map(|p| p[1]).sum();
    pairs.iter().map(|p| [p[0], p[1] / total]).collect()
}
