# Animation System

Blinc provides a multi-layered animation system with physics-based springs and timed keyframe animations.

## Spring Physics

Springs are the foundation of Blinc's animation system, providing natural, interruptible motion.

### Spring Model

A spring follows Hooke's law with damping:

```
Force = -k * (position - target) - d * velocity

where:
  k = stiffness (spring tightness)
  d = damping (friction)
```

### Spring Structure

```rust
struct Spring {
    value: f32,      // Current position
    velocity: f32,   // Current velocity
    target: f32,     // Destination
    config: SpringConfig,
}

struct SpringConfig {
    stiffness: f32,  // Spring constant (k)
    damping: f32,    // Damping coefficient (d)
    mass: f32,       // Virtual mass
}
```

### RK4 Integration

Blinc uses **4th-order Runge-Kutta (RK4)** integration for stability:

```rust
fn step(&mut self, dt: f32) {
    // RK4 provides stable integration even with large timesteps
    let k1 = self.acceleration(self.value, self.velocity);
    let k2 = self.acceleration(
        self.value + self.velocity * dt * 0.5,
        self.velocity + k1 * dt * 0.5
    );
    let k3 = self.acceleration(
        self.value + (self.velocity + k2 * dt * 0.5) * dt * 0.5,
        self.velocity + k2 * dt * 0.5
    );
    let k4 = self.acceleration(
        self.value + (self.velocity + k3 * dt) * dt,
        self.velocity + k3 * dt
    );

    self.velocity += (k1 + 2.0 * k2 + 2.0 * k3 + k4) * dt / 6.0;
    self.value += self.velocity * dt;
}
```

### Spring Presets

| Preset | Stiffness | Damping | Character |
|--------|-----------|---------|-----------|
| `stiff()` | 400 | 30 | Fast, minimal overshoot |
| `snappy()` | 300 | 20 | Quick with slight bounce |
| `gentle()` | 120 | 14 | Soft, slower motion |
| `wobbly()` | 180 | 12 | Bouncy, playful |
| `molasses()` | 50 | 20 | Very slow, heavy |

### Settling Detection

A spring is considered settled when:

```rust
fn is_settled(&self) -> bool {
    let position_settled = (self.value - self.target).abs() < EPSILON;
    let velocity_settled = self.velocity.abs() < VELOCITY_EPSILON;
    position_settled && velocity_settled
}
```

---

## AnimatedValue

`AnimatedValue` wraps a spring for easy use in components:

```rust
// Create an animated value
let scale = ctx.use_animated_value(1.0, SpringConfig::snappy());

// Read current value
let current = scale.lock().unwrap().get();

// Set new target (animates to it)
scale.lock().unwrap().set_target(1.2);

// Set immediately (no animation)
scale.lock().unwrap().set(1.0);
```

### SharedAnimatedValue

For use across closures, values are wrapped in `Arc<Mutex<_>>`:

```rust
let scale = ctx.use_animated_value(1.0, SpringConfig::snappy());

// Clone Arc for closure
let hover_scale = Arc::clone(&scale);

motion()
    .scale(scale.lock().unwrap().get())
    .on_hover_enter(move |_| {
        hover_scale.lock().unwrap().set_target(1.1);
    })
```

---

## Keyframe Animations

For time-based animations with specific durations:

### Keyframe Structure

```rust
struct Keyframe {
    time: f32,           // Time in animation (0.0 - 1.0)
    value: f32,          // Value at this keyframe
    easing: EasingFn,    // Interpolation to next keyframe
}
```

### Easing Functions

| Easing | Description |
|--------|-------------|
| `linear` | Constant speed |
| `ease_in` | Start slow, end fast |
| `ease_out` | Start fast, end slow |
| `ease_in_out` | Slow at both ends |
| `ease_in_quad` | Quadratic ease in |
| `ease_out_cubic` | Cubic ease out |
| `ease_in_out_elastic` | Elastic bounce |

### Animation Fill Modes

| Mode | Description |
|------|-------------|
| `None` | Revert after animation |
| `Forwards` | Keep final value |
| `Backwards` | Apply initial before start |
| `Both` | Forwards + Backwards |

---

## Timelines

Timelines coordinate multiple animations:

```rust
let timeline = ctx.use_animated_timeline();

let entry_id = timeline.lock().unwrap().configure(|t| {
    // Add animation entries
    let rotation_id = t.add(
        0,      // start_ms
        1000,   // duration_ms
        0.0,    // from
        360.0   // to
    );

    // Configure looping
    t.set_loop(-1);  // -1 = infinite loop

    // Start the timeline
    t.start();

    rotation_id
});
```

### Timeline Features

- **Stagger** - Delay between child animations
- **Loop** - Repeat animations
- **Reverse** - Play backwards
- **Alternate** - Ping-pong direction

---

## Animation Scheduler

A background thread ticks all animations at 120fps:

```rust
struct AnimationScheduler {
    springs: Vec<SharedAnimatedValue>,
    timelines: Vec<SharedAnimatedTimeline>,
    running: AtomicBool,
    needs_redraw: Arc<AtomicBool>,
    wake_callback: Box<dyn Fn() + Send>,
}
```

### Scheduler Loop

```rust
fn run(&self) {
    let frame_duration = Duration::from_secs_f64(1.0 / 120.0);

    while self.running.load(Ordering::SeqCst) {
        let start = Instant::now();

        // Tick all springs
        for spring in &self.springs {
            spring.lock().unwrap().step(frame_duration.as_secs_f32());
        }

        // Tick all timelines
        for timeline in &self.timelines {
            timeline.lock().unwrap().tick(frame_duration);
        }

        // If any animation is active, request redraw
        if self.has_active_animations() {
            self.needs_redraw.store(true, Ordering::SeqCst);
            (self.wake_callback)();  // Wake the main thread
        }

        // Sleep for remaining frame time
        let elapsed = start.elapsed();
        if elapsed < frame_duration {
            thread::sleep(frame_duration - elapsed);
        }
    }
}
```

### Benefits of Background Thread

1. **Consistent timing** - Animations run at 120fps regardless of main thread
2. **Survives focus loss** - Continues when window loses focus
3. **Non-blocking** - Doesn't block UI event processing
4. **Battery efficient** - Only runs when animations are active

---

## Motion Container

`motion()` binds animations to elements:

```rust
motion()
    .scale(scale.lock().unwrap().get())      // Read current value
    .opacity(opacity.lock().unwrap().get())
    .translate_y(y.lock().unwrap().get())
    .child(content)
```

### How Motion Works

1. **At build time**: Reads current animation values
2. **Stores binding**: Remembers which animated values to sample
3. **At render time**: Samples current values from scheduler
4. **No rebuild needed**: Animation updates don't trigger tree rebuilds

### Enter/Exit Animations

Motion also provides declarative enter/exit:

```rust
motion()
    .fade_in(300)                           // Fade in over 300ms
    .scale_in(300)                          // Scale from 0 to 1
    .slide_in(SlideDirection::Right, 200)   // Slide from right
    .child(content)
```

---

## Integration Points

### With Stateful Elements

```rust
fn animated_button(ctx: &WindowedContext) -> impl ElementBuilder {
    let scale = ctx.use_animated_value(1.0, SpringConfig::snappy());
    let hover = Arc::clone(&scale);
    let leave = Arc::clone(&scale);

    motion()
        .scale(scale.lock().unwrap().get())
        .on_hover_enter(move |_| {
            hover.lock().unwrap().set_target(1.05);
        })
        .on_hover_leave(move |_| {
            leave.lock().unwrap().set_target(1.0);
        })
        .child(button_content())
}
```

### With BlincComponent

```rust
#[derive(BlincComponent)]
struct ExpandableCard {
    #[animation]
    height: f32,
    #[animation]
    arrow_rotation: f32,
}

fn card(ctx: &WindowedContext) -> impl ElementBuilder {
    let height = ExpandableCard::use_height(ctx, 60.0, SpringConfig::snappy());
    let rotation = ExpandableCard::use_arrow_rotation(ctx, 0.0, SpringConfig::snappy());

    motion()
        .h(height.lock().unwrap().get())
        .on_click(move |_| {
            height.lock().unwrap().set_target(200.0);
            rotation.lock().unwrap().set_target(180.0);
        })
        .child(card_content())
}
```

---

## Performance Considerations

1. **Spring settling** - Stopped springs don't consume CPU
2. **Batched ticks** - All animations tick together
3. **No allocations** - Animation values are pre-allocated
4. **GPU transforms** - Motion transforms are GPU-accelerated
5. **Minimal redraws** - Only redraw when animations are active
