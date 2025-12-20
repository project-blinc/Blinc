//! Gradient fills - re-exported from blinc_core for unified type system

pub use blinc_core::{Gradient, GradientStop};

use crate::{Color, Point};

/// Create a simple linear gradient between two colors
pub fn linear_simple(start: Point, end: Point, from: Color, to: Color) -> Gradient {
    Gradient::Linear {
        start,
        end,
        stops: vec![
            GradientStop {
                offset: 0.0,
                color: from,
            },
            GradientStop {
                offset: 1.0,
                color: to,
            },
        ],
    }
}

/// Create a simple radial gradient between two colors
pub fn radial_simple(center: Point, radius: f32, from: Color, to: Color) -> Gradient {
    Gradient::Radial {
        center,
        radius,
        stops: vec![
            GradientStop {
                offset: 0.0,
                color: from,
            },
            GradientStop {
                offset: 1.0,
                color: to,
            },
        ],
    }
}
