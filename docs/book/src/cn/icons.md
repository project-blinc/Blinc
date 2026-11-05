# Icons

`cn::icon` renders Lucide paths from `blinc_icons` and inherits its surrounding text colour.

```rust
use blinc_cn as cn;
use blinc_icons::icons;
use blinc_layout::prelude::*;

let icons = div().class("toolbar-icons")
    .child(cn::icon(icons::SEARCH).size(cn::IconSize::Large))
    .child(cn::icon(icons::SETTINGS).size(cn::IconSize::Large));
```

Size is a construction option; use `size_px` for a custom dimension. Use a button with a label or tooltip when the icon performs an action. Decorative icons are not independent keyboard stops.

Style `.cn-icon` or an application class on its parent. Keep colours in CSS so icons follow theme and scheme changes:

```css
.toolbar-icons { color: var(--primary); gap: var(--space-4); }
```
