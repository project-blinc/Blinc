//! Markdown to and from a `RichDocument`.
//!
//! | Markdown | RichDocument |
//! |---|---|
//! | `# H1` to `###### H6`       | `BlockKind::Heading(n)` |
//! | Paragraph                   | `BlockKind::Paragraph` |
//! | `> quote`                   | `BlockKind::Quote` |
//! | `- item`, `* item`          | `BlockKind::BulletItem` (`indent` follows nesting) |
//! | `1. item`                   | `BlockKind::NumberedItem` |
//! | `---`, `***`                | `BlockKind::Divider` |
//! | `**bold**`, `<strong>`      | `TextSpan.bold` |
//! | `*italic*`, `<em>`          | `TextSpan.italic` |
//! | `~~strike~~`, `<del>`       | `TextSpan.strikethrough` |
//! | `` `code` ``                | `TextSpan.code` |
//! | `[label](url)`              | `TextSpan.link_url` |
//! | `<u>underline</u>`          | `TextSpan.underline` |
//! | `<span style="color:#rrggbb">` | `TextSpan.color` |
//! | `\\` at a line's end, `<br>`  | a soft break: a new line in the block |
//! | `<br>` alone                | an empty paragraph |
//!
//! Footnotes and other HTML are not mapped. Use `blinc_layout::markdown::markdown` for full
//! CommonMark and GFM rendering.

mod export;
mod import;
mod inline;

#[cfg(test)]
mod tests;
