//! `with @stateful([…]) { … }` — inline reactive regions.
//!
//! The point of the construct is scope: the region re-renders on its
//! own, and the view around it does not. So these check both halves —
//! that the body actually renders, and that mounting one does NOT set
//! the whole-program `@stateful` flag.
use blinc_dsl_core::{BlincDsl, extern_widget};
use blinc_layout::div::ElementBuilder;
use std::sync::{Arc, Mutex};

/// Tags of every probe that got built, in build order. A region that
/// silently rendered nothing leaves this empty, which is the failure
/// every structural assertion here would otherwise miss.
static BUILT: Mutex<Vec<String>> = Mutex::new(Vec::new());

#[extern_widget(name = "Probe")]
pub struct Probe {
    pub tag: String,
    #[skip]
    inner: std::cell::OnceCell<blinc_layout::div::Div>,
}

impl Probe {
    fn get_or_build(&self) -> &blinc_layout::div::Div {
        self.inner.get_or_init(|| {
            BUILT
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(self.tag.clone());
            blinc_layout::div::div().w(10.0).h(10.0)
        })
    }
}

impl ElementBuilder for Probe {
    fn build(&self, tree: &mut blinc_layout::LayoutTree) -> blinc_layout::LayoutNodeId {
        self.get_or_build().build(tree)
    }
    fn render_props(&self) -> blinc_layout::RenderProps {
        self.get_or_build().render_props()
    }
    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().children_builders()
    }
}

fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        // Without this a signal write reaches no `Stateful` at all, and
        // every re-render assertion below would pass by measuring
        // nothing.
        blinc_core::reactive::set_stateful_deps_notifier(|ids| {
            blinc_layout::check_stateful_deps(ids);
        });
        blinc_theme::ThemeState::init_default();
        if !blinc_core::BlincContextState::is_initialized() {
            blinc_core::BlincContextState::init(
                blinc_core::reactive::global_graph(),
                Arc::new(Mutex::new(blinc_core::context_state::HookState::new())),
                Arc::new(std::sync::atomic::AtomicBool::new(false)),
            );
        }
    });
}

/// One DSL instance per test, and the probe log is shared.
static LOCK: Mutex<()> = Mutex::new(());

fn compile(src: &str) -> BlincDsl {
    init();
    let dsl = BlincDsl::new().expect("dsl");
    dsl.register_extern_widget::<Probe>()
        .expect("register Probe");
    dsl.compile_source(src, "with_blocks.blinc")
        .expect("compile");
    dsl
}

/// Lay the program out, which is what runs the probes.
fn render(dsl: &BlincDsl) -> Vec<String> {
    BUILT.lock().unwrap_or_else(|e| e.into_inner()).clear();
    let host = blinc_layout::div::div()
        .w(400.0)
        .h(300.0)
        .child_box(dsl.view_widget());
    let mut tree = blinc_layout::renderer::RenderTree::from_element(&host);
    tree.compute_layout(400.0, 300.0);
    BUILT.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

const COUNTER: &str = r#"
signal count: i32 = 0
view {
    Div {
        Probe(tag = "outside")
        with @stateful([count]) {
            Probe(tag = "inside")
        }
    }
}
"#;

#[test]
fn a_with_block_lifts_its_body_to_a_view() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dsl = compile(COUNTER);
    let views = dsl.value_returning_views();
    assert!(
        views
            .iter()
            .any(|s| s.starts_with("__blinc_with_") && s.ends_with("$view")),
        "the region's body must become a view of its own: {views:?}"
    );
}

/// The whole reason for the construct. `@stateful` on a view sets a
/// global flag that wraps the entire program in one `Stateful`; a
/// `with` block must not.
#[test]
fn a_with_block_does_not_wrap_the_whole_program() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dsl = compile(COUNTER);
    assert!(
        !dsl.has_stateful_view(),
        "a `with` region is scoped — the program must not be wrapped"
    );
}

/// A region that renders nothing passes every structural check above.
#[test]
fn a_with_block_renders_its_body() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dsl = compile(COUNTER);
    let built = render(&dsl);
    assert!(
        built.iter().any(|t| t == "inside"),
        "the region's body must reach the tree: built {built:?}"
    );
    assert!(
        built.iter().any(|t| t == "outside"),
        "and the view around it still renders: built {built:?}"
    );
}

/// `with` inside a widget body, which is where it will mostly be
/// written — the marker has to survive children collection.
#[test]
fn a_with_block_nests_inside_a_widget_body() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dsl = compile(
        r#"
signal count: i32 = 0
view {
    Div {
        Div {
            with @stateful([count]) {
                Probe(tag = "nested")
            }
        }
    }
}
"#,
    );
    let built = render(&dsl);
    assert!(
        built.iter().any(|t| t == "nested"),
        "a nested region must render: built {built:?}"
    );
}

/// The whole point, and the one thing the checks above cannot infer: a
/// write to a listed signal re-renders the REGION and nothing else. If
/// the region never re-rendered, the second assertion would pass
/// vacuously, so the first has to come with it.
#[test]
fn a_write_re_renders_the_region_and_not_the_view_around_it() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dsl = compile(COUNTER);

    let host = blinc_layout::div::div()
        .w(400.0)
        .h(300.0)
        .child_box(dsl.view_widget());
    let mut tree = blinc_layout::renderer::RenderTree::from_element(&host);
    tree.compute_layout(400.0, 300.0);
    tree.process_pending_subtree_rebuilds();
    tree.compute_layout(400.0, 300.0);

    BUILT.lock().unwrap_or_else(|e| e.into_inner()).clear();
    dsl.set_signal_i32("count", 1);
    tree.process_pending_subtree_rebuilds();
    tree.compute_layout(400.0, 300.0);

    let built = BUILT.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert!(
        built.iter().any(|t| t == "inside"),
        "the region must re-render on a write to its dep: built {built:?}"
    );
    assert!(
        !built.iter().any(|t| t == "outside"),
        "and nothing outside it may: built {built:?}"
    );
}

/// Two regions in one view get distinct ids, so neither mounts against
/// the other's body.
#[test]
fn two_regions_are_independent() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dsl = compile(
        r#"
signal count: i32 = 0
view {
    Div {
        with @stateful([count]) { Probe(tag = "first") }
        with @stateful([count]) { Probe(tag = "second") }
    }
}
"#,
    );
    let built = render(&dsl);
    assert!(
        built.iter().any(|t| t == "first") && built.iter().any(|t| t == "second"),
        "both regions must render their own body: built {built:?}"
    );
}
