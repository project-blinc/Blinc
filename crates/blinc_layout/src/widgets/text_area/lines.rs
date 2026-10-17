//! Visual lines: how the text wraps, and where a position falls on screen.

use super::*;

impl TextAreaState {
    /// Calculate the number of visual lines a text line takes when wrapped
    ///
    /// Returns 1 for short lines, more for lines that wrap.
    pub(super) fn visual_lines_for_text(text: &str, font_size: f32, available_width: f32) -> usize {
        if text.is_empty() || available_width <= 0.0 {
            return 1;
        }

        let metrics = crate::text_measure::measure_text(text, font_size);
        if metrics.width <= available_width {
            return 1;
        }

        // Estimate number of lines by dividing total width by available width
        // Add 1 because integer division rounds down
        ((metrics.width / available_width).ceil() as usize).max(1)
    }

    /// What the visual lines are laid out from: the text and the sizes.
    pub(super) fn layout_key(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::hash::DefaultHasher::new();
        self.lines.hash(&mut h);
        self.font_size.to_bits().hash(&mut h);
        self.available_width.to_bits().hash(&mut h);
        self.wrap_enabled.hash(&mut h);
        h.finish()
    }

    /// Compute visual lines for all text content
    ///
    /// This creates a list of VisualLine entries that map logical lines to
    /// visual lines, accounting for word wrapping. Each VisualLine contains:
    /// - The logical line index it belongs to
    /// - Start and end character indices within that logical line
    /// - The actual text content and its measured width
    ///
    /// Call this whenever:
    /// - Text content changes
    /// - Available width changes (e.g., container resize)
    /// - Font size changes
    pub fn compute_visual_lines(&mut self) {
        self.visual_lines.clear();

        let font_size = self.font_size;
        let available_width = self.available_width;
        let wrap_enabled = self.wrap_enabled;

        for (logical_line_idx, line_text) in self.lines.iter().enumerate() {
            if line_text.is_empty() {
                // Empty line still takes up one visual line
                self.visual_lines.push(VisualLine {
                    logical_line: logical_line_idx,
                    start_char: 0,
                    end_char: 0,
                    text: String::new(),
                    width: 0.0,
                });
                continue;
            }

            if !wrap_enabled || available_width <= 0.0 {
                // No wrapping - entire logical line is one visual line
                let width = crate::text_measure::measure_text(line_text, font_size).width;
                self.visual_lines.push(VisualLine {
                    logical_line: logical_line_idx,
                    start_char: 0,
                    end_char: line_text.chars().count(),
                    text: line_text.clone(),
                    width,
                });
                continue;
            }

            // Wrapping enabled - split line into visual lines
            // Use word-based wrapping: try to break at word boundaries
            let chars: Vec<char> = line_text.chars().collect();
            let char_count = chars.len();
            let mut start_char = 0;

            while start_char < char_count {
                // Find how many characters fit in available_width
                let mut end_char = start_char;
                let mut last_word_break = start_char;
                let mut current_width = 0.0;

                // Scan forward to find break point
                while end_char < char_count {
                    // Check width up to this character (inclusive)
                    let test_end = end_char + 1;
                    let test_text: String = chars[start_char..test_end].iter().collect();
                    let test_width = crate::text_measure::measure_text(&test_text, font_size).width;

                    if test_width > available_width && end_char > start_char {
                        // This character would exceed width, break here
                        break;
                    }

                    current_width = test_width;
                    end_char = test_end;

                    // Track word boundary (space character)
                    if end_char < char_count && chars[end_char - 1].is_whitespace() {
                        last_word_break = end_char;
                    }
                }

                // If we found a word break, use it (unless it's at the start)
                if last_word_break > start_char && end_char < char_count {
                    end_char = last_word_break;
                    let text: String = chars[start_char..end_char].iter().collect();
                    current_width = crate::text_measure::measure_text(&text, font_size).width;
                }

                // Create visual line
                let text: String = chars[start_char..end_char].iter().collect();
                self.visual_lines.push(VisualLine {
                    logical_line: logical_line_idx,
                    start_char,
                    end_char,
                    text,
                    width: current_width,
                });

                start_char = end_char;
            }

            // If line ended exactly at boundary, we should have at least one visual line
            if self.visual_lines.last().map(|vl| vl.logical_line) != Some(logical_line_idx) {
                self.visual_lines.push(VisualLine {
                    logical_line: logical_line_idx,
                    start_char: char_count,
                    end_char: char_count,
                    text: String::new(),
                    width: 0.0,
                });
            }
        }
        self.laid_out = Some(self.layout_key());
    }

    /// Calculate cursor position from a known visual line index and x coordinate
    ///
    /// This is used when a line element's click handler has already determined
    /// which visual line was clicked. The x coordinate is relative to the line element.
    pub fn cursor_position_from_visual_line(&self, visual_line_idx: usize, x: f32) -> TextPosition {
        if visual_line_idx >= self.visual_lines.len() {
            return TextPosition::default();
        }

        let vl = &self.visual_lines[visual_line_idx];
        let column = self.char_position_in_visual_line(visual_line_idx, x, self.font_size);

        TextPosition::new(vl.logical_line, column)
    }

    /// Get the visual line index for a given cursor position
    ///
    /// Returns the index into visual_lines where the cursor is located.
    pub fn visual_line_for_cursor(&self) -> usize {
        let cursor_line = self.cursor.line;
        let cursor_col = self.cursor.column;

        for (idx, vl) in self.visual_lines.iter().enumerate() {
            if vl.logical_line == cursor_line {
                // Check if cursor is within this visual line's range
                if cursor_col >= vl.start_char && cursor_col <= vl.end_char {
                    return idx;
                }
                // If cursor is at the end of a wrapped line, it might be at start of next visual line
                if cursor_col == vl.end_char {
                    // Check if there's another visual line for same logical line
                    if idx + 1 < self.visual_lines.len()
                        && self.visual_lines[idx + 1].logical_line == cursor_line
                        && self.visual_lines[idx + 1].start_char == cursor_col
                    {
                        return idx + 1;
                    }
                    return idx;
                }
            }
        }

        // Fallback: return last visual line
        self.visual_lines.len().saturating_sub(1)
    }

    /// Get cursor X position within its visual line
    ///
    /// Returns the pixel offset from the left edge of the visual line to the cursor.
    pub fn cursor_x_in_visual_line(&self) -> f32 {
        let cursor_line = self.cursor.line;
        let cursor_col = self.cursor.column;

        // Find the visual line containing the cursor
        for vl in &self.visual_lines {
            if vl.logical_line == cursor_line
                && cursor_col >= vl.start_char
                && cursor_col <= vl.end_char
            {
                // Measure text from start of visual line to cursor
                let local_col = cursor_col - vl.start_char;
                if local_col == 0 {
                    return 0.0;
                }
                let text_before: String = vl.text.chars().take(local_col).collect();
                return crate::text_measure::measure_text(&text_before, self.font_size).width;
            }
        }

        0.0
    }

    /// Get cursor position (x, visual_y) using computed visual lines
    ///
    /// Returns (cursor_x, cursor_visual_y) for positioning the cursor element.
    pub fn cursor_position_from_visual_lines(&self) -> (f32, f32) {
        let visual_line_idx = self.visual_line_for_cursor();
        let cursor_x = self.cursor_x_in_visual_line();
        let cursor_visual_y = visual_line_idx as f32 * self.line_height;
        (cursor_x, cursor_visual_y)
    }

    /// Get total visual line count
    pub fn visual_line_count(&self) -> usize {
        if self.visual_lines.is_empty() {
            // Fallback when visual lines not computed yet
            self.lines.len()
        } else {
            self.visual_lines.len()
        }
    }

    /// Get content height based on visual lines
    pub fn content_height_from_visual_lines(&self) -> f32 {
        self.visual_line_count() as f32 * self.line_height
    }

    /// Calculate cursor Y position accounting for text wrapping
    ///
    /// Returns (cursor_y, content_height) where cursor_y is the visual Y position
    /// of the cursor line, and content_height is the total visual height.
    pub(super) fn calculate_wrapped_positions(
        &self,
        font_size: f32,
        line_height: f32,
        available_width: f32,
        wrap_enabled: bool,
    ) -> (f32, f32) {
        if !wrap_enabled || available_width <= 0.0 {
            // No wrapping - simple calculation
            let cursor_y = self.cursor.line as f32 * line_height;
            let content_height = self.lines.len() as f32 * line_height;
            return (cursor_y, content_height);
        }

        // With wrapping, count visual lines up to cursor line
        let mut visual_line_count = 0usize;
        let mut cursor_visual_y = 0.0f32;

        for (idx, line) in self.lines.iter().enumerate() {
            let line_visual_count = Self::visual_lines_for_text(line, font_size, available_width);

            if idx < self.cursor.line {
                visual_line_count += line_visual_count;
            } else if idx == self.cursor.line {
                cursor_visual_y = visual_line_count as f32 * line_height;
                // For the cursor's line, we need to account for wrapped position
                // within that line based on cursor column position
                if line_visual_count > 1 && !line.is_empty() {
                    let text_before: String = line.chars().take(self.cursor.column).collect();
                    let prefix_width =
                        crate::text_measure::measure_text(&text_before, font_size).width;
                    let lines_into = (prefix_width / available_width).floor() as usize;
                    cursor_visual_y += lines_into as f32 * line_height;
                }
                visual_line_count += line_visual_count;
            } else {
                visual_line_count += line_visual_count;
            }
        }

        let content_height = visual_line_count as f32 * line_height;
        (cursor_visual_y, content_height)
    }

    /// Ensure the cursor is visible by adjusting scroll offset if needed
    ///
    /// This should be called after any cursor movement to auto-scroll
    /// when the cursor moves outside the visible area.
    /// Uses cached wrap settings from the state.
    pub fn ensure_cursor_visible(&mut self, line_height: f32, viewport_height: f32) {
        // Use visual lines for accurate cursor position if available
        let (cursor_y, content_height) = if !self.visual_lines.is_empty() {
            let (_, cursor_y) = self.cursor_position_from_visual_lines();
            let content_height = self.content_height_from_visual_lines();
            (cursor_y, content_height)
        } else {
            self.calculate_wrapped_positions(
                self.font_size,
                line_height,
                self.available_width,
                self.wrap_enabled,
            )
        };
        let cursor_bottom = cursor_y + line_height;

        // Get current scroll offset from physics (offset_y is negative when scrolled down)
        let mut physics = self.scroll_physics.lock().unwrap();
        let current_offset = -physics.offset_y; // Convert to positive scroll offset

        // If cursor is above visible area, scroll up
        let mut new_offset = current_offset;
        if cursor_y < current_offset {
            new_offset = cursor_y;
        }

        // If cursor is below visible area, scroll down
        if cursor_bottom > current_offset + viewport_height {
            new_offset = cursor_bottom - viewport_height;
        }

        // Clamp scroll offset to valid range
        let max_scroll = (content_height - viewport_height).max(0.0);
        new_offset = new_offset.clamp(0.0, max_scroll);

        // Update physics offset (negative for scroll physics convention)
        physics.offset_y = -new_offset;
    }

    /// Get current scroll offset (positive value, 0 = top)
    pub fn scroll_offset(&self) -> f32 {
        -self.scroll_physics.lock().unwrap().offset_y
    }

    /// Calculate cursor position from click coordinates
    ///
    /// Takes x/y coordinates relative to the text content area (after padding).
    /// Returns the TextPosition (line, column) for the clicked location.
    pub fn cursor_position_from_xy(&self, x: f32, y: f32) -> TextPosition {
        if self.lines.is_empty() {
            return TextPosition::default();
        }

        let line_height = self.line_height;
        let font_size = self.font_size;
        let scroll_offset = self.scroll_offset();

        // Account for scroll offset - y is in viewport space
        let text_y = y + scroll_offset;

        // Find the visual line index that was clicked
        let visual_line_idx = (text_y / line_height).floor().max(0.0) as usize;

        // Use computed visual lines if available for accurate positioning
        if !self.visual_lines.is_empty() {
            // Clamp to valid range
            let visual_line_idx = visual_line_idx.min(self.visual_lines.len().saturating_sub(1));
            let vl = &self.visual_lines[visual_line_idx];

            // Find character position within this visual line
            let column = self.char_position_in_visual_line(visual_line_idx, x, font_size);

            return TextPosition::new(vl.logical_line, column);
        }

        // Fallback: no visual lines computed
        let logical_line = visual_line_idx.min(self.lines.len().saturating_sub(1));
        let column = self.char_position_from_x(logical_line, x, font_size);
        TextPosition::new(logical_line, column)
    }

    /// Find character position from x coordinate within a visual line
    pub(super) fn char_position_in_visual_line(
        &self,
        visual_line_idx: usize,
        x: f32,
        font_size: f32,
    ) -> usize {
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

        // Check position before each character and after the last within this visual line
        for i in 0..=char_count {
            let prefix: String = vl.text.chars().take(i).collect();
            let prefix_width = crate::text_measure::measure_text(&prefix, font_size).width;

            let dist = (prefix_width - x).abs();
            if dist < min_dist {
                min_dist = dist;
                best_pos = i;
            }
        }

        // Convert local position to absolute position within logical line
        vl.start_char + best_pos
    }

    /// Find character position from x coordinate within a line
    pub(super) fn char_position_from_x(&self, line_index: usize, x: f32, font_size: f32) -> usize {
        if line_index >= self.lines.len() {
            return 0;
        }

        let line = &self.lines[line_index];
        if line.is_empty() {
            return 0;
        }

        let char_count = line.chars().count();
        let mut best_pos = 0;
        let mut min_dist = f32::MAX;

        // Check position before each character and after the last
        for i in 0..=char_count {
            let prefix: String = line.chars().take(i).collect();
            let prefix_width = crate::text_measure::measure_text(&prefix, font_size).width;

            let dist = (prefix_width - x).abs();
            if dist < min_dist {
                min_dist = dist;
                best_pos = i;
            }
        }

        best_pos
    }
}

/// Convert character index to byte index
pub(super) fn char_to_byte_pos(line: &str, char_pos: usize) -> usize {
    line.char_indices()
        .nth(char_pos)
        .map(|(i, _)| i)
        .unwrap_or(line.len())
}
