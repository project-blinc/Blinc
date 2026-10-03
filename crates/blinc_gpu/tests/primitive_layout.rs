//! Guards that `GpuPrimitive` and the WGSL `struct Primitive` agree.
//!
//! The Rust struct is uploaded straight into the storage buffer the
//! shaders read as `Primitive`. Nothing in the type system ties the two
//! together, and the WGSL side is written out by hand in twelve shader
//! files. Add a field, miss one copy, and every shader reading past that
//! offset gets whatever the next field happens to be: no compile error,
//! no failing test, just wrong pixels.
//!
//! Blinc has had two bugs of this family already — aux_data routing in
//! the dynamic batch, and the tight-render mesh aux_data offset.
//!
//! These tests do not need a GPU.

use std::collections::BTreeMap;

/// Every shader that declares the struct, by the path `include_str!`
/// would use. Kept explicit so a new shader has to be added here too.
const SHADERS: &[(&str, &str)] = &[
    ("sdf_core", include_str!("../src/shaders/sdf_core.wgsl")),
    (
        "sdf_core_dt",
        include_str!("../src/shaders/sdf_core_dt.wgsl"),
    ),
    (
        "sdf_core_vb",
        include_str!("../src/shaders/sdf_core_vb.wgsl"),
    ),
    ("sdf_shadow", include_str!("../src/shaders/sdf_shadow.wgsl")),
    (
        "sdf_shadow_dt",
        include_str!("../src/shaders/sdf_shadow_dt.wgsl"),
    ),
    (
        "sdf_shadow_vb",
        include_str!("../src/shaders/sdf_shadow_vb.wgsl"),
    ),
    ("sdf_notch", include_str!("../src/shaders/sdf_notch.wgsl")),
    (
        "sdf_notch_dt",
        include_str!("../src/shaders/sdf_notch_dt.wgsl"),
    ),
    (
        "sdf_notch_vb",
        include_str!("../src/shaders/sdf_notch_vb.wgsl"),
    ),
    ("sdf_3d", include_str!("../src/shaders/sdf_3d.wgsl")),
    ("sdf_3d_dt", include_str!("../src/shaders/sdf_3d_dt.wgsl")),
    ("sdf_3d_vb", include_str!("../src/shaders/sdf_3d_vb.wgsl")),
];

/// `(name, wgsl_type)` in declaration order.
fn primitive_fields(src: &str) -> Vec<(String, String)> {
    let start = src
        .find("\nstruct Primitive {")
        .expect("no `struct Primitive` declaration");
    let body_start = src[start..].find('{').unwrap() + start + 1;
    let body_end = src[body_start..].find("\n}").unwrap() + body_start;

    let mut out = Vec::new();
    for line in src[body_start..body_end].lines() {
        let line = line.split("//").next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let line = line.trim_end_matches(',');
        let Some((name, ty)) = line.split_once(':') else {
            continue;
        };
        out.push((
            name.trim().to_string(),
            ty.trim().trim_end_matches(',').to_string(),
        ));
    }
    out
}

/// Size and alignment of a WGSL type, per the layout rules wgpu applies
/// to a storage buffer.
fn wgsl_size_align(ty: &str) -> (usize, usize) {
    match ty {
        "vec4<f32>" | "vec4<u32>" | "vec4<i32>" => (16, 16),
        "vec3<f32>" | "vec3<u32>" | "vec3<i32>" => (12, 16),
        "vec2<f32>" | "vec2<u32>" | "vec2<i32>" => (8, 8),
        "f32" | "u32" | "i32" => (4, 4),
        other => panic!("unmapped WGSL type `{other}` — extend this table"),
    }
}

/// The Rust type each WGSL type must be paired with.
fn expected_rust(ty: &str) -> &'static str {
    match ty {
        "vec4<f32>" => "[f32; 4]",
        "vec4<u32>" => "[u32; 4]",
        "vec4<i32>" => "[i32; 4]",
        "vec2<f32>" => "[f32; 2]",
        "f32" => "f32",
        "u32" => "u32",
        "i32" => "i32",
        other => panic!("unmapped WGSL type `{other}`"),
    }
}

/// All twelve copies must declare the same struct. One drifting copy is
/// the likeliest way this breaks, and the hardest to spot by eye.
#[test]
fn every_shader_declares_the_same_primitive() {
    let mut by_shape: BTreeMap<Vec<(String, String)>, Vec<&str>> = BTreeMap::new();
    for (name, src) in SHADERS {
        by_shape
            .entry(primitive_fields(src))
            .or_default()
            .push(name);
    }
    assert_eq!(
        by_shape.len(),
        1,
        "the WGSL `struct Primitive` has diverged between shaders: {:?}",
        by_shape.values().map(|v| v.join(", ")).collect::<Vec<_>>()
    );
}

/// The WGSL struct's computed size must equal the Rust struct's, or the
/// shader reads a different stride than the buffer was written with.
#[test]
fn the_wgsl_struct_is_the_size_rust_uploads() {
    let fields = primitive_fields(SHADERS[0].1);
    assert!(!fields.is_empty(), "parsed no fields");

    let mut offset = 0usize;
    let mut struct_align = 1usize;
    for (name, ty) in &fields {
        let (size, align) = wgsl_size_align(ty);
        struct_align = struct_align.max(align);
        // Round up to this member's alignment, as WGSL does.
        let pad = (align - offset % align) % align;
        assert_eq!(
            pad, 0,
            "`{name}: {ty}` needs {pad} bytes of padding in WGSL that Rust \
             will not insert, because [f32; N] aligns to 4 and vec4 to 16. \
             Reorder so every member lands on its own alignment."
        );
        offset += size;
    }
    let wgsl_size = offset + (struct_align - offset % struct_align) % struct_align;

    assert_eq!(
        wgsl_size,
        std::mem::size_of::<blinc_gpu::GpuPrimitive>(),
        "WGSL `struct Primitive` is {} bytes and Rust `GpuPrimitive` is {}. \
         Every shader would read the wrong stride.",
        wgsl_size,
        std::mem::size_of::<blinc_gpu::GpuPrimitive>()
    );
}

/// Field names and types must line up one for one. Catches a reorder,
/// which keeps the size identical and corrupts everything.
#[test]
fn the_fields_line_up_with_the_rust_struct() {
    let wgsl = primitive_fields(SHADERS[0].1);
    let rust = rust_primitive_fields();

    assert_eq!(
        wgsl.len(),
        rust.len(),
        "WGSL declares {} fields, Rust has {}:\n  wgsl: {:?}\n  rust: {:?}",
        wgsl.len(),
        rust.len(),
        wgsl.iter().map(|(n, _)| n).collect::<Vec<_>>(),
        rust.iter().map(|(n, _)| n).collect::<Vec<_>>()
    );

    for (i, ((wn, wt), (rn, rt))) in wgsl.iter().zip(rust.iter()).enumerate() {
        assert_eq!(
            wn, rn,
            "field {i} is `{wn}` in WGSL and `{rn}` in Rust — a reorder \
             keeps the size and corrupts every read"
        );
        assert_eq!(
            expected_rust(wt),
            rt.as_str(),
            "field `{wn}` is `{wt}` in WGSL and `{rt}` in Rust"
        );
    }
}

/// Parsed from the source rather than hand-listed, so it cannot go stale.
fn rust_primitive_fields() -> Vec<(String, String)> {
    let src = include_str!("../src/primitives.rs");
    let start = src
        .find("pub struct GpuPrimitive {")
        .expect("no GpuPrimitive declaration");
    let body_start = src[start..].find('{').unwrap() + start + 1;
    let body_end = src[body_start..].find("\n}").unwrap() + body_start;

    let mut out = Vec::new();
    for line in src[body_start..body_end].lines() {
        let line = line.trim();
        if !line.starts_with("pub ") {
            continue;
        }
        let line = line.trim_start_matches("pub ").trim_end_matches(',');
        if let Some((name, ty)) = line.split_once(':') {
            out.push((name.trim().to_string(), ty.trim().to_string()));
        }
    }
    out
}
