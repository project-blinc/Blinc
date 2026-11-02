//! The palette charts draw series in, one per scheme.
//!
//! Exported as `--chart-1` to `--chart-5` and `--chart-baseline` (the colour
//! a comparison's baseline takes). A stylesheet that sets those variables
//! restyles every chart.

use crate::ColorScheme;

/// The chart variables for `scheme`, as `(name, value)` without the `--`.
pub fn chart_palette(scheme: ColorScheme) -> [(&'static str, &'static str); 6] {
    match scheme {
        ColorScheme::Light => [
            ("chart-1", "#2563eb"),
            ("chart-2", "#14b8a6"),
            ("chart-3", "#f59e0b"),
            ("chart-4", "#8b5cf6"),
            ("chart-5", "#ec4899"),
            ("chart-baseline", "#cbd5e1"),
        ],
        ColorScheme::Dark => [
            ("chart-1", "#3b82f6"),
            ("chart-2", "#2dd4bf"),
            ("chart-3", "#fbbf24"),
            ("chart-4", "#a78bfa"),
            ("chart-5", "#f472b6"),
            ("chart-baseline", "#475569"),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_scheme_has_its_own_palette() {
        let (light, dark) = (
            chart_palette(ColorScheme::Light),
            chart_palette(ColorScheme::Dark),
        );
        for ((name, l), (other, d)) in light.iter().zip(dark.iter()) {
            assert_eq!(name, other);
            assert_ne!(l, d, "--{name} is the same in both schemes");
        }
    }
}
