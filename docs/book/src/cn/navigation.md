# Navigation Components

Components for navigation: tabs, menus, breadcrumbs, and sidebars.

## Tabs

Organize content into tabbed sections:

```rust
use blinc_cn::prelude::*;

tabs()
    .value(active_tab)
    .on_change(|tab| set_active_tab(tab))
    .child(tabs_list()
        .child(tabs_trigger("account").child(text("Account")))
        .child(tabs_trigger("password").child(text("Password")))
        .child(tabs_trigger("settings").child(text("Settings"))))
    .child(tabs_content("account")
        .child(text("Account settings...")))
    .child(tabs_content("password")
        .child(text("Password settings...")))
    .child(tabs_content("settings")
        .child(text("Other settings...")))
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

Application sidebar navigation:

```rust
sidebar()
    .child(sidebar_header()
        .child(
            div().flex_row().items_center().gap(8.0)
                .child(icon(icons::BOX).size(24.0))
                .child(text("My App").weight(FontWeight::Bold))
        ))
    .child(sidebar_content()
        .child(sidebar_group()
            .child(sidebar_group_label("Main"))
            .child(sidebar_menu()
                .child(sidebar_menu_item("dashboard")
                    .icon(icons::HOME)
                    .active(current_route == "dashboard")
                    .on_click(|| navigate("/dashboard"))
                    .child(text("Dashboard")))
                .child(sidebar_menu_item("projects")
                    .icon(icons::FOLDER)
                    .on_click(|| navigate("/projects"))
                    .child(text("Projects")))
                .child(sidebar_menu_item("tasks")
                    .icon(icons::CHECK_SQUARE)
                    .on_click(|| navigate("/tasks"))
                    .child(text("Tasks")))))
        .child(sidebar_group()
            .child(sidebar_group_label("Settings"))
            .child(sidebar_menu()
                .child(sidebar_menu_item("settings")
                    .icon(icons::SETTINGS)
                    .on_click(|| navigate("/settings"))
                    .child(text("Settings")))
                .child(sidebar_menu_item("help")
                    .icon(icons::HELP_CIRCLE)
                    .on_click(|| navigate("/help"))
                    .child(text("Help"))))))
    .child(sidebar_footer()
        .child(
            div().flex_row().items_center().gap(8.0)
                .child(avatar().src("user.jpg").size(AvatarSize::Sm))
                .child(text("John Doe"))
        ))
```

### Collapsible Sidebar

```rust
let is_collapsed = use_state_keyed("sidebar_collapsed", || false);

sidebar()
    .collapsed(is_collapsed.clone())
    .child(sidebar_header()
        .child(sidebar_trigger()
            .on_click({
                let is_collapsed = is_collapsed.clone();
                move |_| is_collapsed.set(!is_collapsed.get())
            })))
    .child(/* rest of sidebar */)
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
