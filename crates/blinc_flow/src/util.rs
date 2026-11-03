//! The small parsing helpers the flow grammar uses: whitespace and comments, identifiers, and the
//! diagnostic type its statements report.

use nom::{
    IResult,
    branch::alt,
    bytes::complete::{tag, take_until, take_while1},
    character::complete::multispace1,
    combinator::value,
    error::{ParseError as NomParseError, VerboseError},
    multi::many0,
    sequence::{delimited, preceded},
};

/// What a flow statement parser returns.
pub(crate) type ParseResult<'a, O> = IResult<&'a str, O, VerboseError<&'a str>>;

/// How serious a diagnostic is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// The block could not be read.
    Error,
    /// The block was read, with something ignored.
    Warning,
    /// Informational.
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

/// A diagnostic from reading a flow.
#[derive(Debug, Clone)]
pub struct ParseError {
    pub severity: Severity,
    pub message: String,
    /// 1-indexed; 0 when the statement did not record a position.
    pub line: usize,
    pub column: usize,
    /// The input fragment the statement failed on.
    pub fragment: String,
    pub contexts: Vec<String>,
    pub property: Option<String>,
    pub value: Option<String>,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "flow {}: line {}, column {}: {}",
            self.severity, self.line, self.column, self.message
        )
    }
}

/// Whitespace and comments (`/* */` and `//`).
pub(crate) fn ws<'a, E: NomParseError<&'a str>>(input: &'a str) -> IResult<&'a str, (), E> {
    value(
        (),
        many0(alt((
            value((), multispace1),
            value((), parse_comment),
            value((), parse_line_comment),
        ))),
    )(input)
}

/// A block comment, `/* ... */`.
pub(crate) fn parse_comment<'a, E: NomParseError<&'a str>>(
    input: &'a str,
) -> IResult<&'a str, &'a str, E> {
    delimited(tag("/*"), take_until("*/"), tag("*/"))(input)
}

/// A `//` line comment.
pub(crate) fn parse_line_comment<'a, E: NomParseError<&'a str>>(
    input: &'a str,
) -> IResult<&'a str, &'a str, E> {
    preceded(tag("//"), nom::bytes::complete::take_till(|c| c == '\n'))(input)
}

/// An identifier: alphanumerics, hyphen and underscore.
pub(crate) fn identifier<'a, E: NomParseError<&'a str>>(
    input: &'a str,
) -> IResult<&'a str, &'a str, E> {
    take_while1(|c: char| c.is_alphanumeric() || c == '-' || c == '_')(input)
}
