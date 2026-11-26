# Notched Surfaces

`notch` creates concave, rounded, and stepped corners. Configure the shape with the builder. Keep the colors of the surface in CSS.

```rust
use blinc::layout::prelude::*;

let fill = blinc::layout::css_parser::active_stylesheet()
    .and_then(|sheet| sheet.resolve_color("var(--shaped-panel-fill)"))
    .unwrap_or(blinc::core::Color::TRANSPARENT);
let panel = notch().w(280.0).h(160.0)
    .concave_tr(32.0).rounded_bl(24.0)
    .bg(fill)
    .child(div().child(text("A different kind of corner")));
```

```css
:root { --shaped-panel-fill: var(--primary); }
```

The custom canvas fill reads the root variable of the application at widget build time. The children of the widget are regular elements with their own classes and bindings. Unless you add interactive content to a shaped surface, it has no keyboard action.

## Interactive gallery

The component page adapts `examples/blinc_app_examples/examples/notch_demo.rs`. Hover the menu icons to reveal and slide the concave dropdown. The navigation bulge, angle cuts, peak, and scooped dock show different edge treatments in context.

The gallery exposes `--gallery-notch-radius` (8–40px), `--gallery-notch-bar`, and `--gallery-notch-active` as CSS variables. When you apply CSS, the geometry refreshes and the current demo instance stays:

```css
:root {
    --gallery-notch-radius: 24px;
    --gallery-notch-active: var(--success);
}
```
