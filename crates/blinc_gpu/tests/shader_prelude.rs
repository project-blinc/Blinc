//! Guards the two rules the shared shader prelude depends on.
//!
//! `build.rs` prepends `shaders/common/sdf_common.wgsl` to each
//! `sdf_*.wgsl` before the shader declares its bindings. Break either
//! rule below and twelve shaders stop compiling at once, so it is worth
//! failing here with a sentence instead.

const PRELUDE: &str = include_str!("../src/shaders/common/sdf_common.wgsl");

/// Every binding name any SDF shader declares.
const BINDINGS: &[&str] = &[
    "primitives",
    "aux_data",
    "aux_tex",
    "uniforms",
    "prim_data",
    "glyph_atlas",
    "color_glyph_atlas",
    "glyph_sampler",
];

fn code_only(src: &str) -> String {
    src.lines()
        .map(|l| l.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The prelude comes first, so a binding is not in scope yet. A helper
/// that needs one belongs in the shader that declares it — which is why
/// `sdf_3d_eval` and the `eval_group_*` family stayed behind.
#[test]
fn the_prelude_touches_no_binding() {
    let code = code_only(PRELUDE);
    for name in BINDINGS {
        for (i, line) in code.lines().enumerate() {
            let hit = line
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .any(|w| w == *name);
            assert!(
                !hit,
                "sdf_common.wgsl:{} references the binding `{name}`, which is \
                 not in scope where the prelude is prepended:\n  {}",
                i + 1,
                line.trim()
            );
        }
    }
}

/// A helper must appear after everything it calls.
///
/// `build.rs` preserves this file's order when it subsets, so a callee
/// placed later is emitted later and the composed output ends up with a
/// forward reference. naga tolerated one in a shader that happened not to
/// include the caller, which is exactly the kind of accident this avoids.
#[test]
fn the_prelude_is_declared_in_dependency_order() {
    let code = code_only(PRELUDE);
    let names: Vec<(usize, String)> = code
        .lines()
        .enumerate()
        .filter_map(|(i, l)| {
            l.strip_prefix("fn ")
                .and_then(|r| r.split('(').next())
                .map(|n| (i, n.trim().to_string()))
        })
        .collect();
    assert!(names.len() > 20, "parsed only {} helpers", names.len());

    let at: std::collections::HashMap<&str, usize> =
        names.iter().map(|(i, n)| (n.as_str(), *i)).collect();

    // Walk each body and check every call resolves to something earlier.
    let lines: Vec<&str> = code.lines().collect();
    for (idx, (start, name)) in names.iter().enumerate() {
        let end = names.get(idx + 1).map(|(i, _)| *i).unwrap_or(lines.len());
        for line in &lines[*start..end] {
            for word in line.split(|c: char| !c.is_alphanumeric() && c != '_') {
                if word == name {
                    continue;
                }
                if let Some(&other) = at.get(word) {
                    assert!(
                        other < *start,
                        "`{name}` calls `{word}`, which is declared later, so the \
                         composed shader would reference it before it is \
                         declared. Move `{word}` up."
                    );
                }
            }
        }
    }
}
