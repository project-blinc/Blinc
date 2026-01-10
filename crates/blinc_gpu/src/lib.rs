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
    BlurUniforms, ClipType, ColorMatrixUniforms, CompositeUniforms, DropShadowUniforms, FillType,
    GlassType, GlassUniforms, GlowUniforms, GpuGlassPrimitive, GpuGlyph, GpuPrimitive, LayerCommand,
    LayerCommandEntry, LayerCompositeUniforms, PathBatch, PathUniforms, PrimitiveBatch,
    PrimitiveType, Uniforms,
};
pub use renderer::{GpuRenderer, LayerTexture, LayerTextureCache, RendererConfig};
pub use shaders::{
    BLUR_SHADER, COLOR_MATRIX_SHADER, COMPOSITE_SHADER, DROP_SHADOW_SHADER, GLASS_SHADER,
    GLOW_SHADER, IMAGE_SHADER, LAYER_COMPOSITE_SHADER, PATH_SHADER, SDF_SHADER,
    SIMPLE_GLASS_SHADER, TEXT_SHADER,
};
pub use text::TextRenderingContext;

// Re-export text types for convenience
pub use blinc_text::{ColorSpan, FontRegistry, GenericFont, TextAlignment, TextAnchor};
