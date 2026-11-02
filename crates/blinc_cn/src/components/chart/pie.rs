//! Pie and donut charts: slices by their share of the total, clockwise from
//! the top, sweeping in and easing to new values. Hovering pulls a slice out
//! and shows its value and share.

use std::cell::OnceCell;
use std::f32::consts::{FRAC_PI_2, TAU};
use std::sync::Arc;

use blinc_core::reactive::{Computed, ReactiveGraph, Signal, computed};
use blinc_core::{Brush, Color, DrawContext, Path, Point};
use blinc_layout::binding::{IntoReactive, Reactive};
use blinc_layout::canvas::canvas;
use blinc_layout::div::ElementTypeId;
use blinc_layout::element::RenderProps;
use blinc_layout::prelude::*;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};

use super::ease::Easer;
use super::math::compact;
use super::plot::{Format, ink, palette_ink};
use super::view::{label, mark};

/// How far the slice under the pointer moves out.
const PULL: f32 = 6.0;

/// A labelled slice of a pie.
#[derive(Clone, Debug, PartialEq)]
pub struct PieSlice {
    pub label: String,
    pub value: f64,
    /// Any CSS colour; the next palette colour if unset.
    pub color: Option<String>,
}

impl PieSlice {
    pub fn new(label: impl Into<String>, value: f64) -> Self {
        Self {
            label: label.into(),
            value,
            color: None,
        }
    }

    /// Its own colour, any CSS colour, in place of the palette's.
    pub fn color(mut self, css: impl Into<String>) -> Self {
        self.color = Some(css.into());
        self
    }
}

/// Each slice's share of the total, from 0 to 1; negative values count as
/// none.
fn shares(slices: &[PieSlice]) -> Vec<f64> {
    let total: f64 = slices.iter().map(|s| s.value.max(0.0)).sum();
    slices
        .iter()
        .map(|s| {
            if total > 0.0 {
                s.value.max(0.0) / total
            } else {
                0.0
            }
        })
        .collect()
}

/// Each share's start and end angle, clockwise from the top.
fn angles(shares: &[f64]) -> Vec<(f32, f32)> {
    let mut at = -FRAC_PI_2;
    shares
        .iter()
        .map(|share| {
            let end = at + *share as f32 * TAU;
            let span = (at, end);
            at = end;
            span
        })
        .collect()
}

/// A pie's live state.
struct Pie {
    slices: Computed<Vec<PieSlice>>,
    hover: Signal<i32>,
    easer: Easer,
    size: f32,
    /// The hole's radius as a share of the pie's.
    hole: f32,
}

impl Pie {
    fn radius(&self) -> f32 {
        self.size / 2.0 - PULL - 2.0
    }

    /// The slice at `(x, y)` from the pie's top left, if any.
    fn slice_at(&self, x: f32, y: f32) -> Option<usize> {
        let (dx, dy) = (x - self.size / 2.0, y - self.size / 2.0);
        let d = (dx * dx + dy * dy).sqrt();
        let r = self.radius();
        if d > r + PULL || d < r * self.hole {
            return None;
        }
        let mut angle = dy.atan2(dx);
        if angle < -FRAC_PI_2 {
            angle += TAU;
        }
        let list = self.slices.try_get().unwrap_or_default();
        angles(&shares(&list))
            .iter()
            .position(|(start, end)| angle >= *start && angle < *end)
    }

    fn draw(&self, ctx: &mut dyn DrawContext) {
        let list = self.slices.try_get().unwrap_or_default();
        self.easer.note(&[shares(&list)]);
        let shown = self.easer.shown().pop().unwrap_or_default();
        let (c, r) = (self.size / 2.0, self.radius());
        if r <= 0.0 {
            return;
        }
        let at = self.hover.try_get().unwrap_or(-1);
        let gap = ink("var(--cn-chart-surface)", Color::WHITE);
        let spans = angles(&shown);
        let several = spans.iter().filter(|(a, b)| b > a).count() > 1;
        for (i, (start, end)) in spans.into_iter().enumerate() {
            if end <= start {
                continue;
            }
            let mid = (start + end) / 2.0;
            let pull = if i as i32 == at { PULL } else { 0.0 };
            let centre = Point::new(c + mid.cos() * pull, c + mid.sin() * pull);
            let outer = arc(centre, r, start, end);
            let inner = if self.hole > 0.0 {
                arc(centre, r * self.hole, end, start)
            } else {
                vec![centre]
            };
            let outline: Vec<Point> = outer.iter().chain(inner.iter()).copied().collect();
            let mut fill = Path::new().move_to(outline[0].x, outline[0].y);
            for p in &outline[1..] {
                fill = fill.line_to(p.x, p.y);
            }
            let colour = palette_ink(list.get(i).and_then(|s| s.color.as_deref()), i);
            ctx.fill_path(&fill.close(), Brush::Solid(colour));
            // The slices are set apart by the surface's colour along their
            // edges, drawn as capsules so every visible edge is an SDF one.
            // A lone slice has no sides to set apart, only its arcs.
            let mut edges: Vec<(Point, Point)> = outer.windows(2).map(|w| (w[0], w[1])).collect();
            if self.hole > 0.0 {
                edges.extend(inner.windows(2).map(|w| (w[0], w[1])));
            }
            if several {
                edges.push((*outer.last().unwrap(), inner[0]));
                edges.push((*inner.last().unwrap(), outer[0]));
            }
            ctx.sdf_build(&mut |b| {
                for (p, q) in &edges {
                    let edge = b.line(*p, *q, 2.0);
                    b.fill(edge, Brush::Solid(gap));
                }
            });
        }
    }
}

/// Points along the arc of radius `r` round `centre` from `start` to `end`,
/// about 3px apart.
fn arc(centre: Point, r: f32, start: f32, end: f32) -> Vec<Point> {
    let steps = (((end - start).abs() * r / 3.0).ceil() as usize).max(2);
    (0..=steps)
        .map(|k| {
            let a = start + (end - start) * k as f32 / steps as f32;
            Point::new(centre.x + a.cos() * r, centre.y + a.sin() * r)
        })
        .collect()
}

/// Builder for a pie or donut chart; usable inline as an element.
pub struct PieBuilder {
    slices: Vec<PieSlice>,
    data: Option<Reactive<Vec<PieSlice>>>,
    hole: f32,
    total: Option<String>,
    size: f32,
    legend: bool,
    tooltip: bool,
    format: Format,
    classes: Vec<String>,
    built: OnceCell<Div>,
}

impl PieBuilder {
    fn new() -> Self {
        Self {
            slices: Vec::new(),
            data: None,
            hole: 0.0,
            total: None,
            size: 220.0,
            legend: true,
            tooltip: true,
            format: Arc::new(compact),
            classes: Vec::new(),
            built: OnceCell::new(),
        }
    }

    /// A slice of constant value, in the next palette colour.
    pub fn slice(mut self, label: impl Into<String>, value: f64) -> Self {
        self.slices.push(PieSlice::new(label, value));
        self
    }

    /// A slice of constant value in its own colour, any CSS colour.
    pub fn slice_colored(
        mut self,
        label: impl Into<String>,
        value: f64,
        css: impl Into<String>,
    ) -> Self {
        self.slices.push(PieSlice::new(label, value).color(css));
        self
    }

    /// All the slices at once: a list, Signal or Computed, followed in
    /// place. Replaces any added with [`Self::slice`].
    pub fn data(mut self, slices: impl IntoReactive<Vec<PieSlice>>) -> Self {
        self.data = Some(slices.into_reactive());
        self
    }

    /// A hole in the middle, as a share of the radius from 0 to 0.9.
    pub fn donut(mut self, hole: f32) -> Self {
        self.hole = hole.clamp(0.0, 0.9);
        self
    }

    /// Writes the total in the hole with `caption` under it, such as
    /// "Visitors". Needs [`Self::donut`].
    pub fn total(mut self, caption: impl Into<String>) -> Self {
        self.total = Some(caption.into());
        self
    }

    /// The pie's diameter; 220 unless set.
    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    /// Whether the slices' labels and colours show under the pie; shown
    /// unless set.
    pub fn legend(mut self, shown: bool) -> Self {
        self.legend = shown;
        self
    }

    /// Without pulling out the slice under the pointer and its tooltip.
    pub fn no_tooltip(mut self) -> Self {
        self.tooltip = false;
        self
    }

    /// How values are written in the tooltip and the total; `12.5k` style
    /// unless set.
    pub fn format<F>(mut self, format: F) -> Self
    where
        F: Fn(f64) -> String + Send + Sync + 'static,
    {
        self.format = Arc::new(format);
        self
    }

    /// A class for rules of your own to match.
    pub fn class(mut self, name: impl Into<String>) -> Self {
        self.classes.push(name.into());
        self
    }

    fn get_or_build(&self) -> &Div {
        ::blinc_layout::build_once::build_once(&self.built, || self.build_pie())
    }

    fn build_pie(&self) -> Div {
        let slices = match self.data.clone() {
            None => {
                let constant = self.slices.clone();
                computed(move |_: &ReactiveGraph| constant.clone())
            }
            Some(Reactive::Const(list)) => computed(move |_: &ReactiveGraph| list.clone()),
            Some(Reactive::Bound(state)) => {
                let signal = state.signal();
                computed(move |g: &ReactiveGraph| g.get(signal).unwrap_or_default())
            }
            Some(Reactive::Computed(c)) => c,
        };
        let pie = Arc::new(Pie {
            slices: slices.clone(),
            hover: blinc_core::reactive::signal(-1),
            easer: Easer::new(true),
            size: self.size,
            hole: if self.hole > 0.0 { self.hole } else { 0.0 },
        });
        // New data starts its easing when it arrives.
        let (watched, source) = (Arc::downgrade(&pie), slices.clone());
        let _ = blinc_core::reactive::effect(move |_: &ReactiveGraph| {
            if let (Some(pie), Some(list)) = (watched.upgrade(), source.try_get()) {
                pie.easer.note(&[shares(&list)]);
            }
        });

        let canvas = {
            let (draw, wants) = (Arc::clone(&pie), Arc::clone(&pie));
            canvas(move |ctx, _| draw.draw(ctx))
                .absolute()
                .left(0.0)
                .top(0.0)
                .w_full()
                .h_full()
                .redraw_while(move || {
                    let list = wants.slices.try_get().unwrap_or_default();
                    wants.easer.wants_frames(Some(&[shares(&list)]))
                })
        };
        let mut plot = div()
            .class("cn-chart-plot")
            .relative()
            .w(self.size)
            .h(self.size)
            .flex_shrink_0()
            .child(canvas);
        if let (true, Some(caption)) = (self.hole > 0.0, self.total.clone()) {
            let (list, format) = (slices.clone(), Arc::clone(&self.format));
            plot = plot.child(
                div()
                    .absolute()
                    .left(0.0)
                    .top(0.0)
                    .w_full()
                    .h_full()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .pointer_events_none()
                    .child(label("cn-chart-pie-total", move |_| {
                        let total: f64 = list
                            .try_get()
                            .unwrap_or_default()
                            .iter()
                            .map(|s| s.value.max(0.0))
                            .sum();
                        format(total)
                    }))
                    .child(label("cn-chart-pie-caption", move |_| caption.clone())),
            );
        }
        if self.tooltip {
            plot = with_hover(plot, &pie, &self.format);
        }
        let mut root = div()
            .class("cn-chart")
            .attr("data-kind", "pie")
            .w_full()
            .flex_col()
            .items_center()
            .child(plot);
        if self.legend {
            root = root.child(legend(&slices));
        }
        self.classes.iter().fold(root, |d, c| d.class(c))
    }
}

/// The slice under the pointer moves out, and a tooltip of its label, value
/// and share shows over it.
fn with_hover(plot: Div, pie: &Arc<Pie>, format: &Format) -> Div {
    let hover = pie.hover;
    let at = {
        let slices = pie.slices.clone();
        computed(move |g: &ReactiveGraph| {
            let i = g.get(hover).unwrap_or(-1);
            let n = slices.try_get().map_or(0, |s| s.len());
            (i >= 0 && (i as usize) < n).then_some(i as usize)
        })
    };
    let hovered = {
        let at = at.clone();
        computed(move |_: &ReactiveGraph| at.try_get().flatten().is_some())
    };
    // Over the slice, a third of the way out from the middle.
    let place = {
        let (at, slices, size) = (at.clone(), pie.slices.clone(), pie.size);
        move |_: &ReactiveGraph| -> (f32, f32) {
            let Some(Some(i)) = at.try_get() else {
                return (0.0, 0.0);
            };
            let list = slices.try_get().unwrap_or_default();
            let Some((start, end)) = angles(&shares(&list)).get(i).copied() else {
                return (0.0, 0.0);
            };
            let mid = (start + end) / 2.0;
            (
                (size / 2.0 + mid.cos() * size * 0.32 - 40.0).max(0.0),
                (size / 2.0 + mid.sin() * size * 0.32 - 16.0).max(0.0),
            )
        }
    };
    let text = {
        let (at, slices, format) = (at.clone(), pie.slices.clone(), Arc::clone(format));
        move |_: &ReactiveGraph| -> (String, String) {
            let Some(Some(i)) = at.try_get() else {
                return (String::new(), String::new());
            };
            let list = slices.try_get().unwrap_or_default();
            let share = shares(&list).get(i).copied().unwrap_or(0.0);
            list.get(i).map_or((String::new(), String::new()), |s| {
                (
                    s.label.clone(),
                    format!("{} · {}%", format(s.value), (share * 100.0).round()),
                )
            })
        }
    };
    let (across, down, name, value) = (place.clone(), place, text.clone(), text);
    let holder = div()
        .absolute()
        .left(0.0)
        .top(0.0)
        .w_full()
        .h_full()
        .foreground()
        .flex_col()
        .pointer_events_none()
        .show(hovered, move || {
            let (across, down, name, value) =
                (across.clone(), down.clone(), name.clone(), value.clone());
            div()
                .flex_col()
                .child(
                    div()
                        .flex_shrink_0()
                        .h(computed(move |g: &ReactiveGraph| down(g).1)),
                )
                .child(
                    div()
                        .flex_row()
                        .child(
                            div()
                                .flex_shrink_0()
                                .w(computed(move |g: &ReactiveGraph| across(g).0)),
                        )
                        .child(
                            div()
                                .class("cn-chart-tooltip")
                                .flex_col()
                                .flex_shrink_0()
                                .child(
                                    div()
                                        .class("cn-chart-tooltip-row")
                                        .flex_row()
                                        .items_center()
                                        .child(label("cn-chart-tooltip-name", move |g| name(g).0))
                                        .child(label("cn-chart-tooltip-value", move |g| {
                                            value(g).1
                                        })),
                                ),
                        ),
                )
        });
    let (moved, leave) = (Arc::clone(pie), pie.hover);
    plot.child(holder)
        .on_mouse_move(move |ctx| {
            let i = moved
                .slice_at(ctx.local_x, ctx.local_y)
                .map_or(-1, |i| i as i32);
            if moved.hover.get() != i {
                moved.hover.set(i);
            }
        })
        .on_hover_leave(move |_| {
            if leave.get() != -1 {
                leave.set(-1);
            }
        })
}

/// The slices' labels and colours.
fn legend(slices: &Computed<Vec<PieSlice>>) -> Div {
    let (indices, items) = (slices.clone(), slices.clone());
    div()
        .class("cn-chart-legend")
        .flex_row()
        .flex_wrap()
        .justify_center()
        .for_each(
            computed(move |_: &ReactiveGraph| {
                (0..indices.try_get().map_or(0, |s| s.len())).collect::<Vec<usize>>()
            }),
            |k: &usize| *k,
            move |k: usize| {
                let name = items.clone();
                let own = items
                    .try_get()
                    .and_then(|s| s.get(k).and_then(|s| s.color.clone()));
                div()
                    .class("cn-chart-legend-item")
                    .flex_row()
                    .items_center()
                    .child(mark(own.as_deref(), k))
                    .child(label("cn-chart-legend-name", move |_| {
                        name.try_get()
                            .and_then(|s| s.get(k).map(|s| s.label.clone()))
                            .unwrap_or_default()
                    }))
            },
        )
}

impl ElementBuilder for PieBuilder {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.get_or_build().build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.get_or_build().render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().children_builders()
    }

    fn element_type_id(&self) -> ElementTypeId {
        self.get_or_build().element_type_id()
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        ElementBuilder::layout_style(self.get_or_build())
    }

    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(self.get_or_build())
    }

    fn element_id(&self) -> Option<&str> {
        ElementBuilder::element_id(self.get_or_build())
    }

    fn element_classes(&self) -> &[Arc<str>] {
        self.get_or_build().element_classes()
    }
}

/// A pie chart. See the module docs; `.donut(..)` makes it a donut.
pub fn pie_chart() -> PieBuilder {
    PieBuilder::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slices_share_the_circle_clockwise_from_the_top() {
        let s = [
            PieSlice::new("a", 3.0),
            PieSlice::new("b", 1.0),
            PieSlice::new("c", -2.0),
        ];
        assert_eq!(shares(&s), vec![0.75, 0.25, 0.0]);
        let a = angles(&shares(&s));
        assert!((a[0].0 + FRAC_PI_2).abs() < 1e-6, "starts at the top");
        assert!((a[0].1 - a[0].0 - 0.75 * TAU).abs() < 1e-5);
        assert_eq!(a[1].1, a[2].0);
    }

    #[test]
    fn the_pointer_finds_its_slice() {
        let pie = Pie {
            slices: computed(|_: &ReactiveGraph| {
                vec![PieSlice::new("a", 1.0), PieSlice::new("b", 1.0)]
            }),
            hover: blinc_core::reactive::signal(-1),
            easer: Easer::new(true),
            size: 200.0,
            hole: 0.5,
        };
        // Right of the middle is the first half, left the second.
        assert_eq!(pie.slice_at(160.0, 100.0), Some(0));
        assert_eq!(pie.slice_at(40.0, 100.0), Some(1));
        assert_eq!(pie.slice_at(100.0, 100.0), None, "in the hole");
        assert_eq!(pie.slice_at(199.0, 199.0), None, "past the rim");
    }
}
