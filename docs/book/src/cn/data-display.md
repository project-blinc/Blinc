# Data Display Components

Tooltips, popovers, charts, tables and trees show information around the content
of the application. Install `cn_bundle()` as described in
[Getting Started](overview.md). The examples use these imports and the `ctx` of
the application builder:

```rust
use blinc::cn::prelude::*;
use blinc::layout::binding::IntoReactive;
use blinc::layout::prelude::{FontWeight, Text, div, text};
```

## Tooltip

Pass a trigger builder, and give the tooltip its text:

```rust
tooltip(|| div().child(button("Hover me")))
    .text("More information about this action")
    .side(TooltipSide::Top)
```

`TooltipSide` has `Top` (the default), `Bottom`, `Left` and `Right`. To control
placement and timing, use `.align(..)`, `.offset(..)`, `.open_delay_ms(..)` and
`.close_delay_ms(..)`. The text can be a Signal or Computed:

```rust
let hint = ctx.use_signal("Ready to upload".to_string());

tooltip(|| div().child(button("Upload"))).text(hint)
```

The popup uses `.cn-tooltip` and a side class such as `.cn-tooltip--top`. CSS
sets its appearance. There is no arrow part that you can configure.

## Hover Card

A hover card takes a trigger builder and a content builder. Each builder returns
a `Div`:

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

The popup is `.cn-hover-card-content`. To adjust the interaction delays, use
`.open_delay_ms(..)` and `.close_delay_ms(..)`.

## Popover

A popover opens on click, and the component toggles and dismisses it. Its
trigger builder receives the current open value:

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

The content builder returns a `Div`. To use runtime values in the content, bind
them to Signals. There is no public `.open(signal)` or `.on_open_change(...)`
builder.

`.class(..)` targets the trigger wrapper. The panel mounts separately, and its
class is `.cn-popover-content`. To restyle the panel, use that class, not a
descendant selector that starts at the trigger.

## Chart

Line, area and bar charts draw on a canvas. The value axis, category labels,
legend and tooltip are ordinary elements. When the pointer moves across a chart,
the chart marks the category under the pointer. The mark is a line through the
points of that category, or a band behind its bars. The chart also shows the
value of each series there.

```rust
use blinc::cn::prelude::*;

cn::line_chart()
    .series("CPU", &[30.0, 45.0, 40.0, 60.0, 55.0])
    .series("Memory", &[20.0, 25.0, 30.0, 35.0, 40.0])
    .labels(vec!["Mon".into(), "Tue".into(), "Wed".into(), "Thu".into(), "Fri".into()])
    .format(|v| format!("{v}%"))
    .dots()
```

`cn::area_chart()` fills down to the axis. With `.stacked()`, it stacks the
series, and each series fills down to the one below.

`cn::bar_chart()` puts the series side by side in each category. With
`.stacked()`, it stacks them. `.horizontal()` lays the bars along the width, with
the categories down the side. Bars are rounded at the end away from the axis.

```rust
cn::bar_chart()
    .series("Desktop", &[186.0, 305.0, 237.0])
    .series("Mobile", &[80.0, 200.0, 120.0])
    .labels(vec!["Jan".into(), "Feb".into(), "Mar".into()])
```

These methods change the chart:

- `.curve(ChartCurve::Linear)` or `Step` replaces the smooth curve.
- `.height(..)` and `.width(..)` set the size. If you do not set a width, the
  chart fills the width of its parent.
- `.min(..)` and `.max(..)` fix the value axis.
- `.no_grid()`, `.no_axes()`, `.no_tooltip()` and `.legend(false)` leave parts
  out.

### Pie and donut

`cn::pie_chart()` sizes each slice by its share of the total. The slices go
clockwise from the top. They sweep in, and they ease to new values.
`.donut(0.6)` cuts a hole with that share of the radius. `.total("Visitors")`
writes the total in the hole, with the caption under it. When the pointer hovers
over a slice, the slice pulls out and shows its value and share.

```rust
cn::pie_chart()
    .slice("Chrome", 275.0)
    .slice("Safari", 200.0)
    .slice("Other", 90.0)
    .donut(0.6)
    .total("Visitors")
```

`.data(..)` takes a list, Signal or Computed of `PieSlice`. `.size(..)` sets the
diameter, which is 220 by default. The total is `.cn-chart-pie-total`, and its
caption is `.cn-chart-pie-caption`.

### Live data

`.data(..)` takes all the series as a list, Signal or Computed of
`ChartSeries`. `.labels(..)` also takes a list, Signal or Computed. The chart
follows changes to them in place. It eases from the values on screen to the new
values:

```rust
let visitors = ctx.use_signal(vec![
    ChartSeries::new("Desktop", vec![186.0, 305.0, 237.0]),
    ChartSeries::new("Mobile", vec![80.0, 200.0, 120.0]),
]);

cn::area_chart().data(visitors).stacked()
```

### Styling

Series take the theme colors `--chart-1` to `--chart-5` in turn. The theme has
one palette per scheme. To give one series a different color, use
`ChartSeries::color(..)`, which takes any CSS color. The variables
`--cn-chart-grid`, `--cn-chart-cursor` and `--cn-chart-surface` control the grid,
the pointer cursor and the rings around dots. To restyle every chart, set these
variables:

```css
:root { --chart-1: #16a34a; --cn-chart-grid: transparent; }
```

The parts are:

- `.cn-chart`
- `.cn-chart-plot`
- `.cn-chart-axis`. A crowded label carries `data-hidden`.
- `.cn-chart-legend`, with `.cn-chart-legend-item` and `.cn-chart-swatch`
- `.cn-chart-tooltip`, with its `-title`, `-row`, `-name` and `-value`

### Sparklines, thresholds, histograms and comparisons

These charts use the same plot as line, area and bar charts:

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

Any chart takes `.band(..)` for a shaded value range and
`.marker(ChartMarker::new(v))` for a dashed line at a value. A band with no color
of its own takes the good, warning or bad color, in turn. The label of a band is
`.cn-chart-band-label`. The notes of a comparison are `.cn-chart-note`. Past the
threshold, a note has `data-tone="bad"`. For a decrease, the tone is `"good"`.

## Table

A table has rows of cells under a header, with an optional footer and caption:

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

The `.cn-table*` rules set the appearance. `.cn-table` sets the border, the
radius and the text color that every cell inherits. `.cn-table-header` and
`.cn-table-footer` use `--surface-overlay` as their background. The text of
`.cn-table-head` and `.cn-table-caption` is muted. Body rows highlight on hover,
and a selected row carries `data-state="selected"`. To adjust the appearance, use
the variables `--cn-table-border`, `--cn-table-radius`, `--cn-table-row-hover`
and `--cn-table-row-selected`:

```css
.cn-table-row[data-state="selected"] { background: color-mix(in srgb, var(--primary) 20%, transparent); }
```

## Tree

`tree_view()` adds roots with `.node(key, label, configure)`. To add nested
nodes, use `.child(key, label, configure)`:

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

When a group first opens, the tree builds its rows and then keeps them. As a
result, a large tree costs only what was opened. `.selected(..)` sets an initial
key. `.on_select(..)` reports later selections. It does not bind the selection of
the tree to an external signal.

A click on a row selects the row and opens or closes it. After you click the
tree, these keys work:

- Up and Down move through the visible rows.
- Right opens a row or enters its children.
- Left closes a row or goes to its parent.
- Home and End jump to the ends.
- Enter or Space selects.

`.diff(TreeNodeDiff::Added)` on a node colors its row. You can also pass
`Removed` or `Modified`. `.guides(false)` hides the guide lines. `.indent(px)`
sets the indent, which is 14px by default.

### Styling

Each `.cn-tree-item` holds a `.cn-tree-row`. An item with children also holds a
`.cn-tree-group`. A row carries `data-selected` and `data-diff`. While the keys
are on a row, it also carries `data-focus-visible`. An item and its group carry
`data-state="open"` or `"closed"`. The other parts are `.cn-tree-chevron`,
`.cn-tree-icon`, `.cn-tree-label` and `.cn-tree-guide`:

```css
.cn-tree-row[data-selected] { background: var(--accent-subtle); }
.cn-tree-row[data-diff="added"] { color: #22c55e; }
.cn-tree[data-guides] .cn-tree-guide { background: var(--text-tertiary); }
```

## Kbd

A keyboard shortcut can combine several keycaps:

```rust
div().flex_row().items_center().gap(2.0)
    .child(text("Search"))
    .child(div().flex_row().gap(0.5)
        .child(kbd("⌘"))
        .child(kbd("K")))
```

`.cn-kbd` sets the fill, border, radius, shadow and text color. To recolor a
keycap, use `--cn-kbd-bg` and `--cn-kbd-border`.

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
