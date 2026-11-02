# Navigation Components

Components for navigation: tabs, menus, breadcrumbs, and sidebars.

## Tabs

Organize content into tabbed sections, bound to a `Signal<String>` holding the
selected tab's value:

```rust
use blinc_cn::prelude::*;

let tab = ctx.use_signal("account".to_string());

tabs(tab)
    .tab("account", "Account", || div().child(text("Account settings...")))
    .tab("password", "Password", || div().child(text("Password settings...")))
    .tab("settings", "Settings", || div().child(text("Other settings...")))
    .transition(TabsTransition::SlideLeft)
    .on_change(|value| println!("now on {value}"))
```

### Switching

The new panel enters and the old one leaves by the `.cn-tabs-panel` rules for
its `data-transition` (`fade`, the default, `slide-left`, `slide-right`,
`slide-up` or `slide-down`; `TabsTransition::None` switches at once). A panel
carries `data-motion="enter"`, then `"exit"` while it leaves, overlaying the
new one until its exit has played. A rule of your own changes the motion:

```css
.cn-tabs-panel[data-transition="fade"][data-motion="enter"] {
    animation: cn-fade-in 400ms var(--ease-out);
}
```

## Dropdown Menu

```rust
cn::dropdown_menu("Options")
    .item("Edit", || edit_item())
    .item_with_shortcut("Save", "Cmd+S", || save())
    .item_disabled("Archive")
    .separator()
    .item("Delete", || delete_item())
```

`dropdown_menu_custom(|open| ..)` takes a trigger of your own, built for the
open and the closed state. `.min_width(..)`, `.position(..)`, `.align(..)` and
`.offset(..)` place the menu.

With the menu open, Down and Up move through the enabled rows (wrapping past
either end, skipping separators and disabled rows), Home and End jump to the
first and last, and Enter or Space activates the active row. The active row has
the class `cn-dropdown-item--active` and takes the hover fill. While a submenu
is open it receives the keys instead.

The default trigger carries the class `cn-dropdown-trigger`; its fill is
`:hover` and `:active` rules with a `transition`, so restyle it with rules of
your own:

```css
.cn-dropdown-trigger:hover { background: #1e1e2a; }
```

## Context Menu

Right-click menu:

```rust
context_menu()
    .child(context_menu_trigger()
        .child(div().w(200.0).h(150.0).bg(Color::GRAY)
            .child(text("Right-click me"))))
    .child(context_menu_content()
        .child(context_menu_item("cut").child(text("Cut")))
        .child(context_menu_item("copy").child(text("Copy")))
        .child(context_menu_item("paste").child(text("Paste")))
        .child(context_menu_separator())
        .child(context_menu_item("delete").child(text("Delete"))))
```

## Menubar

Application menu bar:

```rust
menubar()
    .child(menubar_menu()
        .child(menubar_trigger().child(text("File")))
        .child(menubar_content()
            .child(menubar_item("new").child(text("New File")))
            .child(menubar_item("open").child(text("Open...")))
            .child(menubar_separator())
            .child(menubar_item("save").child(text("Save")))
            .child(menubar_item("save-as").child(text("Save As...")))))
    .child(menubar_menu()
        .child(menubar_trigger().child(text("Edit")))
        .child(menubar_content()
            .child(menubar_item("undo").child(text("Undo")))
            .child(menubar_item("redo").child(text("Redo")))))
```

## Breadcrumb

Navigation path:

```rust
breadcrumb()
    .child(breadcrumb_list()
        .child(breadcrumb_item()
            .child(breadcrumb_link("Home").href("/")))
        .child(breadcrumb_separator())
        .child(breadcrumb_item()
            .child(breadcrumb_link("Products").href("/products")))
        .child(breadcrumb_separator())
        .child(breadcrumb_item()
            .child(breadcrumb_page("Details"))))  // Current page (not a link)
```

### With Ellipsis

```rust
breadcrumb()
    .child(breadcrumb_list()
        .child(breadcrumb_item().child(breadcrumb_link("Home")))
        .child(breadcrumb_separator())
        .child(breadcrumb_ellipsis())  // Collapsed items
        .child(breadcrumb_separator())
        .child(breadcrumb_item().child(breadcrumb_link("Category")))
        .child(breadcrumb_separator())
        .child(breadcrumb_item().child(breadcrumb_page("Current"))))
```

## Pagination

Page buttons over a page count, bound to the current page. The page is a
`Signal<usize>`, a `Signal<i32>` or a float signal, rounded to a page:

```rust
let page = ctx.use_signal(1usize);

cn::pagination(10, page)
    .visible_pages(7)
    .show_first_last(true)
    .on_page_change(|page| println!("page {page}"))
```

`.small()` and `.large()` set the size.

## Sidebar

A navigation rail that collapses to its icons:

```rust
let collapsed = ctx.use_signal(false);
let page = ctx.use_signal(0usize);

sidebar(collapsed)
    .section("Main")
    .item_active("Dashboard", to_svg(icons::HOUSE, 16.0), move || page.set(0))
    .item("Projects", to_svg(icons::FOLDER, 16.0), move || page.set(1))
    .section("Settings")
    .item("Settings", to_svg(icons::SETTINGS, 16.0), move || page.set(2))
    .content(|_| div().p(6.0).child(text("Page")))
```

The toggle at the top flips `collapsed`, and setting the signal from
elsewhere collapses it the same way. Clicking an item marks it active.
`.content(..)` places a page beside the rail that moves with it as it
narrows; drive what the page shows from a signal the items write.

### Styling

The rail is `.cn-sidebar` with `data-state="expanded"` or `"collapsed"`. Its
width is a rule, `var(--cn-sidebar-width, 200px)` open and
`var(--cn-sidebar-collapsed-width, 41px)` collapsed, eased by a `width`
transition over the theme's normal duration; the page beside it, a
`.cn-sidebar-inset`, is laid out again each frame. Labels
(`.cn-sidebar-label`) fade and group titles (`.cn-sidebar-group-label`) fold
away while collapsed. An item is `.cn-sidebar-item`, `data-active` while it is
the selection, with a `.cn-sidebar-icon`.

```css
.wide-nav.cn-sidebar { width: 280px; }
.wide-nav.cn-sidebar[data-state="collapsed"] { width: 64px; }
.wide-nav.cn-sidebar { transition: width 400ms var(--ease-state); }
```

```rust
sidebar(collapsed).class("wide-nav")
```

## Navigation Menu

Horizontal navigation with dropdowns:

```rust
navigation_menu()
    .child(navigation_menu_list()
        .child(navigation_menu_item()
            .child(navigation_menu_trigger().child(text("Products")))
            .child(navigation_menu_content()
                .child(navigation_menu_link("analytics").child(text("Analytics")))
                .child(navigation_menu_link("reports").child(text("Reports")))))
        .child(navigation_menu_item()
            .child(navigation_menu_link("pricing").child(text("Pricing"))))
        .child(navigation_menu_item()
            .child(navigation_menu_link("about").child(text("About")))))
```
