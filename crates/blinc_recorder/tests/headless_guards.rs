//! Every test that builds a `Headless` checks for a GPU adapter first, so a
//! machine without one (the coverage runner) skips instead of panicking.

use std::fs;
use std::path::Path;

#[test]
fn tests_that_build_a_headless_check_for_an_adapter() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut unguarded = Vec::new();
    for entry in fs::read_dir(&dir).expect("tests dir") {
        let path = entry.expect("entry").path();
        if path.extension().is_none_or(|e| e != "rs") || path.ends_with("headless_guards.rs") {
            continue;
        }
        let text = fs::read_to_string(&path).expect("read test");
        if text.contains("Headless::new") && !text.contains("Headless::available") {
            unguarded.push(path.file_name().unwrap().to_string_lossy().into_owned());
        }
    }
    assert!(
        unguarded.is_empty(),
        "these tests build a Headless without Headless::available(): {unguarded:?}"
    );
}
