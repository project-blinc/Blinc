//! A parse failure has to say where it is, in the source.
use blinc_dsl_core::BlincDsl;

#[test]
fn a_parse_error_renders_the_offending_line() {
    let dsl = BlincDsl::new().expect("dsl init");
    let src = "\
signal count: i32 = 0
signal bad: = 3
view {
    Div {}
}
";
    let err = dsl
        .compile_source(src, "broken.blinc")
        .expect_err("must not parse");
    let rendered = err.to_string();
    println!("{rendered}");

    assert!(
        rendered.contains("broken.blinc"),
        "the report names the file: {rendered}"
    );
    // A snippet box with a gutter and a caret, not a coordinate pair
    // the reader has to go and look up.
    assert!(
        rendered.contains('│'),
        "the report draws a snippet: {rendered}"
    );
    assert!(
        rendered.contains("signal bad: = 3"),
        "the offending line appears verbatim: {rendered}"
    );
    assert!(
        !rendered.contains("expected [\"end of input\"]"),
        "the raw PEG expectation is not shown to users: {rendered}"
    );
}

/// Compilation must fail, not abort. A `.blinc` file is unparseable for
/// most of the time it is being typed, and a hot-reloading host has to
/// survive every keystroke in between.
#[test]
fn an_unparseable_source_returns_rather_than_panicking() {
    let dsl = BlincDsl::new().expect("dsl init");
    for src in ["view {", "component {}", "fsm", "@@@", "signal x: = "] {
        assert!(
            dsl.compile_source(src, "junk.blinc").is_err(),
            "expected an error for {src:?}"
        );
    }
}
