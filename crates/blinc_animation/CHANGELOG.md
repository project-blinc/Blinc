# Changelog

All notable changes to `blinc_animation` will be documented in this file.

## [Unreleased]

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
