//! Slider component for range value selection
//!
//! A themed slider/range input with click-to-set and drag-to-adjust.
//! Uses context-driven state for proper persistence across UI rebuilds.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
//!     // Create slider state from context (0.0 to 1.0 by default)
//!     let volume = ctx.use_state_for("volume", 0.5);
//!
//!     cn::slider(ctx, &volume)
//!         .label("Volume")
//!         .on_change(|value| println!("Volume: {}", value))
//! }
//!
//! // Custom range
//! let brightness = ctx.use_state_for("brightness", 50.0);
//! cn::slider(ctx, &brightness)
//!     .min(0.0)
//!     .max(100.0)
//!     .step(1.0)
//!
//! // Different sizes
//! cn::slider(ctx, &value)
//!     .size(SliderSize::Large)
//!
//! // Custom colors
//! cn::slider(ctx, &value)
//!     .track_color(Color::GRAY)
//!     .fill_color(Color::BLUE)
//!     .thumb_color(Color::WHITE)
//!
//! // Disabled state
//! cn::slider(ctx, &value)
//!     .disabled(true)
//! ```

use blinc_animation::{AnimationContext, SpringConfig, get_scheduler};
use blinc_core::events::event_types;
use blinc_core::{BlincContext, BlincContextState, Color, State};
use blinc_layout::div::ElementTypeId;
use blinc_layout::element::{CursorStyle, RenderProps};
use blinc_layout::motion::motion;
use blinc_layout::prelude::*;
use blinc_layout::stateful::{NoState, StateTransitions, stateful_with_key};
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_macros::BlincComponent;
use blinc_theme::{ColorToken, RadiusToken, ThemeState};
use std::sync::{Arc, Mutex};

use super::label::{LabelSize, label};
use blinc_layout::InstanceKey;

/// Halo grow / shrink duration in milliseconds. FPS-independent —
/// the framework's `on_next_animation_frame` provides the wall-clock
/// delta each refresh, so the animation lasts the same wall time at
/// 30 Hz, 60 Hz, or 120 Hz.
const HALO_DURATION_MS: u32 = 220;

/// Slider thumb interaction + halo-animation lifecycle.
///
/// `Idle / Hovered / Pressed / Dragging` are the user-facing
/// interaction phases. `Entering { elapsed_ms }` / `Exiting { elapsed_ms }`
/// are the transient animation phases — `elapsed_ms` accumulates
/// the wall-clock delta each `on_next_animation_frame`, and the
/// FSM transitions out once it reaches `HALO_DURATION_MS`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SliderThumbState {
    #[default]
    Idle,
    /// Halo growing in. `elapsed_ms` advances each frame via
    /// `on_next_animation_frame`; transitions to `Hovered` once
    /// it crosses `HALO_DURATION_MS`.
    Entering {
        elapsed_ms: u32,
    },
    Hovered,
    Pressed,
    Dragging,
    /// Halo shrinking out. Same shape as `Entering`; transitions
    /// to `Idle` once `elapsed_ms` crosses `HALO_DURATION_MS`.
    Exiting {
        elapsed_ms: u32,
    },
}

impl StateTransitions for SliderThumbState {
    fn on_event(&self, event: u32) -> Option<Self> {
        use event_types::*;
        match (self, event) {
            // Enter / re-enter from any non-engaged state.
            (Self::Idle, POINTER_ENTER) => Some(Self::Entering { elapsed_ms: 0 }),
            (Self::Exiting { .. }, POINTER_ENTER) => Some(Self::Entering { elapsed_ms: 0 }),

            // Engage on press from any visible state.
            (Self::Entering { .. }, POINTER_DOWN) => Some(Self::Pressed),
            (Self::Hovered, POINTER_DOWN) => Some(Self::Pressed),

            // Release / drag transitions.
            (Self::Pressed, POINTER_UP) => Some(Self::Hovered),
            (Self::Pressed, DRAG) => Some(Self::Dragging),

            // Exit from any visible state. `Dragging × POINTER_LEAVE`
            // falls through (catch-all `_ => None`) so a drag that
            // wanders off the track stays Dragging — the halo
            // remains lit until DRAG_END.
            (Self::Entering { .. }, POINTER_LEAVE) => Some(Self::Exiting { elapsed_ms: 0 }),
            (Self::Hovered, POINTER_LEAVE) => Some(Self::Exiting { elapsed_ms: 0 }),
            (Self::Pressed, POINTER_LEAVE) => Some(Self::Exiting { elapsed_ms: 0 }),

            // Drag end. POINTER_UP is the fallback when the host
            // event stream doesn't deliver a discrete DRAG_END.
            (Self::Dragging, DRAG_END) => Some(Self::Exiting { elapsed_ms: 0 }),
            (Self::Dragging, POINTER_UP) => Some(Self::Exiting { elapsed_ms: 0 }),

            _ => None,
        }
    }

    /// Time-driven transition. The framework hands us the wall-clock
    /// delta since the previous refresh; accumulate it into the
    /// variant's `elapsed_ms` and pop into the next steady state
    /// once `HALO_DURATION_MS` has elapsed. FPS-invariant by
    /// construction.
    fn on_next_animation_frame(&self, delta_ms: f32) -> Option<Self> {
        let advance = |e: u32| -> u32 { e.saturating_add(delta_ms.max(0.0).round() as u32) };
        match self {
            Self::Entering { elapsed_ms } => {
                let next = advance(*elapsed_ms);
                if next >= HALO_DURATION_MS {
                    Some(Self::Hovered)
                } else {
                    Some(Self::Entering { elapsed_ms: next })
                }
            }
            Self::Exiting { elapsed_ms } => {
                let next = advance(*elapsed_ms);
                if next >= HALO_DURATION_MS {
                    Some(Self::Idle)
                } else {
                    Some(Self::Exiting { elapsed_ms: next })
                }
            }
            _ => None,
        }
    }
}

/// BlincComponent for slider state and animations
/// Generates type-safe hooks that persist across UI rebuilds:
/// - `SliderState::use_thumb_offset(ctx, initial, config) -> SharedAnimatedValue`
/// - `SliderState::use_drag_start_x(ctx, 0.0) -> State<f32>`
#[derive(BlincComponent)]
struct SliderState {
    /// Animated X offset for thumb position
    #[animation]
    thumb_offset: f32,
    /// Mouse X position at drag start (screen coordinates)
    drag_start_x: f32,
    /// Thumb offset at drag start
    drag_start_offset: f32,
    /// Whether a drag is currently in progress (to suppress click-to-jump)
    is_dragging: bool,
}

/// Slider size variants
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SliderSize {
    /// Small slider (track: 4px, thumb: 14px)
    Small,
    /// Medium slider (track: 6px, thumb: 18px)
    #[default]
    Medium,
    /// Large slider (track: 8px, thumb: 22px)
    Large,
}

impl SliderSize {
    /// Get the track height for this size
    fn track_height(&self) -> f32 {
        match self {
            SliderSize::Small => 4.0,
            SliderSize::Medium => 6.0,
            SliderSize::Large => 8.0,
        }
    }

    /// Get the thumb diameter for this size
    fn thumb_size(&self) -> f32 {
        match self {
            SliderSize::Small => 14.0,
            SliderSize::Medium => 18.0,
            SliderSize::Large => 22.0,
        }
    }
}

/// Slider component
///
/// A range slider with click-to-set and drag-to-adjust value.
/// Uses context-driven state for proper persistence across UI rebuilds.
pub struct Slider {
    /// The fully-built inner element (Div containing slider and optional label)
    inner: Div,
}

impl Slider {
    /// Create a new slider with context and state
    ///
    /// # Example
    /// ```ignore
    /// let volume = ctx.use_state_for("volume", 0.5);
    /// cn::slider(&volume)
    /// ```
    #[track_caller]
    pub fn new(value_state: &State<f32>) -> Self {
        Self::with_config(
            InstanceKey::new("slider"),
            SliderConfig::new(value_state.clone()),
        )
    }

    /// Create from a full configuration
    fn with_config(key: InstanceKey, config: SliderConfig) -> Self {
        let theme = ThemeState::get();
        let track_height = config.size.track_height();
        let thumb_size = config.size.thumb_size();
        let radius = theme.radius(RadiusToken::Full);

        // Get colors. Mute track + fill + thumb when disabled so the
        // whole control reads as inert — same approach `cn::switch`
        // takes (track → input-bg-disabled, no opacity dimming on the
        // surfaces). Pre-fix the slider only muted via container
        // `opacity(0.5)`, which dimmed every layer including the
        // thumb's bg and left the knob looking like a translucent ring.
        let track_bg = config.track_color.unwrap_or_else(|| {
            if config.disabled {
                theme.color(ColorToken::InputBgDisabled)
            } else {
                theme.color(ColorToken::Border)
            }
        });
        // Fill color for the filled portion of the track. Killing the
        // primary blue on disabled is what makes the control stop
        // reading as "active"; without it, a blue fill on a gray
        // disabled track still looks interactive.
        let fill_bg = config.fill_color.unwrap_or_else(|| {
            if config.disabled {
                theme.color(ColorToken::BorderSecondary)
            } else {
                theme.color(ColorToken::Primary)
            }
        });

        let disabled = config.disabled;
        let min = config.min;
        let max = config.max;
        let step = config.step;
        let width: Option<f32> = config.width;

        // Track width - use config width or default
        let track_width = config.width.unwrap_or(300.0);

        // Calculate initial thumb offset based on current value
        let initial_value = config.value_state.get();
        let initial_norm = ((initial_value - min) / (max - min)).clamp(0.0, 1.0);
        let initial_offset = initial_norm * (track_width - thumb_size);

        // Get PERSISTED state from context using BlincComponent macro
        // These survive across UI rebuilds!
        // Use the instance_key from InstanceKey so each slider has its own state
        let instance_key = key.get().to_string();

        let ctx = BlincContextState::get();
        let scheduler = get_scheduler();

        let thumb_offset = Arc::new(Mutex::new(AnimatedValue::new(
            scheduler.clone(),
            initial_offset,
            SpringConfig::snappy(),
        )));
        let drag_start_x = ctx.use_state_keyed(&format!("{}_drag_start_x", instance_key), || 0.0);
        let drag_start_offset =
            ctx.use_state_keyed(&format!("{}_drag_start_offset", instance_key), || 0.0);
        // Click-to-jump suppression between DRAG_END and the click
        // that fires right after. (Click events fire even after a
        // drag; without this we'd seek the thumb to the release point.)
        let just_dragged = ctx.use_state_keyed(&format!("{}_just_dragged", instance_key), || false);

        // Clones for closures
        let thumb_offset_for_click = thumb_offset.clone();

        // Round to step helper
        let round_to_step = move |value: f32| -> f32 {
            if let Some(s) = step {
                if s > 0.0 {
                    let steps = ((value - min) / s).round();
                    (min + steps * s).clamp(min, max)
                } else {
                    value.clamp(min, max)
                }
            } else {
                value.clamp(min, max)
            }
        };
        let round_to_step_click = round_to_step;
        let round_to_step_drag = round_to_step;

        // Clones for event handlers
        let value_state_for_click = config.value_state.clone();
        let value_state_for_drag = config.value_state.clone();
        let on_change_for_click = config.on_change.clone();
        let on_change_for_drag = config.on_change.clone();

        // Clone for container drag handling and fill
        let thumb_offset_for_fill = thumb_offset.clone();
        let thumb_offset_for_drag = thumb_offset.clone();
        let thumb_offset_for_down = thumb_offset.clone();
        let drag_start_x_for_down = drag_start_x.clone();
        let drag_start_offset_for_down = drag_start_offset.clone();
        let drag_start_x_for_drag = drag_start_x.clone();
        let drag_start_offset_for_drag = drag_start_offset.clone();
        let just_dragged_for_click = just_dragged.clone();
        let just_dragged_for_drag_end = just_dragged.clone();

        // Get visual feedback colors. The thumb chrome no longer
        // varies between idle / hover / drag — matching the design
        // reference (Material-style slider) the thumb is a 2px primary
        // ring with a Surface fill across all active states, and the
        // halo carries all hover/drag feedback. Pre-fix idle used 1px
        // Border (gray) and hover/drag used 2px Primary (blue), which
        // (a) was inconsistent with the reference and (b) re-introduced
        // AA-fighting between the SDF circle edge and a thicker stroke
        // when the bg was transparent.
        let thumb_border_active = theme.color(ColorToken::Primary);
        let thumb_border_disabled = theme.color(ColorToken::BorderSecondary);

        let halo_size = thumb_size * 2.0;
        // Soft accent-tinted halo. `AccentSubtle` is the shared
        // hover-bg token (combobox / select / etc.) so the colour
        // family is consistent across widgets, but at its native
        // alpha (~14-16 %) the halo reads heavier than the
        // Material-style reference. Drop to ~8 % so the halo
        // registers as a soft glow rather than a solid disc.
        let halo_bg = theme.color(ColorToken::Primary).with_alpha(0.08);
        let halo_offset = (halo_size - thumb_size) / 2.0;

        // Thumb chrome builder. Captures are Copy/'static so the
        // closure is Fn — callable each time the Stateful re-renders.
        let thumb_fill_override = config.thumb_color;
        let make_thumb_div = move || {
            let mut td = div()
                .class("cn-slider-thumb")
                .w(thumb_size)
                .h(thumb_size)
                .rounded(thumb_size / 2.0);
            if disabled {
                td = td
                    .class("cn-slider-thumb--disabled")
                    .bg(theme.color(ColorToken::InputBgDisabled))
                    .border(1.0, thumb_border_disabled);
            } else {
                let fill = thumb_fill_override.unwrap_or_else(|| theme.color(ColorToken::Surface));
                td = td.bg(fill).border(2.0, thumb_border_active);
            }
            td
        };

        // Fill bar geometry (precomputed; used inside on_state).
        let fill_left = thumb_size / 2.0 - track_width;

        // Clones for the stateful's child handlers and on_state body.
        let thumb_offset_for_state = thumb_offset.clone();
        let thumb_offset_for_fill_in = thumb_offset_for_fill.clone();
        let just_dragged_for_click = just_dragged.clone();

        // Outer container is a `Stateful<SliderThumbState>` — the
        // framework auto-dispatches POINTER_*/DRAG/DRAG_END from this
        // host element to the FSM (`SliderThumbState::on_event`), so
        // we don't dispatch manually. The on_state callback re-runs
        // on every transition and reads `sctx.state()` to drive the
        // halo's visibility.
        let slider_state_key = format!("{}_state", instance_key);
        let slider_container = stateful_with_key::<SliderThumbState>(&slider_state_key)
            .initial(SliderThumbState::Idle)
            .on_state(move |sctx| {
                let state = sctx.state();

                // Halo ticker — its duration is intentionally much
                // longer than `HALO_DURATION_MS` so the kf is still
                // playing through worst-case stall scenarios (where
                // `delta_ms` clamping makes `elapsed_ms` lag wall-
                // clock by up to ~3x). Without the buffer the kf
                // could settle before the FSM transitions out,
                // dropping the stateful from the animation refresh
                // registry mid-animation and freezing the halo
                // partway through the ramp.
                //
                // `loop_count(0)` is essential: it pins `iterations`
                // at 0 so `KeyframeTrack::should_continue` returns
                // `false`, which reduces `is_playing()` to just
                // `animation.is_playing()`. With the default
                // `iterations = 1`, a freshly-created (never
                // started) kf reports `is_playing = true` (because
                // `current_iteration(0) < iterations(1)`) and the
                // stateful re-registers for animation refresh every
                // frame from creation onward — pegging idle CPU.
                // With `loop_count(-1)` (loop_infinite) the same
                // bug persists forever even after `.stop()`.
                let kf = sctx.use_keyframes("halo_ticker", |b| {
                    b.at(0, 0.0)
                        .at(HALO_DURATION_MS * 4, 1.0)
                        .ease(Easing::Linear)
                        .loop_count(0)
                });

                let halo_scale = match state {
                    SliderThumbState::Idle => {
                        if kf.is_playing() {
                            kf.stop();
                        }
                        0.0
                    }
                    SliderThumbState::Entering { elapsed_ms } => {
                        if !kf.is_playing() {
                            kf.restart();
                        }
                        (elapsed_ms as f32 / HALO_DURATION_MS as f32).clamp(0.0, 1.0)
                    }
                    SliderThumbState::Hovered
                    | SliderThumbState::Pressed
                    | SliderThumbState::Dragging => {
                        if kf.is_playing() {
                            kf.stop();
                        }
                        1.0
                    }
                    SliderThumbState::Exiting { elapsed_ms } => {
                        if !kf.is_playing() {
                            kf.restart();
                        }
                        1.0 - (elapsed_ms as f32 / HALO_DURATION_MS as f32).clamp(0.0, 1.0)
                    }
                };
                let halo_scale = if disabled { 0.0 } else { halo_scale };

                // Halo — `Transform::scale` is a static CSS-style
                // element transform applied at walker time, so
                // corner_radius is freshly baked each frame.
                let halo = div()
                    .class("cn-slider-halo")
                    .absolute()
                    .top(-halo_offset)
                    .left(-halo_offset)
                    .w(halo_size)
                    .h(halo_size)
                    .rounded(halo_size / 2.0)
                    .bg(halo_bg)
                    .pointer_events_none()
                    .transform(Transform::scale(halo_scale, halo_scale));

                // Thumb assembly — halo + static thumb chrome, wrapped
                // in motion for translate_x binding to thumb_offset.
                let thumb_combo = div()
                    .relative()
                    .w(thumb_size)
                    .h(thumb_size)
                    .child(halo)
                    .child(make_thumb_div());
                let thumb_wrapper = div().absolute().left(0.0).top(0.0).child(
                    motion()
                        .translate_x(thumb_offset_for_state.clone())
                        .child(thumb_combo),
                );

                // Fill bar.
                let fill_bar = div()
                    .class("cn-slider-fill")
                    .w(track_width)
                    .h(track_height)
                    .rounded(radius)
                    .bg(fill_bg);
                let fill_positioned = div().absolute().left(fill_left).top(0.0).child(fill_bar);
                let animated_fill = motion()
                    .translate_x(thumb_offset_for_fill_in.clone())
                    .child(fill_positioned);
                let track_fill = div()
                    .absolute()
                    .left(0.0)
                    .top((thumb_size - track_height) / 2.0)
                    .w(track_width)
                    .h(track_height)
                    .overflow_clip()
                    .rounded(radius)
                    .relative()
                    .child(animated_fill);

                // Track visual — purely cosmetic now. Click-to-seek
                // is on the Stateful host below so it catches clicks
                // anywhere on the slider's bounds (including over
                // the blue fill and the thumb, which stack above
                // this element and would otherwise swallow events).
                let track_visual = div()
                    .class("cn-slider-track")
                    .absolute()
                    .left(0.0)
                    .right(0.0)
                    .top((thumb_size - track_height) / 2.0)
                    .h(track_height)
                    .rounded(radius)
                    .bg(track_bg)
                    .cursor_pointer();

                let mut container = div()
                    .relative()
                    .h(thumb_size)
                    .overflow_visible()
                    .cursor(CursorStyle::Grab)
                    .child(track_visual)
                    .child(track_fill)
                    .child(thumb_wrapper);
                if let Some(w) = width {
                    container = container.w(w);
                } else {
                    container = container.w_full();
                }
                container
            })
            // POINTER_DOWN auto-dispatches Hovered → Pressed. This
            // handler does only the drag-start bookkeeping.
            .on_mouse_down(move |event| {
                if disabled {
                    return;
                }
                drag_start_x_for_down.set(event.mouse_x);
                let current = thumb_offset_for_down.lock().unwrap().get();
                drag_start_offset_for_down.set(current);
            })
            // DRAG auto-dispatches Pressed → Dragging. This handler
            // updates the thumb position + value_state from mouse delta.
            //
            // When `step` is set the thumb snaps to step positions —
            // the visible thumb tracks `value_state` (which is
            // already snapped) instead of the continuous mouse
            // position. Without this the continuous thumb and the
            // snapped value diverge, and the previous baked
            // continuous-position primitives can stay in the cache
            // while the new render reflects the snapped value,
            // producing a "double thumb" visual.
            .on_drag(move |event| {
                if disabled {
                    return;
                }
                let start_x = drag_start_x_for_drag.get();
                let delta_x = event.mouse_x - start_x;
                let start_offset = drag_start_offset_for_drag.get();
                let max_offset = track_width - thumb_size;
                let continuous_offset = (start_offset + delta_x).clamp(0.0, max_offset);
                let norm = if max_offset > 0.0 {
                    continuous_offset / max_offset
                } else {
                    0.0
                };
                let raw = min + norm * (max - min);
                let new_val = round_to_step_drag(raw);
                // Snap the thumb to the value when `step` is set;
                // otherwise track the cursor continuously.
                let target_offset = if step.is_some() {
                    if (max - min).abs() > f32::EPSILON {
                        ((new_val - min) / (max - min)) * max_offset
                    } else {
                        0.0
                    }
                } else {
                    continuous_offset
                };
                thumb_offset_for_drag
                    .lock()
                    .unwrap()
                    .set_immediate(target_offset);
                if (value_state_for_drag.get() - new_val).abs() > f32::EPSILON {
                    value_state_for_drag.set(new_val);
                    if let Some(ref cb) = on_change_for_drag {
                        cb(new_val);
                    }
                }
            })
            // DRAG_END auto-dispatches Dragging → Idle. This handler
            // sets the click-suppression flag for the click that
            // fires immediately after.
            .on_drag_end(move |_event| {
                just_dragged_for_drag_end.set(true);
            })
            // Click-to-seek: a click anywhere on the slider container
            // (track, fill, or thumb) jumps the thumb to the click
            // position. Attached to the Stateful host so it catches
            // clicks regardless of which child element they land on
            // — track_visual, track_fill, and thumb_wrapper all stack
            // above each other and would otherwise swallow events
            // depending on which one happens to be on top at the
            // click point.
            //
            // `just_dragged` is the post-drag suppression flag: a
            // click event also fires after every drag (on POINTER_UP
            // with no movement filter), so without this gate every
            // released drag would seek to the release point.
            .on_click(move |event| {
                if disabled {
                    return;
                }
                if just_dragged_for_click.get() {
                    just_dragged_for_click.set(false);
                    return;
                }
                let container_w = event.bounds_width;
                if container_w <= 0.0 {
                    return;
                }
                // The thumb's centre is what tracks the value: when
                // the user clicks at `x`, drop the centre there.
                // `x_offset` is the thumb's LEFT edge — shift left
                // by `thumb_size/2` and clamp to the travel range.
                let max_offset = container_w - thumb_size;
                let x_offset = (event.local_x - thumb_size / 2.0).clamp(0.0, max_offset);
                let norm = if max_offset > 0.0 {
                    x_offset / max_offset
                } else {
                    0.0
                };
                let raw = min + norm * (max - min);
                let new_val = round_to_step_click(raw);
                // Mirror the drag handler: when `step` is set, snap the
                // thumb to the stepped value rather than to the raw
                // pointer x, so the visible thumb and the value stay
                // aligned. Without this, clicking on a stepped slider
                // springs the thumb to the cursor while value_state
                // snaps to the nearest step — visibly diverging
                // positions and (during the multi-frame spring) the
                // double-thumb ghost the user reported.
                let target_offset = if step.is_some() {
                    if (max - min).abs() > f32::EPSILON {
                        ((new_val - min) / (max - min)) * max_offset
                    } else {
                        0.0
                    }
                } else {
                    x_offset
                };
                value_state_for_click.set(new_val);
                // `set_immediate` (not `set_target`) — drag handler
                // also uses set_immediate and produces no artifact;
                // the spring's multi-frame in-flight state was
                // leaving a ghost thumb chrome at the prior position
                // (matching the screenshot symptom). Click-to-seek
                // now jumps cleanly to target, same as drag.
                thumb_offset_for_click
                    .lock()
                    .unwrap()
                    .set_immediate(target_offset);
                if let Some(ref cb) = on_change_for_click {
                    cb(new_val);
                }
            });

        // Pre-fix the entire container was `.opacity(0.5)` on disabled
        // — that dimmed the thumb's bg along with everything else and
        // left the knob looking like a translucent ring. Matching the
        // switch toggle's pattern: keep all surfaces fully opaque,
        // express disabled via chrome changes (thinner border on thumb,
        // see thumb construction above). Track / fill colour overrides
        // for disabled can go via .track_color() / .fill_color() per
        // builder; the default tracks (--border) already read as inert
        // chrome.

        // If there's a label or show_value, wrap in a container
        let inner = if config.label.is_some() || config.show_value {
            let spacing = theme.spacing_value(blinc_theme::SpacingToken::Space2);
            let mut outer = div().h_fit().flex_col().gap_px(spacing);

            // Apply width to container.
            //
            // The fallback is `track_width`, not `w_full`: the thumb
            // travel and the fill are laid out against `track_width`,
            // while the background track pins `left(0).right(0)` and
            // takes whatever the container gives it. Stretching the
            // container drew a track longer than the range it maps —
            // the thumb hit its maximum well short of the end.
            outer = outer.w(width.unwrap_or(track_width));

            // Header row with label and optional value
            if config.label.is_some() || config.show_value {
                let mut header = div().flex_row().justify_between().items_center();

                if let Some(ref label_text) = config.label {
                    let mut lbl = label(label_text).size(LabelSize::Medium);
                    if disabled {
                        lbl = lbl.disabled(true);
                    }
                    header = header.child(lbl);
                }

                if config.show_value {
                    let value_color = if disabled {
                        theme.color(ColorToken::TextTertiary)
                    } else {
                        theme.color(ColorToken::TextSecondary)
                    };
                    let value_state_for_display = config.value_state.clone();
                    let step_for_display = config.step;

                    // Use Stateful with deps to make value text reactive
                    let value_display_key = format!("{}_value_display", instance_key);
                    let value_display = stateful_with_key::<NoState>(&value_display_key)
                        .deps([config.value_state.signal_id()])
                        .on_state(move |_ctx| {
                            let current_value = value_state_for_display.get();
                            let value_text =
                                if step_for_display.is_some() && step_for_display.unwrap() >= 1.0 {
                                    format!("{:.0}", current_value)
                                } else {
                                    format!("{:.2}", current_value)
                                };
                            div().child(text(&value_text).size(14.0).color(value_color))
                        });
                    header = header.child(value_display);
                }

                outer = outer.child(header);
            }

            outer = outer.child(slider_container);
            outer
        } else {
            // Same sizing rule as the labelled branch: the track is
            // only as wide as the travel the thumb is laid out against.
            div()
                .h_fit()
                .w(width.unwrap_or(track_width))
                .child(slider_container)
        };

        Self {
            inner: div().child(inner),
        }
    }

    /// Add a CSS class for selector matching
    pub fn class(mut self, name: impl AsRef<str>) -> Self {
        self.inner = self.inner.class(name);
        self
    }

    /// Set the element ID for CSS selector matching
    pub fn id(mut self, id: &str) -> Self {
        self.inner = self.inner.id(id);
        self
    }
}

impl ElementBuilder for Slider {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.inner.build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.inner.children_builders()
    }

    fn element_type_id(&self) -> ElementTypeId {
        self.inner.element_type_id()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.inner.element_classes()
    }
}

/// Internal configuration for building a Slider
#[derive(Clone)]
struct SliderConfig {
    value_state: State<f32>,
    min: f32,
    max: f32,
    step: Option<f32>,
    size: SliderSize,
    label: Option<String>,
    show_value: bool,
    disabled: bool,
    width: Option<f32>,
    track_color: Option<Color>,
    fill_color: Option<Color>,
    thumb_color: Option<Color>,
    on_change: Option<Arc<dyn Fn(f32) + Send + Sync>>,
}

impl SliderConfig {
    fn new(value_state: State<f32>) -> Self {
        Self {
            value_state,
            min: 0.0,
            max: 1.0,
            step: None,
            size: SliderSize::default(),
            label: None,
            show_value: false,
            disabled: false,
            width: None,
            track_color: None,
            fill_color: None,
            thumb_color: None,
            on_change: None,
        }
    }
}

/// Builder for creating Slider components with fluent API
///
/// Implements `ElementBuilder` directly via a lazy `OnceCell<Slider>`
/// so the slider can be used inline without calling `build_final()` —
/// the same pattern as `CheckboxBuilder` and `RadioGroupBuilder`.
pub struct SliderBuilder {
    key: InstanceKey,
    config: SliderConfig,
    built: std::cell::OnceCell<Slider>,
}

impl SliderBuilder {
    /// Create a new slider builder with value state
    ///
    /// Uses `#[track_caller]` to generate a unique instance key based on the call site.
    #[track_caller]
    pub fn new(value_state: &State<f32>) -> Self {
        Self {
            key: InstanceKey::new("slider"),
            config: SliderConfig::new(value_state.clone()),
            built: std::cell::OnceCell::new(),
        }
    }

    /// Create a slider builder with an explicit key
    pub fn with_key(key: impl Into<String>, value_state: &State<f32>) -> Self {
        Self {
            key: InstanceKey::explicit(key),
            config: SliderConfig::new(value_state.clone()),
            built: std::cell::OnceCell::new(),
        }
    }

    /// Get or build the inner Slider. Materialized on the first
    /// `ElementBuilder` access; subsequent builder method calls
    /// after this point are no-ops because the inner is cached.
    fn get_or_build(&self) -> &Slider {
        ::blinc_layout::build_once::build_once(&self.built, || {
            Slider::with_config(self.key.clone(), self.config.clone())
        })
    }

    /// Set the minimum value (default: 0.0)
    pub fn min(mut self, min: f32) -> Self {
        self.config.min = min;
        self
    }

    /// Set the maximum value (default: 1.0)
    pub fn max(mut self, max: f32) -> Self {
        self.config.max = max;
        self
    }

    /// Set the step size for discrete values
    pub fn step(mut self, step: f32) -> Self {
        self.config.step = Some(step);
        self
    }

    /// Set the slider size
    pub fn size(mut self, size: SliderSize) -> Self {
        self.config.size = size;
        self
    }

    /// Add a label above the slider
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.config.label = Some(label.into());
        self
    }

    /// Show the current value next to the slider
    pub fn show_value(mut self) -> Self {
        self.config.show_value = true;
        self
    }

    /// Set disabled state
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.config.disabled = disabled;
        self
    }

    /// Set a fixed width for the slider track
    pub fn w(mut self, width: f32) -> Self {
        self.config.width = Some(width);
        self
    }

    /// Set the unfilled track color
    pub fn track_color(mut self, color: impl Into<Color>) -> Self {
        self.config.track_color = Some(color.into());
        self
    }

    /// Set the filled portion color
    pub fn fill_color(mut self, color: impl Into<Color>) -> Self {
        self.config.fill_color = Some(color.into());
        self
    }

    /// Set the thumb color
    pub fn thumb_color(mut self, color: impl Into<Color>) -> Self {
        self.config.thumb_color = Some(color.into());
        self
    }

    /// Set the change callback
    ///
    /// Called when the slider value changes.
    pub fn on_change<F>(mut self, callback: F) -> Self
    where
        F: Fn(f32) + Send + Sync + 'static,
    {
        self.config.on_change = Some(Arc::new(callback));
        self
    }
}

impl ElementBuilder for SliderBuilder {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.get_or_build().build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.get_or_build().render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().children_builders()
    }

    fn element_type_id(&self) -> ElementTypeId {
        self.get_or_build().element_type_id()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.get_or_build().element_classes()
    }
}

/// Create a slider with context and state
///
/// The slider uses context-driven state that persists across UI rebuilds.
/// `SliderBuilder` implements `ElementBuilder`, so the builder can be
/// used inline as a child — no terminal `build_final()` call needed.
///
/// # Example
///
/// ```ignore
/// use blinc_cn::prelude::*;
///
/// fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
///     let volume = ctx.use_state_for("volume", 0.5);
///
///     cn::slider(&volume)
///         .min(0.0)
///         .max(1.0)
///         .label("Volume")
///         .show_value()
///         .on_change(|v| println!("Volume: {}", v))
/// }
/// ```
#[track_caller]
pub fn slider(state: &State<f32>) -> SliderBuilder {
    SliderBuilder::new(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slider_sizes() {
        assert_eq!(SliderSize::Small.track_height(), 4.0);
        assert_eq!(SliderSize::Medium.track_height(), 6.0);
        assert_eq!(SliderSize::Large.track_height(), 8.0);
    }

    #[test]
    fn test_slider_thumb_sizes() {
        assert_eq!(SliderSize::Small.thumb_size(), 14.0);
        assert_eq!(SliderSize::Medium.thumb_size(), 18.0);
        assert_eq!(SliderSize::Large.thumb_size(), 22.0);
    }
}
