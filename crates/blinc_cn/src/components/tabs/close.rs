//! A closable tab's close button: an icon at the end of its trigger, shown on the active trigger
//! and while another is hovered (`.cn-tabs-close`). Pressing it reports the tab's value and does not select the tab.

use std::sync::{Arc, Mutex};

use blinc_layout::prelude::*;

/// Called with a tab's value when its close button is pressed.
pub(super) type OnClose = Arc<dyn Fn(&str) + Send + Sync>;

/// Where the close button was last pressed. A click bubbles from the button to its trigger, so
/// the trigger's click handler asks [`Press::took`] before it selects.
#[derive(Clone, Default)]
pub(super) struct Press(Arc<Mutex<Option<(f32, f32)>>>);

impl Press {
    /// How far a release may land from its press and still be the same click.
    const SLOP: f32 = 4.0;

    /// Whether the click released at `(x, y)` is the close button's, forgetting the press.
    pub(super) fn took(&self, x: f32, y: f32) -> bool {
        self.0
            .lock()
            .unwrap()
            .take()
            .is_some_and(|(px, py)| (x - px).abs() <= Self::SLOP && (y - py).abs() <= Self::SLOP)
    }
}

/// The close button of the trigger `{trigger_key}:{value}`, and its press.
pub(super) fn button(trigger_key: &str, value: &str, on_close: OnClose) -> (Div, Press) {
    let value = value.to_string();
    let press = Press::default();
    let pressed = press.clone();
    let button = div()
        .id(format!("{trigger_key}:{value}:close"))
        .class("cn-tabs-close")
        .flex_row()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .child(crate::components::icon::icon(blinc_icons::icons::X).size_px(12.0))
        .on_mouse_down(move |ctx| *pressed.0.lock().unwrap() = Some((ctx.mouse_x, ctx.mouse_y)))
        .on_click(move |_| on_close(&value));
    (button, press)
}
