//! Canvas element for custom GPU drawing
//!
//! The canvas element provides direct access to the GPU rendering context
//! (`GpuPaintContext` via the `DrawContext` trait) within the layout system.
//! Canvas inherits transforms, clipping, and layer settings from parent elements.
//!
//! # GPU Context
//!
//! The callback receives a `GpuPaintContext` (as `&mut dyn DrawContext`) which
//! provides the full GPU-accelerated drawing API:
//! - `fill_rect`, `stroke_rect` - Rectangle primitives
//! - `fill_circle`, `stroke_circle` - Circle primitives
//! - `draw_shadow` - GPU-accelerated shadows
//! - `push_transform`, `pop_transform` - Transform stack
//! - `push_clip`, `pop_clip` - Clipping regions
//! - Glass effects, gradients, and more
//!
//! # Example
//!
//! ```ignore
//! use blinc_layout::prelude::*;
//! use blinc_core::{DrawContext, Brush, Color, Rect};
//!
//! // Draw a custom cursor
//! canvas(|ctx: &mut dyn DrawContext, bounds| {
//!     ctx.fill_rect(
//!         Rect::new(0.0, 0.0, 2.0, bounds.height),
//!         0.0.into(),
//!         Brush::Solid(Color::WHITE),
//!     );
//! })
//! .w(2.0)
//! .h(20.0)
//! ```

use std::rc::Rc;

use blinc_core::DrawContext;
use taffy::prelude::*;

use std::sync::Arc;

use blinc_core::FlowShader;

use crate::binding::{IntoReactive, PendingBinding, Reactive, TypedPendingBinding};
use crate::div::{ElementBuilder, ElementTypeId};
use crate::element::{RenderLayer, RenderProps};
use crate::property::PropertyId;
use crate::tree::{LayoutNodeId, LayoutTree};

/// Bounds passed to canvas render callback.
///
/// `x` / `y` are the canvas's offset inside the `DrawContext`'s current
/// transform space — normally `0.0` because the render pipeline pushes a
/// translate(bounds.x, bounds.y) before invoking the callback, so drawing
/// at (0, 0) already lands at the canvas origin. They're surfaced
/// explicitly so callbacks that need an absolute rect (media players,
/// sketches, any code that forwards a `Rect` through to helpers not
/// written against the canvas convention) can construct one without
/// having to inspect the transform stack.
#[derive(Clone, Copy, Debug)]
pub struct CanvasBounds {
    /// Offset of the canvas element in the current `DrawContext` frame.
    /// Normally `0.0` after the pipeline's own `translate(bounds.x,
    /// bounds.y)` is applied, but propagated explicitly so the callback
    /// never has to guess.
    pub x: f32,
    /// Offset of the canvas element in the current `DrawContext` frame.
    /// See [`Self::x`].
    pub y: f32,
    /// Width of the canvas element.
    pub width: f32,
    /// Height of the canvas element.
    pub height: f32,
}

/// What drives a canvas's flow shader from frame to frame.
///
/// A flow reads the app clock, so one that reads `time` changes every frame and needs a frame
/// each vsync to animate; one that does not changes only when its inputs do.
#[derive(Clone, Default)]
pub enum FlowLoop {
    /// A frame every vsync while the canvas is painted, if the shader reads `time` or
    /// `frame_index`; otherwise none.
    #[default]
    Auto,
    /// A frame every vsync while the canvas is painted.
    Always,
    /// No frames of its own: the flow draws when something else repaints the canvas.
    Never,
    /// A frame every vsync while the closure returns true.
    While(Rc<dyn Fn() -> bool>),
    /// The shader's `time` is the clock's, and the canvas runs every frame only while the clock
    /// plays. Paused, it draws again when the clock is stepped or sought.
    Clock(crate::flow_clock::FlowClock),
}

impl FlowLoop {
    /// A loop that runs while `wants` returns true.
    pub fn while_<F: Fn() -> bool + 'static>(wants: F) -> Self {
        Self::While(Rc::new(wants))
    }

    fn wants_frames(&self, shader: &blinc_core::FlowShader) -> bool {
        match self {
            Self::Auto => shader.animated,
            Self::Always => true,
            Self::Never => false,
            Self::While(wants) => wants(),
            Self::Clock(clock) => clock.is_playing(),
        }
    }
}

/// Canvas render callback type
///
/// The callback receives:
/// - `ctx`: The GPU paint context (`GpuPaintContext` as `&mut dyn DrawContext`)
/// - `bounds`: The computed bounds of the canvas element
///
/// The context already has the correct transform applied (canvas position),
/// so drawing at (0, 0) draws at the canvas origin. The context also inherits
/// clip regions and opacity from parent elements.
/// Canvas render function type - uses Rc for single-threaded UI
pub type CanvasRenderFn = Rc<dyn Fn(&mut dyn DrawContext, CanvasBounds)>;

/// Whether a canvas wants another frame. See [`Canvas::redraw_while`].
pub type CanvasRedrawFn = Rc<dyn Fn() -> bool>;

/// Canvas element for custom GPU drawing
///
/// The canvas element reserves space in the layout and provides direct access
/// to the GPU paint context during rendering. This enables:
/// - Custom cursor rendering
/// - Procedural graphics
/// - Charts and visualizations
/// - Animated elements
/// - 3D viewports (via DrawContext's 3D capabilities)
///
/// Canvas respects the layout system:
/// - Transforms are inherited from parent elements
/// - Clipping from scroll containers is applied
/// - Layer ordering (background/foreground/glass) is respected
pub struct Canvas {
    /// Taffy layout style
    style: Style,
    /// The render callback
    render_fn: Option<CanvasRenderFn>,
    /// Opacity
    opacity: f32,
    /// Render layer (background, foreground, glass)
    layer: RenderLayer,
    /// When frames are wanted; `None` wants one every frame it is visible.
    redraw_while: Option<CanvasRedrawFn>,
    /// A flow shader drawn over the canvas's bounds, read when the canvas paints.
    flow: Option<Reactive<Arc<FlowShader>>>,
    /// When the flow asks for frames.
    flow_loop: FlowLoop,
    /// Corner radius of the flow shader's rounded rectangle.
    flow_radius: f32,
}

impl Canvas {
    /// Create a new canvas element
    pub fn new() -> Self {
        Self {
            style: Style::default(),
            render_fn: None,
            opacity: 1.0,
            layer: RenderLayer::default(),
            redraw_while: None,
            flow: None,
            flow_loop: FlowLoop::default(),
            flow_radius: 0.0,
        }
    }

    /// Create a canvas with a render callback
    pub fn with_render<F>(render_fn: F) -> Self
    where
        F: Fn(&mut dyn DrawContext, CanvasBounds) + 'static,
    {
        Self {
            style: Style::default(),
            render_fn: Some(Rc::new(render_fn)),
            opacity: 1.0,
            layer: RenderLayer::default(),
            redraw_while: None,
            flow: None,
            flow_loop: FlowLoop::default(),
            flow_radius: 0.0,
        }
    }

    /// Set the render callback
    pub fn render<F>(mut self, render_fn: F) -> Self
    where
        F: Fn(&mut dyn DrawContext, CanvasBounds) + 'static,
    {
        self.render_fn = Some(Rc::new(render_fn));
        self
    }

    /// Asks for frames only while `wants` returns true.
    ///
    /// A visible canvas otherwise asks for a frame every vsync, since its
    /// callback may draw something different each time. A canvas that
    /// animates only in some state, such as a caret while its field is
    /// focused, names that state here so the window can idle outside it.
    /// It still repaints on frames something else asks for.
    pub fn redraw_while<F>(mut self, wants: F) -> Self
    where
        F: Fn() -> bool + 'static,
    {
        self.redraw_while = Some(Rc::new(wants));
        self
    }

    /// Draws a flow shader over the canvas's bounds, after its render callback.
    ///
    /// The shader is what `flow!` builds at compile time. Its `uv` covers the canvas, `time` is
    /// the app clock, and `pointer` follows the cursor, 0 to 1 across the canvas. It is clipped
    /// by the canvas's ancestors. A shader that reads `time` asks for a frame every vsync while
    /// the canvas is painted; [`Canvas::flow_loop`] changes that.
    ///
    /// ```rust,ignore
    /// canvas(|_, _| {}).w(200.0).h(200.0).flow(flow!(ripple, fragment, {
    ///     input uv: builtin(uv);
    ///     input time: builtin(time);
    ///     node d = distance(uv, vec2(0.5, 0.5));
    ///     node wave = sin(d * 20.0 - time * 4.0);
    ///     output color = vec4(wave, wave, wave, 1.0);
    /// }))
    /// ```
    ///
    /// A signal or computed of a shader is followed in place: the canvas draws the shader it
    /// holds when it paints, and repaints when it changes, with no rebuild.
    pub fn flow(mut self, shader: impl FlowSource) -> Self {
        self.flow = Some(shader.into_flow());
        self
    }

    /// Sets what drives the flow from frame to frame. The default, [`FlowLoop::Auto`], runs the
    /// loop while the shader reads `time`.
    pub fn flow_loop(mut self, flow_loop: FlowLoop) -> Self {
        self.flow_loop = flow_loop;
        self
    }

    /// Rounds the corners of the flow shader's rectangle.
    pub fn rounded(mut self, radius: f32) -> Self {
        self.flow_radius = radius;
        self
    }

    /// Set fixed width
    pub fn w(mut self, width: f32) -> Self {
        self.style.size.width = Dimension::length(width);
        self
    }

    /// Set fixed height
    pub fn h(mut self, height: f32) -> Self {
        self.style.size.height = Dimension::length(height);
        self
    }

    /// Set both width and height
    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.style.size.width = Dimension::length(width);
        self.style.size.height = Dimension::length(height);
        self
    }

    /// Set width to 100% of parent
    pub fn w_full(mut self) -> Self {
        self.style.size.width = Dimension::percent(1.0);
        self
    }

    /// Set height to 100% of parent
    pub fn h_full(mut self) -> Self {
        self.style.size.height = Dimension::percent(1.0);
        self
    }

    /// Set opacity (0.0 = transparent, 1.0 = opaque)
    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity.clamp(0.0, 1.0);
        self
    }

    /// Set flex grow factor
    pub fn flex_grow(mut self) -> Self {
        self.style.flex_grow = 1.0;
        self
    }

    /// Set absolute positioning
    pub fn absolute(mut self) -> Self {
        self.style.position = Position::Absolute;
        self
    }

    /// Set left position (for absolute positioning)
    pub fn left(mut self, value: f32) -> Self {
        self.style.inset.left = LengthPercentageAuto::length(value);
        self
    }

    /// Set top position (for absolute positioning)
    pub fn top(mut self, value: f32) -> Self {
        self.style.inset.top = LengthPercentageAuto::length(value);
        self
    }

    /// Set render layer (background, foreground, glass)
    pub fn layer(mut self, layer: RenderLayer) -> Self {
        self.layer = layer;
        self
    }

    /// Render in foreground layer (on top of glass effects)
    pub fn foreground(mut self) -> Self {
        self.layer = RenderLayer::Foreground;
        self
    }

    /// Render in background layer (behind glass effects)
    pub fn background(mut self) -> Self {
        self.layer = RenderLayer::Background;
        self
    }

    /// Get the render function
    pub fn render_fn(&self) -> Option<&CanvasRenderFn> {
        self.render_fn.as_ref()
    }
}

impl Default for Canvas {
    fn default() -> Self {
        Self::new()
    }
}

impl ElementBuilder for Canvas {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        let node = tree.create_node(self.style.clone());
        // A bound shader is read at paint time; its binding only asks for the repaint.
        let repaint = |_: &mut RenderProps, _: Arc<FlowShader>| {};
        match &self.flow {
            Some(Reactive::Bound(state)) => {
                TypedPendingBinding::new(state.clone(), PropertyId::Paint, repaint).register(node)
            }
            Some(Reactive::Computed(computed)) => {
                TypedPendingBinding::from_computed(computed.clone(), PropertyId::Paint, repaint)
                    .register(node)
            }
            _ => {}
        }
        node
    }

    fn render_props(&self) -> RenderProps {
        RenderProps {
            opacity: self.opacity,
            layer: self.layer,
            ..Default::default()
        }
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        // Canvas has no children in the layout tree
        &[]
    }

    fn element_type_id(&self) -> ElementTypeId {
        ElementTypeId::Canvas
    }

    fn canvas_render_info(&self) -> Option<CanvasRenderFn> {
        let Some(source) = self.flow.clone() else {
            return self.render_fn.clone();
        };
        // The shader draws over the canvas's bounds after the callback, through the same
        // context, so the canvas's transform, clips and opacity apply to it.
        let callback = self.render_fn.clone();
        let corner_radius = self.flow_radius;
        let flow_loop = self.flow_loop.clone();
        Some(Rc::new(move |ctx, bounds| {
            let Some(shader) = source.now() else {
                if let Some(callback) = &callback {
                    callback(ctx, bounds);
                }
                return;
            };
            let inputs = blinc_core::ShaderInputs {
                corner_radius,
                time: match &flow_loop {
                    FlowLoop::Clock(clock) => Some(clock.time()),
                    _ => None,
                },
                ..Default::default()
            };
            if let Some(callback) = &callback {
                callback(ctx, bounds);
            }
            ctx.draw_shader(
                &shader,
                blinc_core::Rect::new(bounds.x, bounds.y, bounds.width, bounds.height),
                &inputs,
            );
            // The window asks for the next frame only while a driver re-asks on each paint.
            if flow_loop.wants_frames(&shader) {
                crate::request_animation_tick();
            }
        }))
    }

    fn canvas_redraw_while(&self) -> Option<CanvasRedrawFn> {
        self.redraw_while.clone()
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        Some(&self.style)
    }
}

/// Data stored in the render tree for canvas elements
#[derive(Clone)]
pub struct CanvasData {
    /// The render callback
    pub render_fn: Option<CanvasRenderFn>,
    /// See [`ElementBuilder::is_static_canvas`]. When `true`, the
    /// walker emits the canvas's primitives to the bg batch but
    /// skips the `CanvasPaintRecord` insert so the compositor's
    /// per-frame canvas-overlay pass won't re-draw the canvas on
    /// top of any children. Also short-circuits the
    /// `had_canvas_painted` flag so the fast path can engage.
    pub is_static: bool,
    /// See [`Canvas::redraw_while`].
    pub redraw_while: Option<CanvasRedrawFn>,
}

impl CanvasData {
    /// Whether this canvas asks for another frame while it is visible.
    pub fn wants_frames(&self) -> bool {
        self.redraw_while.as_ref().is_none_or(|wants| wants())
    }
}

impl std::fmt::Debug for CanvasData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CanvasData")
            .field("has_render_fn", &self.render_fn.is_some())
            .field("is_static", &self.is_static)
            .field("has_redraw_while", &self.redraw_while.is_some())
            .finish()
    }
}

/// Create a canvas element with a render callback
///
/// The callback receives a `DrawContext` which provides the unified drawing API
/// for 2D/3D rendering (fill_rect, stroke_circle, paths, gradients, 3D meshes, etc.)
///
/// The DrawContext already has transforms applied for the canvas position,
/// so drawing at (0, 0) draws at the canvas origin. Clipping and opacity from
/// parent elements are also inherited.
///
/// # Example
///
/// ```ignore
/// use blinc_layout::prelude::*;
/// use blinc_core::{Brush, Color, Rect};
///
/// // Simple colored rectangle
/// canvas(|ctx, bounds| {
///     ctx.fill_rect(
///         Rect::new(0.0, 0.0, bounds.width, bounds.height),
///         0.0.into(),
///         Brush::Solid(Color::RED),
///     );
/// })
/// .w(100.0)
/// .h(50.0)
///
/// // Cursor bar (inherits clip from scroll parent)
/// canvas(|ctx, bounds| {
///     ctx.fill_rect(
///         Rect::new(0.0, 0.0, 2.0, bounds.height),
///         0.0.into(),
///         Brush::Solid(Color::WHITE),
///     );
/// })
/// .w(2.0)
/// .h(16.0)
/// ```
pub fn canvas<F>(render_fn: F) -> Canvas
where
    F: Fn(&mut dyn DrawContext, CanvasBounds) + 'static,
{
    Canvas::with_render(render_fn)
}

/// What [`Canvas::flow`] takes: a shader, or a signal or computed of one, which the canvas
/// follows in place.
pub trait FlowSource {
    /// The shader as the canvas holds it.
    fn into_flow(self) -> Reactive<Arc<FlowShader>>;
}

impl FlowSource for FlowShader {
    fn into_flow(self) -> Reactive<Arc<FlowShader>> {
        Reactive::Const(Arc::new(self))
    }
}

impl FlowSource for Arc<FlowShader> {
    fn into_flow(self) -> Reactive<Arc<FlowShader>> {
        Reactive::Const(self)
    }
}

impl FlowSource for blinc_core::Signal<Arc<FlowShader>> {
    fn into_flow(self) -> Reactive<Arc<FlowShader>> {
        self.into_reactive()
    }
}

impl FlowSource for blinc_core::reactive::Computed<Arc<FlowShader>> {
    fn into_flow(self) -> Reactive<Arc<FlowShader>> {
        self.into_reactive()
    }
}

impl FlowSource for Reactive<Arc<FlowShader>> {
    fn into_flow(self) -> Reactive<Arc<FlowShader>> {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canvas_creation() {
        let c = canvas(|_ctx, _bounds| {
            // empty render
        })
        .w(100.0)
        .h(50.0);

        assert!(
            matches!(c.style.size.width.expand(), taffy::style::ExpandedDimension::Length(w) if (w - 100.0).abs() < 0.001)
        );
        assert!(
            matches!(c.style.size.height.expand(), taffy::style::ExpandedDimension::Length(h) if (h - 50.0).abs() < 0.001)
        );
        assert!(c.render_fn.is_some());
    }

    #[test]
    fn test_canvas_opacity() {
        let c = Canvas::new().opacity(0.5);
        assert_eq!(c.opacity, 0.5);
    }

    #[test]
    fn test_canvas_absolute_positioning() {
        let c = canvas(|_ctx, _bounds| {}).absolute().left(10.0).top(20.0);

        assert!(matches!(c.style.position, Position::Absolute));
    }
}
