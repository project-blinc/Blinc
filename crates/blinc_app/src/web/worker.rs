//! DOM-free entry points for hosts that own the worker and frame clock.

use super::*;

pub(super) enum WebCanvas {
    Html(web_sys::HtmlCanvasElement),
    Offscreen(web_sys::OffscreenCanvas),
}

impl WebCanvas {
    pub(super) fn html(&self) -> Option<&web_sys::HtmlCanvasElement> {
        match self {
            Self::Html(canvas) => Some(canvas),
            Self::Offscreen(_) => None,
        }
    }

    pub(super) fn set_width(&self, width: u32) {
        match self {
            Self::Html(canvas) => canvas.set_width(width),
            Self::Offscreen(canvas) => canvas.set_width(width),
        }
    }

    pub(super) fn set_height(&self, height: u32) {
        match self {
            Self::Html(canvas) => canvas.set_height(height),
            Self::Offscreen(canvas) => canvas.set_height(height),
        }
    }
}

/// Ordered browser input in logical canvas coordinates.
///
/// The host handles DOM focus, IME composition and clipboard permission. Send
/// committed text once, after composition; do not duplicate it from keydown.
#[derive(Debug, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum WebInput {
    PointerMove { x: f32, y: f32 },
    PointerDown { x: f32, y: f32, button: u16 },
    PointerUp { x: f32, y: f32, button: u16 },
    PointerLeave,
    Wheel { x: f32, y: f32 },
    KeyDown { code: u32 },
    KeyUp { code: u32 },
    Text { text: String },
    Focus { focused: bool },
}

impl WebApp {
    /// Create a runner in a dedicated worker using a transferred canvas.
    ///
    /// Dimensions are logical pixels. The host owns resize/DPR notifications,
    /// input forwarding and frame scheduling. Call [`Self::run_one_frame`] to
    /// warm the surface before reporting readiness, and stop scheduling while
    /// hidden. Dropping the runner and terminating its worker releases it.
    pub async fn new_offscreen(
        canvas: web_sys::OffscreenCanvas,
        width: f32,
        height: f32,
        scale: f64,
    ) -> Result<Self> {
        if !width.is_finite()
            || !height.is_finite()
            || !scale.is_finite()
            || width <= 0.0
            || height <= 0.0
            || scale <= 0.0
        {
            return Err(BlincError::Platform("Invalid worker viewport".into()));
        }
        blinc_noto_emoji::register();
        blinc_noto_symbols::register();
        canvas.set_width((width * scale as f32).round().max(1.0) as u32);
        canvas.set_height((height * scale as f32).round().max(1.0) as u32);
        let config = BlincConfig {
            max_primitives: 20_000,
            sample_count: 1,
            ..Default::default()
        };
        let (app, surface) = BlincApp::with_offscreen_canvas(canvas.clone(), Some(config)).await?;
        Self::from_surface(
            WebCanvas::Offscreen(canvas),
            app,
            surface,
            width,
            height,
            scale,
        )
    }

    /// Dispatch one normalized event with the modifiers held at its source.
    /// Returns whether keyboard navigation consumed the event.
    pub fn dispatch_input(
        &mut self,
        input: WebInput,
        modifiers: blinc_platform::Modifiers,
    ) -> bool {
        self.cached_modifiers = modifiers;
        let blinc_platform::Modifiers {
            shift,
            ctrl,
            alt,
            meta,
        } = modifiers;
        let button = |value| match value {
            1 => blinc_platform::MouseButton::Middle,
            2 => blinc_platform::MouseButton::Right,
            _ => blinc_platform::MouseButton::Left,
        };
        match input {
            WebInput::PointerMove { x, y } => Self::dispatch_mouse_move(self, x, y),
            WebInput::PointerDown { x, y, button: b } => {
                Self::dispatch_mouse_down(self, x, y, button(b))
            }
            WebInput::PointerUp { x, y, button: b } => {
                Self::dispatch_mouse_up(self, x, y, button(b))
            }
            WebInput::PointerLeave => {
                if let Some(tree) = self.current_tree.as_ref() {
                    let pending = self.ctx.event_router.on_mouse_leave(tree);
                    Self::dispatch_pending(self, pending);
                }
            }
            WebInput::Wheel { x, y } => {
                self.pending_wheel_delta.0 += x;
                self.pending_wheel_delta.1 += y;
            }
            WebInput::KeyDown { code } => {
                return Self::dispatch_key_down(self, code, shift, ctrl, alt, meta);
            }
            WebInput::KeyUp { code } => Self::dispatch_key_up(self, code, shift, ctrl, alt, meta),
            WebInput::Text { text } => {
                for ch in text.chars() {
                    Self::dispatch_text_input(self, ch, shift, ctrl, alt, meta);
                }
            }
            WebInput::Focus { focused } => {
                self.ctx.focused = focused;
                if !focused {
                    blinc_layout::widgets::blur_all_text_inputs();
                    self.dispatch_input(WebInput::PointerLeave, modifiers);
                }
            }
        }
        false
    }

    /// CSS cursor requested by the hovered Blinc element.
    pub fn cursor(&self) -> &'static str {
        self.last_cursor
    }
}
