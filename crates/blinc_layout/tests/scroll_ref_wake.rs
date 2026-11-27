//! A scroll command asks for a frame, so one sent while the window is idle is
//! applied without waiting for input.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use blinc_animation::AnimationScheduler;
use blinc_layout::selector::{ScrollBehavior, ScrollRef};

#[test]
fn each_scroll_command_wakes_the_window() {
    let woken = Arc::new(AtomicUsize::new(0));
    let mut scheduler = AnimationScheduler::new();
    let count = Arc::clone(&woken);
    scheduler.set_wake_callback(move || {
        count.fetch_add(1, Ordering::SeqCst);
    });
    blinc_layout::render_state::set_global_scheduler(scheduler.handle());

    let scroll = ScrollRef::new();
    let commands: [(&str, &dyn Fn()); 5] = [
        ("set_scroll_offset", &|| scroll.set_scroll_offset(0.0, 10.0)),
        ("scroll_by", &|| scroll.scroll_by(0.0, 10.0)),
        ("scroll_to", &|| scroll.scroll_to("row")),
        ("scroll_to_top", &|| scroll.scroll_to_top()),
        ("scroll_to_bottom", &|| {
            scroll.scroll_to_bottom_with_behavior(ScrollBehavior::Smooth)
        }),
    ];
    for (name, command) in commands {
        scheduler.take_needs_redraw();
        let before = woken.load(Ordering::SeqCst);
        command();
        assert!(woken.load(Ordering::SeqCst) > before, "{name} did not wake");
        assert!(scheduler.take_needs_redraw(), "{name} asked for no redraw");
    }
}
