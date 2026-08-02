//! `cn.Slider` — a bound number picked by dragging.
use blinc_dsl_core::BlincDsl;
use blinc_layout::div::div;
use blinc_layout::renderer::{ElementType, RenderTree};
use std::sync::{Arc, Mutex};

fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        blinc_theme::ThemeState::init_default();
        let s = blinc_animation::AnimationScheduler::new();
        blinc_animation::set_global_scheduler(s.handle());
        blinc_layout::render_state::set_global_scheduler(s.handle());
        Box::leak(Box::new(s));
        blinc_core::reactive::set_stateful_deps_notifier(|ids| {
            blinc_layout::check_stateful_deps(ids);
        });
        if !blinc_core::BlincContextState::is_initialized() {
            blinc_core::BlincContextState::init(
                blinc_core::reactive::global_graph(),
                Arc::new(Mutex::new(blinc_core::context_state::HookState::new())),
                Arc::new(std::sync::atomic::AtomicBool::new(false)),
            );
        }
    });
}

fn texts(dsl: &BlincDsl) -> Vec<String> {
    let host = div().w(400.0).h(200.0).child_box(dsl.view_widget());
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(400.0, 200.0);
    let mut out = Vec::new();
    let mut stack = vec![tree.root().expect("root")];
    while let Some(id) = stack.pop() {
        if let Some(node) = tree.get_render_node(id)
            && let ElementType::Text(t) = &node.element_type
        {
            out.push(t.content.clone());
        }
        stack.extend(tree.layout_tree.children(id).iter().copied());
    }
    out
}

/// The slider seeds itself from the bound signal, so a value set before
/// the first build is the one it shows — not the range's floor.
#[test]
fn the_slider_opens_at_the_bound_value() {
    init();
    let dsl = BlincDsl::new().expect("runtime init");
    blinc_cn_dsl::register_all(&dsl).expect("register");
    dsl.compile_source(
        r#"signal volume: f64 = 40.0

           view {
             cn.Slider(value = volume, min = 0.0, max = 100.0, step = 5.0,
                       label = "Volume", show_value = true)
           }"#,
        "slider.blinc",
    )
    .expect("compile");

    let found = texts(&dsl);
    assert!(
        found.iter().any(|t| t == "Volume"),
        "the label renders: {found:?}"
    );
    // `show_value` prints the number it opened at, which is the signal's
    // value rather than `min`.
    assert!(
        found.iter().any(|t| t.contains("40")),
        "it opened at the bound value, not the floor: {found:?}"
    );

    dsl.set_signal_f64("volume", 75.0);
    let found = texts(&dsl);
    assert!(
        found.iter().any(|t| t.contains("75")),
        "and a later write is read on the next build: {found:?}"
    );
}

/// The drawn track is only as wide as the travel the thumb is laid out
/// against.
///
/// They came apart: the container stretched to its parent while the
/// thumb and fill stayed on a fixed width, so the track ran past the
/// point the maximum mapped to and the thumb stopped well short of the
/// end.
#[test]
fn the_track_is_as_wide_as_the_thumbs_travel() {
    init();
    let dsl = BlincDsl::new().expect("runtime init");
    blinc_cn_dsl::register_all(&dsl).expect("register");
    dsl.compile_source(
        r#"signal volume: f64 = 100.0
           view {
             cn.Slider(value = volume, min = 0.0, max = 100.0, label = "Volume")
           }"#,
        "slider_width.blinc",
    )
    .expect("compile");

    // A host far wider than the slider's own width, which is what let
    // the two disagree.
    let host = div().w(900.0).h(200.0).child_box(dsl.view_widget());
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(900.0, 200.0);

    // Every horizontal bar in the slider — track, fill, clip — has to
    // agree on one width.
    let mut bar_widths = Vec::new();
    let mut stack = vec![tree.root().expect("root")];
    while let Some(id) = stack.pop() {
        if let Some(l) = tree.layout_tree.get_layout(id)
            && l.size.height > 0.0
            && l.size.height <= 8.0
            && l.size.width > 50.0
        {
            bar_widths.push(l.size.width);
        }
        stack.extend(tree.layout_tree.children(id).iter().copied());
    }

    assert!(!bar_widths.is_empty(), "the slider drew a track");
    let first = bar_widths[0];
    assert!(
        bar_widths.iter().all(|w| (*w - first).abs() < 0.5),
        "track, fill and clip all span one width: {bar_widths:?}"
    );
    assert!(
        first < 900.0,
        "and it is the slider's own width, not the container's: {first}"
    );
}
