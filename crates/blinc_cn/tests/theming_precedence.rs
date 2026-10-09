//! Who wins when a property is set by a component, an app stylesheet and a
//! call site.
//!
//! Three tiers: a component's own defaults, below the app's stylesheet,
//! below a fluent call at the use site. Today the stylesheet beats
//! everything the element set in Rust, so a component default and a
//! call-site value are not told apart.
//!
//! `an_app_stylesheet_themes_a_cn_component` is the contract theming relies
//! on and must hold on both sides of any change to precedence.

use blinc_core::Color;
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;

fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        blinc_theme::ThemeState::init_default();
        if !blinc_animation::is_scheduler_initialized() {
            let s = blinc_animation::AnimationScheduler::new();
            blinc_animation::set_global_scheduler(s.handle());
            Box::leak(Box::new(s));
        }
        if !blinc_core::BlincContextState::is_initialized() {
            blinc_core::BlincContextState::init(
                blinc_core::reactive::global_graph(),
                std::sync::Arc::new(std::sync::Mutex::new(
                    blinc_core::context_state::HookState::new(),
                )),
                std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            );
        }
    });
}

/// Lay out `host` and apply `css` the way an app's stylesheet reaches it.
fn render(host: &impl blinc_layout::div::ElementBuilder, css: &str) -> RenderTree {
    let mut tree = RenderTree::from_element(host);
    set_css(&mut tree, css);
    tree.compute_layout(300.0, 80.0);
    tree
}

fn set_css(tree: &mut RenderTree, css: &str) {
    tree.set_stylesheet(blinc_layout::css_parser::Stylesheet::parse(css).expect("css"));
    tree.apply_stylesheet_layout_overrides();
    tree.apply_stylesheet_base_styles();
}

/// The first child of the root: the element under test.
fn subject(tree: &RenderTree) -> blinc_layout::LayoutNodeId {
    let root = tree.root().expect("root");
    tree.layout_tree.children(root)[0]
}

fn radius(tree: &RenderTree) -> f32 {
    tree.get_render_node(subject(tree))
        .expect("render node")
        .props
        .border_radius
        .top_left
}

/// The solid colour a node is filled with, if it has one.
fn fill(tree: &RenderTree) -> Option<Color> {
    match tree
        .get_render_node(subject(tree))
        .expect("render node")
        .props
        .background
        .as_ref()
    {
        Some(blinc_core::Brush::Solid(c)) => Some(*c),
        _ => None,
    }
}

fn button() -> impl blinc_layout::div::ElementBuilder {
    div().w(300.0).h(80.0).child(blinc_cn::button("Go"))
}

/// An application themes a cn component by naming its class, and that beats
/// what the component set for itself. cn_demo's `#css-overrides` does this.
#[test]
fn an_app_stylesheet_themes_a_cn_component() {
    init();
    let plain = render(&button(), "");
    assert!(
        radius(&plain) > 0.0,
        "sanity: a cn button has a rounded default, got {}",
        radius(&plain)
    );

    let themed = render(&button(), ".cn-button--primary { border-radius: 0px; }");
    assert_eq!(
        radius(&themed),
        0.0,
        "the app's rule did not reach the button"
    );
}

/// The same holds for a fill: the app's colour replaces the component's.
#[test]
fn an_app_stylesheet_recolours_a_cn_component() {
    init();
    let red = Color::rgba(1.0, 0.0, 0.0, 1.0);
    let themed = render(&button(), ".cn-button--primary { background: #ff0000; }");
    assert_eq!(
        fill(&themed),
        Some(red),
        "the app's background did not reach the button"
    );
}

/// A call-site fluent value beats the stylesheet, as inline style does on
/// the web. Today the stylesheet wins, so this is ignored until precedence
/// moves.
#[test]
#[ignore = "needs the per-property tier: the stylesheet still beats fluent values"]
fn a_fluent_call_beats_a_stylesheet_rule() {
    init();
    let red = Color::rgba(1.0, 0.0, 0.0, 1.0);
    let host = div()
        .w(300.0)
        .h(80.0)
        .child(div().class("themed").w(100.0).h(40.0).bg(red).rounded(3.0));
    let tree = render(
        &host,
        ".themed { border-radius: 20px; background: #0000ff; }",
    );
    assert_eq!(radius(&tree), 3.0, "the stylesheet overrode rounded(3)");
    assert_eq!(fill(&tree), Some(red), "the stylesheet overrode bg(red)");
}

/// Taking a rule away reveals what was underneath rather than clearing it.
/// Today the value the rule wrote stays: the stylesheet pass overwrites the
/// props in place and keeps no lower tier to fall back to.
#[test]
#[ignore = "needs the Default tier: removing a rule leaves the value it wrote"]
fn removing_a_rule_restores_the_components_default() {
    init();
    let mut tree = render(&button(), "");
    let default = radius(&tree);

    set_css(&mut tree, ".cn-button--primary { border-radius: 0px; }");
    assert_eq!(radius(&tree), 0.0, "sanity: the rule applied");

    set_css(&mut tree, "");
    assert_eq!(
        radius(&tree),
        default,
        "removing the rule left the button at {} instead of its default {default}",
        radius(&tree)
    );
}
