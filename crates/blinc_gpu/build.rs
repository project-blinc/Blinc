//! Composes each SDF shader from the shared prelude plus its own body,
//! writing the result to `OUT_DIR` for `include_str!`.
//!
//! WGSL has no `#include`. The helpers in `shaders/common/sdf_common.wgsl`
//! were copy-pasted into every `sdf_*.wgsl`, so a fix had to be applied by
//! hand to nine or twelve files (see 2ad76a9, 178a63b).
//!
//! Only the helpers a shader actually reaches are emitted. Pasting the
//! whole prelude into all twelve would trade 547 KiB of duplicated source
//! for 775 KiB of shader text in the binary; subsetting keeps both down.
//!
//! The prelude goes first, so nothing in it may reference a binding — a
//! shader declares those afterwards. Keeping it first also means a
//! shader's own lines shift by a fixed amount in any compiler error.
//!
//! Both rules are asserted by `tests/shader_prelude.rs`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const PRELUDE: &str = "src/shaders/common/sdf_common.wgsl";

/// Split WGSL into top-level `fn` definitions, keyed by name, each with
/// any comment block directly above it.
fn split_functions(src: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let bytes = src.as_bytes();
    let mut i = 0usize;
    while let Some(rel) = src[i..].find("\nfn ") {
        let start = i + rel + 1;
        let name_start = start + 3;
        let name_end = match src[name_start..].find('(') {
            Some(p) => name_start + p,
            None => break,
        };
        let name = src[name_start..name_end].trim().to_string();

        // Walk braces to the end of the body.
        let Some(open) = src[name_end..].find('{').map(|p| name_end + p) else {
            break;
        };
        let mut depth = 0i32;
        let mut end = open;
        while end < bytes.len() {
            match bytes[end] {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            end += 1;
        }

        // Absorb the comment lines immediately above.
        let mut head = start;
        while head > 0 {
            let prev = src[..head - 1].rfind('\n').map(|p| p + 1).unwrap_or(0);
            if src[prev..head - 1].trim_start().starts_with("//") {
                head = prev;
            } else {
                break;
            }
        }
        out.insert(name, src[head..=end].to_string());
        i = end + 1;
    }
    out
}

/// Whether `haystack` names `ident` as a whole word.
fn mentions(haystack: &str, ident: &str) -> bool {
    let mut from = 0;
    while let Some(rel) = haystack[from..].find(ident) {
        let at = from + rel;
        let before_ok = at == 0
            || !haystack.as_bytes()[at - 1].is_ascii_alphanumeric()
                && haystack.as_bytes()[at - 1] != b'_';
        let after = at + ident.len();
        let after_ok = after >= haystack.len()
            || !haystack.as_bytes()[after].is_ascii_alphanumeric()
                && haystack.as_bytes()[after] != b'_';
        if before_ok && after_ok {
            return true;
        }
        from = at + 1;
    }
    false
}

fn main() {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let shaders = Path::new("src/shaders");

    println!("cargo:rerun-if-changed={PRELUDE}");
    let prelude_src =
        std::fs::read_to_string(PRELUDE).unwrap_or_else(|e| panic!("cannot read {PRELUDE}: {e}"));
    let helpers = split_functions(&prelude_src);
    assert!(!helpers.is_empty(), "parsed no helpers out of {PRELUDE}");

    // Declaration order in the prelude is topological, so preserve it.
    let order: Vec<String> = {
        let mut v: Vec<(usize, String)> = helpers
            .keys()
            .map(|n| {
                (
                    prelude_src.find(&format!("\nfn {n}(")).unwrap_or(0),
                    n.clone(),
                )
            })
            .collect();
        v.sort();
        v.into_iter().map(|(_, n)| n).collect()
    };

    let mut paths: Vec<PathBuf> = std::fs::read_dir(shaders)
        .expect("src/shaders")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.extension().is_some_and(|x| x == "wgsl")
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("sdf_"))
        })
        .collect();
    paths.sort();
    assert!(
        !paths.is_empty(),
        "no sdf_*.wgsl found — did src/shaders move?"
    );

    for path in paths {
        let name = path.file_name().unwrap().to_str().unwrap().to_string();
        println!("cargo:rerun-if-changed={}", path.display());
        let body = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));

        // Everything the shader reaches, transitively through the prelude.
        let mut needed: BTreeSet<String> = order
            .iter()
            .filter(|n| mentions(&body, n))
            .cloned()
            .collect();
        loop {
            let mut added = false;
            for n in needed.clone() {
                let def = &helpers[&n];
                for m in &order {
                    if !needed.contains(m) && m != &n && mentions(def, m) {
                        needed.insert(m.clone());
                        added = true;
                    }
                }
            }
            if !added {
                break;
            }
        }

        let used: Vec<&str> = order
            .iter()
            .filter(|n| needed.contains(*n))
            .map(String::as_str)
            .collect();
        let prelude: String = used
            .iter()
            .map(|n| helpers[*n].trim_end())
            .collect::<Vec<_>>()
            .join("\n\n");

        let out = format!(
            "// GENERATED by build.rs — {} of {} shared helpers, then\n\
             // src/shaders/{name}. Edit those, not this.\n\n{prelude}\n\n{body}",
            used.len(),
            order.len()
        );
        std::fs::write(out_dir.join(&name), out)
            .unwrap_or_else(|e| panic!("cannot write {name}: {e}"));
    }
}
