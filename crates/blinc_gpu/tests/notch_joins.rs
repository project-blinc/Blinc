//! The notch distance function's joins.
//!
//! A flare lies along the body's edge, so it is joined with a plain `min`:
//! a blend there bulges the shared edge and leaves a step where the flare
//! rect ends. A bulge or a peak is buried deeper into the body than a flare,
//! and clipped to its own width below its base line.

use blinc_gpu::shaders::{SDF_NOTCH_DT_SHADER, SDF_NOTCH_SHADER, SDF_NOTCH_VB_SHADER};

/// The notch distance function, in russl.
const RUSSL: &str = include_str!("../src/sdf_russl/shared/notch.rs");

fn squeeze(src: &str) -> String {
    src.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn the_russl_notch_keeps_the_same_joins() {
    let src = squeeze(RUSSL);
    assert_eq!(src.matches("d = min(d, flare);").count(), 4);
    assert!(!src.contains("smin(d, flare"), "a flare is blended again");
    assert!(src.contains("let join_depth = min(8.0, min(inner_size.x, inner_size.y) * 0.5);"));
    assert!(src.contains("let deep_depth = min(inner_size.x, inner_size.y) * 0.5;"));
    assert_eq!(
        src.matches("abs(p.x - cx) - half_w").count(),
        4,
        "a piece is not clipped to its own width"
    );
    assert!(
        !src.contains("p.y - base_y - join_depth") && !src.contains("base_y - p.y - join_depth"),
        "a bulge still stops at the join depth"
    );
}

#[test]
fn the_notch_shaders_still_validate() {
    use naga::valid::{Capabilities, ValidationFlags, Validator};
    for (name, src) in [
        ("sdf_notch", SDF_NOTCH_SHADER),
        ("sdf_notch_vb", SDF_NOTCH_VB_SHADER),
        ("sdf_notch_dt", SDF_NOTCH_DT_SHADER),
    ] {
        let module = naga::front::wgsl::parse_str(src)
            .unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(src)));
        Validator::new(ValidationFlags::all(), Capabilities::all())
            .validate(&module)
            .unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(src)));
    }
}
