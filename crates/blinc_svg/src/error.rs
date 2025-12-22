//! SVG error types

use std::io;
use thiserror::Error;

/// Errors that can occur when loading or rendering SVG files
#[derive(Error, Debug)]
pub enum SvgError {
    /// IO error when reading the file
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    /// SVG parsing error
    #[error("SVG parsing error: {0}")]
    Parse(String),

    /// Unsupported SVG feature
    #[error("Unsupported SVG feature: {0}")]
    Unsupported(String),
}
