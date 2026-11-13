# Changelog

All notable changes to `blinc_animation` will be documented in this file.

## [Unreleased]

### Added
- `AnimatedValue::drive(value, target)` sets the value with no spring, as something else that plays an animation moves it, and reads as animating until it reaches `target`: a timed animation that is not a spring can drive a motion binding.
- `AnimationScheduler::set_manual_stepping` makes `tick` step nothing, so only `tick_by` moves the animations. A driver that steps a fixed `dt` sets it so a tick made on the way by a frame does not add the time since the last frame.
- `AnimationDescriptor`: an animation as data, `{ start_ms, duration_ms, from, to, curve, initial_velocity }`, with `evaluate`, `velocity`, `is_finished` and `retarget`. Nothing is stepped, so a value and its velocity at any time come straight from the descriptor and a stalled frame skips ahead as a CSS animation does. `descriptor::now_ms` reads the frame clock on the scale descriptors use.
- `Curve`: `Linear`, `CubicBezier`, `Steps` with `StepJump`, `Power` with `PowerMode` (the polynomial easings, exactly) and `Spring`, a damped spring in closed form for under, critical and over damping. A spring's duration is when it has settled, within a thousandth of the distance it moved.
- `Easing::to_curve`, `Spring::to_descriptor`, `AnimatedValue::descriptor` and `SchedulerHandle::get_spring`.
- `descriptor::lerp_color_premultiplied`, the sRGB premultiplied colour mix CSS uses.
- The curve maths is checked against one file of float64 vectors computed outside either implementation (`tests/vectors/curves.txt`, made by `generate.py`): the CPU code and the shader in blinc_gpu both agree with it to better than 1e-6 of the distance moved.

### Fixed
- The README's examples use the API that exists. They called `Spring::animate_to` and `update`, `SpringConfig::bouncy`, `KeyframeAnimation::update`, `Timeline::play` and `AnimationScheduler::global`, none of which are there.

## [0.6.0] - 2026-11-04

### Performance
- **EMA-smoothed tick `dt`** — `AnimationScheduler::tick` now feeds the raw vsync interval through an exponential moving average before stepping springs / keyframes. Single-frame stutters from jittery vsync (Wayland fractional scaling, GPU contention) stop hitting the spring damping equation as a sharp impulse, killing the "shudder" on long-running springs at the cost of a one-frame phase lag — invisible in normal motion, measurable on slow-mo capture.

### Added
- `KeyframeProperties::overlay` and `overlaps`, field-wise over every property.
- `MultiKeyframeAnimation::playback_id()` identifies playback for cache ownership. Construction and explicit starts assign fresh identities; ticks, loops and snapshot clones retain them.
- Caller-supplied `Easing` on the `scale_in` / `scale_out` / `slide_in` / `slide_out` keyframe presets. Lets callers (cn::dialog / sheet / drawer / toast) feed the active theme's semantic easing role (`ease_default`, `ease_in`, `ease_out`, `ease_in_out`) into the preset instead of the previous hard-coded `EaseInOut`.
- `AnimationScheduler::wake()` — hint the bg thread that activity has changed, called from `add_spring` / `add_keyframe` / `add_timeline` / `add_tick_callback` / `set_continuous_redraw(true)` / `request_redraw` / `set_target_fps` and the equivalent `SchedulerHandle` paths
- `target_fps` is read fresh on every iteration so `set_target_fps` takes effect immediately
- `KeyframeProperties::needs_vsync_for_smoothness()` classifies which animated properties demand native-vsync timing to look right (transforms, 3D rotation, layout sizing, font-size, clip-path geometry) versus which tolerate sub-vsync rates (opacity, colors, shadows, filters, light, corner radius/shape, border / outline widths). Backs `WindowConfig::animation_fps_cap`'s automatic per-property override in the windowed app.

### Changed
- Colour properties interpolate with premultiplied alpha, as CSS does, and a colour with no counterpart fades its alpha only.
- The desktop background thread parks on a `Condvar` whenever `has_active` and `wants_continuous` are both false. Idle apps now sit at zero CPU; previously the loop spun unconditionally at 120 fps even on a static UI (issue #28).
- When `wants_continuous` is the only reason to tick (e.g. cursor blink), the bg thread runs at half `target_fps` instead of full rate. Cuts the CPU floor for any focused text input by ~50%.
- `stop_background` wakes the parked thread before joining so it observes the stop flag.

## [0.4.0] - 2026-04-05

### Added
- Animation suspension scopes for router page transitions (`create_scope`, `enter_scope`, `exit_scope`)
- `Spring::pause()` / `resume()` / `is_paused()` for animation lifecycle control
- `AnimatedValue` auto-registers springs in active suspension scope

## [0.1.15] - 2026-03-22

### Fixed

- Smooth corner radius artifact on thin borders in keyframe interpolation

## [0.1.13] - 2026-02-18

### Added

- CSS transitions with automatic detection via `detect_and_start_transitions()` and snapshot/interpolation
- CSS filter animations: blur, drop-shadow, grayscale, sepia, invert, brightness, contrast, saturate, hue-rotate
- Backdrop-filter animation: blur, saturate, brightness
- Text-shadow property animation
- Gradient color stop animation with OBB coordinate fix
- Animated clip-path on hover with keyframe interpolation
- Advanced CSS selectors: `:not()`, `:is()`, `:where()`, structural pseudo-classes
- Outline property animation (width, color, offset)
- Layout animation (width, height, padding, margin, gap) with taffy style updates
- SVG path morphing via `d` attribute in @keyframes
- Corner-shape (superellipse) and overflow-fade animation support

### Fixed

- Easing curve corrections for ease-in, ease-out, ease-in-out
- Transform-origin jitter on hover reverse transitions
- Animation timing for iteration count and fill mode
- Border morph on rounded clip containers
