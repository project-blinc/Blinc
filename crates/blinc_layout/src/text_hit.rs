//! Pointer targets inside a text element, addressed by byte range.
//!
//! A link occupies part of a paragraph, so the paragraph node cannot answer
//! "what is under the pointer?" with a single cursor. It could once publish
//! x-ranges instead, but a rect is only meaningful at a known width: the same
//! markup wrapped into 200px and into 900px puts its links in different places,
//! and on different lines. Width is not known until layout has run, which is
//! after the element is built.
//!
//! So the element publishes what it knows without a width, byte ranges over its
//! own content, and the rects are derived on demand from a width the caller
//! supplies. The cursor query and the click handler both go through
//! [`TextHitSpans::hit`], which is what keeps them agreeing.

use std::sync::Arc;

use crate::element::CursorStyle;
use crate::text_measure::{TextLayoutOptions, measure_text_with_options, text_line_spans};

/// A range of text that answers the pointer differently to its surroundings
#[derive(Debug, Clone, PartialEq)]
pub struct TextHitSpan {
    /// Byte offset into the owning element's content
    pub start: usize,
    /// Byte offset one past the end
    pub end: usize,
    /// Cursor to show over this range
    pub cursor: CursorStyle,
    /// URL to open when the range is clicked, if it is a link
    pub url: Option<Arc<str>>,
}

/// A hit span placed at a width, in element-local coordinates
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HitRect {
    /// Left edge
    pub x0: f32,
    /// Top edge
    pub y0: f32,
    /// Right edge
    pub x1: f32,
    /// Bottom edge
    pub y1: f32,
    /// Index of the span in [`TextHitSpans::spans`] that produced this rect
    pub span: usize,
}

impl HitRect {
    /// Whether a point in element-local space falls inside this rect
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x0 && x <= self.x1 && y >= self.y0 && y <= self.y1
    }
}

/// Everything a text element knows about its pointer targets before layout
///
/// Cheap to clone: the content and spans are shared, and nothing is measured
/// until a width arrives.
#[derive(Debug, Clone)]
pub struct TextHitSpans {
    /// The element's full plain-text content, which span offsets index
    pub content: Arc<str>,
    /// Font size the content is laid out at
    pub font_size: f32,
    /// Line height multiplier
    pub line_height: f32,
    /// Font selection and spacing, minus any width
    pub options: TextLayoutOptions,
    /// The spans themselves
    pub spans: Arc<[TextHitSpan]>,
}

impl TextHitSpans {
    /// Place every span at `width`, in element-local coordinates
    ///
    /// A span crossing a wrap point yields one rect per line it covers, so a
    /// link broken across two lines is hit on both halves and on neither of
    /// the gaps beside them.
    pub fn rects(&self, width: f32) -> Vec<HitRect> {
        if self.spans.is_empty() || self.content.is_empty() {
            return Vec::new();
        }

        let mut wrap_options = self.options.clone();
        wrap_options.max_width = (width > 0.0).then_some(width);
        let lines = text_line_spans(&self.content, self.font_size, &wrap_options);

        // Prefix widths are measured unwrapped: the substring being measured
        // is already known to fit on one line.
        let mut run_options = self.options.clone();
        run_options.max_width = None;

        let line_height_px = crate::text_measure::line_height_px(self.font_size, &self.options);
        let mut rects = Vec::new();

        for (row, line) in lines.iter().enumerate() {
            for (index, span) in self.spans.iter().enumerate() {
                let start = span.start.max(line.start);
                let end = span.end.min(line.end);
                if start >= end || !self.is_measurable(line.start, start, end) {
                    continue;
                }

                let x0 = self.run_width(line.start, start, &run_options);
                let x1 = self.run_width(line.start, end, &run_options);
                let y0 = row as f32 * line_height_px;

                rects.push(HitRect {
                    x0,
                    y0,
                    x1,
                    y1: y0 + line_height_px,
                    span: index,
                });
            }
        }

        rects
    }

    /// The span under a point in element-local space, if any
    pub fn hit(&self, x: f32, y: f32, width: f32) -> Option<&TextHitSpan> {
        self.rects(width)
            .into_iter()
            .find(|rect| rect.contains(x, y))
            .and_then(|rect| self.spans.get(rect.span))
    }

    /// Whether the three offsets are usable slice boundaries
    ///
    /// Line starts come from glyph clusters and span offsets from a markup
    /// parser. Both should land on character boundaries, but slicing on a bad
    /// one panics, and a mis-parsed span is not worth a crash.
    fn is_measurable(&self, line_start: usize, start: usize, end: usize) -> bool {
        line_start <= start
            && end <= self.content.len()
            && self.content.is_char_boundary(line_start)
            && self.content.is_char_boundary(start)
            && self.content.is_char_boundary(end)
    }

    /// Width of `content[from..to]`
    fn run_width(&self, from: usize, to: usize, options: &TextLayoutOptions) -> f32 {
        if from >= to {
            return 0.0;
        }
        measure_text_with_options(&self.content[from..to], self.font_size, options).width
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONTENT: &str = "Press Enter to accept, or read the manual for the full list \
                           of shortcuts and their meanings in every mode.";

    fn spans() -> TextHitSpans {
        let link = CONTENT.find("manual").expect("link text");
        TextHitSpans {
            content: CONTENT.into(),
            font_size: 14.0,
            line_height: 1.2,
            options: TextLayoutOptions::new(),
            spans: vec![TextHitSpan {
                start: link,
                end: link + "manual".len(),
                cursor: CursorStyle::Pointer,
                url: Some("https://example.com".into()),
            }]
            .into(),
        }
    }

    #[test]
    fn a_narrow_container_places_the_link_differently() {
        assert_ne!(spans().rects(900.0), spans().rects(200.0));
    }

    #[test]
    fn no_rect_escapes_the_container() {
        for rect in spans().rects(200.0) {
            assert!(rect.x1 <= 200.0, "{rect:?} escapes a 200px container");
        }
    }

    #[test]
    fn a_wrapped_link_is_hit_on_the_line_it_landed_on() {
        let hit = spans();
        let rect = *hit.rects(200.0).first().expect("the link places a rect");
        let mid_x = (rect.x0 + rect.x1) / 2.0;
        let mid_y = (rect.y0 + rect.y1) / 2.0;

        assert!(hit.hit(mid_x, mid_y, 200.0).is_some());
        // Same x, a line above: outside the link.
        assert!(hit.hit(mid_x, rect.y0 - 1.0, 200.0).is_none());
    }

    #[test]
    fn text_outside_the_link_has_no_span() {
        assert!(spans().hit(0.0, 0.0, 900.0).is_none());
    }

    /// A link whose own text is long enough to break gets one rect per
    /// line it occupies, not one box spanning the gap between them.
    fn long_link() -> TextHitSpans {
        const TEXT: &str = "See the complete reference manual for keyboard shortcuts and \
                            editing commands before filing an issue.";
        let start = TEXT.find("complete").expect("link start");
        let end = TEXT.find(" before").expect("link end");
        TextHitSpans {
            content: TEXT.into(),
            font_size: 14.0,
            line_height: 1.2,
            options: TextLayoutOptions::new(),
            spans: vec![TextHitSpan {
                start,
                end,
                cursor: CursorStyle::Pointer,
                url: Some("https://example.com".into()),
            }]
            .into(),
        }
    }

    #[test]
    fn a_link_spanning_lines_gets_one_rect_per_line() {
        let hit = long_link();
        let rects = hit.rects(200.0);
        assert!(rects.len() > 1, "link crosses a wrap point: {rects:?}");

        // Every rect belongs to the same span, and each sits on its own line.
        assert!(rects.iter().all(|r| r.span == 0));
        let mut rows: Vec<f32> = rects.iter().map(|r| r.y0).collect();
        rows.dedup();
        assert_eq!(rows.len(), rects.len(), "one rect per line: {rects:?}");
    }

    #[test]
    fn every_line_of_a_multi_line_link_is_hoverable() {
        let hit = long_link();
        for rect in hit.rects(200.0) {
            let x = (rect.x0 + rect.x1) / 2.0;
            let y = (rect.y0 + rect.y1) / 2.0;
            assert!(
                hit.hit(x, y, 200.0).is_some(),
                "{rect:?} is drawn but not hittable",
            );
        }
    }

    /// The continuation line starts at the left edge, so a rect that kept
    /// the first line's x offset would leave the link's tail dead.
    #[test]
    fn a_continuation_line_starts_at_the_left_edge() {
        let rects = long_link().rects(200.0);
        let later = rects.iter().filter(|r| r.y0 > 0.0).collect::<Vec<_>>();
        assert!(!later.is_empty(), "expected a wrapped continuation");
        for rect in later {
            assert_eq!(rect.x0, 0.0, "{rect:?} should begin the line");
        }
    }

    #[test]
    fn an_empty_span_list_places_nothing() {
        let mut hit = spans();
        hit.spans = Vec::new().into();
        assert!(hit.rects(200.0).is_empty());
    }
}
