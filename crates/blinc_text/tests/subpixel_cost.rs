//! What subpixel positioning costs, measured rather than assumed.
//!
//! Phasing a glyph multiplies its atlas entries by the phase count, so
//! these tests measure that directly and pin the behaviour that keeps it
//! bounded: a pressure limit, and a phase-0 fallback that gives up even
//! spacing instead of the frame.

use std::num::NonZeroU8;

use blinc_text::layout::LayoutOptions;
use blinc_text::registry::GenericFont;
use blinc_text::{SubpixelX, TextRenderer};

/// Every test needs a real face. Minimal CI images may have none, and a
/// silent pass would hide that, so skip loudly.
fn renderer() -> Option<TextRenderer> {
    let mut r = TextRenderer::new();
    let probe = r.prepare_text_with_style(
        "a",
        16.0,
        [1.0; 4],
        &LayoutOptions::default(),
        None,
        GenericFont::SansSerif,
        400,
        false,
    );
    match probe {
        Ok(_) => Some(r),
        Err(e) => {
            eprintln!("SKIP: no usable system font ({e:?})");
            None
        }
    }
}

fn draw(r: &mut TextRenderer, text: &str, subpixel: Option<SubpixelX>) -> blinc_text::PreparedText {
    r.prepare_text_subpixel(
        text,
        16.0,
        [1.0; 4],
        &LayoutOptions::default(),
        None,
        GenericFont::SansSerif,
        400,
        false,
        subpixel,
    )
    .expect("prepared")
}

fn sub(phases: u8, origin_x: f32) -> SubpixelX {
    SubpixelX::new(NonZeroU8::new(phases).unwrap(), origin_x)
}

const SAMPLE: &str = "the quick brown fox";

/// Passing `None` must cost exactly what it did before the feature
/// existed: no phased rasters, no fallbacks. This is the rollback path,
/// so it is the one that matters most.
#[test]
fn whole_pixel_positioning_creates_no_phased_rasters() {
    let Some(mut r) = renderer() else { return };

    for x in [0.0, 0.3, 0.5, 7.25] {
        let _ = draw(&mut r, SAMPLE, None);
        let _ = x;
    }

    let stats = r.subpixel_stats();
    assert_eq!(stats.phased_rasters, 0);
    assert_eq!(stats.phase0_fallbacks, 0);
}

/// One phase is whole-pixel positioning by definition, so opting in with
/// `phases: 1` must also cost nothing extra.
#[test]
fn a_single_phase_costs_nothing() {
    let Some(mut r) = renderer() else { return };

    for i in 0..8 {
        let _ = draw(&mut r, SAMPLE, Some(sub(1, i as f32 * 0.37)));
    }

    assert_eq!(r.subpixel_stats().phased_rasters, 0, "phases=1 phased a glyph");
}

/// The measurement this feature has to justify: how many more atlas
/// entries three phases cost over one, for the same text.
///
/// The bound is the point. Three phases can at worst triple the entries
/// for a glyph, so the cache must not exceed 3x the whole-pixel count,
/// and in practice lands well under it because many pen positions share
/// a phase.
#[test]
fn three_phases_cost_at_most_three_times_the_entries() {
    let Some(mut r) = renderer() else { return };

    // Baseline: the same text at whole-pixel positions.
    for i in 0..32 {
        let _ = draw(&mut r, SAMPLE, Some(sub(1, i as f32 * 0.31)));
    }
    let whole_pixel_entries = r.glyph_cache_len();
    assert!(whole_pixel_entries > 0, "nothing was cached");

    // Same text, same spread of origins, three phases.
    let mut phased = TextRenderer::new();
    for i in 0..32 {
        let _ = phased.prepare_text_subpixel(
            SAMPLE,
            16.0,
            [1.0; 4],
            &LayoutOptions::default(),
            None,
            GenericFont::SansSerif,
            400,
            false,
            Some(sub(3, i as f32 * 0.31)),
        );
    }
    let phased_entries = phased.glyph_cache_len();

    eprintln!(
        "atlas entries: {whole_pixel_entries} whole-pixel -> {phased_entries} at 3 phases \
         ({:.2}x), utilization {:.1}% -> {:.1}%, phased rasters {}",
        phased_entries as f32 / whole_pixel_entries as f32,
        r.atlas_utilization() * 100.0,
        phased.atlas_utilization() * 100.0,
        phased.subpixel_stats().phased_rasters,
    );

    assert!(
        phased_entries >= whole_pixel_entries,
        "phasing cannot reduce entries: {whole_pixel_entries} -> {phased_entries}"
    );
    assert!(
        phased_entries <= whole_pixel_entries * 3,
        "three phases should cost at most 3x: {whole_pixel_entries} -> {phased_entries}"
    );
    assert!(
        phased.subpixel_stats().phased_rasters > 0,
        "nothing was actually phased, so the comparison is meaningless"
    );
}

/// Glyphs must come back on whole pixels: that is what lets a caller
/// place the run at `floor(origin_x)` and get even spacing without
/// resampling the raster.
#[test]
fn phased_glyphs_land_on_whole_pixels() {
    let Some(mut r) = renderer() else { return };

    // A deliberately awkward origin fraction.
    let prepared = draw(&mut r, SAMPLE, Some(sub(3, 100.7)));
    assert!(!prepared.glyphs.is_empty(), "no glyphs to check");

    for g in &prepared.glyphs {
        let x = g.bounds[0];
        assert_eq!(
            x,
            x.round(),
            "glyph x {x} is not a whole pixel, so the caller cannot snap the origin"
        );
    }
}

/// Phasing must not move the run: the glyphs stay within a pixel of
/// where whole-pixel positioning put them. A regression here would show
/// up as text drifting when the feature is switched on.
#[test]
fn phasing_does_not_shift_the_run() {
    let Some(mut r) = renderer() else { return };

    let plain = draw(&mut r, SAMPLE, None);
    let phased = draw(&mut r, SAMPLE, Some(sub(3, 0.0)));
    assert_eq!(plain.glyphs.len(), phased.glyphs.len());

    for (a, b) in plain.glyphs.iter().zip(phased.glyphs.iter()) {
        let drift = (a.bounds[0] - b.bounds[0]).abs();
        assert!(drift <= 1.0, "glyph moved {drift}px");
    }
}

/// The fallback: once the atlas is past its pressure limit, no new
/// phased rasters are created. Spacing degrades to whole-pixel and the
/// fallback is counted, rather than the atlas growing toward its cap for
/// a cosmetic feature.
#[test]
fn atlas_pressure_falls_back_to_whole_pixel() {
    let Some(mut r) = renderer() else { return };

    // Fill the atlas with distinct sizes until it is under pressure.
    // Many sizes, each a fresh set of rasters.
    let mut size = 8.0_f32;
    while r.atlas_utilization() < 0.80 && size < 220.0 {
        let _ = r.prepare_text_with_style(
            "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789",
            size,
            [1.0; 4],
            &LayoutOptions::default(),
            None,
            GenericFont::SansSerif,
            400,
            false,
        );
        size += 0.5;
    }

    if r.atlas_utilization() < 0.80 {
        eprintln!(
            "SKIP: could not push the atlas past its pressure limit (at {:.1}%)",
            r.atlas_utilization() * 100.0
        );
        return;
    }

    r.reset_subpixel_stats();
    // Novel glyphs at a phase, which under pressure must not be phased.
    let prepared = draw(&mut r, "zyxwvu", Some(sub(3, 0.5)));

    assert!(
        !prepared.glyphs.is_empty(),
        "text still has to render under pressure"
    );
    let stats = r.subpixel_stats();
    eprintln!(
        "under pressure ({:.1}%): {} fallbacks, {} phased",
        r.atlas_utilization() * 100.0,
        stats.phase0_fallbacks,
        stats.phased_rasters
    );
    assert!(
        stats.phase0_fallbacks > 0,
        "pressure limit did not engage: {stats:?}"
    );
}
