# Dialog

Dialogs display content in a modal overlay that requires user interaction.

## Basic Usage

```rust
use blinc_cn::prelude::*;
use blinc_core::use_state_keyed;

let is_open = use_state_keyed("dialog_open", || false);

dialog()
    .open(is_open.clone())
    .on_open_change({
        let is_open = is_open.clone();
        move |open| is_open.set(open)
    })
    .child(dialog_trigger()
        .child(button("Open Dialog")))
    .child(dialog_content()
        .child(dialog_header()
            .child(dialog_title("Dialog Title"))
            .child(dialog_description("Dialog description")))
        .child(text("Dialog content goes here."))
        .child(dialog_footer()
            .child(button("Close").on_click({
                let is_open = is_open.clone();
                move |_| is_open.set(false)
            }))))
```

## Dialog Parts

### dialog()

The root component that manages open state.

```rust
dialog()
    .open(is_open)
    .on_open_change(|open| set_open(open))
```

### dialog_trigger()

The element that opens the dialog when clicked.

```rust
dialog_trigger()
    .child(button("Open"))
```

### dialog_content()

The modal content container with backdrop.

```rust
dialog_content()
    .child(/* dialog parts */)
```

### dialog_header()

Contains title and description.

```rust
dialog_header()
    .child(dialog_title("Title"))
    .child(dialog_description("Description"))
```

### dialog_footer()

Contains action buttons.

```rust
dialog_footer()
    .child(button("Cancel").variant(ButtonVariant::Outline))
    .child(button("Confirm"))
```

### dialog_close()

A button that closes the dialog.

```rust
dialog_close()
    .child(button("Close"))
```

## Alert Dialog

For destructive or important confirmations:

```rust
let is_open = use_state_keyed("alert_dialog_open", || false);

alert_dialog()
    .open(is_open.clone())
    .on_open_change({
        let is_open = is_open.clone();
        move |open| is_open.set(open)
    })
    .child(alert_dialog_trigger()
        .child(button("Delete").variant(ButtonVariant::Destructive)))
    .child(alert_dialog_content()
        .child(alert_dialog_header()
            .child(alert_dialog_title("Are you sure?"))
            .child(alert_dialog_description(
                "This action cannot be undone."
            )))
        .child(alert_dialog_footer()
            .child(alert_dialog_cancel().child(button("Cancel")))
            .child(alert_dialog_action()
                .child(button("Delete").variant(ButtonVariant::Destructive)))))
```

## Sheet

A panel that slides in from the edge:

```rust
let is_open = use_state_keyed("sheet_open", || false);

sheet()
    .open(is_open.clone())
    .side(SheetSide::Right)  // Left, Right, Top, Bottom
    .on_open_change({
        let is_open = is_open.clone();
        move |open| is_open.set(open)
    })
    .child(sheet_trigger()
        .child(button("Open Sheet")))
    .child(sheet_content()
        .child(sheet_header()
            .child(sheet_title("Settings")))
        .child(/* content */)
        .child(sheet_footer()
            .child(button("Save changes"))))
```

## Drawer

A mobile-friendly bottom sheet:

```rust
let is_open = use_state_keyed("drawer_open", || false);

drawer()
    .open(is_open.clone())
    .on_open_change({
        let is_open = is_open.clone();
        move |open| is_open.set(open)
    })
    .child(drawer_trigger()
        .child(button("Open Drawer")))
    .child(drawer_content()
        .child(drawer_header()
            .child(drawer_title("Menu")))
        .child(/* content */))
```

## Motion

Dialogs, sheets and drawers enter and leave by rules of `CN_STYLES`. Each one's
wrapper carries `data-motion="enter"`, then `"exit"` while it closes, and stays
until its exit animation has played; the backdrop behind it,
`.overlay-backdrop`, fades in and out with it.

| Overlay | Wrapper class | Enter | Exit |
|---|---|---|---|
| dialog | `.cn-dialog-motion` | `cn-rise-in` | `cn-rise-out` |
| sheet | `.cn-sheet-motion-{left,right,top,bottom}` | `cn-slide-from-{side}` | `cn-slide-to-{side}` |
| drawer | `.cn-drawer-motion-{left,right}` | `cn-slide-from-{side}` | `cn-slide-to-{side}` |

A sheet or drawer slides by its own size (`translateX(100%)`). The timing is
the theme's motion tokens; a rule of your own changes it:

```css
.cn-dialog-motion[data-motion="enter"] {
    animation: cn-rise-in 400ms var(--ease-sheet);
}
.cn-sheet-motion-right[data-motion="exit"] {
    animation: cn-slide-to-right var(--duration-fast) var(--ease-in) forwards;
}
```

## Text that changes

A dialog's title, description and button text, a sheet's title and
description and a drawer's title each take a string or a signal. A signal
replaces the text in place while the overlay is open:

```rust
let remaining = signal("3 files".to_string());

cn::dialog()
    .title("Delete files?")
    .description(remaining)
    .confirm_text("Delete")
    .cancel_text("Keep")
    .confirm_destructive(true)
    .show();

remaining.set("2 files".to_string());
```

## Examples

### Form Dialog

```rust
let is_open = use_state_keyed("form_dialog_open", || false);
let name = use_state_keyed("form_dialog_name", || String::new());
let email = use_state_keyed("form_dialog_email", || String::new());

dialog()
    .open(is_open.clone())
    .on_open_change({
        let is_open = is_open.clone();
        move |open| is_open.set(open)
    })
    .child(dialog_trigger()
        .child(button("Edit Profile")))
    .child(dialog_content()
        .child(dialog_header()
            .child(dialog_title("Edit Profile"))
            .child(dialog_description("Update your profile information")))
        .child(
            div()
                .flex_col()
                .gap(16.0)
                .child(
                    div().flex_col().gap(4.0)
                        .child(label("Name"))
                        .child(input()
                            .value(&name)
                            .on_change({
                                let name = name.clone();
                                move |v| name.set(v)
                            }))
                )
                .child(
                    div().flex_col().gap(4.0)
                        .child(label("Email"))
                        .child(input()
                            .value(&email)
                            .on_change({
                                let email = email.clone();
                                move |v| email.set(v)
                            }))
                )
        )
        .child(dialog_footer()
            .child(dialog_close().child(
                button("Cancel").variant(ButtonVariant::Outline)
            ))
            .child(button("Save").on_click({
                let is_open = is_open.clone();
                move |_| {
                    save_profile();
                    is_open.set(false);
                }
            }))))
```

### Confirmation Dialog

```rust
let is_open = use_state_keyed("confirm_dialog_open", || false);

alert_dialog()
    .open(is_open.clone())
    .on_open_change({
        let is_open = is_open.clone();
        move |open| is_open.set(open)
    })
    .child(alert_dialog_trigger()
        .child(button("Delete Account").variant(ButtonVariant::Destructive)))
    .child(alert_dialog_content()
        .child(alert_dialog_header()
            .child(alert_dialog_title("Delete Account"))
            .child(alert_dialog_description(
                "Are you sure you want to delete your account? \
                 All your data will be permanently removed."
            )))
        .child(alert_dialog_footer()
            .child(alert_dialog_cancel().child(
                button("Cancel").variant(ButtonVariant::Outline)
            ))
            .child(alert_dialog_action().child(
                button("Delete")
                    .variant(ButtonVariant::Destructive)
                    .on_click(move |_| delete_account())
            ))))
```

## API Reference

### dialog()

| Prop | Type | Default | Description |
|------|------|---------|-------------|
| `open` | `bool` | `false` | Whether dialog is open |
| `on_open_change` | `Fn(bool)` | - | Called when open state changes |

### sheet()

| Prop | Type | Default | Description |
|------|------|---------|-------------|
| `open` | `bool` | `false` | Whether sheet is open |
| `side` | `SheetSide` | `Right` | Which side to slide from |
| `on_open_change` | `Fn(bool)` | - | Called when open state changes |

### SheetSide

```rust
enum SheetSide {
    Left,
    Right,
    Top,
    Bottom,
}
```
