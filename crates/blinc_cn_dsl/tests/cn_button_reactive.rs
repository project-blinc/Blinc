//! `cn.Button(disabled = …)` reactive-prop coverage.
//!
//! `disabled` is the first *structural* reactive prop to cross the DSL
//! boundary. Unlike `cn.Progress`'s `value`, it can't be patched as a
//! single `RenderProps` write: it picks the background, border, shadow,
//! text colour and the FSM's start state. The cn side therefore reads
//! the value at build time and registers a `deps()` subscription so the
//! subtree rebuilds on set.
//!
//! These cases pin the DSL half of that path: the macro's two-slot FFI,
//! the `lower_reactive_args` pass, and the `IntoReactive<bool>` bridge
//! from the runtime `Reactive<T>` onto the layout channel. A regression
//! in any link surfaces as a `compile_source` error rather than a
//! silent build-time snapshot.

use blinc_dsl_core::BlincDsl;

fn dsl() -> BlincDsl {
    let _ = tracing_subscriber::fmt::try_init();
    let dsl = BlincDsl::new().expect("dsl init");
    blinc_cn_dsl::register_all(&dsl).expect("register cn.* widgets");
    dsl
}

#[test]
fn cn_button_disabled_literal() {
    let src = r#"
        view {
            cn.Button(label = "Save", disabled = true)
        }
    "#;
    dsl()
        .compile_source(src, "cn_button_disabled_literal.blinc")
        .expect("compile cn.Button(disabled = literal)");
}

#[test]
fn cn_button_disabled_signal() {
    let src = r#"
        signal busy: bool
        view {
            cn.Button(label = "Save", disabled = busy)
        }
    "#;
    dsl()
        .compile_source(src, "cn_button_disabled_signal.blinc")
        .expect("compile cn.Button(disabled = signal)");
}

#[test]
fn cn_button_disabled_computed() {
    let src = r#"
        signal busy: bool
        view {
            cn.Button(label = "Next", disabled = computed { busy } : bool)
        }
    "#;
    dsl()
        .compile_source(src, "cn_button_disabled_computed.blinc")
        .expect("compile cn.Button(disabled = computed)");
}

/// Bare signal reads inside a `computed { }` body. Regression guard
/// for the missing `Variable` arm in `resolve_signal_calls`: a bare
/// `<signal>` (no `.get()`) was never lowered to
/// `__signal_get_by_id_<T>`, so SSA fell back to an undefined variable
/// or, when the name collided with a function, an extern ref that
/// failed to link. Verified at the HIR level: the body now emits
/// `call @__signal_get_by_id_i32(<id>)`.
#[test]
fn cn_button_disabled_computed_bare_signal_read() {
    let src = r#"
        signal n: i32
        view {
            cn.Button(label = "Next", disabled = computed { n > 3 } : bool)
        }
    "#;
    dsl()
        .compile_source(src, "cn_button_disabled_bare_read.blinc")
        .expect("compile cn.Button(disabled = computed over bare signal)");
}

/// The default must survive: omitting `disabled` leaves the button
/// enabled, and the prop's `Reactive::Literal(false)` default still
/// converts cleanly through the bridge.
#[test]
fn cn_button_disabled_omitted() {
    let src = r#"
        view {
            cn.Button(label = "Save")
        }
    "#;
    dsl()
        .compile_source(src, "cn_button_disabled_omitted.blinc")
        .expect("compile cn.Button without disabled");
}

/// `label` crosses live too. Text content has no property-binding
/// writer, so a bound label takes the same `deps()` rebuild path as
/// `disabled` rather than patching in place.
#[test]
fn cn_button_label_signal() {
    let src = r#"
        signal caption: string
        view {
            cn.Button(label = caption)
        }
    "#;
    dsl()
        .compile_source(src, "cn_button_label_signal.blinc")
        .expect("compile cn.Button(label = signal)");
}

#[test]
fn cn_button_label_and_disabled_both_bound() {
    let src = r#"
        signal caption: string
        signal busy: bool
        view {
            cn.Button(label = caption, disabled = busy)
        }
    "#;
    dsl()
        .compile_source(src, "cn_button_both_bound.blinc")
        .expect("compile cn.Button with both props bound");
}
