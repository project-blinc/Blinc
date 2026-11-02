# Component Library Overview

`blinc_cn` is a comprehensive component library for Blinc UI, inspired by [shadcn/ui](https://ui.shadcn.com/). It provides 40+ production-ready, themeable components built on top of `blinc_layout`.

## Installation

Add `blinc_cn` to your `Cargo.toml`:

```toml
[dependencies]
blinc_cn = { path = "path/to/blinc_cn" }
```

## Quick Start

Install the cn theme bundle with [`WindowedApp::run_with_theme`]. `cn_bundle()` is the framework's platform-detected
theme pre-loaded with `CN_STYLES`, so cn components look right out of the box without any manual `add_css` calls:

```rust
use blinc_app::prelude::*;
use blinc_cn::{cn_bundle, prelude::*};

fn main() -> Result<()> {
    let config = WindowConfig {
        title: "My cn app".to_string(),
        ..Default::default()
    };

    WindowedApp::run_with_theme(
        config,
        cn_bundle(),
        ColorScheme::Light,
        // Closure wrapper: on edition 2024, `fn build_ui(ctx) -> impl Element`
        // captures `ctx`'s lifetime in the return type, which breaks the
        // higher-ranked `FnMut` bound when passed by name. The closure
        // dodges this. See `WindowedApp::run` docs for the alternative
        // (`+ use<>` on the named fn).
        |ctx| build_ui(ctx),
    )
}

fn build_ui(ctx: &mut WindowedContext) -> impl ElementBuilder {
    div()
        .flex_col()
        .gap(16.0)
        .p(24.0)
        .child(
            card()
                .child(card_header()
                    .child(card_title("Welcome"))
                    .child(card_description("Get started with blinc_cn")))
                .child(card_content()
                    .child(text("Beautiful, accessible components.")))
                .child(card_footer()
                    .child(button("Get Started")))
        )
}
```

### Layering CSS overrides

Chain [`with_css`](../core/theming.md#themebundlewith_css) (or `with_css_file`) on the bundle to attach extra
stylesheets — they cascade after `CN_STYLES`, so they win on conflicts:

```rust
let bundle = cn_bundle()
    .with_css(r#"
        .cn-button--primary { border-radius: 0; }
        .cn-card { border-width: 2px; }
    "#)
    .with_css_file("./styles/brand.css");

WindowedApp::run_with_theme(config, bundle, ColorScheme::Light, |ctx| build_ui(ctx))
```

### Using a custom theme

If you want a different aesthetic, build your own [`ThemeBundle`](../core/theming.md#custom-theme-bundles) (light + dark
variants) and pass it instead. Component classes (`.cn-button`, `.cn-card`, etc.) keep working because the cn CSS
references theme tokens via `var()`.

## Design Principles

### Composable

Components are built from smaller primitives that can be combined:

```rust
// Compose dialog from parts
dialog()
    .child(dialog_trigger().child(button("Open")))
    .child(dialog_content()
        .child(dialog_header().child(dialog_title("Title")))
        .child(/* content */)
        .child(dialog_footer().child(button("Close"))))
```

### Themeable

All components use theme tokens and automatically support dark mode:

```rust
// Components adapt to theme automatically
button("Click me") // Uses theme.colors.primary

// Override theme
ThemeState::set_color_scheme(ColorScheme::Dark);
```

`CN_STYLES` gives the document a text colour, `:root { color: var(--text-primary) }`,
which text and icons that set none inherit, as on a web page. A `cn::icon`
without a colour of its own takes the colour around it.

### Accessible

Components include keyboard navigation and proper semantics:

- Focus management
- Keyboard shortcuts
- Screen reader support (planned)

## Component Categories

| Category | Components |
|----------|------------|
| **Buttons** | Button |
| **Cards** | Card, CardHeader, CardContent, CardFooter |
| **Dialogs** | Dialog, AlertDialog, Sheet, Drawer |
| **Forms** | Input, Textarea, Checkbox, Switch, Radio, Select, Slider |
| **Navigation** | Tabs, DropdownMenu, ContextMenu, Breadcrumb, Sidebar |
| **Feedback** | Alert, Badge, Progress, Spinner, Skeleton, Toast |
| **Layout** | Avatar, Separator, AspectRatio, ScrollArea, Accordion |
| **Data** | Tooltip, HoverCard, Popover, Chart |

## Prelude

Import common components with the prelude:

```rust
use blinc_cn::prelude::*;

// Includes:
// - All component builders (button, card, dialog, etc.)
// - Variant enums (ButtonVariant, AlertVariant, etc.)
// - Size enums (ButtonSize, AvatarSize, etc.)
// - Common types and traits
```

## Next Steps

- [Button](./button.md) - Learn about button variants and usage
- [Card](./card.md) - Build card-based layouts
- [Dialog](./dialog.md) - Create modal dialogs
- [Form Components](./form.md) - Build forms with inputs
