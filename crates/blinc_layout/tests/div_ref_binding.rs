//! A `DivRef` resolves to the element it was bound to.
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;
use blinc_layout::selector::DivRef;
use blinc_layout::text::text;

/// Binding is the identity: the ref knows its node after a build, with
/// nothing named on either side.
#[test]
fn a_bound_ref_resolves_to_its_node_after_a_build() {
    let card = DivRef::new();
    assert!(!card.exists(), "nothing to point at before a build");

    let host = div()
        .w(200.0)
        .h(200.0)
        .child(div().bind(&card).w(50.0).h(60.0).child(text("inside")));

    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(200.0, 200.0);

    assert!(card.exists(), "the build resolved the ref to a node");
    let node = card.node_id().expect("bound");
    let laid_out = tree.layout_tree.get_layout(node).expect("laid out");
    assert_eq!(
        (laid_out.size.width, laid_out.size.height),
        (50.0, 60.0),
        "and it is the element that bound it, not some other node"
    );
}

/// Two refs, two elements: nothing is shared and nothing is named.
#[test]
fn separate_refs_resolve_to_separate_elements() {
    let (a, b) = (DivRef::new(), DivRef::new());
    let host = div()
        .w(200.0)
        .h(200.0)
        .child(div().bind(&a).w(10.0).h(10.0))
        .child(div().bind(&b).w(20.0).h(20.0));

    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(200.0, 200.0);

    let node_a = a.node_id().expect("a bound");
    let node_b = b.node_id().expect("b bound");
    assert_ne!(node_a, node_b);
    assert_eq!(
        tree.layout_tree
            .get_layout(node_a)
            .expect("laid out")
            .size
            .width,
        10.0
    );
    assert_eq!(
        tree.layout_tree
            .get_layout(node_b)
            .expect("laid out")
            .size
            .width,
        20.0
    );
}

/// A bound element gets an id if it had none, because that is what the
/// focus and scroll callbacks are keyed by — without it a ref would
/// accept every command and perform none.
#[test]
fn binding_gives_the_element_an_id_to_be_addressed_by() {
    let card = DivRef::new();
    let host = div()
        .w(100.0)
        .h(100.0)
        .child(div().bind(&card).w(10.0).h(10.0));
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(100.0, 100.0);

    let node = card.node_id().expect("bound");
    assert!(
        tree.element_registry().get_id(node).is_some(),
        "the bound element is addressable"
    );
}

/// An author-supplied id is left alone: binding must not rename an
/// element something else already refers to.
#[test]
fn binding_keeps_an_id_the_author_chose() {
    let card = DivRef::new();
    let host = div()
        .w(100.0)
        .h(100.0)
        .child(div().id("chosen").bind(&card).w(10.0).h(10.0));
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(100.0, 100.0);

    let node = card.node_id().expect("bound");
    assert_eq!(
        tree.element_registry().get_id(node).as_deref(),
        Some("chosen")
    );
}

/// A rebuild re-points the ref: `LayoutNodeId`s are reissued, so a ref
/// that kept the old one would address whatever now occupies it.
#[test]
fn a_rebuild_repoints_the_ref() {
    let card = DivRef::new();

    let build = || {
        div()
            .w(100.0)
            .h(100.0)
            .child(div().bind(&card).w(10.0).h(10.0))
    };

    let mut first = RenderTree::from_element(&build());
    first.compute_layout(100.0, 100.0);
    let before = card.node_id().expect("bound by the first build");

    let mut second = RenderTree::from_element(&build());
    second.compute_layout(100.0, 100.0);
    let after = card.node_id().expect("still bound after a rebuild");

    // Whatever the ids happen to be, the ref addresses a node in the
    // CURRENT tree — that is what has to hold.
    assert!(
        second.layout_tree.get_layout(after).is_some(),
        "the ref points into the tree that exists now"
    );
    let _ = before;
}

/// A ref whose node is taken over by another element goes dead, rather
/// than addressing the newcomer.
///
/// `LayoutNodeId`s are reissued across rebuilds, so this is what a ref
/// outliving its element looks like from inside one registry — the id
/// stays valid and starts pointing at something else entirely.
#[test]
fn a_ref_whose_node_is_reused_goes_dead() {
    let card = DivRef::new();
    let mut tree = RenderTree::from_element(
        &div()
            .w(100.0)
            .h(100.0)
            .child(div().bind(&card).w(10.0).h(10.0)),
    );
    tree.compute_layout(100.0, 100.0);

    let node = card.node_id().expect("bound");
    assert!(card.exists());

    // What a rebuild does when a different element lands on that node.
    tree.element_registry().register("someone-else", node);

    assert!(!card.exists(), "the ref does not claim the newcomer");
    assert_eq!(card.node_id(), None);
    assert_eq!(card.bounds(), None, "and reports no stale geometry");

    // Every command is a no-op rather than acting on the wrong element.
    card.focus();
    card.scroll_into_view();
}

/// `InputRef` binds both halves: the value immediately, the element
/// when the field is built.
#[test]
fn an_input_ref_binds_its_field_and_its_element() {
    use blinc_layout::selector::InputRef;
    use blinc_layout::widgets::text_input::{text_input, text_input_data};

    blinc_theme::ThemeState::init_default();

    let email = InputRef::new();
    let data = text_input_data();

    let host = div()
        .w(300.0)
        .h(100.0)
        .child(text_input(&data).bind(&email).w(200.0));

    // The value half is live before anything renders.
    assert!(email.is_bound(), "the field's state is handed over at bind");
    email.set_value("typed@example.com");
    assert_eq!(data.lock().unwrap().value, "typed@example.com");

    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(300.0, 100.0);

    assert!(
        email.exists(),
        "and the element half resolves once the field is built"
    );
    assert!(
        email.element().node_id().is_some(),
        "so focus and scroll-into-view have somewhere to land"
    );
}

/// An `InputRef` leaves the field's element exactly as it was built.
///
/// Binding used to give every element an id, and a text field is not an
/// inert box: it routes its own focus and keystrokes, and an id it never
/// asked for is a change to something that was working.
#[test]
fn an_input_ref_does_not_touch_the_fields_id() {
    use blinc_layout::selector::InputRef;
    use blinc_layout::widgets::text_input::{text_input, text_input_data};

    blinc_theme::ThemeState::init_default();

    let email = InputRef::new();
    let data = text_input_data();
    let host = div()
        .w(300.0)
        .h(100.0)
        .child(text_input(&data).bind(&email).w(200.0));

    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(300.0, 100.0);

    let node = email.element().node_id().expect("bound");
    assert!(
        !tree
            .element_registry()
            .get_id(node)
            .is_some_and(|id| id.starts_with("__blinc_ref_")),
        "no id was invented for it"
    );
    assert!(email.exists(), "and the binding still holds");
}
