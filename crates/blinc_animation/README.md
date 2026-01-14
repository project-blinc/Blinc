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

// Create a spring
let spring = Spring::new(SpringConfig {
    stiffness: 300.0,
    damping: 20.0,
    mass: 1.0,
});

// Animate to target
spring.animate_to(100.0);

// Update each frame
let value = spring.update(delta_time);
```

### Spring Presets

```rust
SpringConfig::gentle()      // Slow, gentle movement
SpringConfig::default()     // Balanced default
SpringConfig::bouncy()      // Playful bounce
SpringConfig::stiff()       // Quick, snappy
```

## Keyframe Animations

```rust
use blinc_animation::{KeyframeAnimation, Keyframe, Easing};

let animation = KeyframeAnimation::new()
    .keyframe(Keyframe::new(0.0, 0.0))
    .keyframe(Keyframe::new(0.5, 100.0).easing(Easing::EaseOut))
    .keyframe(Keyframe::new(1.0, 80.0).easing(Easing::EaseInOut))
    .duration(Duration::from_millis(500));

// Update each frame
let value = animation.update(delta_time);
```

### Easing Functions

```rust
Easing::Linear
Easing::EaseIn
Easing::EaseOut
Easing::EaseInOut
Easing::CubicBezier(0.4, 0.0, 0.2, 1.0)
```

## Timelines

```rust
use blinc_animation::{Timeline, TimelineEntry};

let timeline = Timeline::new()
    .add(opacity_animation, Duration::ZERO)
    .add(scale_animation, Duration::from_millis(100))
    .add(position_animation, Duration::from_millis(200));

// Play/pause/seek
timeline.play();
timeline.pause();
timeline.seek(Duration::from_millis(150));
```

## Animation Scheduler

```rust
use blinc_animation::AnimationScheduler;

// Global scheduler manages all active animations
let scheduler = AnimationScheduler::global();

// Schedule an animation
scheduler.schedule(animation);

// Update all animations
scheduler.update(delta_time);
```

## Multi-Property Animation

```rust
use blinc_animation::AnimatedValue;

struct AnimatedElement {
    x: AnimatedValue<f32>,
    y: AnimatedValue<f32>,
    opacity: AnimatedValue<f32>,
    scale: AnimatedValue<f32>,
}

impl AnimatedElement {
    fn animate_to(&mut self, x: f32, y: f32) {
        self.x.animate_to(x);
        self.y.animate_to(y);
    }

    fn update(&mut self, dt: f32) {
        self.x.update(dt);
        self.y.update(dt);
        self.opacity.update(dt);
        self.scale.update(dt);
    }
}
```

## Presets

```rust
use blinc_animation::presets;

// Entry animations
presets::fade_in()
presets::slide_in_left()
presets::slide_in_right()
presets::scale_in()
presets::bounce_in()

// Exit animations
presets::fade_out()
presets::slide_out_left()
presets::slide_out_right()
presets::scale_out()
```

## License

MIT OR Apache-2.0
