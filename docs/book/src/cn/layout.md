# Layout Components

Avatars, separators, scroll areas and expandable sections give structure around
the content of an application. Install `cn_bundle()` as described in
[Getting Started](overview.md). The examples use these imports and the `ctx` of
the application builder:

```rust
use blinc::cn::prelude::*;
use blinc::layout::prelude::{div, img, text};
```

## Avatar

An image can fall back to initials:

```rust
avatar().src("user.jpg").alt("Jane Doe").fallback("JD")
```

Sizes are `ExtraSmall` (24px), `Small` (32px), `Medium` (40px, the default),
`Large` (48px) and `ExtraLarge` (64px):

```rust
div().flex_row().gap(2.0)
    .child(avatar().fallback("JD").size(AvatarSize::Small))
    .child(avatar().fallback("AL").size(AvatarSize::Large))
```

`.online()`, `.offline()`, `.away()` and `.busy()` add a status dot:

```rust
avatar().fallback("JD").online()
```

The root is `.cn-avatar`. Its fill and initials use `--cn-avatar-bg` and
`--cn-avatar-fg`. An empty avatar shows `.cn-avatar-placeholder`. The status dot
is `.cn-avatar-status` with `data-status`.

### Avatar Group

```rust
avatar_group()
    .max(3)
    .child(avatar().fallback("JD"))
    .child(avatar().fallback("AL"))
    .child(avatar().fallback("MK"))
    .child(avatar().fallback("RS"))
    .child(avatar().fallback("TW"))
```

This shows three avatars and a `+2` chip. The group is `.cn-avatar-group`, and
the overflow chip is `.cn-avatar-group__more`. CSS sets their rings and fills.

## Separator

The default separator is horizontal. On a vertical separator, set an explicit
height:

```rust
div().flex_row().items_center().gap(2.0)
    .child(text("Account"))
    .child(separator().vertical().h(24.0))
    .child(text("Settings"))
```

A horizontal divider with a label uses flexible containers around the lines:

```rust
div().flex_row().items_center().gap(2.0)
    .child(div().flex_grow().child(separator()))
    .child(muted("or"))
    .child(div().flex_grow().child(separator()))
```

The separator carries `data-orientation`. Its `.cn-separator` rule reads
`--cn-separator-color`.

## Aspect Ratio

Wrap content in a fixed ratio:

```rust
aspect_ratio(16.0 / 9.0)
    .w(320.0)
    .child(img("video-thumbnail.jpg").cover())
```

Convenience builders include `aspect_ratio_16_9()`, `aspect_ratio_4_3()`,
`aspect_ratio_square()`, `aspect_ratio_9_16()` and `aspect_ratio_21_9()`.

CSS `aspect-ratio` changes the native layout constraint of the container. Set one
dimension, and let the other dimension follow it. If you set both width and height
explicitly, both dimensions are fixed.

```css
.cn-aspect-ratio {
    width: 240px;
    aspect-ratio: 4 / 3;
}
```

The gallery compares all eight presets. They include 3:2, 2:3 and 3:4. A value
can be a positive number, a positive fraction such as `16 / 9`, or `auto`. The
value `auto` clears the constraint.

## Scroll Area

```rust
scroll_area().h(240.0).child(
    div().flex_col().gap(2.0)
        .children((0..50).map(|i| text(format!("Item {i}")))),
)
```

To change the scroll direction, use `.horizontal()` or `.both_directions()`:

```rust
scroll_area().horizontal().w(320.0).h(120.0).child(
    div().flex_row().gap(2.0)
        .children((0..10).map(|i| {
            div().w(150.0).flex_shrink_0()
                .child(card().child(text(format!("Card {i}"))))
        })),
)
```

`.scrollbar(ScrollbarVisibility::Always)` keeps the bars visible. `Auto` is the
default. While the content scrolls, `Auto` shows the bars. They fade out 1.5 s
after the last scroll. At present, you set the scrollbar thumb and track colors
with `.thumb_color(..)` and `.track_color(..)`, not with CSS `scrollbar-color`.

## Collapsible

Pass a `Signal<bool>` to both the trigger and the content:

```rust
let open = ctx.use_signal(false);

div().flex_col()
    .child(CollapsibleTrigger::new("Show details", open))
    .child(collapsible(open).content(|| {
        div().child(text("Built when the section first opens, then kept."))
    }))
```

If an event or application callback calls `open.set(true)`, the same section
opens. `collapsible_section` combines the trigger and body:

```rust
let open = ctx.use_signal(false);

collapsible_section("Details", open, || {
    div().child(text("Additional information"))
})
```

When the section first opens, it builds the body and then keeps it. A close folds
the body but does not destroy it. As a result, the body keeps its state and eases
shut. If a section never opens, it costs nothing.

There are no separate `collapsible_trigger()` or `collapsible_content()`
functions. The parts are `.cn-collapsible-trigger`, `.cn-collapsible-content`
and `.cn-chevron`. Their `data-state` is `open` or `closed`. CSS sets the closed
height and the chevron transform. The theme sets the timing of the expansion.

## Accordion

Add each section with a key, a label and a body builder. By default, one section
opens at a time:

```rust
accordion()
    .default_open("start")
    .item("start", "How do I get started?", || {
        div().child(text("Install Blinc and create an application."))
    })
    .item("toolchain", "Which toolchain do I need?", || {
        div().child(text("Blinc 0.6 requires Rust 1.89 or newer."))
    })
```

When a section first opens, it builds its body and keeps it, as a collapsible
does.

To let several sections stay open, use `.multi_open()`:

```rust
accordion().multi_open()
    .item("account", "Account", || div().child(text("Account details")))
    .item("billing", "Billing", || div().child(text("Billing details")))
```

For sections that the application controls, use `.item_with_state(...)`:

```rust
let open = ctx.use_signal(true);

accordion().item_with_state("help", "Help", open, || {
    div().child(text("The application can open or close this section."))
})
```

The trigger and body of each section use `.cn-accordion-trigger` and
`.cn-accordion-content`, with `data-state="open"` or `"closed"`. To set colors,
override those rules. Do not select colors in the body builder.

Content wrappers and spacing belong to the application. The accordion directly
renders the body that the closure returns. If you need padding, add your own
padded container:

```rust
accordion().item("help", "Help", || {
    div().class("help-copy").child(text("How can we help?"))
})
```

```css
.help-copy { padding: 12px; }
```

To let `.cn-accordion-content` fold to zero height, keep the padding on that
container, which your code owns.

## Resizable

Panel sizes and limits are in pixels. A `flex_grow()` panel takes the remaining
space, and the group inserts its handles between panels:

```rust
resizable_group().horizontal()
    .panel(resizable_panel()
        .default_size(200.0).min_size(100.0).max_size(400.0)
        .child(text("Left panel")))
    .panel(resizable_panel().flex_grow().child(text("Right panel")))
    .on_resize(|sizes| {
        let _ = sizes;
    })
```

A vertical group uses the same panel builder:

```rust
resizable_group().vertical()
    .panel(resizable_panel().default_size(150.0).child(text("Top panel")))
    .panel(resizable_panel().flex_grow().child(text("Bottom panel")))
```

The `visible` method of a panel takes a bool, a signal or a computed. While the
value is false, the panel and the handle beside it leave the layout. The flexible
panels take the space in place, with no rebuild. If you show the panel again, it
has the size that it had. This is how you build a collapsible bottom panel or
sidebar:

```rust
let panel_open = signal(true);
resizable_group().vertical()
    .panel(resizable_panel().flex_grow().child(text("Editor area")))
    .panel(resizable_panel().default_size(160.0).visible(panel_open)
        .child(text("Output area")))
```

On a fixed panel, `collapsed(when, size)` folds the panel to `size` pixels
instead. The folded panel is below its minimum size and has no handle. When the
panel opens again, it has the size that it had. A sidebar that folds to its icon
rail is one panel that holds both:

```rust
let inspector_open = signal(true);
resizable_group().horizontal()
    .panel(resizable_panel().default_size(340.0).min_size(260.0)
        .collapsed(computed(move |g| !g.get(inspector_open).unwrap_or(true)), 43.0)
        .child(div().flex_row().child(rail).child(inspector.visible(inspector_open))))
    .panel(resizable_panel().flex_grow().child(text("Main area")))
```

The `.cn-resizable-handle` contains `.cn-resizable-line`. Its hover and
`data-state="dragging"` rules control the appearance of the line.
`handle_thickness` sets the width of the line. `handle_hit_padding` sets how far
the grab area reaches past the line on each side. The gap between two panels is
the line plus twice the padding. There is no standalone `resizable_handle()`
builder.

## Split Pane Example

Nested groups combine horizontal navigation with content that splits vertically:

```rust
resizable_group().horizontal()
    .panel(resizable_panel().default_size(200.0).min_size(120.0)
        .child(tree_view().node("src", "src", |n| {
            n.expanded().child("main", "main.rs", |n| n)
        })))
    .panel(resizable_panel().flex_grow().child(
        resizable_group().vertical()
            .panel(resizable_panel().flex_grow().child(text("Editor area")))
            .panel(resizable_panel().default_size(120.0)
                .child(text("Output area"))),
    ))
```

Place the group in a parent with a definite width and height. This gives its
flexible panels space to divide.
