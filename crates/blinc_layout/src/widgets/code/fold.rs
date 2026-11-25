//! Folding: which lines are hidden, and the row each line shown takes. Everything that places
//! something against a line, or finds the line under a point, goes through the rows here.

use super::{CodeEditorData, FoldRegion};

impl CodeEditorData {
    /// Detect foldable regions from bracket matching, among the lines shown
    pub fn detect_fold_regions(&self) -> Vec<FoldRegion> {
        brackets(&self.lines, |line| !self.is_line_folded(line))
    }

    /// Check if a line is inside a folded region
    pub fn is_line_folded(&self, line: usize) -> bool {
        self.folded_regions
            .iter()
            .any(|&(start, end)| line > start && line < end)
    }

    /// Check if a line is the start of a folded region
    pub fn is_fold_start(&self, line: usize) -> bool {
        self.folded_regions.iter().any(|&(start, _)| start == line)
    }

    /// Toggle fold at a line (fold if unfolded, unfold if folded)
    pub fn toggle_fold(&mut self, line: usize) {
        self.settle_folds();
        // If already folded at this line, unfold
        if let Some(idx) = self
            .folded_regions
            .iter()
            .position(|&(start, _)| start == line)
        {
            self.folded_regions.remove(idx);
            return;
        }
        // Otherwise, find the fold region starting at this line and fold it
        let regions = self.detect_fold_regions();
        if let Some(region) = regions.iter().find(|r| r.start_line == line) {
            self.folded_regions
                .push((region.start_line, region.end_line));
            self.fold_basis.clone_from(&self.lines);
            // Move cursor to end of fold start line if it's inside the folded region
            if self.cursor.line > region.start_line && self.cursor.line <= region.end_line {
                self.cursor.line = region.start_line;
                self.cursor.column = self.lines[region.start_line].chars().count();
            }
            self.selection_start = None;
        }
    }

    /// Get the visible line indices (skipping folded lines)
    pub fn visible_lines(&self) -> Vec<usize> {
        (0..self.lines.len())
            .filter(|&i| !self.is_line_folded(i))
            .collect()
    }

    /// The line shown at `row`, the last one for a row past the end.
    pub(super) fn line_at_row(&self, row: usize) -> usize {
        let visible = self.visible_lines();
        visible.get(row).or(visible.last()).copied().unwrap_or(0)
    }

    /// The row `line` is shown at. A hidden line has the row of the line its block opens on.
    pub(super) fn row_of(&self, line: usize) -> usize {
        match self.visible_lines().binary_search(&line) {
            Ok(row) => row,
            Err(next) => next.saturating_sub(1),
        }
    }

    /// The line shown above `line`.
    pub(super) fn line_above(&self, line: usize) -> Option<usize> {
        (0..line).rev().find(|&l| !self.is_line_folded(l))
    }

    /// The line shown below `line`.
    pub(super) fn line_below(&self, line: usize) -> Option<usize> {
        (line + 1..self.lines.len()).find(|&l| !self.is_line_folded(l))
    }

    /// Keep the folds on their blocks after an edit: those after the changed lines move with
    /// them, and a fold whose block no longer opens and closes where it did is opened.
    pub(super) fn settle_folds(&mut self) {
        if self.folded_regions.is_empty() {
            self.fold_basis.clear();
            return;
        }
        // Folds placed on `folded_regions` directly are taken as being on the lines as they are.
        if self.fold_basis.is_empty() {
            self.fold_basis.clone_from(&self.lines);
        }
        if self.fold_basis == self.lines {
            return;
        }
        let old = std::mem::replace(&mut self.fold_basis, self.lines.clone());
        let new = &self.lines;
        let same = |(a, b): &(&String, &String)| a == b;
        let head = old.iter().zip(new).take_while(same).count();
        let tail = old
            .iter()
            .rev()
            .zip(new.iter().rev())
            .take_while(same)
            .count()
            .min(old.len().min(new.len()) - head);
        // A line in the unchanged tail moves by what the text grew or shrank.
        let moved = |line: usize| {
            if line >= old.len() - tail {
                line + new.len() - old.len()
            } else {
                line
            }
        };
        let blocks = brackets(new, |_| true);
        self.folded_regions = self
            .folded_regions
            .iter()
            .map(|&(start, end)| (moved(start), moved(end)))
            .filter(|&(start, end)| {
                blocks
                    .iter()
                    .any(|b| b.start_line == start && b.end_line == end)
            })
            .collect();
    }

    /// Open whatever hides the cursor's line.
    pub(super) fn reveal_cursor(&mut self) {
        let line = self.cursor.line;
        self.folded_regions
            .retain(|&(start, end)| !(line > start && line < end));
    }
}

/// The blocks that open on one line and close on a later one, matching the braces of the lines
/// `counted` takes.
fn brackets(lines: &[String], counted: impl Fn(usize) -> bool) -> Vec<FoldRegion> {
    let mut regions = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if !counted(i) {
            continue;
        }
        for ch in line.chars() {
            if ch == '{' {
                stack.push(i);
            } else if ch == '}'
                && let Some(start) = stack.pop()
                && start != i
            {
                regions.push(FoldRegion {
                    start_line: start,
                    end_line: i,
                });
            }
        }
    }
    regions
}
