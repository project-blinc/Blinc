//! The hashes stored while a tree is collected match those computed from its builders.

use crate::diff::DivHash;
use crate::div::{ElementBuilder, div};
use crate::renderer::RenderTree;
use crate::text::text;

fn nested() -> crate::div::Div {
    let mut grid = div().class("grid").flex_row();
    for i in 0..4 {
        grid = grid.child(
            div()
                .class("cell")
                .child(text(format!("{i}")))
                .child(div().class("dot").w(4.0).h(4.0)),
        );
    }
    div().id("page").flex_col().child(grid).child(text("after"))
}

#[test]
fn stored_tree_hashes_equal_the_builders_hashes() {
    let ui = nested();
    let tree = RenderTree::from_element(&ui);
    let root = tree.root().expect("a root");
    let &(own, tree_hash, topology) = tree.node_hashes.get(&root).expect("root hashes");
    assert_eq!(own, DivHash::compute_element(&ui));
    assert_eq!(tree_hash, DivHash::compute_element_tree(&ui));
    assert_eq!(topology, DivHash::compute_topology_tree(&ui));

    // A node deeper down too.
    let grid = tree.layout_tree.children(root)[0];
    let cell = tree.layout_tree.children(grid)[2];
    let builder = ui.children_builders()[0].children_builders()[2].as_ref();
    let &(_, tree_hash, topology) = tree.node_hashes.get(&cell).expect("cell hashes");
    assert_eq!(tree_hash, DivHash::compute_element_tree(builder));
    assert_eq!(topology, DivHash::compute_topology_tree(builder));
}

#[test]
fn a_change_deep_in_the_tree_changes_the_root_hashes() {
    let a = nested();
    let b = nested().child(div().child(div().child(text("deep"))));
    assert_ne!(
        DivHash::compute_element_tree(&a),
        DivHash::compute_element_tree(&b)
    );
    assert_ne!(
        DivHash::compute_topology_tree(&a),
        DivHash::compute_topology_tree(&b)
    );
}
