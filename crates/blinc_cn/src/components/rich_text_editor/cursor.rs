//! Cursor, selection, and active formatting state.
//!
//! `DocPosition` is a `(block, line, col)` triple where `col` is a *character*
//! column inside a `StyledLine`'s `text`. Byte positions are computed on the
//! fly via [`super::document::char_to_byte`] when needed for slicing.

use super::document::{Block, RichDocument};
use blinc_core::Color;

/// A position inside a rich document.
///
/// `block` indexes into [`RichDocument::blocks`], `line` indexes into the
/// chosen block's `lines` (a block has 1+ lines after soft breaks), and
/// `col` is a character column inside that line's text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DocPosition {
    pub block: usize,
    pub line: usize,
    pub col: usize,
}

impl DocPosition {
    pub const ZERO: DocPosition = DocPosition {
        block: 0,
        line: 0,
        col: 0,
    };

    pub fn new(block: usize, line: usize, col: usize) -> Self {
        Self { block, line, col }
    }

    /// Position at the start of `block`.
    pub fn start_of(block: usize) -> Self {
        Self {
            block,
            line: 0,
            col: 0,
        }
    }

    /// Position at the end of `block` in `doc` (last line, last column).
    /// Returns `ZERO` if the block index is out of range.
    pub fn end_of(doc: &RichDocument, block: usize) -> Self {
        let Some(b) = doc.blocks.get(block) else {
            return Self::ZERO;
        };
        let last_line = b.lines.len().saturating_sub(1);
        let col = b.lines[last_line].text.chars().count();
        Self {
            block,
            line: last_line,
            col,
        }
    }

    /// Clamp this position to a valid location inside `doc`. Out-of-range
    /// indices are pinned to the nearest valid neighbour.
    pub fn clamp(self, doc: &RichDocument) -> Self {
        if doc.blocks.is_empty() {
            return Self::ZERO;
        }
        let block = self.block.min(doc.blocks.len() - 1);
        let b = &doc.blocks[block];
        let line = self.line.min(b.lines.len().saturating_sub(1));
        let col = self.col.min(b.lines[line].text.chars().count());
        Self { block, line, col }
    }
}

/// A selection range. Two `DocPosition`s; ordering is stored verbatim
/// (anchor → head) so that the editor knows which end the user is dragging.
///
/// Use [`Selection::ordered`] to get `(start, end)` with `start <= end`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub anchor: DocPosition,
    pub head: DocPosition,
}

impl Selection {
    /// A degenerate (empty) selection at `pos`.
    pub fn empty(pos: DocPosition) -> Self {
        Self {
            anchor: pos,
            head: pos,
        }
    }

    /// Whether the selection is empty (anchor == head).
    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    /// Return `(start, end)` with `start <= end`.
    pub fn ordered(&self) -> (DocPosition, DocPosition) {
        if self.anchor <= self.head {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }
}

/// Active inline formatting that will be applied to the next typed character.
///
/// When the user clicks Bold without a selection, this struct's `bold` flag
/// flips and any character they type next inherits it. With a selection, the
/// edit ops apply the mark to the selected range and update this state to
/// match the resulting cursor location.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ActiveFormat {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub code: bool,
    pub color: Option<Color>,
    pub link: Option<String>,
}

impl ActiveFormat {
    /// Build an `ActiveFormat` mirroring the span at `pos` in `doc`. Used
    /// after cursor movement so toolbar buttons reflect the format under
    /// the cursor.
    pub fn from_position(doc: &RichDocument, pos: DocPosition) -> Self {
        let Some(block) = doc.blocks.get(pos.block) else {
            return Self::default();
        };
        let Some(line) = block.lines.get(pos.line) else {
            return Self::default();
        };
        // The span ending at or covering the cursor, so that typing at a
        // boundary takes the previous run's marks; at the start of a line,
        // the first span.
        let byte = super::document::char_to_byte(&line.text, pos.col);
        let span = line
            .spans
            .iter()
            .find(|s| s.start < byte && byte <= s.end)
            .or_else(|| line.spans.iter().find(|s| s.start <= byte && byte < s.end))
            .or_else(|| line.spans.iter().rev().find(|s| s.end <= byte));
        match span {
            Some(s) => Self {
                bold: s.bold,
                italic: s.italic,
                underline: s.underline,
                strikethrough: s.strikethrough,
                code: s.code,
                color: Some(s.color),
                link: s.link_url.clone(),
            },
            None => Self::default(),
        }
    }
}

/// Convenience: end-of-document position for `doc`.
pub fn document_end(doc: &RichDocument) -> DocPosition {
    let last = doc.blocks.len().saturating_sub(1);
    DocPosition::end_of(doc, last)
}

/// Convenience: walk one character forward through the document, crossing
/// soft line breaks and block boundaries. Returns `None` if `pos` is already
/// at the document end.
pub fn step_forward(doc: &RichDocument, pos: DocPosition) -> Option<DocPosition> {
    let block = doc.blocks.get(pos.block)?;
    let line = block.lines.get(pos.line)?;
    let line_len = line.text.chars().count();
    if pos.col < line_len {
        return Some(DocPosition::new(pos.block, pos.line, pos.col + 1));
    }
    if pos.line + 1 < block.lines.len() {
        return Some(DocPosition::new(pos.block, pos.line + 1, 0));
    }
    if pos.block + 1 < doc.blocks.len() {
        return Some(DocPosition::start_of(pos.block + 1));
    }
    None
}

/// Convenience: walk one character backward through the document.
pub fn step_backward(doc: &RichDocument, pos: DocPosition) -> Option<DocPosition> {
    if pos.col > 0 {
        return Some(DocPosition::new(pos.block, pos.line, pos.col - 1));
    }
    if pos.line > 0 {
        let prev_line = pos.line - 1;
        let block = doc.blocks.get(pos.block)?;
        let len = block.lines[prev_line].text.chars().count();
        return Some(DocPosition::new(pos.block, prev_line, len));
    }
    if pos.block > 0 {
        return Some(DocPosition::end_of(doc, pos.block - 1));
    }
    None
}

/// Whether `block` allows cursor positioning at all (Dividers don't).
pub fn block_is_editable(block: &Block) -> bool {
    !matches!(block.kind, super::document::BlockKind::Divider)
}

#[cfg(test)]
mod tests {
    use super::super::document::{Block, BlockKind, RichDocument};
    use super::*;
    use blinc_layout::styled_text::{StyledLine, TextSpan};

    fn doc_with(blocks: Vec<Block>) -> RichDocument {
        RichDocument::from_blocks(blocks)
    }

    #[test]
    fn doc_position_ordering() {
        let a = DocPosition::new(0, 0, 5);
        let b = DocPosition::new(0, 1, 0);
        let c = DocPosition::new(1, 0, 0);
        assert!(a < b);
        assert!(b < c);
        assert!(a < c);
    }

    #[test]
    fn end_of_block_walks_to_last_char() {
        let doc = doc_with(vec![Block::paragraph("hello", Color::WHITE)]);
        let end = DocPosition::end_of(&doc, 0);
        assert_eq!(end, DocPosition::new(0, 0, 5));
    }

    #[test]
    fn clamp_pins_to_valid_neighbour() {
        let doc = doc_with(vec![Block::paragraph("hi", Color::WHITE)]);
        assert_eq!(
            DocPosition::new(99, 99, 99).clamp(&doc),
            DocPosition::new(0, 0, 2)
        );
    }

    #[test]
    fn step_forward_crosses_block_boundary() {
        let doc = doc_with(vec![
            Block::paragraph("ab", Color::WHITE),
            Block::paragraph("cd", Color::WHITE),
        ]);
        let mut p = DocPosition::ZERO;
        let mut visited = vec![p];
        while let Some(next) = step_forward(&doc, p) {
            visited.push(next);
            p = next;
        }
        assert_eq!(
            visited,
            vec![
                DocPosition::new(0, 0, 0),
                DocPosition::new(0, 0, 1),
                DocPosition::new(0, 0, 2),
                DocPosition::new(1, 0, 0),
                DocPosition::new(1, 0, 1),
                DocPosition::new(1, 0, 2),
            ]
        );
    }

    #[test]
    fn step_backward_is_inverse_of_forward() {
        let doc = doc_with(vec![
            Block::paragraph("ab", Color::WHITE),
            Block::paragraph("cd", Color::WHITE),
        ]);
        let end = document_end(&doc);
        let mut p = end;
        let mut steps = 0;
        while let Some(prev) = step_backward(&doc, p) {
            p = prev;
            steps += 1;
        }
        assert_eq!(p, DocPosition::ZERO);
        // 6 positions total (3 in each block) = 5 hops to walk through them
        assert_eq!(steps, 5);
    }

    #[test]
    fn selection_ordered_canonicalizes() {
        let a = DocPosition::new(0, 0, 5);
        let b = DocPosition::new(0, 0, 2);
        let sel = Selection { anchor: a, head: b };
        assert_eq!(sel.ordered(), (b, a));
    }

    #[test]
    fn active_format_picks_up_span_at_cursor() {
        let mut line = StyledLine::plain("hello", Color::WHITE);
        // Apply a bold span over "hel"
        line.spans = vec![
            TextSpan::new(0, 3, Color::WHITE, true),
            TextSpan::colored(3, 5, Color::WHITE),
        ];
        let doc = RichDocument::from_blocks(vec![Block {
            kind: BlockKind::Paragraph,
            lines: vec![line],
            indent: 0,
            align: Default::default(),
        }]);
        // Cursor inside the bold run
        let fmt = ActiveFormat::from_position(&doc, DocPosition::new(0, 0, 1));
        assert!(fmt.bold);
        // Cursor inside the plain run
        let fmt2 = ActiveFormat::from_position(&doc, DocPosition::new(0, 0, 4));
        assert!(!fmt2.bold);
    }

    #[test]
    fn active_format_at_run_boundary_inherits_previous_run() {
        let mut line = StyledLine::plain("ab", Color::WHITE);
        line.spans = vec![TextSpan::new(0, 2, Color::WHITE, true)];
        let doc = RichDocument::from_blocks(vec![Block {
            kind: BlockKind::Paragraph,
            lines: vec![line],
            indent: 0,
            align: Default::default(),
        }]);
        // Cursor at end of bold run — typing here should still be bold
        let fmt = ActiveFormat::from_position(&doc, DocPosition::new(0, 0, 2));
        assert!(fmt.bold);
    }
}
