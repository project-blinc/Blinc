# Sketches

A sketch is a `struct` that owns its own animation state plus a `draw()` method called every frame. Implement the `Sketch` trait on your struct, then mount it into a `Div` tree with `sketch(key, impl)`.

## The `Sketch` trait

```rust
use blinc_canvas_kit::prelude::*;
use blinc::core::layer::Color;

struct Bouncer {
    x: f32,
    vx: f32,
}

impl Sketch for Bouncer {
    fn draw(&mut self, ctx: &mut SketchContext<'_>, _t: f32, dt: f32) {
        self.x += self.vx * dt;
        if self.x < 0.0 || self.x + 40.0 > ctx.width {
            self.vx = -self.vx;
        }

        let mut p = ctx.painter();
        p.fill(Color::WHITE).no_stroke();
        p.rect(self.x, 100.0, 40.0, 40.0);
    }
}
```

The trait has two methods:

| Method | Called | Purpose |
|--------|--------|---------|
| `setup(&mut self, ctx)` | Once before the first `draw` | Asset preload, GPU upload, one-shot layout. Default: no-op. |
| `draw(&mut self, ctx, t, dt)` | Every frame | Mutate state; emit draw calls. `t` = seconds since the sketch started; `dt` = seconds since the previous frame. |

Sketches must be `Send + 'static` — their state lives behind an `Arc<Mutex<...>>` in Blinc's persistent state bag.

## Mounting: `sketch()`

```rust
fn build_ui() -> impl ElementBuilder {
    div()
        .w(600)
        .h(400)
        .child(sketch("bouncer", Bouncer { x: 0.0, vx: 200.0 }))
}
```

The `key` identifies the sketch for state persistence. Every `sketch("bouncer", ...)` with the same key reuses the same persisted state across rebuilds — hot reload, layout changes, route transitions all preserve counters, particle systems, and elapsed time. Pick unique keys per instance.

Wrap the returned `Div` in a sized container (`.w(...)`, `.h(...)`, `.aspect_ratio(...)`, or a flex parent) to control bounds. The sketch fills its parent.

## `SketchContext`

The per-frame context exposes the canvas size, a frame counter, and three drawing entry points:

```rust
pub struct SketchContext<'a> {
    pub width: f32,        // Canvas width in layout units
    pub height: f32,       // Canvas height in layout units
    pub frame_count: u64,  // Frames drawn since setup()
    // ...
}
```

| Method | Returns | Use for |
|--------|---------|---------|
| `ctx.painter()` | `Painter2D<'_>` | Stateful immediate-mode drawing (Processing-style) |
| `ctx.draw_context()` | `&mut dyn DrawContext` | Full GPU access: gradients, glass, clips, 3D, images, text |
| `ctx.play(&mut player, rect, t)` | `()` | Forward to a [`Player`](./players.md) |

`painter()` and `draw_context()` each mutably borrow the underlying `DrawContext` — drop one before calling the other.

## `Painter2D`

The painter holds a current fill, stroke, and transform stack so you don't repeat those arguments on every primitive call.

### Fill & stroke state

```rust
let mut p = ctx.painter();

p.fill(Color::RED).no_stroke();     // Red fill, no outline
p.rect(10.0, 10.0, 100.0, 50.0);

p.stroke(Color::BLACK, 2.0);         // Add a 2px black stroke
p.circle(200.0, 200.0, 40.0);

p.no_fill().stroke(Color::BLUE, 1.0);
p.line(0.0, 0.0, 300.0, 300.0);
```

### Transform stack

`push()` / `pop()` bracket grouped transforms. A single `pop()` undoes every transform pushed since its matching `push()`:

```rust
p.push();
p.translate(100.0, 100.0);
p.rotate(std::f32::consts::FRAC_PI_4);
p.scale(2.0, 2.0);
p.rect(-10.0, -10.0, 20.0, 20.0);   // All three transforms active
p.pop();                              // All three transforms undone
```

Calling `translate` / `rotate` / `scale` without a surrounding `push()` still pushes onto the underlying stack, but `pop()` can't undo them. Always use the bracketed pattern for scoped transforms.

When `Painter2D`'s operations aren't enough — gradients, glass, clips, 3D, images, text — drop the painter and reach for `ctx.draw_context()` directly. See [Canvas Drawing](../widgets/canvas.md) for the full `DrawContext` surface.
