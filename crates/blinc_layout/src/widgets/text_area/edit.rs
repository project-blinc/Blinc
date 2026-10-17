//! Editing the text and moving the cursor.

use super::*;

impl TextAreaState {
    /// Insert text at cursor
    pub fn insert(&mut self, text: &str) {
        self.delete_selection();

        if text.contains('\n') {
            for (i, part) in text.split('\n').enumerate() {
                if i > 0 {
                    self.insert_newline_internal();
                }
                self.insert_text(part);
            }
        } else {
            self.insert_text(text);
        }
    }

    pub(super) fn insert_text(&mut self, text: &str) {
        let line_idx = self.cursor.line.min(self.lines.len().saturating_sub(1));
        let byte_pos = char_to_byte_pos(&self.lines[line_idx], self.cursor.column);
        self.lines[line_idx].insert_str(byte_pos, text);
        self.cursor.column += text.chars().count();
    }

    /// Insert a newline at cursor
    pub fn insert_newline(&mut self) {
        self.insert_newline_internal();
    }

    /// Internal newline insertion (no notify)
    pub(super) fn insert_newline_internal(&mut self) {
        self.delete_selection();

        let line_idx = self.cursor.line.min(self.lines.len().saturating_sub(1));
        let byte_pos = char_to_byte_pos(&self.lines[line_idx], self.cursor.column);

        let after = self.lines[line_idx].split_off(byte_pos);
        self.lines.insert(line_idx + 1, after);

        self.cursor.line += 1;
        self.cursor.column = 0;
    }

    /// Delete character before cursor (backspace)
    pub fn delete_backward(&mut self) {
        if self.delete_selection() {
            return;
        }

        if self.cursor.column > 0 {
            let start_byte =
                char_to_byte_pos(&self.lines[self.cursor.line], self.cursor.column - 1);
            let end_byte = char_to_byte_pos(&self.lines[self.cursor.line], self.cursor.column);
            self.lines[self.cursor.line].replace_range(start_byte..end_byte, "");
            self.cursor.column -= 1;
        } else if self.cursor.line > 0 {
            let current_line = self.lines.remove(self.cursor.line);
            self.cursor.line -= 1;
            self.cursor.column = self.lines[self.cursor.line].chars().count();
            self.lines[self.cursor.line].push_str(&current_line);
        }
    }

    /// Delete character after cursor (delete)
    pub fn delete_forward(&mut self) {
        if self.delete_selection() {
            return;
        }

        let line_len = self.lines[self.cursor.line].chars().count();
        if self.cursor.column < line_len {
            let start_byte = char_to_byte_pos(&self.lines[self.cursor.line], self.cursor.column);
            let end_byte = char_to_byte_pos(&self.lines[self.cursor.line], self.cursor.column + 1);
            self.lines[self.cursor.line].replace_range(start_byte..end_byte, "");
        } else if self.cursor.line < self.lines.len() - 1 {
            let next_line = self.lines.remove(self.cursor.line + 1);
            self.lines[self.cursor.line].push_str(&next_line);
        }
    }

    /// Delete selected text
    pub(super) fn delete_selection(&mut self) -> bool {
        if let Some(start) = self.selection_start {
            let (from, to) = self.order_positions(start, self.cursor);

            if from != to {
                if from.line == to.line {
                    let start_byte = char_to_byte_pos(&self.lines[from.line], from.column);
                    let end_byte = char_to_byte_pos(&self.lines[from.line], to.column);
                    self.lines[from.line].replace_range(start_byte..end_byte, "");
                } else {
                    let from_byte = char_to_byte_pos(&self.lines[from.line], from.column);
                    self.lines[from.line].truncate(from_byte);

                    let to_byte = char_to_byte_pos(&self.lines[to.line], to.column);
                    let after_text = self.lines[to.line][to_byte..].to_string();
                    self.lines[from.line].push_str(&after_text);

                    for _ in from.line + 1..=to.line {
                        if from.line + 1 < self.lines.len() {
                            self.lines.remove(from.line + 1);
                        }
                    }
                }

                self.cursor = from;
                self.selection_start = None;
                return true;
            }
        }
        self.selection_start = None;
        false
    }

    /// Order two positions (returns (earlier, later))
    pub(super) fn order_positions(
        &self,
        a: TextPosition,
        b: TextPosition,
    ) -> (TextPosition, TextPosition) {
        if a.line < b.line || (a.line == b.line && a.column <= b.column) {
            (a, b)
        } else {
            (b, a)
        }
    }

    /// Move cursor left
    pub fn move_left(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !select {
            if let Some(start) = self.selection_start {
                let (from, _) = self.order_positions(start, self.cursor);
                self.cursor = from;
                self.selection_start = None;
                return;
            }
        }

        if self.cursor.column > 0 {
            self.cursor.column -= 1;
        } else if self.cursor.line > 0 {
            self.cursor.line -= 1;
            self.cursor.column = self.lines[self.cursor.line].chars().count();
        }

        if !select {
            self.selection_start = None;
        }
    }

    /// Move cursor right
    pub fn move_right(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !select {
            if let Some(start) = self.selection_start {
                let (_, to) = self.order_positions(start, self.cursor);
                self.cursor = to;
                self.selection_start = None;
                return;
            }
        }

        let line_len = self.lines[self.cursor.line].chars().count();
        if self.cursor.column < line_len {
            self.cursor.column += 1;
        } else if self.cursor.line < self.lines.len() - 1 {
            self.cursor.line += 1;
            self.cursor.column = 0;
        }

        if !select {
            self.selection_start = None;
        }
    }

    /// Move cursor up (handles visual lines for wrapped text)
    pub fn move_up(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !select {
            self.selection_start = None;
        }

        // If we have visual lines, use them for navigation
        if !self.visual_lines.is_empty() && self.wrap_enabled {
            let current_visual_idx = self.visual_line_for_cursor();
            if current_visual_idx > 0 {
                // Move to the visual line above
                let prev_visual_idx = current_visual_idx - 1;
                let prev_vl = &self.visual_lines[prev_visual_idx];

                // Calculate cursor x position to maintain horizontal position
                let cursor_x = self.cursor_x_in_visual_line();

                // Find the column in the previous visual line that best matches our x position
                let column = self.find_column_at_x(prev_visual_idx, cursor_x);

                self.cursor.line = prev_vl.logical_line;
                self.cursor.column = column;
            }
        } else {
            // Fallback: simple logical line navigation
            if self.cursor.line > 0 {
                self.cursor.line -= 1;
                let line_len = self.lines[self.cursor.line].chars().count();
                self.cursor.column = self.cursor.column.min(line_len);
            }
        }
    }

    /// Move cursor down (handles visual lines for wrapped text)
    pub fn move_down(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !select {
            self.selection_start = None;
        }

        // If we have visual lines, use them for navigation
        if !self.visual_lines.is_empty() && self.wrap_enabled {
            let current_visual_idx = self.visual_line_for_cursor();
            if current_visual_idx < self.visual_lines.len() - 1 {
                // Move to the visual line below
                let next_visual_idx = current_visual_idx + 1;
                let next_vl = &self.visual_lines[next_visual_idx];

                // Calculate cursor x position to maintain horizontal position
                let cursor_x = self.cursor_x_in_visual_line();

                // Find the column in the next visual line that best matches our x position
                let column = self.find_column_at_x(next_visual_idx, cursor_x);

                self.cursor.line = next_vl.logical_line;
                self.cursor.column = column;
            }
        } else {
            // Fallback: simple logical line navigation
            if self.cursor.line < self.lines.len() - 1 {
                self.cursor.line += 1;
                let line_len = self.lines[self.cursor.line].chars().count();
                self.cursor.column = self.cursor.column.min(line_len);
            }
        }
    }

    /// Find the column in a visual line that best matches a given x position
    pub(super) fn find_column_at_x(&self, visual_line_idx: usize, target_x: f32) -> usize {
        if visual_line_idx >= self.visual_lines.len() {
            return 0;
        }

        let vl = &self.visual_lines[visual_line_idx];
        if vl.text.is_empty() {
            return vl.start_char;
        }

        let char_count = vl.text.chars().count();
        let mut best_pos = 0;
        let mut min_dist = f32::MAX;

        // Find character position that best matches target_x
        for i in 0..=char_count {
            let prefix: String = vl.text.chars().take(i).collect();
            let prefix_width = crate::text_measure::measure_text(&prefix, self.font_size).width;

            let dist = (prefix_width - target_x).abs();
            if dist < min_dist {
                min_dist = dist;
                best_pos = i;
            }
        }

        vl.start_char + best_pos
    }

    /// Move to start of line
    pub fn move_to_line_start(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !select {
            self.selection_start = None;
        }
        self.cursor.column = 0;
    }

    /// Move to end of line
    pub fn move_to_line_end(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !select {
            self.selection_start = None;
        }
        self.cursor.column = self.lines[self.cursor.line].chars().count();
    }

    /// Move to start of text
    pub fn move_to_start(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !select {
            self.selection_start = None;
        }
        self.cursor = TextPosition::new(0, 0);
    }

    /// Move to end of text
    pub fn move_to_end(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !select {
            self.selection_start = None;
        }
        let last_line = self.lines.len().saturating_sub(1);
        self.cursor = TextPosition::new(last_line, self.lines[last_line].chars().count());
    }

    /// Move cursor one word left
    pub fn move_word_left(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !select {
            self.selection_start = None;
        }
        if self.cursor.column == 0 && self.cursor.line > 0 {
            self.cursor.line -= 1;
            self.cursor.column = self.lines[self.cursor.line].chars().count();
        } else if self.cursor.line < self.lines.len() {
            self.cursor.column = crate::widgets::text_edit::word_boundary_left(
                &self.lines[self.cursor.line],
                self.cursor.column,
            );
        }
    }

    /// Move cursor one word right
    pub fn move_word_right(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !select {
            self.selection_start = None;
        }
        if self.cursor.line < self.lines.len() {
            let line_len = self.lines[self.cursor.line].chars().count();
            if self.cursor.column >= line_len && self.cursor.line + 1 < self.lines.len() {
                self.cursor.line += 1;
                self.cursor.column = 0;
            } else {
                self.cursor.column = crate::widgets::text_edit::word_boundary_right(
                    &self.lines[self.cursor.line],
                    self.cursor.column,
                );
            }
        }
    }

    /// Delete from cursor to the previous word boundary
    pub fn delete_word_backward(&mut self) {
        if self.selection_start.is_some() {
            self.delete_selection();
            return;
        }
        let saved = self.cursor;
        self.move_word_left(false);
        if self.cursor != saved {
            self.selection_start = Some(saved);
            self.delete_selection();
        }
    }

    /// Delete from cursor to the next word boundary
    pub fn delete_word_forward(&mut self) {
        if self.selection_start.is_some() {
            self.delete_selection();
            return;
        }
        let saved = self.cursor;
        self.move_word_right(false);
        if self.cursor != saved {
            self.selection_start = Some(saved);
            self.cursor = saved;
            // Swap: we want to delete forward, so select from cursor to word end
            self.selection_start = Some(self.cursor);
            self.move_word_right(false);
            let end = self.cursor;
            self.cursor = saved;
            self.selection_start = Some(end);
            self.delete_selection();
        }
    }

    /// Select all text
    pub fn select_all(&mut self) {
        self.selection_start = Some(TextPosition::new(0, 0));
        let last_line = self.lines.len().saturating_sub(1);
        self.cursor = TextPosition::new(last_line, self.lines[last_line].chars().count());
    }

    /// Get selected text
    pub fn selected_text(&self) -> Option<String> {
        self.selection_start.map(|start| {
            let (from, to) = self.order_positions(start, self.cursor);

            if from.line == to.line {
                self.lines[from.line]
                    .chars()
                    .skip(from.column)
                    .take(to.column - from.column)
                    .collect()
            } else {
                let mut result = String::new();
                result.extend(self.lines[from.line].chars().skip(from.column));

                for line in &self.lines[from.line + 1..to.line] {
                    result.push('\n');
                    result.push_str(line);
                }

                if to.line > from.line {
                    result.push('\n');
                    result.extend(self.lines[to.line].chars().take(to.column));
                }

                result
            }
        })
    }
}
