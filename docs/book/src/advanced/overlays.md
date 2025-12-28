# Overlay System

Blinc provides an overlay system for modals, dialogs, toasts, and context menus.

## Overview

Overlays render on top of the main UI and handle their own lifecycle. Access the overlay manager through the context:

```rust
ctx.overlay_manager()
```

## Modals

Full-screen overlays with backdrop:

```rust
ctx.overlay_manager()
    .modal()
    .title("Confirm Action")
    .content(|| {
        div()
            .flex_col()
            .gap(16.0)
            .child(text("Are you sure you want to proceed?"))
    })
    .show();
```

### Modal with Actions

```rust
ctx.overlay_manager()
    .modal()
    .title("Delete Item")
    .content(|| {
        text("This action cannot be undone.")
    })
    .primary_action("Delete", |_| {
        delete_item();
    })
    .secondary_action("Cancel", |_| {
        // Modal closes automatically
    })
    .show();
```

## Dialogs

Centered dialogs with customizable content:

```rust
ctx.overlay_manager()
    .dialog()
    .title("Settings")
    .content(|| build_settings_form())
    .primary_action("Save", |_| {
        save_settings();
    })
    .secondary_action("Cancel", |_| {})
    .show();
```

### Dialog Sizing

```rust
ctx.overlay_manager()
    .dialog()
    .width(600.0)
    .height(400.0)
    .title("Large Dialog")
    .content(|| content)
    .show();
```

## Toasts

Brief notifications:

```rust
// Simple toast
ctx.overlay_manager()
    .toast("Item saved successfully!")
    .show();

// With duration
ctx.overlay_manager()
    .toast("Processing...")
    .duration(5000)  // 5 seconds
    .show();

// Positioned
ctx.overlay_manager()
    .toast("Copied to clipboard")
    .position(ToastPosition::BottomCenter)
    .show();
```

### Toast Positions

```rust
ToastPosition::TopLeft
ToastPosition::TopCenter
ToastPosition::TopRight
ToastPosition::BottomLeft
ToastPosition::BottomCenter
ToastPosition::BottomRight
```

## Context Menus

Right-click menus:

```rust
div()
    .on_context_menu(|evt| {
        ctx.overlay_manager()
            .context_menu()
            .item("Copy", || copy_to_clipboard())
            .item("Paste", || paste_from_clipboard())
            .separator()
            .item("Delete", || delete_selected())
            .show_at(evt.mouse_x, evt.mouse_y);
    })
```

### Nested Menus

```rust
ctx.overlay_manager()
    .context_menu()
    .item("Edit", || {})
    .submenu("Export", |menu| {
        menu.item("PNG", || export_png())
            .item("JPEG", || export_jpeg())
            .item("SVG", || export_svg())
    })
    .show_at(x, y);
```

## Dismissing Overlays

Overlays close when:
- User clicks outside (backdrop click)
- Escape key is pressed
- Action callback completes
- Programmatically dismissed

```rust
let overlay_id = ctx.overlay_manager()
    .modal()
    .title("Loading...")
    .content(|| spinner())
    .show();

// Later, dismiss programmatically
ctx.overlay_manager().dismiss(overlay_id);
```

## Custom Overlay Content

For full control, use a custom overlay:

```rust
ctx.overlay_manager()
    .custom(|| {
        stack()
            .w_full()
            .h_full()
            // Backdrop
            .child(
                div()
                    .w_full()
                    .h_full()
                    .bg(Color::rgba(0.0, 0.0, 0.0, 0.5))
            )
            // Content
            .child(
                div()
                    .absolute()
                    .inset(0.0)
                    .flex_center()
                    .child(my_custom_modal())
            )
    })
    .show();
```

## Best Practices

1. **Use appropriate overlay type** - Modal for blocking actions, toast for notifications, dialog for forms.

2. **Provide escape routes** - Always include a way to close (cancel button, backdrop click).

3. **Keep toasts brief** - Short messages that don't require action.

4. **Position context menus near cursor** - Use event coordinates for natural placement.

5. **Limit overlay nesting** - Avoid opening overlays from within overlays.
