# Theming

Blinc provides a comprehensive theming system with design tokens, light/dark mode support, animated theme transitions, and platform-native color scheme detection.

## Overview

The theming system is built around these core concepts:

- **Design Tokens**: Semantic color, typography, spacing, and radius values
- **ThemeState**: Global singleton for theme access and switching
- **Animated Transitions**: Smooth spring-based color interpolation between themes
- **Platform Detection**: Automatic system dark/light mode detection

---

## Quick Start

### Accessing Theme Tokens

```rust
use blinc_theme::{ThemeState, ColorToken};

fn my_component() -> impl ElementBuilder {
    let theme = ThemeState::get();

    // Get semantic colors
    let bg = theme.color(ColorToken::Background);
    let text = theme.color(ColorToken::TextPrimary);
    let primary = theme.color(ColorToken::Primary);

    // Get spacing values
    let padding = theme.spacing().space_4;

    // Get typography
    let font_size = theme.typography().text_base;

    // Get border radius
    let radius = theme.radii().radius_lg;

    div()
        .bg(bg)
        .p(padding)
        .rounded(radius)
        .child(
            text("Hello, themed world!")
                .size(font_size)
                .color(text)
        )
}
```

### Toggling Color Scheme

> ⚠️ **Known Limitation: Dynamic Theme Toggle**
>
> Dynamic theme switching at runtime (e.g., toggling between light/dark mode while the app is running) currently has significant limitations:
>
> - **Full UI rebuild required**: Theme changes trigger a complete UI tree rebuild, which is expensive and can cause visual glitches
> - **`on_ready` callbacks fire multiple times**: During theme animation, `on_ready` may fire repeatedly instead of once
> - **Animation ticks cause rebuilds**: Each frame of the theme transition animation triggers another rebuild
>
> **Recommendation**: For production apps, set the theme once at startup based on user preference or system settings. Theme changes should require an app restart.
>
> This limitation will be addressed in a future release with token-based color resolution that allows visual-only repaints without tree rebuilds.

```rust
// Toggle between light and dark mode
ThemeState::get().toggle_scheme();

// Or set explicitly
use blinc_theme::ColorScheme;
ThemeState::get().set_scheme(ColorScheme::Dark);
ThemeState::get().set_scheme(ColorScheme::Light);

// Check current scheme
let scheme = ThemeState::get().scheme();
match scheme {
    ColorScheme::Light => { /* ... */ }
    ColorScheme::Dark => { /* ... */ }
}
```

---

## Color Tokens

Color tokens provide semantic meaning to colors, making it easy to build consistent UIs that adapt to theme changes.

### Token Categories

| Category | Tokens | Description |
|----------|--------|-------------|
| **Brand** | `Primary`, `PrimaryHover`, `PrimaryActive`, `Secondary`, `SecondaryHover`, `SecondaryActive` | Main brand colors |
| **Semantic** | `Success`, `Warning`, `Error`, `Info` + `*Bg` variants | Status/feedback colors |
| **Surface** | `Background`, `Surface`, `SurfaceElevated`, `SurfaceOverlay` | Background layers |
| **Text** | `TextPrimary`, `TextSecondary`, `TextTertiary`, `TextInverse`, `TextLink` | Text colors |
| **Border** | `Border`, `BorderHover`, `BorderFocus`, `BorderError` | Border states |
| **Input** | `InputBg`, `InputBgHover`, `InputBgFocus`, `InputBgDisabled` | Form input backgrounds |
| **Selection** | `Selection`, `SelectionText` | Text selection colors |
| **Accent** | `Accent`, `AccentSubtle` | Accent highlights |

### Usage Example

```rust
use blinc_theme::{ThemeState, ColorToken};

fn themed_card() -> impl ElementBuilder {
    let theme = ThemeState::get();

    div()
        .bg(theme.color(ColorToken::Surface))
        .border(1.0, theme.color(ColorToken::Border))
        .rounded(theme.radii().radius_lg)
        .p(theme.spacing().space_4)
        .child(
            text("Card Title")
                .size(theme.typography().text_lg)
                .color(theme.color(ColorToken::TextPrimary))
        )
        .child(
            text("Card description text")
                .size(theme.typography().text_sm)
                .color(theme.color(ColorToken::TextSecondary))
        )
}
```

---

## Typography Tokens

Typography tokens define a consistent type scale:

| Token | Size | Use Case |
|-------|------|----------|
| `text_xs` | 12px | Captions, labels |
| `text_sm` | 14px | Secondary text, buttons |
| `text_base` | 16px | Body text |
| `text_lg` | 18px | Large body text |
| `text_xl` | 20px | Small headings |
| `text_2xl` | 24px | Section headings |
| `text_3xl` | 30px | Page headings |
| `text_4xl` | 36px | Large headings |
| `text_5xl` | 48px | Hero text |

```rust
let theme = ThemeState::get();
let typo = theme.typography();

text("Heading").size(typo.text_2xl)
text("Body").size(typo.text_base)
text("Caption").size(typo.text_xs)
```

---

## Spacing Tokens

Spacing follows a 4px base scale for consistent rhythm:

| Token | Value | Use Case |
|-------|-------|----------|
| `space_1` | 4px | Minimal spacing |
| `space_2` | 8px | Tight spacing |
| `space_2_5` | 10px | Between tight and standard |
| `space_3` | 12px | Standard small |
| `space_4` | 16px | Standard spacing |
| `space_5` | 20px | Medium spacing |
| `space_6` | 24px | Large spacing |
| `space_8` | 32px | Section spacing |
| `space_10` | 40px | Large section spacing |
| `space_12` | 48px | Extra large spacing |

```rust
let theme = ThemeState::get();
let spacing = theme.spacing();

div()
    .p(spacing.space_4)      // 16px padding
    .gap(spacing.space_3)    // 12px gap between children
    .my(spacing.space_6)     // 24px vertical margin
```

---

## Radius Tokens

Border radius tokens for consistent rounded corners:

| Token | Value | Use Case |
|-------|-------|----------|
| `radius_none` | 0px | Sharp corners |
| `radius_sm` | 4px | Subtle rounding |
| `radius_md` | 6px | Standard rounding |
| `radius_lg` | 8px | Pronounced rounding |
| `radius_xl` | 12px | Large rounding |
| `radius_2xl` | 16px | Extra large rounding |
| `radius_full` | 9999px | Pill shape |

```rust
let theme = ThemeState::get();

div().rounded(theme.radii().radius_lg)   // 8px corners
div().rounded(theme.radii().radius_full) // Pill shape
```

---

## Animated Theme Transitions

> ⚠️ **Experimental Feature**
>
> Animated theme transitions are currently experimental and have known issues. See the [Known Limitation](#toggling-color-scheme) above. For production use, disable animations and require app restart for theme changes.

When switching between light and dark mode, colors smoothly animate using spring physics. This happens automatically when you call `toggle_scheme()` or `set_scheme()`.

### How It Works

1. Theme colors are stored as `AnimatedValue` in the global `ThemeState`
2. When the scheme changes, target colors animate from current to new values
3. The animation scheduler drives smooth interpolation
4. UI rebuilds on each frame with interpolated colors (⚠️ this is the source of current performance issues)

### Configuration

The transition uses a gentle spring configuration for smooth, natural motion:

```rust
// Internal spring config for theme transitions
SpringConfig::gentle()  // stiffness: 120, damping: 14
```

### Reading Animated Colors

Colors are read during each render, automatically getting the interpolated value:

```rust
fn my_component() -> impl ElementBuilder {
    let theme = ThemeState::get();

    // This color will be interpolated during transitions
    let bg = theme.color(ColorToken::Background);

    div().bg(bg)
}
```

**Important**: Always read colors from `ThemeState` inside your component function, not captured in closures at initialization time. This ensures colors update during animations.

---

## Reactive Theme Updates

For interactive elements that need to respond to theme changes within event handlers, fetch colors inside the callback:

```rust
fn themed_button(ctx: &WindowedContext) -> impl ElementBuilder {
    let handle = ctx.use_state_for("btn", ButtonState::Idle);

    stateful(handle)
        .on_state(|state, div| {
            // Fetch colors inside callback for theme reactivity
            let theme = ThemeState::get();
            let primary = theme.color(ColorToken::Primary);
            let primary_hover = theme.color(ColorToken::PrimaryHover);

            match state {
                ButtonState::Idle => div.set_bg(primary),
                ButtonState::Hovered => div.set_bg(primary_hover),
                // ...
            }
        })
        .child(text("Click me"))
}
```

---

## Default Theme: Catppuccin

Blinc's default theme is derived from [Catppuccin](https://catppuccin.com/), a community-driven pastel theme:

- **Light mode**: Catppuccin Latte
- **Dark mode**: Catppuccin Mocha

### Latte (Light) Palette

| Role | Color |
|------|-------|
| Background | `#EFF1F5` |
| Surface | `#FFFFFF` |
| Text Primary | `#4C4F69` |
| Primary | `#1E66F5` (Blue) |
| Success | `#40A02B` (Green) |
| Warning | `#DF8E1D` (Yellow) |
| Error | `#D20F39` (Red) |

### Mocha (Dark) Palette

| Role | Color |
|------|-------|
| Background | `#1E1E2E` |
| Surface | `#313244` |
| Text Primary | `#CDD6F4` |
| Primary | `#89B4FA` (Blue) |
| Success | `#A6E3A1` (Green) |
| Warning | `#F9E2AF` (Yellow) |
| Error | `#F38BA8` (Red) |

---

## Platform Color Scheme Detection

Blinc automatically detects the system's preferred color scheme on supported platforms:

| Platform | Detection Method |
|----------|------------------|
| macOS | `AppleInterfaceStyle` from UserDefaults |
| Windows | Windows.UI.ViewManagement API |
| Linux | XDG/GTK settings |
| iOS | Native UITraitCollection |
| Android | Configuration.uiMode |

### Manual Detection

```rust
use blinc_theme::platform::detect_system_color_scheme;

// Get system preference
let scheme = detect_system_color_scheme();

// Initialize theme with system preference
ThemeState::init(BlincTheme::bundle(), scheme);
```

The `WindowedApp` automatically initializes the theme with system color scheme detection.

---

## System Scheme Watcher (Optional)

> ⚠️ **Not Recommended for Production**
>
> Due to the [dynamic theme toggle limitations](#toggling-color-scheme), the system scheme watcher is not recommended for production apps. When the system theme changes, it triggers the same problematic full UI rebuild. Consider detecting the system scheme once at startup instead.

For apps that need to automatically follow system theme changes (e.g., when the user toggles dark mode in system settings), Blinc provides an optional background watcher.

### Enabling the Feature

Add the `watcher` feature to your `Cargo.toml`:

```toml
[dependencies]
blinc_theme = { version = "0.1", features = ["watcher"] }
```

### Basic Usage

```rust
use blinc_theme::{SystemSchemeWatcher, WatcherConfig};
use std::time::Duration;

// Start watching with default interval (1 second)
let watcher = SystemSchemeWatcher::start();

// Or with a custom polling interval
let watcher = SystemSchemeWatcher::start_with_interval(Duration::from_secs(5));

// The watcher runs in a background thread and automatically updates
// ThemeState when the system color scheme changes.

// Stop watching when done (or let it drop)
// watcher.stop();
```

### Using WatcherConfig

```rust
use blinc_theme::WatcherConfig;
use std::time::Duration;

// Builder pattern for configuration
let watcher = WatcherConfig::new()
    .poll_interval(Duration::from_secs(2))  // Check every 2 seconds
    .auto_start(true)                        // Start immediately
    .build();
```

### How It Works

1. The watcher runs in a background thread named `blinc-scheme-watcher`
2. It polls the system color scheme at the configured interval
3. When a change is detected, it calls `ThemeState::set_scheme()` automatically
4. Theme transitions are animated smoothly using spring physics
5. The watcher is thread-safe and cleans up when dropped

### Use Cases

- **Desktop apps**: Follow system dark/light mode preference
- **Long-running apps**: Adapt to user changing system settings
- **Kiosk/display apps**: Automatically switch themes based on time of day (if OS supports scheduling)

### Performance Notes

- The default 1-second interval is a good balance between responsiveness and CPU usage
- For less critical apps, consider using 5-10 second intervals
- The watcher thread sleeps between checks, consuming minimal resources

---

## Dynamic Token Overrides

You can override individual tokens at runtime without changing the entire theme:

```rust
use blinc_theme::{ThemeState, ColorToken};
use blinc_core::Color;

// Override a specific color
ThemeState::get().set_color_override(
    ColorToken::Primary,
    Color::from_hex(0x6366F1)  // Custom brand color
);

// Remove override (revert to theme default)
ThemeState::get().remove_color_override(ColorToken::Primary);

// Clear all overrides
ThemeState::get().clear_overrides();
```

### Override Types

| Method | Triggers |
|--------|----------|
| `set_color_override()` | Repaint only |
| `set_spacing_override()` | Layout recompute |
| `set_radius_override()` | Repaint only |

---

## Building Themed Components

### Pattern 1: Direct Token Access

Best for simple components:

```rust
fn simple_badge(label: &str) -> impl ElementBuilder {
    let theme = ThemeState::get();

    div()
        .px(theme.spacing().space_2)
        .py(theme.spacing().space_1)
        .rounded(theme.radii().radius_md)
        .bg(theme.color(ColorToken::AccentSubtle))
        .child(
            text(label)
                .size(theme.typography().text_xs)
                .color(theme.color(ColorToken::Accent))
        )
}
```

### Pattern 2: Themed Config Struct

Best for complex widgets with many options:

```rust
pub struct CardConfig {
    pub padding: f32,
    pub radius: f32,
    pub show_shadow: bool,
}

impl CardConfig {
    pub fn themed() -> Self {
        let theme = ThemeState::get();
        Self {
            padding: theme.spacing().space_4,
            radius: theme.radii().radius_lg,
            show_shadow: true,
        }
    }
}
```

### Pattern 3: Color Token Parameters

For components that accept different color variants:

```rust
fn status_badge(label: &str, color_token: ColorToken) -> impl ElementBuilder {
    let theme = ThemeState::get();
    let color = theme.color(color_token);

    div()
        .px(theme.spacing().space_2)
        .py(theme.spacing().space_1)
        .rounded(theme.radii().radius_full)
        .bg(color.with_alpha(0.15))
        .child(
            text(label)
                .size(theme.typography().text_xs)
                .color(color)
        )
}

// Usage
status_badge("Success", ColorToken::Success)
status_badge("Warning", ColorToken::Warning)
status_badge("Error", ColorToken::Error)
```

---

## Best Practices

1. **Always use semantic tokens** - Use `ColorToken::Primary` instead of hardcoded colors for automatic theme support.

2. **Read colors at render time** - Access `ThemeState::get()` inside your component function, not at module level.

3. **Fetch in callbacks** - For `on_state` and other callbacks, fetch theme colors inside the callback to respond to theme changes.

4. **Use spacing scale** - Use `theme.spacing().space_*` for consistent visual rhythm.

5. **Match radius to context** - Use smaller radii for small elements, larger for cards and panels.

6. **Test both themes** - Always verify your UI looks good in both light and dark modes.

---

## Example: Complete Themed Component

```rust
use blinc_app::prelude::*;
use blinc_theme::{ThemeState, ColorToken};

fn notification_toast(
    ctx: &WindowedContext,
    message: &str,
    variant: ColorToken,
) -> impl ElementBuilder {
    let theme = ThemeState::get();
    let handle = ctx.use_state_for("toast", ButtonState::Idle);

    let bg_color = theme.color(variant);

    stateful(handle)
        .w(320.0)
        .p(theme.spacing().space_4)
        .rounded(theme.radii().radius_lg)
        .bg(bg_color.with_alpha(0.15))
        .border(1.0, bg_color.with_alpha(0.3))
        .shadow_md()
        .on_state(move |state, div| {
            let theme = ThemeState::get();
            let base = theme.color(variant);

            match state {
                ButtonState::Hovered => {
                    div.set_bg(base.with_alpha(0.2));
                }
                _ => {
                    div.set_bg(base.with_alpha(0.15));
                }
            }
        })
        .flex_row()
        .items_center()
        .gap(theme.spacing().space_3)
        .child(
            // Icon placeholder
            div()
                .w(24.0)
                .h(24.0)
                .rounded(theme.radii().radius_full)
                .bg(bg_color)
        )
        .child(
            text(message)
                .size(theme.typography().text_sm)
                .color(theme.color(ColorToken::TextPrimary))
        )
}

// Usage
notification_toast(ctx, "File saved successfully", ColorToken::Success)
notification_toast(ctx, "Network error occurred", ColorToken::Error)
notification_toast(ctx, "New update available", ColorToken::Info)
```
