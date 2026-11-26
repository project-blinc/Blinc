# Icons

For these examples, enable the `cn` and `icons` features on your `blinc` dependency.

`cn::icon` renders Lucide paths from `blinc_icons`. The icon inherits the color of the text around it.

```rust
use blinc::cn as cn;
use blinc::icons::icons;
use blinc::layout::prelude::*;

let icons = div().class("toolbar-icons")
    .child(cn::icon(icons::SEARCH).size(cn::IconSize::Large))
    .child(cn::icon(icons::SETTINGS).size(cn::IconSize::Large));
```

Size is a construction option. For a custom dimension, use `size_px`. If the icon performs an action, use a button with a label or a tooltip. Decorative icons are not independent keyboard stops.

Style `.cn-icon`, or an application class on its parent. Keep the colors in CSS. Then the icons follow changes of theme and color scheme:

```css
.toolbar-icons { color: var(--primary); gap: var(--space-4); }
```
