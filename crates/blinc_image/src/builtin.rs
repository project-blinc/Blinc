//! Built-in [`ImageDecoder`] implementations, each gated behind its
//! matching cargo feature so apps that don't need a format don't pay its
//! compile cost.
//!
//! PNG and JPEG decode through the `png` and `zune-jpeg` crates directly,
//! with the transformations and conversion to RGBA the `image` crate
//! applies, so their pixels are the ones `image` gives. The other formats
//! go through `image`, which only links the code path of each format
//! enabled.
//!
//! [`ImageDecoder`]: crate::decoder::ImageDecoder

#![allow(dead_code)] // built-ins are conditionally compiled per-feature

use crate::decoder::{DecodedImage, ImageDecoder};
use crate::error::{ImageError, Result};
use crate::source::ImageFormat;

/// The largest decoded buffer, before the conversion to RGBA, that a decoder
/// allocates: the default allocation limit of `image`.
const MAX_DECODED_BYTES: u64 = 512 * 1024 * 1024;

/// Converts decoded samples with `channels` gray, gray-alpha, RGB or RGBA
/// channels to 8-bit RGBA. 16-bit samples are big-endian and round to the
/// nearest 8-bit value.
fn to_rgba8(samples: &[u8], channels: usize, sixteen: bool) -> Vec<u8> {
    let sample = |i: usize| -> u8 {
        if sixteen {
            let v = u16::from_be_bytes([samples[2 * i], samples[2 * i + 1]]);
            ((u32::from(v) + 128) / 257) as u8
        } else {
            samples[i]
        }
    };
    let pixels = samples.len() / channels / if sixteen { 2 } else { 1 };
    let mut out = Vec::with_capacity(pixels * 4);
    for p in 0..pixels {
        let at = p * channels;
        let px = match channels {
            1 => {
                let l = sample(at);
                [l, l, l, 255]
            }
            2 => {
                let l = sample(at);
                [l, l, l, sample(at + 1)]
            }
            3 => [sample(at), sample(at + 1), sample(at + 2), 255],
            _ => [sample(at), sample(at + 1), sample(at + 2), sample(at + 3)],
        };
        out.extend_from_slice(&px);
    }
    out
}

fn check_size(name: &str, width: u32, height: u32, bytes_per_pixel: u64) -> Result<()> {
    let total = u64::from(width) * u64::from(height) * bytes_per_pixel;
    if total > MAX_DECODED_BYTES {
        return Err(ImageError::Decode(format!(
            "{name} decode failed: {width}x{height} needs {total} bytes, over the limit of {MAX_DECODED_BYTES}"
        )));
    }
    Ok(())
}

/// Signatures in the order `image::guess_format` tries them, so a buffer
/// that matches two is detected as the first; formats Blinc has no decoder
/// for detect as `None`.
const SIGNATURES: [(&[u8], &[u8], Option<ImageFormat>); 22] = [
    (b"\x89PNG\r\n\x1a\n", b"", Some(ImageFormat::Png)),
    (&[0xff, 0xd8, 0xff], b"", Some(ImageFormat::Jpeg)),
    (b"GIF89a", b"", Some(ImageFormat::Gif)),
    (b"GIF87a", b"", Some(ImageFormat::Gif)),
    (
        b"RIFF\0\0\0\0WEBP",
        b"\xFF\xFF\xFF\xFF\0\0\0\0",
        Some(ImageFormat::WebP),
    ),
    (b"MM\x00*", b"", Some(ImageFormat::Tiff)),
    (b"II*\x00", b"", Some(ImageFormat::Tiff)),
    (b"DDS ", b"", None),
    (b"BM", b"", Some(ImageFormat::Bmp)),
    (&[0, 0, 1, 0], b"", None),
    (b"#?RADIANCE", b"", None),
    (
        b"\0\0\0\0ftypavif",
        b"\xFF\xFF\0\0",
        Some(ImageFormat::Avif),
    ),
    (&[0x76, 0x2f, 0x31, 0x01], b"", None),
    (b"qoif", b"", None),
    (b"P1", b"", None),
    (b"P2", b"", None),
    (b"P3", b"", None),
    (b"P4", b"", None),
    (b"P5", b"", None),
    (b"P6", b"", None),
    (b"P7", b"", None),
    (b"farbfeld", b"", None),
];

/// Detect a format from magic bytes. Used by callers that hold raw
/// bytes without a format hint; cheap and side-effect-free.
pub fn detect_format(bytes: &[u8]) -> Option<ImageFormat> {
    for (signature, mask, format) in SIGNATURES {
        let matched = if mask.is_empty() {
            bytes.starts_with(signature)
        } else {
            bytes.len() >= signature.len()
                && bytes
                    .iter()
                    .zip(signature)
                    .zip(mask.iter().chain(std::iter::repeat(&0xFF)))
                    .all(|((byte, sig), mask)| byte & mask == *sig)
        };
        if matched {
            return format;
        }
    }
    None
}

#[cfg(feature = "png")]
pub struct PngDecoder;
#[cfg(feature = "png")]
impl ImageDecoder for PngDecoder {
    fn formats(&self) -> &[ImageFormat] {
        &[ImageFormat::Png]
    }
    fn decode(&self, bytes: &[u8]) -> Result<DecodedImage> {
        let fail = |e: png::DecodingError| ImageError::Decode(format!("Png decode failed: {e}"));
        let mut decoder = png::Decoder::new_with_limits(
            std::io::Cursor::new(bytes),
            png::Limits {
                bytes: MAX_DECODED_BYTES as usize,
            },
        );
        // Palette and low bit depths to 8 bits, transparency to alpha; 16 bits stay 16.
        decoder.set_transformations(png::Transformations::EXPAND);
        let mut reader = decoder.read_info().map_err(fail)?;
        let (color, depth) = reader.output_color_type();
        let channels = match color {
            png::ColorType::Grayscale => 1,
            png::ColorType::GrayscaleAlpha => 2,
            png::ColorType::Rgb => 3,
            png::ColorType::Rgba => 4,
            png::ColorType::Indexed => {
                return Err(ImageError::Decode(
                    "Png decode failed: unexpanded palette".into(),
                ));
            }
        };
        let sixteen = match depth {
            png::BitDepth::Eight => false,
            png::BitDepth::Sixteen => true,
            other => {
                return Err(ImageError::Decode(format!(
                    "Png decode failed: unsupported bit depth {other:?}"
                )));
            }
        };
        let (width, height) = (reader.info().width, reader.info().height);
        check_size(
            "Png",
            width,
            height,
            channels as u64 * if sixteen { 2 } else { 1 },
        )?;
        let size = reader.output_buffer_size().ok_or_else(|| {
            ImageError::Decode("Png decode failed: image too large for this target".into())
        })?;
        let mut samples = vec![0; size];
        let frame = reader.next_frame(&mut samples).map_err(fail)?;
        samples.truncate(frame.buffer_size());
        Ok(DecodedImage {
            pixels: to_rgba8(&samples, channels, sixteen),
            width,
            height,
        })
    }
}

#[cfg(feature = "jpeg")]
pub struct JpegDecoder;
#[cfg(feature = "jpeg")]
impl ImageDecoder for JpegDecoder {
    fn formats(&self) -> &[ImageFormat] {
        &[ImageFormat::Jpeg]
    }
    fn decode(&self, bytes: &[u8]) -> Result<DecodedImage> {
        use zune_core::colorspace::ColorSpace;
        use zune_core::options::DecoderOptions;
        let fail = |e: zune_jpeg::errors::DecodeErrors| {
            ImageError::Decode(format!("Jpeg decode failed: {e:?}"))
        };
        let options = DecoderOptions::default()
            .set_strict_mode(false)
            .set_max_width(usize::MAX)
            .set_max_height(usize::MAX);
        let mut decoder = zune_jpeg::JpegDecoder::new_with_options(
            zune_core::bytestream::ZCursor::new(bytes),
            options,
        );
        decoder.decode_headers().map_err(fail)?;
        let (width, height) = decoder.dimensions().expect("headers were decoded");
        let (width, height) = (width as u32, height as u32);
        // Gray and RGB, with or without alpha, as they are; any other colour space as RGB.
        let out = match decoder.input_colorspace().expect("headers were decoded") {
            space @ (ColorSpace::RGB | ColorSpace::RGBA | ColorSpace::Luma | ColorSpace::LumaA) => {
                space
            }
            _ => ColorSpace::RGB,
        };
        let channels = out.num_components();
        check_size("Jpeg", width, height, channels as u64)?;
        decoder.set_options(options.jpeg_set_out_colorspace(out));
        let samples = decoder.decode().map_err(fail)?;
        Ok(DecodedImage {
            pixels: to_rgba8(&samples, channels, false),
            width,
            height,
        })
    }
}

#[cfg(any(
    feature = "gif",
    feature = "webp",
    feature = "bmp",
    feature = "tiff",
    feature = "avif"
))]
fn decode_with_image(bytes: &[u8], format: image::ImageFormat) -> Result<DecodedImage> {
    use image::GenericImageView;
    let img = image::load_from_memory_with_format(bytes, format)
        .map_err(|e| ImageError::Decode(format!("{format:?} decode failed: {e}")))?;
    let (width, height) = img.dimensions();
    Ok(DecodedImage {
        pixels: img.to_rgba8().into_raw(),
        width,
        height,
    })
}

#[cfg(feature = "gif")]
pub struct GifDecoder;
#[cfg(feature = "gif")]
impl ImageDecoder for GifDecoder {
    fn formats(&self) -> &[ImageFormat] {
        &[ImageFormat::Gif]
    }
    fn decode(&self, bytes: &[u8]) -> Result<DecodedImage> {
        decode_with_image(bytes, image::ImageFormat::Gif)
    }
}

#[cfg(feature = "webp")]
pub struct WebPDecoder;
#[cfg(feature = "webp")]
impl ImageDecoder for WebPDecoder {
    fn formats(&self) -> &[ImageFormat] {
        &[ImageFormat::WebP]
    }
    fn decode(&self, bytes: &[u8]) -> Result<DecodedImage> {
        decode_with_image(bytes, image::ImageFormat::WebP)
    }
}

#[cfg(feature = "bmp")]
pub struct BmpDecoder;
#[cfg(feature = "bmp")]
impl ImageDecoder for BmpDecoder {
    fn formats(&self) -> &[ImageFormat] {
        &[ImageFormat::Bmp]
    }
    fn decode(&self, bytes: &[u8]) -> Result<DecodedImage> {
        decode_with_image(bytes, image::ImageFormat::Bmp)
    }
}

#[cfg(feature = "tiff")]
pub struct TiffDecoder;
#[cfg(feature = "tiff")]
impl ImageDecoder for TiffDecoder {
    fn formats(&self) -> &[ImageFormat] {
        &[ImageFormat::Tiff]
    }
    fn decode(&self, bytes: &[u8]) -> Result<DecodedImage> {
        decode_with_image(bytes, image::ImageFormat::Tiff)
    }
}

#[cfg(feature = "avif")]
pub struct AvifDecoder;
#[cfg(feature = "avif")]
impl ImageDecoder for AvifDecoder {
    fn formats(&self) -> &[ImageFormat] {
        &[ImageFormat::Avif]
    }
    fn decode(&self, bytes: &[u8]) -> Result<DecodedImage> {
        decode_with_image(bytes, image::ImageFormat::Avif)
    }
}

#[cfg(test)]
#[path = "builtin_tests.rs"]
mod tests;
