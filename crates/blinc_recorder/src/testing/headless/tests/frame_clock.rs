use super::super::Headless;
use blinc_core::FrameClock;
use blinc_layout::div::div;
use blinc_theme::{ColorScheme, HybridTheme};
use std::time::Duration;

/// Each frame sets the clock the animation code reads, to the session's own time.
#[test]
fn a_frame_sets_the_frame_clock() {
    if !Headless::available() {
        return;
    }
    let mut ui = Headless::new(64, 64).expect("offscreen GPU");
    ui.theme(HybridTheme::bundle(), ColorScheme::Dark);
    ui.mount(|_| div().w(64.0).h(64.0)).settle();
    let frames = FrameClock::frame();
    ui.advance(100);
    assert!(FrameClock::frame() > frames, "no frame began the clock");
    assert!(FrameClock::now() >= Duration::from_millis(100));
    assert!(FrameClock::delta() <= Duration::from_millis(100));
}
