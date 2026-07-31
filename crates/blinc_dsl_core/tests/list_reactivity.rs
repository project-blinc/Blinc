//! Does a list signal re-render anything when it changes?
//!
//! Only inside a `with` region. `__blinc_map_children__` records its
//! read into whatever read scope is open, and `read_scope::record`
//! appends nothing when the stack is empty — so a bare `map` in a view
//! subscribes to nothing and the tree goes stale on the next set.
//!
//! Wrapping the map in `with` opens that scope, so the region observes
//! the list and re-renders just its own subtree.
use blinc_dsl_core::BlincDsl;
use blinc_layout::tree::LayoutTree;
use std::sync::{Arc, Mutex};

/// A `with` region mounts a `Stateful`, which needs the context state a
/// real app initialises at startup. Without it the region panics rather
/// than subscribing, and the assertions below would measure nothing.
fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
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

fn build(dsl: &BlincDsl) -> usize {
    let widget = dsl.view_widget();
    let mut tree = LayoutTree::new();
    let root = widget.build(&mut tree);
    let (mut n, mut stack) = (0, vec![root]);
    while let Some(id) = stack.pop() {
        n += 1;
        stack.extend(tree.children(id));
    }
    n
}

fn compile(src: &str, name: &str) -> BlincDsl {
    init();
    let dsl = BlincDsl::new().expect("runtime init");
    dsl.compile_source(src, name).expect("compile");
    dsl
}

const ROW: &str = r#"fn Row(l: string): View { Div(class="r") { Text(l) } }"#;

/// A map inside `with` subscribes: after the region renders, the list
/// signal has a subscriber, so a set reaches it.
#[test]
fn a_with_region_subscribes_to_the_list_it_maps() {
    blinc_runtime::signal::set_string_list("lr_with", vec!["a".into()]);
    let dsl = compile(
        &format!(
            r#"signal lr_with = ["a"]
               {ROW}
               view {{ Div(class="p") {{ with {{ Div(class="row") {{ lr_with.map(|t| Row(t)) }} }} }} }}"#
        ),
        "lr_with.blinc",
    );
    build(&dsl);

    let (id_raw, _) = blinc_runtime::signal::lookup("lr_with").expect("minted");
    assert!(
        blinc_layout::stateful::check_stateful_deps(&[blinc_core::reactive::SignalId::from_raw(
            id_raw
        )]),
        "the region must depend on the list it rendered from"
    );
}

/// A bare map subscribes to nothing. Pinned so the difference is a
/// stated property rather than a surprise: without `with`, a set does
/// not reach the view.
#[test]
fn a_bare_map_subscribes_to_nothing() {
    blinc_runtime::signal::set_string_list("lr_bare", vec!["a".into()]);
    let dsl = compile(
        &format!(
            r#"signal lr_bare = ["a"]
               {ROW}
               view {{ Div(class="p") {{ lr_bare.map(|t| Row(t)) }} }}"#
        ),
        "lr_bare.blinc",
    );
    build(&dsl);

    let (id_raw, _) = blinc_runtime::signal::lookup("lr_bare").expect("minted");
    assert!(
        !blinc_layout::stateful::check_stateful_deps(&[blinc_core::reactive::SignalId::from_raw(
            id_raw
        )]),
        "no scope was open, so nothing observed the read"
    );
}
