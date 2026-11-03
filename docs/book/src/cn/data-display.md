# Data Display Components

Tooltips, popovers, charts, tables and trees present information around the
application's content. Install `cn_bundle()` as described in
[Getting Started](overview.md). Examples use these imports and the application
builder's `ctx`:

```rust
use blinc_cn::prelude::*;
use blinc_layout::binding::IntoReactive;
use blinc_layout::prelude::{FontWeight, Text, div, text};
```

## Tooltip

Pass a trigger builder and supply its text:

```rust
tooltip(|| div().child(button("Hover me")))
    .text("More information about this action")
    .side(TooltipSide::Top)
```

`TooltipSide` has `Top` (the default), `Bottom`, `Left` and `Right`.
`.align(..)`, `.offset(..)`, `.open_delay_ms(..)` and `.close_delay_ms(..)`
control placement and timing. The text accepts a Signal or Computed:

```rust
let hint = ctx.use_signal("Ready to upload".to_string());

tooltip(|| div().child(button("Upload"))).text(hint)
```

The popup uses `.cn-tooltip` and a side class such as `.cn-tooltip--top`.
Its look comes from CSS. There is no configurable arrow part.

## Hover Card

A hover card takes trigger and content builders returning `Div`:

```rust
hover_card(|| div().child(text("@jane")))
    .content(|| {
        div().flex_row().gap(3.0)
            .child(avatar().fallback("JD").size(AvatarSize::Large))
            .child(div().flex_col().gap(1.0)
                .child(text("Jane Doe").weight(FontWeight::Bold))
                .child(muted("Software developer")))
    })
    .side(HoverCardSide::Bottom)
    .align(HoverCardAlign::Start)
```

Its popup is `.cn-hover-card-content`. `.open_delay_ms(..)` and
`.close_delay_ms(..)` tune the interaction delays.

## Popover

A popover opens on click. Its trigger builder receives the current open
value; the component manages toggling and dismissal:

```rust
let notifications = ctx.use_signal(true);

popover(|_open| div().child(button("Preferences")))
    .content(move || {
        div().flex_col().gap(3.0)
            .child(text("Notifications"))
            .child(switch(notifications).label("Email notifications"))
    })
    .side(PopoverSide::Bottom)
    .align(PopoverAlign::Start)
    .offset(8.0)
```

The content builder returns a `Div`. Runtime values inside it bind to
Signals; there is no public `.open(signal)` or `.on_open_change(...)` builder.
`.class(..)` targets the trigger wrapper. The separately mounted panel is
`.cn-popover-content`, so use that class to restyle the panel rather than a
descendant selector rooted at the trigger.

## Chart

Line, area and bar charts draw on a canvas, with the value axis, category
labels, legend and tooltip as ordinary elements. Moving the pointer across a
chart marks the category under it (a line through its points, a band behind
its bars) and shows each series' value there.

```rust
use blinc_cn::prelude::*;

cn::line_chart()
    .series("CPU", &[30.0, 45.0, 40.0, 60.0, 55.0])
    .series("Memory", &[20.0, 25.0, 30.0, 35.0, 40.0])
    .labels(vec!["Mon".into(), "Tue".into(), "Wed".into(), "Thu".into(), "Fri".into()])
    .format(|v| format!("{v}%"))
    .dots()
```

`cn::area_chart()` fills down to the axis; `.stacked()` piles the series, each
filling down to the one below. `cn::bar_chart()` sets the series side by side
in each category, or piles them when `.stacked()`; `.horizontal()` lays the
bars along the width with the categories down the side. Bars are rounded at
the end away from the axis.

```rust
cn::bar_chart()
    .series("Desktop", &[186.0, 305.0, 237.0])
    .series("Mobile", &[80.0, 200.0, 120.0])
    .labels(vec!["Jan".into(), "Feb".into(), "Mar".into()])
```

`.curve(ChartCurve::Linear)` or `Step` replace
the smooth curve, `.height(..)` and `.width(..)` size it (it fills its parent's
width unless set), `.min(..)` and `.max(..)` fix the value axis, and
`.no_grid()`, `.no_axes()`, `.no_tooltip()` and `.legend(false)` leave parts
out.

### Pie and donut

`cn::pie_chart()` sizes slices by their share of the total, clockwise from the
top; they sweep in, and ease to new values. `.donut(0.6)` cuts a hole of that
share of the radius, and `.total("Visitors")` writes the total in it with the
caption under. Hovering pulls a slice out and shows its value and share.

```rust
cn::pie_chart()
    .slice("Chrome", 275.0)
    .slice("Safari", 200.0)
    .slice("Other", 90.0)
    .donut(0.6)
    .total("Visitors")
```

`.data(..)` takes a list, Signal or Computed of `PieSlice`; `.size(..)` sets the
diameter (220 unless set). The total is `.cn-chart-pie-total` and its caption
`.cn-chart-pie-caption`.

### Live data

`.data(..)` takes all the series as a list, Signal or Computed of
`ChartSeries`, and `.labels(..)` likewise. The chart follows them in place,
easing from the values it shows to the new ones:

```rust
let visitors = ctx.use_signal(vec![
    ChartSeries::new("Desktop", vec![186.0, 305.0, 237.0]),
    ChartSeries::new("Mobile", vec![80.0, 200.0, 120.0]),
]);

cn::area_chart().data(visitors).stacked()
```

### Styling

Series take `--chart-1` to `--chart-5` in turn, from the theme (one palette per
scheme); `ChartSeries::color(..)` takes any CSS colour for one. The grid, the
pointer's cursor and the rings round dots are `--cn-chart-grid`,
`--cn-chart-cursor` and `--cn-chart-surface`. Setting those variables restyles
every chart:

```css
:root { --chart-1: #16a34a; --cn-chart-grid: transparent; }
```

The parts are `.cn-chart`, `.cn-chart-plot`, `.cn-chart-axis` (a crowded
label carries `data-hidden`), `.cn-chart-legend` with `.cn-chart-legend-item`
and `.cn-chart-swatch`, and `.cn-chart-tooltip` with its `-title`, `-row`,
`-name` and `-value`.

### Sparklines, thresholds, histograms and comparisons

These are drawn on the same plot:

```rust
let frame_times = vec![12.0, 18.0, 15.0, 22.0];
let diffs = vec![1.0, 2.0, 2.5, 3.0, 5.0];

// A trend beside text: no axes, grid, legend or tooltip, 100 by 24.
cn::spark_line(vec![1.0, 2.5, 2.0, 3.5, 3.0]).color("var(--success)").filled();

// Values against budgets: each dot takes its band's colour.
cn::threshold_line_chart()
    .values(frame_times)
    .band(ChartBand::new(0.0, 16.67).label("60 fps"))
    .band(ChartBand::new(16.67, 33.33).label("30 fps"))
    .baseline(16.67);

// Values counted into equal-width bins.
cn::histogram(diffs).bins(30).log_scale().marker(5.0);

// Baseline against current per category, the change written beside it.
cn::comparison_bar_chart()
    .item("Render", 12.5, 14.2)
    .item("Layout", 3.2, 3.0)
    .threshold(10.0)
```

Any chart takes `.band(..)` for a shaded value range (good, warning and bad in
turn unless given a colour) and `.marker(ChartMarker::new(v))` for a dashed
line at a value. A band's label is `.cn-chart-band-label`; a comparison's notes
are `.cn-chart-note` with `data-tone="bad"` past the threshold or `"good"` for a
decrease.

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

`tree_view()` adds roots with `.node(key, label, configure)` and nested nodes
with `.child(key, label, configure)`:

```rust
let chosen = ctx.use_signal("main".to_string());

div().flex_col().gap(2.0)
    .child(tree_view()
        .node("src", "src", |n| {
            n.expanded()
                .child("main", "main.rs", |n| n.icon(to_svg(icons::FILE, 16.0)))
                .child("ui", "ui", |n| {
                    n.child("button", "button.rs", |n| n)
                })
        })
        .node("cargo", "Cargo.toml", |n| n)
        .selected("main")
        .on_select(move |key| chosen.set(key.to_string())))
    .child(Text::bound(chosen.into_reactive()))
```

`.selected(..)` sets an initial key. `.on_select(..)` reports subsequent
selection; it does not bind the tree's selection to an external signal.

Clicking a row selects it and opens or closes it. Once the tree has been
clicked, Up and Down walk the shown rows, Right opens a row or enters its
children, Left closes it or goes to its parent, Home and End jump to the ends,
and Enter or Space selects.

A node's `.diff(TreeNodeDiff::Added)` (or `Removed`, `Modified`) colours its
row. `.guides(false)` hides the guide lines; `.indent(px)` sets the indent
(14px by default).

### Styling

Each `.cn-tree-item` holds a `.cn-tree-row` and, with children, a
`.cn-tree-group`. A row carries `data-selected`, `data-diff` and, while the
keys are on it, `data-focus-visible`. An item and its group carry
`data-state="open"` or `"closed"`. The other parts are `.cn-tree-chevron`,
`.cn-tree-icon`, `.cn-tree-label` and `.cn-tree-guide`:

```css
.cn-tree-row[data-selected] { background: var(--accent-subtle); }
.cn-tree-row[data-diff="added"] { color: #22c55e; }
.cn-tree[data-guides] .cn-tree-guide { background: var(--text-tertiary); }
```

## Kbd

Keyboard shortcuts can combine several keycaps:

```rust
div().flex_row().items_center().gap(2.0)
    .child(text("Search"))
    .child(div().flex_row().gap(0.5)
        .child(kbd("⌘"))
        .child(kbd("K")))
```

The fill, border, radius, shadow and text colour come from `.cn-kbd`.
`--cn-kbd-bg` and `--cn-kbd-border` recolour it.

## Examples

### User Profile Card

```rust
hover_card(|| {
    div().flex_row().items_center().gap(2.0)
        .child(avatar().fallback("JD").size(AvatarSize::Small))
        .child(text("Jane Doe"))
})
.content(|| {
    div().flex_col().gap(3.0)
        .child(text("Jane Doe").weight(FontWeight::Bold))
        .child(muted("Developer and designer"))
        .child(text("Building desktop tools with Blinc."))
        .child(div().flex_row().gap(4.0)
            .child(label("120 followers"))
            .child(label("42 following")))
})
```

### Dashboard Chart

```rust
card()
    .child(card_header()
        .title("Revenue overview")
        .description("Monthly revenue"))
    .child(card_content().child(
        area_chart()
            .series("Revenue", &[1200.0, 1500.0, 1400.0])
            .labels(vec!["Jan".into(), "Feb".into(), "Mar".into()])
            .height(300.0)
            .format(|value| format!("${value:.0}")),
    ))
```
