//! CPU decoding of BC1 / BC3 / BC4 / BC5 to RGBA8.
//!
//! Adapters without `TEXTURE_COMPRESSION_BC` (iOS, most mobile GPUs) cannot sample BC
//! textures, so a BC-encoded mesh texture is expanded here and uploaded as plain RGBA8.
//! BC4 decodes to `(r, 0, 0, 255)` and BC5 to `(r, g, 0, 255)`, matching what a GPU
//! returns when sampling those formats.

use blinc_core::draw::TexturePixelFormat;

/// Expand `data` (tightly packed 4x4 blocks) into `width * height` RGBA8 pixels.
///
/// Returns `None` for `Rgba8` input, zero dimensions, or data shorter than the block grid.
pub fn decode_to_rgba8(
    data: &[u8],
    format: TexturePixelFormat,
    width: u32,
    height: u32,
) -> Option<Vec<u8>> {
    let block_bytes = match format {
        TexturePixelFormat::Rgba8 => return None,
        TexturePixelFormat::Bc1 | TexturePixelFormat::Bc4 => 8,
        TexturePixelFormat::Bc3 | TexturePixelFormat::Bc5 => 16,
    };
    if width == 0 || height == 0 {
        return None;
    }
    let (w, h) = (width as usize, height as usize);
    let (bx, by) = (w.div_ceil(4), h.div_ceil(4));
    if data.len() < bx * by * block_bytes {
        return None;
    }

    let mut out = vec![0u8; w * h * 4];
    for block_y in 0..by {
        for block_x in 0..bx {
            let block = &data[(block_y * bx + block_x) * block_bytes..][..block_bytes];
            let texels = decode_block(block, format);
            for py in 0..4 {
                for px in 0..4 {
                    let (x, y) = (block_x * 4 + px, block_y * 4 + py);
                    if x < w && y < h {
                        out[(y * w + x) * 4..][..4].copy_from_slice(&texels[py * 4 + px]);
                    }
                }
            }
        }
    }
    Some(out)
}

fn decode_block(block: &[u8], format: TexturePixelFormat) -> [[u8; 4]; 16] {
    let mut texels = [[0, 0, 0, 255]; 16];
    match format {
        TexturePixelFormat::Bc1 => decode_color(block, true, &mut texels),
        TexturePixelFormat::Bc3 => {
            decode_color(&block[8..], false, &mut texels);
            let alpha = decode_plane(&block[..8]);
            for (texel, a) in texels.iter_mut().zip(alpha) {
                texel[3] = a;
            }
        }
        TexturePixelFormat::Bc4 => {
            for (texel, r) in texels.iter_mut().zip(decode_plane(block)) {
                texel[0] = r;
            }
        }
        TexturePixelFormat::Bc5 => {
            let (red, green) = (decode_plane(&block[..8]), decode_plane(&block[8..]));
            for (i, texel) in texels.iter_mut().enumerate() {
                texel[0] = red[i];
                texel[1] = green[i];
            }
        }
        TexturePixelFormat::Rgba8 => {}
    }
    texels
}

fn expand565(c: u16) -> [u8; 3] {
    let (r, g, b) = ((c >> 11) as u8 & 31, (c >> 5) as u8 & 63, c as u8 & 31);
    [
        (r << 3) | (r >> 2),
        (g << 2) | (g >> 4),
        (b << 3) | (b >> 2),
    ]
}

/// One BC1 colour block. `punch_through` enables BC1's transparent fourth entry when
/// `c0 <= c1`; BC3's colour half always uses four opaque entries.
fn decode_color(block: &[u8], punch_through: bool, texels: &mut [[u8; 4]; 16]) {
    let c0 = u16::from_le_bytes([block[0], block[1]]);
    let c1 = u16::from_le_bytes([block[2], block[3]]);
    let (e0, e1) = (expand565(c0), expand565(c1));
    let mix =
        |a: u8, b: u8, wa: u16, wb: u16, div: u16| ((a as u16 * wa + b as u16 * wb) / div) as u8;
    let mut palette = [[0u8; 4]; 4];
    palette[0] = [e0[0], e0[1], e0[2], 255];
    palette[1] = [e1[0], e1[1], e1[2], 255];
    if c0 > c1 || !punch_through {
        for c in 0..3 {
            palette[2][c] = mix(e0[c], e1[c], 2, 1, 3);
            palette[3][c] = mix(e0[c], e1[c], 1, 2, 3);
        }
        palette[2][3] = 255;
        palette[3][3] = 255;
    } else {
        for c in 0..3 {
            palette[2][c] = mix(e0[c], e1[c], 1, 1, 2);
        }
        palette[2][3] = 255;
        palette[3] = [0, 0, 0, 0];
    }
    let indices = u32::from_le_bytes([block[4], block[5], block[6], block[7]]);
    for (i, texel) in texels.iter_mut().enumerate() {
        *texel = palette[((indices >> (2 * i)) & 3) as usize];
    }
}

/// One BC4 plane (also BC3's alpha half and each half of BC5): two endpoints and sixteen
/// 3-bit indices.
fn decode_plane(block: &[u8]) -> [u8; 16] {
    let (a0, a1) = (block[0] as u16, block[1] as u16);
    let mut values = [0u8; 8];
    values[0] = a0 as u8;
    values[1] = a1 as u8;
    if a0 > a1 {
        for i in 2..8u16 {
            values[i as usize] = (((8 - i) * a0 + (i - 1) * a1) / 7) as u8;
        }
    } else {
        for i in 2..6u16 {
            values[i as usize] = (((6 - i) * a0 + (i - 1) * a1) / 5) as u8;
        }
        values[6] = 0;
        values[7] = 255;
    }
    let mut bits = 0u64;
    for (i, byte) in block[2..8].iter().enumerate() {
        bits |= (*byte as u64) << (8 * i);
    }
    let mut out = [0u8; 16];
    for (i, v) in out.iter_mut().enumerate() {
        *v = values[((bits >> (3 * i)) & 7) as usize];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED565: u16 = 0xF800;
    const BLUE565: u16 = 0x001F;

    fn bc1_block(c0: u16, c1: u16, indices: u32) -> Vec<u8> {
        let mut b = c0.to_le_bytes().to_vec();
        b.extend(c1.to_le_bytes());
        b.extend(indices.to_le_bytes());
        b
    }

    #[test]
    fn bc1_selects_endpoints_and_blends() {
        // Indices 0,1,2,3 repeated: red, blue, 2/3 red + 1/3 blue, 1/3 red + 2/3 blue.
        let data = bc1_block(
            RED565,
            BLUE565,
            0b11_10_01_00_11_10_01_00_11_10_01_00_11_10_01_00,
        );
        let px = decode_to_rgba8(&data, TexturePixelFormat::Bc1, 4, 4).unwrap();
        assert_eq!(&px[0..4], &[255, 0, 0, 255]);
        assert_eq!(&px[4..8], &[0, 0, 255, 255]);
        assert_eq!(&px[8..12], &[170, 0, 85, 255]);
        assert_eq!(&px[12..16], &[85, 0, 170, 255]);
    }

    #[test]
    fn bc1_punch_through_only_when_c0_is_not_greater() {
        let data = bc1_block(
            BLUE565,
            RED565,
            0b11_11_11_11_11_11_11_11_11_11_11_11_11_11_11_11,
        );
        let px = decode_to_rgba8(&data, TexturePixelFormat::Bc1, 4, 4).unwrap();
        assert_eq!(&px[0..4], &[0, 0, 0, 0]);
        let data = bc1_block(
            BLUE565,
            RED565,
            0b10_10_10_10_10_10_10_10_10_10_10_10_10_10_10_10,
        );
        let px = decode_to_rgba8(&data, TexturePixelFormat::Bc1, 4, 4).unwrap();
        assert_eq!(&px[0..4], &[127, 0, 127, 255]);
    }

    /// A BC4 plane whose sixteen indices are all `index`.
    fn plane(a0: u8, a1: u8, index: u8) -> Vec<u8> {
        let mut v = vec![a0, a1];
        let mut bits = 0u64;
        for i in 0..16 {
            bits |= (index as u64) << (3 * i);
        }
        v.extend(&bits.to_le_bytes()[..6]);
        v
    }

    #[test]
    fn bc3_takes_alpha_from_its_own_block_and_never_punches_through() {
        // Alpha endpoints 255 and 0, every index 1 (= a1 = 0).
        let mut data = plane(255, 0, 1);
        // Colour half with c0 <= c1 and index 3: opaque in BC3, transparent in BC1.
        data.extend(bc1_block(BLUE565, RED565, u32::MAX));
        let px = decode_to_rgba8(&data, TexturePixelFormat::Bc3, 4, 4).unwrap();
        assert_eq!(px[3], 0);
        assert_eq!(&px[0..3], &[170, 0, 85]);
    }

    #[test]
    fn bc4_and_bc5_fill_the_channels_a_gpu_would_return() {
        // a0 > a1: index 0 is a0.
        let px = decode_to_rgba8(&plane(200, 10, 0), TexturePixelFormat::Bc4, 4, 4).unwrap();
        assert_eq!(&px[0..4], &[200, 0, 0, 255]);
        // a0 <= a1: index 6 is 0 and index 7 is 255.
        let px = decode_to_rgba8(&plane(10, 10, 7), TexturePixelFormat::Bc4, 4, 4).unwrap();
        assert_eq!(px[0], 255);
        let mut data = plane(90, 90, 0);
        data.extend(plane(30, 30, 0));
        let px = decode_to_rgba8(&data, TexturePixelFormat::Bc5, 4, 4).unwrap();
        assert_eq!(&px[0..4], &[90, 30, 0, 255]);
    }

    #[test]
    fn partial_edge_blocks_are_clipped_and_short_data_is_refused() {
        let data = bc1_block(RED565, RED565, 0);
        let px = decode_to_rgba8(&data, TexturePixelFormat::Bc1, 3, 2).unwrap();
        assert_eq!(px.len(), 3 * 2 * 4);
        assert!(decode_to_rgba8(&data[..7], TexturePixelFormat::Bc1, 4, 4).is_none());
        assert!(decode_to_rgba8(&data, TexturePixelFormat::Bc1, 8, 4).is_none());
        assert!(decode_to_rgba8(&data, TexturePixelFormat::Rgba8, 4, 4).is_none());
    }

    #[cfg(feature = "bc-encode")]
    #[test]
    fn round_trips_what_the_encoder_wrote() {
        let (w, h) = (16u32, 8u32);
        // A ramp along one line in RGB, which BC1's four-entry palette reproduces closely, so a
        // decoder bug shows as a large error.
        let pixels: Vec<u8> = (0..w * h)
            .flat_map(|i| {
                let (x, y) = (i % w, i / w);
                let v = x * 12 + y * 4;
                [v as u8, (v / 2) as u8, 128, 255]
            })
            .collect();
        let texture = crate::bc_encode::encode_bc1(&pixels, w, h);
        let bytes = texture.with_bytes(|b| b.to_vec()).unwrap();
        let out = decode_to_rgba8(&bytes, TexturePixelFormat::Bc1, w, h).unwrap();
        let worst = pixels
            .chunks(4)
            .zip(out.chunks(4))
            .flat_map(|(a, b)| (0..3).map(move |c| a[c].abs_diff(b[c])))
            .max()
            .unwrap();
        assert!(worst < 24, "decoded colour strayed {worst} from the source");
    }
}
