//! CSS-like units for layout dimensions
//!
//! Provides semantic unit types that make layout code clearer and prevent
//! confusion between raw pixels and scaled units.
//!
//! # Examples
//!
//! ```rust,ignore
//! use blinc_layout::units::{px, sp, pct, Length};
//!
//! // Raw pixels
//! div().padding(px(16.0))
//!
//! // Spacing units (4px grid - for consistent spacing)
//! div().padding(sp(4.0))  // 4 * 4 = 16px
//!
//! // Percentage
//! div().w(pct(50.0))  // 50% of parent
//! ```

use taffy::{LengthPercentage, LengthPercentageAuto};

/// A length value with its unit
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Length {
    /// Raw pixels (no scaling)
    Px(f32),
    /// Spacing units (multiplied by 4.0 for a 4px grid system)
    Sp(f32),
    /// Percentage of parent dimension
    Pct(f32),
    /// Auto sizing
    Auto,
}

impl Length {
    /// Convert to raw pixels
    pub fn to_px(self) -> f32 {
        match self {
            Length::Px(v) => v,
            Length::Sp(v) => v * 4.0,
            Length::Pct(_) => 0.0, // Percentage needs context
            Length::Auto => 0.0,
        }
    }

    /// Check if this is a percentage
    pub fn is_percentage(&self) -> bool {
        matches!(self, Length::Pct(_))
    }

    /// Check if this is auto
    pub fn is_auto(&self) -> bool {
        matches!(self, Length::Auto)
    }
}

impl Default for Length {
    fn default() -> Self {
        Length::Px(0.0)
    }
}

// Conversion to Taffy types
impl From<Length> for LengthPercentage {
    fn from(len: Length) -> Self {
        match len {
            Length::Px(v) => LengthPercentage::Length(v),
            Length::Sp(v) => LengthPercentage::Length(v * 4.0),
            Length::Pct(v) => LengthPercentage::Percent(v / 100.0),
            Length::Auto => LengthPercentage::Length(0.0),
        }
    }
}

impl From<Length> for LengthPercentageAuto {
    fn from(len: Length) -> Self {
        match len {
            Length::Px(v) => LengthPercentageAuto::Length(v),
            Length::Sp(v) => LengthPercentageAuto::Length(v * 4.0),
            Length::Pct(v) => LengthPercentageAuto::Percent(v / 100.0),
            Length::Auto => LengthPercentageAuto::Auto,
        }
    }
}

// Convenience constructors
/// Create a pixel length value
#[inline]
pub const fn px(value: f32) -> Length {
    Length::Px(value)
}

/// Create a spacing unit length value (4px grid)
///
/// Common usage:
/// - `sp(1)` = 4px
/// - `sp(2)` = 8px
/// - `sp(4)` = 16px
/// - `sp(8)` = 32px
#[inline]
pub const fn sp(units: f32) -> Length {
    Length::Sp(units)
}

/// Create a percentage length value
#[inline]
pub const fn pct(value: f32) -> Length {
    Length::Pct(value)
}

/// Tuple conversion for ergonomic unit specification
/// Allows: `(16.0, Px)` or `(4.0, Sp)` syntax
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Unit {
    /// Raw pixels
    Px,
    /// Spacing units (4px grid)
    Sp,
    /// Percentage
    Pct,
}

impl From<(f32, Unit)> for Length {
    fn from((value, unit): (f32, Unit)) -> Self {
        match unit {
            Unit::Px => Length::Px(value),
            Unit::Sp => Length::Sp(value),
            Unit::Pct => Length::Pct(value),
        }
    }
}

// Allow raw f32 to be used as pixels for backwards compatibility
impl From<f32> for Length {
    fn from(value: f32) -> Self {
        Length::Px(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_px_conversion() {
        assert_eq!(px(16.0).to_px(), 16.0);
    }

    #[test]
    fn test_sp_conversion() {
        assert_eq!(sp(4.0).to_px(), 16.0); // 4 * 4 = 16
        assert_eq!(sp(1.0).to_px(), 4.0);
    }

    #[test]
    fn test_tuple_syntax() {
        let len: Length = (16.0, Unit::Px).into();
        assert_eq!(len.to_px(), 16.0);

        let len: Length = (4.0, Unit::Sp).into();
        assert_eq!(len.to_px(), 16.0);
    }

    #[test]
    fn test_taffy_conversion() {
        let lp: LengthPercentage = px(16.0).into();
        assert!(matches!(lp, LengthPercentage::Length(v) if (v - 16.0).abs() < 0.001));

        let lp: LengthPercentage = sp(4.0).into();
        assert!(matches!(lp, LengthPercentage::Length(v) if (v - 16.0).abs() < 0.001));

        let lp: LengthPercentage = pct(50.0).into();
        assert!(matches!(lp, LengthPercentage::Percent(v) if (v - 0.5).abs() < 0.001));
    }
}
