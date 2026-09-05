//! CSS `calc()` expression engine
//!
//! Parses and evaluates CSS `calc()` expressions including arithmetic,
//! units, CSS variables, and environment variables.
//!
//! # Example
//!
//! ```ignore
//! use blinc_layout::calc::*;
//!
//! // calc(100% - 20px)
//! let expr = CalcExpr::Sub(
//!     Box::new(CalcExpr::Percentage(1.0)),
//!     Box::new(CalcExpr::Dimension(20.0, CalcUnit::Px)),
//! );
//!
//! let ctx = CalcContext {
//!     parent_size: 400.0,
//!     ..Default::default()
//! };
//! assert_eq!(expr.eval(&ctx), 380.0);
//! ```

use std::collections::HashMap;
use std::fmt;

// ---------------------------------------------------------------------------
// Units
// ---------------------------------------------------------------------------

/// Unit types supported in calc expressions
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CalcUnit {
    /// Pixels (absolute)
    Px,
    /// Font-size relative (relative to element font-size)
    Em,
    /// Root font-size relative
    Rem,
    /// Degrees (angle)
    Deg,
    /// Radians (angle)
    Rad,
    /// Turns (angle, 1turn = 360deg)
    Turn,
    /// Seconds (time)
    S,
    /// Milliseconds (time)
    Ms,
    /// Viewport width (1vw = 1% of viewport width)
    Vw,
    /// Viewport height (1vh = 1% of viewport height)
    Vh,
}

impl CalcUnit {
    /// Parse a unit suffix string
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "px" => Some(Self::Px),
            "em" => Some(Self::Em),
            "rem" => Some(Self::Rem),
            "deg" => Some(Self::Deg),
            "rad" => Some(Self::Rad),
            "turn" => Some(Self::Turn),
            "s" => Some(Self::S),
            "ms" => Some(Self::Ms),
            "vw" => Some(Self::Vw),
            "vh" => Some(Self::Vh),
            _ => None,
        }
    }

    /// Convert a value with this unit to pixels (or base unit)
    pub fn to_pixels(&self, value: f32, ctx: &CalcContext) -> f32 {
        match self {
            Self::Px => value,
            Self::Em => value * ctx.font_size,
            Self::Rem => value * ctx.root_font_size,
            Self::Deg => value, // angles stay as degrees
            Self::Rad => value * (180.0 / std::f32::consts::PI),
            Self::Turn => value * 360.0,
            Self::S => value * 1000.0, // convert to ms
            Self::Ms => value,
            Self::Vw => value * ctx.viewport_width / 100.0,
            Self::Vh => value * ctx.viewport_height / 100.0,
        }
    }
}

impl fmt::Display for CalcUnit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Px => write!(f, "px"),
            Self::Em => write!(f, "em"),
            Self::Rem => write!(f, "rem"),
            Self::Deg => write!(f, "deg"),
            Self::Rad => write!(f, "rad"),
            Self::Turn => write!(f, "turn"),
            Self::S => write!(f, "s"),
            Self::Ms => write!(f, "ms"),
            Self::Vw => write!(f, "vw"),
            Self::Vh => write!(f, "vh"),
        }
    }
}

// ---------------------------------------------------------------------------
// Expression AST
// ---------------------------------------------------------------------------

/// A CSS `calc()` expression tree — evaluated at layout/render time
#[derive(Clone, Debug, PartialEq)]
pub enum CalcExpr {
    /// Bare number (unitless)
    Literal(f32),
    /// Percentage value (0.5 = 50%)
    Percentage(f32),
    /// Number with a unit
    Dimension(f32, CalcUnit),
    /// CSS variable reference: `var(--name)` — resolved before eval
    Var(String),
    /// Environment variable: `env(pointer-x)` — resolved per-frame
    EnvVar(String),

    // Arithmetic
    /// Addition: `a + b`
    Add(Box<CalcExpr>, Box<CalcExpr>),
    /// Subtraction: `a - b`
    Sub(Box<CalcExpr>, Box<CalcExpr>),
    /// Multiplication: `a * b`
    Mul(Box<CalcExpr>, Box<CalcExpr>),
    /// Division: `a / b`
    Div(Box<CalcExpr>, Box<CalcExpr>),
    /// Negation: `-a`
    Neg(Box<CalcExpr>),

    // Standard CSS math functions
    /// `clamp(min, val, max)`
    Clamp(Box<CalcExpr>, Box<CalcExpr>, Box<CalcExpr>),
    /// `min(a, b)`
    Min(Box<CalcExpr>, Box<CalcExpr>),
    /// `max(a, b)`
    Max(Box<CalcExpr>, Box<CalcExpr>),

    // Blinc extensions (available in later phases)
    /// `mix(a, b, t)` → `a + (b - a) * t`
    Mix(Box<CalcExpr>, Box<CalcExpr>, Box<CalcExpr>),
    /// `smoothstep(edge0, edge1, x)` → Hermite interpolation
    Smoothstep(Box<CalcExpr>, Box<CalcExpr>, Box<CalcExpr>),
    /// `step(edge, x)` → 0 if x < edge, 1 otherwise
    Step(Box<CalcExpr>, Box<CalcExpr>),
    /// `remap(val, in_lo, in_hi, out_lo, out_hi)`
    Remap {
        val: Box<CalcExpr>,
        in_lo: Box<CalcExpr>,
        in_hi: Box<CalcExpr>,
        out_lo: Box<CalcExpr>,
        out_hi: Box<CalcExpr>,
    },
}

// ---------------------------------------------------------------------------
// Evaluation context
// ---------------------------------------------------------------------------

/// Context for evaluating calc expressions
#[derive(Clone, Debug)]
pub struct CalcContext {
    /// Size of the containing block (for percentage resolution)
    pub parent_size: f32,
    /// Viewport width in logical pixels
    pub viewport_width: f32,
    /// Viewport height in logical pixels
    pub viewport_height: f32,
    /// Element's computed font size
    pub font_size: f32,
    /// Root element font size
    pub root_font_size: f32,
    /// Environment variable resolver (e.g., pointer-x, pointer-y)
    pub env_vars: HashMap<String, f32>,
    /// CSS custom property resolver (--name → value)
    pub css_vars: HashMap<String, f32>,
}

impl Default for CalcContext {
    fn default() -> Self {
        Self {
            parent_size: 0.0,
            viewport_width: 0.0,
            viewport_height: 0.0,
            font_size: 16.0,
            root_font_size: 16.0,
            env_vars: HashMap::new(),
            css_vars: HashMap::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Evaluation
// ---------------------------------------------------------------------------

impl CalcExpr {
    /// Evaluate the expression tree to a concrete f32 value
    pub fn eval(&self, ctx: &CalcContext) -> f32 {
        match self {
            Self::Literal(v) => *v,
            Self::Percentage(p) => p * ctx.parent_size,
            Self::Dimension(v, unit) => unit.to_pixels(*v, ctx),
            Self::Var(name) => ctx.css_vars.get(name).copied().unwrap_or(0.0),
            Self::EnvVar(name) => ctx.env_vars.get(name).copied().unwrap_or(0.0),

            Self::Add(a, b) => a.eval(ctx) + b.eval(ctx),
            Self::Sub(a, b) => a.eval(ctx) - b.eval(ctx),
            Self::Mul(a, b) => a.eval(ctx) * b.eval(ctx),
            Self::Div(a, b) => {
                let denom = b.eval(ctx);
                if denom.abs() < 1e-10 {
                    0.0
                } else {
                    a.eval(ctx) / denom
                }
            }
            Self::Neg(a) => -a.eval(ctx),

            Self::Clamp(min, val, max) => {
                let min_v = min.eval(ctx);
                let val_v = val.eval(ctx);
                let max_v = max.eval(ctx);
                val_v.clamp(min_v, max_v)
            }
            Self::Min(a, b) => a.eval(ctx).min(b.eval(ctx)),
            Self::Max(a, b) => a.eval(ctx).max(b.eval(ctx)),

            Self::Mix(a, b, t) => {
                let a_v = a.eval(ctx);
                let b_v = b.eval(ctx);
                let t_v = t.eval(ctx);
                a_v + (b_v - a_v) * t_v
            }
            Self::Smoothstep(edge0, edge1, x) => {
                let e0 = edge0.eval(ctx);
                let e1 = edge1.eval(ctx);
                let x_v = x.eval(ctx);
                let t = ((x_v - e0) / (e1 - e0)).clamp(0.0, 1.0);
                t * t * (3.0 - 2.0 * t)
            }
            Self::Step(edge, x) => {
                if x.eval(ctx) < edge.eval(ctx) {
                    0.0
                } else {
                    1.0
                }
            }
            Self::Remap {
                val,
                in_lo,
                in_hi,
                out_lo,
                out_hi,
            } => {
                let v = val.eval(ctx);
                let il = in_lo.eval(ctx);
                let ih = in_hi.eval(ctx);
                let ol = out_lo.eval(ctx);
                let oh = out_hi.eval(ctx);
                let range = ih - il;
                if range.abs() < 1e-10 {
                    ol
                } else {
                    let t = (v - il) / range;
                    ol + (oh - ol) * t
                }
            }
        }
    }

    /// Returns true if this expression contains any `EnvVar` references
    /// (meaning it needs per-frame re-evaluation)
    pub fn is_dynamic(&self) -> bool {
        match self {
            Self::Literal(_) | Self::Percentage(_) | Self::Dimension(_, _) | Self::Var(_) => false,
            Self::EnvVar(_) => true,
            Self::Add(a, b)
            | Self::Sub(a, b)
            | Self::Mul(a, b)
            | Self::Div(a, b)
            | Self::Min(a, b)
            | Self::Max(a, b)
            | Self::Step(a, b) => a.is_dynamic() || b.is_dynamic(),
            Self::Neg(a) => a.is_dynamic(),
            Self::Clamp(a, b, c) | Self::Mix(a, b, c) | Self::Smoothstep(a, b, c) => {
                a.is_dynamic() || b.is_dynamic() || c.is_dynamic()
            }
            Self::Remap {
                val,
                in_lo,
                in_hi,
                out_lo,
                out_hi,
            } => {
                val.is_dynamic()
                    || in_lo.is_dynamic()
                    || in_hi.is_dynamic()
                    || out_lo.is_dynamic()
                    || out_hi.is_dynamic()
            }
        }
    }

    /// Returns true if this expression contains any `Var` references
    pub fn has_css_vars(&self) -> bool {
        match self {
            Self::Var(_) => true,
            Self::Literal(_) | Self::Percentage(_) | Self::Dimension(_, _) | Self::EnvVar(_) => {
                false
            }
            Self::Add(a, b)
            | Self::Sub(a, b)
            | Self::Mul(a, b)
            | Self::Div(a, b)
            | Self::Min(a, b)
            | Self::Max(a, b)
            | Self::Step(a, b) => a.has_css_vars() || b.has_css_vars(),
            Self::Neg(a) => a.has_css_vars(),
            Self::Clamp(a, b, c) | Self::Mix(a, b, c) | Self::Smoothstep(a, b, c) => {
                a.has_css_vars() || b.has_css_vars() || c.has_css_vars()
            }
            Self::Remap {
                val,
                in_lo,
                in_hi,
                out_lo,
                out_hi,
            } => {
                val.has_css_vars()
                    || in_lo.has_css_vars()
                    || in_hi.has_css_vars()
                    || out_lo.has_css_vars()
                    || out_hi.has_css_vars()
            }
        }
    }
}

// ---------------------------------------------------------------------------
// CalcOrValue — wraps properties that can be either fixed or calc()
// ---------------------------------------------------------------------------

/// A property value that's either a fixed value or a `calc()` expression
#[derive(Clone, Debug, PartialEq)]
pub enum CalcOrValue<T: Clone + PartialEq> {
    /// Fixed, pre-resolved value
    Fixed(T),
    /// Dynamic calc expression (needs context to evaluate)
    Calc(CalcExpr),
}

impl<T: Clone + PartialEq> CalcOrValue<T> {
    /// Returns the fixed value if available, or None if this is a calc expression
    pub fn as_fixed(&self) -> Option<&T> {
        match self {
            Self::Fixed(v) => Some(v),
            Self::Calc(_) => None,
        }
    }

    /// Returns true if this is a calc expression
    pub fn is_calc(&self) -> bool {
        matches!(self, Self::Calc(_))
    }

    /// Returns true if this is a dynamic calc (contains env vars)
    pub fn is_dynamic(&self) -> bool {
        match self {
            Self::Fixed(_) => false,
            Self::Calc(expr) => expr.is_dynamic(),
        }
    }
}

impl CalcOrValue<f32> {
    /// Evaluate to f32, using the calc context if needed
    pub fn resolve(&self, ctx: &CalcContext) -> f32 {
        match self {
            Self::Fixed(v) => *v,
            Self::Calc(expr) => expr.eval(ctx),
        }
    }
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/// Parse a `calc(...)` expression string into a CalcExpr tree.
///
/// Supports:
/// - Arithmetic: `+`, `-`, `*`, `/`
/// - Units: `px`, `em`, `rem`, `deg`, `rad`, `turn`, `s`, `ms`, `vw`, `vh`
/// - Percentages: `50%`
/// - Nested parentheses
/// - CSS functions: `min()`, `max()`, `clamp()`
/// - Environment variables: `env(name)`
/// - CSS variables: `var(--name)`
/// - Blinc extensions: `mix()`, `smoothstep()`, `step()`, `remap()`
///
/// Returns None if parsing fails.
pub fn parse_calc(input: &str) -> Option<CalcExpr> {
    let trimmed = input.trim();

    // Strip outer calc(...) wrapper if present
    let inner = if trimmed.starts_with("calc(") && trimmed.ends_with(')') {
        &trimmed[5..trimmed.len() - 1]
    } else {
        trimmed
    };

    let tokens = tokenize(inner)?;
    let (expr, rest) = parse_additive(&tokens)?;
    if rest.is_empty() {
        Some(expr)
    } else {
        None // Unconsumed tokens
    }
}

// ---------------------------------------------------------------------------
// Tokenizer
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Number(f32),
    Percent(f32),
    Dimension(f32, CalcUnit),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    LParen,
    RParen,
    Comma,
}

fn tokenize(input: &str) -> Option<Vec<Token>> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let c = chars[i];

        // Skip whitespace
        if c.is_whitespace() {
            i += 1;
            continue;
        }

        match c {
            '+' => {
                tokens.push(Token::Plus);
                i += 1;
            }
            '*' => {
                tokens.push(Token::Star);
                i += 1;
            }
            '/' => {
                tokens.push(Token::Slash);
                i += 1;
            }
            '(' => {
                tokens.push(Token::LParen);
                i += 1;
            }
            ')' => {
                tokens.push(Token::RParen);
                i += 1;
            }
            ',' => {
                tokens.push(Token::Comma);
                i += 1;
            }
            '-' => {
                // Check for CSS custom property `--name`
                if i + 2 < len
                    && chars[i + 1] == '-'
                    && (chars[i + 2].is_ascii_alphabetic() || chars[i + 2] == '_')
                {
                    // Parse `--name` as an identifier (CSS custom property)
                    let start = i;
                    i += 2; // skip `--`
                    while i < len
                        && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '-')
                    {
                        i += 1;
                    }
                    let ident: String = chars[start..i].iter().collect();
                    tokens.push(Token::Ident(ident));
                    continue;
                }

                // Check if this is a negative number or a minus operator
                // It's a negative number if:
                // - It's the first token, OR
                // - The previous token is an operator or LParen or Comma
                let is_unary = tokens.is_empty()
                    || matches!(
                        tokens.last(),
                        Some(
                            Token::Plus
                                | Token::Minus
                                | Token::Star
                                | Token::Slash
                                | Token::LParen
                                | Token::Comma
                        )
                    );

                if is_unary && i + 1 < len && (chars[i + 1].is_ascii_digit() || chars[i + 1] == '.')
                {
                    // Parse as negative number
                    let start = i;
                    i += 1;
                    while i < len && (chars[i].is_ascii_digit() || chars[i] == '.') {
                        i += 1;
                    }
                    let num_str: String = chars[start..i].iter().collect();
                    let num: f32 = num_str.parse().ok()?;

                    // Check for unit or %
                    if i < len && chars[i] == '%' {
                        tokens.push(Token::Percent(num / 100.0));
                        i += 1;
                    } else {
                        let unit_start = i;
                        while i < len && chars[i].is_ascii_alphabetic() {
                            i += 1;
                        }
                        if i > unit_start {
                            let unit_str: String = chars[unit_start..i].iter().collect();
                            if let Some(unit) = CalcUnit::parse(&unit_str) {
                                tokens.push(Token::Dimension(num, unit));
                            } else {
                                return None;
                            }
                        } else {
                            tokens.push(Token::Number(num));
                        }
                    }
                } else {
                    tokens.push(Token::Minus);
                    i += 1;
                }
            }
            _ if c.is_ascii_digit() || c == '.' => {
                let start = i;
                while i < len && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                let num_str: String = chars[start..i].iter().collect();
                let num: f32 = num_str.parse().ok()?;

                // Check for % or unit suffix
                if i < len && chars[i] == '%' {
                    tokens.push(Token::Percent(num / 100.0));
                    i += 1;
                } else {
                    let unit_start = i;
                    while i < len && chars[i].is_ascii_alphabetic() {
                        i += 1;
                    }
                    if i > unit_start {
                        let unit_str: String = chars[unit_start..i].iter().collect();
                        if let Some(unit) = CalcUnit::parse(&unit_str) {
                            tokens.push(Token::Dimension(num, unit));
                        } else {
                            return None;
                        }
                    } else {
                        tokens.push(Token::Number(num));
                    }
                }
            }
            '#' => {
                // Hex color — parse as a literal number for now (just the alpha channel)
                // Full color support can be added later
                i += 1;
                while i < len && chars[i].is_ascii_hexdigit() {
                    i += 1;
                }
                // Skip hex colors in calc — they don't make sense in arithmetic
                return None;
            }
            _ if c.is_ascii_alphabetic() || c == '_' || c == '-' => {
                let start = i;
                while i < len
                    && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '-')
                {
                    i += 1;
                }
                let ident: String = chars[start..i].iter().collect();
                tokens.push(Token::Ident(ident));
            }
            _ => return None,
        }
    }

    Some(tokens)
}

// ---------------------------------------------------------------------------
// Recursive descent parser
// ---------------------------------------------------------------------------

type ParseResult<'a> = Option<(CalcExpr, &'a [Token])>;

/// Parse additive expression: term (('+' | '-') term)*
fn parse_additive(tokens: &[Token]) -> ParseResult<'_> {
    let (mut left, mut rest) = parse_multiplicative(tokens)?;

    loop {
        match rest.first() {
            Some(Token::Plus) => {
                let (right, r) = parse_multiplicative(&rest[1..])?;
                left = CalcExpr::Add(Box::new(left), Box::new(right));
                rest = r;
            }
            Some(Token::Minus) => {
                let (right, r) = parse_multiplicative(&rest[1..])?;
                left = CalcExpr::Sub(Box::new(left), Box::new(right));
                rest = r;
            }
            _ => break,
        }
    }

    Some((left, rest))
}

/// Parse multiplicative expression: unary (('*' | '/') unary)*
fn parse_multiplicative(tokens: &[Token]) -> ParseResult<'_> {
    let (mut left, mut rest) = parse_unary(tokens)?;

    loop {
        match rest.first() {
            Some(Token::Star) => {
                let (right, r) = parse_unary(&rest[1..])?;
                left = CalcExpr::Mul(Box::new(left), Box::new(right));
                rest = r;
            }
            Some(Token::Slash) => {
                let (right, r) = parse_unary(&rest[1..])?;
                left = CalcExpr::Div(Box::new(left), Box::new(right));
                rest = r;
            }
            _ => break,
        }
    }

    Some((left, rest))
}

/// Parse unary: '-' unary | primary
fn parse_unary(tokens: &[Token]) -> ParseResult<'_> {
    if let Some(Token::Minus) = tokens.first() {
        let (expr, rest) = parse_unary(&tokens[1..])?;
        Some((CalcExpr::Neg(Box::new(expr)), rest))
    } else {
        parse_primary(tokens)
    }
}

/// Parse primary: number | percent | dimension | function call | parenthesized | ident
fn parse_primary(tokens: &[Token]) -> ParseResult<'_> {
    match tokens.first()? {
        Token::Number(n) => Some((CalcExpr::Literal(*n), &tokens[1..])),
        Token::Percent(p) => Some((CalcExpr::Percentage(*p), &tokens[1..])),
        Token::Dimension(v, u) => Some((CalcExpr::Dimension(*v, *u), &tokens[1..])),

        Token::LParen => {
            let (expr, rest) = parse_additive(&tokens[1..])?;
            if matches!(rest.first(), Some(Token::RParen)) {
                Some((expr, &rest[1..]))
            } else {
                None // Missing closing paren
            }
        }

        Token::Ident(name) => {
            // Check if followed by '(' → function call
            if matches!(tokens.get(1), Some(Token::LParen)) {
                parse_function_call(name, &tokens[2..])
            } else {
                // Bare identifier — treat as env var reference (e.g., pointer-x)
                Some((CalcExpr::EnvVar(name.clone()), &tokens[1..]))
            }
        }

        _ => None,
    }
}

/// Parse a function call: name '(' args ')' — tokens start after the '('
fn parse_function_call<'a>(name: &str, tokens: &'a [Token]) -> ParseResult<'a> {
    match name {
        "env" => {
            // env(name) or env(name, fallback)
            if let Some(Token::Ident(var_name)) = tokens.first() {
                let var_name = var_name.clone();
                let rest = &tokens[1..];
                if matches!(rest.first(), Some(Token::RParen)) {
                    Some((CalcExpr::EnvVar(var_name), &rest[1..]))
                } else if matches!(rest.first(), Some(Token::Comma)) {
                    // env(name, fallback) — parse fallback but use var if available
                    let (_fallback, rest2) = parse_additive(&rest[1..])?;
                    if matches!(rest2.first(), Some(Token::RParen)) {
                        // For now, just use the env var (fallback evaluation would need runtime)
                        Some((CalcExpr::EnvVar(var_name), &rest2[1..]))
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        }

        "var" => {
            // var(--name)
            if let Some(Token::Ident(var_name)) = tokens.first() {
                let clean_name = var_name.strip_prefix("--").unwrap_or(var_name);
                let rest = &tokens[1..];
                if matches!(rest.first(), Some(Token::RParen)) {
                    Some((CalcExpr::Var(clean_name.to_string()), &rest[1..]))
                } else {
                    None
                }
            } else {
                None
            }
        }

        "calc" => {
            // Nested calc()
            let (expr, rest) = parse_additive(tokens)?;
            if matches!(rest.first(), Some(Token::RParen)) {
                Some((expr, &rest[1..]))
            } else {
                None
            }
        }

        // 2-argument functions
        "min" => parse_two_arg_func(tokens, |a, b| CalcExpr::Min(Box::new(a), Box::new(b))),
        "max" => parse_two_arg_func(tokens, |a, b| CalcExpr::Max(Box::new(a), Box::new(b))),
        "step" => parse_two_arg_func(tokens, |a, b| CalcExpr::Step(Box::new(a), Box::new(b))),

        // 3-argument functions
        "clamp" => parse_three_arg_func(tokens, |a, b, c| {
            CalcExpr::Clamp(Box::new(a), Box::new(b), Box::new(c))
        }),
        "mix" => parse_three_arg_func(tokens, |a, b, c| {
            CalcExpr::Mix(Box::new(a), Box::new(b), Box::new(c))
        }),
        "smoothstep" => parse_three_arg_func(tokens, |a, b, c| {
            CalcExpr::Smoothstep(Box::new(a), Box::new(b), Box::new(c))
        }),

        // 5-argument function
        "remap" => {
            let (val, rest) = parse_additive(tokens)?;
            let rest = expect_comma(rest)?;
            let (in_lo, rest) = parse_additive(rest)?;
            let rest = expect_comma(rest)?;
            let (in_hi, rest) = parse_additive(rest)?;
            let rest = expect_comma(rest)?;
            let (out_lo, rest) = parse_additive(rest)?;
            let rest = expect_comma(rest)?;
            let (out_hi, rest) = parse_additive(rest)?;
            if matches!(rest.first(), Some(Token::RParen)) {
                Some((
                    CalcExpr::Remap {
                        val: Box::new(val),
                        in_lo: Box::new(in_lo),
                        in_hi: Box::new(in_hi),
                        out_lo: Box::new(out_lo),
                        out_hi: Box::new(out_hi),
                    },
                    &rest[1..],
                ))
            } else {
                None
            }
        }

        _ => None, // Unknown function
    }
}

fn expect_comma(tokens: &[Token]) -> Option<&[Token]> {
    if matches!(tokens.first(), Some(Token::Comma)) {
        Some(&tokens[1..])
    } else {
        None
    }
}

fn parse_two_arg_func(
    tokens: &[Token],
    build: impl Fn(CalcExpr, CalcExpr) -> CalcExpr,
) -> ParseResult<'_> {
    let (a, rest) = parse_additive(tokens)?;
    let rest = expect_comma(rest)?;
    let (b, rest) = parse_additive(rest)?;
    if matches!(rest.first(), Some(Token::RParen)) {
        Some((build(a, b), &rest[1..]))
    } else {
        None
    }
}

fn parse_three_arg_func(
    tokens: &[Token],
    build: impl Fn(CalcExpr, CalcExpr, CalcExpr) -> CalcExpr,
) -> ParseResult<'_> {
    let (a, rest) = parse_additive(tokens)?;
    let rest = expect_comma(rest)?;
    let (b, rest) = parse_additive(rest)?;
    let rest = expect_comma(rest)?;
    let (c, rest) = parse_additive(rest)?;
    if matches!(rest.first(), Some(Token::RParen)) {
        Some((build(a, b, c), &rest[1..]))
    } else {
        None
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> CalcContext {
        CalcContext {
            parent_size: 400.0,
            viewport_width: 1920.0,
            viewport_height: 1080.0,
            font_size: 16.0,
            root_font_size: 16.0,
            env_vars: {
                let mut m = HashMap::new();
                m.insert("pointer-x".to_string(), 0.5);
                m.insert("pointer-y".to_string(), -0.3);
                m.insert("pointer-distance".to_string(), 0.7);
                m
            },
            css_vars: {
                let mut m = HashMap::new();
                m.insert("spacing".to_string(), 8.0);
                m
            },
        }
    }

    // -----------------------------------------------------------------------
    // Basic arithmetic
    // -----------------------------------------------------------------------

    #[test]
    fn test_literal() {
        let expr = parse_calc("42").unwrap();
        assert_eq!(expr.eval(&ctx()), 42.0);
    }

    #[test]
    fn test_addition() {
        let expr = parse_calc("10 + 20").unwrap();
        assert_eq!(expr.eval(&ctx()), 30.0);
    }

    #[test]
    fn test_subtraction() {
        let expr = parse_calc("50 - 15").unwrap();
        assert_eq!(expr.eval(&ctx()), 35.0);
    }

    #[test]
    fn test_multiplication() {
        let expr = parse_calc("6 * 7").unwrap();
        assert_eq!(expr.eval(&ctx()), 42.0);
    }

    #[test]
    fn test_division() {
        let expr = parse_calc("100 / 4").unwrap();
        assert_eq!(expr.eval(&ctx()), 25.0);
    }

    #[test]
    fn test_division_by_zero() {
        let expr = parse_calc("100 / 0").unwrap();
        assert_eq!(expr.eval(&ctx()), 0.0);
    }

    #[test]
    fn test_operator_precedence() {
        // 2 + 3 * 4 = 14 (not 20)
        let expr = parse_calc("2 + 3 * 4").unwrap();
        assert_eq!(expr.eval(&ctx()), 14.0);
    }

    #[test]
    fn test_parentheses() {
        // (2 + 3) * 4 = 20
        let expr = parse_calc("(2 + 3) * 4").unwrap();
        assert_eq!(expr.eval(&ctx()), 20.0);
    }

    #[test]
    fn test_negation() {
        let expr = parse_calc("-5 + 10").unwrap();
        assert_eq!(expr.eval(&ctx()), 5.0);
    }

    #[test]
    fn test_negative_number() {
        let expr = parse_calc("-3.5").unwrap();
        assert_eq!(expr.eval(&ctx()), -3.5);
    }

    #[test]
    fn test_complex_expression() {
        // (100 - 20) / 2 + 5 * 3 = 40 + 15 = 55
        let expr = parse_calc("(100 - 20) / 2 + 5 * 3").unwrap();
        assert_eq!(expr.eval(&ctx()), 55.0);
    }

    // -----------------------------------------------------------------------
    // Units and percentages
    // -----------------------------------------------------------------------

    #[test]
    fn test_percentage() {
        let expr = parse_calc("50%").unwrap();
        assert_eq!(expr.eval(&ctx()), 200.0); // 50% of 400
    }

    #[test]
    fn test_percentage_minus_px() {
        // calc(100% - 20px)
        let expr = parse_calc("100% - 20px").unwrap();
        assert_eq!(expr.eval(&ctx()), 380.0);
    }

    #[test]
    fn test_dimension_px() {
        let expr = parse_calc("24px").unwrap();
        assert_eq!(expr.eval(&ctx()), 24.0);
    }

    #[test]
    fn test_dimension_em() {
        let expr = parse_calc("2em").unwrap();
        assert_eq!(expr.eval(&ctx()), 32.0); // 2 * 16
    }

    #[test]
    fn test_dimension_vw() {
        let expr = parse_calc("10vw").unwrap();
        assert_eq!(expr.eval(&ctx()), 192.0); // 10% of 1920
    }

    #[test]
    fn test_dimension_deg() {
        let expr = parse_calc("45deg").unwrap();
        assert_eq!(expr.eval(&ctx()), 45.0);
    }

    // -----------------------------------------------------------------------
    // calc() wrapper
    // -----------------------------------------------------------------------

    #[test]
    fn test_calc_wrapper() {
        let expr = parse_calc("calc(100% - 20px)").unwrap();
        assert_eq!(expr.eval(&ctx()), 380.0);
    }

    // -----------------------------------------------------------------------
    // CSS functions
    // -----------------------------------------------------------------------

    #[test]
    fn test_min() {
        let expr = parse_calc("min(100, 50)").unwrap();
        assert_eq!(expr.eval(&ctx()), 50.0);
    }

    #[test]
    fn test_max() {
        let expr = parse_calc("max(100, 50)").unwrap();
        assert_eq!(expr.eval(&ctx()), 100.0);
    }

    #[test]
    fn test_clamp() {
        let expr = parse_calc("clamp(10, 50, 30)").unwrap();
        assert_eq!(expr.eval(&ctx()), 30.0); // clamped to max

        let expr2 = parse_calc("clamp(10, 5, 30)").unwrap();
        assert_eq!(expr2.eval(&ctx()), 10.0); // clamped to min

        let expr3 = parse_calc("clamp(10, 20, 30)").unwrap();
        assert_eq!(expr3.eval(&ctx()), 20.0); // in range
    }

    // -----------------------------------------------------------------------
    // Blinc extensions
    // -----------------------------------------------------------------------

    #[test]
    fn test_mix() {
        // mix(0, 100, 0.5) = 50
        let expr = parse_calc("mix(0, 100, 0.5)").unwrap();
        assert_eq!(expr.eval(&ctx()), 50.0);
    }

    #[test]
    fn test_smoothstep() {
        // smoothstep(0, 1, 0.5) = 0.5^2 * (3 - 2*0.5) = 0.25 * 2 = 0.5
        let expr = parse_calc("smoothstep(0, 1, 0.5)").unwrap();
        assert_eq!(expr.eval(&ctx()), 0.5);

        // At edges: smoothstep(0, 1, 0) = 0, smoothstep(0, 1, 1) = 1
        let expr0 = parse_calc("smoothstep(0, 1, 0)").unwrap();
        assert_eq!(expr0.eval(&ctx()), 0.0);
        let expr1 = parse_calc("smoothstep(0, 1, 1)").unwrap();
        assert_eq!(expr1.eval(&ctx()), 1.0);
    }

    #[test]
    fn test_step() {
        let expr = parse_calc("step(0.5, 0.3)").unwrap();
        assert_eq!(expr.eval(&ctx()), 0.0);

        let expr2 = parse_calc("step(0.5, 0.7)").unwrap();
        assert_eq!(expr2.eval(&ctx()), 1.0);
    }

    #[test]
    fn test_remap() {
        // remap(0.5, 0, 1, 100, 200) = 150
        let expr = parse_calc("remap(0.5, 0, 1, 100, 200)").unwrap();
        assert_eq!(expr.eval(&ctx()), 150.0);
    }

    // -----------------------------------------------------------------------
    // Environment variables
    // -----------------------------------------------------------------------

    #[test]
    fn test_env_var() {
        let expr = parse_calc("env(pointer-x)").unwrap();
        assert_eq!(expr.eval(&ctx()), 0.5);
    }

    #[test]
    fn test_env_var_in_expression() {
        // pointer-x * 15 = 0.5 * 15 = 7.5
        let expr = parse_calc("pointer-x * 15").unwrap();
        assert_eq!(expr.eval(&ctx()), 7.5);
    }

    #[test]
    fn test_env_var_complex() {
        // mix(0.9, 0.05, pointer-distance) = 0.9 + (0.05 - 0.9) * 0.7 = 0.9 - 0.595 = 0.305
        let expr = parse_calc("mix(0.9, 0.05, pointer-distance)").unwrap();
        let result = expr.eval(&ctx());
        assert!((result - 0.305).abs() < 1e-4);
    }

    // -----------------------------------------------------------------------
    // CSS variables
    // -----------------------------------------------------------------------

    #[test]
    fn test_css_var() {
        let expr = parse_calc("var(--spacing)").unwrap();
        assert_eq!(expr.eval(&ctx()), 8.0);
    }

    #[test]
    fn test_css_var_in_expression() {
        let expr = parse_calc("var(--spacing) * 2").unwrap();
        assert_eq!(expr.eval(&ctx()), 16.0);
    }

    // -----------------------------------------------------------------------
    // is_dynamic
    // -----------------------------------------------------------------------

    #[test]
    fn test_is_dynamic_false() {
        let expr = parse_calc("100% - 20px").unwrap();
        assert!(!expr.is_dynamic());
    }

    #[test]
    fn test_is_dynamic_true() {
        let expr = parse_calc("pointer-x * 15deg").unwrap();
        assert!(expr.is_dynamic());
    }

    #[test]
    fn test_is_dynamic_nested() {
        let expr = parse_calc("50 + mix(0, 100, pointer-distance)").unwrap();
        assert!(expr.is_dynamic());
    }

    // -----------------------------------------------------------------------
    // Error cases
    // -----------------------------------------------------------------------

    #[test]
    fn test_empty_string() {
        assert!(parse_calc("").is_none());
    }

    #[test]
    fn test_unbalanced_parens() {
        assert!(parse_calc("(10 + 20").is_none());
    }

    #[test]
    fn test_unknown_unit() {
        assert!(parse_calc("10xyz").is_none());
    }

    // -----------------------------------------------------------------------
    // Pointer query demo expressions (exact CSS values)
    // -----------------------------------------------------------------------

    #[test]
    fn test_demo_mix_with_px_unit() {
        // border-radius: calc(mix(4, 48, env(pointer-inside)) * 1px)
        let expr = parse_calc("calc(mix(4, 48, env(pointer-inside)) * 1px)");
        assert!(
            expr.is_some(),
            "Failed to parse: calc(mix(4, 48, env(pointer-inside)) * 1px)"
        );
        let expr = expr.unwrap();
        assert!(expr.is_dynamic());
        // With pointer-inside=0 → mix(4,48,0) * 1 = 4
        let mut c = CalcContext::default();
        c.env_vars.insert("pointer-inside".to_string(), 0.0);
        assert_eq!(expr.eval(&c), 4.0);
        // With pointer-inside=1 → mix(4,48,1) * 1 = 48
        c.env_vars.insert("pointer-inside".to_string(), 1.0);
        assert_eq!(expr.eval(&c), 48.0);
    }

    #[test]
    fn test_demo_border_width_calc() {
        // border-width: calc(mix(0, 3, env(pointer-inside)) * 1px)
        let expr = parse_calc("calc(mix(0, 3, env(pointer-inside)) * 1px)");
        assert!(
            expr.is_some(),
            "Failed to parse: calc(mix(0, 3, env(pointer-inside)) * 1px)"
        );
    }

    #[test]
    fn test_demo_rotate_calc() {
        // rotate-y: calc(env(pointer-x) * env(pointer-inside) * 15deg)
        let expr = parse_calc("calc(env(pointer-x) * env(pointer-inside) * 15deg)");
        assert!(
            expr.is_some(),
            "Failed to parse: calc(env(pointer-x) * env(pointer-inside) * 15deg)"
        );
        let expr = expr.unwrap();
        let mut c = CalcContext::default();
        c.env_vars.insert("pointer-x".to_string(), 0.5);
        c.env_vars.insert("pointer-inside".to_string(), 1.0);
        // 0.5 * 1.0 * 15 = 7.5
        assert_eq!(expr.eval(&c), 7.5);
    }

    #[test]
    fn test_demo_opacity_mix() {
        // opacity: calc(mix(0.3, 1.0, env(pointer-inside)))
        let expr = parse_calc("calc(mix(0.3, 1.0, env(pointer-inside)))");
        assert!(
            expr.is_some(),
            "Failed to parse: calc(mix(0.3, 1.0, env(pointer-inside)))"
        );
    }

    // -----------------------------------------------------------------------
    // CalcOrValue
    // -----------------------------------------------------------------------

    #[test]
    fn test_calc_or_value_fixed() {
        let v: CalcOrValue<f32> = CalcOrValue::Fixed(42.0);
        assert_eq!(v.resolve(&ctx()), 42.0);
        assert!(!v.is_calc());
        assert!(!v.is_dynamic());
    }

    #[test]
    fn test_calc_or_value_calc() {
        let v: CalcOrValue<f32> = CalcOrValue::Calc(parse_calc("100% - 20px").unwrap());
        assert_eq!(v.resolve(&ctx()), 380.0);
        assert!(v.is_calc());
        assert!(!v.is_dynamic());
    }

    #[test]
    fn test_calc_or_value_dynamic() {
        let v: CalcOrValue<f32> = CalcOrValue::Calc(parse_calc("pointer-x * 100").unwrap());
        assert_eq!(v.resolve(&ctx()), 50.0);
        assert!(v.is_calc());
        assert!(v.is_dynamic());
    }
}
