//! SVG loading and rendering for Blinc
//!
//! This crate provides SVG file loading and conversion to Blinc drawing primitives.
//! It uses `usvg` for parsing and simplification of SVG files.
//!
//! # Example
//!
//! ```ignore
//! use blinc_svg::SvgDocument;
//!
//! let svg = SvgDocument::from_file("icon.svg")?;
//! ctx.draw_svg(&svg, Point::new(10.0, 10.0), 1.0);
//! ```

mod document;
mod error;
mod path;
mod style;

pub use document::{SvgDocument, SvgDrawCommand};
pub use error::SvgError;
