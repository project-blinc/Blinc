//! Typography components
//!
//! Wraps typography helpers from `blinc_layout::typography` with `cn-*` CSS classes
//! for use with the blinc_cn component library. These provide semantic text elements
//! similar to HTML.
//!
//! # Headings
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! // Named heading helpers
//! cn::h1("Welcome")           // 32px, bold, .class("cn-h1")
//! cn::h2("Section Title")     // 24px, bold, .class("cn-h2")
//! cn::h3("Subsection")        // 20px, semibold, .class("cn-h3")
//! cn::h4("Small Heading")     // 18px, semibold, .class("cn-h4")
//! cn::h5("Minor Heading")     // 16px, medium, .class("cn-h5")
//! cn::h6("Smallest Heading")  // 14px, medium, .class("cn-h6")
//!
//! // Or use the generic heading() with level
//! cn::heading(1, "Welcome")   // Same as h1()
//! cn::heading(3, "Section")   // Same as h3()
//! ```
//!
//! # Inline Text
//!
//! ```ignore
//! // Bold text
//! cn::b("Important")
//! cn::strong("Also bold")
//!
//! // Spans (neutral text wrapper)
//! cn::span("Some text")
//!
//! // Small text
//! cn::small("Fine print")
//!
//! // Muted/secondary text
//! cn::muted("Less important")
//!
//! // Paragraph with proper line height
//! cn::p("This is a paragraph...")
//!
//! // Caption text
//! cn::caption("Figure 1: Diagram")
//!
//! // Inline code
//! cn::inline_code("div()")
//! ```
//!
//! # Chained Text
//!
//! ```ignore
//! // Compose inline text with different styles
//! cn::chained_text([
//!     cn::span("This is "),
//!     cn::b("bold"),
//!     cn::span(" and "),
//!     cn::inline_code("code"),
//!     cn::span(" text."),
//! ])
//! ```

use blinc_layout::text::Text;

// Re-export helpers that don't need cn-* classes
pub use blinc_layout::typography::{
    // Inline text helpers
    b,
    chained_text,
    inline_code,
    label,
    small,
    span,
    strong,
};

/// Create a level-1 heading (32px, bold) with `.class("cn-h1")`
pub fn h1(content: impl Into<String>) -> Text {
    blinc_layout::typography::h1(content).class("cn-h1")
}

/// Create a level-2 heading (24px, bold) with `.class("cn-h2")`
pub fn h2(content: impl Into<String>) -> Text {
    blinc_layout::typography::h2(content).class("cn-h2")
}

/// Create a level-3 heading (20px, semibold) with `.class("cn-h3")`
pub fn h3(content: impl Into<String>) -> Text {
    blinc_layout::typography::h3(content).class("cn-h3")
}

/// Create a level-4 heading (18px, semibold) with `.class("cn-h4")`
pub fn h4(content: impl Into<String>) -> Text {
    blinc_layout::typography::h4(content).class("cn-h4")
}

/// Create a level-5 heading (16px, medium) with `.class("cn-h5")`
pub fn h5(content: impl Into<String>) -> Text {
    blinc_layout::typography::h5(content).class("cn-h5")
}

/// Create a level-6 heading (14px, medium) with `.class("cn-h6")`
pub fn h6(content: impl Into<String>) -> Text {
    blinc_layout::typography::h6(content).class("cn-h6")
}

/// Create a heading with a specific level (1-6) with `.class("cn-h{level}")`
///
/// Levels outside 1-6 are clamped to the nearest valid level. A signal
/// here replaces the text in place.
pub fn heading(level: u8, content: impl blinc_layout::binding::IntoReactive<String>) -> Text {
    let clamped = level.clamp(1, 6);
    let class_name = format!("cn-h{}", clamped);
    blinc_layout::typography::heading(level, content).class(class_name)
}

/// Create a paragraph text element (16px, line-height 1.5) with `.class("cn-p")`
pub fn p(content: impl Into<String>) -> Text {
    blinc_layout::typography::p(content).class("cn-p")
}

/// Create muted/secondary text with `.class("cn-muted")`
pub fn muted(content: impl Into<String>) -> Text {
    blinc_layout::typography::muted(content).class("cn-muted")
}

/// Create caption text (12px, muted) with `.class("cn-caption")`
pub fn caption(content: impl Into<String>) -> Text {
    blinc_layout::typography::caption(content).class("cn-caption")
}
