//! Pointer selection in character, word and logical-line units.

use super::*;

#[derive(Clone, Default)]
pub(super) struct ClickSequence {
    last: Option<(web_time::Instant, (f32, f32), u8)>,
}

impl ClickSequence {
    pub(super) fn register(&mut self, point: (f32, f32), now: web_time::Instant) -> u8 {
        let count = self.last.map_or(1, |(previous, (x, y), count)| {
            let nearby = (point.0 - x).powi(2) + (point.1 - y).powi(2) <= 25.0;
            let timely = now
                .checked_duration_since(previous)
                .is_some_and(|elapsed| elapsed < std::time::Duration::from_millis(400));
            if nearby && timely && count < 3 {
                count + 1
            } else {
                1
            }
        });
        self.last = Some((now, point, count));
        count
    }
}

#[derive(Clone, Copy)]
enum SelectionUnit {
    Character,
    Word,
    Line,
}

#[derive(Clone, Copy)]
pub(super) struct DragSelection {
    start: TextPosition,
    end: TextPosition,
    unit: SelectionUnit,
}

impl TextAreaState {
    pub(super) fn select_pointer(
        &mut self,
        position: TextPosition,
        point: (f32, f32),
        shift: bool,
        touch: bool,
    ) -> u8 {
        let (count, start, end, unit) = if shift && !touch {
            self.clicks = Default::default();
            let anchor = self.selection_start.unwrap_or(self.cursor);
            (1, anchor, position, SelectionUnit::Character)
        } else {
            let count = self.clicks.register(point, web_time::Instant::now());
            let unit = match count {
                2 => SelectionUnit::Word,
                3 => SelectionUnit::Line,
                _ => SelectionUnit::Character,
            };
            let (start, end) = self.selection_range(position, unit);
            (count, start, end, unit)
        };
        self.selection_start = (start != end).then_some(start);
        self.cursor = end;
        self.drag_selection = (!touch).then_some(DragSelection {
            start,
            // A Shift-click keeps its original anchor when the drag reverses.
            end: if shift { start } else { end },
            unit,
        });
        count
    }

    pub(super) fn select_word_at(&mut self, position: TextPosition) {
        let (start, end) = self.selection_range(position, SelectionUnit::Word);
        self.selection_start = (start != end).then_some(start);
        self.cursor = end;
        self.reset_cursor_blink();
    }

    pub(super) fn extend_pointer_selection(&mut self, position: TextPosition) -> bool {
        let Some(anchor) = self.drag_selection else {
            return false;
        };
        let (start, end) = self.selection_range(position, anchor.unit);
        let before = |a: TextPosition, b: TextPosition| (a.line, a.column) < (b.line, b.column);
        let (fixed, cursor) = if before(start, anchor.start) {
            (anchor.end, start)
        } else if before(anchor.end, end) {
            (anchor.start, end)
        } else {
            (anchor.start, anchor.end)
        };
        let selection = (fixed != cursor).then_some(fixed);
        let changed = self.cursor != cursor || self.selection_start != selection;
        self.cursor = cursor;
        self.selection_start = selection;
        if changed {
            self.reset_cursor_blink();
        }
        changed
    }

    fn selection_range(
        &self,
        position: TextPosition,
        unit: SelectionUnit,
    ) -> (TextPosition, TextPosition) {
        let line = position.line.min(self.lines.len().saturating_sub(1));
        let text = self.lines.get(line).map(String::as_str).unwrap_or_default();
        let len = text.chars().count();
        match unit {
            SelectionUnit::Character => {
                let position = TextPosition::new(line, position.column.min(len));
                (position, position)
            }
            SelectionUnit::Word => {
                let column = position.column.min(len.saturating_sub(1));
                let (start, end) = crate::widgets::text_edit::word_at_position(text, column);
                (TextPosition::new(line, start), TextPosition::new(line, end))
            }
            SelectionUnit::Line => {
                let end = if line + 1 < self.lines.len() {
                    TextPosition::new(line + 1, 0)
                } else {
                    TextPosition::new(line, len)
                };
                (TextPosition::new(line, 0), end)
            }
        }
    }
}
