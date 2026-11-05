# Motion Containers

The `motion()` element provides declarative enter/exit animations for content. It's ideal for animated lists, page transitions, and conditional rendering.

## Basic Usage

```rust
use blinc::layout::motion::motion;

motion()
    .fade_in(300)     // Duration in milliseconds
    .child(my_content())
```

## Animation Presets

### Fade

```rust
motion()
    .fade_in(300)
    .fade_out(200)
    .child(content)
```

### Scale

```rust
motion()
    .scale_in(300)    // Scale from 0 to 1
    .scale_out(200)   // Scale from 1 to 0
    .child(content)
```

### Slide

```rust
use blinc::layout::motion::SlideDirection;

motion()
    .slide_in(SlideDirection::Left, 300)
    .slide_out(SlideDirection::Right, 200)
    .child(content)

// Available directions:
// SlideDirection::Top
// SlideDirection::Bottom
// SlideDirection::Left
// SlideDirection::Right
```

### Bounce

```rust
motion()
    .bounce_in(400)   // Bouncy entrance
    .bounce_out(200)
    .child(content)
```

### Pop

```rust
motion()
    .pop_in(300)      // Scale with overshoot
    .pop_out(200)
    .child(content)
```

---

## Combining Animations

Apply multiple effects:

```rust
motion()
    .fade_in(300)
    .scale_in(300)
    .child(content)
```

---

## Staggered Lists

Animate list items with delays between each:

```rust
use blinc::layout::motion::{motion, StaggerConfig, AnimationPreset};

let items = vec!["Item 1", "Item 2", "Item 3", "Item 4"];

motion()
    .stagger(
        StaggerConfig::new(100, AnimationPreset::fade_in(300))
    )
    .children(
        items.iter().map(|item| {
            div()
                .p(12.0)
                .bg(Color::rgba(0.2, 0.2, 0.25, 1.0))
                .child(text(*item).color(Color::WHITE))
        })
    )
```

### Stagger Configuration

```rust
StaggerConfig::new(delay_ms, preset)
    .reverse()          // Animate last to first
    .from_center()      // Animate from center outward
    .limit(10)          // Only stagger first N items
```

### Stagger Directions

```rust
// Forward (default): 0, 1, 2, 3, 4...
StaggerConfig::new(100, preset)

// Reverse: 4, 3, 2, 1, 0...
StaggerConfig::new(100, preset).reverse()

// From center: 2, 1/3, 0/4 (for 5 items)
StaggerConfig::new(100, preset).from_center()
```

---

## Binding to AnimatedValue

For continuous animation control, bind motion transforms to AnimatedValue:

```rust
fn pull_to_refresh(ctx: &WindowedContext) -> impl ElementBuilder {
    let offset_y = ctx.use_animated_value(0.0, SpringConfig::wobbly());
    let icon_scale = ctx.use_animated_value(0.5, SpringConfig::snappy());
    let icon_opacity = ctx.use_animated_value(0.0, SpringConfig::snappy());

    stack()
        // Refresh icon (behind content)
        .child(
            motion()
                .scale(icon_scale.clone())
                .opacity(icon_opacity.clone())
                .child(refresh_icon())
        )
        // Content (translates down to reveal icon)
        .child(
            motion()
                .translate_y(offset_y.clone())
                .child(content_list())
        )
}
```

---

## Example: Animated Card List

Use a stateful element with `.deps()` to react to visibility state changes:

```rust
fn animated_card_list(show_cards: State<bool>) -> impl ElementBuilder {
    stateful::<ButtonState>()
        .flex_col()
        .gap(16.0)
        .deps([show_cards.signal_id()])
        .on_state(move |ctx| {
            let visible = show_cards.get();
            let label = if visible { "Hide Cards" } else { "Show Cards" };

            let bg = match ctx.state() {
                ButtonState::Idle => Color::rgba(0.3, 0.5, 0.9, 1.0),
                ButtonState::Hovered => Color::rgba(0.4, 0.6, 1.0, 1.0),
                _ => Color::rgba(0.3, 0.5, 0.9, 1.0),
            };

            div().bg(bg).px(16.0).py(8.0).rounded(8.0)
                .child(text(label).color(Color::WHITE))
        })
        .on_click(move |_| {
            show_cards.update(|v| !v);
        })
        .child(card_list())
}

fn card_list() -> impl ElementBuilder {
    // Cards with staggered animation
    motion()
        .stagger(StaggerConfig::new(80, AnimationPreset::fade_in(300)))
        .children(
            (0..5).map(|i| {
                div()
                    .w(300.0)
                    .p(16.0)
                    .rounded(12.0)
                    .bg(Color::rgba(0.15, 0.15, 0.2, 1.0))
                    .child(text(&format!("Card {}", i + 1)).color(Color::WHITE))
            })
        )
}
```

---

## Example: Page Transition

Use a custom state type for page navigation:

```rust
use blinc::layout::stateful::{stateful, StateTransitions};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
enum Page {
    #[default]
    Home,
    Settings,
    Profile,
}

// Pages don't auto-transition - we change them programmatically
impl StateTransitions for Page {
    fn on_event(&self, _event: u32) -> Option<Self> {
        None  // No automatic transitions
    }
}

fn page_transition(current_page: State<u8>) -> impl ElementBuilder {
    stateful::<NoState>()
        .w_full()
        .h_full()
        .deps([current_page.signal_id()])
        .on_state(move |_ctx| {
            // Render different content based on current page signal
            let content = match current_page.get() {
                0 => div().child(text("Home Page").color(Color::WHITE)),
                1 => div().child(text("Settings Page").color(Color::WHITE)),
                _ => div().child(text("Profile Page").color(Color::WHITE)),
            };

            div().child(
                motion()
                    .fade_in(200)
                    .slide_in(SlideDirection::Right, 200)
                    .child(content)
            )
        })
}

// Navigate programmatically using a shared signal
fn nav_button(current_page: State<u8>, target: u8, label: &str) -> impl ElementBuilder {
    stateful::<ButtonState>()
        .px(16.0)
        .py(8.0)
        .rounded(8.0)
        .on_state(|ctx| {
            let bg = match ctx.state() {
                ButtonState::Idle => Color::rgba(0.3, 0.5, 0.9, 1.0),
                ButtonState::Hovered => Color::rgba(0.4, 0.6, 1.0, 1.0),
                _ => Color::rgba(0.3, 0.5, 0.9, 1.0),
            };
            div().bg(bg)
        })
        .on_click(move |_| {
            current_page.set(target);
        })
        .child(text(label).color(Color::WHITE))
}
```

---

## Motion vs Manual Animation

| Feature | Motion | AnimatedValue |
|---------|--------|---------------|
| Setup | Declarative | Imperative |
| Control | Preset-based | Full control |
| Enter/Exit | Built-in | Manual |
| Lists | Stagger support | Manual delays |
| Use case | Transitions | Interactive |

**Use motion for:**
- List item animations
- Page transitions
- Conditional content
- Staggered reveals

**Use AnimatedValue for:**
- Drag interactions
- Hover effects
- Custom physics
- Continuous binding
