//! Import, export and the round trip between them.

use blinc_core::Color;

use blinc_layout::styled_text::{StyledLine, TextSpan};

use super::inline::{runs, spaces_outside_emphasis};
use crate::components::rich_text_editor::document::{Block, BlockKind, RichDocument};

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

/// A document as blocks of kind, indent and the runs of each line, which is what it says.
/// Emphasis on the spaces at a run's ends says nothing.
fn said(doc: &RichDocument) -> Vec<(BlockKind, u8, Vec<Vec<super::inline::Run>>)> {
    doc.blocks
        .iter()
        .map(|b| {
            let lines = b
                .lines
                .iter()
                .map(|l| spaces_outside_emphasis(runs(l, Color::TRANSPARENT)))
                .collect();
            (b.kind.clone(), b.indent, lines)
        })
        .collect()
}

/// `doc` comes back from its Markdown as it was.
#[track_caller]
fn round_trips(doc: &RichDocument) -> String {
    let markdown = doc.to_markdown(Color::TRANSPARENT);
    let back = RichDocument::from_markdown(&markdown, Color::TRANSPARENT);
    assert_eq!(said(&back), said(doc), "as Markdown:\n{markdown}");
    markdown
}

#[derive(Clone, Copy, Default)]
struct M {
    b: bool,
    i: bool,
    s: bool,
    u: bool,
    c: bool,
    color: Option<Color>,
    link: Option<&'static str>,
}

/// A line of runs.
fn line(parts: &[(&str, M)]) -> StyledLine {
    let mut text = String::new();
    let mut spans = Vec::new();
    for (part, m) in parts {
        let start = text.len();
        text.push_str(part);
        spans.push(TextSpan {
            start,
            end: text.len(),
            color: m.color.unwrap_or(Color::TRANSPARENT),
            bold: m.b,
            italic: m.i,
            underline: m.u || m.link.is_some(),
            strikethrough: m.s,
            code: m.c,
            link_url: m.link.map(String::from),
            token_type: None,
        });
    }
    StyledLine { text, spans }
}

fn block(kind: BlockKind, indent: u8, lines: Vec<StyledLine>) -> Block {
    Block {
        kind,
        lines,
        indent,
    }
}

fn paragraph(parts: &[(&str, M)]) -> RichDocument {
    RichDocument::from_blocks(vec![block(BlockKind::Paragraph, 0, vec![line(parts)])])
}

const P: M = M {
    b: false,
    i: false,
    s: false,
    u: false,
    c: false,
    color: None,
    link: None,
};
const B: M = M { b: true, ..P };
const I: M = M { i: true, ..P };
const S: M = M { s: true, ..P };
const U: M = M { u: true, ..P };
const C: M = M { c: true, ..P };
const BI: M = M { i: true, ..B };

#[test]
fn markdown_comes_out_as_it_went_in() {
    for markdown in [
        "# Title\n\nSome **bold**, *italic*, ~~struck~~ and `code`.\n",
        "[a link](https://blinc.rs) and [**a bold one**](https://blinc.rs/a)\n",
        "- one\n- two\n  - nested\n- three\n",
        "1. one\n2. two\n   - nested\n",
        "> a quote\n\n> another\n\n---\n\nafter\n",
        "first\\\nsecond\n",
        "<u>under</u> and <span style=\"color:#ff0000\">red</span>\n",
    ] {
        let doc = RichDocument::from_markdown(markdown, Color::TRANSPARENT);
        assert_eq!(doc.to_markdown(Color::TRANSPARENT), markdown);
    }
}

#[test]
fn every_pair_of_marks_round_trips_however_they_meet() {
    let all = [
        P,
        B,
        I,
        S,
        U,
        C,
        BI,
        M {
            color: Some(Color::from_hex(0x22c55e)),
            ..I
        },
        M {
            link: Some("https://x.y/z"),
            ..B
        },
    ];
    let texts = ["word", " led", "trailed ", "(punct)", "a.", "\"q\"", "é"];
    for left in all {
        for right in all {
            for a in texts {
                for b in texts {
                    round_trips(&paragraph(&[(a, left), (b, right), ("end", P)]));
                }
            }
        }
    }
}

#[test]
fn text_that_looks_like_markdown_stays_text() {
    for text in [
        "# not a heading",
        "#",
        "ends with #",
        "- not an item",
        "+ nor this",
        "* nor this",
        "1. nor this",
        "12) nor this",
        "> not a quote",
        "---",
        "===",
        "***",
        "~~~",
        "```",
        "*not italic*",
        "**not bold**",
        "_not italic_",
        "snake_case_name",
        "a * b * c",
        "`not code`",
        "[not](a link)",
        "![not](an image)",
        "<u>not a tag</u>",
        "<br>",
        "a < b > c",
        "AT&T",
        "&amp; &#32;",
        "back\\slash",
        "\\*",
        "a | b",
        "| a | b |",
        "~~not struck~~",
        "http://plain.url/a_b",
        "- [ ] not a task",
        "[x] nor this",
        "    not code",
        "  led",
        "trailed  ",
        "two  spaces",
        "\ttabbed",
        " ",
        "1986. A year",
        "a\u{a0}b",
        "emoji 🎉 *x*",
        "=",
        "+",
        "-",
        ">",
        "<!-- x -->",
        "[ref]: http://x",
        "a\\",
        "end!",
        "![",
        "!",
        "`",
        "``",
        "**",
        "_",
    ] {
        round_trips(&paragraph(&[(text, P)]));
        round_trips(&paragraph(&[("x ", P), (text, B)]));
        for kind in [
            BlockKind::Heading(2),
            BlockKind::Quote,
            BlockKind::BulletItem,
            BlockKind::NumberedItem,
        ] {
            round_trips(&RichDocument::from_blocks(vec![block(
                kind,
                0,
                vec![line(&[(text, P)])],
            )]));
        }
    }
}

#[test]
fn code_spans_hold_any_code() {
    for code in [
        "a", "`", "``", "a`b", "`a`", " a ", " a", "  ", "<u>", "*a*", "\\", "a|b", "&amp;",
    ] {
        round_trips(&paragraph(&[("x", P), (code, C), ("y", P)]));
        round_trips(&paragraph(&[(code, M { c: true, ..B })]));
    }
}

#[test]
fn links_keep_any_address() {
    for url in [
        "https://a.b/c?d=e&f=g",
        "a b",
        "(x)",
        "a)b",
        "<x>",
        "a\\b",
        "",
        "#anchor",
        "a&amp;b",
    ] {
        round_trips(&paragraph(&[
            ("see ", P),
            (
                "the [link]",
                M {
                    link: Some(url),
                    ..P
                },
            ),
            ("!", P),
        ]));
    }
    // One link over runs of different marks is one link.
    let link = M {
        link: Some("https://x.y"),
        ..P
    };
    let doc = paragraph(&[
        ("a ", link),
        ("bold", M { b: true, ..link }),
        (" link", link),
    ]);
    assert_eq!(round_trips(&doc), "[a **bold** link](https://x.y)\n");
}

#[test]
fn lines_and_empty_blocks_round_trip() {
    let l = |t: &str| line(&[(t, P)]);
    let empty = || StyledLine {
        text: String::new(),
        spans: Vec::new(),
    };
    for kind in [
        BlockKind::Paragraph,
        BlockKind::Heading(1),
        BlockKind::Quote,
        BlockKind::BulletItem,
        BlockKind::NumberedItem,
    ] {
        for lines in [
            vec![l("a"), l("b")],
            vec![l("a"), empty(), l("b")],
            vec![empty(), l("b")],
            vec![l("a"), empty()],
            vec![l("# a"), l("- b")],
            vec![empty()],
        ] {
            round_trips(&RichDocument::from_blocks(vec![
                block(kind.clone(), 0, lines),
                block(BlockKind::Paragraph, 0, vec![l("after")]),
            ]));
        }
    }
    let doc = RichDocument::from_blocks(vec![
        block(BlockKind::Paragraph, 0, vec![l("a")]),
        Block::paragraph_empty(),
        Block::paragraph_empty(),
        block(BlockKind::Paragraph, 0, vec![l("b")]),
    ]);
    assert_eq!(round_trips(&doc), "a\n\n<br>\n\n<br>\n\nb\n");
    assert_eq!(RichDocument::new().to_markdown(Color::TRANSPARENT), "");
}

#[test]
fn lists_keep_their_nesting() {
    use BlockKind::{BulletItem, NumberedItem, Paragraph};
    let item = |kind, indent, t: &str| block(kind, indent, vec![line(&[(t, P)])]);
    for blocks in [
        vec![
            item(BulletItem, 0, "a"),
            item(BulletItem, 1, "b"),
            item(BulletItem, 2, "c"),
            item(BulletItem, 0, "d"),
        ],
        vec![
            item(NumberedItem, 0, "a"),
            item(NumberedItem, 1, "b"),
            item(NumberedItem, 1, "c"),
            item(NumberedItem, 0, "d"),
        ],
        vec![
            item(BulletItem, 0, "a"),
            item(NumberedItem, 0, "b"),
            item(BulletItem, 0, "c"),
        ],
        vec![
            item(NumberedItem, 0, "a"),
            item(BulletItem, 1, "b"),
            item(NumberedItem, 2, "c"),
            item(BulletItem, 1, "d"),
        ],
        vec![
            item(Paragraph, 0, "p"),
            item(NumberedItem, 0, "a"),
            item(Paragraph, 0, "q"),
            item(BulletItem, 0, "b"),
        ],
    ] {
        round_trips(&RichDocument::from_blocks(blocks));
    }
    let ten: Vec<Block> = (0..11)
        .flat_map(|n| {
            [
                item(NumberedItem, 0, &format!("item {n}")),
                item(BulletItem, 1, "nested"),
            ]
        })
        .collect();
    round_trips(&RichDocument::from_blocks(ten));
}

#[test]
fn the_default_color_is_not_written() {
    let doc = RichDocument::from_markdown("plain **bold**", Color::WHITE);
    assert_eq!(doc.to_markdown(Color::WHITE), "plain **bold**\n");
    assert!(doc.to_markdown(Color::TRANSPARENT).contains("color:#"));
    let translucent = Color::rgba(1.0, 0.0, 0.0, 0.5);
    assert!(
        round_trips(&paragraph(&[(
            "x",
            M {
                color: Some(translucent),
                ..P
            }
        )]))
        .contains("#ff000080")
    );
}

#[test]
fn the_importer_keeps_what_it_once_lost() {
    let read = |md: &str| RichDocument::from_markdown(md, Color::TRANSPARENT);
    let texts = |md: &str| {
        read(md)
            .blocks
            .iter()
            .map(|b| b.plain_text())
            .collect::<Vec<_>>()
    };
    // A hard break is a line of its own.
    assert_eq!(read("a  \nb").blocks[0].lines.len(), 2);
    // A quote's paragraphs stay apart, and a list in a quote is no empty quote.
    assert_eq!(texts("> a\n>\n> b"), ["a", "b"]);
    assert_eq!(texts("> - a"), ["a"]);
    // An item's later paragraphs, before and after a nested list.
    assert_eq!(texts("- a\n\n  b"), ["a\nb"]);
    assert_eq!(texts("- a\n  - b\n\n  c"), ["a", "b", "c"]);
    // A code block keeps its lines, as code.
    let code = read("```\nlet a = 1;\nlet b = 2;\n```");
    assert_eq!(code.blocks[0].plain_text(), "let a = 1;\nlet b = 2;");
    assert!(code.blocks[0].lines.iter().all(|l| l.spans[0].code));
    // Tags that cross and that nothing opened do no harm.
    let crossed = read("<u>a **b</u> c** d</em>");
    assert_eq!(crossed.blocks[0].plain_text(), "a b c d");
}

#[test]
fn common_text_needs_no_tags() {
    for (parts, markdown) in [
        (
            vec![("bold ", B), ("then plain", P)],
            "**bold** then plain\n",
        ),
        (vec![("plain", P), (" italic", I)], "plain *italic*\n"),
        (
            vec![("one ", B), ("two ", BI), ("three", P)],
            "**one *two*** three\n",
        ),
        (
            vec![("mid", P), ("word", B), ("bold", P)],
            "mid**word**bold\n",
        ),
        (
            vec![("snake_case, a.b and 3 - 1 = 2", P)],
            "snake_case, a.b and 3 - 1 = 2\n",
        ),
    ] {
        assert_eq!(round_trips(&paragraph(&parts)), markdown);
    }
}
