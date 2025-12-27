//! Typography helpers for semantic text elements
//!
//! This module provides HTML-like typography helpers that wrap the `text()` element
//! with sensible defaults for common text patterns.
//!
//! # Headings
//!
//! ```ignore
//! use blinc_layout::prelude::*;
//!
//! // Named heading helpers
//! h1("Welcome")           // 32px, bold
//! h2("Section Title")     // 24px, bold
//! h3("Subsection")        // 20px, semibold
//! h4("Small Heading")     // 18px, semibold
//! h5("Minor Heading")     // 16px, medium
//! h6("Smallest Heading")  // 14px, medium
//!
//! // Or use the generic heading() with level
//! heading(1, "Welcome")   // Same as h1()
//! heading(3, "Section")   // Same as h3()
//! ```
//!
//! # Inline Text
//!
//! ```ignore
//! // Bold text
//! b("Important")
//! strong("Also bold")
//!
//! // Spans (neutral text wrapper)
//! span("Some text")
//!
//! // Small text
//! small("Fine print")
//!
//! // Labels
//! label("Field name")
//!
//! // Muted/secondary text
//! muted("Less important")
//! ```
//!
//! # All helpers support the full Text API
//!
//! ```ignore
//! h1("Title")
//!     .color(Color::WHITE)
//!     .text_center()
//!     .shadow(Shadow::new(0.0, 2.0, 4.0, Color::BLACK.with_alpha(0.5)))
//! ```

use blinc_core::Color;

use crate::text::{text, Text};

// ============================================================================
// Heading Sizes Configuration
// ============================================================================

/// Default heading configurations (size, weight)
const HEADING_CONFIG: [(f32, HeadingWeight); 6] = [
    (32.0, HeadingWeight::Bold),     // h1
    (24.0, HeadingWeight::Bold),     // h2
    (20.0, HeadingWeight::SemiBold), // h3
    (18.0, HeadingWeight::SemiBold), // h4
    (16.0, HeadingWeight::Medium),   // h5
    (14.0, HeadingWeight::Medium),   // h6
];

#[derive(Clone, Copy)]
enum HeadingWeight {
    Medium,
    SemiBold,
    Bold,
}

// ============================================================================
// Heading Helpers
// ============================================================================

/// Create a level-1 heading (32px, bold)
///
/// # Example
///
/// ```ignore
/// h1("Page Title").color(Color::WHITE)
/// ```
pub fn h1(content: impl Into<String>) -> Text {
    heading(1, content)
}

/// Create a level-2 heading (24px, bold)
///
/// # Example
///
/// ```ignore
/// h2("Section Title").color(Color::WHITE)
/// ```
pub fn h2(content: impl Into<String>) -> Text {
    heading(2, content)
}

/// Create a level-3 heading (20px, semibold)
///
/// # Example
///
/// ```ignore
/// h3("Subsection").color(Color::WHITE)
/// ```
pub fn h3(content: impl Into<String>) -> Text {
    heading(3, content)
}

/// Create a level-4 heading (18px, semibold)
///
/// # Example
///
/// ```ignore
/// h4("Minor Section").color(Color::WHITE)
/// ```
pub fn h4(content: impl Into<String>) -> Text {
    heading(4, content)
}

/// Create a level-5 heading (16px, medium)
///
/// # Example
///
/// ```ignore
/// h5("Small Heading").color(Color::WHITE)
/// ```
pub fn h5(content: impl Into<String>) -> Text {
    heading(5, content)
}

/// Create a level-6 heading (14px, medium)
///
/// # Example
///
/// ```ignore
/// h6("Smallest Heading").color(Color::WHITE)
/// ```
pub fn h6(content: impl Into<String>) -> Text {
    heading(6, content)
}

/// Create a heading with a specific level (1-6)
///
/// Levels outside 1-6 are clamped to the nearest valid level.
///
/// # Example
///
/// ```ignore
/// // Dynamic heading level
/// let level = 2;
/// heading(level, "Dynamic Title")
///
/// // Equivalent to h2()
/// heading(2, "Section Title")
/// ```
pub fn heading(level: u8, content: impl Into<String>) -> Text {
    let idx = (level.saturating_sub(1).min(5)) as usize;
    let (size, weight) = HEADING_CONFIG[idx];

    let t = text(content).size(size).no_wrap();

    match weight {
        HeadingWeight::Medium => t.medium(),
        HeadingWeight::SemiBold => t.semibold(),
        HeadingWeight::Bold => t.bold(),
    }
}

// ============================================================================
// Inline Text Helpers
// ============================================================================

/// Create bold text
///
/// # Example
///
/// ```ignore
/// div().child(b("Important")).child(text(" regular text"))
/// ```
pub fn b(content: impl Into<String>) -> Text {
    text(content).bold()
}

/// Create bold text (alias for `b()`)
///
/// # Example
///
/// ```ignore
/// strong("Very important")
/// ```
pub fn strong(content: impl Into<String>) -> Text {
    b(content)
}

/// Create a neutral text span
///
/// This is equivalent to `text()` but named for HTML familiarity.
///
/// # Example
///
/// ```ignore
/// span("Some text").color(Color::BLUE)
/// ```
pub fn span(content: impl Into<String>) -> Text {
    text(content)
}

/// Create small text (12px)
///
/// # Example
///
/// ```ignore
/// small("Fine print").color(Color::GRAY)
/// ```
pub fn small(content: impl Into<String>) -> Text {
    text(content).size(12.0)
}

/// Create a label (14px, medium weight)
///
/// Useful for form field labels.
///
/// # Example
///
/// ```ignore
/// div()
///     .flex_col()
///     .gap(4.0)
///     .child(label("Username"))
///     .child(text_input(&state))
/// ```
pub fn label(content: impl Into<String>) -> Text {
    text(content).size(14.0).medium()
}

/// Create muted/secondary text
///
/// Uses a dimmer gray color by default. Override with `.color()`.
///
/// # Example
///
/// ```ignore
/// muted("Less important information")
/// ```
pub fn muted(content: impl Into<String>) -> Text {
    text(content).color(Color::rgba(0.6, 0.6, 0.65, 1.0))
}

/// Create a paragraph text element (16px with line height 1.5)
///
/// Optimized for readability of body text.
///
/// # Example
///
/// ```ignore
/// p("This is a paragraph of text that may span multiple lines...")
/// ```
pub fn p(content: impl Into<String>) -> Text {
    text(content).size(16.0).line_height(1.5)
}

/// Create caption text (12px, muted)
///
/// For image captions, table footnotes, etc.
///
/// # Example
///
/// ```ignore
/// caption("Figure 1: Architecture diagram")
/// ```
pub fn caption(content: impl Into<String>) -> Text {
    text(content)
        .size(12.0)
        .color(Color::rgba(0.5, 0.5, 0.55, 1.0))
}

/// Create code-styled inline text (monospace appearance via content)
///
/// Note: This uses the default font but styles text for inline code.
/// For full code blocks with syntax highlighting, use `code()`.
///
/// # Example
///
/// ```ignore
/// div()
///     .flex_row()
///     .child(text("Use the "))
///     .child(inline_code("div()"))
///     .child(text(" function"))
/// ```
pub fn inline_code(content: impl Into<String>) -> Text {
    text(content)
        .size(13.0)
        .color(Color::rgba(0.9, 0.6, 0.5, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::div::ElementBuilder;
    use crate::tree::LayoutTree;

    #[test]
    fn test_headings() {
        let mut tree = LayoutTree::new();

        let _h1 = h1("Title").build(&mut tree);
        let _h2 = h2("Subtitle").build(&mut tree);
        let _h3 = h3("Section").build(&mut tree);

        assert_eq!(tree.len(), 3);
    }

    #[test]
    fn test_heading_levels() {
        // Heading level 1-6 should work
        let _ = heading(1, "One");
        let _ = heading(6, "Six");

        // Out of bounds should clamp
        let _ = heading(0, "Zero"); // becomes level 1
        let _ = heading(10, "Ten"); // becomes level 6
    }

    #[test]
    fn test_inline_helpers() {
        let mut tree = LayoutTree::new();

        let _bold = b("Bold").build(&mut tree);
        let _strong = strong("Strong").build(&mut tree);
        let _span = span("Span").build(&mut tree);
        let _small = small("Small").build(&mut tree);
        let _label = label("Label").build(&mut tree);
        let _muted = muted("Muted").build(&mut tree);
        let _para = p("Paragraph").build(&mut tree);
        let _cap = caption("Caption").build(&mut tree);

        assert_eq!(tree.len(), 8);
    }
}
