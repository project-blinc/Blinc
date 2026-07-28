//! `signal <name>: <T> = <literal>` seeds the signal.
use blinc_dsl_core::BlincDsl;

fn dsl() -> BlincDsl {
    BlincDsl::new().expect("dsl init")
}

/// The process-global signal registry means two tests declaring the
/// same name would share it; each test uses its own names, and the
/// compile order still matters for the "first sight" rule, so they
/// take turns.
fn lock() -> std::sync::MutexGuard<'static, ()> {
    static L: std::sync::Mutex<()> = std::sync::Mutex::new(());
    L.lock().unwrap_or_else(|e| e.into_inner())
}

#[test]
fn every_type_takes_a_default() {
    let _g = lock();
    let dsl = dsl();
    dsl.compile_source(
        r#"
        signal d_name: string = "Change this value"
        signal d_count: i32 = 7
        signal d_ratio: f64 = 0.25
        signal d_on: bool = true
        view { Div { } }
        "#,
        "defaults.blinc",
    )
    .expect("compile");

    assert_eq!(
        dsl.get_signal_string("d_name").as_deref(),
        Some("Change this value")
    );
    assert_eq!(dsl.get_signal_i32("d_count"), Some(7));
    assert_eq!(dsl.get_signal_f64("d_ratio"), Some(0.25));
    assert_eq!(dsl.get_signal_bool("d_on"), Some(true));
}

/// A bare `signal x: T` still declares an undefaulted signal.
#[test]
fn a_bare_declaration_still_works() {
    let _g = lock();
    let dsl = dsl();
    dsl.compile_source(
        r#"
        signal d_plain: i32
        view { Div { } }
        "#,
        "plain.blinc",
    )
    .expect("compile");
    assert_eq!(dsl.get_signal_i32("d_plain"), Some(0), "defaults to zero");
}

/// `signal ratio: f64 = 1` — an integer literal widens, since that is
/// the natural way to write a whole number.
#[test]
fn an_integer_literal_widens_to_f64() {
    let _g = lock();
    let dsl = dsl();
    dsl.compile_source(
        r#"
        signal d_whole: f64 = 3
        view { Div { } }
        "#,
        "widen.blinc",
    )
    .expect("compile");
    assert_eq!(dsl.get_signal_f64("d_whole"), Some(3.0));
}

/// The default applies when the signal is MINTED, not on every
/// compile: a recompile must not throw away what the user has typed.
#[test]
fn a_recompile_does_not_reset_the_value() {
    let _g = lock();
    let src = r#"
        signal d_sticky: string = "initial"
        view { Div { } }
    "#;
    let first = dsl();
    first.compile_source(src, "sticky.blinc").expect("compile");
    assert_eq!(
        first.get_signal_string("d_sticky").as_deref(),
        Some("initial")
    );

    first.set_signal_string("d_sticky", "typed by the user");

    let second = dsl();
    second
        .compile_source(src, "sticky.blinc")
        .expect("recompile");
    assert_eq!(
        second.get_signal_string("d_sticky").as_deref(),
        Some("typed by the user"),
        "the declared default must not clobber a live value"
    );
}

/// Editing the default in source must reach the running app, while a
/// reload of UNCHANGED source must leave a live value alone. Both are
/// the same code path, told apart by whether the declaration changed.
#[test]
fn an_edited_default_applies_but_an_unchanged_one_does_not() {
    let _g = lock();
    let src = |v: &str| {
        format!(
            r#"signal d_edit: string = "{v}"
                                   view {{ Div {{ }} }}"#
        )
    };

    let a = dsl();
    a.compile_source(&src("first"), "edit.blinc")
        .expect("compile");
    assert_eq!(a.get_signal_string("d_edit").as_deref(), Some("first"));

    // The user types something.
    a.set_signal_string("d_edit", "typed");

    // A reload of the SAME source keeps it.
    let b = dsl();
    b.compile_source(&src("first"), "edit.blinc")
        .expect("reload");
    assert_eq!(
        b.get_signal_string("d_edit").as_deref(),
        Some("typed"),
        "an unchanged default must not clobber a live value"
    );

    // Editing the default is an authoring action, and wins.
    let c = dsl();
    c.compile_source(&src("second"), "edit.blinc")
        .expect("reload");
    assert_eq!(
        c.get_signal_string("d_edit").as_deref(),
        Some("second"),
        "an edited default must reach the running app"
    );
}
