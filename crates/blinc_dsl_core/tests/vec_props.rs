//! `Vec<T>` props on an `#[extern_widget]`, fed by a DSL list literal.
//!
//! The whole path: `["a", "b"]` parses to `TypedExpression::Array`,
//! Zyntax lowers it to `List<T> { data, len, capacity }`, the prop
//! crosses as one pointer, and the thunk copies the elements out at the
//! stride the element type implies.
use blinc_dsl_core::{BlincDsl, extern_widget};
use blinc_layout::div::ElementBuilder;
use std::sync::Mutex;

/// What the widget saw, so the test can assert on the decoded values
/// rather than on pixels.
static SEEN_LABELS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static SEEN_SIZES: Mutex<Vec<f64>> = Mutex::new(Vec::new());
static SEEN_FLAGS: Mutex<Vec<bool>> = Mutex::new(Vec::new());

#[extern_widget(name = "ListProbe")]
pub struct ListProbe {
    pub labels: Vec<String>,
    pub sizes: Vec<f64>,
    pub flags: Vec<bool>,
    #[skip]
    inner: std::cell::OnceCell<blinc_layout::div::Div>,
}

impl ListProbe {
    fn get_or_build(&self) -> &blinc_layout::div::Div {
        self.inner.get_or_init(|| {
            *SEEN_LABELS.lock().unwrap() = self.labels.clone();
            *SEEN_SIZES.lock().unwrap() = self.sizes.clone();
            *SEEN_FLAGS.lock().unwrap() = self.flags.clone();
            blinc_layout::div::div()
        })
    }
}

impl ElementBuilder for ListProbe {
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

/// One DSL instance per test, and the probe statics are shared.
static LOCK: Mutex<()> = Mutex::new(());

fn run(src: &str) {
    let dsl = BlincDsl::new().expect("dsl");
    dsl.register_extern_widget::<ListProbe>()
        .expect("register ListProbe");
    dsl.compile_source(src, "vec_props.blinc").expect("compile");
    // The thunk constructs the widget, but the props are only read when
    // something builds it, so put it in a tree.
    let host = blinc_layout::div::div()
        .w(200.0)
        .h(100.0)
        .child_box(dsl.view_widget());
    let mut tree = blinc_layout::renderer::RenderTree::from_element(&host);
    tree.compute_layout(200.0, 100.0);
}

#[test]
fn a_string_list_reaches_the_widget() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    *SEEN_LABELS.lock().unwrap() = Vec::new();
    run(r#"view { ListProbe(labels = ["alpha", "beta", "gamma"]) }"#);
    assert_eq!(
        *SEEN_LABELS.lock().unwrap(),
        vec!["alpha".to_string(), "beta".to_string(), "gamma".to_string()],
    );
}

/// An omitted collection prop is an empty list, not a panic.
#[test]
fn an_omitted_list_arrives_empty() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    *SEEN_LABELS.lock().unwrap() = vec!["stale".to_string()];
    run("view { ListProbe() }");
    assert!(SEEN_LABELS.lock().unwrap().is_empty());
}

/// Eight-byte elements read directly, no indirection.
#[test]
fn an_f64_list_reaches_the_widget() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    *SEEN_SIZES.lock().unwrap() = Vec::new();
    run("view { ListProbe(sizes = [1.5, 2.5, 3.5]) }");
    assert_eq!(*SEEN_SIZES.lock().unwrap(), vec![1.5, 2.5, 3.5]);
}

/// The stride case that a uniform i64 assumption gets wrong: `bool`
/// lowers to `HirType::Bool`, one byte per element, so a packed buffer
/// read eight bytes at a time would yield garbage.
#[test]
fn a_bool_list_reads_at_the_right_stride() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    *SEEN_FLAGS.lock().unwrap() = Vec::new();
    run("view { ListProbe(flags = [true, false, true, true]) }");
    assert_eq!(*SEEN_FLAGS.lock().unwrap(), vec![true, false, true, true]);
}
