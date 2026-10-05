//! Runtime BC1 / BC3 encoding for the 2D image path.
//!
//! Compiled only when the `bc-encode` cargo feature is enabled.
//! Produces [`TextureData`] values whose `format` is a BC variant,
//! ready for [`crate::image::GpuImage::from_compressed`].
//!
//! Mirrors the 3D pipeline's approach in `blinc_gltf::bc_encode`
//! without the mesh-specific slots (no BC4 occlusion, no BC5
//! normal). The 2D image widget cache only hands us plain RGBA
//! buffers — diffuse-style color data — so the decision tree
//! collapses to:
//!
//! - **effectively opaque** (every alpha ≥ 244) → BC1 (4 bpp)
//! - **has meaningful alpha** → BC3 (8 bpp)
//!
//! Encode cost: ~50-150 ms per 2K × 2K RGBA at load, same ballpark
//! as the mesh-texture pipeline. The caller decides where that
//! cost lives — the current integration runs inline in
//! `BlincContext::preload_images`, which is off the paint path.

use blinc_core::draw::{TextureData, TexturePixelFormat};

/// Cutoff for "effectively opaque" — any pixel with alpha below
/// this uses BC3. 244 / 255 ≈ 0.957, matching the same heuristic
/// the mesh material path uses for auto-demoting BLEND → MASK:
/// a handful of stray near-opaque pixels from PNG antialiasing
/// shouldn't force the whole texture into the 8-bpp BC3 tier.
const OPAQUE_ALPHA_CUTOFF: u8 = 244;

/// Zero-copy reinterpret a packed `[r, g, b, a, ...]` u8 slice
/// as `&[tbc::color::Rgba8]`.
///
/// # Safety
/// `tbc::color::Rgba8` is `#[repr(C)]` with four `u8` fields, so
/// its layout is identical to `[u8; 4]`. Caller guarantees the
/// input length is a multiple of 4 via the `debug_assert` in each
/// public function.
#[inline]
fn rgba8_view(pixels: &[u8]) -> &[tbc::color::Rgba8] {
    debug_assert_eq!(pixels.len() % 4, 0);
    unsafe {
        std::slice::from_raw_parts(
            pixels.as_ptr() as *const tbc::color::Rgba8,
            pixels.len() / 4,
        )
    }
}

// ---------------------------------------------------------------------------
// BC4-style blocks: choosing each pixel's index
// ---------------------------------------------------------------------------
//
// BC4 stores two 8-bit endpoints and a 3-bit index per pixel. BC3's alpha
// block and each half of a BC5 block are the same thing. Given the
// endpoints, the best index for a pixel is the nearest of the eight values
// the endpoints can produce, so an encoder cannot do better than that and
// should not do worse.
//
// tbc 0.3 picks good endpoints but poor indices: against the nearest-entry
// choice for the SAME endpoints it measured 26 dB worse on a gradient, for
// BC4 and for BC3 alpha alike. Here the endpoints are kept and each pixel's
// index is replaced with the nearest palette entry. The raw-tbc canaries in
// `tests/bc4_quality.rs` fail while that is true; when they pass, tbc has
// been fixed and this correction can go.

/// The eight values a BC4 block can reproduce, from its endpoints.
///
/// `a0 > a1` selects six interpolated steps; otherwise four, plus an
/// explicit 0 and 255. Rounded to nearest, as hardware decodes.
fn bc4_palette(a0: u8, a1: u8) -> [u8; 8] {
    let (f0, f1) = (u32::from(a0), u32::from(a1));
    let mut p = [a0, a1, 0, 0, 0, 0, 0, 0];
    if a0 > a1 {
        for k in 1..=6u32 {
            p[1 + k as usize] = (((7 - k) * f0 + k * f1 + 3) / 7) as u8;
        }
    } else {
        for k in 1..=4u32 {
            p[1 + k as usize] = (((5 - k) * f0 + k * f1 + 2) / 5) as u8;
        }
        p[6] = 0;
        p[7] = 255;
    }
    p
}

/// Index of the palette entry nearest `v`; the lowest index on a tie.
fn nearest_index(palette: &[u8; 8], v: u8) -> u8 {
    let mut best = 0u8;
    let mut best_err = u8::abs_diff(palette[0], v);
    for (i, &c) in palette.iter().enumerate().skip(1) {
        let err = u8::abs_diff(c, v);
        if err < best_err {
            best = i as u8;
            best_err = err;
        }
    }
    best
}

/// Rewrite the sixteen 3-bit indices of the BC4 block in `block[..8]` so each
/// is the nearest palette entry to the matching source value. The endpoints
/// are left alone. `src` is row-major within the 4x4 block.
fn repick_block(block: &mut [u8], src: &[u8; 16]) {
    let palette = bc4_palette(block[0], block[1]);
    let mut bits = 0u64;
    for (p, &v) in src.iter().enumerate() {
        bits |= u64::from(nearest_index(&palette, v)) << (3 * p);
    }
    // Six bytes hold the indices, least significant first.
    block[2..8].copy_from_slice(&bits.to_le_bytes()[..6]);
}

/// Repick one BC4-style plane across a whole encoded image.
///
/// `block_bytes` is the stride between blocks (8 for BC4, 16 for BC3 and
/// BC5), `plane_offset` is where this plane's eight bytes start inside a
/// block, and `sample` reads the source value at a pixel. Pixels past the
/// image edge take the nearest edge pixel.
fn repick_plane(
    encoded: &mut [u8],
    block_bytes: usize,
    plane_offset: usize,
    width: u32,
    height: u32,
    sample: impl Fn(u32, u32) -> u8,
) {
    let blocks_x = width.div_ceil(4) as usize;
    for (bi, block) in encoded.chunks_exact_mut(block_bytes).enumerate() {
        let (bx, by) = ((bi % blocks_x) as u32 * 4, (bi / blocks_x) as u32 * 4);
        let mut src = [0u8; 16];
        for (p, v) in src.iter_mut().enumerate() {
            let x = (bx + (p as u32 % 4)).min(width - 1);
            let y = (by + (p as u32 / 4)).min(height - 1);
            *v = sample(x, y);
        }
        repick_block(&mut block[plane_offset..plane_offset + 8], &src);
    }
}

/// Classify an RGBA8 buffer as "effectively opaque" (BC1-eligible)
/// or "has meaningful alpha" (needs BC3). Single sequential pass;
/// short-circuits on the first non-opaque pixel.
pub fn is_effectively_opaque(pixels: &[u8]) -> bool {
    debug_assert_eq!(pixels.len() % 4, 0);
    for chunk in pixels.chunks_exact(4) {
        if chunk[3] < OPAQUE_ALPHA_CUTOFF {
            return false;
        }
    }
    true
}

/// Encode an RGBA8 buffer as BC1 (4 bpp, sRGB or linear variant
/// decided at upload time by the caller's `CompressedColorSpace`).
/// Alpha is discarded — caller must have validated opacity via
/// [`is_effectively_opaque`] first.
pub fn encode_bc1(pixels: &[u8], width: u32, height: u32) -> TextureData {
    debug_assert_eq!(pixels.len(), (width as usize) * (height as usize) * 4);
    let bytes = tbc::encode_image_bc1_conv_u8(rgba8_view(pixels), width as usize, height as usize);
    TextureData::new_compressed(bytes, TexturePixelFormat::Bc1, width, height)
}

/// Encode an RGBA8 buffer as BC3 (8 bpp). Preserves the alpha
/// channel via the BC3 alpha block.
pub fn encode_bc3(pixels: &[u8], width: u32, height: u32) -> TextureData {
    debug_assert_eq!(pixels.len(), (width as usize) * (height as usize) * 4);
    let mut bytes =
        tbc::encode_image_bc3_conv_u8(rgba8_view(pixels), width as usize, height as usize);
    // The alpha block is the first eight bytes of each sixteen. The colour
    // half (the last eight) is left exactly as tbc wrote it.
    repick_plane(&mut bytes, 16, 0, width, height, |x, y| {
        pixels[((y * width + x) * 4 + 3) as usize]
    });
    TextureData::new_compressed(bytes, TexturePixelFormat::Bc3, width, height)
}

/// Encode the **red channel** of an RGBA8 buffer as BC4 (4 bpp, linear).
/// The other channels are discarded. For occlusion maps, which glTF keeps
/// in `.r`.
pub fn encode_bc4_red(pixels: &[u8], width: u32, height: u32) -> TextureData {
    debug_assert_eq!(pixels.len(), (width as usize) * (height as usize) * 4);
    let red: Vec<tbc::color::Red8> = pixels
        .chunks_exact(4)
        .map(|c| tbc::color::Red8 { red: c[0] })
        .collect();
    let mut bytes = tbc::encode_image_bc4_r8_conv_u8(&red, width as usize, height as usize);
    repick_plane(&mut bytes, 8, 0, width, height, |x, y| {
        pixels[((y * width + x) * 4) as usize]
    });
    TextureData::new_compressed(bytes, TexturePixelFormat::Bc4, width, height)
}

/// Encode the **red and green channels** of an RGBA8 buffer as BC5 (8 bpp,
/// linear): two BC4 halves per block, red then green. For tangent-space
/// normal maps, whose blue is rebuilt in the shader.
pub fn encode_bc5_rg(pixels: &[u8], width: u32, height: u32) -> TextureData {
    debug_assert_eq!(pixels.len(), (width as usize) * (height as usize) * 4);
    let rg: Vec<tbc::color::RedGreen8> = pixels
        .chunks_exact(4)
        .map(|c| tbc::color::RedGreen8 {
            red: c[0],
            green: c[1],
        })
        .collect();
    let mut bytes = tbc::encode_image_bc4_rg8_conv_u8(&rg, width as usize, height as usize);
    repick_plane(&mut bytes, 16, 0, width, height, |x, y| {
        pixels[((y * width + x) * 4) as usize]
    });
    repick_plane(&mut bytes, 16, 8, width, height, |x, y| {
        pixels[((y * width + x) * 4 + 1) as usize]
    });
    TextureData::new_compressed(bytes, TexturePixelFormat::Bc5, width, height)
}

/// Pick BC1 or BC3 based on the alpha profile and run the encode.
/// Convenience for the 2D image cache path which doesn't know the
/// texture's role (color / normal / etc.) and just wants the
/// smallest format that preserves data.
pub fn encode_auto(pixels: &[u8], width: u32, height: u32) -> TextureData {
    if is_effectively_opaque(pixels) {
        encode_bc1(pixels, width, height)
    } else {
        encode_bc3(pixels, width, height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Decode a block's sixteen indices.
    fn indices(block: &[u8]) -> [u8; 16] {
        let bits = block[2..8]
            .iter()
            .enumerate()
            .fold(0u64, |acc, (i, &b)| acc | u64::from(b) << (8 * i));
        std::array::from_fn(|p| ((bits >> (3 * p)) & 7) as u8)
    }

    #[test]
    fn the_palette_has_the_eight_step_form_when_a0_is_larger() {
        let p = bc4_palette(200, 100);
        assert_eq!((p[0], p[1]), (200, 100));
        // Evenly spaced between the endpoints, descending.
        assert!(p[2..].windows(2).all(|w| w[0] >= w[1]), "{p:?}");
        assert!(p[2] < 200 && p[7] > 100, "{p:?}");
    }

    #[test]
    fn the_palette_has_the_six_step_form_with_black_and_white_otherwise() {
        let p = bc4_palette(100, 200);
        assert_eq!((p[6], p[7]), (0, 255), "{p:?}");
        assert!(p[2..6].windows(2).all(|w| w[0] <= w[1]), "{p:?}");
    }

    /// The point of the function: every pixel lands on the nearest entry,
    /// in both palette forms.
    #[test]
    fn every_pixel_takes_the_nearest_palette_entry() {
        for (a0, a1) in [(200u8, 100u8), (100, 200), (255, 0), (7, 7)] {
            let pal = bc4_palette(a0, a1);
            // Values across the whole range, sixteen at a time.
            for base in (0..=255u32).step_by(16) {
                let src: [u8; 16] = std::array::from_fn(|i| (base + i as u32).min(255) as u8);
                let mut block = [a0, a1, 0, 0, 0, 0, 0, 0];
                repick_block(&mut block, &src);

                assert_eq!((block[0], block[1]), (a0, a1), "endpoints must not move");
                for (p, &idx) in indices(&block).iter().enumerate() {
                    let chosen = pal[idx as usize];
                    let best = pal.iter().map(|c| u8::abs_diff(*c, src[p])).min().unwrap();
                    assert_eq!(
                        u8::abs_diff(chosen, src[p]),
                        best,
                        "endpoints ({a0},{a1}), value {}: index {idx} gives {chosen}",
                        src[p]
                    );
                }
            }
        }
    }

    /// A constant block reproduces its value exactly when it is an endpoint.
    #[test]
    fn a_flat_block_is_exact() {
        let mut block = [90u8, 90, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff];
        repick_block(&mut block, &[90; 16]);
        let pal = bc4_palette(90, 90);
        assert!(indices(&block).iter().all(|&i| pal[i as usize] == 90));
    }

    /// Only the alpha half of a BC3 block may change. The colour half is
    /// tbc's and must come through byte for byte.
    #[test]
    fn bc3_leaves_the_colour_half_exactly_as_tbc_wrote_it() {
        const W: u32 = 32;
        const H: u32 = 32;
        let pixels: Vec<u8> = (0..W * H)
            .flat_map(|i| {
                let (x, y) = (i % W, i / W);
                [
                    (x * 8) as u8,
                    (y * 8) as u8,
                    ((x + y) * 4) as u8,
                    (x * 7 + y * 3) as u8,
                ]
            })
            .collect();

        let raw = tbc::encode_image_bc3_conv_u8(rgba8_view(&pixels), W as usize, H as usize);
        let ours = encode_bc3(&pixels, W, H)
            .with_bytes(|b| b.to_vec())
            .expect("bytes");

        assert_eq!(raw.len(), ours.len());
        for (i, (a, b)) in raw.chunks_exact(16).zip(ours.chunks_exact(16)).enumerate() {
            assert_eq!(a[8..], b[8..], "block {i}: the colour half changed");
            assert_eq!(a[..2], b[..2], "block {i}: the alpha endpoints changed");
        }
    }

    /// Sizes that are not multiples of four still encode, and the output is
    /// the right length. The edge blocks read the nearest edge pixel.
    #[test]
    fn dimensions_that_are_not_multiples_of_four_encode() {
        for (w, h) in [(10u32, 7u32), (1, 1), (5, 4), (4, 9)] {
            let pixels: Vec<u8> = (0..w * h)
                .flat_map(|i| [(i * 13) as u8, (i * 7) as u8, 0, (i * 29) as u8])
                .collect();
            let blocks = (w.div_ceil(4) * h.div_ceil(4)) as usize;

            let bc4 = encode_bc4_red(&pixels, w, h)
                .with_bytes(|b| b.len())
                .unwrap();
            let bc5 = encode_bc5_rg(&pixels, w, h)
                .with_bytes(|b| b.len())
                .unwrap();
            let bc3 = encode_bc3(&pixels, w, h).with_bytes(|b| b.len()).unwrap();
            assert_eq!(bc4, blocks * 8, "{w}x{h} BC4");
            assert_eq!(bc5, blocks * 16, "{w}x{h} BC5");
            assert_eq!(bc3, blocks * 16, "{w}x{h} BC3");
        }
    }
}
