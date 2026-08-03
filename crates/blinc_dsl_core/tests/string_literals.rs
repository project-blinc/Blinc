//! String literals whose content ends in whitespace.
use blinc_dsl_core::BlincDsl;

/// A string ending in whitespace must parse, wherever it appears.
///
/// It did not. `string_literal` is atomic but `string_inner` was not,
/// and the engine does not inherit atomicity into a called rule — so
/// inside `string_inner` the sequence `!("\"" | "\\") ~ ANY` skipped
/// implicit whitespace before `ANY`. Given `", "` that consumed the
/// space and then the CLOSING QUOTE, leaving the string unterminated
/// and the parser reading to end of input.
///
/// It only showed when the character after the whitespace was the
/// closing quote: `", x"` parsed, `", "` did not.
#[test]
fn a_string_may_end_in_whitespace() {
    let cases = [
        (r#"view { Div { Text(", ") } }"#, "comma then space"),
        (r#"view { Div { Text(",  ") } }"#, "comma then two spaces"),
        (r#"view { Div { Text("and ") } }"#, "word then space"),
        (r#"view { Div { Text(" ") } }"#, "a lone space"),
        ("view { Div { Text(\"x\t\") } }", "trailing tab"),
    ];
    for (src, what) in cases {
        let dsl = BlincDsl::new().expect("runtime init");
        assert!(
            dsl.compile_source(src, "strings.blinc").is_ok(),
            "{what} should parse",
        );
    }
}

/// The neighbouring shapes the fix must not disturb.
#[test]
fn ordinary_strings_still_parse() {
    let cases = [
        r#"view { Div { Text("a, b") } }"#,
        r#"view { Div { Text("a\"b") } }"#,
        r#"view { Div { Text("") } }"#,
        r#"signal s: string = ", "
           view { Div { Text(s) } }"#,
    ];
    for src in cases {
        let dsl = BlincDsl::new().expect("runtime init");
        assert!(dsl.compile_source(src, "strings.blinc").is_ok(), "{src}");
    }
}
