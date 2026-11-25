//! `RichDocument` to Markdown.

use blinc_core::Color;

use super::inline::{self, Breaks, Emphasis};
use crate::components::rich_text_editor::document::{Block, BlockAlign, BlockKind, RichDocument};

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
        let mut aligned = BlockAlign::Start;
        for (i, block) in self.blocks.iter().enumerate() {
            let listed = is_item(block);
            // Items say their alignment themselves, so a list is not cut up by it.
            let align = if listed {
                BlockAlign::Start
            } else {
                block.align
            };
            if i > 0 {
                let before = &self.blocks[i - 1];
                // An item's `</div>` runs on to the next blank line.
                let tight = listed && is_item(before) && before.align == BlockAlign::Start;
                out.push_str(if tight { "\n" } else { "\n\n" });
            }
            if align != aligned {
                if aligned != BlockAlign::Start {
                    out.push_str("</div>\n\n");
                }
                if let Some(open) = open_div(align) {
                    out.push_str(&open);
                    out.push_str("\n\n");
                }
                aligned = align;
            }
            let text = if listed {
                let marker = match self.numbered_ordinal(i) {
                    Some(ordinal) => format!("{ordinal}. "),
                    None => "- ".to_string(),
                };
                let (indent, content) = items.enter(block.indent, marker.len());
                let margin = " ".repeat(content);
                let body = write_block(block, default_color, Breaks::Lines(&margin));
                let body = match (&block.kind, open_div(block.align)) {
                    (BlockKind::TaskItem { checked }, _) => {
                        let tick = if *checked { "[x]" } else { "[ ]" };
                        // A box with nothing after it is text.
                        let body = if body.is_empty() {
                            "<span></span>"
                        } else {
                            &body
                        };
                        // A block's tag may not follow a box.
                        match text_align(block.align) {
                            Some(align) => format!(
                                "{tick} <span style=\"display:block;text-align:{align}\">{body}</span>"
                            ),
                            None => format!("{tick} {body}"),
                        }
                    }
                    (_, Some(open)) => format!("{open}\n\n{margin}{body}\n\n{margin}</div>"),
                    (_, None) => body,
                };
                format!("{}{marker}{body}", " ".repeat(indent))
            } else {
                items = ItemIndents::default();
                write_block(block, default_color, Breaks::Lines(""))
            };
            out.push_str(text.trim_end());
        }
        if aligned != BlockAlign::Start {
            out.push_str("\n\n</div>");
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

/// The tag that aligns what follows it, up to its close.
fn open_div(align: BlockAlign) -> Option<String> {
    text_align(align).map(|value| format!("<div align=\"{value}\">"))
}

/// An alignment as CSS and HTML name it, but for the default.
fn text_align(align: BlockAlign) -> Option<&'static str> {
    match align {
        BlockAlign::Start => None,
        BlockAlign::Center => Some("center"),
        BlockAlign::End => Some("right"),
        BlockAlign::Justify => Some("justify"),
    }
}

fn is_item(block: &Block) -> bool {
    block.kind.is_list_item()
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
        BlockKind::Table { columns } => table(block, columns, default_color, emphasis),
        BlockKind::Quote => format!("> {}", text(Breaks::Lines("> "))),
        BlockKind::Paragraph if block.is_empty() && block.lines.len() == 1 => {
            EMPTY_PARAGRAPH.into()
        }
        _ => text(breaks),
    }
}

/// A table's rows, with the row that says how its columns are aligned under the first.
fn table(
    block: &Block,
    columns: &[BlockAlign],
    default_color: Color,
    emphasis: Emphasis,
) -> String {
    let row = |cells: Vec<String>| format!("| {} |", cells.join(" | "));
    let rows = block.lines.chunks(columns.len().max(1)).map(|cells| {
        let cell = |line| {
            inline::lines(
                std::slice::from_ref(line),
                default_color,
                emphasis,
                Breaks::Cell,
            )
        };
        row(cells.iter().map(cell).collect())
    });
    let rule = |align: &BlockAlign| match align {
        BlockAlign::Start | BlockAlign::Justify => "---",
        BlockAlign::Center => ":---:",
        BlockAlign::End => "---:",
    };
    let mut out: Vec<String> = rows.collect();
    let rule = row(columns.iter().map(|a| rule(a).to_string()).collect());
    out.insert(1.min(out.len()), rule);
    out.join("\n")
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
