//! The text of one block as Markdown: marks, escapes and line breaks.

use blinc_core::Color;
use blinc_layout::styled_text::StyledLine;

/// How emphasis is written. Markdown's own markers do not open or close beside some punctuation,
/// and tags always do.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Emphasis {
    Markers,
    Tags,
}

/// How the lines of a block are joined.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Breaks<'a> {
    /// A backslash and a new line, which starts with this.
    Lines(&'a str),
    /// `<br>`, where the block must stay on one line of Markdown.
    Tags,
    /// None: a table's cell is one line, and a `|` in it ends it, even in a code span.
    Cell,
}

/// The marks of a run of text.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Marks {
    bold: bool,
    italic: bool,
    strike: bool,
    underline: bool,
    code: bool,
    /// As `#rrggbb`, or `#rrggbbaa` where it is translucent; none for the default colour.
    color: Option<String>,
    link: Option<String>,
}

pub(super) type Run = (String, Marks);

/// A line as runs of text with the same marks, `default_color` and transparent text having no
/// colour of their own. A link is underlined as a link is, which is no mark.
pub(super) fn runs(line: &StyledLine, default_color: Color) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::new();
    let mut add = |text: &str, marks: Marks| {
        if text.is_empty() {
            return;
        }
        match out.last_mut() {
            Some((last, m)) if *m == marks => last.push_str(text),
            _ => out.push((text.to_string(), marks)),
        }
    };
    let mut at = 0;
    for span in &line.spans {
        let (start, end) = (span.start.max(at), span.end.min(line.text.len()));
        let (Some(gap), Some(text)) = (line.text.get(at..start), line.text.get(start..end)) else {
            continue;
        };
        add(gap, Marks::default());
        let own_color = span.color.a > 0.0 && span.color != default_color;
        add(
            text,
            Marks {
                bold: span.bold,
                italic: span.italic,
                strike: span.strikethrough,
                underline: span.underline && span.link_url.is_none(),
                code: span.code,
                color: own_color.then(|| span.color.to_hex_string(span.color.a < 1.0)),
                link: span.link_url.clone(),
            },
        );
        at = end;
    }
    add(line.text.get(at..).unwrap_or_default(), Marks::default());
    out
}

/// The lines of a block as Markdown.
pub(super) fn lines(
    lines: &[StyledLine],
    default_color: Color,
    emphasis: Emphasis,
    breaks: Breaks,
) -> String {
    let mut out = String::new();
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            // A backslash ends a line only with text on both sides of it.
            let joined = !lines[i - 1].text.is_empty() && !line.text.is_empty();
            match breaks {
                Breaks::Lines(indent) if joined => {
                    out.push_str("\\\n");
                    out.push_str(indent);
                }
                _ => out.push_str("<br>"),
            }
        }
        let cell = breaks == Breaks::Cell;
        out.push_str(&write_line(runs(line, default_color), emphasis, cell));
    }
    out
}

/// A mark being written, outermost first.
#[derive(Clone, PartialEq)]
enum Token {
    Link(String),
    Color(String),
    Underline,
    Bold,
    Italic,
    Strike,
    /// Code as a tag, with its text escaped as any other.
    Code,
}

impl Token {
    fn of(marks: &Marks) -> Vec<Token> {
        let mut tokens = Vec::new();
        tokens.extend(marks.link.clone().map(Token::Link));
        tokens.extend(marks.color.clone().map(Token::Color));
        if marks.underline {
            tokens.push(Token::Underline);
        }
        if marks.bold {
            tokens.push(Token::Bold);
        }
        if marks.italic {
            tokens.push(Token::Italic);
        }
        if marks.strike {
            tokens.push(Token::Strike);
        }
        tokens
    }

    fn open(&self, emphasis: Emphasis) -> String {
        let tags = emphasis == Emphasis::Tags;
        match self {
            Token::Link(_) => "[".into(),
            Token::Color(color) => format!("<span style=\"color:{color}\">"),
            Token::Underline => "<u>".into(),
            Token::Code => "<code>".into(),
            Token::Bold if tags => "<strong>".into(),
            Token::Italic if tags => "<em>".into(),
            Token::Strike if tags => "<del>".into(),
            Token::Bold => "**".into(),
            Token::Italic => "*".into(),
            Token::Strike => "~~".into(),
        }
    }

    fn close(&self, emphasis: Emphasis) -> String {
        let tags = emphasis == Emphasis::Tags;
        match self {
            Token::Link(url) => format!("]({})", destination(url)),
            Token::Color(_) => "</span>".into(),
            Token::Underline => "</u>".into(),
            Token::Code => "</code>".into(),
            Token::Bold if tags => "</strong>".into(),
            Token::Italic if tags => "</em>".into(),
            Token::Strike if tags => "</del>".into(),
            _ => self.open(emphasis),
        }
    }
}

/// A link's URL as it goes between the parentheses.
fn destination(url: &str) -> String {
    let plain = !url.is_empty()
        && url
            .chars()
            .all(|c| !c.is_whitespace() && !c.is_control() && !"()<>\\&|".contains(c));
    if plain {
        return url.to_string();
    }
    let mut out = String::from("<");
    for c in url.chars() {
        match c {
            '\n' | '\r' => {}
            // A `|` ends a table's cell.
            '<' | '>' | '\\' | '&' | '|' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out.push('>');
    out
}

fn write_line(runs: Vec<Run>, emphasis: Emphasis, cell: bool) -> String {
    let runs = if emphasis == Emphasis::Markers {
        spaces_outside_emphasis(runs)
    } else {
        runs
    };
    let whole: Vec<char> = runs.iter().flat_map(|(t, _)| t.chars()).collect();
    let first = whole.iter().position(|c| !c.is_whitespace());
    let last = whole.iter().rposition(|c| !c.is_whitespace());
    let mut out = String::new();
    let mut open: Vec<Token> = Vec::new();
    let mut at = 0;
    for (text, marks) in &runs {
        let mut wanted = Token::of(marks);
        let tagged_code = marks.code && cell && text.contains('|');
        if tagged_code {
            wanted.push(Token::Code);
        }
        let kept = open.iter().zip(&wanted).take_while(|(a, b)| a == b).count();
        for token in open.drain(kept..).rev() {
            out.push_str(&token.close(emphasis));
        }
        for token in &wanted[kept..] {
            out.push_str(&token.open(emphasis));
        }
        open = wanted;
        if marks.code && !tagged_code {
            out.push_str(&code_span(text));
            at += text.chars().count();
            continue;
        }
        for c in text.chars() {
            // Markdown trims a line's outer spaces, but not character references.
            let outer = first.is_none_or(|f| at < f) || last.is_none_or(|l| at > l);
            match c {
                ' ' if outer => out.push_str("&#32;"),
                '\t' => out.push_str("&#9;"),
                _ => {
                    if escaped(&whole, at, first, last) {
                        out.push('\\');
                    }
                    out.push(c);
                }
            }
            at += 1;
        }
    }
    for token in open.iter().rev() {
        out.push_str(&token.close(emphasis));
    }
    out
}

/// Whether the character at `at` in the line would mean something as Markdown.
fn escaped(line: &[char], at: usize, first: Option<usize>, last: Option<usize>) -> bool {
    let word = |i: Option<usize>| {
        i.and_then(|i| line.get(i))
            .is_some_and(|c| c.is_alphanumeric())
    };
    let (before, after) = (at.checked_sub(1), Some(at + 1));
    match line[at] {
        '\\' | '`' | '*' | '[' | ']' | '<' | '~' => true,
        // Inside a word an underscore is text.
        '_' => !(word(before) && word(after)),
        '&' => word(after) || line.get(at + 1) == Some(&'#'),
        // A heading, a quote, a list item or a rule at the start; a heading's own close at the end.
        '>' | '+' | '-' | '=' => first == Some(at),
        '#' => first == Some(at) || last == Some(at),
        // A numbered item.
        '.' | ')' => {
            first.is_some_and(|f| f < at && line[f..at].iter().all(|c| c.is_ascii_digit()))
        }
        '|' => true,
        _ => false,
    }
}

/// `text` as a code span, fenced by more backticks than it has in a row.
fn code_span(text: &str) -> String {
    let longest = text
        .split(|c| c != '`')
        .map(str::len)
        .max()
        .unwrap_or_default();
    let fence = "`".repeat(longest + 1);
    // One space is taken off each side where both have one.
    let padded = text.starts_with('`')
        || text.ends_with('`')
        || (text.starts_with(' ') && text.ends_with(' ') && !text.trim().is_empty());
    let pad = if padded { " " } else { "" };
    format!("{fence}{pad}{text}{pad}{fence}")
}

/// Move the spaces at a run's ends out of emphasis it does not share with its neighbour, as a
/// marker beside a space does not open or close.
pub(super) fn spaces_outside_emphasis(runs: Vec<Run>) -> Vec<Run> {
    let mut pieces: Vec<(String, Marks, bool)> = Vec::new();
    for (text, marks) in runs {
        if marks.code {
            pieces.push((text, marks, false));
            continue;
        }
        let body = text.trim();
        if body.is_empty() {
            pieces.push((text, marks, true));
            continue;
        }
        let lead = &text[..text.len() - text.trim_start().len()];
        let trail = &text[text.trim_end().len()..];
        for (piece, space) in [(lead, true), (body, false), (trail, true)] {
            if !piece.is_empty() {
                pieces.push((piece.to_string(), marks.clone(), space));
            }
        }
    }
    let solid: Vec<Marks> = pieces.iter().map(|(_, m, _)| m.clone()).collect();
    for i in 0..pieces.len() {
        if !pieces[i].2 {
            continue;
        }
        let before = (0..i).rev().find(|&j| !pieces[j].2).map(|j| &solid[j]);
        let after = (i + 1..pieces.len())
            .find(|&j| !pieces[j].2)
            .map(|j| &solid[j]);
        let shared = |f: fn(&Marks) -> bool| before.is_some_and(f) && after.is_some_and(f);
        let m = &mut pieces[i].1;
        m.bold &= shared(|m| m.bold);
        m.italic &= shared(|m| m.italic);
        m.strike &= shared(|m| m.strike);
    }
    let mut out: Vec<Run> = Vec::new();
    for (text, marks, _) in pieces {
        match out.last_mut() {
            Some((last, m)) if *m == marks => last.push_str(&text),
            _ => out.push((text, marks)),
        }
    }
    out
}
