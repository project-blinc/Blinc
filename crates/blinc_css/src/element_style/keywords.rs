//! Keyword-valued style properties.
//!
//! One enum per CSS property whose value comes from a fixed set of
//! keywords, plus `SpacingRect` for the four-sided shorthands.

/// Text decoration line types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextDecoration {
    /// No decoration
    None,
    /// Underline
    Underline,
    /// Line through the middle of the text
    LineThrough,
}

/// CSS font-style
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontStyle {
    /// Upright
    Normal,
    /// Slanted
    Italic,
}

impl FontStyle {
    /// Whether this style renders italic
    pub fn is_italic(self) -> bool {
        matches!(self, Self::Italic)
    }
}

/// CSS text-overflow behavior
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextOverflow {
    /// Clip overflowing text (default)
    Clip,
    /// Show ellipsis (...) when text overflows
    Ellipsis,
}

/// CSS white-space property
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WhiteSpace {
    /// Normal whitespace handling (collapse + wrap)
    Normal,
    /// No wrapping (single line)
    Nowrap,
    /// Preserve whitespace and newlines (no wrap)
    Pre,
    /// Preserve whitespace but allow wrapping
    PreWrap,
}

/// CSS scrollbar-width values
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScrollbarWidth {
    /// Default scrollbar width
    Auto,
    /// Thin scrollbar
    Thin,
    /// Hidden scrollbar (no space taken)
    None,
}

// ============================================================================
// Layout Style Types
// ============================================================================

/// Spacing values for padding and margin (all in pixels)
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SpacingRect {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl SpacingRect {
    /// All sides equal
    pub fn uniform(px: f32) -> Self {
        Self {
            top: px,
            right: px,
            bottom: px,
            left: px,
        }
    }

    /// Horizontal and vertical
    pub fn xy(x: f32, y: f32) -> Self {
        Self {
            top: y,
            right: x,
            bottom: y,
            left: x,
        }
    }

    /// Individual sides
    pub fn new(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }
}

/// Flex direction
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StyleFlexDirection {
    Row,
    Column,
    RowReverse,
    ColumnReverse,
}

/// Display mode
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StyleDisplay {
    Flex,
    Block,
    None,
}

/// Alignment for align-items and align-self
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StyleAlign {
    Start,
    Center,
    End,
    Stretch,
    Baseline,
}

/// Justify content values
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StyleJustify {
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

/// Overflow behavior
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StyleOverflow {
    Visible,
    Clip,
    Scroll,
}

/// CSS visibility property
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StyleVisibility {
    Visible,
    Hidden,
}

/// CSS position property
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StylePosition {
    Static,
    Relative,
    Absolute,
    Fixed,
    Sticky,
}

/// CSS dimension value (length, auto, or keyword)
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StyleDimension {
    /// Fixed length in pixels
    Length(f32),
    /// Percentage of parent (0.0-1.0)
    Percent(f32),
    /// Auto sizing (shrink to content)
    Auto,
}
