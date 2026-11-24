//! Error types for blinc_image

use thiserror::Error;

/// Result type for blinc_image operations
pub type Result<T> = std::result::Result<T, ImageError>;

/// Errors that can occur during image operations
#[derive(Debug, Error)]
pub enum ImageError {
    /// Failed to load image from file
    #[error("Failed to load image from file: {0}")]
    FileLoad(String),

    /// Failed to decode image data
    #[error("Failed to decode image: {0}")]
    Decode(String),

    /// Invalid base64 data
    #[error("Invalid base64 data: {0}")]
    Base64(String),

    /// Network error (URL loading)
    #[error("Network error: {0}")]
    Network(String),

    /// Invalid image source
    #[error("Invalid image source: {0}")]
    InvalidSource(String),

    /// Unsupported image format
    #[error("Unsupported image format: {0}")]
    UnsupportedFormat(String),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(any(
    feature = "gif",
    feature = "webp",
    feature = "bmp",
    feature = "tiff",
    feature = "avif"
))]
impl From<image::ImageError> for ImageError {
    fn from(err: image::ImageError) -> Self {
        ImageError::Decode(err.to_string())
    }
}

impl From<base64::DecodeError> for ImageError {
    fn from(err: base64::DecodeError) -> Self {
        ImageError::Base64(err.to_string())
    }
}
