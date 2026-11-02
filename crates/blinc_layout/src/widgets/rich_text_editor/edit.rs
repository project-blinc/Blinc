//! Pure text-edit operations on `RichDocument`.
//!
//! Every op takes `&mut RichDocument` plus the inputs it needs and
//! returns the resulting `DocPosition`. They never touch the editor's
//! cursor / selection / undo state — that's the editor's job. Keeping
//! these as free functions over the document means downstream users can
//! build their own toolbars and key handlers without going through a
//! sealed setter API.
//!
//! Spans are kept consistent on every mutation: insertions either extend
//! the span the cursor is sitting in (so typed characters inherit its
//! marks) or create a new run with the supplied `ActiveFormat`. Deletions
//! shrink or merge adjacent spans as needed.

use crate::styled_text::{StyledLine, TextSpan};

use super::cursor::{ActiveFormat, DocPosition, Selection};
use super::document::{Block, BlockKind, RichDocument, char_to_byte};

// =============================================================================
// Public API
// =============================================================================

/// Insert a single character at `pos` carrying `fmt`. Returns the
/// position immediately after the inserted character.
pub fn insert_char(
    doc: &mut RichDocument,
    pos: DocPosition,
    ch: char,
    fmt: &ActiveFormat,
) -> DocPosition {
    let mut buf = [0u8; 4];
    insert_text(doc, pos, ch.encode_utf8(&mut buf), fmt)
}

/// Insert a string at `pos`. Newlines inside `text` are converted to
/// soft line breaks (a new `StyledLine` inside the same block) — use
/// [`split_block`] for hard breaks.
///
/// Returns the position immediately after the inserted text.
pub fn insert_text(
    doc: &mut RichDocument,
    pos: DocPosition,
    text: &str,
    fmt: &ActiveFormat,
) -> DocPosition {
    if text.is_empty() {
        return pos;
    }

    // If text contains '\n', split and insert each piece followed by a
    // soft break.
    if text.contains('\n') {
        let mut current = pos;
        let mut first = true;
        for piece in text.split('\n') {
            if !first {
                current = soft_break(doc, current);
            }
            first = false;
            current = insert_plain(doc, current, piece, fmt);
        }
        return current;
    }

    insert_plain(doc, pos, text, fmt)
}

/// Insert `text` (no newlines) at `pos` with `fmt`. Helper for
/// `insert_text`.
fn insert_plain(
    doc: &mut RichDocument,
    pos: DocPosition,
    text: &str,
    fmt: &ActiveFormat,
) -> DocPosition {
    let pos = pos.clamp(doc);
    let Some(block) = doc.blocks.get_mut(pos.block) else {
        return pos;
    };
    if matches!(block.kind, BlockKind::Divider) {
        return pos;
    }
    let Some(line) = block.lines.get_mut(pos.line) else {
        return pos;
    };

    let byte = char_to_byte(&line.text, pos.col);
    line.text.insert_str(byte, text);
    let added_bytes = text.len();
    let added_chars = text.chars().count();

    // Shift / extend spans.
    rewrite_spans_after_insert(line, byte, added_bytes, fmt);

    DocPosition::new(pos.block, pos.line, pos.col + added_chars)
}

/// Delete the character before `pos`. If `pos` is at column 0 of the
/// first line of a block, the previous block is merged into this one.
pub fn delete_backward(doc: &mut RichDocument, pos: DocPosition) -> DocPosition {
    let pos = pos.clamp(doc);
    if pos.col > 0 {
        return delete_char_in_line(doc, pos.block, pos.line, pos.col - 1);
    }
    if pos.line > 0 {
        // Soft-break boundary — join previous line in the same block.
        return join_lines(doc, pos.block, pos.line - 1);
    }
    if pos.block > 0 {
        return merge_with_previous_block(doc, pos.block);
    }
    pos
}

/// Delete the character at `pos` (forward delete). If `pos` is past the
/// last column of a line, joins with the next line / next block.
pub fn delete_forward(doc: &mut RichDocument, pos: DocPosition) -> DocPosition {
    let pos = pos.clamp(doc);
    let Some(block) = doc.blocks.get(pos.block) else {
        return pos;
    };
    let Some(line) = block.lines.get(pos.line) else {
        return pos;
    };
    let line_len = line.text.chars().count();
    if pos.col < line_len {
        // Plain delete inside the line.
        delete_char_in_line(doc, pos.block, pos.line, pos.col);
        return pos;
    }
    // At end of line: join the next line if any, else merge with the
    // next block.
    if pos.line + 1 < block.lines.len() {
        return join_lines(doc, pos.block, pos.line);
    }
    if pos.block + 1 < doc.blocks.len() {
        // Merge next block into this one. The cursor stays at the join
        // point.
        let pivot = pos;
        merge_with_next_block(doc, pos.block);
        return pivot;
    }
    pos
}

/// Delete the entire selection. Returns the position the cursor should
/// land at (the start of the deleted range).
pub fn delete_selection(doc: &mut RichDocument, sel: Selection) -> DocPosition {
    let (start, end) = sel.ordered();
    if start == end {
        return start;
    }
    // Same line — chop the range out.
    if start.block == end.block && start.line == end.line {
        if let Some(block) = doc.blocks.get_mut(start.block) {
            if let Some(line) = block.lines.get_mut(start.line) {
                let s_byte = char_to_byte(&line.text, start.col);
                let e_byte = char_to_byte(&line.text, end.col);
                line.text.replace_range(s_byte..e_byte, "");
                rewrite_spans_after_delete(line, s_byte, e_byte - s_byte);
            }
        }
        return start;
    }

    // Multi-line / multi-block — repeatedly delete forward from `start`
    // until we reach `end`. Inefficient but correct, and the doc is
    // small enough that it doesn't matter.
    let mut cursor = end;
    while cursor > start {
        let prev = super::cursor::step_backward(doc, cursor).unwrap_or(start);
        delete_backward(doc, cursor);
        cursor = prev;
    }
    start
}

/// Insert a soft line break at `pos` — splits the current `StyledLine`
/// into two lines inside the same block.
pub fn soft_break(doc: &mut RichDocument, pos: DocPosition) -> DocPosition {
    let pos = pos.clamp(doc);
    let Some(block) = doc.blocks.get_mut(pos.block) else {
        return pos;
    };
    if matches!(block.kind, BlockKind::Divider) {
        return pos;
    }
    let Some(line) = block.lines.get(pos.line).cloned() else {
        return pos;
    };
    let byte = char_to_byte(&line.text, pos.col);
    let (left_text, right_text) = line.text.split_at(byte);
    let (left_spans, right_spans) = split_spans(&line.spans, byte);
    let left_line = StyledLine {
        text: left_text.to_string(),
        spans: left_spans,
    };
    let right_line = StyledLine {
        text: right_text.to_string(),
        spans: right_spans,
    };
    block.lines[pos.line] = left_line;
    block.lines.insert(pos.line + 1, right_line);
    DocPosition::new(pos.block, pos.line + 1, 0)
}

/// Insert a hard block break at `pos`: split the current block into two
/// blocks. The new block inherits the kind of the original (Paragraph
/// stays Paragraph; list items stay list items so Enter starts a new
/// list item — pressing Enter on an empty list item exits the list,
/// converting it to a paragraph).
pub fn split_block(doc: &mut RichDocument, pos: DocPosition) -> DocPosition {
    let pos = pos.clamp(doc);
    if doc.blocks.is_empty() {
        doc.blocks.push(Block::paragraph_empty());
        return DocPosition::ZERO;
    }
    let block = doc.blocks[pos.block].clone();

    // Empty list item → exit the list (convert to paragraph).
    if matches!(block.kind, BlockKind::BulletItem | BlockKind::NumberedItem) && block.is_empty() {
        doc.blocks[pos.block].kind = BlockKind::Paragraph;
        doc.blocks[pos.block].indent = 0;
        return DocPosition::new(pos.block, 0, 0);
    }

    // Split the current line at pos.col, then split the line vector
    // around that line.
    let Some(line) = block.lines.get(pos.line).cloned() else {
        return pos;
    };
    let byte = char_to_byte(&line.text, pos.col);
    let (left_text, right_text) = line.text.split_at(byte);
    let (left_spans, right_spans) = split_spans(&line.spans, byte);
    let left_line = StyledLine {
        text: left_text.to_string(),
        spans: left_spans,
    };
    let right_line = StyledLine {
        text: right_text.to_string(),
        spans: right_spans,
    };
    let mut new_lines: Vec<StyledLine> = block.lines[..pos.line].to_vec();
    new_lines.push(left_line);
    let mut next_lines: Vec<StyledLine> = vec![right_line];
    next_lines.extend_from_slice(&block.lines[pos.line + 1..]);

    doc.blocks[pos.block].lines = new_lines;
    let next_kind = match block.kind {
        BlockKind::Heading(_) => BlockKind::Paragraph, // headings don't continue
        other => other,
    };
    let next_block = Block {
        kind: next_kind,
        lines: next_lines,
        indent: block.indent,
    };
    doc.blocks.insert(pos.block + 1, next_block);
    DocPosition::new(pos.block + 1, 0, 0)
}

// =============================================================================
// Internals
// =============================================================================

/// Delete the character at `(block, line, col)`. Updates spans. Returns
/// the new cursor position (same block/line, same col).
fn delete_char_in_line(
    doc: &mut RichDocument,
    block_idx: usize,
    line_idx: usize,
    col: usize,
) -> DocPosition {
    if let Some(block) = doc.blocks.get_mut(block_idx) {
        if let Some(line) = block.lines.get_mut(line_idx) {
            let start_byte = char_to_byte(&line.text, col);
            let end_byte = char_to_byte(&line.text, col + 1);
            if end_byte > start_byte {
                line.text.replace_range(start_byte..end_byte, "");
                let removed = end_byte - start_byte;
                rewrite_spans_after_delete(line, start_byte, removed);
            }
        }
    }
    DocPosition::new(block_idx, line_idx, col)
}

/// Join `line_idx` with `line_idx + 1` inside `block_idx`. The result
/// puts the cursor at the join point.
fn join_lines(doc: &mut RichDocument, block_idx: usize, line_idx: usize) -> DocPosition {
    let Some(block) = doc.blocks.get_mut(block_idx) else {
        return DocPosition::new(block_idx, line_idx, 0);
    };
    if line_idx + 1 >= block.lines.len() {
        return DocPosition::new(
            block_idx,
            line_idx,
            block.lines[line_idx].text.chars().count(),
        );
    }
    let next = block.lines.remove(line_idx + 1);
    let join_byte = block.lines[line_idx].text.len();
    let join_col = block.lines[line_idx].text.chars().count();
    block.lines[line_idx].text.push_str(&next.text);
    // Append spans, shifted by join_byte.
    for mut span in next.spans {
        span.start += join_byte;
        span.end += join_byte;
        block.lines[line_idx].spans.push(span);
    }
    DocPosition::new(block_idx, line_idx, join_col)
}

/// Merge block `block_idx` into block `block_idx - 1`, joining the last
/// line of the previous block with the first line of this one.
fn merge_with_previous_block(doc: &mut RichDocument, block_idx: usize) -> DocPosition {
    if block_idx == 0 || block_idx >= doc.blocks.len() {
        return DocPosition::ZERO;
    }
    let curr = doc.blocks.remove(block_idx);
    let prev = &mut doc.blocks[block_idx - 1];
    if matches!(prev.kind, BlockKind::Divider) {
        // Inserting back to "before the divider" doesn't make sense —
        // re-insert curr and bail.
        doc.blocks.insert(block_idx, curr);
        return DocPosition::start_of(block_idx);
    }
    // The cursor lands at the boundary: the last line of `prev` + 0 col.
    let landing_line = prev.lines.len() - 1;
    let landing_col = prev.lines[landing_line].text.chars().count();
    let landing_byte = prev.lines[landing_line].text.len();

    // Merge first line of `curr` into last line of `prev`.
    if let Some(first) = curr.lines.first() {
        prev.lines[landing_line].text.push_str(&first.text);
        for span in &first.spans {
            let mut s = span.clone();
            s.start += landing_byte;
            s.end += landing_byte;
            prev.lines[landing_line].spans.push(s);
        }
    }
    // Append remaining lines of `curr` as soft-broken lines.
    for line in curr.lines.into_iter().skip(1) {
        prev.lines.push(line);
    }

    DocPosition::new(block_idx - 1, landing_line, landing_col)
}

/// Same as `merge_with_previous_block` but folds `block_idx + 1` into
/// `block_idx`. Used by `delete_forward` at end-of-block.
fn merge_with_next_block(doc: &mut RichDocument, block_idx: usize) {
    if block_idx + 1 >= doc.blocks.len() {
        return;
    }
    let next = doc.blocks.remove(block_idx + 1);
    let curr = &mut doc.blocks[block_idx];
    if matches!(curr.kind, BlockKind::Divider) {
        doc.blocks.insert(block_idx + 1, next);
        return;
    }
    let landing_line = curr.lines.len() - 1;
    let landing_byte = curr.lines[landing_line].text.len();
    if let Some(first) = next.lines.first() {
        curr.lines[landing_line].text.push_str(&first.text);
        for span in &first.spans {
            let mut s = span.clone();
            s.start += landing_byte;
            s.end += landing_byte;
            curr.lines[landing_line].spans.push(s);
        }
    }
    for line in next.lines.into_iter().skip(1) {
        curr.lines.push(line);
    }
}

/// After inserting `added_bytes` at `byte` in `line`, shift / extend
/// the existing spans so they cover the new run.
fn rewrite_spans_after_insert(
    line: &mut StyledLine,
    byte: usize,
    added_bytes: usize,
    fmt: &ActiveFormat,
) {
    // Find the span that contains `byte`. If found, EXTEND it (so the
    // typed text inherits its marks). If at a boundary, prefer the
    // span that ends at `byte` so end-of-run typing continues with
    // that run's marks.
    let mut covered = false;
    for span in line.spans.iter_mut() {
        if span.start <= byte && byte < span.end {
            // Inside this span — grow its end and shift everything later.
            span.end += added_bytes;
            covered = true;
        } else if span.end == byte && format_matches(span, fmt) {
            // Boundary at the trailing edge — extend if the format matches.
            span.end += added_bytes;
            covered = true;
        } else if span.start >= byte {
            // Span lives entirely after the insert point — shift right.
            span.start += added_bytes;
            span.end += added_bytes;
        }
    }
    if !covered {
        // No span absorbed the insertion — emit a new run with the
        // active format.
        let new_span = TextSpan {
            start: byte,
            end: byte + added_bytes,
            // Transparent: the text takes the editor's text colour.
            color: fmt.color.unwrap_or(blinc_core::Color::TRANSPARENT),
            bold: fmt.bold,
            italic: fmt.italic,
            underline: fmt.underline,
            strikethrough: fmt.strikethrough,
            code: fmt.code,
            link_url: fmt.link.clone(),
            token_type: None,
        };
        // Insert in sorted order.
        let pos = line
            .spans
            .iter()
            .position(|s| s.start > byte)
            .unwrap_or(line.spans.len());
        line.spans.insert(pos, new_span);
    }
}

/// After removing `removed_bytes` starting at `byte`, shrink and shift
/// affected spans, dropping any that become empty.
fn rewrite_spans_after_delete(line: &mut StyledLine, byte: usize, removed_bytes: usize) {
    let end_byte = byte + removed_bytes;
    line.spans.retain_mut(|span| {
        if span.end <= byte {
            // Entirely before delete — no change.
            true
        } else if span.start >= end_byte {
            // Entirely after — shift left.
            span.start -= removed_bytes;
            span.end -= removed_bytes;
            true
        } else {
            // Overlaps the deleted range — clamp.
            let new_start = span.start.min(byte);
            let mut new_end = span.end.max(end_byte) - removed_bytes;
            if new_end < new_start {
                new_end = new_start;
            }
            span.start = new_start;
            span.end = new_end;
            span.start < span.end
        }
    });
}

/// Split a span list at byte `at`. Spans crossing `at` are sliced into
/// two; spans entirely before stay in the left list; spans entirely
/// after move to the right list with their byte offsets rebased.
pub(super) fn split_spans(spans: &[TextSpan], at: usize) -> (Vec<TextSpan>, Vec<TextSpan>) {
    let mut left = Vec::new();
    let mut right = Vec::new();
    for span in spans {
        if span.end <= at {
            left.push(span.clone());
        } else if span.start >= at {
            let mut s = span.clone();
            s.start -= at;
            s.end -= at;
            right.push(s);
        } else {
            // Straddles `at` — split.
            let mut l = span.clone();
            l.end = at;
            left.push(l);
            let mut r = span.clone();
            r.start = 0;
            r.end = span.end - at;
            right.push(r);
        }
    }
    (left, right)
}

fn format_matches(span: &TextSpan, fmt: &ActiveFormat) -> bool {
    span.bold == fmt.bold
        && span.italic == fmt.italic
        && span.underline == fmt.underline
        && span.strikethrough == fmt.strikethrough
        && span.code == fmt.code
        && fmt
            .color
            .map(|c| {
                let s = span.color;
                (s.r - c.r).abs() < 1e-3
                    && (s.g - c.g).abs() < 1e-3
                    && (s.b - c.b).abs() < 1e-3
                    && (s.a - c.a).abs() < 1e-3
            })
            .unwrap_or(true)
        && span.link_url == fmt.link
}

#[cfg(test)]
mod tests {
    use super::*;
    use blinc_core::Color;

    fn doc(blocks: Vec<Block>) -> RichDocument {
        RichDocument::from_blocks(blocks)
    }

    #[test]
    fn insert_char_appends_to_empty_paragraph() {
        let mut d = doc(vec![Block::paragraph_empty()]);
        let pos = insert_char(&mut d, DocPosition::ZERO, 'h', &ActiveFormat::default());
        assert_eq!(d.blocks[0].lines[0].text, "h");
        assert_eq!(pos, DocPosition::new(0, 0, 1));
    }

    #[test]
    fn insert_text_in_middle_of_word() {
        let mut d = doc(vec![Block::paragraph("helo", Color::WHITE)]);
        let pos = insert_text(
            &mut d,
            DocPosition::new(0, 0, 3),
            "l",
            &ActiveFormat::default(),
        );
        assert_eq!(d.blocks[0].lines[0].text, "hello");
        assert_eq!(pos, DocPosition::new(0, 0, 4));
    }

    #[test]
    fn insert_text_with_newline_creates_soft_break() {
        let mut d = doc(vec![Block::paragraph("ab", Color::WHITE)]);
        let pos = insert_text(
            &mut d,
            DocPosition::new(0, 0, 1),
            "X\nY",
            &ActiveFormat::default(),
        );
        // Original 'a' / 'b' stays in two lines now.
        assert_eq!(d.blocks[0].lines.len(), 2);
        assert_eq!(d.blocks[0].lines[0].text, "aX");
        assert_eq!(d.blocks[0].lines[1].text, "Yb");
        assert_eq!(pos, DocPosition::new(0, 1, 1));
    }

    #[test]
    fn delete_backward_in_middle_of_line() {
        let mut d = doc(vec![Block::paragraph("hello", Color::WHITE)]);
        let pos = delete_backward(&mut d, DocPosition::new(0, 0, 3));
        assert_eq!(d.blocks[0].lines[0].text, "helo");
        assert_eq!(pos, DocPosition::new(0, 0, 2));
    }

    #[test]
    fn delete_backward_at_start_of_block_merges() {
        let mut d = doc(vec![
            Block::paragraph("foo", Color::WHITE),
            Block::paragraph("bar", Color::WHITE),
        ]);
        let pos = delete_backward(&mut d, DocPosition::new(1, 0, 0));
        assert_eq!(d.blocks.len(), 1);
        assert_eq!(d.blocks[0].lines[0].text, "foobar");
        assert_eq!(pos, DocPosition::new(0, 0, 3));
    }

    #[test]
    fn delete_forward_at_end_of_line_joins_next_line() {
        let mut d = doc(vec![Block {
            kind: BlockKind::Paragraph,
            lines: vec![
                StyledLine::plain("ab", Color::WHITE),
                StyledLine::plain("cd", Color::WHITE),
            ],
            indent: 0,
        }]);
        delete_forward(&mut d, DocPosition::new(0, 0, 2));
        assert_eq!(d.blocks[0].lines.len(), 1);
        assert_eq!(d.blocks[0].lines[0].text, "abcd");
    }

    #[test]
    fn split_block_at_middle_creates_two_blocks() {
        let mut d = doc(vec![Block::paragraph("hello world", Color::WHITE)]);
        let pos = split_block(&mut d, DocPosition::new(0, 0, 5));
        assert_eq!(d.blocks.len(), 2);
        assert_eq!(d.blocks[0].lines[0].text, "hello");
        assert_eq!(d.blocks[1].lines[0].text, " world");
        assert_eq!(pos, DocPosition::new(1, 0, 0));
    }

    #[test]
    fn split_block_after_heading_makes_paragraph() {
        let mut d = doc(vec![Block::heading(1, "title", Color::WHITE)]);
        split_block(&mut d, DocPosition::new(0, 0, 5));
        assert_eq!(d.blocks[0].kind, BlockKind::Heading(1));
        assert_eq!(d.blocks[1].kind, BlockKind::Paragraph);
    }

    #[test]
    fn enter_on_empty_list_item_exits_list() {
        let mut d = doc(vec![Block::bullet("", Color::WHITE)]);
        let pos = split_block(&mut d, DocPosition::new(0, 0, 0));
        assert_eq!(d.blocks.len(), 1);
        assert_eq!(d.blocks[0].kind, BlockKind::Paragraph);
        assert_eq!(pos, DocPosition::new(0, 0, 0));
    }

    #[test]
    fn delete_selection_within_one_line() {
        let mut d = doc(vec![Block::paragraph("hello world", Color::WHITE)]);
        let pos = delete_selection(
            &mut d,
            Selection {
                anchor: DocPosition::new(0, 0, 5),
                head: DocPosition::new(0, 0, 11),
            },
        );
        assert_eq!(d.blocks[0].lines[0].text, "hello");
        assert_eq!(pos, DocPosition::new(0, 0, 5));
    }

    #[test]
    fn delete_selection_across_blocks() {
        let mut d = doc(vec![
            Block::paragraph("foo bar", Color::WHITE),
            Block::paragraph("baz qux", Color::WHITE),
        ]);
        delete_selection(
            &mut d,
            Selection {
                anchor: DocPosition::new(0, 0, 4),
                head: DocPosition::new(1, 0, 4),
            },
        );
        // Should leave: "foo " + " qux" = "foo  qux"… no wait, deleting
        // across the boundary means everything from col 4 of block 0 to
        // col 4 of block 1 goes away, so we end up with "foo" + " qux"
        // joined into one paragraph.
        assert_eq!(d.blocks.len(), 1);
        assert_eq!(d.blocks[0].lines[0].text, "foo qux");
    }

    #[test]
    fn insert_extends_existing_bold_span() {
        let mut p = Block::paragraph("hello", Color::WHITE);
        p.lines[0].spans = vec![TextSpan::new(0, 5, Color::WHITE, true)];
        let mut d = doc(vec![p]);
        let fmt = ActiveFormat {
            bold: true,
            color: Some(Color::WHITE),
            ..Default::default()
        };
        insert_text(&mut d, DocPosition::new(0, 0, 5), "!", &fmt);
        // Single span covering the whole new content.
        assert_eq!(d.blocks[0].lines[0].text, "hello!");
        assert_eq!(d.blocks[0].lines[0].spans.len(), 1);
        assert_eq!(d.blocks[0].lines[0].spans[0].end, 6);
        assert!(d.blocks[0].lines[0].spans[0].bold);
    }

    #[test]
    fn insert_with_different_format_creates_new_span() {
        let mut p = Block::paragraph("hi", Color::WHITE);
        p.lines[0].spans = vec![TextSpan::new(0, 2, Color::WHITE, false)];
        let mut d = doc(vec![p]);
        let fmt = ActiveFormat {
            bold: true,
            color: Some(Color::WHITE),
            ..Default::default()
        };
        insert_text(&mut d, DocPosition::new(0, 0, 2), "!", &fmt);
        let spans = &d.blocks[0].lines[0].spans;
        // The original span should still be there, plus a new bold one.
        assert!(spans.iter().any(|s| s.bold && s.end == 3));
    }
}
