//! The geometry walker: the `LineGeometry` index that hit-testing, the cursor
//! and the selection use, matching the layout [`render_document`](super::render::render_document) produces.

use super::cursor::DocPosition;
use super::document::{Block, BlockKind, RichDocument};
use super::line::{LineBox, LineFont};
use super::render::RichTextTheme;
use super::state::LineGeometry;
use super::wrap::WrappedLine;
use blinc_layout::div::FontWeight;
use blinc_layout::styled_text::StyledLine;

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
            BlockKind::Table { .. } => {
                let (cells, height) = super::grid::layout(block, theme, inner_width);
                let line_height = theme.font_size * theme.line_height;
                for (line, cell) in cells.iter().enumerate() {
                    let (text_x, text_y) = cell.text_origin();
                    for (i, visual) in cell.lines.iter().enumerate() {
                        let line_box = cell.line_box(i);
                        let runs = super::line::runs(visual, &cell.font, &line_box);
                        out.push(LineGeometry {
                            start: DocPosition::new(block_idx, line, visual.source_start_col),
                            x: indent_px + text_x,
                            y: y + text_y + i as f32 * line_height,
                            width: line_box.width,
                            height: line_height,
                            runs: runs.into_iter().map(|(_, run)| run).collect(),
                            cell: Some([indent_px + cell.x, y + cell.y, cell.width, cell.height]),
                        });
                    }
                }
                y += height;
            }
            BlockKind::Paragraph | BlockKind::Heading(_) => {
                let (size, weight) = match &block.kind {
                    BlockKind::Heading(level) => (theme.heading_size(*level), FontWeight::Bold),
                    _ => (theme.font_size, FontWeight::Normal),
                };
                // A quote's text sits beside its bar, in italics.
                let inset = if block.quote {
                    theme.quote_inset()
                } else {
                    0.0
                };
                emit_paragraph_geometry(
                    &mut out,
                    block_idx,
                    block,
                    indent_px + inset,
                    (inner_width - inset).max(0.0),
                    size,
                    weight,
                    block.quote,
                    theme,
                    &mut y,
                );
            }
            BlockKind::BulletItem | BlockKind::NumberedItem | BlockKind::TaskItem { .. } => {
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
        let font = LineFont {
            size: font_size,
            weight,
            italic,
        };
        let visual_lines = wrap_source_line(source_line, line_width, &font);
        let count = visual_lines.len();
        for (i, visual) in visual_lines.into_iter().enumerate() {
            let line_box = LineBox {
                width: line_width,
                align: block.align,
                last: i + 1 == count,
            };
            let runs = super::line::runs(&visual, &font, &line_box)
                .into_iter()
                .map(|(_, run)| run)
                .collect();
            out.push(LineGeometry {
                start: DocPosition::new(block_idx, source_line_idx, visual.source_start_col),
                x: line_x,
                y: *y,
                width: line_width,
                height: line_height_px,
                runs,
                cell: None,
            });
            *y += line_height_px;
        }
    }
}

/// The visual lines of a source line in `width`, at least one.
pub(super) fn wrap_source_line(line: &StyledLine, width: f32, font: &LineFont) -> Vec<WrappedLine> {
    let mut lines = super::wrap::wrap_styled_line_with_offsets(
        line,
        width,
        font.size,
        font.weight,
        font.italic,
    );
    if lines.is_empty() {
        lines.push(WrappedLine {
            line: line.clone(),
            source_start_col: 0,
            source_end_col: line.text.chars().count(),
        });
    }
    lines
}
