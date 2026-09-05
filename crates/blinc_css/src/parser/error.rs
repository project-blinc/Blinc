//! Parse diagnostics.
//!
//! Every failure the parser can report is a [`ParseError`] carrying a
//! severity, the source line and column, the offending fragment, and nom's
//! context stack. Parsing never aborts on a bad rule: errors accumulate in
//! a [`CssParseResult`] while the rest of the sheet keeps parsing, so one
//! typo costs one rule rather than the whole stylesheet.

use nom::error::{VerboseError, VerboseErrorKind};
use tracing::debug;

use crate::parser::*;

/// Severity level for parse warnings/errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Parsing failed completely
    Error,
    /// Parsing succeeded but with issues (e.g., unknown properties)
    Warning,
    /// Informational message
    Info,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Error => write!(f, "error"),
            Severity::Warning => write!(f, "warning"),
            Severity::Info => write!(f, "info"),
        }
    }
}

/// Error type for CSS parsing with context information
#[derive(Debug, Clone)]
pub struct ParseError {
    /// Severity level
    pub severity: Severity,
    /// Human-readable error message with context
    pub message: String,
    /// Line number (1-indexed)
    pub line: usize,
    /// Column number (1-indexed)
    pub column: usize,
    /// The specific input fragment where parsing failed
    pub fragment: String,
    /// Context stack from nom's VerboseError
    pub contexts: Vec<String>,
    /// The property or selector name if applicable
    pub property: Option<String>,
    /// The attempted value if applicable
    pub value: Option<String>,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "CSS {}: line {}, column {}: {}",
            self.severity, self.line, self.column, self.message
        )?;
        if let Some(ref prop) = self.property {
            if let Some(ref val) = self.value {
                write!(f, " ({}:{})", prop, val)?;
            } else {
                write!(f, " ({})", prop)?;
            }
        }
        if !self.contexts.is_empty() {
            write!(f, "\n  Context: {}", self.contexts.join(" > "))?;
        }
        if !self.fragment.is_empty() && self.fragment.len() < 50 {
            write!(f, "\n  Near: \"{}\"", self.fragment)?;
        }
        Ok(())
    }
}

impl std::error::Error for ParseError {}

impl ParseError {
    /// Create a new error with the given severity and message
    pub fn new(severity: Severity, message: impl Into<String>, line: usize, column: usize) -> Self {
        Self {
            severity,
            message: message.into(),
            line,
            column,
            fragment: String::new(),
            contexts: Vec::new(),
            property: None,
            value: None,
        }
    }

    /// Create an error for an unknown property
    pub fn unknown_property(property: &str, line: usize, column: usize) -> Self {
        Self {
            severity: Severity::Warning,
            message: format!("Unknown property '{}' (ignored)", property),
            line,
            column,
            fragment: String::new(),
            contexts: vec!["property".to_string()],
            property: Some(property.to_string()),
            value: None,
        }
    }

    /// Create an error for an invalid property value
    pub fn invalid_value(property: &str, value: &str, line: usize, column: usize) -> Self {
        Self {
            severity: Severity::Warning,
            message: format!("Invalid value for '{}': '{}'", property, value),
            line,
            column,
            fragment: String::new(),
            contexts: vec!["property value".to_string()],
            property: Some(property.to_string()),
            value: Some(value.to_string()),
        }
    }

    /// Create a ParseError from a nom VerboseError
    pub(crate) fn from_verbose(input: &str, err: VerboseError<&str>) -> Self {
        let (line, column, fragment) = if let Some((frag, _)) = err.errors.first() {
            calculate_position(input, frag)
        } else {
            (1, 1, String::new())
        };

        let contexts: Vec<String> = err
            .errors
            .iter()
            .filter_map(|(_, kind)| match kind {
                VerboseErrorKind::Context(ctx) => Some((*ctx).to_string()),
                _ => None,
            })
            .collect();

        let message = format_verbose_error(&err);

        Self {
            severity: Severity::Error,
            message,
            line,
            column,
            fragment,
            contexts,
            property: None,
            value: None,
        }
    }

    /// Format as a human-readable warning for console output
    pub fn to_warning_string(&self) -> String {
        let mut s = String::new();
        s.push_str(&format!(
            "{}[{}:{}]: {}",
            self.severity, self.line, self.column, self.message
        ));
        if let Some(ref prop) = self.property {
            if let Some(ref val) = self.value {
                s.push_str(&format!("\n  Property: {} = {}", prop, val));
            } else {
                s.push_str(&format!("\n  Property: {}", prop));
            }
        }
        if !self.fragment.is_empty() && self.fragment.len() < 80 {
            s.push_str(&format!("\n  Near: \"{}\"", self.fragment));
        }
        s
    }

    /// Format with ANSI color codes for terminal output
    ///
    /// Colors:
    /// - Error: Red
    /// - Warning: Yellow
    /// - Info: Cyan
    /// - Property names: Blue
    /// - Values: Magenta
    /// - Line numbers: Dim
    pub fn to_colored_string(&self) -> String {
        // ANSI color codes
        const RESET: &str = "\x1b[0m";
        const RED: &str = "\x1b[31m";
        const YELLOW: &str = "\x1b[33m";
        const CYAN: &str = "\x1b[36m";
        const BLUE: &str = "\x1b[34m";
        const MAGENTA: &str = "\x1b[35m";
        const DIM: &str = "\x1b[2m";
        const BOLD: &str = "\x1b[1m";

        let (severity_color, icon) = match self.severity {
            Severity::Error => (RED, "✖"),
            Severity::Warning => (YELLOW, "⚠"),
            Severity::Info => (CYAN, "ℹ"),
        };

        let mut s = String::new();

        // Severity with icon and color
        s.push_str(&format!(
            "{}{}{} {}{}{}{RESET} ",
            BOLD, severity_color, icon, severity_color, self.severity, RESET
        ));

        // Location in dim
        s.push_str(&format!("{DIM}[{}:{}]{RESET} ", self.line, self.column));

        // Message
        s.push_str(&self.message);

        // Property and value with colors
        if let Some(ref prop) = self.property {
            s.push_str(&format!("\n  {BLUE}Property:{RESET} {}", prop));
            if let Some(ref val) = self.value {
                s.push_str(&format!(" = {MAGENTA}{}{RESET}", val));
            }
        }

        // Context in dim
        if !self.contexts.is_empty() {
            s.push_str(&format!(
                "\n  {DIM}Context: {}{RESET}",
                self.contexts.join(" > ")
            ));
        }

        // Near fragment
        if !self.fragment.is_empty() && self.fragment.len() < 80 {
            s.push_str(&format!("\n  {DIM}Near:{RESET} \"{}\"", self.fragment));
        }

        s
    }
}

/// Result of parsing CSS with error collection
#[derive(Debug, Clone)]
pub struct CssParseResult {
    /// The parsed stylesheet (may be partial if errors occurred)
    pub stylesheet: Stylesheet,
    /// All errors and warnings collected during parsing
    pub errors: Vec<ParseError>,
}

impl CssParseResult {
    /// Check if parsing had any errors (not just warnings)
    pub fn has_errors(&self) -> bool {
        self.errors.iter().any(|e| e.severity == Severity::Error)
    }

    /// Check if parsing had any warnings
    pub fn has_warnings(&self) -> bool {
        self.errors.iter().any(|e| e.severity == Severity::Warning)
    }

    /// Get only the errors (not warnings)
    pub fn errors_only(&self) -> impl Iterator<Item = &ParseError> {
        self.errors.iter().filter(|e| e.severity == Severity::Error)
    }

    /// Get only the warnings
    pub fn warnings_only(&self) -> impl Iterator<Item = &ParseError> {
        self.errors
            .iter()
            .filter(|e| e.severity == Severity::Warning)
    }

    /// Print all errors and warnings as human-readable text (plain, no colors)
    pub fn print_diagnostics(&self) {
        for err in &self.errors {
            match err.severity {
                Severity::Error => eprintln!("❌ {}", err.to_warning_string()),
                Severity::Warning => eprintln!("⚠️  {}", err.to_warning_string()),
                Severity::Info => eprintln!("ℹ️  {}", err.to_warning_string()),
            }
        }
    }

    /// Print all errors and warnings with ANSI color coding
    ///
    /// Uses terminal colors for better readability:
    /// - Errors: Red
    /// - Warnings: Yellow
    /// - Info: Cyan
    pub fn print_colored_diagnostics(&self) {
        for err in &self.errors {
            eprintln!("{}", err.to_colored_string());
        }
    }

    /// Print a summary line with counts (colored)
    pub fn print_summary(&self) {
        const RESET: &str = "\x1b[0m";
        const RED: &str = "\x1b[31m";
        const YELLOW: &str = "\x1b[33m";
        const GREEN: &str = "\x1b[32m";
        const BOLD: &str = "\x1b[1m";

        let error_count = self.errors_only().count();
        let warning_count = self.warnings_only().count();

        if error_count == 0 && warning_count == 0 {
            eprintln!("{BOLD}{GREEN}✓ CSS parsed successfully{RESET}");
        } else {
            let mut parts = Vec::new();
            if error_count > 0 {
                parts.push(format!("{RED}{} error(s){RESET}", error_count));
            }
            if warning_count > 0 {
                parts.push(format!("{YELLOW}{} warning(s){RESET}", warning_count));
            }
            eprintln!(
                "{BOLD}CSS parsing completed with {}{RESET}",
                parts.join(", ")
            );
        }
    }

    /// Log all errors and warnings via tracing
    pub fn log_diagnostics(&self) {
        for err in &self.errors {
            match err.severity {
                Severity::Error => debug!(
                    severity = "error",
                    line = err.line,
                    column = err.column,
                    message = %err.message,
                    property = ?err.property,
                    value = ?err.value,
                    "CSS parse error"
                ),
                Severity::Warning => debug!(
                    severity = "warning",
                    line = err.line,
                    column = err.column,
                    message = %err.message,
                    property = ?err.property,
                    value = ?err.value,
                    "CSS parse warning"
                ),
                Severity::Info => debug!(
                    severity = "info",
                    line = err.line,
                    column = err.column,
                    message = %err.message,
                    "CSS parse info"
                ),
            }
        }
    }
}

/// Format a VerboseError into a human-readable message
pub(crate) fn format_verbose_error(err: &VerboseError<&str>) -> String {
    let mut parts = Vec::new();

    for (input, kind) in &err.errors {
        match kind {
            VerboseErrorKind::Context(ctx) => {
                parts.push(format!("in {}", ctx));
            }
            VerboseErrorKind::Char(c) => {
                let preview: String = input.chars().take(20).collect();
                parts.push(format!("expected '{}' near \"{}\"", c, preview));
            }
            VerboseErrorKind::Nom(ek) => {
                parts.push(format!("{:?}", ek));
            }
        }
    }

    if parts.is_empty() {
        "unknown parse error".to_string()
    } else {
        parts.join(", ")
    }
}

/// Calculate line and column from the original input and the error fragment
pub(crate) fn calculate_position(original: &str, fragment: &str) -> (usize, usize, String) {
    // Find where the fragment starts in the original input
    let offset = original.len().saturating_sub(fragment.len());
    let consumed = &original[..offset];

    let line = consumed.matches('\n').count() + 1;
    let column = consumed
        .rfind('\n')
        .map(|pos| offset - pos)
        .unwrap_or(offset + 1);

    let preview: String = fragment.chars().take(30).collect();
    (line, column, preview)
}
