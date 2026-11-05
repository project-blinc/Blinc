# Dialog, Sheet and Drawer

Dialogs ask for a decision. Sheets and drawers place content beside the main
view. Install `cn_bundle()` as described in [Getting Started](overview.md)
to load their styles and motion.

The examples use these imports. `ctx` is the `WindowedContext` supplied to
the application builder:

```rust
use blinc_cn::prelude::*;
use blinc_layout::prelude::{div, text};
```

## Open a Dialog

Configure a builder and call `.show()` from an event handler:

```rust
button("Open dialog").on_click(|_| {
    dialog()
        .title("Continue?")
        .description("Review the details before continuing.")
        .content(|| div().child(text("Your current selection will be used.")))
        .confirm_text("Continue")
        .on_confirm(|| {
            toast_success("Confirmed").show();
        })
        .show();
})
```

`.show()` returns an `OverlayHandle`. Keep it when the application needs to
call `handle.close()`. The default footer has Cancel and Confirm buttons,
which close the dialog. Escape and a backdrop click also dismiss it.

`on_confirm` and `on_cancel` report their respective buttons.
`on_close` reports dismissal by any route, including Escape and the backdrop.
These callbacks take no arguments.

### Confirmation Dialog

Use `confirm_destructive(true)` for a destructive action:

```rust
let deleted = ctx.use_signal(false);

button("Delete draft")
    .variant(ButtonVariant::Destructive)
    .on_click(move |_| {
        dialog()
            .title("Delete draft?")
            .description("This removes the draft from this session.")
            .confirm_text("Delete")
            .cancel_text("Keep draft")
            .confirm_destructive(true)
            .on_confirm(move || deleted.set(true))
            .show();
    })
```

`alert_dialog()` is an acknowledgement dialog: it hides Cancel. It is not a
separate set of trigger, content or action components.

```rust
button("Show notice").on_click(|_| {
    alert_dialog()
        .title("Import complete")
        .description("Your files are ready to use.")
        .confirm_text("OK")
        .size(DialogSize::Small)
        .show();
})
```

## Form Dialog

Create editor data once and clone its handle into the deferred body. Input
changes can update a Signal that the confirmation callback reads:

```rust
use blinc_layout::widgets::text_input::text_input_data;

let name_data = text_input_data();
let name = ctx.use_signal(String::new());

button("Edit profile").on_click(move |_| {
    let name_data = name_data.clone();
    dialog()
        .title("Edit profile")
        .description("Update the name shown on your profile.")
        .content(move || {
            div().child(
                input(&name_data)
                    .label("Name")
                    .placeholder("Your name")
                    .on_change(move |value| name.set(value.to_string())),
            )
        })
        .confirm_text("Save")
        .on_confirm(move || {
            toast_success("Profile saved")
                .description(name.get())
                .show();
        })
        .show();
})
```

The default footer suits this form. `.footer(|| div()...)` replaces the
entire footer, including its automatic Cancel and Confirm buttons; custom
actions must arrange dismissal themselves.

## Text That Changes

Titles, descriptions, `confirm_text` and `cancel_text` accept strings,
Signals or Computeds. Bind them before showing the dialog:

```rust
let remaining = ctx.use_signal("3 files".to_string());

button("Review files").on_click(move |_| {
    dialog()
        .title("Selected files")
        .description(remaining)
        .content(move || {
            div().child(button("Remove one from selection").on_click(move |_| {
                remaining.set("2 files".to_string());
            }))
        })
        .confirm_text("Done")
        .show();
})
```

The description changes in place while the dialog remains open.

## Sheet

A sheet can enter from any edge. The default is the right edge:

```rust
button("Open settings").on_click(|_| {
    sheet()
        .side(SheetSide::Right)
        .size(SheetSize::Medium)
        .title("Settings")
        .description("Preferences for this workspace.")
        .content(|| div().child(text("Settings content")))
        .footer(|| div().child(muted("Changes are saved automatically.")))
        .show();
})
```

`sheet_left()`, `sheet_right()`, `sheet_top()` and `sheet_bottom()` select the
edge directly. `SheetSize` is `Small`, `Medium`, `Large` or `Full`.
`.show_close(false)` hides the close button; `.on_close(|| ...)` observes
dismissal. The title and description accept reactive values.

Sheet and Drawer close controls are standard ghost icon buttons. Their existing
`.cn-sheet-close` and `.cn-drawer-close` classes remain available for styling.

## Drawer

A drawer is a left or right navigation panel, with the left as the default:

```rust
button("Open navigation").on_click(|_| {
    drawer_left()
        .size(DrawerSize::Medium)
        .title("Workspace")
        .child(|| div().child(text("Projects and recent activity")))
        .footer(|| div().child(muted("Signed in as Jane")))
        .show();
})
```

`DrawerSide` has `Left` and `Right`; `DrawerSize` has `Narrow`, `Medium` and
`Wide`. Use a bottom sheet for a panel entering from below. `.header(|| ...)`
replaces the title header. Each `.child(|| ...)` adds a body section.

When opened with the keyboard, a dialog focuses its first enabled control.
Tab and Shift+Tab stay inside the topmost dialog or nested popup. Closing
restores focus to the opening control if it still exists and is enabled.
Pointer focus uses the same target without a keyboard outline; keyboard
navigation uses the shared `:focus-visible` rules.

## Styling and Motion

The panel classes are `.cn-dialog`, `.cn-sheet` and `.cn-drawer`. Dialog text
uses `.cn-dialog-title` and `.cn-dialog-description`; sheets use
`.cn-sheet-title` and `.cn-sheet-description`; drawers use `.cn-drawer-title`.
Add a panel class with `.class("...")` and override it in application CSS.

The overlay wrapper carries `data-motion="enter"` or `"exit"`. It remains
mounted while its exit animation plays. The backdrop is `.overlay-backdrop`.

| Overlay | Motion wrapper | Default motion |
| --- | --- | --- |
| Dialog | `.cn-dialog-motion` | `cn-rise-in`, `cn-rise-out` |
| Sheet | `.cn-sheet-motion-{left,right,top,bottom}` | `cn-slide-from-{side}`, `cn-slide-to-{side}` |
| Drawer | `.cn-drawer-motion-{left,right}` | `cn-slide-from-{side}`, `cn-slide-to-{side}` |

Timing comes from theme tokens. Change it through rules:

```css
.cn-dialog-motion[data-motion="enter"] {
    animation: cn-rise-in 400ms var(--ease-sheet);
}
.cn-sheet-motion-right[data-motion="exit"] {
    animation: cn-slide-to-right var(--duration-fast) var(--ease-in) forwards;
}
```

## Builder Reference

| Builder | Content | Actions | Presentation |
| --- | --- | --- | --- |
| `dialog()` | `title`, `description`, `content` | `confirm_text`, `cancel_text`, `on_confirm`, `on_cancel`, `on_close`, `hide_cancel`, `footer` | `size`, `confirm_destructive`, `class`, `id` |
| `alert_dialog()` | `title`, `description` | `confirm_text`, `on_confirm` | `size` |
| `sheet()` | `title`, `description`, `content`, `footer` | `on_close`, `show_close` | `side`, `size` |
| `drawer()` | `title`, `header`, `child`, `child_builder`, `footer` | `on_close`, `show_close` | `side`, `size`, `class`, `id` |

All four open with `.show()`. Their builders have no `.open(signal)` or
`.on_open_change(...)` API. Dialog sizes are `Small` (400px maximum),
`Medium` (500px), `Large` (600px) and `Full` (800px).
