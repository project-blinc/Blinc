//! HTML-like markup parser for rich text
//!
//! Parses a subset of HTML tags for inline text formatting.

use blinc_core::Color;

use crate::styled_text::TextSpan;

/// Parse HTML-like markup into plain text and spans
///
/// Returns (plain_text, spans) where plain_text has all tags stripped
/// and spans contain the formatting information.
pub fn parse(markup: &str) -> (String, Vec<TextSpan>) {
    let mut parser = Parser::new(markup);
    parser.parse();
    (parser.output, parser.spans)
}

/// Active style state (for tracking nested tags)
#[derive(Clone, Default)]
struct StyleState {
    bold: bool,
    italic: bool,
    underline: bool,
    strikethrough: bool,
    color: Option<Color>,
    link_url: Option<String>,
}

/// Tag with optional attributes
#[derive(Debug)]
struct Tag {
    name: String,
    is_closing: bool,
    attrs: Vec<(String, String)>,
}

struct Parser<'a> {
    input: &'a str,
    pos: usize,
    output: String,
    spans: Vec<TextSpan>,
    style_stack: Vec<(usize, StyleState)>, // (start_pos, style)
    current_style: StyleState,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            pos: 0,
            output: String::with_capacity(input.len()),
            spans: Vec::new(),
            style_stack: Vec::new(),
            current_style: StyleState::default(),
        }
    }

    fn parse(&mut self) {
        while self.pos < self.input.len() {
            if self.peek() == Some('<') {
                if let Some(tag) = self.try_parse_tag() {
                    self.handle_tag(tag);
                    continue;
                }
            }

            if self.peek() == Some('&') {
                if let Some(decoded) = self.try_parse_entity() {
                    self.output.push_str(&decoded);
                    continue;
                }
            }

            // Regular character
            if let Some(ch) = self.next_char() {
                self.output.push(ch);
            }
        }

        // Close any remaining open tags
        self.close_remaining_tags();
    }

    fn peek(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    fn next_char(&mut self) -> Option<char> {
        let ch = self.input[self.pos..].chars().next()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }

    fn try_parse_tag(&mut self) -> Option<Tag> {
        let start = self.pos;

        // Must start with <
        if self.next_char()? != '<' {
            self.pos = start;
            return None;
        }

        // Check for closing tag
        let is_closing = if self.peek() == Some('/') {
            self.next_char();
            true
        } else {
            false
        };

        // Parse tag name
        let name_start = self.pos;
        while let Some(ch) = self.peek() {
            if ch.is_alphanumeric() || ch == '-' || ch == '_' {
                self.next_char();
            } else {
                break;
            }
        }

        let name = self.input[name_start..self.pos].to_lowercase();
        if name.is_empty() {
            self.pos = start;
            return None;
        }

        // Skip whitespace
        self.skip_whitespace();

        // Parse attributes (for opening tags)
        let mut attrs = Vec::new();
        if !is_closing {
            while let Some((key, value)) = self.try_parse_attribute() {
                attrs.push((key, value));
                self.skip_whitespace();
            }
        }

        // Skip to closing >
        self.skip_whitespace();

        // Handle self-closing />
        if self.peek() == Some('/') {
            self.next_char();
        }

        if self.peek() != Some('>') {
            self.pos = start;
            return None;
        }
        self.next_char(); // consume >

        Some(Tag {
            name,
            is_closing,
            attrs,
        })
    }

    fn try_parse_attribute(&mut self) -> Option<(String, String)> {
        let start = self.pos;

        // Parse attribute name
        let name_start = self.pos;
        while let Some(ch) = self.peek() {
            if ch.is_alphanumeric() || ch == '-' || ch == '_' {
                self.next_char();
            } else {
                break;
            }
        }

        let name = self.input[name_start..self.pos].to_lowercase();
        if name.is_empty() {
            self.pos = start;
            return None;
        }

        self.skip_whitespace();

        // Expect =
        if self.peek() != Some('=') {
            self.pos = start;
            return None;
        }
        self.next_char();

        self.skip_whitespace();

        // Parse value (quoted)
        let quote = self.peek();
        if quote != Some('"') && quote != Some('\'') {
            self.pos = start;
            return None;
        }
        let quote = quote.unwrap();
        self.next_char();

        let value_start = self.pos;
        while let Some(ch) = self.peek() {
            if ch == quote {
                break;
            }
            self.next_char();
        }

        let value = self.input[value_start..self.pos].to_string();

        // Consume closing quote
        if self.peek() == Some(quote) {
            self.next_char();
        }

        Some((name, value))
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() {
                self.next_char();
            } else {
                break;
            }
        }
    }

    fn try_parse_entity(&mut self) -> Option<String> {
        let start = self.pos;

        if self.next_char()? != '&' {
            self.pos = start;
            return None;
        }

        // Check for numeric entity
        if self.peek() == Some('#') {
            self.next_char();
            let is_hex = if self.peek() == Some('x') || self.peek() == Some('X') {
                self.next_char();
                true
            } else {
                false
            };

            let num_start = self.pos;
            while let Some(ch) = self.peek() {
                if (is_hex && ch.is_ascii_hexdigit()) || (!is_hex && ch.is_ascii_digit()) {
                    self.next_char();
                } else {
                    break;
                }
            }

            let num_str = &self.input[num_start..self.pos];
            if num_str.is_empty() {
                self.pos = start;
                return None;
            }

            // Consume semicolon if present
            if self.peek() == Some(';') {
                self.next_char();
            }

            let code = if is_hex {
                u32::from_str_radix(num_str, 16).ok()?
            } else {
                num_str.parse::<u32>().ok()?
            };

            return char::from_u32(code).map(|c| c.to_string());
        }

        // Named entity
        let name_start = self.pos;
        while let Some(ch) = self.peek() {
            if ch.is_alphanumeric() {
                self.next_char();
            } else {
                break;
            }
        }

        let name = &self.input[name_start..self.pos];

        // Consume semicolon if present
        if self.peek() == Some(';') {
            self.next_char();
        }

        // Decode common entities
        let decoded = match name {
            "lt" => "<",
            "gt" => ">",
            "amp" => "&",
            "quot" => "\"",
            "apos" => "'",
            "nbsp" => "\u{00A0}",
            "copy" => "©",
            "reg" => "®",
            "trade" => "™",
            "mdash" => "—",
            "ndash" => "–",
            "hellip" => "…",
            "lsquo" => "\u{2018}", // '
            "rsquo" => "\u{2019}", // '
            "ldquo" => "\u{201C}", // "
            "rdquo" => "\u{201D}", // "
            "bull" => "•",
            "middot" => "·",
            _ => {
                self.pos = start;
                return None;
            }
        };

        Some(decoded.to_string())
    }

    fn handle_tag(&mut self, tag: Tag) {
        if tag.is_closing {
            self.handle_closing_tag(&tag.name);
        } else {
            self.handle_opening_tag(&tag.name, &tag.attrs);
        }
    }

    fn handle_opening_tag(&mut self, name: &str, attrs: &[(String, String)]) {
        // Save current position and style
        let start_pos = self.output.len();

        // Push current style to stack
        self.style_stack
            .push((start_pos, self.current_style.clone()));

        // Modify current style based on tag
        match name {
            "b" | "strong" => self.current_style.bold = true,
            "i" | "em" => self.current_style.italic = true,
            "u" => self.current_style.underline = true,
            "s" | "strike" | "del" => self.current_style.strikethrough = true,
            "a" => {
                self.current_style.underline = true;
                if let Some((_, url)) = attrs.iter().find(|(k, _)| k == "href") {
                    self.current_style.link_url = Some(url.clone());
                }
            }
            "span" => {
                if let Some((_, color_str)) = attrs.iter().find(|(k, _)| k == "color") {
                    if let Some(color) = parse_color(color_str) {
                        self.current_style.color = Some(color);
                    }
                }
            }
            _ => {
                // Unknown tag - pop from stack since we won't use it
                self.style_stack.pop();
            }
        }
    }

    fn handle_closing_tag(&mut self, name: &str) {
        // Find matching opening tag on stack
        let tag_matches = |tag_name: &str| match name {
            "b" | "strong" => tag_name == "b" || tag_name == "strong",
            "i" | "em" => tag_name == "i" || tag_name == "em",
            "u" => tag_name == "u",
            "s" | "strike" | "del" => tag_name == "s" || tag_name == "strike" || tag_name == "del",
            "a" => tag_name == "a",
            "span" => tag_name == "span",
            _ => false,
        };

        // Check if this is a known tag
        if !matches!(
            name,
            "b" | "strong" | "i" | "em" | "u" | "s" | "strike" | "del" | "a" | "span"
        ) {
            return;
        }

        // Pop from stack and create span
        if let Some((start_pos, prev_style)) = self.style_stack.pop() {
            let end_pos = self.output.len();

            if end_pos > start_pos {
                // Create span with current style
                // Use TRANSPARENT as sentinel for "no explicit color" - rendering will substitute default_color
                let span = TextSpan {
                    start: start_pos,
                    end: end_pos,
                    color: self.current_style.color.unwrap_or(Color::TRANSPARENT),
                    bold: self.current_style.bold,
                    italic: self.current_style.italic,
                    underline: self.current_style.underline,
                    strikethrough: self.current_style.strikethrough,
                    link_url: self.current_style.link_url.clone(),
                    token_type: None,
                };
                self.spans.push(span);
            }

            // Restore previous style
            self.current_style = prev_style;
        }
    }

    fn close_remaining_tags(&mut self) {
        // Close any remaining open tags
        while let Some((start_pos, prev_style)) = self.style_stack.pop() {
            let end_pos = self.output.len();

            if end_pos > start_pos && self.has_any_style() {
                let span = TextSpan {
                    start: start_pos,
                    end: end_pos,
                    color: self.current_style.color.unwrap_or(Color::TRANSPARENT),
                    bold: self.current_style.bold,
                    italic: self.current_style.italic,
                    underline: self.current_style.underline,
                    strikethrough: self.current_style.strikethrough,
                    link_url: self.current_style.link_url.clone(),
                    token_type: None,
                };
                self.spans.push(span);
            }

            self.current_style = prev_style;
        }
    }

    fn has_any_style(&self) -> bool {
        self.current_style.bold
            || self.current_style.italic
            || self.current_style.underline
            || self.current_style.strikethrough
            || self.current_style.color.is_some()
            || self.current_style.link_url.is_some()
    }
}

/// Parse a color string (hex, named, or rgba)
fn parse_color(s: &str) -> Option<Color> {
    let s = s.trim();

    // Hex color
    if s.starts_with('#') {
        return parse_hex_color(&s[1..]);
    }

    // RGBA
    if s.starts_with("rgba(") && s.ends_with(')') {
        return parse_rgba(&s[5..s.len() - 1]);
    }

    // RGB
    if s.starts_with("rgb(") && s.ends_with(')') {
        return parse_rgb(&s[4..s.len() - 1]);
    }

    // Named colors
    parse_named_color(s)
}

fn parse_hex_color(hex: &str) -> Option<Color> {
    let hex = hex.trim_start_matches('#');

    match hex.len() {
        3 => {
            // Short form: #RGB -> #RRGGBB
            let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
            Some(Color::rgb(
                r as f32 / 255.0,
                g as f32 / 255.0,
                b as f32 / 255.0,
            ))
        }
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some(Color::rgb(
                r as f32 / 255.0,
                g as f32 / 255.0,
                b as f32 / 255.0,
            ))
        }
        8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
            Some(Color::rgba(
                r as f32 / 255.0,
                g as f32 / 255.0,
                b as f32 / 255.0,
                a as f32 / 255.0,
            ))
        }
        _ => None,
    }
}

fn parse_rgba(s: &str) -> Option<Color> {
    let parts: Vec<&str> = s.split(',').map(|p| p.trim()).collect();
    if parts.len() != 4 {
        return None;
    }

    let r: f32 = parts[0].parse().ok()?;
    let g: f32 = parts[1].parse().ok()?;
    let b: f32 = parts[2].parse().ok()?;
    let a: f32 = parts[3].parse().ok()?;

    // Normalize if values are 0-255 range
    if r > 1.0 || g > 1.0 || b > 1.0 {
        Some(Color::rgba(r / 255.0, g / 255.0, b / 255.0, a))
    } else {
        Some(Color::rgba(r, g, b, a))
    }
}

fn parse_rgb(s: &str) -> Option<Color> {
    let parts: Vec<&str> = s.split(',').map(|p| p.trim()).collect();
    if parts.len() != 3 {
        return None;
    }

    let r: f32 = parts[0].parse().ok()?;
    let g: f32 = parts[1].parse().ok()?;
    let b: f32 = parts[2].parse().ok()?;

    // Normalize if values are 0-255 range
    if r > 1.0 || g > 1.0 || b > 1.0 {
        Some(Color::rgba(r / 255.0, g / 255.0, b / 255.0, 1.0))
    } else {
        Some(Color::rgba(r, g, b, 1.0))
    }
}

fn parse_named_color(name: &str) -> Option<Color> {
    // CSS named colors (subset) - Color::rgb takes f32 in 0.0-1.0 range
    match name.to_lowercase().as_str() {
        "black" => Some(Color::BLACK),
        "white" => Some(Color::WHITE),
        "red" => Some(Color::RED),
        "green" => Some(Color::rgb(0.0, 0.5, 0.0)), // CSS green is #008000
        "blue" => Some(Color::BLUE),
        "yellow" => Some(Color::YELLOW),
        "cyan" | "aqua" => Some(Color::CYAN),
        "magenta" | "fuchsia" => Some(Color::MAGENTA),
        "gray" | "grey" => Some(Color::GRAY),
        "silver" => Some(Color::rgb(0.75, 0.75, 0.75)),
        "maroon" => Some(Color::rgb(0.5, 0.0, 0.0)),
        "olive" => Some(Color::rgb(0.5, 0.5, 0.0)),
        "navy" => Some(Color::rgb(0.0, 0.0, 0.5)),
        "purple" => Some(Color::PURPLE),
        "teal" => Some(Color::rgb(0.0, 0.5, 0.5)),
        "orange" => Some(Color::ORANGE),
        "pink" => Some(Color::rgb(1.0, 0.75, 0.8)),
        "brown" => Some(Color::rgb(0.65, 0.16, 0.16)),
        "lime" => Some(Color::rgb(0.0, 1.0, 0.0)),
        "coral" => Some(Color::rgb(1.0, 0.5, 0.31)),
        "gold" => Some(Color::rgb(1.0, 0.84, 0.0)),
        "indigo" => Some(Color::rgb(0.29, 0.0, 0.51)),
        "violet" => Some(Color::rgb(0.93, 0.51, 0.93)),
        "crimson" => Some(Color::rgb(0.86, 0.08, 0.24)),
        "salmon" => Some(Color::rgb(0.98, 0.5, 0.45)),
        "tomato" => Some(Color::rgb(1.0, 0.39, 0.28)),
        "skyblue" => Some(Color::rgb(0.53, 0.81, 0.92)),
        "steelblue" => Some(Color::rgb(0.27, 0.51, 0.71)),
        "transparent" => Some(Color::TRANSPARENT),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_text() {
        let (text, spans) = parse("Hello World");
        assert_eq!(text, "Hello World");
        assert!(spans.is_empty());
    }

    #[test]
    fn test_bold_tag() {
        let (text, spans) = parse("Hello <b>World</b>!");
        assert_eq!(text, "Hello World!");
        assert_eq!(spans.len(), 1);
        assert!(spans[0].bold);
        assert_eq!(spans[0].start, 6);
        assert_eq!(spans[0].end, 11);
    }

    #[test]
    fn test_strong_tag() {
        let (text, spans) = parse("<strong>Bold</strong>");
        assert_eq!(text, "Bold");
        assert_eq!(spans.len(), 1);
        assert!(spans[0].bold);
    }

    #[test]
    fn test_italic_tag() {
        let (text, spans) = parse("Hello <i>World</i>!");
        assert_eq!(text, "Hello World!");
        assert_eq!(spans.len(), 1);
        assert!(spans[0].italic);
    }

    #[test]
    fn test_em_tag() {
        let (text, spans) = parse("<em>Emphasis</em>");
        assert_eq!(text, "Emphasis");
        assert!(spans[0].italic);
    }

    #[test]
    fn test_nested_tags() {
        let (text, spans) = parse("<b>bold <i>and italic</i></b>");
        assert_eq!(text, "bold and italic");
        // Should have span for inner italic, then outer bold
        assert!(spans.len() >= 1);
    }

    #[test]
    fn test_link_tag() {
        let (text, spans) = parse(r#"Click <a href="https://example.com">here</a>"#);
        assert_eq!(text, "Click here");
        assert_eq!(spans.len(), 1);
        assert!(spans[0].underline);
        assert_eq!(spans[0].link_url, Some("https://example.com".to_string()));
    }

    #[test]
    fn test_span_color() {
        let (text, spans) = parse("<span color=\"#FF0000\">Red</span>");
        assert_eq!(text, "Red");
        assert_eq!(spans.len(), 1);
        // Color should be red
        let color = spans[0].color;
        assert!(color.r > 0.9);
        assert!(color.g < 0.1);
        assert!(color.b < 0.1);
    }

    #[test]
    fn test_named_color() {
        let (text, spans) = parse(r#"<span color="blue">Blue</span>"#);
        assert_eq!(text, "Blue");
        assert_eq!(spans.len(), 1);
    }

    #[test]
    fn test_entity_decoding() {
        let (text, _) = parse("&lt;b&gt; &amp; &nbsp;");
        assert_eq!(text, "<b> & \u{00A0}");
    }

    #[test]
    fn test_numeric_entity() {
        let (text, _) = parse("&#65;&#66;&#67;");
        assert_eq!(text, "ABC");
    }

    #[test]
    fn test_hex_entity() {
        let (text, _) = parse("&#x41;&#x42;&#x43;");
        assert_eq!(text, "ABC");
    }

    #[test]
    fn test_unclosed_tag() {
        let (text, spans) = parse("Hello <b>World");
        assert_eq!(text, "Hello World");
        // Should still create span for unclosed tag
        assert_eq!(spans.len(), 1);
        assert!(spans[0].bold);
    }

    #[test]
    fn test_unknown_tag() {
        let (text, spans) = parse("Hello <foo>World</foo>");
        // Unknown tags are stripped but no span created
        assert_eq!(text, "Hello World");
        assert!(spans.is_empty());
    }

    #[test]
    fn test_hex_color_parsing() {
        assert!(parse_hex_color("FF0000").is_some());
        assert!(parse_hex_color("F00").is_some());
        assert!(parse_hex_color("FF0000FF").is_some());
    }

    #[test]
    fn test_rgba_parsing() {
        let color = parse_rgba("255, 0, 0, 1.0").unwrap();
        assert!(color.r > 0.9);
    }

    #[test]
    fn test_underline_tag() {
        let (text, spans) = parse("Hello <u>underlined</u> text");
        assert_eq!(text, "Hello underlined text");
        assert_eq!(spans.len(), 1);
        assert!(spans[0].underline);
    }

    #[test]
    fn test_strikethrough_tag() {
        let (text, spans) = parse("Hello <s>struck</s> text");
        assert_eq!(text, "Hello struck text");
        assert_eq!(spans.len(), 1);
        assert!(spans[0].strikethrough);
    }
}
