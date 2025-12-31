//! Image loading and data management

use crate::error::{ImageError, Result};
use crate::source::ImageSource;
use base64::Engine;
use image::{DynamicImage, GenericImageView};

/// Decoded image data ready for GPU upload
#[derive(Debug, Clone)]
pub struct ImageData {
    /// Raw RGBA pixel data
    pixels: Vec<u8>,
    /// Image width in pixels
    width: u32,
    /// Image height in pixels
    height: u32,
}

impl ImageData {
    /// Create ImageData from raw RGBA pixels
    pub fn from_rgba(pixels: Vec<u8>, width: u32, height: u32) -> Result<Self> {
        let expected_len = (width * height * 4) as usize;
        if pixels.len() != expected_len {
            return Err(ImageError::Decode(format!(
                "Invalid pixel data length: expected {}, got {}",
                expected_len,
                pixels.len()
            )));
        }
        Ok(Self {
            pixels,
            width,
            height,
        })
    }

    /// Load an image from a source (synchronous)
    ///
    /// This method uses the platform asset loader when available (via the "platform" feature),
    /// falling back to direct filesystem access on desktop.
    ///
    /// Note: URL sources require the "network" feature and will fail
    /// without it. Use `load_async` for URL sources.
    pub fn load(source: ImageSource) -> Result<Self> {
        match source {
            ImageSource::File(path) => {
                // Try platform asset loader first (works cross-platform)
                #[cfg(feature = "platform")]
                {
                    if let Some(loader) = blinc_platform::assets::global_asset_loader() {
                        let asset_path =
                            blinc_platform::AssetPath::from(path.to_string_lossy().to_string());
                        match loader.load(&asset_path) {
                            Ok(data) => return Self::from_bytes(&data),
                            Err(e) => {
                                tracing::debug!(
                                    "Platform asset loader failed, trying filesystem: {}",
                                    e
                                );
                            }
                        }
                    }
                }

                // Fallback to direct filesystem access (desktop only)
                let data = std::fs::read(&path)
                    .map_err(|e| ImageError::FileLoad(format!("{}: {}", path.display(), e)))?;
                Self::from_bytes(&data)
            }

            ImageSource::Base64(data) => Self::from_base64(&data),

            ImageSource::Bytes { data, format: _ } => Self::from_bytes(&data),

            ImageSource::Url(_url) => {
                #[cfg(feature = "network")]
                {
                    // For sync loading, we use blocking
                    Err(ImageError::Network(
                        "Use load_async for URL sources, or use blocking runtime".to_string(),
                    ))
                }
                #[cfg(not(feature = "network"))]
                {
                    Err(ImageError::Network(
                        "URL loading requires the 'network' feature".to_string(),
                    ))
                }
            }

            ImageSource::Emoji { emoji, size } => {
                #[cfg(feature = "emoji")]
                {
                    Self::load_emoji(&emoji, size)
                }
                #[cfg(not(feature = "emoji"))]
                {
                    let _ = (emoji, size);
                    Err(ImageError::Decode(
                        "Emoji loading requires the 'emoji' feature".to_string(),
                    ))
                }
            }

            ImageSource::Rgba {
                data,
                width,
                height,
            } => Self::from_rgba(data, width, height),
        }
    }

    /// Load an emoji character as an image
    ///
    /// Uses the system emoji font to render the emoji as an RGBA image.
    /// Uses the global shared font registry to avoid loading the 180MB emoji font multiple times.
    #[cfg(feature = "emoji")]
    fn load_emoji(emoji: &str, size: f32) -> Result<Self> {
        use blinc_text::EmojiRenderer;

        // EmojiRenderer::new() uses the global shared font registry
        let mut renderer = EmojiRenderer::new();
        let sprite = renderer
            .render_string(emoji, size)
            .map_err(|e| ImageError::Decode(format!("Failed to render emoji: {:?}", e)))?;

        Self::from_rgba(sprite.data, sprite.width, sprite.height)
    }

    /// Load an image from a path using the platform asset loader
    ///
    /// This is the preferred way to load images in cross-platform code.
    /// On desktop, paths are filesystem paths. On Android, paths refer to
    /// assets in the APK. On iOS, paths refer to app bundle resources.
    #[cfg(feature = "platform")]
    pub fn load_asset(path: impl Into<String>) -> Result<Self> {
        let path_str = path.into();
        let asset_path = blinc_platform::AssetPath::from(path_str.clone());

        let loader = blinc_platform::assets::global_asset_loader()
            .ok_or_else(|| ImageError::FileLoad("No asset loader configured".to_string()))?;

        let data = loader
            .load(&asset_path)
            .map_err(|e| ImageError::FileLoad(format!("{}: {}", path_str, e)))?;

        Self::from_bytes(&data)
    }

    /// Load an image from a source (asynchronous)
    #[cfg(feature = "network")]
    pub async fn load_async(source: ImageSource) -> Result<Self> {
        match source {
            ImageSource::Url(url) => {
                let response = reqwest::get(&url)
                    .await
                    .map_err(|e| ImageError::Network(e.to_string()))?;

                if !response.status().is_success() {
                    return Err(ImageError::Network(format!(
                        "HTTP error: {}",
                        response.status()
                    )));
                }

                let bytes = response
                    .bytes()
                    .await
                    .map_err(|e| ImageError::Network(e.to_string()))?;

                Self::from_bytes(&bytes)
            }

            // For non-URL sources, delegate to sync load
            other => Self::load(other),
        }
    }

    /// Decode image from raw bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        let img = image::load_from_memory(data)?;
        Self::from_dynamic_image(img)
    }

    /// Decode image from base64 string
    ///
    /// Supports both plain base64 and data URIs like:
    /// - `iVBORw0KGgo...` (plain base64)
    /// - `data:image/png;base64,iVBORw0KGgo...` (data URI)
    pub fn from_base64(data: &str) -> Result<Self> {
        // Handle data URI format
        let base64_data = if data.starts_with("data:") {
            // Find the base64 marker
            data.find(";base64,")
                .map(|pos| &data[pos + 8..])
                .ok_or_else(|| ImageError::Base64("Invalid data URI format".to_string()))?
        } else {
            data
        };

        // Decode base64
        let bytes = base64::engine::general_purpose::STANDARD.decode(base64_data)?;
        Self::from_bytes(&bytes)
    }

    /// Convert a DynamicImage to ImageData
    fn from_dynamic_image(img: DynamicImage) -> Result<Self> {
        let (width, height) = img.dimensions();
        let rgba = img.to_rgba8();
        let pixels = rgba.into_raw();

        Ok(Self {
            pixels,
            width,
            height,
        })
    }

    /// Get the raw RGBA pixel data
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Get the image width
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Get the image height
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Get image dimensions as (width, height)
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Get the aspect ratio (width / height)
    pub fn aspect_ratio(&self) -> f32 {
        self.width as f32 / self.height as f32
    }

    /// Get the number of bytes in the pixel data
    pub fn byte_len(&self) -> usize {
        self.pixels.len()
    }

    /// Take ownership of the pixel data
    pub fn into_pixels(self) -> Vec<u8> {
        self.pixels
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_rgba() {
        // Create a 2x2 red image
        let pixels = vec![
            255, 0, 0, 255, // Red
            255, 0, 0, 255, // Red
            255, 0, 0, 255, // Red
            255, 0, 0, 255, // Red
        ];

        let data = ImageData::from_rgba(pixels, 2, 2).unwrap();
        assert_eq!(data.width(), 2);
        assert_eq!(data.height(), 2);
        assert_eq!(data.byte_len(), 16);
    }

    #[test]
    fn test_invalid_rgba_length() {
        let pixels = vec![255, 0, 0, 255]; // Only 1 pixel for 2x2
        let result = ImageData::from_rgba(pixels, 2, 2);
        assert!(result.is_err());
    }

    #[test]
    fn test_base64_data_uri() {
        // 1x1 red PNG as base64
        let data_uri = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8DwHwAFBQIAX8jx0gAAAABJRU5ErkJggg==";
        let result = ImageData::from_base64(data_uri);
        assert!(result.is_ok());
        let img = result.unwrap();
        assert_eq!(img.width(), 1);
        assert_eq!(img.height(), 1);
    }
}
