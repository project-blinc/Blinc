//! `Text(…, italic = true)` and the CSS properties behind the same flags.
//!
//! Before this, italic was reachable from neither: not a `Text` prop, and
//! `font-style` was not a parsed CSS property. The only route to italic
//! text in a `.blinc` file was `RichText("<i>…</i>")`, which pulls in a
//! markup parser to slant a label.
use blinc_dsl_core::BlincDsl;
use blinc_layout::renderer::{ElementType, RenderTree};

/// Walk from the root for the first text node.
fn find_text(tree: &RenderTree) -> Option<blinc_layout::renderer::TextData> {
    let mut stack = vec![tree.root()?];
    while let Some(id) = stack.pop() {
        if let Some(node) = tree.get_render_node(id)
            && let ElementType::Text(t) = &node.element_type
        {
            return Some(t.clone());
        }
        stack.extend(tree.layout_tree.children(id));
    }
    None
}

fn compile(src: &str, name: &str) -> BlincDsl {
    let dsl = BlincDsl::new().expect("runtime init");
    dsl.compile_source(src, name).expect("compile");
    dsl
}

/// (italic, strikethrough, underline) of the first text node in the tree.
///
/// Read from the render tree rather than the root widget: the view root is
/// a container, and these are the same fields the painter consults.
fn text_flags(src: &str, name: &str) -> (bool, bool, bool) {
    let dsl = compile(src, name);
    let tree = RenderTree::from_element(&dsl.view_widget());
    let text = find_text(&tree).expect("a text node in the tree");
    (text.italic, text.strikethrough, text.underline)
}

#[test]
fn text_takes_the_style_flags_as_props() {
    let (italic, strike, under) = text_flags(
        r#"view { Text("slanted", italic = true, strikethrough = true, underline = true) }"#,
        "styled_all",
    );
    assert!(italic, "italic = true reaches the element");
    assert!(strike, "strikethrough = true reaches the element");
    assert!(under, "underline = true reaches the element");
}

/// Omitted flags must default to off, not to whatever the adjacent slot
/// holds: they are trailing args filled by the arity pass.
#[test]
fn omitted_flags_stay_off() {
    let (italic, strike, under) = text_flags(r#"view { Text("upright") }"#, "plain_text");
    assert!(!italic && !strike && !under, "no flag set is no flag on");
}

#[test]
fn a_flag_set_alone_leaves_the_others_off() {
    let (italic, strike, under) =
        text_flags(r#"view { Text("slanted", italic = true) }"#, "one_flag");
    assert!(italic);
    assert!(!strike && !under, "only the named flag turns on");
}

/// `content` stays positional while the flags are named, which is the
/// shape every call site uses.
#[test]
fn the_content_argument_still_binds_positionally() {
    let dsl = compile(r#"view { Text("hello", italic = true) }"#, "mixed_args");
    let tree = RenderTree::from_element(&dsl.view_widget());
    let text = find_text(&tree).expect("a text node");
    assert_eq!(text.content, "hello");
    assert!(text.italic);
}
