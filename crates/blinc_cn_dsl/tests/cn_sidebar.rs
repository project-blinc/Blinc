//! `cn.Sidebar` — rows, sections, content, and the collapse.
use blinc_dsl_core::BlincDsl;
use blinc_layout::div::div;
use blinc_layout::renderer::{ElementType, RenderTree};
use std::sync::{Arc, Mutex};

fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        blinc_theme::ThemeState::init_default();
        let s = blinc_animation::AnimationScheduler::new();
        blinc_animation::set_global_scheduler(s.handle());
        blinc_layout::render_state::set_global_scheduler(s.handle());
        Box::leak(Box::new(s));
        if !blinc_core::BlincContextState::is_initialized() {
            blinc_core::BlincContextState::init(
                blinc_core::reactive::global_graph(),
                Arc::new(Mutex::new(blinc_core::context_state::HookState::new())),
                Arc::new(std::sync::atomic::AtomicBool::new(false)),
            );
        }
    });
}

fn compiled(src: &str, name: &str) -> BlincDsl {
    init();
    let dsl = BlincDsl::new().expect("runtime init");
    blinc_cn_dsl::register_all(&dsl).expect("register");
    dsl.compile_source(src, name).expect("compile");
    dsl
}

fn laid_out(dsl: &BlincDsl) -> RenderTree {
    let host = div().w(900.0).h(600.0).child_box(dsl.view_widget());
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(900.0, 600.0);
    tree
}

fn texts(tree: &RenderTree) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![tree.root().expect("root")];
    while let Some(id) = stack.pop() {
        if let Some(node) = tree.get_render_node(id)
            && let ElementType::Text(t) = &node.element_type
        {
            out.push(t.content.clone());
        }
        stack.extend(tree.layout_tree.children(id).iter().copied());
    }
    out
}

/// Width of the narrowest node holding `label`: the navigation rail is
/// what changes width on collapse, and the row sits inside it.
fn rail_width(tree: &RenderTree, label: &str) -> f32 {
    let mut stack = vec![(tree.root().expect("root"), Vec::new())];
    while let Some((id, path)) = stack.pop() {
        if let Some(node) = tree.get_render_node(id)
            && let ElementType::Text(t) = &node.element_type
            && t.content == label
        {
            // The row's own width tracks the rail, both being sized from
            // the widest label.
            let row = path.last().copied().unwrap_or(id);
            return tree
                .layout_tree
                .get_layout(row)
                .expect("laid out")
                .size
                .width;
        }
        let mut child_path = path.clone();
        child_path.push(id);
        for child in tree.layout_tree.children(id) {
            stack.push((child, child_path.clone()));
        }
    }
    panic!("no row labelled {label}")
}

const NAV: &str = r#"signal shut: bool = false
                     signal page: i32 = 0

                     view {
                       cn.Sidebar(collapsed = shut) {
                         cn.SidebarSection(title = "Widgets") {
                           cn.SidebarItem(label = "Forms", icon = "square-pen", active = true,
                                          on_click = || page.set(0))
                           cn.SidebarItem(label = "Feedback", icon = "bell",
                                          on_click = || page.set(1))
                         }
                         cn.SidebarContent {
                           with {
                             Div {
                               if page.get() == 0 {
                                 cn.Label("the forms page")
                               } else {
                                 cn.Label("the feedback page")
                               }
                             }
                           }
                         }
                       }
                     }"#;

/// The whole navigation flow: what the rail draws, switching pages, and
/// collapsing to icons.
///
/// One program for all three: every piece of this is keyed
/// process-globally — the sidebar's stateful subtree, the content
/// region — so a page switched or a rail collapsed by one test is still
/// switched and collapsed when the next compiles a fresh program.
/// Walking one instance through the flow is also the stronger claim,
/// since the labels that vanish are the ones just seen.
#[test]
fn the_rail_navigates_and_collapses() {
    // Sharing one test with the bare-item case on purpose: a sidebar's
    // stateful subtree is keyed process-globally, so two tests each
    // compiling their own program see each other's rails, and `cargo
    // test` runs them in an order that varies. Running the expanded
    // case before anything collapses is the only stable order.
    a_bare_item_renders_and_a_loose_child_does_not();

    let dsl = compiled(NAV, "nav_flow.blinc");

    let tree = laid_out(&dsl);
    let found = texts(&tree);
    for want in ["WIDGETS", "Forms", "Feedback", "the forms page"] {
        assert!(found.iter().any(|t| t == want), "{want} renders: {found:?}");
    }
    let expanded = rail_width(&tree, "Forms");
    assert!(expanded > 0.0, "the expanded row had width: {expanded}");

    // The write an item's `on_click` makes.
    dsl.set_signal_i32("page", 1);
    let found = texts(&laid_out(&dsl));
    assert!(
        found.iter().any(|t| t == "the feedback page"),
        "the content area swaps: {found:?}"
    );
    assert!(
        !found.iter().any(|t| t == "the forms page"),
        "and the old page is gone: {found:?}"
    );

    // Qualified: the bare-item program above declares `shut` too, so two
    // modules own that name and an unqualified write has no single signal
    // to land on.
    dsl.set_signal_bool("nav_flow.shut", true);
    let found = texts(&laid_out(&dsl));
    assert!(
        !found.iter().any(|t| t == "Forms" || t == "WIDGETS"),
        "a collapsed rail shows icons only: {found:?}"
    );
    assert!(
        found.iter().any(|t| t == "the feedback page"),
        "and the content area stays: {found:?}"
    );
}

/// An item needs no section around it, and a child that is not an item
/// has no row to be, so it is dropped.
fn a_bare_item_renders_and_a_loose_child_does_not() {
    let dsl = compiled(
        r#"signal shut: bool = false
           view {
             cn.Sidebar(collapsed = shut) {
               cn.SidebarItem(label = "Alone", icon = "house")
               cn.Label("loose")
             }
           }"#,
        "nav_bare.blinc",
    );
    let found = texts(&laid_out(&dsl));
    assert!(
        found.iter().any(|t| t == "Alone"),
        "the row renders without a section: {found:?}"
    );
    assert!(
        !found.iter().any(|t| t == "loose"),
        "and the loose child is dropped: {found:?}"
    );
}

/// `cn.Icon` renders a named glyph, and an unknown name renders nothing
/// rather than a placeholder box.
#[test]
fn cn_icon_renders_a_named_glyph() {
    let dsl = compiled(
        r##"view {
             Div {
               cn.Icon(name = "house")
               cn.Icon(name = "bell", size = "large", color = "#8AB4F8")
               cn.Icon(name = "not-an-icon")
             }
           }"##,
        "icons.blinc",
    );
    let tree = laid_out(&dsl);

    let mut svgs = 0;
    let mut stack = vec![tree.root().expect("root")];
    while let Some(id) = stack.pop() {
        if let Some(node) = tree.get_render_node(id)
            && matches!(node.element_type, ElementType::Svg(_))
        {
            svgs += 1;
        }
        stack.extend(tree.layout_tree.children(id).iter().copied());
    }
    assert!(svgs >= 2, "both named icons render: {svgs} svg nodes");
}
