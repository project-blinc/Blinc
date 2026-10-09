//! Signals that follow a node's pointer and focus state.
//!
//! A widget that restyles on hover or press reads these in a bound property
//! (`bg`, `border_color`, `opacity`, ...) instead of rebuilding inside a
//! `Stateful`, so a change patches the node in place.

use blinc_core::State;
use blinc_core::events::event_types::{
    BLUR, FOCUS, POINTER_DOWN, POINTER_ENTER, POINTER_LEAVE, POINTER_UP,
};

use crate::div::Div;

/// Hovered, pressed and focused as signals.
///
/// Hovered covers the node and everything under it, as the pointer events do.
/// Pressed follows the same transitions as `ButtonState`: a down sets it (and
/// hover, for touch, which never enters first), an up or a leave clears it.
#[derive(Clone)]
pub struct Interaction {
    hovered: State<bool>,
    pressed: State<bool>,
    focused: State<bool>,
}

impl Interaction {
    /// Over signals the caller owns.
    pub fn new(hovered: State<bool>, pressed: State<bool>, focused: State<bool>) -> Self {
        Self {
            hovered,
            pressed,
            focused,
        }
    }

    /// Persistent signals named by `key`, so a rebuild keeps them.
    pub fn keyed(key: &str) -> Self {
        use blinc_core::context_state::use_state_keyed;
        Self::new(
            use_state_keyed(&format!("{key}:hovered"), || false),
            use_state_keyed(&format!("{key}:pressed"), || false),
            use_state_keyed(&format!("{key}:focused"), || false),
        )
    }

    pub fn hovered(&self) -> State<bool> {
        self.hovered.clone()
    }

    pub fn pressed(&self) -> State<bool> {
        self.pressed.clone()
    }

    pub fn focused(&self) -> State<bool> {
        self.focused.clone()
    }
}

/// Write only a change, so a repeated event wakes no dependent.
fn put(state: &State<bool>, value: bool) {
    if state.get() != value {
        state.set(value);
    }
}

impl Div {
    /// Feed `interaction` from this element's pointer and focus events.
    pub fn track(self, interaction: &Interaction) -> Self {
        let (h, p, f) = (
            interaction.hovered.clone(),
            interaction.pressed.clone(),
            interaction.focused.clone(),
        );
        let enter = h.clone();
        let (leave_h, leave_p) = (h.clone(), p.clone());
        let (down_h, down_p) = (h, p.clone());
        let focus = f.clone();
        self.on_event(POINTER_ENTER, move |_| put(&enter, true))
            .on_event(POINTER_LEAVE, move |_| {
                put(&leave_h, false);
                put(&leave_p, false);
            })
            .on_event(POINTER_DOWN, move |_| {
                put(&down_h, true);
                put(&down_p, true);
            })
            .on_event(POINTER_UP, move |_| put(&p, false))
            .on_event(FOCUS, move |_| put(&focus, true))
            .on_event(BLUR, move |_| put(&f, false))
    }
}
