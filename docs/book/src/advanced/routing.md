# Routing & Navigation

The `blinc_router` crate provides cross-platform routing with path matching, navigation history, guards, page transitions, and deep linking.

## Setup

```rust
use blinc_router::{RouterBuilder, Route, PageTransition};

let router = RouterBuilder::new()
    .route(Route::new("/").name("home").view(home_page))
    .route(Route::new("/users").name("users").view(users_page)
        .child(Route::new("/:id").name("user").view(user_detail)))
    .route(Route::new("/settings")
        .view(settings_page)
        .transition(PageTransition::modal()))
    .not_found(not_found_page)
    .build();
```

## Navigation

```rust
// Push (adds to history)
router.push("/users/42");

// Named route with params
router.push_named("user", &[("id", "42")]);

// Replace (no history entry)
router.replace("/login");

// Back / Forward
router.back();
router.forward();

// Check state
router.can_go_back();
router.current_path();
router.params().get("id");
router.query().get("page");
```

## Route Outlet

Place `router.outlet()` where the page content should render:

```rust
fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    div().flex_col()
        .child(nav_bar(&router))
        .child(router.outlet()) // Current route renders here
}
```

On navigation the outlet swaps its view in place. The scaffold around it
(header, nav bar) is left alone, and each navigation builds the view again,
even to the same path.

## use_router() Hook

Inside route views, `use_router()` returns the active router:

```rust
fn user_detail(ctx: RouteContext) -> Div {
    let router = use_router(); // Same as ctx.router
    let id = ctx.params.get("id").unwrap_or("?");

    div()
        .child(text(&format!("User #{}", id)))
        .child(
            div().on_click(move |_| router.back())
                .child(text("Back"))
        )
}
```

Nested routers work automatically — `use_router()` returns whichever router's `outlet()` is currently building.

## Page Transitions

Per-route transitions using Blinc's animation system:

```rust
Route::new("/settings")
    .view(settings_page)
    .transition(PageTransition::slide())      // iOS push style
    .transition(PageTransition::fade())       // Crossfade
    .transition(PageTransition::modal())      // Slide up/down
    .transition(PageTransition::scale())      // Scale in/out
    .transition(PageTransition::none())       // Instant

// Custom with spring physics
    .transition(PageTransition::slide().with_spring(SpringConfig::bouncy()))
```

A route with a transition plays the new page's enter animation and keeps
the old page mounted, out of flow at the start of the outlet, until its
exit animation is done.

## Navigation Guards

Protect routes with guards that allow, redirect, or reject:

```rust
use blinc_router::{NavigationGuard, GuardResult};
use std::sync::Arc;

let auth_guard: NavigationGuard = Arc::new(|_from, _to| {
    if is_authenticated() {
        GuardResult::Allow
    } else {
        GuardResult::Redirect("/login".into())
    }
});

RouterBuilder::new()
    .route(Route::new("/dashboard").view(dashboard).guard(auth_guard))
    .build();
```

## Deep Linking

Deep linking is **automatic** — just build a router and it works on all platforms.
`RouterBuilder::build()` auto-registers the deep link handler and back button.

```rust
// That's it — no platform-specific setup needed in Rust
let router = RouterBuilder::new()
    .route(Route::new("/users/:id").view(user_page))
    .build();
// Deep links to myapp://host/users/42 automatically navigate
```

### Platform Configuration

**Android** — add intent filters in `AndroidManifest.xml`:
```xml
<intent-filter>
    <action android:name="android.intent.action.VIEW" />
    <data android:scheme="myapp" />
</intent-filter>
```

**iOS** — add URL types in `Info.plist`:
```xml
<key>CFBundleURLTypes</key>
<array>
    <dict>
        <key>CFBundleURLSchemes</key>
        <array><string>myapp</string></array>
    </dict>
</array>
```

**Desktop** — register a custom URL scheme with the OS:

*macOS* (`Info.plist`):
```xml
<key>CFBundleURLTypes</key>
<array>
    <dict>
        <key>CFBundleURLSchemes</key>
        <array><string>myapp</string></array>
    </dict>
</array>
```

*Windows* (registry, set up by installer):
```
HKEY_CLASSES_ROOT\myapp\shell\open\command = "C:\path\to\myapp.exe" "--deep-link=%1"
```

*Linux* (`.desktop` file):
```ini
MimeType=x-scheme-handler/myapp
Exec=myapp --deep-link=%u
```

CLI fallback:
```bash
myapp --deep-link=myapp://host/users/42
```

### How It Works

1. `RouterBuilder::build()` registers a global deep link handler
2. Platform runners auto-dispatch incoming URIs to the handler
3. The router parses the URI and calls `push(path)`
4. No user code needed beyond building the router

## System Back Button

Also automatic — `RouterBuilder::build()` registers a back button handler.

- **Android**: system back button navigates back if the router has history
- **Desktop**: `Key::Back` dispatches through the back handler stack
- If at the root route, the event propagates (app exits normally)

## Route Matching

Express-style path patterns:

| Pattern | Example | Matches |
|---------|---------|---------|
| Static | `/about` | Exact match |
| Parameter | `/users/:id` | `/users/42` → `{id: "42"}` |
| Wildcard | `/files/*path` | `/files/a/b/c` → `{path: "a/b/c"}` |
| Nested | parent + child | `/users` + `/:id` → `/users/42` |
| Query | any path | `/search?q=hello` → `{q: "hello"}` |

## Named Routes

Look up routes by name for type-safe navigation:

```rust
// Check if a named route exists
router.path_for("user"); // Some("/users/:id")

// Navigate with params
router.push_named("user", &[("id", "42")]); // → /users/42

// Check if a path matches
router.has_route("/users/42"); // true
```

## Tab Navigator

Use the `tabs()` component from `blinc_cn` with the router's current path as the active tab:

```rust
use blinc_cn::tabs;

fn app_shell(router: &Router) -> Div {
    // Track active tab via router path
    let active_tab = ctx.use_state_keyed("tab", || router.current_path());

    div().flex_col().w_full().h_full()
        // Content area — router outlet
        .child(router.outlet().flex_grow())
        // Bottom tab bar
        .child(
            tabs(&active_tab)
                .tab("Home", "/", {
                    let r = router.clone();
                    move || r.push("/")
                })
                .tab("Search", "/search", {
                    let r = router.clone();
                    move || r.push("/search")
                })
                .tab("Profile", "/profile", {
                    let r = router.clone();
                    move || r.push("/profile")
                })
        )
}
```

Each tab click calls `router.push()` which updates the outlet. The tab state stays in sync with the route.

## Stack Navigator (Page Stack)

The router maintains a **page stack** — pages persist in the tree when
new pages are pushed on top. Suspended pages have input disabled and
are hidden, but their state (scroll position, form values, etc.) is preserved.

```rust
// Renders the page stack — active page visible, suspended pages preserved
router.outlet()
```

When you `router.push("/details")`:
1. The current page becomes **Suspended** (opacity 0, pointer_events_none)
2. The new page is pushed as **Active** on top

When you `router.back()`:
1. The top page is **removed** from the stack
2. The page below becomes **Active** again (with preserved state)

### Page state

```rust
use blinc_router::PageState;

let pages = router.page_stack();
for page in &pages {
    match page.state {
        PageState::Active => println!("Visible: {}", page.route.path),
        PageState::Suspended => println!("Hidden: {}", page.route.path),
    }
}
```

### Entry/exit animations

Use `motion()` containers inside route views for animated transitions:

```rust
fn user_detail(ctx: RouteContext) -> Div {
    motion()
        .slide_in(SlideDirection::Right, 300)
        .child(
            div().w_full().h_full()
                .child(text(&format!("User #{}", ctx.params.get("id").unwrap_or("?"))))
                .child(
                    div().on_click({
                        let r = ctx.router.clone();
                        move |_| r.back()
                    })
                    .child(text("Back"))
                )
        )
}
```

## Nested Route Stacks

Layout routes can contain their own scoped router for sub-navigation.
`use_router()` automatically returns the innermost router:

```rust
fn dashboard_layout(ctx: RouteContext) -> Div {
    // Create a sub-router for dashboard tabs
    let sub_router = RouterBuilder::new()
        .route(Route::new("/").view(dashboard_overview))
        .route(Route::new("/analytics").view(analytics))
        .route(Route::new("/settings").view(settings))
        .initial(&ctx.path) // Start at current sub-path
        .build();

    div().flex_row().w_full().h_full()
        .child(dashboard_sidebar(&sub_router))
        .child(sub_router.outlet()) // Nested outlet — use_router() returns sub_router here
}
```

## Bottom Sheet Navigation

Use `sheet()` from `blinc_cn` for modal-like navigation that slides up from the bottom:

```rust
use blinc_cn::sheet;

fn show_details(router: &Router, item_id: &str) {
    // Navigate to detail route
    router.push(&format!("/items/{}", item_id));

    // Or show as a bottom sheet overlay
    sheet()
        .title("Item Details")
        .content(move || {
            let router = use_router();
            router.outlet() // Render the matched route inside the sheet
        })
        .show();
}
```

For gesture-dismissable sheets on mobile, the sheet component handles the swipe-down gesture automatically. On dismiss, call `router.back()`:

```rust
sheet()
    .on_close({
        let r = router.clone();
        move || r.back()
    })
    .content(|| detail_view())
    .show();
```

## Navigation Patterns Summary

| Pattern | Widget | Router Integration |
|---------|--------|-------------------|
| **Page navigation** | `router.outlet()` | Direct — renders current route |
| **Tab bar** | `blinc_cn::tabs()` | Tab clicks call `router.push()` |
| **Stack with animations** | `stack()` + `motion()` | Wrap route views in motion containers |
| **Bottom sheet** | `blinc_cn::sheet()` | Content renders `router.outlet()`, dismiss calls `router.back()` |
| **Drawer / sidebar** | `blinc_cn::drawer()` | Navigation links call `router.push()` |
| **Back button** | Auto-registered | `RouterBuilder::build()` wires it |
| **Deep links** | Auto-registered | Platform dispatches to router automatically |
