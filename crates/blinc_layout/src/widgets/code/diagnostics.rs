//! Diagnostics shown inline in the code editor: an underline under the span, a tint across
//! the line, a mark in the gutter, and the line's most severe message after its text. A
//! compiler or linter sets them with [`set_code_diagnostics`]; the editor keeps them until the
//! next call, as an editor keeps a language server's.
//!
//! Each piece is a class per severity (`blinc-code-underline-error` and so on), coloured by the
//! theme's `--error`, `--warning`, `--info` and `--text-tertiary`.

use super::{CodeConfig, CodeEditorData, SharedCodeEditorState};
use crate::div::{Div, div};
use crate::text::{Text, text};

/// How serious a diagnostic is. The most severe on a line shows its message.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DiagnosticSeverity {
    Hint,
    Info,
    Warning,
    Error,
}

impl DiagnosticSeverity {
    fn name(self) -> &'static str {
        match self {
            DiagnosticSeverity::Error => "error",
            DiagnosticSeverity::Warning => "warning",
            DiagnosticSeverity::Info => "info",
            DiagnosticSeverity::Hint => "hint",
        }
    }
}

/// One diagnostic: a span on a line, how serious it is, and what it says.
#[derive(Clone, Debug, PartialEq)]
pub struct CodeDiagnostic {
    /// The line, from 0.
    pub line: usize,
    /// The span's first character, from 0.
    pub start: usize,
    /// One past the span's last character; a span that ends where it starts marks one character.
    pub end: usize,
    pub severity: DiagnosticSeverity,
    pub message: String,
}

impl CodeDiagnostic {
    pub fn new(
        line: usize,
        start: usize,
        end: usize,
        severity: DiagnosticSeverity,
        message: impl Into<String>,
    ) -> Self {
        Self {
            line,
            start,
            end,
            severity,
            message: message.into(),
        }
    }
}

/// Show `diagnostics` in the editor, replacing those it showed, and redraw it.
pub fn set_code_diagnostics(state: &SharedCodeEditorState, diagnostics: Vec<CodeDiagnostic>) {
    if let Ok(mut d) = state.lock() {
        if d.diagnostics == diagnostics {
            return;
        }
        d.diagnostics = diagnostics;
    }
    super::search::refresh_editor(state);
}

/// The most severe diagnostic on `line`.
fn worst(diagnostics: &[CodeDiagnostic], line: usize) -> Option<&CodeDiagnostic> {
    diagnostics
        .iter()
        .filter(|d| d.line == line)
        .max_by_key(|d| d.severity)
}

/// The tints and underlines behind the visible lines' text.
pub(super) fn overlays(
    data: &mut CodeEditorData,
    visible_lines: &[usize],
    line_height: f32,
    pad: f32,
) -> Vec<Div> {
    let mut out = Vec::new();
    let diagnostics = data.diagnostics.clone();
    for (row, &line) in visible_lines.iter().enumerate() {
        let Some(worst) = worst(&diagnostics, line) else {
            continue;
        };
        let top = row as f32 * line_height + pad;
        out.push(
            div()
                .class(format!("blinc-code-line-{}", worst.severity.name()))
                .absolute()
                .left(0.0)
                .top(top)
                .w_full()
                .h(line_height),
        );
        let Some(text) = data.lines.get(line).cloned() else {
            continue;
        };
        let chars = text.chars().count();
        for d in diagnostics.iter().filter(|d| d.line == line) {
            let start = d.start.min(chars);
            let end = d.end.max(start + 1).min(chars.max(start + 1));
            let before: String = text.chars().take(start).collect();
            let span: String = text.chars().skip(start).take(end - start).collect();
            let x = data.measure_mono(&before) + pad;
            // Past the end of the line, a character's width marks the place.
            let w = data.measure_mono(&span).max(data.measure_mono("m"));
            out.push(
                div()
                    .class(format!("blinc-code-underline-{}", d.severity.name()))
                    .absolute()
                    .left(x)
                    .top(top + line_height - 3.0)
                    .w(w)
                    .h(2.0)
                    .rounded(1.0),
            );
        }
    }
    out
}

/// The message after a line's text: its most severe diagnostic's, and how many more it has.
pub(super) fn message(
    diagnostics: &[CodeDiagnostic],
    line: usize,
    config: &CodeConfig,
) -> Option<Text> {
    let worst = worst(diagnostics, line)?;
    let more = diagnostics.iter().filter(|d| d.line == line).count() - 1;
    let tail = if more > 0 {
        format!(" (+{more})")
    } else {
        String::new()
    };
    Some(
        text(format!("    {}{tail}", worst.message))
            .class(format!("blinc-code-lens-{}", worst.severity.name()))
            .size(config.font_size * 0.9)
            .no_wrap(),
    )
}

/// The gutter's mark for a line with a diagnostic: a dot in its most severe colour.
pub(super) fn gutter_mark(diagnostics: &[CodeDiagnostic], line: usize, config: &CodeConfig) -> Div {
    let slot = div()
        .w(config.font_size * 0.7)
        .flex_row()
        .items_center()
        .justify_center();
    match worst(diagnostics, line) {
        Some(d) => {
            let dot = config.font_size * 0.4;
            slot.child(
                div()
                    .class(format!("blinc-code-mark-{}", d.severity.name()))
                    .w(dot)
                    .h(dot)
                    .rounded(dot / 2.0),
            )
        }
        None => slot,
    }
}
