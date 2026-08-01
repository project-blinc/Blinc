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
