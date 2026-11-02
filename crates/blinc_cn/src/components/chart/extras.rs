//! The kinds built on the plot: sparklines, threshold charts, histograms and
//! comparisons, and the notes a comparison writes beside its bars.

use std::cell::OnceCell;
use std::sync::Arc;

use blinc_core::reactive::{Computed, ReactiveGraph, computed};
use blinc_layout::binding::{IntoReactive, Reactive};
use blinc_layout::div::ElementTypeId;
use blinc_layout::element::RenderProps;
use blinc_layout::prelude::*;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};

use super::math::compact;
use super::plot::{BOTTOM, Geometry, Notes, Options, TOP};
use super::view::{ChartBuilder, bar_chart, label, line_chart, source};
use super::{ChartCurve, ChartMarker, ChartSeries};

/// A note per category right of the plot, level with its slot: `.cn-chart-note`
/// with `data-tone`.
pub(super) fn notes(geometry: &Computed<Geometry>, notes: &Notes, opts: &Options) -> Div {
    let (lead, indices, all) = (geometry.clone(), notes.clone(), notes.clone());
    div()
        .absolute()
        .left(0.0)
        .top(0.0)
        .w_full()
        .h_full()
        .flex_row()
        .pointer_events_none()
        .child(div().flex_shrink_0().w(computed(move |_: &ReactiveGraph| {
            lead.try_get().map_or(0.0, |g| g.x1 + 6.0)
        })))
        .child(
            div()
                .flex_grow()
                .flex_col()
                .child(div().flex_shrink_0().h(TOP))
                .child(div().flex_grow().flex_col().for_each(
                    computed(move |_: &ReactiveGraph| {
                        (0..indices.try_get().map_or(0, |n| n.len())).collect::<Vec<usize>>()
                    }),
                    |i: &usize| *i,
                    move |i: usize| {
                        let (text, tone) = (all.clone(), all.clone());
                        let tone_is = move |want: &'static str| {
                            let tone = tone.clone();
                            computed(move |_: &ReactiveGraph| {
                                tone.try_get()
                                    .and_then(|n| n.get(i).cloned().flatten())
                                    .and_then(|(_, t)| t)
                                    == Some(want)
                            })
                        };
                        div().flex_grow().h(0.0).flex_row().items_center().child(
                            label("cn-chart-note", move |_| {
                                text.try_get()
                                    .and_then(|n| n.get(i).cloned().flatten())
                                    .map(|(t, _)| t)
                                    .unwrap_or_default()
                            })
                            .attr_when("data-tone", "bad", tone_is("bad"))
                            .attr_when(
                                "data-tone",
                                "good",
                                tone_is("good"),
                            ),
                        )
                    },
                ))
                .child(
                    div()
                        .flex_shrink_0()
                        .h(if opts.axes { BOTTOM } else { 4.0 }),
                ),
        )
}

/// A small line chart showing a trend beside text: no axes, grid, legend or
/// tooltip, 100 by 24 unless sized. `.filled()` fades it down to its
/// lowest value; `.color(..)` colours it.
pub fn spark_line(values: impl IntoReactive<Vec<f64>>) -> ChartBuilder {
    let values = source(values.into_reactive());
    line_chart()
        .data(computed(move |_: &ReactiveGraph| {
            vec![ChartSeries::new("", values.try_get().unwrap_or_default())]
        }))
        .width(100.0)
        .height(24.0)
        .bare()
}

/// A line of values against bands, such as frame times against a budget:
/// each value's dot takes the colour of the band it falls in. Add bands with
/// `.band(..)` and a dashed target with `.baseline(..)`.
pub fn threshold_line_chart() -> ChartBuilder {
    line_chart()
        .curve(ChartCurve::Linear)
        .dots()
        .legend(false)
        .dots_by_band()
}

impl ChartBuilder {
    /// The values of a single-series chart: a list, Signal or Computed.
    pub fn values(self, values: impl IntoReactive<Vec<f64>>) -> Self {
        let values = source(values.into_reactive());
        self.data(computed(move |_: &ReactiveGraph| {
            vec![ChartSeries::new(
                "Value",
                values.try_get().unwrap_or_default(),
            )]
        }))
    }

    /// A dashed reference line at `value`, such as a target.
    pub fn baseline(self, value: f64) -> Self {
        self.marker(ChartMarker::new(value))
    }

    /// Fill down from the line, fading.
    pub fn filled(self) -> Self {
        self.area()
    }
}

/// Values counted into equal-width bins.
#[derive(Clone, Debug, Default, PartialEq)]
struct Binned {
    counts: Vec<f64>,
    starts: Vec<f64>,
    lo: f64,
    width: f64,
}

fn bin(values: &[f64], bins: usize) -> Binned {
    let bins = bins.max(1);
    let finite: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
    let (lo, hi) = finite
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| {
            (a.min(*v), b.max(*v))
        });
    if !lo.is_finite() {
        return Binned::default();
    }
    let width = if hi > lo {
        (hi - lo) / bins as f64
    } else {
        1.0
    };
    let mut counts = vec![0.0; bins];
    for v in finite {
        let k = (((v - lo) / width) as usize).min(bins - 1);
        counts[k] += 1.0;
    }
    let starts = (0..bins).map(|k| lo + k as f64 * width).collect();
    Binned {
        counts,
        starts,
        lo,
        width,
    }
}

/// Builder for a histogram; usable inline as an element.
pub struct HistogramBuilder {
    values: Reactive<Vec<f64>>,
    bins: usize,
    log: bool,
    markers: Vec<(f64, Option<String>)>,
    color: Option<String>,
    height: f32,
    format: Arc<dyn Fn(f64) -> String + Send + Sync>,
    built: OnceCell<ChartBuilder>,
}

impl HistogramBuilder {
    /// How many equal-width bins; 20 unless set.
    pub fn bins(mut self, bins: usize) -> Self {
        self.bins = bins.max(1);
        self
    }

    /// Counts on a logarithmic axis, to show long tails.
    pub fn log_scale(mut self) -> Self {
        self.log = true;
        self
    }

    /// A dashed line down the histogram at a value, such as a noise floor.
    pub fn marker(mut self, value: f64) -> Self {
        self.markers.push((value, None));
        self
    }

    /// A marker in its own colour, any CSS colour.
    pub fn marker_colored(mut self, value: f64, css: impl Into<String>) -> Self {
        self.markers.push((value, Some(css.into())));
        self
    }

    /// The bars' colour, any CSS colour.
    pub fn color(mut self, css: impl Into<String>) -> Self {
        self.color = Some(css.into());
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    /// How the bins' starts are written; `12.5k` style unless set.
    pub fn format<F>(mut self, format: F) -> Self
    where
        F: Fn(f64) -> String + Send + Sync + 'static,
    {
        self.format = Arc::new(format);
        self
    }

    fn chart(&self) -> &ChartBuilder {
        ::blinc_layout::build_once::build_once(&self.built, || {
            let values = source(self.values.clone());
            let (bins, log) = (self.bins, self.log);
            let binned =
                computed(move |_: &ReactiveGraph| bin(&values.try_get().unwrap_or_default(), bins));
            let (counts, starts, marks) = (binned.clone(), binned.clone(), binned);
            let format = Arc::clone(&self.format);
            let markers = self.markers.clone();
            let mut chart = bar_chart()
                .data(computed(move |_: &ReactiveGraph| {
                    let c = counts.try_get().unwrap_or_default().counts;
                    let c = if log {
                        c.iter().map(|n| (1.0 + n).log10()).collect()
                    } else {
                        c
                    };
                    vec![ChartSeries::new("Count", c)]
                }))
                .labels(computed(move |_: &ReactiveGraph| {
                    let b = starts.try_get().unwrap_or_default();
                    b.starts.iter().map(|s| format(*s)).collect()
                }))
                .category_markers(computed(move |_: &ReactiveGraph| {
                    let b = marks.try_get().unwrap_or_default();
                    let span = b.width * b.counts.len() as f64;
                    markers
                        .iter()
                        .map(|(v, c)| {
                            let at = if span > 0.0 {
                                ((v - b.lo) / span) as f32
                            } else {
                                0.0
                            };
                            (at, c.clone())
                        })
                        .collect()
                }))
                .gap(1.0)
                .legend(false)
                .height(self.height);
            if log {
                chart = chart.format(|v| compact((10f64.powf(v) - 1.0).round()));
            }
            if let Some(css) = &self.color {
                chart = chart.color(css.clone());
            }
            chart
        })
    }
}

/// A histogram: `values` counted into equal-width bins, each labelled by its
/// start. A list, Signal or Computed.
pub fn histogram(values: impl IntoReactive<Vec<f64>>) -> HistogramBuilder {
    HistogramBuilder {
        values: values.into_reactive(),
        bins: 20,
        log: false,
        markers: Vec::new(),
        color: None,
        height: 200.0,
        format: Arc::new(compact),
        built: OnceCell::new(),
    }
}

/// One category of a comparison: a baseline and a current value.
#[derive(Clone, Debug, PartialEq)]
pub struct ComparisonItem {
    pub label: String,
    pub baseline: f64,
    pub current: f64,
}

impl ComparisonItem {
    pub fn new(label: impl Into<String>, baseline: f64, current: f64) -> Self {
        Self {
            label: label.into(),
            baseline,
            current,
        }
    }
}

/// Builder for a comparison chart; usable inline as an element.
pub struct ComparisonBuilder {
    items: Vec<ComparisonItem>,
    data: Option<Reactive<Vec<ComparisonItem>>>,
    threshold: f64,
    height: f32,
    format: Arc<dyn Fn(f64) -> String + Send + Sync>,
    built: OnceCell<ChartBuilder>,
}

impl ComparisonBuilder {
    /// A category of constant values.
    pub fn item(mut self, label: impl Into<String>, baseline: f64, current: f64) -> Self {
        self.items
            .push(ComparisonItem::new(label, baseline, current));
        self
    }

    /// All the categories at once: a list, Signal or Computed.
    pub fn data(mut self, items: impl IntoReactive<Vec<ComparisonItem>>) -> Self {
        self.data = Some(items.into_reactive());
        self
    }

    /// The increase over the baseline, in percent, past which a change is
    /// marked bad; 10 unless set.
    pub fn threshold(mut self, percent: f64) -> Self {
        self.threshold = percent;
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    /// How values are written on the axis and in the tooltip.
    pub fn format<F>(mut self, format: F) -> Self
    where
        F: Fn(f64) -> String + Send + Sync + 'static,
    {
        self.format = Arc::new(format);
        self
    }

    fn chart(&self) -> &ChartBuilder {
        ::blinc_layout::build_once::build_once(&self.built, || {
            let items = match self.data.clone() {
                Some(data) => source(data),
                None => {
                    let constant = self.items.clone();
                    computed(move |_: &ReactiveGraph| constant.clone())
                }
            };
            let (series, labels, notes) = (items.clone(), items.clone(), items);
            let threshold = self.threshold;
            let format = Arc::clone(&self.format);
            bar_chart()
                .horizontal()
                .data(computed(move |_: &ReactiveGraph| {
                    let list = series.try_get().unwrap_or_default();
                    vec![
                        ChartSeries::new(
                            "Baseline",
                            list.iter().map(|i| i.baseline).collect::<Vec<_>>(),
                        )
                        .color("var(--chart-baseline)"),
                        ChartSeries::new(
                            "Current",
                            list.iter().map(|i| i.current).collect::<Vec<_>>(),
                        )
                        .color("var(--chart-2)"),
                    ]
                }))
                .labels(computed(move |_: &ReactiveGraph| {
                    labels
                        .try_get()
                        .unwrap_or_default()
                        .iter()
                        .map(|i| i.label.clone())
                        .collect()
                }))
                .notes(computed(move |_: &ReactiveGraph| {
                    notes
                        .try_get()
                        .unwrap_or_default()
                        .iter()
                        .map(|i| change_note(i, threshold))
                        .collect()
                }))
                .format(move |v| format(v))
                .height(self.height)
        })
    }
}

/// The change from baseline to current, as a signed percentage, and whether
/// it is bad (past `threshold`) or good (a decrease).
fn change_note(item: &ComparisonItem, threshold: f64) -> Option<(String, Option<&'static str>)> {
    if item.baseline == 0.0 {
        return None;
    }
    let change = (item.current - item.baseline) / item.baseline.abs() * 100.0;
    let text = format!(
        "{}{}%",
        if change >= 0.0 { "+" } else { "" },
        (change * 10.0).round() / 10.0
    );
    let tone = if change > threshold {
        Some("bad")
    } else if change < 0.0 {
        Some("good")
    } else {
        None
    };
    Some((text, tone))
}

/// Paired horizontal bars, a baseline and a current value per category, with
/// the change written beside them: red past the threshold, green when it
/// falls.
pub fn comparison_bar_chart() -> ComparisonBuilder {
    ComparisonBuilder {
        items: Vec::new(),
        data: None,
        threshold: 10.0,
        height: 200.0,
        format: Arc::new(compact),
        built: OnceCell::new(),
    }
}

macro_rules! forward_element {
    ($ty:ty) => {
        impl ElementBuilder for $ty {
            fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
                self.chart().build(tree)
            }

            fn render_props(&self) -> RenderProps {
                self.chart().render_props()
            }

            fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
                self.chart().children_builders()
            }

            fn element_type_id(&self) -> ElementTypeId {
                self.chart().element_type_id()
            }

            fn layout_style(&self) -> Option<&taffy::Style> {
                self.chart().layout_style()
            }

            fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
                self.chart().event_handlers()
            }

            fn element_id(&self) -> Option<&str> {
                self.chart().element_id()
            }

            fn element_classes(&self) -> &[std::sync::Arc<str>] {
                self.chart().element_classes()
            }

            fn layout_bounds_storage(&self) -> Option<blinc_layout::renderer::LayoutBoundsStorage> {
                self.chart().layout_bounds_storage()
            }

            fn layout_bounds_callback(
                &self,
            ) -> Option<blinc_layout::renderer::LayoutBoundsCallback> {
                self.chart().layout_bounds_callback()
            }
        }
    };
}

forward_element!(HistogramBuilder);
forward_element!(ComparisonBuilder);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_fall_into_equal_bins() {
        let b = bin(&[0.0, 1.0, 2.0, 9.0, 10.0], 5);
        assert_eq!(b.counts, vec![2.0, 1.0, 0.0, 0.0, 2.0]);
        assert_eq!(b.starts, vec![0.0, 2.0, 4.0, 6.0, 8.0]);
    }

    #[test]
    fn a_change_past_the_threshold_is_bad_and_a_fall_good() {
        let note = |b, c| change_note(&ComparisonItem::new("x", b, c), 10.0);
        assert_eq!(note(10.0, 11.36), Some(("+13.6%".into(), Some("bad"))));
        assert_eq!(note(10.0, 9.37), Some(("-6.3%".into(), Some("good"))));
        assert_eq!(note(10.0, 10.95), Some(("+9.5%".into(), None)));
        assert_eq!(note(0.0, 3.0), None);
    }
}
