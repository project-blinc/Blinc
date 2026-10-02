//! Measuring text with real fonts, with no renderer and no `blinc_app`.
//!
//! Run with the feature on:
//!
//! ```sh
//! cargo run -p blinc_layout --features text_measurer \
//!     --example standalone_text_measure
//! ```
//!
//! Without `init_text_measurer()` a `LayoutTree` measures through
//! `EstimatedTextMeasurer`, which bills every glyph the same and so cannot
//! tell `i` from `W`.

fn main() {
    let opts = blinc_layout::TextLayoutOptions::new();
    let measure = |s: &str| blinc_layout::measure_text_with_options(s, 32.0, &opts).width;

    println!("estimated (no measurer installed):");
    println!("  iiiiii {:>7.2}px", measure("iiiiii"));
    println!("  WWWWWW {:>7.2}px", measure("WWWWWW"));

    blinc_layout::init_text_measurer();

    println!("font-backed (after init_text_measurer):");
    println!("  iiiiii {:>7.2}px", measure("iiiiii"));
    println!("  WWWWWW {:>7.2}px", measure("WWWWWW"));
}
