# Navigation Components

Tabs, menus, breadcrumbs and sidebars organize application navigation.
Install `cn_bundle()` as described in [Getting Started](overview.md).
Examples use these imports and the application builder's `ctx`:

```rust
use blinc_cn::prelude::*;
use blinc_layout::binding::IntoReactive;
use blinc_layout::prelude::{Text, div, text};
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
width. Its fill, radius and shadow are CSS rules.

The new panel enters and the old one leaves by `.cn-tabs-panel` rules for
`data-transition`: `slide` (the default), `fade`, `slide-left`, `slide-right`,
`slide-up` or `slide-down`. `TabsTransition::None` switches immediately.
Panels carry `data-motion="enter"` or `"exit"`; the old panel remains until
its exit finishes. With `slide`, `data-direction="next"` or `"prev"`
selects the direction according to tab order.

```css
.cn-tabs-panel[data-transition="fade"][data-motion="enter"] {
    animation: cn-fade-in 400ms var(--ease-out);
}
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

Shortcut labels display hints; application code supplies any global shortcut
bindings. `dropdown_menu_custom(|open| div()...)` supplies a custom trigger.
`.min_width(..)`, `.position(..)`, `.align(..)` and `.offset(..)` place the menu.

While the menu is open, Up and Down wrap through enabled rows, Home and End
jump to the first and last, and Enter or Space activates the focused row.
Right opens a submenu; Left returns to its parent row. Escape closes the
current menu and restores focus. Tab closes the menu and continues past its
trigger; Shift+Tab moves back. Disabled rows and separators are skipped.
Down or Up on a closed trigger opens at its first or last enabled row.
A pointer opens with focus on the panel, ready for the first arrow key.

`.cn-dropdown-trigger` styles the visual trigger. Its surrounding
`[aria-haspopup="menu"]` element holds keyboard focus, including for custom
triggers; restyle `[aria-haspopup="menu"]:focus-visible` for its outline.
The panel and its rows share the
`.cn-menu`, `.cn-menu-item`, `.cn-menu-item-label`, `.cn-menu-shortcut` and
`.cn-menu-separator` rules with other menus:

```css
.cn-dropdown-trigger:hover { background: #1e1e2a; }
.cn-menu-item:hover, .cn-menu-item:focus-visible { background: var(--accent-subtle); }
```

## Context Menu

Open a context menu at the pointer's window coordinates:

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

`on_context_menu` makes the target a Tab stop. Shift+F10 or the Context Menu
key opens at the focused target; right-click opens at the pointer. The menu shares
the dropdown's arrow, activation and dismissal keys. The returned overlay
handle can close the menu programmatically.

Add SVG icons with `item_with_icon`; the menu aligns each icon with its label:

```rust
context_menu().at(x, y)
    .item_with_icon("Duplicate", blinc_icons::to_svg(blinc_icons::icons::COPY, 16.0), || {})
    .item_with_icon("Share", blinc_icons::to_svg(blinc_icons::icons::SHARE_2, 16.0), || {})
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

A menubar is one Tab stop. Left/Right, Home/End move between its triggers;
Down/Up opens the selected menu. Left/Right in an open root menu switches
to the adjacent menu. Submenus use Right to enter and Left to return.

Restyle the shared `.cn-menu-item:focus-visible` highlight and
`.cn-menubar-trigger:focus-visible` outline. Menus also expose
`.cn-context-menu`, `.cn-context-menu-item`, `.cn-dropdown-menu`,
`.cn-dropdown-item`, `.cn-menubar-content` and `.cn-menubar-item`.

## Breadcrumb

Each step except the last invokes an application callback:

```rust
let page = ctx.use_signal("settings".to_string());

breadcrumb()
    .item("Home", move || page.set("home".to_string()))
    .item_with_icon("Projects", to_svg(icons::FOLDER, 16.0), move || {
        page.set("projects".to_string());
    })
    .current("Settings")
```

`.slash_separator()`, `.text_separator(..)` and
`.separator(BreadcrumbSeparator::Svg(..))` replace the chevron.
`.small()` and `.large()` set the size. The last `.cn-breadcrumb-item` carries
`aria-current="page"`; links carry `data-link`. Separators are
`.cn-breadcrumb-separator`.

```css
.cn-breadcrumb-item[data-link]:hover { color: var(--primary); }
```

## Pagination

Bind the current page to a `Signal<usize>`, `Signal<i32>` or float signal,
which is rounded to a page:

```rust
let page = ctx.use_signal(1usize);

pagination(10, page)
    .visible_pages(7)
    .show_first_last(true)
    .on_page_change(|page| {
        let _ = page;
    })
```

Buttons are `.cn-pagination-btn`; the current page carries
`aria-current="page"`, and unavailable arrows carry `disabled`.
Gaps are `.cn-pagination-ellipsis`.

## Sidebar

The collapse state is a `Signal<bool>`. Click handlers can also update a
page signal, which the content binds independently:

```rust
use blinc_core::reactive::computed;

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

The built-in toggle and external writes to `collapsed` control the same rail.
Clicking an item marks it active. `.show_toggle(false)` hides the toggle.
The content builder runs once; its argument is the selection at construction,
so use bindings or `show`/`for_each` regions for subsequent page changes.

### Styling

The rail is `.cn-sidebar` with `data-state="expanded"` or `"collapsed"`.
Its width uses `--cn-sidebar-width` (200px) and
`--cn-sidebar-collapsed-width` (41px), eased over the normal theme duration.
The adjacent page is `.cn-sidebar-inset`. Items use `.cn-sidebar-item` and
`data-active`; their parts are `.cn-sidebar-icon` and `.cn-sidebar-label`.
Section titles are `.cn-sidebar-group-label`.

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

Use `.trigger(..)` for a popup panel and `.item(..)` for a direct action:

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
