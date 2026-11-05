# Feedback Components

Alerts, badges, progress indicators and toasts report application status.
Install `cn_bundle()` as described in [Getting Started](overview.md).
Examples use these imports and the application builder's `ctx`:

```rust
use blinc::cn::prelude::*;
use blinc::layout::prelude::{div, text};
```

## Alert

`alert(message)` displays a single message. `alert_box()` adds a title and
description:

```rust
alert("Your changes are ready to save.")
```

```rust
alert_box()
    .title("Heads up!")
    .description("Review the details before continuing.")
```

### Variants

Both accept `Default`, `Success`, `Warning` or `Destructive`:

```rust
div().flex_col().gap(2.0)
    .child(alert("Saved").variant(AlertVariant::Success))
    .child(alert("Low disk space").variant(AlertVariant::Warning))
    .child(alert_box()
        .variant(AlertVariant::Destructive)
        .title("Upload failed")
        .description("Check your connection and try again."))
```

Messages, titles and descriptions can bind to Signals or Computeds. The root
is `.cn-alert` or `.cn-alert-box`, with a variant class such as
`.cn-alert--success` or `.cn-alert--error`. Restyle those rules rather than
picking colours in the component builder.

## Badge

A badge has a semantic variant and a visual treatment:

```rust
div().flex_row().gap(2.0)
    .child(badge("New"))
    .child(badge("3").variant(BadgeVariant::Secondary))
    .child(badge("Error").variant(BadgeVariant::Destructive))
    .child(badge("Beta").style(BadgeStyle::Outline))
```

Variants are `Default`, `Secondary`, `Success`, `Warning` and `Destructive`.
Styles are `Soft` (the default), `Solid` and `Outline`:

```rust
badge("Done").variant(BadgeVariant::Success).style(BadgeStyle::Solid)
```

### With an Icon

`.icon(..)` accepts an element builder:

```rust
badge("Verified")
    .variant(BadgeVariant::Success)
    .icon_position(IconPosition::Start)
    .icon(icon(icons::CHECK).size_px(12.0))
```

### A Bound Label

Convert a Signal or Computed with `IntoReactive`:

```rust
use blinc::core::reactive::computed;
use blinc::layout::binding::IntoReactive;

let count = ctx.use_signal(3usize);
let shown = computed(move |graph| format!("{} new", graph.get(count).unwrap_or(0)));

badge("").reactive_label(shown.into_reactive())
```

The root is `.cn-badge`. Its fill, border and text colour come from
`.cn-badge--{style}-{variant}`, such as `.cn-badge--soft-success`.
A custom class supports application overrides:

```rust
badge("Done").variant(BadgeVariant::Success).class("my-badge")
```

```css
.my-badge { border-radius: 4px; }
.cn-badge--soft-success { background: #052e16; border-color: #16a34a; color: #4ade80; }
```

## Progress

Values are in the `0..=100` range:

```rust
progress(75.0)
```

### Bound to a Signal

An `f32`, `Signal<f32>` or `Computed<f32>` can drive the bar. Bound values
change in place and ease by the transition on `.cn-progress-bar`:

```rust
let pct = ctx.use_signal(0.0_f32);

div()
    .child(progress(pct).w(300.0))
    .child(button("Advance").on_click(move |_| {
        pct.update(|value| (value + 10.0).min(100.0));
    }))
```

### Indeterminate

For work of unknown length:

```rust
progress_indeterminate().w(300.0)
```

The track is `.cn-progress`, with `data-size` and, when applicable,
`data-indeterminate`. The bar is `.cn-progress-bar`. Their colours read
`--cn-progress-track` and `--cn-progress-bar`; the transition and
`cn-progress-pulse` keyframes control motion.

### With a Label

```rust
use blinc::core::reactive::computed;

let pct = ctx.use_signal(0.0_f32);
let shown = computed(move |graph| format!("{}%", graph.get(pct).unwrap_or(0.0) as i32));

div().flex_col().gap(1.0)
    .child(div().flex_row().justify_between()
        .child(label("Uploading"))
        .child(label(shown)))
    .child(progress(pct))
```

## Spinner

```rust
spinner().size(SpinnerSize::Medium)
```

Sizes are `Small` (16px), `Medium` (24px, the default) and `Large` (32px).
The track is `.cn-spinner__track`; `.cn-spinner__arc` turns by the
`cn-spinner-turn` keyframes. Set speed through CSS:

```css
.slow-spinner .cn-spinner__arc { animation-duration: 2000ms; }
```

### Beside a Busy Button

A button binds its label and disabled prop. Compose a spinner while it is busy:

```rust
use blinc::core::reactive::computed;

let busy = ctx.use_signal(false);
let caption = computed(move |graph| {
    if graph.get(busy).unwrap_or(false) { "Saving" } else { "Save" }.to_string()
});

div().flex_row().items_center().gap(2.0)
    .child(button(caption).disabled(busy).on_click(move |_| busy.set(true)))
    .show(busy, || div().child(spinner().size(SpinnerSize::Small)))
    .child(button("Finish").on_click(move |_| busy.set(false)))
```

In an application, the request completion callback clears `busy`; the Finish
button makes that completion explicit in this example. There is no button
`.loading(...)` prop.

## Skeleton

A skeleton pulses by the `cn-skeleton-pulse` keyframes:

```rust
div().flex_col().gap(2.0)
    .child(skeleton().w(200.0).h(20.0))
    .child(skeleton_circle(48.0))
```

The root is `.cn-skeleton`; a circle also has `.cn-skeleton--circle`.
Override the animation through a custom class:

```css
.my-skeleton { animation-duration: 3000ms; }
```

### Card Skeleton

```rust
card().child(card_content().child(
    div().flex_col().gap(2.0)
        .child(skeleton().w(150.0).h(24.0))
        .child(skeleton().w(200.0).h(16.0))
        .child(skeleton().w_full().h(100.0)),
))
```

## Toast

Show notifications from an event or application callback:

```rust
button("Notify").on_click(|_| {
    toast("Success")
        .description("Your changes have been saved.")
        .show();
})
```

`toast_success`, `toast_warning` and `toast_error` select variants directly.
`.duration_ms(..)` sets the lifetime; `.persistent()` waits for dismissal.
`.corner(..)` sets the tray corner. `.show()` returns a handle whose
`.dismiss()` method removes that toast.

### With an Action

```rust
let undone = ctx.use_signal(false);

button("Create event").on_click(move |_| {
    toast("Event created")
        .description("The event is on your calendar.")
        .action("Undo", move || undone.set(true))
        .duration_ms(5000)
        .show();
})
```

### Styling and Motion

The root is `.cn-toast` with `data-variant`: `default`, `success`, `warning`
or `destructive`. Parts include `.cn-toast-icon`, `.cn-toast-title`,
`.cn-toast-description` and `.cn-toast-close`. Set `--cn-toast-bg` and
`--cn-toast-border` to override their defaults.

The wrapper is `.cn-toast-motion-right` or `.cn-toast-motion-left`, depending
on its corner. It carries `data-motion="enter"` or `"exit"` and remains
mounted until the exit completes:

```css
.cn-toast-motion-right[data-motion="exit"] {
    animation: cn-slide-to-right 400ms var(--ease-in) forwards;
}
```

## Examples

### Loading State

Use a conditional region so content follows the Signal after construction:

```rust
let loading = ctx.use_signal(true);

div().flex_col().gap(3.0)
    .child(button("Toggle loading").on_click(move |_| loading.update(|value| !value)))
    .show_or(loading,
        || div().flex_col().items_center().gap(2.0)
            .child(spinner().size(SpinnerSize::Large))
            .child(text("Loading")),
        || div().child(text("Content is ready")),
    )
```

### Form Submission Feedback

The application owns request state. Bind the button to it, and mount feedback
through a computed condition. The completion button stands in for a request
callback in this interactive example:

```rust
use blinc::core::reactive::computed;

let submitting = ctx.use_signal(false);
let message = ctx.use_signal(String::new());
let has_message = computed(move |graph| !graph.get(message).unwrap_or_default().is_empty());

div().flex_col().gap(3.0)
    .child(button("Submit").disabled(submitting).on_click(move |_| {
        message.set(String::new());
        submitting.set(true);
    }))
    .show(submitting, move || {
        div().flex_col().gap(2.0)
            .child(spinner())
            .child(button("Complete request").on_click(move |_| {
                submitting.set(false);
                message.set("Form submitted successfully.".to_string());
            }))
    })
    .show(has_message, move || {
        div().child(alert(message).variant(AlertVariant::Success))
    })
```

Signals update props or regions; neither example branches on a value read
once at build time. Error feedback can use a separate error Signal and a
`Destructive` alert, with the same binding pattern.

### Notification Helpers

```rust
fn notify_success(message: &str) {
    blinc::cn::cn::toast_success("Success")
        .description(message)
        .duration_ms(5000)
        .show();
}

fn notify_error(message: &str) {
    blinc::cn::cn::toast_error("Error")
        .description(message)
        .duration_ms(10000)
        .show();
}
```
