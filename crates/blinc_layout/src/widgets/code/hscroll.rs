//! Horizontal scrolling of the code area. The gutter stays put: the code area shifts left by
//! `scroll_x` inside a clip, and clicks, drags and the caret count it.

use super::CodeEditorData;

impl CodeEditorData {
    /// The width of the code area the editor shows, from its last known bounds.
    fn code_viewport_width(&self) -> f32 {
        let gutter = if self.config.line_numbers || self.config.code_folding {
            self.config.gutter_width
        } else {
            0.0
        };
        let minimap = if self.config.minimap {
            self.config.minimap_width
        } else {
            0.0
        };
        (self.editor_bounds.2 - gutter - minimap).max(0.0)
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

    /// Keep `scroll_x` between the start of the lines and the end of the widest.
    pub(super) fn clamp_scroll_x(&mut self) {
        let viewport = self.code_viewport_width();
        let max = if viewport > 0.0 {
            (self.content_width() - viewport).max(0.0)
        } else {
            0.0
        };
        self.scroll_x = self.scroll_x.clamp(0.0, max);
    }

    /// Scroll the code area by `dx` pixels. Whether it moved.
    pub(super) fn scroll_x_by(&mut self, dx: f32) -> bool {
        let before = self.scroll_x;
        self.scroll_x += dx;
        self.clamp_scroll_x();
        (self.scroll_x - before).abs() > 0.01
    }

    /// Scroll so the caret column is inside the code area, with a margin.
    pub(super) fn keep_caret_in_view_x(&mut self) {
        let viewport = self.code_viewport_width();
        if viewport <= 0.0 || self.cursor.line >= self.lines.len() {
            return;
        }
        let before: String = self.lines[self.cursor.line]
            .chars()
            .take(self.cursor.column)
            .collect();
        let caret = self.measure_mono(&before) + self.config.padding;
        let margin = (self.config.font_size * 2.0).min(viewport / 3.0);
        if caret - self.scroll_x > viewport - margin {
            self.scroll_x = caret - viewport + margin;
        } else if caret - self.scroll_x < margin {
            self.scroll_x = caret - margin;
        }
        self.clamp_scroll_x();
    }
}
