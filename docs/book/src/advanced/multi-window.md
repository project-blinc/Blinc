# Multi-Window Support

Blinc supports multiple windows on desktop platforms. Each window has its own UI tree, event router, and rendering surface while sharing the GPU device and animation scheduler.

## Opening Windows

Use `open_window_with()` to create a new window with a custom UI builder:

```rust
use blinc::app::windowed::open_window_with;

open_window_with(
    WindowConfig::new("Settings")
        .size(400, 300)
        .center(),
    |ctx| {
        div()
            .w(ctx.width)
            .h(ctx.height)
            .bg(Color::rgb(0.1, 0.1, 0.15))
            .child(text("Settings").size(24.0).color(Color::WHITE))
    },
);
```

The builder closure receives `&mut WindowedContext` with the window's dimensions and is called each frame when the tree needs rebuilding.

## Window Configuration

```rust
WindowConfig::new("My Window")
    .size(800, 600)              // Initial size
    .min_size(400, 300)          // Minimum dimensions
    .max_size(1920, 1080)        // Maximum dimensions
    .position(100, 100)          // Initial position
    .center()                    // Center on screen
    .resizable(true)             // Allow resizing
    .decorations(false)          // Frameless window
    .transparent(true)           // Transparent background
    .always_on_top(true)         // Stay above other windows
    .modal()                     // Block input to other windows
```

## Modal Windows

Modal windows block input to all other application windows until dismissed:

```rust
open_window_with(
    WindowConfig::new("Confirm")
        .size(360, 200)
        .center()
        .resizable(false)
        .modal(),
    |ctx| {
        let close = ctx.close_callback();
        div()
            .w(ctx.width).h(ctx.height)
            .child(text("Are you sure?"))
            .child(
                div().on_click(move |_| close())
                    .child(text("OK"))
            )
    },
);
```

## Custom Title Bars

For frameless windows, use `.drag_region()` to create a draggable title bar, and per-window callbacks for window controls:

```rust
open_window_with(
    WindowConfig::new("").size(400, 300).decorations(false),
    |ctx| {
        let drag = ctx.drag_callback();
        let minimize = ctx.minimize_callback();
        let maximize = ctx.maximize_callback();
        let close = ctx.close_callback();

        div()
            .w(ctx.width).h(ctx.height)
            .flex_col()
            // Custom title bar
            .child(
                div().w_full().h(36.0)
                    .flex_row().items_center()
                    // Drag zone (sibling of buttons, not parent)
                    .child(
                        div().flex_grow().h_full()
                            .on_mouse_down(move |_| drag())
                            .child(text("My App"))
                    )
                    // Window controls
                    .child(div().on_click(move |_| minimize()).child(text("-")))
                    .child(div().on_click(move |_| maximize()).child(text("+")))
                    .child(div().on_click(move |_| close()).child(text("x")))
            )
            // Content
            .child(div().flex_grow().child(text("Content")))
    },
);
```

> **Important**: Make the drag zone and control buttons siblings (not parent-child) to prevent event bubbling from buttons triggering the drag.

## Window State Persistence

Save and restore window position/size across launches:

```rust
use blinc::app::window_state::{WindowStateStore, SavedWindowState};

let store = WindowStateStore::new("my_app");

// Load saved state
let mut config = WindowConfig::default();
if let Some(saved) = store.load("main") {
    config = saved.apply_to(config);
}

// Save state on close
store.save("main", &SavedWindowState {
    x: 100, y: 200, width: 800, height: 600, maximized: false,
});
```

## Per-Window Callbacks

Each `WindowedContext` provides window-specific action callbacks:

| Method | Description |
|--------|-------------|
| `ctx.close_callback()` | Returns `Arc<dyn Fn()>` that closes THIS window |
| `ctx.drag_callback()` | Returns `Arc<dyn Fn()>` that starts OS drag |
| `ctx.minimize_callback()` | Returns `Arc<dyn Fn()>` that minimizes |
| `ctx.maximize_callback()` | Returns `Arc<dyn Fn()>` that toggles maximize |
| `ctx.close()` | Close this window directly |
| `ctx.minimize()` | Minimize directly |
| `ctx.maximize()` | Toggle maximize directly |
| `ctx.open_window(config)` | Open a new window |
