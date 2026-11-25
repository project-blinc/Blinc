//! Where a table's cells are, for drawing them and for the cursor.

use blinc_layout::div::FontWeight;

use super::document::{Block, BlockAlign, BlockKind};
use super::geometry::wrap_source_line;
use super::line::{LineBox, LineFont};
use super::render::RichTextTheme;
use super::wrap::WrappedLine;

/// The space between a cell's edges and its text.
const PAD_X: f32 = 10.0;
const PAD_Y: f32 = 6.0;

/// A cell, from the table's corner.
pub(super) struct GridCell {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub header: bool,
    pub font: LineFont,
    pub align: BlockAlign,
    pub lines: Vec<WrappedLine>,
}

impl GridCell {
    /// Where the text starts.
    pub fn text_origin(&self) -> (f32, f32) {
        (self.x + PAD_X, self.y + PAD_Y)
    }

    /// Where the visual line `index` is placed across the cell.
    pub fn line_box(&self, index: usize) -> LineBox {
        LineBox {
            width: text_width(self.width),
            align: self.align,
            last: index + 1 == self.lines.len(),
        }
    }
}

fn text_width(cell: f32) -> f32 {
    (cell - 2.0 * PAD_X).max(0.0)
}

/// A table's cells, in the order of its lines, and its height. The columns share its width.
pub(super) fn layout(block: &Block, theme: &RichTextTheme, width: f32) -> (Vec<GridCell>, f32) {
    let BlockKind::Table { columns } = &block.kind else {
        return (Vec::new(), 0.0);
    };
    let count = columns.len().max(1);
    let cell_width = width / count as f32;
    let line_height = theme.font_size * theme.line_height;
    let mut cells = Vec::with_capacity(block.lines.len());
    let mut y = 0.0;
    for (row, lines) in block.lines.chunks(count).enumerate() {
        let font = LineFont {
            size: theme.font_size,
            weight: if row == 0 {
                FontWeight::Bold
            } else {
                FontWeight::Normal
            },
            italic: false,
        };
        let wrapped: Vec<_> = lines
            .iter()
            .map(|line| wrap_source_line(line, text_width(cell_width), &font))
            .collect();
        let tallest = wrapped.iter().map(Vec::len).max().unwrap_or(1);
        // Whole, so the cells of every row sit on the same pixels.
        let height = (tallest as f32 * line_height + 2.0 * PAD_Y).ceil();
        for (column, lines) in wrapped.into_iter().enumerate() {
            cells.push(GridCell {
                x: column as f32 * cell_width,
                y,
                width: cell_width,
                height,
                header: row == 0,
                font,
                align: columns.get(column).copied().unwrap_or_default(),
                lines,
            });
        }
        y += height;
    }
    (cells, y)
}
