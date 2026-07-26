//! The playground page fills the window width.
//!
//! A DSL component call materialises an `Auto`-sized wrapper between the
//! host and the component's own root. In a row-direction host that
//! wrapper is on the main axis and shrinks to content, so a
//! `width: 100%` inside it resolves against the shrunk box rather than
//! the window -- the page rendered ~620px wide in a 1400px window with
//! everything correctly styled, which makes it look like the CSS failed
//! when it had not. A column host puts the wrapper on the cross axis,
//! where it stretches.

use blinc_dsl_core::BlincDsl;
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

const VIEWPORT_W: f32 = 1400.0;
const VIEWPORT_H: f32 = 900.0;

fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        blinc_theme::ThemeState::init_default();
        if !blinc_animation::is_scheduler_initialized() {
            let s = blinc_animation::AnimationScheduler::new();
            blinc_animation::set_global_scheduler(s.handle());
            Box::leak(Box::new(s));
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
    blinc_core::BlincContextState::get().set_viewport_size(VIEWPORT_W, VIEWPORT_H);
}

#[test]
fn page_fills_the_window_width() {
    let _ = tracing_subscriber::fmt::try_init();
    init();
    let root_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/playground");
    let dsl = BlincDsl::new().expect("dsl init");
    blinc_cn_dsl::register_all(&dsl).expect("register cn.*");
    dsl.compile_project(&root_dir.join("main.blinc"), &root_dir)
        .expect("compile");

    let widget = dsl.view_widget();
    // Install the CSS the DSL queued, as the windowed host does.
    let css: String = blinc_core::BlincContextState::get()
        .drain_stylesheets()
        .join("\n");
    // `flex_col` mirrors the example host; see the module note.
    let host = div()
        .w(VIEWPORT_W)
        .h(VIEWPORT_H)
        .flex_col()
        .child_box(widget);
    let mut tree = RenderTree::from_element(&host);
    tree.set_stylesheet(blinc_layout::css_parser::Stylesheet::parse(&css).expect("dsl css parses"));
    tree.apply_stylesheet_layout_overrides();
    tree.compute_layout(VIEWPORT_W, VIEWPORT_H);

    // Deepest node reachable at depth 2 is the component's `.page` root.
    let root = tree.root().expect("root");
    let wrapper = tree.layout_tree.children(root);
    let page = wrapper
        .first()
        .and_then(|w| tree.layout_tree.children(*w).first().copied())
        .expect("page node");
    let layout = tree.layout_tree.get_layout(page).expect("page layout");
    assert_eq!(
        layout.size.width, VIEWPORT_W,
        "`.page {{ width: 100% }}` must resolve against the window, not a shrunk wrapper"
    );
}
