//! Find and replace over the editor's lines: the search, its matches, and replacing them.
//!
//! The find bar in `search.rs`, and any other front end, drive the editor through these methods.

use super::{CodeEditorData, SharedCodeEditorState, char_to_byte_pos};
use crate::widgets::text_area::TextPosition;

impl CodeEditorData {
    /// The query as a regex: a pattern in regex mode, with the case and whole-word options
    /// applied. `None` outside regex mode, and `Err` for a pattern that does not compile.
    fn query_regex(&self) -> Option<Result<regex::Regex, regex::Error>> {
        if !self.search_regex {
            return None;
        }
        let case = if self.search_case_sensitive {
            ""
        } else {
            "(?i)"
        };
        let pattern = if self.search_whole_word {
            format!(r"{case}\b(?:{})\b", self.search_query)
        } else {
            format!("{case}{}", self.search_query)
        };
        Some(regex::Regex::new(&pattern))
    }

    /// The one-line selection to search for, with the cursor moved to its start, so the search
    /// lands on it rather than on the match after.
    pub fn take_search_seed(&mut self) -> Option<String> {
        let seed = self
            .selected_text()
            .filter(|t| !t.is_empty() && !t.contains('\n'))?;
        if let Some(anchor) = self.selection_start.take() {
            let start = if (anchor.line, anchor.column) < (self.cursor.line, self.cursor.column) {
                anchor
            } else {
                self.cursor
            };
            self.cursor = start;
        }
        Some(seed)
    }

    /// Execute search: find all matches of the query in the lines.
    ///
    /// The query is plain text, or a regex in regex mode. A regex that does not compile finds
    /// nothing and leaves its message in `search_error`.
    pub fn execute_search(&mut self) {
        self.search_matches.clear();
        self.search_match_idx = 0;
        self.search_error = None;

        if self.search_query.is_empty() {
            return;
        }

        if let Some(compiled) = self.query_regex() {
            match compiled {
                Ok(re) => {
                    for (line_idx, line) in self.lines.iter().enumerate() {
                        for m in re.find_iter(line) {
                            let start_col = line[..m.start()].chars().count();
                            let end_col = line[..m.end()].chars().count();
                            self.search_matches.push((line_idx, start_col, end_col));
                        }
                    }
                }
                Err(e) => self.search_error = Some(short_message(&e)),
            }
            return;
        }

        #[allow(clippy::type_complexity)]
        let (query_cmp, make_line_cmp): (String, Box<dyn Fn(&str) -> String>) =
            if self.search_case_sensitive {
                (self.search_query.clone(), Box::new(|s: &str| s.to_string()))
            } else {
                (
                    self.search_query.to_lowercase(),
                    Box::new(|s: &str| s.to_lowercase()),
                )
            };

        for (line_idx, line) in self.lines.iter().enumerate() {
            let line_cmp = make_line_cmp(line);
            let mut search_from = 0;
            while let Some(byte_pos) = line_cmp[search_from..].find(&query_cmp) {
                let abs_byte = search_from + byte_pos;
                let start_col = line[..abs_byte].chars().count();
                let end_col = start_col + self.search_query.chars().count();

                // Whole word check
                let is_match = if self.search_whole_word {
                    let before_ok = start_col == 0
                        || !line
                            .chars()
                            .nth(start_col - 1)
                            .map(|c| c.is_alphanumeric() || c == '_')
                            .unwrap_or(false);
                    let after_ok = end_col >= line.chars().count()
                        || !line
                            .chars()
                            .nth(end_col)
                            .map(|c| c.is_alphanumeric() || c == '_')
                            .unwrap_or(false);
                    before_ok && after_ok
                } else {
                    true
                };

                if is_match {
                    self.search_matches.push((line_idx, start_col, end_col));
                }
                search_from = abs_byte + query_cmp.len();
            }
        }
    }

    /// Select the match at `idx`: the cursor goes to its start and it is scrolled into view.
    fn show_match(&mut self, idx: usize) {
        self.search_match_idx = idx;
        let (line, col, _) = self.search_matches[idx];
        self.cursor = TextPosition::new(line, col);
        self.selection_start = None;
        self.ensure_cursor_visible();
    }

    /// The index of the first match at or after `(line, column)`, or the first match.
    fn match_from(&self, line: usize, column: usize) -> usize {
        self.search_matches
            .iter()
            .position(|&(l, c, _)| l > line || (l == line && c >= column))
            .unwrap_or(0)
    }

    /// Jump to next search match
    pub fn search_next(&mut self) {
        if self.search_matches.is_empty() {
            return;
        }
        self.show_match((self.search_match_idx + 1) % self.search_matches.len());
    }

    /// Jump to previous search match
    pub fn search_prev(&mut self) {
        if self.search_matches.is_empty() {
            return;
        }
        let idx = self
            .search_match_idx
            .checked_sub(1)
            .unwrap_or(self.search_matches.len() - 1);
        self.show_match(idx);
    }

    /// Search for `query` and move to the first match at or after the cursor.
    pub fn apply_search_query(&mut self, query: String) {
        self.search_query = query;
        self.execute_search();
        if !self.search_matches.is_empty() {
            let idx = self.match_from(self.cursor.line, self.cursor.column);
            self.show_match(idx);
        }
    }

    /// Insert a character into the search query
    pub fn search_insert(&mut self, ch: char) {
        self.search_query.push(ch);
        self.execute_search();
        // Jump to nearest match from cursor
        if !self.search_matches.is_empty() {
            let idx = self.match_from(self.cursor.line, self.cursor.column);
            self.search_match_idx = idx;
            let (line, col, _) = self.search_matches[idx];
            self.cursor = TextPosition::new(line, col);
            self.ensure_cursor_visible();
        }
    }

    /// Delete last character from search query
    pub fn search_backspace(&mut self) {
        self.search_query.pop();
        self.execute_search();
    }

    /// What replaces the match `(line, start_col, end_col)`: the replace text, and in regex mode
    /// its `$1`, `${name}`, `$&` and `$$`.
    fn replacement_for(
        &self,
        re: Option<&regex::Regex>,
        line: usize,
        start_col: usize,
        end_col: usize,
    ) -> String {
        let Some(re) = re else {
            return self.replace_text.clone();
        };
        let text = &self.lines[line];
        let (start, end) = (
            char_to_byte_pos(text, start_col),
            char_to_byte_pos(text, end_col),
        );
        // Looked at in place, so `\b` and anchors see the text around the match.
        match re.captures_at(text, start) {
            Some(caps) if caps.get(0).map(|m| (m.start(), m.end())) == Some((start, end)) => {
                expand(&self.replace_text, &caps)
            }
            _ => self.replace_text.clone(),
        }
    }

    /// Replace the current search match with the replacement text, and move to the next match.
    /// Returns whether the text changed.
    pub fn replace_current(&mut self) -> bool {
        if self.config.read_only || self.search_matches.is_empty() {
            return false;
        }
        let (line, col_start, col_end) = self.search_matches[self.search_match_idx];
        let re = self.query_regex().and_then(Result::ok);
        let with = self.replacement_for(re.as_ref(), line, col_start, col_end);
        self.push_undo();

        let line_text = &mut self.lines[line];
        let start_byte = char_to_byte_pos(line_text, col_start);
        let end_byte = char_to_byte_pos(line_text, col_end);
        line_text.replace_range(start_byte..end_byte, &with);

        self.invalidate_all_highlights();
        self.execute_search();
        if !self.search_matches.is_empty() {
            // On from what was just put in, not back to the first match.
            let idx = self.match_from(line, col_start + with.chars().count());
            self.show_match(idx);
        }
        true
    }

    /// Replace every search match with the replacement text, as one undo step. Returns how many
    /// were replaced.
    pub fn replace_all(&mut self) -> usize {
        if self.config.read_only || self.search_matches.is_empty() {
            return 0;
        }
        let re = self.query_regex().and_then(Result::ok);
        let edits: Vec<(usize, usize, usize, String)> = self
            .search_matches
            .iter()
            .map(|&(line, s, e)| (line, s, e, self.replacement_for(re.as_ref(), line, s, e)))
            .collect();
        self.push_undo();

        // From last to first, so earlier positions still hold.
        for (line, col_start, col_end, with) in edits.iter().rev() {
            let line_text = &mut self.lines[*line];
            let start_byte = char_to_byte_pos(line_text, *col_start);
            let end_byte = char_to_byte_pos(line_text, *col_end);
            line_text.replace_range(start_byte..end_byte, with);
        }

        self.invalidate_all_highlights();
        self.search_matches.clear();
        self.search_match_idx = 0;
        edits.len()
    }
}

/// Tell the editor's `on_change` callback the text now, once the state is unlocked.
pub(super) fn notify_changed(state: &SharedCodeEditorState) {
    let changed = state
        .lock()
        .ok()
        .and_then(|d| d.on_change.clone().map(|cb| (cb, d.value())));
    if let Some((cb, value)) = changed {
        cb(&value);
    }
}

/// A regex error's last line, which is the reason without the pattern drawn under it.
fn short_message(error: &regex::Error) -> String {
    let text = error.to_string();
    let last = text
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("");
    last.trim().trim_start_matches("error:").trim().to_string()
}

/// `template` with `$1` (one or two digits), `${1}`, `${name}`, `$&` and `$$` filled from `caps`.
fn expand(template: &str, caps: &regex::Captures<'_>) -> String {
    let chars: Vec<char> = template.chars().collect();
    let mut out = String::new();
    let group = |n: usize| caps.get(n).map_or("", |m| m.as_str());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '$' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        match chars.get(i + 1) {
            Some('$') => {
                out.push('$');
                i += 2;
            }
            Some('&') => {
                out.push_str(group(0));
                i += 2;
            }
            Some(d) if d.is_ascii_digit() => {
                let mut n = d.to_digit(10).unwrap_or(0) as usize;
                i += 2;
                // A second digit makes it a two-digit group only where that group exists.
                if let Some(d2) = chars.get(i).filter(|c| c.is_ascii_digit()) {
                    let two = n * 10 + d2.to_digit(10).unwrap_or(0) as usize;
                    if two < caps.len() {
                        n = two;
                        i += 1;
                    }
                }
                out.push_str(group(n));
            }
            Some('{') => match chars[i + 2..].iter().position(|c| *c == '}') {
                Some(len) => {
                    let name: String = chars[i + 2..i + 2 + len].iter().collect();
                    match name.parse::<usize>() {
                        Ok(n) => out.push_str(group(n)),
                        Err(_) => out.push_str(caps.name(&name).map_or("", |m| m.as_str())),
                    }
                    i += len + 3;
                }
                None => {
                    out.push('$');
                    i += 1;
                }
            },
            _ => {
                out.push('$');
                i += 1;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::code_editor_state;

    fn editor(text: &str) -> super::super::SharedCodeEditorState {
        code_editor_state(text)
    }

    #[test]
    fn replace_all_expands_captures_in_regex_mode() {
        let state = editor("let a = 1;\nlet bc = 22;");
        let mut d = state.lock().unwrap();
        d.search_regex = true;
        d.apply_search_query(r"let (?P<name>\w+) = (\d+)".to_string());
        d.replace_text = "${name}: $2 ($&) $$ $1".to_string();
        assert_eq!(d.replace_all(), 2);
        assert_eq!(
            d.value(),
            "a: 1 (let a = 1) $ a;\nbc: 22 (let bc = 22) $ bc;"
        );
        // One undo step brings both lines back.
        assert!(d.undo());
        assert_eq!(d.value(), "let a = 1;\nlet bc = 22;");
    }

    #[test]
    fn plain_replace_keeps_a_dollar_as_text() {
        let state = editor("a a");
        let mut d = state.lock().unwrap();
        d.apply_search_query("a".to_string());
        d.replace_text = "$1".to_string();
        assert_eq!(d.replace_all(), 2);
        assert_eq!(d.value(), "$1 $1");
    }

    #[test]
    fn a_two_digit_group_needs_the_group() {
        let state = editor("ab");
        let mut d = state.lock().unwrap();
        d.search_regex = true;
        d.apply_search_query("(a)(b)".to_string());
        d.replace_text = "$10-$2$1".to_string();
        d.replace_all();
        // `$10` is group 1 then a zero, as there is no group 10.
        assert_eq!(d.value(), "a0-ba");
    }

    #[test]
    fn an_invalid_regex_says_why() {
        let state = editor("(a)");
        let mut d = state.lock().unwrap();
        d.search_regex = true;
        d.apply_search_query("(a".to_string());
        assert!(d.search_matches.is_empty());
        let why = d.search_error.clone().unwrap();
        assert!(why.contains("unclosed group"), "{why}");
        // A good query clears it.
        d.apply_search_query("a".to_string());
        assert_eq!(
            (d.search_matches.len(), d.search_error.is_none()),
            (1, true)
        );
    }

    #[test]
    fn whole_word_applies_in_regex_mode_too() {
        let state = editor("cat concat cat_x cat");
        let mut d = state.lock().unwrap();
        d.search_regex = true;
        d.search_whole_word = true;
        d.apply_search_query("cat".to_string());
        assert_eq!(d.search_matches, [(0, 0, 3), (0, 17, 20)]);
    }

    #[test]
    fn replace_current_goes_on_to_the_next_match() {
        let state = editor("x a x\nx");
        let mut d = state.lock().unwrap();
        d.apply_search_query("x".to_string());
        d.replace_text = "yy".to_string();
        // The first match, then the second, which is where the cursor goes.
        assert!(d.replace_current());
        assert_eq!(d.value(), "yy a x\nx");
        assert_eq!(d.search_matches.len(), 2);
        assert_eq!(
            (d.search_match_idx, d.cursor.line, d.cursor.column),
            (0, 0, 5)
        );
        assert!(d.replace_current());
        assert!(d.replace_current());
        assert_eq!(d.value(), "yy a yy\nyy");
        // Nothing left: nothing changes.
        assert!(!d.replace_current());
    }

    #[test]
    fn nothing_to_replace_changes_nothing() {
        let state = editor("abc");
        let mut d = state.lock().unwrap();
        d.apply_search_query("z".to_string());
        assert_eq!(d.replace_all(), 0);
        assert!(!d.replace_current());
        assert_eq!(d.value(), "abc");
    }
}
