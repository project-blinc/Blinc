//! A page transition's exit: the old page stays mounted, out of flow, while
//! its motion plays, and goes when the motion is done.
//!
//! The frame here does what the windowed runner does, in its order: process
//! the queued motion exits and sync the motion store, drain the property
//! updates, do the motion frame bookkeeping, then tick.

use std::sync::{Arc, Mutex};

use blinc_animation::AnimationScheduler;
use blinc_core::BlincContextState;
use blinc_core::reactive::global_graph;
use blinc_layout::LayoutNodeId;
use blinc_layout::div::{Div, div};
use blinc_layout::render_state::{RenderState, create_shared_motion_states};
use blinc_layout::renderer::RenderTree;
use blinc_router::{PageTransition, Route, RouteContext, Router, RouterBuilder};

static LOCK: Mutex<()> = Mutex::new(());

fn home(_: RouteContext) -> Div {
    div().w(100.0).h(10.0)
}

fn page_a(_: RouteContext) -> Div {
    div().w(100.0).h(20.0)
}

fn page_b(_: RouteContext) -> Div {
    div().w(100.0).h(30.0)
}

fn router() -> Router {
    RouterBuilder::new()
        .route(Route::new("/").view(home))
        .route(
            Route::new("/a")
                .view(page_a)
                .transition(PageTransition::fade()),
        )
        .route(
            Route::new("/b")
                .view(page_b)
                .transition(PageTransition::fade()),
        )
        .route(
            Route::new("/none")
                .view(page_b)
                .transition(PageTransition::none()),
        )
        .initial("/")
        .build()
}

struct Runner {
    tree: RenderTree,
    rs: RenderState,
    outlet: LayoutNodeId,
    now_ms: u64,
    _guard: std::sync::MutexGuard<'static, ()>,
}

fn runner(router: &Router) -> Runner {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        let scheduler = AnimationScheduler::new();
        blinc_animation::set_global_scheduler(scheduler.handle());
        Box::leak(Box::new(scheduler));
        if !BlincContextState::is_initialized() {
            BlincContextState::init(
                global_graph(),
                Arc::new(Mutex::new(blinc_core::context_state::HookState::new())),
                Arc::new(std::sync::atomic::AtomicBool::new(false)),
            );
        }
    });
    let guard = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let _ = blinc_layout::stateful::take_pending_subtree_rebuilds();
    let _ = blinc_layout::take_pending_partial_prop_updates();
    let _ = blinc_layout::render_state::take_global_motion_exit_starts();

    let shared = create_shared_motion_states();
    let for_callback = Arc::clone(&shared);
    BlincContextState::get().set_motion_state_callback(Arc::new(move |key: &str| {
        for_callback
            .read()
            .ok()
            .and_then(|states| states.get(key).copied())
            .unwrap_or(blinc_core::MotionAnimationState::NotFound)
    }));
    let animations = Arc::new(Mutex::new(AnimationScheduler::new()));
    let mut rs = RenderState::new(animations);
    rs.set_shared_motion_states(shared);

    let ui = div()
        .w(200.0)
        .flex_col()
        .child(div().h(5.0))
        .child(router.outlet());
    let mut tree = RenderTree::from_element(&ui);
    tree.compute_layout(200.0, 400.0);
    let kids = tree.layout_tree.children(tree.root().unwrap());
    let mut r = Runner {
        outlet: kids[1],
        tree,
        rs,
        now_ms: 0,
        _guard: guard,
    };
    r.frame();
    r
}

impl Runner {
    /// One frame, 16ms after the last.
    fn frame(&mut self) {
        self.now_ms += 16;
        self.rs.process_global_motion_exit_cancels();
        self.rs.process_global_motion_exit_starts();
        self.rs.process_global_motion_starts();
        self.rs.sync_shared_motion_states();

        self.tree.process_pending_subtree_rebuilds();
        let updates = blinc_layout::take_pending_partial_prop_updates();
        self.tree.apply_partial_property_updates(updates);
        self.tree.compute_layout(200.0, 400.0);

        self.rs.begin_stable_motion_frame();
        self.tree.initialize_motion_animations(&mut self.rs);
        self.rs.end_stable_motion_frame();
        self.rs.process_global_motion_replays();

        self.rs.process_global_motion_exit_cancels();
        self.rs.process_global_motion_exit_starts();
        self.rs.process_global_motion_starts();
        self.rs.tick(self.now_ms);
        self.rs.sync_shared_motion_states();
    }

    fn pages(&self) -> Vec<LayoutNodeId> {
        self.tree.layout_tree.children(self.outlet)
    }

    fn x(&self, node: LayoutNodeId) -> f32 {
        self.tree.get_absolute_bounds(node).unwrap().x
    }

    /// Frames for as long as an enter animation takes, and a little more.
    fn settle(&mut self) {
        for _ in 0..30 {
            self.frame();
        }
    }

    /// Frames until the outlet has one page again, or `None` if it does not
    /// within `limit_ms`.
    fn until_one_page(&mut self, limit_ms: u64) -> Option<u64> {
        let start = self.now_ms;
        while self.now_ms - start <= limit_ms {
            self.frame();
            if self.pages().len() == 1 {
                return Some(self.now_ms - start);
            }
        }
        None
    }
}

#[test]
fn the_old_page_stays_out_of_flow_while_it_plays_its_exit_and_then_goes() {
    let router = router();
    let mut r = runner(&router);
    router.push("/a");
    r.settle();
    assert_eq!(r.pages().len(), 1);
    let old = r.pages()[0];

    router.push("/b");
    r.frame();
    let both = r.pages();
    assert_eq!(both.len(), 2, "the old page did not stay for its exit");
    assert_eq!(both[0], old);
    let new = both[1];
    assert_eq!(
        r.x(new),
        r.x(old),
        "the new page is pushed aside by the old one"
    );

    let took = r.until_one_page(2000).expect("the old page never left");
    assert!(
        took >= 100,
        "the old page left after {took}ms, before its exit could play"
    );
    assert_eq!(r.pages(), vec![new], "the new page was rebuilt");
}

#[test]
fn a_page_whose_transition_has_no_duration_goes_without_waiting() {
    let router = router();
    let mut r = runner(&router);
    router.push("/none");
    r.settle();

    router.push("/");
    r.frame();
    let took = r.until_one_page(500).expect("the page never left");
    assert!(took <= 64, "a zero-length exit took {took}ms");
}

#[test]
fn a_page_without_a_transition_goes_at_once() {
    let router = router();
    let mut r = runner(&router);
    router.push("/a");
    r.settle();
    router.push("/");
    r.settle();

    router.push("/b");
    r.frame();
    assert_eq!(r.pages().len(), 1, "a page with no exit waited for one");
}

#[test]
fn navigating_again_during_an_exit_still_ends_with_one_page() {
    let router = router();
    let mut r = runner(&router);
    router.push("/a");
    r.frame();
    router.push("/b");
    r.frame();
    router.push("/a");
    r.frame();
    router.push("/");
    r.frame();

    r.until_one_page(3000).expect("pages are left behind");
    let page = r.pages()[0];
    assert_eq!(
        r.tree.get_absolute_bounds(page).unwrap().height,
        10.0,
        "the last page is not the one showing"
    );
}

#[test]
fn motions_do_not_pile_up_over_many_navigations() {
    let router = router();
    let mut r = runner(&router);
    for i in 0..12 {
        router.push(if i % 2 == 0 { "/a" } else { "/b" });
        for _ in 0..30 {
            r.frame();
        }
    }
    r.settle();

    assert_eq!(r.pages().len(), 1);
    assert!(
        r.rs.stable_motion_count() <= 2,
        "{} motions are kept for one page",
        r.rs.stable_motion_count()
    );
}
