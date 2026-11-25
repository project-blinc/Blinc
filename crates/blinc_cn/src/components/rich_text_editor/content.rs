//! What the rich text editor shows, bound to its version signal.

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use blinc_core::reactive::{ReactiveGraph, computed};
use blinc_core::{Brush, Color, CornerRadius, DrawContext, Rect, Signal};

use blinc_layout::canvas::{Canvas, CanvasBounds, canvas};
use blinc_layout::div::{Div, div};
use blinc_layout::widgets::cursor::SharedCursorState;

use super::cursor::Selection;
use super::document::BlockKind;
use super::render::{Marker, RichTextTheme, compute_line_geometry, render_block};
use super::state::{RichTextData, RichTextState};
use super::tasks::task_box;
use super::toolbar::{
    ToolbarPlacement, fixed_height, fixed_overflows, fixed_toolbar, selection_toolbar, signature,
};

/// Bring the line index up to date with the document, once per version.
///
/// Everything bound below reads it, and so do the editor's key handlers
/// after an edit, so it is recomputed whenever the version moves rather than
/// where the edit is made.
pub(super) fn ensure_geometry(state: &RichTextState, theme: &RichTextTheme, version: u32) {
    let Ok(mut data) = state.lock() else {
        return;
    };
    if data.geometry_stamp == Some(version) {
        return;
    }
    let geometry = compute_line_geometry(&data.document, theme, data.wrap_width);
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
    look: String,
}

/// A zero-size anchor at the editor's origin that hosts a region of
/// absolutely placed rows.
fn anchor() -> Div {
    div().absolute().left(0.0).top(0.0).w(0.0).h(0.0)
}

fn digest(data: &RichTextData, index: usize) -> u64 {
    let mut hasher = std::hash::DefaultHasher::new();
    let block = &data.document.blocks[index];
    // A tick is the box's own to show: see `tasks`.
    match block.kind {
        BlockKind::TaskItem { .. } => "task".hash(&mut hasher),
        ref kind => format!("{kind:?}").hash(&mut hasher),
    }
    format!("{:?}{}{:?}", block.lines, block.indent, block.align).hash(&mut hasher);
    // A numbered item's number depends on the blocks before it.
    data.document.numbered_ordinal(index).hash(&mut hasher);
    // A block wraps to the editor's width.
    data.wrap_width.to_bits().hash(&mut hasher);
    hasher.finish()
}

/// The editor's content: the document's blocks, the selection's highlight,
/// the cursor and the floating toolbar.
pub(super) fn content(state: &RichTextState, version: &Signal<u32>, theme: &RichTextTheme) -> Div {
    let signal = *version;
    let pad = theme.padding;
    // Clicks and the floating toolbar measure from the text's corner. Set first: the toolbar's
    // `for_each` builds while this function runs.
    if let Ok(mut data) = state.lock() {
        data.content_left = pad;
        data.content_top = pad;
    }
    // A value read from the editor, with its geometry current.

    let blocks = {
        let (state, theme) = (Arc::clone(state), theme.clone());
        computed(move |g: &ReactiveGraph| {
            ensure_geometry(&state, &theme, g.get(signal).unwrap_or(0));
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
        let (state, version, theme) = (Arc::clone(state), *version, theme.clone());
        move |key: BlockKey| {
            let data = state.lock().unwrap();
            let Some(block) = data.document.blocks.get(key.index).cloned() else {
                return div();
            };
            let text = Marker::text(&data.document, key.index);
            let width = data.wrap_width;
            // A box registers itself with the state.
            drop(data);
            let marker = match block.kind {
                BlockKind::TaskItem { checked } => {
                    Some(Marker::Box(task_box(&state, version, key.index, checked)))
                }
                _ => text.map(Marker::Text),
            };
            render_block(&block, marker, &theme, width)
        }
    };
    let document = div()
        .class("blinc-rich-text")
        .w_full()
        .flex_col()
        .gap_px(theme.block_gap)
        .for_each(&blocks, |key| (key.index, key.digest), render);

    let spans = {
        let (state, theme) = (Arc::clone(state), theme.clone());
        computed(move |g: &ReactiveGraph| {
            ensure_geometry(&state, &theme, g.get(signal).unwrap_or(0));
            let data = state.lock().unwrap();
            match data.selection {
                // Also while a toolbar field has the focus, so the text it acts on stays marked.
                Some(sel) if (data.focused || data.link_open) && !sel.is_empty() => {
                    selection_spans(&data, sel)
                }
                _ => Vec::new(),
            }
        })
    };
    let highlight = theme.text.map(|c| c.with_alpha(0.18));
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
            let rect = div()
                .class("blinc-rich-text-selection")
                .absolute()
                .left(span.x)
                .top(span.y)
                .w(span.w)
                .h(span.h);
            match highlight {
                Some(c) => rect.bg(c),
                None => rect,
            }
        },
    );

    let cursor_state = state.lock().unwrap().cursor_state.clone();
    let cursor = cursor_overlay_canvas(Arc::clone(state), cursor_state, theme.text);

    let toolbars = {
        let (state, theme) = (Arc::clone(state), theme.clone());
        computed(move |g: &ReactiveGraph| {
            ensure_geometry(&state, &theme, g.get(signal).unwrap_or(0));
            let mut data = state.lock().unwrap();
            let bounds = data.selection_bounds().filter(|_| !data.pressing);
            let Some((x, y, w, h)) = bounds else {
                data.toolbar_rect = None;
                return Vec::new();
            };
            vec![ToolbarKey {
                bounds: [x.to_bits(), y.to_bits(), w.to_bits(), h.to_bits()],
                // The toolbar fits itself to the editor's width.
                look: format!("{}|{}", signature(&data), data.wrap_width.round()),
            }]
        })
    };
    let toolbar = {
        let (state, version, theme) = (Arc::clone(state), *version, theme.clone());
        anchor().for_each(
            &toolbars,
            |key| (key.bounds, key.look.clone()),
            move |_| selection_toolbar(&state, &version, &theme).unwrap_or_else(div),
        )
    };

    // Relative, so the cursor, highlight and floating toolbar are placed against the text.
    // The text wraps to the width the editor is given: a new width re-wraps it.
    let text = {
        let (state, version) = (Arc::clone(state), *version);
        div().on_layout_change(move |bounds| {
            let changed = state.lock().is_ok_and(|mut d| {
                let changed = (d.wrap_width - bounds.width).abs() > 0.5;
                if changed {
                    d.wrap_width = bounds.width;
                }
                changed
            });
            if changed {
                version.update(|n| n.wrapping_add(1));
            }
        })
    };
    let text = text
        .w_full()
        .relative()
        .cursor_text()
        .child(document)
        .child(selection)
        .child(cursor)
        .child(toolbar);
    let body = div()
        .w_full()
        .padding_x_px(pad)
        .padding_y_px(pad)
        .child(text);
    // The `blinc-rich-text-editor` class is the canonical CSS hook for stylesheet overrides
    // (background, font).
    if theme.toolbar != ToolbarPlacement::Fixed {
        return body.class("blinc-rich-text-editor");
    }
    let looks = {
        let state = Arc::clone(state);
        computed(move |g: &ReactiveGraph| {
            g.get(signal);
            let mut data = state.lock().unwrap();
            // Clicks on the text are measured below the toolbar.
            data.content_top = fixed_height(&data) + pad;
            let overflows = fixed_overflows(data.wrap_width + 2.0 * pad);
            vec![format!("{}|{overflows}", signature(&data))]
        })
    };
    let fixed = {
        let (state, version) = (Arc::clone(state), *version);
        div().w_full().for_each(
            &looks,
            |look| look.clone(),
            move |_| fixed_toolbar(&state, &version, pad),
        )
    };
    div()
        .w_full()
        .flex_col()
        .class("blinc-rich-text-editor")
        .child(fixed)
        .child(body)
}

/// Canvas overlay that draws a blinking cursor at the current document
/// position. It reads the cursor geometry from `state` on each paint, so the
/// blink and a moved cursor need no rebuild. Without a `color` it is
/// `--blinc-rich-text-caret`, else the text colour, read each paint.
fn cursor_overlay_canvas(
    state: RichTextState,
    cursor_state: SharedCursorState,
    color: Option<Color>,
) -> Canvas {
    let (state_for_frames, cursor_for_frames) = (state.clone(), cursor_state.clone());
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

        let color = color.unwrap_or_else(|| {
            blinc_layout::ua_styles::stylesheet_color(
                "var(--blinc-rich-text-caret, var(--text-primary))",
                blinc_theme::ColorToken::TextPrimary,
            )
        });
        let c = Color::rgba(color.r, color.g, color.b, color.a * opacity);
        ctx.fill_rect(
            Rect::new(x, y, 2.0, h),
            CornerRadius::default(),
            Brush::Solid(c),
        );
    })
    .redraw_while(move || {
        cursor_for_frames.lock().is_ok_and(|s| s.visible)
            && state_for_frames.lock().is_ok_and(|d| d.focused)
    })
    .absolute()
    .left(0.0)
    .top(0.0)
    .w_full()
    .h_full()
    // Presses go to the text and the checklist boxes under it.
    .pointer_events_none()
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
