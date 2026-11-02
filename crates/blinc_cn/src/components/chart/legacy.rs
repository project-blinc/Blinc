//! Chart components for data visualization
//!
//! Simple, composable chart components for displaying time series and categorical data.
//! Designed for dashboards, debuggers, and analytics displays.
//!
//! # Available Charts
//!
//! - `LineChart` - Time series data with multiple series support
//! - `BarChart` - Categorical data comparison
//! - `SparkLine` - Minimal inline chart for quick trends
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! // Simple line chart
//! cn::line_chart()
//!     .width(400.0)
//!     .height(200.0)
//!     .series("CPU", &[0.4, 0.6, 0.5, 0.8, 0.7])
//!     .series("Memory", &[0.3, 0.35, 0.4, 0.45, 0.5])
//!     .build()
//!
//! // Bar chart
//! cn::bar_chart()
//!     .width(300.0)
//!     .height(150.0)
//!     .data(&[("Jan", 100.0), ("Feb", 150.0), ("Mar", 120.0)])
//!     .build()
//!
//! // Inline sparkline
//! cn::spark_line(&[1.0, 2.0, 1.5, 3.0, 2.5]).build()
//! ```

use blinc_core::Color;
use blinc_layout::div::ElementTypeId;
use blinc_layout::element::RenderProps;
use blinc_layout::prelude::*;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_theme::{ColorToken, ThemeState};

/// A data point for charts
#[derive(Clone, Debug)]
pub struct DataPoint {
    /// X value (or label)
    pub x: f64,
    /// Y value
    pub y: f64,
    /// Optional label
    pub label: Option<String>,
}

impl DataPoint {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y, label: None }
    }

    pub fn labeled(x: f64, y: f64, label: impl Into<String>) -> Self {
        Self {
            x,
            y,
            label: Some(label.into()),
        }
    }
}

/// A data series for multi-line charts
#[derive(Clone, Debug)]
pub struct DataSeries {
    /// Series name
    pub name: String,
    /// Data points
    pub data: Vec<f64>,
    /// Series color
    pub color: Color,
}

/// Chart grid configuration
#[derive(Clone, Debug)]
pub struct ChartGrid {
    /// Show horizontal grid lines
    pub horizontal: bool,
    /// Show vertical grid lines
    pub vertical: bool,
    /// Grid line color
    pub color: Color,
    /// Number of horizontal divisions
    pub h_divisions: usize,
    /// Number of vertical divisions
    pub v_divisions: usize,
}

impl Default for ChartGrid {
    fn default() -> Self {
        Self {
            horizontal: true,
            vertical: false,
            color: Color::WHITE.with_alpha(0.1),
            h_divisions: 5,
            v_divisions: 10,
        }
    }
}

// ============================================================================
// LineChart
// ============================================================================

/// Line chart for time series data
pub struct LineChart {
    inner: Div,
}

impl LineChart {
    /// Add a CSS class for selector matching
    pub fn class(mut self, name: impl AsRef<str>) -> Self {
        self.inner = self.inner.class(name);
        self
    }

    /// Set the element ID for CSS selector matching
    pub fn id(mut self, id: &str) -> Self {
        self.inner = self.inner.id(id);
        self
    }
}

impl ElementBuilder for LineChart {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.inner.build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.inner.children_builders()
    }

    fn element_type_id(&self) -> ElementTypeId {
        self.inner.element_type_id()
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.inner.layout_style()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.inner.element_classes()
    }
}

/// Builder for LineChart
pub struct LineChartBuilder {
    width: f32,
    height: f32,
    series: Vec<DataSeries>,
    grid: ChartGrid,
    show_dots: bool,
    stroke_width: f32,
    padding: f32,
}

impl LineChartBuilder {
    pub fn new() -> Self {
        Self {
            width: 300.0,
            height: 150.0,
            series: Vec::new(),
            grid: ChartGrid::default(),
            show_dots: false,
            stroke_width: 2.0,
            padding: 8.0,
        }
    }

    /// Set chart width
    pub fn width(mut self, w: f32) -> Self {
        self.width = w;
        self
    }

    /// Set chart height
    pub fn height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }

    /// Add a data series
    pub fn series(mut self, name: impl Into<String>, data: &[f64]) -> Self {
        let theme = ThemeState::get();
        let colors = [
            theme.color(ColorToken::Primary),
            theme.color(ColorToken::Secondary),
            theme.color(ColorToken::Success),
            theme.color(ColorToken::Warning),
            theme.color(ColorToken::Error),
        ];
        let color = colors[self.series.len() % colors.len()];

        self.series.push(DataSeries {
            name: name.into(),
            data: data.to_vec(),
            color,
        });
        self
    }

    /// Add a data series with custom color
    pub fn series_colored(mut self, name: impl Into<String>, data: &[f64], color: Color) -> Self {
        self.series.push(DataSeries {
            name: name.into(),
            data: data.to_vec(),
            color,
        });
        self
    }

    /// Show data point dots
    pub fn with_dots(mut self) -> Self {
        self.show_dots = true;
        self
    }

    /// Set stroke width
    pub fn stroke_width(mut self, width: f32) -> Self {
        self.stroke_width = width;
        self
    }

    /// Configure grid
    pub fn grid(mut self, grid: ChartGrid) -> Self {
        self.grid = grid;
        self
    }

    /// Disable grid
    pub fn no_grid(mut self) -> Self {
        self.grid.horizontal = false;
        self.grid.vertical = false;
        self
    }

    /// Build the chart
    pub fn build(self) -> LineChart {
        let theme = ThemeState::get();
        let bg = theme.color(ColorToken::Surface);
        let border = theme.color(ColorToken::Border);

        // Calculate bounds
        let (min_val, max_val) = self.calculate_bounds();
        let range = if (max_val - min_val).abs() < f64::EPSILON {
            1.0
        } else {
            max_val - min_val
        };

        let chart_width = self.width - self.padding * 2.0;
        let chart_height = self.height - self.padding * 2.0;

        let mut container = div()
            .w(self.width)
            .h(self.height)
            .bg(bg)
            .border(1.0, border)
            .rounded(4.0)
            .relative()
            .overflow_clip();

        // Add grid lines
        if self.grid.horizontal {
            for i in 0..=self.grid.h_divisions {
                let y = self.padding + (i as f32 / self.grid.h_divisions as f32) * chart_height;
                container = container.child(
                    div()
                        .absolute()
                        .left(self.padding)
                        .top(y)
                        .w(chart_width)
                        .h(1.0)
                        .bg(self.grid.color),
                );
            }
        }

        // Add series lines using SVG
        for series in &self.series {
            if series.data.is_empty() {
                continue;
            }

            let points: Vec<(f32, f32)> = series
                .data
                .iter()
                .enumerate()
                .map(|(i, &val)| {
                    let x = self.padding
                        + if series.data.len() > 1 {
                            (i as f32 / (series.data.len() - 1) as f32) * chart_width
                        } else {
                            chart_width / 2.0
                        };
                    let y = self.padding + ((max_val - val) / range) as f32 * chart_height;
                    (x, y)
                })
                .collect();

            // Create line path
            if !points.is_empty() {
                let mut path_data = format!("M {} {}", points[0].0, points[0].1);
                for (x, y) in points.iter().skip(1) {
                    path_data.push_str(&format!(" L {} {}", x, y));
                }

                let svg_str = format!(
                    r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {} {}"><path d="{}" fill="none" stroke="currentColor" stroke-width="{}" stroke-linecap="round" stroke-linejoin="round"/></svg>"#,
                    self.width, self.height, path_data, self.stroke_width
                );

                container = container.child(
                    div()
                        .absolute()
                        .left(0.0)
                        .top(0.0)
                        .w(self.width)
                        .h(self.height)
                        .child(
                            svg(&svg_str)
                                .size(self.width, self.height)
                                .color(series.color),
                        ),
                );
            }

            // Add dots if enabled
            if self.show_dots {
                for (x, y) in &points {
                    container = container.child(
                        div()
                            .absolute()
                            .left(x - 3.0)
                            .top(y - 3.0)
                            .w(6.0)
                            .h(6.0)
                            .rounded_full()
                            .bg(series.color),
                    );
                }
            }
        }

        LineChart { inner: container }
    }

    fn calculate_bounds(&self) -> (f64, f64) {
        let mut min = f64::MAX;
        let mut max = f64::MIN;

        for series in &self.series {
            for &val in &series.data {
                min = min.min(val);
                max = max.max(val);
            }
        }

        if min == f64::MAX {
            min = 0.0;
        }
        if max == f64::MIN {
            max = 1.0;
        }

        // Add some padding
        let range = max - min;
        (min - range * 0.05, max + range * 0.05)
    }
}

impl Default for LineChartBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// BarChart
// ============================================================================

/// Bar chart for categorical data
pub struct BarChart {
    inner: Div,
}

impl BarChart {
    /// Add a CSS class for selector matching
    pub fn class(mut self, name: impl AsRef<str>) -> Self {
        self.inner = self.inner.class(name);
        self
    }

    /// Set the element ID for CSS selector matching
    pub fn id(mut self, id: &str) -> Self {
        self.inner = self.inner.id(id);
        self
    }
}

impl ElementBuilder for BarChart {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.inner.build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.inner.children_builders()
    }

    fn element_type_id(&self) -> ElementTypeId {
        self.inner.element_type_id()
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.inner.layout_style()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.inner.element_classes()
    }
}

/// Builder for BarChart
pub struct BarChartBuilder {
    width: f32,
    height: f32,
    data: Vec<(String, f64)>,
    color: Option<Color>,
    bar_gap: f32,
    show_labels: bool,
    horizontal: bool,
}

impl BarChartBuilder {
    pub fn new() -> Self {
        Self {
            width: 300.0,
            height: 150.0,
            data: Vec::new(),
            color: None,
            bar_gap: 4.0,
            show_labels: true,
            horizontal: false,
        }
    }

    /// Set chart width
    pub fn width(mut self, w: f32) -> Self {
        self.width = w;
        self
    }

    /// Set chart height
    pub fn height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }

    /// Set chart data
    pub fn data(mut self, data: &[(&str, f64)]) -> Self {
        self.data = data
            .iter()
            .map(|(label, val)| (label.to_string(), *val))
            .collect();
        self
    }

    /// Set bar color
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    /// Set gap between bars
    pub fn gap(mut self, gap: f32) -> Self {
        self.bar_gap = gap;
        self
    }

    /// Hide labels
    pub fn no_labels(mut self) -> Self {
        self.show_labels = false;
        self
    }

    /// Make horizontal bar chart
    pub fn horizontal(mut self) -> Self {
        self.horizontal = true;
        self
    }

    /// Build the chart
    pub fn build(self) -> BarChart {
        let theme = ThemeState::get();
        let bg = theme.color(ColorToken::Surface);
        let border = theme.color(ColorToken::Border);
        let bar_color = self
            .color
            .unwrap_or_else(|| theme.color(ColorToken::Primary));
        let text_color = theme.color(ColorToken::TextSecondary);

        let max_val = self
            .data
            .iter()
            .map(|(_, v)| *v)
            .fold(f64::MIN, |a, b| a.max(b))
            .max(0.001);

        let padding = 8.0;
        let label_height = if self.show_labels { 20.0 } else { 0.0 };

        let mut container = div()
            .w(self.width)
            .h(self.height)
            .bg(bg)
            .border(1.0, border)
            .rounded(4.0)
            .relative()
            .overflow_clip();

        if !self.horizontal {
            // Vertical bars
            let available_width = self.width - padding * 2.0;
            let bar_width = if !self.data.is_empty() {
                (available_width - self.bar_gap * (self.data.len() - 1) as f32)
                    / self.data.len() as f32
            } else {
                available_width
            };
            let chart_height = self.height - padding * 2.0 - label_height;

            for (i, (label, val)) in self.data.iter().enumerate() {
                let bar_height = (val / max_val) as f32 * chart_height;
                let x = padding + (bar_width + self.bar_gap) * i as f32;
                let y = padding + chart_height - bar_height;

                // Bar
                container = container.child(
                    div()
                        .absolute()
                        .left(x)
                        .top(y)
                        .w(bar_width)
                        .h(bar_height)
                        .bg(bar_color)
                        .rounded(2.0),
                );

                // Label
                if self.show_labels {
                    container = container.child(
                        div()
                            .absolute()
                            .left(x)
                            .top(self.height - padding - label_height)
                            .w(bar_width)
                            .h(label_height)
                            .items_center()
                            .justify_center()
                            .child(text(label).size(10.0).color(text_color)),
                    );
                }
            }
        } else {
            // Horizontal bars
            let available_height = self.height - padding * 2.0;
            let bar_height = if !self.data.is_empty() {
                (available_height - self.bar_gap * (self.data.len() - 1) as f32)
                    / self.data.len() as f32
            } else {
                available_height
            };
            let label_width = 60.0;
            let chart_width = self.width - padding * 2.0 - label_width;

            for (i, (label, val)) in self.data.iter().enumerate() {
                let bar_width = (val / max_val) as f32 * chart_width;
                let y = padding + (bar_height + self.bar_gap) * i as f32;

                // Label
                if self.show_labels {
                    container = container.child(
                        div()
                            .absolute()
                            .left(padding)
                            .top(y)
                            .w(label_width - 4.0)
                            .h(bar_height)
                            .items_center()
                            .justify_end()
                            .child(text(label).size(10.0).color(text_color)),
                    );
                }

                // Bar
                container = container.child(
                    div()
                        .absolute()
                        .left(padding + if self.show_labels { label_width } else { 0.0 })
                        .top(y)
                        .w(bar_width)
                        .h(bar_height)
                        .bg(bar_color)
                        .rounded(2.0),
                );
            }
        }

        BarChart { inner: container }
    }
}

impl Default for BarChartBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// SparkLine
// ============================================================================

/// Minimal inline chart for showing trends
pub struct SparkLine {
    inner: Div,
}

impl SparkLine {
    /// Add a CSS class for selector matching
    pub fn class(mut self, name: impl AsRef<str>) -> Self {
        self.inner = self.inner.class(name);
        self
    }

    /// Set the element ID for CSS selector matching
    pub fn id(mut self, id: &str) -> Self {
        self.inner = self.inner.id(id);
        self
    }
}

impl ElementBuilder for SparkLine {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.inner.build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.inner.children_builders()
    }

    fn element_type_id(&self) -> ElementTypeId {
        self.inner.element_type_id()
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.inner.layout_style()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.inner.element_classes()
    }
}

/// Builder for SparkLine
pub struct SparkLineBuilder {
    data: Vec<f64>,
    width: f32,
    height: f32,
    color: Option<Color>,
    stroke_width: f32,
    fill: bool,
}

impl SparkLineBuilder {
    pub fn new(data: &[f64]) -> Self {
        Self {
            data: data.to_vec(),
            width: 80.0,
            height: 24.0,
            color: None,
            stroke_width: 1.5,
            fill: false,
        }
    }

    /// Set sparkline width
    pub fn width(mut self, w: f32) -> Self {
        self.width = w;
        self
    }

    /// Set sparkline height
    pub fn height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }

    /// Set line color
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    /// Set stroke width
    pub fn stroke_width(mut self, width: f32) -> Self {
        self.stroke_width = width;
        self
    }

    /// Fill area under the line
    pub fn filled(mut self) -> Self {
        self.fill = true;
        self
    }

    /// Build the sparkline
    pub fn build(self) -> SparkLine {
        let theme = ThemeState::get();
        let line_color = self
            .color
            .unwrap_or_else(|| theme.color(ColorToken::Primary));

        if self.data.is_empty() {
            return SparkLine {
                inner: div().w(self.width).h(self.height),
            };
        }

        let min = self.data.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = self.data.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let range = if (max - min).abs() < f64::EPSILON {
            1.0
        } else {
            max - min
        };

        let points: Vec<(f32, f32)> = self
            .data
            .iter()
            .enumerate()
            .map(|(i, &val)| {
                let x = if self.data.len() > 1 {
                    (i as f32 / (self.data.len() - 1) as f32) * self.width
                } else {
                    self.width / 2.0
                };
                let y = ((max - val) / range) as f32 * self.height;
                (x, y.clamp(0.0, self.height))
            })
            .collect();

        let mut path_data = format!("M {} {}", points[0].0, points[0].1);
        for (x, y) in points.iter().skip(1) {
            path_data.push_str(&format!(" L {} {}", x, y));
        }

        let fill_attr = if self.fill {
            // Close path for fill
            let fill_path = format!(
                "{} L {} {} L {} {} Z",
                path_data, self.width, self.height, 0.0, self.height
            );
            format!(
                r#"<path d="{}" fill="currentColor" fill-opacity="0.2"/>"#,
                fill_path
            )
        } else {
            String::new()
        };

        let svg_str = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {} {}">{}<path d="{}" fill="none" stroke="currentColor" stroke-width="{}" stroke-linecap="round" stroke-linejoin="round"/></svg>"#,
            self.width, self.height, fill_attr, path_data, self.stroke_width
        );

        let inner = div().w(self.width).h(self.height).child(
            svg(&svg_str)
                .size(self.width, self.height)
                .color(line_color),
        );

        SparkLine { inner }
    }
}

// ============================================================================
// ThresholdLineChart (for regression detection)
// ============================================================================

/// A threshold band for visual regression detection
#[derive(Clone, Debug)]
pub struct ThresholdBand {
    /// Lower bound of the band
    pub min: f64,
    /// Upper bound of the band
    pub max: f64,
    /// Band color
    pub color: Color,
    /// Optional label
    pub label: Option<String>,
}

impl ThresholdBand {
    pub fn new(min: f64, max: f64, color: Color) -> Self {
        Self {
            min,
            max,
            color,
            label: None,
        }
    }

    pub fn labeled(min: f64, max: f64, color: Color, label: impl Into<String>) -> Self {
        Self {
            min,
            max,
            color,
            label: Some(label.into()),
        }
    }
}

/// Line chart with threshold bands for regression detection
pub struct ThresholdLineChart {
    inner: Div,
}

impl ThresholdLineChart {
    /// Add a CSS class for selector matching
    pub fn class(mut self, name: impl AsRef<str>) -> Self {
        self.inner = self.inner.class(name);
        self
    }

    /// Set the element ID for CSS selector matching
    pub fn id(mut self, id: &str) -> Self {
        self.inner = self.inner.id(id);
        self
    }
}

impl ElementBuilder for ThresholdLineChart {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.inner.build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.inner.children_builders()
    }

    fn element_type_id(&self) -> ElementTypeId {
        self.inner.element_type_id()
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.inner.layout_style()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.inner.element_classes()
    }
}

/// Builder for ThresholdLineChart
pub struct ThresholdLineChartBuilder {
    width: f32,
    height: f32,
    data: Vec<f64>,
    bands: Vec<ThresholdBand>,
    line_color: Option<Color>,
    stroke_width: f32,
    padding: f32,
    show_current_marker: bool,
    baseline: Option<f64>,
}

impl ThresholdLineChartBuilder {
    pub fn new() -> Self {
        Self {
            width: 400.0,
            height: 150.0,
            data: Vec::new(),
            bands: Vec::new(),
            line_color: None,
            stroke_width: 2.0,
            padding: 12.0,
            show_current_marker: true,
            baseline: None,
        }
    }

    /// Set chart width
    pub fn width(mut self, w: f32) -> Self {
        self.width = w;
        self
    }

    /// Set chart height
    pub fn height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }

    /// Set the data series
    pub fn data(mut self, data: &[f64]) -> Self {
        self.data = data.to_vec();
        self
    }

    /// Add a threshold band (e.g., acceptable range)
    pub fn threshold_band(mut self, min: f64, max: f64, color: Color) -> Self {
        self.bands.push(ThresholdBand::new(min, max, color));
        self
    }

    /// Add a labeled threshold band
    pub fn threshold_band_labeled(
        mut self,
        min: f64,
        max: f64,
        color: Color,
        label: impl Into<String>,
    ) -> Self {
        self.bands
            .push(ThresholdBand::labeled(min, max, color, label));
        self
    }

    /// Convenience: Add "good/warning/critical" bands for metrics
    /// e.g., for frame time: good < 16.67ms, warning < 33.33ms, critical > 33.33ms
    pub fn regression_bands(mut self, good_max: f64, warning_max: f64) -> Self {
        let good = Color::from_hex(0x22C55E).with_alpha(0.15); // green
        let warning = Color::from_hex(0xFBBF24).with_alpha(0.15); // yellow
        let critical = Color::from_hex(0xEF4444).with_alpha(0.15); // red

        self.bands
            .push(ThresholdBand::labeled(0.0, good_max, good, "Good"));
        self.bands.push(ThresholdBand::labeled(
            good_max,
            warning_max,
            warning,
            "Warning",
        ));
        self.bands.push(ThresholdBand::labeled(
            warning_max,
            f64::MAX,
            critical,
            "Critical",
        ));
        self
    }

    /// Set line color
    pub fn line_color(mut self, color: Color) -> Self {
        self.line_color = Some(color);
        self
    }

    /// Set stroke width
    pub fn stroke_width(mut self, width: f32) -> Self {
        self.stroke_width = width;
        self
    }

    /// Set a baseline value to show as a dashed line
    pub fn baseline(mut self, value: f64) -> Self {
        self.baseline = Some(value);
        self
    }

    /// Hide the current value marker
    pub fn no_marker(mut self) -> Self {
        self.show_current_marker = false;
        self
    }

    /// Build the chart
    pub fn build(self) -> ThresholdLineChart {
        let theme = ThemeState::get();
        let bg = theme.color(ColorToken::Surface);
        let border = theme.color(ColorToken::Border);
        let line_color = self
            .line_color
            .unwrap_or_else(|| theme.color(ColorToken::TextPrimary));
        let text_color = theme.color(ColorToken::TextSecondary);

        let chart_width = self.width - self.padding * 2.0;
        let chart_height = self.height - self.padding * 2.0;

        // Calculate bounds including bands
        let (mut min_val, mut max_val) = self.calculate_data_bounds();
        for band in &self.bands {
            if band.min.is_finite() {
                min_val = min_val.min(band.min);
            }
            if band.max.is_finite() && band.max < 1e10 {
                max_val = max_val.max(band.max);
            }
        }
        if let Some(bl) = self.baseline {
            min_val = min_val.min(bl);
            max_val = max_val.max(bl);
        }

        let range = if (max_val - min_val).abs() < f64::EPSILON {
            1.0
        } else {
            max_val - min_val
        };

        let mut container = div()
            .w(self.width)
            .h(self.height)
            .bg(bg)
            .border(1.0, border)
            .rounded(4.0)
            .relative()
            .overflow_clip();

        // Draw threshold bands
        for band in &self.bands {
            let band_min = band.min.max(min_val);
            let band_max = band.max.min(max_val);

            if band_max <= band_min {
                continue;
            }

            let y_top = self.padding + ((max_val - band_max) / range) as f32 * chart_height;
            let y_bottom = self.padding + ((max_val - band_min) / range) as f32 * chart_height;
            let band_height = y_bottom - y_top;

            container = container.child(
                div()
                    .absolute()
                    .left(self.padding)
                    .top(y_top)
                    .w(chart_width)
                    .h(band_height)
                    .bg(band.color),
            );

            // Band label on the right
            if let Some(ref label) = band.label {
                if band_height > 14.0 {
                    container = container.child(
                        div()
                            .absolute()
                            .right(self.padding + 4.0)
                            .top(y_top + 2.0)
                            .child(text(label).size(9.0).color(band.color.with_alpha(0.8))),
                    );
                }
            }
        }

        // Draw baseline if set
        if let Some(bl) = self.baseline {
            let y = self.padding + ((max_val - bl) / range) as f32 * chart_height;
            // Dashed line effect using multiple small segments
            let segment_width: f32 = 6.0;
            let gap_width: f32 = 4.0;
            let mut x = self.padding;
            while x < self.padding + chart_width {
                let seg_w = segment_width.min(self.padding + chart_width - x);
                container = container.child(
                    div()
                        .absolute()
                        .left(x)
                        .top(y)
                        .w(seg_w)
                        .h(1.0)
                        .bg(text_color.with_alpha(0.5)),
                );
                x += segment_width + gap_width;
            }
        }

        // Draw the data line
        if !self.data.is_empty() {
            let points: Vec<(f32, f32)> = self
                .data
                .iter()
                .enumerate()
                .map(|(i, &val)| {
                    let x = self.padding
                        + if self.data.len() > 1 {
                            (i as f32 / (self.data.len() - 1) as f32) * chart_width
                        } else {
                            chart_width / 2.0
                        };
                    let y = self.padding + ((max_val - val) / range) as f32 * chart_height;
                    (x, y.clamp(self.padding, self.padding + chart_height))
                })
                .collect();

            if !points.is_empty() {
                let mut path_data = format!("M {} {}", points[0].0, points[0].1);
                for (x, y) in points.iter().skip(1) {
                    path_data.push_str(&format!(" L {} {}", x, y));
                }

                let svg_str = format!(
                    r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {} {}"><path d="{}" fill="none" stroke="currentColor" stroke-width="{}" stroke-linecap="round" stroke-linejoin="round"/></svg>"#,
                    self.width, self.height, path_data, self.stroke_width
                );

                container = container.child(
                    div()
                        .absolute()
                        .left(0.0)
                        .top(0.0)
                        .w(self.width)
                        .h(self.height)
                        .child(
                            svg(&svg_str)
                                .size(self.width, self.height)
                                .color(line_color),
                        ),
                );

                // Current value marker (last point)
                if self.show_current_marker && !points.is_empty() {
                    let (last_x, last_y) = points[points.len() - 1];
                    let last_val = self.data[self.data.len() - 1];

                    // Determine marker color based on which band it falls in
                    let marker_color = self
                        .bands
                        .iter()
                        .find(|b| last_val >= b.min && last_val < b.max)
                        .map(|b| b.color.with_alpha(1.0))
                        .unwrap_or(line_color);

                    // Outer ring
                    container = container.child(
                        div()
                            .absolute()
                            .left(last_x - 6.0)
                            .top(last_y - 6.0)
                            .w(12.0)
                            .h(12.0)
                            .rounded_full()
                            .bg(marker_color.with_alpha(0.3)),
                    );
                    // Inner dot
                    container = container.child(
                        div()
                            .absolute()
                            .left(last_x - 4.0)
                            .top(last_y - 4.0)
                            .w(8.0)
                            .h(8.0)
                            .rounded_full()
                            .bg(marker_color),
                    );
                }
            }
        }

        ThresholdLineChart { inner: container }
    }

    fn calculate_data_bounds(&self) -> (f64, f64) {
        if self.data.is_empty() {
            return (0.0, 1.0);
        }
        let min = self.data.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = self.data.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let range = max - min;
        (min - range * 0.05, max + range * 0.05)
    }
}

impl Default for ThresholdLineChartBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Histogram (for diff distribution analysis)
// ============================================================================

/// Histogram chart for distribution visualization
pub struct Histogram {
    inner: Div,
}

impl Histogram {
    /// Add a CSS class for selector matching
    pub fn class(mut self, name: impl AsRef<str>) -> Self {
        self.inner = self.inner.class(name);
        self
    }

    /// Set the element ID for CSS selector matching
    pub fn id(mut self, id: &str) -> Self {
        self.inner = self.inner.id(id);
        self
    }
}

impl ElementBuilder for Histogram {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.inner.build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.inner.children_builders()
    }

    fn element_type_id(&self) -> ElementTypeId {
        self.inner.element_type_id()
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.inner.layout_style()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.inner.element_classes()
    }
}

/// Builder for Histogram
pub struct HistogramBuilder {
    width: f32,
    height: f32,
    data: Vec<f64>,
    bins: usize,
    color: Option<Color>,
    threshold_lines: Vec<(f64, Color, String)>,
    show_axis: bool,
    log_scale: bool,
}

impl HistogramBuilder {
    pub fn new(data: &[f64]) -> Self {
        Self {
            width: 300.0,
            height: 120.0,
            data: data.to_vec(),
            bins: 30,
            color: None,
            threshold_lines: Vec::new(),
            show_axis: true,
            log_scale: false,
        }
    }

    /// Set chart width
    pub fn width(mut self, w: f32) -> Self {
        self.width = w;
        self
    }

    /// Set chart height
    pub fn height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }

    /// Set number of bins
    pub fn bins(mut self, n: usize) -> Self {
        self.bins = n.max(1);
        self
    }

    /// Set bar color
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    /// Add a threshold line (e.g., noise floor)
    pub fn threshold_line(mut self, value: f64, label: impl Into<String>) -> Self {
        let color = Color::from_hex(0xEF4444); // red
        self.threshold_lines.push((value, color, label.into()));
        self
    }

    /// Add a threshold line with custom color
    pub fn threshold_line_colored(
        mut self,
        value: f64,
        color: Color,
        label: impl Into<String>,
    ) -> Self {
        self.threshold_lines.push((value, color, label.into()));
        self
    }

    /// Use logarithmic scale for Y axis (useful for long-tail distributions)
    pub fn log_scale(mut self) -> Self {
        self.log_scale = true;
        self
    }

    /// Hide axis labels
    pub fn no_axis(mut self) -> Self {
        self.show_axis = false;
        self
    }

    /// Build the histogram
    pub fn build(self) -> Histogram {
        let theme = ThemeState::get();
        let bg = theme.color(ColorToken::Surface);
        let border = theme.color(ColorToken::Border);
        let bar_color = self
            .color
            .unwrap_or_else(|| theme.color(ColorToken::Primary));
        let text_color = theme.color(ColorToken::TextSecondary);

        let padding = 8.0;
        let axis_height = if self.show_axis { 16.0 } else { 0.0 };
        let chart_width = self.width - padding * 2.0;
        let chart_height = self.height - padding * 2.0 - axis_height;

        let mut container = div()
            .w(self.width)
            .h(self.height)
            .bg(bg)
            .border(1.0, border)
            .rounded(4.0)
            .relative()
            .overflow_clip();

        if self.data.is_empty() {
            return Histogram { inner: container };
        }

        // Calculate histogram bins
        let data_min = self.data.iter().cloned().fold(f64::INFINITY, f64::min);
        let data_max = self.data.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let data_range = if (data_max - data_min).abs() < f64::EPSILON {
            1.0
        } else {
            data_max - data_min
        };

        let bin_width = data_range / self.bins as f64;
        let mut bin_counts: Vec<usize> = vec![0; self.bins];

        for &val in &self.data {
            let bin_idx = ((val - data_min) / bin_width) as usize;
            let bin_idx = bin_idx.min(self.bins - 1);
            bin_counts[bin_idx] += 1;
        }

        // Find max count for scaling
        let max_count = *bin_counts.iter().max().unwrap_or(&1) as f64;
        let max_count = if self.log_scale {
            (max_count + 1.0).ln()
        } else {
            max_count
        };

        let bar_w = chart_width / self.bins as f32;
        let bar_gap = 1.0_f32;

        // Draw bars
        for (i, &count) in bin_counts.iter().enumerate() {
            let scaled_count = if self.log_scale {
                (count as f64 + 1.0).ln()
            } else {
                count as f64
            };
            let bar_height = (scaled_count / max_count) as f32 * chart_height;
            let x = padding + i as f32 * bar_w;
            let y = padding + chart_height - bar_height;

            container = container.child(
                div()
                    .absolute()
                    .left(x + bar_gap / 2.0)
                    .top(y)
                    .w((bar_w - bar_gap).max(1.0))
                    .h(bar_height.max(0.0))
                    .bg(bar_color),
            );
        }

        // Draw threshold lines
        for (value, color, label) in &self.threshold_lines {
            if *value >= data_min && *value <= data_max {
                let x = padding + ((value - data_min) / data_range) as f32 * chart_width;

                // Vertical line
                container = container.child(
                    div()
                        .absolute()
                        .left(x)
                        .top(padding)
                        .w(2.0)
                        .h(chart_height)
                        .bg(*color),
                );

                // Label at bottom
                if self.show_axis {
                    container = container.child(
                        div()
                            .absolute()
                            .left(x + 4.0)
                            .top(self.height - padding - axis_height + 2.0)
                            .child(text(label).size(9.0).color(*color)),
                    );
                }
            }
        }

        // X-axis labels
        if self.show_axis {
            // Min value
            container = container.child(
                div()
                    .absolute()
                    .left(padding)
                    .top(self.height - padding - axis_height + 2.0)
                    .child(text(format!("{:.1}", data_min)).size(9.0).color(text_color)),
            );
            // Max value
            container = container.child(
                div()
                    .absolute()
                    .right(padding)
                    .top(self.height - padding - axis_height + 2.0)
                    .child(text(format!("{:.1}", data_max)).size(9.0).color(text_color)),
            );
        }

        Histogram { inner: container }
    }
}

// ============================================================================
// ComparisonBarChart (side-by-side baseline vs current)
// ============================================================================

/// Bar chart for before/after comparison
pub struct ComparisonBarChart {
    inner: Div,
}

impl ComparisonBarChart {
    /// Add a CSS class for selector matching
    pub fn class(mut self, name: impl AsRef<str>) -> Self {
        self.inner = self.inner.class(name);
        self
    }

    /// Set the element ID for CSS selector matching
    pub fn id(mut self, id: &str) -> Self {
        self.inner = self.inner.id(id);
        self
    }
}

impl ElementBuilder for ComparisonBarChart {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.inner.build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.inner.children_builders()
    }

    fn element_type_id(&self) -> ElementTypeId {
        self.inner.element_type_id()
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.inner.layout_style()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.inner.element_classes()
    }
}

/// Builder for ComparisonBarChart
pub struct ComparisonBarChartBuilder {
    width: f32,
    height: f32,
    data: Vec<(String, f64, f64)>, // (label, baseline, current)
    baseline_color: Option<Color>,
    current_color: Option<Color>,
    threshold_pct: f64, // Percentage change considered regression
}

impl ComparisonBarChartBuilder {
    pub fn new() -> Self {
        Self {
            width: 400.0,
            height: 200.0,
            data: Vec::new(),
            baseline_color: None,
            current_color: None,
            threshold_pct: 10.0, // 10% change triggers warning
        }
    }

    /// Set chart width
    pub fn width(mut self, w: f32) -> Self {
        self.width = w;
        self
    }

    /// Set chart height
    pub fn height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }

    /// Add a comparison data point
    pub fn item(mut self, label: impl Into<String>, baseline: f64, current: f64) -> Self {
        self.data.push((label.into(), baseline, current));
        self
    }

    /// Add multiple comparison items
    pub fn items(mut self, items: &[(&str, f64, f64)]) -> Self {
        for (label, baseline, current) in items {
            self.data.push((label.to_string(), *baseline, *current));
        }
        self
    }

    /// Set threshold percentage for regression warning (default 10%)
    pub fn threshold(mut self, pct: f64) -> Self {
        self.threshold_pct = pct;
        self
    }

    /// Set baseline bar color
    pub fn baseline_color(mut self, color: Color) -> Self {
        self.baseline_color = Some(color);
        self
    }

    /// Set current bar color
    pub fn current_color(mut self, color: Color) -> Self {
        self.current_color = Some(color);
        self
    }

    /// Build the chart
    pub fn build(self) -> ComparisonBarChart {
        let theme = ThemeState::get();
        let bg = theme.color(ColorToken::Surface);
        let border = theme.color(ColorToken::Border);
        let baseline_color = self
            .baseline_color
            .unwrap_or_else(|| theme.color(ColorToken::TextTertiary));
        let text_color = theme.color(ColorToken::TextSecondary);

        let good_color = Color::from_hex(0x22C55E); // green
        let warning_color = Color::from_hex(0xFBBF24); // yellow
        let critical_color = Color::from_hex(0xEF4444); // red

        let padding = 12.0;
        let label_width = 80.0;
        let legend_height = 24.0;
        let chart_width = self.width - padding * 2.0 - label_width;
        let chart_height = self.height - padding * 2.0 - legend_height;

        let mut container = div()
            .w(self.width)
            .h(self.height)
            .bg(bg)
            .border(1.0, border)
            .rounded(4.0)
            .relative()
            .overflow_clip();

        if self.data.is_empty() {
            return ComparisonBarChart { inner: container };
        }

        // Find max value for scaling
        let max_val = self
            .data
            .iter()
            .flat_map(|(_, b, c)| [*b, *c])
            .fold(f64::MIN, f64::max)
            .max(0.001);

        let row_height = chart_height / self.data.len() as f32;
        let bar_height = (row_height - 8.0) / 2.0;

        for (i, (label, baseline, current)) in self.data.iter().enumerate() {
            let y_base = padding + i as f32 * row_height;

            // Label
            container = container.child(
                div()
                    .absolute()
                    .left(padding)
                    .top(y_base + row_height / 2.0 - 8.0)
                    .w(label_width - 8.0)
                    .child(text(label).size(11.0).color(text_color)),
            );

            // Baseline bar
            let baseline_w = (baseline / max_val) as f32 * chart_width;
            container = container.child(
                div()
                    .absolute()
                    .left(padding + label_width)
                    .top(y_base + 2.0)
                    .w(baseline_w)
                    .h(bar_height)
                    .bg(baseline_color)
                    .rounded(2.0),
            );

            // Current bar with regression color coding
            let pct_change = if *baseline > 0.0 {
                ((current - baseline) / baseline) * 100.0
            } else {
                0.0
            };

            let current_bar_color = if pct_change <= -self.threshold_pct {
                good_color // improved
            } else if pct_change >= self.threshold_pct * 2.0 {
                critical_color // severe regression
            } else if pct_change >= self.threshold_pct {
                warning_color // mild regression
            } else {
                self.current_color
                    .unwrap_or_else(|| theme.color(ColorToken::Primary))
            };

            let current_w = (current / max_val) as f32 * chart_width;
            container = container.child(
                div()
                    .absolute()
                    .left(padding + label_width)
                    .top(y_base + bar_height + 4.0)
                    .w(current_w)
                    .h(bar_height)
                    .bg(current_bar_color)
                    .rounded(2.0),
            );

            // Change indicator
            if pct_change.abs() >= 1.0 {
                let indicator = if pct_change > 0.0 {
                    format!("+{:.0}%", pct_change)
                } else {
                    format!("{:.0}%", pct_change)
                };
                let indicator_color = if pct_change <= -self.threshold_pct {
                    good_color
                } else if pct_change >= self.threshold_pct {
                    critical_color
                } else {
                    text_color
                };

                container = container.child(
                    div()
                        .absolute()
                        .left(padding + label_width + current_w + 4.0)
                        .top(y_base + bar_height + 4.0)
                        .child(text(indicator).size(10.0).color(indicator_color)),
                );
            }
        }

        // Legend
        container = container.child(
            div()
                .absolute()
                .left(padding + label_width)
                .top(self.height - padding - legend_height + 4.0)
                .flex_row()
                .gap(16.0)
                .child(
                    div()
                        .flex_row()
                        .items_center()
                        .gap(4.0)
                        .child(div().w(12.0).h(12.0).bg(baseline_color).rounded(2.0))
                        .child(text("Baseline").size(10.0).color(text_color)),
                )
                .child(
                    div()
                        .flex_row()
                        .items_center()
                        .gap(4.0)
                        .child(
                            div()
                                .w(12.0)
                                .h(12.0)
                                .bg(self
                                    .current_color
                                    .unwrap_or_else(|| theme.color(ColorToken::Primary)))
                                .rounded(2.0),
                        )
                        .child(text("Current").size(10.0).color(text_color)),
                ),
        );

        ComparisonBarChart { inner: container }
    }
}

impl Default for ComparisonBarChartBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Constructor Functions
// ============================================================================

/// Create a line chart
///
/// # Example
///
/// ```ignore
/// cn::line_chart()
///     .width(400.0)
///     .height(200.0)
///     .series("CPU", &[0.4, 0.6, 0.5, 0.8])
///     .build()
/// ```
pub fn line_chart() -> LineChartBuilder {
    LineChartBuilder::new()
}

/// Create a bar chart
///
/// # Example
///
/// ```ignore
/// cn::bar_chart()
///     .width(300.0)
///     .height(150.0)
///     .data(&[("A", 10.0), ("B", 20.0), ("C", 15.0)])
///     .build()
/// ```
pub fn bar_chart() -> BarChartBuilder {
    BarChartBuilder::new()
}

/// Create a sparkline for inline trend display
///
/// # Example
///
/// ```ignore
/// cn::spark_line(&[1.0, 2.0, 1.5, 3.0, 2.5])
///     .width(100.0)
///     .build()
/// ```
pub fn spark_line(data: &[f64]) -> SparkLineBuilder {
    SparkLineBuilder::new(data)
}

/// Create a line chart with threshold bands for regression detection
///
/// # Example
///
/// ```ignore
/// cn::threshold_line_chart()
///     .width(400.0)
///     .height(150.0)
///     .data(&[12.5, 14.2, 15.8, 18.3, 16.1, 22.4])
///     .regression_bands(16.67, 33.33)  // 60fps, 30fps budgets
///     .build()
/// ```
pub fn threshold_line_chart() -> ThresholdLineChartBuilder {
    ThresholdLineChartBuilder::new()
}

/// Create a histogram for distribution visualization
///
/// # Example
///
/// ```ignore
/// cn::histogram(&pixel_diffs)
///     .bins(50)
///     .threshold_line(5.0, "noise floor")
///     .build()
/// ```
pub fn histogram(data: &[f64]) -> HistogramBuilder {
    HistogramBuilder::new(data)
}

/// Create a comparison bar chart for baseline vs current analysis
///
/// # Example
///
/// ```ignore
/// cn::comparison_bar_chart()
///     .item("Render time", 12.5, 14.2)
///     .item("Layout time", 3.2, 3.1)
///     .item("Paint time", 8.4, 11.2)
///     .threshold(10.0)  // 10% change = warning
///     .build()
/// ```
pub fn comparison_bar_chart() -> ComparisonBarChartBuilder {
    ComparisonBarChartBuilder::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_point() {
        let p = DataPoint::new(1.0, 2.0);
        assert_eq!(p.x, 1.0);
        assert_eq!(p.y, 2.0);
        assert!(p.label.is_none());

        let p = DataPoint::labeled(1.0, 2.0, "Test");
        assert!(p.label.is_some());
    }

    #[test]
    fn test_chart_grid_default() {
        let grid = ChartGrid::default();
        assert!(grid.horizontal);
        assert!(!grid.vertical);
    }
}
