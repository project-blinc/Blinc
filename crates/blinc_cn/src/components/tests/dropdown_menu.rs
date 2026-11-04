use super::*;

#[test]
fn test_dropdown_position_bottom() {
    let bounds = ElementBounds {
        x: 100.0,
        y: 50.0,
        width: 80.0,
        height: 32.0,
    };
    let (x, y) = calculate_dropdown_position(
        &bounds,
        DropdownPosition::Bottom,
        DropdownAlign::Start,
        4.0,
        180.0,
    );
    assert_eq!(x, 100.0);
    assert_eq!(y, 86.0); // 50 + 32 + 4
}

#[test]
fn test_dropdown_position_end_align() {
    let bounds = ElementBounds {
        x: 100.0,
        y: 50.0,
        width: 80.0,
        height: 32.0,
    };
    let (x, _y) = calculate_dropdown_position(
        &bounds,
        DropdownPosition::Bottom,
        DropdownAlign::End,
        4.0,
        180.0,
    );
    assert_eq!(x, 0.0); // 100 + 80 - 180 = 0 (clamped)
}

fn init_context() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        ThemeState::init_default();
        if !BlincContextState::is_initialized() {
            BlincContextState::init(
                blinc_core::reactive::global_graph(),
                Arc::new(std::sync::Mutex::new(
                    blinc_core::context_state::HookState::new(),
                )),
                Arc::new(std::sync::atomic::AtomicBool::new(false)),
            );
        }
    });
}

/// Key events reach every menu built in the process, so the tests that send them run one at a
/// time.
fn serial() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn keyed_menu(
    id: &str,
) -> (
    blinc_layout::renderer::RenderTree,
    blinc_layout::event_router::EventRouter,
    Arc<std::sync::Mutex<Vec<&'static str>>>,
) {
    init_context();
    let fired = Arc::new(std::sync::Mutex::new(Vec::new()));
    let record = |name: &'static str| {
        let fired = fired.clone();
        move || fired.lock().unwrap().push(name)
    };
    let items = vec![
        ContextMenuItem::new("A").on_click(record("A")),
        ContextMenuItem::separator(),
        ContextMenuItem::new("B").on_click(record("B")).disabled(),
        ContextMenuItem::new("C").on_click(record("C")),
    ];
    let chain = BlincContextState::get()
        .use_signal_keyed(&format!("keyed_menu_chain_{id}"), Vec::<String>::new);
    let menu = build_dropdown_menu_div(
        &items,
        160.0,
        OverlayHandle::from_raw(u64::MAX),
        id,
        &format!("{id}-key"),
        &chain,
        4.0,
        14.0,
        12.0,
    );
    let mut tree = blinc_layout::renderer::RenderTree::from_element(&menu);
    tree.compute_layout(400.0, 300.0);
    let mut router = blinc_layout::event_router::EventRouter::new();
    router.set_focus(tree.root());
    (tree, router, fired)
}

fn row(tree: &blinc_layout::renderer::RenderTree, index: usize) -> LayoutNodeId {
    tree.layout_tree.children(tree.root().unwrap())[index]
}

#[test]
fn arrows_focus_enabled_rows_and_wrap() {
    let _serial = serial();
    let (mut tree, mut router, _) = keyed_menu("keyed-menu-arrows");
    for (key, index) in [(40, 0), (40, 3), (40, 0), (38, 3), (36, 0), (35, 3)] {
        assert!(tree.handle_navigation_key(&mut router, key, false));
        assert_eq!(router.focused(), Some(row(&tree, index)));
    }
}

#[test]
fn up_from_panel_focuses_last_row() {
    let _serial = serial();
    let (mut tree, mut router, _) = keyed_menu("keyed-menu-up");
    tree.handle_navigation_key(&mut router, 38, false);
    assert_eq!(router.focused(), Some(row(&tree, 3)));
}

#[test]
fn activation_clicks_the_focused_row_once() {
    let _serial = serial();
    let (mut tree, mut router, fired) = keyed_menu("keyed-menu-enter");
    for key in [40, 13, 40, 32] {
        tree.handle_navigation_key(&mut router, key, false);
    }
    assert_eq!(*fired.lock().unwrap(), vec!["A", "C"]);
}
