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
