//! A signal belongs to the module that declared it, unless exported.
//!
//! The collision this removes was real and silent: the playground's
//! `main.blinc` owned `page` for which tab the sidebar showed, a demo in
//! `navigation.blinc` declared `signal page` for a page number, and
//! clicking a page navigated the app. The second declaration found the
//! first and adopted it, and behaviour was the only report.
//!
//! Only the REGISTRY key is qualified. References inside a module still
//! say `page` and resolve through the pass's own map to a baked id, so
//! nothing in the source changes.
use blinc_dsl_core::signal_registry_key;

/// Two modules, one name, in a program that has a surface: two keys.
#[test]
fn the_same_name_in_two_modules_is_two_signals() {
    let surface = vec!["something_else".to_string()];
    let a = signal_registry_key("page", "main", &surface);
    let b = signal_registry_key("page", "navigation", &surface);
    assert_ne!(a, b, "two modules must not share one signal: {a} vs {b}");
}

/// A program that declares NO export is untouched: scoping is opt-in,
/// so a host driving a signal in an imported module keeps working.
#[test]
fn a_program_with_no_exports_is_not_qualified() {
    assert_eq!(signal_registry_key("page", "widgets", &[]), "page");
}

/// An exported signal keeps its bare name — that is what a host reaches
/// it by, and being reachable is the point of exporting.
#[test]
fn an_exported_signal_keeps_its_bare_name() {
    let exports = vec!["page".to_string()];
    assert_eq!(signal_registry_key("page", "navigation", &exports), "page");
}

/// Exporting is per name, not per module: a module's other signals stay
/// private beside an exported one.
#[test]
fn exporting_one_name_does_not_expose_the_rest() {
    let exports = vec!["page".to_string()];
    assert_eq!(signal_registry_key("page", "nav", &exports), "page");
    assert_ne!(
        signal_registry_key("draft", "nav", &exports),
        "draft",
        "declaring a surface makes everything outside it private",
    );
}

/// A single-file program has no namespace, so nothing is qualified and
/// every existing host call site keeps working.
#[test]
fn a_single_file_program_is_unqualified() {
    assert_eq!(signal_registry_key("page", "", &[]), "page");
}

/// Two modules both exporting one name DO share it, deliberately: that
/// is how a program says two files mean the same signal.
#[test]
fn two_modules_exporting_one_name_share_it() {
    let exports = vec!["theme".to_string()];
    assert_eq!(
        signal_registry_key("theme", "main", &exports),
        signal_registry_key("theme", "settings", &exports),
        "sharing is what an export is for, when both ask for it",
    );
}
