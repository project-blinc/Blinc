//! Inline formatting operations — mark / unmark a selection range.
//!
//! These are pure functions over `&mut RichDocument` that walk every
//! `StyledLine` covered by a selection and rewrite its `spans` so each
//! span fully inside the selection has the requested attribute toggled
//! (or set), spans straddling the selection boundary are split, and
//! adjacent spans with identical attributes are merged.
//!
//! The toggle behaviour matches every other rich editor: if *every*
//! character in the selection already has the mark, applying the mark
//! removes it; otherwise the mark is added to whatever doesn't have it.
//!
//! All ops are no-ops when the selection is empty.

use blinc_core::Color;

use crate::styled_text::{StyledLine, TextSpan};

use super::cursor::{DocPosition, Selection};
use super::document::{RichDocument, char_to_byte};

/// Which attribute an op should rewrite.
#[derive(Clone, Debug)]
pub enum Mark {
    Bold,
    Italic,
    Underline,
    Strikethrough,
    Code,
    /// Set the color (does not "toggle" — `apply_mark_to_selection`
    /// always sets it).
    Color(Color),
    /// Set or clear the link target. `Some` sets, `None` clears.
    Link(Option<String>),
}

/// Apply (or toggle) `mark` over the entire `sel` range.
///
/// For boolean marks (Bold, Italic, …) this checks whether the entire
/// selection already has the mark — if so it clears it, otherwise it
/// sets it. For `Color` and `Link` it always sets the requested value.
///
/// Returns `true` if anything changed, so the editor can decide whether
/// to push an undo entry.
pub fn apply_mark_to_selection(doc: &mut RichDocument, sel: Selection, mark: Mark) -> bool {
    let (start, end) = sel.ordered();
    if start == end {
        return false;
    }

    // Decide whether this is a "set" or "clear" pass for boolean marks.
    let action = match &mark {
        Mark::Bold | Mark::Italic | Mark::Underline | Mark::Strikethrough | Mark::Code => {
            if selection_fully_has(doc, sel, &mark) {
                MarkAction::Clear
            } else {
                MarkAction::Set
            }
        }
        Mark::Color(_) | Mark::Link(_) => MarkAction::Set,
    };

    let mut changed = false;
    for_each_line_in_selection(doc, start, end, |line, byte_start, byte_end| {
        if rewrite_line_marks(line, byte_start, byte_end, &mark, action) {
            changed = true;
        }
    });
    changed
}

#[derive(Clone, Copy)]
enum MarkAction {
    Set,
    Clear,
}

/// Test whether every span fully covered by the selection already has
/// the boolean mark `mark`. Used to decide whether the toggle should
/// set or clear.
fn selection_fully_has(doc: &RichDocument, sel: Selection, mark: &Mark) -> bool {
    let (start, end) = sel.ordered();
    let mut any = false;
    let mut all = true;
    walk_selection_chars(doc, start, end, |span| {
        any = true;
        let has = match mark {
            Mark::Bold => span.bold,
            Mark::Italic => span.italic,
            Mark::Underline => span.underline,
            Mark::Strikethrough => span.strikethrough,
            Mark::Code => span.code,
            _ => true,
        };
        if !has {
            all = false;
        }
    });
    any && all
}

/// Walk every span byte that overlaps the selection (in document order),
/// invoking `visit` once per character.
fn walk_selection_chars<F: FnMut(&TextSpan)>(
    doc: &RichDocument,
    start: DocPosition,
    end: DocPosition,
    mut visit: F,
) {
    for_each_line_in_selection_ref(doc, start, end, |line, byte_start, byte_end| {
        for span in &line.spans {
            let s = span.start.max(byte_start);
            let e = span.end.min(byte_end);
            if s >= e {
                continue;
            }
            visit(span);
        }
    });
}

/// Compute the `(byte_start, byte_end)` range to mutate inside `line`
/// for the given selection, then invoke `f`.
fn for_each_line_in_selection<F: FnMut(&mut StyledLine, usize, usize)>(
    doc: &mut RichDocument,
    start: DocPosition,
    end: DocPosition,
    mut f: F,
) {
    if start.block > end.block || (start.block == end.block && start > end) {
        return;
    }
    let block_count = doc.blocks.len();
    let mut block_idx = start.block;
    while block_idx <= end.block && block_idx < block_count {
        // Snapshot line count to avoid mutable borrow conflicts.
        let line_count = doc.blocks[block_idx].lines.len();
        let first_line = if block_idx == start.block {
            start.line
        } else {
            0
        };
        let last_line = if block_idx == end.block {
            end.line
        } else {
            line_count.saturating_sub(1)
        };
        let stop_line = last_line.min(line_count.saturating_sub(1));
        for (line_idx, line) in doc.blocks[block_idx]
            .lines
            .iter_mut()
            .enumerate()
            .take(stop_line + 1)
            .skip(first_line)
        {
            // Compute the byte range for this line within the selection.
            let line_start_col = if block_idx == start.block && line_idx == start.line {
                start.col
            } else {
                0
            };
            let line_end_col = if block_idx == end.block && line_idx == end.line {
                end.col
            } else {
                line.text.chars().count()
            };
            let bs = char_to_byte(&line.text, line_start_col);
            let be = char_to_byte(&line.text, line_end_col);
            if bs >= be {
                continue;
            }
            f(line, bs, be);
        }
        block_idx += 1;
    }
}

/// Same as `for_each_line_in_selection` but with an immutable line
/// reference. Used by the "does the whole selection have this mark?"
/// query.
fn for_each_line_in_selection_ref<F: FnMut(&StyledLine, usize, usize)>(
    doc: &RichDocument,
    start: DocPosition,
    end: DocPosition,
    mut f: F,
) {
    if start.block > end.block || (start.block == end.block && start > end) {
        return;
    }
    let block_count = doc.blocks.len();
    let mut block_idx = start.block;
    while block_idx <= end.block && block_idx < block_count {
        let line_count = doc.blocks[block_idx].lines.len();
        let first_line = if block_idx == start.block {
            start.line
        } else {
            0
        };
        let last_line = if block_idx == end.block {
            end.line
        } else {
            line_count.saturating_sub(1)
        };
        let stop_line = last_line.min(line_count.saturating_sub(1));
        for (line_idx, line) in doc.blocks[block_idx]
            .lines
            .iter()
            .enumerate()
            .take(stop_line + 1)
            .skip(first_line)
        {
            let line_start_col = if block_idx == start.block && line_idx == start.line {
                start.col
            } else {
                0
            };
            let line_end_col = if block_idx == end.block && line_idx == end.line {
                end.col
            } else {
                line.text.chars().count()
            };
            let bs = char_to_byte(&line.text, line_start_col);
            let be = char_to_byte(&line.text, line_end_col);
            if bs >= be {
                continue;
            }
            f(line, bs, be);
        }
        block_idx += 1;
    }
}

/// Rewrite the spans of `line` so that all bytes in `[start, end)` carry
/// `mark` set or cleared per `action`. Spans straddling the boundaries
/// are split; adjacent spans with identical attributes are merged.
///
/// Returns `true` if any span attribute actually changed.
fn rewrite_line_marks(
    line: &mut StyledLine,
    start: usize,
    end: usize,
    mark: &Mark,
    action: MarkAction,
) -> bool {
    if start >= end || line.spans.is_empty() {
        return false;
    }
    let mut new_spans: Vec<TextSpan> = Vec::with_capacity(line.spans.len() + 4);
    let mut changed = false;
    for span in line.spans.drain(..) {
        // Split into up to three pieces: [span.start..start), [start..end),
        // [end..span.end), keeping only the slices that exist.
        let s = span.start;
        let e = span.end;
        if e <= start || s >= end {
            // Entirely outside the mark range — keep as-is.
            new_spans.push(span);
            continue;
        }
        // Left piece (untouched).
        if s < start {
            let mut left = span.clone();
            left.end = start;
            new_spans.push(left);
        }
        // Middle piece (the part covered by the mark range).
        let mid_start = s.max(start);
        let mid_end = e.min(end);
        let mut mid = span.clone();
        mid.start = mid_start;
        mid.end = mid_end;
        if apply_mark_to_span(&mut mid, mark, action) {
            changed = true;
        }
        new_spans.push(mid);
        // Right piece (untouched).
        if e > end {
            let mut right = span.clone();
            right.start = end;
            new_spans.push(right);
        }
    }

    // Merge adjacent spans that share identical attributes.
    let merged = merge_adjacent(new_spans);
    line.spans = merged;
    changed
}

/// Apply `mark` to a single span according to `action`. Returns `true`
/// if any field actually changed.
fn apply_mark_to_span(span: &mut TextSpan, mark: &Mark, action: MarkAction) -> bool {
    let set = matches!(action, MarkAction::Set);
    match mark {
        Mark::Bold => {
            if span.bold != set {
                span.bold = set;
                return true;
            }
        }
        Mark::Italic => {
            if span.italic != set {
                span.italic = set;
                return true;
            }
        }
        Mark::Underline => {
            if span.underline != set {
                span.underline = set;
                return true;
            }
        }
        Mark::Strikethrough => {
            if span.strikethrough != set {
                span.strikethrough = set;
                return true;
            }
        }
        Mark::Code => {
            if span.code != set {
                span.code = set;
                return true;
            }
        }
        Mark::Color(c) => {
            if !color_eq(span.color, *c) {
                span.color = *c;
                return true;
            }
        }
        Mark::Link(url) => {
            if span.link_url != *url {
                span.link_url = url.clone();
                // Mirror standard editor behaviour: links carry an
                // underline. Clearing a link doesn't strip the
                // underline (the user can do that explicitly).
                if url.is_some() {
                    span.underline = true;
                }
                return true;
            }
        }
    }
    false
}

fn color_eq(a: Color, b: Color) -> bool {
    (a.r - b.r).abs() < 1e-3
        && (a.g - b.g).abs() < 1e-3
        && (a.b - b.b).abs() < 1e-3
        && (a.a - b.a).abs() < 1e-3
}

/// Merge consecutive spans with identical attributes into single runs.
fn merge_adjacent(spans: Vec<TextSpan>) -> Vec<TextSpan> {
    let mut out: Vec<TextSpan> = Vec::with_capacity(spans.len());
    for span in spans {
        if span.start >= span.end {
            continue;
        }
        if let Some(last) = out.last_mut() {
            if last.end == span.start && spans_format_match(last, &span) {
                last.end = span.end;
                continue;
            }
        }
        out.push(span);
    }
    out
}

fn spans_format_match(a: &TextSpan, b: &TextSpan) -> bool {
    a.bold == b.bold
        && a.italic == b.italic
        && a.underline == b.underline
        && a.strikethrough == b.strikethrough
        && a.code == b.code
        && color_eq(a.color, b.color)
        && a.link_url == b.link_url
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::rich_text_editor::document::Block;
    use blinc_core::Color;

    fn doc_one_para(text: &str) -> RichDocument {
        RichDocument::from_blocks(vec![Block::paragraph(text, Color::WHITE)])
    }

    fn sel(b1: usize, l1: usize, c1: usize, b2: usize, l2: usize, c2: usize) -> Selection {
        Selection {
            anchor: DocPosition::new(b1, l1, c1),
            head: DocPosition::new(b2, l2, c2),
        }
    }

    #[test]
    fn empty_selection_is_no_op() {
        let mut d = doc_one_para("hello");
        let pos = DocPosition::new(0, 0, 2);
        let changed = apply_mark_to_selection(
            &mut d,
            Selection {
                anchor: pos,
                head: pos,
            },
            Mark::Bold,
        );
        assert!(!changed);
    }

    #[test]
    fn bold_first_word() {
        let mut d = doc_one_para("hello world");
        apply_mark_to_selection(&mut d, sel(0, 0, 0, 0, 0, 5), Mark::Bold);
        let line = &d.blocks[0].lines[0];
        // Expect a bold span over [0, 5) and a non-bold span over [5, 11).
        let bold = line.spans.iter().find(|s| s.bold).unwrap();
        assert_eq!(bold.start, 0);
        assert_eq!(bold.end, 5);
        assert!(
            line.spans
                .iter()
                .any(|s| !s.bold && s.start == 5 && s.end == 11)
        );
    }

    #[test]
    fn toggle_bold_clears_when_already_bold() {
        let mut d = doc_one_para("hello");
        apply_mark_to_selection(&mut d, sel(0, 0, 0, 0, 0, 5), Mark::Bold);
        // Now the whole word is bold; toggling it should clear it.
        let changed = apply_mark_to_selection(&mut d, sel(0, 0, 0, 0, 0, 5), Mark::Bold);
        assert!(changed);
        assert!(d.blocks[0].lines[0].spans.iter().all(|s| !s.bold));
    }

    #[test]
    fn italic_middle_of_word_splits_spans() {
        let mut d = doc_one_para("abcdef");
        apply_mark_to_selection(&mut d, sel(0, 0, 2, 0, 0, 4), Mark::Italic);
        let spans = &d.blocks[0].lines[0].spans;
        // Expect three spans: [0..2) plain, [2..4) italic, [4..6) plain.
        let italic_count = spans.iter().filter(|s| s.italic).count();
        assert_eq!(italic_count, 1);
        let i = spans.iter().find(|s| s.italic).unwrap();
        assert_eq!(i.start, 2);
        assert_eq!(i.end, 4);
    }

    #[test]
    fn merge_adjacent_after_toggle() {
        let mut d = doc_one_para("ab");
        // Bold each char individually.
        apply_mark_to_selection(&mut d, sel(0, 0, 0, 0, 0, 1), Mark::Bold);
        apply_mark_to_selection(&mut d, sel(0, 0, 1, 0, 0, 2), Mark::Bold);
        // Spans should have merged to a single bold span over [0, 2).
        let spans = &d.blocks[0].lines[0].spans;
        let bold_runs: Vec<_> = spans.iter().filter(|s| s.bold).collect();
        assert_eq!(bold_runs.len(), 1);
        assert_eq!(bold_runs[0].start, 0);
        assert_eq!(bold_runs[0].end, 2);
    }

    #[test]
    fn color_change_overrides_existing_color() {
        let mut d = doc_one_para("hello");
        let red = Color::rgba(1.0, 0.0, 0.0, 1.0);
        apply_mark_to_selection(&mut d, sel(0, 0, 0, 0, 0, 5), Mark::Color(red));
        let line = &d.blocks[0].lines[0];
        for span in &line.spans {
            if span.start < 5 {
                assert!(
                    color_eq(span.color, red),
                    "expected red, got {:?}",
                    span.color
                );
            }
        }
    }

    #[test]
    fn link_sets_url_and_underline() {
        let mut d = doc_one_para("click");
        apply_mark_to_selection(
            &mut d,
            sel(0, 0, 0, 0, 0, 5),
            Mark::Link(Some("https://example.com".to_string())),
        );
        let line = &d.blocks[0].lines[0];
        let linked = line
            .spans
            .iter()
            .find(|s| s.link_url.as_deref() == Some("https://example.com"))
            .expect("link span exists");
        assert!(linked.underline);
        assert_eq!(linked.start, 0);
        assert_eq!(linked.end, 5);
    }

    #[test]
    fn link_clear_removes_url() {
        let mut d = doc_one_para("click");
        apply_mark_to_selection(
            &mut d,
            sel(0, 0, 0, 0, 0, 5),
            Mark::Link(Some("https://example.com".to_string())),
        );
        apply_mark_to_selection(&mut d, sel(0, 0, 0, 0, 0, 5), Mark::Link(None));
        let line = &d.blocks[0].lines[0];
        assert!(line.spans.iter().all(|s| s.link_url.is_none()));
    }

    #[test]
    fn multi_line_selection_marks_all_lines() {
        // Two-block doc, select across the boundary.
        let mut d = RichDocument::from_blocks(vec![
            Block::paragraph("foo bar", Color::WHITE),
            Block::paragraph("baz qux", Color::WHITE),
        ]);
        apply_mark_to_selection(&mut d, sel(0, 0, 4, 1, 0, 3), Mark::Bold);
        // First block's [4..7) should be bold.
        let line0 = &d.blocks[0].lines[0];
        let bold0 = line0
            .spans
            .iter()
            .find(|s| s.bold && s.start == 4)
            .expect("bold run on first line");
        assert_eq!(bold0.end, 7);
        // Second block's [0..3) should be bold.
        let line1 = &d.blocks[1].lines[0];
        let bold1 = line1
            .spans
            .iter()
            .find(|s| s.bold && s.start == 0)
            .expect("bold run on second line");
        assert_eq!(bold1.end, 3);
    }
}
