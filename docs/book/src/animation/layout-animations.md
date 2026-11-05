# Layout Animations (FLIP)

Blinc provides two FLIP-based systems for animating layout changes:

1. **`animate_bounds()` (Rust API)** — animates position, size, or both, over the theme's motion tokens or on a spring. Used by components like the accordion, collapsible, tree view and sidebar.
2. **CSS FLIP transitions** — CSS-transition-driven, animates position via `transform`. Used for sortable lists and grids.

Both follow the same principle: layout runs once to compute final positions, then visual offsets animate elements from where they *were* to where they *are*.

## What is FLIP?

FLIP stands for **F**irst, **L**ast, **I**nvert, **P**lay:

1. **First** — Snapshot every element's bounds before the layout change.
2. **Last** — Compute the new layout after the change.
3. **Invert** — Apply an offset that moves each element back to where it was.
4. **Play** — Animate the offset from inverted back to zero (the final layout position).

The result: elements glide from their old positions to their new positions, even though the layout change happens instantly.

---

## animate_bounds (Rust API)

The primary way to add layout animations. Call `.animate_bounds()` on any `Div` with a `VisualAnimationConfig`:

```rust
use blinc::layout::visual_animation::VisualAnimationConfig;

div()
    .animate_bounds(
        VisualAnimationConfig::height()
            .with_key("my-panel")
            .clip_to_animated()
            .theme_timed(),
    )
```

`theme_timed()` eases over the theme's normal duration on its ease-out curve,
which is what cn's components use. For open and shut, put the closed geometry
in a rule keyed on an attribute the builder binds, and let the animation ease
the change:

```css
.my-panel[data-state="closed"] { height: 0; }
```

### VisualAnimationConfig Presets

| Preset | Animates | Use Case |
| --- | --- | --- |
| `height()` | Height | Accordion panels, collapsible content |
| `width()` | Width | Sidebar expand/collapse |
| `size()` | Width + Height | Containers that resize both axes |
| `position()` | X + Y | Items that shift when siblings change |
| `all()` | Position + Size | Full bounds animation |

### Builder Methods

```rust
VisualAnimationConfig::height()
    .with_key("unique-key")       // Stable identity across rebuilds (required in stateful)
    .clip_to_animated()           // Clip content to animated bounds during animation
    .theme_timed()                // The theme's normal duration and ease-out

// Timed
.timed(250, Easing::EaseOut)      // A duration in ms and a curve

// Spring presets
.gentle()                         // Slow, smooth (stiffness: 120, damping: 14)
.snappy()                         // Quick, responsive (stiffness: 300, damping: 20)
.stiff()                          // Fast, minimal overshoot (stiffness: 400, damping: 30)
.wobbly()                         // Bouncy, playful (stiffness: 180, damping: 12)
.with_spring(SpringConfig { .. }) // Custom spring

// Clipping
.clip_to_animated()               // Clip to animated size (hides overflow during collapse)
.clip_to_layout()                 // Clip to final layout size
.no_clip()                        // No clipping (content overflows during animation)

// Threshold
.with_threshold(2.0)              // Minimum px change to trigger animation (default: 1.0)
```

### Stable Keys

Inside stateful containers, elements get new `LayoutNodeId`s on every rebuild. The `.with_key()` method provides a stable string identity so the animation system can track an element across rebuilds and smoothly continue from its current visual position.

```rust
// Always use .with_key() inside stateful on_state closures
stateful_with_key::<NoState>("my-container")
    .on_state(move |ctx| {
        div()
            .animate_bounds(
                VisualAnimationConfig::height()
                    .with_key("content-panel")  // Survives rebuilds
                    .clip_to_animated()
                    .snappy(),
            )
    })
```

### How It Works

Unlike CSS animations that modify render properties, `animate_bounds` operates at the visual offset level and never touches the layout tree:

1. **Before rebuild**: Snapshot each element's bounds (keyed by stable key).
2. **After rebuild**: Taffy computes new layout (final positions).
3. **Detect changes**: Compare old bounds to new bounds per key.
4. **Create spring animations**: For each changed element, create `AnimatedValue` springs that start at the delta (old - new) and target 0.
5. **Each frame**: Spring values converge toward 0, visual offsets shrink, element glides to final position.

The key principle: **Taffy owns layout truth** — animations only apply visual offsets on top of layout. This means layout is always correct and animations are purely cosmetic.

### Example: Accordion (Height Animation)

An accordion animates the height of collapsible content panels. When a section opens, the content grows from 0 to its natural height. When it closes, it shrinks back.

```rust
use blinc::layout::visual_animation::VisualAnimationConfig;

// The outer accordion container — animates total height as sections open/close
let mut container = div()
    .flex_col()
    .overflow_clip()
    .animate_bounds(
        VisualAnimationConfig::height()
            .with_key("accordion-container")
            .clip_to_animated()
            .gentle(),
    );

// Each collapsible section — animates its own height
let collapsible = div()
    .flex_col()
    .overflow_clip()
    .animate_bounds(
        VisualAnimationConfig::height()
            .with_key(&format!("section-{}", key))
            .clip_to_animated()
            .gentle(),
    )
    .child(content())
    .when(!is_open, |d| d.h(0.0));  // Collapsed: height = 0

// Each item — animates position as siblings expand/collapse
let item = div()
    .flex_col()
    .animate_bounds(
        VisualAnimationConfig::position()
            .with_key(&format!("item-{}", key))
            .gentle(),
    )
    .child(trigger)
    .child(collapsible);

container = container.child(item);
```

The three animation layers work together:

- **Container** (`height`): Border and background smoothly grow/shrink to fit content.
- **Collapsible** (`height` + `clip_to_animated`): Content area smoothly expands from 0 height, clipped during animation.
- **Items** (`position`): Sibling items smoothly slide down/up as the collapsible content grows/shrinks.

### Example: Sidebar (Size Animation)

A sidebar animates width when collapsing from expanded (with labels) to collapsed (icons only):

```rust
use blinc::layout::visual_animation::VisualAnimationConfig;

// Items container — animates width + clips during collapse
let items = div()
    .flex_col()
    .w_fit()
    .overflow_clip()
    .animate_bounds(
        VisualAnimationConfig::all()
            .with_key("sidebar-items")
            .clip_to_animated()
            .snappy(),
    );

// Main content area — animates position and size as sidebar shrinks
let content = div()
    .flex_1()
    .overflow_clip()
    .animate_bounds(
        VisualAnimationConfig::all()
            .with_key("sidebar-content")
            .clip_to_animated()
            .snappy(),
    )
    .child(main_content);

// Outer layout
div().flex_row().w_full().h_full()
    .child(items)
    .child(content)
```

When the sidebar collapses:

1. The items container width shrinks (text labels disappear, only icons remain).
2. `clip_to_animated()` hides overflowing text during the width transition.
3. The main content area smoothly expands to fill the freed space.

### Clip Behavior

Clipping is essential for collapse/expand animations. Without it, content overflows during the transition:

- **`clip_to_animated()`** — Clips to the current animated size. Content is hidden as the element shrinks. Use for collapse/expand.
- **`clip_to_layout()`** — Clips to the final layout size. Content is visible during expansion but hidden during collapse.
- **`no_clip()`** — No clipping. Use for position-only animations where size doesn't change.

---

## CSS FLIP Transitions

For simpler reorder animations (sortable lists, grids), you can use CSS transitions on `transform`. This system activates automatically when elements with stable IDs move during a subtree rebuild.

### Enabling CSS FLIP

Two conditions must be met:

1. An element has a **stable string ID** (`.id("my-item")`).
2. A **CSS transition on `transform`** is defined for that element.

```css
.sort-item {
    transition: transform 200ms ease;
}
```

```rust
let items: Vec<Div> = data.iter().map(|item| {
    div()
        .id(&format!("item-{}", item.id))   // Stable identity
        .class("sort-item")                   // CSS transition on transform
        .child(text(&item.label))
}).collect();

div().children(items)
```

When `data` order changes and the container rebuilds, each `.sort-item` slides from its old position to its new one.

### How CSS FLIP Works

```text
1. update_flip_bounds()        — snapshot positions (keyed by element string ID)
2. Subtree rebuild             — recreate children from new data
3. compute_layout()            — taffy computes new positions
4. apply_flip_transitions()    — compare old vs new, create translate animations
5. tick_flip_animations(dt)    — advance by frame delta
6. apply_flip_animation_props()— apply current transform to render props
7. Render
```

FLIP animations are stored keyed by string element ID (not `LayoutNodeId`), so they survive subtree rebuilds. Elements with an existing `transform` (e.g., a dragged item) are automatically excluded.

### Customizing the Animation

The FLIP animation inherits transition properties from CSS:

```css
/* Slow, bouncy reorder */
.sort-item {
    transition: transform 500ms cubic-bezier(0.34, 1.56, 0.64, 1);
}

/* Fast, linear reorder */
.sort-item {
    transition: transform 100ms linear;
}

/* With delay */
.sort-item {
    transition: transform 300ms ease 50ms;
}
```

### Example: Sortable Grid

```css
.grid-item {
    width: 100px;
    height: 100px;
    border-radius: 12px;
    transition: transform 200ms ease;
}
```

FLIP computes dx/dy from bounding boxes, so horizontal and vertical movement are both animated automatically.

---

## Choosing Between the Two Systems

| Feature | `animate_bounds()` | CSS FLIP |
| --- | --- | --- |
| Animation engine | Spring physics | CSS timing functions (ease, linear) |
| Animates position | Yes | Yes |
| Animates size | Yes | No |
| Clip during animation | Yes (`clip_to_animated`) | No |
| Configuration | Rust builder API | CSS `transition` property |
| Best for | Accordions, sidebars, panels | Sortable lists, grid reorder |
| Identity tracking | `.with_key("...")` | `.id("...")` |

**Use `animate_bounds()`** when you need size animation (expand/collapse), content clipping, or spring physics.

**Use CSS FLIP** when you have a sortable list/grid where items only change position and you want CSS-controlled timing.
