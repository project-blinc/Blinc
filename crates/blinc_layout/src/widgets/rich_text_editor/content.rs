//! What the rich text editor shows, bound to its version signal.

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use blinc_core::reactive::{ReactiveGraph, computed};
use blinc_core::{Brush, Color, CornerRadius, DrawContext, Rect, State};

use crate::canvas::{Canvas, CanvasBounds, canvas};
use crate::div::{Div, div};
use crate::widgets::cursor::SharedCursorState;

use super::cursor::Selection;
use super::render::{RichTextTheme, compute_line_geometry, render_block};
use super::state::{RichTextData, RichTextState};
use super::toolbar::selection_toolbar;

/// Bring the line index up to date with the document, once per version.
///
/// Everything bound below reads it, and so do the editor's key handlers
/// after an edit, so it is recomputed whenever the version moves rather than
/// where the edit is made.
pub(super) fn ensure_geometry(
    state: &RichTextState,
    theme: &RichTextTheme,
    content_width: f32,
    version: u32,
) {
    let Ok(mut data) = state.lock() else {
        return;
    };
    if data.geometry_stamp == Some(version) {
        return;
    }
    let geometry = compute_line_geometry(&data.document, theme, content_width);
    data.set_line_index(geometry);
    // The blink state follows focus.
    data.set_cursor_visible(data.focused);
    data.geometry_stamp = Some(version);
}

/// A block of the document, named by where it is and what is in it: a block
/// whose key was there before keeps its element.
#[derive(Clone)]
struct BlockKey {
    index: usize,
    digest: u64,
}

/// One rectangle of the selection's highlight.
#[derive(Clone)]
struct Span {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

/// The toolbar as it should look: it is rebuilt when any of this moves.
#[derive(Clone)]
struct ToolbarKey {
    bounds: [u32; 4],
    picker: String,
}

/// A zero-size anchor at the editor's origin that hosts a region of
/// absolutely placed rows.
fn anchor() -> Div {
    div().absolute().left(0.0).top(0.0).w(0.0).h(0.0)
}

fn digest(data: &RichTextData, index: usize) -> u64 {
    let mut hasher = std::hash::DefaultHasher::new();
    format!("{:?}", data.document.blocks[index]).hash(&mut hasher);
    // A numbered item's number depends on the blocks before it.
    data.document.numbered_ordinal(index).hash(&mut hasher);
    hasher.finish()
}

/// The editor's content: the document's blocks, the selection's highlight,
/// the cursor and the floating toolbar.
pub(super) fn content(
    state: &RichTextState,
    version: &State<u32>,
    theme: &RichTextTheme,
    content_width: f32,
) -> Div {
    let signal = version.signal();
    // A value read from the editor, with its geometry current.

    let blocks = {
        let (state, theme) = (Arc::clone(state), theme.clone());
        computed(move |g: &ReactiveGraph| {
            ensure_geometry(&state, &theme, content_width, g.get(signal).unwrap_or(0));
            let data = state.lock().unwrap();
            (0..data.document.blocks.len())
                .map(|index| BlockKey {
                    index,
                    digest: digest(&data, index),
                })
                .collect::<Vec<_>>()
        })
    };
    let render = {
        let (state, theme) = (Arc::clone(state), theme.clone());
        move |key: BlockKey| {
            let data = state.lock().unwrap();
            match data.document.blocks.get(key.index) {
                Some(block) => {
                    render_block(&data.document, key.index, block, &theme, content_width)
                }
                None => div(),
            }
        }
    };
    let document = div().w_full().flex_col().gap_px(theme.block_gap).for_each(
        &blocks,
        |key| (key.index, key.digest),
        render,
    );

    let spans = {
        let (state, theme) = (Arc::clone(state), theme.clone());
        computed(move |g: &ReactiveGraph| {
            ensure_geometry(&state, &theme, content_width, g.get(signal).unwrap_or(0));
            let data = state.lock().unwrap();
            match data.selection {
                Some(sel) if data.focused && !sel.is_empty() => selection_spans(&data, sel),
                _ => Vec::new(),
            }
        })
    };
    let highlight = Color::rgba(theme.text.r, theme.text.g, theme.text.b, 0.18);
    let selection = anchor().for_each(
        &spans,
        |span| {
            (
                span.x.to_bits(),
                span.y.to_bits(),
                span.w.to_bits(),
                span.h.to_bits(),
            )
        },
        move |span: Span| {
            div()
                .absolute()
                .left(span.x)
                .top(span.y)
                .w(span.w)
                .h(span.h)
                .bg(highlight)
        },
    );

    let cursor_state = state.lock().unwrap().cursor_state.clone();
    let cursor = cursor_overlay_canvas(Arc::clone(state), cursor_state, theme.text, content_width);

    let toolbars = {
        let (state, theme) = (Arc::clone(state), theme.clone());
        computed(move |g: &ReactiveGraph| {
            ensure_geometry(&state, &theme, content_width, g.get(signal).unwrap_or(0));
            let mut data = state.lock().unwrap();
            let Some((x, y, w, h)) = data.selection_bounds() else {
                data.toolbar_rect = None;
                return Vec::new();
            };
            vec![ToolbarKey {
                bounds: [x.to_bits(), y.to_bits(), w.to_bits(), h.to_bits()],
                picker: format!("{:?}", data.picker),
            }]
        })
    };
    let toolbar = {
        let (state, version, theme) = (Arc::clone(state), version.clone(), theme.clone());
        anchor().for_each(
            &toolbars,
            |key| (key.bounds, key.picker.clone()),
            move |_| selection_toolbar(&state, &version, &theme).unwrap_or_else(div),
        )
    };

    // The `blinc-rich-text-editor` class is the canonical CSS hook for
    // stylesheet overrides (background, padding, font). Relative, so the
    // cursor, highlight and toolbar are placed against the content rect.
    div()
        .w_full()
        .relative()
        .cursor_text()
        .class("blinc-rich-text-editor")
        .child(document)
        .child(selection)
        .child(cursor)
        .child(toolbar)
}

/// Canvas overlay that draws a blinking cursor at the current document
/// position. It reads the cursor geometry from `state` on each paint, so the
/// blink and a moved cursor need no rebuild.
fn cursor_overlay_canvas(
    state: RichTextState,
    cursor_state: SharedCursorState,
    color: Color,
    width: f32,
) -> Canvas {
    canvas(move |ctx: &mut dyn DrawContext, _bounds: CanvasBounds| {
        // Compute opacity from blink state.
        let (opacity, visible) = match cursor_state.lock() {
            Ok(s) => (s.current_opacity(), s.visible),
            Err(_) => (1.0, false),
        };
        if !visible || opacity < 0.01 {
            return;
        }

        let Ok(data) = state.lock() else {
            return;
        };
        if !data.focused {
            return;
        }
        let Some((x, y, h)) = data.cursor_geometry() else {
            return;
        };

        let c = Color::rgba(color.r, color.g, color.b, color.a * opacity);
        ctx.fill_rect(
            Rect::new(x, y, 2.0, h),
            CornerRadius::default(),
            Brush::Solid(c),
        );
    })
    .absolute()
    .left(0.0)
    .top(0.0)
    .w(width)
    .h_full()
}

/// The rectangles the selection covers: one per visual line, as wide as the
/// selected text on it.
fn selection_spans(data: &RichTextData, sel: Selection) -> Vec<Span> {
    let (start, end) = sel.ordered();
    let mut spans = Vec::new();
    for g in &data.line_index {
        // The [start .. end) columns of this visual line in the selection.
        let line_chars = g.total_chars();
        let line_end_col = g.start.col + line_chars;
        let on_block = g.start.block;
        let on_line = g.start.line;

        // Skip lines fully outside the selection.
        let after_start = (on_block, on_line, line_end_col) >= (start.block, start.line, start.col);
        let before_end = (on_block, on_line, g.start.col) <= (end.block, end.line, end.col);
        if !(after_start && before_end) {
            continue;
        }

        // Compute clamped local columns within this visual line.
        let line_start_pos = (on_block, on_line, g.start.col);
        let line_end_pos = (on_block, on_line, line_end_col);
        let sel_start_pos = (start.block, start.line, start.col);
        let sel_end_pos = (end.block, end.line, end.col);

        let sx_global = sel_start_pos.max(line_start_pos);
        let ex_global = sel_end_pos.min(line_end_pos);
        if sx_global >= ex_global {
            continue;
        }
        let local_start = sx_global.2 - g.start.col;
        let local_end = ex_global.2 - g.start.col;

        // Walk runs to find the pixel x for each end of the selection
        // range, measuring each prefix with its run's actual font.
        let start_px = super::state::pixel_x_for_local_col(g, local_start);
        let end_px = super::state::pixel_x_for_local_col(g, local_end);
        let width = end_px - start_px;
        if width <= 0.0 {
            continue;
        }
        spans.push(Span {
            x: g.x + start_px,
            y: g.y,
            w: width,
            h: g.height,
        });
    }
    spans
}
