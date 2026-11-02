//! The plot every line, area and bar chart is: its geometry, the values it
//! shows while easing to new ones, and what it draws on its canvas.

use std::sync::{Arc, Mutex};

use blinc_animation::{SharedAnimatedValue, SpringConfig};
use blinc_core::reactive::{Computed, Signal};
use blinc_core::{Brush, Color, CornerRadius, DrawContext, Gradient, Path, Point, Rect};
use blinc_layout::text_measure::{TextLayoutOptions, measure_text_with_options};

use super::math::{flatten, follow, nice_ticks};
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
    pub bars: bool,
    /// Bars along the width, categories down the side.
    pub horizontal: bool,
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
/// it maps, along its height or, for horizontal bars, its width.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Geometry {
    pub horizontal: bool,
    pub lo: f64,
    pub hi: f64,
    pub ticks: Vec<f64>,
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl Geometry {
    /// The plot's box in a canvas `width` wide, for `series` under `labels`
    /// drawn by `opts`.
    pub fn of(series: &[ChartSeries], labels: &[String], opts: &Options, width: f32) -> Self {
        let horizontal = opts.bars && opts.horizontal;
        let (d0, d1) = domain(series, opts);
        let span = if horizontal {
            width
        } else {
            opts.height - TOP - BOTTOM
        };
        let ticks = if opts.axes || opts.grid {
            let room = if horizontal { 80.0 } else { 44.0 };
            nice_ticks(d0, d1, ((span.max(1.0) / room) as usize).max(2))
        } else {
            vec![d0, d1]
        };
        let lo = d0.min(ticks[0]);
        let hi = d1.max(*ticks.last().unwrap_or(&d1));
        let x0 = if opts.axes {
            // Room for what is beside the plot: the value ticks, or the
            // categories of horizontal bars.
            let options = TextLayoutOptions::new();
            let texts: Vec<String> = if horizontal {
                labels.to_vec()
            } else {
                ticks.iter().map(|t| (opts.format)(*t)).collect()
            };
            let widest = texts
                .iter()
                .map(|t| measure_text_with_options(t, LABEL_SIZE, &options).width)
                .fold(0.0f32, f32::max);
            (widest + 12.0).min(140.0)
        } else {
            4.0
        };
        let y1 = opts.height - if opts.axes { BOTTOM } else { 4.0 };
        Self {
            horizontal,
            lo,
            hi,
            ticks,
            x0,
            y0: TOP,
            x1: (width - RIGHT).max(x0 + 1.0),
            y1: y1.max(TOP + 1.0),
        }
    }

    /// Where a value falls along the value axis: a height, or for
    /// horizontal bars a distance across.
    pub fn at(&self, v: f64) -> f32 {
        let f = ((v - self.lo) / (self.hi - self.lo)) as f32;
        if self.horizontal {
            self.x0 + f * (self.x1 - self.x0)
        } else {
            self.y1 - f * (self.y1 - self.y0)
        }
    }

    /// The category axis's start and end.
    fn categories(&self) -> (f32, f32) {
        if self.horizontal {
            (self.y0, self.y1)
        } else {
            (self.x0, self.x1)
        }
    }

    /// The length of one of `n` category slots.
    pub fn slot(&self, n: usize) -> f32 {
        let (a, b) = self.categories();
        (b - a) / n.max(1) as f32
    }

    /// Where category `i` of `n` starts along the category axis.
    pub fn start(&self, i: usize, n: usize) -> f32 {
        self.categories().0 + i as f32 * self.slot(n)
    }

    /// The centre of category `i` of `n` along the category axis.
    pub fn centre(&self, i: usize, n: usize) -> f32 {
        self.start(i, n) + self.slot(n) / 2.0
    }

    /// The category of `n` at `p` along the category axis, if any.
    pub fn category_at(&self, p: f32, n: usize) -> Option<usize> {
        let (a, b) = self.categories();
        if n == 0 || p < a || p > b {
            return None;
        }
        Some((((p - a) / self.slot(n)) as usize).min(n - 1))
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
    if opts.area || opts.bars {
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
    pub labels: Computed<Vec<String>>,
    pub hover: Signal<i32>,
    pub width: Signal<f32>,
    pub opts: Options,
    easing: Mutex<Easing>,
    /// From 0 to 1 while easing to new values; none without a scheduler.
    progress: Option<SharedAnimatedValue>,
}

impl Plot {
    pub fn new(
        series: Computed<Vec<ChartSeries>>,
        labels: Computed<Vec<String>>,
        opts: Options,
    ) -> Arc<Self> {
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
            labels,
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
        let labels = self.labels.try_get().unwrap_or_default();
        let g = Geometry::of(&series, &labels, opts, width);
        let n = count(&series);
        let hover = self.hover.try_get().unwrap_or(-1);
        let at = (hover >= 0 && (hover as usize) < n).then_some(hover as usize);

        // Everything but the area fills is an SDF primitive, antialiased by
        // its distance field on any target, multisampled or not.
        if opts.grid {
            let grid = ink("var(--cn-chart-grid)", Color::rgba(0.5, 0.5, 0.5, 0.4));
            // Dashes 3 on, 4 off, across the plot at each tick, or down it
            // for horizontal bars; a pixel wide on a pixel row.
            for t in &g.ticks {
                let p = g.at(*t).round();
                let (from, to) = if g.horizontal {
                    (g.y0, g.y1)
                } else {
                    (g.x0, g.x1)
                };
                let mut q = from;
                while q < to {
                    let len = (to - q).min(3.0);
                    let dash = if g.horizontal {
                        Rect::new(p, q, 1.0, len)
                    } else {
                        Rect::new(q, p, len, 1.0)
                    };
                    ctx.fill_rect(dash, CornerRadius::ZERO, Brush::Solid(grid));
                    q += 7.0;
                }
            }
        }
        // The category under the pointer: a band behind its bars, a line
        // through its points.
        if let (Some(i), true) = (at, opts.tooltip) {
            let cursor = ink("var(--cn-chart-cursor)", Color::rgb(0.6, 0.6, 0.6));
            let (mark, alpha) = if opts.bars {
                let (s, size) = (g.start(i, n), g.slot(n));
                let band = if g.horizontal {
                    Rect::new(g.x0, s, g.x1 - g.x0, size)
                } else {
                    Rect::new(s, g.y0, size, g.y1 - g.y0)
                };
                (band, 0.08)
            } else {
                let x = g.centre(i, n).round();
                (Rect::new(x, g.y0, 1.0, g.y1 - g.y0), 0.5)
            };
            ctx.fill_rect(
                mark,
                CornerRadius::ZERO,
                Brush::Solid(cursor.with_alpha(alpha)),
            );
        }
        if opts.bars {
            self.draw_bars(ctx, &g, &series, &values);
        } else {
            self.draw_lines(ctx, &g, &series, &values, at);
        }
    }

    /// Bars side by side in each slot, or piled when stacked, rounded at
    /// the end away from the axis; in a pile only the outermost on its side
    /// of the axis is.
    fn draw_bars(
        &self,
        ctx: &mut dyn DrawContext,
        g: &Geometry,
        series: &[ChartSeries],
        values: &[Vec<f64>],
    ) {
        let n = count(series);
        let stacked = self.opts.stacked;
        let groups = if stacked { 1 } else { series.len().max(1) };
        let value =
            |k: usize, i: usize| values.get(k).and_then(|v| v.get(i)).copied().unwrap_or(0.0);
        for i in 0..n {
            let (s, size) = (g.start(i, n), g.slot(n));
            let pad = size * if groups > 1 { 0.12 } else { 0.18 };
            let inner = size - pad * 2.0;
            let gap = if groups > 1 {
                (inner * 0.08).min(4.0)
            } else {
                0.0
            };
            let thick = ((inner - gap * (groups - 1) as f32) / groups as f32).max(1.0);
            let (mut up, mut down) = (0.0, 0.0);
            for (k, s_k) in series.iter().enumerate() {
                let v = value(k, i);
                let mut base = 0.0;
                if stacked {
                    base = if v >= 0.0 { up } else { down };
                    if v >= 0.0 { up += v } else { down += v }
                }
                let (a, b) = (g.at(base), g.at(base + v));
                let along = s
                    + pad
                    + if stacked {
                        0.0
                    } else {
                        k as f32 * (thick + gap)
                    };
                let outermost = !stacked
                    || (k + 1..series.len()).all(|j| {
                        let w = value(j, i);
                        w == 0.0 || (w >= 0.0) != (v >= 0.0)
                    });
                let r = if outermost {
                    (thick / 2.0).min(4.0)
                } else {
                    0.0
                };
                let (rect, end) = if g.horizontal {
                    let rect = Rect::new(a.min(b), along, (b - a).abs(), thick);
                    (rect, if b >= a { End::Right } else { End::Left })
                } else {
                    let rect = Rect::new(along, a.min(b), thick, (b - a).abs());
                    (rect, if b <= a { End::Top } else { End::Bottom })
                };
                let r = r.min(rect.size.width / 2.0).min(rect.size.height / 2.0);
                ctx.fill_rect(rect, end.rounded(r), Brush::Solid(series_ink(s_k, k)));
            }
        }
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
                xs.push(g.centre(i, n));
                ys.push(g.at(top));
                bases.push(g.at(base.clamp(g.lo, g.hi)));
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
            // The line as capsules along the flattened curve: their round
            // ends make the joins.
            let points = flatten(&xs, &ys, opts.curve);
            ctx.sdf_build(&mut |b| {
                for pair in points.windows(2) {
                    let (p, q) = (
                        Point::new(pair[0].0, pair[0].1),
                        Point::new(pair[1].0, pair[1].1),
                    );
                    let segment = b.line(p, q, 2.0);
                    b.fill(segment, Brush::Solid(c));
                }
            });
            for i in 0..xs.len() {
                let big = Some(i) == at;
                if !(opts.dots || big) {
                    continue;
                }
                let centre = Point::new(xs[i], ys[i]);
                let (ring, dot) = if big { (5.0, 3.5) } else { (3.5, 2.5) };
                ctx.fill_circle(centre, ring, Brush::Solid(surface));
                ctx.fill_circle(centre, dot, Brush::Solid(c));
            }
        }
    }
}

/// The end of a bar away from the axis, the one that is rounded.
#[derive(Clone, Copy)]
enum End {
    Top,
    Right,
    Bottom,
    Left,
}

impl End {
    /// Corner radii rounding this end by `r` and leaving the other square.
    fn rounded(self, r: f32) -> CornerRadius {
        let (tl, tr, br, bl) = match self {
            End::Top => (r, r, 0.0, 0.0),
            End::Right => (0.0, r, r, 0.0),
            End::Bottom => (0.0, 0.0, r, r),
            End::Left => (r, 0.0, 0.0, r),
        };
        CornerRadius {
            top_left: tl,
            top_right: tr,
            bottom_right: br,
            bottom_left: bl,
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
            bars: false,
            horizontal: false,
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
        let g = Geometry::of(&s, &[], &opts(), 400.0);
        let n = 4;
        assert_eq!(g.category_at(g.x0 - 1.0, n), None);
        assert_eq!(g.category_at(g.centre(0, n), n), Some(0));
        assert_eq!(g.category_at(g.centre(3, n), n), Some(3));
        assert_eq!(g.category_at(g.x1, n), Some(3));
    }

    #[test]
    fn values_blend_towards_the_new_ones() {
        let from = vec![vec![0.0, 10.0]];
        let to = vec![vec![10.0, 20.0, 30.0]];
        assert_eq!(blend(&from, &to, 0.5), vec![vec![5.0, 15.0, 15.0]]);
    }
}
