//! Pluggable image decoders.
//!
//! `blinc_image` ships per-format built-in decoders behind cargo
//! features (`png`, `jpeg`, `gif`, `webp`, `bmp`, `tiff`, `avif`).
//! Apps that need a different decoder — `zune-image`, `libvips`, a
//! WebGPU compute decoder, a no-op stub for headless tests — register
//! their own [`ImageDecoder`] on a [`DecoderRegistry`] and the loader
//! picks it up.
//!
//! # Default registry
//!
//! `ImageData::from_bytes` consults a process-wide registry routed
//! through [`decode_with_global_registry`]. The first call
//! materialises it via [`DecoderRegistry::with_builtins`], registering
//! every decoder whose feature flag is enabled at compile time. Apps
//! that want non-default behaviour install their registry early —
//! before any image load — with [`set_global_registry`].
//!
//! ```ignore
//! use std::sync::Arc;
//! use blinc_image::decoder::{DecoderRegistry, ImageDecoder, set_global_registry};
//!
//! struct ZuneJpeg;
//! impl ImageDecoder for ZuneJpeg { /* ... */ }
//!
//! let mut registry = DecoderRegistry::empty();
//! registry.register(Arc::new(ZuneJpeg));
//! set_global_registry(registry);
//! ```
//!
//! # Opting out completely
//!
//! Build with `default-features = false` (or `default-features = false`
//! plus only the features you need). The registry will start empty;
//! `from_bytes` returns [`ImageError::UnsupportedFormat`] until you
//! register a decoder. This is the right choice for apps that fetch
//! pre-decoded RGBA from somewhere else (a shared GPU asset cache,
//! a custom render pipeline, etc.) and never need the
//! `image` crate's decoders compiled in.

use std::sync::{Arc, OnceLock, RwLock};

use crate::error::{ImageError, Result};
use crate::source::ImageFormat;

/// Decoded image bytes ready for `ImageData::from_rgba`.
#[derive(Debug, Clone)]
pub struct DecodedImage {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// A pluggable image decoder.
///
/// Implementations claim a set of [`ImageFormat`]s via [`formats`] and
/// turn raw bytes into [`DecodedImage`] via [`decode`]. The registry
/// dispatches to the first decoder whose `formats()` covers the
/// detected format; if no decoder accepts, it tries them all in
/// registration order until one returns `Ok`.
///
/// [`formats`]: ImageDecoder::formats
/// [`decode`]: ImageDecoder::decode
pub trait ImageDecoder: Send + Sync {
    /// Formats this decoder claims to support. Used to pick the
    /// preferred decoder when format detection succeeds.
    fn formats(&self) -> &[ImageFormat];

    /// Decode `bytes` into RGBA pixels.
    fn decode(&self, bytes: &[u8]) -> Result<DecodedImage>;
}

/// Registry of image decoders.
///
/// Tries decoders in registration order; the first one that succeeds
/// wins. When format detection (via magic bytes) returns a known
/// format, decoders whose `formats()` covers it are tried first.
#[derive(Default, Clone)]
pub struct DecoderRegistry {
    decoders: Vec<Arc<dyn ImageDecoder>>,
}

impl DecoderRegistry {
    /// An empty registry. Useful when you want to opt every built-in
    /// decoder out and only enable specific custom ones.
    pub fn empty() -> Self {
        Self {
            decoders: Vec::new(),
        }
    }

    /// A registry pre-populated with every built-in decoder whose
    /// cargo feature is enabled. With default features (`png`,
    /// `jpeg`) you get those two; with `all-formats` you get all
    /// five built-ins.
    pub fn with_builtins() -> Self {
        // `mut` may be unused when every per-format feature is off
        // (e.g. `default-features = false`); the register calls below
        // are individually cfg-gated.
        #[allow(unused_mut)]
        let mut registry = Self::empty();

        #[cfg(feature = "png")]
        registry.register(Arc::new(crate::builtin::PngDecoder));
        #[cfg(feature = "jpeg")]
        registry.register(Arc::new(crate::builtin::JpegDecoder));
        #[cfg(feature = "gif")]
        registry.register(Arc::new(crate::builtin::GifDecoder));
        #[cfg(feature = "webp")]
        registry.register(Arc::new(crate::builtin::WebPDecoder));
        #[cfg(feature = "bmp")]
        registry.register(Arc::new(crate::builtin::BmpDecoder));
        #[cfg(feature = "tiff")]
        registry.register(Arc::new(crate::builtin::TiffDecoder));
        #[cfg(feature = "avif")]
        registry.register(Arc::new(crate::builtin::AvifDecoder));

        registry
    }

    /// Append a decoder. Later registrations are tried later — the
    /// expected pattern is "register your override first, then call
    /// `with_builtins` if you also want fallbacks".
    pub fn register(&mut self, decoder: Arc<dyn ImageDecoder>) {
        self.decoders.push(decoder);
    }

    /// `true` if no decoders are registered.
    pub fn is_empty(&self) -> bool {
        self.decoders.is_empty()
    }

    /// Decode `bytes`. If `format` is `Some(_)`, decoders that
    /// advertise that format are tried first; otherwise every
    /// decoder is tried in registration order.
    pub fn decode(&self, bytes: &[u8], format: Option<ImageFormat>) -> Result<DecodedImage> {
        if self.decoders.is_empty() {
            return Err(ImageError::UnsupportedFormat(
                "no image decoder registered (build with `png`, `jpeg`, ... or call \
                 DecoderRegistry::register before loading)"
                    .to_string(),
            ));
        }

        // Preferred pass: decoders that advertise the detected format
        if let Some(fmt) = format {
            for decoder in &self.decoders {
                if decoder.formats().contains(&fmt)
                    && let Ok(decoded) = decoder.decode(bytes)
                {
                    return Ok(decoded);
                }
            }
        }

        // Fallback pass: try every decoder in order
        let mut last_err: Option<ImageError> = None;
        for decoder in &self.decoders {
            match decoder.decode(bytes) {
                Ok(decoded) => return Ok(decoded),
                Err(e) => last_err = Some(e),
            }
        }

        Err(last_err.unwrap_or_else(|| {
            ImageError::UnsupportedFormat("no decoder accepted the image bytes".to_string())
        }))
    }
}

// ============================================================================
// Process-wide registry
// ============================================================================

static GLOBAL_REGISTRY: OnceLock<RwLock<DecoderRegistry>> = OnceLock::new();

fn registry_lock() -> &'static RwLock<DecoderRegistry> {
    GLOBAL_REGISTRY.get_or_init(|| RwLock::new(DecoderRegistry::with_builtins()))
}

/// Replace the process-wide registry. Call this once at startup —
/// before any `ImageData::from_bytes` call — to plug in a custom
/// decoder set.
pub fn set_global_registry(registry: DecoderRegistry) {
    let lock = registry_lock();
    *lock.write().expect("decoder registry poisoned") = registry;
}

/// Decode bytes through the process-wide registry. Used internally
/// by `ImageData::from_bytes`; exposed publicly so callers that want
/// to bypass `ImageData` (e.g. to feed pixels into a custom GPU
/// pipeline) can still benefit from the registered decoders.
pub fn decode_with_global_registry(
    bytes: &[u8],
    format: Option<ImageFormat>,
) -> Result<DecodedImage> {
    let lock = registry_lock();
    let registry = lock.read().expect("decoder registry poisoned");
    registry.decode(bytes, format)
}
