//! Text alignment and font weight.
//!
//! These sit below the widget layer because a stylesheet resolves
//! `text-align` and `font-weight` before there is any element to carry them.

/// Text alignment options
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextAlign {
    /// Align text to the left (default)
    #[default]
    Left,
    /// Center text
    Center,
    /// Align text to the right
    Right,
}

/// Font weight options
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FontWeight {
    /// Thin (100)
    Thin,
    /// Extra Light (200)
    ExtraLight,
    /// Light (300)
    Light,
    /// Normal/Regular (400)
    #[default]
    Normal,
    /// Medium (500)
    Medium,
    /// Semi Bold (600)
    SemiBold,
    /// Bold (700)
    Bold,
    /// Extra Bold (800)
    ExtraBold,
    /// Black (900)
    Black,
}

impl FontWeight {
    /// Get the numeric weight value (100-900)
    pub fn weight(&self) -> u16 {
        match self {
            FontWeight::Thin => 100,
            FontWeight::ExtraLight => 200,
            FontWeight::Light => 300,
            FontWeight::Normal => 400,
            FontWeight::Medium => 500,
            FontWeight::SemiBold => 600,
            FontWeight::Bold => 700,
            FontWeight::ExtraBold => 800,
            FontWeight::Black => 900,
        }
    }
}
