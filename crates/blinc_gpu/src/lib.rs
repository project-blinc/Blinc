//! Blinc GPU Renderer
//!
//! SDF-based GPU rendering using wgpu.
//!
//! # Features
//!
//! - **SDF Primitives**: Rounded rectangles, circles, ellipses with anti-aliasing
//! - **Shadows**: Gaussian blur shadows via error function approximation
//! - **Gradients**: Linear and radial gradient fills
//! - **Glass/Vibrancy**: Backdrop blur effects for frosted glass UI (Apple-style)
//! - **Text**: SDF-based text rendering with glyph atlases
//! - **Compositing**: Layer blending with various blend modes
//! - **Backbuffer**: Double/triple buffering for WASM and glass effects
//! - **Paint Context**: GPU-backed DrawContext implementation
//! - **Path Rendering**: Vector path tessellation via lyon

pub mod backbuffer;
pub mod gradient_texture;
pub mod image;
pub mod paint;
pub mod path;
pub mod primitives;
pub mod renderer;
pub mod shaders;
pub mod text;

pub use backbuffer::{Backbuffer, BackbufferConfig, FrameContext};
pub use gradient_texture::{GradientTextureCache, RasterizedGradient, GRADIENT_TEXTURE_WIDTH};
pub use image::{GpuImage, GpuImageInstance, ImageRenderingContext};
pub use paint::GpuPaintContext;
pub use path::{
    extract_brush_info, tessellate_fill, tessellate_stroke, PathBrushInfo, PathBrushType,
    PathVertex, TessellatedPath,
};
pub use primitives::{
    ClipType, CompositeUniforms, FillType, GlassType, GlassUniforms, GpuGlassPrimitive, GpuGlyph,
    GpuPrimitive, PathBatch, PathUniforms, PrimitiveBatch, PrimitiveType, Uniforms,
};
pub use renderer::{GpuRenderer, RendererConfig};
pub use shaders::{
    COMPOSITE_SHADER, GLASS_SHADER, IMAGE_SHADER, PATH_SHADER, SDF_SHADER, TEXT_SHADER,
};
pub use text::TextRenderingContext;

// Re-export text types for convenience
pub use blinc_text::{ColorSpan, FontRegistry, GenericFont, TextAlignment, TextAnchor};
