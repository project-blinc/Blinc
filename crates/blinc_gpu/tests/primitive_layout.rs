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

use naga::{Expression as E, Literal, ScalarKind, Statement, TypeInner};
use std::collections::{BTreeMap, BTreeSet};

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

#[test]
fn data_texture_rows_hold_every_primitive_field() {
    let texels = blinc_gpu::GpuPrimitive::DATA_TEXTURE_TEXELS as usize;
    assert_eq!(texels, rust_primitive_fields().len());
    assert_eq!(
        texels * std::mem::size_of::<[f32; 4]>(),
        std::mem::size_of::<blinc_gpu::GpuPrimitive>()
    );
}

#[test]
fn every_data_texture_loader_reads_the_structs_columns_and_types() {
    for (name, source) in SHADERS.iter().filter(|(name, _)| name.ends_with("_dt")) {
        let result = std::panic::catch_unwind(|| check_data_texture_loader(source));
        assert!(result.is_ok(), "{name}: primitive texture reader diverged");
    }
}

fn wgsl_definition(src: &str, marker: &str) -> String {
    let start = src.find(marker).unwrap();
    let end = src[start..].find("\n}").unwrap() + start + 2;
    src[start..end].to_string()
}
fn check_data_texture_loader(src: &str) {
    let source = format!(
        "{}\n@group(0) @binding(1) var prim_data: texture_2d<f32>;\n{}",
        wgsl_definition(src, "struct Primitive {"),
        wgsl_definition(src, "fn load_primitive(")
    );
    let module = naga::front::wgsl::parse_str(&source)
        .map_err(|e| e.emit_to_string(&source))
        .unwrap();
    let (_, f) = module
        .functions
        .iter()
        .find(|(_, f)| f.name.as_deref() == Some("load_primitive"))
        .unwrap();
    let (p, variable) = f
        .local_variables
        .iter()
        .find(|(_, v)| module.types[v.ty].name.as_deref() == Some("Primitive"))
        .unwrap();
    let TypeInner::Struct { members, .. } = &module.types[variable.ty].inner else {
        panic!("primitive struct")
    };
    let mut seen = BTreeSet::new();
    for statement in f.body.iter() {
        let Statement::Store { pointer, value } = *statement else {
            continue;
        };
        let E::AccessIndex { base, index } = f.expressions[pointer] else {
            panic!("field store")
        };
        assert!(matches!(f.expressions[base], E::LocalVariable(v) if v == p));
        assert!(seen.insert(index), "duplicate field {index}");
        let member = &members[index as usize];
        let TypeInner::Vector { scalar, .. } = module.types[member.ty].inner else {
            panic!("vector field")
        };
        let value = if scalar.kind == ScalarKind::Uint {
            let E::As {
                expr,
                kind: ScalarKind::Uint,
                convert: None,
            } = f.expressions[value]
            else {
                panic!("integer bitcast")
            };
            expr
        } else {
            value
        };
        let E::ImageLoad {
            image,
            coordinate,
            level: Some(level),
            ..
        } = f.expressions[value]
        else {
            panic!("texture load")
        };
        let E::GlobalVariable(image) = f.expressions[image] else {
            panic!("global texture")
        };
        assert_eq!(
            module.global_variables[image].name.as_deref(),
            Some("prim_data")
        );
        assert!(matches!(f.expressions[level], E::Literal(Literal::I32(0))));
        let E::Compose { ref components, .. } = f.expressions[coordinate] else {
            panic!("texel coordinate")
        };
        assert_eq!(components.len(), 2);
        assert!(
            matches!(f.expressions[components[0]], E::Literal(Literal::I32(column)) if column == index as i32),
            "{} must load column {index}",
            member.name.as_deref().unwrap()
        );
        let E::As {
            expr,
            kind: ScalarKind::Sint,
            convert: Some(4),
        } = f.expressions[components[1]]
        else {
            panic!("primitive row")
        };
        assert!(matches!(f.expressions[expr], E::FunctionArgument(0)));
    }
    assert_eq!(
        seen.len(),
        members.len(),
        "every primitive field must be loaded"
    );
    let returns_primitive = f.body.iter().any(|statement| {
        let Statement::Return { value: Some(value) } = *statement else {
            return false;
        };
        let E::Load { pointer } = f.expressions[value] else {
            return false;
        };
        matches!(f.expressions[pointer], E::LocalVariable(variable) if variable == p)
    });
    assert!(returns_primitive, "return the loaded primitive");
}
