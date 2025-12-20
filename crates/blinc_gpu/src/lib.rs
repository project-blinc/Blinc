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

pub mod primitives;
pub mod renderer;
pub mod shaders;

pub use primitives::{
    CompositeUniforms, FillType, GlassType, GlassUniforms, GpuGlassPrimitive, GpuGlyph,
    GpuPrimitive, PrimitiveBatch, PrimitiveType, Uniforms,
};
pub use renderer::GpuRenderer;
pub use shaders::{COMPOSITE_SHADER, GLASS_SHADER, SDF_SHADER, TEXT_SHADER};
