//! Syntax highlighting for code elements
//!
//! This module provides a trait-based syntax highlighting system that uses regex
//! patterns to match tokens. The code widgets give each token the class
//! `blinc-code-token-<kind>`, and the stylesheet colours it from the theme.
//!
//! # Example
//!
//! ```ignore
//! use blinc_layout::syntax::{SyntaxHighlighter, TokenRule, SyntaxConfig};
//! use blinc_core::Color;
//!
//! struct MyHighlighter {
//!     rules: Vec<TokenRule>,
//! }
//!
//! impl SyntaxHighlighter for MyHighlighter {
//!     fn token_rules(&self) -> &[TokenRule] {
//!         &self.rules
//!     }
//! }
//!
//! // Rules made with `TokenRule::themed` take the stylesheet's colours.
//! code("let x = 42;")
//!     .syntax(SyntaxConfig::new(MyHighlighter { rules: vec![...] }))
//! ```

use std::borrow::Cow;
use std::sync::{Arc, OnceLock};

use blinc_core::Color;
use regex::Regex;

use crate::styled_text::{StyledLine, StyledText, TextSpan};

/// Token type identifier for callbacks
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TokenType {
    /// A keyword (fn, let, if, etc.)
    Keyword,
    /// A string literal
    String,
    /// A comment
    Comment,
    /// A number literal
    Number,
    /// A type name
    Type,
    /// A function name
    Function,
    /// A variable name
    Variable,
    /// A macro
    Macro,
    /// An operator
    Operator,
    /// A lifetime (Rust-specific)
    Lifetime,
    /// Custom token type with a name
    Custom(String),
}

impl TokenType {
    /// Create a custom token type
    pub fn custom(name: impl Into<String>) -> Self {
        TokenType::Custom(name.into())
    }

    /// The class a code widget gives a token of this type: `blinc-code-token-` and the type's
    /// name. A custom name is lower-cased, with a dash for what is not a letter or a digit.
    pub fn css_class(&self) -> Cow<'static, str> {
        Cow::Borrowed(match self {
            TokenType::Keyword => "blinc-code-token-keyword",
            TokenType::String => "blinc-code-token-string",
            TokenType::Comment => "blinc-code-token-comment",
            TokenType::Number => "blinc-code-token-number",
            TokenType::Type => "blinc-code-token-type",
            TokenType::Function => "blinc-code-token-function",
            TokenType::Variable => "blinc-code-token-variable",
            TokenType::Macro => "blinc-code-token-macro",
            TokenType::Operator => "blinc-code-token-operator",
            TokenType::Lifetime => "blinc-code-token-lifetime",
            TokenType::Custom(name) => {
                let name: String = name
                    .chars()
                    .map(|c| {
                        if c.is_ascii_alphanumeric() {
                            c.to_ascii_lowercase()
                        } else {
                            '-'
                        }
                    })
                    .collect();
                return Cow::Owned(format!("blinc-code-token-{name}"));
            }
        })
    }
}

/// Information about a token hit (for intellisense callbacks)
#[derive(Clone, Debug)]
pub struct TokenHit {
    /// The matched token text
    pub text: String,
    /// The token type
    pub token_type: TokenType,
    /// Line number (0-based)
    pub line: usize,
    /// Start column (0-based, in characters)
    pub start_column: usize,
    /// End column (0-based, exclusive)
    pub end_column: usize,
}

impl TokenHit {
    /// Create a new token hit
    pub fn new(
        text: impl Into<String>,
        token_type: TokenType,
        line: usize,
        start_column: usize,
        end_column: usize,
    ) -> Self {
        Self {
            text: text.into(),
            token_type,
            line,
            start_column,
            end_column,
        }
    }
}

/// A single token style rule with regex pattern
#[derive(Clone)]
pub struct TokenRule {
    /// Compiled regex pattern to match
    pattern: Regex,
    /// Color to apply to matched tokens, over the stylesheet's. Transparent leaves them to it.
    pub color: Color,
    /// Whether text should be bold
    pub bold: bool,
    /// Token type for identification in callbacks
    pub token_type: TokenType,
}

impl TokenRule {
    /// Create a new token rule with a regex pattern
    ///
    /// # Panics
    /// Panics if the regex pattern is invalid
    pub fn new(pattern: &str, color: Color) -> Self {
        Self {
            pattern: Regex::new(pattern).expect("Invalid regex pattern"),
            color,
            bold: false,
            token_type: TokenType::Custom("unknown".to_string()),
        }
    }

    /// A rule for tokens of `token_type` that the stylesheet colours, by their class.
    ///
    /// # Panics
    /// Panics if the regex pattern is invalid
    pub fn themed(pattern: &str, token_type: TokenType) -> Self {
        Self::new(pattern, Color::TRANSPARENT).token_type(token_type)
    }

    /// Try to create a token rule, returning None if pattern is invalid
    pub fn try_new(pattern: &str, color: Color) -> Option<Self> {
        Regex::new(pattern).ok().map(|pattern| Self {
            pattern,
            color,
            bold: false,
            token_type: TokenType::Custom("unknown".to_string()),
        })
    }

    /// Set the token type for this rule
    pub fn token_type(mut self, token_type: TokenType) -> Self {
        self.token_type = token_type;
        self
    }

    /// Make this token bold
    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    /// Get the regex pattern
    pub fn pattern(&self) -> &Regex {
        &self.pattern
    }
}

impl std::fmt::Debug for TokenRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenRule")
            .field("pattern", &self.pattern.as_str())
            .field("color", &self.color)
            .field("bold", &self.bold)
            .finish()
    }
}

/// Trait for syntax highlighting implementations
///
/// Implement this trait to create custom syntax highlighters for different
/// languages or formats. The highlighter provides regex-based token rules
/// that are applied in order (first match wins for overlapping regions).
pub trait SyntaxHighlighter: Send + Sync {
    /// Get the list of token rules in priority order (first match wins)
    fn token_rules(&self) -> &[TokenRule];

    /// Colour of text no rule matches, over the stylesheet's, when the highlighter has one
    fn default_color(&self) -> Option<Color> {
        None
    }

    /// Background of the code block, over the stylesheet's, when the highlighter has one
    fn background_color(&self) -> Option<Color> {
        None
    }

    /// Line number colour, over the stylesheet's, when the highlighter has one
    fn line_number_color(&self) -> Option<Color> {
        None
    }

    /// Apply highlighting to text, returning styled text
    fn highlight(&self, text: &str) -> StyledText {
        // A transparent span is one the stylesheet colours.
        let default_color = self.default_color().unwrap_or(Color::TRANSPARENT);
        let rules = self.token_rules();

        // Handle empty string specially - .lines() returns empty iterator for ""
        if text.is_empty() {
            return StyledText::from_lines(vec![highlight_line("", rules, default_color)]);
        }

        let lines = text
            .lines()
            .map(|line| highlight_line(line, rules, default_color))
            .collect();

        StyledText::from_lines(lines)
    }
}

/// Configuration for syntax highlighting passed to code elements
pub struct SyntaxConfig {
    highlighter: Box<dyn SyntaxHighlighter>,
}

impl SyntaxConfig {
    /// Create a new syntax config with the given highlighter
    pub fn new(highlighter: impl SyntaxHighlighter + 'static) -> Self {
        Self {
            highlighter: Box::new(highlighter),
        }
    }

    /// Get a reference to the highlighter
    pub fn highlighter(&self) -> &dyn SyntaxHighlighter {
        self.highlighter.as_ref()
    }

    /// Convert the config into an Arc-wrapped highlighter
    pub fn into_arc(self) -> std::sync::Arc<dyn SyntaxHighlighter> {
        std::sync::Arc::from(self.highlighter)
    }
}

/// A match found by a token rule
#[derive(Debug, Clone)]
struct TokenMatch {
    start: usize,
    end: usize,
    color: Color,
    bold: bool,
    token_type: TokenType,
}

/// Highlight a single line of text using the given rules
fn highlight_line(line: &str, rules: &[TokenRule], default_color: Color) -> StyledLine {
    if line.is_empty() {
        return StyledLine::new(line, vec![]);
    }

    // Find all matches from all rules
    let mut matches: Vec<TokenMatch> = Vec::new();

    for rule in rules {
        for m in rule.pattern.find_iter(line) {
            matches.push(TokenMatch {
                start: m.start(),
                end: m.end(),
                color: rule.color,
                bold: rule.bold,
                token_type: rule.token_type.clone(),
            });
        }
    }

    // Sort by start position (earlier first), then by rule priority (earlier in rules list)
    // Since we iterate rules in order and push matches, matches from earlier rules come first
    matches.sort_by_key(|m| m.start);

    // Build non-overlapping spans (first match wins for overlapping regions)
    let mut spans: Vec<TextSpan> = Vec::new();
    let mut current_pos = 0;

    for m in matches {
        // Skip if this match overlaps with already processed region
        if m.start < current_pos {
            continue;
        }

        // Add default-colored span for gap before this match
        if m.start > current_pos {
            spans.push(TextSpan::new(current_pos, m.start, default_color, false));
        }

        // Add the matched span with token type
        let mut span = TextSpan::new(m.start, m.end, m.color, m.bold);
        span.token_type = Some(m.token_type.clone());
        spans.push(span);
        current_pos = m.end;
    }

    // Add remaining text in default color
    if current_pos < line.len() {
        spans.push(TextSpan::new(current_pos, line.len(), default_color, false));
    }

    StyledLine::new(line, spans)
}

// ============================================================================
// Built-in Highlighters
// ============================================================================

/// Generic-language highlighter. Covers the constructs that read
/// the same way across most C-family / scripting languages —
/// strings (double + single quoted), numeric literals, and
/// line comments in the three common shells (`//`, `#`, `--`). No
/// keyword set: anything not matched is plain text.
/// Designed for the "unknown language but I still want some
/// visual structure" case used by the inline script editor's
/// fallback path.
pub struct PlainHighlighter {
    rules: Arc<[TokenRule]>,
    text_color: Option<Color>,
    bg_color: Option<Color>,
}

impl PlainHighlighter {
    pub fn new() -> Self {
        Self {
            rules: plain_rules(),
            text_color: None,
            bg_color: None,
        }
    }

    /// Override the default text colour — applied to spans that no
    /// rule matched.
    pub fn text_color(mut self, color: Color) -> Self {
        self.text_color = Some(color);
        self
    }

    /// Override the editor background colour.
    pub fn background(mut self, color: Color) -> Self {
        self.bg_color = Some(color);
        self
    }
}

fn plain_rules() -> Arc<[TokenRule]> {
    static CACHE: OnceLock<Arc<[TokenRule]>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            use TokenType::{Comment, Number, String};
            let rules: Vec<TokenRule> = vec![
                // Line comments — order matters: catch before
                // numbers / strings so a `// foo bar` segment
                // doesn't get split by an inner match.
                TokenRule::themed(r"//.*$", Comment),
                TokenRule::themed(r"--.*$", Comment),
                TokenRule::themed(r"#.*$", Comment),
                // Strings — double + single quoted with `\` escape.
                TokenRule::themed(r#""(?:[^"\\]|\\.)*""#, String),
                TokenRule::themed(r#"'(?:[^'\\]|\\.)*'"#, String),
                // Numbers — integer / float / scientific / hex.
                TokenRule::themed(r"\b\d+(\.\d+)?([eE][+-]?\d+)?\b", Number),
                TokenRule::themed(r"\b0[xX][0-9a-fA-F]+\b", Number),
            ];
            Arc::from(rules)
        })
        .clone()
}

impl Default for PlainHighlighter {
    fn default() -> Self {
        Self::new()
    }
}

impl SyntaxHighlighter for PlainHighlighter {
    fn token_rules(&self) -> &[TokenRule] {
        &self.rules
    }

    fn default_color(&self) -> Option<Color> {
        self.text_color
    }

    fn background_color(&self) -> Option<Color> {
        self.bg_color
    }
}

/// A basic Rust syntax highlighter
///
/// `rules` is shared via `Arc<[TokenRule]>` across all instances of
/// `RustHighlighter::new()` — each fresh instance bumps a refcount
/// instead of re-compiling the ~12 regex patterns. For a typical
/// app with several `code()` blocks this is the difference between
/// paying the regex DFA tables once or N times.
pub struct RustHighlighter {
    rules: Arc<[TokenRule]>,
}

impl RustHighlighter {
    /// Create a new Rust highlighter
    pub fn new() -> Self {
        Self {
            rules: rust_rules(),
        }
    }
}

/// Shared, lazily-built Rust token rule set.
///
/// First call compiles every regex; subsequent calls hand out cheap
/// `Arc::clone`s. Safe across threads — `Regex` and `Arc<[T]>` are both
/// `Send + Sync`.
fn rust_rules() -> Arc<[TokenRule]> {
    static CACHE: OnceLock<Arc<[TokenRule]>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            use TokenType::{Comment, Function, Keyword, Lifetime, Macro, Number, String, Type};
            let rules: Vec<TokenRule> = vec![
                // Comments (must be early to capture before other rules)
                TokenRule::themed(r"//.*$", Comment),
                // Strings (double-quoted)
                TokenRule::themed(r#""[^"]*""#, String),
                // Characters (single quoted, simplified)
                TokenRule::themed(r"'[^']{1,2}'", String),
                // Lifetimes
                TokenRule::themed(r"'[a-zA-Z_][a-zA-Z0-9_]*", Lifetime),
                // Macros
                TokenRule::themed(r"\b[a-z_][a-zA-Z0-9_]*!", Macro),
                // Keywords
                TokenRule::themed(
                    r"\b(fn|let|mut|const|static|pub|use|mod|struct|enum|trait|impl|for|while|loop|if|else|match|return|break|continue|async|await|move|ref|self|Self|super|crate|where|type|dyn|unsafe|extern)\b",
                    Keyword,
                )
                .bold(),
                // Types (PascalCase)
                TokenRule::themed(r"\b[A-Z][a-zA-Z0-9_]*\b", Type),
                // Numbers
                TokenRule::themed(r"\b\d+(\.\d+)?([eE][+-]?\d+)?[fiu]?(8|16|32|64|128|size)?\b", Number),
                TokenRule::themed(r"\b0x[0-9a-fA-F_]+\b", Number),
                TokenRule::themed(r"\b0b[01_]+\b", Number),
                TokenRule::themed(r"\b0o[0-7_]+\b", Number),
                // Function calls
                TokenRule::themed(r"\b([a-z_][a-zA-Z0-9_]*)\s*\(", Function),
            ];
            Arc::from(rules)
        })
        .clone()
}

impl Default for RustHighlighter {
    fn default() -> Self {
        Self::new()
    }
}

impl SyntaxHighlighter for RustHighlighter {
    fn token_rules(&self) -> &[TokenRule] {
        &self.rules
    }
}

/// A basic JSON syntax highlighter
///
/// Like [`RustHighlighter`], the rule set is interned — every
/// `JsonHighlighter::new()` shares the same compiled regex tables.
pub struct JsonHighlighter {
    rules: Arc<[TokenRule]>,
}

impl JsonHighlighter {
    /// Create a new JSON highlighter
    pub fn new() -> Self {
        Self {
            rules: json_rules(),
        }
    }
}

fn json_rules() -> Arc<[TokenRule]> {
    static CACHE: OnceLock<Arc<[TokenRule]>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            let rules: Vec<TokenRule> = vec![
                // Keys (strings followed by colon)
                TokenRule::themed(r#""[^"]*"\s*:"#, TokenType::custom("property")),
                // Strings
                TokenRule::themed(r#""(?:[^"\\]|\\.)*""#, TokenType::String),
                // Numbers
                TokenRule::themed(r"-?\b\d+(\.\d+)?([eE][+-]?\d+)?\b", TokenType::Number),
                // Keywords
                TokenRule::themed(r"\b(true|false|null)\b", TokenType::Keyword),
            ];
            Arc::from(rules)
        })
        .clone()
}

impl Default for JsonHighlighter {
    fn default() -> Self {
        Self::new()
    }
}

impl SyntaxHighlighter for JsonHighlighter {
    fn token_rules(&self) -> &[TokenRule] {
        &self.rules
    }
}

/// A basic Lua syntax highlighter
///
/// Like [`RustHighlighter`] and [`JsonHighlighter`], the rule set
/// is interned via `OnceLock` — every `LuaHighlighter::new()` shares
/// the same compiled regex tables. Covers Lua 5.x keywords + the
/// common stdlib functions (`tostring`, `tonumber`, `pairs`, etc.)
/// as the `Function` token class, plus single + double-quoted
/// strings, `--` line comments, and the literal keywords
/// `nil` / `true` / `false`.
pub struct LuaHighlighter {
    rules: Arc<[TokenRule]>,
}

impl LuaHighlighter {
    pub fn new() -> Self {
        Self { rules: lua_rules() }
    }
}

fn lua_rules() -> Arc<[TokenRule]> {
    static CACHE: OnceLock<Arc<[TokenRule]>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            use TokenType::{Comment, Function, Keyword, Number, String};
            let rules: Vec<TokenRule> = vec![
                // Comments — `--` to end-of-line. Lua's block comment
                // form `--[[ ... ]]` spans lines and isn't easily
                // captured with a per-line regex pass; skipped here.
                TokenRule::themed(r"--.*$", Comment),
                // Strings — single + double quoted with `\` escapes.
                TokenRule::themed(r#""(?:[^"\\]|\\.)*""#, String),
                TokenRule::themed(r#"'(?:[^'\\]|\\.)*'"#, String),
                // Literals first so they don't get caught by the
                // generic keyword regex below.
                TokenRule::themed(r"\b(nil|true|false)\b", Keyword),
                // Control + structure keywords.
                TokenRule::themed(
                    r"\b(and|break|do|else|elseif|end|for|function|goto|if|in|local|not|or|repeat|return|then|until|while)\b",
                    Keyword,
                )
                .bold(),
                // Numbers — integer, float, scientific, hex.
                TokenRule::themed(r"\b\d+(\.\d+)?([eE][+-]?\d+)?\b", Number),
                TokenRule::themed(r"\b0[xX][0-9a-fA-F]+\b", Number),
                // Function calls — identifier followed by `(`.
                TokenRule::themed(r"\b([a-zA-Z_][a-zA-Z_0-9]*)\s*\(", Function),
            ];
            Arc::from(rules)
        })
        .clone()
}

impl Default for LuaHighlighter {
    fn default() -> Self {
        Self::new()
    }
}

impl SyntaxHighlighter for LuaHighlighter {
    fn token_rules(&self) -> &[TokenRule] {
        &self.rules
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_highlighter() {
        let highlighter = PlainHighlighter::new();
        let styled = highlighter.highlight("Hello, World!");

        assert_eq!(styled.line_count(), 1);
        assert_eq!(styled.lines[0].spans.len(), 1);
        assert_eq!(styled.lines[0].spans[0].start, 0);
        assert_eq!(styled.lines[0].spans[0].end, 13);
    }

    #[test]
    fn test_rust_keywords() {
        let highlighter = RustHighlighter::new();
        let styled = highlighter.highlight("fn main() {}");

        assert_eq!(styled.line_count(), 1);
        // Should have spans for: "fn", " ", "main", "()", " ", "{}"
        assert!(!styled.lines[0].spans.is_empty());
    }

    #[test]
    fn test_multiline() {
        let highlighter = RustHighlighter::new();
        let code = "fn main() {\n    println!(\"Hello\");\n}";
        let styled = highlighter.highlight(code);

        assert_eq!(styled.line_count(), 3);
    }

    #[test]
    fn test_json_highlighter() {
        let highlighter = JsonHighlighter::new();
        let styled = highlighter.highlight(r#"{"key": "value", "num": 42}"#);

        assert_eq!(styled.line_count(), 1);
        assert!(!styled.lines[0].spans.is_empty());
    }

    #[test]
    fn test_empty_line() {
        let highlighter = PlainHighlighter::new();
        let styled = highlighter.highlight("");

        assert_eq!(styled.line_count(), 1);
        assert!(styled.lines[0].spans.is_empty());
    }
}
