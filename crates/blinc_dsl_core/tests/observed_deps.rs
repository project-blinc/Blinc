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
//! The distinction that decides which happens is BINDING vs READ, and it
//! is easy to get backwards. `Text("{x}")` passes a handle: the widget
//! subscribes to the property itself and the JIT body never reads a
//! value, so a region containing only bindings observes nothing and
//! falls back — correctly, since there is nothing to observe. A body
//! that reads a VALUE, as control flow must, records at the
//! `__signal_get_by_id_*` choke point and gets an exact set.
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
             with {
               if obs_shown.get() > 0 { Text("yes") } else { Text("no") }
             }
           }"#,
        "observed_one.blinc",
    );
    build(&dsl);

    let deps = blinc_dsl_core::__deps_mentioning(id_of("obs_shown"))
        .expect("a region subscribed to the signal it read");
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
             with {
               if obs_a.get() + obs_b.get() > 0 { Text("yes") } else { Text("no") }
             }
           }"#,
        "observed_both.blinc",
    );
    build(&dsl);

    let deps = blinc_dsl_core::__deps_mentioning(id_of("obs_a"))
        .expect("a region subscribed to the first signal");
    assert!(
        deps.contains(&id_of("obs_b")),
        "and to the second it read: {deps:?}",
    );
}

/// A body of pure BINDINGS reads nothing, so it falls back to the
/// registered set rather than subscribing to nothing — which would leave
/// it permanently stale. `Text("{x}")` is the shape: the widget takes a
/// handle and the JIT body never performs a read.
#[test]
fn a_region_that_reads_nothing_falls_back() {
    let dsl = compile(
        r#"signal obs_bound: i32 = 1

           view {
             with { Text("{obs_bound}") }
           }"#,
        "observed_none.blinc",
    );
    build(&dsl);

    let deps = blinc_dsl_core::__deps_mentioning(id_of("obs_bound"))
        .expect("the fallback subscribed it to the registered set");
    assert!(!deps.is_empty(), "never subscribes to nothing: {deps:?}");
}

/// A bare `@stateful` view narrows to what it read, rather than staying
/// subscribed to every declared signal.
///
/// The dep list is fixed when the `Stateful` is built, before anything
/// has rendered, so the first build has nothing to go on and takes the
/// blanket fallback. The render records what it touched and the next
/// build uses that. This asserts the narrowing, and that the first build
/// still works from the fallback rather than subscribing to nothing.
#[test]
fn a_bare_stateful_view_narrows_after_its_first_render() {
    let dsl = compile(
        r#"signal sv_read: i32 = 1
           signal sv_unread: i32 = 9

           @stateful
           view {
             if sv_read.get() > 0 { Text("yes") } else { Text("no") }
           }"#,
        "stateful_narrow.blinc",
    );

    // First build: no render has happened, so the fallback applies and
    // the program subscribes to everything declared.
    build(&dsl);
    // Second build: the first render recorded its reads.
    build(&dsl);

    let deps = blinc_dsl_core::__deps_mentioning(id_of("sv_read"))
        .expect("the program stateful subscribed to the signal it read");
    assert!(
        !deps.contains(&id_of("sv_unread")),
        "narrowed away the signal it never read: {deps:?}",
    );
}
