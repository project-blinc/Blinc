use blinc_text::{FontFace, TextShaper};

#[test]
fn migration_preserves_shaping_of_bundled_multiscript_and_variable_faces() {
    let shaper = TextShaper::new();
    for line in include_str!("fixtures/shaping-baseline.tsv").lines() {
        let parts: Vec<_> = line.split('\t').collect();
        let bytes: &[u8] = match parts[0] {
            "latin" => include_bytes!(
                "../../../examples/blinc_component_demos/assets/NotoSans-Regular.ttf"
            ),
            "arabic" => include_bytes!("fixtures/notosansarabic/font.ttf"),
            "devanagari" => include_bytes!("fixtures/notosansdevanagari/font.ttf"),
            _ => panic!("unknown font fixture"),
        };
        let mut font = FontFace::from_data(bytes.to_vec()).unwrap();
        font.set_variation_weight(parts[1].parse().unwrap());
        let text = parts[3];
        assert!(text.chars().all(|c| c.is_whitespace() || font.has_glyph(c)));
        for _ in 0..2 {
            let shaped = if parts[2] == "default" {
                shaper.shape(text, &font, 16.0)
            } else {
                shaper.shape_with_features(text, &font, 16.0, &[parts[2].parse().unwrap()])
            };
            let actual = shaped
                .glyphs
                .iter()
                .map(|g| {
                    format!(
                        "{}:{}:{}:{}:{}:{}",
                        g.glyph_id, g.x_advance, g.y_advance, g.x_offset, g.y_offset, g.cluster
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            assert_eq!(
                actual, parts[4],
                "{} weight {} {}",
                parts[0], parts[1], text
            );
            assert!(
                shaped
                    .glyphs
                    .iter()
                    .all(|g| text.is_char_boundary(g.cluster as usize))
            );
        }
    }
}
