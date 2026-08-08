//! A `with` region subscribes to what it READ, not to what was declared.
//!
//! The registered set is a compile-time guess: every signal the region
//! could reach. The observed set is what the body actually touched while
//! rendering. Where they differ, the guess over-subscribes, and a region
//! re-renders for a write it does not depend on.
//!
//! These pin the difference. Signals are process-global by name, so each
//! test names its own.
//!
//! `a_region_subscribes_only_to_what_it_read` FAILS TODAY, on purpose:
//! it is the specification. `read_scope` and `mount` are both in place —
//! reads record at the `__signal_get_by_id_*` choke point, and `mount`
//! prefers an observed set over the registered one — but a `with` body's
//! reads are not reaching the scope, so `observed` comes back empty and
//! the fallback subscribes to every signal in scope. The measurement is
//! the point: three earlier versions of these tests asserted on rendered
//! values and passed while missing it entirely.
use blinc_dsl_core::BlincDsl;
use blinc_layout::div::ElementBuilder;

fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        blinc_core::BlincContextState::init(
            blinc_core::reactive::global_graph(),
            std::sync::Arc::new(std::sync::Mutex::new(
                blinc_core::context_state::HookState::new(),
            )),
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        );
    });
}

fn compile(src: &str, name: &str) -> BlincDsl {
    init();
    let dsl = BlincDsl::new().expect("runtime init");
    dsl.compile_source(src, name).expect("compile");
    dsl
}

/// Build once, which renders the region and closes its read scope.
fn build(dsl: &BlincDsl) {
    let widget = dsl.view_widget();
    let mut tree = blinc_layout::tree::LayoutTree::new();
    let _ = widget.build(&mut tree);
}

/// The id a named signal was minted under, for comparing against what
/// a region subscribed to.
fn id_of(name: &str) -> u64 {
    blinc_runtime::signal::lookup(name)
        .map(|(raw, _)| raw)
        .unwrap_or_else(|| panic!("{name} was never minted"))
}

/// A region reading one of two signals subscribes to exactly one.
///
/// This is the whole point of observing: the compile-time set cannot
/// tell which values a body actually touched, so it registers
/// everything in scope and the region wakes for writes it does not
/// depend on.
#[test]
fn a_region_subscribes_only_to_what_it_read() {
    let dsl = compile(
        r#"signal obs_shown: i32 = 1
           signal obs_ignored: i32 = 9

           view {
             with { Text("{obs_shown}") }
           }"#,
        "observed_one.blinc",
    );
    blinc_dsl_core::__clear_mounted_deps();
    build(&dsl);

    let (_region, deps) =
        blinc_dsl_core::__last_mounted_deps().expect("the region mounted a Stateful");
    assert!(
        deps.contains(&id_of("obs_shown")),
        "subscribed to the signal it rendered: {deps:?}",
    );
    assert!(
        !deps.contains(&id_of("obs_ignored")),
        "did NOT subscribe to the one it never read: {deps:?}",
    );
}

/// Reading two subscribes to both, so this is observation rather than
/// "the first read wins".
#[test]
fn a_region_subscribes_to_every_signal_it_read() {
    let dsl = compile(
        r#"signal obs_a: i32 = 1
           signal obs_b: i32 = 2

           view {
             with { Text("{obs_a} {obs_b}") }
           }"#,
        "observed_both.blinc",
    );
    blinc_dsl_core::__clear_mounted_deps();
    build(&dsl);

    let (_region, deps) =
        blinc_dsl_core::__last_mounted_deps().expect("the region mounted a Stateful");
    for name in ["obs_a", "obs_b"] {
        assert!(deps.contains(&id_of(name)), "{name} missing from {deps:?}");
    }
}

/// A region that reads nothing falls back to the registered set rather
/// than subscribing to nothing, which would leave it permanently stale.
#[test]
fn a_region_that_reads_nothing_falls_back() {
    let dsl = compile(
        r#"signal obs_unread: i32 = 1

           view {
             with { Text("static") }
           }"#,
        "observed_none.blinc",
    );
    blinc_dsl_core::__clear_mounted_deps();
    build(&dsl);

    let (_region, deps) =
        blinc_dsl_core::__last_mounted_deps().expect("the region mounted a Stateful");
    assert!(
        !deps.is_empty(),
        "a region with no observed reads must not subscribe to nothing: {deps:?}",
    );
}
