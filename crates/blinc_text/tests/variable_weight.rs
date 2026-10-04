//! A variable face must honour the weight it was asked for.
//!
//! A variable file carries every weight in one face, so selecting by
//! weight returns the same file each time. Without setting the `wght`
//! axis, every weight shapes and rasterizes at the file's default
//! instance, and headings draw as regular. That is why Blinc could not
//! adopt SF Pro, whose only macOS face is the variable SFNS.ttf.

use blinc_text::registry::{FontRegistry, GenericFont};

/// The widest face available on this machine that actually has a wght
/// axis. Skips loudly rather than passing when none is installed.
fn variable_face(reg: &mut FontRegistry) -> Option<std::sync::Arc<blinc_text::FontFace>> {
    for generic in [
        GenericFont::System,
        GenericFont::SansSerif,
        GenericFont::Serif,
        GenericFont::Monospace,
    ] {
        if let Ok(f) = reg.load_generic_with_style(generic, 400, false) {
            if f.has_weight_axis() {
                return Some(f);
            }
        }
    }
    eprintln!("SKIP: no variable face with a wght axis installed");
    None
}

/// The axis value follows the requested weight, and a static face is
/// left alone.
#[test]
fn a_variable_face_takes_the_requested_weight() {
    let mut reg = FontRegistry::new();
    if variable_face(&mut reg).is_none() {
        return;
    }

    for generic in [GenericFont::System, GenericFont::SansSerif] {
        let Ok(light) = reg.load_generic_with_style(generic, 300, false) else {
            continue;
        };
        let Ok(black) = reg.load_generic_with_style(generic, 900, false) else {
            continue;
        };
        if !light.has_weight_axis() {
            // Static family: the registry picks a file per weight and
            // there is no axis to move.
            assert_eq!(light.variation_weight(), None);
            continue;
        }
        assert_eq!(light.variation_weight(), Some(300.0));
        assert_eq!(black.variation_weight(), Some(900.0));
    }
}

/// The payoff: a heavier weight must actually shape wider on a variable
/// face. Without the axis both weights shape at the default instance
/// and the advances are identical, which is the bug.
#[test]
fn a_heavier_weight_shapes_wider() {
    let mut reg = FontRegistry::new();
    if variable_face(&mut reg).is_none() {
        return;
    }

    let shaper = blinc_text::TextShaper::new();
    for generic in [GenericFont::System, GenericFont::SansSerif] {
        let (Ok(light), Ok(black)) = (
            reg.load_generic_with_style(generic, 200, false),
            reg.load_generic_with_style(generic, 900, false),
        ) else {
            continue;
        };
        if !light.has_weight_axis() {
            continue;
        }

        let text = "Handgloves";
        // Advances are unscaled font units.
        let w_light: i64 = shaper
            .shape(text, &light, 48.0)
            .glyphs
            .iter()
            .map(|g| g.x_advance as i64)
            .sum();
        let w_black: i64 = shaper
            .shape(text, &black, 48.0)
            .glyphs
            .iter()
            .map(|g| g.x_advance as i64)
            .sum();

        assert!(
            w_black > w_light,
            "{generic:?}: weight 900 shaped {w_black} vs 200 at {w_light}; \
             the wght axis is not reaching the shaper"
        );
        return;
    }
    eprintln!("SKIP: no variable face among the generics tried");
}

/// Verified against the actual variable file this feature exists for.
///
/// The generics above resolve to a static family on a stock macOS, so
/// those tests skip until `GenericFont::System` points at SF Pro. This
/// one loads SFNS.ttf directly so the axis is exercised now.
#[test]
fn sf_pro_shapes_each_weight_differently() {
    const SFNS: &str = "/System/Library/Fonts/SFNS.ttf";
    let Ok(data) = std::fs::read(SFNS) else {
        eprintln!("SKIP: {SFNS} not present");
        return;
    };

    let mut light = blinc_text::FontFace::from_data(data.clone()).expect("SFNS parses");
    let mut black = blinc_text::FontFace::from_data(data).expect("SFNS parses");
    assert!(
        light.has_weight_axis(),
        "SFNS.ttf should carry a wght axis; without one this feature is untested"
    );

    light.set_variation_weight(200);
    black.set_variation_weight(900);
    assert_eq!(light.variation_weight(), Some(200.0));
    assert_eq!(black.variation_weight(), Some(900.0));

    let shaper = blinc_text::TextShaper::new();
    let text = "Handgloves";
    let sum = |f: &blinc_text::FontFace| -> i64 {
        shaper
            .shape(text, f, 48.0)
            .glyphs
            .iter()
            .map(|g| g.x_advance as i64)
            .sum()
    };

    let (w_light, w_black) = (sum(&light), sum(&black));
    eprintln!("SF Pro advances: weight 200 -> {w_light}, weight 900 -> {w_black}");
    assert!(
        w_black > w_light,
        "weight 900 shaped no wider than 200 ({w_black} vs {w_light}); \
         the wght axis is not reaching the shaper"
    );
}
