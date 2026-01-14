# blinc_cn

> **Part of the [Blinc UI Framework](https://project-blinc.github.io/Blinc)**
>
> This crate is a component of Blinc, a GPU-accelerated UI framework for Rust.
> For full documentation and guides, visit the [Blinc documentation](https://project-blinc.github.io/Blinc).

Component library for Blinc UI - shadcn/ui-style themed components.

## Overview

`blinc_cn` provides a comprehensive set of production-ready UI components built on top of `blinc_layout`. Inspired by [shadcn/ui](https://ui.shadcn.com/), it offers beautifully designed, accessible components with consistent theming.

## Features

- **40+ Components**: Buttons, cards, dialogs, menus, forms, and more
- **Theme Integration**: Automatic dark/light mode support
- **Variants & Sizes**: Multiple visual variants for each component
- **Accessibility**: Keyboard navigation and ARIA support
- **Customizable**: Override styles and behaviors as needed

## Installation

```toml
[dependencies]
blinc_cn = { path = "../blinc_cn" }
```

## Quick Start

```rust
use blinc_cn::prelude::*;

fn build_ui() -> impl ElementBuilder {
    div()
        .flex_col()
        .gap(16.0)
        .p(24.0)
        .child(
            card()
                .child(card_header()
                    .child(card_title("Welcome"))
                    .child(card_description("Get started with Blinc")))
                .child(card_content()
                    .child(text("Your content here")))
                .child(card_footer()
                    .child(button("Continue").variant(ButtonVariant::Primary)))
        )
}
```

## Components

### Buttons

```rust
// Variants
button("Primary").variant(ButtonVariant::Primary)
button("Secondary").variant(ButtonVariant::Secondary)
button("Destructive").variant(ButtonVariant::Destructive)
button("Outline").variant(ButtonVariant::Outline)
button("Ghost").variant(ButtonVariant::Ghost)
button("Link").variant(ButtonVariant::Link)

// Sizes
button("Small").size(ButtonSize::Sm)
button("Default").size(ButtonSize::Default)
button("Large").size(ButtonSize::Lg)
button("Icon").size(ButtonSize::Icon)

// With icon
button("Settings").icon(icons::SETTINGS)
```

### Cards

```rust
card()
    .child(card_header()
        .child(card_title("Card Title"))
        .child(card_description("Card description")))
    .child(card_content()
        .child(/* content */))
    .child(card_footer()
        .child(/* actions */))
```

### Dialogs

```rust
dialog()
    .open(is_open)
    .on_open_change(|open| set_is_open(open))
    .child(dialog_trigger()
        .child(button("Open Dialog")))
    .child(dialog_content()
        .child(dialog_header()
            .child(dialog_title("Dialog Title"))
            .child(dialog_description("Dialog description")))
        .child(/* content */)
        .child(dialog_footer()
            .child(button("Cancel").variant(ButtonVariant::Outline))
            .child(button("Continue"))))
```

### Form Components

```rust
// Input
input()
    .placeholder("Enter email...")
    .value(email)
    .on_change(|v| set_email(v))

// Textarea
textarea()
    .placeholder("Enter message...")
    .rows(4)

// Checkbox
checkbox()
    .checked(is_checked)
    .on_change(|c| set_checked(c))
    .child(label("Accept terms"))

// Switch
switch_()
    .checked(is_enabled)
    .on_change(|e| set_enabled(e))

// Radio Group
radio_group()
    .value(selected)
    .on_change(|v| set_selected(v))
    .child(radio_item("option1").child(label("Option 1")))
    .child(radio_item("option2").child(label("Option 2")))

// Select
select()
    .value(selected)
    .on_change(|v| set_selected(v))
    .child(select_trigger()
        .child(select_value()))
    .child(select_content()
        .child(select_item("opt1").child(text("Option 1")))
        .child(select_item("opt2").child(text("Option 2"))))

// Slider
slider()
    .value(volume)
    .min(0.0)
    .max(100.0)
    .on_change(|v| set_volume(v))
```

### Navigation

```rust
// Tabs
tabs()
    .value(active_tab)
    .on_change(|t| set_active_tab(t))
    .child(tabs_list()
        .child(tabs_trigger("tab1").child(text("Tab 1")))
        .child(tabs_trigger("tab2").child(text("Tab 2"))))
    .child(tabs_content("tab1").child(/* content */))
    .child(tabs_content("tab2").child(/* content */))

// Dropdown Menu
dropdown_menu()
    .child(dropdown_menu_trigger()
        .child(button("Menu")))
    .child(dropdown_menu_content()
        .child(dropdown_menu_item("edit").child(text("Edit")))
        .child(dropdown_menu_separator())
        .child(dropdown_menu_item("delete").child(text("Delete"))))

// Breadcrumb
breadcrumb()
    .child(breadcrumb_list()
        .child(breadcrumb_item().child(breadcrumb_link("Home")))
        .child(breadcrumb_separator())
        .child(breadcrumb_item().child(breadcrumb_link("Products")))
        .child(breadcrumb_separator())
        .child(breadcrumb_item().child(breadcrumb_page("Details"))))

// Sidebar
sidebar()
    .child(sidebar_header()
        .child(text("App Name")))
    .child(sidebar_content()
        .child(sidebar_group()
            .child(sidebar_group_label("Menu"))
            .child(sidebar_menu()
                .child(sidebar_menu_item("Dashboard").icon(icons::HOME))
                .child(sidebar_menu_item("Settings").icon(icons::SETTINGS)))))
```

### Feedback

```rust
// Alert
alert()
    .variant(AlertVariant::Destructive)
    .child(alert_title("Error"))
    .child(alert_description("Something went wrong"))

// Badge
badge("New").variant(BadgeVariant::Default)
badge("Beta").variant(BadgeVariant::Secondary)

// Progress
progress().value(75.0)

// Spinner
spinner().size(SpinnerSize::Lg)

// Skeleton
skeleton().w(200.0).h(20.0)

// Toast
toast()
    .title("Success")
    .description("Your changes have been saved")
    .variant(ToastVariant::Success)
```

### Layout

```rust
// Avatar
avatar()
    .src("user.jpg")
    .fallback("JD")
    .size(AvatarSize::Lg)

// Avatar Group
avatar_group()
    .max(3)
    .child(avatar().src("user1.jpg"))
    .child(avatar().src("user2.jpg"))
    .child(avatar().src("user3.jpg"))
    .child(avatar().src("user4.jpg"))

// Separator
separator().orientation(Orientation::Horizontal)

// Aspect Ratio
aspect_ratio(16.0 / 9.0)
    .child(img("video-thumbnail.jpg"))

// Scroll Area
scroll_area()
    .h(400.0)
    .child(/* scrollable content */)

// Collapsible
collapsible()
    .open(is_open)
    .child(collapsible_trigger()
        .child(button("Toggle")))
    .child(collapsible_content()
        .child(/* hidden content */))

// Accordion
accordion()
    .child(accordion_item("item1")
        .child(accordion_trigger().child(text("Section 1")))
        .child(accordion_content().child(text("Content 1"))))
```

### Data Display

```rust
// Tooltip
tooltip()
    .child(tooltip_trigger()
        .child(button("Hover me")))
    .child(tooltip_content()
        .child(text("Tooltip text")))

// Hover Card
hover_card()
    .child(hover_card_trigger()
        .child(text("@username")))
    .child(hover_card_content()
        .child(/* user profile card */))

// Popover
popover()
    .child(popover_trigger()
        .child(button("Open")))
    .child(popover_content()
        .child(/* popover content */))

// Charts
chart()
    .chart_type(ChartType::Line)
    .data(&data_points)
    .x_axis("Date")
    .y_axis("Value")
```

## Theming

Components automatically use theme tokens:

```rust
use blinc_theme::ThemeState;

// Set theme
ThemeState::set_color_scheme(ColorScheme::Dark);

// Components automatically update
button("Themed Button") // Uses theme colors
```

## Component List

| Category | Components |
|----------|------------|
| **Buttons** | Button |
| **Cards** | Card, CardHeader, CardTitle, CardDescription, CardContent, CardFooter |
| **Dialogs** | Dialog, AlertDialog, Sheet, Drawer |
| **Forms** | Input, Textarea, Checkbox, Switch, Radio, Select, Combobox, Slider |
| **Navigation** | Tabs, DropdownMenu, ContextMenu, Menubar, NavigationMenu, Breadcrumb, Pagination, Sidebar |
| **Feedback** | Alert, Badge, Progress, Spinner, Skeleton, Toast |
| **Layout** | Avatar, Separator, AspectRatio, ScrollArea, Collapsible, Accordion, Resizable |
| **Data** | Tooltip, HoverCard, Popover, Chart, Tree |
| **Typography** | Typography (H1-H6, P, Blockquote, etc.) |
| **Misc** | Icon, Kbd, Label |

## License

MIT OR Apache-2.0
