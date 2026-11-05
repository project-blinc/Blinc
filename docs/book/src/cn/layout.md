# Layout Components

Avatars, separators, scroll areas and expandable sections provide structure
around application content. Install `cn_bundle()` as described in
[Getting Started](overview.md). Examples use these imports and the application
builder's `ctx`:

```rust
use blinc_cn::prelude::*;
use blinc_layout::prelude::{div, img, text};
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

The root is `.cn-avatar`; its fill and initials use `--cn-avatar-bg` and
`--cn-avatar-fg`. An empty avatar shows `.cn-avatar-placeholder`. The status
dot is `.cn-avatar-status` with `data-status`.

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

This shows three avatars and a `+2` chip. The group is `.cn-avatar-group`;
the overflow chip is `.cn-avatar-group__more`. Their rings and fills are CSS.

## Separator

The default separator is horizontal. Set an explicit height on a vertical
separator:

```rust
div().flex_row().items_center().gap(2.0)
    .child(text("Account"))
    .child(separator().vertical().h(24.0))
    .child(text("Settings"))
```

A labelled horizontal divider uses flexible containers around the lines:

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

CSS `aspect-ratio` changes the container's native layout constraint. Set one dimension and let the other follow it; setting both width and height explicitly fixes both dimensions.

```css
.cn-aspect-ratio {
    width: 240px;
    aspect-ratio: 4 / 3;
}
```

The gallery compares all eight presets, including 3:2, 2:3 and 3:4. Values may be a positive number, a positive fraction such as `16 / 9`, or `auto` to clear the constraint.

## Scroll Area

```rust
scroll_area().h(240.0).child(
    div().flex_col().gap(2.0)
        .children((0..50).map(|i| text(format!("Item {i}")))),
)
```

Use `.horizontal()` or `.both_directions()` to change the scroll direction:

```rust
scroll_area().horizontal().w(320.0).h(120.0).child(
    div().flex_row().gap(2.0)
        .children((0..10).map(|i| {
            div().w(150.0).flex_shrink_0()
                .child(card().child(text(format!("Card {i}"))))
        })),
)
```

`.scrollbar(ScrollbarVisibility::Always)` keeps the bars visible;
`Auto` is the default. Scrollbar thumb and track colours currently use
`.thumb_color(..)` and `.track_color(..)` rather than CSS `scrollbar-color`.

## Collapsible

Pass a `Signal<bool>` to both the trigger and content:

```rust
let open = ctx.use_signal(false);

div().flex_col()
    .child(CollapsibleTrigger::new("Show details", open))
    .child(collapsible(open).content(|| {
        div().child(text("Details remain mounted while this section folds."))
    }))
```

Writing `open.set(true)` from an event or application callback opens the same
section. `collapsible_section` combines the trigger and body:

```rust
let open = ctx.use_signal(false);

collapsible_section("Details", open, || {
    div().child(text("Additional information"))
})
```

There are no separate `collapsible_trigger()` or `collapsible_content()`
functions. The parts are `.cn-collapsible-trigger`, `.cn-collapsible-content`
and `.cn-chevron`; their `data-state` is `open` or `closed`. CSS sets the
closed height and chevron transform, with theme-timed expansion.

## Accordion

Add each section with a key, label and body builder. One section opens at a
time by default:

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

`.multi_open()` allows several sections to stay open:

```rust
accordion().multi_open()
    .item("account", "Account", || div().child(text("Account details")))
    .item("billing", "Billing", || div().child(text("Billing details")))
```

For application-controlled sections, use `.item_with_state(...)`:

```rust
let open = ctx.use_signal(true);

accordion().item_with_state("help", "Help", open, || {
    div().child(text("The application can open or close this section."))
})
```

Each section's trigger and body use `.cn-accordion-trigger` and
`.cn-accordion-content`, with `data-state="open"` or `"closed"`.
Content wrappers and spacing belong to the application. Accordion renders the
body returned by the closure directly; add your own padded container when needed:

```rust
accordion().item("help", "Help", || {
    div().class("help-copy").child(text("How can we help?"))
})
```

```css
.help-copy { padding: 12px; }
```

Keep padding on that caller-owned container so `.cn-accordion-content` can fold
to zero height.
Override those rules instead of selecting colours in the body builder.

## Resizable

Panel sizes and limits are pixels. A `flex_grow()` panel takes the remaining
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

The `.cn-resizable-handle` contains `.cn-resizable-line`. Its hover and
`data-state="dragging"` rules control the line's appearance. There is no
standalone `resizable_handle()` builder.

## Split Pane Example

Nested groups combine horizontal navigation with vertically split content:

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

Place the group in a parent with a definite width and height so its flexible
panels have space to divide.
