//! Subpixel horizontal glyph positioning.
//!
//! Rounding each glyph's pen position to a whole device pixel leaves
//! every gap between letters off by up to half a pixel, which is what
//! makes letter spacing look uneven at small sizes. Rasterizing a glyph
//! at one of a few horizontal sub-pixel offsets, and placing it on a
//! whole pixel, keeps the spacing even without blurring the glyph.
//!
//! The cost lands on the atlas: a glyph cached at N phases occupies N
//! entries. Callers opt in per text run and pick N, and the renderer
//! falls back to phase 0 rather than growing the atlas when it is under
//! pressure.

use std::num::NonZeroU8;

/// Horizontal subpixel positioning for one text run.
///
/// The two values are only meaningful together, so they travel as one:
/// `None` is whole-pixel positioning, exactly the behaviour without
/// this feature.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SubpixelX {
    /// How many horizontal offsets a glyph may be rasterized at. The
    /// phase count divides one pixel, so 3 gives thirds.
    pub phases: NonZeroU8,
    /// Where the text run's origin sits in device pixels. The fraction
    /// of this is what the first glyph's phase is measured from.
    pub origin_x: f32,
}

impl SubpixelX {
    /// `phases` offsets per pixel, for a run whose origin lands at
    /// `origin_x` device pixels.
    pub fn new(phases: NonZeroU8, origin_x: f32) -> Self {
        Self { phases, origin_x }
    }

    /// The whole device pixel the glyph is placed on, and the phase to
    /// rasterize it at.
    ///
    /// `pen_x` is the shaper's pen position, relative to the run origin.
    /// A phase that rounds up to `phases` carries into the next pixel
    /// rather than becoming an out-of-range phase.
    pub fn split(&self, pen_x: f32) -> (f32, u8) {
        let phases = self.phases.get() as f32;
        let pen = self.origin_x + pen_x;
        let whole = pen.floor();
        let phase = ((pen - whole) * phases).round();

        if phase >= phases {
            (whole + 1.0, 0)
        } else {
            (whole, phase as u8)
        }
    }

    /// The horizontal offset, as a fraction of a pixel, to rasterize
    /// `phase` at.
    pub fn offset(&self, phase: u8) -> f32 {
        phase as f32 / self.phases.get() as f32
    }

    /// Where the glyph sits relative to the run origin once the caller
    /// has snapped that origin to a whole pixel.
    ///
    /// A caller draws the run at `origin_x.floor()`; this returns the
    /// local x that puts the glyph on `whole`.
    pub fn local_x(&self, whole: f32) -> f32 {
        whole - self.origin_x.floor()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(phases: u8, origin_x: f32) -> SubpixelX {
        SubpixelX::new(NonZeroU8::new(phases).unwrap(), origin_x)
    }

    /// One phase is whole-pixel positioning: every pen position lands on
    /// phase 0, which is the un-offset raster.
    #[test]
    fn a_single_phase_is_always_phase_zero() {
        let s = sub(1, 0.0);
        for pen in [0.0, 0.1, 0.49, 0.5, 0.9, 1.0, 7.3] {
            let (_, phase) = s.split(pen);
            assert_eq!(phase, 0, "pen {pen}");
        }
    }

    #[test]
    fn thirds_pick_the_nearest_phase() {
        let s = sub(3, 0.0);
        assert_eq!(s.split(0.0), (0.0, 0));
        assert_eq!(s.split(0.33), (0.0, 1));
        assert_eq!(s.split(0.67), (0.0, 2));
        // 0.9 is nearer to the next whole pixel than to 2/3.
        assert_eq!(s.split(0.9), (1.0, 0));
    }

    /// A fraction that rounds up to `phases` must carry into the next
    /// pixel. Without the carry it would be phase 3 of 3, which no
    /// raster exists for.
    #[test]
    fn a_phase_that_rounds_up_carries() {
        for phases in [2u8, 3, 4] {
            let s = sub(phases, 0.0);
            let (whole, phase) = s.split(0.999);
            assert_eq!(whole, 1.0, "phases {phases}");
            assert_eq!(phase, 0, "phases {phases}");
        }
    }

    /// Every phase a split can produce is in range, so it always names
    /// a raster that exists.
    #[test]
    fn no_split_produces_an_out_of_range_phase() {
        for phases in [1u8, 2, 3, 4, 8] {
            let s = sub(phases, 0.0);
            for i in 0..2000 {
                let pen = i as f32 / 211.0;
                let (_, phase) = s.split(pen);
                assert!(phase < phases, "phases {phases}, pen {pen} -> {phase}");
            }
        }
    }

    /// The run origin's own fraction shifts every glyph's phase, which
    /// is the whole point: a run starting at x.5 is not the same as one
    /// starting at x.0.
    #[test]
    fn the_origin_fraction_shifts_the_phases() {
        let at_zero = sub(4, 10.0);
        let at_half = sub(4, 10.5);
        assert_eq!(at_zero.split(0.0).1, 0);
        assert_eq!(at_half.split(0.0).1, 2);
    }

    /// Placing the run at floor(origin) and the glyph at local_x must
    /// put the glyph on the whole pixel the split chose.
    #[test]
    fn local_x_lands_the_glyph_on_the_chosen_pixel() {
        let s = sub(3, 10.7);
        let (whole, _) = s.split(2.1);
        assert_eq!(s.origin_x.floor() + s.local_x(whole), whole);
    }

    #[test]
    fn the_offset_is_the_phase_as_a_fraction() {
        let s = sub(4, 0.0);
        assert_eq!(s.offset(0), 0.0);
        assert_eq!(s.offset(1), 0.25);
        assert_eq!(s.offset(2), 0.5);
        assert_eq!(s.offset(3), 0.75);
    }

    /// A negative origin still floors toward minus infinity, so the
    /// phase stays non-negative and in range.
    #[test]
    fn a_negative_origin_keeps_the_phase_in_range() {
        let s = sub(3, -4.25);
        let (whole, phase) = s.split(0.0);
        assert_eq!(whole, -5.0);
        assert!(phase < 3);
        assert_eq!(phase, 2);
    }
}
