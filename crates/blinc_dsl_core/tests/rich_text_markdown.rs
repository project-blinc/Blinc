//! `RichText` / `Markdown` — core widgets declared with
//! `#[extern_widget]` rather than three hand-written declarations.
use blinc_dsl_core::BlincDsl;

fn compiles(src: &str) -> Result<(), String> {
    let dsl = BlincDsl::new().expect("runtime init");
    dsl.compile_source(src, "core_widgets.blinc")
        .map(|_| ())
        .map_err(|e| format!("{e}"))
}

/// Both are available with no `register_*` call, in every arg form the
/// macro derives: positional, named, and with the extra props the old
/// hand-written ABI could not reach.
#[test]
fn the_core_widgets_are_registered_by_default() {
    let cases: &[(&str, &str)] = &[
        (
            "rich positional",
            "view { Div { RichText(\"a <b>b</b>\") } }",
        ),
        (
            "rich named",
            "view { Div { RichText(markup = \"a <b>b</b>\") } }",
        ),
        (
            "rich with props",
            "view { Div { RichText(\"x\", size = 18.0, align = \"center\") } }",
        ),
        ("markdown positional", "view { Div { Markdown(\"## H\") } }"),
        (
            "markdown named",
            "view { Div { Markdown(source = \"## H\") } }",
        ),
    ];
    for (what, src) in cases {
        assert!(
            compiles(src).is_ok(),
            "{what} should compile: {:?}",
            compiles(src)
        );
    }
}

/// A bound signal is accepted where the old ABI took only a literal
/// string — the reactive prop the macro derives.
#[test]
fn markup_and_source_accept_a_bound_signal() {
    let src = "signal note: string = \"state: <b>ready</b>\"
               signal doc: string = \"## Heading\"
               view {
                 Div {
                   RichText(note)
                   Markdown(doc)
                 }
               }";
    assert!(compiles(src).is_ok(), "bound markup and source compile");
}

/// An unknown align warns and falls back rather than failing the build.
#[test]
fn an_unknown_align_is_tolerated() {
    let src = "view { Div { RichText(\"x\", align = \"sideways\") } }";
    assert!(compiles(src).is_ok(), "unknown align warns, does not fail");
}
