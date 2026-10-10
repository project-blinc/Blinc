//! Cross-platform routing for Blinc
//!
//! Declarative routing with path matching, navigation history,
//! and guards. Routers are scoped — not global singletons.
//!
//! # Example
//!
//! ```ignore
//! use blinc_router::{RouterBuilder, Route};
//!
//! // Build the router (returns a clonable handle)
//! let router = RouterBuilder::new()
//!     .route(Route::new("/").name("home").view(home_page))
//!     .route(Route::new("/users/:id").view(user_detail))
//!     .not_found(not_found_page)
//!     .build();
//!
//! // Navigate
//! router.push("/users/42");
//! router.back();
//!
//! // Build current route's view
//! let page = router.outlet(); // returns Div
//! ```

// `clippy::missing_const_for_thread_local` mis-fires on nightly clippy
// 0.1.96+ when the initializer is already wrapped in `const { ... }`.
// See `blinc_layout::motion` for the same workaround.
#![allow(clippy::missing_const_for_thread_local)]

pub mod history;
pub mod route;
pub mod transition;

use std::sync::{Arc, Mutex};

use blinc_core::context_state::BlincContextState;
use blinc_core::reactive::{ReactiveGraph, Signal, computed};
use history::RouterHistory;
use route::RouteTrie;

/// Route view function: receives context, returns a Div
pub type RouteView = fn(RouteContext) -> blinc_layout::div::Div;

/// Navigation guard
pub type NavigationGuard = Arc<dyn Fn(&HistoryEntry, &MatchedRoute) -> GuardResult + Send + Sync>;

/// Result of a navigation guard check
pub enum GuardResult {
    Allow,
    Redirect(String),
    Reject(String),
}

/// Internal router state
struct RouterInner {
    trie: RouteTrie,
    views: Vec<RouteView>,
    guards: Vec<NavigationGuard>,
    history: RouterHistory,
    current_match: Option<MatchedRoute>,
    named_routes: rustc_hash::FxHashMap<String, String>,
    /// Animation suspension scopes keyed by route path
    route_scopes: rustc_hash::FxHashMap<String, blinc_animation::suspension::ScopeId>,
    /// The currently active route path (for suspension tracking)
    active_route_path: Option<String>,
    /// Monotonic counter bumped on every successful navigation
    /// (`push`/`replace`/`back`/`forward`). Each `outlet()` follows it:
    /// bumping it builds the new view as a row of the outlet and takes
    /// the old one away, leaving the surrounding tree alone.
    ///
    /// Lazy-created — the reactive graph only exists after
    /// `BlincContextState::init`, which the windowed runner does
    /// before invoking the user's UI builder. So the signal gets
    /// created on the first `outlet()` call from inside the build
    /// closure; navigations that happen before that (e.g. an initial
    /// path set inside `RouterBuilder::build`) are picked up by the
    /// first build naturally because they wrote `current_match`
    /// directly.
    route_version_signal: Option<Signal<u32>>,
}

/// A router instance. Clone to share across closures.
///
/// Not a global singleton — create one per navigation scope.
/// Pass it through your UI builder or store it in context state.
#[derive(Clone)]
pub struct Router {
    inner: Arc<Mutex<RouterInner>>,
}

impl Router {
    /// Lazily create (and return) the route-version signal.
    ///
    /// Returns `None` when called before `BlincContextState::init` —
    /// the reactive graph only exists inside the build closure, so
    /// callers must tolerate the early-build path having no signal
    /// yet. `outlet()` registers the dep on the signal only when
    /// available; the initial frame's `current_match` is wired
    /// through directly.
    fn ensure_route_signal(&self) -> Option<Signal<u32>> {
        if let Some(sig) = self.inner.lock().unwrap().route_version_signal {
            return Some(sig);
        }
        let ctx = BlincContextState::try_get()?;
        let sig = ctx.use_signal(0u32);
        let mut state = self.inner.lock().unwrap();
        // Lost the race? Keep the first-created signal so anyone
        // already subscribed stays connected.
        if let Some(existing) = state.route_version_signal {
            return Some(existing);
        }
        state.route_version_signal = Some(sig);
        Some(sig)
    }

    /// Bump the route-version signal, which every `outlet()` follows.
    /// Called from every state-mutating navigation method after the
    /// inner lock is released.
    fn notify_route_change(&self) {
        let Some(sig) = self.ensure_route_signal() else {
            // Pre-init navigation — first build will pick up
            // `current_match` directly.
            return;
        };
        let Some(ctx) = BlincContextState::try_get() else {
            return;
        };
        let cur = ctx.get_signal(sig).unwrap_or(0);
        sig.set(cur.wrapping_add(1));
    }

    /// Navigate to a path
    pub fn push(&self, path: impl Into<String>) {
        let path = path.into();
        let mut state = self.inner.lock().unwrap();

        let matched = state.trie.match_path(&path);

        // Run guards
        if let Some(ref m) = matched {
            for guard in &state.guards {
                match guard(&state.history.current, m) {
                    GuardResult::Allow => {}
                    GuardResult::Redirect(redirect_path) => {
                        drop(state);
                        self.push(redirect_path);
                        return;
                    }
                    GuardResult::Reject(reason) => {
                        tracing::warn!("Navigation to '{}' rejected: {}", path, reason);
                        return;
                    }
                }
            }
        }

        let entry = HistoryEntry {
            path: path.clone(),
            params: matched
                .as_ref()
                .map(|m| m.params.clone())
                .unwrap_or_default(),
            query: matched
                .as_ref()
                .map(|m| m.query.clone())
                .unwrap_or_default(),
            title: None,
        };
        state.history.push(entry);
        state.current_match = matched;
        drop(state);
        self.notify_route_change();
    }

    /// Replace current route (no history entry)
    pub fn replace(&self, path: impl Into<String>) {
        let path = path.into();
        let mut state = self.inner.lock().unwrap();
        let matched = state.trie.match_path(&path);

        let entry = HistoryEntry {
            path: path.clone(),
            params: matched
                .as_ref()
                .map(|m| m.params.clone())
                .unwrap_or_default(),
            query: matched
                .as_ref()
                .map(|m| m.query.clone())
                .unwrap_or_default(),
            title: None,
        };
        state.history.replace(entry);
        state.current_match = matched;
        drop(state);
        self.notify_route_change();
    }

    /// Go back
    pub fn back(&self) {
        let mut state = self.inner.lock().unwrap();
        let moved = if let Some(entry) = state.history.back() {
            let path = entry.path.clone();
            state.current_match = state.trie.match_path(&path);
            true
        } else {
            false
        };
        drop(state);
        if moved {
            self.notify_route_change();
        }
    }

    /// Go forward
    pub fn forward(&self) {
        let mut state = self.inner.lock().unwrap();
        let moved = if let Some(entry) = state.history.forward() {
            let path = entry.path.clone();
            state.current_match = state.trie.match_path(&path);
            true
        } else {
            false
        };
        drop(state);
        if moved {
            self.notify_route_change();
        }
    }

    pub fn can_go_back(&self) -> bool {
        self.inner.lock().unwrap().history.can_go_back()
    }

    pub fn can_go_forward(&self) -> bool {
        self.inner.lock().unwrap().history.can_go_forward()
    }

    /// Get the current path
    pub fn current_path(&self) -> String {
        self.inner.lock().unwrap().history.current.path.clone()
    }

    /// Get the current matched route
    pub fn current_route(&self) -> Option<MatchedRoute> {
        self.inner.lock().unwrap().current_match.clone()
    }

    /// Get current route parameters
    pub fn params(&self) -> RouteParams {
        self.current_route().map(|r| r.params).unwrap_or_default()
    }

    /// Get current query parameters
    pub fn query(&self) -> QueryParams {
        self.current_route().map(|r| r.query).unwrap_or_default()
    }

    /// Check if a path matches any registered route
    pub fn has_route(&self, path: &str) -> bool {
        let state = self.inner.lock().unwrap();
        state.trie.match_path(path).is_some()
    }

    /// Get the path template for a named route
    pub fn path_for(&self, name: &str) -> Option<String> {
        self.inner.lock().unwrap().named_routes.get(name).cloned()
    }

    /// Navigate to a named route with parameters
    pub fn push_named(&self, name: &str, params: &[(&str, &str)]) {
        if let Some(template) = self.path_for(name) {
            let mut path = template;
            for (key, value) in params {
                path = path.replace(&format!(":{}", key), value);
            }
            self.push(path);
        } else {
            tracing::warn!("Named route '{}' not found", name);
        }
    }

    /// Handle a deep link URI from the platform.
    ///
    /// Parses the URI and navigates to the extracted path.
    /// Call this from the platform runner when receiving a deep link.
    pub fn handle_deep_link(&self, uri: &str) {
        use blinc_platform::deep_link::{DeepLink, DeepLinkSource};
        if let Some(dl) = DeepLink::parse(uri, DeepLinkSource::System) {
            let path = dl.route_path();
            tracing::info!("Deep link: {} → {}", uri, path);
            self.push(path);
        } else {
            tracing::warn!("Failed to parse deep link URI: {}", uri);
        }
    }

    /// Register this router as the handler for the system back button.
    ///
    /// When back is pressed and the router can go back, it navigates back
    /// and consumes the event. Otherwise, the event propagates (app exit).
    pub fn register_back_handler(&self) -> blinc_layout::back_handler::BackHandlerHandle {
        let router = self.clone();
        blinc_layout::back_handler::push_back_handler(move || {
            if router.can_go_back() {
                router.back();
                true // consumed
            } else {
                false // let app handle (exit)
            }
        })
    }

    /// Build the current route's view directly, without reactive
    /// wrapping. Pulled out of `outlet()` so the latter can build it
    /// again on every route change without duplicating the
    /// suspension-scope bookkeeping.
    fn build_current_view(&self) -> blinc_layout::div::Div {
        let view_and_ctx = {
            let state = self.inner.lock().unwrap();
            state.current_match.as_ref().and_then(|matched| {
                state.views.get(matched.view_index).map(|view| {
                    let ctx = RouteContext {
                        params: matched.params.clone(),
                        query: matched.query.clone(),
                        path: matched.path.clone(),
                        router: self.clone(),
                    };
                    let transition = matched.transition.clone();
                    (*view, ctx, transition)
                })
            })
        };

        if let Some((view, ctx, transition)) = view_and_ctx {
            let route_path = ctx.path.clone();

            // Manage animation suspension scopes
            {
                let mut state = self.inner.lock().unwrap();
                let handle = blinc_animation::get_scheduler();

                // Suspend the old route's animations
                if let Some(ref old_path) = state.active_route_path {
                    if *old_path != route_path {
                        if let Some(&scope) = state.route_scopes.get(old_path) {
                            blinc_animation::suspension::suspend_scope(scope, &handle);
                        }
                    }
                }

                // Get or create scope for the new route
                let scope = *state
                    .route_scopes
                    .entry(route_path.clone())
                    .or_insert_with(blinc_animation::suspension::create_scope);

                // Resume this route's animations (if previously suspended)
                blinc_animation::suspension::resume_scope(scope, &handle);

                // Enter the scope so new AnimatedValues register here
                blinc_animation::suspension::enter_scope(scope);
                state.active_route_path = Some(route_path);
            }

            push_router_context(self);
            let result = view(ctx);
            pop_router_context();

            // Exit the scope
            blinc_animation::suspension::exit_scope();

            // Wrap in a motion container when the route declares a
            // `PageTransition` so the page actually animates on
            // appear. The outlet builds a fresh row per navigation, so
            // the new view's motion container is freshly minted on every
            // route change and its `enter_animation` plays.
            //
            // `.transient()` is critical here: without it, motion
            // defaults to `use_stable_key=true` and reuses the same
            // `RenderState::stable_motions` entry on every navigation,
            // and the state machine accumulates partial transitions
            // across them (visible as ghosted / half-scaled glyphs after
            // a few navigations). With `transient`, motion state is
            // keyed by the freshly allocated `LayoutNodeId` and cleaned
            // up on subtree removal.
            //
            // The `exit_animation` is set for completeness but does not
            // play: the outlet removes the outgoing view at once, and
            // exit playback needs a stable-keyed motion it can address.
            if let Some(transition) = transition {
                // The host `Div` + motion wrapper must size to fit
                // their child (the view), not collapse to zero and
                // not expand to fill — `router.outlet()` is most
                // often parked inside a centered flex parent (see
                // the GH #35 reproducer) and we want the wrapped
                // page to occupy exactly the space the bare view
                // would have. `w_fit().h_fit()` matches the natural
                // sizing of the view returned by the route closure.
                blinc_layout::div::div().w_fit().h_fit().child(
                    blinc_layout::motion::motion()
                        .transient()
                        .enter_animation(transition.enter)
                        .exit_animation(transition.exit)
                        .child(result),
                )
            } else {
                result
            }
        } else {
            blinc_layout::div::div()
        }
    }

    /// Build the current route's view.
    ///
    /// Returns a `Div` whose one child is the current view. `push`/
    /// `replace`/`back`/`forward` bump the router's route-version
    /// signal; the outlet then builds the new view and takes the old one
    /// away, in place, and the surrounding scaffold (header, nav chrome,
    /// etc.) is left untouched (GH #35). Each navigation builds the view
    /// again, even to the same path.
    ///
    /// The outer `Div` is unstyled and keeps the return type chainable
    /// (`router.outlet().flex_grow()`).
    pub fn outlet(&self) -> blinc_layout::div::Div {
        let host = blinc_layout::div::div();
        let Some(sig) = self.ensure_route_signal() else {
            // Before the reactive graph exists there is nothing to follow,
            // and the view is built once.
            return host.child(self.build_current_view());
        };
        let router = self.clone();
        let version = computed(move |g: &ReactiveGraph| vec![g.get(sig).unwrap_or(0)]);
        host.for_each(
            &version,
            |version: &u32| *version,
            move |_| router.build_current_view(),
        )
    }
}

/// Router builder
pub struct RouterBuilder {
    routes: Vec<Route>,
    not_found: Option<RouteView>,
    guards: Vec<NavigationGuard>,
    initial_path: String,
}

impl RouterBuilder {
    pub fn new() -> Self {
        Self {
            routes: Vec::new(),
            not_found: None,
            guards: Vec::new(),
            initial_path: "/".to_string(),
        }
    }

    pub fn route(mut self, route: Route) -> Self {
        self.routes.push(route);
        self
    }

    pub fn not_found(mut self, view: RouteView) -> Self {
        self.not_found = Some(view);
        self
    }

    pub fn guard(mut self, guard: NavigationGuard) -> Self {
        self.guards.push(guard);
        self
    }

    pub fn initial(mut self, path: impl Into<String>) -> Self {
        self.initial_path = path.into();
        self
    }

    /// Build the router. Returns a clonable Router handle.
    pub fn build(self) -> Router {
        let mut trie = RouteTrie::new();
        let mut views: Vec<RouteView> = Vec::new();
        let mut named_routes = rustc_hash::FxHashMap::default();

        fn register_routes(
            trie: &mut RouteTrie,
            views: &mut Vec<RouteView>,
            named: &mut rustc_hash::FxHashMap<String, String>,
            routes: &[Route],
            prefix: &str,
        ) {
            for route in routes {
                let full_path = if prefix == "/" {
                    route.path.clone()
                } else {
                    format!("{}{}", prefix, route.path)
                };

                if let Some(view) = route.view {
                    let idx = views.len();
                    views.push(view);
                    trie.add(
                        &full_path,
                        idx,
                        route.name.as_deref(),
                        route.transition.clone(),
                    );

                    if let Some(ref name) = route.name {
                        named.insert(name.clone(), full_path.clone());
                    }
                }

                if !route.children.is_empty() {
                    register_routes(trie, views, named, &route.children, &full_path);
                }
            }
        }

        register_routes(&mut trie, &mut views, &mut named_routes, &self.routes, "/");

        if let Some(nf_view) = self.not_found {
            let idx = views.len();
            views.push(nf_view);
            trie.set_not_found(idx);
        }

        let initial_match = trie.match_path(&self.initial_path);

        let router = Router {
            inner: Arc::new(Mutex::new(RouterInner {
                trie,
                views,
                guards: self.guards,
                history: RouterHistory::new(&self.initial_path),
                current_match: initial_match,
                named_routes,
                route_scopes: rustc_hash::FxHashMap::default(),
                active_route_path: None,
                route_version_signal: None,
            })),
        };

        // Auto-register deep link handler so platforms dispatch to this router
        {
            let r = router.clone();
            *DEEP_LINK_HANDLER.lock().unwrap() = Some(Box::new(move |uri| {
                r.handle_deep_link(uri);
            }));
        }

        // Auto-register back button handler
        router.register_back_handler();

        // Handle CLI deep link if present (desktop)
        if let Some(uri) = cli_deep_link() {
            router.handle_deep_link(&uri);
        }

        router
    }
}

impl Default for RouterBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// Re-export key types
pub use history::HistoryEntry;
pub use route::{MatchedRoute, QueryParams, Route, RouteContext, RouteParams};
pub use transition::PageTransition;

// ============================================================================
// Global deep link dispatch
// ============================================================================

/// Global deep link callback — auto-registered by Router::build()
type DeepLinkFn = Box<dyn Fn(&str) + Send + Sync>;
static DEEP_LINK_HANDLER: std::sync::Mutex<Option<DeepLinkFn>> = std::sync::Mutex::new(None);

/// Dispatch an incoming deep link URI to the registered router.
///
/// Called automatically by platform runners (Android/iOS/Desktop).
/// Users don't need to call this directly.
pub fn dispatch_deep_link(uri: &str) {
    if let Ok(guard) = DEEP_LINK_HANDLER.lock() {
        if let Some(ref handler) = *guard {
            handler(uri);
        }
    }
}

/// Check CLI arguments for a `--deep-link=URI` flag and return the URI.
///
/// Call this at app startup to handle deep links passed via command line:
/// ```ignore
/// let router = RouterBuilder::new().route(...).build();
/// if let Some(uri) = blinc_router::cli_deep_link() {
///     router.handle_deep_link(&uri);
/// }
/// ```
pub fn cli_deep_link() -> Option<String> {
    std::env::args().find_map(|arg| arg.strip_prefix("--deep-link=").map(|s| s.to_string()))
}

// ============================================================================
// Scoped router context
// ============================================================================

use std::cell::RefCell;

thread_local! {
    /// Stack of active routers. The top is the "current" router for use_router().
    /// Pushed when route_outlet() builds a view, popped when done.
    static ROUTER_STACK: RefCell<Vec<Router>> = const { RefCell::new(Vec::new()) };
}

/// Push a router onto the context stack (called by outlet before building views)
pub(crate) fn push_router_context(router: &Router) {
    ROUTER_STACK.with(|stack| {
        stack.borrow_mut().push(router.clone());
    });
}

/// Pop a router from the context stack (called by outlet after building views)
pub(crate) fn pop_router_context() {
    ROUTER_STACK.with(|stack| {
        stack.borrow_mut().pop();
    });
}

/// Get the currently active router.
///
/// Returns the router whose `outlet()` is currently being built.
/// For nested routers, returns the innermost one.
///
/// Panics if no router is in scope (i.e., not called during an outlet build).
///
/// # Example
///
/// ```ignore
/// fn my_page(ctx: RouteContext) -> Div {
///     let router = use_router(); // same as ctx.router, but works in child components
///     div()
///         .child(text(&format!("Path: {}", router.current_path())))
///         .child(
///             div().on_click(move |_| router.push("/other"))
///                 .child(text("Navigate"))
///         )
/// }
/// ```
pub fn use_router() -> Router {
    ROUTER_STACK.with(|stack| {
        stack
            .borrow()
            .last()
            .cloned()
            .expect("use_router() called outside of a route outlet. Ensure the component is rendered inside a Router.outlet().")
    })
}

/// Get the current route's parameters (convenience for `use_router().params()`)
pub fn use_params() -> RouteParams {
    use_router().params()
}

/// Get the current route's query parameters
pub fn use_query() -> QueryParams {
    use_router().query()
}
