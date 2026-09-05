//! CSS subset parser for ElementStyle
//!
//! Parses a simplified CSS syntax into ElementStyle objects, enabling
//! stylesheet-based styling for Blinc applications.
//!
//! # Error Handling
//!
//! This parser uses nom's context-based error capture for diagnostics.
//! All parse failures are collected into an error array that can be used
//! for reporting. Errors are also logged via tracing at DEBUG level.
//! The parser gracefully continues after errors - the built-in theme is
//! used when style parsing fails.
//!
//! # Supported Syntax
//!
//! - ID-based selectors: `#element-id { ... }` (matches `.id("element-id")`)
//! - Properties: `background`, `border-radius`, `box-shadow`, `transform`, `opacity`
//! - Theme references: `theme(primary)`, `theme(radius-lg)`, `theme(shadow-md)`
//! - Colors: hex (#rgb, #rrggbb, #rrggbbaa), rgb(), rgba(), named colors
//! - Units: px, %, unitless numbers
//!
//! # Example
//!
//! ```ignore
//! use blinc_layout::css_parser::{Stylesheet, ParseResult as CssParseResult};
//!
//! let css = r#"
//!     #card {
//!         background: theme(surface);
//!         border-radius: theme(radius-lg);
//!         box-shadow: theme(shadow-md);
//!     }
//!     #button-primary {
//!         background: theme(primary);
//!         opacity: 0.9;
//!     }
//! "#;
//!
//! let result = Stylesheet::parse_with_errors(css);
//! let stylesheet = result.stylesheet;
//!
//! // Report any errors that occurred
//! for err in &result.errors {
//!     eprintln!("Warning: {}", err);
//! }
//!
//! // Apply styles to elements
//! div().id("card").style(stylesheet.get("card").unwrap())
//! ```
//!
//! # Layout
//!
//! - [`error`] — diagnostics: severities, positions, and the accumulating
//!   result type that lets one bad rule cost only that rule.
//! - [`selector`] — the selector data model; `grammar::selector` parses it.
//! - [`keyframes`] / [`animation`] — `@keyframes` geometry and the
//!   `animation` / `transition` timing that drives it.
//! - [`stylesheet`] — the parsed sheet and the process-wide active slot.
//! - `grammar` — comments, declarations, rules, at-rule dispatch, and the
//!   sheet driver.
//! - `properties` — turning one declaration into a field on an
//!   [`ElementStyle`](crate::element_style::ElementStyle), silently or with
//!   diagnostics.
//! - [`value`] — one module per family of value: colors, gradients,
//!   lengths, transforms, shadows, filters, clip paths.
//! - [`flow`] — the `@flow` shader DAG: statements, semantic forms, and the
//!   expression grammar.
//! - `util` — paren-aware splitting shared by the value grammars.

use nom::{IResult, error::VerboseError};

pub mod animation;
pub mod error;
pub mod flow;
pub mod grammar;
pub mod keyframes;
pub mod properties;
pub mod selector;
pub mod stylesheet;
pub mod util;
pub mod value;

#[cfg(test)]
mod tests;

// Everything is re-exported flat. The submodules are an organizing device,
// not a second set of paths for callers to track: `parser::Stylesheet` and
// `parser::parse_clip_path` resolve as they always have. Items that were
// private to the original single file are re-exported at crate visibility
// only, so the split did not widen the public surface.
pub use animation::*;
pub use error::*;
pub use flow::*;
pub(crate) use grammar::*;
pub use keyframes::*;
pub(crate) use properties::*;
pub use selector::*;
pub use stylesheet::*;
pub(crate) use util::*;
pub use value::*;

/// Parser result carrying nom's `VerboseError` so a failure keeps the
/// context stack that [`error::ParseError`] reports from.
pub(crate) type ParseResult<'a, O> = IResult<&'a str, O, VerboseError<&'a str>>;
