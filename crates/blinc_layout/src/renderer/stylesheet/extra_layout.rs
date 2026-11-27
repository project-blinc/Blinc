//! The layout declarations every path that applies a style to taffy shares:
//! padding and margin per side, percent and `none` for min and max sizes,
//! `auto` margins, `flex-basis`, per-axis gaps and grid placement. Runs after
//! the `gap` shorthand so the per-axis gaps can refine it.

pub(crate) fn apply_extra_layout(
    es: &crate::element_style::ElementStyle,
    style: &mut taffy::Style,
) {
    use crate::element_style::StyleDimension as D;
    use taffy::style::{Dimension, LengthPercentage, LengthPercentageAuto};
    let size = |d: D| match d {
        D::Length(px) => Dimension::length(px),
        D::Percent(p) => Dimension::percent(p),
        D::Auto => Dimension::auto(),
    };
    let limit = |d: D| match d {
        D::Length(px) => LengthPercentageAuto::length(px),
        D::Percent(p) => LengthPercentageAuto::percent(p),
        D::Auto => LengthPercentageAuto::auto(),
    };
    if let Some(d) = es.min_width {
        style.min_size.width = limit(d);
    }
    if let Some(d) = es.min_height {
        style.min_size.height = limit(d);
    }
    if let Some(d) = es.max_width {
        style.max_size.width = limit(d);
    }
    if let Some(d) = es.max_height {
        style.max_size.height = limit(d);
    }
    if let Some(ratio) = es.aspect_ratio {
        style.aspect_ratio = (ratio > 0.0).then_some(ratio);
    }
    if let Some(d) = es.flex_basis {
        style.flex_basis = size(d);
    }
    if let Some(padding) = es.padding {
        let [top, right, bottom, left] = padding.sides().map(|v| v.map(LengthPercentage::length));
        set_sides(&mut style.padding, top, right, bottom, left);
    }
    if let Some(margin) = es.margin {
        let [top, right, bottom, left] =
            margin.sides().map(|v| v.map(LengthPercentageAuto::length));
        set_sides(&mut style.margin, top, right, bottom, left);
    }
    if es.margin_auto[0] {
        style.margin.top = LengthPercentageAuto::auto();
    }
    if es.margin_auto[1] {
        style.margin.right = LengthPercentageAuto::auto();
    }
    if es.margin_auto[2] {
        style.margin.bottom = LengthPercentageAuto::auto();
    }
    if es.margin_auto[3] {
        style.margin.left = LengthPercentageAuto::auto();
    }
    if let Some(gap) = es.row_gap {
        style.gap.height = LengthPercentage::length(gap);
    }
    if let Some(gap) = es.column_gap {
        style.gap.width = LengthPercentage::length(gap);
    }
    apply_grid(es, style);
}

/// Writes the sides that are given and leaves the others as they are.
fn set_sides<T>(
    rect: &mut taffy::geometry::Rect<T>,
    top: Option<T>,
    right: Option<T>,
    bottom: Option<T>,
    left: Option<T>,
) {
    for (slot, side) in [
        (&mut rect.top, top),
        (&mut rect.right, right),
        (&mut rect.bottom, bottom),
        (&mut rect.left, left),
    ] {
        if let Some(side) = side {
            *slot = side;
        }
    }
}

fn apply_grid(es: &crate::element_style::ElementStyle, style: &mut taffy::Style) {
    use crate::element_style::{
        StyleGridLine, StyleGridPlacement, StyleTrack, StyleTrackBound, StyleTrackItem,
    };
    use taffy::style::{
        GridPlacement, GridTemplateComponent, MaxTrackSizingFunction, MinTrackSizingFunction,
        TrackSizingFunction,
    };

    fn min_fn(b: StyleTrackBound) -> MinTrackSizingFunction {
        match b {
            StyleTrackBound::Length(px) => MinTrackSizingFunction::length(px),
            StyleTrackBound::Percent(p) => MinTrackSizingFunction::percent(p),
            StyleTrackBound::MinContent => MinTrackSizingFunction::min_content(),
            StyleTrackBound::MaxContent => MinTrackSizingFunction::max_content(),
            StyleTrackBound::Auto | StyleTrackBound::Fr(_) => MinTrackSizingFunction::auto(),
        }
    }
    fn max_fn(b: StyleTrackBound) -> MaxTrackSizingFunction {
        match b {
            StyleTrackBound::Length(px) => MaxTrackSizingFunction::length(px),
            StyleTrackBound::Percent(p) => MaxTrackSizingFunction::percent(p),
            StyleTrackBound::MinContent => MaxTrackSizingFunction::min_content(),
            StyleTrackBound::MaxContent => MaxTrackSizingFunction::max_content(),
            StyleTrackBound::Auto => MaxTrackSizingFunction::auto(),
            StyleTrackBound::Fr(f) => MaxTrackSizingFunction::fr(f),
        }
    }
    fn track(t: &StyleTrack) -> TrackSizingFunction {
        TrackSizingFunction {
            min: min_fn(t.min),
            max: max_fn(t.max),
        }
    }
    fn template(items: &[StyleTrackItem]) -> Vec<GridTemplateComponent<String>> {
        items
            .iter()
            .map(|item| match item {
                StyleTrackItem::Single(t) => GridTemplateComponent::Single(track(t)),
                StyleTrackItem::Repeat(n, tracks) => {
                    taffy::style_helpers::repeat(*n, tracks.iter().map(track).collect::<Vec<_>>())
                }
            })
            .collect()
    }
    fn line(l: StyleGridLine) -> GridPlacement<String> {
        match l {
            StyleGridLine::Auto => GridPlacement::Auto,
            StyleGridLine::Line(n) => taffy::style_helpers::line(n),
            StyleGridLine::Span(n) => taffy::style_helpers::span(n),
        }
    }
    fn placement(p: StyleGridPlacement) -> taffy::geometry::Line<GridPlacement<String>> {
        taffy::geometry::Line {
            start: line(p.start),
            end: line(p.end),
        }
    }
    if let Some(cols) = &es.grid_template_columns {
        style.grid_template_columns = template(cols);
    }
    if let Some(rows) = &es.grid_template_rows {
        style.grid_template_rows = template(rows);
    }
    if let Some(p) = es.grid_column {
        style.grid_column = placement(p);
    }
    if let Some(p) = es.grid_row {
        style.grid_row = placement(p);
    }
}
