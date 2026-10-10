//! Executable spec for the box-shadow edge rules in the SDF shadow shaders.
//!
//! `spread_radius` is a port of `shadow_spread_radius` in
//! `shaders/common/sdf_common.wgsl`. The renderer grows a shadow by its
//! spread and CSS grows a corner by the spread only as far as the corner is
//! already round, so a square corner stays square.
//!
//! The source checks keep the three variants (storage buffer, vertex buffer,
//! data texture) in step: they are separate files with the same fragment code.

use blinc_gpu::shaders::{SDF_SHADOW_DT_SHADER, SDF_SHADOW_SHADER, SDF_SHADOW_VB_SHADER};

fn spread_radius(r: f32, s: f32) -> f32 {
    if s > 0.0 {
        let u = (r / s).min(1.0) - 1.0;
        r + s * (1.0 + u * u * u)
    } else {
        (r + s).max(0.0)
    }
}

#[test]
fn a_square_corner_stays_square_under_spread() {
    assert_eq!(spread_radius(0.0, 8.0), 0.0);
    assert_eq!(spread_radius(0.0, 0.5), 0.0);
}

#[test]
fn a_corner_at_least_as_round_as_the_spread_grows_by_it() {
    assert_eq!(spread_radius(8.0, 8.0), 16.0);
    assert_eq!(spread_radius(20.0, 8.0), 28.0);
}

#[test]
fn between_square_and_round_it_grows_smoothly() {
    // CSS: r + s * (1 + (r/s - 1)^3) for 0 < r < s.
    let (r, s) = (4.0_f32, 8.0_f32);
    assert!((spread_radius(r, s) - (r + s * (1.0 - 0.125))).abs() < 1e-5);

    // Monotonic in r with slope at most 4 (at r = 0), so no jump anywhere,
    // including where it meets r + s.
    let mut last = spread_radius(0.0, s);
    for i in 1..=80 {
        let now = spread_radius(i as f32 * 0.1, s);
        assert!(now >= last, "not monotonic at r = {}", i as f32 * 0.1);
        assert!(now - last < 0.4 + 1e-4, "jump at r = {}", i as f32 * 0.1);
        last = now;
    }
}

#[test]
fn a_negative_spread_shrinks_the_corner_and_stops_at_square() {
    assert_eq!(spread_radius(10.0, -4.0), 6.0);
    assert_eq!(spread_radius(2.0, -4.0), 0.0);
    assert_eq!(spread_radius(0.0, -4.0), 0.0);
}

const VARIANTS: [(&str, &str); 3] = [
    ("sdf_shadow", SDF_SHADOW_SHADER),
    ("sdf_shadow_vb", SDF_SHADOW_VB_SHADER),
    ("sdf_shadow_dt", SDF_SHADOW_DT_SHADER),
];

fn squeeze(src: &str) -> String {
    src.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn every_variant_grows_the_quad_by_the_spread() {
    for (name, src) in VARIANTS {
        let src = squeeze(src);
        assert!(
            src.contains(".w, 0.0) + 1.0;") && src.contains("let blur_expand"),
            "{name}: the vertex stage does not pad by the spread and an antialiasing pixel"
        );
    }
}

#[test]
fn every_variant_draws_an_outer_shadow_that_has_only_an_offset() {
    for (name, src) in VARIANTS {
        let src = squeeze(src);
        assert!(
            !src.contains("prim.shadow.w != 0.0"),
            "{name}: an outer shadow with no blur and no spread is still skipped"
        );
    }
}

#[test]
fn every_variant_grows_corners_the_way_css_does() {
    for (name, src) in VARIANTS {
        let src = squeeze(src);
        assert!(
            src.contains("shadow_spread_radius(prim.corner_radius, spread)"),
            "{name}: corners are not grown by shadow_spread_radius"
        );
        assert!(
            !src.contains("prim.corner_radius + vec4<f32>(spread)"),
            "{name}: spread is added to every corner radius"
        );
    }
}

#[test]
fn every_variant_antialiases_an_unblurred_edge() {
    for (name, src) in VARIANTS {
        let src = squeeze(src);
        assert!(
            src.contains("clamp(0.5 - shadow_sdf_dist, 0.0, 1.0)"),
            "{name}: the outer edge with no blur is a hard step"
        );
        assert_eq!(
            src.matches("clamp(0.5 + spread - edge_dist, 0.0, 1.0)")
                .count(),
            2,
            "{name}: an inset shadow with no blur (box and circle) is not antialiased"
        );
    }
}

#[test]
fn every_variant_still_validates() {
    use naga::valid::{Capabilities, ValidationFlags, Validator};
    for (name, src) in VARIANTS {
        let module = naga::front::wgsl::parse_str(src)
            .unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(src)));
        Validator::new(ValidationFlags::all(), Capabilities::all())
            .validate(&module)
            .unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(src)));
    }
}
