# Gestures

`GestureExt` adds tap, long-press and directional swipe recognition to a `Div`.

```rust
use blinc_layout::prelude::*;
use blinc_layout::widgets::gesture::GestureExt;
use blinc_core::reactive::signal;

let message = signal(String::new());
let surface = div().class("gesture-surface")
    .child(text("Tap, hold or swipe"))
    .on_tap(move |_| message.set("Tapped".into()))
    .on_long_press(move |_| message.set("Long press".into()))
    .on_swipe(move |direction, _| message.set(format!("Swiped {direction:?}")));
```

Gesture callbacks update bindings; use `computed` for derived labels and `effect` for external side effects. Each surface owns its recognizer. Gestures are pointer interactions; provide a labelled keyboard action when a gesture performs an essential operation.

The recognizer adds no visual style or public CSS class. Give its surface an application class and use theme variables:

```css
.gesture-surface {
  padding: var(--space-6);
  background: var(--surface-elevated);
  color: var(--text-primary);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
}
```
