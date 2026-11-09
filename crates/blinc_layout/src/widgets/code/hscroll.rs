//! The code area's sideways scroll: its own horizontal scroll container, whose physics the
//! editor keeps across rebuilds, so the gutter stays and the offset survives an edit.

use super::CodeEditorData;

impl CodeEditorData {
    /// How far the code area is scrolled right, in pixels.
    pub(super) fn scroll_x(&self) -> f32 {
        self.h_scroll.lock().map(|p| -p.offset_x).unwrap_or(0.0)
    }

    /// The widest line, with the padding either side and room for the caret after it.
    pub(super) fn content_width(&mut self) -> f32 {
        let widest = self
            .lines
            .iter()
            .map(|line| line.chars().count())
            .max()
            .unwrap_or(0);
        self.measure_chars(widest) + self.config.padding * 2.0 + self.config.font_size
    }

    /// Scroll so the caret column is inside the code area, with a margin.
    pub(super) fn keep_caret_in_view_x(&mut self) {
        if self.cursor.line >= self.lines.len() {
            return;
        }
        let before: String = self.lines[self.cursor.line]
            .chars()
            .take(self.cursor.column)
            .collect();
        let caret = self.measure_mono(&before) + self.config.padding;
        let content = self.content_width();
        let Ok(mut p) = self.h_scroll.lock() else {
            return;
        };
        let viewport = p.viewport_width;
        if viewport <= 0.0 {
            return;
        }
        let margin = (self.config.font_size * 2.0).min(viewport / 3.0);
        let mut x = -p.offset_x;
        if caret - x > viewport - margin {
            x = caret - viewport + margin;
        } else if caret - x < margin {
            x = caret - margin;
        }
        p.offset_x = -x.clamp(0.0, (content - viewport).max(0.0));
    }
}
