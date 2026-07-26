//! Every exposed `cn.*` widget must actually RENDER, not merely compile.
//!
//! A widget whose lowering silently drops it still compiles cleanly — the
//! call just disappears and the parent renders childless. These tests build
//! the layout tree and count nodes, so a drop fails the assertion instead of
//! passing as a green compile.
use blinc_dsl_core::BlincDsl;
use blinc_layout::tree::LayoutTree;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

/// cn widgets read the theme and the process-global reactive context at
/// build time; both are app-startup singletons in a real app.
///
/// Tests in one binary share a process, and cargo runs them in
/// parallel, so a `is_initialized()` check followed by `init()` races:
/// two threads both observe "not yet" and the second `init()` panics
/// with "called more than once". `Once` makes the whole block run
/// exactly one time, and later callers block until it has.
fn init_runtime_singletons() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        blinc_theme::ThemeState::init_default();
        if !blinc_animation::is_scheduler_initialized() {
            let scheduler = blinc_animation::AnimationScheduler::new();
            blinc_animation::set_global_scheduler(scheduler.handle());
            // Keep the scheduler alive for the process; the handle holds a Weak.
            Box::leak(Box::new(scheduler));
        }
        if !blinc_core::BlincContextState::is_initialized() {
            blinc_core::BlincContextState::init(
                blinc_core::reactive::global_graph(),
                Arc::new(std::sync::Mutex::new(
                    blinc_core::context_state::HookState::new(),
                )),
                Arc::new(AtomicBool::new(false)),
            );
        }
    });
}

/// Node count of the rendered view (root included).
fn node_count(dsl: &BlincDsl) -> usize {
    let widget = dsl.view_widget();
    let mut tree = LayoutTree::new();
    let root = widget.build(&mut tree);
    let mut count = 0;
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        count += 1;
        stack.extend(tree.children(id));
    }
    count
}

/// Render `Div(class="host") { <widget> }` and return its node count.
fn render_one(widget_src: &str, file: &str) -> usize {
    init_runtime_singletons();
    let dsl = BlincDsl::new().expect("dsl init");
    blinc_cn_dsl::register_basics(&dsl).expect("register cn basics");
    let src = format!(r#"view {{ Div(class = "host") {{ {widget_src} }} }}"#);
    dsl.compile_source(&src, file)
        .unwrap_or_else(|e| panic!("compile {widget_src}: {e}"));
    node_count(&dsl)
}

/// Baseline: the DSL view root plus the host Div — two nodes. Anchors
/// every count below; without it a low count reads as a broken probe
/// rather than a dropped widget.
///
/// The view root is the element every DSL view mounts under so it
/// inherits the viewport (`materialize_view`). It is why the baseline is
/// two rather than one.
#[test]
fn empty_host_div_is_two_nodes() {
    init_runtime_singletons();
    let dsl = BlincDsl::new().expect("dsl init");
    blinc_cn_dsl::register_basics(&dsl).expect("register cn basics");
    dsl.compile_source(r#"view { Div(class = "host") { } }"#, "empty.blinc")
        .expect("compile");
    assert_eq!(node_count(&dsl), 2);
}

#[test]
fn every_exposed_cn_widget_renders() {
    // (label, DSL source) for each widget `register_basics` exposes.
    let cases: &[(&str, &str)] = &[
        ("Button", r#"cn.Button("Save", variant = "primary")"#),
        ("Badge", r#"cn.Badge("New", variant = "success")"#),
        ("Alert", r#"cn.Alert("Heads up", variant = "warning")"#),
        ("Label", r#"cn.Label("Email", required = true)"#),
        ("Separator", r#"cn.Separator(orientation = "horizontal")"#),
        ("Spinner", r#"cn.Spinner(size = "small")"#),
        ("Card", r#"cn.Card { }"#),
        ("Progress", r#"cn.Progress(value = 0.42, size = "medium")"#),
        (
            "Avatar",
            r#"cn.Avatar(src = "/jd.png", fallback = "JD", size = "medium")"#,
        ),
        (
            "Skeleton",
            r#"cn.Skeleton(w = 200.0, h = 16.0, rounded = 4.0)"#,
        ),
        ("Kbd", r#"cn.Kbd("Ctrl", size = "small")"#),
        (
            "Input",
            r#"cn.Input(key = "probe_user", placeholder = "Username", label = "User")"#,
        ),
        (
            "Textarea",
            r#"cn.Textarea(key = "probe_bio", placeholder = "Bio", rows = 3)"#,
        ),
        (
            "Switch",
            r#"cn.Switch(checked = true, label = "Wifi", size = "medium")"#,
        ),
        (
            "Checkbox",
            r#"cn.Checkbox(checked = false, label = "Accept", size = "small")"#,
        ),
    ];

    let mut dropped = Vec::new();
    for (name, src) in cases {
        let n = render_one(src, &format!("cn_{name}.blinc"));
        println!("  {name:<10} nodes={n}");
        if n < 2 {
            dropped.push(*name);
        }
    }
    assert!(
        dropped.is_empty(),
        "these cn widgets compiled but rendered nothing: {dropped:?}"
    );
}

/// The same widgets nested in a loop body — the shape that exposed the
/// argument-positionalisation gap in the widget-rewriting passes.
#[test]
fn cn_widgets_render_inside_a_loop() {
    init_runtime_singletons();
    let dsl = BlincDsl::new().expect("dsl init");
    blinc_cn_dsl::register_basics(&dsl).expect("register cn basics");
    dsl.compile_source(
        r#"
        signal cn_rows: i32
        view {
            Div(class = "host") {
                while cn_rows.get() < 3 {
                    cn.Badge("row", variant = "success")
                    cn_rows.set(cn_rows.get() + 1)
                }
            }
        }"#,
        "cn_loop.blinc",
    )
    .expect("compile");
    dsl.set_signal_i32("cn_rows", 0);
    let n = node_count(&dsl);
    assert_eq!(
        dsl.get_signal_i32("cn_rows"),
        Some(3),
        "loop must run 3 times"
    );
    assert!(
        n > 3,
        "expected a Badge subtree per iteration, got {n} nodes"
    );
}
