//! What a layer's blur costs by deviation: the Gaussian a display list asks for,
//! against the Kawase approximation Blinc's own layer effects use.
//!
//! This measures, it does not assert. Run it with `--ignored --nocapture`.
//! Times are wall clock: a batch of calls and then a wait on the GPU, divided by
//! the batch, so the wait is shared. They carry the machine's own overhead and
//! are for comparing the two against each other. The Gaussian's time includes its
//! composite over a target, which the Kawase time does not.
//!
//! Two layers are measured. The first is the whole of a 1920 by 1080 target. The
//! second is an element of 400 by 300 on that target, its content in a texture of
//! just that size, as layers are drawn: the Gaussian's passes are confined to its
//! region, and Kawase blurs the same texture.

#[cfg(test)]
mod tests {
    use crate::harness::TestHarness;
    use blinc_gpu::GpuPrimitive;
    use blinc_gpu::LayerTexture;
    use blinc_gpu::display_list::effects::{LayerEffects, LayerFrame, LayerFx};
    use blinc_gpu::display_list::plan::{Item, Layer, Painter, Plan};
    use std::time::Instant;

    const SIZE: (u32, u32) = (1920, 1080);
    const ELEMENT: (u32, u32) = (400, 300);
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
                let tight = LayerTexture::new(&device, ELEMENT, format, false);
                let mut effects = LayerEffects::new(&device, format);
                let wait = |device: &wgpu::Device| {
                    let _ = device.poll(wgpu::PollType::Wait);
                };
                let whole = [0, 0, SIZE.0, SIZE.1];
                let element = [760, 390, ELEMENT.0, ELEMENT.1];

                for sigma in [1.0f32, 2.0, 4.0, 8.0, 16.0, 32.0] {
                    let fx = LayerFx {
                        blur: sigma,
                        ..LayerFx::fade(1.0)
                    };
                    let mut gaussian = |texture: &LayerTexture, region: [u32; 4]| {
                        median_ms(
                            (0..RUNS + 3)
                                .map(|_| {
                                    let start = Instant::now();
                                    for _ in 0..BATCH {
                                        let frame = LayerFrame {
                                            size: texture.size,
                                            origin: [region[0], region[1]],
                                            region,
                                        };
                                        effects.composite(
                                            &device,
                                            &queue,
                                            &texture.view,
                                            target,
                                            frame,
                                            &fx,
                                        );
                                    }
                                    wait(&device);
                                    start.elapsed().as_secs_f64() * 1000.0 / BATCH as f64
                                })
                                .skip(3)
                                .collect(),
                        )
                    };
                    let gaussian_whole = gaussian(&content, whole);
                    let gaussian_element = gaussian(&tight, element);

                    let passes = ((sigma / 2.0).ceil().max(2.0) as u32).min(8);
                    let mut kawase = |texture: &LayerTexture| {
                        median_ms(
                            (0..RUNS + 3)
                                .map(|_| {
                                    let start = Instant::now();
                                    let blurred: Vec<_> = (0..BATCH)
                                        .map(|_| renderer.apply_blur(texture, sigma, passes))
                                        .collect();
                                    wait(&device);
                                    let elapsed =
                                        start.elapsed().as_secs_f64() * 1000.0 / BATCH as f64;
                                    blurred
                                        .into_iter()
                                        .for_each(|t| renderer.release_layer_texture(t));
                                    elapsed
                                })
                                .skip(3)
                                .collect(),
                        )
                    };
                    let kawase_whole = kawase(&content);
                    let kawase_element = kawase(&tight);
                    table.push((
                        sigma,
                        gaussian_whole,
                        kawase_whole,
                        gaussian_element,
                        kawase_element,
                    ));
                }
            })
            .unwrap();

        println!("blur per call, median of {RUNS} batches of {BATCH}, in ms");
        println!(
            "      | whole {}x{}          | element {}x{}",
            SIZE.0, SIZE.1, ELEMENT.0, ELEMENT.1
        );
        println!("sigma | gaussian | kawase   | gaussian | kawase");
        for (sigma, gw, kw, ge, ke) in table {
            println!("{sigma:>5} | {gw:>8.3} | {kw:>8.3} | {ge:>8.3} | {ke:>6.3}");
        }
    }

    /// A whole frame of one box in a layer, drawn through the executor, with the
    /// layer's bounds the whole target and then just the element.
    #[test]
    #[ignore] // Requires GPU
    fn layer_cost_by_region() {
        let harness = TestHarness::new().unwrap();
        let path = harness.output_path("layer_cost");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut rows = Vec::new();
        harness
            .render_with(SIZE.0, SIZE.1, &path, |renderer, target| {
                let device = renderer.device_arc();
                let mut painter = Painter::default();
                let element = GpuPrimitive {
                    bounds: [760.0, 390.0, 400.0, 300.0],
                    color: [0.9, 0.1, 0.1, 1.0],
                    ..GpuPrimitive::rect(760.0, 390.0, 400.0, 300.0)
                };
                for (what, blur) in [("fade", 0.0f32), ("blur 4", 4.0), ("blur 16", 16.0)] {
                    for (where_, bounds) in [
                        ("whole target", [0.0, 0.0, SIZE.0 as f32, SIZE.1 as f32]),
                        ("element", [760.0, 390.0, 400.0, 300.0]),
                    ] {
                        let plan = Plan {
                            items: vec![Item::Layer(Layer {
                                items: vec![Item::Primitives(vec![element])],
                                fx: LayerFx {
                                    blur,
                                    ..LayerFx::fade(0.5)
                                },
                                bounds,
                            })],
                            unsupported: Vec::new(),
                        };
                        let frames = (0..RUNS + 3)
                            .map(|_| {
                                let start = Instant::now();
                                for _ in 0..BATCH {
                                    painter.render(
                                        renderer,
                                        target,
                                        SIZE,
                                        &plan,
                                        [1.0; 4],
                                        &|_| None,
                                    );
                                }
                                let _ = device.poll(wgpu::PollType::Wait);
                                start.elapsed().as_secs_f64() * 1000.0 / BATCH as f64
                            })
                            .skip(3)
                            .collect::<Vec<_>>();
                        rows.push((what, where_, median_ms(frames)));
                    }
                }
            })
            .unwrap();
        println!("one layer, a whole frame through the executor, ms per frame");
        for (what, where_, ms) in rows {
            println!("{what:>8} | {where_:>12} | {ms:>6.3}");
        }
    }
}
