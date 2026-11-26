# Gestures

`GestureExt` adds recognition of taps, long presses, and directional swipes to a `Div`.

```rust
use blinc::layout::prelude::*;
use blinc::layout::widgets::gesture::GestureExt;
use blinc::core::reactive::signal;

let message = signal(String::new());
let surface = div().class("gesture-surface")
    .child(text("Tap, hold or swipe"))
    .on_tap(move |_| message.set("Tapped".into()))
    .on_long_press(move |_| message.set("Long press".into()))
    .on_swipe(move |direction, _| message.set(format!("Swiped {direction:?}")));
```

Gesture callbacks update bindings. Use `computed` for derived labels and `effect` for external side effects. Each surface owns its recognizer (the code that detects the gesture). Gestures are pointer interactions. If a gesture performs an essential operation, provide a keyboard action with a label.

The recognizer adds no visual style and no public CSS class. Give its surface an application class, and use theme variables:

```css
.gesture-surface {
  padding: var(--space-6);
  background: var(--surface-elevated);
  color: var(--text-primary);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
}
```
