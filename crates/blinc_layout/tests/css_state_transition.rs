//! A state rule's `transition` eases the properties it changes, each over
//! its own duration, and a change to other properties leaves a running
//! transition alone.

use blinc_core::{Brush, Color};
use blinc_layout::LayoutNodeId;
use blinc_layout::css_parser::Stylesheet;
use blinc_layout::div::div;
use blinc_layout::event_router::{EventRouter, MouseButton};
use blinc_layout::renderer::RenderTree;

struct Scene {
    tree: RenderTree,
    router: EventRouter,
    node: LayoutNodeId,
}

fn scene(css: &str) -> Scene {
    let ui = div()
        .w(400.0)
        .h(300.0)
        .child(div().class("b").w(100.0).h(100.0));
    let mut tree = RenderTree::from_element(&ui);
    tree.set_stylesheet(Stylesheet::parse(css).expect("css"));
    tree.apply_stylesheet_layout_overrides();
    tree.apply_stylesheet_base_styles();
    tree.compute_layout(400.0, 300.0);
    let node = tree.layout_tree.children(tree.root().unwrap())[0];
    Scene {
        tree,
        router: EventRouter::new(),
        node,
    }
}

impl Scene {
    fn restyle(&mut self) {
        self.tree.apply_stylesheet_state_styles(&self.router);
        self.tree.apply_all_css_transition_props();
    }

    fn hover(&mut self) {
        let _ = self.router.on_mouse_move(&self.tree, 50.0, 50.0);
        self.restyle();
    }

    fn press(&mut self) {
        let _ = self
            .router
            .on_mouse_down(&self.tree, 50.0, 50.0, MouseButton::Left);
        self.restyle();
    }

    /// Advance the transitions by `ms`, in the steps the clock allows.
    fn tick(&mut self, ms: f32) {
        let mut left = ms;
        while left > 0.0 {
            let step = left.min(50.0);
            self.tree.css_anim_store().lock().unwrap().tick(step);
            self.tree.apply_all_css_transition_props();
            left -= step;
        }
    }

    fn props(&self) -> blinc_layout::element::RenderProps {
        self.tree.get_render_node(self.node).unwrap().props.clone()
    }

    fn blue(&self) -> f32 {
        match self.props().background {
            Some(Brush::Solid(Color { b, .. })) => b,
            other => panic!("not a solid fill: {other:?}"),
        }
    }

    fn border_blue(&self) -> f32 {
        self.props().border_color.expect("a border colour").b
    }
}

const SPEC_NAMES: &str = r#"
.b { background: #ff0000; border: 1px solid #ff0000;
     transition: background-color 200ms linear, border-top-color 200ms linear; }
.b:hover { background: #0000ff; border-color: #0000ff; }
"#;

#[test]
fn a_longhand_colour_name_eases_the_property_it_animates() {
    let mut s = scene(SPEC_NAMES);
    s.hover();
    assert_eq!(s.blue(), 0.0, "the fill starts from the old colour");
    s.tick(100.0);
    let (fill, border) = (s.blue(), s.border_blue());
    assert!((0.4..0.6).contains(&fill), "fill half way: {fill}");
    assert!((0.4..0.6).contains(&border), "border half way: {border}");
    s.tick(150.0);
    assert_eq!((s.blue(), s.border_blue()), (1.0, 1.0));
}

#[test]
fn each_property_runs_over_its_own_duration() {
    let mut s = scene(
        r#"
.b { background: #ff0000; border: 1px solid #ff0000;
     transition: background-color 100ms linear, border-color 400ms linear; }
.b:hover { background: #0000ff; border-color: #0000ff; }
"#,
    );
    s.hover();
    s.tick(100.0);
    assert_eq!(s.blue(), 1.0, "the short one is done");
    let border = s.border_blue();
    assert!(
        (0.2..0.3).contains(&border),
        "the long one is a quarter in: {border}"
    );
    s.tick(300.0);
    assert_eq!(s.border_blue(), 1.0);
}

#[test]
fn a_change_to_other_properties_leaves_a_running_transition_alone() {
    let mut s = scene(
        r#"
.b { background: #ff0000; transition: background-color 400ms linear, transform 100ms linear; }
.b:hover { background: #0000ff; }
.b:active { transform: scale(0.9, 0.9); }
"#,
    );
    s.hover();
    s.tick(100.0);
    let before = s.blue();
    s.press();
    s.tick(50.0);
    let after = s.blue();
    assert!(
        after > before && after < 0.5,
        "the fill carried on from {before} to {after}"
    );
}

#[test]
fn a_transition_with_no_property_covers_every_one() {
    let mut s = scene(
        r#"
.b { background: #ff0000; transition: 200ms linear; }
.b:hover { background: #0000ff; }
"#,
    );
    s.hover();
    s.tick(100.0);
    assert!((0.4..0.6).contains(&s.blue()), "{}", s.blue());
}
