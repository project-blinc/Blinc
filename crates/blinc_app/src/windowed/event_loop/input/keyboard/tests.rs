use super::key_code;
use blinc_platform::Key;

#[test]
fn function_keys_and_insert_reach_shortcut_codes() {
    let function_keys = [
        Key::F1,
        Key::F2,
        Key::F3,
        Key::F4,
        Key::F5,
        Key::F6,
        Key::F7,
        Key::F8,
        Key::F9,
        Key::F10,
        Key::F11,
        Key::F12,
    ];
    for (key, expected) in function_keys.iter().zip(112..=123) {
        assert_eq!(key_code(key), expected, "{key:?}");
    }
    assert_eq!(key_code(&Key::Insert), 45);
    assert_eq!(key_code(&Key::ContextMenu), 93);
    assert_eq!(key_code(&Key::A), 65);
    assert_eq!(key_code(&Key::Num0), 48);
    assert_eq!(key_code(&Key::Equals), 187);
    assert_eq!(key_code(&Key::Unknown), 0);
}

#[test]
fn shortcut_handlers_run_before_text_insertion() {
    use super::{dispatch_keyboard_events, queue_key_press};
    use blinc_layout::div::div;
    use blinc_layout::widgets::text_input::{focus_text_input, text_input, text_input_state};
    use std::sync::{Arc, Mutex};

    let _ui = init_ui();
    let state = text_input_state();
    let observed = Arc::new(Mutex::new(Vec::new()));
    let (typed, seen) = (state.clone(), observed.clone());
    let root = div()
        .w(300.0)
        .h(80.0)
        .on_key_down(move |_| {
            seen.lock()
                .unwrap()
                .push(typed.lock().unwrap().value.clone())
        })
        .child(text_input(&state));
    let mut tree = blinc_layout::renderer::RenderTree::from_element(&root);
    tree.compute_layout(300.0, 80.0);
    focus_text_input(&state);
    let mut events = Vec::new();
    queue_key_press(
        &mut events,
        70,
        Some('f'),
        &blinc_platform::Modifiers {
            alt: true,
            ..Default::default()
        },
    );
    dispatch_keyboard_events(&mut tree, events);
    assert_eq!(*observed.lock().unwrap(), vec![String::new()]);
    assert_eq!(state.lock().unwrap().value, "f");
}

static UI: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn init_ui() -> std::sync::MutexGuard<'static, ()> {
    use std::sync::{Arc, Mutex, Once, atomic::AtomicBool};
    let guard = UI.lock().unwrap_or_else(|e| e.into_inner());
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        blinc_theme::ThemeState::init_default();
        blinc_core::BlincContextState::init(
            blinc_core::reactive::global_graph(),
            Arc::new(Mutex::new(blinc_core::context_state::HookState::new())),
            Arc::new(AtomicBool::new(false)),
        );
    });
    blinc_layout::widgets::text_input::blur_all_text_inputs();
    guard
}

#[test]
fn handled_alt_shortcut_cancels_only_its_text_and_still_reaches_other_handlers() {
    use super::{dispatch_keyboard_events, queue_key_press};
    use blinc_layout::div::div;
    use blinc_layout::widgets::text_input::{focus_text_input, text_input, text_input_state};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    let _ui = init_ui();
    let state = text_input_state();
    let seen = Arc::new(AtomicUsize::new(0));
    let observed = seen.clone();
    let root = div()
        .w(300.0)
        .h(80.0)
        .on_key_down(|ctx| {
            if ctx.alt && ctx.key_code == 70 {
                ctx.prevent_default();
            }
        })
        .child(div().on_key_down(move |_| {
            observed.fetch_add(1, Ordering::Relaxed);
        }))
        .child(text_input(&state));
    let mut tree = blinc_layout::renderer::RenderTree::from_element(&root);
    tree.compute_layout(300.0, 80.0);
    focus_text_input(&state);
    let mut events = Vec::new();
    queue_key_press(
        &mut events,
        70,
        Some('f'),
        &blinc_platform::Modifiers {
            alt: true,
            ..Default::default()
        },
    );
    dispatch_keyboard_events(&mut tree, events);
    assert_eq!(state.lock().unwrap().value, "");
    let mut events = Vec::new();
    queue_key_press(&mut events, 66, Some('b'), &Default::default());
    dispatch_keyboard_events(&mut tree, events);
    assert_eq!(state.lock().unwrap().value, "b");
    assert_eq!(seen.load(Ordering::Relaxed), 2);
}

#[test]
fn platform_text_and_option_characters_survive_unhandled_key_presses() {
    use super::{dispatch_keyboard_events, queue_key_press};
    use blinc_layout::{
        div::div,
        widgets::text_input::{focus_text_input, text_input, text_input_state},
    };
    use blinc_platform::{KeyState, KeyboardEvent, Modifiers};
    let _ui = init_ui();
    let state = text_input_state();
    let mut tree =
        blinc_layout::renderer::RenderTree::from_element(&div().child(text_input(&state)));
    tree.compute_layout(300.0, 80.0);
    focus_text_input(&state);
    for (key, text, modifiers) in [
        (
            Key::F,
            "ƒ",
            Modifiers {
                alt: true,
                ..Default::default()
            },
        ),
        (
            Key::Q,
            "@",
            Modifiers {
                alt: true,
                ..Default::default()
            },
        ),
        (
            Key::A,
            "Ä",
            Modifiers {
                shift: true,
                ..Default::default()
            },
        ),
        (Key::Char('漢'), "漢", Modifiers::default()),
    ] {
        let event = KeyboardEvent {
            key,
            text: Some(text.into()),
            modifiers,
            state: KeyState::Pressed,
        };
        let mut events = Vec::new();
        queue_key_press(
            &mut events,
            key_code(&event.key),
            super::key_char(&event),
            &event.modifiers,
        );
        dispatch_keyboard_events(&mut tree, events);
    }
    assert_eq!(state.lock().unwrap().value, "ƒ@Ä漢");
}

#[test]
fn ctrl_and_meta_shortcuts_do_not_generate_text() {
    use super::queue_key_press;
    use blinc_core::events::event_types;
    for modifiers in [
        blinc_platform::Modifiers {
            ctrl: true,
            ..Default::default()
        },
        blinc_platform::Modifiers {
            meta: true,
            ..Default::default()
        },
    ] {
        let mut events = Vec::new();
        queue_key_press(&mut events, 70, Some('f'), &modifiers);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, event_types::KEY_DOWN);
        assert_eq!(
            (events[0].ctrl, events[0].meta),
            (modifiers.ctrl, modifiers.meta)
        );
    }
}

#[test]
fn cancellation_does_not_discard_later_committed_text_in_an_area() {
    use super::{dispatch_keyboard_events, queue_key_press};
    use blinc_layout::{
        div::div,
        widgets::text_area::{focus_text_area, text_area, text_area_state},
    };
    let _ui = init_ui();
    let state = text_area_state();
    let root = div()
        .on_key_down(|ctx| {
            if ctx.alt {
                ctx.prevent_default();
            }
        })
        .child(text_area(&state));
    let mut tree = blinc_layout::renderer::RenderTree::from_element(&root);
    tree.compute_layout(300.0, 160.0);
    focus_text_area(&state);
    let mut events = Vec::new();
    queue_key_press(
        &mut events,
        70,
        Some('f'),
        &blinc_platform::Modifiers {
            alt: true,
            ..Default::default()
        },
    );
    dispatch_keyboard_events(&mut tree, events);
    assert_eq!(state.lock().unwrap().value(), "");
    let mut events = Vec::new();
    queue_key_press(&mut events, 0, Some('漢'), &Default::default());
    dispatch_keyboard_events(&mut tree, events);
    assert_eq!(state.lock().unwrap().value(), "漢");
}

fn assert_altgr_preserves_text(
    tree: &mut blinc_layout::renderer::RenderTree,
    value: impl Fn() -> String,
) {
    use super::{dispatch_keyboard_events, queue_key_press};
    use blinc_platform::Modifiers;
    let mut press = |code, text, modifiers| {
        let mut events = Vec::new();
        queue_key_press(&mut events, code, text, &modifiers);
        dispatch_keyboard_events(tree, events);
    };
    for ch in "keep".chars() {
        press(
            ch.to_ascii_uppercase() as u32,
            Some(ch),
            Modifiers::default(),
        );
    }
    press(
        65,
        Some('ą'),
        Modifiers {
            ctrl: true,
            alt: true,
            ..Default::default()
        },
    );
    assert_eq!(
        value(),
        "keepą",
        "AltGr must neither drop its text nor select all"
    );
    press(
        65,
        None,
        Modifiers {
            ctrl: true,
            ..Default::default()
        },
    );
    press(81, Some('q'), Modifiers::default());
    assert_eq!(value(), "q", "ordinary Ctrl+A must still select all");
}

#[test]
fn altgr_text_in_input_does_not_trigger_ctrl_shortcuts() {
    use blinc_layout::{
        div::div,
        renderer::RenderTree,
        widgets::text_input::{focus_text_input, text_input, text_input_state},
    };
    let _ui = init_ui();
    let state = text_input_state();
    let mut tree = RenderTree::from_element(&div().child(text_input(&state)));
    tree.compute_layout(300.0, 100.0);
    focus_text_input(&state);
    assert_altgr_preserves_text(&mut tree, || state.lock().unwrap().value.clone());
}

#[test]
fn altgr_text_in_area_does_not_trigger_ctrl_shortcuts() {
    use blinc_layout::{
        div::div,
        renderer::RenderTree,
        widgets::text_area::{focus_text_area, text_area, text_area_state},
    };
    let _ui = init_ui();
    let state = text_area_state();
    let mut tree = RenderTree::from_element(&div().child(text_area(&state)));
    tree.compute_layout(300.0, 100.0);
    focus_text_area(&state);
    assert_altgr_preserves_text(&mut tree, || state.lock().unwrap().value());
}

#[test]
fn altgr_text_in_code_does_not_trigger_ctrl_shortcuts() {
    use blinc_layout::{
        div::div,
        renderer::RenderTree,
        widgets::code::{code_editor, code_editor_state},
    };
    let _ui = init_ui();
    let state = code_editor_state("");
    let mut tree = RenderTree::from_element(&div().child(code_editor(&state)));
    tree.compute_layout(400.0, 200.0);
    state.lock().unwrap().focused = true;
    assert_altgr_preserves_text(&mut tree, || state.lock().unwrap().value());
}

#[test]
fn ctrl_alt_text_requires_printable_platform_output() {
    use blinc_platform::{KeyState, KeyboardEvent, Modifiers};
    let mut event = KeyboardEvent {
        key: Key::A,
        state: KeyState::Pressed,
        modifiers: Modifiers {
            ctrl: true,
            alt: true,
            ..Default::default()
        },
        text: None,
    };
    assert_eq!(super::key_char(&event), None);
    event.text = Some("ą".into());
    assert_eq!(super::key_char(&event), Some('ą'));
    event.text = Some("\u{1}".into());
    assert_eq!(super::key_char(&event), None);
}

#[test]
fn router_notifications_are_replaced_only_by_matching_key_broadcasts() {
    use super::{has_key_broadcast, queue_key_press};
    use blinc_core::events::event_types;
    let mut events = Vec::new();
    let modifiers = blinc_platform::Modifiers {
        shift: true,
        ..Default::default()
    };
    queue_key_press(&mut events, 0, None, &modifiers);
    assert!(events.is_empty());
    assert!(!has_key_broadcast(&events, event_types::KEY_DOWN));
    queue_key_press(&mut events, 112, None, &modifiers);
    assert!(has_key_broadcast(&events, event_types::KEY_DOWN));
    assert!(!has_key_broadcast(&events, event_types::KEY_UP));
    assert!(!has_key_broadcast(&events, event_types::POINTER_DOWN));
}
