# Elements & Layout

Blinc provides a set of core elements that can be composed to build any UI. All elements implement the `ElementBuilder` trait and use a fluent builder pattern.

## Core Elements

### Div - The Universal Container

`div()` is the primary building block. It's a flexible container that supports:
- Flexbox layout
- Background colors and materials
- Borders and shadows
- Event handling
- Child elements

```rust
div()
    .w(200.0)
    .h(100.0)
    .bg(Color::rgba(0.2, 0.2, 0.3, 1.0))
    .rounded(8.0)
    .flex_center()
    .child(text("Hello"))
```

### Text - Typography

`text(content)` renders text with customizable typography:

```rust
text("Hello, World!")
    .size(24.0)
    .weight(FontWeight::Bold)
    .color(Color::WHITE)
    .family("Inter")
```

**Text Properties:**
- `.size(px)` - Font size in pixels
- `.weight(FontWeight)` - Bold, SemiBold, Medium, Regular, Light
- `.color(Color)` - Text color
- `.family(name)` - Font family
- `.italic()` - Italic style
- `.underline()` - Underline decoration
- `.line_height(multiplier)` - Line height as multiplier of font size
- `.letter_spacing(px)` - Space between characters
- `.align(TextAlign)` - Left, Center, Right, Justify

**Typography Helpers:**

```rust
h1("Heading 1")      // 32px bold
h2("Heading 2")      // 28px bold
h3("Heading 3")      // 24px bold
h4("Heading 4")      // 20px semibold
h5("Heading 5")      // 16px semibold
h6("Heading 6")      // 14px semibold
p("Paragraph")       // 14px regular
caption("Caption")   // 12px regular
label("Label")       // 14px medium
muted("Muted text")  // Reduced opacity
b("Bold text")       // Bold weight
small("Small")       // 12px
```

### Stack - Overlapping Layers

`stack()` positions children on top of each other, useful for overlays and layered designs:

```rust
stack()
    .w(200.0)
    .h(200.0)
    // Background layer
    .child(
        div().w_full().h_full().bg(Color::BLUE)
    )
    // Foreground layer
    .child(
        div()
            .absolute()
            .right(10.0)
            .bottom(10.0)
            .w(50.0)
            .h(50.0)
            .bg(Color::RED)
    )
```

### Canvas - Custom Drawing

`canvas(render_fn)` provides direct GPU drawing access:

```rust
canvas(|ctx: &mut dyn DrawContext, bounds| {
    ctx.fill_rect(
        Rect::new(0.0, 0.0, bounds.width, bounds.height),
        CornerRadius::uniform(8.0),
        Brush::Solid(Color::RED),
    );
})
.w(200.0)
.h(100.0)
```

See [Canvas Drawing](../widgets/canvas.md) for more details.

### Image & SVG

```rust
// Raster images
image("path/to/image.png")
    .w(200.0)
    .h(150.0)
    .cover()  // Object-fit: cover

// SVG with tint
svg("path/to/icon.svg")
    .w(24.0)
    .h(24.0)
    .tint(Color::WHITE)
```

See [Images & SVG](../widgets/media.md) for more details.

---

## Layout System

Blinc uses Flexbox for layout, powered by [Taffy](https://github.com/DioxusLabs/taffy).

### Sizing

```rust
div()
    .w(200.0)           // Fixed width in pixels
    .h(100.0)           // Fixed height in pixels
    .w_full()           // 100% width
    .h_full()           // 100% height
    .w_auto()           // Auto width (content-based)
    .h_auto()           // Auto height (content-based)
    .w_fit()            // Shrink-wrap to content
    .size(200.0, 100.0) // Set both dimensions
    .square(100.0)      // Square element
    .min_w(50.0)        // Minimum width
    .max_w(500.0)       // Maximum width
    .min_h(50.0)        // Minimum height
    .max_h(300.0)       // Maximum height
    .aspect_ratio(16.0 / 9.0)  // Maintain aspect ratio
```

### Flex Container

```rust
div()
    .flex()             // Enable flexbox
    .flex_row()         // Horizontal layout (default)
    .flex_col()         // Vertical layout
    .flex_row_reverse() // Right to left
    .flex_col_reverse() // Bottom to top
    .flex_wrap()        // Wrap children
```

### Flex Items

```rust
div()
    .flex_grow()        // Grow to fill space (flex-grow: 1)
    .flex_shrink()      // Allow shrinking (flex-shrink: 1)
    .flex_shrink_0()    // Don't shrink (flex-shrink: 0)
    .flex_1()           // flex: 1 1 0% (grow and shrink)
    .flex_auto()        // flex: 1 1 auto
```

### Alignment

**Align Items** (cross-axis alignment):

```rust
div()
    .items_start()      // Align to start
    .items_center()     // Center alignment
    .items_end()        // Align to end
    .items_stretch()    // Stretch to fill
    .items_baseline()   // Align baselines
```

**Justify Content** (main-axis distribution):

```rust
div()
    .justify_start()    // Pack at start
    .justify_center()   // Center items
    .justify_end()      // Pack at end
    .justify_between()  // Space between items
    .justify_around()   // Space around items
    .justify_evenly()   // Equal spacing
```

**Convenience Methods:**

```rust
div().flex_center()     // Center both axes
div().flex_col().justify_center().items_center()  // Same as above
```

### Gap (Spacing Between Children)

```rust
div()
    .gap(16.0)          // Gap in pixels
    .gap_x(8.0)         // Horizontal gap only
    .gap_y(12.0)        // Vertical gap only
```

### Padding

Padding uses a 4px unit system (like Tailwind CSS):

```rust
div()
    .p(4.0)             // 16px padding all sides (4 * 4px)
    .px(2.0)            // 8px horizontal padding
    .py(3.0)            // 12px vertical padding
    .pt(1.0)            // 4px top padding
    .pr(2.0)            // 8px right padding
    .pb(3.0)            // 12px bottom padding
    .pl(4.0)            // 16px left padding
    .p_px(20.0)         // 20px (exact pixels, not units)
```

### Margin

Same unit system as padding:

```rust
div()
    .m(4.0)             // 16px margin all sides
    .mx(2.0)            // 8px horizontal margin
    .my(3.0)            // 12px vertical margin
    .mt(1.0)            // 4px top margin
    .mr(2.0)            // 8px right margin
    .mb(3.0)            // 12px bottom margin
    .ml(4.0)            // 16px left margin
    .mx_auto()          // Auto horizontal margins (centering)
```

### Positioning

```rust
div()
    .relative()         // Position relative
    .absolute()         // Position absolute
    .inset(10.0)        // 10px from all edges
    .top(20.0)          // 20px from top
    .right(20.0)        // 20px from right
    .bottom(20.0)       // 20px from bottom
    .left(20.0)         // 20px from left
```

### Overflow

```rust
div()
    .overflow_clip()    // Clip overflowing content
    .overflow_visible() // Allow overflow
    .overflow_scroll()  // Enable scrolling
```

---

## Common Layout Patterns

### Centered Content

```rust
div()
    .w_full()
    .h_full()
    .flex_center()
    .child(content)
```

### Sidebar Layout

```rust
div()
    .w_full()
    .h_full()
    .flex_row()
    .child(
        div().w(250.0).h_full()  // Sidebar
    )
    .child(
        div().flex_1().h_full()  // Main content
    )
```

### Card Grid

```rust
div()
    .w_full()
    .flex_row()
    .flex_wrap()
    .gap(16.0)
    .child(card().w(300.0))
    .child(card().w(300.0))
    .child(card().w(300.0))
```

### Header/Content/Footer

```rust
div()
    .w_full()
    .h_full()
    .flex_col()
    .child(
        div().h(60.0).w_full()  // Header
    )
    .child(
        div().flex_1().w_full() // Content (fills remaining)
    )
    .child(
        div().h(40.0).w_full()  // Footer
    )
```

### Horizontal Navigation

```rust
div()
    .w_full()
    .h(60.0)
    .flex_row()
    .items_center()
    .justify_between()
    .px(4.0)
    .child(logo())
    .child(
        div()
            .flex_row()
            .gap(24.0)
            .child(nav_item("Home"))
            .child(nav_item("About"))
            .child(nav_item("Contact"))
    )
```

---

## The `.child()` Pattern

Add children with `.child()`. For multiple children of the same type, use iterators:

```rust
// Single child
div().child(text("Hello"))

// Multiple children
div()
    .child(text("First"))
    .child(text("Second"))
    .child(text("Third"))

// From iterator
let items = vec!["Apple", "Banana", "Cherry"];
div().child(
    items.into_iter().map(|item| text(item))
)
```

---

## ElementBuilder Trait

All elements implement `ElementBuilder`:

```rust
pub trait ElementBuilder {
    fn build(self, tree: &mut LayoutTree) -> LayoutNodeId;
}
```

This allows composing any element type:

```rust
fn my_component() -> impl ElementBuilder {
    div().child(text("Hello"))
}

// Use it
div().child(my_component())
```
