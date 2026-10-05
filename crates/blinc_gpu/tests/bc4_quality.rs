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
//!
//! Two kinds of test live here. The ones that call `tbc` directly are
//! #[ignore]d canaries: they document tbc's defect and fail while it exists.
//! The ones that call `blinc_gpu::bc_encode` are the live guards, and assert
//! that Blinc's encoders are within a decibel of the best output their own
//! endpoints allow, and well above tbc's raw quality.

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
#[ignore = "canary: tbc 0.3 BC4 picks poor indices. Blinc corrects for it in bc_encode; when this passes, tbc is fixed and the correction can go"]
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

/// BC3's alpha block is a BC4 block, so a BC4 index defect would also cost
/// every image with transparency that Blinc's own image cache encodes as
/// BC3. Same method: decode tbc's output, and compare against the best
/// possible output for the alpha endpoints tbc itself chose.
#[test]
#[ignore = "canary: tbc 0.3 BC3 alpha uses the same poor index pick. Blinc corrects for it in bc_encode"]
fn bc3_alpha_indices_are_the_nearest_palette_entry() {
    const W: usize = 256;
    const H: usize = 256;
    let alpha = source(W, H);

    // Through Blinc's own wrapper, the path real callers take: tbc's pixel
    // type has private fields, so it takes packed RGBA bytes.
    let rgba: Vec<u8> = alpha.iter().flat_map(|&a| [120u8, 80, 40, a]).collect();
    let texture = blinc_gpu::bc_encode::encode_bc3(&rgba, W as u32, H as u32);
    let encoded = texture
        .with_bytes(|b| b.to_vec())
        .expect("CPU bytes are present until the first upload");

    let mut as_encoded = vec![0u8; W * H];
    let mut best_possible = vec![0u8; W * H];
    let blocks_x = W / 4;
    for (bi, block) in encoded.chunks_exact(16).enumerate() {
        let (bx, by) = (bi % blocks_x, bi / blocks_x);
        // The first eight bytes are the alpha block, laid out as BC4.
        let pal = palette(block[0], block[1]);
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
            let want = alpha[py * W + px];
            best_possible[py * W + px] = *pal
                .iter()
                .min_by_key(|c| (**c as i32 - want as i32).abs())
                .unwrap();
        }
    }

    let got = psnr(&alpha, &as_encoded);
    let best = psnr(&alpha, &best_possible);
    eprintln!("BC3 alpha: tbc {got:.2}dB, nearest-of-palette with tbc's OWN endpoints {best:.2}dB");
    assert!(
        best - got < 1.0,
        "tbc's BC3 alpha indices are {:.2}dB worse than the nearest palette entry ({got:.2} vs {best:.2})",
        best - got
    );
}

// ---------------------------------------------------------------------------
// Live guards: Blinc's encoders, which correct tbc's index choice.
// ---------------------------------------------------------------------------

/// Decode one BC4-style plane (eight bytes at `offset` in each `stride`-byte
/// block) and also build the best possible output for its endpoints.
fn decode_plane(
    encoded: &[u8],
    stride: usize,
    offset: usize,
    w: usize,
    h: usize,
    src: &[u8],
) -> (Vec<u8>, Vec<u8>) {
    let mut got = vec![0u8; w * h];
    let mut best = vec![0u8; w * h];
    let blocks_x = w / 4;
    for (bi, block) in encoded.chunks_exact(stride).enumerate() {
        let (bx, by) = (bi % blocks_x, bi / blocks_x);
        let b = &block[offset..offset + 8];
        let pal = palette(b[0], b[1]);
        let bits = (0..6).fold(0u64, |acc, i| acc | u64::from(b[2 + i]) << (8 * i));
        for p in 0..16 {
            let (px, py) = (bx * 4 + p % 4, by * 4 + p / 4);
            let idx = ((bits >> (3 * p)) & 0x7) as usize;
            got[py * w + px] = pal[idx];
            let want = src[py * w + px];
            best[py * w + px] = *pal
                .iter()
                .min_by_key(|c| (**c as i32 - want as i32).abs())
                .unwrap();
        }
    }
    (got, best)
}

const LIVE_W: usize = 256;
const LIVE_H: usize = 256;

fn live_bytes(texture: &blinc_core::draw::TextureData) -> Vec<u8> {
    texture
        .with_bytes(|b| b.to_vec())
        .expect("CPU bytes are present until the first upload")
}

/// Blinc's BC4 must be within a decibel of the best possible for the
/// endpoints it uses, and far above tbc's raw 42 dB on this image.
#[test]
fn blinc_bc4_is_within_a_decibel_of_the_best_possible() {
    let src = source(LIVE_W, LIVE_H);
    let rgba: Vec<u8> = src.iter().flat_map(|&r| [r, 0, 0, 255]).collect();
    let enc = live_bytes(&blinc_gpu::bc_encode::encode_bc4_red(
        &rgba,
        LIVE_W as u32,
        LIVE_H as u32,
    ));

    let (got, best) = decode_plane(&enc, 8, 0, LIVE_W, LIVE_H, &src);
    let (q, ceiling) = (psnr(&src, &got), psnr(&src, &best));
    eprintln!("blinc BC4: {q:.2}dB, ceiling for its endpoints {ceiling:.2}dB");
    assert!(q >= ceiling - 1.0, "{q:.2} vs {ceiling:.2}");
    assert!(q > 60.0, "BC4 came out at {q:.2}dB; tbc raw is about 42");
}

/// Same for BC3's alpha, which is the half that cost transparent images.
#[test]
fn blinc_bc3_alpha_is_within_a_decibel_of_the_best_possible() {
    let alpha = source(LIVE_W, LIVE_H);
    let rgba: Vec<u8> = alpha.iter().flat_map(|&a| [120u8, 80, 40, a]).collect();
    let enc = live_bytes(&blinc_gpu::bc_encode::encode_bc3(
        &rgba,
        LIVE_W as u32,
        LIVE_H as u32,
    ));

    let (got, best) = decode_plane(&enc, 16, 0, LIVE_W, LIVE_H, &alpha);
    let (q, ceiling) = (psnr(&alpha, &got), psnr(&alpha, &best));
    eprintln!("blinc BC3 alpha: {q:.2}dB, ceiling for its endpoints {ceiling:.2}dB");
    assert!(q >= ceiling - 1.0, "{q:.2} vs {ceiling:.2}");
    assert!(
        q > 60.0,
        "BC3 alpha came out at {q:.2}dB; tbc raw is about 42"
    );
}

/// BC5 is two BC4 planes, red then green, and both must be right.
#[test]
fn blinc_bc5_gets_both_planes_right() {
    let r = source(LIVE_W, LIVE_H);
    // A different, still smooth, green so a swapped plane would show.
    let g: Vec<u8> = r.iter().map(|&v| 255 - v / 2).collect();
    let rgba: Vec<u8> = r
        .iter()
        .zip(&g)
        .flat_map(|(&r, &g)| [r, g, 0, 255])
        .collect();
    let enc = live_bytes(&blinc_gpu::bc_encode::encode_bc5_rg(
        &rgba,
        LIVE_W as u32,
        LIVE_H as u32,
    ));

    let (got_r, best_r) = decode_plane(&enc, 16, 0, LIVE_W, LIVE_H, &r);
    let (got_g, best_g) = decode_plane(&enc, 16, 8, LIVE_W, LIVE_H, &g);
    let (qr, qg) = (psnr(&r, &got_r), psnr(&g, &got_g));
    eprintln!("blinc BC5: red {qr:.2}dB, green {qg:.2}dB");
    // Written as q >= ceiling - 1 because a plane that reproduces exactly has
    // an infinite PSNR, and inf - inf is NaN, which fails every comparison.
    assert!(qr >= psnr(&r, &best_r) - 1.0, "red below its ceiling");
    assert!(qg >= psnr(&g, &best_g) - 1.0, "green below its ceiling");
    assert!(qr > 60.0 && qg > 60.0, "red {qr:.2}dB, green {qg:.2}dB");
}

/// Not specific to smooth data. On noise with hard edges, the corrected
/// encoder must still never be worse than raw tbc, since picking the nearest
/// entry cannot lose to picking some other one for the same endpoints.
#[test]
fn blinc_bc4_is_never_worse_than_raw_tbc_on_noisy_data() {
    // A fixed linear congruential generator, so the data is the same every
    // run, with a checkerboard laid over it for hard edges.
    let mut state = 0x2545_f491u32;
    let mut next = move || {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (state >> 24) as u8
    };
    let src: Vec<u8> = (0..LIVE_W * LIVE_H)
        .map(|i| {
            let (x, y) = (i % LIVE_W, i / LIVE_W);
            let edge = if (x / 16 + y / 16) % 2 == 0 {
                40u8
            } else {
                200
            };
            edge.saturating_add(next() / 6)
        })
        .collect();

    let red: Vec<tbc::color::Red8> = src.iter().map(|&r| tbc::color::Red8 { red: r }).collect();
    let raw = tbc::encode_image_bc4_r8_conv_u8(&red, LIVE_W, LIVE_H);
    let (raw_got, _) = decode_plane(&raw, 8, 0, LIVE_W, LIVE_H, &src);

    let rgba: Vec<u8> = src.iter().flat_map(|&r| [r, 0, 0, 255]).collect();
    let ours = live_bytes(&blinc_gpu::bc_encode::encode_bc4_red(
        &rgba,
        LIVE_W as u32,
        LIVE_H as u32,
    ));
    let (our_got, _) = decode_plane(&ours, 8, 0, LIVE_W, LIVE_H, &src);

    let (q_raw, q_ours) = (psnr(&src, &raw_got), psnr(&src, &our_got));
    eprintln!("noisy + edges: raw tbc {q_raw:.2}dB, blinc {q_ours:.2}dB");
    assert!(
        q_ours >= q_raw,
        "the correction made it worse: {q_raw:.2} -> {q_ours:.2}"
    );
}
