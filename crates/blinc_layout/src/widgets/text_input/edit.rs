//! Editing the value, moving the cursor and keeping it in view.

use super::*;

impl TextInputData {
    pub fn new() -> Self {
        Self {
            value: String::new(),
            cursor: 0,
            selection_start: None,
            placeholder: String::new(),
            input_type: InputType::Text,
            constraints: InputConstraints::default(),
            disabled: false,
            masked: false,
            is_valid: true,
            visual: TextFieldState::Idle,
            focus_time_ms: 0,
            cursor_state: cursor_state(),
            scroll_offset_x: 0.0,
            computed_width: None,
            layout_bounds_storage: Arc::new(Mutex::new(None)),
            revision: None,
            node_id: None,
            on_change_callback: None,
            force_sync_once: false,
            on_step_callback: None,
            on_backspace_empty_callback: None,
            on_paste_override_callback: None,
            on_focus_request_callback: None,
            css_element_id: None,
            css_classes: Vec::new(),
            last_click_time: None,
            drag_select_anchor: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    /// Snapshot the current `(value, cursor, selection_start)` triple
    /// onto the undo stack and clear the redo stack. Called from inside
    /// the text-mutating helpers BEFORE they apply their change so the
    /// snapshot represents the pre-edit state.
    ///
    /// New edits invalidate the redo branch — once you type something
    /// after an undo, the path you undid is no longer reachable.
    pub(crate) fn push_undo(&mut self) {
        self.undo_stack.push(UndoEntry {
            value: self.value.clone(),
            cursor: self.cursor,
            selection_start: self.selection_start,
        });
        if self.undo_stack.len() > UNDO_HISTORY_MAX {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    /// Pop the most recent undo entry, push the current state onto the
    /// redo stack, and restore the popped state. Returns `true` if any
    /// state was actually restored (i.e. the undo stack was non-empty).
    pub fn undo(&mut self) -> bool {
        let Some(entry) = self.undo_stack.pop() else {
            return false;
        };
        self.redo_stack.push(UndoEntry {
            value: self.value.clone(),
            cursor: self.cursor,
            selection_start: self.selection_start,
        });
        if self.redo_stack.len() > UNDO_HISTORY_MAX {
            self.redo_stack.remove(0);
        }
        self.value = entry.value;
        self.cursor = entry.cursor;
        self.selection_start = entry.selection_start;
        true
    }

    /// Symmetric inverse of [`Self::undo`]. Returns `true` when the
    /// redo stack had something to apply.
    pub fn redo(&mut self) -> bool {
        let Some(entry) = self.redo_stack.pop() else {
            return false;
        };
        self.undo_stack.push(UndoEntry {
            value: self.value.clone(),
            cursor: self.cursor,
            selection_start: self.selection_start,
        });
        if self.undo_stack.len() > UNDO_HISTORY_MAX {
            self.undo_stack.remove(0);
        }
        self.value = entry.value;
        self.cursor = entry.cursor;
        self.selection_start = entry.selection_start;
        true
    }

    pub fn with_placeholder(placeholder: impl Into<String>) -> Self {
        Self {
            placeholder: placeholder.into(),
            ..Self::new()
        }
    }

    pub fn with_value(value: impl Into<String>) -> Self {
        let v: String = value.into();
        let cursor = v.chars().count();
        Self {
            value: v,
            cursor,
            ..Self::new()
        }
    }

    /// Get display text (masked for password, or actual value)
    pub fn display_text(&self) -> String {
        if self.masked {
            "•".repeat(self.value.chars().count())
        } else {
            self.value.clone()
        }
    }

    /// Insert text at cursor, respecting input type constraints
    pub fn insert(&mut self, text: &str) {
        // Snapshot for undo BEFORE the mutation. We snapshot
        // unconditionally even if the eventual `filtered` text turns
        // out empty (e.g. typing a non-digit into an InputType::Number)
        // because the cost is one no-op undo entry, and the
        // alternative — snapshotting after filtering — would mean the
        // undo history conflates "I typed nothing" with "I typed
        // something that got dropped", which is the wrong UX.
        self.push_undo();
        // Delete selection first if any
        if let Some(start) = self.selection_start {
            let (from, to) = if start < self.cursor {
                (start, self.cursor)
            } else {
                (self.cursor, start)
            };
            let before: String = self.value.chars().take(from).collect();
            let after: String = self.value.chars().skip(to).collect();
            self.value = before + &after;
            self.cursor = from;
            self.selection_start = None;
        }

        // Filter based on input type
        let filtered: String = match self.input_type {
            InputType::Number => text
                .chars()
                .filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
                .collect(),
            InputType::Integer => text
                .chars()
                .filter(|c| c.is_ascii_digit() || *c == '-')
                .collect(),
            InputType::Tel => text
                .chars()
                .filter(|c| c.is_ascii_digit() || *c == '+' || *c == '-' || *c == ' ')
                .collect(),
            _ => text.to_string(),
        };

        if filtered.is_empty() {
            return;
        }

        // Check max length
        if let Some(max) = self.constraints.max_length {
            if self.value.chars().count() + filtered.chars().count() > max {
                return;
            }
        }

        // Insert at cursor
        let before: String = self.value.chars().take(self.cursor).collect();
        let after: String = self.value.chars().skip(self.cursor).collect();
        self.value = before + &filtered + &after;
        self.cursor += filtered.chars().count();

        self.validate();
        // NOTE: Don't call trigger_content_refresh() here - caller must do it
        // after releasing the lock to avoid deadlock
    }

    pub fn delete_backward(&mut self) {
        // Snapshot for undo BEFORE the mutation. We always snapshot
        // even when there's nothing to delete (cursor at 0, no
        // selection) — the resulting no-op undo entry is harmless
        // and keeps the call sites simple.
        self.push_undo();
        if let Some(start) = self.selection_start {
            let (from, to) = if start < self.cursor {
                (start, self.cursor)
            } else {
                (self.cursor, start)
            };
            let before: String = self.value.chars().take(from).collect();
            let after: String = self.value.chars().skip(to).collect();
            self.value = before + &after;
            self.cursor = from;
            self.selection_start = None;
        } else if self.cursor > 0 {
            let before: String = self.value.chars().take(self.cursor - 1).collect();
            let after: String = self.value.chars().skip(self.cursor).collect();
            self.value = before + &after;
            self.cursor -= 1;
        }
        self.validate();
        // NOTE: Don't call trigger_content_refresh() here - caller must do it
        // after releasing the lock to avoid deadlock
    }

    pub fn delete_forward(&mut self) {
        self.push_undo();
        if let Some(start) = self.selection_start {
            let (from, to) = if start < self.cursor {
                (start, self.cursor)
            } else {
                (self.cursor, start)
            };
            let before: String = self.value.chars().take(from).collect();
            let after: String = self.value.chars().skip(to).collect();
            self.value = before + &after;
            self.cursor = from;
            self.selection_start = None;
        } else if self.cursor < self.value.chars().count() {
            let before: String = self.value.chars().take(self.cursor).collect();
            let after: String = self.value.chars().skip(self.cursor + 1).collect();
            self.value = before + &after;
        }
        self.validate();
        // NOTE: Don't call trigger_content_refresh() here - caller must do it
        // after releasing the lock to avoid deadlock
    }

    pub fn move_left(&mut self, shift: bool) {
        if shift {
            if self.selection_start.is_none() {
                self.selection_start = Some(self.cursor);
            }
        } else {
            self.selection_start = None;
        }
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn move_right(&mut self, shift: bool) {
        if shift {
            if self.selection_start.is_none() {
                self.selection_start = Some(self.cursor);
            }
        } else {
            self.selection_start = None;
        }
        if self.cursor < self.value.chars().count() {
            self.cursor += 1;
        }
    }

    pub fn move_to_start(&mut self, shift: bool) {
        if shift {
            if self.selection_start.is_none() {
                self.selection_start = Some(self.cursor);
            }
        } else {
            self.selection_start = None;
        }
        self.cursor = 0;
    }

    pub fn move_to_end(&mut self, shift: bool) {
        if shift {
            if self.selection_start.is_none() {
                self.selection_start = Some(self.cursor);
            }
        } else {
            self.selection_start = None;
        }
        self.cursor = self.value.chars().count();
    }

    pub fn select_all(&mut self) {
        self.selection_start = Some(0);
        self.cursor = self.value.chars().count();
    }

    pub fn selected_text(&self) -> Option<String> {
        self.selection_start.map(|start| {
            let (from, to) = if start < self.cursor {
                (start, self.cursor)
            } else {
                (self.cursor, start)
            };
            self.value.chars().skip(from).take(to - from).collect()
        })
    }

    /// Move cursor to the previous word boundary
    pub fn move_word_left(&mut self, shift: bool) {
        if shift && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !shift {
            self.selection_start = None;
        }
        self.cursor = crate::widgets::text_edit::word_boundary_left(&self.value, self.cursor);
    }

    /// Move cursor to the next word boundary
    pub fn move_word_right(&mut self, shift: bool) {
        if shift && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !shift {
            self.selection_start = None;
        }
        self.cursor = crate::widgets::text_edit::word_boundary_right(&self.value, self.cursor);
    }

    /// Delete from cursor to the previous word boundary
    pub fn delete_word_backward(&mut self) {
        // Push BEFORE the early-return delete_selection branch so we
        // don't end up with the selection-delete branch double-pushing
        // (delete_selection also pushes its own undo). Take the
        // selection-delete fast path WITHOUT pushing here, then bail.
        if self.selection_start.is_some() {
            self.delete_selection();
            return;
        }
        self.push_undo();
        let target = crate::widgets::text_edit::word_boundary_left(&self.value, self.cursor);
        if target < self.cursor {
            let byte_start = self
                .value
                .char_indices()
                .nth(target)
                .map(|(i, _)| i)
                .unwrap_or(0);
            let byte_end = self
                .value
                .char_indices()
                .nth(self.cursor)
                .map(|(i, _)| i)
                .unwrap_or(self.value.len());
            self.value = format!("{}{}", &self.value[..byte_start], &self.value[byte_end..]);
            self.cursor = target;
        }
    }

    /// Delete from cursor to the next word boundary
    pub fn delete_word_forward(&mut self) {
        if self.selection_start.is_some() {
            self.delete_selection();
            return;
        }
        self.push_undo();
        let target = crate::widgets::text_edit::word_boundary_right(&self.value, self.cursor);
        if target > self.cursor {
            let byte_start = self
                .value
                .char_indices()
                .nth(self.cursor)
                .map(|(i, _)| i)
                .unwrap_or(self.value.len());
            let byte_end = self
                .value
                .char_indices()
                .nth(target)
                .map(|(i, _)| i)
                .unwrap_or(self.value.len());
            self.value = format!("{}{}", &self.value[..byte_start], &self.value[byte_end..]);
        }
    }

    /// Delete the current selection, returning true if text changed
    pub fn delete_selection(&mut self) -> bool {
        if self.selection_start.is_none() {
            return false;
        }
        // Snapshot before mutating, but only if there's actually a
        // selection to delete. Calling `push_undo` for a no-op delete
        // would otherwise pollute the history with empty entries.
        self.push_undo();
        let start = self.selection_start.take().expect("checked above");
        let (from, to) = if start < self.cursor {
            (start, self.cursor)
        } else {
            (self.cursor, start)
        };
        let byte_from = self
            .value
            .char_indices()
            .nth(from)
            .map(|(i, _)| i)
            .unwrap_or(0);
        let byte_to = self
            .value
            .char_indices()
            .nth(to)
            .map(|(i, _)| i)
            .unwrap_or(self.value.len());
        self.value = format!("{}{}", &self.value[..byte_from], &self.value[byte_to..]);
        self.cursor = from;
        true
    }

    pub fn validate(&mut self) {
        self.is_valid = match self.input_type {
            InputType::Email => {
                self.value.is_empty() || (self.value.contains('@') && self.value.contains('.'))
            }
            InputType::Number => self.value.is_empty() || self.value.parse::<f64>().is_ok(),
            InputType::Integer => self.value.is_empty() || self.value.parse::<i64>().is_ok(),
            InputType::Url => {
                self.value.is_empty()
                    || self.value.starts_with("http://")
                    || self.value.starts_with("https://")
            }
            _ => true,
        };

        if self.constraints.required && self.value.is_empty() {
            self.is_valid = false;
        }

        if let Some(min) = self.constraints.min_length {
            if self.value.len() < min {
                self.is_valid = false;
            }
        }
    }

    pub fn reset_cursor_blink(&mut self) {
        if let Ok(mut cs) = self.cursor_state.lock() {
            cs.reset_blink();
        }
    }

    pub fn sync_global_selection(&self) {
        if let Some(start) = self.selection_start {
            if start != self.cursor {
                let (from, to) = if start < self.cursor {
                    (start, self.cursor)
                } else {
                    (self.cursor, start)
                };
                let selected: String = self.value.chars().skip(from).take(to - from).collect();
                set_selection(selected, SelectionSource::TextInput, true);
            } else {
                clear_selection();
            }
        } else {
            clear_selection();
        }
    }

    /// Calculate cursor position from x coordinate (relative to text content area)
    ///
    /// This is used for click-to-position cursor functionality.
    /// The x coordinate should be relative to the start of the text content (after padding).
    pub fn cursor_position_from_x(&self, x: f32, font_size: f32) -> usize {
        let display = self.display_text();
        if display.is_empty() {
            return 0;
        }

        // Account for scroll offset - the click x is in viewport space,
        // so add scroll_offset to get position in text space
        let text_x = x + self.scroll_offset_x;

        // Binary search would be more efficient, but for typical text input lengths,
        // linear search is fast enough
        let char_count = display.chars().count();
        let mut best_pos = 0;
        let mut min_dist = f32::MAX;

        // Check position before each character and after the last
        for i in 0..=char_count {
            let prefix: String = display.chars().take(i).collect();
            let prefix_width = crate::text_measure::measure_text(&prefix, font_size).width;

            let dist = (prefix_width - text_x).abs();
            if dist < min_dist {
                min_dist = dist;
                best_pos = i;
            }
        }

        best_pos
    }

    /// Ensure the cursor is visible by adjusting horizontal scroll offset.
    /// This implements HTML-like behavior where text scrolls left when typing
    /// extends beyond the visible width.
    pub fn ensure_cursor_visible(&mut self, config: &TextInputConfig) {
        // Try to get computed width from layout bounds storage first
        // This is updated after each layout computation
        let layout_width = self
            .layout_bounds_storage
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().map(|b| b.width));

        // Use layout width if available, otherwise fall back to stored computed_width
        let effective_computed_width = layout_width.or(self.computed_width);

        // For full-width inputs without computed bounds yet, don't scroll
        // This prevents incorrect scrolling before we have the real container width
        if config.use_full_width && effective_computed_width.is_none() {
            self.scroll_offset_x = 0.0;
            return;
        }

        // Calculate total text width
        let display = self.display_text();
        let total_text_width = if !display.is_empty() {
            crate::text_measure::measure_text(&display, config.font_size).width
        } else {
            0.0
        };

        // Calculate cursor x position (where cursor is in the full text)
        let cursor_x = if self.cursor > 0 && !display.is_empty() {
            let text_before: String = display.chars().take(self.cursor).collect();
            crate::text_measure::measure_text(&text_before, config.font_size).width
        } else {
            0.0
        };

        // Calculate available width for text (the visible viewport)
        // Use computed_width if available (set after layout), otherwise fall back to config.width
        // Account for padding on both sides and border
        let base_width = effective_computed_width.unwrap_or(config.width);
        let available_width = base_width - config.padding_x * 2.0 - config.border_width * 2.0;

        // Simple approach: measure if text exceeds viewport
        // If cursor is past the visible right edge, scroll to show cursor
        let visible_right = self.scroll_offset_x + available_width;
        let cursor_margin = 4.0; // Small margin so cursor isn't at the very edge

        if cursor_x > visible_right - cursor_margin {
            // Cursor is past the right edge - scroll right to show it
            self.scroll_offset_x = cursor_x - available_width + cursor_margin;
        } else if cursor_x < self.scroll_offset_x {
            // Cursor is past the left edge - scroll left to show it
            self.scroll_offset_x = cursor_x;
        }

        // Clamp: can't scroll past start, and don't scroll more than necessary
        self.scroll_offset_x = self.scroll_offset_x.max(0.0);

        // Also clamp max scroll so we don't scroll past the end of text
        let max_scroll = (total_text_width - available_width + cursor_margin).max(0.0);
        self.scroll_offset_x = self.scroll_offset_x.min(max_scroll);
    }
}
