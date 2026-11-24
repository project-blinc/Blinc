//! The built-in decoders give the pixels the `image` crate gives, and detect
//! formats as `image::guess_format` does.

use crate::builtin::{JpegDecoder, PngDecoder, detect_format};
use crate::decoder::ImageDecoder;
use crate::source::ImageFormat;

/// Deterministic bytes for fixture pixels.
fn noise(len: usize, seed: u32) -> Vec<u8> {
    let mut x = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            (x >> 24) as u8
        })
        .collect()
}

fn encode_png(
    width: u32,
    height: u32,
    color: png::ColorType,
    depth: png::BitDepth,
    palette: Option<&[u8]>,
    trns: Option<&[u8]>,
    seed: u32,
) -> Vec<u8> {
    let bits = depth as usize;
    let samples = match color {
        png::ColorType::Grayscale | png::ColorType::Indexed => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
    };
    let row = (width as usize * samples * bits).div_ceil(8);
    let mut data = noise(row * height as usize, seed);
    if let (png::ColorType::Indexed, Some(palette)) = (color, palette) {
        // Keep indices inside the palette.
        let entries = (palette.len() / 3) as u8;
        let per_byte = 8 / bits;
        let mask = ((1u16 << bits) - 1) as u8;
        for byte in &mut data {
            let mut out = 0u8;
            for k in 0..per_byte {
                let v = ((*byte >> (k * bits)) & mask) % entries;
                out |= v << (k * bits);
            }
            *byte = out;
        }
    }
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(color);
        encoder.set_depth(depth);
        if let Some(palette) = palette {
            encoder.set_palette(palette.to_vec());
        }
        if let Some(trns) = trns {
            encoder.set_trns(trns.to_vec());
        }
        let mut writer = encoder.write_header().expect("header");
        writer.write_image_data(&data).expect("data");
    }
    bytes
}

fn with_image(bytes: &[u8], format: image::ImageFormat) -> (u32, u32, Vec<u8>) {
    let img = image::load_from_memory_with_format(bytes, format).expect("image decodes");
    let rgba = img.to_rgba8();
    (rgba.width(), rgba.height(), rgba.into_raw())
}

fn assert_same(name: &str, decoder: &dyn ImageDecoder, bytes: &[u8], format: image::ImageFormat) {
    let ours = decoder
        .decode(bytes)
        .unwrap_or_else(|e| panic!("{name}: {e}"));
    let (w, h, theirs) = with_image(bytes, format);
    assert_eq!((ours.width, ours.height), (w, h), "{name}: size");
    assert!(ours.pixels == theirs, "{name}: pixels differ");
}

#[test]
fn png_pixels_match_image_for_every_color_type_and_depth() {
    use png::BitDepth::*;
    use png::ColorType::*;
    let palette: Vec<u8> = noise(16 * 3, 7);
    let cases: Vec<(
        &str,
        png::ColorType,
        png::BitDepth,
        Option<&[u8]>,
        Option<Vec<u8>>,
    )> = vec![
        ("gray1", Grayscale, One, None, None),
        ("gray2", Grayscale, Two, None, None),
        ("gray4", Grayscale, Four, None, None),
        ("gray8", Grayscale, Eight, None, None),
        ("gray16", Grayscale, Sixteen, None, None),
        ("gray8 trns", Grayscale, Eight, None, Some(vec![0, 77])),
        (
            "gray16 trns",
            Grayscale,
            Sixteen,
            None,
            Some(vec![0x12, 0x34]),
        ),
        ("gray-alpha8", GrayscaleAlpha, Eight, None, None),
        ("gray-alpha16", GrayscaleAlpha, Sixteen, None, None),
        ("rgb8", Rgb, Eight, None, None),
        ("rgb16", Rgb, Sixteen, None, None),
        (
            "rgb8 trns",
            Rgb,
            Eight,
            None,
            Some(vec![0, 10, 0, 20, 0, 30]),
        ),
        ("rgba8", Rgba, Eight, None, None),
        ("rgba16", Rgba, Sixteen, None, None),
        ("palette1", Indexed, One, Some(&palette[..6]), None),
        ("palette2", Indexed, Two, Some(&palette[..12]), None),
        ("palette4", Indexed, Four, Some(&palette[..48]), None),
        ("palette8", Indexed, Eight, Some(&palette[..48]), None),
        (
            "palette8 trns",
            Indexed,
            Eight,
            Some(&palette[..48]),
            Some(noise(9, 3)),
        ),
    ];
    for (i, (name, color, depth, palette, trns)) in cases.into_iter().enumerate() {
        // An odd width exercises the padding at the end of each row.
        let bytes = encode_png(13, 7, color, depth, palette, trns.as_deref(), i as u32);
        assert_same(name, &PngDecoder, &bytes, image::ImageFormat::Png);
    }
}

#[test]
fn jpeg_pixels_match_image() {
    for (i, (color, channels)) in [
        (image::ExtendedColorType::Rgb8, 3),
        (image::ExtendedColorType::L8, 1),
    ]
    .into_iter()
    .enumerate()
    {
        for quality in [40, 90] {
            let (w, h) = (37u32, 29u32);
            let raw = noise((w * h) as usize * channels, i as u32 + quality as u32);
            let mut bytes = Vec::new();
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, quality)
                .encode(&raw, w, h, color)
                .expect("jpeg encodes");
            let name = format!("{color:?} q{quality}");
            assert_same(&name, &JpegDecoder, &bytes, image::ImageFormat::Jpeg);
        }
    }
}

/// Every PNG and JPEG under the repository's crates and examples up to 4 MB: real encoders,
/// palettes, colour profiles and progressive JPEG.
#[test]
fn repository_images_match_image() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut stack = vec![root.join("crates"), root.join("examples")];
    let mut seen = 0;
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if path.is_dir() {
                if name != "target" && !name.starts_with('.') {
                    stack.push(path);
                }
                continue;
            }
            let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
            let (decoder, format): (&dyn ImageDecoder, _) = match ext.as_str() {
                "png" => (&PngDecoder, image::ImageFormat::Png),
                "jpg" | "jpeg" => (&JpegDecoder, image::ImageFormat::Jpeg),
                _ => continue,
            };
            if entry.metadata().map_or(true, |m| m.len() > 4 << 20) {
                continue;
            }
            let bytes = std::fs::read(&path).expect("read");
            if image::load_from_memory_with_format(&bytes, format).is_err() {
                assert!(decoder.decode(&bytes).is_err(), "{name}: image rejects it");
                continue;
            }
            assert_same(&path.display().to_string(), decoder, &bytes, format);
            seen += 1;
        }
    }
    // A clean checkout has the tracked images only; a local tree adds ignored ones.
    assert!(seen >= 20, "only {seen} images found");
}

#[test]
fn formats_detect_as_image_guesses_them() {
    let mut cases: Vec<Vec<u8>> = vec![
        b"\x89PNG\r\n\x1a\n rest".to_vec(),
        vec![0xff, 0xd8, 0xff, 0xe0],
        b"GIF89a....".to_vec(),
        b"GIF87a....".to_vec(),
        b"RIFF\x10\x00\x00\x00WEBPVP8 ".to_vec(),
        b"MM\x00*....".to_vec(),
        b"II*\x00....".to_vec(),
        b"BM......".to_vec(),
        b"DDS .....".to_vec(),
        b"\x00\x00\x00\x1cftypavif....".to_vec(),
        // Also an ICO header: the first match wins, as in `image`.
        b"\x00\x00\x01\x00ftypavif....".to_vec(),
        b"qoif....".to_vec(),
        b"P6\n".to_vec(),
        b"RIFF".to_vec(),
        Vec::new(),
    ];
    cases.extend((0..64).map(|i| noise(16, i)));
    let map = |f: image::ImageFormat| match f {
        image::ImageFormat::Png => Some(ImageFormat::Png),
        image::ImageFormat::Jpeg => Some(ImageFormat::Jpeg),
        image::ImageFormat::Gif => Some(ImageFormat::Gif),
        image::ImageFormat::WebP => Some(ImageFormat::WebP),
        image::ImageFormat::Bmp => Some(ImageFormat::Bmp),
        image::ImageFormat::Tiff => Some(ImageFormat::Tiff),
        image::ImageFormat::Avif => Some(ImageFormat::Avif),
        _ => None,
    };
    for bytes in cases {
        let theirs = image::guess_format(&bytes).ok().and_then(map);
        assert_eq!(detect_format(&bytes), theirs, "{bytes:?}");
    }
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in bytes {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

#[test]
fn an_image_over_the_size_limit_fails_before_it_is_allocated() {
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&40_000u32.to_be_bytes());
    ihdr.extend_from_slice(&40_000u32.to_be_bytes());
    ihdr.extend_from_slice(&[16, 6, 0, 0, 0]);
    let chunk = |kind: &[u8], data: &[u8]| {
        let mut c = (data.len() as u32).to_be_bytes().to_vec();
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        c.extend_from_slice(&body);
        c.extend_from_slice(&crc32(&body).to_be_bytes());
        c
    };
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    bytes.extend(chunk(b"IHDR", &ihdr));
    bytes.extend(chunk(b"IDAT", &[0x78, 0x01]));
    bytes.extend(chunk(b"IEND", &[]));
    assert!(PngDecoder.decode(&bytes).is_err());
    assert!(image::load_from_memory_with_format(&bytes, image::ImageFormat::Png).is_err());
}
