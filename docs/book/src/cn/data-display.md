# Data Display Components

Components for displaying data: tooltips, popovers, hover cards, charts, and trees.

## Tooltip

Brief information on hover:

```rust
use blinc_cn::prelude::*;

tooltip()
    .child(tooltip_trigger()
        .child(button("Hover me")))
    .child(tooltip_content()
        .child(text("This is a tooltip")))
```

### Tooltip Position

```rust
tooltip()
    .side(TooltipSide::Top)     // Top (default)
    .side(TooltipSide::Bottom)  // Bottom
    .side(TooltipSide::Left)    // Left
    .side(TooltipSide::Right)   // Right
```

### With Arrow

```rust
tooltip()
    .child(tooltip_trigger().child(icon(icons::INFO)))
    .child(tooltip_content()
        .with_arrow(true)
        .child(text("More information")))
```

## Hover Card

Rich content on hover:

```rust
hover_card()
    .child(hover_card_trigger()
        .child(text("@username").color(Color::BLUE)))
    .child(hover_card_content()
        .child(
            div().flex_row().gap(12.0)
                .child(avatar().src("user.jpg").size(AvatarSize::Lg))
                .child(
                    div().flex_col().gap(4.0)
                        .child(text("John Doe").weight(FontWeight::Bold))
                        .child(text("@johndoe").color(Color::GRAY))
                        .child(text("Software developer at Acme Inc."))
                )
        ))
```

## Popover

Interactive content in a popup:

```rust
let is_open = use_state_keyed("popover_open", || false);

popover()
    .open(is_open.clone())
    .on_open_change({
        let is_open = is_open.clone();
        move |open| is_open.set(open)
    })
    .child(popover_trigger()
        .child(button("Open Popover")))
    .child(popover_content()
        .child(
            div().flex_col().gap(12.0)
                .child(text("Settings").weight(FontWeight::Bold))
                .child(
                    div().flex_col().gap(8.0)
                        .child(
                            div().flex_row().justify_between()
                                .child(label("Notifications"))
                                .child(switch_())
                        )
                        .child(
                            div().flex_row().justify_between()
                                .child(label("Dark Mode"))
                                .child(switch_())
                        )
                )
        ))
```

## Chart

Data visualization:

```rust
chart()
    .chart_type(ChartType::Line)
    .data(&[
        DataPoint::new("Jan", 100.0),
        DataPoint::new("Feb", 150.0),
        DataPoint::new("Mar", 120.0),
        DataPoint::new("Apr", 180.0),
    ])
    .x_label("Month")
    .y_label("Sales")
```

### Chart Types

```rust
// Line chart
chart().chart_type(ChartType::Line)

// Bar chart
chart().chart_type(ChartType::Bar)

// Area chart
chart().chart_type(ChartType::Area)

// Pie chart
chart().chart_type(ChartType::Pie)

// Histogram
chart().chart_type(ChartType::Histogram)

// Scatter plot
chart().chart_type(ChartType::Scatter)
```

### Multi-Series

```rust
chart()
    .chart_type(ChartType::Line)
    .series("Revenue", &revenue_data, Color::BLUE)
    .series("Expenses", &expense_data, Color::RED)
    .series("Profit", &profit_data, Color::GREEN)
    .legend(true)
```

### Bar Chart

```rust
chart()
    .chart_type(ChartType::Bar)
    .data(&[
        DataPoint::new("Q1", 1200.0),
        DataPoint::new("Q2", 1500.0),
        DataPoint::new("Q3", 1800.0),
        DataPoint::new("Q4", 2100.0),
    ])
    .color(Color::BLUE)
    .show_values(true)
```

### Pie Chart

```rust
chart()
    .chart_type(ChartType::Pie)
    .data(&[
        DataPoint::new("Desktop", 45.0),
        DataPoint::new("Mobile", 35.0),
        DataPoint::new("Tablet", 20.0),
    ])
    .show_labels(true)
    .show_percentages(true)
```

## Table

Rows of cells under a header, with an optional footer and caption:

```rust
let chosen = ctx.use_signal(false);

table()
    .w_full()
    .child(table_header().child(
        table_row().child(table_head("Invoice")).child(table_head("Amount")),
    ))
    .child(table_body()
        .child(table_row()
            .child(table_cell().child(text("INV001")))
            .child(table_cell().child(text("$250.00"))))
        .child(table_row()
            .selected(chosen)
            .child(table_cell().child(text("INV002")))
            .child(table_cell().child(text("$1,200.00")))))
    .child(table_caption("Recent invoices"))
```

`.selected(..)` takes a bool, Signal or Computed and follows it in place.

### Styling

The look is the `.cn-table*` rules. `.cn-table` sets the text colour every
cell inherits, the border and the radius; `.cn-table-header` and
`.cn-table-footer` sit on `--surface-overlay`; `.cn-table-head` and
`.cn-table-caption` text is muted. Body rows highlight on hover, and a
selected row carries `data-state="selected"`. The variables
`--cn-table-border`, `--cn-table-radius`, `--cn-table-row-hover` and
`--cn-table-row-selected` adjust them:

```css
.cn-table-row[data-state="selected"] { background: color-mix(in srgb, var(--primary) 20%, transparent); }
```

## Tree

A selectable tree whose items open and close:

```rust
tree_view()
    .node("src", "src", |n| {
        n.expanded()
            .child("main", "main.rs", |c| c.icon(to_svg(icons::FILE, 16.0)))
            .child("ui", "ui", |c| c.child("button", "button.rs", |c| c.icon(to_svg(icons::FILE, 16.0))))
    })
    .node("cargo", "Cargo.toml", |n| n.icon(to_svg(icons::FILE, 16.0)))
    .selected("main")
    .on_select(|key| println!("selected {key}"))
```

Clicking a row selects it and opens or closes it. Once the tree has been
clicked, Up and Down walk the shown rows, Right opens a row or enters its
children, Left closes it or goes to its parent, Home and End jump to the ends,
and Enter or Space selects.

A node's `.diff(TreeNodeDiff::Added)` (or `Removed`, `Modified`) colours its
row. Guide lines run beside open groups; `.guides(false)` hides them, and
`.indent(px)` sets how far a group sits in (14px by default).

### Styling

Each `.cn-tree-item` holds a `.cn-tree-row` and, with children, a
`.cn-tree-group`. A row carries `data-selected`, `data-diff` and, while the
keys are on it, `data-focus-visible`; an item and its group carry
`data-state="open"` or `"closed"`. A closed group is at no height by rule, and
the change eases over the theme's normal duration with the rows below moving
with it. The parts are `.cn-tree-chevron`, `.cn-tree-icon`, `.cn-tree-label`
and `.cn-tree-guide`:

```css
.cn-tree-row[data-selected] { background: var(--accent-subtle); }
.cn-tree-row[data-diff="added"] { color: #22c55e; }
.cn-tree[data-guides] .cn-tree-guide { background: var(--text-tertiary); }
```

## Kbd

Keyboard shortcut display:

```rust
// Single key
kbd("⌘")

// Key combination
div().flex_row().gap(4.0)
    .child(kbd("⌘"))
    .child(kbd("K"))

// In context
div().flex_row().items_center().gap(8.0)
    .child(text("Search"))
    .child(
        div().flex_row().gap(2.0)
            .child(kbd("⌘"))
            .child(kbd("K"))
    )
```

## Examples

### User Profile Card

```rust
hover_card()
    .child(hover_card_trigger()
        .child(
            div().flex_row().items_center().gap(8.0)
                .child(avatar().src(&user.avatar).size(AvatarSize::Sm))
                .child(text(&user.name))
        ))
    .child(hover_card_content()
        .w(300.0)
        .child(
            div().flex_col().gap(12.0)
                .child(
                    div().flex_row().gap(12.0)
                        .child(avatar().src(&user.avatar).size(AvatarSize::Lg))
                        .child(
                            div().flex_col()
                                .child(text(&user.name).weight(FontWeight::Bold))
                                .child(text(&user.title).color(Color::GRAY))
                        )
                )
                .child(text(&user.bio))
                .child(
                    div().flex_row().gap(16.0)
                        .child(
                            div().flex_col()
                                .child(text(&user.followers.to_string()).weight(FontWeight::Bold))
                                .child(text("Followers").size(12.0).color(Color::GRAY))
                        )
                        .child(
                            div().flex_col()
                                .child(text(&user.following.to_string()).weight(FontWeight::Bold))
                                .child(text("Following").size(12.0).color(Color::GRAY))
                        )
                )
        ))
```

### Dashboard Chart

```rust
card()
    .child(card_header()
        .child(card_title("Revenue Overview"))
        .child(card_description("Monthly revenue for 2024")))
    .child(card_content()
        .child(
            chart()
                .chart_type(ChartType::Area)
                .h(300.0)
                .data(&monthly_revenue)
                .color(Color::rgba(0.2, 0.5, 1.0, 0.5))
                .stroke_color(Color::BLUE)
                .x_label("Month")
                .y_label("Revenue ($)")
                .grid(true)
        ))
```

### File Tree

```rust
tree()
    .child(tree_item("src")
        .expandable(true)
        .expanded(true)
        .child(tree_item_trigger()
            .child(icon(icons::FOLDER_OPEN))
            .child(text("src")))
        .child(tree_item_content()
            .child(tree_item("main")
                .on_click(|| open_file("src/main.rs"))
                .child(tree_item_content()
                    .child(icon(icons::FILE_CODE))
                    .child(text("main.rs"))))
            .child(tree_item("lib")
                .on_click(|| open_file("src/lib.rs"))
                .child(tree_item_content()
                    .child(icon(icons::FILE_CODE))
                    .child(text("lib.rs"))))))
```
