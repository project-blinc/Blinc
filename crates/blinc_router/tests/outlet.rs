//! `Router::outlet`: the current view is the outlet's one child, and a
//! navigation swaps it in place without touching what surrounds the outlet.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use blinc_core::reactive::{Signal, global_graph};
use blinc_layout::LayoutNodeId;
use blinc_layout::div::{Div, div};
use blinc_layout::renderer::RenderTree;
use blinc_router::{PageTransition, Route, RouteContext, Router, RouterBuilder, use_router};

/// The pending queues and the router's back-handler stack are process-wide.
static LOCK: Mutex<()> = Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        let scheduler = blinc_animation::AnimationScheduler::new();
        blinc_animation::set_global_scheduler(scheduler.handle());
        Box::leak(Box::new(scheduler));
        // What the app installs, so a navigation reaches statefuls too.
        blinc_core::reactive::set_stateful_deps_notifier(|ids| {
            blinc_layout::check_stateful_deps(ids);
        });
        if !blinc_core::BlincContextState::is_initialized() {
            blinc_core::BlincContextState::init(
                global_graph(),
                Arc::new(Mutex::new(blinc_core::context_state::HookState::new())),
                Arc::new(std::sync::atomic::AtomicBool::new(false)),
            );
        }
    });
    let guard = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let _ = blinc_layout::stateful::take_pending_subtree_rebuilds();
    let _ = blinc_layout::take_pending_partial_prop_updates();
    guard
}

static B_BUILDS: AtomicUsize = AtomicUsize::new(0);
static PATHS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static MADE: Mutex<Vec<Signal<u32>>> = Mutex::new(Vec::new());

fn home(_: RouteContext) -> Div {
    div().h(10.0)
}

fn page_b(_: RouteContext) -> Div {
    B_BUILDS.fetch_add(1, Ordering::SeqCst);
    div().h(20.0)
}

fn probe(ctx: RouteContext) -> Div {
    // The router in scope while a view builds is this one.
    PATHS.lock().unwrap().push(use_router().current_path());
    PATHS
        .lock()
        .unwrap()
        .push(ctx.params.get("id").unwrap_or("").to_string());
    div().h(30.0)
}

fn owner(_: RouteContext) -> Div {
    MADE.lock()
        .unwrap()
        .push(blinc_core::reactive::signal(7_u32));
    div().h(40.0)
}

fn router() -> Router {
    RouterBuilder::new()
        .route(Route::new("/").view(home))
        .route(Route::new("/b").view(page_b))
        .route(Route::new("/probe/:id").view(probe))
        .route(Route::new("/owner").view(owner))
        .route(
            Route::new("/fade")
                .view(page_b)
                .transition(PageTransition::fade()),
        )
        .initial("/")
        .build()
}

struct Scene {
    tree: RenderTree,
    header: LayoutNodeId,
    outlet: LayoutNodeId,
}

fn scene(router: &Router) -> Scene {
    let ui = div()
        .w(200.0)
        .flex_col()
        .child(div().h(5.0))
        .child(router.outlet());
    let mut tree = RenderTree::from_element(&ui);
    tree.compute_layout(200.0, 400.0);
    let kids = tree.layout_tree.children(tree.root().unwrap());
    Scene {
        header: kids[0],
        outlet: kids[1],
        tree,
    }
}

impl Scene {
    /// What a frame does: the rebuilds, then the property updates.
    fn frame(&mut self) {
        self.tree.process_pending_subtree_rebuilds();
        let updates = blinc_layout::take_pending_partial_prop_updates();
        self.tree.apply_partial_property_updates(updates);
        self.tree.compute_layout(200.0, 400.0);
    }

    fn views(&self) -> Vec<LayoutNodeId> {
        self.tree.layout_tree.children(self.outlet)
    }

    fn height(&self) -> f32 {
        let view = self.views();
        assert_eq!(view.len(), 1, "the outlet has one view");
        self.tree.get_absolute_bounds(view[0]).unwrap().height
    }
}

#[test]
fn the_outlet_shows_the_current_view() {
    let _g = serial();
    let s = scene(&router());
    assert_eq!(s.height(), 10.0);
}

#[test]
fn a_navigation_swaps_the_view_and_leaves_the_scaffold_alone() {
    let _g = serial();
    let router = router();
    let mut s = scene(&router);
    let old = s.views();

    router.push("/b");
    s.frame();

    assert_eq!(s.height(), 20.0);
    assert_ne!(s.views(), old, "the old view stayed");
    assert_eq!(
        s.tree.layout_tree.children(s.tree.root().unwrap()),
        vec![s.header, s.outlet],
        "the header or the outlet was rebuilt"
    );
}

#[test]
fn back_forward_and_replace_swap_the_view_too() {
    let _g = serial();
    let router = router();
    let mut s = scene(&router);

    router.push("/b");
    s.frame();
    assert_eq!(s.height(), 20.0);
    router.back();
    s.frame();
    assert_eq!(s.height(), 10.0);
    router.forward();
    s.frame();
    assert_eq!(s.height(), 20.0);
    router.replace("/");
    s.frame();
    assert_eq!(s.height(), 10.0);
}

#[test]
fn a_view_is_built_per_navigation_and_sees_its_router_and_params() {
    let _g = serial();
    PATHS.lock().unwrap().clear();
    let router = router();
    let mut s = scene(&router);

    router.push("/probe/42");
    s.frame();
    assert_eq!(s.height(), 30.0);
    assert_eq!(
        *PATHS.lock().unwrap(),
        vec!["/probe/42".to_string(), "42".to_string()]
    );

    // The same path again builds the view again.
    router.push("/probe/42");
    s.frame();
    assert_eq!(PATHS.lock().unwrap().len(), 4);
}

#[test]
fn a_view_is_built_once_while_it_stays() {
    let _g = serial();
    let router = router();
    let mut s = scene(&router);
    let before = B_BUILDS.load(Ordering::SeqCst);

    router.push("/b");
    s.frame();
    s.frame();
    s.frame();
    assert_eq!(B_BUILDS.load(Ordering::SeqCst), before + 1);
}

#[test]
fn a_page_transition_wraps_the_view_in_a_motion_container() {
    let _g = serial();
    let router = router();
    let mut s = scene(&router);
    router.push("/fade");
    s.frame();

    let host = s.views();
    assert_eq!(host.len(), 1);
    let motion = s.tree.layout_tree.children(host[0]);
    assert_eq!(motion.len(), 1, "a fit-sized host around the motion");
    let view = s.tree.layout_tree.children(motion[0]);
    assert_eq!(view.len(), 1, "the view inside the motion container");
    assert_eq!(s.tree.get_absolute_bounds(view[0]).unwrap().height, 20.0);
}

#[test]
fn two_outlets_of_one_router_both_follow() {
    let _g = serial();
    let router = router();
    let ui = div()
        .w(200.0)
        .flex_col()
        .child(router.outlet())
        .child(router.outlet());
    let mut tree = RenderTree::from_element(&ui);
    tree.compute_layout(200.0, 400.0);
    let outlets = tree.layout_tree.children(tree.root().unwrap());

    router.push("/b");
    tree.process_pending_subtree_rebuilds();
    let updates = blinc_layout::take_pending_partial_prop_updates();
    tree.apply_partial_property_updates(updates);
    tree.compute_layout(200.0, 400.0);

    for outlet in outlets {
        let view = tree.layout_tree.children(outlet);
        assert_eq!(view.len(), 1);
        assert_eq!(tree.get_absolute_bounds(view[0]).unwrap().height, 20.0);
    }
}

#[test]
fn a_navigation_queues_no_subtree_rebuild() {
    let _g = serial();
    let router = router();
    let mut s = scene(&router);

    router.push("/b");

    assert!(
        blinc_layout::stateful::take_pending_subtree_rebuilds().is_empty(),
        "the navigation asked for a subtree rebuild"
    );
    s.frame();
    assert_eq!(s.height(), 20.0);
}

#[test]
fn what_a_view_creates_goes_with_it() {
    let _g = serial();
    MADE.lock().unwrap().clear();
    let router = router();
    let mut s = scene(&router);

    router.push("/owner");
    s.frame();
    let made = MADE.lock().unwrap()[0];
    assert!(made.try_get().is_some());

    router.push("/");
    s.frame();
    assert!(
        made.try_get().is_none(),
        "a view's signal outlived its view"
    );
}
