# Building Reusable Components

This guide covers patterns for creating composable, reusable UI components in Blinc.

## Component Patterns

### Simple Function Components

The simplest pattern - a function returning an element:

```rust
fn card(title: &str) -> Div {
    div()
        .p(16.0)
        .rounded(12.0)
        .bg(Color::rgba(0.15, 0.15, 0.2, 1.0))
        .child(
            text(title)
                .size(18.0)
                .weight(FontWeight::SemiBold)
                .color(Color::WHITE)
        )
}

// Usage
div().child(card("My Card"))
```

### Components with Children

Accept generic children with `impl ElementBuilder`:

```rust
fn card_with_content<E: ElementBuilder>(title: &str, content: E) -> Div {
    div()
        .p(16.0)
        .rounded(12.0)
        .bg(Color::rgba(0.15, 0.15, 0.2, 1.0))
        .flex_col()
        .gap(12.0)
        .child(
            text(title)
                .size(18.0)
                .weight(FontWeight::SemiBold)
                .color(Color::WHITE)
        )
        .child(content)
}

// Usage
card_with_content("Settings",
    div()
        .flex_col()
        .gap(8.0)
        .child(text("Option 1"))
        .child(text("Option 2"))
)
```

### Components with Context

For components needing state or animations:

```rust
use blinc_layout::stateful::stateful;

fn counter_card(ctx: &WindowedContext) -> impl ElementBuilder {
    let count = ctx.use_state_keyed("counter_card_count", || 0i32);
    let card_handle = ctx.use_state(ButtonState::Idle);

    stateful(card_handle)
        .p(16.0)
        .rounded(12.0)
        .bg(Color::rgba(0.15, 0.15, 0.2, 1.0))
        .flex_col()
        .gap(12.0)
        .deps(&[count.signal_id()])
        .on_state(move |_state, container| {
            let current = count.get();
            container.merge(
                div()
                    .child(text(&format!("Count: {}", current)).color(Color::WHITE))
            );
        })
        .child(increment_btn(ctx, count))
}

fn increment_btn(ctx: &WindowedContext, count: State<i32>) -> impl ElementBuilder {
    let handle = ctx.use_state(ButtonState::Idle);

    stateful(handle)
        .px(16.0)
        .py(8.0)
        .rounded(8.0)
        .on_state(|state, div| {
            let bg = match state {
                ButtonState::Idle => Color::rgba(0.3, 0.5, 0.9, 1.0),
                ButtonState::Hovered => Color::rgba(0.4, 0.6, 1.0, 1.0),
                _ => Color::rgba(0.3, 0.5, 0.9, 1.0),
            };
            div.set_bg(bg);
        })
        .on_click(move |_| {
            count.update(|v| v + 1);
        })
        .child(text("+").color(Color::WHITE))
}
```

---

## Animated Components

Use `motion()` for components with spring animations:

```rust
use blinc_layout::motion::motion;

#[derive(BlincComponent)]
struct AnimatedCard {
    #[animation]
    scale: f32,
    #[animation]
    opacity: f32,
}

fn animated_card(ctx: &WindowedContext, title: &str) -> impl ElementBuilder {
    let scale = AnimatedCard::use_scale(ctx, 1.0, SpringConfig::snappy());
    let opacity = AnimatedCard::use_opacity(ctx, 1.0, SpringConfig::gentle());

    let hover_scale = Arc::clone(&scale);
    let leave_scale = Arc::clone(&scale);

    // motion() is a container - apply transforms to it, style the child
    motion()
        .scale(scale.lock().unwrap().get())
        .opacity(opacity.lock().unwrap().get())
        .on_hover_enter(move |_| {
            hover_scale.lock().unwrap().set_target(1.05);
        })
        .on_hover_leave(move |_| {
            leave_scale.lock().unwrap().set_target(1.0);
        })
        .child(
            div()
                .p(16.0)
                .rounded(12.0)
                .bg(Color::rgba(0.15, 0.15, 0.2, 1.0))
                .child(text(title).color(Color::WHITE))
        )
}
```

**Note:** For hover-only visual effects without animations, prefer `Stateful` instead - it's more efficient as it doesn't require continuous redraws.

---

## Stateful Components

Use `stateful(handle)` for components with visual states:

```rust
use blinc_layout::stateful::stateful;

fn interactive_card(ctx: &WindowedContext, title: &str) -> impl ElementBuilder {
    // Use use_state_for with title as key for reusable component
    let handle = ctx.use_state_for(title, ButtonState::Idle);

    stateful(handle)
        .p(16.0)
        .rounded(12.0)
        .on_state(|state, div| {
            let bg = match state {
                ButtonState::Idle => Color::rgba(0.15, 0.15, 0.2, 1.0),
                ButtonState::Hovered => Color::rgba(0.18, 0.18, 0.25, 1.0),
                ButtonState::Pressed => Color::rgba(0.12, 0.12, 0.16, 1.0),
                _ => Color::rgba(0.15, 0.15, 0.2, 1.0),
            };
            div.set_bg(bg);
        })
        .child(text(title).color(Color::WHITE))
}
```

---

## Builder Pattern

For highly configurable components:

```rust
pub struct CardBuilder {
    title: String,
    subtitle: Option<String>,
    icon: Option<String>,
    bg_color: Color,
    on_click: Option<Box<dyn Fn()>>,
}

impl CardBuilder {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            subtitle: None,
            icon: None,
            bg_color: Color::rgba(0.15, 0.15, 0.2, 1.0),
            on_click: None,
        }
    }

    pub fn subtitle(mut self, text: impl Into<String>) -> Self {
        self.subtitle = Some(text.into());
        self
    }

    pub fn icon(mut self, path: impl Into<String>) -> Self {
        self.icon = Some(path.into());
        self
    }

    pub fn bg(mut self, color: Color) -> Self {
        self.bg_color = color;
        self
    }

    pub fn build(self) -> Div {
        let mut card = div()
            .p(16.0)
            .rounded(12.0)
            .bg(self.bg_color)
            .flex_col()
            .gap(8.0);

        if let Some(icon_path) = self.icon {
            card = card.child(
                svg(&icon_path).w(24.0).h(24.0).tint(Color::WHITE)
            );
        }

        card = card.child(
            text(&self.title)
                .size(18.0)
                .weight(FontWeight::SemiBold)
                .color(Color::WHITE)
        );

        if let Some(sub) = self.subtitle {
            card = card.child(
                text(&sub)
                    .size(14.0)
                    .color(Color::rgba(0.6, 0.6, 0.7, 1.0))
            );
        }

        card
    }
}

// Usage
CardBuilder::new("Settings")
    .subtitle("Manage your preferences")
    .icon("icons/settings.svg")
    .build()
```

---

## Component Libraries

Organize related components in modules:

```rust
// src/components/cards.rs
pub mod cards {
    use blinc_app::prelude::*;

    pub fn simple_card(title: &str) -> Div {
        // ...
    }

    pub fn image_card(title: &str, image_url: &str) -> Div {
        // ...
    }

    pub fn action_card<F: Fn() + 'static>(title: &str, on_action: F) -> Div {
        // ...
    }
}

// src/components/mod.rs
pub mod cards;
pub mod buttons;
pub mod inputs;

// Usage
use crate::components::cards::*;
```

---

## Prop Structs

For components with many parameters:

```rust
pub struct NotificationProps {
    pub title: String,
    pub message: String,
    pub variant: NotificationVariant,
    pub dismissible: bool,
    pub on_dismiss: Option<Box<dyn Fn()>>,
}

pub enum NotificationVariant {
    Info,
    Success,
    Warning,
    Error,
}

pub fn notification(props: NotificationProps) -> Div {
    let (bg, icon) = match props.variant {
        NotificationVariant::Info => (Color::rgba(0.2, 0.4, 0.8, 1.0), "info.svg"),
        NotificationVariant::Success => (Color::rgba(0.2, 0.7, 0.4, 1.0), "check.svg"),
        NotificationVariant::Warning => (Color::rgba(0.8, 0.6, 0.2, 1.0), "warning.svg"),
        NotificationVariant::Error => (Color::rgba(0.8, 0.3, 0.3, 1.0), "error.svg"),
    };

    div()
        .p(16.0)
        .rounded(8.0)
        .bg(bg)
        .flex_row()
        .gap(12.0)
        .items_center()
        .child(svg(icon).w(20.0).h(20.0).tint(Color::WHITE))
        .child(
            div()
                .flex_1()
                .flex_col()
                .gap(4.0)
                .child(text(&props.title).weight(FontWeight::SemiBold).color(Color::WHITE))
                .child(text(&props.message).size(14.0).color(Color::rgba(1.0, 1.0, 1.0, 0.8)))
        )
}
```

---

## Best Practices

1. **Keep components focused** - One component, one responsibility.

2. **Use `impl ElementBuilder`** - For maximum flexibility in return types.

3. **Document public components** - Add doc comments explaining usage.

4. **Consistent naming** - Use descriptive names that indicate the component's purpose.

5. **Default sensible styles** - Provide good defaults, allow overrides.

6. **Separate stateless and stateful** - Pure components are easier to test and reuse.

7. **Use BlincComponent for state and animations** - Type-safe hooks for both `State<T>` and `SharedAnimatedValue` prevent key collisions.

8. **Use Stateful for visual states** - Hover, press, focus effects should use `Stateful` rather than signals.

9. **Use motion() for animated values** - Wrap animated content in `motion()` for proper redraws.
