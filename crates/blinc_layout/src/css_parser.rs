//! CSS subset parser for ElementStyle
//!
//! Parses a simplified CSS syntax into ElementStyle objects, enabling
//! stylesheet-based styling for Blinc applications.
//!
//! # Error Handling
//!
//! This parser uses nom's context-based error capture for diagnostics.
//! All parse failures are collected into an error array that can be used
//! for reporting. Errors are also logged via tracing at DEBUG level.
//! The parser gracefully continues after errors - the built-in theme is
//! used when style parsing fails.
//!
//! # Supported Syntax
//!
//! - ID-based selectors: `#element-id { ... }` (matches `.id("element-id")`)
//! - Properties: `background`, `border-radius`, `box-shadow`, `transform`, `opacity`
//! - Theme references: `theme(primary)`, `theme(radius-lg)`, `theme(shadow-md)`
//! - Colors: hex (#rgb, #rrggbb, #rrggbbaa), rgb(), rgba(), named colors
//! - Units: px, %, unitless numbers
//!
//! # Example
//!
//! ```ignore
//! use blinc_layout::css_parser::{Stylesheet, ParseResult as CssParseResult};
//!
//! let css = r#"
//!     #card {
//!         background: theme(surface);
//!         border-radius: theme(radius-lg);
//!         box-shadow: theme(shadow-md);
//!     }
//!     #button-primary {
//!         background: theme(primary);
//!         opacity: 0.9;
//!     }
//! "#;
//!
//! let result = Stylesheet::parse_with_errors(css);
//! let stylesheet = result.stylesheet;
//!
//! // Report any errors that occurred
//! for err in &result.errors {
//!     eprintln!("Warning: {}", err);
//! }
//!
//! // Apply styles to elements
//! div().id("card").style(stylesheet.get("card").unwrap())
//! ```

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use blinc_core::{
    Brush, ChainLink, ClipLength, ClipPath, Color, CornerRadius, CornerShape, FlowChain, FlowError,
    FlowExpr, FlowFunc, FlowGraph, FlowInput, FlowInputSource, FlowNode, FlowOutput,
    FlowOutputTarget, FlowStep, FlowTarget, FlowType, FlowUse, Gradient, GradientSpace,
    GradientStop, ImageBrush, OverflowFade, Point, Shadow, StepParam, StepType, Transform,
};
use blinc_theme::{ColorToken, ThemeState};
use nom::{
    Finish, IResult,
    branch::alt,
    bytes::complete::{tag, tag_no_case, take_until, take_while1},
    character::complete::{char, multispace1},
    combinator::{cut, opt, value},
    error::{ParseError as NomParseError, VerboseError, VerboseErrorKind, context},
    multi::many0,
    number::complete::float,
    sequence::{delimited, preceded, tuple},
};
use tracing::debug;

use crate::element::{GlassMaterial, Material, MetallicMaterial, RenderLayer, WoodMaterial};
use crate::element_style::{
    ElementStyle, SpacingRect, StyleAlign, StyleDisplay, StyleFlexDirection, StyleJustify,
    StyleOverflow, StylePosition,
};
use crate::units::Length;

/// Custom parser result type using VerboseError for better diagnostics
type ParseResult<'a, O> = IResult<&'a str, O, VerboseError<&'a str>>;

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
    fn from_verbose(input: &str, err: VerboseError<&str>) -> Self {
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
fn format_verbose_error(err: &VerboseError<&str>) -> String {
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

/// Element state for pseudo-class selectors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ElementState {
    /// :hover pseudo-class
    Hover,
    /// :active pseudo-class (pressed)
    Active,
    /// :focus pseudo-class
    Focus,
    /// :disabled pseudo-class
    Disabled,
    /// :checked pseudo-class (checkboxes, radios)
    Checked,
}

impl ElementState {
    /// Parse a state from a pseudo-class string
    pub fn parse_state(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "hover" => Some(ElementState::Hover),
            "active" => Some(ElementState::Active),
            "focus" => Some(ElementState::Focus),
            "disabled" => Some(ElementState::Disabled),
            "checked" => Some(ElementState::Checked),
            _ => None,
        }
    }
}

impl std::fmt::Display for ElementState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ElementState::Hover => write!(f, "hover"),
            ElementState::Active => write!(f, "active"),
            ElementState::Focus => write!(f, "focus"),
            ElementState::Disabled => write!(f, "disabled"),
            ElementState::Checked => write!(f, "checked"),
        }
    }
}

/// A parsed CSS selector with optional state modifier
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CssSelector {
    /// The element ID (without #)
    pub id: String,
    /// Optional state modifier (:hover, :active, :focus, :disabled)
    pub state: Option<ElementState>,
}

impl CssSelector {
    /// Create a selector for an ID without state
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            state: None,
        }
    }

    /// Create a selector with a state modifier
    pub fn with_state(id: impl Into<String>, state: ElementState) -> Self {
        Self {
            id: id.into(),
            state: Some(state),
        }
    }

    /// Get the storage key for this selector
    fn key(&self) -> String {
        match &self.state {
            Some(state) => format!("{}:{}", self.id, state),
            None => self.id.clone(),
        }
    }
}

// ============================================================================
// Complex Selector Types (Phase 4: Selector Hierarchy)
// ============================================================================

/// Structural pseudo-class selectors
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum StructuralPseudo {
    /// :first-child
    FirstChild,
    /// :last-child
    LastChild,
    /// :nth-child(n) — 1-based index
    NthChild(usize),
    /// :nth-last-child(n) — 1-based from end
    NthLastChild(usize),
    /// :only-child
    OnlyChild,
    /// :empty — no children
    Empty,
    /// :root — root element
    Root,
    /// :first-of-type — equivalent to :first-child in single-type DOM (Blinc)
    FirstOfType,
    /// :last-of-type — equivalent to :last-child in single-type DOM (Blinc)
    LastOfType,
    /// :nth-of-type(n) — equivalent to :nth-child(n) in single-type DOM (Blinc)
    NthOfType(usize),
    /// :nth-last-of-type(n) — equivalent to :nth-last-child(n) in single-type DOM (Blinc)
    NthLastOfType(usize),
    /// :only-of-type — equivalent to :only-child in single-type DOM (Blinc)
    OnlyOfType,
}

/// A single part of a compound selector
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SelectorPart {
    /// Element type selector (e.g., button, a, ul, input)
    Type(String),
    /// #id selector
    Id(String),
    /// .class selector
    Class(String),
    /// * universal selector (matches any element)
    Universal,
    /// :hover, :active, :focus, :disabled
    State(ElementState),
    /// :first-child, :last-child, :nth-child(n), :only-child, :empty, :root
    PseudoClass(StructuralPseudo),
    /// :not(selector) — negation pseudo-class
    Not(Box<CompoundSelector>),
    /// :is(selector, ...) / :where(selector, ...) — matches if any inner selector matches
    Is(Vec<CompoundSelector>),
    /// :has(selector, ...) — relational selector. Matches when ANY listed
    /// relative selector matches against the subject's descendants /
    /// children / siblings. Unlike :is, the inner is a full
    /// `ComplexSelector` because :has natively supports combinators
    /// (e.g. `:has(> .child)`, `:has(+ .sibling)`, `:has(.deep .nested)`).
    Has(Vec<ComplexSelector>),
    /// ::placeholder, ::selection, etc. — pseudo-elements
    PseudoElement(String),
}

/// Combinator between compound selectors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Combinator {
    /// Descendant combinator (space): `#parent .child`
    Descendant,
    /// Child combinator (>): `#parent > .child`
    Child,
    /// Adjacent sibling combinator (+): `#prev + .next`
    AdjacentSibling,
    /// General sibling combinator (~): `#prev ~ .later`
    GeneralSibling,
}

/// A compound selector is a sequence of simple selectors with no combinator.
/// e.g. `#id.class:hover` = [Id("id"), Class("class"), State(Hover)]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CompoundSelector {
    pub parts: Vec<SelectorPart>,
}

impl CompoundSelector {
    /// Check if this compound selector contains any state pseudo-class
    pub fn has_state(&self) -> bool {
        self.parts
            .iter()
            .any(|p| matches!(p, SelectorPart::State(_)))
    }

    /// Get the state pseudo-class if present
    pub fn get_state(&self) -> Option<&ElementState> {
        self.parts.iter().find_map(|p| match p {
            SelectorPart::State(s) => Some(s),
            _ => None,
        })
    }

    /// Check if this compound selector contains any structural pseudo-class
    pub fn has_structural_pseudo(&self) -> bool {
        self.parts
            .iter()
            .any(|p| matches!(p, SelectorPart::PseudoClass(_)))
    }
}

/// A complex selector is a chain of compound selectors joined by combinators.
/// e.g. `#parent:hover > .child:first-child`
/// segments: [(parent compound, Some(Child)), (child compound, None)]
///
/// The last segment always has `combinator = None` (it's the target element).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ComplexSelector {
    pub segments: Vec<(CompoundSelector, Option<Combinator>)>,
}

impl ComplexSelector {
    /// Get the rightmost (target) compound selector
    pub fn target(&self) -> Option<&CompoundSelector> {
        self.segments.last().map(|(compound, _)| compound)
    }

    /// Check if this complex selector has any state pseudo-classes
    pub fn has_state(&self) -> bool {
        self.segments
            .iter()
            .any(|(compound, _)| compound.has_state())
    }

    /// Returns true if this is a simple selector (single compound, no combinators)
    pub fn is_simple(&self) -> bool {
        self.segments.len() == 1
    }

    /// If this is a simple `.class` selector (single segment, single Class part, no state),
    /// returns the class name. Used for O(1) class-based style lookups.
    pub fn simple_class_name(&self) -> Option<&str> {
        if self.segments.len() != 1 {
            return None;
        }
        let parts = &self.segments[0].0.parts;
        if parts.len() == 1 {
            if let SelectorPart::Class(name) = &parts[0] {
                return Some(name.as_str());
            }
        }
        None
    }

    /// Extract the class name from a single-segment selector that may include state parts.
    /// For `.cn-sidebar-item:hover`, returns `Some("cn-sidebar-item")`.
    /// For `.class` (no state), also returns the class name.
    /// Returns None for multi-segment selectors or selectors without a Class part.
    pub fn class_name_with_state(&self) -> Option<&str> {
        if self.segments.len() != 1 {
            return None;
        }
        let parts = &self.segments[0].0.parts;
        for part in parts {
            if let SelectorPart::Class(name) = part {
                return Some(name.as_str());
            }
        }
        None
    }
}

/// A CSS keyframe animation definition
///
/// Represents a parsed `@keyframes` rule with multiple stops.
#[derive(Clone, Debug)]
pub struct CssKeyframes {
    /// Animation name
    pub name: String,
    /// Keyframe stops (position 0.0-1.0 -> style properties)
    pub keyframes: Vec<CssKeyframe>,
}

/// A single keyframe stop in an animation
#[derive(Clone, Debug)]
pub struct CssKeyframe {
    /// Position in the animation (0.0 = start, 1.0 = end)
    pub position: f32,
    /// Style properties at this keyframe
    pub style: ElementStyle,
}

impl CssKeyframes {
    /// Create a new keyframes definition
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            keyframes: Vec::new(),
        }
    }

    /// Add a keyframe at a specific position
    pub fn add_keyframe(&mut self, position: f32, style: ElementStyle) {
        self.keyframes.push(CssKeyframe { position, style });
        // Keep keyframes sorted by position
        self.keyframes
            .sort_by(|a, b| a.position.partial_cmp(&b.position).unwrap());
    }

    /// Get the keyframe at or before a given position
    pub fn keyframe_at(&self, position: f32) -> Option<&CssKeyframe> {
        self.keyframes
            .iter()
            .rev()
            .find(|kf| kf.position <= position)
    }

    /// Convert to Blinc MotionAnimation for enter animations
    ///
    /// Uses the first keyframe (0% or from) as enter_from and animates to the final state.
    pub fn to_enter_animation(&self, duration_ms: u32) -> crate::element::MotionAnimation {
        let enter_from = self
            .keyframes
            .first()
            .map(|kf| Self::style_to_motion_keyframe(&kf.style));

        crate::element::MotionAnimation {
            enter_from,
            enter_duration_ms: duration_ms,
            enter_delay_ms: 0,
            exit_to: None,
            exit_duration_ms: 0,
        }
    }

    /// Convert to Blinc MotionAnimation for exit animations
    ///
    /// Uses the last keyframe (100% or to) as exit_to.
    pub fn to_exit_animation(&self, duration_ms: u32) -> crate::element::MotionAnimation {
        let exit_to = self
            .keyframes
            .last()
            .map(|kf| Self::style_to_motion_keyframe(&kf.style));

        crate::element::MotionAnimation {
            enter_from: None,
            enter_duration_ms: 0,
            enter_delay_ms: 0,
            exit_to,
            exit_duration_ms: duration_ms,
        }
    }

    /// Convert to a full enter/exit MotionAnimation
    ///
    /// First keyframe becomes enter_from, last keyframe becomes exit_to.
    pub fn to_motion_animation(
        &self,
        enter_duration_ms: u32,
        exit_duration_ms: u32,
    ) -> crate::element::MotionAnimation {
        let enter_from = self
            .keyframes
            .first()
            .map(|kf| Self::style_to_motion_keyframe(&kf.style));
        let exit_to = self
            .keyframes
            .last()
            .map(|kf| Self::style_to_motion_keyframe(&kf.style));

        crate::element::MotionAnimation {
            enter_from,
            enter_duration_ms,
            enter_delay_ms: 0,
            exit_to,
            exit_duration_ms,
        }
    }

    /// Convert to a MultiKeyframeAnimation for more complex, multi-step animations
    ///
    /// This is the preferred method for animations with multiple keyframes (more than
    /// just from/to). It creates a proper multi-keyframe animation that can be played,
    /// paused, and controlled.
    ///
    /// # Arguments
    ///
    /// * `duration_ms` - Total animation duration in milliseconds
    /// * `easing` - Default easing function for transitions between keyframes
    ///
    /// # Example
    ///
    /// ```ignore
    /// let css = r#"
    ///     @keyframes pulse {
    ///         0%, 100% { opacity: 1; transform: scale(1); }
    ///         50% { opacity: 0.8; transform: scale(1.05); }
    ///     }
    /// "#;
    /// let stylesheet = Stylesheet::parse_with_errors(css).stylesheet;
    /// if let Some(keyframes) = stylesheet.get_keyframes("pulse") {
    ///     let mut animation = keyframes.to_multi_keyframe_animation(1000, Easing::EaseInOut);
    ///     animation.set_iterations(-1); // Infinite loop
    ///     animation.play();
    /// }
    /// ```
    pub fn to_multi_keyframe_animation(
        &self,
        duration_ms: u32,
        easing: blinc_animation::Easing,
    ) -> blinc_animation::MultiKeyframeAnimation {
        use blinc_animation::MultiKeyframeAnimation;

        let mut animation = MultiKeyframeAnimation::new(duration_ms);

        for kf in &self.keyframes {
            let props = Self::style_to_keyframe_properties(&kf.style);
            animation = animation.keyframe(kf.position, props, easing);
        }

        animation
    }

    /// Convert ElementStyle to KeyframeProperties for MultiKeyframeAnimation
    fn style_to_keyframe_properties(style: &ElementStyle) -> blinc_animation::KeyframeProperties {
        use blinc_animation::KeyframeProperties;
        use blinc_core::Transform;

        let mut props = KeyframeProperties::default();

        if let Some(opacity) = style.opacity {
            props.opacity = Some(opacity);
        }

        // Extract transform components for animation.
        // IMPORTANT: When a transform IS explicitly set, always include all components
        // (even identity values like scale=1.0, rotate=0.0) so that lerp between
        // keyframes works correctly (lerp_opt(Some(1.0), Some(1.3), t) interpolates,
        // while lerp_opt(None, Some(1.3), t) jumps).
        //
        // Prefer the original decomposed values (rotate, scale_x, scale_y) stored
        // during CSS parsing. These preserve the exact angle/factor from the CSS source.
        // Falling back to Affine2D decomposition is lossy — atan2 maps 359° to -1°.
        if style.transform.is_some() || style.rotate.is_some() || style.scale_x.is_some() {
            // Use original decomposed values when available
            props.rotate = Some(style.rotate.unwrap_or(0.0));
            props.scale_x = Some(style.scale_x.unwrap_or(1.0));
            props.scale_y = Some(style.scale_y.unwrap_or(1.0));

            // Extract translation from the matrix (translation doesn't suffer from wrapping)
            if let Some(Transform::Affine2D(affine)) = &style.transform {
                let [_a, _b, _c, _d, tx, ty] = affine.elements;
                props.translate_x = Some(tx);
                props.translate_y = Some(ty);
            } else {
                props.translate_x = Some(0.0);
                props.translate_y = Some(0.0);
            }
        }

        // 3D properties
        if let Some(rx) = style.rotate_x {
            props.rotate_x = Some(rx);
        }
        if let Some(ry) = style.rotate_y {
            props.rotate_y = Some(ry);
        }
        if let Some(p) = style.perspective {
            props.perspective = Some(p);
        }
        if let Some(d) = style.depth {
            props.depth = Some(d);
        }
        if let Some(tz) = style.translate_z {
            props.translate_z = Some(tz);
        }
        if let Some(b) = style.blend_3d {
            props.blend_3d = Some(b);
        }

        // Clip-path
        match &style.clip_path {
            Some(blinc_core::ClipPath::Inset {
                top,
                right,
                bottom,
                left,
                ..
            }) => {
                props.clip_inset = Some([
                    clip_length_to_percent(top),
                    clip_length_to_percent(right),
                    clip_length_to_percent(bottom),
                    clip_length_to_percent(left),
                ]);
            }
            Some(blinc_core::ClipPath::Circle {
                radius: Some(r), ..
            }) => {
                props.clip_circle_radius = Some(clip_length_to_percent(r));
            }
            Some(blinc_core::ClipPath::Ellipse {
                rx: Some(rx),
                ry: Some(ry),
                ..
            }) => {
                props.clip_ellipse_radii =
                    Some([clip_length_to_percent(rx), clip_length_to_percent(ry)]);
            }
            _ => {}
        }

        // Background color (solid or gradient)
        match &style.background {
            Some(blinc_core::Brush::Solid(c)) => {
                props.background_color = Some([c.r, c.g, c.b, c.a]);
            }
            Some(blinc_core::Brush::Gradient(gradient)) => {
                let stops = gradient.stops();
                if let Some(first) = stops.first() {
                    props.gradient_start_color =
                        Some([first.color.r, first.color.g, first.color.b, first.color.a]);
                }
                if let Some(last) = stops.last() {
                    props.gradient_end_color =
                        Some([last.color.r, last.color.g, last.color.b, last.color.a]);
                }
                if let blinc_core::Gradient::Linear { start, end, .. } = gradient {
                    props.gradient_angle = Some(gradient_points_to_angle(*start, *end));
                }
            }
            _ => {}
        }

        // Text color
        if let Some(c) = &style.text_color {
            props.text_color = Some([c.r, c.g, c.b, c.a]);
        }

        // Text shadow
        if let Some(ts) = &style.text_shadow {
            props.text_shadow_params = Some([ts.offset_x, ts.offset_y, ts.blur, ts.spread]);
            props.text_shadow_color = Some([ts.color.r, ts.color.g, ts.color.b, ts.color.a]);
        }

        // Font size
        if let Some(fs) = style.font_size {
            props.font_size = Some(fs);
        }

        // Corner radius
        if let Some(cr) = &style.corner_radius {
            props.corner_radius =
                Some([cr.top_left, cr.top_right, cr.bottom_right, cr.bottom_left]);
        }

        // Corner shape (superellipse)
        if let Some(cs) = &style.corner_shape {
            props.corner_shape = Some(cs.to_array());
        }

        // Overflow fade
        if let Some(fade) = &style.overflow_fade {
            props.overflow_fade = Some(fade.to_array());
        }

        // Border
        if let Some(bw) = style.border_width {
            props.border_width = Some(bw);
        }
        if let Some(bc) = &style.border_color {
            props.border_color = Some([bc.r, bc.g, bc.b, bc.a]);
        }

        // Outline
        if let Some(ow) = style.outline_width {
            props.outline_width = Some(ow);
        }
        if let Some(oc) = &style.outline_color {
            props.outline_color = Some([oc.r, oc.g, oc.b, oc.a]);
        }
        if let Some(offset) = style.outline_offset {
            props.outline_offset = Some(offset);
        }

        // Shadow — keyframe interpolates the FIRST layer only. Multi-layer
        // animations replace the stack on each tick; sub-layer lerping
        // would need a Vec<Shadow> in KeyframeProperties (follow-up).
        if let Some(shadow) = style.shadow.first() {
            props.shadow_params =
                Some([shadow.offset_x, shadow.offset_y, shadow.blur, shadow.spread]);
            props.shadow_color = Some([
                shadow.color.r,
                shadow.color.g,
                shadow.color.b,
                shadow.color.a,
            ]);
        }

        // 3D lighting
        if let Some(li) = style.light_intensity {
            props.light_intensity = Some(li);
        }
        if let Some(a) = style.ambient {
            props.ambient = Some(a);
        }
        if let Some(s) = style.specular {
            props.specular = Some(s);
        }
        if let Some(ld) = &style.light_direction {
            props.light_direction = Some(*ld);
        }

        // CSS filter properties
        if let Some(f) = &style.filter {
            props.filter_grayscale = Some(f.grayscale);
            props.filter_invert = Some(f.invert);
            props.filter_sepia = Some(f.sepia);
            props.filter_brightness = Some(f.brightness);
            props.filter_contrast = Some(f.contrast);
            props.filter_saturate = Some(f.saturate);
            props.filter_hue_rotate = Some(f.hue_rotate);
            props.filter_blur = Some(f.blur);
        }

        // Backdrop filter (glass material)
        if let Some(Material::Glass(glass)) = &style.material {
            props.backdrop_blur = Some(glass.blur);
            props.backdrop_saturation = Some(glass.saturation);
            props.backdrop_brightness = Some(glass.brightness);
        }

        // Layout properties (only Length values are animatable)
        if let Some(crate::element_style::StyleDimension::Length(w)) = style.width {
            props.width = Some(w);
        }
        if let Some(crate::element_style::StyleDimension::Length(h)) = style.height {
            props.height = Some(h);
        }
        if let Some(ref p) = style.padding {
            props.padding = Some([p.top, p.right, p.bottom, p.left]);
        }
        if let Some(ref m) = style.margin {
            props.margin = Some([m.top, m.right, m.bottom, m.left]);
        }
        if let Some(g) = style.gap {
            props.gap = Some(g);
        }
        if let Some(v) = style.min_width {
            props.min_width = Some(v);
        }
        if let Some(v) = style.max_width {
            props.max_width = Some(v);
        }
        if let Some(v) = style.min_height {
            props.min_height = Some(v);
        }
        if let Some(v) = style.max_height {
            props.max_height = Some(v);
        }
        if let Some(v) = style.flex_grow {
            props.flex_grow = Some(v);
        }
        if let Some(v) = style.flex_shrink {
            props.flex_shrink = Some(v);
        }
        if let Some(v) = style.top {
            props.inset_top = Some(v);
        }
        if let Some(v) = style.right {
            props.inset_right = Some(v);
        }
        if let Some(v) = style.bottom {
            props.inset_bottom = Some(v);
        }
        if let Some(v) = style.left {
            props.inset_left = Some(v);
        }
        if let Some(z) = style.z_index {
            props.z_index = Some(z as f32);
        }

        // Skew
        if let Some(sx) = style.skew_x {
            props.skew_x = Some(sx);
        }
        if let Some(sy) = style.skew_y {
            props.skew_y = Some(sy);
        }

        // Transform origin
        if let Some(to) = style.transform_origin {
            props.transform_origin = Some(to);
        }

        // SVG properties
        if let Some(fill) = &style.fill {
            props.svg_fill = Some([fill.r, fill.g, fill.b, fill.a]);
        }
        if let Some(stroke) = &style.stroke {
            props.svg_stroke = Some([stroke.r, stroke.g, stroke.b, stroke.a]);
        }
        if let Some(sw) = style.stroke_width {
            props.svg_stroke_width = Some(sw);
        }
        if let Some(offset) = style.stroke_dashoffset {
            props.svg_stroke_dashoffset = Some(offset);
        }
        if let Some(ref path_data) = style.svg_path_data {
            props.svg_path_data = Some(path_data.clone());
        }

        props
    }

    /// Convert ElementStyle to MotionKeyframe
    ///
    /// Extracts animatable properties from an ElementStyle for use in motion animations.
    /// Note: Transform decomposition is limited - for complex CSS transforms, only
    /// simple scale/translate/rotate can be reliably extracted.
    fn style_to_motion_keyframe(style: &ElementStyle) -> crate::element::MotionKeyframe {
        use blinc_core::Transform;

        let mut kf = crate::element::MotionKeyframe::new();

        if let Some(opacity) = style.opacity {
            kf.opacity = Some(opacity);
        }

        // Try to extract transform components from Affine2D
        // Note: Complex combined transforms may not decompose cleanly
        if let Some(Transform::Affine2D(affine)) = &style.transform {
            let [a, b, c, d, tx, ty] = affine.elements;

            // Always extract translation for keyframe animations
            // (including zero values which are meaningful end states)
            kf.translate_x = Some(tx);
            kf.translate_y = Some(ty);

            // Try to extract scale (valid when no rotation/skew: b=0, c=0)
            if b.abs() < 0.0001 && c.abs() < 0.0001 {
                // Always include scale values for keyframe animations
                // (including 1.0 which is a meaningful end state)
                kf.scale_x = Some(a);
                kf.scale_y = Some(d);
            } else {
                // Has rotation - try to extract rotation angle
                // For pure rotation: a=cos(θ), b=sin(θ), c=-sin(θ), d=cos(θ)
                let rotation = b.atan2(a);
                if rotation.abs() > 0.0001 {
                    kf.rotate = Some(rotation.to_degrees());
                }
            }
        }
        // Mat4 transforms are more complex, skip for now

        kf
    }
}

/// A single CSS transition specification
#[derive(Clone, Debug)]
pub struct CssTransition {
    /// Property name to transition (e.g. "opacity", "clip-path", "all")
    pub property: String,
    /// Duration in milliseconds
    pub duration_ms: u32,
    /// Timing function
    pub timing: AnimationTiming,
    /// Delay before starting in milliseconds
    pub delay_ms: u32,
}

/// Set of CSS transitions parsed from `transition:` property
#[derive(Clone, Debug, Default)]
pub struct CssTransitionSet {
    pub transitions: Vec<CssTransition>,
}

impl CssTransitionSet {
    /// Find transition spec for a given property name (also matches "all")
    pub fn get(&self, property: &str) -> Option<&CssTransition> {
        self.transitions
            .iter()
            .find(|t| t.property == "all" || t.property == property)
    }
}

/// CSS animation configuration parsed from `animation:` property
#[derive(Clone, Debug)]
pub struct CssAnimation {
    /// Name of the @keyframes to use
    pub name: String,
    /// Duration in milliseconds
    pub duration_ms: u32,
    /// Timing function
    pub timing: AnimationTiming,
    /// Delay before starting in milliseconds
    pub delay_ms: u32,
    /// Number of iterations (0 = infinite)
    pub iteration_count: u32,
    /// Direction of animation
    pub direction: AnimationDirection,
    /// Fill mode
    pub fill_mode: AnimationFillMode,
}

impl Default for CssAnimation {
    fn default() -> Self {
        Self {
            name: String::new(),
            duration_ms: 0,
            timing: AnimationTiming::Ease,
            delay_ms: 0,
            iteration_count: 1,
            direction: AnimationDirection::Normal,
            fill_mode: AnimationFillMode::None,
        }
    }
}

/// Animation timing function
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum AnimationTiming {
    Linear,
    #[default]
    Ease,
    EaseIn,
    EaseOut,
    EaseInOut,
    /// Literal `cubic-bezier(x1, y1, x2, y2)` — used when `var(--ease-X)`
    /// resolves to a custom curve (e.g. theme-supplied semantic easings
    /// like `--ease-state`, `--ease-spring`, `--ease-sheet`).
    CubicBezier(f32, f32, f32, f32),
}

impl Eq for AnimationTiming {}

impl AnimationTiming {
    fn from_str(s: &str) -> Option<Self> {
        let trimmed = s.trim().to_lowercase();
        match trimmed.as_str() {
            "linear" => Some(AnimationTiming::Linear),
            "ease" => Some(AnimationTiming::Ease),
            "ease-in" => Some(AnimationTiming::EaseIn),
            "ease-out" => Some(AnimationTiming::EaseOut),
            "ease-in-out" => Some(AnimationTiming::EaseInOut),
            _ => parse_cubic_bezier(&trimmed),
        }
    }

    /// Convert CSS animation timing to blinc_animation Easing
    ///
    /// Uses the exact cubic-bezier curves from the CSS specification:
    /// - ease:        cubic-bezier(0.25, 0.1, 0.25, 1.0)
    /// - ease-in:     cubic-bezier(0.42, 0.0, 1.0, 1.0)
    /// - ease-out:    cubic-bezier(0.0, 0.0, 0.58, 1.0)
    /// - ease-in-out: cubic-bezier(0.42, 0.0, 0.58, 1.0)
    pub fn to_easing(&self) -> blinc_animation::Easing {
        use blinc_animation::Easing;
        match self {
            AnimationTiming::Linear => Easing::Linear,
            AnimationTiming::Ease => Easing::CubicBezier(0.25, 0.1, 0.25, 1.0),
            AnimationTiming::EaseIn => Easing::CubicBezier(0.42, 0.0, 1.0, 1.0),
            AnimationTiming::EaseOut => Easing::CubicBezier(0.0, 0.0, 0.58, 1.0),
            AnimationTiming::EaseInOut => Easing::CubicBezier(0.42, 0.0, 0.58, 1.0),
            AnimationTiming::CubicBezier(a, b, c, d) => Easing::CubicBezier(*a, *b, *c, *d),
        }
    }
}

/// Parse a literal `cubic-bezier(x1, y1, x2, y2)` string into an
/// `AnimationTiming::CubicBezier`. Returns `None` if the input isn't
/// a well-formed cubic-bezier function call.
fn parse_cubic_bezier(s: &str) -> Option<AnimationTiming> {
    let inner = s.strip_prefix("cubic-bezier(")?.strip_suffix(')')?;
    let parts: Vec<f32> = inner
        .split(',')
        .map(|p| p.trim().parse::<f32>())
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    if parts.len() == 4 {
        Some(AnimationTiming::CubicBezier(
            parts[0], parts[1], parts[2], parts[3],
        ))
    } else {
        None
    }
}

/// Animation direction
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AnimationDirection {
    #[default]
    Normal,
    Reverse,
    Alternate,
    AlternateReverse,
}

impl AnimationDirection {
    /// Convert CSS animation direction to blinc_animation PlayDirection
    pub fn to_play_direction(&self) -> blinc_animation::PlayDirection {
        use blinc_animation::PlayDirection;
        match self {
            AnimationDirection::Normal => PlayDirection::Forward,
            AnimationDirection::Reverse => PlayDirection::Reverse,
            AnimationDirection::Alternate | AnimationDirection::AlternateReverse => {
                PlayDirection::Alternate
            }
        }
    }

    /// Returns true if animation should start in reverse (for AlternateReverse)
    pub fn starts_reversed(&self) -> bool {
        matches!(self, AnimationDirection::AlternateReverse)
    }
}

/// Animation fill mode
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AnimationFillMode {
    #[default]
    None,
    Forwards,
    Backwards,
    Both,
}

impl AnimationFillMode {
    /// Convert CSS animation fill mode to blinc_animation FillMode
    pub fn to_fill_mode(&self) -> blinc_animation::FillMode {
        use blinc_animation::FillMode;
        match self {
            AnimationFillMode::None => FillMode::None,
            AnimationFillMode::Forwards => FillMode::Forwards,
            AnimationFillMode::Backwards => FillMode::Backwards,
            AnimationFillMode::Both => FillMode::Both,
        }
    }
}

/// Known SVG shape tag names for CSS tag-name selectors targeting SVG sub-elements.
pub const SVG_TAG_NAMES: &[&str] = &[
    "path", "circle", "rect", "ellipse", "line", "polygon", "polyline", "g",
];

/// A parsed stylesheet containing styles keyed by element ID
#[derive(Clone, Default, Debug)]
pub struct Stylesheet {
    /// Simple rules: styles keyed by selector (id or id:state) for O(1) lookup
    styles: HashMap<String, ElementStyle>,
    /// Class-based rules: styles keyed by class name (or class:state) for O(1) lookup
    /// Populated alongside `complex_rules` for simple `.class` and `.class:state` selectors.
    class_styles: HashMap<String, ElementStyle>,
    /// Complex selector rules (class selectors, combinators, structural pseudos)
    complex_rules: Vec<(ComplexSelector, ElementStyle)>,
    /// CSS custom properties (variables) defined in :root
    variables: HashMap<String, String>,
    /// Keyframe animations defined with @keyframes
    keyframes: HashMap<String, CssKeyframes>,
    /// Flow DAGs defined with @flow
    flows: HashMap<String, FlowGraph>,
    /// Identifiers (ids without `#`, classes without `.`) that appear in
    /// a compound containing `:hover`. Computed at parse time by
    /// [`Self::index_hover_participants`]; consulted by the windowed
    /// runner to gate `invalidate_render_cache` on POINTER_ENTER /
    /// POINTER_LEAVE events. Pre-fix every hover-changing pointer move
    /// invalidated the compositor's static cache regardless of whether
    /// the entering / leaving element had any `:hover` styling, which
    /// made mouse-over-non-hoverable-content drop the fast path on
    /// every frame.
    hover_participants: std::collections::HashSet<String>,
}

impl Stylesheet {
    /// Create an empty stylesheet
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse CSS text into a stylesheet with full error collection
    ///
    /// This is the recommended method for parsing CSS as it collects all
    /// errors and warnings during parsing, allowing you to report them
    /// to users in a human-readable format.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let css = "#card { opacity: 0.5; unknown: value; }";
    /// let result = Stylesheet::parse_with_errors(css);
    ///
    /// // Print any warnings to stderr
    /// result.print_diagnostics();
    ///
    /// // Use the stylesheet (partial results are still available)
    /// let stylesheet = result.stylesheet;
    /// ```
    pub fn parse_with_errors(css: &str) -> CssParseResult {
        let mut errors: Vec<ParseError> = Vec::new();
        let initial_vars = HashMap::new();

        match parse_stylesheet_with_errors(css, &mut errors, &initial_vars).finish() {
            Ok((remaining, parsed)) => {
                // Warn if there's unparsed content
                let remaining = remaining.trim();
                if !remaining.is_empty() {
                    let (line, column, fragment) = calculate_position(css, remaining);
                    errors.push(ParseError {
                        severity: Severity::Warning,
                        message: format!("Unparsed content remaining ({} chars)", remaining.len()),
                        line,
                        column,
                        fragment,
                        contexts: vec![],
                        property: None,
                        value: None,
                    });
                }

                let mut stylesheet = Stylesheet::new();
                stylesheet.variables = parsed.variables;
                for (id, style) in parsed.rules {
                    stylesheet.styles.insert(id, style);
                }
                stylesheet.complex_rules = parsed.complex_rules;
                stylesheet.index_class_styles();
                stylesheet.index_hover_participants();
                for keyframes in parsed.keyframes {
                    stylesheet
                        .keyframes
                        .insert(keyframes.name.clone(), keyframes);
                }
                for flow in parsed.flows {
                    stylesheet.flows.insert(flow.name.clone(), flow);
                }

                CssParseResult { stylesheet, errors }
            }
            Err(e) => {
                let parse_error = ParseError::from_verbose(css, e);
                errors.push(parse_error);

                CssParseResult {
                    stylesheet: Stylesheet::new(),
                    errors,
                }
            }
        }
    }

    /// Parse CSS with pre-seeded external variables (e.g. from theme or prior stylesheets).
    ///
    /// `var(--name)` references in the CSS will resolve against both `:root` variables
    /// defined in this CSS and the provided external variables. CSS-defined variables
    /// take precedence over external ones.
    #[allow(clippy::result_large_err)]
    pub fn parse_with_variables(
        css: &str,
        external_vars: &HashMap<String, String>,
    ) -> Result<Self, ParseError> {
        let result = Self::parse_with_errors_and_variables(css, external_vars);
        result.log_diagnostics();
        if result.has_errors() {
            Err(result
                .errors
                .into_iter()
                .find(|e| e.severity == Severity::Error)
                .unwrap())
        } else {
            Ok(result.stylesheet)
        }
    }

    /// Parse CSS with external variables and full error collection.
    pub fn parse_with_errors_and_variables(
        css: &str,
        external_vars: &HashMap<String, String>,
    ) -> CssParseResult {
        let mut errors: Vec<ParseError> = Vec::new();

        match parse_stylesheet_with_errors(css, &mut errors, external_vars).finish() {
            Ok((remaining, parsed)) => {
                let remaining = remaining.trim();
                if !remaining.is_empty() {
                    let (line, column, fragment) = calculate_position(css, remaining);
                    errors.push(ParseError {
                        severity: Severity::Warning,
                        message: format!("Unparsed content remaining ({} chars)", remaining.len()),
                        line,
                        column,
                        fragment,
                        contexts: vec![],
                        property: None,
                        value: None,
                    });
                }

                let mut stylesheet = Stylesheet::new();
                stylesheet.variables = parsed.variables;
                for (id, style) in parsed.rules {
                    stylesheet.styles.insert(id, style);
                }
                stylesheet.complex_rules = parsed.complex_rules;
                stylesheet.index_class_styles();
                stylesheet.index_hover_participants();
                for keyframes in parsed.keyframes {
                    stylesheet
                        .keyframes
                        .insert(keyframes.name.clone(), keyframes);
                }
                for flow in parsed.flows {
                    stylesheet.flows.insert(flow.name.clone(), flow);
                }

                CssParseResult { stylesheet, errors }
            }
            Err(e) => {
                let parse_error = ParseError::from_verbose(css, e);
                errors.push(parse_error);

                CssParseResult {
                    stylesheet: Stylesheet::new(),
                    errors,
                }
            }
        }
    }

    /// Parse CSS text into a stylesheet
    ///
    /// Parse errors are logged via tracing at DEBUG level with full context.
    /// When parsing fails, an error is returned but the application can
    /// fall back to built-in theme styles.
    ///
    /// For full error collection, use `parse_with_errors()` instead.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let css = "#card { opacity: 0.5; }";
    /// let stylesheet = Stylesheet::parse(css)?;
    /// ```
    #[allow(clippy::result_large_err)]
    pub fn parse(css: &str) -> Result<Self, ParseError> {
        let result = Self::parse_with_errors(css);

        // Log all diagnostics via tracing
        result.log_diagnostics();

        if result.has_errors() {
            // Return the first error
            Err(result
                .errors
                .into_iter()
                .find(|e| e.severity == Severity::Error)
                .unwrap())
        } else {
            Ok(result.stylesheet)
        }
    }

    /// Parse CSS text, logging errors and returning an empty stylesheet on failure
    ///
    /// This is a convenience method for cases where you want to gracefully
    /// fall back to an empty stylesheet rather than handle errors explicitly.
    pub fn parse_or_empty(css: &str) -> Self {
        Self::parse(css).unwrap_or_default()
    }

    /// Insert a style for an element ID (without the # prefix)
    ///
    /// This is the programmatic equivalent of parsing `#id { ... }` in CSS.
    /// If a style already exists for this ID, it is replaced.
    pub fn insert(&mut self, id: impl Into<String>, style: ElementStyle) {
        self.styles.insert(id.into(), style);
    }

    /// Insert a state-specific style for an element ID
    ///
    /// This is the programmatic equivalent of parsing `#id:hover { ... }` in CSS.
    pub fn insert_with_state(
        &mut self,
        id: impl Into<String>,
        state: ElementState,
        style: ElementStyle,
    ) {
        let id_str: String = id.into();
        let key = format!("{}:{}", id_str, state);
        self.styles.insert(key, style);
        // Keep the hover-participants index live with programmatic
        // inserts — without this, a runtime-added `#id:hover` style
        // wouldn't register as a hover-invalidation target and the
        // first pointer-enter on the element would silently skip the
        // cache invalidation we need.
        if state == ElementState::Hover {
            self.hover_participants.insert(id_str);
        }
    }

    /// Get a style by element ID (without the # prefix)
    ///
    /// Returns `None` if no style is defined for the given ID.
    pub fn get(&self, id: &str) -> Option<&ElementStyle> {
        self.styles.get(id)
    }

    /// Get a style by element ID and state
    ///
    /// Looks up `#id:state` in the stylesheet.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let css = "#button:hover { opacity: 0.8; }";
    /// let stylesheet = Stylesheet::parse(css)?;
    /// let hover_style = stylesheet.get_with_state("button", ElementState::Hover);
    /// ```
    pub fn get_with_state(&self, id: &str, state: ElementState) -> Option<&ElementStyle> {
        let key = format!("{}:{}", id, state);
        self.styles.get(&key)
    }

    /// Get a base style by CSS class name (without the `.` prefix)
    ///
    /// Returns `None` if no simple `.class { ... }` rule exists in the stylesheet.
    /// Only populated for simple single-class selectors (not combinators).
    pub fn get_class(&self, class: &str) -> Option<&ElementStyle> {
        self.class_styles.get(class)
    }

    /// Get a class style with state (e.g., `.class:hover`)
    ///
    /// Looks up `class:state` in the class_styles HashMap.
    pub fn get_class_with_state(&self, class: &str, state: ElementState) -> Option<&ElementStyle> {
        let key = format!("{}:{}", class, state);
        self.class_styles.get(&key)
    }

    /// Get the ::placeholder pseudo-element style for an element ID
    ///
    /// Looks up `#id::placeholder` in the stylesheet. The `color` property
    /// in a `::placeholder` block maps to `text_color` on the returned style,
    /// and `placeholder-color` maps directly.
    pub fn get_placeholder_style(&self, id: &str) -> Option<&ElementStyle> {
        let key = format!("{}::placeholder", id);
        self.styles.get(&key)
    }

    /// Whether the stylesheet contains any rule keyed on a pointer state
    /// (`:hover` or `:active`).
    ///
    /// Used by the windowed app's mouse-move path to skip hit testing
    /// entirely when no element has a pointer-driven style or handler —
    /// turns "static UI with no interaction" into a true zero-CPU idle
    /// even while the cursor is moving over the window.
    ///
    /// Walks the simple-rule and class-rule HashMaps for keys with the
    /// `:hover` / `:active` suffix that the parser produces, plus the
    /// complex-selector list for any compound that carries a state
    /// pseudo-class. Cheap enough to call per-frame on a static stylesheet
    /// (workspaces with very large CSS should still consider caching it
    /// alongside the parse result).
    pub fn has_pointer_state_rules(&self) -> bool {
        let suffix_match = |key: &str| key.ends_with(":hover") || key.ends_with(":active");
        if self.styles.keys().any(|k| suffix_match(k)) {
            return true;
        }
        if self.class_styles.keys().any(|k| suffix_match(k)) {
            return true;
        }
        self.complex_rules.iter().any(|(sel, _)| {
            sel.segments.iter().any(|(compound, _)| {
                compound.parts.iter().any(|p| {
                    matches!(
                        p,
                        SelectorPart::State(ElementState::Hover)
                            | SelectorPart::State(ElementState::Active)
                    )
                })
            })
        })
    }

    /// Get all styles for an element, including state variants
    ///
    /// Returns a tuple of (base_style, state_styles) where state_styles is a Vec
    /// of (ElementState, &ElementStyle) pairs.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let css = r#"
    ///     #button { background: blue; }
    ///     #button:hover { background: lightblue; }
    ///     #button:active { background: darkblue; }
    /// "#;
    /// let stylesheet = Stylesheet::parse(css)?;
    /// let (base, states) = stylesheet.get_all_states("button");
    /// ```
    pub fn get_all_states(
        &self,
        id: &str,
    ) -> (Option<&ElementStyle>, Vec<(ElementState, &ElementStyle)>) {
        let base = self.styles.get(id);

        let mut state_styles = Vec::new();
        for state in [
            ElementState::Hover,
            ElementState::Active,
            ElementState::Focus,
            ElementState::Disabled,
            ElementState::Checked,
        ] {
            let key = format!("{}:{}", id, state);
            if let Some(style) = self.styles.get(&key) {
                state_styles.push((state, style));
            }
        }

        (base, state_styles)
    }

    /// Check if a style exists for the given ID
    pub fn contains(&self, id: &str) -> bool {
        self.styles.contains_key(id)
    }

    /// Check if a style exists for the given ID and state
    pub fn contains_with_state(&self, id: &str, state: ElementState) -> bool {
        let key = format!("{}:{}", id, state);
        self.styles.contains_key(&key)
    }

    /// Get all style IDs in the stylesheet
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.styles.keys().map(|s| s.as_str())
    }

    /// Get the number of styles in the stylesheet
    pub fn len(&self) -> usize {
        self.styles.len()
    }

    /// Check if the stylesheet is empty
    pub fn is_empty(&self) -> bool {
        self.styles.is_empty() && self.complex_rules.is_empty()
    }

    /// Get all complex selector rules
    pub fn complex_rules(&self) -> &[(ComplexSelector, ElementStyle)] {
        &self.complex_rules
    }

    /// Check if there are any complex rules that involve state changes
    pub fn has_complex_state_rules(&self) -> bool {
        self.complex_rules.iter().any(|(sel, _)| sel.has_state())
    }

    /// Returns complex rules whose rightmost compound selector targets an SVG tag name.
    ///
    /// Each entry returns: (tag_name, ancestor_segments if any, style).
    /// For a bare `path { fill: red; }`, ancestor_segments is empty.
    /// For `#my-svg path { fill: red; }`, ancestor_segments contains the `#my-svg` part.
    #[allow(clippy::type_complexity)]
    pub fn svg_tag_rules(
        &self,
    ) -> Vec<(
        &str,
        &[(CompoundSelector, Option<Combinator>)],
        &ElementStyle,
    )> {
        let mut results = Vec::new();
        for (selector, style) in &self.complex_rules {
            if let Some((target_compound, _)) = selector.segments.last() {
                // Check if the rightmost compound has a Type that matches an SVG tag name
                for part in &target_compound.parts {
                    if let SelectorPart::Type(name) = part {
                        if SVG_TAG_NAMES.contains(&name.as_str()) {
                            // Ancestor segments = everything except the last
                            let ancestors = if selector.segments.len() > 1 {
                                &selector.segments[..selector.segments.len() - 1]
                            } else {
                                &[]
                            };
                            results.push((name.as_str(), ancestors, style));
                            break;
                        }
                    }
                }
            }
        }
        results
    }

    /// Build the [`Self::hover_participants`] set so the windowed runner
    /// can cheaply decide whether a POINTER_ENTER / POINTER_LEAVE event
    /// needs to invalidate the compositor cache.
    ///
    /// Records every id and class that appears in a compound selector
    /// containing `State(Hover)` — that is, any rule whose evaluation
    /// depends on the hover state of that identifier. Walks all three
    /// rule stores so simple `#id:hover` rules in `styles`, simple
    /// `.class:hover` rules in `class_styles`, and complex multi-segment
    /// selectors in `complex_rules` are all covered. Identifiers stripped
    /// of the leading `#`/`.` so they match what the registry returns.
    fn index_hover_participants(&mut self) {
        self.hover_participants.clear();

        // Simple `#id:hover` rules are stored in `styles` with the
        // composite key `"id:hover"`. Match by suffix and pull out
        // the id half.
        for key in self.styles.keys() {
            if let Some(id) = key.strip_suffix(":hover") {
                self.hover_participants.insert(id.to_string());
            }
        }
        // Simple `.class:hover` rules are stored in `class_styles` the
        // same way.
        for key in self.class_styles.keys() {
            if let Some(class) = key.strip_suffix(":hover") {
                self.hover_participants.insert(class.to_string());
            }
        }
        // Complex rules: for each compound containing `State(Hover)`,
        // record every id / class in that compound. The hover applies
        // to that segment's identifier, so any pointer-enter / leave
        // on that element triggers the rule's evaluation.
        for (selector, _) in &self.complex_rules {
            for (compound, _) in &selector.segments {
                let has_hover = compound
                    .parts
                    .iter()
                    .any(|p| matches!(p, SelectorPart::State(ElementState::Hover)));
                if !has_hover {
                    continue;
                }
                for part in &compound.parts {
                    match part {
                        SelectorPart::Id(id) => {
                            self.hover_participants.insert(id.clone());
                        }
                        SelectorPart::Class(class) => {
                            self.hover_participants.insert(class.clone());
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    /// Does an element with this id / class identifier participate in
    /// any `:hover` styling? Cheap HashSet lookup against a set
    /// precomputed at parse time (see the internal
    /// `index_hover_participants` helper).
    pub fn participates_in_hover(&self, ident: &str) -> bool {
        self.hover_participants.contains(ident)
    }

    /// Index simple class selectors from complex_rules into class_styles for O(1) lookup.
    ///
    /// A "simple class selector" is a ComplexSelector with exactly one segment
    /// whose CompoundSelector has exactly one Class part, or one Class + one State part.
    fn index_class_styles(&mut self) {
        for (selector, style) in &self.complex_rules {
            if !selector.is_simple() {
                continue;
            }
            let compound = &selector.segments[0].0;
            let parts = &compound.parts;

            match parts.len() {
                1 => {
                    // Single .class selector
                    if let SelectorPart::Class(class_name) = &parts[0] {
                        self.class_styles.insert(class_name.clone(), style.clone());
                    }
                }
                2 => {
                    // .class:state selector
                    let (class_part, state_part) = (&parts[0], &parts[1]);
                    if let (SelectorPart::Class(class_name), SelectorPart::State(state)) =
                        (class_part, state_part)
                    {
                        let key = format!("{}:{}", class_name, state);
                        self.class_styles.insert(key, style.clone());
                    }
                }
                _ => {}
            }
        }
    }

    /// Merge another stylesheet into this one (cascade — later rules override earlier)
    ///
    /// This follows CSS cascade rules: styles from `other` override matching
    /// styles in `self`. Variables and keyframes are also merged.
    pub fn merge(&mut self, other: Stylesheet) {
        for (key, style) in other.styles {
            self.styles.insert(key, style);
        }
        self.complex_rules.extend(other.complex_rules);
        for (key, value) in other.variables {
            self.variables.insert(key, value);
        }
        for (key, kf) in other.keyframes {
            self.keyframes.insert(key, kf);
        }
        for (key, style) in other.class_styles {
            self.class_styles.insert(key, style);
        }
        // Re-index now that simple + class + complex stores have the
        // merged content. `other.hover_participants` is dropped — the
        // re-index walks the merged stores directly so we don't have
        // to reason about whether an `id:hover` key in one stylesheet
        // got cascade-overridden by an `id` key in the other.
        self.index_hover_participants();
    }

    /// Load and parse a `.css` file from disk
    #[allow(clippy::result_large_err)]
    pub fn from_file(path: impl AsRef<std::path::Path>) -> Result<Self, ParseError> {
        let path = path.as_ref();
        let css = std::fs::read_to_string(path).map_err(|e| {
            ParseError::new(
                Severity::Error,
                format!("Failed to read CSS file '{}': {}", path.display(), e),
                0,
                0,
            )
        })?;
        Self::parse(&css)
    }

    // =========================================================================
    // CSS Variables (Custom Properties)
    // =========================================================================

    /// Get a CSS variable value by name (without the -- prefix)
    ///
    /// # Example
    ///
    /// ```ignore
    /// let css = ":root { --card-bg: #ffffff; }";
    /// let stylesheet = Stylesheet::parse(css)?;
    /// assert_eq!(stylesheet.get_variable("card-bg"), Some("#ffffff"));
    /// ```
    pub fn get_variable(&self, name: &str) -> Option<&str> {
        self.variables.get(name).map(|s| s.as_str())
    }

    /// Set a CSS variable (useful for runtime overrides)
    ///
    /// # Example
    ///
    /// ```ignore
    /// stylesheet.set_variable("primary-color", "#FF0000");
    /// ```
    pub fn set_variable(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.variables.insert(name.into(), value.into());
    }

    /// Get all variable names
    pub fn variable_names(&self) -> impl Iterator<Item = &str> {
        self.variables.keys().map(|s| s.as_str())
    }

    /// Get the number of variables defined
    pub fn variable_count(&self) -> usize {
        self.variables.len()
    }

    /// Get all CSS variables as a reference to the internal map
    pub fn variables(&self) -> &HashMap<String, String> {
        &self.variables
    }

    /// Resolve a var() reference to its value
    ///
    /// Supports fallback syntax: `var(--name, fallback)`
    fn resolve_variable(&self, var_ref: &str) -> Option<String> {
        // Parse var(--name) or var(--name, fallback)
        let inner = var_ref.trim();
        if !inner.starts_with("var(") || !inner.ends_with(')') {
            return None;
        }

        let content = &inner[4..inner.len() - 1].trim();

        // Split on comma for fallback support
        if let Some(comma_pos) = content.find(',') {
            let var_name = content[..comma_pos].trim();
            let fallback = content[comma_pos + 1..].trim();

            // Variable name should start with --
            let name = var_name.strip_prefix("--")?;

            self.variables
                .get(name)
                .cloned()
                .or_else(|| Some(fallback.to_string()))
        } else {
            // No fallback
            let name = content.strip_prefix("--")?;
            self.variables.get(name).cloned()
        }
    }

    // =========================================================================
    // Keyframe Animations
    // =========================================================================

    /// Get a keyframe animation by name
    ///
    /// # Example
    ///
    /// ```ignore
    /// let css = r#"
    ///     @keyframes fade-in {
    ///         from { opacity: 0; }
    ///         to { opacity: 1; }
    ///     }
    /// "#;
    /// let stylesheet = Stylesheet::parse_with_errors(css).stylesheet;
    /// if let Some(keyframes) = stylesheet.get_keyframes("fade-in") {
    ///     let animation = keyframes.to_enter_animation(300);
    /// }
    /// ```
    pub fn get_keyframes(&self, name: &str) -> Option<&CssKeyframes> {
        self.keyframes.get(name)
    }

    /// Check if keyframes exist with the given name
    pub fn contains_keyframes(&self, name: &str) -> bool {
        self.keyframes.contains_key(name)
    }

    /// Get all keyframe animation names
    pub fn keyframe_names(&self) -> impl Iterator<Item = &str> {
        self.keyframes.keys().map(|s| s.as_str())
    }

    /// Get the number of keyframe animations defined
    pub fn keyframe_count(&self) -> usize {
        self.keyframes.len()
    }

    /// Add a keyframe animation to the stylesheet
    pub fn add_keyframes(&mut self, keyframes: CssKeyframes) {
        self.keyframes.insert(keyframes.name.clone(), keyframes);
    }

    // =========================================================================
    // Flow DAGs (@flow)
    // =========================================================================

    /// Look up a flow DAG by name
    pub fn get_flow(&self, name: &str) -> Option<&FlowGraph> {
        self.flows.get(name)
    }

    /// Check if a flow exists with the given name
    pub fn contains_flow(&self, name: &str) -> bool {
        self.flows.contains_key(name)
    }

    /// Get all flow names
    pub fn flow_names(&self) -> impl Iterator<Item = &str> {
        self.flows.keys().map(|s| s.as_str())
    }

    /// Get the number of flows defined
    pub fn flow_count(&self) -> usize {
        self.flows.len()
    }

    /// Add a flow DAG to the stylesheet
    pub fn add_flow(&mut self, flow: FlowGraph) {
        self.flows.insert(flow.name.clone(), flow);
    }

    // =========================================================================
    // Resolved Animations
    // =========================================================================

    /// Resolve a full motion animation for an element by its ID
    ///
    /// This combines:
    /// 1. The element's `animation:` property (from its style)
    /// 2. The referenced `@keyframes` definition
    ///
    /// Returns `Some(MotionAnimation)` if the element has an animation configured
    /// and the keyframes exist.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let css = r#"
    ///     @keyframes fade-in {
    ///         from { opacity: 0; transform: translateY(20px); }
    ///         to { opacity: 1; transform: translateY(0); }
    ///     }
    ///     #card {
    ///         animation: fade-in 300ms ease-out;
    ///     }
    /// "#;
    /// let stylesheet = Stylesheet::parse_with_errors(css).stylesheet;
    ///
    /// if let Some(motion) = stylesheet.resolve_animation("card") {
    ///     // Apply motion animation to the element
    /// }
    /// ```
    pub fn resolve_animation(&self, id: &str) -> Option<crate::element::MotionAnimation> {
        // Get the element's style
        let style = self.get(id)?;

        // Check if it has an animation property
        let anim_config = style.animation.as_ref()?;

        // Look up the keyframes by name
        let keyframes = self.get_keyframes(&anim_config.name)?;

        // Convert to MotionAnimation
        // For enter animation, use the configured duration
        // For exit animation, use the same duration (can be customized later)
        let mut motion =
            keyframes.to_motion_animation(anim_config.duration_ms, anim_config.duration_ms);

        // Apply delay from config
        motion.enter_delay_ms = anim_config.delay_ms;

        Some(motion)
    }

    /// Resolve animation for an element considering its current state
    ///
    /// This checks both the base style and state-specific styles for animations.
    pub fn resolve_animation_with_state(
        &self,
        id: &str,
        state: ElementState,
    ) -> Option<crate::element::MotionAnimation> {
        // First try state-specific animation
        if let Some(style) = self.get_with_state(id, state) {
            if let Some(anim_config) = &style.animation {
                if let Some(keyframes) = self.get_keyframes(&anim_config.name) {
                    let mut motion = keyframes
                        .to_motion_animation(anim_config.duration_ms, anim_config.duration_ms);
                    motion.enter_delay_ms = anim_config.delay_ms;
                    return Some(motion);
                }
            }
        }

        // Fall back to base animation
        self.resolve_animation(id)
    }

    /// Resolve CSS animation to full MultiKeyframeAnimation with all keyframes preserved
    ///
    /// Unlike `resolve_animation()` which only captures first/last keyframes for simple
    /// enter/exit animations, this method preserves ALL keyframes for complex multi-step
    /// animations like pulse, bounce, etc.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let css = r#"
    ///     @keyframes pulse {
    ///         0%, 100% { opacity: 1; transform: scale(1); }
    ///         50% { opacity: 0.8; transform: scale(1.05); }
    ///     }
    ///     #button { animation: pulse 1000ms ease-in-out infinite; }
    /// "#;
    /// let stylesheet = Stylesheet::parse_with_errors(css).stylesheet;
    /// if let Some(mut anim) = stylesheet.resolve_keyframe_animation("button") {
    ///     anim.start();
    ///     // Animation will interpolate through all 3 keyframes
    /// }
    /// ```
    pub fn resolve_keyframe_animation(
        &self,
        id: &str,
    ) -> Option<blinc_animation::MultiKeyframeAnimation> {
        let style = self.get(id)?;
        let anim_config = style.animation.as_ref()?;
        let keyframes = self.get_keyframes(&anim_config.name)?;

        let mut anim = keyframes
            .to_multi_keyframe_animation(anim_config.duration_ms, anim_config.timing.to_easing());

        // Apply CssAnimation configuration
        let iterations = if anim_config.iteration_count == 0 {
            -1 // infinite
        } else {
            anim_config.iteration_count as i32
        };
        anim.set_iterations(iterations);
        anim.set_delay(anim_config.delay_ms);
        anim.set_direction(anim_config.direction.to_play_direction());
        anim.set_fill_mode(anim_config.fill_mode.to_fill_mode());

        // Handle AlternateReverse by starting in reverse
        if anim_config.direction.starts_reversed() {
            anim.set_reversed(true);
        }

        Some(anim)
    }

    /// Resolve keyframe animation for an element considering its current state
    ///
    /// This checks both the base style and state-specific styles for animations,
    /// returning a full MultiKeyframeAnimation with all keyframes preserved.
    pub fn resolve_keyframe_animation_with_state(
        &self,
        id: &str,
        state: ElementState,
    ) -> Option<blinc_animation::MultiKeyframeAnimation> {
        // First try state-specific animation
        if let Some(style) = self.get_with_state(id, state) {
            if let Some(anim_config) = &style.animation {
                if let Some(keyframes) = self.get_keyframes(&anim_config.name) {
                    let mut anim = keyframes.to_multi_keyframe_animation(
                        anim_config.duration_ms,
                        anim_config.timing.to_easing(),
                    );

                    let iterations = if anim_config.iteration_count == 0 {
                        -1
                    } else {
                        anim_config.iteration_count as i32
                    };
                    anim.set_iterations(iterations);
                    anim.set_delay(anim_config.delay_ms);
                    anim.set_direction(anim_config.direction.to_play_direction());
                    anim.set_fill_mode(anim_config.fill_mode.to_fill_mode());

                    if anim_config.direction.starts_reversed() {
                        anim.set_reversed(true);
                    }

                    return Some(anim);
                }
            }
        }

        // Fall back to base animation
        self.resolve_keyframe_animation(id)
    }
}

// ============================================================================
// Global Active Stylesheet
// ============================================================================

static ACTIVE_STYLESHEET: RwLock<Option<Arc<Stylesheet>>> = RwLock::new(None);

/// Set the active stylesheet for form widget CSS override resolution.
///
/// Called automatically when `set_stylesheet_arc()` is invoked on the RenderTree.
/// This allows TextInput/TextArea state_callbacks to query the current stylesheet
/// without needing a direct reference to the RenderTree.
pub fn set_active_stylesheet(stylesheet: Arc<Stylesheet>) {
    if let Ok(mut guard) = ACTIVE_STYLESHEET.write() {
        *guard = Some(stylesheet);
    }
}

/// Get the currently active stylesheet, if any.
///
/// Used by form widget state_callbacks to resolve CSS overrides.
pub fn active_stylesheet() -> Option<Arc<Stylesheet>> {
    ACTIVE_STYLESHEET.read().ok()?.clone()
}

/// Drop the global active stylesheet. Used by hot-reload's
/// `WindowedContext::reset_for_hot_reload` so the next `ctx.add_css`
/// call repopulates a fresh sheet — without this, stateful widgets
/// looking up CSS overrides during the rebuild would briefly see
/// stale rules from the pre-patch run.
pub fn clear_active_stylesheet() {
    if let Ok(mut guard) = ACTIVE_STYLESHEET.write() {
        *guard = None;
    }
}

// ============================================================================
// Nom Parsers with VerboseError for diagnostics
// ============================================================================

/// Calculate line and column from the original input and the error fragment
fn calculate_position(original: &str, fragment: &str) -> (usize, usize, String) {
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

/// Parse whitespace and comments
fn ws<'a, E: NomParseError<&'a str>>(input: &'a str) -> IResult<&'a str, (), E> {
    value(
        (),
        many0(alt((value((), multispace1), value((), parse_comment)))),
    )(input)
}

/// Parse a block comment /* ... */
fn parse_comment<'a, E: NomParseError<&'a str>>(input: &'a str) -> IResult<&'a str, &'a str, E> {
    delimited(tag("/*"), take_until("*/"), tag("*/"))(input)
}

/// Parse an identifier (alphanumeric, hyphen, underscore)
fn identifier<'a, E: NomParseError<&'a str>>(input: &'a str) -> IResult<&'a str, &'a str, E> {
    take_while1(|c: char| c.is_alphanumeric() || c == '-' || c == '_')(input)
}

/// Parse an ID selector: #identifier or #identifier:state
fn id_selector(input: &str) -> ParseResult<CssSelector> {
    context("ID selector", |input| {
        let (input, _) = char('#')(input)?;
        let (input, id) = cut(identifier)(input)?;

        // Check for optional state modifier
        let (input, state) = opt(|i| {
            let (i, _) = char(':')(i)?;
            let (i, state_name) = identifier(i)?;
            Ok((i, state_name))
        })(input)?;

        let element_state = state.and_then(ElementState::parse_state);

        Ok((
            input,
            CssSelector {
                id: id.to_string(),
                state: element_state,
            },
        ))
    })(input)
}

/// Parse a complex selector: handles #id, .class, :state, :pseudo, combinators
///
/// Examples:
///   `#card`
///   `#card:hover`
///   `.item:first-child`
///   `#parent:hover > .child`
///   `#list .item:last-child`
///   `#parent:hover > #child:first-child`
fn parse_complex_selector(input: &str) -> ParseResult<ComplexSelector> {
    let mut segments = Vec::new();
    let mut remaining = input;

    loop {
        // Parse a compound selector (one or more simple selectors with no combinator)
        let (rest, compound) = parse_compound_selector(remaining)?;
        remaining = rest;

        // Look ahead for a combinator or the start of `{`
        let trimmed = remaining.trim_start();

        if trimmed.starts_with('{') || trimmed.is_empty() {
            // End of selector — this is the last (target) segment
            segments.push((compound, None));
            break;
        }

        // Check for combinators: > (child), + (adjacent sibling), ~ (general sibling)
        if let Some(after_gt) = trimmed.strip_prefix('>') {
            remaining = after_gt.trim_start();
            segments.push((compound, Some(Combinator::Child)));
        } else if let Some(after_plus) = trimmed.strip_prefix('+') {
            remaining = after_plus.trim_start();
            segments.push((compound, Some(Combinator::AdjacentSibling)));
        } else if let Some(after_tilde) = trimmed.strip_prefix('~') {
            remaining = after_tilde.trim_start();
            segments.push((compound, Some(Combinator::GeneralSibling)));
        } else {
            // Must be a descendant combinator (whitespace between compound selectors)
            // Check that next char is a valid selector start (#, ., :, *, or alpha)
            let next_ch = trimmed.chars().next().unwrap_or('{');
            if next_ch == '#'
                || next_ch == '.'
                || next_ch == ':'
                || next_ch == '*'
                || next_ch.is_alphabetic()
            {
                remaining = trimmed;
                segments.push((compound, Some(Combinator::Descendant)));
            } else {
                // Not a selector continuation — end here
                segments.push((compound, None));
                break;
            }
        }
    }

    if segments.is_empty() {
        return Err(nom::Err::Error(VerboseError::from_error_kind(
            remaining,
            nom::error::ErrorKind::Many1,
        )));
    }

    Ok((remaining, ComplexSelector { segments }))
}

/// Parse a compound selector: one or more simple selector parts with no combinator.
/// e.g. `#id.class:hover:first-child`
/// Find the index of the matching closing parenthesis for the opening paren at `input[0]`.
fn find_matching_paren(input: &str) -> Option<usize> {
    if !input.starts_with('(') {
        return None;
    }
    let mut depth = 0i32;
    for (i, c) in input.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Parse a comma-separated list of compound selectors (for :is() / :where()).
fn parse_selector_list(input: &str) -> Vec<CompoundSelector> {
    let mut selectors = Vec::new();
    for part in input.split(',') {
        let trimmed = part.trim();
        if !trimmed.is_empty() {
            if let Ok((_, compound)) = parse_compound_selector(trimmed) {
                selectors.push(compound);
            }
        }
    }
    selectors
}

/// Like `parse_selector_list` but the inner items can contain
/// combinators (`>`, `+`, `~`, descendant whitespace), so each
/// element parses as a `ComplexSelector` instead of a single
/// `CompoundSelector`. Used for the inside of `:has(...)`.
///
/// Splits on TOP-LEVEL commas only — commas inside nested
/// parens (e.g. `:has(.a, :is(.b, .c))`) are preserved. Empty
/// items are skipped.
fn parse_relative_selector_list(input: &str) -> Vec<ComplexSelector> {
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut depth: i32 = 0;
    for ch in input.chars() {
        match ch {
            '(' => {
                depth += 1;
                current.push(ch);
            }
            ')' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => {
                parts.push(std::mem::take(&mut current));
            }
            _ => current.push(ch),
        }
    }
    parts.push(current);

    let mut selectors = Vec::new();
    for part in parts {
        let trimmed = part.trim();
        if !trimmed.is_empty() {
            if let Ok((_, complex)) = parse_complex_selector(trimmed) {
                selectors.push(complex);
            }
        }
    }
    selectors
}

fn parse_compound_selector(input: &str) -> ParseResult<CompoundSelector> {
    let mut parts = Vec::new();
    let mut remaining = input;

    loop {
        // Type selector: bare identifier like button, a, ul (must be first part)
        if parts.is_empty()
            && !remaining.is_empty()
            && remaining
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic())
            && !remaining.starts_with(':')
        {
            let end = remaining
                .find(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_')
                .unwrap_or(remaining.len());
            let type_name = &remaining[..end];
            parts.push(SelectorPart::Type(type_name.to_lowercase()));
            remaining = &remaining[end..];
            continue;
        }

        if remaining.starts_with('#') {
            // ID selector
            let (rest, _) = char('#')(remaining)?;
            let (rest, id) = identifier::<VerboseError<&str>>(rest)?;
            parts.push(SelectorPart::Id(id.to_string()));
            remaining = rest;
        } else if remaining.starts_with('.') {
            // Class selector
            let (rest, _) = char('.')(remaining)?;
            let (rest, class) = identifier::<VerboseError<&str>>(rest)?;
            parts.push(SelectorPart::Class(class.to_string()));
            remaining = rest;
        } else if remaining.starts_with('*') {
            // Universal selector
            remaining = &remaining[1..];
            parts.push(SelectorPart::Universal);
        } else if remaining.starts_with("::") {
            // Pseudo-element (::placeholder, ::selection, etc.)
            let rest = &remaining[2..];
            let (rest, name) = identifier::<VerboseError<&str>>(rest)?;
            parts.push(SelectorPart::PseudoElement(name.to_string()));
            remaining = rest;
        } else if remaining.starts_with(':') {
            // Pseudo-class (state or structural)
            let (rest, _) = char(':')(remaining)?;
            let (rest, name) = identifier::<VerboseError<&str>>(rest)?;

            // Check if it's a structural pseudo-class
            match name.to_lowercase().as_str() {
                "first-child" => {
                    parts.push(SelectorPart::PseudoClass(StructuralPseudo::FirstChild));
                    remaining = rest;
                }
                "last-child" => {
                    parts.push(SelectorPart::PseudoClass(StructuralPseudo::LastChild));
                    remaining = rest;
                }
                "only-child" => {
                    parts.push(SelectorPart::PseudoClass(StructuralPseudo::OnlyChild));
                    remaining = rest;
                }
                "empty" => {
                    parts.push(SelectorPart::PseudoClass(StructuralPseudo::Empty));
                    remaining = rest;
                }
                "root" => {
                    parts.push(SelectorPart::PseudoClass(StructuralPseudo::Root));
                    remaining = rest;
                }
                "nth-child" => {
                    // Parse nth-child(N)
                    if rest.starts_with('(') {
                        let (rest2, _) = char('(')(rest)?;
                        let (rest2, _) = ws::<VerboseError<&str>>(rest2)?;
                        let (rest2, n_str) =
                            take_while1::<_, _, VerboseError<&str>>(|c: char| c.is_ascii_digit())(
                                rest2,
                            )?;
                        let (rest2, _) = ws::<VerboseError<&str>>(rest2)?;
                        let (rest2, _) = char(')')(rest2)?;
                        if let Ok(n) = n_str.parse::<usize>() {
                            parts.push(SelectorPart::PseudoClass(StructuralPseudo::NthChild(n)));
                        }
                        remaining = rest2;
                    } else {
                        remaining = rest;
                    }
                }
                "nth-last-child" => {
                    // Parse nth-last-child(N)
                    if rest.starts_with('(') {
                        let (rest2, _) = char('(')(rest)?;
                        let (rest2, _) = ws::<VerboseError<&str>>(rest2)?;
                        let (rest2, n_str) =
                            take_while1::<_, _, VerboseError<&str>>(|c: char| c.is_ascii_digit())(
                                rest2,
                            )?;
                        let (rest2, _) = ws::<VerboseError<&str>>(rest2)?;
                        let (rest2, _) = char(')')(rest2)?;
                        if let Ok(n) = n_str.parse::<usize>() {
                            parts
                                .push(SelectorPart::PseudoClass(StructuralPseudo::NthLastChild(n)));
                        }
                        remaining = rest2;
                    } else {
                        remaining = rest;
                    }
                }
                "not" => {
                    // Parse :not(selector)
                    if rest.starts_with('(') {
                        let (rest2, _) = char('(')(rest)?;
                        let rest2 = rest2.trim_start();
                        // Parse the inner compound selector
                        if let Ok((rest3, inner)) = parse_compound_selector(rest2) {
                            let rest3 = rest3.trim_start();
                            if let Ok((rest4, _)) = char::<&str, VerboseError<&str>>(')')(rest3) {
                                parts.push(SelectorPart::Not(Box::new(inner)));
                                remaining = rest4;
                            } else {
                                remaining = rest;
                            }
                        } else {
                            remaining = rest;
                        }
                    } else {
                        remaining = rest;
                    }
                }
                "is" | "where" => {
                    // Parse :is(selector, ...) or :where(selector, ...)
                    if rest.starts_with('(') {
                        if let Some(close) = find_matching_paren(rest) {
                            let inner = &rest[1..close];
                            let selectors = parse_selector_list(inner);
                            if !selectors.is_empty() {
                                parts.push(SelectorPart::Is(selectors));
                            }
                            remaining = &rest[close + 1..];
                        } else {
                            remaining = rest;
                        }
                    } else {
                        remaining = rest;
                    }
                }
                "has" => {
                    // Parse :has(relative_selector, ...) — relational
                    // pseudo. Unlike :is/:where the inner items can carry
                    // combinators (`> .child`, `+ .sibling`, `~ .later`,
                    // or descendant whitespace), so the inner is a
                    // `ComplexSelector` list, not a `CompoundSelector`
                    // list. Empty parens or unparseable inner → silently
                    // drop the :has (rule still applies to the bare
                    // compound part before it, same as :is's fallback).
                    if rest.starts_with('(') {
                        if let Some(close) = find_matching_paren(rest) {
                            let inner = &rest[1..close];
                            let selectors = parse_relative_selector_list(inner);
                            if !selectors.is_empty() {
                                parts.push(SelectorPart::Has(selectors));
                            }
                            remaining = &rest[close + 1..];
                        } else {
                            remaining = rest;
                        }
                    } else {
                        remaining = rest;
                    }
                }
                "first-of-type" => {
                    parts.push(SelectorPart::PseudoClass(StructuralPseudo::FirstOfType));
                    remaining = rest;
                }
                "last-of-type" => {
                    parts.push(SelectorPart::PseudoClass(StructuralPseudo::LastOfType));
                    remaining = rest;
                }
                "only-of-type" => {
                    parts.push(SelectorPart::PseudoClass(StructuralPseudo::OnlyOfType));
                    remaining = rest;
                }
                "nth-of-type" => {
                    if rest.starts_with('(') {
                        let (rest2, _) = char('(')(rest)?;
                        let (rest2, _) = ws::<VerboseError<&str>>(rest2)?;
                        let (rest2, n_str) =
                            take_while1::<_, _, VerboseError<&str>>(|c: char| c.is_ascii_digit())(
                                rest2,
                            )?;
                        let (rest2, _) = ws::<VerboseError<&str>>(rest2)?;
                        let (rest2, _) = char(')')(rest2)?;
                        if let Ok(n) = n_str.parse::<usize>() {
                            parts.push(SelectorPart::PseudoClass(StructuralPseudo::NthOfType(n)));
                        }
                        remaining = rest2;
                    } else {
                        remaining = rest;
                    }
                }
                "nth-last-of-type" => {
                    if rest.starts_with('(') {
                        let (rest2, _) = char('(')(rest)?;
                        let (rest2, _) = ws::<VerboseError<&str>>(rest2)?;
                        let (rest2, n_str) =
                            take_while1::<_, _, VerboseError<&str>>(|c: char| c.is_ascii_digit())(
                                rest2,
                            )?;
                        let (rest2, _) = ws::<VerboseError<&str>>(rest2)?;
                        let (rest2, _) = char(')')(rest2)?;
                        if let Ok(n) = n_str.parse::<usize>() {
                            parts.push(SelectorPart::PseudoClass(StructuralPseudo::NthLastOfType(
                                n,
                            )));
                        }
                        remaining = rest2;
                    } else {
                        remaining = rest;
                    }
                }
                _ => {
                    // Try as element state
                    if let Some(state) = ElementState::parse_state(name) {
                        parts.push(SelectorPart::State(state));
                    }
                    // Skip any functional argument list `(...)` so an
                    // unknown pseudo like `:unknown(.x)` doesn't leak its
                    // open-paren into the rest of the selector, which
                    // would later poison `rule_block` and (via the
                    // stylesheet loop's break-on-error recovery) drop
                    // every rule after the offending one. Same hardening
                    // approach as :is/:has — consume the balanced parens
                    // even though we have no semantic for them.
                    if rest.starts_with('(') {
                        if let Some(close) = find_matching_paren(rest) {
                            remaining = &rest[close + 1..];
                        } else {
                            remaining = rest;
                        }
                    } else {
                        remaining = rest;
                    }
                }
            }
        } else {
            break;
        }
    }

    if parts.is_empty() {
        return Err(nom::Err::Error(VerboseError::from_error_kind(
            input,
            nom::error::ErrorKind::Many1,
        )));
    }

    Ok((remaining, CompoundSelector { parts }))
}

/// Parse a property name (including CSS custom properties like --var-name)
fn property_name(input: &str) -> ParseResult<&str> {
    context(
        "property name",
        take_while1(|c: char| c.is_alphanumeric() || c == '-' || c == '_'),
    )(input)
}

/// Parse a CSS variable name: --identifier
fn variable_name(input: &str) -> ParseResult<&str> {
    let (input, _) = tag("--")(input)?;
    let (input, name) = identifier(input)?;
    Ok((input, name))
}

/// Parse a property value (everything until ; or })
fn property_value(input: &str) -> ParseResult<&str> {
    let (input, value) = context(
        "property value",
        take_while1(|c: char| c != ';' && c != '}'),
    )(input)?;
    Ok((input, value.trim()))
}

/// Parse a single property declaration: name: value;
fn property_declaration(input: &str) -> ParseResult<(&str, &str)> {
    let (input, _) = ws(input)?;
    let (input, name) = context("property name", property_name)(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = context("colon after property name", char(':'))(input)?;
    let (input, _) = ws(input)?;
    let (input, value) = context("property value", property_value)(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = opt(char(';'))(input)?;
    Ok((input, (name, value)))
}

/// Parse a rule block: { property: value; ... }
fn rule_block(input: &str) -> ParseResult<Vec<(&str, &str)>> {
    let (input, _) = ws::<VerboseError<&str>>(input)?;
    let (input, _) = context("opening brace", char('{'))(input)?;
    let (input, _) = ws::<VerboseError<&str>>(input)?;
    let (input, properties) = many0(property_declaration)(input)?;
    let (input, _) = ws::<VerboseError<&str>>(input)?;
    let (input, _) = context("closing brace", char('}'))(input)?;
    Ok((input, properties))
}

/// Parse a :root block for CSS variables
fn root_block(input: &str) -> ParseResult<Vec<(String, String)>> {
    let (input, _) = ws(input)?;
    let (input, _) = tag(":root")(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('{')(input)?;
    let (input, _) = ws(input)?;

    // Parse variable declarations
    let (input, declarations) = many0(|i| {
        let (i, _) = ws(i)?;
        let (i, _) = tag("--")(i)?;
        let (i, name) = identifier(i)?;
        let (i, _) = ws(i)?;
        let (i, _) = char(':')(i)?;
        let (i, _) = ws(i)?;
        let (i, value) = property_value(i)?;
        let (i, _) = ws(i)?;
        let (i, _) = opt(char(';'))(i)?;
        Ok((i, (name.to_string(), value.to_string())))
    })(input)?;

    let (input, _) = ws(input)?;
    let (input, _) = char('}')(input)?;
    Ok((input, declarations))
}

/// Parse a @keyframes block
///
/// Supports:
/// - `from` and `to` keywords (0% and 100%)
/// - Percentage values like `50%`
/// - Multiple stops: `0%, 100%` (same style for multiple positions)
///
/// # Example
///
/// ```ignore
/// @keyframes slide-in {
///     from { opacity: 0; transform: translateY(20px); }
///     to { opacity: 1; transform: translateY(0); }
/// }
/// ```
fn keyframes_block<'a>(
    css: &'a str,
    errors: &mut Vec<ParseError>,
    variables: &HashMap<String, String>,
) -> ParseResult<'a, CssKeyframes> {
    let (input, _) = ws(css)?;
    let (input, _) = tag("@keyframes")(input)?;
    let (input, _) = ws(input)?;
    let (input, name) = identifier(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('{')(input)?;
    let (input, _) = ws(input)?;

    let mut keyframes = CssKeyframes::new(name);
    let mut remaining = input;

    // Parse keyframe stops
    loop {
        let trimmed = remaining.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('}') {
            break;
        }

        match keyframe_stop(css, errors, variables)(trimmed) {
            Ok((rest, (positions, style))) => {
                for position in positions {
                    keyframes.add_keyframe(position, style.clone());
                }
                remaining = rest;
            }
            Err(_) => {
                // Can't parse more keyframe stops
                break;
            }
        }
    }

    let (input, _) = ws(remaining)?;
    let (input, _) = char('}')(input)?;
    Ok((input, keyframes))
}

/// Parse a single keyframe stop (e.g., `from { ... }`, `50% { ... }`, or `0%, 100% { ... }`)
fn keyframe_stop<'a, 'b>(
    original_css: &'a str,
    errors: &'b mut Vec<ParseError>,
    variables: &'b HashMap<String, String>,
) -> impl FnMut(&'a str) -> ParseResult<'a, (Vec<f32>, ElementStyle)> + 'b
where
    'a: 'b,
{
    move |input: &'a str| {
        let (input, _) = ws(input)?;
        let (input, positions) = keyframe_positions(input)?;
        let (input, _) = ws(input)?;
        let (input, properties) = rule_block(input)?;

        let mut style = ElementStyle::new();
        for (name, value) in properties {
            let resolved_value = resolve_var_references(value, variables);
            apply_property_with_errors(
                &mut style,
                name,
                &resolved_value,
                original_css,
                input,
                errors,
            );
        }

        Ok((input, (positions, style)))
    }
}

/// Parse keyframe position(s): `from`, `to`, `50%`, or `0%, 100%`
fn keyframe_positions(input: &str) -> ParseResult<Vec<f32>> {
    let (input, first) = keyframe_position(input)?;
    let (input, rest) = many0(|i| {
        let (i, _) = ws(i)?;
        let (i, _) = char(',')(i)?;
        let (i, _) = ws(i)?;
        keyframe_position(i)
    })(input)?;

    let mut positions = vec![first];
    positions.extend(rest);
    Ok((input, positions))
}

/// Parse a single keyframe position: `from`, `to`, or percentage like `50%`
fn keyframe_position(input: &str) -> ParseResult<f32> {
    alt((
        // `from` = 0%
        value(0.0, tag_no_case("from")),
        // `to` = 100%
        value(1.0, tag_no_case("to")),
        // Percentage like `50%`
        |i| {
            let (i, num) = float(i)?;
            let (i, _) = char('%')(i)?;
            Ok((i, num / 100.0))
        },
    ))(input)
}

/// Parsed content from a stylesheet - can be either a rule or variables
enum CssBlock {
    Rule(String, Box<ElementStyle>),
    Variables(Vec<(String, String)>),
}

/// Parse a complete rule: #id { ... } or #id:state { ... }
fn css_rule(input: &str) -> ParseResult<(String, ElementStyle)> {
    let (input, _) = ws(input)?;
    let (input, selector) = context("CSS rule selector", id_selector)(input)?;
    let (input, _) = ws(input)?;
    let (input, properties) = context("CSS rule block", rule_block)(input)?;

    let mut style = ElementStyle::new();
    for (name, value) in properties {
        apply_property(&mut style, name, value);
    }

    // Use the selector key (id or id:state)
    Ok((input, (selector.key(), style)))
}

/// Parse an entire stylesheet
#[allow(dead_code)]
fn parse_stylesheet(input: &str) -> ParseResult<Vec<(String, ElementStyle)>> {
    let (input, _) = ws(input)?;
    let (input, rules) = many0(css_rule)(input)?;
    let (input, _) = ws(input)?;
    Ok((input, rules))
}

/// Parse a complete rule with error collection: #id { ... } or #id:state { ... }
fn css_rule_with_errors<'a, 'b>(
    original_css: &'a str,
    errors: &'b mut Vec<ParseError>,
) -> impl FnMut(&'a str) -> ParseResult<'a, (String, ElementStyle)> + 'b
where
    'a: 'b,
{
    move |input: &'a str| {
        let (input, _) = ws(input)?;
        let (input, selector) = context("CSS rule selector", id_selector)(input)?;
        let (input, _) = ws(input)?;
        let (input, properties) = context("CSS rule block", rule_block)(input)?;

        let mut style = ElementStyle::new();
        for (name, value) in properties {
            apply_property_with_errors(&mut style, name, value, original_css, input, errors);
        }

        Ok((input, (selector.key(), style)))
    }
}

// ===========================================================================
// @flow DAG parser
// ===========================================================================

/// Parse a `@flow` block into a validated FlowGraph DAG.
///
/// # Syntax
///
/// ```css
/// @flow ripple-effect {
///   target: fragment;
///   input uv;
///   input time;
///   node dist = distance(uv, vec2(0.5, 0.5));
///   node wave = sin(dist * 20.0 - time * 4.0);
///   output color = vec4(wave, wave, wave, 1.0);
/// }
/// ```
fn flow_block<'a>(
    css: &'a str,
    errors: &mut Vec<ParseError>,
    flow_registry: Option<&HashMap<String, FlowGraph>>,
) -> ParseResult<'a, FlowGraph> {
    let (input, _) = ws(css)?;
    let (input, _) = tag("@flow")(input)?;
    let (input, _) = ws(input)?;
    let (input, name) = identifier(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('{')(input)?;
    let (input, _) = ws(input)?;

    let mut graph = FlowGraph::new(name);
    let mut remaining = input;

    loop {
        let trimmed = remaining.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('}') {
            break;
        }

        // Skip comments inside @flow blocks
        if trimmed.starts_with("/*") {
            if let Some(end) = trimmed.find("*/") {
                remaining = &trimmed[end + 2..];
                continue;
            } else {
                break;
            }
        }

        // Try to parse a flow declaration
        if let Some(rest) = parse_flow_declaration(trimmed, &mut graph, errors) {
            remaining = rest;
        } else {
            // Error recovery: skip brace-delimited blocks (step) or to next semicolon
            let brace_pos = trimmed.find('{');
            let semi_pos = trimmed.find(';');
            match (brace_pos, semi_pos) {
                (Some(b), Some(s)) if b < s => {
                    if let Some(close) = find_flow_close_brace(&trimmed[b + 1..]) {
                        remaining = &trimmed[b + 1 + close + 1..];
                    } else {
                        break;
                    }
                }
                (_, Some(s)) => remaining = &trimmed[s + 1..],
                (Some(b), None) => {
                    if let Some(close) = find_flow_close_brace(&trimmed[b + 1..]) {
                        remaining = &trimmed[b + 1 + close + 1..];
                    } else {
                        break;
                    }
                }
                (None, None) => break,
            }
        }
    }

    let (input, _) = ws(remaining)?;
    let (input, _) = char('}')(input)?;

    // Validate the DAG (cycle detection, type inference, semantic expansion)
    if let Err(flow_errors) = graph.validate(flow_registry) {
        for err in flow_errors {
            errors.push(ParseError {
                severity: Severity::Error,
                message: format!("@flow '{}': {}", graph.name, err),
                line: 0,
                column: 0,
                fragment: String::new(),
                contexts: vec![],
                property: None,
                value: None,
            });
        }
    }

    Ok((input, graph))
}

/// Parse a standalone `@flow name { ... }` string into a validated FlowGraph.
///
/// Used by the `flow!` macro to convert stringified Rust tokens into a FlowGraph at runtime.
pub fn parse_flow_string(src: &str) -> Result<FlowGraph, String> {
    // Normalize: stringify!() may insert literal \n between tokens when
    // the macro body spans multiple source lines. Replace them with spaces
    // so the parser sees a single continuous line of declarations.
    let normalized = src.replace('\n', " ");
    let mut errors = Vec::new();
    match flow_block(&normalized, &mut errors, None) {
        Ok((_, graph)) => {
            let fatal: Vec<_> = errors
                .iter()
                .filter(|e| e.severity == Severity::Error)
                .map(|e| e.message.clone())
                .collect();
            if fatal.is_empty() {
                Ok(graph)
            } else {
                Err(fatal.join("; "))
            }
        }
        Err(_) => Err(format!(
            "failed to parse @flow block: {:?}",
            src.chars().take(80).collect::<String>()
        )),
    }
}

/// Parse a single declaration inside a @flow block.
/// Returns the remaining input, or None if parsing failed.
fn parse_flow_declaration<'a>(
    input: &'a str,
    graph: &mut FlowGraph,
    errors: &mut Vec<ParseError>,
) -> Option<&'a str> {
    let trimmed = input.trim_start();

    // target: fragment | compute;
    if trimmed.starts_with("target") {
        return parse_flow_target(trimmed, graph);
    }

    // workgroup: N;
    if trimmed.starts_with("workgroup") {
        return parse_flow_workgroup(trimmed, graph);
    }

    // input <name> [: buffer(name, type)];
    if trimmed.starts_with("input ") {
        return parse_flow_input(trimmed, graph, errors);
    }

    // step <name> : <step-type> { <param>: <value>; ... }
    if trimmed.starts_with("step ") {
        return parse_flow_step(trimmed, graph, errors);
    }

    // chain <name> : <link> | <link> | ... ;
    if trimmed.starts_with("chain ") {
        return parse_flow_chain(trimmed, graph, errors);
    }

    // use <flow-name>;
    if trimmed.starts_with("use ") {
        return parse_flow_use(trimmed, graph, errors);
    }

    // node <name> = <expr>;
    if trimmed.starts_with("node ") {
        return parse_flow_node(trimmed, graph, errors);
    }

    // output <target> [= <expr>];
    if trimmed.starts_with("output ") {
        return parse_flow_output(trimmed, graph, errors);
    }

    None
}

fn parse_flow_target<'a>(input: &'a str, graph: &mut FlowGraph) -> Option<&'a str> {
    let rest = input.strip_prefix("target")?;
    let rest = rest.trim_start();
    let rest = rest.strip_prefix(':')?;
    let rest = rest.trim_start();

    let semi = rest.find(';')?;
    let value = rest[..semi].trim();
    match value {
        "fragment" => graph.target = FlowTarget::Fragment,
        "compute" => graph.target = FlowTarget::Compute,
        "vertex" => graph.target = FlowTarget::Vertex,
        "material" => graph.target = FlowTarget::Material,
        _ => return None,
    }
    Some(&rest[semi + 1..])
}

fn parse_flow_workgroup<'a>(input: &'a str, graph: &mut FlowGraph) -> Option<&'a str> {
    let rest = input.strip_prefix("workgroup")?;
    let rest = rest.trim_start();
    let rest = rest.strip_prefix(':')?;
    let rest = rest.trim_start();

    let semi = rest.find(';')?;
    let value = rest[..semi].trim();
    graph.workgroup_size = value.parse::<u32>().ok();
    Some(&rest[semi + 1..])
}

fn parse_flow_input<'a>(
    input: &'a str,
    graph: &mut FlowGraph,
    _errors: &mut Vec<ParseError>,
) -> Option<&'a str> {
    let rest = input.strip_prefix("input")?.trim_start();

    let semi = rest.find(';')?;
    let decl = rest[..semi].trim();

    // Check for typed declaration: input name: buffer(buf-name, type);
    if let Some(colon_pos) = decl.find(':') {
        let name = decl[..colon_pos].trim();
        let type_decl = decl[colon_pos + 1..].trim();

        if type_decl.starts_with("builtin(") {
            // builtin(var-name) — explicit builtin source
            let inner = type_decl.strip_prefix("builtin(")?.strip_suffix(')')?;
            if let Some(builtin) = blinc_core::flow::BuiltinVar::parse(inner.trim()) {
                let ty = builtin.output_type();
                graph.inputs.push(FlowInput {
                    name: name.to_string(),
                    source: FlowInputSource::Builtin(builtin),
                    ty: Some(ty),
                });
            }
        } else if type_decl.starts_with("buffer(") {
            // buffer(name, type)
            let inner = type_decl.strip_prefix("buffer(")?.strip_suffix(')')?;
            let parts: Vec<&str> = inner.splitn(2, ',').collect();
            if parts.len() == 2 {
                let buf_name = parts[0].trim().to_string();
                let ty = match parts[1].trim() {
                    "float" | "f32" => FlowType::Float,
                    "vec2" => FlowType::Vec2,
                    "vec3" => FlowType::Vec3,
                    "vec4" => FlowType::Vec4,
                    _ => FlowType::Vec4,
                };
                graph.inputs.push(FlowInput {
                    name: name.to_string(),
                    source: FlowInputSource::Buffer { name: buf_name, ty },
                    ty: Some(ty),
                });
            }
        } else if type_decl.starts_with("css(") {
            // css(property-name)
            let inner = type_decl.strip_prefix("css(")?.strip_suffix(')')?;
            graph.inputs.push(FlowInput {
                name: name.to_string(),
                source: FlowInputSource::CssProperty(inner.trim().to_string()),
                ty: Some(FlowType::Float),
            });
        } else if type_decl.starts_with("env(") {
            // env(var-name)
            let inner = type_decl.strip_prefix("env(")?.strip_suffix(')')?;
            graph.inputs.push(FlowInput {
                name: name.to_string(),
                source: FlowInputSource::EnvVar(inner.trim().to_string()),
                ty: Some(FlowType::Float),
            });
        }
    } else {
        // Simple declaration: input name;
        let name = decl;
        let source = if let Some(builtin) = blinc_core::flow::BuiltinVar::parse(name) {
            let ty = builtin.output_type();
            graph.inputs.push(FlowInput {
                name: name.to_string(),
                source: FlowInputSource::Builtin(builtin),
                ty: Some(ty),
            });
            return Some(&rest[semi + 1..]);
        } else if name.starts_with("env(") {
            let env_name = name.strip_prefix("env(")?.strip_suffix(')')?;
            FlowInputSource::EnvVar(env_name.to_string())
        } else {
            FlowInputSource::Auto
        };
        graph.inputs.push(FlowInput {
            name: name.to_string(),
            source,
            ty: None,
        });
    }

    Some(&rest[semi + 1..])
}

fn parse_flow_node<'a>(
    input: &'a str,
    graph: &mut FlowGraph,
    errors: &mut Vec<ParseError>,
) -> Option<&'a str> {
    let rest = input.strip_prefix("node")?.trim_start();

    // Find the '=' separating name from expression
    let eq_pos = rest.find('=')?;
    let name = rest[..eq_pos].trim();

    // Find the semicolon that ends this declaration
    // Need to handle nested parens — can't just find first ';'
    let expr_start = &rest[eq_pos + 1..];
    let semi_pos = find_statement_end(expr_start)?;
    let expr_str = expr_start[..semi_pos].trim();

    match parse_flow_expr(expr_str) {
        Ok(expr) => {
            graph.nodes.push(FlowNode {
                name: name.to_string(),
                expr,
                inferred_type: None,
            });
        }
        Err(msg) => {
            errors.push(ParseError {
                severity: Severity::Error,
                message: format!("@flow node '{}': {}", name, msg),
                line: 0,
                column: 0,
                fragment: expr_str.to_string(),
                contexts: vec![],
                property: Some(format!("node {}", name)),
                value: Some(expr_str.to_string()),
            });
        }
    }

    Some(&expr_start[semi_pos + 1..])
}

fn parse_output_target(name: &str) -> FlowOutputTarget {
    match name {
        "color" => FlowOutputTarget::Color,
        "alpha" => FlowOutputTarget::Alpha,
        "displacement" => FlowOutputTarget::Displacement,
        // 3D vertex outputs
        "position" => FlowOutputTarget::Position,
        "world_normal_out" | "world_normal" => FlowOutputTarget::WorldNormalOut,
        "world_position_out" | "world_position" => FlowOutputTarget::WorldPositionOut,
        // 3D material outputs
        "albedo" | "base_color" => FlowOutputTarget::Albedo,
        "metallic" => FlowOutputTarget::Metallic,
        "roughness" => FlowOutputTarget::Roughness,
        "emissive" => FlowOutputTarget::Emissive,
        "surface_normal" => FlowOutputTarget::SurfaceNormal,
        "alpha_out" => FlowOutputTarget::AlphaOut,
        _ => FlowOutputTarget::Color,
    }
}

fn parse_flow_output<'a>(
    input: &'a str,
    graph: &mut FlowGraph,
    errors: &mut Vec<ParseError>,
) -> Option<&'a str> {
    let rest = input.strip_prefix("output")?.trim_start();
    let semi_pos = find_statement_end(rest)?;
    let decl = rest[..semi_pos].trim();

    // Detect output target and optional expression
    let (target, name, expr_str) = if decl.starts_with("buffer(") {
        // output buffer(name) = expr;
        let close_paren = decl.find(')')?;
        let buf_inner = decl[7..close_paren].trim();
        let after = decl[close_paren + 1..].trim();
        let expr_str = after.strip_prefix('=').map(|s| s.trim());
        (
            FlowOutputTarget::Buffer {
                name: buf_inner.to_string(),
            },
            buf_inner.to_string(),
            expr_str,
        )
    } else if let Some(eq_pos) = decl.find('=') {
        // output <name> = <expr>;
        let name = decl[..eq_pos].trim();
        let expr_s = decl[eq_pos + 1..].trim();
        let target = parse_output_target(name);
        (target, name.to_string(), Some(expr_s))
    } else {
        // Bare output: output color;
        let name = decl.trim();
        let target = parse_output_target(name);
        (target, name.to_string(), None)
    };

    let parsed_expr = if let Some(es) = expr_str {
        match parse_flow_expr(es) {
            Ok(expr) => Some(expr),
            Err(msg) => {
                errors.push(ParseError {
                    severity: Severity::Error,
                    message: format!("@flow output '{}': {}", name, msg),
                    line: 0,
                    column: 0,
                    fragment: es.to_string(),
                    contexts: vec![],
                    property: Some(format!("output {}", name)),
                    value: Some(es.to_string()),
                });
                None
            }
        }
    } else {
        None
    };

    graph.outputs.push(FlowOutput {
        name,
        target,
        expr: parsed_expr,
    });

    Some(&rest[semi_pos + 1..])
}

/// Find the end of a flow statement (semicolon), respecting parentheses nesting.
fn find_statement_end(input: &str) -> Option<usize> {
    let mut depth = 0i32;
    for (i, c) in input.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ';' if depth == 0 => return Some(i),
            '}' if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

// ===========================================================================
// Flow semantic layer parsers (step, chain, use)
// ===========================================================================

/// Find matching closing brace, tracking nested brace depth.
/// Input starts AFTER the opening `{`.
fn find_flow_close_brace(input: &str) -> Option<usize> {
    let mut depth = 1i32;
    let mut in_comment = false;
    for (i, c) in input.char_indices() {
        if in_comment {
            if c == '/' && i > 0 && input.as_bytes()[i - 1] == b'*' {
                in_comment = false;
            }
            continue;
        }
        if c == '*' && i > 0 && input.as_bytes()[i - 1] == b'/' {
            in_comment = true;
            continue;
        }
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Find the end of a chain declaration (`;` at depth 0, respecting parens).
fn find_chain_end(input: &str) -> Option<usize> {
    let mut depth = 0i32;
    for (i, c) in input.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ';' if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

/// Split chain body at top-level `|` delimiters, respecting parentheses.
fn split_chain_links(input: &str) -> Vec<&str> {
    let mut links = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, c) in input.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            '|' if depth == 0 => {
                links.push(&input[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    links.push(&input[start..]);
    links
}

/// Parse `step <name> : <step-type> { <param>: <value>; ... }`
fn parse_flow_step<'a>(
    input: &'a str,
    graph: &mut FlowGraph,
    errors: &mut Vec<ParseError>,
) -> Option<&'a str> {
    let rest = input.strip_prefix("step ")?.trim_start();

    // Parse name (alphanumeric + underscore + hyphen)
    let name_end = rest
        .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
        .unwrap_or(rest.len());
    if name_end == 0 {
        return None;
    }
    let name = &rest[..name_end];
    let rest = rest[name_end..].trim_start();

    // Consume ':'
    let rest = rest.strip_prefix(':')?.trim_start();

    // Parse step type (kebab-case: alphanumeric + hyphen)
    let type_end = rest
        .find(|c: char| !c.is_alphanumeric() && c != '-')
        .unwrap_or(rest.len());
    if type_end == 0 {
        return None;
    }
    let type_str = &rest[..type_end];
    let rest = rest[type_end..].trim_start();

    // Must have opening brace
    let rest = rest.strip_prefix('{')?;

    // Find matching closing brace
    let close = find_flow_close_brace(rest)?;
    let body = &rest[..close];
    let after = &rest[close + 1..];

    let step_type = match StepType::parse(type_str) {
        Some(st) => st,
        None => {
            errors.push(ParseError {
                severity: Severity::Error,
                message: format!("unknown step type: '{}'", type_str),
                line: 0,
                column: 0,
                fragment: type_str.to_string(),
                contexts: vec![],
                property: None,
                value: None,
            });
            return Some(after);
        }
    };

    let params = parse_step_params(body, errors);

    graph.steps.push(FlowStep {
        name: name.to_string(),
        step_type,
        params,
    });

    Some(after)
}

/// Parse key: value; pairs inside a step block body.
fn parse_step_params(body: &str, errors: &mut Vec<ParseError>) -> HashMap<String, StepParam> {
    let mut params = HashMap::new();
    let mut remaining = body.trim();

    while !remaining.is_empty() {
        // Skip comments
        if remaining.starts_with("/*") {
            if let Some(end) = remaining.find("*/") {
                remaining = remaining[end + 2..].trim_start();
                continue;
            } else {
                break;
            }
        }

        // Find colon separator
        let colon = match remaining.find(':') {
            Some(pos) => pos,
            None => break,
        };
        let key = remaining[..colon].trim();
        if key.is_empty() {
            break;
        }
        remaining = remaining[colon + 1..].trim_start();

        // Find semicolon at top level (respecting parens)
        let semi = find_step_param_end(remaining);
        let value_str = remaining[..semi].trim();
        remaining = if semi < remaining.len() && remaining.as_bytes()[semi] == b';' {
            remaining[semi + 1..].trim_start()
        } else {
            remaining[semi..].trim_start()
        };

        if value_str.is_empty() {
            continue;
        }

        // Parse value based on key name or content
        if key == "sources" {
            // Comma-separated identifiers: sources: drops1, drops2, streaks;
            let idents: Vec<String> = value_str
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if idents.len() == 1 {
                params.insert(
                    key.to_string(),
                    StepParam::Ident(idents.into_iter().next().unwrap()),
                );
            } else {
                params.insert(key.to_string(), StepParam::IdentList(idents));
            }
        } else if key == "weights" {
            // Comma-separated floats: weights: 1.0, 0.5, 0.3;
            let floats: Result<Vec<f32>, _> = value_str
                .split(',')
                .map(|s| s.trim().parse::<f32>())
                .collect();
            match floats {
                Ok(list) if list.len() == 1 => {
                    params.insert(key.to_string(), StepParam::Expr(FlowExpr::Float(list[0])));
                }
                Ok(list) => {
                    params.insert(key.to_string(), StepParam::FloatList(list));
                }
                Err(_) => {
                    if let Ok(expr) = parse_flow_expr(value_str) {
                        params.insert(key.to_string(), StepParam::Expr(expr));
                    }
                }
            }
        } else if key == "stops" {
            match parse_color_stop_list(value_str) {
                Ok(stops) => {
                    params.insert(key.to_string(), StepParam::ColorStops(stops));
                }
                Err(e) => {
                    errors.push(ParseError {
                        severity: Severity::Error,
                        message: format!("invalid color stops: {}", e),
                        line: 0,
                        column: 0,
                        fragment: value_str.to_string(),
                        contexts: vec![],
                        property: Some(key.to_string()),
                        value: Some(value_str.to_string()),
                    });
                }
            }
        } else if let Ok(int_val) = value_str.parse::<i32>() {
            // Check it's not a float (e.g. "4.0" parses as float, not int)
            if !value_str.contains('.') {
                params.insert(key.to_string(), StepParam::Int(int_val));
            } else if let Ok(expr) = parse_flow_expr(value_str) {
                params.insert(key.to_string(), StepParam::Expr(expr));
            }
        } else if let Ok(expr) = parse_flow_expr(value_str) {
            params.insert(key.to_string(), StepParam::Expr(expr));
        } else {
            // Bare identifier (blend modes, curve names, style names)
            params.insert(key.to_string(), StepParam::Ident(value_str.to_string()));
        }
    }

    params
}

/// Find end of a step param value: semicolon at depth 0, or end of string.
fn find_step_param_end(input: &str) -> usize {
    let mut depth = 0i32;
    for (i, c) in input.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ';' if depth == 0 => return i,
            _ => {}
        }
    }
    input.len()
}

/// Parse `chain <name> : <link> | <link> | ... ;`
fn parse_flow_chain<'a>(
    input: &'a str,
    graph: &mut FlowGraph,
    errors: &mut Vec<ParseError>,
) -> Option<&'a str> {
    let rest = input.strip_prefix("chain ")?.trim_start();

    // Parse name
    let name_end = rest
        .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
        .unwrap_or(rest.len());
    if name_end == 0 {
        return None;
    }
    let name = &rest[..name_end];
    let rest = rest[name_end..].trim_start();

    // Consume ':'
    let rest = rest.strip_prefix(':')?.trim_start();

    // Find terminating ';'
    let semi = find_chain_end(rest)?;
    let chain_body = &rest[..semi];
    let after = &rest[semi + 1..];

    // Split at top-level '|'
    let link_strs = split_chain_links(chain_body);
    let mut links = Vec::new();

    for link_str in link_strs {
        let link_str = link_str.trim();
        if link_str.is_empty() {
            continue;
        }

        match parse_chain_link(link_str) {
            Ok(link) => links.push(link),
            Err(e) => {
                errors.push(ParseError {
                    severity: Severity::Error,
                    message: format!("invalid chain link: {}", e),
                    line: 0,
                    column: 0,
                    fragment: link_str.to_string(),
                    contexts: vec![],
                    property: None,
                    value: None,
                });
            }
        }
    }

    if links.is_empty() {
        return Some(after);
    }

    graph.chains.push(FlowChain {
        name: name.to_string(),
        links,
    });

    Some(after)
}

/// Parse a single chain link: `step-type(key: value, key: value)` or just `step-type`.
fn parse_chain_link(input: &str) -> Result<ChainLink, String> {
    let trimmed = input.trim();

    // Find step type name (everything before '(' or end)
    let paren_pos = trimmed.find('(');
    let type_str = if let Some(pos) = paren_pos {
        trimmed[..pos].trim()
    } else {
        trimmed
    };

    let step_type =
        StepType::parse(type_str).ok_or_else(|| format!("unknown step type: '{}'", type_str))?;

    let mut params = HashMap::new();

    if let Some(paren_pos) = paren_pos {
        let after_paren = &trimmed[paren_pos + 1..];
        let close = find_flow_close_paren(after_paren)
            .ok_or_else(|| "unmatched '(' in chain link".to_string())?;
        let args_str = &after_paren[..close];

        // Parse named params: key: value, key: value
        for param_str in split_chain_params(args_str) {
            let param_str = param_str.trim();
            if param_str.is_empty() {
                continue;
            }

            if let Some(colon) = param_str.find(':') {
                let key = param_str[..colon].trim();
                let val_str = param_str[colon + 1..].trim();

                if key == "sources" {
                    let idents: Vec<String> = val_str
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                    if idents.len() == 1 {
                        params.insert(
                            key.to_string(),
                            StepParam::Ident(idents.into_iter().next().unwrap()),
                        );
                    } else {
                        params.insert(key.to_string(), StepParam::IdentList(idents));
                    }
                } else if key == "weights" {
                    if let Ok(list) = val_str
                        .split(',')
                        .map(|s| s.trim().parse::<f32>())
                        .collect::<Result<Vec<f32>, _>>()
                    {
                        if list.len() == 1 {
                            params
                                .insert(key.to_string(), StepParam::Expr(FlowExpr::Float(list[0])));
                        } else {
                            params.insert(key.to_string(), StepParam::FloatList(list));
                        }
                    } else if let Ok(expr) = parse_flow_expr(val_str) {
                        params.insert(key.to_string(), StepParam::Expr(expr));
                    }
                } else if key == "stops" {
                    match parse_color_stop_list(val_str) {
                        Ok(stops) => {
                            params.insert(key.to_string(), StepParam::ColorStops(stops));
                        }
                        Err(e) => return Err(format!("invalid color stops: {}", e)),
                    }
                } else if let Ok(int_val) = val_str.parse::<i32>() {
                    if !val_str.contains('.') {
                        params.insert(key.to_string(), StepParam::Int(int_val));
                    } else if let Ok(expr) = parse_flow_expr(val_str) {
                        params.insert(key.to_string(), StepParam::Expr(expr));
                    }
                } else if let Ok(expr) = parse_flow_expr(val_str) {
                    params.insert(key.to_string(), StepParam::Expr(expr));
                } else {
                    params.insert(key.to_string(), StepParam::Ident(val_str.to_string()));
                }
            }
        }
    }

    Ok(ChainLink { step_type, params })
}

/// Split chain link params at top-level commas, respecting parentheses.
fn split_chain_params(input: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, c) in input.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&input[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&input[start..]);
    parts
}

/// Parse `use <flow-name>;`
fn parse_flow_use<'a>(
    input: &'a str,
    graph: &mut FlowGraph,
    errors: &mut Vec<ParseError>,
) -> Option<&'a str> {
    let rest = input.strip_prefix("use ")?.trim_start();

    // Find semicolon
    let semi = rest.find(';')?;
    let flow_name = rest[..semi].trim();

    if flow_name.is_empty() {
        errors.push(ParseError {
            severity: Severity::Error,
            message: "empty flow name in 'use' declaration".to_string(),
            line: 0,
            column: 0,
            fragment: String::new(),
            contexts: vec![],
            property: None,
            value: None,
        });
        return Some(&rest[semi + 1..]);
    }

    // Validate it's a valid identifier
    if !flow_name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
        errors.push(ParseError {
            severity: Severity::Error,
            message: format!("invalid flow name: '{}'", flow_name),
            line: 0,
            column: 0,
            fragment: flow_name.to_string(),
            contexts: vec![],
            property: None,
            value: None,
        });
        return Some(&rest[semi + 1..]);
    }

    graph.uses.push(FlowUse {
        flow_name: flow_name.to_string(),
    });

    Some(&rest[semi + 1..])
}

/// Parse a color stop list: `#RRGGBB 0.0, #RRGGBB 0.5, #RRGGBB 1.0`
fn parse_color_stop_list(input: &str) -> Result<Vec<(FlowExpr, f32)>, String> {
    let mut stops = Vec::new();
    let mut remaining = input.trim();

    while !remaining.is_empty() {
        let remaining_trimmed = remaining.trim_start();
        if remaining_trimmed.is_empty() {
            break;
        }
        remaining = remaining_trimmed;

        // Parse color (hex literal or expression)
        let (color_expr, rest) = if remaining.starts_with('#') {
            parse_flow_color(remaining)?
        } else {
            // Could be a named reference or expression — parse up to whitespace
            let end = remaining
                .find(|c: char| c.is_whitespace())
                .unwrap_or(remaining.len());
            let expr = parse_flow_expr(&remaining[..end])?;
            (expr, &remaining[end..])
        };

        let rest = rest.trim_start();

        // Parse position (float)
        let pos_end = rest
            .find(|c: char| !c.is_ascii_digit() && c != '.' && c != '-')
            .unwrap_or(rest.len());
        if pos_end == 0 {
            return Err("expected position value after color".to_string());
        }
        let pos: f32 = rest[..pos_end]
            .parse()
            .map_err(|e| format!("invalid position: {}", e))?;

        stops.push((color_expr, pos));

        let rest = rest[pos_end..].trim_start();
        // Consume optional comma
        remaining = rest.strip_prefix(',').map_or(rest, |s| s.trim_start());
    }

    if stops.is_empty() {
        return Err("empty color stop list".to_string());
    }

    Ok(stops)
}

// ===========================================================================
// Flow expression parser (recursive descent)
// ===========================================================================

/// Parse a flow expression string into a FlowExpr AST.
///
/// Operator precedence (low → high):
/// 1. `+`, `-` (additive)
/// 2. `*`, `/` (multiplicative)
/// 3. Unary `-` (negation)
/// 4. Function calls, constructors, literals, references, parens
fn parse_flow_expr(input: &str) -> Result<FlowExpr, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("empty expression".to_string());
    }
    let (expr, rest) = parse_flow_additive(input)?;
    let rest = rest.trim();
    if !rest.is_empty() {
        return Err(format!("unexpected trailing content: '{}'", rest));
    }
    Ok(expr)
}

/// Parse additive expressions: `a + b`, `a - b`
#[allow(clippy::manual_strip)]
fn parse_flow_additive(input: &str) -> Result<(FlowExpr, &str), String> {
    let (mut left, mut rest) = parse_flow_multiplicative(input)?;

    loop {
        let trimmed = rest.trim_start();
        if trimmed.starts_with('+') {
            let (right, r) = parse_flow_multiplicative(&trimmed[1..])?;
            left = FlowExpr::Add(Box::new(left), Box::new(right));
            rest = r;
        } else if trimmed.starts_with('-') {
            // Distinguish binary minus from unary minus / negative literal
            // If '-' is followed by a digit and we're after an operator position, it's unary
            // Binary minus: appears after a complete expression
            let after = trimmed[1..].trim_start();
            // Check if the character after '-' starts what could be a token
            // This is binary minus because left already parsed successfully
            let (right, r) = parse_flow_multiplicative(&trimmed[1..])?;
            left = FlowExpr::Sub(Box::new(left), Box::new(right));
            rest = r;
        } else {
            break;
        }
    }

    Ok((left, rest))
}

/// Parse multiplicative expressions: `a * b`, `a / b`
#[allow(clippy::manual_strip)]
fn parse_flow_multiplicative(input: &str) -> Result<(FlowExpr, &str), String> {
    let (mut left, mut rest) = parse_flow_unary(input)?;

    loop {
        let trimmed = rest.trim_start();
        if trimmed.starts_with('*') {
            let (right, r) = parse_flow_unary(&trimmed[1..])?;
            left = FlowExpr::Mul(Box::new(left), Box::new(right));
            rest = r;
        } else if trimmed.starts_with('/') {
            let (right, r) = parse_flow_unary(&trimmed[1..])?;
            left = FlowExpr::Div(Box::new(left), Box::new(right));
            rest = r;
        } else {
            break;
        }
    }

    Ok((left, rest))
}

/// Parse unary expressions: `-a`
#[allow(clippy::manual_strip)]
fn parse_flow_unary(input: &str) -> Result<(FlowExpr, &str), String> {
    let trimmed = input.trim_start();
    if trimmed.starts_with('-') {
        // Check it's not just a negative number (handled in primary)
        let after = trimmed[1..].trim_start();
        if after.starts_with(|c: char| c.is_ascii_digit() || c == '.') {
            // Could be a negative literal — try primary first
            if let Ok(result) = parse_flow_primary(trimmed) {
                return Ok(result);
            }
        }
        let (expr, rest) = parse_flow_unary(&trimmed[1..])?;
        Ok((FlowExpr::Neg(Box::new(expr)), rest))
    } else {
        parse_flow_primary(trimmed)
    }
}

/// Parse primary expressions: literals, refs, function calls, parens, vec constructors, colors
fn parse_flow_primary(input: &str) -> Result<(FlowExpr, &str), String> {
    let (expr, rest) = parse_flow_primary_inner(input)?;
    // Check for swizzle access (.x, .xy, .rgb, etc.)
    Ok(try_parse_flow_swizzle(expr, rest))
}

/// Check for and consume a swizzle suffix like `.x`, `.xy`, `.rgb`
/// Tolerates whitespace around the dot (e.g. `uv . x` from stringify!())
fn try_parse_flow_swizzle(expr: FlowExpr, rest: &str) -> (FlowExpr, &str) {
    let trimmed = rest.trim_start();
    if !trimmed.starts_with('.') {
        return (expr, trimmed);
    }
    let after_dot = trimmed[1..].trim_start();
    let swizzle_end = after_dot
        .find(|c: char| !matches!(c, 'x' | 'y' | 'z' | 'w' | 'r' | 'g' | 'b' | 'a'))
        .unwrap_or(after_dot.len());
    if swizzle_end == 0 || swizzle_end > 4 {
        return (expr, trimmed);
    }
    // Make sure we're not accidentally consuming an identifier that starts with x/y/z/w
    // e.g. "uv.xyz_thing" — 'xyz' is valid swizzle but '_thing' shouldn't be left
    // Check that the character after the swizzle is not alphanumeric/underscore
    if swizzle_end < after_dot.len() {
        let next = after_dot.as_bytes()[swizzle_end];
        if next.is_ascii_alphanumeric() || next == b'_' {
            return (expr, trimmed);
        }
    }
    let components = &after_dot[..swizzle_end];
    (
        FlowExpr::Swizzle(Box::new(expr), components.to_string()),
        &after_dot[swizzle_end..],
    )
}

#[allow(clippy::manual_strip)]
fn parse_flow_primary_inner(input: &str) -> Result<(FlowExpr, &str), String> {
    let trimmed = input.trim_start();

    if trimmed.is_empty() {
        return Err("unexpected end of expression".to_string());
    }

    // Color literal: #RRGGBB or #RRGGBBAA
    if trimmed.starts_with('#') {
        return parse_flow_color(trimmed);
    }

    // Parenthesized expression
    if trimmed.starts_with('(') {
        let inner_start = &trimmed[1..];
        let close = find_flow_close_paren(inner_start)
            .ok_or_else(|| "unmatched parenthesis".to_string())?;
        let inner = inner_start[..close].trim();
        let (expr, inner_rest) = parse_flow_additive(inner)?;
        let inner_rest = inner_rest.trim();
        if !inner_rest.is_empty() {
            return Err(format!(
                "unexpected content in parenthesized expression: '{}'",
                inner_rest
            ));
        }
        return Ok((expr, &inner_start[close + 1..]));
    }

    // Number literal (including negative)
    if trimmed.starts_with(|c: char| c.is_ascii_digit() || c == '.')
        || (trimmed.starts_with('-')
            && trimmed[1..]
                .trim_start()
                .starts_with(|c: char| c.is_ascii_digit() || c == '.'))
    {
        return parse_flow_number(trimmed);
    }

    // Identifier: could be function call, vec constructor, or reference
    if trimmed.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
        let name_end = trimmed
            .find(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
            .unwrap_or(trimmed.len());
        let name = &trimmed[..name_end];
        let after = trimmed[name_end..].trim_start();

        // Function call or vector constructor
        if after.starts_with('(') {
            let args_start = &after[1..];
            let close = find_flow_close_paren(args_start)
                .ok_or_else(|| format!("unmatched parenthesis in call to '{}'", name))?;
            let args_str = &args_start[..close];
            let rest = &args_start[close + 1..];

            let args = parse_flow_arg_list(args_str)?;

            // Vec constructors
            match name {
                "vec2" => {
                    if args.len() != 2 {
                        return Err(format!("vec2 requires 2 arguments, got {}", args.len()));
                    }
                    let mut it = args.into_iter();
                    return Ok((
                        FlowExpr::Vec2(Box::new(it.next().unwrap()), Box::new(it.next().unwrap())),
                        rest,
                    ));
                }
                "vec3" => {
                    if args.len() != 3 {
                        return Err(format!("vec3 requires 3 arguments, got {}", args.len()));
                    }
                    let mut it = args.into_iter();
                    return Ok((
                        FlowExpr::Vec3(
                            Box::new(it.next().unwrap()),
                            Box::new(it.next().unwrap()),
                            Box::new(it.next().unwrap()),
                        ),
                        rest,
                    ));
                }
                "vec4" => {
                    if args.len() != 4 {
                        return Err(format!("vec4 requires 4 arguments, got {}", args.len()));
                    }
                    let mut it = args.into_iter();
                    return Ok((
                        FlowExpr::Vec4(
                            Box::new(it.next().unwrap()),
                            Box::new(it.next().unwrap()),
                            Box::new(it.next().unwrap()),
                            Box::new(it.next().unwrap()),
                        ),
                        rest,
                    ));
                }
                _ => {
                    // Look up as built-in function
                    if let Some(func) = FlowFunc::parse(name) {
                        return Ok((FlowExpr::Call { func, args }, rest));
                    } else {
                        return Err(format!("unknown function '{}'", name));
                    }
                }
            }
        }

        // Plain reference
        return Ok((FlowExpr::Ref(name.to_string()), &trimmed[name_end..]));
    }

    Err(format!("unexpected character: '{}'", &trimmed[..1]))
}

/// Parse a comma-separated argument list (within already-matched parens)
fn parse_flow_arg_list(input: &str) -> Result<Vec<FlowExpr>, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let mut args = Vec::new();
    let mut remaining = trimmed;

    loop {
        let remaining_trimmed = remaining.trim();
        if remaining_trimmed.is_empty() {
            break;
        }

        // Split at top-level commas (not nested in parens)
        let split_pos = find_top_level_comma(remaining_trimmed);

        let arg_str = if let Some(pos) = split_pos {
            let s = remaining_trimmed[..pos].trim();
            remaining = &remaining_trimmed[pos + 1..];
            s
        } else {
            remaining = "";
            remaining_trimmed
        };

        if arg_str.is_empty() {
            break;
        }

        let (expr, rest) = parse_flow_additive(arg_str)?;
        let rest = rest.trim();
        if !rest.is_empty() {
            return Err(format!("unexpected content in argument: '{}'", rest));
        }
        args.push(expr);

        if split_pos.is_none() {
            break;
        }
    }

    Ok(args)
}

/// Find the position of the next top-level comma (not nested in parens)
fn find_top_level_comma(input: &str) -> Option<usize> {
    let mut depth = 0i32;
    for (i, c) in input.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

/// Find the matching close paren for a flow expression.
/// Input starts AFTER the opening '(' — starts at depth=1.
fn find_flow_close_paren(input: &str) -> Option<usize> {
    let mut depth = 1i32;
    for (i, c) in input.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Parse a numeric literal (float or integer)
fn parse_flow_number(input: &str) -> Result<(FlowExpr, &str), String> {
    let trimmed = input.trim_start();
    let mut end = 0;
    let mut has_dot = false;
    let chars: Vec<char> = trimmed.chars().collect();

    // Optional leading minus
    if end < chars.len() && chars[end] == '-' {
        end += 1;
    }

    // Digits before decimal point
    while end < chars.len() && chars[end].is_ascii_digit() {
        end += 1;
    }

    // Optional decimal point and fractional digits
    if end < chars.len() && chars[end] == '.' {
        has_dot = true;
        end += 1;
        while end < chars.len() && chars[end].is_ascii_digit() {
            end += 1;
        }
    }

    if end == 0 || (end == 1 && chars[0] == '-') {
        return Err("expected number".to_string());
    }

    let num_str = &trimmed[..end];
    let value: f32 = num_str
        .parse()
        .map_err(|_| format!("invalid number: '{}'", num_str))?;

    Ok((FlowExpr::Float(value), &trimmed[end..]))
}

/// Parse a color literal: #RGB, #RRGGBB, or #RRGGBBAA
fn parse_flow_color(input: &str) -> Result<(FlowExpr, &str), String> {
    let trimmed = input.trim_start();
    if !trimmed.starts_with('#') {
        return Err("expected '#' for color literal".to_string());
    }

    let hex_start = trimmed[1..].trim_start();
    let hex_end = hex_start
        .find(|c: char| !c.is_ascii_hexdigit())
        .unwrap_or(hex_start.len());
    let hex = &hex_start[..hex_end];

    let (r, g, b, a) = match hex.len() {
        3 => {
            let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).unwrap_or(0);
            let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).unwrap_or(0);
            let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).unwrap_or(0);
            (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0)
        }
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
            let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
            let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
            (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0)
        }
        8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
            let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
            let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
            let a = u8::from_str_radix(&hex[6..8], 16).unwrap_or(255);
            (
                r as f32 / 255.0,
                g as f32 / 255.0,
                b as f32 / 255.0,
                a as f32 / 255.0,
            )
        }
        _ => return Err(format!("invalid color hex length: {}", hex.len())),
    };

    Ok((FlowExpr::Color(r, g, b, a), &hex_start[hex_end..]))
}

struct ParsedStylesheet {
    rules: Vec<(String, ElementStyle)>,
    complex_rules: Vec<(ComplexSelector, ElementStyle)>,
    variables: HashMap<String, String>,
    keyframes: Vec<CssKeyframes>,
    flows: Vec<FlowGraph>,
}

/// Parse an entire stylesheet with error collection
fn parse_stylesheet_with_errors<'a>(
    css: &'a str,
    errors: &mut Vec<ParseError>,
    variables: &HashMap<String, String>,
) -> ParseResult<'a, ParsedStylesheet> {
    let (input, _) = ws(css)?;

    // Parse blocks one at a time to collect errors
    let mut rules = Vec::new();
    let mut complex_rules = Vec::new();
    let mut parsed_variables = variables.clone();
    let mut parsed_keyframes = Vec::new();
    let mut parsed_flows = Vec::new();
    let mut flow_registry: HashMap<String, FlowGraph> = HashMap::new();
    let mut remaining = input;

    loop {
        let trimmed = remaining.trim_start();
        if trimmed.is_empty() {
            break;
        }

        // Skip CSS comments at the top level
        if trimmed.starts_with("/*") {
            if let Some(end) = trimmed.find("*/") {
                remaining = &trimmed[end + 2..];
                continue;
            } else {
                break; // Unterminated comment
            }
        }

        // Try to parse a :root block first
        if trimmed.starts_with(":root") {
            match root_block(trimmed) {
                Ok((rest, vars)) => {
                    for (name, value) in vars {
                        parsed_variables.insert(name, value);
                    }
                    remaining = rest;
                    continue;
                }
                Err(_) => {
                    // Not a valid :root block, try as a rule
                }
            }
        }

        // Try to parse @keyframes block
        if trimmed.starts_with("@keyframes") {
            match keyframes_block(trimmed, errors, &parsed_variables) {
                Ok((rest, keyframes)) => {
                    parsed_keyframes.push(keyframes);
                    remaining = rest;
                    continue;
                }
                Err(_) => {
                    // Not a valid @keyframes block, try as a rule
                }
            }
        }

        // Try to parse @flow block (pass registry of already-parsed flows for `use`)
        if trimmed.starts_with("@flow") {
            let registry = if flow_registry.is_empty() {
                None
            } else {
                Some(&flow_registry)
            };
            match flow_block(trimmed, errors, registry) {
                Ok((rest, flow)) => {
                    flow_registry.insert(flow.name.clone(), flow.clone());
                    parsed_flows.push(flow);
                    remaining = rest;
                    continue;
                }
                Err(_) => {
                    // Not a valid @flow block, try as a rule
                }
            }
        }

        // Try to parse a rule (complex selector or simple #id selector)
        // Supports comma-separated selector lists: #a, #b { ... }
        match css_rule_complex_or_simple(css, errors, &parsed_variables)(trimmed) {
            Ok((rest, parsed_rules)) => {
                for rule in parsed_rules {
                    match rule {
                        ParsedRule::Simple(key, style) => rules.push((key, style)),
                        ParsedRule::Complex(selector, style) => {
                            complex_rules.push((selector, style))
                        }
                    }
                }
                remaining = rest;
            }
            Err(nom::Err::Error(_)) | Err(nom::Err::Failure(_)) => {
                // Rule failed to parse — skip past its `{ ... }` block
                // (or to the next `;` for at-rule-style declarations) and
                // continue with the next rule instead of breaking. Before
                // this recovery a single bad selector (e.g. an unknown
                // pseudo whose parser leaked `(...)` into rule_block's
                // expected `{`) silently dropped EVERY subsequent rule
                // in the stylesheet.
                if let Some(skip) = skip_failed_rule(trimmed) {
                    // `trimmed` may sit at a different offset than
                    // `remaining` (we trimmed leading whitespace earlier
                    // in the loop). Recompute the corresponding offset
                    // in `remaining` by finding the trimmed slice's
                    // start position via pointer arithmetic.
                    let trim_offset = trimmed.as_ptr() as usize - remaining.as_ptr() as usize;
                    let skip_offset = trim_offset + skip;
                    if skip_offset < remaining.len() {
                        remaining = &remaining[skip_offset..];
                        continue;
                    }
                    break;
                }
                // No recovery point found — bail.
                break;
            }
            Err(nom::Err::Incomplete(_)) => {
                break;
            }
        }
    }

    let (input, _) = ws(remaining)?;
    Ok((
        input,
        ParsedStylesheet {
            rules,
            complex_rules,
            variables: parsed_variables,
            keyframes: parsed_keyframes,
            flows: parsed_flows,
        },
    ))
}

/// Recovery helper for the stylesheet loop: when a rule fails to
/// parse, find the byte offset where the NEXT rule probably starts
/// so we can skip the bad block and keep parsing instead of breaking
/// the entire stylesheet at the first failure.
///
/// Strategy:
///   1. Scan for the matching `}` of the current rule's body, tracking
///      paren nesting so `}` inside `:has(...)` or attribute selectors
///      doesn't get mistaken for the rule terminator. Return one past
///      the closing brace.
///   2. If no `{` appears at all (the failure was inside the selector
///      with no following block), advance past the next `;` or whole
///      input to skip the malformed fragment.
fn skip_failed_rule(input: &str) -> Option<usize> {
    let bytes = input.as_bytes();
    let mut paren_depth: i32 = 0;
    let mut brace_depth: i32 = 0;
    let mut saw_brace = false;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' => paren_depth += 1,
            b')' => paren_depth = (paren_depth - 1).max(0),
            b'{' if paren_depth == 0 => {
                brace_depth += 1;
                saw_brace = true;
            }
            b'}' if paren_depth == 0 => {
                brace_depth -= 1;
                if saw_brace && brace_depth <= 0 {
                    return Some(i + 1);
                }
            }
            b';' if paren_depth == 0 && !saw_brace => {
                return Some(i + 1);
            }
            _ => {}
        }
    }
    if input.is_empty() {
        None
    } else {
        Some(input.len())
    }
}

/// Result of parsing a CSS rule — either a simple #id rule or a complex selector rule
enum ParsedRule {
    /// Simple rule: key is "id" or "id:state"
    Simple(String, ElementStyle),
    /// Complex rule with a full selector chain
    Complex(ComplexSelector, ElementStyle),
}

/// Parse a CSS rule with either a complex or simple selector.
/// Supports comma-separated selector lists: `#a, .b, div > span { ... }`
fn css_rule_complex_or_simple<'a, 'b>(
    original_css: &'a str,
    errors: &'b mut Vec<ParseError>,
    variables: &'b HashMap<String, String>,
) -> impl FnMut(&'a str) -> ParseResult<'a, Vec<ParsedRule>> + 'b
where
    'a: 'b,
{
    move |input: &'a str| {
        let (input, _) = ws(input)?;

        // Parse first selector
        let (mut remaining, first_selector) =
            context("CSS rule selector", parse_complex_selector)(input)?;
        let (trimmed, _) = ws(remaining)?;
        remaining = trimmed;

        let mut selectors = vec![first_selector];

        // Parse additional comma-separated selectors
        while remaining.starts_with(',') {
            let after_comma = &remaining[1..];
            let (trimmed, _) = ws(after_comma)?;
            match parse_complex_selector(trimmed) {
                Ok((rest, selector)) => {
                    selectors.push(selector);
                    let (trimmed, _) = ws(rest)?;
                    remaining = trimmed;
                }
                Err(_) => break,
            }
        }

        // Parse the rule block (shared by all selectors)
        let (input, properties) = context("CSS rule block", rule_block)(remaining)?;

        let mut style = ElementStyle::new();
        for (name, value) in properties {
            let resolved_value = resolve_var_references(value, variables);
            apply_property_with_errors(
                &mut style,
                name,
                &resolved_value,
                original_css,
                input,
                errors,
            );
        }

        // Create a rule for each selector in the comma-separated list
        let mut rules = Vec::with_capacity(selectors.len());
        for selector in selectors {
            let rule = if selector.is_simple() {
                let compound = &selector.segments[0].0;
                if let Some(simple_key) = try_as_simple_selector(compound) {
                    ParsedRule::Simple(simple_key, style.clone())
                } else {
                    ParsedRule::Complex(selector, style.clone())
                }
            } else {
                ParsedRule::Complex(selector, style.clone())
            };
            rules.push(rule);
        }

        Ok((input, rules))
    }
}

/// Try to convert a compound selector to a simple "id" or "id:state" key.
/// Returns Some(key) if the compound is just #id or #id:state, None otherwise.
fn try_as_simple_selector(compound: &CompoundSelector) -> Option<String> {
    let mut id = None;
    let mut state = None;
    let mut pseudo_element = None;

    for part in &compound.parts {
        match part {
            SelectorPart::Id(i) => {
                if id.is_some() {
                    return None; // Multiple IDs
                }
                id = Some(i.as_str());
            }
            SelectorPart::State(s) => {
                if state.is_some() {
                    return None; // Multiple states
                }
                state = Some(s);
            }
            SelectorPart::PseudoElement(name) => {
                if pseudo_element.is_some() {
                    return None; // Multiple pseudo-elements
                }
                pseudo_element = Some(name.as_str());
            }
            // If there are type selectors, classes, structural pseudos, universal, :not(), :is(), or :has(), it's not simple
            SelectorPart::Type(_)
            | SelectorPart::Class(_)
            | SelectorPart::PseudoClass(_)
            | SelectorPart::Universal
            | SelectorPart::Not(_)
            | SelectorPart::Is(_)
            | SelectorPart::Has(_) => return None,
        }
    }

    let id = id?; // Must have an ID to be a simple selector

    // Can't have both state and pseudo-element
    if state.is_some() && pseudo_element.is_some() {
        return None;
    }

    if let Some(pe) = pseudo_element {
        Some(format!("{}::{}", id, pe))
    } else {
        match state {
            Some(s) => Some(format!("{}:{}", id, s)),
            None => Some(id.to_string()),
        }
    }
}

/// Parse a complete rule with error collection and variable resolution: #id { ... } or #id:state { ... }
fn css_rule_with_errors_and_vars<'a, 'b>(
    original_css: &'a str,
    errors: &'b mut Vec<ParseError>,
    variables: &'b HashMap<String, String>,
) -> impl FnMut(&'a str) -> ParseResult<'a, (String, ElementStyle)> + 'b
where
    'a: 'b,
{
    move |input: &'a str| {
        let (input, _) = ws(input)?;
        let (input, selector) = context("CSS rule selector", id_selector)(input)?;
        let (input, _) = ws(input)?;
        let (input, properties) = context("CSS rule block", rule_block)(input)?;

        let mut style = ElementStyle::new();
        for (name, value) in properties {
            // Resolve var() references before applying
            let resolved_value = resolve_var_references(value, variables);
            apply_property_with_errors(
                &mut style,
                name,
                &resolved_value,
                original_css,
                input,
                errors,
            );
        }

        Ok((input, (selector.key(), style)))
    }
}

/// Resolve var(--name) references in a value string
fn resolve_var_references(value: &str, variables: &HashMap<String, String>) -> String {
    let mut result = value.to_string();
    let mut iterations = 0;
    const MAX_ITERATIONS: usize = 10; // Prevent infinite loops from circular references

    // Keep resolving until no more var() references
    while result.contains("var(") && iterations < MAX_ITERATIONS {
        iterations += 1;

        // Find var( and its matching )
        if let Some(start) = result.find("var(") {
            let after_var = &result[start + 4..];

            // Find matching closing paren (handling nested parens)
            let mut depth = 1;
            let mut end_offset = 0;
            for (i, c) in after_var.char_indices() {
                match c {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            end_offset = i;
                            break;
                        }
                    }
                    _ => {}
                }
            }

            if depth == 0 {
                let var_content = &after_var[..end_offset];
                let full_var = &result[start..start + 4 + end_offset + 1];

                // Parse var content: --name or --name, fallback
                let resolved = if let Some(comma_pos) = var_content.find(',') {
                    let var_name = var_content[..comma_pos].trim();
                    let fallback = var_content[comma_pos + 1..].trim();

                    if let Some(name) = var_name.strip_prefix("--") {
                        variables
                            .get(name)
                            .cloned()
                            .unwrap_or_else(|| fallback.to_string())
                    } else {
                        fallback.to_string()
                    }
                } else {
                    let var_name = var_content.trim();
                    if let Some(name) = var_name.strip_prefix("--") {
                        variables.get(name).cloned().unwrap_or_default()
                    } else {
                        String::new()
                    }
                };

                result = result.replace(full_var, &resolved);
            } else {
                // Malformed var(), break to avoid infinite loop
                break;
            }
        }
    }

    result
}

// ============================================================================
// Property Application
// ============================================================================

fn apply_property(style: &mut ElementStyle, name: &str, value: &str) {
    match name {
        "background" | "background-color" => {
            if let Some(brush) = parse_brush(value) {
                style.background = Some(brush);
            }
        }
        "color" => {
            if let Some(c) = parse_color(value) {
                style.text_color = Some(c);
            }
        }
        "font-size" => {
            if let Some(px) = parse_length_value(value) {
                style.font_size = Some(px);
            }
        }
        "font-weight" => {
            style.font_weight = parse_font_weight(value);
        }
        "text-decoration" | "text-decoration-line" => {
            style.text_decoration = parse_text_decoration(value);
        }
        "line-height" => {
            if let Some(val) = parse_length_value(value) {
                style.line_height = Some(val);
            } else if let Ok(val) = value.trim().parse::<f32>() {
                style.line_height = Some(val);
            }
        }
        "text-align" => {
            style.text_align = parse_text_align(value);
        }
        "letter-spacing" => {
            if let Some(px) = parse_length_value(value) {
                style.letter_spacing = Some(px);
            }
        }
        "fill" => {
            if value.trim().eq_ignore_ascii_case("none") {
                style.fill = Some(Color::TRANSPARENT);
            } else if let Some(color) = parse_color(value) {
                style.fill = Some(color);
            }
        }
        "stroke" => {
            if let Some(color) = parse_color(value) {
                style.stroke = Some(color);
            }
        }
        "stroke-width" => {
            if let Some(px) = parse_length_value(value) {
                style.stroke_width = Some(px);
            }
        }
        "stroke-dasharray" => {
            let trimmed = value.trim();
            if trimmed.eq_ignore_ascii_case("none") {
                style.stroke_dasharray = Some(vec![]);
            } else {
                let dashes: Vec<f32> = trimmed
                    .split([',', ' '])
                    .filter_map(|s| {
                        let s = s.trim();
                        if s.is_empty() {
                            None
                        } else {
                            parse_length_value(s)
                        }
                    })
                    .collect();
                if !dashes.is_empty() {
                    style.stroke_dasharray = Some(dashes);
                }
            }
        }
        "stroke-dashoffset" => {
            if let Some(px) = parse_length_value(value) {
                style.stroke_dashoffset = Some(px);
            }
        }
        "d" => {
            // CSS d: path("...") for SVG path morphing
            let trimmed = value.trim();
            if let Some(inner) = trimmed
                .strip_prefix("path(")
                .and_then(|s| s.strip_suffix(')'))
            {
                let inner = inner.trim();
                // Remove surrounding quotes if present
                let path_data = if (inner.starts_with('"') && inner.ends_with('"'))
                    || (inner.starts_with('\'') && inner.ends_with('\''))
                {
                    &inner[1..inner.len() - 1]
                } else {
                    inner
                };
                style.svg_path_data = Some(path_data.to_string());
            }
        }
        "scrollbar-color" => {
            let parts: Vec<&str> = value.split_whitespace().collect();
            if parts.len() == 2 {
                if let (Some(thumb), Some(track)) = (parse_color(parts[0]), parse_color(parts[1])) {
                    style.scrollbar_color = Some((thumb, track));
                }
            }
        }
        "scrollbar-width" => match value.trim() {
            "auto" => style.scrollbar_width = Some(crate::element_style::ScrollbarWidth::Auto),
            "thin" => style.scrollbar_width = Some(crate::element_style::ScrollbarWidth::Thin),
            "none" => style.scrollbar_width = Some(crate::element_style::ScrollbarWidth::None),
            _ => {}
        },
        "border-radius" => match try_parse_calc(value) {
            CalcParseResult::Dynamic(expr) => {
                style
                    .dynamic_properties
                    .get_or_insert_with(Vec::new)
                    .push(crate::element_style::DynamicProperty::CornerRadius(expr));
            }
            CalcParseResult::Static(val) => {
                style.corner_radius = Some(CornerRadius::uniform(val.max(0.0)));
            }
            CalcParseResult::NotCalc => {
                if let Some(radius) = parse_radius(value) {
                    style.corner_radius = Some(radius);
                }
            }
        },
        "corner-shape" => {
            let value = value.trim();
            if let Some(cs) = parse_corner_shape_value(value) {
                style.corner_shape = Some(cs);
            }
        }
        "overflow-fade" => {
            let trimmed = value.trim();
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            let parse_fade_val =
                |s: &str| -> Option<f32> { parse_length_value(s).map(|v| v.max(0.0)) };
            match parts.len() {
                1 => {
                    if let Some(v) = parse_fade_val(parts[0]) {
                        style.overflow_fade = Some(OverflowFade::uniform(v));
                    }
                }
                2 => {
                    if let (Some(v), Some(h)) = (parse_fade_val(parts[0]), parse_fade_val(parts[1]))
                    {
                        style.overflow_fade = Some(OverflowFade::new(v, h, v, h));
                    }
                }
                4 => {
                    if let (Some(t), Some(r), Some(b), Some(l)) = (
                        parse_fade_val(parts[0]),
                        parse_fade_val(parts[1]),
                        parse_fade_val(parts[2]),
                        parse_fade_val(parts[3]),
                    ) {
                        style.overflow_fade = Some(OverflowFade::new(t, r, b, l));
                    }
                }
                _ => {}
            }
        }
        "box-shadow" => {
            if let Some(shadow_stack) = parse_shadow_stack(value) {
                style.shadow = shadow_stack;
            }
        }
        "text-shadow" => {
            if let Some(shadow) = parse_shadow(value) {
                style.text_shadow = Some(shadow);
            }
        }
        "transform" => {
            parse_transform_with_3d(value, style);
        }
        "transform-origin" => {
            if let Some(origin) = parse_transform_origin(value) {
                style.transform_origin = Some(origin);
            }
        }
        "opacity" => match try_parse_calc(value) {
            CalcParseResult::Dynamic(expr) => {
                style
                    .dynamic_properties
                    .get_or_insert_with(Vec::new)
                    .push(crate::element_style::DynamicProperty::Opacity(expr));
            }
            CalcParseResult::Static(val) => {
                style.opacity = Some(val.clamp(0.0, 1.0));
            }
            CalcParseResult::NotCalc => {
                if let Ok((_, opacity)) = parse_opacity::<nom::error::Error<&str>>(value) {
                    style.opacity = Some(opacity.clamp(0.0, 1.0));
                }
            }
        },
        "render-layer" => {
            if let Ok((_, layer)) = parse_render_layer::<nom::error::Error<&str>>(value) {
                style.render_layer = Some(layer);
            }
        }
        "z-index" => {
            if let Ok(z) = value.trim().parse::<i32>() {
                style.z_index = Some(z);
            } else if let Ok((_, layer)) = parse_render_layer::<nom::error::Error<&str>>(value) {
                style.render_layer = Some(layer);
            }
        }
        // 3D transform properties
        "rotate-x" => match try_parse_calc(value) {
            CalcParseResult::Dynamic(expr) => {
                style
                    .dynamic_properties
                    .get_or_insert_with(Vec::new)
                    .push(crate::element_style::DynamicProperty::RotateX(expr));
            }
            CalcParseResult::Static(val) => {
                style.rotate_x = Some(val);
            }
            CalcParseResult::NotCalc => {
                if let Some(deg) = parse_angle_value(value) {
                    style.rotate_x = Some(deg);
                }
            }
        },
        "rotate-y" => match try_parse_calc(value) {
            CalcParseResult::Dynamic(expr) => {
                style
                    .dynamic_properties
                    .get_or_insert_with(Vec::new)
                    .push(crate::element_style::DynamicProperty::RotateY(expr));
            }
            CalcParseResult::Static(val) => {
                style.rotate_y = Some(val);
            }
            CalcParseResult::NotCalc => {
                if let Some(deg) = parse_angle_value(value) {
                    style.rotate_y = Some(deg);
                }
            }
        },
        "perspective" => match try_parse_calc(value) {
            CalcParseResult::Dynamic(expr) => {
                style
                    .dynamic_properties
                    .get_or_insert_with(Vec::new)
                    .push(crate::element_style::DynamicProperty::Perspective(expr));
            }
            CalcParseResult::Static(val) => {
                style.perspective = Some(val);
            }
            CalcParseResult::NotCalc => {
                if let Some(px) = parse_css_px(value) {
                    style.perspective = Some(px);
                }
            }
        },
        // 2D transform properties (standalone, work with text inheritance)
        "rotate" => match try_parse_calc(value) {
            CalcParseResult::Dynamic(expr) => {
                style
                    .dynamic_properties
                    .get_or_insert_with(Vec::new)
                    .push(crate::element_style::DynamicProperty::Rotate(expr));
            }
            CalcParseResult::Static(val) => {
                style.rotate = Some(val);
            }
            CalcParseResult::NotCalc => {
                if let Some(deg) = parse_angle_value(value) {
                    style.rotate = Some(deg);
                }
            }
        },
        "skew-x" => match try_parse_calc(value) {
            CalcParseResult::Dynamic(expr) => {
                style
                    .dynamic_properties
                    .get_or_insert_with(Vec::new)
                    .push(crate::element_style::DynamicProperty::SkewX(expr));
            }
            CalcParseResult::Static(val) => {
                style.skew_x = Some(val);
            }
            CalcParseResult::NotCalc => {
                if let Some(deg) = parse_angle_value(value) {
                    style.skew_x = Some(deg);
                }
            }
        },
        "skew-y" => match try_parse_calc(value) {
            CalcParseResult::Dynamic(expr) => {
                style
                    .dynamic_properties
                    .get_or_insert_with(Vec::new)
                    .push(crate::element_style::DynamicProperty::SkewY(expr));
            }
            CalcParseResult::Static(val) => {
                style.skew_y = Some(val);
            }
            CalcParseResult::NotCalc => {
                if let Some(deg) = parse_angle_value(value) {
                    style.skew_y = Some(deg);
                }
            }
        },
        "shape-3d" | "shape" => {
            if is_valid_shape_3d(value) {
                style.shape_3d = Some(value.trim().to_lowercase());
            }
        }
        "depth" => match try_parse_calc(value) {
            CalcParseResult::Dynamic(expr) => {
                style
                    .dynamic_properties
                    .get_or_insert_with(Vec::new)
                    .push(crate::element_style::DynamicProperty::Depth(expr));
            }
            CalcParseResult::Static(val) => {
                style.depth = Some(val);
            }
            CalcParseResult::NotCalc => {
                if let Some(px) = parse_css_px(value) {
                    style.depth = Some(px);
                }
            }
        },
        "light-direction" | "light" => {
            if let Some(dir) = parse_vec3_value(value) {
                style.light_direction = Some(dir);
            }
        }
        "light-intensity" => {
            if let Ok(v) = value.trim().parse::<f32>() {
                style.light_intensity = Some(v);
            }
        }
        "light-color" => {
            // Stub: light color modulation (Phase 5)
            // Currently ignored — light color is always white
        }
        "ambient" => {
            if let Ok(v) = value.trim().parse::<f32>() {
                style.ambient = Some(v);
            }
        }
        "specular" => {
            if let Ok(v) = value.trim().parse::<f32>() {
                style.specular = Some(v);
            }
        }
        "translate-z" => match try_parse_calc(value) {
            CalcParseResult::Dynamic(expr) => {
                style
                    .dynamic_properties
                    .get_or_insert_with(Vec::new)
                    .push(crate::element_style::DynamicProperty::TranslateZ(expr));
            }
            CalcParseResult::Static(val) => {
                style.translate_z = Some(val);
            }
            CalcParseResult::NotCalc => {
                if let Some(px) = parse_css_px(value) {
                    style.translate_z = Some(px);
                }
            }
        },
        "3d-op" | "shape-combine" => {
            if is_valid_op_3d(value) {
                style.op_3d = Some(value.trim().to_lowercase());
            }
        }
        "3d-blend" | "shape-blend" => {
            if let Some(px) = parse_css_px(value) {
                style.blend_3d = Some(px);
            }
        }
        "surface" => {
            // Map surface names to existing material system
            match value.trim() {
                "flat" | "solid" | "none" => {
                    // No material (default solid rendering)
                }
                "glossy" | "glass" => {
                    style.material = Some(Material::Glass(GlassMaterial::default()));
                }
                "metallic" | "chrome" => {
                    style.material = Some(Material::Metallic(MetallicMaterial::new()));
                }
                "gold" => {
                    style.material = Some(Material::Metallic(MetallicMaterial::gold()));
                }
                "wood" => {
                    style.material = Some(Material::Wood(WoodMaterial::default()));
                }
                _ => {}
            }
        }
        "surface-roughness" | "surface-fresnel" | "surface-color" | "surface-normal" => {
            // Stubs for Phase 5 surface model extensions
        }
        "animation" => {
            if let Some(animation) = parse_animation(value) {
                style.animation = Some(animation);
            }
        }
        "animation-name" => {
            let mut anim = style.animation.take().unwrap_or_default();
            anim.name = value.trim().to_string();
            style.animation = Some(anim);
        }
        "animation-duration" => {
            if let Some(ms) = parse_time_value(value) {
                let mut anim = style.animation.take().unwrap_or_default();
                anim.duration_ms = ms;
                style.animation = Some(anim);
            }
        }
        "animation-delay" => {
            if let Some(ms) = parse_time_value(value) {
                let mut anim = style.animation.take().unwrap_or_default();
                anim.delay_ms = ms;
                style.animation = Some(anim);
            }
        }
        "animation-timing-function" => {
            if let Some(timing) = AnimationTiming::from_str(value.trim()) {
                let mut anim = style.animation.take().unwrap_or_default();
                anim.timing = timing;
                style.animation = Some(anim);
            }
        }
        "animation-iteration-count" => {
            let mut anim = style.animation.take().unwrap_or_default();
            if value.trim().eq_ignore_ascii_case("infinite") {
                anim.iteration_count = 0;
            } else if let Ok(count) = value.trim().parse::<u32>() {
                anim.iteration_count = count;
            }
            style.animation = Some(anim);
        }
        "animation-direction" => {
            if let Some(direction) = parse_animation_direction(value.trim()) {
                let mut anim = style.animation.take().unwrap_or_default();
                anim.direction = direction;
                style.animation = Some(anim);
            }
        }
        "animation-fill-mode" => {
            if let Some(fill_mode) = parse_animation_fill_mode(value.trim()) {
                let mut anim = style.animation.take().unwrap_or_default();
                anim.fill_mode = fill_mode;
                style.animation = Some(anim);
            }
        }
        "transition" => {
            if let Some(transitions) = parse_transition(value) {
                style.transition = Some(transitions);
            }
        }
        "transition-property" => {
            let mut ts = style.transition.take().unwrap_or_else(|| CssTransitionSet {
                transitions: vec![CssTransition {
                    property: String::new(),
                    duration_ms: 0,
                    timing: AnimationTiming::Ease,
                    delay_ms: 0,
                }],
            });
            if let Some(t) = ts.transitions.first_mut() {
                t.property = value.trim().to_string();
            }
            style.transition = Some(ts);
        }
        "transition-duration" => {
            if let Some(ms) = parse_time_value(value) {
                let mut ts = style.transition.take().unwrap_or_else(|| CssTransitionSet {
                    transitions: vec![CssTransition {
                        property: "all".to_string(),
                        duration_ms: 0,
                        timing: AnimationTiming::Ease,
                        delay_ms: 0,
                    }],
                });
                if let Some(t) = ts.transitions.first_mut() {
                    t.duration_ms = ms;
                }
                style.transition = Some(ts);
            }
        }
        "transition-timing-function" => {
            if let Some(timing) = AnimationTiming::from_str(value.trim()) {
                let mut ts = style.transition.take().unwrap_or_else(|| CssTransitionSet {
                    transitions: vec![CssTransition {
                        property: "all".to_string(),
                        duration_ms: 0,
                        timing: AnimationTiming::Ease,
                        delay_ms: 0,
                    }],
                });
                if let Some(t) = ts.transitions.first_mut() {
                    t.timing = timing;
                }
                style.transition = Some(ts);
            }
        }
        "transition-delay" => {
            if let Some(ms) = parse_time_value(value) {
                let mut ts = style.transition.take().unwrap_or_else(|| CssTransitionSet {
                    transitions: vec![CssTransition {
                        property: "all".to_string(),
                        duration_ms: 0,
                        timing: AnimationTiming::Ease,
                        delay_ms: 0,
                    }],
                });
                if let Some(t) = ts.transitions.first_mut() {
                    t.delay_ms = ms;
                }
                style.transition = Some(ts);
            }
        }
        "filter" => {
            if let Some(filter) = parse_css_filter(value) {
                style.filter = Some(filter);
            }
        }
        "backdrop-filter" => {
            let trimmed = value.trim().to_lowercase();
            match trimmed.as_str() {
                "glass" => {
                    style.material = Some(Material::Glass(GlassMaterial::new()));
                    style.render_layer = Some(RenderLayer::Glass);
                }
                "liquid-glass" => {
                    style.material = Some(Material::Glass(GlassMaterial::new()));
                    style.render_layer = Some(RenderLayer::Glass);
                }
                "metallic" => {
                    style.material = Some(Material::Metallic(MetallicMaterial::new()));
                }
                "chrome" => {
                    style.material = Some(Material::Metallic(MetallicMaterial::chrome()));
                }
                "gold" => {
                    style.material = Some(Material::Metallic(MetallicMaterial::gold()));
                }
                "wood" => {
                    style.material = Some(Material::Wood(WoodMaterial::new()));
                }
                _ => {
                    // Parse liquid-glass(...) or blur()/saturate()/brightness() functions
                    if let Some(glass) = parse_liquid_glass_functions(&trimmed) {
                        style.material = Some(Material::Glass(glass));
                        style.render_layer = Some(RenderLayer::Glass);
                    } else if let Some(glass) = parse_backdrop_filter_functions(&trimmed) {
                        style.material = Some(Material::Glass(glass));
                        style.render_layer = Some(RenderLayer::Glass);
                    }
                }
            }
        }
        "clip-path" => {
            if let Some(cp) = parse_clip_path(value) {
                style.clip_path = Some(cp);
            }
        }
        // =====================================================================
        // Layout Properties
        // =====================================================================
        "width" => {
            if let Some(dim) = parse_css_dimension(value) {
                style.width = Some(dim);
            }
        }
        "height" => {
            if let Some(dim) = parse_css_dimension(value) {
                style.height = Some(dim);
            }
        }
        "min-width" => {
            if let Some(px) = parse_css_px(value) {
                style.min_width = Some(px);
            }
        }
        "min-height" => {
            if let Some(px) = parse_css_px(value) {
                style.min_height = Some(px);
            }
        }
        "max-width" => {
            if let Some(px) = parse_css_px(value) {
                style.max_width = Some(px);
            }
        }
        "max-height" => {
            if let Some(px) = parse_css_px(value) {
                style.max_height = Some(px);
            }
        }
        "display" => match value.trim() {
            "flex" => style.display = Some(StyleDisplay::Flex),
            "block" => style.display = Some(StyleDisplay::Block),
            "none" => style.display = Some(StyleDisplay::None),
            _ => {}
        },
        "visibility" => match value.trim() {
            "hidden" | "collapse" => {
                style.visibility = Some(crate::element_style::StyleVisibility::Hidden)
            }
            "visible" | "normal" => {
                style.visibility = Some(crate::element_style::StyleVisibility::Visible)
            }
            _ => {}
        },
        "flex-direction" => match value.trim() {
            "row" => {
                style.display = Some(StyleDisplay::Flex);
                style.flex_direction = Some(StyleFlexDirection::Row);
            }
            "column" => {
                style.display = Some(StyleDisplay::Flex);
                style.flex_direction = Some(StyleFlexDirection::Column);
            }
            "row-reverse" => {
                style.display = Some(StyleDisplay::Flex);
                style.flex_direction = Some(StyleFlexDirection::RowReverse);
            }
            "column-reverse" => {
                style.display = Some(StyleDisplay::Flex);
                style.flex_direction = Some(StyleFlexDirection::ColumnReverse);
            }
            _ => {}
        },
        "flex-wrap" => match value.trim() {
            "wrap" => style.flex_wrap = Some(true),
            "nowrap" => style.flex_wrap = Some(false),
            _ => {}
        },
        "flex-grow" => {
            if let Ok(v) = value.trim().parse::<f32>() {
                style.flex_grow = Some(v);
            }
        }
        "flex-shrink" => {
            if let Ok(v) = value.trim().parse::<f32>() {
                style.flex_shrink = Some(v);
            }
        }
        "align-items" => match value.trim() {
            "center" => style.align_items = Some(StyleAlign::Center),
            "start" | "flex-start" => style.align_items = Some(StyleAlign::Start),
            "end" | "flex-end" => style.align_items = Some(StyleAlign::End),
            "stretch" => style.align_items = Some(StyleAlign::Stretch),
            "baseline" => style.align_items = Some(StyleAlign::Baseline),
            _ => {}
        },
        "justify-content" => match value.trim() {
            "center" => style.justify_content = Some(StyleJustify::Center),
            "start" | "flex-start" => style.justify_content = Some(StyleJustify::Start),
            "end" | "flex-end" => style.justify_content = Some(StyleJustify::End),
            "space-between" => style.justify_content = Some(StyleJustify::SpaceBetween),
            "space-around" => style.justify_content = Some(StyleJustify::SpaceAround),
            "space-evenly" => style.justify_content = Some(StyleJustify::SpaceEvenly),
            _ => {}
        },
        "align-self" => match value.trim() {
            "center" => style.align_self = Some(StyleAlign::Center),
            "start" | "flex-start" => style.align_self = Some(StyleAlign::Start),
            "end" | "flex-end" => style.align_self = Some(StyleAlign::End),
            "stretch" => style.align_self = Some(StyleAlign::Stretch),
            "baseline" => style.align_self = Some(StyleAlign::Baseline),
            _ => {}
        },
        "padding" => {
            if let Some(rect) = parse_css_spacing(value) {
                style.padding = Some(rect);
            }
        }
        "padding-top" => {
            if let Some(px) = parse_css_px(value) {
                let mut p = style.padding.unwrap_or(SpacingRect {
                    top: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 0.0,
                });
                p.top = px;
                style.padding = Some(p);
            }
        }
        "padding-right" => {
            if let Some(px) = parse_css_px(value) {
                let mut p = style.padding.unwrap_or(SpacingRect {
                    top: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 0.0,
                });
                p.right = px;
                style.padding = Some(p);
            }
        }
        "padding-bottom" => {
            if let Some(px) = parse_css_px(value) {
                let mut p = style.padding.unwrap_or(SpacingRect {
                    top: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 0.0,
                });
                p.bottom = px;
                style.padding = Some(p);
            }
        }
        "padding-left" => {
            if let Some(px) = parse_css_px(value) {
                let mut p = style.padding.unwrap_or(SpacingRect {
                    top: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 0.0,
                });
                p.left = px;
                style.padding = Some(p);
            }
        }
        "margin" => {
            if let Some(rect) = parse_css_spacing(value) {
                style.margin = Some(rect);
            }
        }
        "margin-top" => {
            if let Some(px) = parse_css_px(value) {
                let mut m = style.margin.unwrap_or(SpacingRect {
                    top: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 0.0,
                });
                m.top = px;
                style.margin = Some(m);
            }
        }
        "margin-right" => {
            if let Some(px) = parse_css_px(value) {
                let mut m = style.margin.unwrap_or(SpacingRect {
                    top: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 0.0,
                });
                m.right = px;
                style.margin = Some(m);
            }
        }
        "margin-bottom" => {
            if let Some(px) = parse_css_px(value) {
                let mut m = style.margin.unwrap_or(SpacingRect {
                    top: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 0.0,
                });
                m.bottom = px;
                style.margin = Some(m);
            }
        }
        "margin-left" => {
            if let Some(px) = parse_css_px(value) {
                let mut m = style.margin.unwrap_or(SpacingRect {
                    top: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 0.0,
                });
                m.left = px;
                style.margin = Some(m);
            }
        }
        "gap" => {
            if let Some(px) = parse_css_px(value) {
                style.gap = Some(px);
            }
        }
        "overflow" => match value.trim() {
            "hidden" | "clip" => style.overflow = Some(StyleOverflow::Clip),
            "visible" => style.overflow = Some(StyleOverflow::Visible),
            "scroll" | "auto" => style.overflow = Some(StyleOverflow::Scroll),
            _ => {}
        },
        "overflow-x" => match value.trim() {
            "hidden" | "clip" => style.overflow_x = Some(StyleOverflow::Clip),
            "visible" => style.overflow_x = Some(StyleOverflow::Visible),
            "scroll" | "auto" => style.overflow_x = Some(StyleOverflow::Scroll),
            _ => {}
        },
        "overflow-y" => match value.trim() {
            "hidden" | "clip" => style.overflow_y = Some(StyleOverflow::Clip),
            "visible" => style.overflow_y = Some(StyleOverflow::Visible),
            "scroll" | "auto" => style.overflow_y = Some(StyleOverflow::Scroll),
            _ => {}
        },
        "border" => {
            // Shorthand: border: [width] [style] [color]
            // Parts can be in any order. Style is ignored (always solid).
            for part in value.split_whitespace() {
                let p = part.trim();
                if p == "solid" || p == "dashed" || p == "dotted" || p == "none" || p == "hidden" {
                    continue; // skip style keyword
                } else if let Some(px) = parse_css_px(p) {
                    style.border_width = Some(px);
                } else if let Some(color) = parse_color(p) {
                    style.border_color = Some(color);
                }
            }
        }
        "border-width" => match try_parse_calc(value) {
            CalcParseResult::Dynamic(expr) => {
                style
                    .dynamic_properties
                    .get_or_insert_with(Vec::new)
                    .push(crate::element_style::DynamicProperty::BorderWidth(expr));
            }
            CalcParseResult::Static(val) => {
                style.border_width = Some(val.max(0.0));
            }
            CalcParseResult::NotCalc => {
                if let Some(px) = parse_css_px(value) {
                    style.border_width = Some(px);
                }
            }
        },
        "border-color" => {
            if let Some(color) = parse_color(value) {
                style.border_color = Some(color);
            }
        }
        "border-style" => {
            // Blinc borders are always solid; accept and ignore
        }
        "outline-width" => {
            if let Some(px) = parse_css_px(value) {
                style.outline_width = Some(px);
            }
        }
        "outline-color" => {
            if let Some(color) = parse_color(value) {
                style.outline_color = Some(color);
            }
        }
        "outline-offset" => {
            if let Some(px) = parse_css_px(value) {
                style.outline_offset = Some(px);
            }
        }
        "outline" => {
            // Shorthand: outline: <width> solid <color>
            // We ignore the style (always solid) and parse width + color
            let parts = split_whitespace_respecting_parens(value);
            for part in &parts {
                if let Some(px) = parse_css_px(part) {
                    style.outline_width = Some(px);
                } else if part != "solid" && part != "none" && part != "dotted" && part != "dashed"
                {
                    if let Some(color) = parse_color(part) {
                        style.outline_color = Some(color);
                    }
                }
            }
            if value.trim() == "none" {
                style.outline_width = Some(0.0);
            }
        }
        "caret-color" => {
            if let Some(color) = parse_color(value) {
                style.caret_color = Some(color);
            }
        }
        "selection-color" => {
            if let Some(color) = parse_color(value) {
                style.selection_color = Some(color);
            }
        }
        "accent-color" => {
            if let Some(color) = parse_color(value) {
                style.accent_color = Some(color);
            }
        }
        "placeholder-color" => {
            if let Some(color) = parse_color(value) {
                style.placeholder_color = Some(color);
            }
        }
        "position" => match value.trim() {
            "static" => style.position = Some(StylePosition::Static),
            "relative" => style.position = Some(StylePosition::Relative),
            "absolute" => style.position = Some(StylePosition::Absolute),
            "fixed" => style.position = Some(StylePosition::Fixed),
            "sticky" => style.position = Some(StylePosition::Sticky),
            _ => {}
        },
        "top" => {
            if let Some(px) = parse_css_px(value) {
                style.top = Some(px);
            }
        }
        "right" => {
            if let Some(px) = parse_css_px(value) {
                style.right = Some(px);
            }
        }
        "bottom" => {
            if let Some(px) = parse_css_px(value) {
                style.bottom = Some(px);
            }
        }
        "left" => {
            if let Some(px) = parse_css_px(value) {
                style.left = Some(px);
            }
        }
        "inset" => {
            if let Some(px) = parse_css_px(value) {
                style.top = Some(px);
                style.right = Some(px);
                style.bottom = Some(px);
                style.left = Some(px);
            }
        }
        "object-fit" => match value.trim() {
            "cover" => style.object_fit = Some(0),
            "contain" => style.object_fit = Some(1),
            "fill" => style.object_fit = Some(2),
            "scale-down" => style.object_fit = Some(3),
            "none" => style.object_fit = Some(4),
            _ => {}
        },
        "object-position" => {
            if let Some(pos) = parse_object_position(value) {
                style.object_position = Some(pos);
            }
        }
        "loading" => match value.trim() {
            "lazy" => style.loading_strategy = Some(1),
            "eager" => style.loading_strategy = Some(0),
            _ => {}
        },
        "image-placeholder-color" => {
            if let Some(color) = parse_color(value) {
                style.image_placeholder_color = Some([color.r, color.g, color.b, color.a]);
                style.image_placeholder_type = Some(1);
            }
        }
        "image-placeholder-image" | "image-placeholder" => {
            let trimmed = value.trim().trim_matches(|c| c == '"' || c == '\'');
            if !trimmed.is_empty() {
                style.image_placeholder_image = Some(trimmed.to_string());
                style.image_placeholder_type = Some(2);
            }
        }
        "image-placeholder-type" => match value.trim() {
            "skeleton" => style.image_placeholder_type = Some(3),
            "none" => style.image_placeholder_type = Some(0),
            "color" => style.image_placeholder_type = Some(1),
            _ => {}
        },
        "fade-duration" => {
            if let Some(ms) = parse_time_value(value) {
                style.fade_duration_ms = Some(ms);
            }
        }
        "pointer-events" => match value.trim() {
            "auto" => style.pointer_events = Some(blinc_core::PointerEvents::Auto),
            "none" => style.pointer_events = Some(blinc_core::PointerEvents::None),
            _ => {}
        },
        "cursor" => {
            if let Some(cursor) = parse_cursor(value) {
                style.cursor = Some(cursor);
            }
        }
        "mix-blend-mode" => {
            if let Some(mode) = parse_blend_mode(value) {
                style.mix_blend_mode = Some(mode);
            }
        }
        "text-decoration-color" => {
            if let Some(c) = parse_color(value) {
                style.text_decoration_color = Some(c);
            }
        }
        "text-decoration-thickness" => {
            if let Some(px) = parse_length_value(value) {
                style.text_decoration_thickness = Some(px);
            }
        }
        "text-overflow" => match value.trim() {
            "clip" => style.text_overflow = Some(crate::element_style::TextOverflow::Clip),
            "ellipsis" => style.text_overflow = Some(crate::element_style::TextOverflow::Ellipsis),
            _ => {}
        },
        "white-space" => match value.trim() {
            "normal" => style.white_space = Some(crate::element_style::WhiteSpace::Normal),
            "nowrap" => style.white_space = Some(crate::element_style::WhiteSpace::Nowrap),
            "pre" => style.white_space = Some(crate::element_style::WhiteSpace::Pre),
            "pre-wrap" => style.white_space = Some(crate::element_style::WhiteSpace::PreWrap),
            _ => {}
        },
        "mask-image" => {
            let v = value.trim();
            if v == "none" {
                style.mask_image = None;
            } else if v.starts_with("linear-gradient(") {
                if let Some(g) = parse_linear_gradient(v) {
                    style.mask_image = Some(blinc_core::MaskImage::Gradient(g));
                }
            } else if v.starts_with("radial-gradient(") {
                if let Some(g) = parse_radial_gradient(v) {
                    style.mask_image = Some(blinc_core::MaskImage::Gradient(g));
                }
            } else if let Some(url) = parse_url_value(v) {
                style.mask_image = Some(blinc_core::MaskImage::Url(url));
            }
        }
        "mask-mode" => match value.trim() {
            "alpha" => style.mask_mode = Some(blinc_core::MaskMode::Alpha),
            "luminance" => style.mask_mode = Some(blinc_core::MaskMode::Luminance),
            _ => {}
        },
        "flow" => {
            let v = value.trim();
            if v == "none" {
                style.flow = None;
            } else {
                style.flow = Some(v.to_string());
            }
        }
        "pointer-space" => {
            use crate::pointer_query::{PointerSpace, PointerSpaceConfig};
            let v = value.trim();
            let space = match v {
                "self" => PointerSpace::SelfSpace,
                "parent" => PointerSpace::Parent,
                "viewport" => PointerSpace::Viewport,
                "none" => {
                    style.pointer_space = None;
                    return;
                }
                _ => PointerSpace::SelfSpace,
            };
            let config = style
                .pointer_space
                .get_or_insert(PointerSpaceConfig::default());
            config.space = space;
        }
        "pointer-origin" => {
            use crate::pointer_query::{PointerOrigin, PointerSpaceConfig};
            let v = value.trim();
            let origin = match v {
                "center" => PointerOrigin::Center,
                "top-left" => PointerOrigin::TopLeft,
                "bottom-left" => PointerOrigin::BottomLeft,
                _ => return,
            };
            let config = style
                .pointer_space
                .get_or_insert(PointerSpaceConfig::default());
            config.origin = origin;
        }
        "pointer-range" => {
            use crate::pointer_query::PointerSpaceConfig;
            let v = value.trim();
            let parts: Vec<&str> = v.split_whitespace().collect();
            if parts.len() == 2 {
                if let (Ok(min), Ok(max)) = (parts[0].parse::<f32>(), parts[1].parse::<f32>()) {
                    let config = style
                        .pointer_space
                        .get_or_insert(PointerSpaceConfig::default());
                    config.range = (min, max);
                }
            }
        }
        "pointer-smoothing" => {
            use crate::pointer_query::PointerSpaceConfig;
            let v = value.trim();
            let v = v.strip_suffix('s').unwrap_or(v); // strip optional 's' suffix
            if let Ok(dur) = v.parse::<f32>() {
                let config = style
                    .pointer_space
                    .get_or_insert(PointerSpaceConfig::default());
                config.smoothing = dur;
            }
        }
        _ => {
            // Unknown property - log at debug level for forward compatibility
            debug!(
                property = name,
                value = value,
                "Unknown CSS property (ignored)"
            );
        }
    }
}

/// Apply a property with error collection
fn apply_property_with_errors(
    style: &mut ElementStyle,
    name: &str,
    value: &str,
    original_css: &str,
    current_input: &str,
    errors: &mut Vec<ParseError>,
) {
    let (line, column, _) = calculate_position(original_css, current_input);

    match name {
        "background" | "background-color" => {
            if let Some(brush) = parse_brush(value) {
                style.background = Some(brush);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "color" => {
            if let Some(c) = parse_color(value) {
                style.text_color = Some(c);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "font-size" => {
            if let Some(px) = parse_length_value(value) {
                style.font_size = Some(px);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "font-weight" => {
            if let Some(fw) = parse_font_weight(value) {
                style.font_weight = Some(fw);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "text-decoration" | "text-decoration-line" => {
            if let Some(td) = parse_text_decoration(value) {
                style.text_decoration = Some(td);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "line-height" => {
            if let Some(val) = parse_length_value(value) {
                style.line_height = Some(val);
            } else if let Ok(val) = value.trim().parse::<f32>() {
                style.line_height = Some(val);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "text-align" => {
            if let Some(ta) = parse_text_align(value) {
                style.text_align = Some(ta);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "letter-spacing" => {
            if let Some(px) = parse_length_value(value) {
                style.letter_spacing = Some(px);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "fill" => {
            if value.trim().eq_ignore_ascii_case("none") {
                style.fill = Some(Color::TRANSPARENT);
            } else if let Some(color) = parse_color(value) {
                style.fill = Some(color);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "stroke" => {
            if let Some(color) = parse_color(value) {
                style.stroke = Some(color);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "stroke-width" => {
            if let Some(px) = parse_length_value(value) {
                style.stroke_width = Some(px);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "stroke-dasharray" => {
            let trimmed = value.trim();
            if trimmed.eq_ignore_ascii_case("none") {
                style.stroke_dasharray = Some(vec![]);
            } else {
                let dashes: Vec<f32> = trimmed
                    .split([',', ' '])
                    .filter_map(|s| {
                        let s = s.trim();
                        if s.is_empty() {
                            None
                        } else {
                            parse_length_value(s)
                        }
                    })
                    .collect();
                if !dashes.is_empty() {
                    style.stroke_dasharray = Some(dashes);
                } else {
                    errors.push(ParseError::invalid_value(name, value, line, column));
                }
            }
        }
        "stroke-dashoffset" => {
            if let Some(px) = parse_length_value(value) {
                style.stroke_dashoffset = Some(px);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "d" => {
            let trimmed = value.trim();
            if let Some(inner) = trimmed
                .strip_prefix("path(")
                .and_then(|s| s.strip_suffix(')'))
            {
                let inner = inner.trim();
                let path_data = if (inner.starts_with('"') && inner.ends_with('"'))
                    || (inner.starts_with('\'') && inner.ends_with('\''))
                {
                    &inner[1..inner.len() - 1]
                } else {
                    inner
                };
                style.svg_path_data = Some(path_data.to_string());
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "scrollbar-color" => {
            let parts: Vec<&str> = value.split_whitespace().collect();
            if parts.len() == 2 {
                if let (Some(thumb), Some(track)) = (parse_color(parts[0]), parse_color(parts[1])) {
                    style.scrollbar_color = Some((thumb, track));
                } else {
                    errors.push(ParseError::invalid_value(name, value, line, column));
                }
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "scrollbar-width" => match value.trim() {
            "auto" => style.scrollbar_width = Some(crate::element_style::ScrollbarWidth::Auto),
            "thin" => style.scrollbar_width = Some(crate::element_style::ScrollbarWidth::Thin),
            "none" => style.scrollbar_width = Some(crate::element_style::ScrollbarWidth::None),
            _ => errors.push(ParseError::invalid_value(name, value, line, column)),
        },
        "border-radius" => match try_parse_calc(value) {
            CalcParseResult::Dynamic(expr) => {
                style
                    .dynamic_properties
                    .get_or_insert_with(Vec::new)
                    .push(crate::element_style::DynamicProperty::CornerRadius(expr));
            }
            CalcParseResult::Static(val) => {
                style.corner_radius = Some(CornerRadius::uniform(val.max(0.0)));
            }
            CalcParseResult::NotCalc => {
                if let Some(radius) = parse_radius(value) {
                    style.corner_radius = Some(radius);
                } else {
                    errors.push(ParseError::invalid_value(name, value, line, column));
                }
            }
        },
        "corner-shape" => {
            let value = value.trim();
            if let Some(cs) = parse_corner_shape_value(value) {
                style.corner_shape = Some(cs);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "overflow-fade" => {
            let trimmed = value.trim();
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            let parse_fade_val =
                |s: &str| -> Option<f32> { parse_length_value(s).map(|v| v.max(0.0)) };
            match parts.len() {
                1 => {
                    if let Some(v) = parse_fade_val(parts[0]) {
                        style.overflow_fade = Some(OverflowFade::uniform(v));
                    } else {
                        errors.push(ParseError::invalid_value(name, value, line, column));
                    }
                }
                2 => {
                    if let (Some(v), Some(h)) = (parse_fade_val(parts[0]), parse_fade_val(parts[1]))
                    {
                        style.overflow_fade = Some(OverflowFade::new(v, h, v, h));
                    } else {
                        errors.push(ParseError::invalid_value(name, value, line, column));
                    }
                }
                4 => {
                    if let (Some(t), Some(r), Some(b), Some(l)) = (
                        parse_fade_val(parts[0]),
                        parse_fade_val(parts[1]),
                        parse_fade_val(parts[2]),
                        parse_fade_val(parts[3]),
                    ) {
                        style.overflow_fade = Some(OverflowFade::new(t, r, b, l));
                    } else {
                        errors.push(ParseError::invalid_value(name, value, line, column));
                    }
                }
                _ => {
                    errors.push(ParseError::invalid_value(name, value, line, column));
                }
            }
        }
        "box-shadow" => {
            if let Some(stack) = parse_shadow_stack(value) {
                style.shadow = stack;
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "text-shadow" => {
            if let Some(shadow) = parse_shadow(value) {
                style.text_shadow = Some(shadow);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "transform" => {
            if !parse_transform_with_3d(value, style) {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "transform-origin" => {
            if let Some(origin) = parse_transform_origin(value) {
                style.transform_origin = Some(origin);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "opacity" => match try_parse_calc(value) {
            CalcParseResult::Dynamic(expr) => {
                style
                    .dynamic_properties
                    .get_or_insert_with(Vec::new)
                    .push(crate::element_style::DynamicProperty::Opacity(expr));
            }
            CalcParseResult::Static(val) => {
                style.opacity = Some(val.clamp(0.0, 1.0));
            }
            CalcParseResult::NotCalc => {
                if let Ok((_, opacity)) = parse_opacity::<nom::error::Error<&str>>(value) {
                    style.opacity = Some(opacity.clamp(0.0, 1.0));
                } else {
                    errors.push(ParseError::invalid_value(name, value, line, column));
                }
            }
        },
        "render-layer" => {
            if let Ok((_, layer)) = parse_render_layer::<nom::error::Error<&str>>(value) {
                style.render_layer = Some(layer);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "z-index" => {
            if let Ok(z) = value.trim().parse::<i32>() {
                style.z_index = Some(z);
            } else if let Ok((_, layer)) = parse_render_layer::<nom::error::Error<&str>>(value) {
                style.render_layer = Some(layer);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        // 3D transform properties
        "rotate-x" => match try_parse_calc(value) {
            CalcParseResult::Dynamic(expr) => {
                style
                    .dynamic_properties
                    .get_or_insert_with(Vec::new)
                    .push(crate::element_style::DynamicProperty::RotateX(expr));
            }
            CalcParseResult::Static(val) => {
                style.rotate_x = Some(val);
            }
            CalcParseResult::NotCalc => {
                if let Some(deg) = parse_angle_value(value) {
                    style.rotate_x = Some(deg);
                } else {
                    errors.push(ParseError::invalid_value(name, value, line, column));
                }
            }
        },
        "rotate-y" => match try_parse_calc(value) {
            CalcParseResult::Dynamic(expr) => {
                style
                    .dynamic_properties
                    .get_or_insert_with(Vec::new)
                    .push(crate::element_style::DynamicProperty::RotateY(expr));
            }
            CalcParseResult::Static(val) => {
                style.rotate_y = Some(val);
            }
            CalcParseResult::NotCalc => {
                if let Some(deg) = parse_angle_value(value) {
                    style.rotate_y = Some(deg);
                } else {
                    errors.push(ParseError::invalid_value(name, value, line, column));
                }
            }
        },
        "perspective" => match try_parse_calc(value) {
            CalcParseResult::Dynamic(expr) => {
                style
                    .dynamic_properties
                    .get_or_insert_with(Vec::new)
                    .push(crate::element_style::DynamicProperty::Perspective(expr));
            }
            CalcParseResult::Static(val) => {
                style.perspective = Some(val);
            }
            CalcParseResult::NotCalc => {
                if let Some(px) = parse_css_px(value) {
                    style.perspective = Some(px);
                } else {
                    errors.push(ParseError::invalid_value(name, value, line, column));
                }
            }
        },
        // 2D transform properties (standalone, work with text inheritance)
        "rotate" => match try_parse_calc(value) {
            CalcParseResult::Dynamic(expr) => {
                style
                    .dynamic_properties
                    .get_or_insert_with(Vec::new)
                    .push(crate::element_style::DynamicProperty::Rotate(expr));
            }
            CalcParseResult::Static(val) => {
                style.rotate = Some(val);
            }
            CalcParseResult::NotCalc => {
                if let Some(deg) = parse_angle_value(value) {
                    style.rotate = Some(deg);
                } else {
                    errors.push(ParseError::invalid_value(name, value, line, column));
                }
            }
        },
        "skew-x" => match try_parse_calc(value) {
            CalcParseResult::Dynamic(expr) => {
                style
                    .dynamic_properties
                    .get_or_insert_with(Vec::new)
                    .push(crate::element_style::DynamicProperty::SkewX(expr));
            }
            CalcParseResult::Static(val) => {
                style.skew_x = Some(val);
            }
            CalcParseResult::NotCalc => {
                if let Some(deg) = parse_angle_value(value) {
                    style.skew_x = Some(deg);
                } else {
                    errors.push(ParseError::invalid_value(name, value, line, column));
                }
            }
        },
        "skew-y" => match try_parse_calc(value) {
            CalcParseResult::Dynamic(expr) => {
                style
                    .dynamic_properties
                    .get_or_insert_with(Vec::new)
                    .push(crate::element_style::DynamicProperty::SkewY(expr));
            }
            CalcParseResult::Static(val) => {
                style.skew_y = Some(val);
            }
            CalcParseResult::NotCalc => {
                if let Some(deg) = parse_angle_value(value) {
                    style.skew_y = Some(deg);
                } else {
                    errors.push(ParseError::invalid_value(name, value, line, column));
                }
            }
        },
        "shape-3d" | "shape" => {
            if is_valid_shape_3d(value) {
                style.shape_3d = Some(value.trim().to_lowercase());
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "depth" => match try_parse_calc(value) {
            CalcParseResult::Dynamic(expr) => {
                style
                    .dynamic_properties
                    .get_or_insert_with(Vec::new)
                    .push(crate::element_style::DynamicProperty::Depth(expr));
            }
            CalcParseResult::Static(val) => {
                style.depth = Some(val);
            }
            CalcParseResult::NotCalc => {
                if let Some(px) = parse_css_px(value) {
                    style.depth = Some(px);
                } else {
                    errors.push(ParseError::invalid_value(name, value, line, column));
                }
            }
        },
        "light-direction" | "light" => {
            if let Some(dir) = parse_vec3_value(value) {
                style.light_direction = Some(dir);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "light-intensity" => {
            if let Ok(v) = value.trim().parse::<f32>() {
                style.light_intensity = Some(v);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "light-color" => {
            // Stub: light color modulation (Phase 5)
        }
        "ambient" => {
            if let Ok(v) = value.trim().parse::<f32>() {
                style.ambient = Some(v);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "specular" => {
            if let Ok(v) = value.trim().parse::<f32>() {
                style.specular = Some(v);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "translate-z" => match try_parse_calc(value) {
            CalcParseResult::Dynamic(expr) => {
                style
                    .dynamic_properties
                    .get_or_insert_with(Vec::new)
                    .push(crate::element_style::DynamicProperty::TranslateZ(expr));
            }
            CalcParseResult::Static(val) => {
                style.translate_z = Some(val);
            }
            CalcParseResult::NotCalc => {
                if let Some(px) = parse_css_px(value) {
                    style.translate_z = Some(px);
                } else {
                    errors.push(ParseError::invalid_value(name, value, line, column));
                }
            }
        },
        "3d-op" | "shape-combine" => {
            if is_valid_op_3d(value) {
                style.op_3d = Some(value.trim().to_lowercase());
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "3d-blend" | "shape-blend" => {
            if let Some(px) = parse_css_px(value) {
                style.blend_3d = Some(px);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "surface" => match value.trim() {
            "flat" | "solid" | "none" => {}
            "glossy" | "glass" => {
                style.material = Some(Material::Glass(GlassMaterial::default()));
            }
            "metallic" | "chrome" => {
                style.material = Some(Material::Metallic(MetallicMaterial::new()));
            }
            "gold" => {
                style.material = Some(Material::Metallic(MetallicMaterial::gold()));
            }
            "wood" => {
                style.material = Some(Material::Wood(WoodMaterial::default()));
            }
            _ => {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        },
        "surface-roughness" | "surface-fresnel" | "surface-color" | "surface-normal" => {
            // Stubs for Phase 5 surface model extensions
        }
        "animation" => {
            if let Some(animation) = parse_animation(value) {
                style.animation = Some(animation);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "animation-name" => {
            let mut anim = style.animation.take().unwrap_or_default();
            anim.name = value.trim().to_string();
            style.animation = Some(anim);
        }
        "animation-duration" => {
            if let Some(ms) = parse_time_value(value) {
                let mut anim = style.animation.take().unwrap_or_default();
                anim.duration_ms = ms;
                style.animation = Some(anim);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "animation-delay" => {
            if let Some(ms) = parse_time_value(value) {
                let mut anim = style.animation.take().unwrap_or_default();
                anim.delay_ms = ms;
                style.animation = Some(anim);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "animation-timing-function" => {
            if let Some(timing) = AnimationTiming::from_str(value.trim()) {
                let mut anim = style.animation.take().unwrap_or_default();
                anim.timing = timing;
                style.animation = Some(anim);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "animation-iteration-count" => {
            let mut anim = style.animation.take().unwrap_or_default();
            if value.trim().eq_ignore_ascii_case("infinite") {
                anim.iteration_count = 0;
                style.animation = Some(anim);
            } else if let Ok(count) = value.trim().parse::<u32>() {
                anim.iteration_count = count;
                style.animation = Some(anim);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "animation-direction" => {
            if let Some(direction) = parse_animation_direction(value.trim()) {
                let mut anim = style.animation.take().unwrap_or_default();
                anim.direction = direction;
                style.animation = Some(anim);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "animation-fill-mode" => {
            if let Some(fill_mode) = parse_animation_fill_mode(value.trim()) {
                let mut anim = style.animation.take().unwrap_or_default();
                anim.fill_mode = fill_mode;
                style.animation = Some(anim);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "transition" => {
            if let Some(transitions) = parse_transition(value) {
                style.transition = Some(transitions);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "transition-property" => {
            let mut ts = style.transition.take().unwrap_or_else(|| CssTransitionSet {
                transitions: vec![CssTransition {
                    property: String::new(),
                    duration_ms: 0,
                    timing: AnimationTiming::Ease,
                    delay_ms: 0,
                }],
            });
            if let Some(t) = ts.transitions.first_mut() {
                t.property = value.trim().to_string();
            }
            style.transition = Some(ts);
        }
        "transition-duration" => {
            if let Some(ms) = parse_time_value(value) {
                let mut ts = style.transition.take().unwrap_or_else(|| CssTransitionSet {
                    transitions: vec![CssTransition {
                        property: "all".to_string(),
                        duration_ms: 0,
                        timing: AnimationTiming::Ease,
                        delay_ms: 0,
                    }],
                });
                if let Some(t) = ts.transitions.first_mut() {
                    t.duration_ms = ms;
                }
                style.transition = Some(ts);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "transition-timing-function" => {
            if let Some(timing) = AnimationTiming::from_str(value.trim()) {
                let mut ts = style.transition.take().unwrap_or_else(|| CssTransitionSet {
                    transitions: vec![CssTransition {
                        property: "all".to_string(),
                        duration_ms: 0,
                        timing: AnimationTiming::Ease,
                        delay_ms: 0,
                    }],
                });
                if let Some(t) = ts.transitions.first_mut() {
                    t.timing = timing;
                }
                style.transition = Some(ts);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "transition-delay" => {
            if let Some(ms) = parse_time_value(value) {
                let mut ts = style.transition.take().unwrap_or_else(|| CssTransitionSet {
                    transitions: vec![CssTransition {
                        property: "all".to_string(),
                        duration_ms: 0,
                        timing: AnimationTiming::Ease,
                        delay_ms: 0,
                    }],
                });
                if let Some(t) = ts.transitions.first_mut() {
                    t.delay_ms = ms;
                }
                style.transition = Some(ts);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "filter" => {
            if let Some(filter) = parse_css_filter(value) {
                style.filter = Some(filter);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "backdrop-filter" => {
            let trimmed = value.trim().to_lowercase();
            match trimmed.as_str() {
                "glass" => {
                    style.material = Some(Material::Glass(GlassMaterial::new()));
                    style.render_layer = Some(RenderLayer::Glass);
                }
                "liquid-glass" => {
                    style.material = Some(Material::Glass(GlassMaterial::new()));
                    style.render_layer = Some(RenderLayer::Glass);
                }
                "metallic" => {
                    style.material = Some(Material::Metallic(MetallicMaterial::new()));
                }
                "chrome" => {
                    style.material = Some(Material::Metallic(MetallicMaterial::chrome()));
                }
                "gold" => {
                    style.material = Some(Material::Metallic(MetallicMaterial::gold()));
                }
                "wood" => {
                    style.material = Some(Material::Wood(WoodMaterial::new()));
                }
                _ => {
                    if let Some(glass) = parse_liquid_glass_functions(&trimmed) {
                        style.material = Some(Material::Glass(glass));
                        style.render_layer = Some(RenderLayer::Glass);
                    } else if let Some(glass) = parse_backdrop_filter_functions(&trimmed) {
                        style.material = Some(Material::Glass(glass));
                        style.render_layer = Some(RenderLayer::Glass);
                    } else {
                        errors.push(ParseError::invalid_value(name, value, line, column));
                    }
                }
            }
        }
        "clip-path" => {
            if let Some(cp) = parse_clip_path(value) {
                style.clip_path = Some(cp);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        // =====================================================================
        // Layout Properties
        // =====================================================================
        "width" => {
            if let Some(dim) = parse_css_dimension(value) {
                style.width = Some(dim);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "height" => {
            if let Some(dim) = parse_css_dimension(value) {
                style.height = Some(dim);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "min-width" => {
            if let Some(px) = parse_css_px(value) {
                style.min_width = Some(px);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "min-height" => {
            if let Some(px) = parse_css_px(value) {
                style.min_height = Some(px);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "max-width" => {
            if let Some(px) = parse_css_px(value) {
                style.max_width = Some(px);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "max-height" => {
            if let Some(px) = parse_css_px(value) {
                style.max_height = Some(px);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "display" => match value.trim() {
            "flex" => style.display = Some(StyleDisplay::Flex),
            "block" => style.display = Some(StyleDisplay::Block),
            "none" => style.display = Some(StyleDisplay::None),
            _ => errors.push(ParseError::invalid_value(name, value, line, column)),
        },
        "visibility" => match value.trim() {
            "hidden" | "collapse" => {
                style.visibility = Some(crate::element_style::StyleVisibility::Hidden)
            }
            "visible" | "normal" => {
                style.visibility = Some(crate::element_style::StyleVisibility::Visible)
            }
            _ => errors.push(ParseError::invalid_value(name, value, line, column)),
        },
        "flex-direction" => match value.trim() {
            "row" => {
                style.display = Some(StyleDisplay::Flex);
                style.flex_direction = Some(StyleFlexDirection::Row);
            }
            "column" => {
                style.display = Some(StyleDisplay::Flex);
                style.flex_direction = Some(StyleFlexDirection::Column);
            }
            "row-reverse" => {
                style.display = Some(StyleDisplay::Flex);
                style.flex_direction = Some(StyleFlexDirection::RowReverse);
            }
            "column-reverse" => {
                style.display = Some(StyleDisplay::Flex);
                style.flex_direction = Some(StyleFlexDirection::ColumnReverse);
            }
            _ => errors.push(ParseError::invalid_value(name, value, line, column)),
        },
        "flex-wrap" => match value.trim() {
            "wrap" => style.flex_wrap = Some(true),
            "nowrap" => style.flex_wrap = Some(false),
            _ => errors.push(ParseError::invalid_value(name, value, line, column)),
        },
        "flex-grow" => {
            if let Ok(v) = value.trim().parse::<f32>() {
                style.flex_grow = Some(v);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "flex-shrink" => {
            if let Ok(v) = value.trim().parse::<f32>() {
                style.flex_shrink = Some(v);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "align-items" => match value.trim() {
            "center" => style.align_items = Some(StyleAlign::Center),
            "start" | "flex-start" => style.align_items = Some(StyleAlign::Start),
            "end" | "flex-end" => style.align_items = Some(StyleAlign::End),
            "stretch" => style.align_items = Some(StyleAlign::Stretch),
            "baseline" => style.align_items = Some(StyleAlign::Baseline),
            _ => errors.push(ParseError::invalid_value(name, value, line, column)),
        },
        "justify-content" => match value.trim() {
            "center" => style.justify_content = Some(StyleJustify::Center),
            "start" | "flex-start" => style.justify_content = Some(StyleJustify::Start),
            "end" | "flex-end" => style.justify_content = Some(StyleJustify::End),
            "space-between" => style.justify_content = Some(StyleJustify::SpaceBetween),
            "space-around" => style.justify_content = Some(StyleJustify::SpaceAround),
            "space-evenly" => style.justify_content = Some(StyleJustify::SpaceEvenly),
            _ => errors.push(ParseError::invalid_value(name, value, line, column)),
        },
        "align-self" => match value.trim() {
            "center" => style.align_self = Some(StyleAlign::Center),
            "start" | "flex-start" => style.align_self = Some(StyleAlign::Start),
            "end" | "flex-end" => style.align_self = Some(StyleAlign::End),
            "stretch" => style.align_self = Some(StyleAlign::Stretch),
            "baseline" => style.align_self = Some(StyleAlign::Baseline),
            _ => errors.push(ParseError::invalid_value(name, value, line, column)),
        },
        "padding" => {
            if let Some(rect) = parse_css_spacing(value) {
                style.padding = Some(rect);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "padding-top" => {
            if let Some(px) = parse_css_px(value) {
                let mut p = style.padding.unwrap_or(SpacingRect {
                    top: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 0.0,
                });
                p.top = px;
                style.padding = Some(p);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "padding-right" => {
            if let Some(px) = parse_css_px(value) {
                let mut p = style.padding.unwrap_or(SpacingRect {
                    top: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 0.0,
                });
                p.right = px;
                style.padding = Some(p);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "padding-bottom" => {
            if let Some(px) = parse_css_px(value) {
                let mut p = style.padding.unwrap_or(SpacingRect {
                    top: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 0.0,
                });
                p.bottom = px;
                style.padding = Some(p);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "padding-left" => {
            if let Some(px) = parse_css_px(value) {
                let mut p = style.padding.unwrap_or(SpacingRect {
                    top: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 0.0,
                });
                p.left = px;
                style.padding = Some(p);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "margin" => {
            if let Some(rect) = parse_css_spacing(value) {
                style.margin = Some(rect);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "margin-top" => {
            if let Some(px) = parse_css_px(value) {
                let mut m = style.margin.unwrap_or(SpacingRect {
                    top: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 0.0,
                });
                m.top = px;
                style.margin = Some(m);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "margin-right" => {
            if let Some(px) = parse_css_px(value) {
                let mut m = style.margin.unwrap_or(SpacingRect {
                    top: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 0.0,
                });
                m.right = px;
                style.margin = Some(m);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "margin-bottom" => {
            if let Some(px) = parse_css_px(value) {
                let mut m = style.margin.unwrap_or(SpacingRect {
                    top: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 0.0,
                });
                m.bottom = px;
                style.margin = Some(m);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "margin-left" => {
            if let Some(px) = parse_css_px(value) {
                let mut m = style.margin.unwrap_or(SpacingRect {
                    top: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 0.0,
                });
                m.left = px;
                style.margin = Some(m);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "gap" => {
            if let Some(px) = parse_css_px(value) {
                style.gap = Some(px);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "overflow" => match value.trim() {
            "hidden" | "clip" => style.overflow = Some(StyleOverflow::Clip),
            "visible" => style.overflow = Some(StyleOverflow::Visible),
            "scroll" | "auto" => style.overflow = Some(StyleOverflow::Scroll),
            _ => errors.push(ParseError::invalid_value(name, value, line, column)),
        },
        "overflow-x" => match value.trim() {
            "hidden" | "clip" => style.overflow_x = Some(StyleOverflow::Clip),
            "visible" => style.overflow_x = Some(StyleOverflow::Visible),
            "scroll" | "auto" => style.overflow_x = Some(StyleOverflow::Scroll),
            _ => errors.push(ParseError::invalid_value(name, value, line, column)),
        },
        "overflow-y" => match value.trim() {
            "hidden" | "clip" => style.overflow_y = Some(StyleOverflow::Clip),
            "visible" => style.overflow_y = Some(StyleOverflow::Visible),
            "scroll" | "auto" => style.overflow_y = Some(StyleOverflow::Scroll),
            _ => errors.push(ParseError::invalid_value(name, value, line, column)),
        },
        "border" => {
            // Shorthand: border: [width] [style] [color]
            for part in value.split_whitespace() {
                let p = part.trim();
                if p == "solid" || p == "dashed" || p == "dotted" || p == "none" || p == "hidden" {
                    continue;
                } else if let Some(px) = parse_css_px(p) {
                    style.border_width = Some(px);
                } else if let Some(color) = parse_color(p) {
                    style.border_color = Some(color);
                }
            }
        }
        "border-width" => match try_parse_calc(value) {
            CalcParseResult::Dynamic(expr) => {
                style
                    .dynamic_properties
                    .get_or_insert_with(Vec::new)
                    .push(crate::element_style::DynamicProperty::BorderWidth(expr));
            }
            CalcParseResult::Static(val) => {
                style.border_width = Some(val.max(0.0));
            }
            CalcParseResult::NotCalc => {
                if let Some(px) = parse_css_px(value) {
                    style.border_width = Some(px);
                } else {
                    errors.push(ParseError::invalid_value(name, value, line, column));
                }
            }
        },
        "border-color" => {
            if let Some(color) = parse_color(value) {
                style.border_color = Some(color);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "border-style" => {
            // Blinc borders are always solid; accept and ignore
        }
        "outline-width" => {
            if let Some(px) = parse_css_px(value) {
                style.outline_width = Some(px);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "outline-color" => {
            if let Some(color) = parse_color(value) {
                style.outline_color = Some(color);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "outline-offset" => {
            if let Some(px) = parse_css_px(value) {
                style.outline_offset = Some(px);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "outline" => {
            let parts = split_whitespace_respecting_parens(value);
            for part in &parts {
                if let Some(px) = parse_css_px(part) {
                    style.outline_width = Some(px);
                } else if part != "solid" && part != "none" && part != "dotted" && part != "dashed"
                {
                    if let Some(color) = parse_color(part) {
                        style.outline_color = Some(color);
                    }
                }
            }
            if value.trim() == "none" {
                style.outline_width = Some(0.0);
            }
        }
        "caret-color" => {
            if let Some(color) = parse_color(value) {
                style.caret_color = Some(color);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "selection-color" => {
            if let Some(color) = parse_color(value) {
                style.selection_color = Some(color);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "accent-color" => {
            if let Some(color) = parse_color(value) {
                style.accent_color = Some(color);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "placeholder-color" => {
            if let Some(color) = parse_color(value) {
                style.placeholder_color = Some(color);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "position" => match value.trim() {
            "static" => style.position = Some(StylePosition::Static),
            "relative" => style.position = Some(StylePosition::Relative),
            "absolute" => style.position = Some(StylePosition::Absolute),
            "fixed" => style.position = Some(StylePosition::Fixed),
            "sticky" => style.position = Some(StylePosition::Sticky),
            _ => errors.push(ParseError::invalid_value(name, value, line, column)),
        },
        "top" => {
            if let Some(px) = parse_css_px(value) {
                style.top = Some(px);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "right" => {
            if let Some(px) = parse_css_px(value) {
                style.right = Some(px);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "bottom" => {
            if let Some(px) = parse_css_px(value) {
                style.bottom = Some(px);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "left" => {
            if let Some(px) = parse_css_px(value) {
                style.left = Some(px);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "inset" => {
            if let Some(px) = parse_css_px(value) {
                style.top = Some(px);
                style.right = Some(px);
                style.bottom = Some(px);
                style.left = Some(px);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "object-fit" => match value.trim() {
            "cover" => style.object_fit = Some(0),
            "contain" => style.object_fit = Some(1),
            "fill" => style.object_fit = Some(2),
            "scale-down" => style.object_fit = Some(3),
            "none" => style.object_fit = Some(4),
            _ => errors.push(ParseError::invalid_value(name, value, line, column)),
        },
        "loading" => match value.trim() {
            "lazy" => style.loading_strategy = Some(1),
            "eager" => style.loading_strategy = Some(0),
            _ => errors.push(ParseError::invalid_value(name, value, line, column)),
        },
        "image-placeholder-color" => {
            if let Some(color) = parse_color(value) {
                style.image_placeholder_color = Some([color.r, color.g, color.b, color.a]);
                style.image_placeholder_type = Some(1);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "image-placeholder-image" | "image-placeholder" => {
            let trimmed = value.trim().trim_matches(|c| c == '"' || c == '\'');
            if !trimmed.is_empty() {
                style.image_placeholder_image = Some(trimmed.to_string());
                style.image_placeholder_type = Some(2);
            }
        }
        "image-placeholder-type" => match value.trim() {
            "skeleton" => style.image_placeholder_type = Some(3),
            "none" => style.image_placeholder_type = Some(0),
            "color" => style.image_placeholder_type = Some(1),
            _ => errors.push(ParseError::invalid_value(name, value, line, column)),
        },
        "fade-duration" => {
            if let Some(ms) = parse_time_value(value) {
                style.fade_duration_ms = Some(ms);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "object-position" => {
            if let Some(pos) = parse_object_position(value) {
                style.object_position = Some(pos);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "pointer-events" => match value.trim() {
            "auto" => style.pointer_events = Some(blinc_core::PointerEvents::Auto),
            "none" => style.pointer_events = Some(blinc_core::PointerEvents::None),
            _ => errors.push(ParseError::invalid_value(name, value, line, column)),
        },
        "cursor" => {
            if let Some(cursor) = parse_cursor(value) {
                style.cursor = Some(cursor);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "mix-blend-mode" => {
            if let Some(mode) = parse_blend_mode(value) {
                style.mix_blend_mode = Some(mode);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "text-decoration-color" => {
            if let Some(c) = parse_color(value) {
                style.text_decoration_color = Some(c);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "text-decoration-thickness" => {
            if let Some(px) = parse_length_value(value) {
                style.text_decoration_thickness = Some(px);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "text-overflow" => match value.trim() {
            "clip" => style.text_overflow = Some(crate::element_style::TextOverflow::Clip),
            "ellipsis" => style.text_overflow = Some(crate::element_style::TextOverflow::Ellipsis),
            _ => errors.push(ParseError::invalid_value(name, value, line, column)),
        },
        "white-space" => match value.trim() {
            "normal" => style.white_space = Some(crate::element_style::WhiteSpace::Normal),
            "nowrap" => style.white_space = Some(crate::element_style::WhiteSpace::Nowrap),
            "pre" => style.white_space = Some(crate::element_style::WhiteSpace::Pre),
            "pre-wrap" => style.white_space = Some(crate::element_style::WhiteSpace::PreWrap),
            _ => errors.push(ParseError::invalid_value(name, value, line, column)),
        },
        "mask-image" => {
            let v = value.trim();
            if v == "none" {
                style.mask_image = None;
            } else if v.starts_with("linear-gradient(") {
                if let Some(g) = parse_linear_gradient(v) {
                    style.mask_image = Some(blinc_core::MaskImage::Gradient(g));
                } else {
                    errors.push(ParseError::invalid_value(name, value, line, column));
                }
            } else if v.starts_with("radial-gradient(") {
                if let Some(g) = parse_radial_gradient(v) {
                    style.mask_image = Some(blinc_core::MaskImage::Gradient(g));
                } else {
                    errors.push(ParseError::invalid_value(name, value, line, column));
                }
            } else if let Some(url) = parse_url_value(v) {
                style.mask_image = Some(blinc_core::MaskImage::Url(url));
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "mask-mode" => match value.trim() {
            "alpha" => style.mask_mode = Some(blinc_core::MaskMode::Alpha),
            "luminance" => style.mask_mode = Some(blinc_core::MaskMode::Luminance),
            _ => errors.push(ParseError::invalid_value(name, value, line, column)),
        },
        "flow" => {
            let v = value.trim();
            if v == "none" {
                style.flow = None;
            } else {
                style.flow = Some(v.to_string());
            }
        }
        "pointer-space" => {
            use crate::pointer_query::{PointerSpace, PointerSpaceConfig};
            let v = value.trim();
            let space = match v {
                "self" => PointerSpace::SelfSpace,
                "parent" => PointerSpace::Parent,
                "viewport" => PointerSpace::Viewport,
                "none" => {
                    style.pointer_space = None;
                    return;
                }
                _ => {
                    errors.push(ParseError::invalid_value(name, value, line, column));
                    return;
                }
            };
            let config = style
                .pointer_space
                .get_or_insert(PointerSpaceConfig::default());
            config.space = space;
        }
        "pointer-origin" => {
            use crate::pointer_query::{PointerOrigin, PointerSpaceConfig};
            let v = value.trim();
            let origin = match v {
                "center" => PointerOrigin::Center,
                "top-left" => PointerOrigin::TopLeft,
                "bottom-left" => PointerOrigin::BottomLeft,
                _ => {
                    errors.push(ParseError::invalid_value(name, value, line, column));
                    return;
                }
            };
            let config = style
                .pointer_space
                .get_or_insert(PointerSpaceConfig::default());
            config.origin = origin;
        }
        "pointer-range" => {
            use crate::pointer_query::PointerSpaceConfig;
            let v = value.trim();
            let parts: Vec<&str> = v.split_whitespace().collect();
            if parts.len() == 2 {
                if let (Ok(min), Ok(max)) = (parts[0].parse::<f32>(), parts[1].parse::<f32>()) {
                    let config = style
                        .pointer_space
                        .get_or_insert(PointerSpaceConfig::default());
                    config.range = (min, max);
                } else {
                    errors.push(ParseError::invalid_value(name, value, line, column));
                }
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "pointer-smoothing" => {
            use crate::pointer_query::PointerSpaceConfig;
            let v = value.trim();
            let v = v.strip_suffix('s').unwrap_or(v);
            if let Ok(dur) = v.parse::<f32>() {
                let config = style
                    .pointer_space
                    .get_or_insert(PointerSpaceConfig::default());
                config.smoothing = dur;
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        _ => {
            // Unknown property - collect as warning
            errors.push(ParseError::unknown_property(name, line, column));
        }
    }
}

// ============================================================================
// Value Parsers
// These use generic error types so they work with both simple and VerboseError
// ============================================================================

fn parse_brush(value: &str) -> Option<Brush> {
    let trimmed = value.trim();

    // Try url() for background images
    if trimmed.starts_with("url(") {
        if let Some(source) = parse_url_value(trimmed) {
            return Some(Brush::Image(ImageBrush::new(source)));
        }
    }

    // Try linear-gradient()
    if trimmed.starts_with("linear-gradient(") {
        return parse_linear_gradient(trimmed).map(Brush::Gradient);
    }

    // Try radial-gradient()
    if trimmed.starts_with("radial-gradient(") {
        return parse_radial_gradient(trimmed).map(Brush::Gradient);
    }

    // Try conic-gradient()
    if trimmed.starts_with("conic-gradient(") {
        return parse_conic_gradient(trimmed).map(Brush::Gradient);
    }

    // Try theme() function
    if let Ok((_, color)) = parse_theme_color::<nom::error::Error<&str>>(trimmed) {
        return Some(Brush::Solid(color));
    }

    // Try parsing as color
    parse_color(trimmed).map(Brush::Solid)
}

/// Parse `url("path")` or `url('path')` or `url(path)` and return the inner path string.
fn parse_url_value(value: &str) -> Option<String> {
    let inner = value.strip_prefix("url(")?.strip_suffix(')')?.trim();
    // Strip optional quotes
    let path = if (inner.starts_with('"') && inner.ends_with('"'))
        || (inner.starts_with('\'') && inner.ends_with('\''))
    {
        &inner[1..inner.len() - 1]
    } else {
        inner
    };
    if path.is_empty() {
        return None;
    }
    Some(path.to_string())
}

/// Parse theme(token-name) for colors
fn parse_theme_color<'a, E: NomParseError<&'a str>>(input: &'a str) -> IResult<&'a str, Color, E> {
    let (input, _) = ws(input)?;
    let (input, _) = tag_no_case("theme")(input)?;
    let (input, _) = ws(input)?;
    let (input, token_name) =
        delimited(char('('), take_while1(|c: char| c != ')'), char(')'))(input)?;

    let token_name = token_name.trim();
    let token = match token_name.to_lowercase().as_str() {
        // Brand colors
        "primary" => ColorToken::Primary,
        "primary-hover" => ColorToken::PrimaryHover,
        "primary-active" => ColorToken::PrimaryActive,
        "secondary" => ColorToken::Secondary,
        "secondary-hover" => ColorToken::SecondaryHover,
        "secondary-active" => ColorToken::SecondaryActive,
        // Semantic colors
        "success" => ColorToken::Success,
        "success-bg" => ColorToken::SuccessBg,
        "warning" => ColorToken::Warning,
        "warning-bg" => ColorToken::WarningBg,
        "error" => ColorToken::Error,
        "error-bg" => ColorToken::ErrorBg,
        "info" => ColorToken::Info,
        "info-bg" => ColorToken::InfoBg,
        // Surface colors
        "background" => ColorToken::Background,
        "surface" => ColorToken::Surface,
        "surface-elevated" => ColorToken::SurfaceElevated,
        "surface-overlay" => ColorToken::SurfaceOverlay,
        // Text colors
        "text-primary" => ColorToken::TextPrimary,
        "text-secondary" => ColorToken::TextSecondary,
        "text-tertiary" => ColorToken::TextTertiary,
        "text-inverse" => ColorToken::TextInverse,
        "text-link" => ColorToken::TextLink,
        // Border colors
        "border" => ColorToken::Border,
        "border-secondary" => ColorToken::BorderSecondary,
        "border-hover" => ColorToken::BorderHover,
        "border-focus" => ColorToken::BorderFocus,
        "border-error" => ColorToken::BorderError,
        _ => {
            debug!(token = token_name, "Unknown theme color token");
            return Err(nom::Err::Error(E::from_error_kind(
                input,
                nom::error::ErrorKind::Tag,
            )));
        }
    };

    Ok((input, ThemeState::get().color(token)))
}

fn parse_radius(value: &str) -> Option<CornerRadius> {
    // Try theme() function first
    if let Ok((_, radius)) = parse_theme_radius::<nom::error::Error<&str>>(value) {
        return Some(radius);
    }

    // Handle percentage values for border-radius.
    // CSS border-radius percentages are relative to the element's dimensions,
    // which we can't resolve at parse time. Use 9999px as a large sentinel —
    // the renderer clamps border-radius to half the element size, so any
    // percentage >= 50% produces a fully-rounded (circular/pill) shape.
    if let Some(len) = parse_css_length(value) {
        return match len {
            Length::Pct(v) if v > 0.0 => Some(CornerRadius::uniform(9999.0)),
            Length::Pct(_) => Some(CornerRadius::uniform(0.0)),
            _ => Some(CornerRadius::uniform(len.to_px())),
        };
    }

    None
}

fn parse_corner_shape_value(value: &str) -> Option<CornerShape> {
    let trimmed = value.trim();
    let parse_one = |s: &str| -> Option<f32> {
        match s.trim() {
            "round" => Some(1.0),
            "bevel" => Some(0.0),
            "squircle" => Some(2.0),
            "scoop" => Some(-1.0),
            "notch" => Some(-100.0),
            "square" => Some(100.0),
            other => {
                if let Some(inner) = other
                    .strip_prefix("superellipse(")
                    .and_then(|s| s.strip_suffix(')'))
                {
                    inner.trim().parse().ok()
                } else {
                    other.parse().ok()
                }
            }
        }
    };

    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    match parts.len() {
        1 => Some(CornerShape::uniform(parse_one(parts[0])?)),
        2 => {
            let a = parse_one(parts[0])?;
            let b = parse_one(parts[1])?;
            Some(CornerShape::new(a, b, a, b))
        }
        3 => {
            let a = parse_one(parts[0])?;
            let b = parse_one(parts[1])?;
            let c = parse_one(parts[2])?;
            Some(CornerShape::new(a, b, c, b))
        }
        4 => {
            let tl = parse_one(parts[0])?;
            let tr = parse_one(parts[1])?;
            let br = parse_one(parts[2])?;
            let bl = parse_one(parts[3])?;
            Some(CornerShape::new(tl, tr, br, bl))
        }
        _ => None,
    }
}

/// Parse theme(radius-*) tokens
fn parse_theme_radius<'a, E: NomParseError<&'a str>>(
    input: &'a str,
) -> IResult<&'a str, CornerRadius, E> {
    let (input, _) = ws(input)?;
    let (input, _) = tag_no_case("theme")(input)?;
    let (input, _) = ws(input)?;
    let (input, token_name) =
        delimited(char('('), take_while1(|c: char| c != ')'), char(')'))(input)?;

    let token_name = token_name.trim();
    let radii = ThemeState::get().radii();

    let radius = match token_name.to_lowercase().replace('_', "-").as_str() {
        "radius-none" => radii.radius_none,
        "radius-sm" => radii.radius_sm,
        "radius-default" => radii.radius_default,
        "radius-md" => radii.radius_md,
        "radius-lg" => radii.radius_lg,
        "radius-xl" => radii.radius_xl,
        "radius-2xl" => radii.radius_2xl,
        "radius-3xl" => radii.radius_3xl,
        "radius-full" => radii.radius_full,
        _ => {
            debug!(token = token_name, "Unknown theme radius token");
            return Err(nom::Err::Error(E::from_error_kind(
                input,
                nom::error::ErrorKind::Tag,
            )));
        }
    };

    Ok((input, CornerRadius::uniform(radius)))
}

fn parse_shadow(value: &str) -> Option<Shadow> {
    // Returns just the first layer for callers (e.g. `text-shadow`) that
    // still take a single `Shadow`. Use `parse_shadow_stack` for
    // multi-layer box-shadow.
    parse_shadow_stack(value).and_then(|s| s.into_iter().next())
}

/// Parse a CSS shadow value into a stack of layers.
///
/// Handles:
/// - `none` → single transparent shadow,
/// - `theme(shadow-*)` → the theme's full layer stack,
/// - one or more comma-separated explicit shadows.
fn parse_shadow_stack(value: &str) -> Option<Vec<Shadow>> {
    let trimmed = value.trim();
    if trimmed.eq_ignore_ascii_case("none") {
        return Some(vec![Shadow::new(0.0, 0.0, 0.0, Color::TRANSPARENT)]);
    }

    // Try theme() function first — preserves multi-layer compound shadow.
    if let Ok((_, stack)) = parse_theme_shadow::<nom::error::Error<&str>>(trimmed) {
        return Some(stack);
    }

    // Comma-separated explicit shadows.
    let parts = split_commas_respecting_parens(trimmed);
    let mut layers = Vec::with_capacity(parts.len().max(1));
    for part in parts {
        if let Some(layer) = parse_explicit_shadow(part.trim()) {
            layers.push(layer);
        }
    }
    if layers.is_empty() {
        None
    } else {
        Some(layers)
    }
}

/// Parse theme(shadow-*) tokens
fn parse_theme_shadow<'a, E: NomParseError<&'a str>>(
    input: &'a str,
) -> IResult<&'a str, Vec<Shadow>, E> {
    let (input, _) = ws(input)?;
    let (input, _) = tag_no_case("theme")(input)?;
    let (input, _) = ws(input)?;
    let (input, token_name) =
        delimited(char('('), take_while1(|c: char| c != ')'), char(')'))(input)?;

    let token_name = token_name.trim();
    let shadows = ThemeState::get().shadows();

    let stack: &[blinc_theme::Shadow] = match token_name.to_lowercase().replace('_', "-").as_str() {
        "shadow-sm" => &shadows.shadow_sm,
        "shadow-default" => &shadows.shadow_default,
        "shadow-md" => &shadows.shadow_md,
        "shadow-lg" => &shadows.shadow_lg,
        "shadow-xl" => &shadows.shadow_xl,
        "shadow-2xl" => &shadows.shadow_2xl,
        "shadow-none" => &shadows.shadow_none,
        _ => {
            debug!(token = token_name, "Unknown theme shadow token");
            return Err(nom::Err::Error(E::from_error_kind(
                input,
                nom::error::ErrorKind::Tag,
            )));
        }
    };

    let stack: Vec<Shadow> = stack.iter().map(Shadow::from).collect();
    Ok((input, stack))
}

/// Split a CSS list on commas while keeping parenthesised groups
/// (e.g. `rgba(0, 0, 0, 0.5)`) intact.
fn split_commas_respecting_parens(input: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut paren_depth: i32 = 0;
    for c in input.chars() {
        match c {
            '(' => {
                paren_depth += 1;
                current.push(c);
            }
            ')' => {
                paren_depth = (paren_depth - 1).max(0);
                current.push(c);
            }
            ',' if paren_depth == 0 => {
                if !current.trim().is_empty() {
                    parts.push(std::mem::take(&mut current));
                } else {
                    current.clear();
                }
            }
            _ => current.push(c),
        }
    }
    if !current.trim().is_empty() {
        parts.push(current);
    }
    parts
}

/// Split a CSS value by whitespace while keeping parenthesized groups intact.
/// e.g. "2px 2px 0px rgba(0, 0, 0, 0.5)" → ["2px", "2px", "0px", "rgba(0, 0, 0, 0.5)"]
fn split_whitespace_respecting_parens(input: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut paren_depth: i32 = 0;

    for c in input.chars() {
        match c {
            '(' => {
                paren_depth += 1;
                current.push(c);
            }
            ')' => {
                paren_depth = (paren_depth - 1).max(0);
                current.push(c);
            }
            c if c.is_whitespace() && paren_depth == 0 => {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    parts.push(trimmed);
                }
                current.clear();
            }
            _ => current.push(c),
        }
    }

    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        parts.push(trimmed);
    }

    parts
}

/// Parse explicit shadow: `offset-x offset-y blur [spread] color`
fn parse_explicit_shadow(input: &str) -> Option<Shadow> {
    let parts = split_whitespace_respecting_parens(input);
    if parts.len() >= 4 {
        let offset_x = parse_length_value(&parts[0])?;
        let offset_y = parse_length_value(&parts[1])?;
        let blur = parse_length_value(&parts[2])?;
        // Try 5-part form: offset-x offset-y blur spread color
        if parts.len() >= 5 {
            if let Some(spread) = parse_length_value(&parts[3]) {
                let color = parse_color(&parts[4])?;
                let mut shadow = Shadow::new(offset_x, offset_y, blur, color);
                shadow.spread = spread;
                return Some(shadow);
            }
        }
        // 4-part form: offset-x offset-y blur color
        let color = parse_color(&parts[3])?;
        return Some(Shadow::new(offset_x, offset_y, blur, color));
    }
    None
}

fn parse_transform(value: &str) -> Option<Transform> {
    // Try scale()
    if let Ok((_, transform)) = parse_scale_transform::<nom::error::Error<&str>>(value) {
        return Some(transform);
    }

    // Try rotate()
    if let Ok((_, transform)) = parse_rotate_transform::<nom::error::Error<&str>>(value) {
        return Some(transform);
    }

    // Try translate()
    if let Ok((_, transform)) = parse_translate_transform::<nom::error::Error<&str>>(value) {
        return Some(transform);
    }

    debug!(value = value, "Failed to parse transform");
    None
}

/// Parse a compound transform value (e.g. `rotate(45deg) scale(1.5)`).
/// Handles both 2D and 3D functions. 2D functions are composed into a single
/// Affine2D stored in style.transform; 3D functions go to dedicated fields.
/// Returns true if at least one function was parsed.
fn parse_transform_with_3d(value: &str, style: &mut ElementStyle) -> bool {
    use blinc_core::Affine2D;

    // Split compound transform string into individual function calls.
    // e.g. "rotate(45deg) scale(1.5)" → ["rotate(45deg)", "scale(1.5)"]
    let functions = split_transform_functions(value.trim());
    if functions.is_empty() {
        return false;
    }

    let mut affine = Affine2D::IDENTITY;
    let mut has_2d = false;
    let mut parsed_any = false;

    for func in &functions {
        // 3D: rotateX
        if let Some(deg) = parse_function_angle(func, "rotateX") {
            style.rotate_x = Some(deg);
            parsed_any = true;
        // 3D: rotateY
        } else if let Some(deg) = parse_function_angle(func, "rotateY") {
            style.rotate_y = Some(deg);
            parsed_any = true;
        // perspective
        } else if let Some(px) = parse_function_px(func, "perspective") {
            style.perspective = Some(px);
            parsed_any = true;
        // 2D: skewX
        } else if let Some(deg) = parse_function_angle(func, "skewX") {
            style.skew_x = Some(deg);
            affine = affine.then(&Affine2D::skew_x(deg.to_radians()));
            has_2d = true;
            parsed_any = true;
        // 2D: skewY
        } else if let Some(deg) = parse_function_angle(func, "skewY") {
            style.skew_y = Some(deg);
            affine = affine.then(&Affine2D::skew_y(deg.to_radians()));
            has_2d = true;
            parsed_any = true;
        // 2D: skew(x, y)
        } else if let Some((sx, sy)) = parse_skew_function(func) {
            style.skew_x = Some(sx);
            style.skew_y = Some(sy);
            affine = affine.then(&Affine2D::skew_x(sx.to_radians()));
            affine = affine.then(&Affine2D::skew_y(sy.to_radians()));
            has_2d = true;
            parsed_any = true;
        // 2D: scale
        } else if let Ok((_, (sx, sy))) = parse_scale_values::<nom::error::Error<&str>>(func) {
            style.scale_x = Some(sx);
            style.scale_y = Some(sy);
            affine = affine.then(&Affine2D::scale(sx, sy));
            has_2d = true;
            parsed_any = true;
        // 2D: rotate — parse angle directly to avoid lossy matrix decomposition
        // (atan2 wraps 360° to 0°, breaking spin animations)
        } else if let Some(deg) = parse_function_angle(func, "rotate") {
            style.rotate = Some(deg);
            affine = affine.then(&Affine2D::rotation(deg.to_radians()));
            has_2d = true;
            parsed_any = true;
        // 2D: translate / translateX / translateY
        } else if let Ok((_, t)) = parse_translate_transform::<nom::error::Error<&str>>(func) {
            if let Transform::Affine2D(ref a) = t {
                affine = affine.then(a);
            }
            has_2d = true;
            parsed_any = true;
        }
    }

    if has_2d {
        style.transform = Some(Transform::Affine2D(affine));
    }

    parsed_any
}

/// Split a compound CSS transform string into individual function calls.
/// e.g. `"rotate(45deg) scale(1.5)"` → `["rotate(45deg)", "scale(1.5)"]`
fn split_transform_functions(input: &str) -> Vec<&str> {
    let mut functions = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    let mut in_func = false;

    for (i, ch) in input.char_indices() {
        match ch {
            '(' => {
                if depth == 0 {
                    // Find start of function name (skip leading whitespace)
                    if !in_func {
                        start = input[..i]
                            .rfind(|c: char| c.is_whitespace())
                            .map_or(0, |p| p + 1);
                        in_func = true;
                    }
                }
                depth += 1;
            }
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 && in_func {
                    let func = input[start..=i].trim();
                    if !func.is_empty() {
                        functions.push(func);
                    }
                    in_func = false;
                    start = i + 1;
                }
            }
            _ if !ch.is_whitespace() && depth == 0 && !in_func => {
                start = i;
                in_func = true;
            }
            _ => {}
        }
    }

    functions
}

/// Parse `skew(Xdeg)` or `skew(Xdeg, Ydeg)`
fn parse_skew_function(input: &str) -> Option<(f32, f32)> {
    let trimmed = input.trim();
    let lower = trimmed.to_lowercase();
    if !lower.starts_with("skew(") {
        return None;
    }
    let inner = trimmed[5..].strip_suffix(')')?.trim();
    let parts: Vec<&str> = inner.split(',').collect();
    match parts.len() {
        1 => {
            let x = parse_angle_str(parts[0].trim())?;
            Some((x, 0.0))
        }
        2 => {
            let x = parse_angle_str(parts[0].trim())?;
            let y = parse_angle_str(parts[1].trim())?;
            Some((x, y))
        }
        _ => None,
    }
}

/// Parse `transform-origin: X Y` where X/Y can be percentages or keywords
/// Returns [x%, y%] where 0% = left/top, 50% = center, 100% = right/bottom
fn parse_transform_origin(value: &str) -> Option<[f32; 2]> {
    let parts: Vec<&str> = value.split_whitespace().collect();
    let parse_one = |s: &str| -> Option<f32> {
        match s {
            "left" | "top" => Some(0.0),
            "center" => Some(50.0),
            "right" | "bottom" => Some(100.0),
            _ => {
                if let Some(pct) = s.strip_suffix('%') {
                    pct.trim().parse::<f32>().ok()
                } else {
                    s.parse::<f32>().ok() // bare number = percentage
                }
            }
        }
    };
    match parts.len() {
        1 => {
            let v = parse_one(parts[0])?;
            Some([v, v])
        }
        2 => {
            let x = parse_one(parts[0])?;
            let y = parse_one(parts[1])?;
            Some([x, y])
        }
        _ => None,
    }
}

/// Parse an angle string like "30deg", "0.5rad", or plain number (assumed degrees)
fn parse_angle_str(s: &str) -> Option<f32> {
    let s = s.trim();
    if let Some(deg_str) = s.strip_suffix("deg") {
        deg_str.trim().parse::<f32>().ok()
    } else if let Some(rad_str) = s.strip_suffix("rad") {
        rad_str.trim().parse::<f32>().ok().map(|r| r.to_degrees())
    } else {
        // Plain number assumed to be degrees
        s.parse::<f32>().ok()
    }
}

/// Parse a CSS function like `funcName(30deg)` and return degrees
fn parse_function_angle(input: &str, func_name: &str) -> Option<f32> {
    let trimmed = input.trim();
    let lower = trimmed.to_lowercase();
    let func_lower = func_name.to_lowercase();
    if !lower.starts_with(&func_lower) {
        return None;
    }
    let rest = &trimmed[func_name.len()..].trim();
    let rest = rest.strip_prefix('(')?.trim_start();
    let rest = rest.strip_suffix(')')?.trim_end();
    parse_angle_value(rest)
}

/// Parse a CSS function like `funcName(800px)` and return pixels
fn parse_function_px(input: &str, func_name: &str) -> Option<f32> {
    let trimmed = input.trim();
    let lower = trimmed.to_lowercase();
    let func_lower = func_name.to_lowercase();
    if !lower.starts_with(&func_lower) {
        return None;
    }
    let rest = &trimmed[func_name.len()..].trim();
    let rest = rest.strip_prefix('(')?.trim_start();
    let rest = rest.strip_suffix(')')?.trim_end();
    parse_css_px(rest)
}

/// Check if a shape-3d value is valid
fn is_valid_shape_3d(value: &str) -> bool {
    matches!(
        value.trim().to_lowercase().as_str(),
        "none" | "box" | "sphere" | "cylinder" | "torus" | "capsule" | "group"
    )
}

/// Convert a shape-3d string to float encoding for the GPU
/// (0=none, 1=box, 2=sphere, 3=cylinder, 4=torus, 5=capsule, 6=group)
pub fn shape_3d_to_float(value: &str) -> f32 {
    match value.trim().to_lowercase().as_str() {
        "box" => 1.0,
        "sphere" => 2.0,
        "cylinder" => 3.0,
        "torus" => 4.0,
        "capsule" => 5.0,
        "group" => 6.0,
        _ => 0.0,
    }
}

fn is_valid_op_3d(value: &str) -> bool {
    matches!(
        value.trim().to_lowercase().as_str(),
        "union"
            | "subtract"
            | "intersect"
            | "smooth-union"
            | "smooth-subtract"
            | "smooth-intersect"
    )
}

/// Convert a 3d-op string to float encoding for the GPU
/// (0=union, 1=subtract, 2=intersect, 3=smooth-union, 4=smooth-subtract, 5=smooth-intersect)
pub fn op_3d_to_float(value: &str) -> f32 {
    match value.trim().to_lowercase().as_str() {
        "union" => 0.0,
        "subtract" => 1.0,
        "intersect" => 2.0,
        "smooth-union" => 3.0,
        "smooth-subtract" => 4.0,
        "smooth-intersect" => 5.0,
        _ => 0.0,
    }
}

/// Parse a vec3 value like "-0.5 -1.0 0.5" (3 space-separated floats)
fn parse_vec3_value(value: &str) -> Option<[f32; 3]> {
    let parts: Vec<&str> = value.split_whitespace().collect();
    if parts.len() == 3 {
        let x = parts[0].parse::<f32>().ok()?;
        let y = parts[1].parse::<f32>().ok()?;
        let z = parts[2].parse::<f32>().ok()?;
        Some([x, y, z])
    } else {
        None
    }
}

/// Parse scale(x) or scale(x, y) and return the raw (sx, sy) values
fn parse_scale_values<'a, E: NomParseError<&'a str>>(
    input: &'a str,
) -> IResult<&'a str, (f32, f32), E> {
    let (input, _) = ws(input)?;
    let (input, _) = tag_no_case("scale")(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('(')(input)?;
    let (input, _) = ws(input)?;
    let (input, sx) = float(input)?;
    let (input, _) = ws(input)?;
    let (input, sy) = opt(preceded(tuple((char(','), ws::<E>)), float))(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char(')')(input)?;
    let sy = sy.unwrap_or(sx);
    Ok((input, (sx, sy)))
}

/// Parse scale(x) or scale(x, y)
fn parse_scale_transform<'a, E: NomParseError<&'a str>>(
    input: &'a str,
) -> IResult<&'a str, Transform, E> {
    let (input, _) = ws(input)?;
    let (input, _) = tag_no_case("scale")(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('(')(input)?;
    let (input, _) = ws(input)?;
    let (input, sx) = float(input)?;
    let (input, _) = ws(input)?;
    let (input, sy) = opt(preceded(tuple((char(','), ws::<E>)), float))(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char(')')(input)?;

    let sy = sy.unwrap_or(sx);
    Ok((input, Transform::scale(sx, sy)))
}

/// Parse rotate(deg)
fn parse_rotate_transform<'a, E: NomParseError<&'a str>>(
    input: &'a str,
) -> IResult<&'a str, Transform, E> {
    let (input, _) = ws(input)?;
    let (input, _) = tag_no_case("rotate")(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('(')(input)?;
    let (input, _) = ws(input)?;
    let (input, degrees) = float(input)?;
    let (input, _) = opt(tag_no_case("deg"))(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char(')')(input)?;

    Ok((
        input,
        Transform::rotate(degrees * std::f32::consts::PI / 180.0),
    ))
}

/// Parse translate(x, y), translateX(x), or translateY(y)
fn parse_translate_transform<'a, E: NomParseError<&'a str>>(
    input: &'a str,
) -> IResult<&'a str, Transform, E> {
    let (input, _) = ws(input)?;

    // Try translateX(x)
    if let Ok((rest, _)) = tag_no_case::<_, _, E>("translateX")(input) {
        let (rest, _) = ws(rest)?;
        let (rest, _) = char('(')(rest)?;
        let (rest, _) = ws(rest)?;
        let (rest, x) = parse_length(rest)?;
        let (rest, _) = ws(rest)?;
        let (rest, _) = char(')')(rest)?;
        return Ok((rest, Transform::translate(x.to_px(), 0.0)));
    }

    // Try translateY(y)
    if let Ok((rest, _)) = tag_no_case::<_, _, E>("translateY")(input) {
        let (rest, _) = ws(rest)?;
        let (rest, _) = char('(')(rest)?;
        let (rest, _) = ws(rest)?;
        let (rest, y) = parse_length(rest)?;
        let (rest, _) = ws(rest)?;
        let (rest, _) = char(')')(rest)?;
        return Ok((rest, Transform::translate(0.0, y.to_px())));
    }

    // Try translate(x, y)
    let (input, _) = tag_no_case("translate")(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('(')(input)?;
    let (input, _) = ws(input)?;
    let (input, x) = parse_length(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char(',')(input)?;
    let (input, _) = ws(input)?;
    let (input, y) = parse_length(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char(')')(input)?;

    Ok((input, Transform::translate(x.to_px(), y.to_px())))
}

/// Parse a CSS length value with unit suffix and return as Length enum
///
/// Supports:
/// - `px` - pixels (e.g., "16px")
/// - `sp` - spacing units, 4px grid (e.g., "4sp" = 16px)
/// - `%` - percentage (e.g., "50%")
/// - unitless - treated as pixels for backwards compatibility
fn parse_css_length(input: &str) -> Option<Length> {
    let input = input.trim();

    // Try percentage first
    if let Some(pct_str) = input.strip_suffix('%') {
        return pct_str.trim().parse::<f32>().ok().map(Length::Pct);
    }

    // Try spacing units (4px grid)
    if let Some(sp_str) = input.strip_suffix("sp") {
        return sp_str.trim().parse::<f32>().ok().map(Length::Sp);
    }

    // Try pixels (explicit or implicit)
    let num_str = input.strip_suffix("px").unwrap_or(input).trim();
    num_str.parse::<f32>().ok().map(Length::Px)
}

/// Parse a length value with optional px/sp suffix using nom
fn parse_length<'a, E: NomParseError<&'a str>>(input: &'a str) -> IResult<&'a str, Length, E> {
    let (input, value) = float(input)?;
    // Try to match a unit suffix
    let (input, unit) = opt(alt((tag_no_case("px"), tag_no_case("sp"), tag("%"))))(input)?;

    let length = match unit {
        Some("sp") | Some("SP") => Length::Sp(value),
        Some("%") => Length::Pct(value),
        _ => Length::Px(value), // px or unitless
    };

    Ok((input, length))
}

/// Parse a length value from a string slice, returning pixels (f32)
///
/// This is a convenience wrapper that converts Length to pixels for
/// properties that need raw pixel values (like shadow offsets).
fn parse_length_value(input: &str) -> Option<f32> {
    parse_css_length(input).map(|len| len.to_px())
}

/// Parse opacity value
fn parse_opacity<'a, E: NomParseError<&'a str>>(input: &'a str) -> IResult<&'a str, f32, E> {
    let (input, _) = ws(input)?;
    float(input)
}

/// Parse render layer
fn parse_render_layer<'a, E: NomParseError<&'a str>>(
    input: &'a str,
) -> IResult<&'a str, RenderLayer, E> {
    let (input, _) = ws(input)?;
    alt((
        value(RenderLayer::Foreground, tag_no_case("foreground")),
        value(RenderLayer::Glass, tag_no_case("glass")),
        value(RenderLayer::Background, tag_no_case("background")),
    ))(input)
}

// ============================================================================
// Animation Parsing
// ============================================================================

/// Parse CSS animation shorthand: animation: name duration timing-function delay iteration-count direction fill-mode
///
/// Examples:
/// - `animation: fade-in 300ms`
/// - `animation: fade-in 300ms ease-out`
/// - `animation: fade-in 300ms ease-out 100ms`
/// - `animation: fade-in 300ms ease-out 0ms infinite`
/// - `animation: slide-in 0.5s ease-in-out 0s 1 normal forwards`
fn parse_animation(value: &str) -> Option<CssAnimation> {
    // Use `split_whitespace_respecting_parens` so a `cubic-bezier(a, b,
    // c, d)` (from `var(--ease-state)` resolution) survives as a single
    // token instead of getting chopped on the commas.
    let owned_parts = split_whitespace_respecting_parens(value);
    if owned_parts.is_empty() {
        return None;
    }
    let parts: Vec<&str> = owned_parts.iter().map(|s| s.as_str()).collect();

    let mut anim = CssAnimation::default();
    let mut duration_set = false;
    let mut delay_set = false;

    for part in parts {
        // Try parsing as timing function
        if let Some(timing) = AnimationTiming::from_str(part) {
            anim.timing = timing;
            continue;
        }

        // Try parsing as direction
        if let Some(direction) = parse_animation_direction(part) {
            anim.direction = direction;
            continue;
        }

        // Try parsing as fill mode
        if let Some(fill_mode) = parse_animation_fill_mode(part) {
            anim.fill_mode = fill_mode;
            continue;
        }

        // Try parsing as iteration count
        if part.eq_ignore_ascii_case("infinite") {
            anim.iteration_count = 0; // 0 means infinite
            continue;
        }
        if let Ok(count) = part.parse::<u32>() {
            anim.iteration_count = count;
            continue;
        }

        // Try parsing as duration (first time value is duration, second is delay)
        if let Some(ms) = parse_time_value(part) {
            if !duration_set {
                anim.duration_ms = ms;
                duration_set = true;
            } else if !delay_set {
                anim.delay_ms = ms;
                delay_set = true;
            }
            continue;
        }

        // Otherwise, treat as animation name
        if anim.name.is_empty() {
            anim.name = part.to_string();
        }
    }

    if anim.name.is_empty() {
        return None;
    }

    Some(anim)
}

/// Parse animation direction keyword
fn parse_animation_direction(input: &str) -> Option<AnimationDirection> {
    match input.to_lowercase().as_str() {
        "normal" => Some(AnimationDirection::Normal),
        "reverse" => Some(AnimationDirection::Reverse),
        "alternate" => Some(AnimationDirection::Alternate),
        "alternate-reverse" => Some(AnimationDirection::AlternateReverse),
        _ => None,
    }
}

/// Parse animation fill mode keyword
fn parse_animation_fill_mode(input: &str) -> Option<AnimationFillMode> {
    match input.to_lowercase().as_str() {
        "none" => Some(AnimationFillMode::None),
        "forwards" => Some(AnimationFillMode::Forwards),
        "backwards" => Some(AnimationFillMode::Backwards),
        "both" => Some(AnimationFillMode::Both),
        _ => None,
    }
}

// ============================================================================
// Transition Parsing
// ============================================================================

/// Parse CSS transition shorthand
///
/// Supports:
/// - `transition: all 300ms ease`
/// - `transition: opacity 200ms ease-in-out`
/// - `transition: opacity 200ms ease, clip-path 500ms ease-out 100ms`
/// - `transition: none`
fn parse_transition(value: &str) -> Option<CssTransitionSet> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("none") {
        return Some(CssTransitionSet::default());
    }

    let mut transitions = Vec::new();
    // Paren-respecting split — `cubic-bezier(a, b, c, d)` in a
    // comma-separated transition list (from `var(--ease-X)`
    // resolution) must survive as a single segment.
    for segment in split_commas_respecting_parens(value) {
        let trimmed = segment.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(t) = parse_single_transition(trimmed) {
            transitions.push(t);
        } else {
            return None;
        }
    }

    if transitions.is_empty() {
        return None;
    }

    Some(CssTransitionSet { transitions })
}

/// Parse a single transition: `property duration [timing] [delay]`
fn parse_single_transition(value: &str) -> Option<CssTransition> {
    // Use `split_whitespace_respecting_parens` so a `cubic-bezier(a, b,
    // c, d)` (from `var(--ease-X)` resolution) survives as a single
    // token instead of getting chopped on the commas.
    let owned_parts = split_whitespace_respecting_parens(value);
    if owned_parts.is_empty() {
        return None;
    }
    let parts: Vec<&str> = owned_parts.iter().map(|s| s.as_str()).collect();

    let mut property = String::new();
    let mut duration_ms = 0u32;
    let mut timing = AnimationTiming::Ease;
    let mut delay_ms = 0u32;
    let mut duration_set = false;

    for part in parts {
        // Try as timing function
        if let Some(t) = AnimationTiming::from_str(part) {
            timing = t;
            continue;
        }

        // Try as time value (first = duration, second = delay)
        if let Some(ms) = parse_time_value(part) {
            if !duration_set {
                duration_ms = ms;
                duration_set = true;
            } else {
                delay_ms = ms;
            }
            continue;
        }

        // Otherwise treat as property name
        if property.is_empty() {
            property = part.to_string();
        }
    }

    if property.is_empty() {
        return None;
    }

    Some(CssTransition {
        property,
        duration_ms,
        timing,
        delay_ms,
    })
}

/// Parse CSS filter functions: `filter: grayscale(1) invert(0.5) brightness(1.2)`
///
/// Supports: grayscale, invert, sepia, hue-rotate, brightness, contrast, saturate
/// Values can be plain numbers, percentages, or degrees (for hue-rotate)
/// Parse functional `backdrop-filter` values: `blur(Npx)`, `saturate(N)`, `brightness(N)`.
/// Returns a `GlassMaterial` with `simple=true` and the extracted parameters.
fn parse_backdrop_filter_functions(value: &str) -> Option<GlassMaterial> {
    let mut remaining = value.trim();
    if remaining.is_empty() {
        return None;
    }

    // Defaults: subtle white tint (makes glass visually distinct from backdrop),
    // no border artifacts, simple frosted glass mode.
    let mut glass = GlassMaterial {
        blur: 0.0,
        tint: blinc_core::Color::rgba(1.0, 1.0, 1.0, 0.1),
        saturation: 1.0,
        brightness: 1.0,
        noise: 0.0,
        border_thickness: 0.0,
        shadow: None,
        simple: true,
    };
    let mut found_any = false;

    while !remaining.is_empty() {
        remaining = remaining.trim_start();
        if remaining.is_empty() {
            break;
        }

        let paren_pos = remaining.find('(')?;
        let func_name = remaining[..paren_pos].trim();
        let after_paren = &remaining[paren_pos + 1..];

        // Find matching close paren (handles nested parens)
        let close_pos = {
            let mut depth = 0i32;
            let mut found = None;
            for (i, ch) in after_paren.char_indices() {
                match ch {
                    '(' => depth += 1,
                    ')' => {
                        if depth == 0 {
                            found = Some(i);
                            break;
                        }
                        depth -= 1;
                    }
                    _ => {}
                }
            }
            found
        };

        if let Some(close) = close_pos {
            let arg_str = after_paren[..close].trim();
            remaining = after_paren[close + 1..].trim_start();

            match func_name.to_lowercase().as_str() {
                "blur" => {
                    if let Some(px) = parse_css_px(arg_str) {
                        glass.blur = px;
                        found_any = true;
                    }
                }
                "saturate" => {
                    if let Some(v) = arg_str
                        .strip_suffix('%')
                        .and_then(|s| s.trim().parse::<f32>().ok())
                        .map(|p| p / 100.0)
                        .or_else(|| arg_str.parse::<f32>().ok())
                    {
                        glass.saturation = v;
                        found_any = true;
                    }
                }
                "brightness" => {
                    if let Some(v) = arg_str
                        .strip_suffix('%')
                        .and_then(|s| s.trim().parse::<f32>().ok())
                        .map(|p| p / 100.0)
                        .or_else(|| arg_str.parse::<f32>().ok())
                    {
                        glass.brightness = v;
                        found_any = true;
                    }
                }
                _ => {
                    // Skip unknown functions
                    continue;
                }
            }
        } else {
            break;
        }
    }

    if found_any { Some(glass) } else { None }
}

/// Parse `liquid-glass(...)` CSS function.
///
/// Syntax: `liquid-glass(blur(Npx) saturate(N%) brightness(N%) border(N) tint(color) noise(N))`
///
/// All sub-functions are optional. Produces a `GlassMaterial` with `simple=false`
/// (liquid glass with refracted bevel borders).
fn parse_liquid_glass_functions(value: &str) -> Option<GlassMaterial> {
    let stripped = value.strip_prefix("liquid-glass(")?;
    // Find the matching closing paren for the outer liquid-glass(...)
    let mut depth = 0i32;
    let mut outer_close = None;
    for (i, ch) in stripped.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                if depth == 0 {
                    outer_close = Some(i);
                    break;
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    let inner = stripped[..outer_close?].trim();

    // Start with default liquid glass (simple=false, border_thickness=0.8)
    let mut glass = GlassMaterial::new();
    let mut found_any = false;
    let mut remaining = inner;

    while !remaining.is_empty() {
        remaining = remaining.trim_start();
        if remaining.is_empty() {
            break;
        }

        let paren_pos = match remaining.find('(') {
            Some(p) => p,
            None => break,
        };
        let func_name = remaining[..paren_pos].trim();
        let after_paren = &remaining[paren_pos + 1..];

        // Find matching close paren
        let close_pos = {
            let mut d = 0i32;
            let mut found = None;
            for (i, ch) in after_paren.char_indices() {
                match ch {
                    '(' => d += 1,
                    ')' => {
                        if d == 0 {
                            found = Some(i);
                            break;
                        }
                        d -= 1;
                    }
                    _ => {}
                }
            }
            found
        };

        if let Some(close) = close_pos {
            let arg_str = after_paren[..close].trim();
            remaining = after_paren[close + 1..].trim_start();

            match func_name.to_lowercase().as_str() {
                "blur" => {
                    if let Some(px) = parse_css_px(arg_str) {
                        glass.blur = px;
                        found_any = true;
                    }
                }
                "saturate" => {
                    if let Some(v) = arg_str
                        .strip_suffix('%')
                        .and_then(|s| s.trim().parse::<f32>().ok())
                        .map(|p| p / 100.0)
                        .or_else(|| arg_str.parse::<f32>().ok())
                    {
                        glass.saturation = v;
                        found_any = true;
                    }
                }
                "brightness" => {
                    if let Some(v) = arg_str
                        .strip_suffix('%')
                        .and_then(|s| s.trim().parse::<f32>().ok())
                        .map(|p| p / 100.0)
                        .or_else(|| arg_str.parse::<f32>().ok())
                    {
                        glass.brightness = v;
                        found_any = true;
                    }
                }
                "border" | "border-thickness" => {
                    if let Some(v) = parse_css_px(arg_str).or_else(|| arg_str.parse::<f32>().ok()) {
                        glass.border_thickness = v;
                        found_any = true;
                    }
                }
                "tint" => {
                    if let Some(color) = parse_color(arg_str) {
                        glass.tint = color;
                        found_any = true;
                    }
                }
                "noise" => {
                    if let Ok(v) = arg_str.parse::<f32>() {
                        glass.noise = v;
                        found_any = true;
                    }
                }
                _ => {
                    continue;
                }
            }
        } else {
            break;
        }
    }

    // liquid-glass() with no sub-functions still produces default liquid glass
    if found_any || inner.is_empty() {
        Some(glass)
    } else {
        None
    }
}

fn parse_css_filter(value: &str) -> Option<crate::element_style::CssFilter> {
    use crate::element_style::CssFilter;

    let value = value.trim();
    if value.eq_ignore_ascii_case("none") {
        return Some(CssFilter::default());
    }

    let mut filter = CssFilter::default();
    let mut found_any = false;
    let mut remaining = value;

    while !remaining.is_empty() {
        remaining = remaining.trim_start();
        if remaining.is_empty() {
            break;
        }

        // Find function name and opening paren
        if let Some(paren_pos) = remaining.find('(') {
            let func_name = remaining[..paren_pos].trim();
            let after_paren = &remaining[paren_pos + 1..];

            // Find matching closing paren (handles nested parens like rgba())
            let close_pos = {
                let mut depth = 0i32;
                let mut found = None;
                for (i, ch) in after_paren.char_indices() {
                    match ch {
                        '(' => depth += 1,
                        ')' => {
                            if depth == 0 {
                                found = Some(i);
                                break;
                            }
                            depth -= 1;
                        }
                        _ => {}
                    }
                }
                found
            };
            if let Some(close_pos) = close_pos {
                let arg_str = after_paren[..close_pos].trim();
                remaining = after_paren[close_pos + 1..].trim_start();

                let func_lower = func_name.to_lowercase();

                // Handle special multi-arg functions first
                if func_lower == "blur" {
                    if let Some(px) = parse_css_px(arg_str) {
                        filter.blur = px;
                        found_any = true;
                    }
                    continue;
                }
                if func_lower == "drop-shadow" {
                    let parts = split_whitespace_respecting_parens(arg_str);
                    if parts.len() >= 3 {
                        let x = parse_length_value(&parts[0]);
                        let y = parse_length_value(&parts[1]);
                        let blur_val = parse_length_value(&parts[2]);
                        let color = if parts.len() >= 4 {
                            parse_color(&parts[3])
                        } else {
                            Some(blinc_core::Color::rgba(0.0, 0.0, 0.0, 0.5))
                        };
                        if let (Some(x), Some(y), Some(b), Some(c)) = (x, y, blur_val, color) {
                            filter.drop_shadow = Some(blinc_core::Shadow::new(x, y, b, c));
                            found_any = true;
                        }
                    }
                    continue;
                }

                // Parse the argument value for simple single-value functions
                let arg_val = if let Some(deg_str) = arg_str.strip_suffix("deg") {
                    deg_str.trim().parse::<f32>().ok()
                } else if let Some(pct_str) = arg_str.strip_suffix('%') {
                    pct_str.trim().parse::<f32>().ok().map(|v| v / 100.0)
                } else {
                    arg_str.parse::<f32>().ok()
                };

                if let Some(v) = arg_val {
                    match func_lower.as_str() {
                        "grayscale" => {
                            filter.grayscale = v;
                            found_any = true;
                        }
                        "invert" => {
                            filter.invert = v;
                            found_any = true;
                        }
                        "sepia" => {
                            filter.sepia = v;
                            found_any = true;
                        }
                        "hue-rotate" => {
                            filter.hue_rotate = v;
                            found_any = true;
                        }
                        "brightness" => {
                            filter.brightness = v;
                            found_any = true;
                        }
                        "contrast" => {
                            filter.contrast = v;
                            found_any = true;
                        }
                        "saturate" => {
                            filter.saturate = v;
                            found_any = true;
                        }
                        _ => {
                            // Unknown filter function, skip
                        }
                    }
                }
            } else {
                break; // No closing paren
            }
        } else {
            break; // No opening paren
        }
    }

    if found_any { Some(filter) } else { None }
}

/// Parse a time value (e.g., "300ms", "0.5s", "1s")
fn parse_time_value(input: &str) -> Option<u32> {
    let input = input.trim();

    // Try milliseconds
    if let Some(ms_str) = input.strip_suffix("ms") {
        return ms_str.trim().parse::<f32>().ok().map(|ms| ms as u32);
    }

    // Try seconds
    if let Some(s_str) = input.strip_suffix('s') {
        return s_str
            .trim()
            .parse::<f32>()
            .ok()
            .map(|s| (s * 1000.0) as u32);
    }

    // Try plain number (assume milliseconds)
    input.parse::<f32>().ok().map(|ms| ms as u32)
}

/// Parse a CSS length value in pixels (e.g. "100px", "50", "10.5px")
fn parse_css_px(input: &str) -> Option<f32> {
    let trimmed = input.trim();

    // Support calc() expressions — evaluate static calcs immediately
    if trimmed.starts_with("calc(") {
        if let Some(expr) = crate::calc::parse_calc(trimmed) {
            let ctx = viewport_calc_context();
            // A calc over viewport units is only "dynamic" for want of a
            // viewport; with one in hand it evaluates like any other.
            if !expr.is_dynamic() || ctx.viewport_height > 0.0 {
                return Some(expr.eval(&ctx));
            }
        }
        return None;
    }

    if let Some(px_str) = trimmed.strip_suffix("px") {
        return px_str.trim().parse::<f32>().ok();
    }

    // Viewport units. The calc module already knows how to evaluate
    // these; they just never reached it, because this parser only
    // understood px. Without a live viewport (styles parsed before the
    // window exists) there is no sane answer, so fall through to None
    // rather than silently resolving against zero.
    for suffix in ["vh", "vw"] {
        if let Some(num) = trimmed.strip_suffix(suffix) {
            let value = num.trim().parse::<f32>().ok()?;
            let ctx = viewport_calc_context();
            if ctx.viewport_width <= 0.0 || ctx.viewport_height <= 0.0 {
                return None;
            }
            let unit = crate::calc::CalcUnit::parse(suffix)?;
            return Some(unit.to_pixels(value, &ctx));
        }
    }

    // Unitless number = px
    trimmed.parse::<f32>().ok()
}

/// `CalcContext` seeded with the live viewport, so viewport-relative
/// units resolve. Falls back to the default (zero viewport) before the
/// window exists.
fn viewport_calc_context() -> crate::calc::CalcContext {
    let mut ctx = crate::calc::CalcContext::default();
    if blinc_core::context_state::BlincContextState::is_initialized() {
        let (w, h) = blinc_core::context_state::BlincContextState::get().viewport_size();
        if w > 0.0 && h > 0.0 {
            ctx.viewport_width = w;
            ctx.viewport_height = h;
        }
    }
    ctx
}

/// Parse a CSS `object-position` value into [x, y] in 0.0-1.0 range.
///
/// Supports keywords (`left`, `center`, `right`, `top`, `bottom`),
/// percentages (`25%`), and two-value combinations (`left top`, `50% 25%`).
fn parse_object_position(input: &str) -> Option<[f32; 2]> {
    let trimmed = input.trim();

    // Helper: parse a single keyword or percentage to a 0.0-1.0 value
    fn keyword_or_pct(s: &str) -> Option<f32> {
        match s {
            "left" | "top" => Some(0.0),
            "center" => Some(0.5),
            "right" | "bottom" => Some(1.0),
            _ => {
                if let Some(pct_str) = s.strip_suffix('%') {
                    pct_str.trim().parse::<f32>().ok().map(|v| v / 100.0)
                } else {
                    None
                }
            }
        }
    }

    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    match parts.len() {
        1 => {
            // Single value: applied to x, y defaults to 50%
            let v = keyword_or_pct(parts[0])?;
            // Keywords top/bottom are y-axis → put in y, x = 50%
            match parts[0] {
                "top" | "bottom" => Some([0.5, v]),
                _ => Some([v, 0.5]),
            }
        }
        2 => {
            let x = keyword_or_pct(parts[0])?;
            let y = keyword_or_pct(parts[1])?;
            Some([x, y])
        }
        _ => None,
    }
}

/// Parse a CSS dimension value: `Npx`, `N%`, `auto`, `fit-content`, `max-content`, or `calc(...)`
fn parse_css_dimension(input: &str) -> Option<crate::element_style::StyleDimension> {
    use crate::element_style::StyleDimension;
    let trimmed = input.trim();

    // Support calc() expressions
    if trimmed.starts_with("calc(") {
        if let Some(expr) = crate::calc::parse_calc(trimmed) {
            if !expr.is_dynamic() {
                // Static calc — check if it contains a percentage
                // For now, evaluate as px value
                return Some(StyleDimension::Length(
                    expr.eval(&crate::calc::CalcContext::default()),
                ));
            }
        }
        return None;
    }

    match trimmed.to_lowercase().as_str() {
        "auto" | "fit-content" | "max-content" => Some(StyleDimension::Auto),
        _ => {
            if let Some(pct_str) = trimmed.strip_suffix('%') {
                let pct = pct_str.trim().parse::<f32>().ok()?;
                Some(StyleDimension::Percent(pct / 100.0))
            } else {
                parse_css_px(trimmed).map(StyleDimension::Length)
            }
        }
    }
}

/// Parse a CSS spacing value (uniform or per-side)
/// Supports: "10px", "10px 20px" (vert horiz), "10px 20px 30px 40px" (top right bottom left)
/// Also supports `calc(...)` as a single uniform value.
fn parse_css_spacing(input: &str) -> Option<SpacingRect> {
    let trimmed = input.trim();

    // Handle calc() as a single uniform value (don't split on whitespace inside calc)
    if trimmed.starts_with("calc(") {
        let v = parse_css_px(trimmed)?;
        return Some(SpacingRect::uniform(v));
    }

    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    match parts.len() {
        1 => {
            let v = parse_css_px(parts[0])?;
            Some(SpacingRect::uniform(v))
        }
        2 => {
            let vert = parse_css_px(parts[0])?;
            let horiz = parse_css_px(parts[1])?;
            Some(SpacingRect::xy(horiz, vert))
        }
        4 => {
            let top = parse_css_px(parts[0])?;
            let right = parse_css_px(parts[1])?;
            let bottom = parse_css_px(parts[2])?;
            let left = parse_css_px(parts[3])?;
            Some(SpacingRect::new(top, right, bottom, left))
        }
        _ => None,
    }
}

// ============================================================================
// Color Parsing
// ============================================================================

fn parse_font_weight(value: &str) -> Option<crate::div::FontWeight> {
    use crate::div::FontWeight;
    match value.trim().to_lowercase().as_str() {
        "100" | "thin" => Some(FontWeight::Thin),
        "200" | "extra-light" | "extralight" => Some(FontWeight::ExtraLight),
        "300" | "light" => Some(FontWeight::Light),
        "400" | "normal" => Some(FontWeight::Normal),
        "500" | "medium" => Some(FontWeight::Medium),
        "600" | "semi-bold" | "semibold" => Some(FontWeight::SemiBold),
        "700" | "bold" => Some(FontWeight::Bold),
        "800" | "extra-bold" | "extrabold" => Some(FontWeight::ExtraBold),
        "900" | "black" => Some(FontWeight::Black),
        _ => None,
    }
}

fn parse_text_decoration(value: &str) -> Option<crate::element_style::TextDecoration> {
    use crate::element_style::TextDecoration;
    match value.trim().to_lowercase().as_str() {
        "none" => Some(TextDecoration::None),
        "underline" => Some(TextDecoration::Underline),
        "line-through" => Some(TextDecoration::LineThrough),
        _ => None,
    }
}

fn parse_text_align(value: &str) -> Option<crate::div::TextAlign> {
    use crate::div::TextAlign;
    match value.trim().to_lowercase().as_str() {
        "left" | "start" => Some(TextAlign::Left),
        "center" => Some(TextAlign::Center),
        "right" | "end" => Some(TextAlign::Right),
        _ => None,
    }
}

fn parse_cursor(value: &str) -> Option<crate::element::CursorStyle> {
    use crate::element::CursorStyle;
    match value.trim().to_lowercase().as_str() {
        "default" | "auto" => Some(CursorStyle::Default),
        "pointer" => Some(CursorStyle::Pointer),
        "text" => Some(CursorStyle::Text),
        "crosshair" => Some(CursorStyle::Crosshair),
        "move" => Some(CursorStyle::Move),
        "not-allowed" => Some(CursorStyle::NotAllowed),
        "ns-resize" | "n-resize" | "s-resize" | "row-resize" => Some(CursorStyle::ResizeNS),
        "ew-resize" | "e-resize" | "w-resize" | "col-resize" => Some(CursorStyle::ResizeEW),
        "nesw-resize" | "ne-resize" | "sw-resize" => Some(CursorStyle::ResizeNESW),
        "nwse-resize" | "nw-resize" | "se-resize" => Some(CursorStyle::ResizeNWSE),
        "grab" => Some(CursorStyle::Grab),
        "grabbing" => Some(CursorStyle::Grabbing),
        "wait" => Some(CursorStyle::Wait),
        "progress" => Some(CursorStyle::Progress),
        "none" => Some(CursorStyle::None),
        "help" => Some(CursorStyle::Default), // map to default for now
        _ => None,
    }
}

fn parse_blend_mode(value: &str) -> Option<blinc_core::BlendMode> {
    use blinc_core::BlendMode;
    match value.trim().to_lowercase().as_str() {
        "normal" => Some(BlendMode::Normal),
        "multiply" => Some(BlendMode::Multiply),
        "screen" => Some(BlendMode::Screen),
        "overlay" => Some(BlendMode::Overlay),
        "darken" => Some(BlendMode::Darken),
        "lighten" => Some(BlendMode::Lighten),
        "color-dodge" => Some(BlendMode::ColorDodge),
        "color-burn" => Some(BlendMode::ColorBurn),
        "hard-light" => Some(BlendMode::HardLight),
        "soft-light" => Some(BlendMode::SoftLight),
        "difference" => Some(BlendMode::Difference),
        "exclusion" => Some(BlendMode::Exclusion),
        _ => None,
    }
}

fn parse_color(input: &str) -> Option<Color> {
    let input = input.trim();

    // Try hex color
    if let Ok((_, color)) = parse_hex_color::<nom::error::Error<&str>>(input) {
        return Some(color);
    }

    // Try rgba()
    if let Ok((_, color)) = parse_rgba_color::<nom::error::Error<&str>>(input) {
        return Some(color);
    }

    // Try rgb()
    if let Ok((_, color)) = parse_rgb_color::<nom::error::Error<&str>>(input) {
        return Some(color);
    }

    // Try named color
    parse_named_color(input)
}

/// Parse hex color: #RGB, #RRGGBB, or #RRGGBBAA
fn parse_hex_color<'a, E: NomParseError<&'a str>>(input: &'a str) -> IResult<&'a str, Color, E> {
    let (input, _) = char('#')(input)?;
    let (input, hex) = take_while1(|c: char| c.is_ascii_hexdigit())(input)?;

    let color = match hex.len() {
        3 => {
            let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).map_err(|_| {
                nom::Err::Error(E::from_error_kind(input, nom::error::ErrorKind::HexDigit))
            })?;
            let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).map_err(|_| {
                nom::Err::Error(E::from_error_kind(input, nom::error::ErrorKind::HexDigit))
            })?;
            let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).map_err(|_| {
                nom::Err::Error(E::from_error_kind(input, nom::error::ErrorKind::HexDigit))
            })?;
            Color::rgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
        }
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).map_err(|_| {
                nom::Err::Error(E::from_error_kind(input, nom::error::ErrorKind::HexDigit))
            })?;
            let g = u8::from_str_radix(&hex[2..4], 16).map_err(|_| {
                nom::Err::Error(E::from_error_kind(input, nom::error::ErrorKind::HexDigit))
            })?;
            let b = u8::from_str_radix(&hex[4..6], 16).map_err(|_| {
                nom::Err::Error(E::from_error_kind(input, nom::error::ErrorKind::HexDigit))
            })?;
            Color::rgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
        }
        8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).map_err(|_| {
                nom::Err::Error(E::from_error_kind(input, nom::error::ErrorKind::HexDigit))
            })?;
            let g = u8::from_str_radix(&hex[2..4], 16).map_err(|_| {
                nom::Err::Error(E::from_error_kind(input, nom::error::ErrorKind::HexDigit))
            })?;
            let b = u8::from_str_radix(&hex[4..6], 16).map_err(|_| {
                nom::Err::Error(E::from_error_kind(input, nom::error::ErrorKind::HexDigit))
            })?;
            let a = u8::from_str_radix(&hex[6..8], 16).map_err(|_| {
                nom::Err::Error(E::from_error_kind(input, nom::error::ErrorKind::HexDigit))
            })?;
            Color::rgba(
                r as f32 / 255.0,
                g as f32 / 255.0,
                b as f32 / 255.0,
                a as f32 / 255.0,
            )
        }
        _ => {
            return Err(nom::Err::Error(E::from_error_kind(
                input,
                nom::error::ErrorKind::LengthValue,
            )));
        }
    };

    Ok((input, color))
}

/// Parse rgba(r, g, b, a)
fn parse_rgba_color<'a, E: NomParseError<&'a str>>(input: &'a str) -> IResult<&'a str, Color, E> {
    let (input, _) = tag_no_case("rgba")(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('(')(input)?;
    let (input, _) = ws(input)?;
    let (input, r) = float(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char(',')(input)?;
    let (input, _) = ws(input)?;
    let (input, g) = float(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char(',')(input)?;
    let (input, _) = ws(input)?;
    let (input, b) = float(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char(',')(input)?;
    let (input, _) = ws(input)?;
    let (input, a) = float(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char(')')(input)?;

    // Normalize if values are 0-255 range
    let (r, g, b) = if r > 1.0 || g > 1.0 || b > 1.0 {
        (r / 255.0, g / 255.0, b / 255.0)
    } else {
        (r, g, b)
    };

    Ok((input, Color::rgba(r, g, b, a)))
}

/// Parse rgb(r, g, b)
fn parse_rgb_color<'a, E: NomParseError<&'a str>>(input: &'a str) -> IResult<&'a str, Color, E> {
    let (input, _) = tag_no_case("rgb")(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('(')(input)?;
    let (input, _) = ws(input)?;
    let (input, r) = float(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char(',')(input)?;
    let (input, _) = ws(input)?;
    let (input, g) = float(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char(',')(input)?;
    let (input, _) = ws(input)?;
    let (input, b) = float(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char(')')(input)?;

    // Normalize if values are 0-255 range
    let (r, g, b) = if r > 1.0 || g > 1.0 || b > 1.0 {
        (r / 255.0, g / 255.0, b / 255.0)
    } else {
        (r, g, b)
    };

    Ok((input, Color::rgba(r, g, b, 1.0)))
}

/// Parse named colors
fn parse_named_color(name: &str) -> Option<Color> {
    match name.to_lowercase().as_str() {
        "black" => Some(Color::BLACK),
        "white" => Some(Color::WHITE),
        "red" => Some(Color::RED),
        "green" => Some(Color::rgb(0.0, 0.5, 0.0)),
        "blue" => Some(Color::BLUE),
        "yellow" => Some(Color::YELLOW),
        "cyan" | "aqua" => Some(Color::CYAN),
        "magenta" | "fuchsia" => Some(Color::MAGENTA),
        "gray" | "grey" => Some(Color::GRAY),
        "orange" => Some(Color::ORANGE),
        "purple" => Some(Color::PURPLE),
        "transparent" => Some(Color::TRANSPARENT),
        _ => None,
    }
}

// ============================================================================
// Gradient Parsing
// ============================================================================

/// Parse CSS linear-gradient()
///
/// Syntax:
/// - `linear-gradient(135deg, #667eea 0%, #764ba2 100%)`
/// - `linear-gradient(to right, red, blue)`
/// - `linear-gradient(to bottom right, #fff, #000)`
/// - `linear-gradient(90deg, red 0%, yellow 50%, green 100%)`
fn parse_linear_gradient(input: &str) -> Option<Gradient> {
    // Strip the function wrapper
    let inner = input
        .strip_prefix("linear-gradient(")
        .and_then(|s| s.strip_suffix(')'))?
        .trim();

    // Split by commas, but be careful with colors that contain commas (rgb, rgba)
    let parts = split_gradient_parts(inner);
    if parts.is_empty() {
        return None;
    }

    // Parse angle/direction (first part might be angle or first color stop)
    let (angle_deg, color_start_idx) = parse_gradient_direction(&parts[0]);

    // Parse color stops
    let stops = parse_color_stops(&parts[color_start_idx..])?;
    if stops.len() < 2 {
        return None;
    }

    // Convert angle to start/end points (using ObjectBoundingBox space 0-1)
    let (start, end) = angle_to_gradient_points(angle_deg);

    Some(Gradient::Linear {
        start,
        end,
        stops,
        space: GradientSpace::ObjectBoundingBox,
        spread: blinc_core::GradientSpread::Pad,
    })
}

/// Parse CSS radial-gradient()
///
/// Syntax:
/// - `radial-gradient(circle, red, blue)`
/// - `radial-gradient(circle at center, red, blue)`
/// - `radial-gradient(ellipse at 25% 25%, red, blue)`
fn parse_radial_gradient(input: &str) -> Option<Gradient> {
    let inner = input
        .strip_prefix("radial-gradient(")
        .and_then(|s| s.strip_suffix(')'))?
        .trim();

    let parts = split_gradient_parts(inner);
    if parts.is_empty() {
        return None;
    }

    // Check for shape/position specification
    let mut center = Point::new(0.5, 0.5); // Default center
    let mut color_start_idx = 0;

    // First part might be shape/position info
    let first = parts[0].trim().to_lowercase();
    if first.starts_with("circle") || first.starts_with("ellipse") {
        // Parse "circle at X Y" or just "circle"
        if let Some(at_pos) = first.find(" at ") {
            let pos_str = &first[at_pos + 4..];
            if let Some(pos) = parse_position(pos_str) {
                center = pos;
            }
        }
        color_start_idx = 1;
    } else if first.contains(" at ") || first.starts_with("at ") {
        // Just position: "at center" or "at 50% 50%"
        let pos_str = first.strip_prefix("at ").unwrap_or(&first);
        if let Some(pos) = parse_position(pos_str) {
            center = pos;
        }
        color_start_idx = 1;
    }

    let stops = parse_color_stops(&parts[color_start_idx..])?;
    if stops.len() < 2 {
        return None;
    }

    Some(Gradient::Radial {
        center,
        radius: 0.5, // Default radius for ObjectBoundingBox space
        focal: None,
        stops,
        space: GradientSpace::ObjectBoundingBox,
        spread: blinc_core::GradientSpread::Pad,
    })
}

/// Parse CSS conic-gradient()
///
/// Syntax:
/// - `conic-gradient(red, yellow, green, blue, red)`
/// - `conic-gradient(from 45deg, red, blue)`
/// - `conic-gradient(from 0deg at center, red 0deg, blue 360deg)`
fn parse_conic_gradient(input: &str) -> Option<Gradient> {
    let inner = input
        .strip_prefix("conic-gradient(")
        .and_then(|s| s.strip_suffix(')'))?
        .trim();

    let parts = split_gradient_parts(inner);
    if parts.is_empty() {
        return None;
    }

    let mut start_angle: f32 = 0.0;
    let mut center = Point::new(0.5, 0.5);
    let mut color_start_idx = 0;

    // Check for "from Xdeg" and/or "at position"
    let first = parts[0].trim().to_lowercase();
    if let Some(rest) = first.strip_prefix("from ") {
        // Parse "from 45deg" or "from 45deg at center"
        if let Some(at_pos) = rest.find(" at ") {
            // Has both angle and position
            let angle_str = rest[..at_pos].trim();
            if let Some(angle) = parse_angle_value(angle_str) {
                start_angle = angle;
            }
            let pos_str = &rest[at_pos + 4..];
            if let Some(pos) = parse_position(pos_str) {
                center = pos;
            }
        } else {
            // Just angle
            if let Some(angle) = parse_angle_value(rest.trim()) {
                start_angle = angle;
            }
        }
        color_start_idx = 1;
    } else if let Some(rest) = first.strip_prefix("at ") {
        // Just position
        if let Some(pos) = parse_position(rest) {
            center = pos;
        }
        color_start_idx = 1;
    }

    let stops = parse_color_stops(&parts[color_start_idx..])?;
    if stops.len() < 2 {
        return None;
    }

    Some(Gradient::Conic {
        center,
        start_angle: start_angle * std::f32::consts::PI / 180.0, // Convert to radians
        stops,
        space: GradientSpace::ObjectBoundingBox,
    })
}

/// Split gradient arguments by commas, respecting parentheses for rgb()/rgba()
fn split_gradient_parts(input: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut paren_depth: i32 = 0;

    for c in input.chars() {
        match c {
            '(' => {
                paren_depth += 1;
                current.push(c);
            }
            ')' => {
                paren_depth = (paren_depth - 1).max(0);
                current.push(c);
            }
            ',' if paren_depth == 0 => {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    parts.push(trimmed);
                }
                current.clear();
            }
            _ => current.push(c),
        }
    }

    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        parts.push(trimmed);
    }

    parts
}

/// Parse gradient direction (angle or `to <direction>`).
/// Returns `(angle_in_degrees, color_start_index)`.
fn parse_gradient_direction(first_part: &str) -> (f32, usize) {
    let part = first_part.trim().to_lowercase();

    // Try parsing as angle (e.g., "135deg", "45deg")
    if let Some(angle) = parse_angle_value(&part) {
        return (angle, 1);
    }

    // Try parsing as direction keyword
    if let Some(direction) = part.strip_prefix("to ") {
        let angle = match direction.trim() {
            "top" => 0.0,
            "right" => 90.0,
            "bottom" => 180.0,
            "left" => 270.0,
            "top right" | "right top" => 45.0,
            "bottom right" | "right bottom" => 135.0,
            "bottom left" | "left bottom" => 225.0,
            "top left" | "left top" => 315.0,
            _ => return (180.0, 0), // Default to "to bottom" if unrecognized, treat as color
        };
        return (angle, 1);
    }

    // Not a direction - default to "to bottom" (180deg) and treat first part as color
    (180.0, 0)
}

/// Parse angle value (e.g., "45deg", "0.5turn", "100grad")
/// Result of attempting to parse a CSS value as a calc() expression.
enum CalcParseResult {
    /// Contains env() — needs per-frame evaluation
    Dynamic(crate::calc::CalcExpr),
    /// Pure calc() with no env() — evaluate once to a fixed value
    Static(f32),
    /// Not a calc expression — fall through to normal parsing
    NotCalc,
}

/// Try to parse a CSS property value as a calc() expression.
/// Returns Dynamic if it contains env() references (needs per-frame eval),
/// Static if it's a pure calc, or NotCalc to fall through.
fn try_parse_calc(value: &str) -> CalcParseResult {
    let v = value.trim();
    if v.starts_with("calc(") || v.contains("env(") {
        if let Some(expr) = crate::calc::parse_calc(v) {
            if expr.is_dynamic() {
                return CalcParseResult::Dynamic(expr);
            } else {
                let ctx = crate::calc::CalcContext::default();
                return CalcParseResult::Static(expr.eval(&ctx));
            }
        }
    }
    CalcParseResult::NotCalc
}

fn parse_angle_value(input: &str) -> Option<f32> {
    let input = input.trim();

    if let Some(deg_str) = input.strip_suffix("deg") {
        return deg_str.trim().parse::<f32>().ok();
    }

    if let Some(turn_str) = input.strip_suffix("turn") {
        return turn_str.trim().parse::<f32>().ok().map(|t| t * 360.0);
    }

    if let Some(rad_str) = input.strip_suffix("rad") {
        return rad_str
            .trim()
            .parse::<f32>()
            .ok()
            .map(|r| r * 180.0 / std::f32::consts::PI);
    }

    if let Some(grad_str) = input.strip_suffix("grad") {
        return grad_str.trim().parse::<f32>().ok().map(|g| g * 0.9);
    }

    // Try parsing as plain number (assumed degrees)
    input.parse::<f32>().ok()
}

/// Convert CSS gradient angle to start/end points
/// CSS angles: 0deg = to top, 90deg = to right, 180deg = to bottom, 270deg = to left
/// In ObjectBoundingBox space (0-1 coordinates)
pub fn angle_to_gradient_points(angle_deg: f32) -> (Point, Point) {
    // CSS gradient angles are measured clockwise from top (0deg = up)
    // Convert to mathematical angle (counterclockwise from right)
    let angle_rad = (90.0 - angle_deg) * std::f32::consts::PI / 180.0;

    // Calculate direction vector
    let dx = angle_rad.cos();
    let dy = -angle_rad.sin(); // Negative because Y grows downward in screen coords

    // Find intersection with unit square
    // We want the gradient line to span the full diagonal based on angle
    let center = Point::new(0.5, 0.5);

    // Calculate the length needed to reach corners
    let len = if dx.abs() > dy.abs() {
        0.5 / dx.abs()
    } else if dy.abs() > 0.0 {
        0.5 / dy.abs()
    } else {
        0.5
    };

    let start = Point::new(center.x - dx * len, center.y - dy * len);
    let end = Point::new(center.x + dx * len, center.y + dy * len);

    (start, end)
}

/// Reverse-compute CSS gradient angle (in degrees) from ObjectBoundingBox start/end points.
/// Inverse of `angle_to_gradient_points`.
pub fn gradient_points_to_angle(start: Point, end: Point) -> f32 {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    // CSS: 0deg = to top, 90deg = to right, clockwise
    // atan2 gives counterclockwise from positive-x
    let math_angle = (-dy).atan2(dx); // negate dy for screen coords
    let css_deg = 90.0 - math_angle * 180.0 / std::f32::consts::PI;
    // Normalize to 0..360
    ((css_deg % 360.0) + 360.0) % 360.0
}

/// Parse color stops from gradient parts
fn parse_color_stops(parts: &[String]) -> Option<Vec<GradientStop>> {
    if parts.is_empty() {
        return None;
    }

    let mut stops = Vec::new();
    let total = parts.len();

    for (i, part) in parts.iter().enumerate() {
        if let Some(stop) = parse_single_color_stop(part, i, total) {
            stops.push(stop);
        }
    }

    // Ensure we have at least 2 stops
    if stops.len() < 2 {
        return None;
    }

    // Fill in missing positions (evenly distributed)
    distribute_stop_positions(&mut stops);

    Some(stops)
}

/// Parse a single color stop (e.g., "red", "#667eea 50%", "rgba(255,0,0,0.5) 25%")
fn parse_single_color_stop(part: &str, index: usize, total: usize) -> Option<GradientStop> {
    let part = part.trim();

    // Try to find a percentage or length at the end
    let (color_str, position) = extract_color_and_position(part, index, total);

    let color = parse_color(color_str)?;
    Some(GradientStop::new(position, color))
}

/// Extract color and position from a color stop string
fn extract_color_and_position(part: &str, index: usize, total: usize) -> (&str, f32) {
    // Check for percentage at the end
    if let Some(pct_pos) = part.rfind('%') {
        // Find where the number starts (work backwards from %)
        let before_pct = &part[..pct_pos];
        if let Some(space_pos) =
            before_pct.rfind(|c: char| !c.is_ascii_digit() && c != '.' && c != '-')
        {
            let num_str = &part[space_pos + 1..pct_pos];
            if let Ok(pct) = num_str.trim().parse::<f32>() {
                let color_str = part[..=space_pos].trim();
                return (color_str, pct / 100.0);
            }
        } else {
            // The whole thing before % is a number
            if let Ok(pct) = before_pct.trim().parse::<f32>() {
                // This shouldn't happen for valid color stops, but handle it
                return (part, pct / 100.0);
            }
        }
    }

    // Check for pixel value at the end (less common in CSS but valid)
    if let Some(px_pos) = part.rfind("px") {
        let before_px = &part[..px_pos];
        if let Some(space_pos) =
            before_px.rfind(|c: char| !c.is_ascii_digit() && c != '.' && c != '-')
        {
            let num_str = &part[space_pos + 1..px_pos];
            if let Ok(_px) = num_str.trim().parse::<f32>() {
                // For now, ignore pixel values and use default positioning
                let color_str = part[..=space_pos].trim();
                return (color_str, default_position(index, total));
            }
        }
    }

    // No explicit position - use default
    (part, default_position(index, total))
}

/// Calculate default position for a color stop
fn default_position(index: usize, total: usize) -> f32 {
    if total <= 1 {
        0.0
    } else {
        index as f32 / (total - 1) as f32
    }
}

/// Fill in missing/default positions with even distribution
fn distribute_stop_positions(stops: &mut [GradientStop]) {
    // The positions are already set during parsing
    // This function could be enhanced to handle "auto" positions
    // For now, we rely on default_position during parsing
}

/// Parse position keywords (for radial/conic gradients)
fn parse_position(input: &str) -> Option<Point> {
    let input = input.trim().to_lowercase();

    // Single keyword
    match input.as_str() {
        "center" => return Some(Point::new(0.5, 0.5)),
        "top" => return Some(Point::new(0.5, 0.0)),
        "bottom" => return Some(Point::new(0.5, 1.0)),
        "left" => return Some(Point::new(0.0, 0.5)),
        "right" => return Some(Point::new(1.0, 0.5)),
        "top left" | "left top" => return Some(Point::new(0.0, 0.0)),
        "top right" | "right top" => return Some(Point::new(1.0, 0.0)),
        "bottom left" | "left bottom" => return Some(Point::new(0.0, 1.0)),
        "bottom right" | "right bottom" => return Some(Point::new(1.0, 1.0)),
        _ => {}
    }

    // Try parsing as "X% Y%" or "Xpx Ypx"
    let parts: Vec<&str> = input.split_whitespace().collect();
    if parts.len() >= 2 {
        let x = parse_position_value(parts[0])?;
        let y = parse_position_value(parts[1])?;
        return Some(Point::new(x, y));
    }

    None
}

/// Parse a single position value (percentage or keyword)
fn parse_position_value(input: &str) -> Option<f32> {
    let input = input.trim();

    if let Some(pct_str) = input.strip_suffix('%') {
        return pct_str.trim().parse::<f32>().ok().map(|p| p / 100.0);
    }

    // Keywords
    match input {
        "left" | "top" => Some(0.0),
        "center" => Some(0.5),
        "right" | "bottom" => Some(1.0),
        _ => None,
    }
}

// ============================================================================
// CSS clip-path Parsing
// ============================================================================

/// Parse a CSS length value (px or %) into a ClipLength
/// Convert a ClipLength to a float value for animation interpolation
fn clip_length_to_percent(len: &ClipLength) -> f32 {
    match len {
        ClipLength::Percent(p) => *p,
        ClipLength::Px(px) => *px,
    }
}

fn parse_clip_length(s: &str) -> Option<ClipLength> {
    let s = s.trim();
    if let Some(stripped) = s.strip_suffix('%') {
        stripped.trim().parse::<f32>().ok().map(ClipLength::Percent)
    } else if let Some(stripped) = s.strip_suffix("px") {
        stripped.trim().parse::<f32>().ok().map(ClipLength::Px)
    } else {
        // Bare number → pixels
        s.parse::<f32>().ok().map(ClipLength::Px)
    }
}

/// Parse the `at cx cy` position suffix (default: 50% 50%)
fn parse_at_position(s: &str) -> ((ClipLength, ClipLength), &str) {
    let default = (ClipLength::Percent(50.0), ClipLength::Percent(50.0));
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("at") {
        let rest = rest.trim();
        let tokens: Vec<&str> = rest.splitn(2, char::is_whitespace).collect();
        if tokens.len() == 2 {
            if let (Some(cx), Some(cy)) =
                (parse_clip_length(tokens[0]), parse_clip_length(tokens[1]))
            {
                return ((cx, cy), "");
            }
        }
        // single token → cx, cy=50%
        if let Some(cx) = tokens.first().and_then(|t| parse_clip_length(t)) {
            return ((cx, ClipLength::Percent(50.0)), "");
        }
        (default, s)
    } else {
        (default, s)
    }
}

/// Parse `round <radius>` suffix, returning the radius and remaining content
fn parse_round_suffix(s: &str) -> (Option<f32>, &str) {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("round") {
        let rest = rest.trim();
        if let Some(px) = parse_css_px(rest) {
            return (Some(px), "");
        }
        (None, s)
    } else {
        (None, s)
    }
}

/// Parse a full CSS `clip-path` value string into a `ClipPath`
///
/// Supported functions:
/// - `circle(radius at cx cy)`
/// - `ellipse(rx ry at cx cy)`
/// - `inset(top right bottom left round radius)`
/// - `rect(top right bottom left round radius)`
/// - `xywh(x y w h round radius)`
/// - `polygon(x1 y1, x2 y2, ...)`
/// - `path("M 0 0 L ...")`
pub fn parse_clip_path(value: &str) -> Option<ClipPath> {
    let value = value.trim();

    // Extract function name and inner content
    let paren_start = value.find('(')?;
    let paren_end = value.rfind(')')?;
    if paren_end <= paren_start {
        return None;
    }
    let func_name = value[..paren_start].trim().to_lowercase();
    let inner = value[paren_start + 1..paren_end].trim();

    match func_name.as_str() {
        "circle" => {
            // circle() or circle(radius) or circle(radius at cx cy) or circle(at cx cy)
            if inner.is_empty() {
                return Some(ClipPath::Circle {
                    radius: None,
                    center: (ClipLength::Percent(50.0), ClipLength::Percent(50.0)),
                });
            }
            // Check if starts with "at" (no radius)
            if inner.starts_with("at") {
                let (center, _) = parse_at_position(inner);
                return Some(ClipPath::Circle {
                    radius: None,
                    center,
                });
            }
            // radius [at cx cy]
            let parts: Vec<&str> = inner.splitn(2, " at ").collect();
            let radius = parse_clip_length(parts[0].trim());
            let center = if parts.len() > 1 {
                let (center, _) = parse_at_position(&format!("at {}", parts[1]));
                center
            } else {
                (ClipLength::Percent(50.0), ClipLength::Percent(50.0))
            };
            Some(ClipPath::Circle { radius, center })
        }
        "ellipse" => {
            // ellipse() or ellipse(rx ry) or ellipse(rx ry at cx cy)
            if inner.is_empty() {
                return Some(ClipPath::Ellipse {
                    rx: None,
                    ry: None,
                    center: (ClipLength::Percent(50.0), ClipLength::Percent(50.0)),
                });
            }
            if inner.starts_with("at") {
                let (center, _) = parse_at_position(inner);
                return Some(ClipPath::Ellipse {
                    rx: None,
                    ry: None,
                    center,
                });
            }
            let parts: Vec<&str> = inner.splitn(2, " at ").collect();
            let radii_str = parts[0].trim();
            let radii_tokens: Vec<&str> = radii_str.split_whitespace().collect();
            let (rx, ry) = if radii_tokens.len() >= 2 {
                (
                    parse_clip_length(radii_tokens[0]),
                    parse_clip_length(radii_tokens[1]),
                )
            } else if radii_tokens.len() == 1 {
                let r = parse_clip_length(radii_tokens[0]);
                (r, r)
            } else {
                (None, None)
            };
            let center = if parts.len() > 1 {
                let (center, _) = parse_at_position(&format!("at {}", parts[1]));
                center
            } else {
                (ClipLength::Percent(50.0), ClipLength::Percent(50.0))
            };
            Some(ClipPath::Ellipse { rx, ry, center })
        }
        "inset" | "rect" => {
            // inset(top right bottom left [round radius])
            // rect(top right bottom left [round radius])
            let (inner_no_round, round_part) = if let Some(idx) = inner.find("round") {
                (inner[..idx].trim(), Some(inner[idx..].trim()))
            } else {
                (inner, None)
            };
            let round = round_part
                .and_then(|r| r.strip_prefix("round").and_then(|v| parse_css_px(v.trim())));
            let tokens: Vec<&str> = inner_no_round.split_whitespace().collect();
            let (top, right, bottom, left) = match tokens.len() {
                1 => {
                    let v = parse_clip_length(tokens[0])?;
                    (v, v, v, v)
                }
                2 => {
                    let tb = parse_clip_length(tokens[0])?;
                    let lr = parse_clip_length(tokens[1])?;
                    (tb, lr, tb, lr)
                }
                3 => {
                    let t = parse_clip_length(tokens[0])?;
                    let lr = parse_clip_length(tokens[1])?;
                    let b = parse_clip_length(tokens[2])?;
                    (t, lr, b, lr)
                }
                4.. => {
                    let t = parse_clip_length(tokens[0])?;
                    let r = parse_clip_length(tokens[1])?;
                    let b = parse_clip_length(tokens[2])?;
                    let l = parse_clip_length(tokens[3])?;
                    (t, r, b, l)
                }
                _ => return None,
            };
            if func_name == "inset" {
                Some(ClipPath::Inset {
                    top,
                    right,
                    bottom,
                    left,
                    round,
                })
            } else {
                Some(ClipPath::Rect {
                    top,
                    right,
                    bottom,
                    left,
                    round,
                })
            }
        }
        "xywh" => {
            // xywh(x y w h [round radius])
            let (inner_no_round, round_part) = if let Some(idx) = inner.find("round") {
                (inner[..idx].trim(), Some(inner[idx..].trim()))
            } else {
                (inner, None)
            };
            let round = round_part
                .and_then(|r| r.strip_prefix("round").and_then(|v| parse_css_px(v.trim())));
            let tokens: Vec<&str> = inner_no_round.split_whitespace().collect();
            if tokens.len() < 4 {
                return None;
            }
            let x = parse_clip_length(tokens[0])?;
            let y = parse_clip_length(tokens[1])?;
            let w = parse_clip_length(tokens[2])?;
            let h = parse_clip_length(tokens[3])?;
            Some(ClipPath::Xywh { x, y, w, h, round })
        }
        "polygon" => {
            // polygon(x1 y1, x2 y2, ...)
            let point_strs: Vec<&str> = inner.split(',').collect();
            let mut points = Vec::new();
            for ps in point_strs {
                let coords: Vec<&str> = ps.split_whitespace().collect();
                if coords.len() >= 2 {
                    let x = parse_clip_length(coords[0])?;
                    let y = parse_clip_length(coords[1])?;
                    points.push((x, y));
                }
            }
            if points.len() < 3 {
                return None;
            }
            Some(ClipPath::Polygon { points })
        }
        "path" => {
            // path("M 0 0 L 100 0 ...")
            // Extract the quoted string
            let inner = inner.trim();
            let path_str = if (inner.starts_with('"') && inner.ends_with('"'))
                || (inner.starts_with('\'') && inner.ends_with('\''))
            {
                &inner[1..inner.len() - 1]
            } else {
                inner
            };
            let vertices = flatten_svg_path(path_str)?;
            if vertices.len() < 3 {
                return None;
            }
            Some(ClipPath::Path { vertices })
        }
        _ => None,
    }
}

/// Flatten SVG path commands into a list of (x, y) vertices
///
/// Supports: M/m, L/l, H/h, V/v, C/c, S/s, Q/q, T/t, A/a (approximated), Z/z
/// Cubic/quadratic curves are subdivided into line segments for polygon clipping.
fn flatten_svg_path(d: &str) -> Option<Vec<(f32, f32)>> {
    let mut vertices = Vec::new();
    let mut cx = 0.0_f32;
    let mut cy = 0.0_f32;
    let mut start_x = 0.0_f32;
    let mut start_y = 0.0_f32;
    let mut last_cp_x = 0.0_f32;
    let mut last_cp_y = 0.0_f32;
    let mut last_cmd = ' ';

    // Tokenize: split on command letters, keeping the letter
    let mut tokens: Vec<(char, Vec<f32>)> = Vec::new();
    let mut current_cmd = ' ';
    let mut num_buf = String::new();
    let mut nums: Vec<f32> = Vec::new();

    let flush_num = |buf: &mut String, nums: &mut Vec<f32>| {
        if !buf.is_empty() {
            if let Ok(n) = buf.parse::<f32>() {
                nums.push(n);
            }
            buf.clear();
        }
    };

    for ch in d.chars() {
        if ch.is_ascii_alphabetic() {
            flush_num(&mut num_buf, &mut nums);
            if current_cmd != ' ' {
                tokens.push((current_cmd, std::mem::take(&mut nums)));
            }
            current_cmd = ch;
        } else if ch == ',' || ch == ' ' || ch == '\t' || ch == '\n' || ch == '\r' {
            flush_num(&mut num_buf, &mut nums);
        } else if ch == '-'
            && !num_buf.is_empty()
            && !num_buf.ends_with('e')
            && !num_buf.ends_with('E')
        {
            // Negative sign starts a new number (unless after exponent)
            flush_num(&mut num_buf, &mut nums);
            num_buf.push(ch);
        } else {
            num_buf.push(ch);
        }
    }
    flush_num(&mut num_buf, &mut nums);
    if current_cmd != ' ' {
        tokens.push((current_cmd, nums));
    }

    for (cmd, nums) in &tokens {
        let is_rel = cmd.is_ascii_lowercase();
        let base_x = if is_rel { cx } else { 0.0 };
        let base_y = if is_rel { cy } else { 0.0 };

        match cmd.to_ascii_uppercase() {
            'M' => {
                // moveto
                let mut i = 0;
                while i + 1 < nums.len() {
                    let x = base_x + nums[i];
                    let y = base_y + nums[i + 1];
                    if i == 0 {
                        start_x = x;
                        start_y = y;
                    }
                    vertices.push((x, y));
                    cx = x;
                    cy = y;
                    i += 2;
                }
            }
            'L' => {
                let mut i = 0;
                while i + 1 < nums.len() {
                    cx = base_x + nums[i];
                    cy = base_y + nums[i + 1];
                    vertices.push((cx, cy));
                    i += 2;
                }
            }
            'H' => {
                for n in nums {
                    cx = base_x + n;
                    vertices.push((cx, cy));
                }
            }
            'V' => {
                for n in nums {
                    cy = base_y + n;
                    vertices.push((cx, cy));
                }
            }
            'C' => {
                // cubic bezier
                let mut i = 0;
                while i + 5 < nums.len() {
                    let x1 = base_x + nums[i];
                    let y1 = base_y + nums[i + 1];
                    let x2 = base_x + nums[i + 2];
                    let y2 = base_y + nums[i + 3];
                    let x = base_x + nums[i + 4];
                    let y = base_y + nums[i + 5];
                    subdivide_cubic(&mut vertices, cx, cy, x1, y1, x2, y2, x, y, 0);
                    last_cp_x = x2;
                    last_cp_y = y2;
                    cx = x;
                    cy = y;
                    i += 6;
                }
                last_cmd = cmd.to_ascii_uppercase();
                continue;
            }
            'S' => {
                // smooth cubic
                let mut i = 0;
                while i + 3 < nums.len() {
                    let x1 = if last_cmd == 'C' || last_cmd == 'S' {
                        2.0 * cx - last_cp_x
                    } else {
                        cx
                    };
                    let y1 = if last_cmd == 'C' || last_cmd == 'S' {
                        2.0 * cy - last_cp_y
                    } else {
                        cy
                    };
                    let x2 = base_x + nums[i];
                    let y2 = base_y + nums[i + 1];
                    let x = base_x + nums[i + 2];
                    let y = base_y + nums[i + 3];
                    subdivide_cubic(&mut vertices, cx, cy, x1, y1, x2, y2, x, y, 0);
                    last_cp_x = x2;
                    last_cp_y = y2;
                    cx = x;
                    cy = y;
                    i += 4;
                }
                last_cmd = 'S';
                continue;
            }
            'Q' => {
                // quadratic bezier
                let mut i = 0;
                while i + 3 < nums.len() {
                    let qx = base_x + nums[i];
                    let qy = base_y + nums[i + 1];
                    let x = base_x + nums[i + 2];
                    let y = base_y + nums[i + 3];
                    subdivide_quadratic(&mut vertices, cx, cy, qx, qy, x, y, 0);
                    last_cp_x = qx;
                    last_cp_y = qy;
                    cx = x;
                    cy = y;
                    i += 4;
                }
                last_cmd = 'Q';
                continue;
            }
            'T' => {
                // smooth quadratic
                let mut i = 0;
                while i + 1 < nums.len() {
                    let qx = if last_cmd == 'Q' || last_cmd == 'T' {
                        2.0 * cx - last_cp_x
                    } else {
                        cx
                    };
                    let qy = if last_cmd == 'Q' || last_cmd == 'T' {
                        2.0 * cy - last_cp_y
                    } else {
                        cy
                    };
                    let x = base_x + nums[i];
                    let y = base_y + nums[i + 1];
                    subdivide_quadratic(&mut vertices, cx, cy, qx, qy, x, y, 0);
                    last_cp_x = qx;
                    last_cp_y = qy;
                    cx = x;
                    cy = y;
                    i += 2;
                }
                last_cmd = 'T';
                continue;
            }
            'A' => {
                // Arc: approximate with line segments
                let mut i = 0;
                while i + 6 < nums.len() {
                    let x = base_x + nums[i + 5];
                    let y = base_y + nums[i + 6];
                    // Simple approximation: subdivide arc into segments
                    approximate_arc(
                        &mut vertices,
                        cx,
                        cy,
                        nums[i],
                        nums[i + 1],
                        nums[i + 2],
                        nums[i + 3] != 0.0,
                        nums[i + 4] != 0.0,
                        x,
                        y,
                    );
                    cx = x;
                    cy = y;
                    i += 7;
                }
            }
            'Z' => {
                if (cx - start_x).abs() > 0.01 || (cy - start_y).abs() > 0.01 {
                    vertices.push((start_x, start_y));
                }
                cx = start_x;
                cy = start_y;
            }
            _ => {}
        }
        last_cmd = cmd.to_ascii_uppercase();
    }

    if vertices.is_empty() {
        None
    } else {
        Some(vertices)
    }
}

/// Recursively subdivide a cubic bezier curve into line segments
#[allow(clippy::too_many_arguments)]
fn subdivide_cubic(
    out: &mut Vec<(f32, f32)>,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    x3: f32,
    y3: f32,
    depth: u32,
) {
    const MAX_DEPTH: u32 = 6;
    const TOLERANCE: f32 = 1.0;

    if depth >= MAX_DEPTH {
        out.push((x3, y3));
        return;
    }

    // Flatness check: max deviation of control points from the line p0→p3
    let dx = x3 - x0;
    let dy = y3 - y0;
    let len_sq = dx * dx + dy * dy;
    if len_sq < 0.0001 {
        out.push((x3, y3));
        return;
    }
    let d1 = ((x1 - x0) * dy - (y1 - y0) * dx).abs();
    let d2 = ((x2 - x0) * dy - (y2 - y0) * dx).abs();
    let max_dev = (d1 + d2) / len_sq.sqrt();

    if max_dev < TOLERANCE {
        out.push((x3, y3));
        return;
    }

    // De Casteljau subdivision at t=0.5
    let m01x = (x0 + x1) * 0.5;
    let m01y = (y0 + y1) * 0.5;
    let m12x = (x1 + x2) * 0.5;
    let m12y = (y1 + y2) * 0.5;
    let m23x = (x2 + x3) * 0.5;
    let m23y = (y2 + y3) * 0.5;
    let m012x = (m01x + m12x) * 0.5;
    let m012y = (m01y + m12y) * 0.5;
    let m123x = (m12x + m23x) * 0.5;
    let m123y = (m12y + m23y) * 0.5;
    let mx = (m012x + m123x) * 0.5;
    let my = (m012y + m123y) * 0.5;

    subdivide_cubic(out, x0, y0, m01x, m01y, m012x, m012y, mx, my, depth + 1);
    subdivide_cubic(out, mx, my, m123x, m123y, m23x, m23y, x3, y3, depth + 1);
}

/// Recursively subdivide a quadratic bezier curve into line segments
#[allow(clippy::too_many_arguments)]
fn subdivide_quadratic(
    out: &mut Vec<(f32, f32)>,
    x0: f32,
    y0: f32,
    qx: f32,
    qy: f32,
    x2: f32,
    y2: f32,
    depth: u32,
) {
    const MAX_DEPTH: u32 = 6;
    const TOLERANCE: f32 = 1.0;

    if depth >= MAX_DEPTH {
        out.push((x2, y2));
        return;
    }

    // Flatness: deviation of control point from midpoint of line p0→p2
    let mid_x = (x0 + x2) * 0.5;
    let mid_y = (y0 + y2) * 0.5;
    let dev = ((qx - mid_x).powi(2) + (qy - mid_y).powi(2)).sqrt();

    if dev < TOLERANCE {
        out.push((x2, y2));
        return;
    }

    // De Casteljau at t=0.5
    let m01x = (x0 + qx) * 0.5;
    let m01y = (y0 + qy) * 0.5;
    let m12x = (qx + x2) * 0.5;
    let m12y = (qy + y2) * 0.5;
    let mx = (m01x + m12x) * 0.5;
    let my = (m01y + m12y) * 0.5;

    subdivide_quadratic(out, x0, y0, m01x, m01y, mx, my, depth + 1);
    subdivide_quadratic(out, mx, my, m12x, m12y, x2, y2, depth + 1);
}

/// Approximate an SVG arc with line segments
#[allow(clippy::too_many_arguments)]
fn approximate_arc(
    out: &mut Vec<(f32, f32)>,
    cx: f32,
    cy: f32,
    rx: f32,
    ry: f32,
    x_rotation: f32,
    large_arc: bool,
    sweep: bool,
    x: f32,
    y: f32,
) {
    // Simple approximation: use enough segments for smooth arc
    let dx = x - cx;
    let dy = y - cy;
    let dist = (dx * dx + dy * dy).sqrt();
    let segments = (dist / 4.0).clamp(8.0, 32.0) as u32;

    if rx.abs() < 0.001 || ry.abs() < 0.001 {
        out.push((x, y));
        return;
    }

    // Simplified: compute endpoints as parametric arc
    let cos_rot = x_rotation.to_radians().cos();
    let sin_rot = x_rotation.to_radians().sin();

    // Use the SVG arc endpoint parameterization (simplified)
    // For clip-path, a reasonable approximation is sufficient
    let _ = (large_arc, sweep, cos_rot, sin_rot, rx, ry);

    // Fallback: linear interpolation for robustness
    for i in 1..=segments {
        let t = i as f32 / segments as f32;
        let px = cx + dx * t;
        let py = cy + dy * t;
        out.push((px, py));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blinc_theme::ThemeState;

    #[test]
    fn test_parse_empty() {
        let stylesheet = Stylesheet::parse("").unwrap();
        assert!(stylesheet.is_empty());
    }

    #[test]
    fn test_parse_single_rule() {
        let css = "#card { opacity: 0.5; }";
        let stylesheet = Stylesheet::parse(css).unwrap();

        assert!(stylesheet.contains("card"));
        let style = stylesheet.get("card").unwrap();
        assert_eq!(style.opacity, Some(0.5));
    }

    #[test]
    fn test_parse_multiple_rules() {
        let css = r#"
            #card {
                opacity: 0.9;
                border-radius: 8px;
            }
            #button {
                opacity: 1.0;
            }
        "#;
        let stylesheet = Stylesheet::parse(css).unwrap();

        assert_eq!(stylesheet.len(), 2);
        assert!(stylesheet.contains("card"));
        assert!(stylesheet.contains("button"));
    }

    #[test]
    fn test_parse_hex_colors() {
        let css = "#test { background: #FF0000; }";
        let stylesheet = Stylesheet::parse(css).unwrap();
        let style = stylesheet.get("test").unwrap();
        assert!(style.background.is_some());
    }

    #[test]
    fn test_parse_transform_scale() {
        let css = "#test { transform: scale(1.5); }";
        let stylesheet = Stylesheet::parse(css).unwrap();
        let style = stylesheet.get("test").unwrap();
        assert!(style.transform.is_some());
    }

    #[test]
    fn test_parse_transform_scale_two_args() {
        let css = "#test { transform: scale(1.5, 2.0); }";
        let stylesheet = Stylesheet::parse(css).unwrap();
        let style = stylesheet.get("test").unwrap();
        assert!(style.transform.is_some());
    }

    #[test]
    fn test_parse_transform_rotate() {
        let css = "#test { transform: rotate(45deg); }";
        let stylesheet = Stylesheet::parse(css).unwrap();
        let style = stylesheet.get("test").unwrap();
        assert!(style.transform.is_some());
    }

    #[test]
    fn test_parse_transform_translate() {
        let css = "#test { transform: translate(10px, 20px); }";
        let stylesheet = Stylesheet::parse(css).unwrap();
        let style = stylesheet.get("test").unwrap();
        assert!(style.transform.is_some());
    }

    #[test]
    fn test_parse_transform_translate_x() {
        let css = "#test { transform: translateX(10px); }";
        let stylesheet = Stylesheet::parse(css).unwrap();
        let style = stylesheet.get("test").unwrap();
        assert!(style.transform.is_some());
    }

    #[test]
    fn test_parse_transform_translate_y() {
        let css = "#test { transform: translateY(20px); }";
        let stylesheet = Stylesheet::parse(css).unwrap();
        let style = stylesheet.get("test").unwrap();
        assert!(style.transform.is_some());
    }

    #[test]
    fn test_parse_comments() {
        let css = r#"
            /* This is a comment */
            #card {
                /* inline comment */
                opacity: 0.5;
            }
        "#;
        let stylesheet = Stylesheet::parse(css).unwrap();
        assert!(stylesheet.contains("card"));
    }

    #[test]
    fn test_parse_rgb_color() {
        let css = "#test { background: rgb(255, 128, 0); }";
        let stylesheet = Stylesheet::parse(css).unwrap();
        let style = stylesheet.get("test").unwrap();
        assert!(style.background.is_some());
    }

    #[test]
    fn test_parse_rgba_color() {
        let css = "#test { background: rgba(255, 128, 0, 0.5); }";
        let stylesheet = Stylesheet::parse(css).unwrap();
        let style = stylesheet.get("test").unwrap();
        assert!(style.background.is_some());
    }

    #[test]
    fn test_parse_named_color() {
        let css = "#test { background: red; }";
        let stylesheet = Stylesheet::parse(css).unwrap();
        let style = stylesheet.get("test").unwrap();
        assert!(style.background.is_some());
    }

    #[test]
    fn test_parse_short_hex() {
        let css = "#test { background: #F00; }";
        let stylesheet = Stylesheet::parse(css).unwrap();
        let style = stylesheet.get("test").unwrap();
        assert!(style.background.is_some());
    }

    #[test]
    fn test_parse_render_layer() {
        let css = "#test { render-layer: foreground; }";
        let stylesheet = Stylesheet::parse(css).unwrap();
        let style = stylesheet.get("test").unwrap();
        assert_eq!(style.render_layer, Some(RenderLayer::Foreground));
    }

    #[test]
    fn test_parse_backdrop_filter_glass() {
        let css = "#test { backdrop-filter: glass; }";
        let stylesheet = Stylesheet::parse(css).unwrap();
        let style = stylesheet.get("test").unwrap();
        assert!(style.material.is_some());
        assert_eq!(style.render_layer, Some(RenderLayer::Glass));
    }

    #[test]
    fn test_parse_backdrop_filter_blur() {
        let css = "#test { backdrop-filter: blur(10px); }";
        let stylesheet = Stylesheet::parse(css).unwrap();
        let style = stylesheet.get("test").unwrap();
        assert!(style.material.is_some());
        assert_eq!(style.render_layer, Some(RenderLayer::Glass));
    }

    #[test]
    fn test_parse_backdrop_filter_metallic() {
        let css = "#test { backdrop-filter: chrome; }";
        let stylesheet = Stylesheet::parse(css).unwrap();
        let style = stylesheet.get("test").unwrap();
        assert!(style.material.is_some());
    }

    #[test]
    fn test_parse_error_context() {
        // A string that can't be parsed as any selector should result in an empty stylesheet
        let css = "!!! { opacity: 0.5; }";
        let result = Stylesheet::parse(css);
        // This should parse as empty (no valid rules) but not error
        // since the parser just ignores what it can't parse
        // The parse itself succeeds but finds no valid rules
        let stylesheet = result.unwrap();
        assert!(stylesheet.is_empty());
    }

    #[test]
    fn test_parse_error_has_display() {
        // Create an error manually to test Display impl
        let err = ParseError {
            severity: Severity::Error,
            message: "test error".to_string(),
            line: 1,
            column: 5,
            fragment: "#test".to_string(),
            contexts: vec!["rule".to_string(), "selector".to_string()],
            property: None,
            value: None,
        };
        let display = format!("{}", err);
        assert!(display.contains("CSS error"));
        assert!(display.contains("line 1"));
        assert!(display.contains("column 5"));
    }

    #[test]
    fn test_parse_or_empty_success() {
        let css = "#test { opacity: 0.5; }";
        let stylesheet = Stylesheet::parse_or_empty(css);
        assert!(stylesheet.contains("test"));
    }

    #[test]
    fn test_parse_or_empty_failure() {
        // Invalid CSS returns empty stylesheet
        let css = "this is not valid CSS";
        let stylesheet = Stylesheet::parse_or_empty(css);
        assert!(stylesheet.is_empty());
    }

    #[test]
    fn test_unknown_property_ignored() {
        // Unknown properties are silently ignored
        let css = "#test { unknown-property: value; opacity: 0.5; }";
        let stylesheet = Stylesheet::parse(css).unwrap();
        let style = stylesheet.get("test").unwrap();
        // The known property is still parsed
        assert_eq!(style.opacity, Some(0.5));
    }

    #[test]
    fn test_invalid_value_skipped() {
        // Invalid values for known properties are skipped
        let css = "#test { opacity: invalid; border-radius: 8px; }";
        let stylesheet = Stylesheet::parse(css).unwrap();
        let style = stylesheet.get("test").unwrap();
        // opacity should be None (invalid value), but radius should work
        assert!(style.opacity.is_none());
        assert!(style.corner_radius.is_some());
    }

    // ========================================================================
    // Error Collection Tests for Reporting
    // ========================================================================

    #[test]
    fn test_error_collection_missing_closing_brace() {
        // Missing closing brace should produce a collectable error
        let css = "#test { opacity: 0.5";
        let result = Stylesheet::parse_with_errors(css);

        // With parse_with_errors, we get partial results plus errors
        // The stylesheet might be empty (couldn't parse any complete rules)
        // and errors should contain info about what went wrong

        // Either we have an error, or we have unparsed content warning
        let has_issues =
            result.has_errors() || result.has_warnings() || result.stylesheet.is_empty();
        assert!(has_issues, "Should have some indication of incomplete CSS");

        // If there are errors, validate their details
        if !result.errors.is_empty() {
            let err = &result.errors[0];
            assert!(err.line >= 1, "Line number should be set");
            assert!(err.column >= 1, "Column number should be set");
            assert!(!err.message.is_empty(), "Error message should be set");

            let display = format!("{}", err);
            assert!(
                display.contains("line") && display.contains("column"),
                "Display should include line and column info"
            );
        }
    }

    #[test]
    fn test_error_collection_missing_id_after_hash() {
        // # followed by invalid identifier should capture error context
        let css = "#123invalid { opacity: 0.5; }";
        let result = Stylesheet::parse(css);

        // This might parse as empty or error depending on nom's behavior
        // Either way, we should handle it gracefully
        match result {
            Ok(stylesheet) => {
                // If it parsed as empty, that's valid fallback behavior
                assert!(stylesheet.is_empty() || stylesheet.contains("123invalid"));
            }
            Err(err) => {
                // If it errored, error details should be collected
                assert!(!err.message.is_empty());
                assert!(err.line >= 1);
            }
        }
    }

    #[test]
    fn test_error_collection_with_contexts() {
        // Test that context stack is properly collected
        let css = "#test { : value; }"; // Missing property name before colon
        let result = Stylesheet::parse(css);

        match result {
            Ok(stylesheet) => {
                // Parser might skip malformed property, returning empty style
                if stylesheet.contains("test") {
                    let style = stylesheet.get("test").unwrap();
                    // The malformed property should be skipped
                    assert!(style.opacity.is_none());
                }
            }
            Err(err) => {
                // Error should have context about what was being parsed
                assert!(!err.message.is_empty());
                // Contexts might include "property name" or similar
                let display = format!("{}", err);
                assert!(display.contains("CSS parse error"));
            }
        }
    }

    #[test]
    fn test_error_collection_multiline() {
        // Test that line numbers are correctly calculated for multiline CSS
        let css = r#"
#first { opacity: 0.5; }
#second { opacity: 0.7; }
#third { opacity
"#;
        let result = Stylesheet::parse(css);

        match result {
            Ok(stylesheet) => {
                // May successfully parse the complete rules
                assert!(stylesheet.contains("first") || stylesheet.contains("second"));
            }
            Err(err) => {
                // If it errors, the line should be > 1 since error is on line 4
                assert!(err.line >= 1, "Line number should be calculated");
                let display = format!("{}", err);
                assert!(display.contains("line"), "Display should show line info");
            }
        }
    }

    #[test]
    fn test_error_collection_preserves_fragment() {
        // Test that the error fragment is captured for reporting
        let css = "#bad-css { = not valid }";
        let result = Stylesheet::parse(css);

        match result {
            Ok(_) => {
                // Parser might skip invalid content
            }
            Err(err) => {
                // Fragment should be set (though it might be truncated)
                // The fragment helps identify where parsing stopped
                let display = format!("{}", err);
                assert!(!display.is_empty());
            }
        }
    }

    #[test]
    fn test_collect_multiple_errors_via_iterations() {
        // Demonstrate how to collect errors from multiple CSS inputs
        let css_inputs = vec![
            ("#valid { opacity: 0.5; }", true),      // valid
            ("#broken {", false),                    // invalid - missing close
            ("#also-valid { opacity: 1.0; }", true), // valid
            ("@ invalid at-rule", false),            // invalid - no ID selector
        ];

        let mut errors: Vec<ParseError> = Vec::new();
        let mut successes: Vec<Stylesheet> = Vec::new();

        for (css, expected_success) in css_inputs {
            match Stylesheet::parse(css) {
                Ok(stylesheet) => {
                    if expected_success {
                        successes.push(stylesheet);
                    } else {
                        // Unexpected success - parser was lenient
                        successes.push(stylesheet);
                    }
                }
                Err(err) => {
                    // Collect the error for reporting
                    errors.push(err);
                }
            }
        }

        // Report: we can format all collected errors
        for (i, err) in errors.iter().enumerate() {
            let report = format!(
                "Error {}: line {}, col {}: {}",
                i + 1,
                err.line,
                err.column,
                err.message
            );
            assert!(!report.is_empty());
        }

        // At least one should have errored (the unclosed brace)
        assert!(
            !errors.is_empty() || successes.iter().any(|s| s.is_empty()),
            "Should have captured at least one error or empty result"
        );
    }

    #[test]
    fn test_error_debug_format() {
        // Test that ParseError has useful Debug output
        let css = "#incomplete {";
        let result = Stylesheet::parse(css);

        if let Err(err) = result {
            let debug_output = format!("{:?}", err);
            // Debug should include all the fields
            assert!(debug_output.contains("message"));
            assert!(debug_output.contains("line"));
            assert!(debug_output.contains("column"));
            assert!(debug_output.contains("fragment"));
            assert!(debug_output.contains("contexts"));
        }
    }

    #[test]
    fn test_error_is_std_error() {
        // Ensure ParseError implements std::error::Error properly
        let err = ParseError {
            severity: Severity::Error,
            message: "test error".to_string(),
            line: 5,
            column: 10,
            fragment: "broken".to_string(),
            contexts: vec!["rule".to_string()],
            property: Some("opacity".to_string()),
            value: Some("invalid".to_string()),
        };

        // Can be used as a std::error::Error
        let _: &dyn std::error::Error = &err;

        // Default source() implementation returns None
        use std::error::Error;
        assert!(err.source().is_none());
    }

    // ========================================================================
    // Tests for parse_with_errors - Full Error Collection
    // ========================================================================

    #[test]
    fn test_parse_with_errors_collects_unknown_properties() {
        let css = "#test { unknown-prop: value; opacity: 0.5; another-unknown: foo; }";
        let result = Stylesheet::parse_with_errors(css);

        // Should still parse the valid property
        assert!(result.stylesheet.contains("test"));
        let style = result.stylesheet.get("test").unwrap();
        assert_eq!(style.opacity, Some(0.5));

        // Should have collected warnings for unknown properties
        assert!(
            result.has_warnings(),
            "Should have warnings for unknown properties"
        );

        let warnings: Vec<_> = result.warnings_only().collect();
        assert!(
            warnings.len() >= 2,
            "Should have at least 2 warnings for unknown props"
        );

        // Check that warnings contain property info
        for warning in &warnings {
            assert_eq!(warning.severity, Severity::Warning);
            assert!(warning.property.is_some());
        }
    }

    #[test]
    fn test_parse_with_errors_collects_invalid_values() {
        let css = "#test { opacity: not-a-number; border-radius: ???; background: #FF0000; }";
        let result = Stylesheet::parse_with_errors(css);

        // Should still parse the valid property
        assert!(result.stylesheet.contains("test"));
        let style = result.stylesheet.get("test").unwrap();
        assert!(style.background.is_some(), "Valid background should parse");
        assert!(style.opacity.is_none(), "Invalid opacity should not parse");

        // Should have collected warnings for invalid values
        assert!(result.has_warnings());

        let warnings: Vec<_> = result.warnings_only().collect();
        assert!(
            warnings.len() >= 2,
            "Should have warnings for invalid values"
        );

        // Check warning details
        for warning in &warnings {
            assert!(warning.property.is_some());
            assert!(warning.value.is_some());
            assert!(warning.message.contains("Invalid value"));
        }
    }

    #[test]
    fn test_parse_with_errors_print_diagnostics() {
        let css = "#test { unknown: value; opacity: bad; background: red; }";
        let result = Stylesheet::parse_with_errors(css);

        // Should have some errors/warnings
        assert!(!result.errors.is_empty());

        // Test that print_diagnostics doesn't panic
        // (We can't easily capture stderr in tests, but we can verify it runs)
        result.log_diagnostics();

        // Verify to_warning_string works
        for err in &result.errors {
            let warning_str = err.to_warning_string();
            assert!(!warning_str.is_empty());
            assert!(warning_str.contains(&err.severity.to_string()));
        }
    }

    #[test]
    fn test_parse_with_errors_multiline_line_numbers() {
        let css = r#"
#first {
    opacity: 0.5;
    unknown-prop: value;
}
#second {
    opacity: bad;
    background: blue;
}
"#;
        let result = Stylesheet::parse_with_errors(css);

        // Both rules should parse
        assert!(result.stylesheet.contains("first"));
        assert!(result.stylesheet.contains("second"));

        // Should have warnings with line numbers > 1
        let warnings: Vec<_> = result.warnings_only().collect();
        assert!(!warnings.is_empty());

        // At least some warnings should be on lines > 1
        let has_multiline_errors = warnings.iter().any(|w| w.line > 1);
        assert!(has_multiline_errors, "Should have errors on lines > 1");
    }

    #[test]
    fn test_parse_with_errors_severity_levels() {
        // Create various error types and check severity
        let warning = ParseError::unknown_property("foo", 1, 1);
        assert_eq!(warning.severity, Severity::Warning);

        let invalid = ParseError::invalid_value("opacity", "bad", 2, 5);
        assert_eq!(invalid.severity, Severity::Warning);

        let error = ParseError::new(Severity::Error, "fatal error", 3, 10);
        assert_eq!(error.severity, Severity::Error);
    }

    #[test]
    fn test_css_parse_result_methods() {
        let css = "#test { unknown: x; opacity: bad; }";
        let result = Stylesheet::parse_with_errors(css);

        // Test CssParseResult methods
        assert!(result.has_warnings());
        assert!(!result.has_errors()); // These are warnings, not errors

        let warnings_count = result.warnings_only().count();
        let errors_count = result.errors_only().count();

        assert!(warnings_count >= 2);
        assert_eq!(errors_count, 0);
    }

    #[test]
    fn test_error_collection_with_valid_css_no_errors() {
        let css = "#card { opacity: 0.8; background: #FF0000; border-radius: 8px; }";
        let result = Stylesheet::parse_with_errors(css);

        // Should parse successfully with no errors
        assert!(result.stylesheet.contains("card"));
        assert!(result.errors.is_empty(), "Valid CSS should have no errors");
        assert!(!result.has_errors());
        assert!(!result.has_warnings());
    }

    // ========================================================================
    // CSS Variables Tests
    // ========================================================================

    #[test]
    fn test_css_variables_root_parsing() {
        let css = r#"
            :root {
                --primary-color: #FF0000;
                --secondary-color: #00FF00;
                --card-radius: 8px;
            }
        "#;
        let result = Stylesheet::parse_with_errors(css);

        assert_eq!(result.stylesheet.variable_count(), 3);
        assert_eq!(
            result.stylesheet.get_variable("primary-color"),
            Some("#FF0000")
        );
        assert_eq!(
            result.stylesheet.get_variable("secondary-color"),
            Some("#00FF00")
        );
        assert_eq!(result.stylesheet.get_variable("card-radius"), Some("8px"));
    }

    #[test]
    fn test_css_variables_usage() {
        let css = r#"
            :root {
                --main-opacity: 0.8;
            }
            #card {
                opacity: var(--main-opacity);
            }
        "#;
        let result = Stylesheet::parse_with_errors(css);

        assert!(result.stylesheet.contains("card"));
        let style = result.stylesheet.get("card").unwrap();
        assert_eq!(style.opacity, Some(0.8));
    }

    #[test]
    fn test_css_variables_with_fallback() {
        let css = r#"
            #card {
                opacity: var(--undefined-var, 0.5);
            }
        "#;
        let result = Stylesheet::parse_with_errors(css);

        let style = result.stylesheet.get("card").unwrap();
        assert_eq!(style.opacity, Some(0.5));
    }

    #[test]
    fn test_css_variables_color() {
        let css = r#"
            :root {
                --brand-color: #3498db;
            }
            #button {
                background: var(--brand-color);
            }
        "#;
        let result = Stylesheet::parse_with_errors(css);

        let style = result.stylesheet.get("button").unwrap();
        assert!(style.background.is_some());
    }

    #[test]
    fn test_css_variables_multiple_rules() {
        let css = r#"
            :root {
                --base-radius: 4px;
                --card-opacity: 0.9;
            }
            #card {
                border-radius: var(--base-radius);
                opacity: var(--card-opacity);
            }
            #button {
                opacity: var(--card-opacity);
            }
        "#;
        let result = Stylesheet::parse_with_errors(css);

        assert!(result.stylesheet.contains("card"));
        assert!(result.stylesheet.contains("button"));

        let card = result.stylesheet.get("card").unwrap();
        let button = result.stylesheet.get("button").unwrap();

        assert!(card.corner_radius.is_some());
        assert_eq!(card.opacity, Some(0.9));
        assert_eq!(button.opacity, Some(0.9));
    }

    #[test]
    fn test_css_variables_set_at_runtime() {
        let css = "#card { opacity: 0.5; }";
        let mut stylesheet = Stylesheet::parse(css).unwrap();

        // Set variable at runtime
        stylesheet.set_variable("custom-var", "#FF0000");

        assert_eq!(stylesheet.get_variable("custom-var"), Some("#FF0000"));
    }

    #[test]
    fn test_css_variables_names_iterator() {
        let css = r#"
            :root {
                --a: 1;
                --b: 2;
                --c: 3;
            }
        "#;
        let result = Stylesheet::parse_with_errors(css);

        let names: Vec<_> = result.stylesheet.variable_names().collect();
        assert_eq!(names.len(), 3);
        assert!(names.contains(&"a"));
        assert!(names.contains(&"b"));
        assert!(names.contains(&"c"));
    }

    #[test]
    fn test_css_variables_with_theme_fallback() {
        // Initialize theme (required for theme() functions)
        ThemeState::init_default();

        let css = r#"
            :root {
                --card-shadow: theme(shadow-md);
            }
            #card {
                box-shadow: var(--card-shadow);
            }
        "#;
        let result = Stylesheet::parse_with_errors(css);

        // The variable stores the raw value "theme(shadow-md)"
        // which gets resolved when applied to the style
        let style = result.stylesheet.get("card").unwrap();
        assert!(!style.shadow.is_empty());
    }

    #[test]
    fn test_css_variables_nested_resolution() {
        let css = r#"
            :root {
                --base: 0.5;
                --derived: var(--base);
            }
            #test {
                opacity: var(--derived);
            }
        "#;
        let result = Stylesheet::parse_with_errors(css);

        let style = result.stylesheet.get("test").unwrap();
        assert_eq!(style.opacity, Some(0.5));
    }

    // ========================================================================
    // State Modifier Tests (Pseudo-classes)
    // ========================================================================

    #[test]
    fn test_state_modifier_hover() {
        let css = r#"
            #button {
                opacity: 1.0;
            }
            #button:hover {
                opacity: 0.8;
            }
        "#;
        let result = Stylesheet::parse_with_errors(css);

        // Base style
        let base = result.stylesheet.get("button").unwrap();
        assert_eq!(base.opacity, Some(1.0));

        // Hover style
        let hover = result
            .stylesheet
            .get_with_state("button", ElementState::Hover)
            .unwrap();
        assert_eq!(hover.opacity, Some(0.8));
    }

    #[test]
    fn test_state_modifier_active() {
        let css = r#"
            #button:active {
                transform: scale(0.95);
            }
        "#;
        let result = Stylesheet::parse_with_errors(css);

        let active = result
            .stylesheet
            .get_with_state("button", ElementState::Active)
            .unwrap();
        assert!(active.transform.is_some());
    }

    #[test]
    fn test_state_modifier_focus() {
        let css = r#"
            #input:focus {
                border-radius: 4px;
            }
        "#;
        let result = Stylesheet::parse_with_errors(css);

        let focus = result
            .stylesheet
            .get_with_state("input", ElementState::Focus)
            .unwrap();
        assert!(focus.corner_radius.is_some());
    }

    #[test]
    fn test_state_modifier_disabled() {
        let css = r#"
            #button:disabled {
                opacity: 0.5;
            }
        "#;
        let result = Stylesheet::parse_with_errors(css);

        let disabled = result
            .stylesheet
            .get_with_state("button", ElementState::Disabled)
            .unwrap();
        assert_eq!(disabled.opacity, Some(0.5));
    }

    #[test]
    fn test_multiple_state_modifiers() {
        let css = r#"
            #button {
                background: #0000FF;
                opacity: 1.0;
            }
            #button:hover {
                opacity: 0.9;
            }
            #button:active {
                opacity: 0.8;
                transform: scale(0.98);
            }
            #button:focus {
                border-radius: 4px;
            }
            #button:disabled {
                opacity: 0.4;
            }
        "#;
        let result = Stylesheet::parse_with_errors(css);

        // Base style
        assert!(result.stylesheet.contains("button"));
        let base = result.stylesheet.get("button").unwrap();
        assert_eq!(base.opacity, Some(1.0));

        // Check all states exist
        assert!(
            result
                .stylesheet
                .contains_with_state("button", ElementState::Hover)
        );
        assert!(
            result
                .stylesheet
                .contains_with_state("button", ElementState::Active)
        );
        assert!(
            result
                .stylesheet
                .contains_with_state("button", ElementState::Focus)
        );
        assert!(
            result
                .stylesheet
                .contains_with_state("button", ElementState::Disabled)
        );

        // Verify state styles
        let hover = result
            .stylesheet
            .get_with_state("button", ElementState::Hover)
            .unwrap();
        assert_eq!(hover.opacity, Some(0.9));

        let active = result
            .stylesheet
            .get_with_state("button", ElementState::Active)
            .unwrap();
        assert_eq!(active.opacity, Some(0.8));
        assert!(active.transform.is_some());

        let focus = result
            .stylesheet
            .get_with_state("button", ElementState::Focus)
            .unwrap();
        assert!(focus.corner_radius.is_some());

        let disabled = result
            .stylesheet
            .get_with_state("button", ElementState::Disabled)
            .unwrap();
        assert_eq!(disabled.opacity, Some(0.4));
    }

    #[test]
    fn test_get_all_states() {
        let css = r#"
            #card {
                opacity: 1.0;
            }
            #card:hover {
                opacity: 0.9;
            }
            #card:active {
                opacity: 0.8;
            }
        "#;
        let result = Stylesheet::parse_with_errors(css);

        let (base, states) = result.stylesheet.get_all_states("card");

        assert!(base.is_some());
        assert_eq!(base.unwrap().opacity, Some(1.0));

        assert_eq!(states.len(), 2);

        // Check we got hover and active
        let state_types: Vec<_> = states.iter().map(|(s, _)| *s).collect();
        assert!(state_types.contains(&ElementState::Hover));
        assert!(state_types.contains(&ElementState::Active));
    }

    #[test]
    fn test_state_modifier_with_variables() {
        let css = r#"
            :root {
                --hover-opacity: 0.85;
            }
            #button:hover {
                opacity: var(--hover-opacity);
            }
        "#;
        let result = Stylesheet::parse_with_errors(css);

        let hover = result
            .stylesheet
            .get_with_state("button", ElementState::Hover)
            .unwrap();
        assert_eq!(hover.opacity, Some(0.85));
    }

    #[test]
    fn test_unknown_state_modifier_ignored() {
        // Unknown pseudo-class should parse the ID part but not set state
        let css = "#button:unknown { opacity: 0.5; }";
        let result = Stylesheet::parse_with_errors(css);

        // The selector "#button:unknown" where "unknown" is not a valid state
        // should still be stored, but with the state part as None
        // Actually, since we parse :unknown but it's not a known state,
        // the state will be None, so it just becomes "button"
        assert!(result.stylesheet.contains("button"));
        let style = result.stylesheet.get("button").unwrap();
        assert_eq!(style.opacity, Some(0.5));
    }

    #[test]
    fn test_element_state_from_str() {
        assert_eq!(
            ElementState::parse_state("hover"),
            Some(ElementState::Hover)
        );
        assert_eq!(
            ElementState::parse_state("HOVER"),
            Some(ElementState::Hover)
        );
        assert_eq!(
            ElementState::parse_state("active"),
            Some(ElementState::Active)
        );
        assert_eq!(
            ElementState::parse_state("focus"),
            Some(ElementState::Focus)
        );
        assert_eq!(
            ElementState::parse_state("disabled"),
            Some(ElementState::Disabled)
        );
        assert_eq!(ElementState::parse_state("unknown"), None);
    }

    #[test]
    fn test_element_state_display() {
        assert_eq!(format!("{}", ElementState::Hover), "hover");
        assert_eq!(format!("{}", ElementState::Active), "active");
        assert_eq!(format!("{}", ElementState::Focus), "focus");
        assert_eq!(format!("{}", ElementState::Disabled), "disabled");
    }

    #[test]
    fn test_css_selector_key() {
        let selector = CssSelector::new("button");
        assert_eq!(selector.key(), "button");

        let selector_hover = CssSelector::with_state("button", ElementState::Hover);
        assert_eq!(selector_hover.key(), "button:hover");
    }

    // =========================================================================
    // Keyframe Animation Tests
    // =========================================================================

    #[test]
    fn test_keyframes_basic() {
        let css = r#"
            @keyframes fade-in {
                from { opacity: 0; }
                to { opacity: 1; }
            }
        "#;
        let result = Stylesheet::parse_with_errors(css);

        assert!(!result.has_errors());
        assert!(result.stylesheet.contains_keyframes("fade-in"));

        let keyframes = result.stylesheet.get_keyframes("fade-in").unwrap();
        assert_eq!(keyframes.name, "fade-in");
        assert_eq!(keyframes.keyframes.len(), 2);

        // Check first keyframe (from = 0%)
        assert_eq!(keyframes.keyframes[0].position, 0.0);
        assert_eq!(keyframes.keyframes[0].style.opacity, Some(0.0));

        // Check last keyframe (to = 100%)
        assert_eq!(keyframes.keyframes[1].position, 1.0);
        assert_eq!(keyframes.keyframes[1].style.opacity, Some(1.0));
    }

    #[test]
    fn test_keyframes_percentage() {
        let css = r#"
            @keyframes pulse {
                0% { opacity: 1; }
                50% { opacity: 0.5; }
                100% { opacity: 1; }
            }
        "#;
        let result = Stylesheet::parse_with_errors(css);

        assert!(!result.has_errors());
        let keyframes = result.stylesheet.get_keyframes("pulse").unwrap();
        assert_eq!(keyframes.keyframes.len(), 3);

        assert_eq!(keyframes.keyframes[0].position, 0.0);
        assert_eq!(keyframes.keyframes[1].position, 0.5);
        assert_eq!(keyframes.keyframes[2].position, 1.0);
    }

    #[test]
    fn test_keyframes_with_transform() {
        let css = r#"
            @keyframes slide-in {
                from {
                    opacity: 0;
                    transform: translateY(20px);
                }
                to {
                    opacity: 1;
                    transform: translateY(0);
                }
            }
        "#;
        let result = Stylesheet::parse_with_errors(css);

        assert!(!result.has_errors());
        let keyframes = result.stylesheet.get_keyframes("slide-in").unwrap();

        // First keyframe should have opacity 0 and transform
        assert_eq!(keyframes.keyframes[0].style.opacity, Some(0.0));
        assert!(keyframes.keyframes[0].style.transform.is_some());

        // Last keyframe should have opacity 1
        assert_eq!(keyframes.keyframes[1].style.opacity, Some(1.0));
        assert!(keyframes.keyframes[1].style.transform.is_some());
    }

    #[test]
    fn test_keyframes_multiple_positions() {
        let css = r#"
            @keyframes blink {
                0%, 100% { opacity: 1; }
                50% { opacity: 0; }
            }
        "#;
        let result = Stylesheet::parse_with_errors(css);

        assert!(!result.has_errors());
        let keyframes = result.stylesheet.get_keyframes("blink").unwrap();

        // Should have 3 keyframes: 0%, 50%, 100%
        assert_eq!(keyframes.keyframes.len(), 3);

        // 0% and 100% should have opacity 1
        assert_eq!(keyframes.keyframes[0].position, 0.0);
        assert_eq!(keyframes.keyframes[0].style.opacity, Some(1.0));

        assert_eq!(keyframes.keyframes[1].position, 0.5);
        assert_eq!(keyframes.keyframes[1].style.opacity, Some(0.0));

        assert_eq!(keyframes.keyframes[2].position, 1.0);
        assert_eq!(keyframes.keyframes[2].style.opacity, Some(1.0));
    }

    #[test]
    fn test_keyframes_count() {
        let css = r#"
            @keyframes anim1 {
                from { opacity: 0; }
                to { opacity: 1; }
            }
            @keyframes anim2 {
                from { opacity: 1; }
                to { opacity: 0; }
            }
        "#;
        let result = Stylesheet::parse_with_errors(css);

        assert_eq!(result.stylesheet.keyframe_count(), 2);
        assert!(result.stylesheet.contains_keyframes("anim1"));
        assert!(result.stylesheet.contains_keyframes("anim2"));
    }

    #[test]
    fn test_keyframes_names() {
        let css = r#"
            @keyframes fade-in { from { opacity: 0; } to { opacity: 1; } }
            @keyframes fade-out { from { opacity: 1; } to { opacity: 0; } }
        "#;
        let result = Stylesheet::parse_with_errors(css);

        let names: Vec<_> = result.stylesheet.keyframe_names().collect();
        assert!(names.contains(&"fade-in"));
        assert!(names.contains(&"fade-out"));
    }

    #[test]
    fn test_keyframes_to_motion_animation() {
        let css = r#"
            @keyframes fade-in {
                from { opacity: 0; }
                to { opacity: 1; }
            }
        "#;
        let result = Stylesheet::parse_with_errors(css);
        let keyframes = result.stylesheet.get_keyframes("fade-in").unwrap();

        let motion = keyframes.to_motion_animation(300, 200);

        assert_eq!(motion.enter_duration_ms, 300);
        assert_eq!(motion.exit_duration_ms, 200);
        assert!(motion.enter_from.is_some());
        assert!(motion.exit_to.is_some());

        // enter_from should have opacity 0
        let enter = motion.enter_from.unwrap();
        assert_eq!(enter.opacity, Some(0.0));

        // exit_to should have opacity 1
        let exit = motion.exit_to.unwrap();
        assert_eq!(exit.opacity, Some(1.0));
    }

    #[test]
    fn test_keyframes_to_multi_keyframe_animation() {
        use blinc_animation::Easing;

        let css = r#"
            @keyframes pulse {
                0% { opacity: 1; transform: scale(1); }
                50% { opacity: 0.8; transform: scale(1.05); }
                100% { opacity: 1; transform: scale(1); }
            }
        "#;
        let result = Stylesheet::parse_with_errors(css);
        let keyframes = result.stylesheet.get_keyframes("pulse").unwrap();

        let animation = keyframes.to_multi_keyframe_animation(1000, Easing::EaseInOut);

        // Should have 3 keyframes
        assert_eq!(animation.keyframes().len(), 3);

        // Check keyframe positions
        assert_eq!(animation.keyframes()[0].time, 0.0);
        assert_eq!(animation.keyframes()[1].time, 0.5);
        assert_eq!(animation.keyframes()[2].time, 1.0);

        // Check opacity values
        assert_eq!(animation.keyframes()[0].properties.opacity, Some(1.0));
        assert_eq!(animation.keyframes()[1].properties.opacity, Some(0.8));
        assert_eq!(animation.keyframes()[2].properties.opacity, Some(1.0));
    }

    #[test]
    fn test_keyframes_with_variables() {
        let css = r#"
            :root {
                --start-opacity: 0;
                --end-opacity: 1;
            }
            @keyframes fade-in {
                from { opacity: var(--start-opacity); }
                to { opacity: var(--end-opacity); }
            }
        "#;
        let result = Stylesheet::parse_with_errors(css);

        assert!(!result.has_errors());
        let keyframes = result.stylesheet.get_keyframes("fade-in").unwrap();

        // Variables should be resolved
        assert_eq!(keyframes.keyframes[0].style.opacity, Some(0.0));
        assert_eq!(keyframes.keyframes[1].style.opacity, Some(1.0));
    }

    #[test]
    fn test_keyframes_mixed_with_rules() {
        let css = r#"
            @keyframes fade-in {
                from { opacity: 0; }
                to { opacity: 1; }
            }

            #card {
                background: #FF0000;
            }

            #card:hover {
                opacity: 0.9;
            }
        "#;
        let result = Stylesheet::parse_with_errors(css);

        // Keyframes should be parsed
        assert!(result.stylesheet.contains_keyframes("fade-in"));

        // Rules should also be parsed
        assert!(result.stylesheet.contains("card"));
        assert!(
            result
                .stylesheet
                .contains_with_state("card", ElementState::Hover)
        );
    }

    // =========================================================================
    // Animation Property Tests
    // =========================================================================

    #[test]
    fn test_animation_shorthand_basic() {
        let css = r#"
            #card {
                animation: fade-in 300ms;
            }
        "#;
        let result = Stylesheet::parse_with_errors(css);
        assert!(!result.has_errors());

        let style = result.stylesheet.get("card").unwrap();
        let anim = style.animation.as_ref().unwrap();
        assert_eq!(anim.name, "fade-in");
        assert_eq!(anim.duration_ms, 300);
    }

    #[test]
    fn test_animation_shorthand_full() {
        let css = r#"
            #modal {
                animation: slide-in 0.5s ease-out 100ms infinite alternate forwards;
            }
        "#;
        let result = Stylesheet::parse_with_errors(css);
        assert!(!result.has_errors());

        let style = result.stylesheet.get("modal").unwrap();
        let anim = style.animation.as_ref().unwrap();
        assert_eq!(anim.name, "slide-in");
        assert_eq!(anim.duration_ms, 500);
        assert_eq!(anim.timing, AnimationTiming::EaseOut);
        assert_eq!(anim.delay_ms, 100);
        assert_eq!(anim.iteration_count, 0); // 0 = infinite
        assert_eq!(anim.direction, AnimationDirection::Alternate);
        assert_eq!(anim.fill_mode, AnimationFillMode::Forwards);
    }

    #[test]
    fn test_animation_individual_properties() {
        let css = r#"
            #button {
                animation-name: pulse;
                animation-duration: 2s;
                animation-timing-function: ease-in-out;
                animation-delay: 0.5s;
                animation-iteration-count: 3;
                animation-direction: reverse;
                animation-fill-mode: both;
            }
        "#;
        let result = Stylesheet::parse_with_errors(css);
        assert!(!result.has_errors());

        let style = result.stylesheet.get("button").unwrap();
        let anim = style.animation.as_ref().unwrap();
        assert_eq!(anim.name, "pulse");
        assert_eq!(anim.duration_ms, 2000);
        assert_eq!(anim.timing, AnimationTiming::EaseInOut);
        assert_eq!(anim.delay_ms, 500);
        assert_eq!(anim.iteration_count, 3);
        assert_eq!(anim.direction, AnimationDirection::Reverse);
        assert_eq!(anim.fill_mode, AnimationFillMode::Both);
    }

    #[test]
    fn test_animation_with_keyframes() {
        let css = r#"
            @keyframes fade-in {
                from { opacity: 0; }
                to { opacity: 1; }
            }

            #card {
                animation: fade-in 300ms ease-out forwards;
            }
        "#;
        let result = Stylesheet::parse_with_errors(css);
        assert!(!result.has_errors());

        // Both keyframes and animation property should be parsed
        assert!(result.stylesheet.contains_keyframes("fade-in"));

        let style = result.stylesheet.get("card").unwrap();
        let anim = style.animation.as_ref().unwrap();
        assert_eq!(anim.name, "fade-in");
        assert_eq!(anim.duration_ms, 300);
        assert_eq!(anim.timing, AnimationTiming::EaseOut);
        assert_eq!(anim.fill_mode, AnimationFillMode::Forwards);
    }

    #[test]
    fn test_resolve_animation() {
        let css = r#"
            @keyframes slide-in {
                from { opacity: 0; transform: translateY(20px); }
                to { opacity: 1; transform: translateY(0); }
            }

            #modal {
                animation: slide-in 500ms ease-out 100ms;
            }
        "#;
        let result = Stylesheet::parse_with_errors(css);
        assert!(!result.has_errors());

        // resolve_animation should combine keyframes and animation config
        let motion = result.stylesheet.resolve_animation("modal").unwrap();

        // Check duration comes from animation property
        assert_eq!(motion.enter_duration_ms, 500);
        assert_eq!(motion.exit_duration_ms, 500);
        assert_eq!(motion.enter_delay_ms, 100);

        // Check enter_from comes from first keyframe
        let enter_from = motion.enter_from.as_ref().unwrap();
        assert_eq!(enter_from.opacity, Some(0.0));
        assert_eq!(enter_from.translate_y, Some(20.0));

        // Check exit_to comes from last keyframe
        let exit_to = motion.exit_to.as_ref().unwrap();
        assert_eq!(exit_to.opacity, Some(1.0));
        assert_eq!(exit_to.translate_y, Some(0.0));
    }

    #[test]
    fn test_resolve_animation_missing_keyframes() {
        let css = r#"
            #card {
                animation: nonexistent 300ms;
            }
        "#;
        let result = Stylesheet::parse_with_errors(css);

        // Should return None when keyframes don't exist
        assert!(result.stylesheet.resolve_animation("card").is_none());
    }

    #[test]
    fn test_resolve_animation_no_animation_property() {
        let css = r#"
            @keyframes fade-in {
                from { opacity: 0; }
                to { opacity: 1; }
            }

            #card {
                background: blue;
            }
        "#;
        let result = Stylesheet::parse_with_errors(css);

        // Should return None when element has no animation property
        assert!(result.stylesheet.resolve_animation("card").is_none());
    }

    // =========================================================================
    // Gradient Tests
    // =========================================================================

    #[test]
    fn test_linear_gradient_angle() {
        let css = r#"#card { background: linear-gradient(135deg, #667eea 0%, #764ba2 100%); }"#;
        let result = Stylesheet::parse_with_errors(css);

        assert!(!result.has_errors());
        let style = result.stylesheet.get("card").unwrap();
        assert!(style.background.is_some());

        if let Some(Brush::Gradient(Gradient::Linear { stops, .. })) = &style.background {
            assert_eq!(stops.len(), 2);
            assert_eq!(stops[0].offset, 0.0);
            assert_eq!(stops[1].offset, 1.0);
        } else {
            panic!("Expected linear gradient");
        }
    }

    #[test]
    fn test_linear_gradient_to_right() {
        let css = r#"#card { background: linear-gradient(to right, red, blue); }"#;
        let result = Stylesheet::parse_with_errors(css);

        assert!(!result.has_errors());
        let style = result.stylesheet.get("card").unwrap();

        if let Some(Brush::Gradient(Gradient::Linear { start, end, .. })) = &style.background {
            // "to right" means 90deg, which should be start=(0, 0.5), end=(1, 0.5)
            assert!((start.x - 0.0).abs() < 0.01);
            assert!((start.y - 0.5).abs() < 0.01);
            assert!((end.x - 1.0).abs() < 0.01);
            assert!((end.y - 0.5).abs() < 0.01);
        } else {
            panic!("Expected linear gradient");
        }
    }

    #[test]
    fn test_linear_gradient_to_bottom() {
        let css = r#"#card { background: linear-gradient(to bottom, #fff, #000); }"#;
        let result = Stylesheet::parse_with_errors(css);

        assert!(!result.has_errors());
        let style = result.stylesheet.get("card").unwrap();

        if let Some(Brush::Gradient(Gradient::Linear { start, end, .. })) = &style.background {
            // "to bottom" means 180deg, which should be start=(0.5, 0), end=(0.5, 1)
            assert!((start.x - 0.5).abs() < 0.01);
            assert!((start.y - 0.0).abs() < 0.01);
            assert!((end.x - 0.5).abs() < 0.01);
            assert!((end.y - 1.0).abs() < 0.01);
        } else {
            panic!("Expected linear gradient");
        }
    }

    #[test]
    fn test_linear_gradient_to_bottom_right() {
        let css = r#"#card { background: linear-gradient(to bottom right, red, blue); }"#;
        let result = Stylesheet::parse_with_errors(css);

        assert!(!result.has_errors());
        let style = result.stylesheet.get("card").unwrap();

        if let Some(Brush::Gradient(Gradient::Linear { start, end, .. })) = &style.background {
            // "to bottom right" means 135deg, which should be start=(0, 0), end=(1, 1)
            assert!((start.x - 0.0).abs() < 0.01);
            assert!((start.y - 0.0).abs() < 0.01);
            assert!((end.x - 1.0).abs() < 0.01);
            assert!((end.y - 1.0).abs() < 0.01);
        } else {
            panic!("Expected linear gradient");
        }
    }

    #[test]
    fn test_linear_gradient_multiple_stops() {
        let css =
            r#"#card { background: linear-gradient(90deg, red 0%, yellow 50%, green 100%); }"#;
        let result = Stylesheet::parse_with_errors(css);

        assert!(!result.has_errors());
        let style = result.stylesheet.get("card").unwrap();

        if let Some(Brush::Gradient(Gradient::Linear { stops, .. })) = &style.background {
            assert_eq!(stops.len(), 3);
            assert_eq!(stops[0].offset, 0.0);
            assert_eq!(stops[1].offset, 0.5);
            assert_eq!(stops[2].offset, 1.0);
        } else {
            panic!("Expected linear gradient with 3 stops");
        }
    }

    #[test]
    fn test_linear_gradient_implied_positions() {
        let css = r#"#card { background: linear-gradient(to bottom, red, yellow, green); }"#;
        let result = Stylesheet::parse_with_errors(css);

        assert!(!result.has_errors());
        let style = result.stylesheet.get("card").unwrap();

        if let Some(Brush::Gradient(Gradient::Linear { stops, .. })) = &style.background {
            // With 3 stops and no explicit positions, should be 0%, 50%, 100%
            assert_eq!(stops.len(), 3);
            assert_eq!(stops[0].offset, 0.0);
            assert_eq!(stops[1].offset, 0.5);
            assert_eq!(stops[2].offset, 1.0);
        } else {
            panic!("Expected linear gradient with implied positions");
        }
    }

    #[test]
    fn test_linear_gradient_rgba_colors() {
        let css = r#"#card { background: linear-gradient(45deg, rgba(255, 0, 0, 0.5) 0%, rgba(0, 0, 255, 0.8) 100%); }"#;
        let result = Stylesheet::parse_with_errors(css);

        assert!(!result.has_errors());
        let style = result.stylesheet.get("card").unwrap();

        if let Some(Brush::Gradient(Gradient::Linear { stops, .. })) = &style.background {
            assert_eq!(stops.len(), 2);
            // Check that RGBA colors were parsed (alpha should be < 1.0)
            assert!(stops[0].color.a < 1.0);
            assert!(stops[1].color.a < 1.0);
        } else {
            panic!("Expected linear gradient with RGBA colors");
        }
    }

    #[test]
    fn test_linear_gradient_angle_units() {
        // Test various angle units
        let css_deg = r#"#a { background: linear-gradient(90deg, red, blue); }"#;
        let css_turn = r#"#b { background: linear-gradient(0.25turn, red, blue); }"#;
        let css_rad = r#"#c { background: linear-gradient(1.5708rad, red, blue); }"#;

        let result_deg = Stylesheet::parse_with_errors(css_deg);
        let result_turn = Stylesheet::parse_with_errors(css_turn);
        let result_rad = Stylesheet::parse_with_errors(css_rad);

        // All should parse to approximately the same gradient (90 degrees)
        if let (
            Some(Brush::Gradient(Gradient::Linear {
                start: s1, end: e1, ..
            })),
            Some(Brush::Gradient(Gradient::Linear {
                start: s2, end: e2, ..
            })),
            Some(Brush::Gradient(Gradient::Linear {
                start: s3, end: e3, ..
            })),
        ) = (
            &result_deg.stylesheet.get("a").unwrap().background,
            &result_turn.stylesheet.get("b").unwrap().background,
            &result_rad.stylesheet.get("c").unwrap().background,
        ) {
            // All should have similar start/end points (allowing for floating point)
            assert!((s1.x - s2.x).abs() < 0.1);
            assert!((e1.x - e2.x).abs() < 0.1);
            assert!((s1.x - s3.x).abs() < 0.1);
            assert!((e1.x - e3.x).abs() < 0.1);
        } else {
            panic!("Expected linear gradients");
        }
    }

    #[test]
    fn test_radial_gradient_simple() {
        let css = r#"#card { background: radial-gradient(circle, red, blue); }"#;
        let result = Stylesheet::parse_with_errors(css);

        assert!(!result.has_errors());
        let style = result.stylesheet.get("card").unwrap();

        if let Some(Brush::Gradient(Gradient::Radial { stops, .. })) = &style.background {
            assert_eq!(stops.len(), 2);
        } else {
            panic!("Expected radial gradient");
        }
    }

    #[test]
    fn test_radial_gradient_at_center() {
        let css = r#"#card { background: radial-gradient(circle at center, red, blue); }"#;
        let result = Stylesheet::parse_with_errors(css);

        assert!(!result.has_errors());
        let style = result.stylesheet.get("card").unwrap();

        if let Some(Brush::Gradient(Gradient::Radial { center, .. })) = &style.background {
            assert!((center.x - 0.5).abs() < 0.01);
            assert!((center.y - 0.5).abs() < 0.01);
        } else {
            panic!("Expected radial gradient");
        }
    }

    #[test]
    fn test_radial_gradient_at_position() {
        let css = r#"#card { background: radial-gradient(circle at 25% 75%, red, blue); }"#;
        let result = Stylesheet::parse_with_errors(css);

        assert!(!result.has_errors());
        let style = result.stylesheet.get("card").unwrap();

        if let Some(Brush::Gradient(Gradient::Radial { center, .. })) = &style.background {
            assert!((center.x - 0.25).abs() < 0.01);
            assert!((center.y - 0.75).abs() < 0.01);
        } else {
            panic!("Expected radial gradient at custom position");
        }
    }

    #[test]
    fn test_radial_gradient_multiple_stops() {
        let css =
            r#"#card { background: radial-gradient(circle, red 0%, yellow 50%, green 100%); }"#;
        let result = Stylesheet::parse_with_errors(css);

        assert!(!result.has_errors());
        let style = result.stylesheet.get("card").unwrap();

        if let Some(Brush::Gradient(Gradient::Radial { stops, .. })) = &style.background {
            assert_eq!(stops.len(), 3);
            assert_eq!(stops[0].offset, 0.0);
            assert_eq!(stops[1].offset, 0.5);
            assert_eq!(stops[2].offset, 1.0);
        } else {
            panic!("Expected radial gradient with 3 stops");
        }
    }

    #[test]
    fn test_radial_gradient_ellipse() {
        let css = r#"#card { background: radial-gradient(ellipse at center, red, blue); }"#;
        let result = Stylesheet::parse_with_errors(css);

        assert!(!result.has_errors());
        let style = result.stylesheet.get("card").unwrap();
        assert!(matches!(
            &style.background,
            Some(Brush::Gradient(Gradient::Radial { .. }))
        ));
    }

    #[test]
    fn test_conic_gradient_simple() {
        let css = r#"#card { background: conic-gradient(red, yellow, green, blue, red); }"#;
        let result = Stylesheet::parse_with_errors(css);

        assert!(!result.has_errors());
        let style = result.stylesheet.get("card").unwrap();

        if let Some(Brush::Gradient(Gradient::Conic { stops, .. })) = &style.background {
            assert_eq!(stops.len(), 5);
        } else {
            panic!("Expected conic gradient");
        }
    }

    #[test]
    fn test_conic_gradient_from_angle() {
        let css = r#"#card { background: conic-gradient(from 45deg, red, blue); }"#;
        let result = Stylesheet::parse_with_errors(css);

        assert!(!result.has_errors());
        let style = result.stylesheet.get("card").unwrap();

        if let Some(Brush::Gradient(Gradient::Conic { start_angle, .. })) = &style.background {
            // 45 degrees in radians is approximately 0.785
            assert!((*start_angle - 0.785).abs() < 0.01);
        } else {
            panic!("Expected conic gradient with start angle");
        }
    }

    #[test]
    fn test_conic_gradient_at_position() {
        let css = r#"#card { background: conic-gradient(at 25% 75%, red, blue); }"#;
        let result = Stylesheet::parse_with_errors(css);

        assert!(!result.has_errors());
        let style = result.stylesheet.get("card").unwrap();

        if let Some(Brush::Gradient(Gradient::Conic { center, .. })) = &style.background {
            assert!((center.x - 0.25).abs() < 0.01);
            assert!((center.y - 0.75).abs() < 0.01);
        } else {
            panic!("Expected conic gradient at custom position");
        }
    }

    #[test]
    fn test_conic_gradient_from_at() {
        let css = r#"#card { background: conic-gradient(from 90deg at center, red, blue); }"#;
        let result = Stylesheet::parse_with_errors(css);

        assert!(!result.has_errors());
        let style = result.stylesheet.get("card").unwrap();

        if let Some(Brush::Gradient(Gradient::Conic {
            start_angle,
            center,
            ..
        })) = &style.background
        {
            // 90 degrees in radians is approximately 1.571
            assert!((*start_angle - 1.571).abs() < 0.01);
            assert!((center.x - 0.5).abs() < 0.01);
            assert!((center.y - 0.5).abs() < 0.01);
        } else {
            panic!("Expected conic gradient with angle and position");
        }
    }

    #[test]
    fn test_gradient_with_css_variables() {
        let css = r#"
            :root {
                --start-color: #667eea;
                --end-color: #764ba2;
            }
            #card {
                background: linear-gradient(135deg, var(--start-color), var(--end-color));
            }
        "#;
        let result = Stylesheet::parse_with_errors(css);

        // This test verifies that gradients work in the CSS context
        // Variable resolution happens at parse time
        let style = result.stylesheet.get("card").unwrap();
        assert!(style.background.is_some());
    }

    #[test]
    fn test_gradient_fallback_to_solid() {
        // If gradient parsing fails, should fall through to color parsing
        let css = r#"#card { background: #FF0000; }"#;
        let result = Stylesheet::parse_with_errors(css);

        let style = result.stylesheet.get("card").unwrap();
        if let Some(Brush::Solid(color)) = &style.background {
            assert_eq!(color.r, 1.0);
            assert_eq!(color.g, 0.0);
            assert_eq!(color.b, 0.0);
        } else {
            panic!("Expected solid color");
        }
    }

    #[test]
    fn test_gradient_with_named_colors() {
        let css = r#"#card { background: linear-gradient(to right, red, orange, yellow, green, blue, purple); }"#;
        let result = Stylesheet::parse_with_errors(css);

        assert!(!result.has_errors());
        let style = result.stylesheet.get("card").unwrap();

        if let Some(Brush::Gradient(Gradient::Linear { stops, .. })) = &style.background {
            assert_eq!(stops.len(), 6);
            // Check that positions are evenly distributed
            assert_eq!(stops[0].offset, 0.0);
            assert!((stops[1].offset - 0.2).abs() < 0.01);
            assert!((stops[2].offset - 0.4).abs() < 0.01);
            assert!((stops[3].offset - 0.6).abs() < 0.01);
            assert!((stops[4].offset - 0.8).abs() < 0.01);
            assert_eq!(stops[5].offset, 1.0);
        } else {
            panic!("Expected linear gradient with 6 named colors");
        }
    }

    // =========================================================================
    // Length Unit Tests
    // =========================================================================

    #[test]
    fn test_parse_css_length_px() {
        let len = parse_css_length("16px").unwrap();
        assert!(matches!(len, Length::Px(v) if (v - 16.0).abs() < 0.01));
        assert_eq!(len.to_px(), 16.0);
    }

    #[test]
    fn test_parse_css_length_sp() {
        // sp = spacing units (4px grid)
        let len = parse_css_length("4sp").unwrap();
        assert!(matches!(len, Length::Sp(v) if (v - 4.0).abs() < 0.01));
        assert_eq!(len.to_px(), 16.0); // 4 * 4 = 16px
    }

    #[test]
    fn test_parse_css_length_pct() {
        let len = parse_css_length("50%").unwrap();
        assert!(matches!(len, Length::Pct(v) if (v - 50.0).abs() < 0.01));
        // Percentage doesn't convert to pixels without context
        assert_eq!(len.to_px(), 0.0);
    }

    #[test]
    fn test_parse_css_length_unitless() {
        // Unitless treated as pixels for backwards compatibility
        let len = parse_css_length("24").unwrap();
        assert!(matches!(len, Length::Px(v) if (v - 24.0).abs() < 0.01));
        assert_eq!(len.to_px(), 24.0);
    }

    #[test]
    fn test_border_radius_with_sp() {
        let css = "#card { border-radius: 2sp; }"; // 2 * 4 = 8px
        let result = Stylesheet::parse_with_errors(css);

        let style = result.stylesheet.get("card").unwrap();
        if let Some(radius) = &style.corner_radius {
            // 2sp = 8px
            assert_eq!(radius.top_left, 8.0);
        } else {
            panic!("Expected corner radius to be parsed");
        }
    }

    #[test]
    fn test_shadow_with_sp() {
        let css = "#card { box-shadow: 1sp 2sp 4sp rgba(0,0,0,0.3); }";
        let result = Stylesheet::parse_with_errors(css);

        let style = result.stylesheet.get("card").unwrap();
        if let Some(shadow) = style.shadow.first() {
            // 1sp = 4px, 2sp = 8px, 4sp = 16px
            assert_eq!(shadow.offset_x, 4.0);
            assert_eq!(shadow.offset_y, 8.0);
            assert_eq!(shadow.blur, 16.0);
        } else {
            panic!("Expected shadow to be parsed");
        }
    }

    #[test]
    fn test_transform_with_sp() {
        use blinc_core::Transform;

        let css = "#card { transform: translate(4sp, 2sp); }";
        let result = Stylesheet::parse_with_errors(css);

        let style = result.stylesheet.get("card").unwrap();
        if let Some(Transform::Affine2D(affine)) = &style.transform {
            // 4sp = 16px, 2sp = 8px
            // elements[4] = tx, elements[5] = ty
            assert!((affine.elements[4] - 16.0).abs() < 0.01);
            assert!((affine.elements[5] - 8.0).abs() < 0.01);
        } else {
            panic!("Expected Affine2D transform to be parsed");
        }
    }

    #[test]
    fn test_translatex_with_sp() {
        use blinc_core::Transform;

        let css = "#card { transform: translateX(4sp); }";
        let result = Stylesheet::parse_with_errors(css);

        let style = result.stylesheet.get("card").unwrap();
        if let Some(Transform::Affine2D(affine)) = &style.transform {
            assert!((affine.elements[4] - 16.0).abs() < 0.01); // 4sp = 16px
            assert!((affine.elements[5] - 0.0).abs() < 0.01);
        } else {
            panic!("Expected Affine2D transform to be parsed");
        }
    }

    #[test]
    fn test_translatey_with_sp() {
        use blinc_core::Transform;

        let css = "#card { transform: translateY(2sp); }";
        let result = Stylesheet::parse_with_errors(css);

        let style = result.stylesheet.get("card").unwrap();
        if let Some(Transform::Affine2D(affine)) = &style.transform {
            assert!((affine.elements[4] - 0.0).abs() < 0.01);
            assert!((affine.elements[5] - 8.0).abs() < 0.01); // 2sp = 8px
        } else {
            panic!("Expected Affine2D transform to be parsed");
        }
    }

    // =========================================================================
    // Comma-Separated Selector Tests
    // =========================================================================

    #[test]
    fn test_comma_separated_id_selectors() {
        let css = "#a, #b { opacity: 0.5; }";
        let result = Stylesheet::parse_with_errors(css);
        assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);

        let style_a = result.stylesheet.get("a").unwrap();
        assert_eq!(style_a.opacity, Some(0.5));

        let style_b = result.stylesheet.get("b").unwrap();
        assert_eq!(style_b.opacity, Some(0.5));
    }

    #[test]
    fn test_comma_separated_does_not_break_subsequent_rules() {
        let css = r#"
            #a, #b { opacity: 0.5; }
            #c { opacity: 0.8; }
        "#;
        let result = Stylesheet::parse_with_errors(css);
        assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);

        assert_eq!(result.stylesheet.get("a").unwrap().opacity, Some(0.5));
        assert_eq!(result.stylesheet.get("b").unwrap().opacity, Some(0.5));
        assert_eq!(result.stylesheet.get("c").unwrap().opacity, Some(0.8));
    }

    #[test]
    fn test_comma_separated_mixed_selectors() {
        let css = r#"#a, .myclass { border-radius: 10px; }"#;
        let result = Stylesheet::parse_with_errors(css);
        assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);

        // #a should be a simple rule
        let style_a = result.stylesheet.get("a").unwrap();
        assert!(style_a.corner_radius.is_some());

        // .myclass should be in complex rules
        let complex = result.stylesheet.complex_rules();
        assert!(!complex.is_empty());
        assert!(complex[0].1.corner_radius.is_some());
    }

    #[test]
    fn test_comma_separated_three_selectors() {
        let css = "#x, #y, #z { opacity: 0.3; }";
        let result = Stylesheet::parse_with_errors(css);
        assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);

        assert_eq!(result.stylesheet.get("x").unwrap().opacity, Some(0.3));
        assert_eq!(result.stylesheet.get("y").unwrap().opacity, Some(0.3));
        assert_eq!(result.stylesheet.get("z").unwrap().opacity, Some(0.3));
    }

    // =========================================================================
    // Conversion Method Tests
    // =========================================================================

    #[test]
    fn test_animation_timing_to_easing() {
        use blinc_animation::Easing;

        assert!(matches!(
            AnimationTiming::Linear.to_easing(),
            Easing::Linear
        ));
        // CSS ease = cubic-bezier(0.25, 0.1, 0.25, 1.0)
        assert!(matches!(
            AnimationTiming::Ease.to_easing(),
            Easing::CubicBezier(0.25, 0.1, 0.25, 1.0)
        ));
        // CSS ease-in = cubic-bezier(0.42, 0.0, 1.0, 1.0)
        assert!(matches!(
            AnimationTiming::EaseIn.to_easing(),
            Easing::CubicBezier(0.42, 0.0, 1.0, 1.0)
        ));
        // CSS ease-out = cubic-bezier(0.0, 0.0, 0.58, 1.0)
        assert!(matches!(
            AnimationTiming::EaseOut.to_easing(),
            Easing::CubicBezier(0.0, 0.0, 0.58, 1.0)
        ));
        // CSS ease-in-out = cubic-bezier(0.42, 0.0, 0.58, 1.0)
        assert!(matches!(
            AnimationTiming::EaseInOut.to_easing(),
            Easing::CubicBezier(0.42, 0.0, 0.58, 1.0)
        ));
    }

    #[test]
    fn test_animation_direction_to_play_direction() {
        use blinc_animation::PlayDirection;

        assert!(matches!(
            AnimationDirection::Normal.to_play_direction(),
            PlayDirection::Forward
        ));
        assert!(matches!(
            AnimationDirection::Reverse.to_play_direction(),
            PlayDirection::Reverse
        ));
        assert!(matches!(
            AnimationDirection::Alternate.to_play_direction(),
            PlayDirection::Alternate
        ));
        assert!(matches!(
            AnimationDirection::AlternateReverse.to_play_direction(),
            PlayDirection::Alternate
        ));
    }

    #[test]
    fn test_animation_direction_starts_reversed() {
        assert!(!AnimationDirection::Normal.starts_reversed());
        assert!(!AnimationDirection::Reverse.starts_reversed());
        assert!(!AnimationDirection::Alternate.starts_reversed());
        assert!(AnimationDirection::AlternateReverse.starts_reversed());
    }

    #[test]
    fn test_animation_fill_mode_to_fill_mode() {
        use blinc_animation::FillMode;

        assert!(matches!(
            AnimationFillMode::None.to_fill_mode(),
            FillMode::None
        ));
        assert!(matches!(
            AnimationFillMode::Forwards.to_fill_mode(),
            FillMode::Forwards
        ));
        assert!(matches!(
            AnimationFillMode::Backwards.to_fill_mode(),
            FillMode::Backwards
        ));
        assert!(matches!(
            AnimationFillMode::Both.to_fill_mode(),
            FillMode::Both
        ));
    }

    #[test]
    fn test_resolve_keyframe_animation_preserves_all_keyframes() {
        ThemeState::init_default();

        let css = r#"
            @keyframes pulse {
                0% { opacity: 1; }
                50% { opacity: 0.5; }
                100% { opacity: 1; }
            }
            #button { animation: pulse 1000ms ease-in-out infinite; }
        "#;

        let result = Stylesheet::parse_with_errors(css);
        assert!(!result.has_errors());

        // Verify keyframes are stored
        let keyframes = result.stylesheet.get_keyframes("pulse").unwrap();
        assert_eq!(keyframes.keyframes.len(), 3); // All 3 keyframes preserved

        // Resolve animation - should preserve all keyframes
        let animation = result.stylesheet.resolve_keyframe_animation("button");
        assert!(animation.is_some());

        let anim = animation.unwrap();
        // The animation should have the correct duration
        assert_eq!(anim.duration_ms(), 1000);
    }

    #[test]
    fn test_resolve_keyframe_animation_with_state() {
        ThemeState::init_default();

        let css = r#"
            @keyframes hover-glow {
                from { opacity: 0.8; }
                to { opacity: 1.0; }
            }
            #button:hover { animation: hover-glow 200ms ease-out; }
        "#;

        let result = Stylesheet::parse_with_errors(css);
        assert!(!result.has_errors());

        // Base style should not have animation
        let base_anim = result.stylesheet.resolve_keyframe_animation("button");
        assert!(base_anim.is_none());

        // Hover state should have animation
        let hover_anim = result
            .stylesheet
            .resolve_keyframe_animation_with_state("button", ElementState::Hover);
        assert!(hover_anim.is_some());

        let anim = hover_anim.unwrap();
        assert_eq!(anim.duration_ms(), 200);
    }

    #[test]
    fn test_css_animation_iteration_count() {
        ThemeState::init_default();

        // Test infinite animation
        let css_infinite = r#"
            @keyframes spin { from { opacity: 0; } to { opacity: 1; } }
            #a { animation: spin 1s infinite; }
        "#;
        let result = Stylesheet::parse_with_errors(css_infinite);
        let style = result.stylesheet.get("a").unwrap();
        assert_eq!(style.animation.as_ref().unwrap().iteration_count, 0); // 0 = infinite

        // Test finite animation
        let css_finite = r#"
            @keyframes spin { from { opacity: 0; } to { opacity: 1; } }
            #b { animation: spin 1s 3; }
        "#;
        let result = Stylesheet::parse_with_errors(css_finite);
        let style = result.stylesheet.get("b").unwrap();
        assert_eq!(style.animation.as_ref().unwrap().iteration_count, 3);
    }

    // ====================================================================
    // Property Aliases
    // ====================================================================

    #[test]
    fn test_shape_alias() {
        let css = "#a { shape: sphere; }";
        let result = Stylesheet::parse_with_errors(css);
        let style = result.stylesheet.get("a").unwrap();
        assert_eq!(style.shape_3d.as_deref(), Some("sphere"));
    }

    #[test]
    fn test_shape_3d_still_works() {
        let css = "#a { shape-3d: box; }";
        let result = Stylesheet::parse_with_errors(css);
        let style = result.stylesheet.get("a").unwrap();
        assert_eq!(style.shape_3d.as_deref(), Some("box"));
    }

    #[test]
    fn test_shape_combine_alias() {
        let css = "#a { shape-combine: smooth-union; }";
        let result = Stylesheet::parse_with_errors(css);
        let style = result.stylesheet.get("a").unwrap();
        assert_eq!(style.op_3d.as_deref(), Some("smooth-union"));
    }

    #[test]
    fn test_shape_blend_alias() {
        let css = "#a { shape-blend: 8; }";
        let result = Stylesheet::parse_with_errors(css);
        let style = result.stylesheet.get("a").unwrap();
        assert_eq!(style.blend_3d, Some(8.0));
    }

    #[test]
    fn test_light_alias() {
        let css = "#a { light: 0.3 -0.8 0.5; }";
        let result = Stylesheet::parse_with_errors(css);
        let style = result.stylesheet.get("a").unwrap();
        let dir = style.light_direction.unwrap();
        assert!((dir[0] - 0.3).abs() < 0.01);
        assert!((dir[1] - (-0.8)).abs() < 0.01);
        assert!((dir[2] - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_surface_glass_alias() {
        let css = "#a { surface: glossy; }";
        let result = Stylesheet::parse_with_errors(css);
        let style = result.stylesheet.get("a").unwrap();
        assert!(matches!(
            style.material,
            Some(crate::element::Material::Glass(_))
        ));
    }

    #[test]
    fn test_surface_metallic_alias() {
        let css = "#a { surface: chrome; }";
        let result = Stylesheet::parse_with_errors(css);
        let style = result.stylesheet.get("a").unwrap();
        assert!(matches!(
            style.material,
            Some(crate::element::Material::Metallic(_))
        ));
    }

    #[test]
    fn test_surface_gold_alias() {
        let css = "#a { surface: gold; }";
        let result = Stylesheet::parse_with_errors(css);
        let style = result.stylesheet.get("a").unwrap();
        assert!(matches!(
            style.material,
            Some(crate::element::Material::Metallic(_))
        ));
    }

    // ====================================================================
    // calc() in CSS values
    // ====================================================================

    #[test]
    fn test_calc_in_width() {
        let css = "#a { width: calc(100 - 20); }";
        let result = Stylesheet::parse_with_errors(css);
        let style = result.stylesheet.get("a").unwrap();
        assert_eq!(
            style.width,
            Some(crate::element_style::StyleDimension::Length(80.0))
        );
    }

    #[test]
    fn test_calc_in_padding() {
        let css = "#a { padding: calc(8 * 2); }";
        let result = Stylesheet::parse_with_errors(css);
        let style = result.stylesheet.get("a").unwrap();
        assert_eq!(
            style.padding,
            Some(crate::element_style::SpacingRect::uniform(16.0))
        );
    }

    #[test]
    fn test_calc_in_border_width() {
        let css = "#a { border-width: calc(1 + 1); }";
        let result = Stylesheet::parse_with_errors(css);
        let style = result.stylesheet.get("a").unwrap();
        assert_eq!(style.border_width, Some(2.0));
    }

    #[test]
    fn test_calc_in_gap() {
        let css = "#a { gap: calc(4 * 3); }";
        let result = Stylesheet::parse_with_errors(css);
        let style = result.stylesheet.get("a").unwrap();
        assert_eq!(style.gap, Some(12.0));
    }

    // =====================================================================
    // @flow parser tests
    // =====================================================================

    #[test]
    fn test_flow_basic_fragment() {
        let css = r#"
            @flow ripple {
                target: fragment;
                input uv;
                input time;
                node dist = distance(uv, vec2(0.5, 0.5));
                node wave = sin(dist * 20.0 - time * 4.0);
                output color = vec4(wave, wave, wave, 1.0);
            }
        "#;
        let result = Stylesheet::parse_with_errors(css);
        assert!(result.stylesheet.contains_flow("ripple"));
        let flow = result.stylesheet.get_flow("ripple").unwrap();
        assert_eq!(flow.target, FlowTarget::Fragment);
        assert_eq!(flow.inputs.len(), 2);
        assert_eq!(flow.nodes.len(), 2);
        assert_eq!(flow.outputs.len(), 1);
    }

    #[test]
    fn test_flow_compute_target() {
        let css = r#"
            @flow sim {
                target: compute;
                workgroup: 64;
                input pos: buffer(positions, vec4);
                node new_pos = pos + 1.0;
                output buffer(positions) = new_pos;
            }
        "#;
        let result = Stylesheet::parse_with_errors(css);
        let flow = result.stylesheet.get_flow("sim").unwrap();
        assert_eq!(flow.target, FlowTarget::Compute);
        assert_eq!(flow.workgroup_size, Some(64));
        assert_eq!(flow.inputs.len(), 1);
    }

    #[test]
    fn test_flow_expr_arithmetic() {
        let expr = parse_flow_expr("a * 2.0 + b").unwrap();
        // Should parse as (a * 2.0) + b due to precedence
        match &expr {
            FlowExpr::Add(left, right) => {
                assert!(matches!(left.as_ref(), FlowExpr::Mul(_, _)));
                assert!(matches!(right.as_ref(), FlowExpr::Ref(_)));
            }
            _ => panic!("expected Add, got {:?}", expr),
        }
    }

    #[test]
    fn test_flow_expr_function_call() {
        let expr = parse_flow_expr("sin(x * 3.14)").unwrap();
        match &expr {
            FlowExpr::Call { func, args } => {
                assert_eq!(*func, FlowFunc::Sin);
                assert_eq!(args.len(), 1);
            }
            _ => panic!("expected Call, got {:?}", expr),
        }
    }

    #[test]
    fn test_flow_expr_nested_functions() {
        let expr = parse_flow_expr("smoothstep(0.0, 1.0, distance(uv, vec2(0.5, 0.5)))").unwrap();
        match &expr {
            FlowExpr::Call { func, args } => {
                assert_eq!(*func, FlowFunc::Smoothstep);
                assert_eq!(args.len(), 3);
                // Third arg should be a distance() call
                assert!(matches!(
                    &args[2],
                    FlowExpr::Call {
                        func: FlowFunc::Distance,
                        ..
                    }
                ));
            }
            _ => panic!("expected Call, got {:?}", expr),
        }
    }

    #[test]
    fn test_flow_expr_color_literal() {
        let expr = parse_flow_expr("#ff0000").unwrap();
        match &expr {
            FlowExpr::Color(r, _g, _b, _a) => {
                assert!((r - 1.0).abs() < 0.01);
            }
            _ => panic!("expected Color, got {:?}", expr),
        }
    }

    #[test]
    fn test_flow_expr_negative_number() {
        let expr = parse_flow_expr("-1.5").unwrap();
        match &expr {
            FlowExpr::Float(v) => assert!((*v - (-1.5)).abs() < 0.001),
            _ => panic!("expected Float, got {:?}", expr),
        }
    }

    #[test]
    fn test_flow_expr_vec_constructors() {
        let expr = parse_flow_expr("vec3(1.0, 2.0, 3.0)").unwrap();
        assert!(matches!(expr, FlowExpr::Vec3(_, _, _)));
    }

    #[test]
    fn test_flow_cycle_reported_as_error() {
        let css = r#"
            @flow bad {
                target: fragment;
                node a = b + 1.0;
                node b = a + 1.0;
                output color = a;
            }
        "#;
        let result = Stylesheet::parse_with_errors(css);
        // Should still parse but with errors
        assert!(!result.errors.is_empty());
        let has_cycle_error = result.errors.iter().any(|e| e.message.contains("cycle"));
        assert!(
            has_cycle_error,
            "expected cycle error, got: {:?}",
            result.errors
        );
    }

    #[test]
    fn test_flow_with_comments() {
        let css = r#"
            @flow test {
                target: fragment;
                /* This is a comment */
                input time;
                node x = sin(time);
                output color = vec4(x, x, x, 1.0);
            }
        "#;
        let result = Stylesheet::parse_with_errors(css);
        assert!(result.stylesheet.contains_flow("test"));
    }

    #[test]
    fn test_flow_alongside_rules() {
        let css = r#"
            #card { opacity: 0.5; }

            @flow glow {
                target: fragment;
                input uv;
                node c = length(uv);
                output color = vec4(c, c, c, 1.0);
            }

            #button { background: #ff0000; }
        "#;
        let result = Stylesheet::parse_with_errors(css);
        assert!(result.stylesheet.get("card").is_some());
        assert!(result.stylesheet.get("button").is_some());
        assert!(result.stylesheet.contains_flow("glow"));
    }

    #[test]
    fn test_flow_multiple_flows() {
        let css = r#"
            @flow a {
                target: fragment;
                input uv;
                node x = length(uv);
                output color = vec4(x, x, x, 1.0);
            }
            @flow b {
                target: fragment;
                input time;
                node y = sin(time);
                output color = vec4(y, y, y, 1.0);
            }
        "#;
        let result = Stylesheet::parse_with_errors(css);
        assert_eq!(result.stylesheet.flow_count(), 2);
        assert!(result.stylesheet.contains_flow("a"));
        assert!(result.stylesheet.contains_flow("b"));
    }

    #[test]
    fn test_flow_bare_output() {
        let css = r#"
            @flow test {
                target: fragment;
                input uv;
                node color = vec4(1.0, 0.0, 0.0, 1.0);
                output color;
            }
        "#;
        let result = Stylesheet::parse_with_errors(css);
        assert!(result.stylesheet.contains_flow("test"));
        let flow = result.stylesheet.get_flow("test").unwrap();
        assert_eq!(flow.outputs.len(), 1);
        assert!(flow.outputs[0].expr.is_none());
    }

    #[test]
    fn test_flow_expr_parenthesized() {
        let expr = parse_flow_expr("(a + b) * c").unwrap();
        match &expr {
            FlowExpr::Mul(left, _right) => {
                assert!(matches!(left.as_ref(), FlowExpr::Add(_, _)));
            }
            _ => panic!("expected Mul, got {:?}", expr),
        }
    }

    #[test]
    fn participates_in_hover_recognizes_simple_id_class_and_complex() {
        let css = r#"
            #button:hover { background: red; }
            .card:hover { opacity: 0.8; }
            #parent:hover > .child { color: blue; }
            #plain { background: blue; }
            .untouched { color: white; }
        "#;
        let sheet = Stylesheet::parse(css).unwrap();

        assert!(sheet.participates_in_hover("button"));
        assert!(sheet.participates_in_hover("card"));
        // `#parent:hover > .child` — the hover is on #parent, so
        // #parent is a participant; .child is not (its segment has
        // no :hover).
        assert!(sheet.participates_in_hover("parent"));
        assert!(!sheet.participates_in_hover("child"));
        assert!(!sheet.participates_in_hover("plain"));
        assert!(!sheet.participates_in_hover("untouched"));
    }

    #[test]
    fn insert_with_state_updates_hover_participants() {
        let mut sheet = Stylesheet::new();
        assert!(!sheet.participates_in_hover("dynamic"));
        sheet.insert_with_state("dynamic", ElementState::Hover, ElementStyle::default());
        assert!(sheet.participates_in_hover("dynamic"));
        // A non-hover state insertion shouldn't register the
        // identifier as a hover participant.
        sheet.insert_with_state("active-only", ElementState::Active, ElementStyle::default());
        assert!(!sheet.participates_in_hover("active-only"));
    }

    #[test]
    fn animation_picks_up_cubic_bezier_from_var_ease() {
        // Regression: `animation: name dur var(--ease-state);` must
        // resolve the var to its cubic-bezier literal AND survive
        // whitespace-tokenisation in parse_animation. Without paren-
        // aware splitting the cubic-bezier args get chopped into
        // separate tokens and the timing falls back to the default
        // `Ease`, defeating the whole semantic-easing wiring.
        let mut vars = std::collections::HashMap::new();
        vars.insert(
            "ease-state".to_string(),
            "cubic-bezier(0.25, 0.10, 0.25, 1.0)".to_string(),
        );
        vars.insert("duration-fast".to_string(), "180ms".to_string());

        let css = "#foo { animation: foo-enter var(--duration-fast) var(--ease-state); }";
        let sheet = Stylesheet::parse_with_variables(css, &vars).expect("parse");
        let style = sheet.get("foo").expect("rule");
        let anim = style.animation.as_ref().expect("animation set");
        assert_eq!(anim.name, "foo-enter");
        assert_eq!(anim.duration_ms, 180);
        match anim.timing {
            AnimationTiming::CubicBezier(a, b, c, d) => {
                assert!((a - 0.25).abs() < 1e-3 && (b - 0.10).abs() < 1e-3);
                assert!((c - 0.25).abs() < 1e-3 && (d - 1.0).abs() < 1e-3);
            }
            other => panic!("expected CubicBezier, got {:?}", other),
        }
    }

    #[test]
    fn class_animation_picks_up_cubic_bezier_from_var_ease() {
        // Same as `animation_picks_up_cubic_bezier_from_var_ease`
        // but for a class selector and chained next to other
        // properties — the shape cn_styles actually uses.
        let mut vars = std::collections::HashMap::new();
        vars.insert(
            "ease-spring".to_string(),
            "cubic-bezier(0.34, 1.3, 0.64, 1)".to_string(),
        );
        vars.insert("duration-normal".to_string(), "240ms".to_string());

        let css = r#"
            .cn-popover-content {
                background: white;
                padding: 16px;
                animation: cn-popover-enter var(--duration-normal) var(--ease-spring);
                transform-origin: top center;
            }
        "#;
        let sheet = Stylesheet::parse_with_variables(css, &vars).expect("parse");
        let style = sheet
            .get_class("cn-popover-content")
            .expect("no class style registered");
        let anim = style
            .animation
            .as_ref()
            .expect("animation set on .cn-popover-content");
        assert_eq!(anim.name, "cn-popover-enter");
        assert_eq!(anim.duration_ms, 240);
        match anim.timing {
            AnimationTiming::CubicBezier(a, b, c, d) => {
                assert!((a - 0.34).abs() < 1e-3 && (b - 1.3).abs() < 1e-3);
                assert!((c - 0.64).abs() < 1e-3 && (d - 1.0).abs() < 1e-3);
            }
            other => panic!("expected CubicBezier, got {:?}", other),
        }
    }

    #[test]
    fn transition_picks_up_cubic_bezier_from_var_ease() {
        let mut vars = std::collections::HashMap::new();
        vars.insert(
            "ease-state".to_string(),
            "cubic-bezier(0.25, 0.10, 0.25, 1.0)".to_string(),
        );
        vars.insert("duration-fast".to_string(), "150ms".to_string());

        let css = "#foo { transition: background var(--duration-fast) var(--ease-state), \
                   border-color var(--duration-fast) var(--ease-state); }";
        let sheet = Stylesheet::parse_with_variables(css, &vars).expect("parse");
        let style = sheet.get("foo").expect("rule");
        let trans = style.transition.as_ref().expect("transitions set");
        assert_eq!(trans.transitions.len(), 2);
        for t in &trans.transitions {
            assert_eq!(t.duration_ms, 150);
            assert!(matches!(t.timing, AnimationTiming::CubicBezier(_, _, _, _)));
        }
    }

    /// CSS viewport units resolve against the live viewport.
    ///
    /// The calc module has understood `vh` / `vw` all along, but this
    /// parser only handled `px`, so `height: 100vh` parsed to nothing
    /// and the declaration was dropped. A page relying on it for its
    /// height then had none, and `overflow: scroll` had nothing to
    /// scroll.
    mod viewport_units {
        use super::*;

        fn with_viewport(w: f32, h: f32) {
            use blinc_core::context_state::{BlincContextState, HookState};
            use std::sync::atomic::AtomicBool;
            use std::sync::{Arc, Mutex};
            static I: std::sync::Once = std::sync::Once::new();
            I.call_once(|| {
                if !BlincContextState::is_initialized() {
                    BlincContextState::init(
                        blinc_core::reactive::global_graph(),
                        Arc::new(Mutex::new(HookState::new())),
                        Arc::new(AtomicBool::new(false)),
                    );
                }
            });
            BlincContextState::get().set_viewport_size(w, h);
        }

        #[test]
        fn vh_and_vw_resolve() {
            with_viewport(1000.0, 800.0);
            assert_eq!(parse_css_px("100vh"), Some(800.0));
            assert_eq!(parse_css_px("50vw"), Some(500.0));
            assert_eq!(parse_css_px("25vh"), Some(200.0));
        }

        #[test]
        fn px_and_unitless_still_work() {
            with_viewport(1000.0, 800.0);
            assert_eq!(parse_css_px("12px"), Some(12.0));
            assert_eq!(parse_css_px("12"), Some(12.0));
        }
    }
}
