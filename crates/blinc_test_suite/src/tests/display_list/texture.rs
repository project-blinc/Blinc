//! Native 3D primitives also use the complete primitive texture row.
use crate::harness::{TestHarness, TestHarnessConfig};
use blinc_gpu::{GpuPrimitive, PrimitiveBatch};

#[test]
#[ignore] // Requires GPU
fn native_3d_and_core_primitives_read_separate_texture_rows() {
    let harness = TestHarness::with_config(TestHarnessConfig {
        sample_count: 1,
        ..Default::default()
    })
    .unwrap();
    let colors = [
        [0.8, 0.2, 0.4, 1.0],
        [0.2, 0.8, 0.4, 1.0],
        [0.2, 0.4, 0.8, 1.0],
    ];
    let mut batch = PrimitiveBatch::new();
    for (i, color) in colors.iter().copied().enumerate() {
        let mut p = GpuPrimitive::rect(12.0 + i as f32 * 62.0, 22.0, 40.0, 48.0);
        p.color = color;
        if i > 0 {
            // Ambient light fixes interior colour independently of the surface normal.
            p = p
                .with_3d_shape(i as f32, 40.0)
                .with_3d_rotation(0.1, 0.15, 800.0)
                .with_3d_material(1.0, 256.0)
                .with_light([1.0, 0.0, 0.0], 0.0);
        }
        batch.primitives.push(p);
    }
    let path = harness.output_path("primitive_texture_native_3d");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    harness.render_to_png(&batch, 200, 100, &path).unwrap();
    let page = image::open(path).unwrap().to_rgba8();
    assert_eq!(page.get_pixel(0, 0).0, [255; 4]);
    for (i, color) in colors.iter().enumerate() {
        let pixel = page.get_pixel(32 + i as u32 * 62, 46);
        for (ch, expected) in color.iter().enumerate() {
            assert!(
                (pixel[ch] as i32 - (expected * 255.0_f32).round() as i32).abs() <= 1,
                "primitive {i}, channel {ch}: {}",
                pixel[ch]
            );
        }
    }
}
