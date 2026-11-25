//! The code editor's minimap: each visible line as a bar, and, when the text is taller than the
//! view, the part in view as a slider over them. One canvas draws both when it is painted, from the editor's lines and its
//! scroll physics as they are then, so the slider follows a scroll and the bars follow an edit
//! without the minimap being built again.
//!
//! Colours come from the stylesheet: `--blinc-code-minimap-line` for a line without syntax
//! colour and `--blinc-code-minimap-slider` for the slider. A line with syntax colour takes the
//! colour of its first span: the highlighter's, else its token's `--blinc-syntax-*`.

use std::collections::HashMap;
use std::sync::Arc;

use blinc_core::{Brush, Color, CornerRadius, DrawContext, Rect};
use blinc_theme::ColorToken;

use super::{CodeEditorData, SharedCodeEditorState};
use crate::canvas::{CanvasBounds, canvas};
use crate::div::{Div, div};
use crate::syntax::TokenType;

/// Space above the first bar and below the last.
const PAD_Y: f32 = 4.0;
/// Space either side of the bars.
const PAD_X: f32 = 2.0;
/// The tallest a line's bar is; more lines than fit at this height share the height.
const MAX_LINE_H: f32 = 2.0;
/// The shortest the slider is, so it stays visible over a long text.
const MIN_SLIDER_H: f32 = 8.0;
/// The line length that fills the minimap's width.
const FULL_LINE_CHARS: f32 = 80.0;

const LINE_COLOR: &str =
    "var(--blinc-code-minimap-line, color-mix(in srgb, var(--text-primary) 40%, transparent))";
const SLIDER_COLOR: &str =
    "var(--blinc-code-minimap-slider, color-mix(in srgb, var(--text-primary) 8%, transparent))";

/// Where the bars and the slider sit in a minimap.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Geometry {
    /// Height of one line's bar.
    pub line_h: f32,
    /// Height of all the bars.
    pub lines_h: f32,
    pub slider_top: f32,
    pub slider_h: f32,
}

/// The geometry of a minimap `height` tall for `lines` visible lines, of a text `content` tall
/// scrolled by `offset` in a `viewport`, the editor's own height.
pub(super) fn geometry(
    lines: usize,
    height: f32,
    viewport: f32,
    content: f32,
    offset: f32,
) -> Geometry {
    let room = (height - 2.0 * PAD_Y).max(0.0);
    let line_h = if lines == 0 {
        MAX_LINE_H
    } else {
        (room / lines as f32).min(MAX_LINE_H)
    };
    let lines_h = lines as f32 * line_h;
    let ratio = if content > 0.0 {
        (viewport / content).min(1.0)
    } else {
        1.0
    };
    // A text that fits the view has nothing to scroll, and no slider.
    let slider_h = if content <= viewport {
        0.0
    } else {
        (lines_h * ratio).max(MIN_SLIDER_H)
    };
    let scrollable = content - viewport;
    let fraction = if scrollable > 0.0 {
        (offset / scrollable).clamp(0.0, 1.0)
    } else {
        0.0
    };
    Geometry {
        line_h,
        lines_h,
        slider_top: PAD_Y + fraction * (lines_h - slider_h).max(0.0),
        slider_h,
    }
}

/// The scroll offset that puts the line under `y` of the minimap in the middle of the editor.
/// `line_height` and `pad` are the editor's line height and padding.
pub(super) fn offset_for(
    y: f32,
    lines: usize,
    height: f32,
    viewport: f32,
    content: f32,
    line_height: f32,
    pad: f32,
) -> f32 {
    if lines == 0 {
        return 0.0;
    }
    let g = geometry(lines, height, viewport, content, 0.0);
    let index = ((y - PAD_Y) / g.line_h).clamp(0.0, (lines - 1) as f32);
    let line_mid = index * line_height + pad + line_height / 2.0;
    (line_mid - viewport / 2.0).clamp(0.0, (content - viewport).max(0.0))
}

/// One bar to draw: its top, height, the share of the width it takes, and its colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Bar {
    pub top: f32,
    pub h: f32,
    pub width: f32,
    pub color: Color,
}

/// The bars for `lines` (each a width share and a colour) at `line_h` per line. Lines thinner
/// than a pixel share a one-pixel bar, as wide as the widest of them and coloured as the first.
pub(super) fn bars(lines: &[(f32, Color)], line_h: f32) -> Vec<Bar> {
    if line_h >= 1.0 {
        return lines
            .iter()
            .enumerate()
            .map(|(i, &(width, color))| Bar {
                top: PAD_Y + i as f32 * line_h,
                h: line_h,
                width,
                color,
            })
            .collect();
    }
    let mut out: Vec<Bar> = Vec::new();
    for (i, &(width, color)) in lines.iter().enumerate() {
        let top = (PAD_Y + i as f32 * line_h).floor();
        match out.last_mut() {
            Some(bar) if bar.top == top => bar.width = bar.width.max(width),
            _ => out.push(Bar {
                top,
                h: 1.0,
                width,
                color,
            }),
        }
    }
    out
}

/// Each visible line's share of the minimap's width and its colour; `ink` colours a line the
/// stylesheet colours.
fn line_shares(data: &CodeEditorData, ink: Color) -> Vec<(f32, Color)> {
    let inherit = data.highlighter.is_none() && data.config.text_color.is_none();
    // The stylesheet is asked once for each kind of token.
    let mut tokens: HashMap<&TokenType, Option<Color>> = HashMap::new();
    data.visible_lines()
        .into_iter()
        .filter_map(|line| data.lines.get(line).map(|text| (line, text)))
        .map(|(line, text)| {
            let width = (text.chars().count() as f32 / FULL_LINE_CHARS).min(1.0);
            let color = if inherit {
                ink
            } else if data.highlighter.is_none() {
                data.config.plain_ink().with_alpha(0.8)
            } else {
                data.highlight_cache
                    .get(line)
                    .and_then(|cached| cached.as_ref())
                    .and_then(|styled| styled.spans.first())
                    .and_then(|span| {
                        if span.color.a > 0.0 {
                            return Some(span.color);
                        }
                        let token = span.token_type.as_ref()?;
                        *tokens.entry(token).or_insert_with(|| {
                            crate::ua_styles::token_ink(token).map(|css| {
                                crate::ua_styles::stylesheet_color(css, ColorToken::TextPrimary)
                            })
                        })
                    })
                    .map_or(ink, |color| color.with_alpha(0.8))
            };
            (width, color)
        })
        .collect()
}

/// The canvas that draws the minimap of `state` across its bounds.
fn minimap_canvas(state: &SharedCodeEditorState) -> crate::canvas::Canvas {
    let state = Arc::clone(state);
    canvas(move |ctx: &mut dyn DrawContext, bounds: CanvasBounds| {
        // A paint never waits on the editor: a frame drawn while it is busy keeps the last one.
        let Ok(data) = state.try_lock() else {
            return;
        };
        let (viewport, content, offset) = data
            .scroll_physics
            .try_lock()
            .map(|p| (p.viewport_height, p.content_height, -p.offset_y))
            .unwrap_or((0.0, 0.0, 0.0));
        let ink = crate::ua_styles::stylesheet_color(LINE_COLOR, ColorToken::TextPrimary);
        let lines = line_shares(&data, ink);
        drop(data);
        let height = if viewport > 0.0 {
            bounds.height.min(viewport)
        } else {
            bounds.height
        };
        let g = geometry(lines.len(), height, viewport, content, offset);
        let span = (bounds.width - 2.0 * PAD_X).max(0.0);
        for bar in bars(&lines, g.line_h) {
            ctx.fill_rect(
                Rect::new(
                    bounds.x + PAD_X,
                    bounds.y + bar.top,
                    span * bar.width,
                    bar.h,
                ),
                CornerRadius::uniform(0.5),
                Brush::Solid(bar.color),
            );
        }
        if g.slider_h > 0.0 {
            let slider = crate::ua_styles::stylesheet_color(SLIDER_COLOR, ColorToken::TextPrimary);
            ctx.fill_rect(
                Rect::new(bounds.x, bounds.y + g.slider_top, bounds.width, g.slider_h),
                CornerRadius::uniform(1.0),
                Brush::Solid(slider),
            );
        }
    })
}

/// Scroll the editor of `state` so the line under `y` of a minimap `height` tall is in the middle
/// of the view, and redraw the editor.
pub(super) fn scroll_to(state: &SharedCodeEditorState, y: f32, height: f32) {
    let refresh = {
        let Ok(data) = state.lock() else {
            return;
        };
        let lines = data.visible_lines().len();
        let line_height = data.config.font_size * data.config.line_height;
        let pad = data.config.padding;
        if let Ok(mut physics) = data.scroll_physics.lock() {
            let (viewport, content) = (physics.viewport_height, physics.content_height);
            let height = if viewport > 0.0 {
                height.min(viewport)
            } else {
                height
            };
            physics.offset_y = -offset_for(y, lines, height, viewport, content, line_height, pad);
        }
        data.editor_stateful.clone()
    };
    if let Some(editor) = refresh {
        crate::stateful::refresh_stateful(&editor);
    }
}

/// The minimap docked inside the editor at its right edge: fixed, so it stays in view while the
/// text scrolls under it.
pub(super) fn docked(state: &SharedCodeEditorState, width: f32) -> Div {
    div()
        .class("blinc-code-minimap")
        .fixed()
        .top(0.0)
        .bottom(0.0)
        .right(0.0)
        .w(width)
        .child(minimap_canvas(state).w_full().h_full())
}

/// A minimap of the editor of `state`, placed by the caller, for example beside the editor in a
/// row. It is drawn from the editor's state each time it is painted, and a press or a drag on it
/// scrolls the editor. `code_editor(..).minimap(true)` docks one inside the editor instead.
pub fn code_minimap(state: &SharedCodeEditorState) -> Div {
    let width = state.lock().map_or(60.0, |d| d.config.minimap_width);
    let (press, drag) = (Arc::clone(state), Arc::clone(state));
    div()
        .class("blinc-code-minimap")
        .w(width)
        .flex_shrink_0()
        .on_mouse_down(move |ctx| scroll_to(&press, ctx.local_y, ctx.bounds_height))
        .on_event(blinc_core::events::event_types::DRAG, move |ctx| {
            scroll_to(&drag, ctx.local_y, ctx.bounds_height)
        })
        .child(minimap_canvas(state).w_full().h_full())
}

#[cfg(test)]
#[path = "minimap_tests.rs"]
mod tests;
