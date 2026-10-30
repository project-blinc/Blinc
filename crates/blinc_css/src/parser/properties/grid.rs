//! `display: grid` support: track lists and item placement.

use crate::element_style::{
    ElementStyle, StyleGridLine, StyleGridPlacement, StyleTrack, StyleTrackBound, StyleTrackItem,
};
use crate::parser::util::split_whitespace_respecting_parens;
use crate::parser::value::parse_css_px;

/// Applies the declaration if `name` is a grid one. `None` if it is not;
/// otherwise whether the value parsed.
pub(crate) fn apply_grid_property(
    style: &mut ElementStyle,
    name: &str,
    value: &str,
) -> Option<bool> {
    match name {
        "grid-template-columns" => Some(set(&mut style.grid_template_columns, parse_tracks(value))),
        "grid-template-rows" => Some(set(&mut style.grid_template_rows, parse_tracks(value))),
        "grid-column" => Some(set(&mut style.grid_column, parse_placement(value))),
        "grid-row" => Some(set(&mut style.grid_row, parse_placement(value))),
        _ => None,
    }
}

fn set<T>(slot: &mut Option<T>, parsed: Option<T>) -> bool {
    match parsed {
        Some(v) => {
            *slot = Some(v);
            true
        }
        None => false,
    }
}

fn bound(text: &str) -> Option<StyleTrackBound> {
    let t = text.trim();
    Some(match t {
        "auto" => StyleTrackBound::Auto,
        "min-content" => StyleTrackBound::MinContent,
        "max-content" => StyleTrackBound::MaxContent,
        _ => {
            if let Some(fr) = t.strip_suffix("fr") {
                StyleTrackBound::Fr(fr.trim().parse().ok()?)
            } else if let Some(p) = t.strip_suffix('%') {
                StyleTrackBound::Percent(p.trim().parse::<f32>().ok()? / 100.0)
            } else {
                StyleTrackBound::Length(parse_css_px(t)?)
            }
        }
    })
}

/// A single size, or `minmax(min, max)`.
fn track(text: &str) -> Option<StyleTrack> {
    let t = text.trim();
    if let Some(args) = t.strip_prefix("minmax(").and_then(|r| r.strip_suffix(')')) {
        let (min, max) = args.split_once(',')?;
        return Some(StyleTrack {
            min: bound(min)?,
            max: bound(max)?,
        });
    }
    let b = bound(t)?;
    // A flexible size alone is `minmax(auto, <flex>)`.
    let min = if matches!(b, StyleTrackBound::Fr(_)) {
        StyleTrackBound::Auto
    } else {
        b
    };
    Some(StyleTrack { min, max: b })
}

fn parse_tracks(value: &str) -> Option<Vec<StyleTrackItem>> {
    let v = value.trim();
    if v.eq_ignore_ascii_case("none") {
        return Some(Vec::new());
    }
    let mut items = Vec::new();
    for part in split_whitespace_respecting_parens(v) {
        let part = part.trim();
        if let Some(args) = part
            .strip_prefix("repeat(")
            .and_then(|r| r.strip_suffix(')'))
        {
            let (count, tracks) = args.split_once(',')?;
            let count: u16 = count.trim().parse().ok()?;
            let tracks = split_whitespace_respecting_parens(tracks)
                .iter()
                .map(|t| track(t))
                .collect::<Option<Vec<_>>>()?;
            items.push(StyleTrackItem::Repeat(count, tracks));
        } else {
            items.push(StyleTrackItem::Single(track(part)?));
        }
    }
    (!items.is_empty()).then_some(items)
}

fn line(text: &str) -> Option<StyleGridLine> {
    let t = text.trim();
    if t == "auto" {
        return Some(StyleGridLine::Auto);
    }
    if let Some(n) = t.strip_prefix("span") {
        return Some(StyleGridLine::Span(n.trim().parse().ok()?));
    }
    Some(StyleGridLine::Line(t.parse().ok()?))
}

/// `2`, `1 / 3`, `1 / span 2`, `span 2`, `auto`.
fn parse_placement(value: &str) -> Option<StyleGridPlacement> {
    let (start, end) = match value.split_once('/') {
        Some((a, b)) => (line(a)?, line(b)?),
        None => {
            let l = line(value)?;
            (l, StyleGridLine::Auto)
        }
    };
    Some(StyleGridPlacement { start, end })
}
