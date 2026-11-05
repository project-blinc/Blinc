# Element Query API

The Element Query API provides programmatic access to elements in the UI tree, enabling imperative operations like scrolling, focusing, reading bounds, and triggering updates.

## Overview

```rust
// Query an element by its string ID
let handle = ctx.query("my-element");

// Check if it exists
if handle.exists() {
    // Get computed bounds
    if let Some(bounds) = handle.bounds() {
        println!("Element at ({}, {}) size {}x{}",
            bounds.x, bounds.y, bounds.width, bounds.height);
    }

    // Scroll into view
    handle.scroll_into_view();

    // Focus the element
    handle.focus();
}
```

---

## Assigning Element IDs

To query an element, it must have a string ID assigned via `.id()`:

```rust
div()
    .id("sidebar")
    .w(250.0)
    .h_full()
    .child(
        div()
            .id("nav-item-home")
            .child(text("Home"))
    )
    .child(
        div()
            .id("nav-item-settings")
            .child(text("Settings"))
    )
```

IDs should be unique within your UI. Duplicate IDs will cause the last element to win.

---

## ElementHandle API

### Creation & Existence

```rust
// Get a handle - works even if element doesn't exist yet
let handle = ctx.query("my-element");

// Check if element exists in the tree
if handle.exists() {
    // Element is rendered
}

// Get the string ID
let id = handle.id();  // "my-element"
```

### Bounds & Visibility

```rust
// Get computed bounds after layout
if let Some(bounds) = handle.bounds() {
    println!("Position: ({}, {})", bounds.x, bounds.y);
    println!("Size: {}x{}", bounds.width, bounds.height);
}

// Check if visible in viewport
if handle.is_visible() {
    // Element intersects with window viewport
}
```

### Navigation

```rust
// Scroll element into view (smooth scroll)
handle.scroll_into_view();

// Focus the element (for inputs, updates EventRouter)
handle.focus();

// Remove focus
handle.blur();

// Check focus state
if handle.is_focused() {
    // Element has keyboard focus
}
```

### Tree Traversal

```rust
// Get parent element
if let Some(parent) = handle.parent() {
    println!("Parent ID: {}", parent.id());
}

// Iterate over ancestors (parent → grandparent → root)
for ancestor in handle.ancestors() {
    println!("Ancestor: {}", ancestor.id());
}
```

### Triggering Updates

ElementHandle provides three levels of update granularity:

```rust
// 1. Visual-only update (fastest - skips layout)
// Use for: background color, opacity, shadows, transforms
handle.mark_visual_dirty(
    RenderProps::default().with_background(Color::RED.into())
);

// 2. Subtree rebuild with new children
// Use for: structural changes where you know the new content
handle.mark_dirty_subtree(
    div().child(text("New content"))
);

// 3. Full rebuild (fallback)
// Triggers complete UI rebuild, diffing determines actual changes
handle.mark_dirty();
```

### Signal Integration

```rust
// Emit a signal to trigger reactive updates
// Only rebuilds stateful elements that depend on this signal
handle.emit_signal(my_signal_id);
```

### On-Ready Callbacks

Register callbacks that fire once after an element's first layout:

```rust
ctx.query("progress-bar").on_ready(|bounds| {
    // Element has been laid out
    println!("Progress bar width: {}", bounds.width);

    // Start an animation based on computed size
    progress_anim.lock().unwrap().set_target(bounds.width * 0.75);
});
```

On-ready callbacks:
- Fire only once per element ID
- Work even if element doesn't exist yet (callback queued)
- Survive tree rebuilds (tracked by string ID)

---

## Querying in Event Handlers

Inside event handlers, use the global `query()` function to get an `ElementHandle`:

```rust
use blinc::layout::prelude::*;

div()
    .on_click(|_| {
        // query() returns Option<ElementHandle> - None if element doesn't exist
        if let Some(handle) = query("my-element") {
            handle.scroll_into_view();
            handle.focus();
        }
    })
```

The `query()` function uses the global `BlincContextState` internally, so you don't need to capture any context or registry in your closures.

For simple operations like scroll and focus without needing the full handle:

```rust
use blinc::core::BlincContextState;

div()
    .on_click(|_| {
        // Direct access for simple operations
        if let Some(ctx) = BlincContextState::try_get() {
            ctx.scroll_element_into_view("my-element");
            ctx.set_focus(Some("my-input"));
        }
    })
```

---

## Use Cases

### Scroll to Element on Action

```rust
fn scrollable_list(ctx: &WindowedContext) -> impl ElementBuilder {
    div()
        .flex_col()
        .child(
            div()
                .on_click(|_| {
                    // Use query() to get handle and scroll
                    if let Some(handle) = query("list-bottom") {
                        handle.scroll_into_view();
                    }
                })
                .child(text("Jump to Bottom"))
        )
        .child(
            scroll()
                .h(400.0)
                .child(
                    div()
                        .flex_col()
                        .children((0..100).map(|i| {
                            div()
                                .id(format!("item-{}", i))
                                .child(text(format!("Item {}", i)))
                        }))
                        .child(
                            div().id("list-bottom").h(1.0)
                        )
                )
        )
}
```

### Focus Management

```rust
fn login_form(ctx: &WindowedContext) -> impl ElementBuilder {
    div()
        .flex_col()
        .gap(16.0)
        .child(
            text_input(ctx.use_state_keyed::<TextInputState>("username"))
                .id("username-input")
                .placeholder("Username")
        )
        .child(
            text_input(ctx.use_state_keyed::<TextInputState>("password"))
                .id("password-input")
                .placeholder("Password")
                .on_key_down(|evt| {
                    if evt.key_code == 9 && evt.shift {  // Shift+Tab
                        if let Some(handle) = query("username-input") {
                            handle.focus();
                        }
                    }
                })
        )
        .child(
            div()
                .on_click(|_| {
                    // Focus username on form reset
                    if let Some(handle) = query("username-input") {
                        handle.focus();
                    }
                })
                .child(text("Reset"))
        )
}
```

### Measure Element After Layout

```rust
fn responsive_card(ctx: &WindowedContext) -> impl ElementBuilder {
    let card_width = ctx.use_signal(0.0f32);

    // Register callback to measure after layout
    ctx.query("adaptive-card").on_ready(move |bounds| {
        // on_ready callback has access to bounds directly
        println!("Card width: {}", bounds.width);
    });

    let width = ctx.get(card_width).unwrap_or(0.0);
    let columns = if width > 600.0 { 3 } else if width > 400.0 { 2 } else { 1 };

    div()
        .id("adaptive-card")
        .w_full()
        .flex_wrap()
        .children((0..9).map(|i| {
            div()
                .w(pct(100.0 / columns as f32))
                .child(text(format!("Item {}", i)))
        }))
}
```

### Efficient Visual Updates

Use `mark_visual_dirty` for visual-only changes that don't affect layout:

```rust
fn highlight_on_selection(ctx: &WindowedContext, selected_id: Option<&str>) -> impl ElementBuilder {
    let selected = selected_id.map(|s| s.to_string());

    div()
        .flex_col()
        .children(["item-a", "item-b", "item-c"].iter().map(|id| {
            let is_selected = selected.as_deref() == Some(*id);
            let id_string = id.to_string();

            div()
                .id(*id)
                .p(12.0)
                .bg(if is_selected {
                    Color::rgba(0.2, 0.5, 1.0, 0.3)
                } else {
                    Color::TRANSPARENT
                })
                .on_click(move |_| {
                    // Visual-only update - skips layout recomputation
                    if let Some(handle) = query(&id_string) {
                        handle.mark_visual_dirty(
                            RenderProps::default()
                                .with_background(Color::rgba(0.2, 0.5, 1.0, 0.3).into())
                        );
                    }
                })
                .child(text(*id))
        }))
}
```

### Carousel with Snap Points

```rust
fn carousel(ctx: &WindowedContext, items: &[String]) -> impl ElementBuilder {
    let current_index = ctx.use_signal(0usize);

    div()
        .flex_col()
        .child(
            scroll()
                .id("carousel-scroll")
                .w(300.0)
                .h(200.0)
                .scroll_x()
                .child(
                    div()
                        .flex_row()
                        .children(items.iter().enumerate().map(|(i, item)| {
                            div()
                                .id(format!("slide-{}", i))
                                .w(300.0)
                                .h(200.0)
                                .flex_center()
                                .child(text(item))
                        }))
                )
        )
        .child(
            div()
                .flex_row()
                .justify_center()
                .gap(8.0)
                .children((0..items.len()).map(|i| {
                    div()
                        .circle(8.0)
                        .bg(Color::WHITE.with_alpha(0.5))
                        .on_click(move |_| {
                            if let Some(handle) = query(&format!("slide-{}", i)) {
                                handle.scroll_into_view();
                            }
                        })
                }))
        )
}

---

## Performance Considerations

### Update Granularity

Choose the right update method based on what changed:

| Method | When to Use | Layout Cost |
|--------|-------------|-------------|
| `mark_visual_dirty(props)` | Background, opacity, shadow, transform | None (visual only) |
| `mark_dirty_subtree(div)` | Children structure changed | Subtree only |
| `mark_dirty()` | Unknown changes, fallback | Full rebuild |
| `emit_signal(id)` | Signal-based state change | Targeted stateful |

### Avoid Frequent Queries in Render

```rust
// Bad: Query in render function (called every frame)
fn bad_example(ctx: &WindowedContext) -> impl ElementBuilder {
    let bounds = ctx.query("my-element").bounds();  // Called every render!
    // ...
}

// Good: Query in event handler or on_ready
fn good_example(ctx: &WindowedContext) -> impl ElementBuilder {
    div()
        .on_click(|_| {
            // query() is designed for use in event handlers
            if let Some(handle) = query("my-element") {
                let bounds = handle.bounds();
                // Use bounds...
            }
        })
}
```

### Use on_ready for Post-Layout Measurements

```rust
// The on_ready callback fires once after first layout
ctx.query("my-element").on_ready(|bounds| {
    // Safe to use bounds here - layout is complete
    setup_animations_based_on_size(bounds);
});
```

---

## Best Practices

1. **Assign meaningful IDs** - Use descriptive IDs like `"sidebar"`, `"submit-button"`, `"user-avatar"` rather than generic names.

2. **Prefer declarative state** - Use signals and reactive state for most UI updates. Use ElementHandle for imperative operations like scroll-to and focus.

3. **Use visual-only updates** - When only colors/opacity/shadows change, use `mark_visual_dirty()` to skip layout.

4. **Handle missing elements** - Always check `exists()` or handle `None` from `bounds()` when the element might not be rendered.

5. **Avoid ID collisions** - Each ID should be unique. Consider namespacing like `"dialog-submit"`, `"sidebar-nav-home"`.

6. **Use on_ready for measurements** - Don't assume bounds are available immediately. Use `on_ready` for post-layout operations.
