//! Layout style helpers

pub use taffy::prelude::*;

/// Helper to create common layout styles
pub struct LayoutStyle;

impl LayoutStyle {
    /// Create a flex container style
    pub fn flex_container() -> Style {
        Style {
            display: Display::Flex,
            ..Default::default()
        }
    }

    /// Create a flex row style
    pub fn flex_row() -> Style {
        Style {
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            ..Default::default()
        }
    }

    /// Create a flex column style
    pub fn flex_column() -> Style {
        Style {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            ..Default::default()
        }
    }

    /// Create a centered container
    pub fn centered() -> Style {
        Style {
            display: Display::Flex,
            justify_content: Some(JustifyContent::CENTER),
            align_items: Some(AlignItems::CENTER),
            ..Default::default()
        }
    }

    /// Create a fixed size style
    pub fn fixed_size(width: f32, height: f32) -> Style {
        Style {
            size: Size {
                width: Dimension::length(width),
                height: Dimension::length(height),
            },
            ..Default::default()
        }
    }
}
