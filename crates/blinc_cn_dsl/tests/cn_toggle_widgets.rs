//! `cn.Switch` / `cn.Checkbox` share their signal with the DSL.
//!
//! Both cn widgets take a `&State<bool>` and own the toggle. Binding a
//! DSL `signal` maps onto that same signal, so flipping the widget
//! writes the signal and setting the signal moves the widget -- one
//! source of truth rather than two copies drifting apart.

use blinc_dsl_core::BlincDsl;

fn dsl() -> BlincDsl {
    let _ = tracing_subscriber::fmt::try_init();
    let dsl = BlincDsl::new().expect("dsl init");
    blinc_cn_dsl::register_all(&dsl).expect("register cn.* widgets");
    dsl
}

#[test]
fn toggles_accept_a_bound_signal() {
    let src = r#"
        signal wifi: bool
        signal accepted: bool
        view {
            Div {
                cn.Switch(checked = wifi, label = "Wifi")
                cn.Checkbox(checked = accepted, label = "Accept")
            }
        }
    "#;
    dsl()
        .compile_source(src, "toggles_bound.blinc")
        .expect("compile bound toggles");
}

#[test]
fn toggles_accept_a_literal() {
    let src = r#"
        view {
            Div {
                cn.Switch(checked = true, size = "large")
                cn.Checkbox(checked = false, disabled = true)
            }
        }
    "#;
    dsl()
        .compile_source(src, "toggles_literal.blinc")
        .expect("compile literal toggles");
}

#[test]
fn kbd_renders_its_chord() {
    let src = r#"view { Div { cn.Kbd("Ctrl") cn.Kbd("K", size = "small") } }"#;
    dsl().compile_source(src, "kbd.blinc").expect("compile kbd");
}

/// The whole point of `cn.Input`'s `key`: typed text must survive a
/// rebuild.
///
/// `cn::input` keeps its contents in external state, and a DSL wrapper
/// is reconstructed every render -- so without a keyed store each
/// render would hand the widget a fresh, empty buffer and typing would
/// vanish. Two lookups with the same key must be the same buffer.
#[test]
fn keyed_text_state_survives_a_rebuild() {
    use blinc_cn_dsl::bridge::{text_area_state_keyed, text_input_data_keyed};

    let a = text_input_data_keyed("user");
    a.lock().unwrap().value = "typed".to_string();
    let b = text_input_data_keyed("user");
    assert_eq!(
        b.lock().unwrap().value,
        "typed",
        "the same key must resolve to the same buffer"
    );

    let other = text_input_data_keyed("email");
    assert_ne!(
        other.lock().unwrap().value,
        "typed",
        "a different key must be a different buffer"
    );

    // An empty key is explicitly detached: fresh buffer each call.
    let d1 = text_input_data_keyed("");
    d1.lock().unwrap().value = "scratch".to_string();
    assert!(
        text_input_data_keyed("").lock().unwrap().value.is_empty(),
        "an empty key must not share state"
    );

    // Same contract for the textarea store.
    let t1 = text_area_state_keyed("bio");
    let t2 = text_area_state_keyed("bio");
    assert!(
        std::sync::Arc::ptr_eq(&t1, &t2),
        "the same key must resolve to the same textarea state"
    );
}
