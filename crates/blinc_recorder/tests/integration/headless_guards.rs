//! Every test that drives a `Headless` checks for a GPU adapter first, so a
//! machine without one (the coverage runner) skips instead of panicking.

use std::fs;
use std::path::{Path, PathBuf};

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read dir") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// The `#[test]` functions of `text` with their bodies, by brace matching.
fn tests(text: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("#[test]") {
        rest = &rest[at + "#[test]".len()..];
        let Some(fn_at) = rest.find("fn ") else { break };
        let name: String = rest[fn_at + 3..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        let Some(open) = rest[fn_at..].find('{') else {
            break;
        };
        let body_start = fn_at + open;
        let mut depth = 0;
        let mut end = body_start;
        for (i, c) in rest[body_start..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = body_start + i;
                        break;
                    }
                }
                _ => {}
            }
        }
        found.push((name, rest[body_start..end].to_owned()));
        rest = &rest[end..];
    }
    found
}

#[test]
fn tests_that_drive_a_headless_check_for_an_adapter() {
    let mut files = Vec::new();
    rust_files(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests"),
        &mut files,
    );
    let mut unguarded = Vec::new();
    for path in files {
        if path.ends_with("headless_guards.rs") {
            continue;
        }
        let text = fs::read_to_string(&path).expect("read test");
        if !text.contains("Headless") {
            continue;
        }
        for (name, body) in tests(&text) {
            let drives = [
                "session",
                "scene(",
                "open(",
                "mount(",
                "Headless::new",
                "gallery(",
            ]
            .iter()
            .any(|w| body.contains(w));
            let guarded = body.contains("Headless::available") || body.contains("else {");
            if drives && !guarded {
                unguarded.push(format!(
                    "{}::{name}",
                    path.file_name().unwrap().to_string_lossy()
                ));
            }
        }
    }
    assert!(
        unguarded.is_empty(),
        "these tests drive a Headless without checking Headless::available(): {unguarded:?}"
    );
}
