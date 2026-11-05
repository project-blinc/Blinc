# Component Library Overview

`blinc_cn` is a comprehensive component library for Blinc UI, inspired by [shadcn/ui](https://ui.shadcn.com/). It provides 40+ production-ready, themeable components built on top of `blinc_layout`.

## Installation

Enable the `cn` feature on `blinc`:

```toml
[dependencies]
blinc = { version = "0.6", features = ["cn"] }
```

## Quick Start

Application rules restyle ordinary component colours. Selected, active and
checked colours retain contrast through explicit colour importance. Use theme
variables or a variant's component-layer declarations to restyle those states;
see [Selected component colours](../core/css-styling.md#selected-component-colours).

Install the cn theme bundle with `WindowedApp::run_with_theme`. `cn_bundle()` is the framework's platform-detected
theme pre-loaded with `CN_STYLES`, so cn components look right out of the box without any manual `add_css` calls:

```rust
use blinc::app::prelude::{ColorScheme, Result, WindowConfig};
use blinc::app::windowed::WindowedApp;
use blinc::cn::{cn_bundle, prelude::*};
use blinc::layout::prelude::{div, text};

fn main() -> Result<()> {
    WindowedApp::run_with_theme(
        WindowConfig {
            title: "My cn app".to_string(),
            ..Default::default()
        },
        cn_bundle(),
        ColorScheme::Light,
        |_ctx| {
            div().flex_col().gap(4.0).p(6.0)
                .child(card()
                    .child(card_header()
                        .title("Welcome")
                        .description("Get started with blinc_cn"))
                    .child(card_content().child(text("Components styled by CSS.")))
                    .child(card_footer().child(button("Get Started"))))
        },
    )
}
```

### Layering CSS overrides

Chain [`with_css`](../core/theming.md#themebundlewith_css) (or `with_css_file`) on the bundle to attach extra
stylesheets. Application rules take precedence over the component layer:

```rust
use blinc::app::prelude::{ColorScheme, WindowConfig};
use blinc::app::windowed::WindowedApp;
use blinc::cn::{cn_bundle, prelude::*};
use blinc::layout::prelude::div;

let bundle = cn_bundle().with_css(r#"
    .cn-button--primary { border-radius: 0; }
    .cn-card { border-width: 2px; }
"#);

WindowedApp::run_with_theme(
    WindowConfig::default(),
    bundle,
    ColorScheme::Light,
    |_ctx| div().child(button("Get started")),
)
```

### Using a custom theme

If you want a different aesthetic, build your own [`ThemeBundle`](../core/theming.md#custom-theme-bundles) (light + dark
variants) and pass it instead. Component classes (`.cn-button`, `.cn-card`, etc.) keep working because the cn CSS
references theme tokens via `var()`.

## Design Principles

### Composable

Components are built from smaller primitives that can be combined:

```rust
use blinc::cn::prelude::*;
use blinc::layout::prelude::{div, text};

button("Open").on_click(|_| {
    dialog()
        .title("Title")
        .content(|| div().child(text("Dialog content")))
        .confirm_text("Close")
        .hide_cancel()
        .show();
})
```

### Themeable

All components use theme tokens and automatically support dark mode:

```rust
use blinc::cn::prelude::*;
use blinc::theme::{ColorScheme, ThemeState};

button("Use dark mode").on_click(|_| {
    ThemeState::get().set_scheme(ColorScheme::Dark);
})
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
| **Forms** | Input, Textarea, Checkbox, Switch, Radio, Select, Slider, [Date Picker, Time Picker, Colour Picker](pickers.md) |
| **Navigation** | Tabs, DropdownMenu, ContextMenu, Breadcrumb, Sidebar |
| **Feedback** | Alert, Badge, Progress, Spinner, Skeleton, Toast |
| **Layout** | Avatar, Separator, AspectRatio, ScrollArea, Accordion |
| **Data** | Tooltip, HoverCard, Popover, Chart |

## Prelude

Import common components with the prelude:

```rust
use blinc::cn::prelude::*;

// Includes:
// - All component builders (button, card, dialog, etc.)
// - Variant enums (ButtonVariant, AlertVariant, etc.)
// - Size enums (ButtonSize, AvatarSize, etc.)
// - Common types and traits
```

## Next Steps

- [Button](./button.md) - Learn about button variants and usage
- [Card](./card.md) - Build card-based layouts
- [Typography](./typography.md) - Headings, body copy and inline text
- [Dialog](./dialog.md) - Create modal dialogs
- [Form Components](./form.md) - Build forms with inputs
