//! Deterministic pointer paths for the native recordings.
type Position = (f32, f32);

pub fn origin(slug: &str) -> Position {
    let seed = seed(slug);
    (414.0 + seed * 28.0, 258.0 + seed * 14.0)
}

pub fn position(slug: &str, frame: usize, target: Position, second: Position) -> Position {
    let seed = seed(slug);
    let arrival = 10 + (seed * 2.0).round() as usize;
    let departure = 20 + (seed * 2.0).round() as usize;
    let origin = origin(slug);
    if matches!(slug, "button" | "input") {
        return match frame {
            i if i <= arrival => travel(origin, target, i as f32 / arrival as f32, 0.0),
            i if i < 36 => target,
            i if i < 44 => travel(target, origin, (i - 36) as f32 / 7.0, 0.0),
            _ => origin,
        };
    }
    match frame {
        i if i <= arrival => travel(
            origin,
            target,
            i as f32 / arrival as f32,
            30.0 + seed * 22.0,
        ),
        i if i < departure => hover(
            target,
            (i - arrival) as f32 / (departure - arrival) as f32,
            seed,
        ),
        i if i < 28 => travel(
            target,
            second,
            (i - departure) as f32 / (27 - departure) as f32,
            if matches!(slug, "pie-chart" | "donut-chart") {
                -44.0
            } else {
                18.0 + seed * 12.0
            },
        ),
        i if i < 36 => hover(second, (i - 28) as f32 / 7.0, seed + 1.0),
        i => travel(second, origin, (i - 36) as f32 / 11.0, 38.0 + seed * 16.0),
    }
}

fn seed(slug: &str) -> f32 {
    (slug.bytes().fold(0_u32, |value, byte| {
        value.wrapping_mul(31).wrapping_add(byte as u32)
    }) % 97) as f32
        / 96.0
}

fn travel(from: Position, to: Position, t: f32, bend: f32) -> Position {
    let t = t.clamp(0.0, 1.0);
    let t = t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let length = dx.hypot(dy).max(1.0);
    let bend = bend.clamp(-length * 0.65, length * 0.65);
    let normal = (-dy / length * bend, dx / length * bend);
    let a = (from.0 + dx * 0.3 + normal.0, from.1 + dy * 0.3 + normal.1);
    let b = (
        from.0 + dx * 0.72 + normal.0 * 0.65,
        from.1 + dy * 0.72 + normal.1 * 0.65,
    );
    let u = 1.0 - t;
    (
        u.powi(3) * from.0 + 3.0 * u * u * t * a.0 + 3.0 * u * t * t * b.0 + t.powi(3) * to.0,
        u.powi(3) * from.1 + 3.0 * u * u * t * a.1 + 3.0 * u * t * t * b.1 + t.powi(3) * to.1,
    )
}

fn hover(at: Position, t: f32, phase: f32) -> Position {
    let envelope = (std::f32::consts::PI * t).sin();
    (
        at.0 + 1.5 * envelope * (t + phase).sin(),
        at.1 + 0.9 * envelope * (t * 1.8 + phase).cos(),
    )
}
