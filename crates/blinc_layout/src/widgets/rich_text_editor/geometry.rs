//! The geometry walker: the `LineGeometry` index that hit-testing, the cursor
//! and the selection use, matching the layout [`render_document`](super::render::render_document) produces.

use super::cursor::DocPosition;
use super::document::{Block, BlockKind, RichDocument};
use super::render::RichTextTheme;
use super::state::LineGeometry;
use crate::div::FontWeight;

/// Compute the line-geometry index for `doc` at the given `content_width`,
/// matching the layout that [`render_document`](super::render::render_document) produces. Walks every
/// block, applies the same indent / wrap / list-marker accounting, and
/// emits one `LineGeometry` per visual line.
///
/// The returned y-coordinates are relative to the editor's content rect
/// (the same rect whose origin matches the renderer tree). The editor's
/// click handler can use them directly with `local_x` / `local_y` from
/// the mouse-event context.
pub fn compute_line_geometry(
    doc: &RichDocument,
    theme: &RichTextTheme,
    content_width: f32,
) -> Vec<LineGeometry> {
    let mut out = Vec::new();
    let mut y = 0.0f32;
    for (block_idx, block) in doc.blocks.iter().enumerate() {
        let indent_px = (block.indent as f32) * theme.indent_step;
        let inner_width = (content_width - indent_px).max(0.0);

        match &block.kind {
            BlockKind::Divider => {
                // Divider has no inline content; it occupies 1px + the
                // implicit block_gap below. Skip it for cursor purposes.
                y += 1.0;
            }
            BlockKind::Paragraph => {
                emit_paragraph_geometry(
                    &mut out,
                    block_idx,
                    block,
                    indent_px,
                    inner_width,
                    theme.font_size,
                    FontWeight::Normal,
                    false,
                    theme,
                    &mut y,
                );
            }
            BlockKind::Heading(level) => {
                emit_paragraph_geometry(
                    &mut out,
                    block_idx,
                    block,
                    indent_px,
                    inner_width,
                    theme.heading_size(*level),
                    FontWeight::Bold,
                    false,
                    theme,
                    &mut y,
                );
            }
            BlockKind::BulletItem | BlockKind::NumberedItem => {
                let marker_total = theme.bullet_column_width + 8.0;
                let line_x = indent_px + marker_total;
                let line_w = (inner_width - marker_total).max(0.0);
                emit_paragraph_geometry(
                    &mut out,
                    block_idx,
                    block,
                    line_x,
                    line_w,
                    theme.font_size,
                    FontWeight::Normal,
                    false,
                    theme,
                    &mut y,
                );
            }
            BlockKind::Quote => {
                let border_total = theme.quote_border_width + 12.0;
                let line_x = indent_px + border_total;
                let line_w = (inner_width - border_total).max(0.0);
                emit_paragraph_geometry(
                    &mut out,
                    block_idx,
                    block,
                    line_x,
                    line_w,
                    theme.font_size,
                    FontWeight::Normal,
                    true,
                    theme,
                    &mut y,
                );
            }
        }
        // Block gap (matches gap_px on the root flex_col).
        y += theme.block_gap;
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn emit_paragraph_geometry(
    out: &mut Vec<LineGeometry>,
    block_idx: usize,
    block: &Block,
    line_x: f32,
    line_width: f32,
    font_size: f32,
    weight: FontWeight,
    italic: bool,
    theme: &RichTextTheme,
    y: &mut f32,
) {
    let line_height_px = font_size * theme.line_height;
    for (source_line_idx, source_line) in block.lines.iter().enumerate() {
        let visual_lines = if line_width > 0.0 {
            super::wrap::wrap_styled_line_with_offsets(
                source_line,
                line_width,
                font_size,
                weight,
                italic,
            )
        } else {
            vec![super::wrap::WrappedLine {
                line: source_line.clone(),
                source_start_col: 0,
                source_end_col: source_line.text.chars().count(),
            }]
        };
        let to_emit: Vec<_> = if visual_lines.is_empty() {
            vec![super::wrap::WrappedLine {
                line: source_line.clone(),
                source_start_col: 0,
                source_end_col: 0,
            }]
        } else {
            visual_lines
        };
        for visual in to_emit {
            let runs = build_runs_for_visual_line(&visual, font_size, weight, italic);
            out.push(LineGeometry {
                start: DocPosition::new(block_idx, source_line_idx, visual.source_start_col),
                x: line_x,
                y: *y,
                width: line_width,
                height: line_height_px,
                runs,
            });
            *y += line_height_px;
        }
    }
}

/// Decompose a wrapped visual line into `RunGeometry` runs, splitting
/// at every transition between code and non-code coverage. Each run is
/// measured with its own font so the cursor x stays correct on
/// mixed-font lines.
fn build_runs_for_visual_line(
    visual: &super::wrap::WrappedLine,
    font_size: f32,
    weight: FontWeight,
    italic: bool,
) -> Vec<super::state::RunGeometry> {
    use super::state::{RunGeometry, measure_width};

    let line = &visual.line;
    let text = &line.text;
    let len = text.len();
    let body_font = crate::div::FontFamily::default();
    let code_font = crate::div::FontFamily::monospace();

    // Empty visual line — emit one zero-width run so the cursor has a
    // valid landing spot.
    if len == 0 {
        return vec![RunGeometry {
            source_col: visual.source_start_col,
            text: String::new(),
            x_in_line: 0.0,
            width: 0.0,
            font_family: body_font,
            font_size,
            weight,
            italic,
        }];
    }

    // Build a per-byte boolean: is this byte covered by a `code` span?
    let mut code_at: Vec<bool> = vec![false; len];
    for span in &line.spans {
        if !span.code {
            continue;
        }
        let s = span.start.min(len);
        let e = span.end.min(len);
        for b in code_at.iter_mut().take(e).skip(s) {
            *b = true;
        }
    }

    // Walk byte-by-byte and emit one run per code/non-code transition.
    let mut runs: Vec<RunGeometry> = Vec::new();
    let mut x_cursor = 0.0_f32;
    let mut seg_start_byte = 0usize;
    let mut seg_start_char_offset = 0usize;
    let mut current_is_code = code_at.first().copied().unwrap_or(false);

    let mut byte = 0usize;
    let mut char_offset = 0usize;
    while byte < len {
        let here = code_at[byte];
        if here != current_is_code {
            push_run(
                &mut runs,
                &mut x_cursor,
                visual.source_start_col + seg_start_char_offset,
                &text[seg_start_byte..byte],
                current_is_code,
                font_size,
                weight,
                italic,
                &body_font,
                &code_font,
            );
            seg_start_byte = byte;
            seg_start_char_offset = char_offset;
            current_is_code = here;
        }
        let ch_len = text[byte..]
            .chars()
            .next()
            .map(|c| c.len_utf8())
            .unwrap_or(1);
        byte += ch_len;
        char_offset += 1;
    }
    push_run(
        &mut runs,
        &mut x_cursor,
        visual.source_start_col + seg_start_char_offset,
        &text[seg_start_byte..len],
        current_is_code,
        font_size,
        weight,
        italic,
        &body_font,
        &code_font,
    );

    let _ = measure_width; // silence unused-import warning if there are no runs
    runs
}

#[allow(clippy::too_many_arguments)]
fn push_run(
    runs: &mut Vec<super::state::RunGeometry>,
    x_cursor: &mut f32,
    source_col: usize,
    text: &str,
    is_code: bool,
    body_font_size: f32,
    weight: FontWeight,
    italic: bool,
    body_font: &crate::div::FontFamily,
    code_font: &crate::div::FontFamily,
) {
    if text.is_empty() {
        return;
    }
    let font_family = if is_code {
        code_font.clone()
    } else {
        body_font.clone()
    };
    let font_size = body_font_size;
    let width = super::state::measure_width(text, font_size, weight, italic, Some(&font_family));
    runs.push(super::state::RunGeometry {
        source_col,
        text: text.to_string(),
        x_in_line: *x_cursor,
        width,
        font_family,
        font_size,
        weight,
        italic,
    });
    *x_cursor += width;
}
