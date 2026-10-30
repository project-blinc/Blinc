//! The `css!` macro: a [`Stylesheet`](crate::parser::Stylesheet) written as CSS.

/// A stylesheet of class, id and pseudo-class rules, written as CSS text.
///
/// The argument is a string literal because CSS is not Rust token syntax: a
/// hex colour such as `#1a2b3c` or a value such as `1.5px` does not survive
/// the Rust lexer as it is written. Use a raw string.
///
/// Panics if the text does not parse, so a malformed stylesheet fails the
/// first time its component is built.
///
/// ```ignore
/// let sheet = css!(r#"
///     .card { background: #1a2b3c; border-radius: 8px; }
///     .card:hover { background: #24384d; }
///     .card-title { font-size: 14px; }
/// "#);
/// ```
///
/// For a single inline style written in Rust values, use `style!` or
/// `css_style!`.
#[macro_export]
macro_rules! css {
    ($text:expr) => {
        $crate::parser::Stylesheet::parse($text).expect("css!: invalid stylesheet")
    };
}
