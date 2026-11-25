//! Word-wrap a `StyledLine` into multiple lines that each fit a target width.
//!
//! This is a *build-time* wrap: we walk word boundaries inside the source
//! line, measure each candidate prefix in the faces of its spans, and emit a new `StyledLine` whenever adding the next word would
//! overflow `max_width`.
//!
//! Why pre-wrap instead of letting the styled-text render path do it: the
//! styled-text path in `blinc_app::context` walks segments left-to-right
//! at a fixed `y` and never breaks lines, so anything that doesn't fit
//! overflows the container on the right. Pre-wrapping is fully contained
//! in the editor renderer and doesn't require touching the shared text or
//! styled-text rendering code.
//!
//! Each output line preserves the original spans, sliced to the new line's
//! byte range and rebased so `start` is `0`-relative. A span that straddles
//! a wrap point becomes two spans, one on each line.

use blinc_layout::div::FontWeight;
use blinc_layout::styled_text::{StyledLine, TextSpan};

use super::line::{LineFont, width_of};

/// One visual line produced by [`wrap_styled_line`].
///
/// `line` is the wrapped chunk (with spans rebased to start at 0). The
/// other fields tell the caller how the chunk maps back to the source
/// line, which the editor's hit-tester needs to recover document
/// positions from clicks.
#[derive(Clone, Debug)]
pub struct WrappedLine {
    /// The wrapped chunk.
    pub line: StyledLine,
    /// Character column of `line.text[0]` in the source line.
    pub source_start_col: usize,
    /// Character column one past the end of `line.text` in the source
    /// (after any trimmed trailing whitespace).
    pub source_end_col: usize,
}

/// Wrap `line` into one or more `StyledLine`s that each fit within
/// `max_width` pixels. Span attributes are preserved across the wrap.
///
/// `font_size`, `weight`, and `italic` are the block's font; bold, italic and code spans are
/// measured in their own.
///
/// `max_width <= 0.0` is treated as "no wrapping" and returns the input
/// unchanged.
pub fn wrap_styled_line(
    line: &StyledLine,
    max_width: f32,
    font_size: f32,
    weight: FontWeight,
    italic: bool,
) -> Vec<StyledLine> {
    wrap_styled_line_with_offsets(line, max_width, font_size, weight, italic)
        .into_iter()
        .map(|w| w.line)
        .collect()
}

/// Like [`wrap_styled_line`] but returns each chunk with its char-column
/// offsets back into the source line. The editor's hit-tester uses this
/// to recover `(block, line, col)` from a click position.
pub fn wrap_styled_line_with_offsets(
    line: &StyledLine,
    max_width: f32,
    font_size: f32,
    weight: FontWeight,
    italic: bool,
) -> Vec<WrappedLine> {
    if line.text.is_empty() {
        return vec![WrappedLine {
            line: line.clone(),
            source_start_col: 0,
            source_end_col: 0,
        }];
    }
    if max_width <= 0.0 {
        let total_chars = line.text.chars().count();
        return vec![WrappedLine {
            line: line.clone(),
            source_start_col: 0,
            source_end_col: total_chars,
        }];
    }

    let font = LineFont {
        size: font_size,
        weight,
        italic,
    };

    // Fast path: whole line fits.
    let total = width_of(line, 0..line.text.len(), &font);
    if total <= max_width {
        let total_chars = line.text.chars().count();
        return vec![WrappedLine {
            line: line.clone(),
            source_start_col: 0,
            source_end_col: total_chars,
        }];
    }

    // Walk word boundaries. We treat any whitespace (space, tab, ideographic
    // space, etc.) as a break opportunity and preserve runs of internal
    // whitespace inside words by splitting at the *trailing* whitespace
    // following each word.
    let words = collect_words(&line.text);
    if words.is_empty() {
        return vec![WrappedLine {
            line: line.clone(),
            source_start_col: 0,
            source_end_col: line.text.chars().count(),
        }];
    }

    let mut out: Vec<WrappedLine> = Vec::new();
    // Byte range of the current visual line within line.text.
    let mut line_start: usize = words[0].start;
    // The byte index just past the last word currently in the line.
    let mut line_end: usize = line_start;

    for word in &words {
        // Candidate end if we add this word: extend to word.end (no trailing
        // whitespace) for measurement, so trailing space at line end
        // doesn't push us over max_width.
        let candidate_end = word.end;
        let candidate_width = width_of(line, line_start..candidate_end, &font);

        if candidate_width > max_width && line_end > line_start {
            // Doesn't fit. Emit the current line (up to line_end) and
            // start a fresh line beginning with this word.
            out.push(make_wrapped(line, line_start..line_end));
            line_start = word.start;
            line_end = word.end_with_trailing;
        } else {
            // Fits — extend current line to include this word and its
            // trailing whitespace.
            line_end = word.end_with_trailing;
        }
    }

    // Emit the trailing line.
    if line_end > line_start {
        out.push(make_wrapped(line, line_start..line_end));
    }

    if out.is_empty() {
        return vec![WrappedLine {
            line: line.clone(),
            source_start_col: 0,
            source_end_col: line.text.chars().count(),
        }];
    }
    out
}

fn make_wrapped(source: &StyledLine, range: std::ops::Range<usize>) -> WrappedLine {
    // Translate byte range to char columns in the source.
    let start_col = source.text[..range.start].chars().count();
    let end_col = source.text[..range.end].chars().count();
    let line = slice(source, range);
    WrappedLine {
        line,
        source_start_col: start_col,
        source_end_col: end_col,
    }
}

/// A word inside the source text plus its trailing whitespace, all in byte
/// indices.
struct Word {
    /// Inclusive start byte of the word.
    start: usize,
    /// Exclusive end byte of the word (no trailing whitespace).
    end: usize,
    /// Exclusive end byte including any trailing whitespace.
    end_with_trailing: usize,
}

/// Tokenise `text` into words plus trailing whitespace runs.
///
/// Leading whitespace before the first word is attached to the first word
/// (so it shows up at the start of the first wrapped line).
fn collect_words(text: &str) -> Vec<Word> {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut words = Vec::new();
    let mut i = 0usize;

    // Skip leading whitespace but remember its start so it sticks to the
    // first word.
    let initial_start = i;
    while i < len {
        let ch = char_at(text, i);
        if !ch.is_whitespace() {
            break;
        }
        i += ch.len_utf8();
    }

    while i < len {
        // Word body — everything up to the next whitespace run.
        let word_start = if words.is_empty() { initial_start } else { i };
        let body_start = i;
        while i < len {
            let ch = char_at(text, i);
            if ch.is_whitespace() {
                break;
            }
            i += ch.len_utf8();
        }
        let word_end = i;
        if word_end == body_start && words.is_empty() && initial_start == word_start {
            // Pure-whitespace input — bail.
            break;
        }
        // Trailing whitespace run.
        while i < len {
            let ch = char_at(text, i);
            if !ch.is_whitespace() {
                break;
            }
            i += ch.len_utf8();
        }
        let trailing_end = i;
        words.push(Word {
            start: word_start,
            end: word_end,
            end_with_trailing: trailing_end,
        });
    }

    words
}

/// Helper that decodes the char at byte index `i` of `text`. Caller must
/// guarantee `i` is on a UTF-8 boundary, which our walks always do.
fn char_at(text: &str, i: usize) -> char {
    text[i..].chars().next().unwrap_or('\0')
}

/// Build a `StyledLine` covering byte range `range` of `source`,
/// rebasing span byte indices so the new line starts at 0.
///
/// Trailing whitespace is intentionally **kept** in the wrapped chunk:
/// the editor's cursor / hit-test code (`state::cursor_geometry`,
/// `state::position_from_click`) sums `g.start.col + g.text.chars().count()`
/// to recover the source `(block, line, col)` of every visual line.
/// Trimming would make `g.text.chars().count()` smaller than the
/// actual source range covered by this chunk, which causes
/// off-by-N cursor positioning whenever a wrap point lands at a word
/// boundary with trailing whitespace.
fn slice(source: &StyledLine, range: std::ops::Range<usize>) -> StyledLine {
    let text = source.text[range.clone()].to_string();
    let mut spans = Vec::new();
    for span in &source.spans {
        // Intersect [span.start, span.end) with `range`.
        let s = span.start.max(range.start);
        let e = span.end.min(range.end);
        if s >= e {
            continue;
        }
        // Rebase to local coords inside the new line.
        let local_start = s - range.start;
        let local_end = e - range.start;
        spans.push(TextSpan {
            start: local_start,
            end: local_end,
            color: span.color,
            bold: span.bold,
            italic: span.italic,
            underline: span.underline,
            strikethrough: span.strikethrough,
            code: span.code,
            link_url: span.link_url.clone(),
            token_type: span.token_type.clone(),
        });
    }
    StyledLine { text, spans }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blinc_core::Color;

    fn plain(text: &str) -> StyledLine {
        StyledLine::plain(text, Color::WHITE)
    }

    #[test]
    fn short_line_returns_unchanged() {
        let line = plain("hello world");
        let wrapped = wrap_styled_line(&line, 10000.0, 14.0, FontWeight::Normal, false);
        assert_eq!(wrapped.len(), 1);
        assert_eq!(wrapped[0].text, "hello world");
    }

    #[test]
    fn empty_line_returns_unchanged() {
        let line = plain("");
        let wrapped = wrap_styled_line(&line, 100.0, 14.0, FontWeight::Normal, false);
        assert_eq!(wrapped.len(), 1);
        assert_eq!(wrapped[0].text, "");
    }

    #[test]
    fn zero_max_width_disables_wrap() {
        let line = plain("hello world");
        let wrapped = wrap_styled_line(&line, 0.0, 14.0, FontWeight::Normal, false);
        assert_eq!(wrapped.len(), 1);
    }

    #[test]
    fn long_line_wraps_into_multiple_lines() {
        // EstimatedTextMeasurer uses ~0.55 * font_size per char. With
        // font_size = 14, each char is ~7.7px. 30 chars = ~231px. Max
        // width 60px → ~7-8 chars per line.
        let line = plain("one two three four five six seven eight nine ten");
        let wrapped = wrap_styled_line(&line, 60.0, 14.0, FontWeight::Normal, false);
        assert!(wrapped.len() > 1);
        // Round-trip: every word should appear in some output line.
        let joined: String = wrapped
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        for word in [
            "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
        ] {
            assert!(
                joined.contains(word),
                "missing word `{}` in `{}`",
                word,
                joined
            );
        }
    }

    #[test]
    fn span_split_across_wrap_points() {
        // Build a line where a single bold span covers the entire text.
        // After wrapping, each output line should have exactly one bold
        // span covering its full text.
        let mut line = plain("the quick brown fox jumps over the lazy dog");
        line.spans = vec![TextSpan::new(0, line.text.len(), Color::WHITE, true)];

        let wrapped = wrap_styled_line(&line, 60.0, 14.0, FontWeight::Normal, false);
        assert!(wrapped.len() > 1);
        for l in &wrapped {
            assert_eq!(l.spans.len(), 1, "expected one span per line");
            assert_eq!(l.spans[0].start, 0);
            assert_eq!(l.spans[0].end, l.text.len());
            assert!(l.spans[0].bold);
        }
    }

    #[test]
    fn very_long_word_emits_its_own_line() {
        // A single word wider than max_width should still appear on its
        // own line (overflowing) rather than dropping content.
        let line = plain("supercalifragilisticexpialidocious");
        let wrapped = wrap_styled_line(&line, 30.0, 14.0, FontWeight::Normal, false);
        let joined: String = wrapped.iter().map(|l| l.text.as_str()).collect();
        assert!(joined.contains("supercalifragilisticexpialidocious"));
    }

    #[test]
    fn span_only_on_first_word_stays_on_first_line() {
        let mut line = plain("alpha beta gamma delta epsilon zeta");
        // Bold only "alpha"
        line.spans = vec![
            TextSpan::new(0, 5, Color::WHITE, true),
            TextSpan::colored(5, line.text.len(), Color::WHITE),
        ];
        let wrapped = wrap_styled_line(&line, 60.0, 14.0, FontWeight::Normal, false);
        // The bold span should appear in the first wrapped line only.
        assert!(wrapped[0].spans.iter().any(|s| s.bold));
        for l in &wrapped[1..] {
            assert!(!l.spans.iter().any(|s| s.bold));
        }
    }
}
