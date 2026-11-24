//! Markdown → `RichDocument` parser.
//!
//! Walks `pulldown_cmark` events and emits a flat list of `Block`s with
//! inline `TextSpan`s for marks. Mapping:
//!
//! | Markdown | RichDocument |
//! |---|---|
//! | `# H1` … `###### H6`        | `BlockKind::Heading(n)` |
//! | Paragraph                   | `BlockKind::Paragraph` |
//! | `> quote`                   | `BlockKind::Quote` |
//! | `- item` / `* item`         | `BlockKind::BulletItem` (`indent` follows nesting) |
//! | `1. item`                   | `BlockKind::NumberedItem` |
//! | `---` / `***`               | `BlockKind::Divider` |
//! | `**bold**`                  | `TextSpan.bold = true` |
//! | `*italic*`                  | `TextSpan.italic = true` |
//! | `~~strike~~`                | `TextSpan.strikethrough = true` |
//! | `` `code` ``                | `TextSpan.code = true` |
//! | `[label](url)`              | `TextSpan.link_url = Some(url)` |
//!
//! Tables, footnotes, and HTML blocks are intentionally **not** mapped
//! — they don't have a representation in `RichDocument` yet, and the
//! editor isn't a Markdown previewer. Use the standalone
//! `blinc_layout::markdown::markdown` widget if you need full
//! CommonMark + GFM rendering.

use blinc_core::Color;
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use blinc_layout::styled_text::{StyledLine, TextSpan};

use super::document::{Block, BlockKind, RichDocument};

impl RichDocument {
    /// Parse `markdown` (CommonMark + strikethrough) into a `RichDocument`.
    ///
    /// `default_color` is applied to every span that doesn't carry an
    /// explicit color (i.e. all spans for now — color isn't part of
    /// CommonMark syntax). Headings, lists, quotes, dividers, and the
    /// inline marks listed in the [module docs](self) all map directly
    /// to their `RichDocument` equivalents.
    ///
    /// # Example
    /// ```ignore
    /// use blinc_cn::components::rich_text_editor::RichDocument;
    /// use blinc_core::Color;
    ///
    /// let doc = RichDocument::from_markdown(
    ///     "# Title\n\nThis is **bold** and *italic*.",
    ///     Color::WHITE,
    /// );
    /// ```
    pub fn from_markdown(markdown: &str, default_color: Color) -> Self {
        let mut options = Options::empty();
        options.insert(Options::ENABLE_STRIKETHROUGH);

        let parser = Parser::new_ext(markdown, options);
        let mut walker = MarkdownWalker::new(default_color);
        for event in parser {
            walker.handle(event);
        }
        walker.finalize();
        if walker.blocks.is_empty() {
            return RichDocument::new();
        }
        RichDocument {
            blocks: walker.blocks,
        }
    }
}

/// Internal stack frame: which kind of block is currently open. We use
/// this to decide what `BlockKind` the next text run gets attached to.
#[derive(Clone, Debug)]
enum Frame {
    Paragraph,
    Heading(u8),
    Quote,
    BulletList,
    NumberedList {
        next_ordinal: u32,
    },
    /// Inside a `<li>` of the parent list. The `is_numbered` flag tells
    /// us which `BlockKind` to emit when the item closes. The `indent`
    /// is the depth at which the item was opened.
    Item {
        is_numbered: bool,
        indent: u8,
    },
}

/// Inline mark state — what flags are currently active on the next
/// text event we receive. Stacks because tags can nest.
#[derive(Clone, Debug, Default)]
struct InlineState {
    bold: bool,
    italic: bool,
    strikethrough: bool,
    link: Option<String>,
}

struct MarkdownWalker {
    default_color: Color,
    blocks: Vec<Block>,
    /// The block currently being assembled — appended to `blocks` when
    /// the matching end-tag arrives.
    current_block: Option<PendingBlock>,
    /// Lexical stack so we know which block kind we're inside. The
    /// outermost frame is at index 0; the innermost is the back.
    frames: Vec<Frame>,
    /// Inline mark state stack. Each Start(emphasis/bold/strike/link)
    /// pushes; each End() pops.
    inline_stack: Vec<InlineState>,
}

#[derive(Clone, Debug)]
struct PendingBlock {
    kind: BlockKind,
    indent: u8,
    /// Accumulated `(start_byte, end_byte, marks)` segments. The text
    /// itself is in `text`. We resolve segments into `TextSpan`s when
    /// the block is finalized.
    segments: Vec<PendingSegment>,
    text: String,
}

#[derive(Clone, Debug)]
struct PendingSegment {
    start: usize,
    end: usize,
    bold: bool,
    italic: bool,
    strikethrough: bool,
    code: bool,
    link: Option<String>,
}

impl MarkdownWalker {
    fn new(default_color: Color) -> Self {
        Self {
            default_color,
            blocks: Vec::new(),
            current_block: None,
            frames: Vec::new(),
            inline_stack: vec![InlineState::default()],
        }
    }

    fn current_inline(&self) -> InlineState {
        self.inline_stack.last().cloned().unwrap_or_default()
    }

    fn push_inline_with<F>(&mut self, mutator: F)
    where
        F: FnOnce(&mut InlineState),
    {
        let mut next = self.current_inline();
        mutator(&mut next);
        self.inline_stack.push(next);
    }

    fn pop_inline(&mut self) {
        if self.inline_stack.len() > 1 {
            self.inline_stack.pop();
        }
    }

    fn current_indent(&self) -> u8 {
        // Item frames carry their own indent — use the deepest one.
        self.frames
            .iter()
            .rev()
            .find_map(|f| match f {
                Frame::Item { indent, .. } => Some(*indent),
                _ => None,
            })
            .unwrap_or(0)
    }

    fn list_depth(&self) -> u8 {
        self.frames
            .iter()
            .filter(|f| matches!(f, Frame::BulletList | Frame::NumberedList { .. }))
            .count()
            .saturating_sub(1) as u8
    }

    fn open_block(&mut self, kind: BlockKind) {
        // Close any previously-open block first (defensive — pulldown
        // shouldn't allow nesting blocks like this, but better to flush
        // than corrupt state).
        if self.current_block.is_some() {
            self.flush_block();
        }
        self.current_block = Some(PendingBlock {
            kind,
            indent: self.current_indent(),
            segments: Vec::new(),
            text: String::new(),
        });
    }

    fn flush_block(&mut self) {
        let Some(pending) = self.current_block.take() else {
            return;
        };
        if matches!(pending.kind, BlockKind::Divider) {
            self.blocks.push(Block {
                kind: BlockKind::Divider,
                lines: vec![StyledLine {
                    text: String::new(),
                    spans: Vec::new(),
                }],
                indent: pending.indent,
            });
            return;
        }
        let mut spans = Vec::with_capacity(pending.segments.len());
        for seg in pending.segments {
            if seg.start >= seg.end {
                continue;
            }
            spans.push(TextSpan {
                start: seg.start,
                end: seg.end,
                color: self.default_color,
                bold: seg.bold,
                italic: seg.italic,
                underline: seg.link.is_some(),
                strikethrough: seg.strikethrough,
                code: seg.code,
                link_url: seg.link,
                token_type: None,
            });
        }
        // If the block ended up with no spans (e.g. an empty heading),
        // synthesize a single covering span so the line still renders
        // with a usable cursor target.
        if spans.is_empty() && !pending.text.is_empty() {
            spans.push(TextSpan {
                start: 0,
                end: pending.text.len(),
                color: self.default_color,
                bold: false,
                italic: false,
                underline: false,
                strikethrough: false,
                code: false,
                link_url: None,
                token_type: None,
            });
        }
        self.blocks.push(Block {
            kind: pending.kind,
            lines: vec![StyledLine {
                text: pending.text,
                spans,
            }],
            indent: pending.indent,
        });
    }

    fn append_text(&mut self, text: &str, code: bool) {
        let Some(block) = self.current_block.as_mut() else {
            return;
        };
        let start = block.text.len();
        block.text.push_str(text);
        let end = block.text.len();
        let inline = if let Some(top) = self.inline_stack.last() {
            top.clone()
        } else {
            InlineState::default()
        };
        block.segments.push(PendingSegment {
            start,
            end,
            bold: inline.bold,
            italic: inline.italic,
            strikethrough: inline.strikethrough,
            code,
            link: inline.link,
        });
    }

    fn handle(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => {
                if self.current_block.is_some() {
                    self.append_text(&text, false);
                }
            }
            Event::Code(code) => {
                if self.current_block.is_some() {
                    self.append_text(&code, true);
                }
            }
            Event::SoftBreak => {
                if self.current_block.is_some() {
                    self.append_text(" ", false);
                }
            }
            Event::HardBreak => {
                if self.current_block.is_some() {
                    self.append_text("\n", false);
                }
            }
            Event::Rule => {
                self.open_block(BlockKind::Divider);
                self.flush_block();
            }
            // Tables, footnotes, HTML, task markers, inline HTML — not
            // mapped. The corresponding text content (if any) still
            // arrives via `Event::Text` so it's not lost.
            Event::Html(_) | Event::InlineHtml(_) => {}
            Event::FootnoteReference(_) => {}
            Event::TaskListMarker(_) => {}
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => {
                // Paragraphs inside a list item OR a blockquote become
                // the parent block's text; we already opened the item
                // / quote block earlier, so skip the redundant open.
                let inside_container =
                    matches!(self.frames.last(), Some(Frame::Item { .. } | Frame::Quote));
                if !inside_container {
                    self.open_block(BlockKind::Paragraph);
                }
                self.frames.push(Frame::Paragraph);
            }
            Tag::Heading { level, .. } => {
                let n = match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    HeadingLevel::H4 => 4,
                    HeadingLevel::H5 => 5,
                    HeadingLevel::H6 => 6,
                };
                self.open_block(BlockKind::Heading(n));
                self.frames.push(Frame::Heading(n));
            }
            Tag::BlockQuote => {
                self.open_block(BlockKind::Quote);
                self.frames.push(Frame::Quote);
            }
            Tag::List(start) => {
                if let Some(start) = start {
                    self.frames.push(Frame::NumberedList {
                        next_ordinal: start as u32,
                    });
                } else {
                    self.frames.push(Frame::BulletList);
                }
            }
            Tag::Item => {
                let depth = self.list_depth();
                let is_numbered = matches!(
                    self.frames
                        .iter()
                        .rev()
                        .find(|f| matches!(f, Frame::BulletList | Frame::NumberedList { .. })),
                    Some(Frame::NumberedList { .. })
                );
                if let Some(Frame::NumberedList { next_ordinal }) = self
                    .frames
                    .iter_mut()
                    .rev()
                    .find(|f| matches!(f, Frame::NumberedList { .. }))
                {
                    *next_ordinal += 1;
                }
                self.frames.push(Frame::Item {
                    is_numbered,
                    indent: depth,
                });
                let kind = if is_numbered {
                    BlockKind::NumberedItem
                } else {
                    BlockKind::BulletItem
                };
                self.open_block(kind);
            }
            Tag::Emphasis => self.push_inline_with(|s| s.italic = true),
            Tag::Strong => self.push_inline_with(|s| s.bold = true),
            Tag::Strikethrough => self.push_inline_with(|s| s.strikethrough = true),
            Tag::Link { dest_url, .. } => {
                let url = dest_url.into_string();
                self.push_inline_with(|s| s.link = Some(url));
            }
            // Code blocks are not yet first-class in `RichDocument`; we
            // emit them as plain paragraphs with the code text inline
            // so the editor at least preserves the content.
            Tag::CodeBlock(_) => {
                self.open_block(BlockKind::Paragraph);
                self.frames.push(Frame::Paragraph);
                // Force the next text run to be marked as code so it
                // renders as a chip — close enough to a code block for
                // a single-line snippet, and still readable for longer.
                self.push_inline_with(|_| {});
            }
            // Inline tags we don't track explicitly: image, footnote
            // definition, table, table cell, etc. Their inner text still
            // arrives via Event::Text under whatever inline state is
            // active.
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => {
                if matches!(self.frames.last(), Some(Frame::Paragraph)) {
                    self.frames.pop();
                }
                // Flush only if the paragraph wasn't part of a list
                // item or a blockquote — those flush on their own
                // TagEnd::Item / TagEnd::BlockQuote.
                if !matches!(self.frames.last(), Some(Frame::Item { .. } | Frame::Quote)) {
                    self.flush_block();
                }
            }
            TagEnd::Heading(_) => {
                if matches!(self.frames.last(), Some(Frame::Heading(_))) {
                    self.frames.pop();
                }
                self.flush_block();
            }
            TagEnd::BlockQuote => {
                if matches!(self.frames.last(), Some(Frame::Quote)) {
                    self.frames.pop();
                }
                self.flush_block();
            }
            TagEnd::List(_) => {
                if matches!(
                    self.frames.last(),
                    Some(Frame::BulletList | Frame::NumberedList { .. })
                ) {
                    self.frames.pop();
                }
            }
            TagEnd::Item => {
                if matches!(self.frames.last(), Some(Frame::Item { .. })) {
                    self.frames.pop();
                }
                self.flush_block();
            }
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => self.pop_inline(),
            TagEnd::Link => self.pop_inline(),
            TagEnd::CodeBlock => {
                if matches!(self.frames.last(), Some(Frame::Paragraph)) {
                    self.frames.pop();
                }
                self.pop_inline();
                self.flush_block();
            }
            _ => {}
        }
    }

    fn finalize(&mut self) {
        self.flush_block();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_headings_and_paragraphs() {
        let doc = RichDocument::from_markdown("# Hi\n\nWorld", Color::WHITE);
        assert_eq!(doc.blocks.len(), 2);
        assert_eq!(doc.blocks[0].kind, BlockKind::Heading(1));
        assert_eq!(doc.blocks[0].lines[0].text, "Hi");
        assert_eq!(doc.blocks[1].kind, BlockKind::Paragraph);
        assert_eq!(doc.blocks[1].lines[0].text, "World");
    }

    #[test]
    fn parses_bold_italic_strike() {
        let doc = RichDocument::from_markdown("**bold** *italic* ~~strike~~", Color::WHITE);
        let line = &doc.blocks[0].lines[0];
        assert_eq!(line.text, "bold italic strike");
        assert!(
            line.spans
                .iter()
                .any(|s| s.bold && line.text[s.start..s.end] == *"bold")
        );
        assert!(
            line.spans
                .iter()
                .any(|s| s.italic && line.text[s.start..s.end] == *"italic")
        );
        assert!(
            line.spans
                .iter()
                .any(|s| s.strikethrough && line.text[s.start..s.end] == *"strike")
        );
    }

    #[test]
    fn parses_inline_code() {
        let doc = RichDocument::from_markdown("Use `print()` here", Color::WHITE);
        let line = &doc.blocks[0].lines[0];
        assert!(
            line.spans
                .iter()
                .any(|s| s.code && line.text[s.start..s.end] == *"print()")
        );
    }

    #[test]
    fn parses_link_with_underline() {
        let doc = RichDocument::from_markdown("[click](https://example.com)", Color::WHITE);
        let line = &doc.blocks[0].lines[0];
        let link_span = line.spans.iter().find(|s| s.link_url.is_some()).unwrap();
        assert_eq!(link_span.link_url.as_deref(), Some("https://example.com"));
        assert!(link_span.underline);
    }

    #[test]
    fn parses_bullet_and_numbered_lists() {
        let doc = RichDocument::from_markdown("- one\n- two\n\n1. first\n2. second", Color::WHITE);
        let kinds: Vec<_> = doc.blocks.iter().map(|b| b.kind.clone()).collect();
        assert_eq!(
            kinds,
            vec![
                BlockKind::BulletItem,
                BlockKind::BulletItem,
                BlockKind::NumberedItem,
                BlockKind::NumberedItem,
            ]
        );
    }

    #[test]
    fn parses_blockquote() {
        let doc = RichDocument::from_markdown("> a wise quote", Color::WHITE);
        assert_eq!(doc.blocks[0].kind, BlockKind::Quote);
        assert_eq!(doc.blocks[0].lines[0].text, "a wise quote");
    }

    #[test]
    fn parses_horizontal_rule() {
        let doc = RichDocument::from_markdown("before\n\n---\n\nafter", Color::WHITE);
        assert!(
            doc.blocks
                .iter()
                .any(|b| matches!(b.kind, BlockKind::Divider))
        );
    }

    #[test]
    fn empty_input_returns_empty_document() {
        let doc = RichDocument::from_markdown("", Color::WHITE);
        // RichDocument::new() yields a single empty paragraph
        assert_eq!(doc.blocks.len(), 1);
        assert_eq!(doc.blocks[0].kind, BlockKind::Paragraph);
    }
}
