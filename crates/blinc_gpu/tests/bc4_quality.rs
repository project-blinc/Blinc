//! Does tbc's BC4 encoder pick the best index for each pixel?
//!
//! BC4 stores two endpoints per 4x4 block plus a 3-bit index per pixel.
//! With a0 > a1 the palette is a0, a1, then six interpolated steps
//! ((7-k)*a0 + k*a1)/7 for k in 1..=6. Given endpoints, the best index
//! for a pixel is simply the nearest palette entry, so an encoder cannot
//! do better than that and should not do worse.
//!
//! Reported by the ashui session against a real 2K texture. This builds
//! its own image so the check stands alone.

#![cfg(feature = "bc-encode")]

/// Decode one BC4 block's palette from its two endpoints.
fn palette(a0: u8, a1: u8) -> [u8; 8] {
    let (f0, f1) = (a0 as f32, a1 as f32);
    let mut p = [0u8; 8];
    p[0] = a0;
    p[1] = a1;
    if a0 > a1 {
        for k in 1..=6u32 {
            p[1 + k as usize] = (((7 - k) as f32 * f0 + k as f32 * f1) / 7.0).round() as u8;
        }
    } else {
        for k in 1..=4u32 {
            p[1 + k as usize] = (((5 - k) as f32 * f0 + k as f32 * f1) / 5.0).round() as u8;
        }
        p[6] = 0;
        p[7] = 255;
    }
    p
}

/// A deterministic image with smooth gradients and some structure, the
/// shape an occlusion or normal channel actually has.
fn source(w: usize, h: usize) -> Vec<u8> {
    let mut v = vec![0u8; w * h];
    for y in 0..h {
        for x in 0..w {
            let fx = x as f32 / w as f32;
            let fy = y as f32 / h as f32;
            let grad = (fx * 180.0 + fy * 60.0) as i32;
            let ripple = ((fx * 24.0).sin() * (fy * 18.0).cos() * 20.0) as i32;
            v[y * w + x] = (grad + ripple).clamp(0, 255) as u8;
        }
    }
    v
}

fn psnr(a: &[u8], b: &[u8]) -> f64 {
    let mse: f64 = a
        .iter()
        .zip(b)
        .map(|(p, q)| {
            let d = *p as f64 - *q as f64;
            d * d
        })
        .sum::<f64>()
        / a.len() as f64;
    if mse == 0.0 {
        return f64::INFINITY;
    }
    10.0 * (255.0f64 * 255.0 / mse).log10()
}

#[test]
#[ignore = "tbc 0.3 BC4 picks poor indices; needs a dependency decision, see git-bug"]
fn bc4_indices_are_the_nearest_palette_entry() {
    const W: usize = 256;
    const H: usize = 256;
    let src = source(W, H);

    let red: Vec<tbc::color::Red8> = src.iter().map(|&r| tbc::color::Red8 { red: r }).collect();
    let encoded = tbc::encode_image_bc4_r8_conv_u8(&red, W, H);

    // Decode tbc's own output, and alongside it the best possible
    // output for the endpoints tbc chose.
    let mut as_encoded = vec![0u8; W * H];
    let mut best_possible = vec![0u8; W * H];

    let blocks_x = W / 4;
    for (bi, block) in encoded.chunks_exact(8).enumerate() {
        let (bx, by) = (bi % blocks_x, bi / blocks_x);
        let pal = palette(block[0], block[1]);

        // Six bytes hold sixteen 3-bit indices, little-endian.
        let bits = u64::from(block[2])
            | u64::from(block[3]) << 8
            | u64::from(block[4]) << 16
            | u64::from(block[5]) << 24
            | u64::from(block[6]) << 32
            | u64::from(block[7]) << 40;

        for p in 0..16 {
            let (px, py) = (bx * 4 + p % 4, by * 4 + p / 4);
            let idx = ((bits >> (3 * p)) & 0x7) as usize;
            as_encoded[py * W + px] = pal[idx];

            let want = src[py * W + px];
            best_possible[py * W + px] = *pal
                .iter()
                .min_by_key(|c| (**c as i32 - want as i32).abs())
                .unwrap();
        }
    }

    let got = psnr(&src, &as_encoded);
    let best = psnr(&src, &best_possible);
    eprintln!("BC4: tbc {got:.2}dB, nearest-of-palette with tbc's OWN endpoints {best:.2}dB");

    assert!(
        best - got < 1.0,
        "tbc's indices are {:.2}dB worse than simply taking the nearest palette entry \
         for the endpoints it already chose ({got:.2} vs {best:.2})",
        best - got
    );
}
