//! `font-style` as a CSS property.
//!
//! Italic was the one text style with no CSS route: `font-weight` and
//! `text-decoration` both parsed and reached the renderer, `font-style`
//! did not, so a class could embolden or underline text but not slant it.
use blinc_layout::css_parser::Stylesheet;
use blinc_layout::div::div;
use blinc_layout::element_style::FontStyle;
use blinc_layout::renderer::RenderTree;
use blinc_layout::text::text;

/// The font style the first text node ends up with.
fn font_style_of(css: &str, build: fn() -> blinc_layout::div::Div) -> Option<FontStyle> {
    let mut tree = RenderTree::from_element(&build());
    tree.set_stylesheet(Stylesheet::parse(css).expect("the sheet parses"));
    tree.apply_stylesheet_base_styles();
    tree.compute_layout(400.0, 200.0);

    let mut stack = vec![tree.root().expect("root")];
    while let Some(id) = stack.pop() {
        if let Some(node) = tree.get_render_node(id)
            && matches!(
                node.element_type,
                blinc_layout::renderer::ElementType::Text(_)
            )
        {
            return node.props.font_style;
        }
        stack.extend(tree.layout_tree.children(id));
    }
    None
}

fn classed_text() -> blinc_layout::div::Div {
    div().child(text("slanted").class("em"))
}

fn nested_text() -> blinc_layout::div::Div {
    div().child(div().class("quote").child(text("slanted")))
}

#[test]
fn font_style_italic_reaches_the_render_props() {
    assert_eq!(
        font_style_of(".em { font-style: italic; }", classed_text),
        Some(FontStyle::Italic),
    );
}

#[test]
fn font_style_normal_is_kept_distinct_from_unset() {
    assert_eq!(
        font_style_of(".em { font-style: normal; }", classed_text),
        Some(FontStyle::Normal),
        "an explicit `normal` must override an inherited italic, so it \
         cannot collapse to None",
    );
}

/// `oblique` selects the same face: there is no synthetic slant to tell
/// the two apart.
#[test]
fn oblique_is_treated_as_italic() {
    assert_eq!(
        font_style_of(".em { font-style: oblique; }", classed_text),
        Some(FontStyle::Italic),
    );
}

#[test]
fn an_unstyled_node_has_no_font_style() {
    assert_eq!(
        font_style_of(".other { font-style: italic; }", classed_text),
        None
    );
}

/// Italic inherits, so a rule on a container slants the text inside it.
#[test]
fn font_style_inherits_to_descendants() {
    assert_eq!(
        font_style_of(".quote { font-style: italic; }", nested_text),
        Some(FontStyle::Italic),
    );
}
