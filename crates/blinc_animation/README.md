# blinc_animation

> **Part of the [Blinc UI Framework](https://project-blinc.github.io/Blinc)**
>
> This crate is a component of Blinc, a GPU-accelerated UI framework for Rust.
> For full documentation and guides, visit the [Blinc documentation](https://project-blinc.github.io/Blinc).

Animation system for Blinc UI - spring physics, keyframes, and timeline orchestration.

## Overview

`blinc_animation` provides a powerful animation system with spring physics, keyframe animations, and timeline orchestration. All animations are interruptible and preserve velocity when interrupted.

## Features

- **Spring Physics**: RK4-integrated springs with configurable stiffness, damping, and mass
- **Keyframe Animations**: Timed sequences with easing functions
- **Timelines**: Orchestrate multiple animations with offsets
- **Interruptible**: Animations inherit velocity when interrupted
- **Presets**: Common entry/exit animation patterns

## Spring Animations

```rust
use blinc_animation::{Spring, SpringConfig};

// A spring at 0.0, with its own stiffness, damping and mass.
let mut spring = Spring::new(SpringConfig::new(300.0, 20.0, 1.0), 0.0);

// Aim it at a target. It keeps its velocity.
spring.set_target(100.0);

// Step it each frame by the time since the last, in seconds.
spring.step(1.0 / 60.0);
let value = spring.value();
let done = spring.is_settled();
```

### Spring Presets

```rust
SpringConfig::gentle()      // Slow, gentle movement
SpringConfig::wobbly()      // Playful overshoot
SpringConfig::stiff()       // Quick, snappy (the default)
SpringConfig::snappy()      // Quick with little overshoot
SpringConfig::molasses()    // Very slow
```

## Keyframe Animations

```rust
use blinc_animation::{Easing, Keyframe, KeyframeAnimation};

// A duration in milliseconds, and keyframes at 0.0..=1.0 of it. The easing
// of a keyframe applies on the way to it.
let mut animation = KeyframeAnimation::new(
    500,
    vec![
        Keyframe { time: 0.0, value: 0.0, easing: Easing::Linear },
        Keyframe { time: 0.5, value: 100.0, easing: Easing::EaseOut },
        Keyframe { time: 1.0, value: 80.0, easing: Easing::EaseInOut },
    ],
);
animation.start();
let value = animation.value();
```

The scheduler advances animations it holds; see below.

### Easing Functions

```rust
Easing::Linear
Easing::EaseIn
Easing::EaseOut
Easing::EaseInOut
Easing::EaseInQuad      // also Out and InOut, and Cubic and Quart
Easing::CubicBezier(0.4, 0.0, 0.2, 1.0)
```

## Timelines

```rust
use blinc_animation::{Easing, Timeline};

let mut timeline = Timeline::new();

// Offset and duration in milliseconds, then the start and end values.
let opacity = timeline.add(0, 300, 0.0, 1.0);
let scale = timeline.add_with_easing(100, 400, 0.8, 1.0, Easing::EaseOut);

timeline.start();
timeline.tick(16.0); // milliseconds
let opacity_now = timeline.value(opacity); // Option<f32>

timeline.pause();
timeline.resume();
timeline.seek(150.0);
```

## Animation Scheduler

```rust
use blinc_animation::{AnimationScheduler, Spring, SpringConfig};

// The scheduler holds springs, keyframe animations and timelines.
let scheduler = AnimationScheduler::new();

let id = scheduler.add_spring(Spring::new(SpringConfig::snappy(), 0.0));
scheduler.set_spring_target(id, 1.0);

// Advances everything it holds by the time since the last tick.
scheduler.tick();
let value = scheduler.get_spring_value(id);
```

## Presets

```rust
use blinc_animation::AnimationPreset;

// Each takes a duration in milliseconds.
AnimationPreset::fade_in(300)
AnimationPreset::fade_out(300)
AnimationPreset::scale_in(300)
AnimationPreset::scale_out(300)
AnimationPreset::pop_in(300)
AnimationPreset::bounce_in(500)
AnimationPreset::bounce_out(500)
AnimationPreset::shake(400, 8.0)        // and an intensity

// Slides also take a distance.
AnimationPreset::slide_in_left(300, 40.0)   // right, top and bottom too
AnimationPreset::slide_out_left(300, 40.0)
```

## License

MIT OR Apache-2.0
