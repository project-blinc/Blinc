use blinc_text::{
    font::FontFace,
    rasterizer::{GlyphFormat, GlyphRasterizer, RasterizedGlyph},
};

fn glyph(ch: char) -> RasterizedGlyph {
    let font = FontFace::from_data(include_bytes!("fixtures/colrv1.ttf").to_vec()).unwrap();
    GlyphRasterizer::new()
        .rasterize_color(&font, font.glyph_id(ch).unwrap(), 100.0)
        .unwrap()
}

fn pixel(image: &RasterizedGlyph, x: f32, y: f32) -> [u8; 4] {
    let x = (x / 10.0 - image.bearing_x as f32).floor() as i32;
    let y = (image.bearing_y as f32 - y / 10.0).floor() as i32;
    if x < 0 || y < 0 || x >= image.width as i32 || y >= image.height as i32 {
        return [0; 4];
    }
    image.bitmap[((y as u32 * image.width + x as u32) * 4) as usize..][..4]
        .try_into()
        .unwrap()
}

#[test]
fn colrv1_solid_and_legacy_layers_keep_rgba_and_metrics() {
    let solid = glyph('A');
    assert_eq!(solid.format, GlyphFormat::Rgba);
    assert_eq!(pixel(&solid, 500.0, 500.0), [255, 0, 0, 255]);
    assert_eq!(solid.advance, 100);
    let legacy = pixel(&glyph('H'), 500.0, 500.0);
    assert!(legacy[0] >= 250 && legacy[3] >= 250);
    assert_eq!(&legacy[1..3], &[0, 0]);
}

#[test]
fn colrv1_gradients_follow_font_coordinates() {
    let linear = glyph('B');
    assert!(pixel(&linear, 200.0, 500.0)[0] > 200);
    assert!(pixel(&linear, 800.0, 500.0)[2] > 200);
    let radial = glyph('C');
    assert!(pixel(&radial, 500.0, 500.0)[0] > 240);
    assert!(pixel(&radial, 800.0, 500.0)[2] > 180);
    let sweep = glyph('D');
    assert!(pixel(&sweep, 800.0, 510.0)[0] > 240);
    assert!(pixel(&sweep, 500.0, 200.0)[2] > 180);
}

#[test]
fn colrv1_transforms_clips_and_source_in_preserve_alpha() {
    let transformed = glyph('E');
    assert_eq!(pixel(&transformed, 300.0, 300.0), [255, 0, 0, 255]);
    assert_eq!(pixel(&transformed, 700.0, 300.0), [0; 4]);
    let composite = glyph('F');
    let center = pixel(&composite, 300.0, 500.0);
    assert_eq!(&center[..3], &[0, 0, 255]);
    assert!((center[3] as i32 - 128).abs() <= 1);
    assert_eq!(pixel(&composite, 700.0, 500.0), [0; 4]);
    let clipped = glyph('G');
    assert_eq!(pixel(&clipped, 300.0, 500.0), [0, 0, 255, 255]);
    assert_eq!(pixel(&clipped, 700.0, 500.0), [0; 4]);
}
