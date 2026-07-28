//! A parse failure has to say where it is, in the source, in terms the
//! person writing `.blinc` can act on.
use blinc_dsl_core::BlincDsl;

fn error_for(src: &str) -> String {
    let dsl = BlincDsl::new().expect("dsl init");
    let err = dsl
        .compile_source(src, "broken.blinc")
        .expect_err("must not parse");
    let rendered = err.to_string();
    println!("{rendered}");
    rendered
}

#[test]
fn a_parse_error_renders_the_offending_line() {
    let rendered = error_for(
        "\
signal count: i32 = 0
signal bad: = 3
view {
    Div {}
}
",
    );

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
    // The failure is the missing type, so the types are what to expect.
    assert!(
        rendered.contains("i32"),
        "the alternatives are named: {rendered}"
    );
}

/// The furthest position any attempt reached, not where the outermost
/// rule gave up.
///
/// A PEG's top-level alternation fails at the START of the item it
/// cannot consume, so a missing brace deep inside a component pointed
/// the caret at the `component` keyword — reading as "the keyword isn't
/// recognised" when the keyword was fine.
#[test]
fn an_unclosed_delimiter_points_past_the_declaration_keyword() {
    let rendered = error_for(
        "\
component MediaWidgets {
    view {
        Div {
    }
}
",
    );
    assert!(
        rendered.contains("end of input"),
        "running out of input is named as such: {rendered}"
    );
    assert!(
        !rendered.contains("component MediaWidgets"),
        "the caret is not parked on the declaration keyword: {rendered}"
    );
}

/// The expectation list is the grammar the user could have written, not
/// the parser's bookkeeping.
#[test]
fn the_expectation_list_hides_parser_internals() {
    let rendered = error_for("view {\n    Div(class = ) {\n    }\n}\n");
    for noise in ["memoized", "expected at least", "any character"] {
        assert!(
            !rendered.contains(noise),
            "{noise:?} is internal and must not surface: {rendered}"
        );
    }
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
