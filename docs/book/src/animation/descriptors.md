# Animation Descriptors

An `AnimationDescriptor` is an animation as plain data: where a value starts and ends, when, and by which curve. Nothing is stepped. The value and the velocity at any time are computed from the descriptor, so reading one never depends on a tick having run, and a frame that stalls skips ahead the way a CSS animation does.

```rust
use blinc::animation::{AnimationDescriptor, Easing};
use blinc::animation::descriptor::now_ms;

// Start now, run 300 ms, from 0 to 100, easing out.
let start = now_ms();
let d = AnimationDescriptor::timed(start, 300.0, 0.0, 100.0, Easing::EaseOut.to_curve());

let value = d.evaluate(start + 120.0);
let velocity = d.velocity(start + 120.0); // units per second
let done = d.is_finished(start + 400.0);
```

Times are milliseconds on the frame clock's scale, which `now_ms()` reads. Before it starts a descriptor holds `from`, and once it has ended it holds `to`.

## Curves

| Curve | Meaning |
|---|---|
| `Curve::Linear` | progress as time |
| `Curve::CubicBezier(x1, y1, x2, y2)` | a CSS `cubic-bezier()` |
| `Curve::Steps(n, jump)` | a CSS `steps()`, with `StepJump::{Start, End, None, Both}` |
| `Curve::Power(n, mode)` | `t^n` in, out or in-out: the quad, cubic and quart easings |
| `Curve::Spring { stiffness, damping, mass }` | a damped spring in closed form |

`Easing::to_curve` gives the curve an easing is, exactly.

## Springs

```rust
use blinc::animation::{AnimationDescriptor, SpringConfig};

// From 0 towards 100, leaving at 30 units per second.
let d = AnimationDescriptor::spring(start, 0.0, 100.0, 30.0, SpringConfig::wobbly());
```

A spring's duration is when it has settled: within a thousandth of the distance it moved, so it does not depend on the units the value is in. `Spring::to_descriptor` describes the motion a spring is in, and `AnimatedValue::descriptor` the motion of an animated value, or `None` when it is at rest.

## Changing the target

`retarget(now, new_to)` aims the animation somewhere else from the current value. A spring keeps its velocity. A timed animation starts again from the current value over the full duration with no velocity, as a CSS transition does when its target changes.

```rust
let d = d.retarget(now_ms(), 40.0);
```

The existing `Spring`, `AnimatedValue` and keyframe types work as before.
