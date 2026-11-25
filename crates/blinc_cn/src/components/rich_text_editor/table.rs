//! Tables: rows, columns and moving between cells.
//!
//! A table is one block whose lines are its cells, row by row.

use blinc_layout::styled_text::StyledLine;

use super::cursor::DocPosition;
use super::document::{Block, BlockAlign, BlockKind, RichDocument};

/// A cell of a table in the document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    /// The table's block.
    pub block: usize,
    pub row: usize,
    pub column: usize,
    /// How many rows and columns the table has.
    pub rows: usize,
    pub columns: usize,
}

impl Cell {
    /// The cell a position is in, if it is in a table.
    pub fn at(doc: &RichDocument, pos: DocPosition) -> Option<Self> {
        let block = doc.blocks.get(pos.block)?;
        let columns = block.kind.table_columns()?;
        let line = pos.line.min(block.lines.len().saturating_sub(1));
        Some(Self {
            block: pos.block,
            row: line / columns,
            column: line % columns,
            rows: block.lines.len() / columns,
            columns,
        })
    }

    /// The start of the cell `row`, `column` of the same table.
    fn start_of(&self, row: usize, column: usize) -> DocPosition {
        DocPosition::new(self.block, row * self.columns + column, 0)
    }
}

fn empty_cell() -> StyledLine {
    StyledLine {
        text: String::new(),
        spans: Vec::new(),
    }
}

/// Put an empty table at `pos`: in place of an empty paragraph, or after the block. A paragraph
/// follows it. Returns the start of its first cell.
pub fn insert_table(
    doc: &mut RichDocument,
    pos: DocPosition,
    rows: usize,
    columns: usize,
) -> DocPosition {
    let pos = pos.clamp(doc);
    let table = Block::table(rows, columns);
    let here = &doc.blocks[pos.block];
    let at = if here.kind == BlockKind::Paragraph && here.is_empty() {
        doc.blocks[pos.block] = table;
        pos.block
    } else {
        doc.blocks.insert(pos.block + 1, table);
        pos.block + 1
    };
    let followed = doc
        .blocks
        .get(at + 1)
        .is_some_and(|b| b.kind.table_columns().is_none() && b.kind != BlockKind::Divider);
    if !followed {
        doc.blocks.insert(at + 1, Block::paragraph_empty());
    }
    DocPosition::new(at, 0, 0)
}

/// Add a row before the row `at` of the table `pos` is in, or after the last. Returns the start
/// of the new row's cell in the same column.
pub fn insert_row(doc: &mut RichDocument, pos: DocPosition, at: usize) -> Option<DocPosition> {
    let cell = Cell::at(doc, pos)?;
    let at = at.min(cell.rows);
    let line = at * cell.columns;
    let new = vec![empty_cell(); cell.columns];
    doc.blocks[cell.block].lines.splice(line..line, new);
    Some(cell.start_of(at, cell.column))
}

/// Add a column before the column `at` of the table `pos` is in, or after the last. Returns the
/// start of the new column's cell in the same row.
pub fn insert_column(doc: &mut RichDocument, pos: DocPosition, at: usize) -> Option<DocPosition> {
    let cell = Cell::at(doc, pos)?;
    let at = at.min(cell.columns);
    let block = &mut doc.blocks[cell.block];
    // From the last row, so the rows before it stay where they are.
    for row in (0..cell.rows).rev() {
        block.lines.insert(row * cell.columns + at, empty_cell());
    }
    if let BlockKind::Table { columns } = &mut block.kind {
        columns.insert(at, BlockAlign::Start);
    }
    Some(DocPosition::new(
        cell.block,
        cell.row * (cell.columns + 1) + at,
        0,
    ))
}

/// Remove the row `pos` is in, and with the last one the table. Returns where the cursor goes.
pub fn remove_row(doc: &mut RichDocument, pos: DocPosition) -> Option<DocPosition> {
    let cell = Cell::at(doc, pos)?;
    if cell.rows == 1 {
        return remove_table(doc, pos);
    }
    let line = cell.row * cell.columns;
    doc.blocks[cell.block]
        .lines
        .drain(line..line + cell.columns);
    Some(cell.start_of(cell.row.min(cell.rows - 2), cell.column))
}

/// Remove the column `pos` is in, and with the last one the table. Returns where the cursor goes.
pub fn remove_column(doc: &mut RichDocument, pos: DocPosition) -> Option<DocPosition> {
    let cell = Cell::at(doc, pos)?;
    if cell.columns == 1 {
        return remove_table(doc, pos);
    }
    let block = &mut doc.blocks[cell.block];
    for row in (0..cell.rows).rev() {
        block.lines.remove(row * cell.columns + cell.column);
    }
    if let BlockKind::Table { columns } = &mut block.kind {
        columns.remove(cell.column);
    }
    let columns = cell.columns - 1;
    Some(DocPosition::new(
        cell.block,
        cell.row * columns + cell.column.min(columns - 1),
        0,
    ))
}

/// Remove the table `pos` is in, leaving an empty paragraph. Returns where the cursor goes.
pub fn remove_table(doc: &mut RichDocument, pos: DocPosition) -> Option<DocPosition> {
    let cell = Cell::at(doc, pos)?;
    doc.blocks[cell.block] = Block::paragraph_empty();
    Some(DocPosition::new(cell.block, 0, 0))
}

/// The start of the cell after the one `pos` is in, or before it, in reading order.
pub fn next_cell(doc: &RichDocument, pos: DocPosition, forward: bool) -> Option<DocPosition> {
    let cell = Cell::at(doc, pos)?;
    let line = cell.row * cell.columns + cell.column;
    let next = if forward {
        Some(line + 1).filter(|l| *l < cell.rows * cell.columns)
    } else {
        line.checked_sub(1)
    }?;
    Some(DocPosition::new(cell.block, next, 0))
}

/// The end of the cell below the one `pos` is in, or from the last row the block after the table,
/// which is made where there is none to type in.
pub fn below(doc: &mut RichDocument, pos: DocPosition) -> Option<DocPosition> {
    let cell = Cell::at(doc, pos)?;
    if cell.row + 1 < cell.rows {
        let start = cell.start_of(cell.row + 1, cell.column);
        let end = doc.blocks[cell.block].lines[start.line]
            .text
            .chars()
            .count();
        return Some(DocPosition::new(start.block, start.line, end));
    }
    let after = cell.block + 1;
    let typable = doc
        .blocks
        .get(after)
        .is_some_and(|b| b.kind != BlockKind::Divider);
    if !typable {
        doc.blocks.insert(after, Block::paragraph_empty());
    }
    Some(DocPosition::new(after, 0, 0))
}

/// Align the column `pos` is in. Columns are not justified. Returns whether it changed.
pub fn align_column(doc: &mut RichDocument, pos: DocPosition, align: BlockAlign) -> bool {
    let Some(cell) = Cell::at(doc, pos) else {
        return false;
    };
    match &mut doc.blocks[cell.block].kind {
        BlockKind::Table { columns } if align != BlockAlign::Justify => {
            let changed = columns[cell.column] != align;
            columns[cell.column] = align;
            changed
        }
        _ => false,
    }
}

/// Whether the block holds cells, whose lines never join or split.
pub(super) fn is_table(block: &Block) -> bool {
    block.kind.table_columns().is_some()
}

#[cfg(test)]
mod tests {
    use super::super::cursor::Selection;
    use super::super::edit::{
        delete_backward, delete_forward, delete_selection, insert_text, soft_break, split_block,
    };
    use super::*;
    use blinc_core::Color;

    /// A paragraph, a table of rows `a b`, `c d`, and a paragraph.
    fn doc() -> RichDocument {
        let mut table = Block::table(2, 2);
        for (line, text) in table.lines.iter_mut().zip(["a", "b", "c", "d"]) {
            *line = StyledLine::plain(text, Color::WHITE);
        }
        RichDocument::from_blocks(vec![
            Block::paragraph("before", Color::WHITE),
            table,
            Block::paragraph("after", Color::WHITE),
        ])
    }

    fn cells(doc: &RichDocument) -> Vec<String> {
        doc.blocks[1].lines.iter().map(|l| l.text.clone()).collect()
    }

    fn at(line: usize, col: usize) -> DocPosition {
        DocPosition::new(1, line, col)
    }

    #[test]
    fn rows_and_columns_come_and_go_in_place() {
        let mut d = doc();
        assert_eq!(insert_row(&mut d, at(1, 0), 1), Some(at(3, 0)));
        assert_eq!(cells(&d), ["a", "b", "", "", "c", "d"]);
        assert_eq!(insert_column(&mut d, at(5, 0), 0), Some(at(6, 0)));
        assert_eq!(cells(&d), ["", "a", "b", "", "", "", "", "c", "d"]);
        assert_eq!(d.blocks[1].kind.table_columns(), Some(3));
        assert_eq!(remove_column(&mut d, at(6, 0)), Some(at(4, 0)));
        assert_eq!(remove_row(&mut d, at(2, 0)), Some(at(2, 0)));
        assert_eq!(cells(&d), ["a", "b", "c", "d"]);
        assert_eq!(remove_row(&mut d, at(3, 0)), Some(at(1, 0)));
        assert_eq!(remove_column(&mut d, at(1, 0)), Some(at(0, 0)));
        assert_eq!(cells(&d), ["a"]);
        // The last row or column takes the table with it.
        let mut e = d.clone();
        remove_row(&mut d, at(0, 0));
        remove_column(&mut e, at(0, 0));
        for doc in [d, e] {
            assert_eq!(doc.blocks[1].kind, BlockKind::Paragraph);
        }
    }

    #[test]
    fn a_table_replaces_an_empty_paragraph_and_is_followed_by_text() {
        let mut d = RichDocument::new();
        assert_eq!(
            insert_table(&mut d, DocPosition::ZERO, 2, 3),
            DocPosition::ZERO
        );
        assert_eq!(d.blocks[0].lines.len(), 6);
        assert_eq!(d.blocks[1].kind, BlockKind::Paragraph);
        let mut d = doc();
        let first = insert_table(&mut d, DocPosition::new(0, 0, 3), 1, 1);
        assert_eq!(first, DocPosition::new(1, 0, 0));
        // Two tables are apart by a paragraph.
        assert_eq!(d.blocks[2].kind, BlockKind::Paragraph);
        assert!(is_table(&d.blocks[3]));
    }

    #[test]
    fn moving_between_cells() {
        let mut d = doc();
        assert_eq!(next_cell(&d, at(1, 1), true), Some(at(2, 0)));
        assert_eq!(next_cell(&d, at(2, 1), false), Some(at(1, 0)));
        assert_eq!(next_cell(&d, at(3, 0), true), None);
        assert_eq!(next_cell(&d, at(0, 0), false), None);
        assert_eq!(below(&mut d, at(0, 0)), Some(at(2, 1)));
        assert_eq!(below(&mut d, at(2, 0)), Some(DocPosition::new(2, 0, 0)));
        d.blocks.truncate(2);
        assert_eq!(below(&mut d, at(2, 0)), Some(DocPosition::new(2, 0, 0)));
        assert_eq!(d.blocks.len(), 3);
    }

    #[test]
    fn cells_never_join_or_split() {
        let untouched = doc();
        let mut d = doc();
        for (op, pos) in [
            (delete_backward as fn(&mut _, _) -> _, at(0, 0)),
            (delete_backward, at(1, 0)),
            (delete_backward, at(2, 0)),
            (delete_forward, at(0, 1)),
            (delete_forward, at(3, 1)),
            (delete_forward, DocPosition::new(0, 0, 6)),
            (delete_backward, DocPosition::new(2, 0, 0)),
            (soft_break, at(1, 1)),
            (split_block, at(1, 1)),
        ] {
            op(&mut d, pos);
            assert_eq!(d, untouched, "at {pos:?}");
        }
        // Into the table from the block after it, and into nothing from an empty one.
        assert_eq!(delete_backward(&mut d, DocPosition::new(2, 0, 0)), at(3, 1));
        d.blocks[2] = Block::paragraph_empty();
        assert_eq!(delete_backward(&mut d, DocPosition::new(2, 0, 0)), at(3, 1));
        assert_eq!(d.blocks.len(), 2);
        // A pasted line break is a space.
        let mut d = doc();
        insert_text(&mut d, at(0, 1), " x\ny", &Default::default());
        assert_eq!(cells(&d), ["a x y", "b", "c", "d"]);
        assert_eq!(d.blocks.len(), 3);
        // Enter moves down the column.
        assert_eq!(split_block(&mut d, at(0, 0)), at(2, 1));
    }

    #[test]
    fn deleting_a_selection_empties_cells_and_keeps_the_grid() {
        let select = |a, b| Selection { anchor: a, head: b };
        let mut d = doc();
        delete_selection(&mut d, select(at(0, 0), at(2, 1)));
        assert_eq!(cells(&d), ["", "", "", "d"]);
        // From before the table into it, and from it to after.
        let mut d = doc();
        let start = DocPosition::new(0, 0, 3);
        assert_eq!(delete_selection(&mut d, select(start, at(1, 0))), start);
        assert_eq!(d.blocks[0].plain_text(), "bef");
        assert_eq!(cells(&d), ["", "b", "c", "d"]);
        let mut d = doc();
        delete_selection(&mut d, select(at(2, 0), DocPosition::new(2, 0, 2)));
        assert_eq!(cells(&d), ["a", "b", "", ""]);
        assert_eq!(d.blocks[2].plain_text(), "ter");
        // All of it, with text on both sides.
        let mut d = doc();
        delete_selection(&mut d, select(start, DocPosition::new(2, 0, 2)));
        assert_eq!(d.blocks.len(), 1);
        assert_eq!(d.blocks[0].plain_text(), "befter");
        // Everything.
        let mut d = doc();
        d.blocks.remove(0);
        let end = DocPosition::end_of(&d, 1);
        delete_selection(&mut d, select(DocPosition::ZERO, end));
        assert_eq!(d.blocks.len(), 1);
        assert_eq!(d.blocks[0].kind, BlockKind::Paragraph);
    }

    #[test]
    fn block_changes_leave_tables_alone() {
        use super::super::block_ops::*;
        let untouched = doc();
        let mut d = doc();
        let here = Selection::empty(at(1, 0));
        set_block_kind(&mut d, here, BlockKind::Heading(1));
        toggle_block_kind(&mut d, here, BlockKind::BulletItem);
        toggle_quote(
            &mut d,
            Selection {
                anchor: at(0, 0),
                head: at(0, 1),
            },
        );
        indent_blocks(&mut d, here);
        set_block_align(&mut d, here, BlockAlign::Justify);
        assert_eq!(d, untouched);
        // Alignment is the column's.
        assert!(set_block_align(&mut d, here, BlockAlign::End));
        let BlockKind::Table { columns } = &d.blocks[1].kind else {
            panic!()
        };
        assert_eq!(columns, &[BlockAlign::Start, BlockAlign::End]);
    }

    #[test]
    fn copied_cells_are_apart_by_tabs() {
        let d = doc();
        assert_eq!(d.plain_text_range(at(0, 0), at(3, 1)), "a\tb\nc\td");
    }
}
