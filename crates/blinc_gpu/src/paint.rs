//! Paint Context - GPU-backed DrawContext implementation
//!
//! This module provides `GpuPaintContext`, a GPU-accelerated implementation of
//! the `DrawContext` trait that translates drawing commands into GPU primitives
//! for efficient rendering.
//!
//! # Architecture
//!
//! ```text
//! DrawContext commands
//!        │
//!        ▼
//! ┌─────────────────┐
//! │ GpuPaintContext │  ← Translates commands to GPU primitives
//! └────────┬────────┘
//!          │
//!          ▼
//! ┌─────────────────┐
//! │  PrimitiveBatch │  ← Batched GPU-ready data
//! └────────┬────────┘
//!          │
//!          ▼
//! ┌─────────────────┐
//! │   GpuRenderer   │  ← Executes render passes
//! └─────────────────┘
//! ```
//!
//! # Example
//!
//! ```ignore
//! use blinc_gpu::GpuPaintContext;
//! use blinc_core::{DrawContext, DrawContextExt, Color, Rect};
//!
//! let mut ctx = GpuPaintContext::new(800.0, 600.0);
//!
//! // Draw using the DrawContext API
//! ctx.fill_rect(Rect::new(10.0, 10.0, 100.0, 50.0), 8.0.into(), Color::BLUE.into());
//!
//! // Get the batched primitives for GPU rendering
//! let batch = ctx.take_batch();
//! renderer.render(&target, &batch);
//! ```

use blinc_core::{
    Affine2D, BillboardFacing, BlendMode, Brush, Camera, ClipShape, Color, CornerRadius,
    DrawCommand, DrawContext, Environment, ImageId, ImageOptions, LayerConfig, LayerId, Light,
    Mat4, MaterialId, MeshId, MeshInstance, ParticleBlendMode, ParticleEmitterShape, ParticleForce,
    ParticleSystemData, Path, Point, Rect, Sdf3DViewport, SdfBuilder, Shadow, ShapeId, Size,
    Stroke, TextStyle, Transform,
};

use crate::path::{extract_brush_info, tessellate_fill, tessellate_stroke};
use crate::primitives::{
    ClipType, FillType, GlassType, GpuGlassPrimitive, GpuPrimitive, PrimitiveBatch, PrimitiveType,
    Sdf3DUniform, Viewport3D,
};
use crate::text::TextRenderingContext;

// ─────────────────────────────────────────────────────────────────────────────
// Transform Stack
// ─────────────────────────────────────────────────────────────────────────────

/// Combined 2D transform state (for future optimization)
#[derive(Clone, Debug)]
#[allow(dead_code)]
struct TransformState {
    /// Combined affine transform
    affine: Affine2D,
    /// Combined opacity
    opacity: f32,
    /// Current blend mode
    blend_mode: BlendMode,
}

impl Default for TransformState {
    fn default() -> Self {
        Self {
            affine: Affine2D::IDENTITY,
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Layer Stack
// ─────────────────────────────────────────────────────────────────────────────

/// State for a single layer in the stack
///
/// Tracks the configuration and rendering state when a layer is pushed,
/// so it can be properly restored when the layer is popped.
#[derive(Clone, Debug)]
struct LayerState {
    /// The layer configuration
    config: LayerConfig,
    /// Starting primitive index when this layer was pushed
    primitive_start: usize,
    /// Starting foreground primitive index
    foreground_primitive_start: usize,
    /// Starting path vertex index
    path_start: usize,
    /// Starting foreground path vertex index
    foreground_path_start: usize,
    /// Parent state stack indices (transform, opacity, blend, clip)
    parent_state_indices: (usize, usize, usize, usize),
}

// ─────────────────────────────────────────────────────────────────────────────
// GPU Paint Context
// ─────────────────────────────────────────────────────────────────────────────

/// GPU-backed implementation of DrawContext
///
/// This translates high-level drawing commands into GPU primitives that can
/// be efficiently rendered by the `GpuRenderer`.
pub struct GpuPaintContext<'a> {
    /// Batched primitives ready for GPU rendering
    batch: PrimitiveBatch,
    /// Transform stack
    transform_stack: Vec<Affine2D>,
    /// Opacity stack
    opacity_stack: Vec<f32>,
    /// Blend mode stack
    blend_mode_stack: Vec<BlendMode>,
    /// Clip stack (for tracking, actual clipping done in shader)
    clip_stack: Vec<ClipShape>,
    /// Viewport size
    viewport: Size,
    /// Whether we're in a 3D context
    is_3d: bool,
    /// Current camera (for 3D mode)
    camera: Option<Camera>,
    /// Lights for 3D rendering
    lights: Vec<Light>,
    /// Text rendering context (optional, for draw_text support)
    text_ctx: Option<&'a mut TextRenderingContext>,
    /// Whether we're rendering to the foreground layer (after glass)
    is_foreground: bool,
    /// Current z-layer for interleaved rendering (used by Stack for proper z-ordering)
    z_layer: u32,
    /// Stack of active layers for offscreen rendering
    layer_stack: Vec<LayerState>,
}

impl<'a> GpuPaintContext<'a> {
    /// Create a new GPU paint context
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            batch: PrimitiveBatch::new(),
            transform_stack: vec![Affine2D::IDENTITY],
            opacity_stack: vec![1.0],
            blend_mode_stack: vec![BlendMode::Normal],
            clip_stack: Vec::new(),
            viewport: Size::new(width, height),
            is_3d: false,
            camera: None,
            lights: Vec::new(),
            text_ctx: None,
            is_foreground: false,
            z_layer: 0,
            layer_stack: Vec::new(),
        }
    }

    /// Set whether we're rendering to the foreground layer
    ///
    /// When true, primitives are pushed to the foreground batch (rendered after glass).
    /// When false (default), primitives go to the background batch.
    pub fn set_foreground(&mut self, is_foreground: bool) {
        self.is_foreground = is_foreground;
    }

    /// Create a new GPU paint context with text rendering support
    pub fn with_text_context(
        width: f32,
        height: f32,
        text_ctx: &'a mut TextRenderingContext,
    ) -> Self {
        Self {
            batch: PrimitiveBatch::new(),
            transform_stack: vec![Affine2D::IDENTITY],
            opacity_stack: vec![1.0],
            blend_mode_stack: vec![BlendMode::Normal],
            clip_stack: Vec::new(),
            viewport: Size::new(width, height),
            is_3d: false,
            camera: None,
            lights: Vec::new(),
            text_ctx: Some(text_ctx),
            is_foreground: false,
            z_layer: 0,
            layer_stack: Vec::new(),
        }
    }

    /// Set the text rendering context
    pub fn set_text_context(&mut self, text_ctx: &'a mut TextRenderingContext) {
        self.text_ctx = Some(text_ctx);
    }

    /// Get the current transform
    fn current_affine(&self) -> Affine2D {
        self.transform_stack
            .last()
            .copied()
            .unwrap_or(Affine2D::IDENTITY)
    }

    /// Get the current combined opacity
    fn combined_opacity(&self) -> f32 {
        self.opacity_stack.iter().product()
    }

    /// Transform a point by the current transform
    fn transform_point(&self, p: Point) -> Point {
        let affine = self.current_affine();
        // elements = [a, b, c, d, tx, ty]
        // | a  c  tx |   | x |
        // | b  d  ty | * | y |
        // | 0  0   1 |   | 1 |
        Point::new(
            affine.elements[0] * p.x + affine.elements[2] * p.y + affine.elements[4],
            affine.elements[1] * p.x + affine.elements[3] * p.y + affine.elements[5],
        )
    }

    /// Transform a rect by the current transform (axis-aligned bounding box)
    fn transform_rect(&self, rect: Rect) -> Rect {
        let affine = self.current_affine();

        // For axis-aligned rectangles with simple transforms (scale + translate),
        // we can just transform the origin and scale the size
        let origin = self.transform_point(rect.origin);
        let a = affine.elements[0];
        let b = affine.elements[1];
        let c = affine.elements[2];
        let d = affine.elements[3];
        let scale_x = (a * a + b * b).sqrt();
        let scale_y = (c * c + d * d).sqrt();

        Rect::new(
            origin.x,
            origin.y,
            rect.size.width * scale_x,
            rect.size.height * scale_y,
        )
    }

    /// Scale corner radius by the current transform's average scale factor
    fn scale_corner_radius(&self, corner_radius: CornerRadius) -> CornerRadius {
        let affine = self.current_affine();
        let a = affine.elements[0];
        let b = affine.elements[1];
        let c = affine.elements[2];
        let d = affine.elements[3];
        let scale_x = (a * a + b * b).sqrt();
        let scale_y = (c * c + d * d).sqrt();
        let avg_scale = (scale_x + scale_y) / 2.0;

        CornerRadius::new(
            corner_radius.top_left * avg_scale,
            corner_radius.top_right * avg_scale,
            corner_radius.bottom_right * avg_scale,
            corner_radius.bottom_left * avg_scale,
        )
    }

    /// Transform gradient parameters by the current transform
    /// For linear gradients, transforms (x1, y1, x2, y2) to screen space
    /// For radial gradients, transforms (cx, cy, radius, 0) to screen space
    fn transform_gradient_params(&self, params: [f32; 4], is_radial: bool) -> [f32; 4] {
        if is_radial {
            // Radial gradient: (cx, cy, radius, 0)
            let center = self.transform_point(Point::new(params[0], params[1]));
            // Scale radius by average scale factor
            let affine = self.current_affine();
            let a = affine.elements[0];
            let b = affine.elements[1];
            let c = affine.elements[2];
            let d = affine.elements[3];
            let scale_x = (a * a + b * b).sqrt();
            let scale_y = (c * c + d * d).sqrt();
            let avg_scale = (scale_x + scale_y) / 2.0;
            [center.x, center.y, params[2] * avg_scale, params[3]]
        } else {
            // Linear gradient: (x1, y1, x2, y2)
            let start = self.transform_point(Point::new(params[0], params[1]));
            let end = self.transform_point(Point::new(params[2], params[3]));
            [start.x, start.y, end.x, end.y]
        }
    }

    /// Transform a clip shape by the current transform
    /// Note: For rotated transforms, this computes the axis-aligned bounding box
    fn transform_clip_shape(&self, shape: ClipShape) -> ClipShape {
        let affine = self.current_affine();

        // Check if this is identity transform (common case)
        if affine == Affine2D::IDENTITY {
            return shape;
        }

        match shape {
            ClipShape::Rect(rect) => {
                // Transform all four corners and compute AABB
                let corners = [
                    Point::new(rect.x(), rect.y()),
                    Point::new(rect.x() + rect.width(), rect.y()),
                    Point::new(rect.x() + rect.width(), rect.y() + rect.height()),
                    Point::new(rect.x(), rect.y() + rect.height()),
                ];

                let transformed: Vec<Point> =
                    corners.iter().map(|p| self.transform_point(*p)).collect();

                let min_x = transformed
                    .iter()
                    .map(|p| p.x)
                    .fold(f32::INFINITY, f32::min);
                let max_x = transformed
                    .iter()
                    .map(|p| p.x)
                    .fold(f32::NEG_INFINITY, f32::max);
                let min_y = transformed
                    .iter()
                    .map(|p| p.y)
                    .fold(f32::INFINITY, f32::min);
                let max_y = transformed
                    .iter()
                    .map(|p| p.y)
                    .fold(f32::NEG_INFINITY, f32::max);

                ClipShape::Rect(Rect::new(min_x, min_y, max_x - min_x, max_y - min_y))
            }
            ClipShape::RoundedRect {
                rect,
                corner_radius,
            } => {
                // Transform corners and compute AABB
                let corners = [
                    Point::new(rect.x(), rect.y()),
                    Point::new(rect.x() + rect.width(), rect.y()),
                    Point::new(rect.x() + rect.width(), rect.y() + rect.height()),
                    Point::new(rect.x(), rect.y() + rect.height()),
                ];

                let transformed: Vec<Point> =
                    corners.iter().map(|p| self.transform_point(*p)).collect();

                let min_x = transformed
                    .iter()
                    .map(|p| p.x)
                    .fold(f32::INFINITY, f32::min);
                let max_x = transformed
                    .iter()
                    .map(|p| p.x)
                    .fold(f32::NEG_INFINITY, f32::max);
                let min_y = transformed
                    .iter()
                    .map(|p| p.y)
                    .fold(f32::INFINITY, f32::min);
                let max_y = transformed
                    .iter()
                    .map(|p| p.y)
                    .fold(f32::NEG_INFINITY, f32::max);

                // Scale the corner radii by the average scale factor
                let a = affine.elements[0];
                let b = affine.elements[1];
                let c = affine.elements[2];
                let d = affine.elements[3];
                let scale_x = (a * a + b * b).sqrt();
                let scale_y = (c * c + d * d).sqrt();
                let avg_scale = (scale_x + scale_y) * 0.5;

                let scaled_radius = CornerRadius::new(
                    corner_radius.top_left * avg_scale,
                    corner_radius.top_right * avg_scale,
                    corner_radius.bottom_right * avg_scale,
                    corner_radius.bottom_left * avg_scale,
                );

                ClipShape::RoundedRect {
                    rect: Rect::new(min_x, min_y, max_x - min_x, max_y - min_y),
                    corner_radius: scaled_radius,
                }
            }
            ClipShape::Circle { center, radius } => {
                let transformed_center = self.transform_point(center);

                // For non-uniform scale, circle becomes ellipse - compute approximate radius
                let a = affine.elements[0];
                let b = affine.elements[1];
                let c = affine.elements[2];
                let d = affine.elements[3];
                let scale_x = (a * a + b * b).sqrt();
                let scale_y = (c * c + d * d).sqrt();

                if (scale_x - scale_y).abs() < 0.001 {
                    // Uniform scale - keep as circle
                    ClipShape::Circle {
                        center: transformed_center,
                        radius: radius * scale_x,
                    }
                } else {
                    // Non-uniform scale - convert to ellipse
                    ClipShape::Ellipse {
                        center: transformed_center,
                        radii: blinc_core::Vec2::new(radius * scale_x, radius * scale_y),
                    }
                }
            }
            ClipShape::Ellipse { center, radii } => {
                let transformed_center = self.transform_point(center);

                let a = affine.elements[0];
                let b = affine.elements[1];
                let c = affine.elements[2];
                let d = affine.elements[3];
                let scale_x = (a * a + b * b).sqrt();
                let scale_y = (c * c + d * d).sqrt();

                ClipShape::Ellipse {
                    center: transformed_center,
                    radii: blinc_core::Vec2::new(radii.x * scale_x, radii.y * scale_y),
                }
            }
            ClipShape::Path(path) => {
                // Path clipping with transforms not supported - keep as-is
                ClipShape::Path(path)
            }
        }
    }

    /// Convert a Brush to GPU color components and gradient parameters
    /// Returns (color1, color2, gradient_params, fill_type)
    /// Note: Glass brushes are handled separately in fill methods - this returns transparent
    fn brush_to_colors(&self, brush: &Brush) -> ([f32; 4], [f32; 4], [f32; 4], FillType) {
        let opacity = self.combined_opacity();
        match brush {
            Brush::Solid(color) => {
                let c = [color.r, color.g, color.b, color.a * opacity];
                // Default gradient params (not used for solid)
                (c, c, [0.0, 0.0, 1.0, 0.0], FillType::Solid)
            }
            Brush::Glass(_) => {
                // Glass is handled via glass primitives, not regular primitives
                // Return transparent as a fallback (should never be used)
                ([0.0; 4], [0.0; 4], [0.0, 0.0, 1.0, 0.0], FillType::Solid)
            }
            Brush::Image(_) => {
                // Image backgrounds are handled separately via the image pipeline
                // Return transparent as a fallback
                ([0.0; 4], [0.0; 4], [0.0, 0.0, 1.0, 0.0], FillType::Solid)
            }
            Brush::Blur(_) => {
                // Blur is handled via glass primitives, not regular primitives
                // Return transparent as a fallback (should never be used)
                ([0.0; 4], [0.0; 4], [0.0, 0.0, 1.0, 0.0], FillType::Solid)
            }
            Brush::Gradient(gradient) => {
                let (stops, fill_type, gradient_params) = match gradient {
                    blinc_core::Gradient::Linear {
                        start, end, stops, ..
                    } => {
                        // Linear gradient: (x1, y1, x2, y2) in user space
                        (
                            stops,
                            FillType::LinearGradient,
                            [start.x, start.y, end.x, end.y],
                        )
                    }
                    blinc_core::Gradient::Radial {
                        center,
                        radius,
                        stops,
                        ..
                    } => {
                        // Radial gradient: (cx, cy, radius, 0) in user space
                        (
                            stops,
                            FillType::RadialGradient,
                            [center.x, center.y, *radius, 0.0],
                        )
                    }
                    // Conic gradients treated as radial for now
                    blinc_core::Gradient::Conic { center, stops, .. } => (
                        stops,
                        FillType::RadialGradient,
                        [center.x, center.y, 100.0, 0.0],
                    ),
                };

                let (c1, c2) = if stops.len() >= 2 {
                    let s1 = &stops[0];
                    let s2 = &stops[stops.len() - 1];
                    (
                        [s1.color.r, s1.color.g, s1.color.b, s1.color.a * opacity],
                        [s2.color.r, s2.color.g, s2.color.b, s2.color.a * opacity],
                    )
                } else if !stops.is_empty() {
                    let c = &stops[0].color;
                    let arr = [c.r, c.g, c.b, c.a * opacity];
                    (arr, arr)
                } else {
                    ([1.0, 1.0, 1.0, opacity], [1.0, 1.0, 1.0, opacity])
                };

                (c1, c2, gradient_params, fill_type)
            }
        }
    }

    /// Get clip data from the current clip stack
    /// Returns (clip_bounds, clip_radius, clip_type)
    ///
    /// For multiple rect clips, computes the intersection of all clips.
    /// For mixed clip types, uses the topmost clip (conservative approximation).
    ///
    /// Corner radius handling: A rectangular clip (non-rounded) will reset the
    /// corner radius to 0 for any corners it covers. This ensures that a child
    /// with overflow_clip() doesn't inherit rounded corners from a parent.
    fn get_clip_data(&self) -> ([f32; 4], [f32; 4], ClipType) {
        if self.clip_stack.is_empty() {
            // No clip - use large bounds
            return (
                [-10000.0, -10000.0, 100000.0, 100000.0],
                [0.0; 4],
                ClipType::None,
            );
        }

        // Try to compute intersection of all rect clips
        // Start with very large bounds
        let mut intersect_min_x = f32::NEG_INFINITY;
        let mut intersect_min_y = f32::NEG_INFINITY;
        let mut intersect_max_x = f32::INFINITY;
        let mut intersect_max_y = f32::INFINITY;
        let mut has_rect_clips = false;

        // Track corner radii with their source bounds
        // Each corner's radius is only valid if the intersection edge matches the source edge
        // Format: (radius, source_min_x, source_min_y, source_max_x, source_max_y)
        let mut corner_sources: [(f32, f32, f32, f32, f32); 4] = [
            (
                0.0,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
                f32::INFINITY,
                f32::INFINITY,
            ), // top_left
            (
                0.0,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
                f32::INFINITY,
                f32::INFINITY,
            ), // top_right
            (
                0.0,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
                f32::INFINITY,
                f32::INFINITY,
            ), // bottom_right
            (
                0.0,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
                f32::INFINITY,
                f32::INFINITY,
            ), // bottom_left
        ];

        // Track whether the topmost clip is a plain Rect (not rounded)
        let mut topmost_is_plain_rect = false;

        for clip in &self.clip_stack {
            match clip {
                ClipShape::Rect(rect) => {
                    // Intersect with this rect
                    intersect_min_x = intersect_min_x.max(rect.x());
                    intersect_min_y = intersect_min_y.max(rect.y());
                    intersect_max_x = intersect_max_x.min(rect.x() + rect.width());
                    intersect_max_y = intersect_max_y.min(rect.y() + rect.height());
                    has_rect_clips = true;
                    topmost_is_plain_rect = true;
                }
                ClipShape::RoundedRect {
                    rect,
                    corner_radius,
                } => {
                    let rx = rect.x();
                    let ry = rect.y();
                    let rmax_x = rect.x() + rect.width();
                    let rmax_y = rect.y() + rect.height();

                    // Intersect with this rect
                    intersect_min_x = intersect_min_x.max(rx);
                    intersect_min_y = intersect_min_y.max(ry);
                    intersect_max_x = intersect_max_x.min(rmax_x);
                    intersect_max_y = intersect_max_y.min(rmax_y);

                    // Track corner radii with their source bounds
                    // Only update if this corner radius is larger (take max)
                    if corner_radius.top_left > corner_sources[0].0 {
                        corner_sources[0] = (corner_radius.top_left, rx, ry, rmax_x, rmax_y);
                    }
                    if corner_radius.top_right > corner_sources[1].0 {
                        corner_sources[1] = (corner_radius.top_right, rx, ry, rmax_x, rmax_y);
                    }
                    if corner_radius.bottom_right > corner_sources[2].0 {
                        corner_sources[2] = (corner_radius.bottom_right, rx, ry, rmax_x, rmax_y);
                    }
                    if corner_radius.bottom_left > corner_sources[3].0 {
                        corner_sources[3] = (corner_radius.bottom_left, rx, ry, rmax_x, rmax_y);
                    }

                    has_rect_clips = true;
                    topmost_is_plain_rect = false;
                }
                // For non-rect clips, fall back to topmost-only behavior
                ClipShape::Circle { .. } | ClipShape::Ellipse { .. } | ClipShape::Path(_) => {
                    // Can't easily intersect with circles/ellipses/paths
                    // Fall through to use the topmost clip
                    topmost_is_plain_rect = false;
                }
            }
        }

        // If we have rect clips, use the intersection
        if has_rect_clips {
            let width = (intersect_max_x - intersect_min_x).max(0.0);
            let height = (intersect_max_y - intersect_min_y).max(0.0);

            // Determine final corner radii based on whether intersection edges are within
            // the corner radius region of the source. A corner should be rounded if the
            // intersection boundary is close enough to the source corner that it would
            // visually clip through the rounded area.
            //
            // For each corner, we check if the intersection edge is within (radius + epsilon)
            // of the source edge. If so, apply the rounded corner to prevent visual clipping.

            let mut radii = [0.0f32; 4];

            // Top-left corner: check if intersection is within corner radius region
            let (r, src_min_x, src_min_y, _, _) = corner_sources[0];
            if r > 0.0 {
                let dist_from_left = intersect_min_x - src_min_x;
                let dist_from_top = intersect_min_y - src_min_y;
                // If intersection is within the corner radius region, apply rounding
                if dist_from_left < r && dist_from_top < r {
                    radii[0] = (r - dist_from_left.max(0.0)).max(0.0).min(r);
                }
            }

            // Top-right corner
            let (r, _, src_min_y, src_max_x, _) = corner_sources[1];
            if r > 0.0 {
                let dist_from_right = src_max_x - intersect_max_x;
                let dist_from_top = intersect_min_y - src_min_y;
                if dist_from_right < r && dist_from_top < r {
                    radii[1] = (r - dist_from_right.max(0.0)).max(0.0).min(r);
                }
            }

            // Bottom-right corner
            let (r, _, _, src_max_x, src_max_y) = corner_sources[2];
            if r > 0.0 {
                let dist_from_right = src_max_x - intersect_max_x;
                let dist_from_bottom = src_max_y - intersect_max_y;
                if dist_from_right < r && dist_from_bottom < r {
                    radii[2] = (r - dist_from_right.max(0.0)).max(0.0).min(r);
                }
            }

            // Bottom-left corner
            let (r, src_min_x, _, _, src_max_y) = corner_sources[3];
            if r > 0.0 {
                let dist_from_left = intersect_min_x - src_min_x;
                let dist_from_bottom = src_max_y - intersect_max_y;
                if dist_from_left < r && dist_from_bottom < r {
                    radii[3] = (r - dist_from_left.max(0.0)).max(0.0).min(r);
                }
            }

            return (
                [intersect_min_x, intersect_min_y, width, height],
                radii,
                ClipType::Rect,
            );
        }

        // Fall back to topmost clip for non-rect clips
        let clip = self.clip_stack.last().unwrap();
        match clip {
            ClipShape::Rect(rect) => (
                [rect.x(), rect.y(), rect.width(), rect.height()],
                [0.0; 4],
                ClipType::Rect,
            ),
            ClipShape::RoundedRect {
                rect,
                corner_radius,
            } => (
                [rect.x(), rect.y(), rect.width(), rect.height()],
                [
                    corner_radius.top_left,
                    corner_radius.top_right,
                    corner_radius.bottom_right,
                    corner_radius.bottom_left,
                ],
                ClipType::Rect,
            ),
            ClipShape::Circle { center, radius } => (
                [center.x, center.y, *radius, *radius],
                [*radius, *radius, 0.0, 0.0],
                ClipType::Circle,
            ),
            ClipShape::Ellipse { center, radii } => (
                [center.x, center.y, radii.x, radii.y],
                [radii.x, radii.y, 0.0, 0.0],
                ClipType::Ellipse,
            ),
            ClipShape::Path(_) => {
                // Path clipping not supported in GPU - fall back to no clip
                (
                    [-10000.0, -10000.0, 100000.0, 100000.0],
                    [0.0; 4],
                    ClipType::None,
                )
            }
        }
    }

    /// Take the accumulated batch for rendering
    pub fn take_batch(&mut self) -> PrimitiveBatch {
        std::mem::take(&mut self.batch)
    }

    /// Get a reference to the current batch
    pub fn batch(&self) -> &PrimitiveBatch {
        &self.batch
    }

    /// Get a mutable reference to the current batch
    pub fn batch_mut(&mut self) -> &mut PrimitiveBatch {
        &mut self.batch
    }

    /// Clear the batch
    pub fn clear(&mut self) {
        self.batch.clear();
        self.transform_stack = vec![Affine2D::IDENTITY];
        self.opacity_stack = vec![1.0];
        self.blend_mode_stack = vec![BlendMode::Normal];
        self.clip_stack.clear();
        self.layer_stack.clear();
        self.is_3d = false;
        self.camera = None;
    }

    /// Apply opacity to a brush by modifying the color's alpha channel
    fn apply_opacity_to_brush(brush: Brush, opacity: f32) -> Brush {
        if opacity >= 1.0 {
            return brush;
        }
        match brush {
            Brush::Solid(color) => {
                Brush::Solid(Color::rgba(color.r, color.g, color.b, color.a * opacity))
            }
            // For gradients, we'd need to modify each stop's color
            // For now, return as-is since SVGs typically use solid colors
            other => other,
        }
    }

    /// Resize the viewport
    pub fn resize(&mut self, width: f32, height: f32) {
        self.viewport = Size::new(width, height);
    }

    /// Execute a list of recorded draw commands
    pub fn execute_commands(&mut self, commands: &[DrawCommand]) {
        for cmd in commands {
            self.execute_command(cmd);
        }
    }

    /// Execute a single draw command
    pub fn execute_command(&mut self, cmd: &DrawCommand) {
        match cmd {
            DrawCommand::PushTransform(t) => self.push_transform(t.clone()),
            DrawCommand::PopTransform => self.pop_transform(),
            DrawCommand::PushClip(shape) => self.push_clip(shape.clone()),
            DrawCommand::PopClip => self.pop_clip(),
            DrawCommand::PushOpacity(o) => self.push_opacity(*o),
            DrawCommand::PopOpacity => self.pop_opacity(),
            DrawCommand::PushBlendMode(m) => self.push_blend_mode(*m),
            DrawCommand::PopBlendMode => self.pop_blend_mode(),
            DrawCommand::FillPath { path, brush } => self.fill_path(path, brush.clone()),
            DrawCommand::StrokePath {
                path,
                stroke,
                brush,
            } => self.stroke_path(path, stroke, brush.clone()),
            DrawCommand::FillRect {
                rect,
                corner_radius,
                brush,
            } => self.fill_rect(*rect, *corner_radius, brush.clone()),
            DrawCommand::StrokeRect {
                rect,
                corner_radius,
                stroke,
                brush,
            } => self.stroke_rect(*rect, *corner_radius, stroke, brush.clone()),
            DrawCommand::FillCircle {
                center,
                radius,
                brush,
            } => self.fill_circle(*center, *radius, brush.clone()),
            DrawCommand::StrokeCircle {
                center,
                radius,
                stroke,
                brush,
            } => self.stroke_circle(*center, *radius, stroke, brush.clone()),
            DrawCommand::DrawText {
                text,
                origin,
                style,
            } => self.draw_text(text, *origin, style),
            DrawCommand::DrawImage {
                image,
                rect,
                options,
            } => self.draw_image(*image, *rect, options),
            DrawCommand::DrawShadow {
                rect,
                corner_radius,
                shadow,
            } => self.draw_shadow(*rect, *corner_radius, *shadow),
            DrawCommand::DrawInnerShadow {
                rect,
                corner_radius,
                shadow,
            } => self.draw_inner_shadow(*rect, *corner_radius, *shadow),
            DrawCommand::DrawCircleShadow {
                center,
                radius,
                shadow,
            } => self.draw_circle_shadow(*center, *radius, *shadow),
            DrawCommand::DrawCircleInnerShadow {
                center,
                radius,
                shadow,
            } => self.draw_circle_inner_shadow(*center, *radius, *shadow),
            DrawCommand::SetCamera(camera) => self.set_camera(camera),
            DrawCommand::DrawMesh {
                mesh,
                material,
                transform,
            } => self.draw_mesh(*mesh, *material, *transform),
            DrawCommand::DrawMeshInstanced { mesh, instances } => {
                self.draw_mesh_instanced(*mesh, instances)
            }
            DrawCommand::AddLight(light) => self.add_light(light.clone()),
            DrawCommand::SetEnvironment(env) => self.set_environment(env),
            DrawCommand::PushLayer(config) => self.push_layer(config.clone()),
            DrawCommand::PopLayer => self.pop_layer(),
            DrawCommand::SampleLayer {
                id,
                source_rect,
                dest_rect,
            } => self.sample_layer(*id, *source_rect, *dest_rect),
        }
    }
}

impl<'a> DrawContext for GpuPaintContext<'a> {
    fn push_transform(&mut self, transform: Transform) {
        let current = self.current_affine();
        let new_transform = match transform {
            Transform::Affine2D(affine) => current.then(&affine),
            Transform::Mat4(_) => {
                // For 3D transforms in 2D context, just use identity
                // Real 3D handling would need a separate 3D rendering path
                current
            }
        };
        self.transform_stack.push(new_transform);
    }

    fn pop_transform(&mut self) {
        if self.transform_stack.len() > 1 {
            self.transform_stack.pop();
        }
    }

    fn current_transform(&self) -> Transform {
        Transform::Affine2D(self.current_affine())
    }

    fn push_clip(&mut self, shape: ClipShape) {
        // Transform the clip shape by the current transform
        // Note: This only works correctly for translate + uniform scale transforms.
        // Rotation transforms are approximated (the bounding box is used).
        let transformed_shape = self.transform_clip_shape(shape);
        self.clip_stack.push(transformed_shape);
    }

    fn pop_clip(&mut self) {
        self.clip_stack.pop();
    }

    fn push_opacity(&mut self, opacity: f32) {
        self.opacity_stack.push(opacity);
    }

    fn pop_opacity(&mut self) {
        if self.opacity_stack.len() > 1 {
            self.opacity_stack.pop();
        }
    }

    fn push_blend_mode(&mut self, mode: BlendMode) {
        self.blend_mode_stack.push(mode);
    }

    fn pop_blend_mode(&mut self) {
        if self.blend_mode_stack.len() > 1 {
            self.blend_mode_stack.pop();
        }
    }

    fn set_foreground_layer(&mut self, is_foreground: bool) {
        self.is_foreground = is_foreground;
    }

    fn set_z_layer(&mut self, layer: u32) {
        self.z_layer = layer;
    }

    fn z_layer(&self) -> u32 {
        self.z_layer
    }

    fn fill_path(&mut self, path: &Path, brush: Brush) {
        // Apply current opacity to the brush
        let opacity = self.combined_opacity();
        let brush = Self::apply_opacity_to_brush(brush, opacity);

        // Extract brush info for advanced features (multi-stop gradients, images, glass)
        let brush_info = extract_brush_info(&brush);

        // Tessellate the path using lyon
        let mut tessellated = tessellate_fill(path, &brush);

        // Transform vertices by current transform stack
        let affine = self.current_affine();
        for vertex in &mut tessellated.vertices {
            let x = vertex.position[0];
            let y = vertex.position[1];
            vertex.position[0] =
                affine.elements[0] * x + affine.elements[2] * y + affine.elements[4];
            vertex.position[1] =
                affine.elements[1] * x + affine.elements[3] * y + affine.elements[5];
        }

        if !tessellated.is_empty() {
            // Capture current clip state for paths
            let (clip_bounds, clip_radius, clip_type) = self.get_clip_data();

            if self.is_foreground {
                self.batch.push_foreground_path_with_brush_info(
                    tessellated,
                    clip_bounds,
                    clip_radius,
                    clip_type,
                    &brush_info,
                );
            } else {
                self.batch.push_path_with_brush_info(
                    tessellated,
                    clip_bounds,
                    clip_radius,
                    clip_type,
                    &brush_info,
                );
            }
        }
    }

    fn stroke_path(&mut self, path: &Path, stroke: &Stroke, brush: Brush) {
        // Apply current opacity to the brush
        let opacity = self.combined_opacity();
        let brush = Self::apply_opacity_to_brush(brush, opacity);

        // Extract brush info for advanced features (multi-stop gradients, images, glass)
        let brush_info = extract_brush_info(&brush);

        // Tessellate the stroke using lyon
        let mut tessellated = tessellate_stroke(path, stroke, &brush);

        // Transform vertices by current transform stack
        let affine = self.current_affine();
        for vertex in &mut tessellated.vertices {
            let x = vertex.position[0];
            let y = vertex.position[1];
            vertex.position[0] =
                affine.elements[0] * x + affine.elements[2] * y + affine.elements[4];
            vertex.position[1] =
                affine.elements[1] * x + affine.elements[3] * y + affine.elements[5];
        }

        if !tessellated.is_empty() {
            // Capture current clip state for paths
            let (clip_bounds, clip_radius, clip_type) = self.get_clip_data();

            if self.is_foreground {
                self.batch.push_foreground_path_with_brush_info(
                    tessellated,
                    clip_bounds,
                    clip_radius,
                    clip_type,
                    &brush_info,
                );
            } else {
                self.batch.push_path_with_brush_info(
                    tessellated,
                    clip_bounds,
                    clip_radius,
                    clip_type,
                    &brush_info,
                );
            }
        }
    }

    fn fill_rect(&mut self, rect: Rect, corner_radius: CornerRadius, brush: Brush) {
        let transformed = self.transform_rect(rect);
        let scaled_radius = self.scale_corner_radius(corner_radius);

        // Handle glass brush specially - push to glass primitives
        if let Brush::Glass(style) = &brush {
            let mut glass = GpuGlassPrimitive::new(
                transformed.x(),
                transformed.y(),
                transformed.width(),
                transformed.height(),
            )
            .with_corner_radius_per_corner(
                scaled_radius.top_left,
                scaled_radius.top_right,
                scaled_radius.bottom_right,
                scaled_radius.bottom_left,
            )
            .with_blur(style.blur)
            .with_tint(style.tint.r, style.tint.g, style.tint.b, style.tint.a)
            .with_saturation(style.saturation)
            .with_brightness(style.brightness)
            .with_noise(style.noise)
            .with_border_thickness(style.border_thickness);

            // Apply shadow if present in the glass style
            if let Some(ref shadow) = style.shadow {
                glass = glass.with_shadow_offset(
                    shadow.blur,
                    shadow.color.a, // Use color alpha as opacity
                    shadow.offset_x,
                    shadow.offset_y,
                );
            }

            // Apply current clip bounds to glass primitive (for scroll containers)
            let (clip_bounds, clip_radius, clip_type) = self.get_clip_data();
            match clip_type {
                ClipType::None => {}
                ClipType::Rect => {
                    // Check if this is a rounded rect clip (non-zero radius)
                    let has_radius = clip_radius.iter().any(|&r| r > 0.0);
                    if has_radius {
                        glass = glass.with_clip_rounded_rect_per_corner(
                            clip_bounds[0],
                            clip_bounds[1],
                            clip_bounds[2],
                            clip_bounds[3],
                            clip_radius[0],
                            clip_radius[1],
                            clip_radius[2],
                            clip_radius[3],
                        );
                    } else {
                        glass = glass.with_clip_rect(
                            clip_bounds[0],
                            clip_bounds[1],
                            clip_bounds[2],
                            clip_bounds[3],
                        );
                    }
                }
                ClipType::Circle | ClipType::Ellipse => {
                    // For circle/ellipse clips, use as rect for now
                    // Full support would require shader changes
                    glass = glass.with_clip_rect(
                        clip_bounds[0] - clip_bounds[2],
                        clip_bounds[1] - clip_bounds[3],
                        clip_bounds[2] * 2.0,
                        clip_bounds[3] * 2.0,
                    );
                }
            }

            // Set glass type based on simple flag
            if style.simple {
                glass = glass.with_glass_type(GlassType::Simple);
            }

            self.batch.push_glass(glass);
            return;
        }

        // Handle Blur brush - convert to glass primitive with just blur and optional tint
        if let Brush::Blur(style) = &brush {
            let mut glass = GpuGlassPrimitive::new(
                transformed.x(),
                transformed.y(),
                transformed.width(),
                transformed.height(),
            )
            .with_corner_radius_per_corner(
                scaled_radius.top_left,
                scaled_radius.top_right,
                scaled_radius.bottom_right,
                scaled_radius.bottom_left,
            )
            .with_blur(style.radius)
            .with_saturation(1.0) // No saturation adjustment for pure blur
            .with_brightness(1.0); // No brightness adjustment

            // Apply tint if specified
            if let Some(ref tint) = style.tint {
                glass = glass.with_tint(tint.r, tint.g, tint.b, tint.a * style.opacity);
            } else {
                // Default to slight white tint for visibility
                glass = glass.with_tint(1.0, 1.0, 1.0, 0.1 * style.opacity);
            }

            // Apply current clip bounds to glass primitive
            let (clip_bounds, clip_radius, clip_type) = self.get_clip_data();
            match clip_type {
                ClipType::None => {}
                ClipType::Rect => {
                    let has_radius = clip_radius.iter().any(|&r| r > 0.0);
                    if has_radius {
                        glass = glass.with_clip_rounded_rect_per_corner(
                            clip_bounds[0],
                            clip_bounds[1],
                            clip_bounds[2],
                            clip_bounds[3],
                            clip_radius[0],
                            clip_radius[1],
                            clip_radius[2],
                            clip_radius[3],
                        );
                    } else {
                        glass = glass.with_clip_rect(
                            clip_bounds[0],
                            clip_bounds[1],
                            clip_bounds[2],
                            clip_bounds[3],
                        );
                    }
                }
                ClipType::Circle | ClipType::Ellipse => {
                    glass = glass.with_clip_rect(
                        clip_bounds[0] - clip_bounds[2],
                        clip_bounds[1] - clip_bounds[3],
                        clip_bounds[2] * 2.0,
                        clip_bounds[3] * 2.0,
                    );
                }
            }

            self.batch.push_glass(glass);
            return;
        }

        let (color, color2, gradient_params, fill_type) = self.brush_to_colors(&brush);
        let (clip_bounds, clip_radius, clip_type) = self.get_clip_data();

        // Transform gradient params to screen space
        let is_radial = fill_type == FillType::RadialGradient;
        let transformed_gradient_params = if fill_type != FillType::Solid {
            self.transform_gradient_params(gradient_params, is_radial)
        } else {
            gradient_params
        };

        let primitive = GpuPrimitive {
            bounds: [
                transformed.x(),
                transformed.y(),
                transformed.width(),
                transformed.height(),
            ],
            corner_radius: [
                scaled_radius.top_left,
                scaled_radius.top_right,
                scaled_radius.bottom_right,
                scaled_radius.bottom_left,
            ],
            color,
            color2,
            border: [0.0; 4],
            border_color: [0.0; 4],
            shadow: [0.0; 4],
            shadow_color: [0.0; 4],
            clip_bounds,
            clip_radius,
            gradient_params: transformed_gradient_params,
            type_info: [
                PrimitiveType::Rect as u32,
                fill_type as u32,
                clip_type as u32,
                self.z_layer,
            ],
        };

        if self.is_foreground {
            self.batch.push_foreground(primitive);
        } else {
            self.batch.push(primitive);
        }
    }

    fn fill_rect_with_per_side_border(
        &mut self,
        rect: Rect,
        corner_radius: CornerRadius,
        brush: Brush,
        border_widths: [f32; 4],
        border_color: Color,
    ) {
        let transformed = self.transform_rect(rect);
        let scaled_radius = self.scale_corner_radius(corner_radius);
        let (color, color2, gradient_params, fill_type) = self.brush_to_colors(&brush);
        let (clip_bounds, clip_radius, clip_type) = self.get_clip_data();

        // Scale border widths by transform
        let affine = self.current_affine();
        let a = affine.elements[0];
        let b = affine.elements[1];
        let c = affine.elements[2];
        let d = affine.elements[3];
        let scale_x = (a * a + b * b).sqrt();
        let scale_y = (c * c + d * d).sqrt();

        let scaled_borders = [
            border_widths[0] * scale_y, // top (vertical scale)
            border_widths[1] * scale_x, // right (horizontal scale)
            border_widths[2] * scale_y, // bottom (vertical scale)
            border_widths[3] * scale_x, // left (horizontal scale)
        ];

        // Transform gradient params to screen space
        let is_radial = fill_type == FillType::RadialGradient;
        let transformed_gradient_params = if fill_type != FillType::Solid {
            self.transform_gradient_params(gradient_params, is_radial)
        } else {
            gradient_params
        };

        let opacity = self.combined_opacity();
        let primitive = GpuPrimitive {
            bounds: [
                transformed.x(),
                transformed.y(),
                transformed.width(),
                transformed.height(),
            ],
            corner_radius: [
                scaled_radius.top_left,
                scaled_radius.top_right,
                scaled_radius.bottom_right,
                scaled_radius.bottom_left,
            ],
            color,
            color2,
            border: scaled_borders,
            border_color: [
                border_color.r,
                border_color.g,
                border_color.b,
                border_color.a * opacity,
            ],
            shadow: [0.0; 4],
            shadow_color: [0.0; 4],
            clip_bounds,
            clip_radius,
            gradient_params: transformed_gradient_params,
            type_info: [
                PrimitiveType::Rect as u32,
                fill_type as u32,
                clip_type as u32,
                self.z_layer,
            ],
        };

        if self.is_foreground {
            self.batch.push_foreground(primitive);
        } else {
            self.batch.push(primitive);
        }
    }

    fn stroke_rect(
        &mut self,
        rect: Rect,
        corner_radius: CornerRadius,
        stroke: &Stroke,
        brush: Brush,
    ) {
        let transformed = self.transform_rect(rect);
        let scaled_radius = self.scale_corner_radius(corner_radius);
        let (color, _color2, gradient_params, fill_type) = self.brush_to_colors(&brush);
        let (clip_bounds, clip_radius, clip_type) = self.get_clip_data();

        let primitive = GpuPrimitive {
            bounds: [
                transformed.x(),
                transformed.y(),
                transformed.width(),
                transformed.height(),
            ],
            corner_radius: [
                scaled_radius.top_left,
                scaled_radius.top_right,
                scaled_radius.bottom_right,
                scaled_radius.bottom_left,
            ],
            color: [0.0, 0.0, 0.0, 0.0], // Transparent fill
            color2: [0.0, 0.0, 0.0, 0.0],
            border: [stroke.width, 0.0, 0.0, 0.0],
            border_color: color,
            shadow: [0.0; 4],
            shadow_color: [0.0; 4],
            clip_bounds,
            clip_radius,
            gradient_params,
            type_info: [
                PrimitiveType::Rect as u32,
                fill_type as u32,
                clip_type as u32,
                self.z_layer,
            ],
        };

        if self.is_foreground {
            self.batch.push_foreground(primitive);
        } else {
            self.batch.push(primitive);
        }
    }

    fn fill_circle(&mut self, center: Point, radius: f32, brush: Brush) {
        let transformed_center = self.transform_point(center);
        let affine = self.current_affine();
        let a = affine.elements[0];
        let b = affine.elements[1];
        let c = affine.elements[2];
        let d = affine.elements[3];
        let scale = ((a * a + b * b).sqrt() + (c * c + d * d).sqrt()) / 2.0;
        let transformed_radius = radius * scale;

        // Handle glass brush specially - push to glass primitives
        if let Brush::Glass(style) = &brush {
            let glass = GpuGlassPrimitive::circle(
                transformed_center.x,
                transformed_center.y,
                transformed_radius,
            )
            .with_blur(style.blur)
            .with_tint(style.tint.r, style.tint.g, style.tint.b, style.tint.a)
            .with_saturation(style.saturation)
            .with_brightness(style.brightness)
            .with_noise(style.noise)
            .with_border_thickness(style.border_thickness);
            self.batch.push_glass(glass);
            return;
        }

        let (color, color2, gradient_params, fill_type) = self.brush_to_colors(&brush);
        let (clip_bounds, clip_radius, clip_type) = self.get_clip_data();

        // Transform gradient params to screen space
        let is_radial = fill_type == FillType::RadialGradient;
        let transformed_gradient_params = if fill_type != FillType::Solid {
            self.transform_gradient_params(gradient_params, is_radial)
        } else {
            gradient_params
        };

        let primitive = GpuPrimitive {
            bounds: [
                transformed_center.x - transformed_radius,
                transformed_center.y - transformed_radius,
                transformed_radius * 2.0,
                transformed_radius * 2.0,
            ],
            corner_radius: [0.0; 4], // Not used for circles
            color,
            color2,
            border: [0.0; 4],
            border_color: [0.0; 4],
            shadow: [0.0; 4],
            shadow_color: [0.0; 4],
            clip_bounds,
            clip_radius,
            gradient_params: transformed_gradient_params,
            type_info: [
                PrimitiveType::Circle as u32,
                fill_type as u32,
                clip_type as u32,
                self.z_layer,
            ],
        };

        if self.is_foreground {
            self.batch.push_foreground(primitive);
        } else {
            self.batch.push(primitive);
        }
    }

    fn stroke_circle(&mut self, center: Point, radius: f32, stroke: &Stroke, brush: Brush) {
        let transformed_center = self.transform_point(center);
        let affine = self.current_affine();
        let a = affine.elements[0];
        let b = affine.elements[1];
        let c = affine.elements[2];
        let d = affine.elements[3];
        let scale = ((a * a + b * b).sqrt() + (c * c + d * d).sqrt()) / 2.0;
        let transformed_radius = radius * scale;

        let (color, _, gradient_params, fill_type) = self.brush_to_colors(&brush);
        let (clip_bounds, clip_radius, clip_type) = self.get_clip_data();

        // Transform gradient params to screen space
        let is_radial = fill_type == FillType::RadialGradient;
        let transformed_gradient_params = if fill_type != FillType::Solid {
            self.transform_gradient_params(gradient_params, is_radial)
        } else {
            gradient_params
        };

        let primitive = GpuPrimitive {
            bounds: [
                transformed_center.x - transformed_radius,
                transformed_center.y - transformed_radius,
                transformed_radius * 2.0,
                transformed_radius * 2.0,
            ],
            corner_radius: [0.0; 4],
            color: [0.0, 0.0, 0.0, 0.0], // Transparent fill
            color2: [0.0, 0.0, 0.0, 0.0],
            border: [stroke.width * scale, 0.0, 0.0, 0.0],
            border_color: color,
            shadow: [0.0; 4],
            shadow_color: [0.0; 4],
            clip_bounds,
            clip_radius,
            gradient_params: transformed_gradient_params,
            type_info: [
                PrimitiveType::Circle as u32,
                fill_type as u32,
                clip_type as u32,
                self.z_layer,
            ],
        };

        if self.is_foreground {
            self.batch.push_foreground(primitive);
        } else {
            self.batch.push(primitive);
        }
    }

    fn draw_text(&mut self, text: &str, origin: Point, style: &TextStyle) {
        use blinc_core::{TextAlign, TextBaseline};
        use blinc_text::{TextAlignment, TextAnchor};

        // Check if text context is available
        if self.text_ctx.is_none() {
            return;
        }

        // Transform origin by current transform
        let transformed_origin = self.transform_point(origin);

        // Get current opacity
        let opacity = self.combined_opacity();

        // Get clip data before borrowing text_ctx
        let (clip_bounds, _, _) = self.get_clip_data();

        // Convert TextStyle color to [f32; 4] with opacity applied
        let color = [
            style.color.r,
            style.color.g,
            style.color.b,
            style.color.a * opacity,
        ];

        // Map TextAlign to TextAlignment
        let alignment = match style.align {
            TextAlign::Left => TextAlignment::Left,
            TextAlign::Center => TextAlignment::Center,
            TextAlign::Right => TextAlignment::Right,
        };

        // Map TextBaseline to TextAnchor
        let anchor = match style.baseline {
            TextBaseline::Top => TextAnchor::Top,
            TextBaseline::Middle => TextAnchor::Center,
            TextBaseline::Alphabetic => TextAnchor::Baseline,
            TextBaseline::Bottom => TextAnchor::Baseline, // Approximate with baseline
        };

        // Now borrow text_ctx and prepare glyphs
        let text_ctx = self.text_ctx.as_mut().unwrap();
        if let Ok(mut glyphs) = text_ctx.prepare_text_with_options(
            text,
            transformed_origin.x,
            transformed_origin.y,
            style.size,
            color,
            anchor,
            alignment,
            None,  // No width constraint
            false, // No wrap for canvas text
        ) {
            // Apply current clip bounds to all glyphs
            for glyph in &mut glyphs {
                glyph.clip_bounds = clip_bounds;
            }

            // Add glyphs to batch
            for glyph in glyphs {
                self.batch.push_glyph(glyph);
            }
        }
    }

    fn draw_image(&mut self, _image: ImageId, _rect: Rect, _options: &ImageOptions) {
        // Image rendering would require:
        // 1. Texture loading and caching
        // 2. A separate image rendering pipeline
        // This is a placeholder for now
    }

    fn draw_shadow(&mut self, rect: Rect, corner_radius: CornerRadius, shadow: Shadow) {
        let transformed = self.transform_rect(rect);
        let scaled_radius = self.scale_corner_radius(corner_radius);
        let opacity = self.combined_opacity();
        let (clip_bounds, clip_radius, clip_type) = self.get_clip_data();

        let primitive = GpuPrimitive {
            bounds: [
                transformed.x(),
                transformed.y(),
                transformed.width(),
                transformed.height(),
            ],
            corner_radius: [
                scaled_radius.top_left,
                scaled_radius.top_right,
                scaled_radius.bottom_right,
                scaled_radius.bottom_left,
            ],
            color: [0.0, 0.0, 0.0, 0.0], // Shadow is not filled
            color2: [0.0, 0.0, 0.0, 0.0],
            border: [0.0; 4],
            border_color: [0.0; 4],
            shadow: [shadow.offset_x, shadow.offset_y, shadow.blur, shadow.spread],
            shadow_color: [
                shadow.color.r,
                shadow.color.g,
                shadow.color.b,
                shadow.color.a * opacity,
            ],
            clip_bounds,
            clip_radius,
            gradient_params: [0.0, 0.0, 1.0, 0.0],
            type_info: [
                PrimitiveType::Shadow as u32,
                FillType::Solid as u32,
                clip_type as u32,
                self.z_layer,
            ],
        };

        if self.is_foreground {
            self.batch.push_foreground(primitive);
        } else {
            self.batch.push(primitive);
        }
    }

    fn draw_inner_shadow(&mut self, rect: Rect, corner_radius: CornerRadius, shadow: Shadow) {
        let transformed = self.transform_rect(rect);
        let scaled_radius = self.scale_corner_radius(corner_radius);
        let opacity = self.combined_opacity();
        let (clip_bounds, clip_radius, clip_type) = self.get_clip_data();

        let primitive = GpuPrimitive {
            bounds: [
                transformed.x(),
                transformed.y(),
                transformed.width(),
                transformed.height(),
            ],
            corner_radius: [
                scaled_radius.top_left,
                scaled_radius.top_right,
                scaled_radius.bottom_right,
                scaled_radius.bottom_left,
            ],
            color: [0.0, 0.0, 0.0, 0.0], // Inner shadow is not filled
            color2: [0.0, 0.0, 0.0, 0.0],
            border: [0.0; 4],
            border_color: [0.0; 4],
            shadow: [shadow.offset_x, shadow.offset_y, shadow.blur, shadow.spread],
            shadow_color: [
                shadow.color.r,
                shadow.color.g,
                shadow.color.b,
                shadow.color.a * opacity,
            ],
            clip_bounds,
            clip_radius,
            gradient_params: [0.0, 0.0, 1.0, 0.0],
            type_info: [
                PrimitiveType::InnerShadow as u32,
                FillType::Solid as u32,
                clip_type as u32,
                self.z_layer,
            ],
        };

        if self.is_foreground {
            self.batch.push_foreground(primitive);
        } else {
            self.batch.push(primitive);
        }
    }

    fn draw_circle_shadow(&mut self, center: Point, radius: f32, shadow: Shadow) {
        let transformed_center = self.transform_point(center);
        let opacity = self.combined_opacity();
        let (clip_bounds, clip_radius, clip_type) = self.get_clip_data();

        // Store circle as bounds where the circle fits
        let size = radius * 2.0;
        let primitive = GpuPrimitive {
            bounds: [
                transformed_center.x - radius,
                transformed_center.y - radius,
                size,
                size,
            ],
            corner_radius: [radius, radius, radius, radius], // Used as circle radius indicator
            color: [0.0, 0.0, 0.0, 0.0],
            color2: [0.0, 0.0, 0.0, 0.0],
            border: [0.0; 4],
            border_color: [0.0; 4],
            shadow: [shadow.offset_x, shadow.offset_y, shadow.blur, shadow.spread],
            shadow_color: [
                shadow.color.r,
                shadow.color.g,
                shadow.color.b,
                shadow.color.a * opacity,
            ],
            clip_bounds,
            clip_radius,
            gradient_params: [0.0, 0.0, 1.0, 0.0],
            type_info: [
                PrimitiveType::CircleShadow as u32,
                FillType::Solid as u32,
                clip_type as u32,
                self.z_layer,
            ],
        };

        if self.is_foreground {
            self.batch.push_foreground(primitive);
        } else {
            self.batch.push(primitive);
        }
    }

    fn draw_circle_inner_shadow(&mut self, center: Point, radius: f32, shadow: Shadow) {
        let transformed_center = self.transform_point(center);
        let opacity = self.combined_opacity();
        let (clip_bounds, clip_radius, clip_type) = self.get_clip_data();

        let size = radius * 2.0;
        let primitive = GpuPrimitive {
            bounds: [
                transformed_center.x - radius,
                transformed_center.y - radius,
                size,
                size,
            ],
            corner_radius: [radius, radius, radius, radius],
            color: [0.0, 0.0, 0.0, 0.0],
            color2: [0.0, 0.0, 0.0, 0.0],
            border: [0.0; 4],
            border_color: [0.0; 4],
            shadow: [shadow.offset_x, shadow.offset_y, shadow.blur, shadow.spread],
            shadow_color: [
                shadow.color.r,
                shadow.color.g,
                shadow.color.b,
                shadow.color.a * opacity,
            ],
            clip_bounds,
            clip_radius,
            gradient_params: [0.0, 0.0, 1.0, 0.0],
            type_info: [
                PrimitiveType::CircleInnerShadow as u32,
                FillType::Solid as u32,
                clip_type as u32,
                self.z_layer,
            ],
        };

        if self.is_foreground {
            self.batch.push_foreground(primitive);
        } else {
            self.batch.push(primitive);
        }
    }

    fn sdf_build(&mut self, f: &mut dyn FnMut(&mut dyn SdfBuilder)) {
        let mut builder = GpuSdfBuilder::new(self);
        f(&mut builder);
    }

    fn set_camera(&mut self, camera: &Camera) {
        self.camera = Some(camera.clone());
        self.is_3d = true;
    }

    fn draw_mesh(&mut self, _mesh: MeshId, _material: MaterialId, _transform: Mat4) {
        // 3D mesh rendering is not yet implemented
        // Would require a full 3D rendering pipeline
    }

    fn draw_mesh_instanced(&mut self, _mesh: MeshId, _instances: &[MeshInstance]) {
        // 3D mesh rendering is not yet implemented
    }

    fn add_light(&mut self, light: Light) {
        self.lights.push(light);
    }

    fn set_environment(&mut self, _env: &Environment) {
        // 3D environment is not yet implemented
    }

    fn billboard_draw(
        &mut self,
        _size: Size,
        _transform: Mat4,
        _facing: BillboardFacing,
        f: &mut dyn FnMut(&mut dyn DrawContext),
    ) {
        // For now, just execute the 2D content without the billboard transform
        // Real implementation would require 3D projection
        f(self);
    }

    fn viewport_3d_draw(
        &mut self,
        _rect: Rect,
        camera: &Camera,
        f: &mut dyn FnMut(&mut dyn DrawContext),
    ) {
        // Set up 3D context
        let was_3d = self.is_3d;
        let old_camera = self.camera.take();
        self.set_camera(camera);

        // Execute 3D drawing
        f(self);

        // Restore 2D context
        self.is_3d = was_3d;
        self.camera = old_camera;
    }

    fn draw_sdf_viewport(&mut self, rect: Rect, viewport: &Sdf3DViewport) {
        // Transform the rect to screen coordinates (like fill_rect does)
        let transformed = self.transform_rect(rect);

        // Get current clip bounds and intersect with the viewport
        let (clip_bounds, _, _) = self.get_clip_data();
        let clip_min_x = clip_bounds[0];
        let clip_min_y = clip_bounds[1];
        let clip_max_x = clip_bounds[0] + clip_bounds[2];
        let clip_max_y = clip_bounds[1] + clip_bounds[3];

        // Original viewport bounds
        let orig_x = transformed.x();
        let orig_y = transformed.y();
        let orig_w = transformed.width();
        let orig_h = transformed.height();

        // Intersect viewport with clip region
        let clipped_x = orig_x.max(clip_min_x);
        let clipped_y = orig_y.max(clip_min_y);
        let clipped_right = (orig_x + orig_w).min(clip_max_x);
        let clipped_bottom = (orig_y + orig_h).min(clip_max_y);
        let clipped_w = (clipped_right - clipped_x).max(0.0);
        let clipped_h = (clipped_bottom - clipped_y).max(0.0);

        // Skip if viewport is fully clipped
        if clipped_w <= 0.0 || clipped_h <= 0.0 {
            return;
        }

        // Calculate UV offset and scale for clipped viewports
        let uv_offset_x = if orig_w > 0.0 { (clipped_x - orig_x) / orig_w } else { 0.0 };
        let uv_offset_y = if orig_h > 0.0 { (clipped_y - orig_y) / orig_h } else { 0.0 };
        let uv_scale_x = if orig_w > 0.0 { clipped_w / orig_w } else { 1.0 };
        let uv_scale_y = if orig_h > 0.0 { clipped_h / orig_h } else { 1.0 };

        // Create the uniform data for the shader
        // Must match the WGSL SdfUniform struct layout exactly
        let uniforms = Sdf3DUniform {
            camera_pos: [viewport.camera_pos.x, viewport.camera_pos.y, viewport.camera_pos.z, 1.0],
            camera_dir: [viewport.camera_dir.x, viewport.camera_dir.y, viewport.camera_dir.z, 0.0],
            camera_up: [viewport.camera_up.x, viewport.camera_up.y, viewport.camera_up.z, 0.0],
            camera_right: [
                viewport.camera_right.x,
                viewport.camera_right.y,
                viewport.camera_right.z,
                0.0,
            ],
            // Use ORIGINAL resolution for correct aspect ratio calculation
            resolution: [orig_w, orig_h],
            time: viewport.time,
            fov: viewport.fov,
            max_steps: viewport.max_steps,
            max_distance: viewport.max_distance,
            epsilon: viewport.epsilon,
            _padding: 0.0,
            uv_offset: [uv_offset_x, uv_offset_y],
            uv_scale: [uv_scale_x, uv_scale_y],
        };

        // Create and push the 3D viewport with CLIPPED bounds
        let viewport_3d = Viewport3D {
            shader_wgsl: viewport.shader_wgsl.clone(),
            uniforms,
            bounds: [clipped_x, clipped_y, clipped_w, clipped_h],
            lights: viewport.lights.clone(),
        };

        self.batch.push_viewport_3d(viewport_3d);
    }

    fn draw_particles(&mut self, rect: Rect, particle_data: &ParticleSystemData) {
        use crate::particles::{GpuEmitter, GpuForce};
        use crate::primitives::ParticleViewport3D;

        // Transform the rect to screen coordinates
        let transformed = self.transform_rect(rect);

        // Get current clip bounds and intersect with the viewport
        let (clip_bounds, _, _) = self.get_clip_data();
        let clip_min_x = clip_bounds[0];
        let clip_min_y = clip_bounds[1];
        let clip_max_x = clip_bounds[0] + clip_bounds[2];
        let clip_max_y = clip_bounds[1] + clip_bounds[3];

        // Original viewport bounds
        let orig_x = transformed.x();
        let orig_y = transformed.y();
        let orig_w = transformed.width();
        let orig_h = transformed.height();

        // Intersect viewport with clip region
        let clipped_x = orig_x.max(clip_min_x);
        let clipped_y = orig_y.max(clip_min_y);
        let clipped_right = (orig_x + orig_w).min(clip_max_x);
        let clipped_bottom = (orig_y + orig_h).min(clip_max_y);
        let clipped_w = (clipped_right - clipped_x).max(0.0);
        let clipped_h = (clipped_bottom - clipped_y).max(0.0);

        // Skip if viewport is fully clipped
        if clipped_w <= 0.0 || clipped_h <= 0.0 {
            return;
        }

        // Skip if system is not playing
        if !particle_data.playing {
            return;
        }

        // Convert emitter shape to GPU format
        let (shape_type, shape_params) = match &particle_data.emitter {
            ParticleEmitterShape::Point => (0u32, [0.0f32; 4]),
            ParticleEmitterShape::Sphere { radius } => (1u32, [*radius, 0.0, 0.0, 0.0]),
            ParticleEmitterShape::Hemisphere { radius } => (2u32, [*radius, 0.0, 0.0, 0.0]),
            ParticleEmitterShape::Cone { angle, radius } => (3u32, [*angle, *radius, 0.0, 0.0]),
            ParticleEmitterShape::Box { half_extents } => {
                (4u32, [half_extents.x, half_extents.y, half_extents.z, 0.0])
            }
            ParticleEmitterShape::Circle { radius } => (5u32, [*radius, 0.0, 0.0, 0.0]),
        };

        // Create GPU emitter
        let emitter = GpuEmitter {
            position_shape: [
                particle_data.emitter_position.x,
                particle_data.emitter_position.y,
                particle_data.emitter_position.z,
                shape_type as f32,
            ],
            shape_params,
            direction_randomness: [
                particle_data.direction.x,
                particle_data.direction.y,
                particle_data.direction.z,
                particle_data.direction_randomness,
            ],
            emission_config: [
                particle_data.emission_rate,
                particle_data.burst_count, // burst count for one-shot effects
                0.0, // spawn accumulated (deprecated)
                particle_data.gravity_scale,
            ],
            lifetime_speed: [
                particle_data.lifetime.0,
                particle_data.lifetime.1,
                particle_data.start_speed.0,
                particle_data.start_speed.1,
            ],
            size_config: [
                particle_data.start_size.0,
                particle_data.start_size.1,
                particle_data.end_size.0,
                particle_data.end_size.1,
            ],
            start_color: [
                particle_data.start_color.r,
                particle_data.start_color.g,
                particle_data.start_color.b,
                particle_data.start_color.a,
            ],
            mid_color: [
                particle_data.mid_color.r,
                particle_data.mid_color.g,
                particle_data.mid_color.b,
                particle_data.mid_color.a,
            ],
            end_color: [
                particle_data.end_color.r,
                particle_data.end_color.g,
                particle_data.end_color.b,
                particle_data.end_color.a,
            ],
        };

        // Convert forces to GPU format
        let forces: Vec<GpuForce> = particle_data
            .forces
            .iter()
            .map(|force| match force {
                ParticleForce::Gravity(dir) => GpuForce {
                    type_strength: [0.0, 1.0, 0.0, 0.0],
                    direction_params: [dir.x, dir.y, dir.z, 0.0],
                },
                ParticleForce::Wind { direction, strength, turbulence } => GpuForce {
                    type_strength: [1.0, *strength, 0.0, 0.0],
                    direction_params: [direction.x, direction.y, direction.z, *turbulence],
                },
                ParticleForce::Vortex { axis, center: _, strength } => GpuForce {
                    type_strength: [2.0, *strength, 0.0, 0.0],
                    direction_params: [axis.x, axis.y, axis.z, 0.0],
                },
                ParticleForce::Drag(coefficient) => GpuForce {
                    type_strength: [3.0, *coefficient, 0.0, 0.0],
                    direction_params: [0.0, 0.0, 0.0, 0.0],
                },
                ParticleForce::Turbulence { strength, frequency } => GpuForce {
                    type_strength: [4.0, *strength, 0.0, 0.0],
                    direction_params: [0.0, 0.0, 0.0, *frequency],
                },
                ParticleForce::Attractor { position, strength } => GpuForce {
                    type_strength: [5.0, *strength, 0.0, 0.0],
                    direction_params: [position.x, position.y, position.z, 0.0],
                },
            })
            .collect();

        // Determine blend mode
        let blend_mode = match particle_data.blend_mode {
            ParticleBlendMode::Alpha => 0,
            ParticleBlendMode::Additive => 1,
            ParticleBlendMode::Multiply => 2,
        };

        // Create and push the particle viewport
        let viewport = ParticleViewport3D {
            emitter,
            forces,
            max_particles: particle_data.max_particles,
            bounds: [clipped_x, clipped_y, clipped_w, clipped_h],
            camera_pos: [
                particle_data.camera_pos.x,
                particle_data.camera_pos.y,
                particle_data.camera_pos.z,
            ],
            camera_target: [
                particle_data.camera_pos.x + particle_data.camera_dir.x,
                particle_data.camera_pos.y + particle_data.camera_dir.y,
                particle_data.camera_pos.z + particle_data.camera_dir.z,
            ],
            camera_up: [
                particle_data.camera_up.x,
                particle_data.camera_up.y,
                particle_data.camera_up.z,
            ],
            fov: 0.8, // Default FOV
            time: particle_data.time,
            delta_time: particle_data.delta_time,
            blend_mode,
            playing: particle_data.playing,
        };

        self.batch.push_particle_viewport(viewport);
    }

    fn push_layer(&mut self, config: LayerConfig) {
        // Record current state indices for restoration on pop
        let state = LayerState {
            config: config.clone(),
            primitive_start: self.batch.primitive_count(),
            foreground_primitive_start: self.batch.foreground_primitive_count(),
            path_start: self.batch.path_vertex_count(),
            foreground_path_start: self.batch.foreground_path_vertex_count(),
            parent_state_indices: (
                self.transform_stack.len(),
                self.opacity_stack.len(),
                self.blend_mode_stack.len(),
                self.clip_stack.len(),
            ),
        };
        self.layer_stack.push(state);

        // Apply layer's blend mode if not Normal
        if config.blend_mode != BlendMode::Normal {
            self.blend_mode_stack.push(config.blend_mode);
        }

        // Apply layer's opacity if less than 1.0
        if config.opacity < 1.0 {
            self.opacity_stack.push(config.opacity);
        }

        // Record layer command for GPU renderer to process
        self.batch
            .push_layer_command(crate::primitives::LayerCommand::Push {
                config: config.clone(),
            });
    }

    fn pop_layer(&mut self) {
        if let Some(state) = self.layer_stack.pop() {
            // Restore parent state by trimming stacks to their saved indices
            let (transform_idx, opacity_idx, blend_idx, clip_idx) = state.parent_state_indices;

            // Only truncate if we pushed additional state for this layer
            // (don't go below the base state)
            if self.transform_stack.len() > transform_idx {
                self.transform_stack.truncate(transform_idx.max(1));
            }
            if self.opacity_stack.len() > opacity_idx {
                self.opacity_stack.truncate(opacity_idx.max(1));
            }
            if self.blend_mode_stack.len() > blend_idx {
                self.blend_mode_stack.truncate(blend_idx.max(1));
            }
            if self.clip_stack.len() > clip_idx {
                self.clip_stack.truncate(clip_idx);
            }

            // Record layer command for GPU renderer to process
            self.batch
                .push_layer_command(crate::primitives::LayerCommand::Pop);
        }
    }

    fn sample_layer(&mut self, id: LayerId, source_rect: Rect, dest_rect: Rect) {
        // Record sample command for GPU renderer to process
        self.batch
            .push_layer_command(crate::primitives::LayerCommand::Sample {
                id,
                source: source_rect,
                dest: dest_rect,
            });
    }

    fn viewport_size(&self) -> Size {
        self.viewport
    }

    fn is_3d_context(&self) -> bool {
        self.is_3d
    }

    fn current_opacity(&self) -> f32 {
        self.combined_opacity()
    }

    fn current_blend_mode(&self) -> BlendMode {
        self.blend_mode_stack
            .last()
            .copied()
            .unwrap_or(BlendMode::Normal)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// GPU SDF Builder
// ─────────────────────────────────────────────────────────────────────────────

/// SDF builder that directly emits GPU primitives
struct GpuSdfBuilder<'a, 'b> {
    ctx: &'a mut GpuPaintContext<'b>,
    shapes: Vec<SdfShapeData>,
}

#[derive(Clone, Debug)]
enum SdfShapeData {
    Rect {
        rect: Rect,
        corner_radius: CornerRadius,
    },
    Circle {
        center: Point,
        radius: f32,
    },
    Ellipse {
        center: Point,
        radii: (f32, f32),
    },
}

impl<'a, 'b> GpuSdfBuilder<'a, 'b> {
    fn new(ctx: &'a mut GpuPaintContext<'b>) -> Self {
        Self {
            ctx,
            shapes: Vec::new(),
        }
    }

    fn add_shape(&mut self, shape: SdfShapeData) -> ShapeId {
        let id = ShapeId(self.shapes.len() as u32);
        self.shapes.push(shape);
        id
    }
}

impl<'a, 'b> SdfBuilder for GpuSdfBuilder<'a, 'b> {
    fn rect(&mut self, rect: Rect, corner_radius: CornerRadius) -> ShapeId {
        self.add_shape(SdfShapeData::Rect {
            rect,
            corner_radius,
        })
    }

    fn circle(&mut self, center: Point, radius: f32) -> ShapeId {
        self.add_shape(SdfShapeData::Circle { center, radius })
    }

    fn ellipse(&mut self, center: Point, radii: blinc_core::Vec2) -> ShapeId {
        self.add_shape(SdfShapeData::Ellipse {
            center,
            radii: (radii.x, radii.y),
        })
    }

    fn line(&mut self, _from: Point, _to: Point, _width: f32) -> ShapeId {
        // Line SDF would need a custom primitive type
        ShapeId(self.shapes.len() as u32)
    }

    fn arc(
        &mut self,
        _center: Point,
        _radius: f32,
        _start: f32,
        _end: f32,
        _width: f32,
    ) -> ShapeId {
        ShapeId(self.shapes.len() as u32)
    }

    fn quad_bezier(&mut self, _p0: Point, _p1: Point, _p2: Point, _width: f32) -> ShapeId {
        ShapeId(self.shapes.len() as u32)
    }

    fn union(&mut self, _a: ShapeId, _b: ShapeId) -> ShapeId {
        // Boolean operations would require more complex SDF evaluation
        ShapeId(self.shapes.len() as u32)
    }

    fn subtract(&mut self, _a: ShapeId, _b: ShapeId) -> ShapeId {
        ShapeId(self.shapes.len() as u32)
    }

    fn intersect(&mut self, _a: ShapeId, _b: ShapeId) -> ShapeId {
        ShapeId(self.shapes.len() as u32)
    }

    fn smooth_union(&mut self, _a: ShapeId, _b: ShapeId, _radius: f32) -> ShapeId {
        ShapeId(self.shapes.len() as u32)
    }

    fn smooth_subtract(&mut self, _a: ShapeId, _b: ShapeId, _radius: f32) -> ShapeId {
        ShapeId(self.shapes.len() as u32)
    }

    fn smooth_intersect(&mut self, _a: ShapeId, _b: ShapeId, _radius: f32) -> ShapeId {
        ShapeId(self.shapes.len() as u32)
    }

    fn round(&mut self, _shape: ShapeId, _radius: f32) -> ShapeId {
        ShapeId(self.shapes.len() as u32)
    }

    fn outline(&mut self, _shape: ShapeId, _width: f32) -> ShapeId {
        ShapeId(self.shapes.len() as u32)
    }

    fn offset(&mut self, _shape: ShapeId, _distance: f32) -> ShapeId {
        ShapeId(self.shapes.len() as u32)
    }

    fn fill(&mut self, shape: ShapeId, brush: Brush) {
        if let Some(shape_data) = self.shapes.get(shape.0 as usize) {
            match shape_data.clone() {
                SdfShapeData::Rect {
                    rect,
                    corner_radius,
                } => {
                    self.ctx.fill_rect(rect, corner_radius, brush);
                }
                SdfShapeData::Circle { center, radius } => {
                    self.ctx.fill_circle(center, radius, brush);
                }
                SdfShapeData::Ellipse { center, radii } => {
                    // Ellipse would need its own primitive type
                    // For now, approximate with the larger radius
                    let radius = radii.0.max(radii.1);
                    self.ctx.fill_circle(center, radius, brush);
                }
            }
        }
    }

    fn stroke(&mut self, shape: ShapeId, stroke: &Stroke, brush: Brush) {
        if let Some(shape_data) = self.shapes.get(shape.0 as usize) {
            match shape_data.clone() {
                SdfShapeData::Rect {
                    rect,
                    corner_radius,
                } => {
                    self.ctx.stroke_rect(rect, corner_radius, stroke, brush);
                }
                SdfShapeData::Circle { center, radius } => {
                    self.ctx.stroke_circle(center, radius, stroke, brush);
                }
                SdfShapeData::Ellipse { center, radii } => {
                    let radius = radii.0.max(radii.1);
                    self.ctx.stroke_circle(center, radius, stroke, brush);
                }
            }
        }
    }

    fn shadow(&mut self, shape: ShapeId, shadow: Shadow) {
        if let Some(shape_data) = self.shapes.get(shape.0 as usize) {
            match shape_data.clone() {
                SdfShapeData::Rect {
                    rect,
                    corner_radius,
                } => {
                    self.ctx.draw_shadow(rect, corner_radius, shadow);
                }
                SdfShapeData::Circle { center, radius } => {
                    let rect = Rect::new(
                        center.x - radius,
                        center.y - radius,
                        radius * 2.0,
                        radius * 2.0,
                    );
                    self.ctx.draw_shadow(rect, radius.into(), shadow);
                }
                SdfShapeData::Ellipse { center, radii } => {
                    let rect = Rect::new(
                        center.x - radii.0,
                        center.y - radii.1,
                        radii.0 * 2.0,
                        radii.1 * 2.0,
                    );
                    self.ctx.draw_shadow(rect, CornerRadius::default(), shadow);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blinc_core::Color;

    #[test]
    fn test_gpu_paint_context_creation() {
        let ctx = GpuPaintContext::new(800.0, 600.0);
        assert_eq!(ctx.viewport_size(), Size::new(800.0, 600.0));
        assert!(!ctx.is_3d_context());
        assert_eq!(ctx.current_opacity(), 1.0);
    }

    #[test]
    fn test_fill_rect() {
        let mut ctx = GpuPaintContext::new(800.0, 600.0);

        ctx.fill_rect(
            Rect::new(10.0, 20.0, 100.0, 50.0),
            8.0.into(),
            Color::BLUE.into(),
        );

        assert_eq!(ctx.batch().primitive_count(), 1);
    }

    #[test]
    fn test_transform_stack() {
        let mut ctx = GpuPaintContext::new(800.0, 600.0);

        ctx.push_transform(Transform::translate(10.0, 20.0));
        ctx.fill_rect(
            Rect::new(0.0, 0.0, 100.0, 50.0),
            0.0.into(),
            Color::RED.into(),
        );

        let batch = ctx.batch();
        let prim = &batch.primitives[0];

        // The rect should be translated
        assert_eq!(prim.bounds[0], 10.0);
        assert_eq!(prim.bounds[1], 20.0);
    }

    #[test]
    fn test_opacity_stack() {
        let mut ctx = GpuPaintContext::new(800.0, 600.0);

        ctx.push_opacity(0.5);
        ctx.push_opacity(0.5);

        assert_eq!(ctx.current_opacity(), 0.25);

        ctx.pop_opacity();
        assert_eq!(ctx.current_opacity(), 0.5);
    }

    #[test]
    fn test_execute_commands() {
        use blinc_core::RecordingContext;

        let mut recording = RecordingContext::new(Size::new(800.0, 600.0));
        recording.fill_rect(
            Rect::new(10.0, 20.0, 100.0, 50.0),
            4.0.into(),
            Color::GREEN.into(),
        );

        let commands = recording.take_commands();

        let mut ctx = GpuPaintContext::new(800.0, 600.0);
        ctx.execute_commands(&commands);

        assert_eq!(ctx.batch().primitive_count(), 1);
    }

    #[test]
    fn test_layer_stack_tracking() {
        let mut ctx = GpuPaintContext::new(800.0, 600.0);

        // Initial state
        assert_eq!(ctx.layer_stack.len(), 0);
        assert_eq!(ctx.current_opacity(), 1.0);
        assert_eq!(ctx.current_blend_mode(), BlendMode::Normal);

        // Push a layer with opacity and blend mode
        let config = LayerConfig {
            id: None,
            position: None,
            size: None,
            blend_mode: BlendMode::Multiply,
            opacity: 0.5,
            depth: false,
            effects: Vec::new(),
        };
        ctx.push_layer(config);

        // Layer should be tracked
        assert_eq!(ctx.layer_stack.len(), 1);
        // Blend mode and opacity should be applied
        assert_eq!(ctx.current_opacity(), 0.5);
        assert_eq!(ctx.current_blend_mode(), BlendMode::Multiply);

        // Draw something within the layer
        ctx.fill_rect(
            Rect::new(10.0, 10.0, 100.0, 100.0),
            0.0.into(),
            Color::RED.into(),
        );

        // Pop the layer
        ctx.pop_layer();

        // State should be restored
        assert_eq!(ctx.layer_stack.len(), 0);
        assert_eq!(ctx.current_opacity(), 1.0);
        assert_eq!(ctx.current_blend_mode(), BlendMode::Normal);
    }

    #[test]
    fn test_nested_layers() {
        let mut ctx = GpuPaintContext::new(800.0, 600.0);

        // Push first layer
        let config1 = LayerConfig {
            id: None,
            position: None,
            size: None,
            blend_mode: BlendMode::Normal,
            opacity: 0.8,
            depth: false,
            effects: Vec::new(),
        };
        ctx.push_layer(config1);
        assert_eq!(ctx.layer_stack.len(), 1);
        assert_eq!(ctx.current_opacity(), 0.8);

        // Push second layer (nested)
        let config2 = LayerConfig {
            id: None,
            position: None,
            size: None,
            blend_mode: BlendMode::Screen,
            opacity: 0.5,
            depth: false,
            effects: Vec::new(),
        };
        ctx.push_layer(config2);
        assert_eq!(ctx.layer_stack.len(), 2);
        // Opacity should be combined: 0.8 * 0.5 = 0.4
        assert!((ctx.current_opacity() - 0.4).abs() < 0.001);
        assert_eq!(ctx.current_blend_mode(), BlendMode::Screen);

        // Pop second layer
        ctx.pop_layer();
        assert_eq!(ctx.layer_stack.len(), 1);
        assert_eq!(ctx.current_opacity(), 0.8);

        // Pop first layer
        ctx.pop_layer();
        assert_eq!(ctx.layer_stack.len(), 0);
        assert_eq!(ctx.current_opacity(), 1.0);
    }
}
