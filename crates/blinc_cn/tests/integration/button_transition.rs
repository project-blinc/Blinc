//! A cn button eases between its states through the CSS `transition` in
//! cn's stylesheet: nothing in Rust asks for it.

use blinc_core::Brush;
use blinc_layout::css_parser::Stylesheet;
use blinc_layout::div::div;
use blinc_layout::event_router::EventRouter;
use blinc_layout::renderer::RenderTree;
use blinc_theme::{ColorToken, ThemeState};

fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        blinc_theme::ThemeState::init(
            blinc_theme::HybridTheme::bundle(),
            blinc_theme::ColorScheme::Dark,
        );
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

#[test]
fn a_hovered_button_eases_to_its_hover_fill() {
    init();
    let vars = ThemeState::get().to_css_variable_map();
    let cn = Stylesheet::parse_with_variables(blinc_cn::cn_styles::CN_STYLES, &vars).expect("cn");
    let host = div().w(300.0).h(80.0).child(blinc_cn::button("Go"));
    let mut tree = RenderTree::from_element(&host);
    tree.set_stylesheet(cn);
    tree.apply_stylesheet_layout_overrides();
    tree.apply_stylesheet_base_styles();
    tree.compute_layout(300.0, 80.0);
    let node = tree.layout_tree.children(tree.root().unwrap())[0];
    let b = tree.get_absolute_bounds(node).expect("bounds");

    let red = |tree: &RenderTree| match &tree.get_render_node(node).unwrap().props.background {
        Some(Brush::Solid(c)) => c.r,
        other => panic!("not a solid fill: {other:?}"),
    };
    let theme = ThemeState::get();
    let rest = theme.color(ColorToken::Primary).r;
    let hover = theme.color(ColorToken::PrimaryHover).r;
    assert!(
        (rest - hover).abs() > 0.05,
        "the theme gives no hover change"
    );
    assert!((red(&tree) - rest).abs() < 0.01);

    let mut router = EventRouter::new();
    let _ = router.on_mouse_move(&tree, b.x + b.width / 2.0, b.y + b.height / 2.0);
    tree.apply_stylesheet_state_styles(&router);
    tree.apply_all_css_transition_props();
    assert!(
        (red(&tree) - rest).abs() < 0.01,
        "the fill jumped to the hover colour"
    );

    for _ in 0..2 {
        tree.css_anim_store().lock().unwrap().tick(50.0);
        tree.apply_all_css_transition_props();
    }
    let mid = red(&tree);
    let (lo, hi) = (rest.min(hover), rest.max(hover));
    assert!(
        mid > lo + 0.005 && mid < hi - 0.005,
        "{mid} not between {rest} and {hover}"
    );

    for _ in 0..8 {
        tree.css_anim_store().lock().unwrap().tick(50.0);
        tree.apply_all_css_transition_props();
    }
    assert!((red(&tree) - hover).abs() < 0.01, "it did not arrive");
}
