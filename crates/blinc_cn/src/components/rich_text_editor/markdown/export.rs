//! `RichDocument` to Markdown.

use blinc_core::Color;

use super::inline::{self, Breaks, Emphasis};
use crate::components::rich_text_editor::document::{Block, BlockKind, RichDocument};

impl RichDocument {
    /// The document as Markdown, which [`RichDocument::from_markdown`] reads back as it was, but
    /// for emphasis on the spaces at the ends of a run.
    ///
    /// `default_color` is the colour of text that has none of its own, as given to
    /// `from_markdown`. Text in it, and transparent text, is written without a colour. What
    /// Markdown has no syntax for is written as inline HTML: underline, colour, and emphasis that
    /// starts or ends beside punctuation where a marker would not work.
    ///
    /// # Example
    /// ```ignore
    /// let doc = RichDocument::from_markdown("Some **bold** text", Color::TRANSPARENT);
    /// assert_eq!(doc.to_markdown(Color::TRANSPARENT), "Some **bold** text\n");
    /// ```
    pub fn to_markdown(&self, default_color: Color) -> String {
        let mut out = String::new();
        let mut items = ItemIndents::default();
        for (i, block) in self.blocks.iter().enumerate() {
            let text = match block.kind {
                BlockKind::BulletItem | BlockKind::NumberedItem => {
                    let marker = match self.numbered_ordinal(i) {
                        Some(ordinal) => format!("{ordinal}. "),
                        None => "- ".to_string(),
                    };
                    let (indent, content) = items.enter(block.indent, marker.len());
                    let continued = " ".repeat(content);
                    let body = write_block(block, default_color, Breaks::Lines(&continued));
                    format!("{}{marker}{body}", " ".repeat(indent))
                }
                _ => {
                    items = ItemIndents::default();
                    write_block(block, default_color, Breaks::Lines(""))
                }
            };
            if i > 0 {
                let listed = is_item(block) && is_item(&self.blocks[i - 1]);
                out.push_str(if listed { "\n" } else { "\n\n" });
            }
            out.push_str(text.trim_end());
        }
        // A document with nothing in it is an empty file.
        if out == EMPTY_PARAGRAPH && self.blocks.len() == 1 {
            return String::new();
        }
        out.push('\n');
        out
    }
}

/// How an empty paragraph is written: Markdown drops blank lines.
const EMPTY_PARAGRAPH: &str = "<br>";

fn is_item(block: &Block) -> bool {
    matches!(block.kind, BlockKind::BulletItem | BlockKind::NumberedItem)
}

/// Where the content of each open list level starts, which its nested items are indented to.
#[derive(Default)]
struct ItemIndents {
    levels: Vec<(u8, usize)>,
}

impl ItemIndents {
    /// The columns where an item at `level` starts and where its content does.
    fn enter(&mut self, level: u8, marker: usize) -> (usize, usize) {
        while self.levels.last().is_some_and(|(l, _)| *l >= level) {
            self.levels.pop();
        }
        let start = self.levels.last().map_or(0, |(_, content)| *content);
        self.levels.push((level, start + marker));
        (start, start + marker)
    }
}

/// A block without its list marker, its emphasis as Markdown's markers where they read back as
/// they were, and as tags where they do not.
fn write_block(block: &Block, default_color: Color, breaks: Breaks) -> String {
    let markers = write_block_as(block, default_color, Emphasis::Markers, breaks);
    if reads_back(block, default_color, &markers) {
        return markers;
    }
    write_block_as(block, default_color, Emphasis::Tags, breaks)
}

fn write_block_as(
    block: &Block,
    default_color: Color,
    emphasis: Emphasis,
    breaks: Breaks,
) -> String {
    let text = |breaks| inline::lines(&block.lines, default_color, emphasis, breaks);
    match &block.kind {
        BlockKind::Divider => "---".into(),
        BlockKind::Heading(level) => {
            format!(
                "{} {}",
                "#".repeat((*level).clamp(1, 6) as usize),
                text(Breaks::Tags)
            )
        }
        BlockKind::Quote => format!("> {}", text(Breaks::Lines("> "))),
        BlockKind::Paragraph if block.is_empty() && block.lines.len() == 1 => {
            EMPTY_PARAGRAPH.into()
        }
        BlockKind::Paragraph | BlockKind::BulletItem | BlockKind::NumberedItem => text(breaks),
    }
}

/// Whether `markdown`, written for `block`, has the block's text and marks, emphasis on the spaces
/// at a run's ends aside.
fn reads_back(block: &Block, default_color: Color, markdown: &str) -> bool {
    // An item's text is read as a paragraph, with its lines at the margin.
    let markdown: String = match is_item(block) {
        true => markdown
            .lines()
            .map(|l| l.trim_start().to_string() + "\n")
            .collect(),
        false => markdown.to_string(),
    };
    let read = RichDocument::from_markdown(&markdown, default_color);
    let [read] = read.blocks.as_slice() else {
        return false;
    };
    let said = |line| inline::spaces_outside_emphasis(inline::runs(line, default_color));
    read.lines.len() == block.lines.len()
        && read
            .lines
            .iter()
            .zip(&block.lines)
            .all(|(a, b)| said(a) == said(b))
}
