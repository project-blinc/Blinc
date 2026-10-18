//! What a layer's blur costs by deviation: the exact Gaussian a display list asks
//! for, against the Kawase approximation Blinc's own layer effects use.
//!
//! This measures, it does not assert. Run it with `--ignored --nocapture`.
//! Times are wall clock on one layer of 1920 by 1080: a batch of calls and then
//! a wait on the GPU, divided by the batch, so the wait is shared. They carry the
//! machine's own overhead and are for comparing the two against each other.
//! The Gaussian's time includes its composite over a target, which the Kawase
//! time does not.

#[cfg(test)]
mod tests {
    use crate::harness::TestHarness;
    use blinc_gpu::LayerTexture;
    use blinc_gpu::display_list::effects::{LayerEffects, LayerFx};
    use std::time::Instant;

    const SIZE: (u32, u32) = (1920, 1080);
    const RUNS: usize = 15;
    const BATCH: usize = 10;

    fn median_ms(mut times: Vec<f64>) -> f64 {
        times.sort_by(|a, b| a.partial_cmp(b).unwrap());
        times[times.len() / 2]
    }

    #[test]
    #[ignore] // Requires GPU
    fn blur_cost_by_deviation() {
        let harness = TestHarness::new().unwrap();
        let path = harness.output_path("blur_cost");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut table = Vec::new();
        harness
            .render_with(SIZE.0, SIZE.1, &path, |renderer, target| {
                let (device, queue) = (renderer.device_arc(), renderer.queue_arc());
                let format = wgpu::TextureFormat::Rgba8Unorm;
                let content = LayerTexture::new(&device, SIZE, format, false);
                let rows = LayerTexture::new(&device, SIZE, format, false);
                let shadow = LayerTexture::new(&device, SIZE, format, false);
                let effects = LayerEffects::new(&device, format);
                let wait = |device: &wgpu::Device| {
                    let _ = device.poll(wgpu::PollType::Wait);
                };

                for sigma in [1.0f32, 2.0, 4.0, 8.0, 16.0, 32.0] {
                    let fx = LayerFx {
                        blur: sigma,
                        ..LayerFx::fade(1.0)
                    };
                    let gaussian = (0..RUNS + 3)
                        .map(|_| {
                            let start = Instant::now();
                            for _ in 0..BATCH {
                                effects.composite(
                                    &device,
                                    &queue,
                                    &content.view,
                                    [&rows.view, &shadow.view],
                                    target,
                                    &fx,
                                );
                            }
                            wait(&device);
                            start.elapsed().as_secs_f64() * 1000.0 / BATCH as f64
                        })
                        .skip(3)
                        .collect::<Vec<_>>();

                    let passes = ((sigma / 2.0).ceil().max(2.0) as u32).min(8);
                    let kawase = (0..RUNS + 3)
                        .map(|_| {
                            let start = Instant::now();
                            let blurred: Vec<_> = (0..BATCH)
                                .map(|_| renderer.apply_blur(&content, sigma, passes))
                                .collect();
                            wait(&device);
                            let elapsed = start.elapsed().as_secs_f64() * 1000.0 / BATCH as f64;
                            blurred
                                .into_iter()
                                .for_each(|t| renderer.release_layer_texture(t));
                            elapsed
                        })
                        .skip(3)
                        .collect::<Vec<_>>();
                    table.push((sigma, median_ms(gaussian), passes, median_ms(kawase)));
                }
            })
            .unwrap();

        println!(
            "blur of one {}x{} layer, per call, median of {RUNS} batches of {BATCH}",
            SIZE.0, SIZE.1
        );
        println!("sigma | exact gaussian (ms) | kawase passes | kawase (ms)");
        for (sigma, gaussian, passes, kawase) in table {
            println!("{sigma:>5} | {gaussian:>19.3} | {passes:>13} | {kawase:>11.3}");
        }
    }
}
