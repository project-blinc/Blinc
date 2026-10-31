# Feedback Components

Components for user feedback: alerts, badges, progress indicators, and toasts.

## Alert

Display important messages:

```rust
use blinc_cn::prelude::*;

alert()
    .child(alert_title("Heads up!"))
    .child(alert_description("This is an important message."))
```

### Alert Variants

```rust
// Default
alert()
    .child(alert_title("Note"))
    .child(alert_description("This is a note."))

// Destructive (error/warning)
alert()
    .variant(AlertVariant::Destructive)
    .child(alert_title("Error"))
    .child(alert_description("Something went wrong."))
```

### With Icon

```rust
alert()
    .child(icon(icons::INFO).size(16.0))
    .child(alert_title("Information"))
    .child(alert_description("Here's some useful info."))
```

## Badge

Small labels for status or counts:

```rust
badge("New")
badge("3").variant(BadgeVariant::Secondary)
badge("Error").variant(BadgeVariant::Destructive)
badge("Beta").style(BadgeStyle::Outline)
```

### Variants and styles

A badge has a **variant**, its semantic colour, and a **style**, how that
colour is applied:

```rust
// Variants: Default (primary), Secondary (muted), Success, Warning, Destructive
badge("Done").variant(BadgeVariant::Success)

// Styles: Soft (default, a tinted fill and a same-hue border and text),
// Solid (filled, inverse text), Outline (transparent with a coloured border)
badge("Done").variant(BadgeVariant::Success).style(BadgeStyle::Solid)
badge("Done").variant(BadgeVariant::Success).style(BadgeStyle::Outline)
```

### With Icon

```rust
badge("Verified")
    .icon(icon(icons::CHECK).size(12.0))
    .icon_position(IconPosition::Start)
```

Call `.icon(..)` last in the chain: the other builder methods rebuild the
badge and drop an icon added before them.

### A Bound Label

`reactive_label` follows a signal in place, and takes the same colours as a
static label:

```rust
badge("").reactive_label(blinc_layout::binding::Reactive::Bound(count_text))
```

### Styling

A badge's fill, border, radius and text colour are the
`.cn-badge--{style}-{variant}` rules of `CN_STYLES`, for example
`.cn-badge--soft-success` or `.cn-badge--outline-default`. The label inherits
its `color` from the rule, so a rule of your own changes it, bound or not:

```css
.cn-badge--soft-success { background: #052e16; border-color: #16a34a; color: #4ade80; }
.my-badge { border-radius: 4px; }
```

```rust
badge("Done").variant(BadgeVariant::Success).class("my-badge")
```

## Progress

Progress bar — value is in the 0..=100 range:

```rust
progress(75.0)
```

### Signal-bound (recommended)

`progress(...)` accepts `impl IntoReactive<f32>`, so you can pass
either an eager `f32` or a `&State<f32>`. With a signal, updates patch
the indicator's GPU scale-x transform directly — no Stateful rebuild,
no per-frame layout recompute.

```rust
use blinc_core::context_state::use_state;

let pct = use_state(0.0_f32);

div()
    .child(progress(&pct).w(300.0))
    .child(
        button("Advance").on_click(move |_| {
            pct.update(|v| (v + 10.0).min(100.0));
        })
    )
```

### Indeterminate

```rust
progress(0.0).indeterminate(true)
```

### With Label

The label needs to re-render when `pct` changes, so wrap it in a
`Stateful` with `.deps(...)`. The progress bar itself binds the
signal directly and updates with no rebuild.

```rust
use blinc_layout::stateful::{NoState, stateful};

let pct = use_state(0.0_f32);

div()
    .flex_col()
    .gap(4.0)
    .child(
        stateful::<NoState>()
            .deps([pct.signal_id()])
            .on_state({
                let pct = pct.clone();
                move |_ctx| {
                    div().flex_row().justify_between()
                        .child(text("Uploading..."))
                        .child(text(&format!("{}%", pct.get() as i32)))
                }
            })
    )
    .child(progress(&pct))
```

## Spinner

Loading indicator:

```rust
spinner()
```

### Spinner Sizes

```rust
spinner().size(SpinnerSize::Sm)   // Small
spinner().size(SpinnerSize::Md)   // Medium (default)
spinner().size(SpinnerSize::Lg)   // Large
```

### In Button

```rust
button(if is_loading { "" } else { "Save" })
    .loading(is_loading)
    .disabled(is_loading)
```

## Skeleton

Placeholder for loading content:

```rust
skeleton().w(200.0).h(20.0)
```

### Card Skeleton

```rust
card()
    .child(card_header()
        .child(skeleton().w(150.0).h(24.0))  // Title placeholder
        .child(skeleton().w(200.0).h(16.0))) // Description placeholder
    .child(card_content()
        .child(skeleton().w_full().h(100.0))) // Content placeholder
```

### List Skeleton

```rust
div()
    .flex_col()
    .gap(12.0)
    .child(
        div().flex_row().gap(12.0)
            .child(skeleton().w(48.0).h(48.0).rounded_full())  // Avatar
            .child(
                div().flex_col().gap(4.0)
                    .child(skeleton().w(150.0).h(16.0))  // Name
                    .child(skeleton().w(100.0).h(14.0))) // Subtitle
    )
    // Repeat for more items...
```

## Toast

Temporary notifications:

```rust
// Show a toast
show_toast(
    toast()
        .title("Success")
        .description("Your changes have been saved.")
);

// With variant
show_toast(
    toast()
        .variant(ToastVariant::Destructive)
        .title("Error")
        .description("Failed to save changes.")
);
```

### Toast Variants

```rust
// Default
toast().title("Notification")

// Success
toast()
    .variant(ToastVariant::Success)
    .title("Success")

// Destructive/Error
toast()
    .variant(ToastVariant::Destructive)
    .title("Error")
```

### With Action

```rust
toast()
    .title("Event created")
    .description("Friday, February 10, 2024")
    .action(
        toast_action()
            .child(button("Undo").size(ButtonSize::Sm))
            .on_click(|| undo_action())
    )
```

### Toast Position

```rust
// Configure toast container position
toaster()
    .position(ToasterPosition::TopRight)  // TopLeft, TopRight, BottomLeft, BottomRight
```

## Examples

### Loading State

```rust
let is_loading = use_state_keyed("feedback_loading", || true);

if is_loading.get() {
    div()
        .flex_col()
        .items_center()
        .gap(16.0)
        .child(spinner().size(SpinnerSize::Lg))
        .child(text("Loading..."))
} else {
    // Actual content
}
```

### Form Submission Feedback

```rust
let status = use_state_keyed("form_status", || FormStatus::Idle);

div()
    .flex_col()
    .gap(16.0)
    .child(/* form fields */)
    .child(
        match status.get() {
            FormStatus::Idle => button("Submit").on_click(move |_| submit()),
            FormStatus::Submitting => button("").loading(true).disabled(true),
            FormStatus::Success => alert()
                .child(alert_title("Success"))
                .child(alert_description("Form submitted successfully!")),
            FormStatus::Error(msg) => alert()
                .variant(AlertVariant::Destructive)
                .child(alert_title("Error"))
                .child(alert_description(msg)),
        }
    )
```

### Notification Center

```rust
fn notify_success(message: &str) {
    show_toast(
        toast()
            .variant(ToastVariant::Success)
            .title("Success")
            .description(message)
            .duration(Duration::from_secs(5))
    );
}

fn notify_error(message: &str) {
    show_toast(
        toast()
            .variant(ToastVariant::Destructive)
            .title("Error")
            .description(message)
            .duration(Duration::from_secs(10))
    );
}
```
