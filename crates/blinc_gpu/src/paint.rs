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
    Affine2D, BillboardFacing, BlendMode, Brush, Camera, ClipShape, CornerRadius,
    DrawCommand, DrawContext, Environment, ImageId, ImageOptions, LayerConfig, LayerId,
    Light, Mat4, MeshId, MeshInstance, MaterialId, Path, Point, Rect, SdfBuilder, Shadow, ShapeId,
    Size, Stroke, TextStyle, Transform,
};

use crate::primitives::{ClipType, FillType, GpuPrimitive, PrimitiveBatch, PrimitiveType};

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
// GPU Paint Context
// ─────────────────────────────────────────────────────────────────────────────

/// GPU-backed implementation of DrawContext
///
/// This translates high-level drawing commands into GPU primitives that can
/// be efficiently rendered by the `GpuRenderer`.
pub struct GpuPaintContext {
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
}

impl GpuPaintContext {
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
        }
    }

    /// Get the current transform
    fn current_affine(&self) -> Affine2D {
        self.transform_stack.last().copied().unwrap_or(Affine2D::IDENTITY)
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

                let transformed: Vec<Point> = corners.iter().map(|p| self.transform_point(*p)).collect();

                let min_x = transformed.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
                let max_x = transformed.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max);
                let min_y = transformed.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
                let max_y = transformed.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max);

                ClipShape::Rect(Rect::new(min_x, min_y, max_x - min_x, max_y - min_y))
            }
            ClipShape::RoundedRect { rect, corner_radius } => {
                // Transform corners and compute AABB
                let corners = [
                    Point::new(rect.x(), rect.y()),
                    Point::new(rect.x() + rect.width(), rect.y()),
                    Point::new(rect.x() + rect.width(), rect.y() + rect.height()),
                    Point::new(rect.x(), rect.y() + rect.height()),
                ];

                let transformed: Vec<Point> = corners.iter().map(|p| self.transform_point(*p)).collect();

                let min_x = transformed.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
                let max_x = transformed.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max);
                let min_y = transformed.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
                let max_y = transformed.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max);

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

    /// Convert a Brush to GPU color components
    fn brush_to_colors(&self, brush: &Brush) -> ([f32; 4], [f32; 4], FillType) {
        let opacity = self.combined_opacity();
        match brush {
            Brush::Solid(color) => {
                let c = [color.r, color.g, color.b, color.a * opacity];
                (c, c, FillType::Solid)
            }
            Brush::Gradient(gradient) => {
                let (stops, fill_type) = match gradient {
                    blinc_core::Gradient::Linear { stops, .. } => (stops, FillType::LinearGradient),
                    blinc_core::Gradient::Radial { stops, .. } => (stops, FillType::RadialGradient),
                    // Conic gradients treated as radial for now (GPU shader would need enhancement)
                    blinc_core::Gradient::Conic { stops, .. } => (stops, FillType::RadialGradient),
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

                (c1, c2, fill_type)
            }
        }
    }

    /// Get clip data from the current clip stack
    /// Returns (clip_bounds, clip_radius, clip_type)
    fn get_clip_data(&self) -> ([f32; 4], [f32; 4], ClipType) {
        if self.clip_stack.is_empty() {
            // No clip - use large bounds
            return (
                [-10000.0, -10000.0, 100000.0, 100000.0],
                [0.0; 4],
                ClipType::None,
            );
        }

        // Use the topmost clip shape
        // Note: For multiple clips, a more sophisticated approach would be needed
        // (e.g., stencil buffer or computing intersection)
        let clip = self.clip_stack.last().unwrap();
        match clip {
            ClipShape::Rect(rect) => (
                [rect.x(), rect.y(), rect.width(), rect.height()],
                [0.0; 4],
                ClipType::Rect,
            ),
            ClipShape::RoundedRect { rect, corner_radius } => (
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

    /// Clear the batch
    pub fn clear(&mut self) {
        self.batch.clear();
        self.transform_stack = vec![Affine2D::IDENTITY];
        self.opacity_stack = vec![1.0];
        self.blend_mode_stack = vec![BlendMode::Normal];
        self.clip_stack.clear();
        self.is_3d = false;
        self.camera = None;
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
            DrawCommand::StrokePath { path, stroke, brush } => {
                self.stroke_path(path, stroke, brush.clone())
            }
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
            DrawCommand::DrawText { text, origin, style } => {
                self.draw_text(text, *origin, style)
            }
            DrawCommand::DrawImage { image, rect, options } => {
                self.draw_image(*image, *rect, options)
            }
            DrawCommand::DrawShadow {
                rect,
                corner_radius,
                shadow,
            } => self.draw_shadow(*rect, *corner_radius, *shadow),
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

impl DrawContext for GpuPaintContext {
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

    fn fill_path(&mut self, _path: &Path, _brush: Brush) {
        // Path rendering would require tessellation or more complex SDF generation
        // For now, this is a placeholder - real implementation would:
        // 1. Tessellate the path into triangles, or
        // 2. Generate SDF from the path, or
        // 3. Use a vector graphics renderer like lyon
    }

    fn stroke_path(&mut self, _path: &Path, _stroke: &Stroke, _brush: Brush) {
        // Same as fill_path - requires tessellation
    }

    fn fill_rect(&mut self, rect: Rect, corner_radius: CornerRadius, brush: Brush) {
        let transformed = self.transform_rect(rect);
        let (color, _color2, fill_type) = self.brush_to_colors(&brush);
        let (clip_bounds, clip_radius, clip_type) = self.get_clip_data();

        let primitive = GpuPrimitive {
            bounds: [
                transformed.x(),
                transformed.y(),
                transformed.width(),
                transformed.height(),
            ],
            corner_radius: [
                corner_radius.top_left,
                corner_radius.top_right,
                corner_radius.bottom_right,
                corner_radius.bottom_left,
            ],
            color,
            color2: [0.0; 4], // Gradient endpoint color (unused for solid fills)
            border: [0.0; 4],
            border_color: [0.0; 4],
            shadow: [0.0; 4],
            shadow_color: [0.0; 4],
            clip_bounds,
            clip_radius,
            type_info: [PrimitiveType::Rect as u32, fill_type as u32, clip_type as u32, 0],
        };

        self.batch.push(primitive);
    }

    fn stroke_rect(
        &mut self,
        rect: Rect,
        corner_radius: CornerRadius,
        stroke: &Stroke,
        brush: Brush,
    ) {
        let transformed = self.transform_rect(rect);
        let (color, _color2, fill_type) = self.brush_to_colors(&brush);
        let (clip_bounds, clip_radius, clip_type) = self.get_clip_data();

        let primitive = GpuPrimitive {
            bounds: [
                transformed.x(),
                transformed.y(),
                transformed.width(),
                transformed.height(),
            ],
            corner_radius: [
                corner_radius.top_left,
                corner_radius.top_right,
                corner_radius.bottom_right,
                corner_radius.bottom_left,
            ],
            color: [0.0, 0.0, 0.0, 0.0], // Transparent fill
            color2: [0.0, 0.0, 0.0, 0.0],
            border: [stroke.width, 0.0, 0.0, 0.0],
            border_color: color,
            shadow: [0.0; 4],
            shadow_color: [0.0; 4],
            clip_bounds,
            clip_radius,
            type_info: [PrimitiveType::Rect as u32, fill_type as u32, clip_type as u32, 0],
        };

        self.batch.push(primitive);
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

        let (color, color2, fill_type) = self.brush_to_colors(&brush);
        let (clip_bounds, clip_radius, clip_type) = self.get_clip_data();

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
            type_info: [PrimitiveType::Circle as u32, fill_type as u32, clip_type as u32, 0],
        };

        self.batch.push(primitive);
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

        let (color, _, fill_type) = self.brush_to_colors(&brush);
        let (clip_bounds, clip_radius, clip_type) = self.get_clip_data();

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
            type_info: [PrimitiveType::Circle as u32, fill_type as u32, clip_type as u32, 0],
        };

        self.batch.push(primitive);
    }

    fn draw_text(&mut self, _text: &str, _origin: Point, _style: &TextStyle) {
        // Text rendering would require:
        // 1. Font loading and glyph rasterization
        // 2. Glyph atlas management
        // 3. Text layout (shaping, line breaking)
        // This is a placeholder for now
    }

    fn draw_image(&mut self, _image: ImageId, _rect: Rect, _options: &ImageOptions) {
        // Image rendering would require:
        // 1. Texture loading and caching
        // 2. A separate image rendering pipeline
        // This is a placeholder for now
    }

    fn draw_shadow(&mut self, rect: Rect, corner_radius: CornerRadius, shadow: Shadow) {
        let transformed = self.transform_rect(rect);
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
                corner_radius.top_left,
                corner_radius.top_right,
                corner_radius.bottom_right,
                corner_radius.bottom_left,
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
            type_info: [PrimitiveType::Shadow as u32, FillType::Solid as u32, clip_type as u32, 0],
        };

        self.batch.push(primitive);
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

    fn add_light(&mut self, _light: Light) {
        // 3D lighting is not yet implemented
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

    fn push_layer(&mut self, _config: LayerConfig) {
        // Layer management would require offscreen render targets
        // This is a placeholder for now
    }

    fn pop_layer(&mut self) {
        // Layer management placeholder
    }

    fn sample_layer(&mut self, _id: LayerId, _source_rect: Rect, _dest_rect: Rect) {
        // Layer sampling placeholder
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
        self.blend_mode_stack.last().copied().unwrap_or(BlendMode::Normal)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// GPU SDF Builder
// ─────────────────────────────────────────────────────────────────────────────

/// SDF builder that directly emits GPU primitives
struct GpuSdfBuilder<'a> {
    ctx: &'a mut GpuPaintContext,
    shapes: Vec<SdfShapeData>,
}

#[derive(Clone, Debug)]
enum SdfShapeData {
    Rect { rect: Rect, corner_radius: CornerRadius },
    Circle { center: Point, radius: f32 },
    Ellipse { center: Point, radii: (f32, f32) },
}

impl<'a> GpuSdfBuilder<'a> {
    fn new(ctx: &'a mut GpuPaintContext) -> Self {
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

impl<'a> SdfBuilder for GpuSdfBuilder<'a> {
    fn rect(&mut self, rect: Rect, corner_radius: CornerRadius) -> ShapeId {
        self.add_shape(SdfShapeData::Rect { rect, corner_radius })
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
                SdfShapeData::Rect { rect, corner_radius } => {
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
                SdfShapeData::Rect { rect, corner_radius } => {
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
                SdfShapeData::Rect { rect, corner_radius } => {
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
}
