//! Charts: data drawn on a canvas, with the axes, legend and tooltip as
//! ordinary elements over and under it.
//!
//! Line and area charts share one plot. Moving the pointer across it marks
//! the category under it and shows each series' value there; data given as a
//! Signal or Computed is followed, the drawing easing from the old values to
//! the new. Series take `--chart-1` to `--chart-5` in turn; the grid, cursor
//! and dot rings take `--cn-chart-grid`, `--cn-chart-cursor` and
//! `--cn-chart-surface`; the parts are `.cn-chart*` rules.
//!
//! ```ignore
//! cn::line_chart()
//!     .series("CPU", &[30.0, 45.0, 40.0, 60.0])
//!     .series("Memory", &[20.0, 25.0, 30.0, 35.0])
//!     .labels(vec!["1".into(), "2".into(), "3".into(), "4".into()])
//!     .dots()
//!
//! cn::area_chart().data(visitors).stacked()
//! ```

mod ease;
mod legacy;
mod math;
mod pie;
mod plot;
mod view;

pub use legacy::*;
pub use pie::{PieBuilder, PieSlice, pie_chart};
pub use view::{ChartBuilder, area_chart, bar_chart, line_chart};

/// A named series with one value per category label.
#[derive(Clone, Debug, PartialEq)]
pub struct ChartSeries {
    pub name: String,
    pub values: Vec<f64>,
    /// Any CSS colour, `var(..)` included; the next palette colour if unset.
    pub color: Option<String>,
}

impl ChartSeries {
    pub fn new(name: impl Into<String>, values: impl Into<Vec<f64>>) -> Self {
        Self {
            name: name.into(),
            values: values.into(),
            color: None,
        }
    }

    /// Its own colour, any CSS colour, in place of the palette's.
    pub fn color(mut self, css: impl Into<String>) -> Self {
        self.color = Some(css.into());
        self
    }
}

/// How a line goes from one value to the next.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChartCurve {
    /// Straight segments.
    Linear,
    /// A smooth curve that never overshoots a value.
    #[default]
    Smooth,
    /// Level, stepping at each value.
    Step,
}
