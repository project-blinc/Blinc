//! SVG loading and rendering for Blinc
//!
//! This crate provides SVG file loading and conversion to Blinc drawing primitives.
//! It uses `usvg` for parsing and simplification of SVG files.
//!
//! # Rendering Modes
//!
//! ## Tessellation (Legacy)
//! Converts SVG paths to triangles via Lyon tessellation. Fast but produces
//! aliased edges on thin strokes.
//!
//! ## Rasterization (Recommended)
//! Uses `resvg` for high-quality CPU rasterization with proper anti-aliasing.
//! Produces pixel-perfect output that can be uploaded as GPU textures.
//!
//! # Example
//!
//! ```ignore
//! use blinc_svg::{SvgDocument, RasterizedSvg};
//!
//! // Tessellation mode (legacy)
//! let svg = SvgDocument::from_file("icon.svg")?;
//! ctx.draw_svg(&svg, Point::new(10.0, 10.0), 1.0);
//!
//! // Rasterization mode (recommended for icons)
//! let rasterized = RasterizedSvg::from_str(svg_str, 64, 64)?;
//! // Upload rasterized.data() to GPU texture
//! ```

mod document;
mod error;
mod path;
mod rasterize;
mod style;

pub use document::{SvgDocument, SvgDrawCommand};
pub use error::SvgError;
pub use rasterize::RasterizedSvg;
