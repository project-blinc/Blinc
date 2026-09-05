//! Rich text document model
//!
//! A `RichDocument` is a flat list of `Block`s. Each block has a `BlockKind`
//! (paragraph, heading, list item, quote, divider) and a small vector of
//! `StyledLine`s for its inline content (>1 line only after a soft break).
//!
//! Inline marks (bold, italic, underline, strikethrough, color, link, code)
//! live on the `TextSpan`s inside each line — see [`crate::styled_text`].
//!
//! Numbered list ordinals are *not* stored on the block; the renderer counts
//! contiguous `NumberedItem`s with the same `indent` so splitting and merging
//! list items is free.
//!
//! The model is intentionally flat (no nested `List(Vec<Block>)`) so that
//! cursor positions are simple `(block, line, col)` triples and undo can
//! snapshot the whole document cheaply.

use crate::styled_text::{StyledLine, TextSpan};
use blinc_core::Color;

use super::cursor::DocPosition;

/// A block-level element of a rich document.
#[derive(Clone, Debug, PartialEq)]
pub enum BlockKind {
    /// Plain paragraph (default).
    Paragraph,
    /// Heading at level 1..=6.
    Heading(u8),
    /// Bullet-list item. Sequence of contiguous items at the same `indent`
    /// forms a list; nesting via `indent`.
    BulletItem,
    /// Numbered-list item. Ordinal is computed at render time from the
    /// position within a contiguous run at the same `indent`.
    NumberedItem,
    /// Block quote.
    Quote,
    /// Horizontal divider (no inline content).
    Divider,
}

/// A block in the document — paragraph, heading, list item, quote, divider.
///
/// Public fields by design: every edit op in [`super::edit`] is a free
/// function over `&mut RichDocument`, and downstream users can build their
/// own ops the same way without going through a sealed setter API.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub kind: BlockKind,
    /// Inline content. Always has at least one line; multiple lines occur
    /// only after a soft break (Shift+Enter).
    pub lines: Vec<StyledLine>,
    /// Indentation level — used by list items for nesting and by other
    /// blocks for indented content.
    pub indent: u8,
}

impl Block {
    /// Create an empty paragraph block.
    pub fn paragraph_empty() -> Self {
        Self {
            kind: BlockKind::Paragraph,
            lines: vec![StyledLine {
                text: String::new(),
                spans: Vec::new(),
            }],
            indent: 0,
        }
    }

    /// Create a paragraph block from plain text colored with `color`.
    pub fn paragraph(text: impl Into<String>, color: Color) -> Self {
        Self {
            kind: BlockKind::Paragraph,
            lines: vec![StyledLine::plain(text, color)],
            indent: 0,
        }
    }

    /// Create a heading block at the given level (clamped to 1..=6).
    pub fn heading(level: u8, text: impl Into<String>, color: Color) -> Self {
        Self {
            kind: BlockKind::Heading(level.clamp(1, 6)),
            lines: vec![StyledLine::plain(text, color)],
            indent: 0,
        }
    }

    /// Create a bullet-list item block.
    pub fn bullet(text: impl Into<String>, color: Color) -> Self {
        Self {
            kind: BlockKind::BulletItem,
            lines: vec![StyledLine::plain(text, color)],
            indent: 0,
        }
    }

    /// Create a numbered-list item block.
    pub fn numbered(text: impl Into<String>, color: Color) -> Self {
        Self {
            kind: BlockKind::NumberedItem,
            lines: vec![StyledLine::plain(text, color)],
            indent: 0,
        }
    }

    /// Create a block quote.
    pub fn quote(text: impl Into<String>, color: Color) -> Self {
        Self {
            kind: BlockKind::Quote,
            lines: vec![StyledLine::plain(text, color)],
            indent: 0,
        }
    }

    /// Create a horizontal divider block.
    pub fn divider() -> Self {
        Self {
            kind: BlockKind::Divider,
            lines: vec![StyledLine {
                text: String::new(),
                spans: Vec::new(),
            }],
            indent: 0,
        }
    }

    /// Total length in characters across all soft-broken lines (excluding
    /// the line breaks themselves). Useful for navigation and tests.
    pub fn char_len(&self) -> usize {
        self.lines.iter().map(|l| l.text.chars().count()).sum()
    }

    /// Concatenated plain text (lines joined with `\n`).
    pub fn plain_text(&self) -> String {
        self.lines
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Whether this block has any inline content (any non-empty line).
    pub fn is_empty(&self) -> bool {
        self.lines.iter().all(|l| l.text.is_empty())
    }
}

/// A rich text document — a flat list of blocks.
#[derive(Clone, Debug, PartialEq)]
pub struct RichDocument {
    pub blocks: Vec<Block>,
}

impl Default for RichDocument {
    fn default() -> Self {
        Self::new()
    }
}

impl RichDocument {
    /// Create a new document with a single empty paragraph.
    ///
    /// A document is never allowed to have zero blocks — every operation
    /// that would empty it inserts a fresh paragraph instead. This keeps
    /// cursor positions trivially valid.
    pub fn new() -> Self {
        Self {
            blocks: vec![Block::paragraph_empty()],
        }
    }

    /// Create a document from an explicit list of blocks. If `blocks` is
    /// empty, a single empty paragraph is inserted (see [`Self::new`]).
    pub fn from_blocks(blocks: Vec<Block>) -> Self {
        if blocks.is_empty() {
            Self::new()
        } else {
            Self { blocks }
        }
    }

    /// Number of blocks in the document.
    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    /// Concatenated plain text (blocks joined with `\n`).
    pub fn plain_text(&self) -> String {
        self.blocks
            .iter()
            .map(|b| b.plain_text())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Extract the plain text covered by `start..end` from the document.
    ///
    /// Block boundaries inside the range are joined with `\n`, soft line
    /// breaks (multiple `StyledLine`s within one block) are joined with
    /// `\n` as well. Inline marks (bold, italic, color, links, …) are
    /// dropped — this is the canonical "what to put on the system
    /// clipboard" representation.
    ///
    /// Returns an empty string when the range is collapsed or fully
    /// out-of-bounds.
    pub fn plain_text_range(&self, start: DocPosition, end: DocPosition) -> String {
        if start >= end {
            return String::new();
        }
        if self.blocks.is_empty() {
            return String::new();
        }
        let last_block = self.blocks.len() - 1;
        let s_block = start.block.min(last_block);
        let e_block = end.block.min(last_block);

        let mut out = String::new();
        for (block_idx, block) in self
            .blocks
            .iter()
            .enumerate()
            .take(e_block + 1)
            .skip(s_block)
        {
            if block_idx > s_block {
                out.push('\n');
            }
            let last_line = block.lines.len().saturating_sub(1);
            let s_line = if block_idx == s_block {
                start.line.min(last_line)
            } else {
                0
            };
            let e_line = if block_idx == e_block {
                end.line.min(last_line)
            } else {
                last_line
            };
            for (line_idx, line) in block.lines.iter().enumerate().take(e_line + 1).skip(s_line) {
                if line_idx > s_line {
                    out.push('\n');
                }
                let line_chars = line.text.chars().count();
                let from_col = if block_idx == s_block && line_idx == s_line {
                    start.col.min(line_chars)
                } else {
                    0
                };
                let to_col = if block_idx == e_block && line_idx == e_line {
                    end.col.min(line_chars)
                } else {
                    line_chars
                };
                if to_col > from_col {
                    let from_byte = char_to_byte(&line.text, from_col);
                    let to_byte = char_to_byte(&line.text, to_col);
                    out.push_str(&line.text[from_byte..to_byte]);
                }
            }
        }
        out
    }

    /// Compute the ordinal (1-based) for a NumberedItem at `block_index`.
    ///
    /// Returns `None` if the block at `block_index` is not a `NumberedItem`.
    /// The ordinal is the count of contiguous `NumberedItem`s at the same
    /// `indent` level ending at this block, walking backward and stopping
    /// at the first block that isn't a numbered item at the same indent.
    pub fn numbered_ordinal(&self, block_index: usize) -> Option<u32> {
        let block = self.blocks.get(block_index)?;
        if block.kind != BlockKind::NumberedItem {
            return None;
        }
        let indent = block.indent;
        let mut ordinal: u32 = 1;
        let mut i = block_index;
        while i > 0 {
            i -= 1;
            let prev = &self.blocks[i];
            if prev.kind == BlockKind::NumberedItem && prev.indent == indent {
                ordinal += 1;
            } else if prev.kind == BlockKind::NumberedItem && prev.indent > indent {
                // Skip nested deeper items — they belong to a sub-list and
                // don't break the parent run.
                continue;
            } else {
                break;
            }
        }
        Some(ordinal)
    }
}

// =====================================================================
// Span helpers — used by the editor when splitting / joining lines.
// =====================================================================

/// Convert a character column to a byte index within `text`.
///
/// Saturates to `text.len()` if `char_col` exceeds the line.
pub fn char_to_byte(text: &str, char_col: usize) -> usize {
    text.char_indices()
        .nth(char_col)
        .map(|(b, _)| b)
        .unwrap_or(text.len())
}

/// Convert a byte index to a character column within `text`.
pub fn byte_to_char(text: &str, byte: usize) -> usize {
    text[..byte.min(text.len())].chars().count()
}

/// Total number of characters in a styled line.
pub fn line_char_len(line: &StyledLine) -> usize {
    line.text.chars().count()
}

/// Look up the span covering byte position `byte`. Returns the index of
/// the first span whose `[start, end)` range contains `byte`, or the
/// index where a new span containing `byte` should be inserted if none
/// covers it (e.g., for an empty line).
pub fn span_at_byte(spans: &[TextSpan], byte: usize) -> Option<usize> {
    spans.iter().position(|s| s.start <= byte && byte < s.end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_document_has_one_empty_paragraph() {
        let doc = RichDocument::new();
        assert_eq!(doc.block_count(), 1);
        assert_eq!(doc.blocks[0].kind, BlockKind::Paragraph);
        assert!(doc.blocks[0].is_empty());
    }

    #[test]
    fn from_blocks_empty_falls_back_to_default() {
        let doc = RichDocument::from_blocks(Vec::new());
        assert_eq!(doc.block_count(), 1);
    }

    #[test]
    fn block_paragraph_constructors() {
        let p = Block::paragraph("hello", Color::WHITE);
        assert_eq!(p.kind, BlockKind::Paragraph);
        assert_eq!(p.lines.len(), 1);
        assert_eq!(p.lines[0].text, "hello");
        assert_eq!(p.char_len(), 5);
    }

    #[test]
    fn heading_clamps_level() {
        let h = Block::heading(99, "x", Color::WHITE);
        assert_eq!(h.kind, BlockKind::Heading(6));
        let h0 = Block::heading(0, "x", Color::WHITE);
        assert_eq!(h0.kind, BlockKind::Heading(1));
    }

    #[test]
    fn plain_text_joins_blocks_with_newlines() {
        let doc = RichDocument::from_blocks(vec![
            Block::paragraph("one", Color::WHITE),
            Block::paragraph("two", Color::WHITE),
            Block::paragraph("three", Color::WHITE),
        ]);
        assert_eq!(doc.plain_text(), "one\ntwo\nthree");
    }

    #[test]
    fn numbered_ordinal_counts_contiguous_items() {
        let doc = RichDocument::from_blocks(vec![
            Block::paragraph("intro", Color::WHITE),
            Block::numbered("a", Color::WHITE),
            Block::numbered("b", Color::WHITE),
            Block::numbered("c", Color::WHITE),
            Block::paragraph("break", Color::WHITE),
            Block::numbered("d", Color::WHITE),
        ]);
        assert_eq!(doc.numbered_ordinal(0), None);
        assert_eq!(doc.numbered_ordinal(1), Some(1));
        assert_eq!(doc.numbered_ordinal(2), Some(2));
        assert_eq!(doc.numbered_ordinal(3), Some(3));
        assert_eq!(doc.numbered_ordinal(5), Some(1)); // restarts after break
    }

    #[test]
    fn numbered_ordinal_skips_nested_deeper_items() {
        let mut blocks = vec![
            Block::numbered("top1", Color::WHITE),
            Block::numbered("nested", Color::WHITE),
            Block::numbered("top2", Color::WHITE),
        ];
        blocks[1].indent = 1; // nested deeper
        let doc = RichDocument::from_blocks(blocks);
        assert_eq!(doc.numbered_ordinal(0), Some(1));
        assert_eq!(doc.numbered_ordinal(2), Some(2));
        // nested item is at its own depth — counts independently
        assert_eq!(doc.numbered_ordinal(1), Some(1));
    }

    #[test]
    fn char_byte_round_trip_ascii() {
        let s = "hello";
        assert_eq!(char_to_byte(s, 0), 0);
        assert_eq!(char_to_byte(s, 3), 3);
        assert_eq!(char_to_byte(s, 5), 5);
        assert_eq!(byte_to_char(s, 0), 0);
        assert_eq!(byte_to_char(s, 3), 3);
    }

    #[test]
    fn char_byte_round_trip_unicode() {
        let s = "héllo"; // é = 2 bytes
        assert_eq!(char_to_byte(s, 0), 0);
        assert_eq!(char_to_byte(s, 1), 1);
        assert_eq!(char_to_byte(s, 2), 3);
        assert_eq!(char_to_byte(s, 5), 6);
        assert_eq!(byte_to_char(s, 3), 2);
        assert_eq!(byte_to_char(s, 6), 5);
    }

    #[test]
    fn char_to_byte_saturates_past_end() {
        let s = "abc";
        assert_eq!(char_to_byte(s, 100), 3);
    }

    #[test]
    fn plain_text_range_within_single_line() {
        let doc = RichDocument::from_blocks(vec![Block::paragraph("Hello world", Color::WHITE)]);
        let s = DocPosition::new(0, 0, 6);
        let e = DocPosition::new(0, 0, 11);
        assert_eq!(doc.plain_text_range(s, e), "world");
    }

    #[test]
    fn plain_text_range_across_blocks_uses_newlines() {
        let doc = RichDocument::from_blocks(vec![
            Block::paragraph("first", Color::WHITE),
            Block::paragraph("second", Color::WHITE),
            Block::paragraph("third", Color::WHITE),
        ]);
        let s = DocPosition::new(0, 0, 2);
        let e = DocPosition::new(2, 0, 3);
        assert_eq!(doc.plain_text_range(s, e), "rst\nsecond\nthi");
    }

    #[test]
    fn plain_text_range_collapsed_returns_empty() {
        let doc = RichDocument::from_blocks(vec![Block::paragraph("hello", Color::WHITE)]);
        let p = DocPosition::new(0, 0, 2);
        assert!(doc.plain_text_range(p, p).is_empty());
    }

    #[test]
    fn plain_text_range_across_soft_breaks_uses_newlines() {
        // Build a single block with two soft-broken lines.
        let doc = RichDocument::from_blocks(vec![Block {
            kind: BlockKind::Paragraph,
            lines: vec![
                StyledLine::plain("first line", Color::WHITE),
                StyledLine::plain("second line", Color::WHITE),
            ],
            indent: 0,
        }]);
        let s = DocPosition::new(0, 0, 6);
        let e = DocPosition::new(0, 1, 6);
        assert_eq!(doc.plain_text_range(s, e), "line\nsecond");
    }
}
