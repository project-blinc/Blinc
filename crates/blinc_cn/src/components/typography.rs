//! Typography components
//!
//! Re-exports typography helpers from `blinc_layout::typography` for use with the
//! blinc_cn component library. These provide semantic text elements similar to HTML.
//!
//! # Headings
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! // Named heading helpers
//! cn::h1("Welcome")           // 32px, bold
//! cn::h2("Section Title")     // 24px, bold
//! cn::h3("Subsection")        // 20px, semibold
//! cn::h4("Small Heading")     // 18px, semibold
//! cn::h5("Minor Heading")     // 16px, medium
//! cn::h6("Smallest Heading")  // 14px, medium
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

// Re-export all typography helpers from blinc_layout
pub use blinc_layout::typography::{
    // Headings
    h1, h2, h3, h4, h5, h6, heading,
    // Inline text helpers
    b, caption, chained_text, inline_code, label, muted, p, small, span, strong,
};
