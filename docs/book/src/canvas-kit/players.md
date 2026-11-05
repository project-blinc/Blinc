# Players

The `Player` trait is the contract for time-based animation sources (Lottie, Rive, custom scene files). Implement it once; the same sketch can then drive any player without knowing the format.

## The trait

```rust
pub trait Player: Send + 'static {
    fn duration(&self) -> Option<f32>;
    fn draw_at(&mut self, ctx: &mut SketchContext<'_>, rect: Rect, t: f32);
    fn seek(&mut self, _t: f32) {}
    fn set_playing(&mut self, _playing: bool) {}
}
```

| Method | Default | Purpose |
|--------|---------|---------|
| `duration()` | required | Total playback duration in seconds. `None` signals content that plays indefinitely (procedural, live, user-controlled). |
| `draw_at(ctx, rect, t)` | required | Render one frame at time `t` into `rect`. Interpolate the scene at `t`, dispatch draw calls into `ctx`. |
| `seek(t)` | no-op | Seek internal playback to `t`. Players that derive every frame from the incoming `t` don't need to override. |
| `set_playing(playing)` | no-op | Pause / resume. Paused players should render their frozen pose and ignore `t` in `draw_at`. |

## Playing a Lottie scene

`blinc_lottie::LottiePlayer` implements `Player`. Wrap it in a sketch to run at any size:

```rust
use blinc::app::prelude::*;
use blinc_canvas_kit::prelude::*;
use blinc::core::{Color, Rect};
use blinc_lottie::LottiePlayer;

const LOTTIE_JSON: &str = include_str!("assets/my_animation.json");

struct Loader {
    player: LottiePlayer,
}

impl Sketch for Loader {
    fn draw(&mut self, ctx: &mut SketchContext<'_>, t: f32, _dt: f32) {
        let size = ctx.width.min(ctx.height);
        let x = (ctx.width - size) * 0.5;
        let y = (ctx.height - size) * 0.5;
        ctx.play(&mut self.player, Rect::new(x, y, size, size), t);
    }
}

fn build_ui() -> impl ElementBuilder {
    let player = LottiePlayer::from_json(LOTTIE_JSON).expect("parse Lottie");
    div()
        .w_full()
        .h_full()
        .bg(Color::WHITE)
        .child(sketch("lottie", Loader { player }))
}
```

`ctx.play(&mut player, rect, t)` is a thin forwarder over `Player::draw_at` — provided so sketches holding a player on `self` don't hit borrow-checker friction when `draw` also reads other `self` fields.

Lottie specifically supports both plain JSON (`from_json`) and `.lottie` archives (`from_dotlottie_bytes`, requires the `dotlottie` feature). See the `blinc_lottie` crate for asset-loading variants.

## Writing your own player

Anything that can resolve a pose from a float time implements `Player`. A minimal example: a player that renders an orbiting dot.

```rust
use blinc_canvas_kit::prelude::*;
use blinc::core::{Color, CornerRadius, Brush, Rect};

struct Orbit;

impl Player for Orbit {
    fn duration(&self) -> Option<f32> { None }  // plays forever

    fn draw_at(&mut self, ctx: &mut SketchContext<'_>, rect: Rect, t: f32) {
        let cx = rect.x() + rect.width() * 0.5;
        let cy = rect.y() + rect.height() * 0.5;
        let r = rect.width().min(rect.height()) * 0.4;
        let a = t * std::f32::consts::TAU * 0.5;
        let x = cx + r * a.cos() - 8.0;
        let y = cy + r * a.sin() - 8.0;

        ctx.draw_context().fill_rect(
            Rect::new(x, y, 16.0, 16.0),
            CornerRadius::uniform(8.0),
            Brush::Solid(Color::WHITE),
        );
    }
}
```

Drop `Orbit` into any sketch via `ctx.play(&mut orbit, rect, t)` and it composes with other players, sketches, and UI in the same frame.
