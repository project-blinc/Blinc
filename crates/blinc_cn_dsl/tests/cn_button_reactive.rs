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

/// Known gap, pinned as `#[ignore]` rather than dropped: a computed
/// whose body reads a signal of a *different* type than its declared
/// return type fails to link with "can't resolve symbol <name>". The
/// accessor is picked from the computed's return type, so an `i64`
/// signal inside a `: bool` computed asks for a symbol that was never
/// emitted. Not specific to Button — `cn.Progress(value = computed {
/// count } : f64)` over an `i64` signal fails identically.
#[test]
#[ignore = "mixed-type computed: accessor chosen by return type, not signal type"]
fn cn_button_disabled_computed_mixed_types() {
    let src = r#"
        signal count: i64
        view {
            cn.Button(label = "Next", disabled = computed { count > 3 } : bool)
        }
    "#;
    dsl()
        .compile_source(src, "cn_button_disabled_mixed.blinc")
        .expect("compile cn.Button(disabled = mixed-type computed)");
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
