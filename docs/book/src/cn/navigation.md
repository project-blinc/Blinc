# Navigation Components

Tabs, menus, breadcrumbs and sidebars organize the navigation of an application.
Install `cn_bundle()` as described in [Getting Started](overview.md). The
examples use these imports and the `ctx` of the application builder:

```rust
use blinc::cn::prelude::*;
use blinc::layout::binding::IntoReactive;
use blinc::layout::prelude::{Text, div, text};
```

## Tabs

Bind the selected tab to a `Signal<String>`:

```rust
let tab = ctx.use_signal("account".to_string());

tabs(tab)
    .tab("account", "Account", || div().child(text("Account settings")))
    .tab("password", "Password", || div().child(text("Password settings")))
    .tab("settings", "Settings", || div().child(text("Other settings")))
    .transition(TabsTransition::Fade)
    .on_change(|value| {
        let _ = value;
    })
```

The pill, `.cn-tabs-indicator`, slides to the selected trigger and takes its
width. CSS rules set its fill, radius and shadow.

The `.cn-tabs-panel` rules make the new panel enter and the old panel leave. They
follow the `data-transition` value: `slide` (the default), `fade`, `slide-left`,
`slide-right`, `slide-up` or `slide-down`. `TabsTransition::None` switches
immediately. Panels carry `data-motion="enter"` or `"exit"`. The old panel
remains until its exit finishes. With `slide`, `data-direction="next"` or
`"prev"` selects the direction according to the tab order.

```css
.cn-tabs-panel[data-transition="fade"][data-motion="enter"] {
    animation: cn-fade-in 400ms var(--ease-out);
}
```

With `reorderable`, the user can drag the triggers along the strip. A dragged
trigger follows the pointer and carries `.cn-tabs-trigger--dragging`. The tabs
that it passes slide aside by the `transform` transition on
`.cn-tabs-list--reorderable > .cn-tabs-trigger`. On drop, the callback receives
the values in their new order. A press that does not move still selects its tab.
Keep the order, and build the tabs in that order:

```rust
let order = ctx.use_signal(vec!["a".to_string(), "b".to_string()]);
let mut strip = tabs(tab).reorderable(move |next| order.set(next));
for value in order.get() {
    strip = strip.tab(value.clone(), value.clone(), || div());
}
```

`closable` gives each trigger a close button (`.cn-tabs-close`) at its end. The
active trigger shows its button. While the pointer is over another trigger, that
trigger shows its button too. A press on the button calls back with the value of
the tab and does not select the tab. The app removes the tab:

```rust
let open = ctx.use_signal(vec!["a".to_string(), "b".to_string()]);
let strip = tabs(tab).closable(move |value| {
    open.update(|mut tabs| { tabs.retain(|t| t != value); tabs })
});
```

## Dropdown Menu

Add rows and callbacks directly to the builder:

```rust
let action = ctx.use_signal(String::new());

dropdown_menu("Options")
    .item("Edit", move || action.set("edit".to_string()))
    .item_with_shortcut("Save", "Cmd+S", move || action.set("save".to_string()))
    .item_disabled("Archive")
    .separator()
    .item("Delete", move || action.set("delete".to_string()))
```

A shortcut label shows a hint, and the application code supplies any global
shortcut bindings. For a custom trigger, use
`dropdown_menu_custom(|open| div()...)`. To place the menu, use
`.min_width(..)`, `.position(..)`, `.align(..)` and `.offset(..)`.

While the menu is open, these keys work:

- Up and Down move through the enabled rows and wrap at the ends.
- Home and End jump to the first row and the last row.
- Enter or Space activates the focused row.
- Right opens a submenu.
- Left returns to the parent row of a submenu.
- Escape closes the current menu and restores focus.
- Tab closes the menu and continues past its trigger.
- Shift+Tab moves back.

The keys skip disabled rows and separators. On a closed trigger, Down opens the
menu at its first enabled row. Up opens it at its last enabled row. If a pointer
opens the menu, the panel has focus and is ready for the first arrow key.

`.cn-dropdown-trigger` styles the visual trigger. The `[aria-haspopup="menu"]`
element around it holds keyboard focus, also for custom triggers. To change its
outline, restyle `[aria-haspopup="menu"]:focus-visible`. The panel and its rows
share these rules with other menus: `.cn-menu`, `.cn-menu-item`,
`.cn-menu-item-label`, `.cn-menu-shortcut` and `.cn-menu-separator`.

```css
.cn-dropdown-trigger:hover { background: #1e1e2a; }
.cn-menu-item:hover, .cn-menu-item:focus-visible { background: var(--accent-subtle); }
```

## Context Menu

Open a context menu at the window coordinates of the pointer:

```rust
let action = ctx.use_signal(String::new());

div().w(240.0).h(160.0)
    .child(text("Right-click here"))
    .on_context_menu(move |event| {
        context_menu()
            .at(event.mouse_x, event.mouse_y)
            .item("Cut", move || action.set("cut".to_string()))
            .item("Copy", move || action.set("copy".to_string()))
            .separator()
            .item("Delete", move || action.set("delete".to_string()))
            .show();
    })
```

`on_context_menu` makes the target a Tab stop. If the target has focus, Shift+F10
or the Context Menu key opens the menu at that target. A right-click opens it at
the pointer. The menu uses the same arrow, activation and dismissal keys as the
dropdown. With the returned overlay handle, your code can close the menu.

To add SVG icons, use `item_with_icon`. The menu aligns each icon with its label:

```rust
context_menu().at(x, y)
    .item_with_icon("Duplicate", blinc::icons::to_svg(blinc::icons::icons::COPY, 16.0), || {})
    .item_with_icon("Share", blinc::icons::to_svg(blinc::icons::icons::SHARE_2, 16.0), || {})
    .show();
```

## Menubar

A menubar owns several menus. Each `.menu(label, configure)` receives a
`MenubarMenu` builder:

```rust
let action = ctx.use_signal(String::new());

menubar()
    .menu("File", move |menu| {
        menu.item("New", move || action.set("new".to_string()))
            .item("Open", move || action.set("open".to_string()))
            .separator()
            .item_with_shortcut("Save", "Cmd+S", move || action.set("save".to_string()))
    })
    .menu("Edit", move |menu| {
        menu.item("Undo", move || action.set("undo".to_string()))
            .item("Redo", move || action.set("redo".to_string()))
    })
```

A menubar is one Tab stop. Left/Right and Home/End move between its triggers.
Down/Up opens the selected menu. In an open root menu, Left/Right switches to the
adjacent menu. In submenus, Right enters and Left returns.

Restyle the shared `.cn-menu-item:focus-visible` highlight and the
`.cn-menubar-trigger:focus-visible` outline. Menus also have the classes
`.cn-context-menu`, `.cn-context-menu-item`, `.cn-dropdown-menu`,
`.cn-dropdown-item`, `.cn-menubar-content` and `.cn-menubar-item`.

## Breadcrumb

Each step except the last calls an application callback:

```rust
let page = ctx.use_signal("settings".to_string());

breadcrumb()
    .item("Home", move || page.set("home".to_string()))
    .item_with_icon("Projects", to_svg(icons::FOLDER, 16.0), move || {
        page.set("projects".to_string());
    })
    .current("Settings")
```

To replace the chevron, use `.slash_separator()`, `.text_separator(..)` or
`.separator(BreadcrumbSeparator::Svg(..))`. `.small()` and `.large()` set the
size. The last `.cn-breadcrumb-item` carries `aria-current="page"`, and links
carry `data-link`. Separators are `.cn-breadcrumb-separator`.

```css
.cn-breadcrumb-item[data-link]:hover { color: var(--primary); }
```

## Pagination

Bind the current page to a `Signal<usize>`, a `Signal<i32>` or a float signal.
The component rounds a float value to a page:

```rust
let page = ctx.use_signal(1usize);

pagination(10, page)
    .visible_pages(7)
    .show_first_last(true)
    .on_page_change(|page| {
        let _ = page;
    })
```

Buttons are `.cn-pagination-btn`. The current page carries
`aria-current="page"`, and unavailable arrows carry `disabled`. Gaps are
`.cn-pagination-ellipsis`.

## Sidebar

The collapse state is a `Signal<bool>`. Click handlers can also update a page
signal, which the content binds independently:

```rust
use blinc::core::reactive::computed;

let collapsed = ctx.use_signal(false);
let page = ctx.use_signal("Dashboard".to_string());
let shown = computed(move |graph| graph.get(page).unwrap_or_default());

sidebar(collapsed)
    .section("Main")
    .item_active("Dashboard", to_svg(icons::HOUSE, 16.0), move || {
        page.set("Dashboard".to_string());
    })
    .item("Projects", to_svg(icons::FOLDER, 16.0), move || {
        page.set("Projects".to_string());
    })
    .section("Settings")
    .item("Settings", to_svg(icons::SETTINGS, 16.0), move || {
        page.set("Settings".to_string());
    })
    .content(move |_| div().p(6.0).child(Text::bound(shown.clone().into_reactive())))
```

The built-in toggle and outside writes to `collapsed` control the same rail. A
click on an item marks it active. `.show_toggle(false)` hides the toggle. For an
item that opens and closes something and does not go to a page, use
`toggle_item`. While its bool, signal or computed is true, the item is active. A
press on it does not change the selection.

```rust
sidebar(collapsed).toggle_item("Inspector", icon, inspector_open, move || {
    inspector_open.update(|open| !open)
})
```

The content builder runs once. Its argument is the selection at construction time.
For later page changes, use bindings or `show`/`for_each` regions.

### Styling

The rail is `.cn-sidebar` with `data-state="expanded"` or `"collapsed"`. Its
width uses `--cn-sidebar-width` (200px) and `--cn-sidebar-collapsed-width`
(41px). The width eases over the normal theme duration. The adjacent page is
`.cn-sidebar-inset`. Items use `.cn-sidebar-item` and `data-active`. Their parts
are `.cn-sidebar-icon` and `.cn-sidebar-label`. Section titles are
`.cn-sidebar-group-label`.

```css
.wide-nav.cn-sidebar { width: 280px; }
.wide-nav.cn-sidebar[data-state="collapsed"] { width: 64px; }
.wide-nav.cn-sidebar { transition: width 400ms var(--ease-state); }
```

```rust
let collapsed = ctx.use_signal(false);
sidebar(collapsed).class("wide-nav")
```

## Navigation Menu

For a popup panel, use `.trigger(..)`. For a direct action, use `.item(..)`:

```rust
let page = ctx.use_signal("home".to_string());

navigation_menu()
    .trigger("Products", move || {
        div().flex_col().gap(2.0)
            .child(navigation_link("Analytics")
                .description("Usage and performance")
                .on_click(move || page.set("analytics".to_string())))
            .child(navigation_link("Reports")
                .on_click(move || page.set("reports".to_string())))
    })
    .item("Pricing", move || page.set("pricing".to_string()))
    .item("About", move || page.set("about".to_string()))
```
