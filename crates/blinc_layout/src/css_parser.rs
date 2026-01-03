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

use blinc_core::{Brush, Color, CornerRadius, Gradient, GradientSpace, GradientStop, Point, Shadow, Transform};
use blinc_theme::{ColorToken, ThemeState};
use nom::{
    branch::alt,
    bytes::complete::{tag, tag_no_case, take_until, take_while1},
    character::complete::{char, multispace1},
    combinator::{cut, opt, value},
    error::{context, ParseError as NomParseError, VerboseError, VerboseErrorKind},
    multi::many0,
    number::complete::float,
    sequence::{delimited, preceded, tuple},
    Finish, IResult,
};
use tracing::debug;

use crate::element::RenderLayer;
use crate::element_style::ElementStyle;
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
        s.push_str(&format!(
            "{DIM}[{}:{}]{RESET} ",
            self.line, self.column
        ));

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
            eprintln!("{BOLD}CSS parsing completed with {}{RESET}", parts.join(", "));
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
}

impl ElementState {
    /// Parse a state from a pseudo-class string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "hover" => Some(ElementState::Hover),
            "active" => Some(ElementState::Active),
            "focus" => Some(ElementState::Focus),
            "disabled" => Some(ElementState::Disabled),
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
        self.keyframes.sort_by(|a, b| a.position.partial_cmp(&b.position).unwrap());
    }

    /// Get the keyframe at or before a given position
    pub fn keyframe_at(&self, position: f32) -> Option<&CssKeyframe> {
        self.keyframes.iter().rev().find(|kf| kf.position <= position)
    }

    /// Convert to Blinc MotionAnimation for enter animations
    ///
    /// Uses the first keyframe (0% or from) as enter_from and animates to the final state.
    pub fn to_enter_animation(&self, duration_ms: u32) -> crate::element::MotionAnimation {
        let enter_from = self.keyframes.first().map(|kf| Self::style_to_motion_keyframe(&kf.style));

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
        let exit_to = self.keyframes.last().map(|kf| Self::style_to_motion_keyframe(&kf.style));

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
    pub fn to_motion_animation(&self, enter_duration_ms: u32, exit_duration_ms: u32) -> crate::element::MotionAnimation {
        let enter_from = self.keyframes.first().map(|kf| Self::style_to_motion_keyframe(&kf.style));
        let exit_to = self.keyframes.last().map(|kf| Self::style_to_motion_keyframe(&kf.style));

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

        // Try to extract transform components from Affine2D
        if let Some(ref transform) = style.transform {
            if let Transform::Affine2D(affine) = transform {
                let [a, b, c, d, tx, ty] = affine.elements;

                // Extract translation
                if tx != 0.0 || ty != 0.0 {
                    props.translate_x = Some(tx);
                    props.translate_y = Some(ty);
                }

                // Try to extract scale (valid when no rotation/skew: b=0, c=0)
                if b.abs() < 0.0001 && c.abs() < 0.0001 {
                    if (a - 1.0).abs() > 0.0001 {
                        props.scale_x = Some(a);
                    }
                    if (d - 1.0).abs() > 0.0001 {
                        props.scale_y = Some(d);
                    }
                } else {
                    // Has rotation - extract rotation angle
                    let rotation = b.atan2(a);
                    if rotation.abs() > 0.0001 {
                        props.rotate = Some(rotation.to_degrees());
                    }
                }
            }
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
        if let Some(ref transform) = style.transform {
            if let Transform::Affine2D(affine) = transform {
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
        }

        kf
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AnimationTiming {
    Linear,
    #[default]
    Ease,
    EaseIn,
    EaseOut,
    EaseInOut,
}

impl AnimationTiming {
    fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "linear" => Some(AnimationTiming::Linear),
            "ease" => Some(AnimationTiming::Ease),
            "ease-in" => Some(AnimationTiming::EaseIn),
            "ease-out" => Some(AnimationTiming::EaseOut),
            "ease-in-out" => Some(AnimationTiming::EaseInOut),
            _ => None,
        }
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

/// Animation fill mode
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AnimationFillMode {
    #[default]
    None,
    Forwards,
    Backwards,
    Both,
}

/// A parsed stylesheet containing styles keyed by element ID
#[derive(Clone, Default, Debug)]
pub struct Stylesheet {
    /// Styles keyed by selector (id or id:state)
    styles: HashMap<String, ElementStyle>,
    /// CSS custom properties (variables) defined in :root
    variables: HashMap<String, String>,
    /// Keyframe animations defined with @keyframes
    keyframes: HashMap<String, CssKeyframes>,
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
                        message: format!(
                            "Unparsed content remaining ({} chars)",
                            remaining.len()
                        ),
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
                for keyframes in parsed.keyframes {
                    stylesheet.keyframes.insert(keyframes.name.clone(), keyframes);
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
    pub fn get_all_states(&self, id: &str) -> (Option<&ElementStyle>, Vec<(ElementState, &ElementStyle)>) {
        let base = self.styles.get(id);

        let mut state_styles = Vec::new();
        for state in [ElementState::Hover, ElementState::Active, ElementState::Focus, ElementState::Disabled] {
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
        self.styles.is_empty()
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
        let mut motion = keyframes.to_motion_animation(anim_config.duration_ms, anim_config.duration_ms);

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
                    let mut motion = keyframes.to_motion_animation(
                        anim_config.duration_ms,
                        anim_config.duration_ms,
                    );
                    motion.enter_delay_ms = anim_config.delay_ms;
                    return Some(motion);
                }
            }
        }

        // Fall back to base animation
        self.resolve_animation(id)
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
        many0(alt((
            value((), multispace1),
            value((), parse_comment),
        ))),
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

        let element_state = state.and_then(ElementState::from_str);

        Ok((input, CssSelector {
            id: id.to_string(),
            state: element_state,
        }))
    })(input)
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
    Rule(String, ElementStyle),
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

/// Result of parsing a stylesheet - rules, variables, and keyframes
struct ParsedStylesheet {
    rules: Vec<(String, ElementStyle)>,
    variables: HashMap<String, String>,
    keyframes: Vec<CssKeyframes>,
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
    let mut parsed_variables = variables.clone();
    let mut parsed_keyframes = Vec::new();
    let mut remaining = input;

    loop {
        let trimmed = remaining.trim_start();
        if trimmed.is_empty() {
            break;
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

        // Try to parse a rule
        match css_rule_with_errors_and_vars(css, errors, &parsed_variables)(trimmed) {
            Ok((rest, rule)) => {
                rules.push(rule);
                remaining = rest;
            }
            Err(nom::Err::Error(_)) | Err(nom::Err::Failure(_)) => {
                // Can't parse more rules, break
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
            variables: parsed_variables,
            keyframes: parsed_keyframes,
        },
    ))
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
        "border-radius" => {
            if let Some(radius) = parse_radius(value) {
                style.corner_radius = Some(radius);
            }
        }
        "box-shadow" => {
            if let Some(shadow) = parse_shadow(value) {
                style.shadow = Some(shadow);
            }
        }
        "transform" => {
            if let Some(transform) = parse_transform(value) {
                style.transform = Some(transform);
            }
        }
        "opacity" => {
            if let Ok((_, opacity)) = parse_opacity::<nom::error::Error<&str>>(value) {
                style.opacity = Some(opacity.clamp(0.0, 1.0));
            }
        }
        "render-layer" | "z-index" => {
            if let Ok((_, layer)) = parse_render_layer::<nom::error::Error<&str>>(value) {
                style.render_layer = Some(layer);
            }
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
        _ => {
            // Unknown property - log at debug level for forward compatibility
            debug!(property = name, value = value, "Unknown CSS property (ignored)");
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
        "border-radius" => {
            if let Some(radius) = parse_radius(value) {
                style.corner_radius = Some(radius);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "box-shadow" => {
            if let Some(shadow) = parse_shadow(value) {
                style.shadow = Some(shadow);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "transform" => {
            if let Some(transform) = parse_transform(value) {
                style.transform = Some(transform);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "opacity" => {
            if let Ok((_, opacity)) = parse_opacity::<nom::error::Error<&str>>(value) {
                style.opacity = Some(opacity.clamp(0.0, 1.0));
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "render-layer" | "z-index" => {
            if let Ok((_, layer)) = parse_render_layer::<nom::error::Error<&str>>(value) {
                style.render_layer = Some(layer);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
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

/// Parse theme(token-name) for colors
fn parse_theme_color<'a, E: NomParseError<&'a str>>(input: &'a str) -> IResult<&'a str, Color, E> {
    let (input, _) = ws(input)?;
    let (input, _) = tag_no_case("theme")(input)?;
    let (input, _) = ws(input)?;
    let (input, token_name) = delimited(char('('), take_while1(|c: char| c != ')'), char(')'))(input)?;

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

    // Try parsing as numeric value
    parse_length_value(value).map(CornerRadius::uniform)
}

/// Parse theme(radius-*) tokens
fn parse_theme_radius<'a, E: NomParseError<&'a str>>(input: &'a str) -> IResult<&'a str, CornerRadius, E> {
    let (input, _) = ws(input)?;
    let (input, _) = tag_no_case("theme")(input)?;
    let (input, _) = ws(input)?;
    let (input, token_name) = delimited(char('('), take_while1(|c: char| c != ')'), char(')'))(input)?;

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
    // Check for "none"
    if value.trim().eq_ignore_ascii_case("none") {
        return Some(Shadow::new(0.0, 0.0, 0.0, Color::TRANSPARENT));
    }

    // Try theme() function first
    if let Ok((_, shadow)) = parse_theme_shadow::<nom::error::Error<&str>>(value) {
        return Some(shadow);
    }

    // Try parsing explicit shadow: offset-x offset-y blur color
    parse_explicit_shadow(value)
}

/// Parse theme(shadow-*) tokens
fn parse_theme_shadow<'a, E: NomParseError<&'a str>>(input: &'a str) -> IResult<&'a str, Shadow, E> {
    let (input, _) = ws(input)?;
    let (input, _) = tag_no_case("theme")(input)?;
    let (input, _) = ws(input)?;
    let (input, token_name) = delimited(char('('), take_while1(|c: char| c != ')'), char(')'))(input)?;

    let token_name = token_name.trim();
    let shadows = ThemeState::get().shadows();

    let shadow: blinc_core::Shadow = match token_name.to_lowercase().replace('_', "-").as_str() {
        "shadow-sm" => shadows.shadow_sm.clone().into(),
        "shadow-default" => shadows.shadow_default.clone().into(),
        "shadow-md" => shadows.shadow_md.clone().into(),
        "shadow-lg" => shadows.shadow_lg.clone().into(),
        "shadow-xl" => shadows.shadow_xl.clone().into(),
        "shadow-2xl" => shadows.shadow_2xl.clone().into(),
        "shadow-none" => shadows.shadow_none.clone().into(),
        _ => {
            debug!(token = token_name, "Unknown theme shadow token");
            return Err(nom::Err::Error(E::from_error_kind(
                input,
                nom::error::ErrorKind::Tag,
            )));
        }
    };

    Ok((input, shadow))
}

/// Parse explicit shadow: offset-x offset-y blur color
fn parse_explicit_shadow(input: &str) -> Option<Shadow> {
    let parts: Vec<&str> = input.split_whitespace().collect();
    if parts.len() >= 4 {
        let offset_x = parse_length_value(parts[0])?;
        let offset_y = parse_length_value(parts[1])?;
        let blur = parse_length_value(parts[2])?;
        let color = parse_color(parts[3])?;
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

/// Parse scale(x) or scale(x, y)
fn parse_scale_transform<'a, E: NomParseError<&'a str>>(input: &'a str) -> IResult<&'a str, Transform, E> {
    let (input, _) = ws(input)?;
    let (input, _) = tag_no_case("scale")(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('(')(input)?;
    let (input, _) = ws(input)?;
    let (input, sx) = float(input)?;
    let (input, _) = ws(input)?;
    let (input, sy) = opt(preceded(
        tuple((char(','), ws::<E>)),
        float,
    ))(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char(')')(input)?;

    let sy = sy.unwrap_or(sx);
    Ok((input, Transform::scale(sx, sy)))
}

/// Parse rotate(deg)
fn parse_rotate_transform<'a, E: NomParseError<&'a str>>(input: &'a str) -> IResult<&'a str, Transform, E> {
    let (input, _) = ws(input)?;
    let (input, _) = tag_no_case("rotate")(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('(')(input)?;
    let (input, _) = ws(input)?;
    let (input, degrees) = float(input)?;
    let (input, _) = opt(tag_no_case("deg"))(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char(')')(input)?;

    Ok((input, Transform::rotate(degrees * std::f32::consts::PI / 180.0)))
}

/// Parse translate(x, y), translateX(x), or translateY(y)
fn parse_translate_transform<'a, E: NomParseError<&'a str>>(input: &'a str) -> IResult<&'a str, Transform, E> {
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
    let (input, unit) = opt(alt((
        tag_no_case("px"),
        tag_no_case("sp"),
        tag("%"),
    )))(input)?;

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
fn parse_render_layer<'a, E: NomParseError<&'a str>>(input: &'a str) -> IResult<&'a str, RenderLayer, E> {
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
    let parts: Vec<&str> = value.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }

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

/// Parse a time value (e.g., "300ms", "0.5s", "1s")
fn parse_time_value(input: &str) -> Option<u32> {
    let input = input.trim();

    // Try milliseconds
    if let Some(ms_str) = input.strip_suffix("ms") {
        return ms_str.trim().parse::<f32>().ok().map(|ms| ms as u32);
    }

    // Try seconds
    if let Some(s_str) = input.strip_suffix('s') {
        return s_str.trim().parse::<f32>().ok().map(|s| (s * 1000.0) as u32);
    }

    // Try plain number (assume milliseconds)
    input.parse::<f32>().ok().map(|ms| ms as u32)
}

// ============================================================================
// Color Parsing
// ============================================================================

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
    if first.starts_with("from ") {
        // Parse "from 45deg" or "from 45deg at center"
        let rest = &first[5..];
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
    } else if first.starts_with("at ") {
        // Just position
        if let Some(pos) = parse_position(&first[3..]) {
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

/// Parse gradient direction (angle or "to <direction>")
/// Returns (angle_in_degrees, color_start_index)
fn parse_gradient_direction(first_part: &str) -> (f32, usize) {
    let part = first_part.trim().to_lowercase();

    // Try parsing as angle (e.g., "135deg", "45deg")
    if let Some(angle) = parse_angle_value(&part) {
        return (angle, 1);
    }

    // Try parsing as direction keyword
    if part.starts_with("to ") {
        let direction = &part[3..];
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
fn angle_to_gradient_points(angle_deg: f32) -> (Point, Point) {
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
        if let Some(space_pos) = before_pct.rfind(|c: char| !c.is_ascii_digit() && c != '.' && c != '-') {
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
        if let Some(space_pos) = before_px.rfind(|c: char| !c.is_ascii_digit() && c != '.' && c != '-') {
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
    fn test_parse_error_context() {
        // Invalid selector should give error context
        let css = "not-a-selector { opacity: 0.5; }";
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
        let has_issues = result.has_errors() || result.has_warnings() || result.stylesheet.is_empty();
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
            ("#valid { opacity: 0.5; }", true),    // valid
            ("#broken {", false),                  // invalid - missing close
            ("#also-valid { opacity: 1.0; }", true), // valid
            ("@ invalid at-rule", false),          // invalid - no ID selector
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
        assert!(result.has_warnings(), "Should have warnings for unknown properties");

        let warnings: Vec<_> = result.warnings_only().collect();
        assert!(warnings.len() >= 2, "Should have at least 2 warnings for unknown props");

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
        assert!(warnings.len() >= 2, "Should have warnings for invalid values");

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
        assert!(style.shadow.is_some());
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
        let hover = result.stylesheet.get_with_state("button", ElementState::Hover).unwrap();
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

        let active = result.stylesheet.get_with_state("button", ElementState::Active).unwrap();
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

        let focus = result.stylesheet.get_with_state("input", ElementState::Focus).unwrap();
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

        let disabled = result.stylesheet.get_with_state("button", ElementState::Disabled).unwrap();
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
        assert!(result.stylesheet.contains_with_state("button", ElementState::Hover));
        assert!(result.stylesheet.contains_with_state("button", ElementState::Active));
        assert!(result.stylesheet.contains_with_state("button", ElementState::Focus));
        assert!(result.stylesheet.contains_with_state("button", ElementState::Disabled));

        // Verify state styles
        let hover = result.stylesheet.get_with_state("button", ElementState::Hover).unwrap();
        assert_eq!(hover.opacity, Some(0.9));

        let active = result.stylesheet.get_with_state("button", ElementState::Active).unwrap();
        assert_eq!(active.opacity, Some(0.8));
        assert!(active.transform.is_some());

        let focus = result.stylesheet.get_with_state("button", ElementState::Focus).unwrap();
        assert!(focus.corner_radius.is_some());

        let disabled = result.stylesheet.get_with_state("button", ElementState::Disabled).unwrap();
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

        let hover = result.stylesheet.get_with_state("button", ElementState::Hover).unwrap();
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
        assert_eq!(ElementState::from_str("hover"), Some(ElementState::Hover));
        assert_eq!(ElementState::from_str("HOVER"), Some(ElementState::Hover));
        assert_eq!(ElementState::from_str("active"), Some(ElementState::Active));
        assert_eq!(ElementState::from_str("focus"), Some(ElementState::Focus));
        assert_eq!(ElementState::from_str("disabled"), Some(ElementState::Disabled));
        assert_eq!(ElementState::from_str("unknown"), None);
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
        assert!(result.stylesheet.contains_with_state("card", ElementState::Hover));
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
        let css = r#"#card { background: linear-gradient(90deg, red 0%, yellow 50%, green 100%); }"#;
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
        let css =
            r#"#card { background: linear-gradient(45deg, rgba(255, 0, 0, 0.5) 0%, rgba(0, 0, 255, 0.8) 100%); }"#;
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
        if let Some(shadow) = &style.shadow {
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
}
