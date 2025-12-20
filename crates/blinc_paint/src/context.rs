//! Paint context - Canvas-like drawing API implementing DrawContext
//!
//! PaintContext provides a 2D-focused drawing API similar to HTML Canvas,
//! while implementing the unified DrawContext trait for GPU rendering.

use blinc_core::{
    BillboardFacing, BlendMode, Brush, Camera, ClipShape, CornerRadius, DrawCommand, DrawContext,
    Environment, ImageId, ImageOptions, LayerConfig, LayerId, Light, Mat4, MaterialId, MeshId,
    MeshInstance, Path, Point, Rect, RecordingContext, SdfBuilder, Shadow, Size, Stroke,
    TextStyle, Transform,
};

// Re-export stroke types for convenience
pub use blinc_core::{LineCap, LineJoin};

/// The paint context used for custom 2D drawing
///
/// PaintContext wraps a RecordingContext to record draw commands,
/// while providing a Canvas-like API for convenience.
pub struct PaintContext {
    recording: RecordingContext,
}

impl PaintContext {
    /// Create a new paint context with the given viewport size
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            recording: RecordingContext::new(Size::new(width, height)),
        }
    }

    /// Create from a Size
    pub fn from_size(size: Size) -> Self {
        Self {
            recording: RecordingContext::new(size),
        }
    }

    /// Get all recorded commands
    pub fn commands(&self) -> &[DrawCommand] {
        self.recording.commands()
    }

    /// Take ownership of recorded commands
    pub fn take_commands(&mut self) -> Vec<DrawCommand> {
        self.recording.take_commands()
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Canvas-like convenience API
    // ═══════════════════════════════════════════════════════════════════════════

    /// Fill a rectangle at (x, y) with width/height and a brush
    pub fn fill_rect_xywh(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        brush: impl Into<Brush>,
    ) {
        self.fill_rect(
            Rect::new(x, y, width, height),
            CornerRadius::default(),
            brush.into(),
        );
    }

    /// Stroke a rectangle at (x, y) with width/height
    pub fn stroke_rect_xywh(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        stroke: &Stroke,
        brush: impl Into<Brush>,
    ) {
        self.stroke_rect(
            Rect::new(x, y, width, height),
            CornerRadius::default(),
            stroke,
            brush.into(),
        );
    }

    /// Fill a rounded rectangle
    pub fn fill_rounded_rect_xywh(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        radius: f32,
        brush: impl Into<Brush>,
    ) {
        self.fill_rect(Rect::new(x, y, width, height), radius.into(), brush.into());
    }

    /// Fill a circle at (cx, cy) with radius
    pub fn fill_circle_xyr(&mut self, cx: f32, cy: f32, radius: f32, brush: impl Into<Brush>) {
        self.fill_circle(Point::new(cx, cy), radius, brush.into());
    }

    /// Stroke a circle at (cx, cy) with radius
    pub fn stroke_circle_xyr(
        &mut self,
        cx: f32,
        cy: f32,
        radius: f32,
        stroke: &Stroke,
        brush: impl Into<Brush>,
    ) {
        self.stroke_circle(Point::new(cx, cy), radius, stroke, brush.into());
    }

    /// Draw text at (x, y) with size and color
    pub fn draw_text_simple(
        &mut self,
        text: impl Into<String>,
        x: f32,
        y: f32,
        size: f32,
        color: blinc_core::Color,
    ) {
        self.draw_text(
            &text.into(),
            Point::new(x, y),
            &TextStyle::new(size).with_color(color),
        );
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Transform convenience methods
    // ═══════════════════════════════════════════════════════════════════════════

    /// Push a translation transform
    pub fn translate(&mut self, x: f32, y: f32) {
        self.push_transform(Transform::translate(x, y));
    }

    /// Push a scale transform
    pub fn scale(&mut self, sx: f32, sy: f32) {
        self.push_transform(Transform::scale(sx, sy));
    }

    /// Push a rotation transform (angle in radians)
    pub fn rotate(&mut self, angle: f32) {
        self.push_transform(Transform::rotate(angle));
    }
}

impl Default for PaintContext {
    fn default() -> Self {
        Self::new(0.0, 0.0)
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// DrawContext Implementation - delegates to RecordingContext
// ═══════════════════════════════════════════════════════════════════════════════

impl DrawContext for PaintContext {
    fn push_transform(&mut self, transform: Transform) {
        self.recording.push_transform(transform);
    }

    fn pop_transform(&mut self) {
        self.recording.pop_transform();
    }

    fn current_transform(&self) -> Transform {
        self.recording.current_transform()
    }

    fn push_clip(&mut self, shape: ClipShape) {
        self.recording.push_clip(shape);
    }

    fn pop_clip(&mut self) {
        self.recording.pop_clip();
    }

    fn push_opacity(&mut self, opacity: f32) {
        self.recording.push_opacity(opacity);
    }

    fn pop_opacity(&mut self) {
        self.recording.pop_opacity();
    }

    fn push_blend_mode(&mut self, mode: BlendMode) {
        self.recording.push_blend_mode(mode);
    }

    fn pop_blend_mode(&mut self) {
        self.recording.pop_blend_mode();
    }

    fn fill_path(&mut self, path: &Path, brush: Brush) {
        self.recording.fill_path(path, brush);
    }

    fn stroke_path(&mut self, path: &Path, stroke: &Stroke, brush: Brush) {
        self.recording.stroke_path(path, stroke, brush);
    }

    fn fill_rect(&mut self, rect: Rect, corner_radius: CornerRadius, brush: Brush) {
        self.recording.fill_rect(rect, corner_radius, brush);
    }

    fn stroke_rect(
        &mut self,
        rect: Rect,
        corner_radius: CornerRadius,
        stroke: &Stroke,
        brush: Brush,
    ) {
        self.recording
            .stroke_rect(rect, corner_radius, stroke, brush);
    }

    fn fill_circle(&mut self, center: Point, radius: f32, brush: Brush) {
        self.recording.fill_circle(center, radius, brush);
    }

    fn stroke_circle(&mut self, center: Point, radius: f32, stroke: &Stroke, brush: Brush) {
        self.recording.stroke_circle(center, radius, stroke, brush);
    }

    fn draw_text(&mut self, text: &str, origin: Point, style: &TextStyle) {
        self.recording.draw_text(text, origin, style);
    }

    fn draw_image(&mut self, image: ImageId, rect: Rect, options: &ImageOptions) {
        self.recording.draw_image(image, rect, options);
    }

    fn draw_shadow(&mut self, rect: Rect, corner_radius: CornerRadius, shadow: Shadow) {
        self.recording.draw_shadow(rect, corner_radius, shadow);
    }

    fn sdf_build(&mut self, f: &mut dyn FnMut(&mut dyn SdfBuilder)) {
        self.recording.sdf_build(f);
    }

    fn set_camera(&mut self, camera: &Camera) {
        self.recording.set_camera(camera);
    }

    fn draw_mesh(&mut self, mesh: MeshId, material: MaterialId, transform: Mat4) {
        self.recording.draw_mesh(mesh, material, transform);
    }

    fn draw_mesh_instanced(&mut self, mesh: MeshId, instances: &[MeshInstance]) {
        self.recording.draw_mesh_instanced(mesh, instances);
    }

    fn add_light(&mut self, light: Light) {
        self.recording.add_light(light);
    }

    fn set_environment(&mut self, env: &Environment) {
        self.recording.set_environment(env);
    }

    fn billboard_draw(
        &mut self,
        size: Size,
        transform: Mat4,
        facing: BillboardFacing,
        f: &mut dyn FnMut(&mut dyn DrawContext),
    ) {
        self.recording.billboard_draw(size, transform, facing, f);
    }

    fn viewport_3d_draw(
        &mut self,
        rect: Rect,
        camera: &Camera,
        f: &mut dyn FnMut(&mut dyn DrawContext),
    ) {
        self.recording.viewport_3d_draw(rect, camera, f);
    }

    fn push_layer(&mut self, config: LayerConfig) {
        self.recording.push_layer(config);
    }

    fn pop_layer(&mut self) {
        self.recording.pop_layer();
    }

    fn sample_layer(&mut self, id: LayerId, source_rect: Rect, dest_rect: Rect) {
        self.recording.sample_layer(id, source_rect, dest_rect);
    }

    fn viewport_size(&self) -> Size {
        self.recording.viewport_size()
    }

    fn is_3d_context(&self) -> bool {
        self.recording.is_3d_context()
    }

    fn current_opacity(&self) -> f32 {
        self.recording.current_opacity()
    }

    fn current_blend_mode(&self) -> BlendMode {
        self.recording.current_blend_mode()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blinc_core::Color;

    #[test]
    fn test_paint_context_creation() {
        let ctx = PaintContext::new(800.0, 600.0);
        assert_eq!(ctx.viewport_size(), Size::new(800.0, 600.0));
    }

    #[test]
    fn test_fill_rect() {
        let mut ctx = PaintContext::new(800.0, 600.0);
        ctx.fill_rect_xywh(10.0, 20.0, 100.0, 50.0, Color::BLUE);
        assert_eq!(ctx.commands().len(), 1);
    }

    #[test]
    fn test_transform_convenience() {
        let mut ctx = PaintContext::new(800.0, 600.0);
        ctx.translate(10.0, 20.0);
        ctx.fill_rect_xywh(0.0, 0.0, 100.0, 50.0, Color::RED);
        ctx.pop_transform();
        assert_eq!(ctx.commands().len(), 3);
    }

    #[test]
    fn test_implements_draw_context() {
        fn use_draw_context(ctx: &mut dyn DrawContext) {
            ctx.fill_rect(
                Rect::new(0.0, 0.0, 100.0, 50.0),
                CornerRadius::default(),
                Color::GREEN.into(),
            );
        }

        let mut ctx = PaintContext::new(800.0, 600.0);
        use_draw_context(&mut ctx);
        assert_eq!(ctx.commands().len(), 1);
    }
}
