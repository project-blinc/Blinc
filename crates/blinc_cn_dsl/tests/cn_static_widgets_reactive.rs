//! The DSL surface for the newly-bindable props on `cn.Kbd` and
//! `cn.Avatar`.
//!
//! Compile-level coverage: a regression in the macro's two-slot FFI,
//! the `lower_reactive_args` pass, or the `Reactive<T>` decoder shows
//! up here as a compile error rather than a silent no-op at runtime.
use blinc_dsl_core::BlincDsl;

fn dsl() -> BlincDsl {
    let dsl = BlincDsl::new().expect("dsl init");
    blinc_cn_dsl::register_all(&dsl).expect("register cn.* widgets");
    dsl
}

#[test]
fn kbd_text_accepts_a_literal_a_signal_and_a_computed() {
    let d = dsl();
    d.compile_source(r#"view { cn.Kbd("Ctrl") }"#, "kbd_literal.blinc")
        .expect("literal");

    let d = dsl();
    d.compile_source(
        "signal key: string = \"K\"\nview { cn.Kbd(text = key) }",
        "kbd_signal.blinc",
    )
    .expect("signal");

    let d = dsl();
    d.compile_source(
        "signal key: string = \"K\"\nview { cn.Kbd(text = computed { key.get() } : string) }",
        "kbd_computed.blinc",
    )
    .expect("computed");
}

#[test]
fn avatar_fallback_and_src_accept_signals() {
    let d = dsl();
    d.compile_source(
        "signal initials: string = \"AB\"\nview { cn.Avatar(fallback = initials) }",
        "avatar_fallback.blinc",
    )
    .expect("bound fallback");

    let d = dsl();
    d.compile_source(
        "signal url: string = \"a.png\"\nview { cn.Avatar(src = url) }",
        "avatar_src.blinc",
    )
    .expect("bound src");
}

/// An omitted prop arrives as an empty literal, and `src` / `fallback`
/// choose different content, so an empty one has to read as absent.
#[test]
fn an_avatar_with_no_props_still_builds() {
    let d = dsl();
    d.compile_source("view { cn.Avatar() }", "avatar_bare.blinc")
        .expect("bare avatar");
    let _ = d.view_widget();
}
