//! One visual line as runs at their pixel positions.
//!
//! The renderer draws the runs and the geometry measures them, so the text, the cursor and the
//! selection agree.

use std::ops::Range;

use blinc_layout::div::{FontFamily, FontWeight};
use blinc_layout::styled_text::StyledLine;

use super::document::BlockAlign;
use super::state::{RunGeometry, measure_width};
use super::wrap::WrappedLine;

/// The font of a block's text, before its spans.
#[derive(Clone, Copy)]
pub(super) struct LineFont {
    pub size: f32,
    pub weight: FontWeight,
    pub italic: bool,
}

/// Where a line is placed.
#[derive(Clone, Copy)]
pub(super) struct LineBox {
    pub width: f32,
    pub align: BlockAlign,
    /// The last visual line of its source line, which is not justified.
    pub last: bool,
}

/// What changes a run's glyphs.
#[derive(Clone, Copy, Default, PartialEq)]
struct Face {
    code: bool,
    bold: bool,
    italic: bool,
}

impl Face {
    fn family(self) -> FontFamily {
        if self.code {
            FontFamily::monospace()
        } else {
            FontFamily::default()
        }
    }

    /// The weight and slant of `font` in this face. Inline code is always plain.
    fn weight(self, font: &LineFont) -> (FontWeight, bool) {
        if self.code {
            (FontWeight::Normal, false)
        } else {
            let weight = if self.bold {
                FontWeight::Bold
            } else {
                font.weight
            };
            (weight, font.italic || self.italic)
        }
    }

    fn measure(self, text: &str, font: &LineFont) -> f32 {
        let (weight, italic) = self.weight(font);
        measure_width(text, font.size, weight, italic, Some(&self.family()))
    }
}

/// The parts of `range` of the line that share a face.
fn faces(line: &StyledLine, range: Range<usize>) -> Vec<(Range<usize>, Face)> {
    let mut cuts = vec![range.start, range.end];
    for span in &line.spans {
        cuts.extend(
            [span.start, span.end]
                .into_iter()
                .filter(|c| range.contains(c)),
        );
    }
    cuts.sort_unstable();
    cuts.dedup();
    let mut out: Vec<(Range<usize>, Face)> = Vec::new();
    for pair in cuts.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        let mut face = Face::default();
        for span in line
            .spans
            .iter()
            .filter(|s| s.start <= start && s.end >= end)
        {
            face.code |= span.code;
            face.bold |= span.bold;
            face.italic |= span.italic;
        }
        match out.last_mut() {
            Some((last, f)) if *f == face => last.end = end,
            _ => out.push((start..end, face)),
        }
    }
    out
}

/// The width of `range` of the line, each part in its own face.
pub(super) fn width_of(line: &StyledLine, range: Range<usize>, font: &LineFont) -> f32 {
    faces(line, range)
        .into_iter()
        .map(|(part, face)| face.measure(&line.text[part], font))
        .sum()
}

/// The runs of a visual line, each with the bytes of the line it covers, placed by the line's
/// alignment. There is always at least one, for the cursor.
pub(super) fn runs(
    visual: &WrappedLine,
    font: &LineFont,
    line_box: &LineBox,
) -> Vec<(Range<usize>, RunGeometry)> {
    let line = &visual.line;
    // The spaces a line wraps at hang past its end.
    let inked = line.text.trim_end().len();
    let slack = (line_box.width - width_of(line, 0..inked, font)).max(0.0);
    let gaps = gaps(&line.text[..inked]);
    let justified = line_box.align == BlockAlign::Justify && !line_box.last && !gaps.is_empty();
    let mut x = match line_box.align {
        BlockAlign::Start | BlockAlign::Justify => 0.0,
        BlockAlign::Center => slack * 0.5,
        BlockAlign::End => slack,
    };
    let mut parts = faces(line, 0..line.text.len());
    if justified {
        parts = split_at(parts, &gaps);
    }
    if parts.is_empty() {
        parts.push((0..0, Face::default()));
    }
    let per_gap = if justified {
        slack / gaps.len() as f32
    } else {
        0.0
    };
    let mut column = visual.source_start_col;
    parts
        .into_iter()
        .map(|(bytes, face)| {
            let text = &line.text[bytes.clone()];
            let (weight, italic) = face.weight(font);
            let mut width = if text.is_empty() {
                0.0
            } else {
                face.measure(text, font)
            };
            if gaps.contains(&bytes.end) {
                width += per_gap;
            }
            let run = RunGeometry {
                source_col: column,
                text: text.to_string(),
                x_in_line: x,
                width,
                font_family: face.family(),
                font_size: font.size,
                weight,
                italic,
            };
            x += width;
            column += text.chars().count();
            (bytes, run)
        })
        .collect()
}

/// The end of each run of spaces between words, where a justified line takes up its slack.
fn gaps(text: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut seen_word = false;
    let mut in_space = false;
    for (at, c) in text.char_indices() {
        if c.is_whitespace() {
            in_space = seen_word;
        } else {
            if in_space {
                out.push(at);
            }
            seen_word = true;
            in_space = false;
        }
    }
    out
}

fn split_at(parts: Vec<(Range<usize>, Face)>, cuts: &[usize]) -> Vec<(Range<usize>, Face)> {
    let mut out = Vec::new();
    for (part, face) in parts {
        let mut start = part.start;
        for &cut in cuts.iter().filter(|c| part.start < **c && **c < part.end) {
            out.push((start..cut, face));
            start = cut;
        }
        out.push((start..part.end, face));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use blinc_core::Color;
    use blinc_layout::styled_text::TextSpan;

    const FONT: LineFont = LineFont {
        size: 16.0,
        weight: FontWeight::Normal,
        italic: false,
    };

    fn visual(text: &str, spans: Vec<TextSpan>) -> WrappedLine {
        WrappedLine {
            line: StyledLine {
                text: text.into(),
                spans,
            },
            source_start_col: 0,
            source_end_col: text.chars().count(),
        }
    }

    fn placed(text: &str, align: BlockAlign, last: bool) -> Vec<(Range<usize>, RunGeometry)> {
        let line_box = LineBox {
            width: 400.0,
            align,
            last,
        };
        runs(&visual(text, Vec::new()), &FONT, &line_box)
    }

    fn right_edge(text: &str, runs: &[(Range<usize>, RunGeometry)]) -> f32 {
        let (bytes, last) = runs.last().unwrap();
        assert_eq!(bytes.end, text.len());
        last.x_in_line + last.width
    }

    #[test]
    fn a_line_is_placed_by_its_alignment() {
        let text = "some words";
        let width = width_of(&visual(text, Vec::new()).line, 0..text.len(), &FONT);
        let x = |align| placed(text, align, true)[0].1.x_in_line;
        assert_eq!(x(BlockAlign::Start), 0.0);
        assert!((x(BlockAlign::Center) - (400.0 - width) / 2.0).abs() < 0.01);
        assert!((x(BlockAlign::End) - (400.0 - width)).abs() < 0.01);
        // The space a line wrapped at hangs past the edge.
        let hung = placed("some words ", BlockAlign::End, false);
        assert!((hung[0].1.x_in_line - (400.0 - width)).abs() < 0.01);
    }

    #[test]
    fn an_empty_line_holds_the_cursor_where_text_would_go() {
        for (align, x) in [
            (BlockAlign::Start, 0.0),
            (BlockAlign::Center, 200.0),
            (BlockAlign::End, 400.0),
            (BlockAlign::Justify, 0.0),
        ] {
            let runs = placed("", align, true);
            assert_eq!((runs.len(), runs[0].1.x_in_line), (1, x));
        }
    }

    #[test]
    fn a_justified_line_reaches_both_edges_but_for_the_last() {
        let text = "three little words";
        let full = placed(text, BlockAlign::Justify, false);
        assert_eq!(full.len(), 3);
        assert_eq!(full[0].1.x_in_line, 0.0);
        assert!((right_edge(text, &full) - 400.0).abs() < 0.01);
        let last = placed(text, BlockAlign::Justify, true);
        assert_eq!(last.len(), 1);
        assert!(right_edge(text, &last) < 390.0);
        // One word has nowhere to put the slack.
        assert_eq!(placed("word", BlockAlign::Justify, false).len(), 1);
    }

    #[test]
    fn runs_cover_the_line_in_order_and_know_their_columns() {
        let mut bold = TextSpan::new(2, 6, Color::WHITE, true);
        bold.italic = true;
        let text = "é bold and code";
        let mut code = TextSpan::new(12, 16, Color::WHITE, false);
        code.code = true;
        let line = visual(text, vec![bold, code]);
        for align in [BlockAlign::Start, BlockAlign::Justify] {
            let line_box = LineBox {
                width: 400.0,
                align,
                last: false,
            };
            let runs = runs(&line, &FONT, &line_box);
            let joined: String = runs.iter().map(|(_, r)| r.text.as_str()).collect();
            assert_eq!(joined, text);
            let mut column = 0;
            for (bytes, run) in &runs {
                assert_eq!(&text[bytes.clone()], run.text);
                assert_eq!(run.source_col, column);
                column += run.text.chars().count();
            }
            assert!(runs.windows(2).all(|w| w[0].1.x_in_line < w[1].1.x_in_line));
            assert!(
                runs.iter()
                    .any(|(_, r)| r.weight == FontWeight::Bold && r.italic)
            );
        }
    }
}
