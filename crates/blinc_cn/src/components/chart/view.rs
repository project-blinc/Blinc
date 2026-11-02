//! A chart's elements: the canvas, the value and category labels around it,
//! the tooltip that follows the pointer, and the legend; and its builder.

use std::cell::OnceCell;
use std::sync::{Arc, Mutex};

use blinc_core::reactive::{Computed, ReactiveGraph, computed};
use blinc_layout::binding::{IntoReactive, Reactive};
use blinc_layout::canvas::canvas;
use blinc_layout::div::ElementTypeId;
use blinc_layout::element::RenderProps;
use blinc_layout::prelude::*;
use blinc_layout::renderer::{LayoutBoundsCallback, LayoutBoundsStorage};
use blinc_layout::text::Text;
use blinc_layout::text_measure::{TextLayoutOptions, measure_text_with_options};
use blinc_layout::tree::{LayoutNodeId, LayoutTree};

use super::math::compact;
use super::plot::{BOTTOM, Format, Geometry, Options, Plot, TOP, count, palette_class, series_ink};
use super::{ChartCurve, ChartSeries};

/// Builder for a line or area chart; usable inline as an element.
pub struct ChartBuilder {
    opts: Options,
    series: Vec<ChartSeries>,
    data: Option<Reactive<Vec<ChartSeries>>>,
    labels: Option<Reactive<Vec<String>>>,
    legend: Option<bool>,
    width: Option<f32>,
    classes: Vec<String>,
    bounds: LayoutBoundsStorage,
    built: OnceCell<(Div, Arc<Plot>)>,
}

impl ChartBuilder {
    fn new(area: bool) -> Self {
        Self {
            opts: Options {
                area,
                height: 200.0,
                curve: ChartCurve::Smooth,
                dots: false,
                stacked: false,
                grid: true,
                axes: true,
                tooltip: true,
                format: Arc::new(compact),
                min: None,
                max: None,
            },
            series: Vec::new(),
            data: None,
            labels: None,
            legend: None,
            width: None,
            classes: Vec::new(),
            bounds: Arc::new(Mutex::new(None)),
            built: OnceCell::new(),
        }
    }

    /// A series of constant values, in the next palette colour.
    pub fn series(mut self, name: impl Into<String>, values: &[f64]) -> Self {
        self.series.push(ChartSeries::new(name, values.to_vec()));
        self
    }

    /// A series of constant values in its own colour, any CSS colour.
    pub fn series_colored(
        mut self,
        name: impl Into<String>,
        values: &[f64],
        css: impl Into<String>,
    ) -> Self {
        self.series
            .push(ChartSeries::new(name, values.to_vec()).color(css));
        self
    }

    /// All the series at once: a list, Signal or Computed, followed in place.
    /// Replaces any added with [`Self::series`].
    pub fn data(mut self, series: impl IntoReactive<Vec<ChartSeries>>) -> Self {
        self.data = Some(series.into_reactive());
        self
    }

    /// The category labels, one per value: a list, Signal or Computed. They
    /// count from 1 unless set.
    pub fn labels(mut self, labels: impl IntoReactive<Vec<String>>) -> Self {
        self.labels = Some(labels.into_reactive());
        self
    }

    /// The chart's height, axes included; 200 unless set.
    pub fn height(mut self, height: f32) -> Self {
        self.opts.height = height;
        self
    }

    /// A fixed width; the chart fills its parent's width unless set.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    pub fn curve(mut self, curve: ChartCurve) -> Self {
        self.opts.curve = curve;
        self
    }

    /// A dot at every value.
    pub fn dots(mut self) -> Self {
        self.opts.dots = true;
        self
    }

    /// Pile the series on one another.
    pub fn stacked(mut self) -> Self {
        self.opts.stacked = true;
        self
    }

    pub fn no_grid(mut self) -> Self {
        self.opts.grid = false;
        self
    }

    /// Without the value and category labels.
    pub fn no_axes(mut self) -> Self {
        self.opts.axes = false;
        self
    }

    /// Whether the series' names and colours show under the plot; shown for
    /// more than one series unless set.
    pub fn legend(mut self, shown: bool) -> Self {
        self.legend = Some(shown);
        self
    }

    /// Without the cursor and tooltip under the pointer.
    pub fn no_tooltip(mut self) -> Self {
        self.opts.tooltip = false;
        self
    }

    /// How values are written on the axis and in the tooltip; `12.5k` style
    /// unless set.
    pub fn format<F>(mut self, format: F) -> Self
    where
        F: Fn(f64) -> String + Send + Sync + 'static,
    {
        self.opts.format = Arc::new(format);
        self
    }

    /// The value axis's lower bound, in place of the data's.
    pub fn min(mut self, min: f64) -> Self {
        self.opts.min = Some(min);
        self
    }

    /// The value axis's upper bound, in place of the data's.
    pub fn max(mut self, max: f64) -> Self {
        self.opts.max = Some(max);
        self
    }

    /// A class for rules of your own to match.
    pub fn class(mut self, name: impl Into<String>) -> Self {
        self.classes.push(name.into());
        self
    }

    fn get_or_build(&self) -> &(Div, Arc<Plot>) {
        ::blinc_layout::build_once::build_once(&self.built, || self.build_chart())
    }

    fn build_chart(&self) -> (Div, Arc<Plot>) {
        let series = list_of(self.data.clone(), self.series.clone());
        let plot = Plot::new(series.clone(), self.opts.clone());
        let labels = {
            let (source, series) = (self.labels.clone(), series.clone());
            computed(move |g: &ReactiveGraph| match &source {
                Some(Reactive::Const(l)) => l.clone(),
                Some(Reactive::Bound(state)) => g.get(state.signal()).unwrap_or_default(),
                Some(Reactive::Computed(c)) => c.try_get().unwrap_or_default(),
                None => (1..=count(&series.try_get().unwrap_or_default()))
                    .map(|i| i.to_string())
                    .collect(),
            })
        };
        let geometry = {
            let (plot, series) = (Arc::clone(&plot), series.clone());
            computed(move |g: &ReactiveGraph| {
                let width = g.get(plot.width).unwrap_or(0.0);
                Geometry::of(&series.try_get().unwrap_or_default(), &plot.opts, width)
            })
        };

        let canvas = {
            let (draw, wants) = (Arc::clone(&plot), Arc::clone(&plot));
            canvas(move |ctx, bounds| draw.draw(ctx, bounds.width))
                .absolute()
                .left(0.0)
                .top(0.0)
                .w_full()
                .h_full()
                .redraw_while(move || wants.wants_frames())
        };
        let mut area = div()
            .class("cn-chart-plot")
            .relative()
            .w_full()
            .h(self.opts.height)
            .flex_shrink_0()
            .child(canvas);
        if self.opts.axes {
            area = area
                .child(value_axis(&geometry, &self.opts))
                .child(category_axis(&geometry, &labels));
        }
        if self.opts.tooltip {
            area = with_hover(area, &plot, &geometry, &labels);
        }

        let mut root = div()
            .class("cn-chart")
            .flex_col()
            .child(area)
            .child(legend(&series, self.legend));
        root = match self.width {
            Some(w) => root.w(w),
            None => root.w_full(),
        };
        let root = self.classes.iter().fold(root, |d, c| d.class(c));
        (root, plot)
    }
}

/// The series to draw, whatever form they were given in.
fn list_of(
    data: Option<Reactive<Vec<ChartSeries>>>,
    constant: Vec<ChartSeries>,
) -> Computed<Vec<ChartSeries>> {
    match data {
        None => computed(move |_: &ReactiveGraph| constant.clone()),
        Some(Reactive::Const(list)) => computed(move |_: &ReactiveGraph| list.clone()),
        Some(Reactive::Bound(state)) => {
            let signal = state.signal();
            computed(move |g: &ReactiveGraph| g.get(signal).unwrap_or_default())
        }
        Some(Reactive::Computed(c)) => c,
    }
}

/// A label part with bound text.
fn label(class: &str, text: impl Fn(&ReactiveGraph) -> String + Send + Sync + 'static) -> Div {
    div()
        .class(class)
        .child(Text::bound(Reactive::Computed(computed(text))).no_wrap())
}

/// The value axis's ticks beside the plot, top to bottom, each centred on
/// its grid line.
fn value_axis(geometry: &Computed<Geometry>, opts: &Options) -> Div {
    let (ticks, width) = (geometry.clone(), geometry.clone());
    let format = Arc::clone(&opts.format);
    div()
        .class("cn-chart-ticks")
        .absolute()
        .left(0.0)
        .top(TOP - 8.0)
        .h(opts.height - TOP - BOTTOM + 16.0)
        .w(computed(move |_: &ReactiveGraph| {
            width.try_get().map_or(0.0, |g| (g.x0 - 8.0).max(0.0))
        }))
        .flex_col()
        .justify_between()
        .pointer_events_none()
        .for_each(
            computed(move |_: &ReactiveGraph| {
                let mut t = ticks.try_get().map(|g| g.ticks).unwrap_or_default();
                t.reverse();
                t
            }),
            |t: &f64| t.to_bits(),
            move |t: f64| {
                let text = format(t);
                div()
                    .h(16.0)
                    .flex_row()
                    .items_center()
                    .justify_end()
                    .child(label("cn-chart-axis", move |_| text.clone()))
            },
        )
}

/// The category labels under the plot, one per slot, every few when they
/// would crowd.
fn category_axis(geometry: &Computed<Geometry>, labels: &Computed<Vec<String>>) -> Div {
    let (lead, slots) = (geometry.clone(), labels.clone());
    let (geometry, labels) = (geometry.clone(), labels.clone());
    // Show every `every`th label: as many as fit their slots.
    let every = computed(move |_: &ReactiveGraph| {
        let (Some(g), Some(l)) = (geometry.try_get(), labels.try_get()) else {
            return 1usize;
        };
        let options = TextLayoutOptions::new();
        let widest = l
            .iter()
            .map(|s| measure_text_with_options(s, 12.0, &options).width)
            .fold(0.0f32, f32::max);
        ((widest + 8.0) / g.slot(l.len()).max(1.0)).ceil().max(1.0) as usize
    });
    let shown = labels_row(slots, every);
    div()
        .absolute()
        .left(0.0)
        .top(0.0)
        .w_full()
        .h_full()
        .flex_col()
        .justify_end()
        .pointer_events_none()
        .child(
            div()
                .w_full()
                .h(BOTTOM)
                .flex_row()
                .items_start()
                .child(div().flex_shrink_0().w(computed(move |_: &ReactiveGraph| {
                    lead.try_get().map_or(0.0, |g| g.x0)
                })))
                .child(shown)
                .child(div().flex_shrink_0().w(8.0)),
        )
}

fn labels_row(labels: Computed<Vec<String>>, every: Computed<usize>) -> Div {
    let indices = {
        let labels = labels.clone();
        computed(move |_: &ReactiveGraph| {
            (0..labels.try_get().map_or(0, |l| l.len())).collect::<Vec<usize>>()
        })
    };
    div().flex_grow().w(0.0).flex_row().pt(1.5).for_each(
        indices,
        |i: &usize| *i,
        move |i: usize| {
            let (text, every) = (labels.clone(), every.clone());
            let hidden = computed(move |_: &ReactiveGraph| i % every.try_get().unwrap_or(1) != 0);
            div().flex_grow().w(0.0).flex_row().justify_center().child(
                label("cn-chart-axis", move |_| {
                    text.try_get()
                        .and_then(|l| l.get(i).cloned())
                        .unwrap_or_default()
                })
                .attr_when("data-hidden", "", hidden),
            )
        },
    )
}

/// The plot follows the pointer: the category under it is `hover`, and the
/// tooltip shows each series' value there, on whichever side has room.
fn with_hover(
    area: Div,
    plot: &Arc<Plot>,
    geometry: &Computed<Geometry>,
    labels: &Computed<Vec<String>>,
) -> Div {
    let hover = plot.hover;
    let at = {
        let series = plot.series.clone();
        computed(move |g: &ReactiveGraph| {
            let i = g.get(hover).unwrap_or(-1);
            let n = count(&series.try_get().unwrap_or_default());
            (i >= 0 && (i as usize) < n).then_some(i as usize)
        })
    };
    let hovered = {
        let at = at.clone();
        computed(move |_: &ReactiveGraph| at.try_get().flatten().is_some())
    };
    // Where the tooltip goes: right of the point, or left of it past 60% of
    // the width, as spacer widths around it.
    let side = {
        let (at, geometry, plot) = (at.clone(), geometry.clone(), Arc::clone(plot));
        move |g: &ReactiveGraph| -> (f32, bool, f32) {
            let width = g.get(plot.width).unwrap_or(0.0);
            let (Some(Some(i)), Some(geo)) = (at.try_get(), geometry.try_get()) else {
                return (0.0, false, 0.0);
            };
            let n = count(&plot.series.try_get().unwrap_or_default());
            let x = geo.x(i, n);
            (x, x > width * 0.6, width)
        }
    };
    let tip = {
        let (plot, labels, at) = (Arc::clone(plot), labels.clone(), at.clone());
        move || tooltip(&plot, &labels, &at)
    };
    let (lead, grow, trail) = (side.clone(), side.clone(), side);
    let holder = div()
        .absolute()
        .left(0.0)
        .top(TOP + 4.0)
        .w_full()
        .flex_row()
        .items_start()
        .pointer_events_none()
        .show(hovered, move || {
            let (lead, grow, trail) = (lead.clone(), grow.clone(), trail.clone());
            div()
                .w_full()
                .flex_row()
                .items_start()
                .child(
                    div()
                        .flex_shrink_0()
                        .w(computed(move |g: &ReactiveGraph| {
                            let (x, right, _) = lead(g);
                            if right { 0.0 } else { x + 14.0 }
                        }))
                        .flex_grow_value(computed(
                            move |g: &ReactiveGraph| {
                                if grow(g).1 { 1.0 } else { 0.0 }
                            },
                        )),
                )
                .child(tip())
                .child(div().flex_shrink_0().w(computed(move |g: &ReactiveGraph| {
                    let (x, right, width) = trail(g);
                    if right { width - x + 14.0 } else { 0.0 }
                })))
        });

    let (move_plot, leave) = (Arc::clone(plot), plot.hover);
    area.child(holder)
        .on_mouse_move(move |ctx| {
            let series = move_plot.series.try_get().unwrap_or_default();
            let geo = Geometry::of(&series, &move_plot.opts, ctx.bounds_width);
            let i = geo
                .category_at(ctx.local_x, count(&series))
                .map_or(-1, |i| i as i32);
            if move_plot.hover.get() != i {
                move_plot.hover.set(i);
            }
        })
        .on_hover_leave(move |_| {
            if leave.get() != -1 {
                leave.set(-1);
            }
        })
}

/// The label under the pointer and each series' value there.
fn tooltip(plot: &Arc<Plot>, labels: &Computed<Vec<String>>, at: &Computed<Option<usize>>) -> Div {
    let title = {
        let (labels, at) = (labels.clone(), at.clone());
        move |_: &ReactiveGraph| {
            let (Some(Some(i)), Some(l)) = (at.try_get(), labels.try_get()) else {
                return String::new();
            };
            l.get(i).cloned().unwrap_or_default()
        }
    };
    let indices = {
        let series = plot.series.clone();
        computed(move |_: &ReactiveGraph| {
            (0..series.try_get().map_or(0, |s| s.len())).collect::<Vec<usize>>()
        })
    };
    let (series, at, format) = (
        plot.series.clone(),
        at.clone(),
        Arc::clone(&plot.opts.format),
    );
    div()
        .class("cn-chart-tooltip")
        .flex_col()
        .flex_shrink_0()
        .child(label("cn-chart-tooltip-title", title))
        .for_each(
            indices,
            |k: &usize| *k,
            move |k: usize| {
                let (name, value) = (series.clone(), series.clone());
                let (at, format) = (at.clone(), Arc::clone(&format));
                div()
                    .class("cn-chart-tooltip-row")
                    .flex_row()
                    .items_center()
                    .child(swatch(&series.try_get().unwrap_or_default(), k))
                    .child(label("cn-chart-tooltip-name", move |_| {
                        name.try_get()
                            .and_then(|s| s.get(k).map(|s| s.name.clone()))
                            .unwrap_or_default()
                    }))
                    .child(label("cn-chart-tooltip-value", move |_| {
                        let (Some(Some(i)), Some(s)) = (at.try_get(), value.try_get()) else {
                            return String::new();
                        };
                        s.get(k)
                            .and_then(|s| s.values.get(i))
                            .map(|v| format(*v))
                            .unwrap_or_default()
                    }))
            },
        )
}

/// A series' colour mark: its palette class, or its own colour.
fn swatch(series: &[ChartSeries], k: usize) -> Div {
    let mark = div()
        .class("cn-chart-swatch")
        .w(10.0)
        .h(10.0)
        .flex_shrink_0();
    match series.get(k) {
        Some(s) if s.color.is_some() => mark.bg(series_ink(s, k)),
        _ => mark.class(palette_class(k)),
    }
}

/// The series' names and colours, shown for more than one series unless set.
fn legend(series: &Computed<Vec<ChartSeries>>, shown: Option<bool>) -> Div {
    let (hidden, indices, items) = (series.clone(), series.clone(), series.clone());
    div()
        .class("cn-chart-legend")
        .flex_row()
        .flex_wrap()
        .justify_center()
        .attr_when(
            "data-hidden",
            "",
            computed(move |_: &ReactiveGraph| {
                let n = hidden.try_get().map_or(0, |s| s.len());
                !shown.unwrap_or(n > 1)
            }),
        )
        .for_each(
            computed(move |_: &ReactiveGraph| {
                (0..indices.try_get().map_or(0, |s| s.len())).collect::<Vec<usize>>()
            }),
            |k: &usize| *k,
            move |k: usize| {
                let name = items.clone();
                div()
                    .class("cn-chart-legend-item")
                    .flex_row()
                    .items_center()
                    .child(swatch(&items.try_get().unwrap_or_default(), k))
                    .child(label("cn-chart-legend-name", move |_| {
                        name.try_get()
                            .and_then(|s| s.get(k).map(|s| s.name.clone()))
                            .unwrap_or_default()
                    }))
            },
        )
}

impl ElementBuilder for ChartBuilder {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.get_or_build().0.build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.get_or_build().0.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().0.children_builders()
    }

    fn element_type_id(&self) -> ElementTypeId {
        self.get_or_build().0.element_type_id()
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        ElementBuilder::layout_style(&self.get_or_build().0)
    }

    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(&self.get_or_build().0)
    }

    fn element_id(&self) -> Option<&str> {
        ElementBuilder::element_id(&self.get_or_build().0)
    }

    fn element_classes(&self) -> &[Arc<str>] {
        self.get_or_build().0.element_classes()
    }

    // The chart's laid-out width is the plot's, which places the labels and
    // the tooltip.
    fn layout_bounds_storage(&self) -> Option<LayoutBoundsStorage> {
        Some(Arc::clone(&self.bounds))
    }

    fn layout_bounds_callback(&self) -> Option<LayoutBoundsCallback> {
        let width = self.get_or_build().1.width;
        Some(Arc::new(move |bounds| {
            if (width.try_get().unwrap_or(0.0) - bounds.width).abs() > 0.5 {
                width.set(bounds.width);
            }
        }))
    }
}

/// A line chart. See the module docs.
pub fn line_chart() -> ChartBuilder {
    ChartBuilder::new(false)
}

/// A line chart filled down to the axis, or to the series below when
/// stacked.
pub fn area_chart() -> ChartBuilder {
    ChartBuilder::new(true)
}
