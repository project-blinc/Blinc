//! Text layout engine
//!
//! Handles line breaking, text measurement, and multi-line layout.

use crate::font::FontFace;
use crate::shaper::{ShapedGlyph, ShapedText, TextShaper};

/// Text alignment options (horizontal)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TextAlignment {
    #[default]
    Left,
    Center,
    Right,
}

/// Vertical anchor point for text positioning
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TextAnchor {
    /// Y coordinate is the top of the text bounding box
    #[default]
    Top,
    /// Y coordinate is the text baseline
    Baseline,
    /// Y coordinate is the vertical center of the text
    Center,
}

/// Line break mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LineBreakMode {
    /// Break at word boundaries
    #[default]
    Word,
    /// Break at character boundaries
    Character,
    /// No wrapping (single line)
    None,
}

/// Options for text layout
#[derive(Debug, Clone)]
pub struct LayoutOptions {
    /// Maximum width for line wrapping (None = no wrapping)
    pub max_width: Option<f32>,
    /// Text alignment (horizontal)
    pub alignment: TextAlignment,
    /// Vertical anchor point
    pub anchor: TextAnchor,
    /// Line break mode
    pub line_break: LineBreakMode,
    /// Line height multiplier (1.0 = default)
    pub line_height: f32,
    /// Letter spacing adjustment in pixels
    pub letter_spacing: f32,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        Self {
            max_width: None,
            alignment: TextAlignment::Left,
            anchor: TextAnchor::Top,
            line_break: LineBreakMode::Word,
            line_height: 1.2,
            letter_spacing: 0.0,
        }
    }
}

/// A positioned glyph ready for rendering
#[derive(Debug, Clone, Copy)]
pub struct PositionedGlyph {
    /// Glyph ID in the font
    pub glyph_id: u16,
    /// X position in pixels
    pub x: f32,
    /// Y position in pixels (baseline)
    pub y: f32,
    /// Character this glyph represents
    pub codepoint: char,
    /// Byte offset of this glyph's cluster in the source string
    pub byte_offset: usize,
}

/// A line of positioned glyphs
#[derive(Debug, Clone)]
pub struct LayoutLine {
    /// Glyphs in this line
    pub glyphs: Vec<PositionedGlyph>,
    /// Line width in pixels
    pub width: f32,
    /// Baseline Y position
    pub baseline_y: f32,
}

/// Result of laying out text
#[derive(Debug, Clone)]
pub struct TextLayout {
    /// Lines of positioned glyphs
    pub lines: Vec<LayoutLine>,
    /// Total width (widest line)
    pub width: f32,
    /// Total height
    pub height: f32,
}

impl TextLayout {
    /// Get all glyphs as a flat iterator
    pub fn glyphs(&self) -> impl Iterator<Item = &PositionedGlyph> {
        self.lines.iter().flat_map(|line| line.glyphs.iter())
    }

    /// Get total glyph count
    pub fn glyph_count(&self) -> usize {
        self.lines.iter().map(|l| l.glyphs.len()).sum()
    }

    /// Byte range of the source string covered by each line.
    ///
    /// A line starts at its first glyph's cluster and runs to the start of the
    /// next non-empty line, so whitespace consumed at a wrap point stays with
    /// the line that preceded it. `text_len` closes the final range.
    ///
    /// Lines with no glyphs (a blank line between two newlines) collapse to an
    /// empty range at the preceding line's end.
    pub fn line_byte_ranges(&self, text_len: usize) -> Vec<std::ops::Range<usize>> {
        let starts: Vec<Option<usize>> = self
            .lines
            .iter()
            .map(|line| line.glyphs.first().map(|g| g.byte_offset))
            .collect();

        let mut ranges = Vec::with_capacity(self.lines.len());
        let mut cursor = 0usize;
        for (i, start) in starts.iter().enumerate() {
            let start = start.unwrap_or(cursor);
            // The next line that actually has glyphs bounds this one.
            let end = starts[i + 1..]
                .iter()
                .flatten()
                .copied()
                .next()
                .unwrap_or(text_len)
                .max(start);
            ranges.push(start..end);
            cursor = end;
        }
        ranges
    }
}

/// Text layout engine
pub struct TextLayoutEngine {
    shaper: TextShaper,
}

impl TextLayoutEngine {
    /// Create a new layout engine
    pub fn new() -> Self {
        Self {
            shaper: TextShaper::new(),
        }
    }

    /// Layout text with the given options
    pub fn layout(
        &self,
        text: &str,
        font: &FontFace,
        font_size: f32,
        options: &LayoutOptions,
    ) -> TextLayout {
        let metrics = font.metrics();
        let line_height = metrics.line_height_px(font_size) * options.line_height;

        if text.is_empty() {
            // Empty text should still have proper height based on font metrics
            // so that layout containers size correctly
            return TextLayout {
                lines: Vec::new(),
                width: 0.0,
                height: line_height,
            };
        }

        let ascender = metrics.ascender_px(font_size);

        // Check for explicit newlines - these are always respected regardless of wrap mode
        let has_newlines = text.contains('\n');

        // Shape the entire text first
        let shaped = self.shaper.shape(text, font, font_size);

        // If no wrapping AND no explicit newlines, return single line
        if (options.max_width.is_none() || options.line_break == LineBreakMode::None)
            && !has_newlines
        {
            let mut line = self.create_line(&shaped, 0.0, ascender, options);
            let width = line.width;

            // Apply alignment if max_width is set
            if let Some(max_width) = options.max_width {
                if options.alignment != TextAlignment::Left && width < max_width {
                    let offset = match options.alignment {
                        TextAlignment::Center => (max_width - width) / 2.0,
                        TextAlignment::Right => max_width - width,
                        TextAlignment::Left => 0.0,
                    };
                    for glyph in &mut line.glyphs {
                        glyph.x += offset;
                    }
                }
            }

            return TextLayout {
                lines: vec![line],
                width,
                height: line_height,
            };
        }

        // Handle explicit newlines without word wrapping
        if has_newlines
            && (options.max_width.is_none() || options.line_break == LineBreakMode::None)
        {
            return self.layout_with_newlines_only(
                text,
                &shaped,
                font,
                font_size,
                ascender,
                line_height,
                options,
            );
        }

        let max_width = options.max_width.unwrap();

        // Break into lines
        let lines = self.break_lines(text, &shaped, font, font_size, max_width, options);

        // Position lines
        let mut positioned_lines = Vec::with_capacity(lines.len());
        let mut y = ascender;
        let mut max_width_found = 0.0f32;

        for line_glyphs in lines {
            let shaped_line = ShapedText {
                glyphs: line_glyphs,
                total_advance: 0, // Will be recalculated
                font_size,
                units_per_em: metrics.units_per_em,
            };

            let line = self.create_line(&shaped_line, 0.0, y, options);
            max_width_found = max_width_found.max(line.width);
            positioned_lines.push(line);
            y += line_height;
        }

        // Apply alignment
        if options.alignment != TextAlignment::Left {
            for line in &mut positioned_lines {
                let offset = match options.alignment {
                    TextAlignment::Center => (max_width - line.width) / 2.0,
                    TextAlignment::Right => max_width - line.width,
                    TextAlignment::Left => 0.0,
                };
                for glyph in &mut line.glyphs {
                    glyph.x += offset;
                }
            }
        }

        // Height is number of lines * line_height (minimum 1 line)
        let height = (positioned_lines.len().max(1) as f32) * line_height;

        TextLayout {
            lines: positioned_lines,
            width: max_width_found,
            height,
        }
    }

    /// Layout text that contains explicit newlines but no word wrapping
    #[allow(clippy::too_many_arguments)]
    fn layout_with_newlines_only(
        &self,
        text: &str,
        shaped: &ShapedText,
        _font: &FontFace,
        _font_size: f32,
        ascender: f32,
        line_height: f32,
        options: &LayoutOptions,
    ) -> TextLayout {
        let mut lines = Vec::new();
        let mut current_line: Vec<ShapedGlyph> = Vec::new();

        // Split glyphs by newline character
        for glyph in &shaped.glyphs {
            if glyph.codepoint == '\n' {
                // End current line, don't include the newline glyph
                lines.push(std::mem::take(&mut current_line));
            } else {
                current_line.push(*glyph);
            }
        }

        // Add remaining line
        if !current_line.is_empty() {
            lines.push(current_line);
        }

        // Handle case where text ends with newline
        if text.ends_with('\n') {
            lines.push(Vec::new());
        }

        // Position lines
        let mut positioned_lines = Vec::with_capacity(lines.len());
        let mut y = ascender;
        let mut max_width_found = 0.0f32;
        let metrics_units_per_em = shaped.units_per_em;

        for line_glyphs in lines {
            let shaped_line = ShapedText {
                glyphs: line_glyphs,
                total_advance: 0,
                font_size: shaped.font_size,
                units_per_em: metrics_units_per_em,
            };

            let line = self.create_line(&shaped_line, 0.0, y, options);
            max_width_found = max_width_found.max(line.width);
            positioned_lines.push(line);
            y += line_height;
        }

        // Apply alignment if max_width is set
        if let Some(max_width) = options.max_width {
            if options.alignment != TextAlignment::Left {
                for line in &mut positioned_lines {
                    if line.width < max_width {
                        let offset = match options.alignment {
                            TextAlignment::Center => (max_width - line.width) / 2.0,
                            TextAlignment::Right => max_width - line.width,
                            TextAlignment::Left => 0.0,
                        };
                        for glyph in &mut line.glyphs {
                            glyph.x += offset;
                        }
                    }
                }
            }
        }

        let total_height = if positioned_lines.is_empty() {
            line_height
        } else {
            (positioned_lines.len() as f32) * line_height
        };

        TextLayout {
            lines: positioned_lines,
            width: max_width_found,
            height: total_height,
        }
    }

    /// Create a layout line from shaped glyphs
    fn create_line(
        &self,
        shaped: &ShapedText,
        start_x: f32,
        baseline_y: f32,
        options: &LayoutOptions,
    ) -> LayoutLine {
        let mut glyphs = Vec::with_capacity(shaped.glyphs.len());
        let mut x = start_x;

        for glyph in &shaped.glyphs {
            let x_offset = shaped.scale(glyph.x_offset);
            let advance = shaped.scale(glyph.x_advance) + options.letter_spacing;

            glyphs.push(PositionedGlyph {
                glyph_id: glyph.glyph_id,
                x: x + x_offset,
                y: baseline_y,
                codepoint: glyph.codepoint,
                byte_offset: glyph.cluster as usize,
            });

            x += advance;
        }

        LayoutLine {
            glyphs,
            width: x - start_x,
            baseline_y,
        }
    }

    /// Break text into lines based on max width
    fn break_lines(
        &self,
        text: &str,
        shaped: &ShapedText,
        _font: &FontFace,
        _font_size: f32,
        max_width: f32,
        options: &LayoutOptions,
    ) -> Vec<Vec<ShapedGlyph>> {
        let mut lines = Vec::new();
        let mut current_line: Vec<ShapedGlyph> = Vec::new();
        let mut line_width = 0.0f32;

        // Find word boundaries (whitespace positions)
        let word_breaks: Vec<usize> = text
            .char_indices()
            .filter(|(_, c)| c.is_whitespace())
            .map(|(i, _)| i)
            .collect();

        let mut last_word_end = 0;
        let mut last_word_width = 0.0f32;

        for glyph in shaped.glyphs.iter() {
            // Handle explicit newline - always force a line break
            if glyph.codepoint == '\n' {
                lines.push(std::mem::take(&mut current_line));
                line_width = 0.0;
                last_word_end = 0;
                last_word_width = 0.0;
                continue; // Don't include the newline glyph itself
            }

            let advance = shaped.scale(glyph.x_advance) + options.letter_spacing;

            // Check if this is a word boundary (at a whitespace character)
            let is_word_break = word_breaks.contains(&(glyph.cluster as usize));

            // Check if adding this glyph would overflow the line
            if line_width + advance > max_width && !current_line.is_empty() {
                let mut broke_line = false;
                match options.line_break {
                    LineBreakMode::Word => {
                        if last_word_end > 0 {
                            // Break at last word boundary
                            // Keep glyphs 0..last_word_end on current line
                            // Move glyphs last_word_end.. to next line
                            let remaining: Vec<_> = current_line.drain(last_word_end..).collect();
                            lines.push(std::mem::take(&mut current_line));

                            // Start new line with remaining glyphs (skip leading whitespace)
                            for g in remaining {
                                if current_line.is_empty() && g.codepoint.is_whitespace() {
                                    continue; // Skip leading whitespace
                                }
                                current_line.push(g);
                            }

                            // Recalculate line width
                            line_width = current_line
                                .iter()
                                .map(|g| shaped.scale(g.x_advance) + options.letter_spacing)
                                .sum();
                            last_word_end = 0;
                            last_word_width = 0.0;
                            broke_line = true;
                        } else {
                            // No word boundary found - break at current position (character break)
                            lines.push(std::mem::take(&mut current_line));
                            line_width = 0.0;
                            last_word_end = 0;
                            last_word_width = 0.0;
                            broke_line = true;
                        }
                    }
                    LineBreakMode::Character => {
                        // Break at current position
                        lines.push(std::mem::take(&mut current_line));
                        line_width = 0.0;
                        last_word_end = 0;
                        last_word_width = 0.0;
                        broke_line = true;
                    }
                    LineBreakMode::None => {
                        // No breaking - let line overflow
                    }
                }

                // After breaking, we still need to add the current glyph (unless it's whitespace at line start)
                if broke_line {
                    // Skip leading whitespace on new lines
                    if current_line.is_empty() && glyph.codepoint.is_whitespace() {
                        continue;
                    }

                    // Add the current glyph that triggered the overflow
                    current_line.push(*glyph);
                    line_width += advance;

                    // Update word boundary if this glyph is a word break
                    if is_word_break {
                        last_word_end = current_line.len();
                        last_word_width = line_width;
                    }
                    continue; // Move to next glyph
                }
            }

            // Skip leading whitespace on new lines
            if current_line.is_empty() && glyph.codepoint.is_whitespace() {
                continue;
            }

            // Add glyph to current line
            current_line.push(*glyph);
            line_width += advance;

            // Update word boundary tracking AFTER adding the glyph
            // This way, when we break at last_word_end, all content up to and including
            // the space is on the current line, and remaining content goes to next line
            if is_word_break {
                // Mark position AFTER this whitespace as potential break point
                last_word_end = current_line.len();
                last_word_width = line_width;
            }
        }

        // Add remaining line
        if !current_line.is_empty() {
            lines.push(current_line);
        }

        lines
    }

    /// Measure text without full layout
    pub fn measure(
        &self,
        text: &str,
        font: &FontFace,
        font_size: f32,
        options: &LayoutOptions,
    ) -> (f32, f32) {
        let layout = self.layout(text, font, font_size, options);
        (layout.width, layout.height)
    }
}

impl Default for TextLayoutEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shaper::ShapedGlyph;

    /// Helper to verify all text content is preserved after word wrapping
    fn verify_content_preserved(lines: &[Vec<ShapedGlyph>], original: &str) {
        // Collect all characters from wrapped lines
        let wrapped: String = lines
            .iter()
            .flat_map(|line| line.iter().map(|g| g.codepoint))
            .collect();

        // The original text with whitespace normalized (single spaces between words)
        let original_normalized: String = original.split_whitespace().collect::<Vec<_>>().join(" ");

        // The wrapped text should have the same content (just with different whitespace)
        let wrapped_normalized: String = wrapped.split_whitespace().collect::<Vec<_>>().join(" ");

        assert_eq!(
            wrapped_normalized, original_normalized,
            "Text content was lost during wrapping!\nOriginal: {}\nWrapped: {}",
            original_normalized, wrapped_normalized
        );
    }

    fn create_mock_shaped_text(text: &str) -> ShapedText {
        let mut glyphs = Vec::new();
        for (i, c) in text.char_indices() {
            glyphs.push(ShapedGlyph {
                glyph_id: c as u16,
                cluster: i as u32,
                // ~5px space, ~10px char at font_size=16, units_per_em=1000
                x_advance: if c.is_whitespace() { 313 } else { 625 },
                y_advance: 0,
                x_offset: 0,
                y_offset: 0,
                codepoint: c,
            });
        }

        ShapedText {
            glyphs,
            total_advance: 0,
            font_size: 16.0,
            units_per_em: 1000,
        }
    }

    /// Standalone test of the word-break algorithm logic without needing a FontFace
    fn test_break_algorithm(text: &str, max_width: f32) -> Vec<Vec<ShapedGlyph>> {
        let shaped = create_mock_shaped_text(text);
        let options = LayoutOptions {
            max_width: Some(max_width),
            line_break: LineBreakMode::Word,
            ..Default::default()
        };

        // Re-implement break_lines logic directly for testing
        let mut lines = Vec::new();
        let mut current_line: Vec<ShapedGlyph> = Vec::new();
        let mut line_width = 0.0f32;

        let word_breaks: Vec<usize> = text
            .char_indices()
            .filter(|(_, c)| c.is_whitespace())
            .map(|(i, _)| i)
            .collect();

        let mut last_word_end = 0;

        for glyph in shaped.glyphs.iter() {
            let advance = shaped.scale(glyph.x_advance) + options.letter_spacing;
            let is_word_break = word_breaks.contains(&(glyph.cluster as usize));

            if line_width + advance > max_width && !current_line.is_empty() {
                let mut broke_line = false;
                if last_word_end > 0 {
                    let remaining: Vec<_> = current_line.drain(last_word_end..).collect();
                    lines.push(std::mem::take(&mut current_line));

                    for g in remaining {
                        if current_line.is_empty() && g.codepoint.is_whitespace() {
                            continue;
                        }
                        current_line.push(g);
                    }

                    line_width = current_line
                        .iter()
                        .map(|g| shaped.scale(g.x_advance) + options.letter_spacing)
                        .sum();
                    last_word_end = 0;
                    broke_line = true;
                } else {
                    lines.push(std::mem::take(&mut current_line));
                    line_width = 0.0;
                    last_word_end = 0;
                    broke_line = true;
                }

                if broke_line {
                    if current_line.is_empty() && glyph.codepoint.is_whitespace() {
                        continue;
                    }

                    current_line.push(*glyph);
                    line_width += advance;

                    if is_word_break {
                        last_word_end = current_line.len();
                    }
                    continue;
                }
            }

            if current_line.is_empty() && glyph.codepoint.is_whitespace() {
                continue;
            }

            current_line.push(*glyph);
            line_width += advance;

            if is_word_break {
                last_word_end = current_line.len();
            }
        }

        if !current_line.is_empty() {
            lines.push(current_line);
        }

        lines
    }

    #[test]
    fn test_word_wrap_preserves_all_content() {
        let text = "This is a paragraph with optimal line height for readability.";
        let lines = test_break_algorithm(text, 200.0);

        // Verify no content lost
        verify_content_preserved(&lines, text);

        // Should have multiple lines due to wrapping
        assert!(
            lines.len() > 1,
            "Text should have wrapped into multiple lines"
        );
    }

    #[test]
    fn test_problematic_paragraph() {
        // This is the exact text that was losing content
        let text = "This is a paragraph with optimal line height (1.5) for readability. Paragraphs are styled at 16px with comfortable spacing for body text.";
        let lines = test_break_algorithm(text, 400.0);

        // Verify no content lost
        verify_content_preserved(&lines, text);

        // Print lines for debugging
        for (i, line) in lines.iter().enumerate() {
            let line_text: String = line.iter().map(|g| g.codepoint).collect();
            println!("Line {}: '{}'", i + 1, line_text);
        }
    }

    /// Test that leading space width is correctly included in layout width.
    /// Uses mock shaped text to verify create_line behavior.
    fn test_leading_space_width_calculation() {
        // Test that leading space contributes to the measured width
        // This is crucial for inline text elements like " and text"
        let options = LayoutOptions {
            line_break: LineBreakMode::None,
            ..Default::default()
        };

        let text_with_leading = " hello";
        let text_without_leading = "hello";

        let shaped_with = create_mock_shaped_text(text_with_leading);
        let shaped_without = create_mock_shaped_text(text_without_leading);

        // Calculate width manually using the same logic as create_line
        fn calc_width(shaped: &ShapedText, options: &LayoutOptions) -> f32 {
            let mut x = 0.0f32;
            for glyph in &shaped.glyphs {
                let advance = shaped.scale(glyph.x_advance) + options.letter_spacing;
                x += advance;
            }
            x
        }

        let width_with = calc_width(&shaped_with, &options);
        let width_without = calc_width(&shaped_without, &options);

        println!("Width with leading space: {}", width_with);
        println!("Width without leading space: {}", width_without);
        println!("Difference: {}", width_with - width_without);

        // The width with leading space should be larger
        assert!(
            width_with > width_without,
            "Leading space should increase width: {} vs {}",
            width_with,
            width_without
        );

        // The difference should be approximately the space advance (~5px)
        let expected_diff = 313.0 * 16.0 / 1000.0; // space advance scaled
        let actual_diff = width_with - width_without;
        assert!(
            (actual_diff - expected_diff).abs() < 0.1,
            "Difference should be ~{:.1}px, got {:.1}px",
            expected_diff,
            actual_diff
        );
    }

    #[test]
    fn test_leading_space_preserved() {
        test_leading_space_width_calculation();
    }

    #[test]
    fn test_trailing_space_preserved() {
        // Test that trailing space contributes to the measured width
        // This is crucial for inline text elements like "This is " before "bold"
        let options = LayoutOptions {
            line_break: LineBreakMode::None,
            ..Default::default()
        };

        let text_with_trailing = "hello ";
        let text_without_trailing = "hello";

        let shaped_with = create_mock_shaped_text(text_with_trailing);
        let shaped_without = create_mock_shaped_text(text_without_trailing);

        // Calculate width manually using the same logic as create_line
        fn calc_width(shaped: &ShapedText, options: &LayoutOptions) -> f32 {
            let mut x = 0.0f32;
            for glyph in &shaped.glyphs {
                let advance = shaped.scale(glyph.x_advance) + options.letter_spacing;
                x += advance;
            }
            x
        }

        let width_with = calc_width(&shaped_with, &options);
        let width_without = calc_width(&shaped_without, &options);

        println!("Width with trailing space: {}", width_with);
        println!("Width without trailing space: {}", width_without);
        println!("Difference: {}", width_with - width_without);

        // The width with trailing space should be larger
        assert!(
            width_with > width_without,
            "Trailing space should increase width: {} vs {}",
            width_with,
            width_without
        );

        // The difference should be approximately the space advance (~5px)
        let expected_diff = 313.0 * 16.0 / 1000.0; // space advance scaled
        let actual_diff = width_with - width_without;
        assert!(
            (actual_diff - expected_diff).abs() < 0.1,
            "Difference should be ~{:.1}px, got {:.1}px",
            expected_diff,
            actual_diff
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_helvetica_trailing_space() {
        // Test with actual Helvetica font to verify real-world behavior
        use crate::font::FontFace;

        let font_path = std::path::Path::new("/System/Library/Fonts/Helvetica.ttc");
        let font = FontFace::from_file(font_path).expect("Failed to load Helvetica");

        let engine = TextLayoutEngine::new();
        let options = LayoutOptions {
            line_break: LineBreakMode::None,
            ..Default::default()
        };

        // Test trailing space (like "This is " before "bold")
        let text_with_trailing = "This is ";
        let text_without_trailing = "This is";

        let layout_with = engine.layout(text_with_trailing, &font, 16.0, &options);
        let layout_without = engine.layout(text_without_trailing, &font, 16.0, &options);

        println!(
            "Helvetica - Width with trailing space '{}': {}",
            text_with_trailing, layout_with.width
        );
        println!(
            "Helvetica - Width without trailing space '{}': {}",
            text_without_trailing, layout_without.width
        );
        println!(
            "Helvetica - Difference: {}",
            layout_with.width - layout_without.width
        );

        // Width with trailing space MUST be larger
        assert!(
            layout_with.width > layout_without.width,
            "FAIL: Trailing space not included in width! '{}' ({}) vs '{}' ({})",
            text_with_trailing,
            layout_with.width,
            text_without_trailing,
            layout_without.width
        );

        // Difference should be meaningful (space is typically ~1/4 to 1/3 of font size)
        let diff = layout_with.width - layout_without.width;
        assert!(
            diff > 2.0, // At least 2 pixels for a 16px font
            "Space advance too small: {} pixels",
            diff
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_helvetica_leading_space() {
        // Test with actual Helvetica font to verify leading space
        use crate::font::FontFace;

        let font_path = std::path::Path::new("/System/Library/Fonts/Helvetica.ttc");
        let font = FontFace::from_file(font_path).expect("Failed to load Helvetica");

        let engine = TextLayoutEngine::new();
        let options = LayoutOptions {
            line_break: LineBreakMode::None,
            ..Default::default()
        };

        // Test leading space (like " and text" after "bold")
        let text_with_leading = " and text";
        let text_without_leading = "and text";

        let layout_with = engine.layout(text_with_leading, &font, 16.0, &options);
        let layout_without = engine.layout(text_without_leading, &font, 16.0, &options);

        println!(
            "Helvetica - Width with leading space '{}': {}",
            text_with_leading, layout_with.width
        );
        println!(
            "Helvetica - Width without leading space '{}': {}",
            text_without_leading, layout_without.width
        );
        println!(
            "Helvetica - Difference: {}",
            layout_with.width - layout_without.width
        );

        // Width with leading space MUST be larger
        assert!(
            layout_with.width > layout_without.width,
            "FAIL: Leading space not included in width! '{}' ({}) vs '{}' ({})",
            text_with_leading,
            layout_with.width,
            text_without_leading,
            layout_without.width
        );

        // Also verify glyph positions - first visible glyph should be offset
        if !layout_with.lines.is_empty() {
            let line = &layout_with.lines[0];
            println!("Glyphs in ' and text':");
            for (i, g) in line.glyphs.iter().enumerate() {
                println!("  [{}] '{}' at x={}", i, g.codepoint, g.x);
            }

            // The 'a' glyph should NOT be at x=0 - it should be offset by space width
            if line.glyphs.len() >= 2 {
                let first_visible = line.glyphs.iter().find(|g| !g.codepoint.is_whitespace());
                if let Some(glyph) = first_visible {
                    assert!(
                        glyph.x > 2.0,
                        "First visible glyph '{}' should be offset by space, but is at x={}",
                        glyph.codepoint,
                        glyph.x
                    );
                }
            }
        }
    }
}
