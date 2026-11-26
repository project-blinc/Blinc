# Dialog, Sheet and Drawer

Dialogs ask for a decision. Sheets and drawers place content beside the main
view. To load their styles and motion, install `cn_bundle()` as described in
[Getting Started](overview.md).

The examples use these imports. `ctx` is the `WindowedContext` that the
application builder receives:

```rust
use blinc::cn::prelude::*;
use blinc::layout::prelude::{div, text};
```

## Open a Dialog

Configure a builder, then call `.show()` from an event handler:

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

`.show()` returns an `OverlayHandle`. If the application needs to call
`handle.close()`, keep the handle. The default footer has Cancel and Confirm
buttons, which close the dialog. Escape and a click on the backdrop (the layer
behind the dialog) also dismiss it.

`on_confirm` runs for the Confirm button, and `on_cancel` runs for the Cancel
button. When the dialog is dismissed by any route, `on_close` runs. These routes
include Escape and the backdrop. These callbacks take no arguments.

### Confirmation Dialog

For a destructive action, use `confirm_destructive(true)`:

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

`alert_dialog()` is an acknowledgment dialog, so it hides Cancel. It is not a
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

Create the editor data one time. Clone its handle into the deferred body (the
`content` closure, which runs later). Input changes can update a Signal that the
confirmation callback reads:

```rust
use blinc::layout::widgets::text_input::text_input_data;

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

The default footer is suitable for this form. `.footer(|| div()...)` replaces
the entire footer, which includes its automatic Cancel and Confirm buttons.
Custom actions must arrange dismissal themselves.

## Text That Changes

Titles, descriptions, `confirm_text` and `cancel_text` accept strings, Signals
or Computeds. Before you show the dialog, bind them:

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

While the dialog remains open, the description changes in place.

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
`.show_close(false)` hides the close button. To observe dismissal, use
`.on_close(|| ...)`. The title and description accept reactive values.

The close controls of Sheet and Drawer are standard ghost icon buttons. Their
existing `.cn-sheet-close` and `.cn-drawer-close` classes remain available for
styling.

## Drawer

A drawer is a navigation panel on the left or the right. The left side is the
default:

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

`DrawerSide` has `Left` and `Right`. `DrawerSize` has `Narrow`, `Medium` and
`Wide`. For a panel that enters from below, use a bottom sheet.
`.header(|| ...)` replaces the title header. Each `.child(|| ...)` adds a body
section.

## Initial Focus

To choose the control that has the focus first, use
`.initial_focus("control-id")`. The target must be visible, enabled and inside
the dialog. For example, a form can start with its second field:

```rust
let first = blinc::layout::widgets::text_input::text_input_state();
let last = blinc::layout::widgets::text_input::text_input_state();

dialog()
    .title("Edit profile")
    .initial_focus("profile-last-name")
    .content(move || {
        div().w_full().flex_col().gap_3()
            .child(input(&first).placeholder("First name"))
            .child(
                input(&last)
                    .placeholder("Last name")
                    .build_component()
                    .id("profile-last-name"),
            )
    })
    .show();
```

The outer ID of an input resolves to its editable field. If a container has
several focusable controls, it needs a more specific target ID.

If no named target is available, the dialog prefers a control marked with
`autofocus`. The normal fallback selects the first enabled control. If the panel
has no controls, the fallback selects the panel. This preference applies one
time for each opening.

The stack `OverlayBuilder` and the legacy modal and dialog builders also accept
`.initial_focus(...)`. `Div::autofocus()` marks a custom control or a container
with one focusable control.

Tab and Shift+Tab stay inside the topmost dialog or nested popup. If the opening
control still exists and is enabled, the focus returns to it when the dialog
closes. Pointer focus uses the same target without a keyboard outline. Keyboard
navigation uses the shared `:focus-visible` rules.

## Styling and Motion

The panel classes are `.cn-dialog`, `.cn-sheet` and `.cn-drawer`. Dialog text
uses `.cn-dialog-title` and `.cn-dialog-description`. Sheets use
`.cn-sheet-title` and `.cn-sheet-description`. Drawers use `.cn-drawer-title`.
Add a panel class with `.class("...")`, and override it in application CSS.

The overlay wrapper carries `data-motion="enter"` or `"exit"`. While its exit
animation plays, it remains mounted. Child transforms stay inside that motion,
so rotated icons slide with the panel. The backdrop is `.overlay-backdrop`.

| Overlay | Motion wrapper | Default motion |
| --- | --- | --- |
| Dialog | `.cn-dialog-motion` | `cn-rise-in`, `cn-rise-out` |
| Sheet | `.cn-sheet-motion-{left,right,top,bottom}` | `cn-slide-from-{side}`, `cn-slide-to-{side}` |
| Drawer | `.cn-drawer-motion-{left,right}` | `cn-slide-from-{side}`, `cn-slide-to-{side}` |

The timing comes from theme tokens. To change it, use rules:

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
