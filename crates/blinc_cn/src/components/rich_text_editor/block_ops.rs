//! Block-level transformations on `RichDocument`.
//!
//! These are pure functions over `&mut RichDocument` (mirroring the
//! style of `edit.rs` and `format.rs`) that change the *kind* or
//! *indent* of one or more blocks. Cursor / selection / undo are the
//! editor's job — these ops don't touch them.
//!
//! Block kind ops support a *toggle* convention: applying a kind that
//! every block in the range already has reverts those blocks to
//! plain `Paragraph`. Otherwise the kind is set on every block in
//! the range, regardless of what they were before. This matches the
//! "click bold while everything is bold → unbold" pattern from
//! inline marks.

use blinc_layout::styled_text::StyledLine;

use super::cursor::{DocPosition, Selection};
use super::document::{Block, BlockAlign, BlockKind, RichDocument, char_to_byte};
use super::edit::split_spans;
use super::table::{align_column, is_table};

// =============================================================================
// Public API
// =============================================================================

/// Translate a `Selection` into the inclusive `[first_block, last_block]`
/// range that block-level operations should touch.
///
/// Trims a trailing block when the selection ends *exactly* at its
/// first column (`end.line == 0 && end.col == 0`) and the start lies
/// in an earlier block — that endpoint is the cursor "sitting at the
/// start of the next block", which is the standard editor convention
/// for "the next block was not actually selected, only crossed into".
/// Without this trim, dragging slightly past the end of one paragraph
/// silently converts the next paragraph too.
fn block_range(start: DocPosition, end: DocPosition, doc_len: usize) -> Option<(usize, usize)> {
    if doc_len == 0 {
        return None;
    }
    let last = doc_len - 1;
    let first_block = start.block.min(last);
    let mut last_block = end.block.min(last);
    if last_block > first_block && end.line == 0 && end.col == 0 {
        last_block -= 1;
    }
    Some((first_block, last_block))
}

/// Set every block touched by `range` to `kind`.
///
/// `range` is a `Selection`; if collapsed, only the block containing
/// the cursor is affected. Returns `true` if any block actually
/// changed (kind differed before).
///
/// For toggle behaviour (revert to `Paragraph` when every block in
/// the range already has the requested kind), use [`toggle_block_kind`].
pub fn set_block_kind(doc: &mut RichDocument, range: Selection, kind: BlockKind) -> bool {
    let (start, end) = range.ordered();
    let Some((first, last)) = block_range(start, end, doc.blocks.len()) else {
        return false;
    };
    let mut changed = false;
    for idx in first..=last {
        let Some(block) = doc.blocks.get_mut(idx) else {
            break;
        };
        // A task keeps its tick, and a table is no other kind of block.
        if !block.kind.is_same_kind(&kind) && !is_table(block) {
            block.kind = kind.clone();
            // Headings have no indent; clear it so they don't visually
            // hang from a previous list level.
            if matches!(kind, BlockKind::Heading(_)) {
                block.indent = 0;
            }
            // A list item is no quote; a paragraph and a heading keep theirs.
            block.quote &= block.quotable();
            changed = true;
        }
    }
    changed
}

/// Toggle every block touched by `range` to `kind`.
///
/// If every block in `range` already has `kind`, all of them revert
/// to `BlockKind::Paragraph`. Otherwise every block in `range` is
/// set to `kind` (whatever its previous kind was).
pub fn toggle_block_kind(doc: &mut RichDocument, range: Selection, kind: BlockKind) -> bool {
    let (start, end) = range.ordered();
    let Some((first, last)) = block_range(start, end, doc.blocks.len()) else {
        return false;
    };
    let mut all_already = true;
    let mut any = false;
    for idx in first..=last {
        if let Some(block) = doc.blocks.get(idx) {
            any = true;
            if !block.kind.is_same_kind(&kind) {
                all_already = false;
            }
        }
    }
    if !any {
        return false;
    }
    let target = if all_already {
        BlockKind::Paragraph
    } else {
        kind
    };
    set_block_kind(doc, range, target)
}

/// Quote the blocks touched by `range`, or, where all of them are quotes, unquote them. Tables and
/// dividers are left alone; a list item becomes a quoted paragraph. Returns whether any changed.
pub fn toggle_quote(doc: &mut RichDocument, range: Selection) -> bool {
    let (start, end) = range.ordered();
    let Some((first, last)) = block_range(start, end, doc.blocks.len()) else {
        return false;
    };
    let quotable = |b: &Block| !is_table(b) && b.kind != BlockKind::Divider;
    let touched: Vec<usize> = (first..=last)
        .filter(|i| quotable(&doc.blocks[*i]))
        .collect();
    let on = !touched.iter().all(|i| doc.blocks[*i].quote);
    let mut changed = false;
    for index in touched {
        let block = &mut doc.blocks[index];
        if on {
            if !block.quotable() {
                block.kind = BlockKind::Paragraph;
            }
            if !block.quote {
                block.quote = true;
                // A quote hangs from the bar, not from a list's level.
                block.indent = 0;
                changed = true;
            }
        } else {
            block.quote = false;
            changed = true;
        }
    }
    changed
}

/// Align every block touched by `range`, dividers aside. Returns whether any changed.
pub fn set_block_align(doc: &mut RichDocument, range: Selection, align: BlockAlign) -> bool {
    let (start, end) = range.ordered();
    let Some((first, last)) = block_range(start, end, doc.blocks.len()) else {
        return false;
    };
    let mut changed = false;
    for index in first..=last {
        let block = &mut doc.blocks[index];
        if is_table(block) {
            // The columns the selection has a cell in.
            let columns = block.kind.table_columns().unwrap_or(1);
            let from = if index == start.block { start.line } else { 0 };
            let to = if index == end.block {
                end.line
            } else {
                block.lines.len().saturating_sub(1)
            };
            for line in from..=to.min(from + columns - 1) {
                changed |= align_column(doc, DocPosition::new(index, line, 0), align);
            }
        } else if block.kind != BlockKind::Divider && block.align != align {
            block.align = align;
            changed = true;
        }
    }
    changed
}

/// Increase the `indent` of every block touched by `range` by one.
///
/// Headings, dividers, and quotes are skipped — only paragraphs and
/// list items support indent. Indent is capped at `u8::MAX`.
pub fn indent_blocks(doc: &mut RichDocument, range: Selection) -> bool {
    let (start, end) = range.ordered();
    let Some((first, last)) = block_range(start, end, doc.blocks.len()) else {
        return false;
    };
    let mut changed = false;
    for idx in first..=last {
        let Some(block) = doc.blocks.get_mut(idx) else {
            break;
        };
        if !block_supports_indent(block) {
            continue;
        }
        let next = block.indent.saturating_add(1);
        if next != block.indent {
            block.indent = next;
            changed = true;
        }
    }
    changed
}

/// Decrease the `indent` of every block touched by `range` by one.
/// Stops at 0 (no underflow).
pub fn outdent_blocks(doc: &mut RichDocument, range: Selection) -> bool {
    let (start, end) = range.ordered();
    let Some((first, last)) = block_range(start, end, doc.blocks.len()) else {
        return false;
    };
    let mut changed = false;
    for idx in first..=last {
        let Some(block) = doc.blocks.get_mut(idx) else {
            break;
        };
        if !block_supports_indent(block) {
            continue;
        }
        if block.indent > 0 {
            block.indent -= 1;
            changed = true;
        }
    }
    changed
}

/// Apply `kind` to the text covered by `range` by splitting the
/// containing block at the selection boundaries.
///
/// This is the "make this selected text a heading" op the toolbar's
/// heading picker uses. Block-level kinds (heading, quote, list item)
/// are inherently per-block, so the only way to make *part* of a
/// paragraph a heading is to break the paragraph into pieces:
///
/// - **Prefix** (text before the selection) keeps the original kind.
/// - **Middle** (selected text) becomes a new block with `kind`.
/// - **Suffix** (text after the selection) keeps the original kind.
///
/// Empty halves are skipped, so a selection that starts at column 0
/// produces no prefix block and a selection that runs to the end of a
/// line produces no suffix block.
///
/// Returns the index of the new "middle" block on success — the caller
/// uses this to position the cursor inside the converted text. Returns
/// `None` (without mutating the document) when the operation falls
/// back to a whole-block conversion via [`set_block_kind`]:
///
/// - the selection is collapsed (no text to convert),
/// - the selection spans more than one block,
/// - the selection's anchor block has multiple soft-broken lines or
///   the selection crosses a soft break (we don't try to splice across
///   soft breaks — fall back to whole-block conversion instead),
/// - the selection covers the entire line (a split would produce one
///   block anyway, so just convert kind in place).
///
/// In all fall-back cases the document IS still updated to reflect the
/// requested kind change; only the *split* is skipped, so callers can
/// still treat a `None` return as "the requested change was applied".
pub fn convert_selection_to_block(
    doc: &mut RichDocument,
    range: Selection,
    kind: BlockKind,
) -> Option<usize> {
    let (start, end) = range.ordered();

    // Cases that fall back to whole-block conversion.
    if start == end || start.block != end.block || start.line != end.line {
        set_block_kind(doc, range, kind);
        return None;
    }

    let block_idx = start.block;
    let block = doc.blocks.get(block_idx)?.clone();
    if is_table(&block) {
        return None;
    }

    // Don't try to splice across soft breaks — keep the implementation
    // simple and just convert the whole block.
    if block.lines.len() != 1 {
        set_block_kind(doc, range, kind);
        return None;
    }
    let line = &block.lines[0];
    let line_len = line.text.chars().count();

    // Selection covers the entire line — no split needed; just
    // convert the kind directly.
    if start.col == 0 && end.col >= line_len {
        set_block_kind(doc, range, kind);
        return None;
    }

    let start_byte = char_to_byte(&line.text, start.col);
    let end_byte = char_to_byte(&line.text, end.col);
    let prefix_text = line.text[..start_byte].to_string();
    let middle_text = line.text[start_byte..end_byte].to_string();
    let suffix_text = line.text[end_byte..].to_string();

    let (prefix_spans, rest_spans) = split_spans(&line.spans, start_byte);
    let middle_local_end = end_byte - start_byte;
    let (middle_spans, suffix_spans) = split_spans(&rest_spans, middle_local_end);

    let original_kind = block.kind.clone();
    let original_indent = block.indent;
    // Headings have no indent — they should never visually hang from
    // the parent block's list level.
    let middle_indent = if matches!(kind, BlockKind::Heading(_)) {
        0
    } else {
        original_indent
    };

    let mut replacement: Vec<Block> = Vec::with_capacity(3);
    let mut middle_offset = 0usize;
    if !prefix_text.is_empty() {
        replacement.push(Block {
            kind: original_kind.clone(),
            lines: vec![StyledLine {
                text: prefix_text,
                spans: prefix_spans,
            }],
            indent: original_indent,
            align: block.align,
            quote: block.quote,
        });
        middle_offset = 1;
    }
    replacement.push(Block {
        kind: kind.clone(),
        lines: vec![StyledLine {
            text: middle_text,
            spans: middle_spans,
        }],
        indent: middle_indent,
        align: block.align,
        quote: block.quote && matches!(kind, BlockKind::Paragraph | BlockKind::Heading(_)),
    });
    if !suffix_text.is_empty() {
        replacement.push(Block {
            kind: original_kind,
            lines: vec![StyledLine {
                text: suffix_text,
                spans: suffix_spans,
            }],
            indent: original_indent,
            align: block.align,
            quote: block.quote,
        });
    }

    doc.blocks.splice(block_idx..=block_idx, replacement);
    Some(block_idx + middle_offset)
}

/// Insert a new horizontal divider block immediately after `block_idx`,
/// followed by a fresh empty paragraph (so the user has a place for
/// the cursor to land after the divider).
///
/// Returns the index of the new paragraph block.
pub fn insert_divider_after(doc: &mut RichDocument, block_idx: usize) -> usize {
    let insert_at = (block_idx + 1).min(doc.blocks.len());
    doc.blocks.insert(insert_at, Block::divider());
    let paragraph_at = insert_at + 1;
    doc.blocks.insert(paragraph_at, Block::paragraph_empty());
    paragraph_at
}

// =============================================================================
// Internals
// =============================================================================

fn block_supports_indent(block: &Block) -> bool {
    !block.quote && (block.kind == BlockKind::Paragraph || block.kind.is_list_item())
}

#[cfg(test)]
mod tests {
    use super::super::cursor::DocPosition;
    use super::*;
    use blinc_core::Color;

    fn doc(blocks: Vec<Block>) -> RichDocument {
        RichDocument::from_blocks(blocks)
    }

    /// Build a selection from the start of `b1` to a non-zero column
    /// inside `b2`, so the trim convention (which excludes a trailing
    /// block whose endpoint is exactly its first column) does not kick
    /// in. Most tests want both blocks to be in the operation range.
    fn sel(b1: usize, b2: usize) -> Selection {
        Selection {
            anchor: DocPosition::new(b1, 0, 0),
            // col=1 is enough to mean "really inside b2", even when b1==b2.
            head: DocPosition::new(b2, 0, 1),
        }
    }

    #[test]
    fn set_kind_changes_one_block() {
        let mut d = doc(vec![Block::paragraph("hi", Color::WHITE)]);
        assert!(set_block_kind(&mut d, sel(0, 0), BlockKind::Heading(2)));
        assert_eq!(d.blocks[0].kind, BlockKind::Heading(2));
    }

    #[test]
    fn set_kind_no_op_when_already_matches() {
        let mut d = doc(vec![Block::heading(1, "hi", Color::WHITE)]);
        assert!(!set_block_kind(&mut d, sel(0, 0), BlockKind::Heading(1)));
    }

    #[test]
    fn set_kind_clears_indent_for_heading() {
        let mut blocks = vec![Block::bullet("hi", Color::WHITE)];
        blocks[0].indent = 3;
        let mut d = doc(blocks);
        set_block_kind(&mut d, sel(0, 0), BlockKind::Heading(1));
        assert_eq!(d.blocks[0].indent, 0);
    }

    #[test]
    fn toggle_kind_reverts_to_paragraph_when_all_match() {
        let mut d = doc(vec![
            Block::bullet("a", Color::WHITE),
            Block::bullet("b", Color::WHITE),
        ]);
        toggle_block_kind(&mut d, sel(0, 1), BlockKind::BulletItem);
        assert_eq!(d.blocks[0].kind, BlockKind::Paragraph);
        assert_eq!(d.blocks[1].kind, BlockKind::Paragraph);
    }

    #[test]
    fn toggle_kind_sets_when_some_dont_match() {
        let mut d = doc(vec![
            Block::bullet("a", Color::WHITE),
            Block::paragraph("b", Color::WHITE),
        ]);
        toggle_block_kind(&mut d, sel(0, 1), BlockKind::BulletItem);
        assert_eq!(d.blocks[0].kind, BlockKind::BulletItem);
        assert_eq!(d.blocks[1].kind, BlockKind::BulletItem);
    }

    #[test]
    fn indent_increments_paragraph_and_bullets() {
        let mut d = doc(vec![
            Block::paragraph("a", Color::WHITE),
            Block::bullet("b", Color::WHITE),
            Block::heading(1, "c", Color::WHITE),
        ]);
        indent_blocks(&mut d, sel(0, 2));
        assert_eq!(d.blocks[0].indent, 1);
        assert_eq!(d.blocks[1].indent, 1);
        // Heading is skipped
        assert_eq!(d.blocks[2].indent, 0);
    }

    #[test]
    fn outdent_floors_at_zero() {
        let mut d = doc(vec![Block::paragraph("a", Color::WHITE)]);
        let changed = outdent_blocks(&mut d, sel(0, 0));
        assert!(!changed);
        assert_eq!(d.blocks[0].indent, 0);
    }

    #[test]
    fn outdent_walks_back_one_level() {
        let mut blocks = vec![Block::bullet("a", Color::WHITE)];
        blocks[0].indent = 2;
        let mut d = doc(blocks);
        outdent_blocks(&mut d, sel(0, 0));
        assert_eq!(d.blocks[0].indent, 1);
    }

    #[test]
    fn insert_divider_after_inserts_two_blocks() {
        let mut d = doc(vec![
            Block::paragraph("a", Color::WHITE),
            Block::paragraph("b", Color::WHITE),
        ]);
        let landing = insert_divider_after(&mut d, 0);
        assert_eq!(d.blocks.len(), 4);
        assert_eq!(d.blocks[1].kind, BlockKind::Divider);
        assert_eq!(d.blocks[2].kind, BlockKind::Paragraph);
        assert!(d.blocks[2].is_empty());
        assert_eq!(landing, 2);
        // Original blocks shift / preserve as expected
        assert_eq!(d.blocks[0].lines[0].text, "a");
        assert_eq!(d.blocks[3].lines[0].text, "b");
    }

    #[test]
    fn set_kind_trims_trailing_col_zero_endpoint() {
        // Selection ends at the very start of block 1 — that endpoint
        // is the cursor "sitting at the start of the next paragraph"
        // and should not drag block 1 into the operation. Only block
        // 0 should become a heading.
        let mut d = doc(vec![
            Block::paragraph("first", Color::WHITE),
            Block::paragraph("second", Color::WHITE),
        ]);
        let range = Selection {
            anchor: DocPosition::new(0, 0, 0),
            head: DocPosition::new(1, 0, 0),
        };
        set_block_kind(&mut d, range, BlockKind::Heading(1));
        assert_eq!(d.blocks[0].kind, BlockKind::Heading(1));
        assert_eq!(d.blocks[1].kind, BlockKind::Paragraph);
    }

    #[test]
    fn set_kind_includes_trailing_block_when_endpoint_is_inside_it() {
        // Endpoint at col 1 of block 1 — the user actually entered
        // block 1, so it should be included.
        let mut d = doc(vec![
            Block::paragraph("first", Color::WHITE),
            Block::paragraph("second", Color::WHITE),
        ]);
        let range = Selection {
            anchor: DocPosition::new(0, 0, 0),
            head: DocPosition::new(1, 0, 1),
        };
        set_block_kind(&mut d, range, BlockKind::Heading(1));
        assert_eq!(d.blocks[0].kind, BlockKind::Heading(1));
        assert_eq!(d.blocks[1].kind, BlockKind::Heading(1));
    }

    #[test]
    fn collapsed_selection_at_block_zero_still_affects_block_zero() {
        // Sanity: a collapsed cursor at (0, 0, 0) is NOT a "trailing
        // boundary" — block 0 must still be included.
        let mut d = doc(vec![Block::paragraph("hi", Color::WHITE)]);
        let range = Selection {
            anchor: DocPosition::new(0, 0, 0),
            head: DocPosition::new(0, 0, 0),
        };
        set_block_kind(&mut d, range, BlockKind::Heading(1));
        assert_eq!(d.blocks[0].kind, BlockKind::Heading(1));
    }

    #[test]
    fn convert_selection_splits_into_three_blocks() {
        // "Hello world" with "lo wor" selected (chars 3..9) →
        // ["Hel", "lo wor", "ld"] with the middle as a heading.
        let mut d = doc(vec![Block::paragraph("Hello world", Color::WHITE)]);
        let range = Selection {
            anchor: DocPosition::new(0, 0, 3),
            head: DocPosition::new(0, 0, 9),
        };
        let middle = convert_selection_to_block(&mut d, range, BlockKind::Heading(2));
        assert_eq!(middle, Some(1));
        assert_eq!(d.blocks.len(), 3);
        assert_eq!(d.blocks[0].kind, BlockKind::Paragraph);
        assert_eq!(d.blocks[0].lines[0].text, "Hel");
        assert_eq!(d.blocks[1].kind, BlockKind::Heading(2));
        assert_eq!(d.blocks[1].lines[0].text, "lo wor");
        assert_eq!(d.blocks[2].kind, BlockKind::Paragraph);
        assert_eq!(d.blocks[2].lines[0].text, "ld");
    }

    #[test]
    fn convert_selection_at_block_start_skips_prefix() {
        let mut d = doc(vec![Block::paragraph("Hello world", Color::WHITE)]);
        let range = Selection {
            anchor: DocPosition::new(0, 0, 0),
            head: DocPosition::new(0, 0, 5),
        };
        let middle = convert_selection_to_block(&mut d, range, BlockKind::Heading(1));
        assert_eq!(middle, Some(0));
        assert_eq!(d.blocks.len(), 2);
        assert_eq!(d.blocks[0].kind, BlockKind::Heading(1));
        assert_eq!(d.blocks[0].lines[0].text, "Hello");
        assert_eq!(d.blocks[1].kind, BlockKind::Paragraph);
        assert_eq!(d.blocks[1].lines[0].text, " world");
    }

    #[test]
    fn convert_selection_at_block_end_skips_suffix() {
        let mut d = doc(vec![Block::paragraph("Hello world", Color::WHITE)]);
        let range = Selection {
            anchor: DocPosition::new(0, 0, 6),
            head: DocPosition::new(0, 0, 11),
        };
        let middle = convert_selection_to_block(&mut d, range, BlockKind::Heading(3));
        assert_eq!(middle, Some(1));
        assert_eq!(d.blocks.len(), 2);
        assert_eq!(d.blocks[0].kind, BlockKind::Paragraph);
        assert_eq!(d.blocks[0].lines[0].text, "Hello ");
        assert_eq!(d.blocks[1].kind, BlockKind::Heading(3));
        assert_eq!(d.blocks[1].lines[0].text, "world");
    }

    #[test]
    fn convert_selection_full_line_returns_none_and_converts_in_place() {
        // Selection covers the entire line → no split, just kind change.
        let mut d = doc(vec![Block::paragraph("Hello", Color::WHITE)]);
        let range = Selection {
            anchor: DocPosition::new(0, 0, 0),
            head: DocPosition::new(0, 0, 5),
        };
        let middle = convert_selection_to_block(&mut d, range, BlockKind::Heading(1));
        assert_eq!(middle, None);
        assert_eq!(d.blocks.len(), 1);
        assert_eq!(d.blocks[0].kind, BlockKind::Heading(1));
    }

    #[test]
    fn convert_selection_collapsed_falls_back_to_set_kind() {
        let mut d = doc(vec![Block::paragraph("Hello", Color::WHITE)]);
        let range = Selection {
            anchor: DocPosition::new(0, 0, 2),
            head: DocPosition::new(0, 0, 2),
        };
        let middle = convert_selection_to_block(&mut d, range, BlockKind::Heading(1));
        assert_eq!(middle, None);
        assert_eq!(d.blocks.len(), 1);
        assert_eq!(d.blocks[0].kind, BlockKind::Heading(1));
    }

    #[test]
    fn convert_selection_clears_indent_for_heading_only() {
        // Original block has indent=2; the middle heading should drop
        // to indent=0, but the prefix/suffix paragraphs should keep
        // the original indent.
        let mut blocks = vec![Block::paragraph("Hello world", Color::WHITE)];
        blocks[0].indent = 2;
        let mut d = doc(blocks);
        let range = Selection {
            anchor: DocPosition::new(0, 0, 3),
            head: DocPosition::new(0, 0, 8),
        };
        convert_selection_to_block(&mut d, range, BlockKind::Heading(2));
        assert_eq!(d.blocks[0].indent, 2); // prefix
        assert_eq!(d.blocks[1].indent, 0); // heading
        assert_eq!(d.blocks[2].indent, 2); // suffix
    }

    #[test]
    fn toggle_quote_sets_then_clears() {
        let mut d = doc(vec![Block::paragraph("hi", Color::WHITE)]);
        toggle_quote(&mut d, sel(0, 0));
        assert!(d.blocks[0].quote);
        toggle_quote(&mut d, sel(0, 0));
        assert!(!d.blocks[0].quote);
        assert_eq!(d.blocks[0].kind, BlockKind::Paragraph);
    }

    #[test]
    fn a_quote_is_a_paragraph_or_a_heading_at_the_same_time() {
        let mut d = doc(vec![Block::quote("hi", Color::WHITE)]);
        // Another kind of text keeps the quote.
        assert!(set_block_kind(&mut d, sel(0, 0), BlockKind::Heading(2)));
        assert_eq!(
            (&d.blocks[0].kind, d.blocks[0].quote),
            (&BlockKind::Heading(2), true)
        );
        // A list item is no quote.
        assert!(set_block_kind(&mut d, sel(0, 0), BlockKind::BulletItem));
        assert!(!d.blocks[0].quote);
        // Quoting a list item makes it a quoted paragraph.
        assert!(toggle_quote(&mut d, sel(0, 0)));
        assert_eq!(
            (&d.blocks[0].kind, d.blocks[0].quote),
            (&BlockKind::Paragraph, true)
        );
        // Part of a quoted block, made a heading, is a quote still.
        let part = Selection {
            anchor: DocPosition::new(0, 0, 0),
            head: DocPosition::new(0, 0, 1),
        };
        let at = convert_selection_to_block(&mut d, part, BlockKind::Heading(1));
        assert_eq!(at, Some(0));
        assert!(d.blocks.iter().all(|b| b.quote), "{d:?}");
        // Quoting everything touched, or none of it: a mix is quoted first.
        let mut d = doc(vec![
            Block::quote("a", Color::WHITE),
            Block::paragraph("b", Color::WHITE),
            Block::divider(),
        ]);
        let all = Selection {
            anchor: DocPosition::new(0, 0, 0),
            head: DocPosition::new(2, 0, 0),
        };
        assert!(toggle_quote(&mut d, all));
        assert_eq!(
            d.blocks.iter().map(|b| b.quote).collect::<Vec<_>>(),
            [true, true, false]
        );
        assert!(toggle_quote(&mut d, all));
        assert!(d.blocks.iter().all(|b| !b.quote));
    }

    #[test]
    fn a_quote_takes_no_indent() {
        let mut d = doc(vec![Block::quote("a", Color::WHITE)]);
        assert!(!indent_blocks(&mut d, sel(0, 0)));
        assert_eq!(d.blocks[0].indent, 0);
    }
}
