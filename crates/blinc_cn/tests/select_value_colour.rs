//! The text a select shows takes its colour from the trigger's rules: dimmed
//! while it is a placeholder or the select is disabled, and following a bound
//! `disabled` in place.

mod common;

use blinc_core::reactive::{Signal, signal};
use blinc_layout::LayoutNodeId;
use blinc_layout::css_parser::Stylesheet;
use blinc_layout::div::div;
use blinc_layout::renderer::{ElementType, RenderTree};
use blinc_theme::{ColorToken, ThemeState};
use common::*;

struct Scene {
    tree: RenderTree,
}

fn scene(value: Signal<String>, disabled: Signal<bool>) -> Scene {
    init();
    let vars = ThemeState::get().to_css_variable_map();
    let sheet =
        Stylesheet::parse_with_variables(blinc_cn::cn_styles::CN_STYLES, &vars).expect("cn styles");
    let select = blinc_cn::select(value)
        .placeholder("Pick a fruit")
        .option("apple", "Apple")
        .disabled(disabled)
        .w(200.0);
    let host = div().w(400.0).h(300.0).child(select);
    let mut tree = RenderTree::from_element(&host);
    tree.set_stylesheet(sheet);
    tree.apply_all_stylesheet_styles();
    tree.compute_layout(400.0, 300.0);
    Scene { tree }
}

impl Scene {
    fn frame(&mut self) {
        let updates = blinc_layout::take_pending_partial_prop_updates();
        self.tree.apply_partial_property_updates(updates);
        self.tree.compute_layout(400.0, 300.0);
    }

    fn shown(&self) -> (String, [f32; 4]) {
        let registry = self.tree.element_registry();
        let mut stack = vec![self.tree.root().unwrap()];
        let trigger = loop {
            let node = stack.pop().expect("a trigger");
            if registry.has_class(node, "cn-select-trigger") {
                break node;
            }
            stack.extend(self.tree.layout_tree.children(node));
        };
        let mut stack: Vec<LayoutNodeId> = vec![trigger];
        while let Some(node) = stack.pop() {
            if let Some(ElementType::Text(t)) =
                self.tree.get_render_node(node).map(|r| &r.element_type)
            {
                let colour = self.tree.resolved_text_color(node).expect("a colour");
                return (t.content.clone(), colour);
            }
            stack.extend(self.tree.layout_tree.children(node));
        }
        panic!("the trigger shows no text");
    }
}

fn token(t: ColorToken) -> [f32; 4] {
    let c = ThemeState::get().color(t);
    [c.r, c.g, c.b, c.a]
}

#[test]
fn a_placeholder_is_dim_and_a_value_is_not() {
    guarded(|| {
        let value = signal(String::new());
        let mut s = scene(value, signal(false));
        assert_eq!(
            s.shown(),
            ("Pick a fruit".into(), token(ColorToken::TextTertiary))
        );
        value.set("apple".into());
        s.frame();
        assert_eq!(s.shown(), ("Apple".into(), token(ColorToken::TextPrimary)));
    });
}

#[test]
fn a_bound_disabled_dims_the_value_in_place() {
    guarded(|| {
        let disabled = signal(false);
        let mut s = scene(signal("apple".into()), disabled);
        assert_eq!(s.shown().1, token(ColorToken::TextPrimary));
        disabled.set(true);
        s.frame();
        assert_eq!(s.shown().1, token(ColorToken::TextTertiary), "disabled");
        disabled.set(false);
        s.frame();
        assert_eq!(s.shown().1, token(ColorToken::TextPrimary), "enabled again");
    });
}
