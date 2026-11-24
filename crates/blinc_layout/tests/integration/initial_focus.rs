//! Initial and requested focus in modal scopes.

use blinc_core::context_state::{BlincContextState, HookState};
use blinc_layout::div::{Div, div};
use blinc_layout::event_router::EventRouter;
use blinc_layout::renderer::RenderTree;
use blinc_layout::selector::ElementHandle;
use blinc_layout::widgets::overlay::OVERLAY_LAYER_ID;
use std::sync::{
    Arc, Mutex, Once,
    atomic::{AtomicBool, Ordering},
};

static INIT: Once = Once::new();
static LOCK: Mutex<()> = Mutex::new(());

fn init() {
    INIT.call_once(|| {
        BlincContextState::init(
            blinc_core::reactive::global_graph(),
            Arc::new(Mutex::new(HookState::new())),
            Arc::new(AtomicBool::new(false)),
        );
        blinc_theme::ThemeState::init_default();
    });
    BlincContextState::get().set_focus_callback(Arc::new(|_| {}));
    blinc_layout::widgets::text_input::blur_all_text_inputs();
    BlincContextState::get().set_focus(None);
    let mut empty = RenderTree::from_element(&div().size(400.0, 300.0));
    empty.compute_layout(400.0, 300.0);
    empty.sync_keyboard_focus(&mut EventRouter::new());
    BlincContextState::get()
        .dirty_flag()
        .store(false, Ordering::SeqCst);
}

fn control(id: &str) -> Div {
    div().id(id).size(80.0, 30.0).attr("tabindex", "0")
}

fn modal() -> Div {
    div()
        .id("modal")
        .size(300.0, 180.0)
        .attr("data-focus-scope", "dialog")
        .attr("data-focus-modal", "true")
        .attr("tabindex", "-1")
}

fn scene(dialog: Div) -> (RenderTree, EventRouter) {
    let ui = div()
        .size(400.0, 300.0)
        .child(control("outside"))
        .child(div().id(OVERLAY_LAYER_ID).size(400.0, 300.0).child(dialog));
    let mut tree = RenderTree::from_element(&ui);
    tree.compute_layout(400.0, 300.0);
    (tree, EventRouter::new())
}

fn focused(tree: &RenderTree, router: &EventRouter, id: &str) {
    assert_eq!(
        router.focused(),
        tree.query_by_id(id),
        "expected focus on {id}"
    );
}

#[test]
fn autofocus_prefers_the_marked_control_once() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    init();
    let (mut tree, mut router) = scene(
        modal()
            .child(control("first"))
            .child(control("safe").attr("autofocus", "")),
    );
    tree.sync_keyboard_focus(&mut router);
    focused(&tree, &router, "safe");
    tree.handle_navigation_key(&mut router, 9, true);
    focused(&tree, &router, "first");
    tree.sync_keyboard_focus(&mut router);
    focused(&tree, &router, "first");
}

#[test]
fn named_initial_focus_takes_priority_over_autofocus() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    init();
    let (mut tree, mut router) = scene(
        modal()
            .attr("data-initial-focus", "safe")
            .child(control("first").attr("autofocus", ""))
            .child(control("safe")),
    );
    tree.sync_keyboard_focus(&mut router);
    focused(&tree, &router, "safe");
}

#[test]
fn invalid_targets_fall_back_to_an_enabled_control_in_the_scope() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for target in ["missing", "outside", "disabled", "hidden"] {
        init();
        let (mut tree, mut router) = scene(
            modal()
                .attr("data-initial-focus", target)
                .child(
                    control("disabled")
                        .attr("disabled", "")
                        .attr("autofocus", ""),
                )
                .child(control("hidden").visible(false).attr("autofocus", ""))
                .child(control("safe")),
        );
        tree.sync_keyboard_focus(&mut router);
        focused(&tree, &router, "safe");
    }
}

#[test]
fn a_focus_request_is_honored_when_the_modal_opens() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    init();
    let (mut tree, mut router) = scene(modal().child(control("first")).child(control("safe")));
    let events = Arc::new(Mutex::new(Vec::new()));
    let seen = events.clone();
    router.set_event_callback(move |node, event| {
        if event == blinc_core::events::event_types::FOCUS {
            seen.lock().unwrap().push(node);
        }
    });
    BlincContextState::get().set_focus(Some("safe"));
    assert!(blinc_layout::stateful::peek_needs_redraw());
    assert!(blinc_layout::stateful::take_needs_redraw());
    assert!(BlincContextState::get().has_pending_focus());
    tree.sync_keyboard_focus(&mut router);
    focused(&tree, &router, "safe");
    assert_eq!(*events.lock().unwrap(), [tree.query_by_id("safe").unwrap()]);
    assert!(!BlincContextState::get().has_pending_focus());
    assert!(
        !BlincContextState::get().dirty_flag().load(Ordering::SeqCst),
        "focus requested a root rebuild"
    );
}

#[test]
fn element_handle_focus_and_blur_reach_the_router() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    init();
    let (mut tree, mut router) = scene(modal().child(control("first")).child(control("safe")));
    tree.sync_keyboard_focus(&mut router);
    let handle: ElementHandle =
        ElementHandle::new("safe".to_string(), tree.element_registry().clone());
    handle.focus();
    tree.sync_keyboard_focus(&mut router);
    focused(&tree, &router, "safe");
    assert!(handle.is_focused());
    handle.blur();
    tree.sync_keyboard_focus(&mut router);
    assert_eq!(router.focused(), None);
    assert!(!handle.is_focused());
}

#[test]
fn a_component_id_resolves_to_its_only_focusable_control() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    init();
    let (mut tree, mut router) = scene(
        modal()
            .attr("data-initial-focus", "field")
            .child(control("first"))
            .child(div().id("field").child(control("editable"))),
    );
    tree.sync_keyboard_focus(&mut router);
    focused(&tree, &router, "editable");
    let handle: ElementHandle = ElementHandle::new("field", tree.element_registry().clone());
    handle.focus();
    tree.sync_keyboard_focus(&mut router);
    focused(&tree, &router, "editable");
    assert!(handle.is_focused());
    handle.blur();
    tree.sync_keyboard_focus(&mut router);
    assert_eq!(router.focused(), None);
}

#[test]
fn an_ambiguous_component_id_keeps_the_fallback_control() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    init();
    let (mut tree, mut router) = scene(
        modal()
            .attr("data-initial-focus", "fields")
            .child(control("safe"))
            .child(
                div()
                    .id("fields")
                    .child(control("first"))
                    .child(control("second")),
            ),
    );
    tree.sync_keyboard_focus(&mut router);
    focused(&tree, &router, "safe");
    BlincContextState::get().set_focus(Some("fields"));
    tree.sync_keyboard_focus(&mut router);
    focused(&tree, &router, "safe");
    assert!(BlincContextState::get().is_focused("safe"));
}

#[test]
fn a_request_cannot_take_focus_outside_the_open_modal() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    init();
    let (mut tree, mut router) = scene(modal().child(control("safe")));
    tree.sync_keyboard_focus(&mut router);
    BlincContextState::get().set_focus(Some("outside"));
    tree.sync_keyboard_focus(&mut router);
    focused(&tree, &router, "safe");
    assert!(BlincContextState::get().is_focused("safe"));
}

#[test]
fn an_empty_modal_can_focus_its_root() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    init();
    let (mut tree, mut router) = scene(modal().attr("data-initial-focus", "missing"));
    tree.sync_keyboard_focus(&mut router);
    focused(&tree, &router, "modal");
}

#[test]
fn a_request_takes_priority_over_the_declarative_target() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    init();
    let (mut tree, mut router) = scene(
        modal()
            .attr("data-initial-focus", "first")
            .child(control("first"))
            .child(control("safe")),
    );
    BlincContextState::get().set_focus(Some("safe"));
    tree.sync_keyboard_focus(&mut router);
    focused(&tree, &router, "safe");
}

#[test]
fn nested_scopes_choose_their_own_initial_targets() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    init();
    let nested = div()
        .id("nested")
        .size(160.0, 80.0)
        .attr("data-focus-scope", "nested-dialog")
        .attr("data-focus-modal", "true")
        .attr("tabindex", "-1")
        .child(control("inner").autofocus());
    let (mut tree, mut router) = scene(modal().child(control("outer").autofocus()).child(nested));
    let events = Arc::new(Mutex::new(Vec::new()));
    let seen = events.clone();
    router.set_event_callback(move |node, event| {
        if event == blinc_core::events::event_types::FOCUS {
            seen.lock().unwrap().push(node);
        }
    });
    tree.sync_keyboard_focus(&mut router);
    focused(&tree, &router, "inner");
    assert_eq!(
        *events.lock().unwrap(),
        [
            tree.query_by_id("outer").unwrap(),
            tree.query_by_id("inner").unwrap()
        ]
    );
    tree.handle_navigation_key(&mut router, 9, false);
    focused(&tree, &router, "inner");
}

#[test]
fn callback_requests_are_reentrant_and_the_latest_request_wins() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    init();
    let (mut tree, mut router) = scene(modal().child(control("first")).child(control("safe")));
    BlincContextState::get().set_focus_callback(Arc::new(|id| {
        assert!(BlincContextState::get().has_pending_focus());
        if id == Some("first") {
            BlincContextState::get().set_focus(Some("safe"));
        }
    }));
    BlincContextState::get().set_focus(Some("first"));
    tree.sync_keyboard_focus(&mut router);
    focused(&tree, &router, "safe");
    assert!(!BlincContextState::get().has_pending_focus());
}

#[test]
fn invalid_programmatic_targets_restore_the_reported_focus() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    init();
    let (mut tree, mut router) = scene(
        modal()
            .child(control("safe"))
            .child(control("disabled").attr("disabled", "")),
    );
    tree.sync_keyboard_focus(&mut router);
    for id in ["missing", "disabled", "outside"] {
        BlincContextState::get().set_focus(Some(id));
        tree.sync_keyboard_focus(&mut router);
        focused(&tree, &router, "safe");
        assert!(BlincContextState::get().is_focused("safe"));
        assert!(!BlincContextState::get().has_pending_focus());
    }
}

#[test]
fn a_named_text_area_receives_and_releases_editable_focus() {
    use blinc_layout::widgets::text_area::{text_area, text_area_state};
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    init();
    let state = text_area_state();
    let (mut tree, mut router) = scene(
        modal()
            .attr("data-initial-focus", "notes")
            .child(control("first"))
            .child(text_area(&state).id("notes").rows(3)),
    );
    tree.sync_keyboard_focus(&mut router);
    focused(&tree, &router, "notes");
    assert!(state.lock().unwrap().is_focused());
    BlincContextState::get().set_focus(None);
    tree.sync_keyboard_focus(&mut router);
    assert_eq!(router.focused(), None);
    assert!(!state.lock().unwrap().is_focused());
}
