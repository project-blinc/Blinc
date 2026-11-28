//! A number field steps with the arrow keys and takes the characters of a number.

use std::sync::{Arc, Mutex};

use blinc_core::events::event_types;
use blinc_layout::div::ElementBuilder;
use blinc_layout::event_handler::EventContext;
use blinc_layout::renderer::RenderTree;
use blinc_layout::widgets::number_input::number_input;

const UP: u32 = 38;
const DOWN: u32 = 40;

#[test]
fn arrows_step_within_the_bounds_and_a_minus_is_typed() {
    blinc_theme::ThemeState::init(
        blinc_theme::HybridTheme::bundle(),
        blinc_theme::ColorScheme::Dark,
    );
    blinc_core::BlincContextState::init(
        blinc_core::reactive::global_graph(),
        Arc::new(Mutex::new(blinc_core::context_state::HookState::new())),
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );
    let value = blinc_core::reactive::signal(0.0f64);
    let heard = Arc::new(Mutex::new(Vec::new()));
    let record = Arc::clone(&heard);
    let input = number_input(value)
        .min(-10.0)
        .max(2.0)
        .step(2.0)
        .precision(0)
        .on_change(move |v| record.lock().unwrap().push(v));
    let data = input.data();
    let mut tree = RenderTree::from_element(&input);
    tree.compute_layout(300.0, 60.0);
    let root = tree.root().unwrap();
    let handlers = ElementBuilder::event_handlers(&input).unwrap();
    // Focused through a press, as a user does.
    handlers.dispatch(
        &EventContext::new(event_types::POINTER_DOWN, root)
            .with_bounds(200.0, 36.0)
            .with_mouse_pos(10.0, 10.0),
    );
    let key = |code| {
        handlers.dispatch(&EventContext::new(event_types::KEY_DOWN, root).with_key_code(code));
    };
    let shown = || data.lock().unwrap().value.clone();

    key(UP);
    assert_eq!((value.try_get(), shown()), (Some(2.0), "2".to_string()));
    key(UP);
    key(DOWN);
    assert_eq!((value.try_get(), shown()), (Some(0.0), "0".to_string()));
    assert_eq!(
        *heard.lock().unwrap(),
        [2.0, 0.0],
        "a step at a bound is not a change"
    );

    handlers.dispatch(
        &EventContext::new(event_types::KEY_DOWN, root)
            .with_key_code(65)
            .with_modifiers(false, true, false, false),
    );
    for c in "-5".chars() {
        handlers.dispatch(&EventContext::new(event_types::TEXT_INPUT, root).with_key_char(c));
    }
    assert_eq!((value.try_get(), shown()), (Some(-5.0), "-5".to_string()));
}
