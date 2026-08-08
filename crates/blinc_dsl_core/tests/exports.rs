//! `export { … }` — the names a host may reach.
//!
//! Today every signal is reachable by name, which is why nothing can be
//! made private: taking the host's grip away is exactly what scoping
//! would do, so the way to reach one has to exist first.
//!
//! The gate is deliberately permissive until a program states a surface.
//! A program with no `export` keeps every signal reachable, so the ~74
//! existing host call sites are unaffected; a program that exports
//! anything is held to what it declared.
use blinc_dsl_core::BlincDsl;

/// The export list is one process-global slot that each compile
/// replaces, so these tests cannot run concurrently — one would compile
/// over another's surface mid-assertion. Serialised here rather than
/// left to `--test-threads=1`, which nothing enforces.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn compile(src: &str, name: &str) -> BlincDsl {
    let dsl = BlincDsl::new().expect("runtime init");
    dsl.compile_source(src, name).expect("compile");
    dsl
}

/// The block parses and its names are collected.
#[test]
fn an_export_block_names_the_reachable_signals() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let _dsl = compile(
        r#"signal ex_a: i32 = 1
           signal ex_b: i32 = 2

           export { ex_a }

           view { Text("hi") }"#,
        "exports_one.blinc",
    );

    let exported = blinc_runtime::signal::exported();
    assert!(exported.contains(&"ex_a".to_string()), "{exported:?}");
}

/// Several names, comma separated, the way an author would write them.
#[test]
fn several_names_are_collected() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let _dsl = compile(
        r#"signal ex_x: i32 = 1
           signal ex_y: i32 = 2
           signal ex_z: i32 = 3

           export { ex_x, ex_y }

           view { Text("hi") }"#,
        "exports_many.blinc",
    );

    let exported = blinc_runtime::signal::exported();
    for name in ["ex_x", "ex_y"] {
        assert!(exported.contains(&name.to_string()), "{name}: {exported:?}");
    }
}

/// Declaring a surface holds the program to it: a signal left out is
/// not reachable, while one named is.
#[test]
fn the_gate_follows_what_was_declared() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let _dsl = compile(
        r#"signal ex_open: i32 = 1
           signal ex_shut: i32 = 2

           export { ex_open }

           view { Text("hi") }"#,
        "exports_gate.blinc",
    );

    assert!(blinc_runtime::signal::is_reachable("ex_open"));
    assert!(
        !blinc_runtime::signal::is_reachable("ex_shut"),
        "a signal the program did not export is not the host's to reach",
    );
}

/// A program that exports NOTHING keeps everything reachable, which is
/// what the existing host call sites rely on.
#[test]
fn no_export_block_leaves_everything_reachable() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    blinc_runtime::signal::set_exported(&[]);
    assert!(blinc_runtime::signal::is_reachable("anything_at_all"));
}
