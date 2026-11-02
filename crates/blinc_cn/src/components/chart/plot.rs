//! The plot every line and area chart is: its geometry, the values it shows
//! while easing to new ones, and what it draws on its canvas.

use std::sync::{Arc, Mutex};

use blinc_animation::{SharedAnimatedValue, SpringConfig};
use blinc_core::reactive::{Computed, Signal};
use blinc_core::{Brush, Color, DrawContext, Gradient, LineCap, LineJoin, Path, Point, Stroke};
use blinc_layout::text_measure::{TextLayoutOptions, measure_text_with_options};

use super::math::{follow, nice_ticks};
use super::{ChartCurve, ChartSeries};

/// Room above the plot, and below it for the category labels.
pub(super) const TOP: f32 = 8.0;
pub(super) const BOTTOM: f32 = 24.0;
/// Right of the plot.
const RIGHT: f32 = 8.0;
/// The axis labels' size, for measuring the room they take.
const LABEL_SIZE: f32 = 12.0;
const PALETTE: usize = 5;

pub(super) type Format = Arc<dyn Fn(f64) -> String + Send + Sync>;

/// What a chart draws and how.
#[derive(Clone)]
pub(super) struct Options {
    pub area: bool,
    pub height: f32,
    pub curve: ChartCurve,
    pub dots: bool,
    pub stacked: bool,
    pub grid: bool,
    pub axes: bool,
    pub tooltip: bool,
    pub format: Format,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

/// Where things are: the plot's box inside the canvas and the value range
/// its height maps.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Geometry {
    pub lo: f64,
    pub hi: f64,
    pub ticks: Vec<f64>,
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl Geometry {
    /// The plot's box in a canvas `width` wide, for `series` drawn by `opts`.
    pub fn of(series: &[ChartSeries], opts: &Options, width: f32) -> Self {
        let (d0, d1) = domain(series, opts);
        let span = (opts.height - TOP - BOTTOM).max(1.0);
        let ticks = if opts.axes || opts.grid {
            nice_ticks(d0, d1, ((span / 44.0) as usize).max(2))
        } else {
            vec![d0, d1]
        };
        let lo = d0.min(ticks[0]);
        let hi = d1.max(*ticks.last().unwrap_or(&d1));
        let x0 = if opts.axes {
            let options = TextLayoutOptions::new();
            let widest = ticks
                .iter()
                .map(|t| measure_text_with_options(&(opts.format)(*t), LABEL_SIZE, &options).width)
                .fold(0.0f32, f32::max);
            (widest + 12.0).min(140.0)
        } else {
            4.0
        };
        let y1 = opts.height - if opts.axes { BOTTOM } else { 4.0 };
        Self {
            lo,
            hi,
            ticks,
            x0,
            y0: TOP,
            x1: (width - RIGHT).max(x0 + 1.0),
            y1: y1.max(TOP + 1.0),
        }
    }

    /// A value's height on the plot.
    pub fn y(&self, v: f64) -> f32 {
        let f = ((v - self.lo) / (self.hi - self.lo)) as f32;
        self.y1 - f * (self.y1 - self.y0)
    }

    /// The width of one of `n` category slots.
    pub fn slot(&self, n: usize) -> f32 {
        (self.x1 - self.x0) / n.max(1) as f32
    }

    /// The centre of category `i` of `n`.
    pub fn x(&self, i: usize, n: usize) -> f32 {
        self.x0 + (i as f32 + 0.5) * self.slot(n)
    }

    /// The category of `n` under `x`, if any.
    pub fn category_at(&self, x: f32, n: usize) -> Option<usize> {
        if n == 0 || x < self.x0 || x > self.x1 {
            return None;
        }
        Some((((x - self.x0) / self.slot(n)) as usize).min(n - 1))
    }
}

/// The values the value axis spans: what every series reaches, piled when
/// stacked, from zero for areas, within `min` and `max` when set.
fn domain(series: &[ChartSeries], opts: &Options) -> (f64, f64) {
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    if opts.stacked {
        for i in 0..count(series) {
            let (mut up, mut down) = (0.0, 0.0);
            for s in series {
                let v = s.values.get(i).copied().unwrap_or(0.0);
                if v >= 0.0 { up += v } else { down += v }
            }
            hi = hi.max(up);
            lo = lo.min(down);
        }
    } else {
        for v in series.iter().flat_map(|s| s.values.iter()) {
            lo = lo.min(*v);
            hi = hi.max(*v);
        }
    }
    if !lo.is_finite() {
        (lo, hi) = (0.0, 1.0);
    }
    if opts.area {
        (lo, hi) = (lo.min(0.0), hi.max(0.0));
    }
    let (lo, hi) = (opts.min.unwrap_or(lo), opts.max.unwrap_or(hi));
    (lo, if hi == lo { lo + 1.0 } else { hi })
}

/// The number of categories: the longest series.
pub(super) fn count(series: &[ChartSeries]) -> usize {
    series.iter().map(|s| s.values.len()).max().unwrap_or(0)
}

/// A colour the active stylesheet gives `css`, `var(--chart-1)` or a
/// literal, or `fallback` without one.
fn ink(css: &str, fallback: Color) -> Color {
    blinc_layout::css_parser::active_stylesheet()
        .and_then(|sheet| sheet.resolve_color(css))
        .unwrap_or(fallback)
}

/// Series `k`'s colour: its own, or the palette's next.
pub(super) fn series_ink(series: &ChartSeries, k: usize) -> Color {
    match &series.color {
        Some(css) => ink(css, Color::rgb(0.53, 0.53, 0.53)),
        None => ink(
            &format!("var(--chart-{})", k % PALETTE + 1),
            Color::rgb(0.53, 0.53, 0.53),
        ),
    }
}

/// The palette class a series' swatch takes, when it has no colour of its own.
pub(super) fn palette_class(k: usize) -> String {
    format!("cn-chart-c{}", k % PALETTE + 1)
}

/// The values on screen, easing from those drawn before a change to the new
/// ones.
struct Easing {
    from: Vec<Vec<f64>>,
    to: Vec<Vec<f64>>,
    /// The data the drawing is heading to, to tell a change.
    seen: Option<Vec<ChartSeries>>,
}

/// A chart's live state: its data, the category under the pointer, its width
/// and the easing between values.
pub(super) struct Plot {
    pub series: Computed<Vec<ChartSeries>>,
    pub hover: Signal<i32>,
    pub width: Signal<f32>,
    pub opts: Options,
    easing: Mutex<Easing>,
    /// From 0 to 1 while easing to new values; none without a scheduler.
    progress: Option<SharedAnimatedValue>,
}

impl Plot {
    pub fn new(series: Computed<Vec<ChartSeries>>, opts: Options) -> Arc<Self> {
        let progress = blinc_layout::render_state::get_global_scheduler().map(|handle| {
            // Critically damped: it settles on the new values without
            // passing them.
            Arc::new(Mutex::new(blinc_animation::AnimatedValue::new(
                handle,
                1.0,
                SpringConfig::new(120.0, 22.0, 1.0),
            )))
        });
        let plot = Arc::new(Self {
            series,
            hover: blinc_core::reactive::signal(-1),
            width: blinc_core::reactive::signal(400.0),
            opts,
            easing: Mutex::new(Easing {
                from: Vec::new(),
                to: Vec::new(),
                seen: None,
            }),
            progress,
        });
        // New data starts its easing when it arrives, not when it is next
        // drawn.
        let (watched, series) = (Arc::downgrade(&plot), plot.series.clone());
        let _ = blinc_core::reactive::effect(move |_: &blinc_core::reactive::ReactiveGraph| {
            if let (Some(plot), Some(list)) = (watched.upgrade(), series.try_get()) {
                plot.note(&list);
            }
        });
        plot
    }

    fn progress(&self) -> f64 {
        self.progress
            .as_ref()
            .map_or(1.0, |p| p.lock().unwrap().get().clamp(0.0, 1.0) as f64)
    }

    /// Whether the canvas has to draw again: the data changed, or the values
    /// are still easing.
    pub fn wants_frames(&self) -> bool {
        let easing = self
            .progress
            .as_ref()
            .is_some_and(|p| p.lock().unwrap().is_animating());
        easing || self.easing.lock().unwrap().seen != self.series.try_get()
    }

    /// The values to draw now.
    fn shown(&self, series: &[ChartSeries]) -> Vec<Vec<f64>> {
        self.note(series);
        let e = self.easing.lock().unwrap();
        blend(&e.from, &e.to, self.progress())
    }

    /// Start easing to `series` from what is on screen, if it is new.
    fn note(&self, series: &[ChartSeries]) {
        let mut e = self.easing.lock().unwrap();
        if e.seen.as_deref() != Some(series) {
            let now = blend(&e.from, &e.to, self.progress());
            e.from = if e.seen.is_none() {
                // The first values are drawn as they are.
                series.iter().map(|s| s.values.clone()).collect()
            } else {
                now
            };
            e.to = series.iter().map(|s| s.values.clone()).collect();
            e.seen = Some(series.to_vec());
            if let Some(p) = &self.progress {
                let mut p = p.lock().unwrap();
                p.set_immediate(0.0);
                p.set_target(1.0);
            }
        }
    }

    /// Draw the plot on a canvas `width` by its height.
    pub fn draw(&self, ctx: &mut dyn DrawContext, width: f32) {
        let Some(series) = self.series.try_get() else {
            return;
        };
        let values = self.shown(&series);
        let opts = &self.opts;
        let g = Geometry::of(&series, opts, width);
        let n = count(&series);
        let hover = self.hover.try_get().unwrap_or(-1);
        let at = (hover >= 0 && (hover as usize) < n).then_some(hover as usize);

        if opts.grid {
            let grid = ink("var(--cn-chart-grid)", Color::rgba(0.5, 0.5, 0.5, 0.4));
            // Dashes as segments of one path: 3 on, 4 off.
            let mut dashes = Path::new();
            for t in &g.ticks {
                let y = g.y(*t).round() + 0.5;
                let mut x = g.x0;
                while x < g.x1 {
                    dashes = dashes.move_to(x, y).line_to((x + 3.0).min(g.x1), y);
                    x += 7.0;
                }
            }
            ctx.stroke_path(&dashes, &Stroke::new(1.0), Brush::Solid(grid));
        }
        if let (Some(i), true) = (at, opts.tooltip) {
            let cursor = ink("var(--cn-chart-cursor)", Color::rgb(0.6, 0.6, 0.6));
            let x = g.x(i, n);
            let line = Path::new().move_to(x, g.y0).line_to(x, g.y1);
            ctx.stroke_path(
                &line,
                &Stroke::new(1.0),
                Brush::Solid(cursor.with_alpha(0.5)),
            );
        }
        self.draw_lines(ctx, &g, &series, &values, at);
    }

    fn draw_lines(
        &self,
        ctx: &mut dyn DrawContext,
        g: &Geometry,
        series: &[ChartSeries],
        values: &[Vec<f64>],
        at: Option<usize>,
    ) {
        let opts = &self.opts;
        let n = count(series);
        let surface = ink("var(--cn-chart-surface)", Color::WHITE);
        let mut below = vec![0.0f64; n];
        for (k, s) in series.iter().enumerate() {
            let c = series_ink(s, k);
            let vals = values.get(k).map(Vec::as_slice).unwrap_or(&[]);
            let (mut xs, mut ys, mut bases) = (Vec::new(), Vec::new(), Vec::new());
            for (i, v) in vals.iter().enumerate().take(n) {
                let base = if opts.stacked { below[i] } else { 0.0 };
                let top = v + base;
                if opts.stacked {
                    below[i] = top;
                }
                xs.push(g.x(i, n));
                ys.push(g.y(top));
                bases.push(g.y(base.clamp(g.lo, g.hi)));
            }
            if xs.is_empty() {
                continue;
            }
            if opts.area {
                let mut fill = follow(Path::new(), &xs, &ys, opts.curve, true);
                let (rx, rb): (Vec<f32>, Vec<f32>) = xs
                    .iter()
                    .rev()
                    .copied()
                    .zip(bases.iter().rev().copied())
                    .unzip();
                fill = if opts.stacked && k > 0 {
                    // Back along the series below.
                    follow(fill, &rx, &rb, opts.curve, false)
                } else {
                    fill.line_to(rx[0], rb[0])
                        .line_to(rx[rx.len() - 1], rb[rb.len() - 1])
                };
                let fade = Gradient::linear(
                    Point::new(0.0, g.y0),
                    Point::new(0.0, g.y1),
                    c.with_alpha(c.a * 0.38),
                    c.with_alpha(c.a * 0.03),
                );
                ctx.fill_path(&fill.close(), Brush::Gradient(fade));
            }
            let line = follow(Path::new(), &xs, &ys, opts.curve, true);
            let stroke = Stroke::new(2.0)
                .with_cap(LineCap::Round)
                .with_join(LineJoin::Round);
            ctx.stroke_path(&line, &stroke, Brush::Solid(c));
            for i in 0..xs.len() {
                let big = Some(i) == at;
                if !(opts.dots || big) {
                    continue;
                }
                let centre = Point::new(xs[i], ys[i]);
                let (ring, dot) = if big { (5.0, 3.5) } else { (3.5, 2.5) };
                ctx.fill_path(&Path::circle(centre, ring), Brush::Solid(surface));
                ctx.fill_path(&Path::circle(centre, dot), Brush::Solid(c));
            }
        }
    }
}

/// `from` moved `t` of the way to `to`; a value with nothing before it
/// starts from zero.
fn blend(from: &[Vec<f64>], to: &[Vec<f64>], t: f64) -> Vec<Vec<f64>> {
    to.iter()
        .enumerate()
        .map(|(s, row)| {
            row.iter()
                .enumerate()
                .map(|(i, b)| {
                    let a = from.get(s).and_then(|r| r.get(i)).copied().unwrap_or(0.0);
                    a + (b - a) * t
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> Options {
        Options {
            area: false,
            height: 200.0,
            curve: ChartCurve::Smooth,
            dots: false,
            stacked: false,
            grid: true,
            axes: true,
            tooltip: true,
            format: Arc::new(super::super::math::compact),
            min: None,
            max: None,
        }
    }

    #[test]
    fn stacked_values_pile_and_areas_start_at_zero() {
        let s = [
            ChartSeries::new("a", vec![10.0, 20.0]),
            ChartSeries::new("b", vec![5.0, 30.0]),
        ];
        let mut o = opts();
        assert_eq!(domain(&s, &o), (5.0, 30.0));
        o.stacked = true;
        assert_eq!(domain(&s, &o), (0.0, 50.0));
        o.stacked = false;
        o.area = true;
        assert_eq!(domain(&s, &o), (0.0, 30.0));
    }

    #[test]
    fn the_pointer_finds_its_category() {
        let s = [ChartSeries::new("a", vec![1.0, 2.0, 3.0, 4.0])];
        let g = Geometry::of(&s, &opts(), 400.0);
        let n = 4;
        assert_eq!(g.category_at(g.x0 - 1.0, n), None);
        assert_eq!(g.category_at(g.x(0, n), n), Some(0));
        assert_eq!(g.category_at(g.x(3, n), n), Some(3));
        assert_eq!(g.category_at(g.x1, n), Some(3));
    }

    #[test]
    fn values_blend_towards_the_new_ones() {
        let from = vec![vec![0.0, 10.0]];
        let to = vec![vec![10.0, 20.0, 30.0]];
        assert_eq!(blend(&from, &to, 0.5), vec![vec![5.0, 15.0, 15.0]]);
    }
}
