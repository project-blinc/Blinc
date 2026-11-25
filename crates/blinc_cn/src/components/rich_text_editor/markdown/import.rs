//! Markdown to `RichDocument`.

use blinc_core::Color;
use pulldown_cmark::{Alignment, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use blinc_layout::styled_text::{StyledLine, TextSpan};

use crate::components::rich_text_editor::document::{Block, BlockAlign, BlockKind, RichDocument};

impl RichDocument {
    /// Parse `markdown` (CommonMark, strikethrough, checklists and tables, with the inline HTML the
    /// module lists) into a `RichDocument`. Text that names no colour takes `default_color`.
    ///
    /// # Example
    /// ```ignore
    /// use blinc_cn::components::rich_text_editor::RichDocument;
    /// use blinc_core::Color;
    ///
    /// let doc = RichDocument::from_markdown(
    ///     "# Title\n\nThis is **bold** and *italic*.",
    ///     Color::TRANSPARENT,
    /// );
    /// ```
    pub fn from_markdown(markdown: &str, default_color: Color) -> Self {
        let mut importer = Importer::new(default_color);
        let options =
            Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS | Options::ENABLE_TABLES;
        for event in Parser::new_ext(markdown, options) {
            importer.event(event);
        }
        importer.flush();
        RichDocument::from_blocks(importer.blocks)
    }
}

/// A container open around the current text.
enum Frame {
    /// A quote, and how many blocks there were before it.
    Quote(usize),
    /// A list, numbered or not.
    List(bool),
    /// An item and its nesting level.
    Item(u8),
}

/// An inline mark in force.
#[derive(Clone, PartialEq)]
enum Inline {
    Bold,
    Italic,
    Strike,
    Underline,
    Code,
    Color(Color),
    Link(String),
    /// A tag that changes nothing, kept so its closing tag finds it.
    Plain,
}

/// The block being read.
struct Pending {
    kind: BlockKind,
    indent: u8,
    lines: Vec<StyledLine>,
    align: BlockAlign,
}

impl Pending {
    fn is_empty(&self) -> bool {
        self.lines.len() == 1 && self.lines[0].text.is_empty()
    }
}

struct Importer {
    default_color: Color,
    blocks: Vec<Block>,
    pending: Option<Pending>,
    frames: Vec<Frame>,
    /// The marks in force, each with the HTML tag that opened it, or none for Markdown's own.
    marks: Vec<(Option<String>, Inline)>,
    /// The alignments of the `<div>`s around the current text.
    aligns: Vec<BlockAlign>,
}

fn empty_line() -> StyledLine {
    StyledLine {
        text: String::new(),
        spans: Vec::new(),
    }
}

impl Importer {
    fn new(default_color: Color) -> Self {
        Self {
            default_color,
            blocks: Vec::new(),
            pending: None,
            frames: Vec::new(),
            marks: Vec::new(),
            aligns: Vec::new(),
        }
    }

    /// The nesting level of the item the text is in.
    fn item_indent(&self) -> Option<u8> {
        self.frames.iter().rev().find_map(|f| match f {
            Frame::Item(indent) => Some(*indent),
            _ => None,
        })
    }

    fn open(&mut self, kind: BlockKind, indent: u8) {
        self.flush();
        self.pending = Some(Pending {
            kind,
            indent,
            lines: vec![empty_line()],
            align: self.aligns.last().copied().unwrap_or_default(),
        });
    }

    fn flush(&mut self) {
        if let Some(pending) = self.pending.take() {
            self.blocks.push(Block {
                kind: pending.kind,
                lines: pending.lines,
                indent: pending.indent,
                align: pending.align,
            });
        }
    }

    /// The span the marks in force give a run of text.
    fn span(&self, start: usize, end: usize, code: bool) -> TextSpan {
        let mut span = TextSpan {
            start,
            end,
            color: self.default_color,
            bold: false,
            italic: false,
            underline: false,
            strikethrough: false,
            code,
            link_url: None,
            token_type: None,
        };
        for (_, mark) in &self.marks {
            match mark {
                Inline::Bold => span.bold = true,
                Inline::Italic => span.italic = true,
                Inline::Strike => span.strikethrough = true,
                Inline::Underline => span.underline = true,
                Inline::Code => span.code = true,
                Inline::Color(color) => span.color = *color,
                Inline::Link(url) => {
                    span.link_url = Some(url.clone());
                    span.underline = true;
                }
                Inline::Plain => {}
            }
        }
        span
    }

    fn text(&mut self, text: &str, code: bool) {
        for (i, piece) in text.split('\n').enumerate() {
            if i > 0 {
                self.line_break();
            }
            if piece.is_empty() {
                continue;
            }
            let Some(start) = self.line().map(|l| l.text.len()) else {
                return;
            };
            let span = self.span(start, start + piece.len(), code);
            let Some(line) = self.line() else {
                return;
            };
            line.text.push_str(piece);
            match line.spans.last_mut() {
                // Parsing splits a run at each escape; the document keeps it whole.
                Some(last) if last.end == start && same_marks(last, &span) => last.end = span.end,
                _ => line.spans.push(span),
            }
        }
    }

    fn line(&mut self) -> Option<&mut StyledLine> {
        self.pending.as_mut()?.lines.last_mut()
    }

    fn line_break(&mut self) {
        match &mut self.pending {
            // A cell is one line.
            Some(pending) if pending.kind.table_columns().is_some() => self.text(" ", false),
            Some(pending) => pending.lines.push(empty_line()),
            None => {}
        }
    }

    fn push(&mut self, mark: Inline) {
        self.marks.push((None, mark));
    }

    /// Close the last of Markdown's own marks of the kind `is` accepts.
    fn pop(&mut self, is: impl Fn(&Inline) -> bool) {
        if let Some(at) = self
            .marks
            .iter()
            .rposition(|(tag, mark)| tag.is_none() && is(mark))
        {
            self.marks.remove(at);
        }
    }

    fn event(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => self.text(&text, false),
            Event::Code(code) => self.text(&code, true),
            Event::SoftBreak => self.text(" ", false),
            Event::HardBreak => self.line_break(),
            Event::Rule => {
                self.open(BlockKind::Divider, 0);
                self.flush();
            }
            Event::InlineHtml(html) => self.inline_html(&html),
            Event::Html(html) => self.html_block(html.trim()),
            Event::TaskListMarker(checked) => {
                if let Some(pending) = &mut self.pending {
                    pending.kind = BlockKind::TaskItem { checked };
                }
            }
            Event::FootnoteReference(_) => {}
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => self.start_paragraph(),
            Tag::Heading { level, .. } => {
                let level = match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    HeadingLevel::H4 => 4,
                    HeadingLevel::H5 => 5,
                    HeadingLevel::H6 => 6,
                };
                self.open(BlockKind::Heading(level), 0);
            }
            Tag::BlockQuote => {
                self.flush();
                self.frames.push(Frame::Quote(self.blocks.len()));
            }
            Tag::List(start) => {
                self.flush();
                self.frames.push(Frame::List(start.is_some()));
            }
            Tag::Item => {
                let lists = self.frames.iter().filter_map(|f| match f {
                    Frame::List(numbered) => Some(*numbered),
                    _ => None,
                });
                let depth = lists.clone().count().saturating_sub(1) as u8;
                let kind = if lists.clone().next_back() == Some(true) {
                    BlockKind::NumberedItem
                } else {
                    BlockKind::BulletItem
                };
                self.open(kind, depth);
                self.frames.push(Frame::Item(depth));
            }
            Tag::Emphasis => self.push(Inline::Italic),
            Tag::Strong => self.push(Inline::Bold),
            Tag::Strikethrough => self.push(Inline::Strike),
            Tag::Link { dest_url, .. } => self.push(Inline::Link(dest_url.into_string())),
            // Code blocks have no block of their own: they are paragraphs of inline code.
            Tag::CodeBlock(_) => {
                self.open(BlockKind::Paragraph, 0);
                self.push(Inline::Code);
            }
            Tag::Table(columns) => {
                let columns = columns.iter().map(column_align).collect();
                self.open(BlockKind::Table { columns }, 0);
                if let Some(pending) = &mut self.pending {
                    pending.lines.clear();
                }
            }
            Tag::TableCell => {
                if let Some(pending) = &mut self.pending {
                    pending.lines.push(empty_line());
                }
            }
            // The text inside other tags still arrives, with the marks in force.
            _ => {}
        }
    }

    fn start_paragraph(&mut self) {
        match self.frames.last() {
            Some(Frame::Quote(_)) => self.open(BlockKind::Quote, 0),
            Some(Frame::Item(_)) => match &self.pending {
                // An item's later paragraphs are lines of it, until a nested list closes it.
                Some(pending) if pending.is_empty() => {}
                Some(_) => self.line_break(),
                None => {
                    let indent = self.item_indent().unwrap_or(0) + 1;
                    self.open(BlockKind::Paragraph, indent);
                }
            },
            _ => self.open(BlockKind::Paragraph, 0),
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => {
                if !matches!(self.frames.last(), Some(Frame::Item(_))) {
                    self.flush();
                }
            }
            TagEnd::Heading(_) | TagEnd::Table => self.flush(),
            TagEnd::BlockQuote => {
                self.flush();
                if let Some(Frame::Quote(before)) = self.frames.pop()
                    && before == self.blocks.len()
                {
                    self.open(BlockKind::Quote, 0);
                    self.flush();
                }
            }
            TagEnd::List(_) | TagEnd::Item => {
                self.flush();
                self.frames.pop();
            }
            TagEnd::Emphasis => self.pop(|m| *m == Inline::Italic),
            TagEnd::Strong => self.pop(|m| *m == Inline::Bold),
            TagEnd::Strikethrough => self.pop(|m| *m == Inline::Strike),
            TagEnd::Link => self.pop(|m| matches!(m, Inline::Link(_))),
            TagEnd::CodeBlock => {
                self.pop(|m| *m == Inline::Code);
                // The newline that ends the code starts no line of its own.
                if let Some(pending) = &mut self.pending
                    && pending.lines.len() > 1
                    && pending.lines.last().is_some_and(|l| l.text.is_empty())
                {
                    pending.lines.pop();
                }
                self.flush();
            }
            _ => {}
        }
    }

    fn inline_html(&mut self, html: &str) {
        let Some(tag) = HtmlTag::parse(html) else {
            return;
        };
        if tag.name == "br" {
            self.line_break();
        } else if tag.closing {
            if let Some(at) = self
                .marks
                .iter()
                .rposition(|(open, _)| open.as_deref() == Some(tag.name.as_str()))
            {
                self.marks.remove(at);
            }
        } else if !tag.self_closing {
            // A task says its alignment after its box, where a tag is inline.
            if let (Some(align), Some(pending)) = (tag.align(), &mut self.pending) {
                pending.align = align;
            }
            let mark = match tag.name.as_str() {
                "b" | "strong" => Inline::Bold,
                "i" | "em" => Inline::Italic,
                "s" | "del" | "strike" => Inline::Strike,
                "u" | "ins" => Inline::Underline,
                "code" => Inline::Code,
                _ => tag
                    .style("color")
                    .and_then(|c| Color::from_hex_str(&c))
                    .map_or(Inline::Plain, Inline::Color),
            };
            self.marks.push((Some(tag.name), mark));
        }
    }

    fn html_block(&mut self, html: &str) {
        let Some(tag) = HtmlTag::parse(html) else {
            return;
        };
        match (tag.name.as_str(), tag.closing) {
            ("br", _) => {
                self.open(BlockKind::Paragraph, 0);
                self.flush();
            }
            ("div", false) => {
                let align = tag.align().unwrap_or_default();
                self.aligns.push(align);
                // An item's own `<div>` comes after the item opened.
                if let Some(pending) = self.pending.as_mut().filter(|p| p.is_empty()) {
                    pending.align = align;
                }
            }
            ("div", true) => {
                self.aligns.pop();
            }
            _ => {}
        }
    }
}

fn column_align(column: &Alignment) -> BlockAlign {
    match column {
        Alignment::None | Alignment::Left => BlockAlign::Start,
        Alignment::Center => BlockAlign::Center,
        Alignment::Right => BlockAlign::End,
    }
}

fn same_marks(a: &TextSpan, b: &TextSpan) -> bool {
    a.bold == b.bold
        && a.italic == b.italic
        && a.underline == b.underline
        && a.strikethrough == b.strikethrough
        && a.code == b.code
        && a.color == b.color
        && a.link_url == b.link_url
}

/// One HTML tag: `<name attributes>`, `</name>` or `<name/>`.
struct HtmlTag {
    name: String,
    closing: bool,
    self_closing: bool,
    attributes: String,
}

impl HtmlTag {
    fn parse(html: &str) -> Option<Self> {
        let body = html.trim().strip_prefix('<')?.strip_suffix('>')?;
        let (closing, body) = match body.strip_prefix('/') {
            Some(rest) => (true, rest),
            None => (false, body),
        };
        let (self_closing, body) = match body.strip_suffix('/') {
            Some(rest) => (true, rest),
            None => (false, body),
        };
        let end = body
            .find(|c: char| !c.is_ascii_alphanumeric())
            .unwrap_or(body.len());
        let name = body[..end].to_ascii_lowercase();
        (!name.is_empty() && !body.contains('<')).then(|| Self {
            name,
            closing,
            self_closing,
            attributes: body[end..].to_string(),
        })
    }

    /// The value of a quoted attribute.
    fn attribute(&self, name: &str) -> Option<String> {
        let rest = self.attributes.split_once(&format!("{name}="))?.1;
        let quote = rest.chars().next().filter(|c| matches!(c, '"' | '\''))?;
        rest[1..].split_once(quote).map(|(v, _)| v.to_string())
    }

    /// The alignment an opening tag names.
    fn align(&self) -> Option<BlockAlign> {
        let named = self.attribute("align").or(self.style("text-align"))?;
        match named.as_str() {
            "left" => Some(BlockAlign::Start),
            "center" => Some(BlockAlign::Center),
            "right" => Some(BlockAlign::End),
            "justify" => Some(BlockAlign::Justify),
            _ => None,
        }
    }

    /// The value of a property in the `style` attribute.
    fn style(&self, property: &str) -> Option<String> {
        self.attribute("style")?.split(';').find_map(|declaration| {
            let (name, value) = declaration.split_once(':')?;
            (name.trim() == property).then(|| value.trim().to_string())
        })
    }
}
