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

/// Each writer is recorded against the property it wrote, so a later pass
/// can tell a call-site value from a stylesheet one.
#[test]
fn the_writer_of_each_property_is_recorded() {
    use blinc_layout::style_tier::{StyleProp, StyleTier};
    init();
    let host = div()
        .w(300.0)
        .h(80.0)
        .child(div().class("themed").w(100.0).h(40.0).bg(Color::RED));
    let tree = render(&host, ".themed { border-radius: 20px; }");
    let tiers = tree
        .get_render_node(subject(&tree))
        .expect("render node")
        .props
        .tiers;
    assert!(tiers.has(StyleProp::Background, StyleTier::Owned));
    assert!(!tiers.has(StyleProp::Background, StyleTier::Stylesheet));
    assert!(tiers.has(StyleProp::BorderRadius, StyleTier::Stylesheet));
    assert!(!tiers.has(StyleProp::BorderRadius, StyleTier::Owned));
}

/// `style` (a `style!` or `css!` value) writes the component tier; a fluent call on the same property
/// takes it over, and untouched ones stay defaults.
#[test]
fn style_marks_the_default_tier_until_a_call_site_takes_over() {
    use blinc_layout::div::ElementBuilder;
    use blinc_layout::style_tier::{StyleProp, StyleTier};
    init();
    let blue = Color::rgba(0.0, 0.0, 1.0, 1.0);
    let tiers = |d: &blinc_layout::div::Div| d.render_props().tiers;

    let component = div().style(&blinc_layout::style! { bg: blue, rounded: 8.0 });
    let t = tiers(&component);
    assert!(t.has(StyleProp::Background, StyleTier::Default));
    assert!(t.has(StyleProp::BorderRadius, StyleTier::Default));
    assert!(!t.has(StyleProp::Background, StyleTier::Owned));

    let used = component.rounded(3.0);
    let t = tiers(&used);
    assert!(t.has(StyleProp::Background, StyleTier::Default));
    assert!(t.has(StyleProp::BorderRadius, StyleTier::Owned));
    assert!(!t.has(StyleProp::BorderRadius, StyleTier::Default));
}

/// A component default is still themed by the app stylesheet.
#[test]
fn a_base_default_yields_to_the_stylesheet() {
    init();
    let blue = Color::rgba(0.0, 0.0, 1.0, 1.0);
    let host = div().w(300.0).h(80.0).child(
        div()
            .class("themed")
            .w(100.0)
            .h(40.0)
            .style(&blinc_layout::style! { bg: blue, rounded: 8.0 }),
    );
    assert_eq!(radius(&render(&host, "")), 8.0);
    assert_eq!(
        radius(&render(&host, ".themed { border-radius: 0px; }")),
        0.0
    );
}

/// A component's own stylesheet sits below the app's, whatever the
/// specificity of either and whichever was added first.
#[test]
fn an_app_rule_beats_a_component_rule_of_higher_specificity() {
    use blinc_layout::css_parser::Stylesheet;
    use blinc_layout::style_tier::{StyleProp, StyleTier};
    init();
    let component = Stylesheet::parse(".a.b { border-radius: 8px; }")
        .expect("css")
        .into_component();
    let app = Stylesheet::parse(".a { background: #ff0000; }").expect("css");
    let app_radius = Stylesheet::parse(".a { border-radius: 0px; }").expect("css");
    let host = div()
        .w(300.0)
        .h(80.0)
        .child(div().class("a").class("b").w(100.0).h(40.0));

    for app_first in [true, false] {
        let mut sheet = Stylesheet::parse("").expect("css");
        if app_first {
            sheet.merge(app_radius.clone());
            sheet.merge(component.clone());
        } else {
            sheet.merge(component.clone());
            sheet.merge(app_radius.clone());
        }
        let mut tree = RenderTree::from_element(&host);
        tree.set_stylesheet(sheet);
        tree.apply_stylesheet_layout_overrides();
        tree.apply_stylesheet_base_styles();
        assert_eq!(radius(&tree), 0.0, "app_first={app_first}");
    }

    let mut sheet = Stylesheet::parse("").expect("css");
    sheet.merge(component);
    sheet.merge(app);
    let mut tree = RenderTree::from_element(&host);
    tree.set_stylesheet(sheet);
    tree.apply_stylesheet_layout_overrides();
    tree.apply_stylesheet_base_styles();
    let tiers = tree
        .get_render_node(subject(&tree))
        .expect("node")
        .props
        .tiers;
    assert_eq!(
        radius(&tree),
        8.0,
        "the component rule applies when unopposed"
    );
    assert!(tiers.has(StyleProp::BorderRadius, StyleTier::Default));
    assert!(tiers.has(StyleProp::Background, StyleTier::Stylesheet));
}

/// cn's button states its radius, shadow and border as component defaults.
#[test]
fn a_cn_button_radius_is_a_component_default() {
    use blinc_layout::style_tier::{StyleProp, StyleTier};
    init();
    let tree = render(&button(), "");
    let tiers = tree
        .get_render_node(subject(&tree))
        .expect("render node")
        .props
        .tiers;
    assert!(tiers.has(StyleProp::BorderRadius, StyleTier::Default));
    assert!(!tiers.has(StyleProp::BorderRadius, StyleTier::Owned));
}

/// cn's own stylesheet is a layer: an app rule beats it even when the app's
/// rule was added first, which plain cascade order would have let cn win.
#[test]
fn the_app_beats_cn_styles_whichever_was_added_first() {
    use blinc_layout::css_parser::Stylesheet;
    init();
    let vars = blinc_theme::ThemeState::get().to_css_variable_map();
    let cn = Stylesheet::parse_with_variables(blinc_cn::cn_styles::CN_STYLES, &vars).expect("cn");
    let app = Stylesheet::parse(".cn-button { border-radius: 0px; }").expect("css");

    let mut sheet = Stylesheet::parse("").expect("css");
    sheet.merge(app);
    sheet.merge(cn);
    let mut tree = RenderTree::from_element(&button());
    tree.set_stylesheet(sheet);
    tree.apply_stylesheet_layout_overrides();
    tree.apply_stylesheet_base_styles();
    tree.compute_layout(300.0, 80.0);
    assert_eq!(radius(&tree), 0.0, "cn's size rule outranked the app's");
}

/// cn's typography base now reaches the label: the button's tracking and the
/// theme's font apply to its text, which the container rule used to miss.
#[test]
fn cn_typography_reaches_a_button_label() {
    use blinc_layout::css_parser::Stylesheet;
    use blinc_layout::renderer::ElementType;
    init();
    let vars = blinc_theme::ThemeState::get().to_css_variable_map();
    let cn = Stylesheet::parse_with_variables(blinc_cn::cn_styles::CN_STYLES, &vars).expect("cn");
    let mut tree = RenderTree::from_element(&button());
    tree.set_stylesheet(cn);
    tree.apply_stylesheet_layout_overrides();
    tree.apply_stylesheet_base_styles();
    tree.compute_layout(300.0, 80.0);
    let mut stack = vec![tree.root().expect("root")];
    let mut label = None;
    while let Some(id) = stack.pop() {
        if let Some(ElementType::Text(t)) = tree.get_render_node(id).map(|n| &n.element_type) {
            label = Some(t.clone());
            break;
        }
        stack.extend(tree.layout_tree.children(id));
    }
    let label = label.expect("a label");
    assert!(
        label.letter_spacing < 0.0,
        "tracking-tight did not reach the label"
    );
}

/// cn's stylesheet shrinks a pressed button, as ashui's does, and a release
/// puts it back.
#[test]
fn a_pressed_cn_button_shrinks_and_a_release_restores_it() {
    use blinc_layout::css_parser::Stylesheet;
    use blinc_layout::event_router::{EventRouter, MouseButton};
    init();
    let vars = blinc_theme::ThemeState::get().to_css_variable_map();
    let cn = Stylesheet::parse_with_variables(blinc_cn::cn_styles::CN_STYLES, &vars).expect("cn");
    blinc_layout::css_parser::set_active_stylesheet(std::sync::Arc::new(cn.clone()));
    let host = div().w(300.0).h(80.0).child(blinc_cn::button("Go"));
    let mut tree = RenderTree::from_element(&host);
    tree.set_stylesheet(cn);
    tree.apply_stylesheet_layout_overrides();
    tree.apply_stylesheet_base_styles();
    tree.compute_layout(300.0, 80.0);
    let node = subject(&tree);
    let b = tree.get_absolute_bounds(node).expect("bounds");
    let at = (b.x + b.width / 2.0, b.y + b.height / 2.0);
    let scale = |tree: &RenderTree| match &tree.get_render_node(node).unwrap().props.transform {
        Some(blinc_core::Transform::Affine2D(a)) => Some(a.elements[0]),
        _ => None,
    };
    assert_eq!(scale(&tree), None);

    let mut router = EventRouter::new();
    let down = router.on_mouse_down(&tree, at.0, at.1, MouseButton::Left);
    for (n, e) in down {
        tree.dispatch_event(n, e, at.0, at.1);
    }
    tree.apply_stylesheet_state_styles(&router);
    let pressed = scale(&tree).expect("a transform while pressed");
    assert!((pressed - 0.97).abs() < 0.01, "scale {pressed}");

    let up = router.on_mouse_up(&tree, at.0, at.1, MouseButton::Left);
    for (n, e) in up {
        tree.dispatch_event(n, e, at.0, at.1);
    }
    tree.apply_stylesheet_state_styles(&router);
    assert!(
        scale(&tree).is_none_or(|s| (s - 1.0).abs() < 0.01),
        "the press outlasted the release"
    );
}
