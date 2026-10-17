//! The code editor's find bar: what is typed into its field reaches the
//! field, the search follows it, and the stylesheet's design lands on it.

use std::sync::Arc;

use blinc_core::Color;
use blinc_layout::div::ElementBuilder;
use blinc_layout::event_handler::EventContext;
use blinc_layout::event_router::{EventRouter, MouseButton};
use blinc_layout::renderer::{ElementType, RenderTree};
use blinc_layout::widgets::code::{
    CodeEditor, SharedCodeEditorState, code_editor, code_editor_state,
};
use std::sync::Mutex;

/// The overlay manager, focus statics and the reactive graph are
/// process-wide.
static LOCK: Mutex<()> = Mutex::new(());

fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        blinc_theme::ThemeState::init_default();
        if !blinc_core::BlincContextState::is_initialized() {
            blinc_core::BlincContextState::init(
                blinc_core::reactive::global_graph(),
                Arc::new(Mutex::new(blinc_core::context_state::HookState::new())),
                Arc::new(std::sync::atomic::AtomicBool::new(false)),
            );
        }
        if !blinc_layout::overlay_state::OverlayContext::is_initialized() {
            blinc_layout::overlay_state::OverlayContext::init(
                blinc_layout::widgets::overlay::overlay_manager(),
            );
        }
    });
}

/// Closes the find bar when a test ends, however it ends: the overlay
/// manager is shared by every test in the process.
struct CloseBar(SharedCodeEditorState);

impl Drop for CloseBar {
    fn drop(&mut self) {
        let handle = self
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .search_overlay_handle
            .take();
        if let Some(handle) = handle {
            let manager = blinc_layout::overlay_state::get_overlay_manager();
            let mut manager = manager.lock().unwrap_or_else(|e| e.into_inner());
            manager.close_immediate(handle);
            manager.cleanup();
        }
        blinc_layout::widgets::text_input::blur_all_text_inputs();
    }
}

fn texts(tree: &RenderTree) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![tree.root().unwrap()];
    while let Some(node) = stack.pop() {
        if let Some(ElementType::Text(t)) = tree.get_render_node(node).map(|r| &r.element_type) {
            out.push(t.content.clone());
        }
        stack.extend(tree.layout_tree.children(node));
    }
    out
}

/// An editor with the find bar open (Cmd+F), and the bar as its own tree.
fn open_bar(state: &SharedCodeEditorState) -> (CodeEditor, RenderTree) {
    init();
    blinc_layout::widgets::text_input::blur_all_text_inputs();
    blinc_layout::overlay_state::get_overlay_manager()
        .lock()
        .unwrap()
        .set_viewport(800.0, 400.0);
    let editor = code_editor(state).search_enabled(true);
    let mut tree = RenderTree::from_element(&editor);
    tree.compute_layout(600.0, 300.0);

    // The editor is focused, then Cmd+F opens the bar.
    state.lock().unwrap().focused = true;
    let mut find = EventContext::new(
        blinc_core::events::event_types::KEY_DOWN,
        tree.root().unwrap(),
    );
    find.key_code = 70;
    find.meta = true;
    ElementBuilder::event_handlers(&editor)
        .unwrap()
        .dispatch(&find);
    assert!(
        state.lock().unwrap().search_active,
        "Cmd+F did not open the find bar"
    );
    blinc_layout::widgets::text_input::process_pending_input_focus();
    (editor, bar_tree())
}

fn bar_tree() -> RenderTree {
    let layer = blinc_layout::overlay_state::get_overlay_manager()
        .lock()
        .unwrap()
        .build_overlay_layer();
    let mut bar = RenderTree::from_element(&layer);
    bar.compute_layout(800.0, 400.0);
    bar
}

fn frame(bar: &mut RenderTree) {
    let updates = blinc_layout::take_pending_partial_prop_updates();
    bar.apply_partial_property_updates(updates);
    bar.compute_layout(800.0, 400.0);
}

#[test]
fn typing_in_the_find_field_runs_the_search() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    init();
    let state = code_editor_state("let child = 1;\nlet other = child + 2;");
    let _close = CloseBar(Arc::clone(&state));
    let (_editor, mut bar) = open_bar(&state);
    assert!(
        texts(&bar).iter().any(|t| t == "Find..."),
        "the find bar is not in the overlay: {:?}",
        texts(&bar)
    );

    // Click the field, as the runner does: blur everything first.
    let field = {
        let mut stack = vec![bar.root().unwrap()];
        let mut found = None;
        while let Some(n) = stack.pop() {
            if let Some(ElementType::Text(t)) = bar.get_render_node(n).map(|r| &r.element_type) {
                if t.content == "Find..." {
                    found = Some(n);
                }
            }
            stack.extend(bar.layout_tree.children(n));
        }
        found.unwrap()
    };
    let b = bar.get_absolute_bounds(field).unwrap();
    let (x, y) = (b.x + 4.0, b.y + b.height / 2.0);
    blinc_layout::widgets::blur_all_text_inputs();
    let mut router = EventRouter::new();
    let mut events = router.on_mouse_move(&bar, x, y);
    events.extend(router.on_mouse_down(&bar, x, y, MouseButton::Left));
    events.extend(router.on_mouse_up(&bar, x, y, MouseButton::Left));
    for (node, event) in events {
        bar.dispatch_event(node, event, x, y);
    }

    for c in "child".chars() {
        bar.broadcast_text_input_event(c, false, false, false, false);
    }
    frame(&mut bar);
    let value = state
        .lock()
        .unwrap()
        .search_input_state
        .lock()
        .unwrap()
        .value
        .clone();
    assert_eq!(
        value, "child",
        "typed characters did not reach the find field"
    );
    assert_eq!(
        state.lock().unwrap().search_matches.len(),
        2,
        "typing did not run the search"
    );
    assert!(
        texts(&bar).contains(&"1 of 2".to_string()),
        "the count did not follow: {:?}",
        texts(&bar)
    );

    // Enter steps to the next match, Shift+Enter back.
    let key = blinc_core::events::event_types::KEY_DOWN;
    bar.broadcast_key_event(key, 13, false, false, false, false);
    frame(&mut bar);
    assert!(
        texts(&bar).contains(&"2 of 2".to_string()),
        "{:?}",
        texts(&bar)
    );
    bar.broadcast_key_event(key, 13, true, false, false, false);
    frame(&mut bar);
    assert!(
        texts(&bar).contains(&"1 of 2".to_string()),
        "{:?}",
        texts(&bar)
    );
}

/// The fill of the first node carrying `class`, if it has one.
fn fill(bar: &RenderTree, class: &str) -> Option<Color> {
    let registry = bar.element_registry();
    let mut stack = vec![bar.root().unwrap()];
    while let Some(n) = stack.pop() {
        if registry.has_class(n, class) {
            return match &bar.get_render_node(n).unwrap().props.background {
                Some(blinc_core::Brush::Solid(c)) => Some(*c),
                _ => None,
            };
        }
        stack.extend(bar.layout_tree.children(n));
    }
    panic!("no .{class} in the find bar");
}

fn near(a: Color, b: Color) -> bool {
    (a.r - b.r).abs() < 0.01
        && (a.g - b.g).abs() < 0.01
        && (a.b - b.b).abs() < 0.01
        && (a.a - b.a).abs() < 0.01
}

#[test]
fn the_stylesheet_designs_the_bar() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    init();
    let state = code_editor_state("let child = 1;");
    let _close = CloseBar(Arc::clone(&state));
    {
        let mut d = state.lock().unwrap();
        d.search_case_sensitive = true;
        d.search_query = "zzz".to_string();
        d.execute_search();
    }
    let (_editor, mut bar) = open_bar(&state);
    frame(&mut bar);

    let vars = blinc_theme::ThemeState::get().to_css_variable_map();
    let sheet = blinc_layout::css_parser::Stylesheet::parse_with_variables(
        blinc_layout::widgets::CODE_EDITOR_STYLES,
        &vars,
    )
    .expect("the stylesheet parses with the theme's variables");
    bar.set_stylesheet_arc(Arc::new(sheet));
    bar.apply_stylesheet_layout_overrides();
    bar.apply_stylesheet_base_styles();

    // `#3a3a3a`, the theme's elevated surface.
    let surface = Color::rgba(58.0 / 255.0, 58.0 / 255.0, 58.0 / 255.0, 1.0);
    assert!(
        near(fill(&bar, "blinc-find").unwrap(), surface),
        "the panel is not the elevated surface"
    );
    // The toggle that is on carries the accent's tint; the others none.
    let accent_tint = Color::rgba(10.0 / 255.0, 132.0 / 255.0, 1.0, 0.15);
    let on = fill(&bar, "blinc-find-toggle--on").expect("the case toggle is on");
    assert!(
        near(on, accent_tint),
        "the toggle's on state is not the accent tint: {on:?}"
    );
    // No results: the count carries its modifier.
    assert!(
        texts(&bar).contains(&"No results".to_string()),
        "{:?}",
        texts(&bar)
    );
    let _ = fill(&bar, "blinc-find-count--none");
}
