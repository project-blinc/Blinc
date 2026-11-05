# Theming

Blinc provides a comprehensive theming system with design tokens, light/dark mode support, animated theme transitions, and platform-native color scheme detection.

## Overview

The theming system is built around these core concepts:

- **Design Tokens**: Semantic color, typography, spacing, and radius values
- **ThemeState**: Global singleton for theme access and switching
- **ThemeBundle**: Light + dark variants plus optional CSS, installed via `WindowedApp::run_with_theme`
- **Animated Transitions**: Smooth spring-based color interpolation between themes
- **Platform Detection**: Automatic system dark/light mode detection

---

## Installing a Theme

The cleanest entry point is [`WindowedApp::run_with_theme`], which installs a `ThemeBundle` before
the runtime's platform default has a chance to claim the global slot:

```rust
use blinc::app::prelude::*;
use blinc::theme::{BlincTheme, ColorScheme};

WindowedApp::run_with_theme(
    WindowConfig::default(),
    BlincTheme::bundle(),       // any ThemeBundle — `cn_bundle()`, `BlincTheme::bundle()`,
                                // or your own
    ColorScheme::Dark,
    // Closure wrapper rather than bare `build_ui`: on edition 2024 a
    // free fn `-> impl Element` captures its input lifetime in the
    // return type, which breaks the higher-ranked `FnMut` bound. See
    // `WindowedApp::run` docs for the alternative (`+ use<>` on the fn).
    |ctx| build_ui(ctx),
)
```

If you'd rather call `ThemeState::init` yourself (e.g. from inside the UI builder, or before a
mobile runner's `run`), that also works — `init` is idempotent. The first call populates the
global state; later calls swap the bundle, re-derive every token set, and fire the redraw
callback so any cached stylesheets re-parse against the new variables.

```rust
// Call from anywhere — including inside `ui_builder`
ThemeState::init(BlincTheme::bundle(), ColorScheme::Dark);
```

### Attaching CSS to a bundle

A `ThemeBundle` can carry one or more CSS sources that the runtime registers automatically
after the bundle is installed:

```rust
use blinc::theme::BlincTheme;

let bundle = BlincTheme::bundle()
    .with_css(r#"
        .my-card { border-radius: 12px; }
        button.primary { background: var(--primary); }
    "#)
    .with_css_file("./styles/overrides.css");

WindowedApp::run_with_theme(config, bundle, ColorScheme::Light, |ctx| build_ui(ctx))
```

Two builder methods on [`ThemeBundle`]:

| Method | Behaviour |
| --- | --- |
| `with_css(css: impl Into<String>)` | Append an inline CSS source. Multiple calls cascade in order. |
| `with_css_file(path: impl AsRef<Path>)` | Read a file and append its contents. Read errors are inlined as a `/* … */` comment so the failure surfaces in the parser. Not available on `wasm32`. |

Attached CSS sources are stashed in a process-wide queue and drained on the next frame's tree
build — they're in place before the first paint and resolve `var()` references against the
installed bundle's variables.

### `cn_bundle()`

When you're using `blinc_cn`, [`blinc::cn::cn_bundle()`] returns the platform default already
pre-loaded with the cn components' default stylesheet:

```rust
WindowedApp::run_with_theme(
    config,
    blinc::cn::cn_bundle(),       // platform theme + CN_STYLES
    ColorScheme::Dark,
    |ctx| build_ui(ctx),
)
```

Chain `.with_css(...)` / `.with_css_file(...)` on it to layer your own overrides after the
defaults.

---

## Quick Start

### Accessing Theme Tokens

```rust
use blinc::theme::{ThemeState, ColorToken};

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

Switching schemes is cheap — it animates the colour tokens via spring interpolation and triggers
a UI rebuild + CSS reparse so cached stylesheets resolve against the new variables.

```rust
// Toggle between light and dark mode
ThemeState::get().toggle_scheme();

// Or set explicitly
use blinc::theme::ColorScheme;
ThemeState::get().set_scheme(ColorScheme::Dark);
ThemeState::get().set_scheme(ColorScheme::Light);

// Check current scheme
let scheme = ThemeState::get().scheme();
match scheme {
    ColorScheme::Light => { /* ... */ }
    ColorScheme::Dark => { /* ... */ }
}
```

### Swapping Bundles at Runtime

Calling `ThemeState::init(new_bundle, scheme)` after the app is running replaces the active
bundle: every token set is re-derived, `color/spacing/radius_overrides` are cleared, the redraw
callback fires (which reparses cached CSS against the new variables), and any CSS the new
bundle carries via `with_css` is queued for registration on the next frame. Useful for
in-app theme pickers:

```rust
on_click(move |_| {
    let next = match ThemeState::get().scheme() {
        ColorScheme::Light => shadcn_bundle(),
        ColorScheme::Dark  => catppuccin_bundle(),
    };
    ThemeState::init(next, ThemeState::get().scheme());
});
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
use blinc::theme::{ThemeState, ColorToken};

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
fn themed_button() -> impl ElementBuilder {
    stateful::<ButtonState>()
        .on_state(|ctx| {
            // Fetch colors inside callback for theme reactivity
            let theme = ThemeState::get();
            let primary = theme.color(ColorToken::Primary);
            let primary_hover = theme.color(ColorToken::PrimaryHover);

            let bg = match ctx.state() {
                ButtonState::Idle => primary,
                ButtonState::Hovered => primary_hover,
                _ => primary,
            };
            div().bg(bg)
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
use blinc::theme::platform::detect_system_color_scheme;

// Get system preference
let scheme = detect_system_color_scheme();

// Initialize theme with system preference
ThemeState::init(BlincTheme::bundle(), scheme);
```

The `WindowedApp` automatically initializes the theme with system color scheme detection.

---

## System Scheme Watcher (Optional)

For apps that need to automatically follow system theme changes (e.g., when the user toggles dark mode in system settings), Blinc provides an optional background watcher.

### Enabling the Feature

Add the `theme-watcher` feature to your `Cargo.toml`:

```toml
[dependencies]
blinc = { version = "0.6", features = ["theme-watcher"] }
```

### Basic Usage

```rust
use blinc::theme::{SystemSchemeWatcher, WatcherConfig};
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
use blinc::theme::WatcherConfig;
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

## Custom Theme Bundles

A `ThemeBundle` is a `(name, light, dark)` triple plus any attached CSS. To build your own,
implement the [`Theme`] trait on two structs (one per scheme) and hand them to `ThemeBundle::new`:

```rust
use blinc::theme::{
    AnimationTokens, ColorScheme, ColorTokens, RadiusTokens, ShadowTokens, SpacingTokens, Theme,
    ThemeBundle, TypographyTokens,
};

#[derive(Clone)]
pub struct MyTheme {
    scheme: ColorScheme,
    colors: ColorTokens,
    typography: TypographyTokens,
    spacing: SpacingTokens,
    radii: RadiusTokens,
    shadows: ShadowTokens,
    animations: AnimationTokens,
}

impl MyTheme {
    pub fn light() -> Self {
        Self {
            scheme: ColorScheme::Light,
            colors: ColorTokens { /* … your tokens … */ ..Default::default() },
            typography: TypographyTokens::default(),
            spacing:    SpacingTokens::default(),
            radii:      RadiusTokens::default(),
            shadows:    ShadowTokens::light(),
            animations: AnimationTokens::default(),
        }
    }

    pub fn dark() -> Self { /* ditto with dark colors + ShadowTokens::dark() */ }

    pub fn bundle() -> ThemeBundle {
        ThemeBundle::new("MyTheme", Self::light(), Self::dark())
    }
}

impl Theme for MyTheme {
    fn name(&self) -> &str { "MyTheme" }
    fn color_scheme(&self) -> ColorScheme { self.scheme }
    fn colors(&self) -> &ColorTokens { &self.colors }
    fn typography(&self) -> &TypographyTokens { &self.typography }
    fn spacing(&self) -> &SpacingTokens { &self.spacing }
    fn radii(&self) -> &RadiusTokens { &self.radii }
    fn shadows(&self) -> &ShadowTokens { &self.shadows }
    fn animations(&self) -> &AnimationTokens { &self.animations }
}
```

Then install it with `run_with_theme`:

```rust
WindowedApp::run_with_theme(
    config,
    MyTheme::bundle().with_css(MY_STYLES),
    ColorScheme::Dark,
    |ctx| build_ui(ctx),
)
```

---

## Dynamic Token Overrides

You can override individual tokens at runtime without changing the entire theme:

```rust
use blinc::theme::{ThemeState, ColorToken};
use blinc::core::Color;

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
use blinc::app::prelude::*;
use blinc::theme::{ThemeState, ColorToken};

fn notification_toast(
    message: &str,
    variant: ColorToken,
) -> impl ElementBuilder {
    let theme = ThemeState::get();
    let bg_color = theme.color(variant);

    stateful::<ButtonState>()
        .w(320.0)
        .p(theme.spacing().space_4)
        .rounded(theme.radii().radius_lg)
        .bg(bg_color.with_alpha(0.15))
        .border(1.0, bg_color.with_alpha(0.3))
        .shadow_md()
        .on_state(move |ctx| {
            let theme = ThemeState::get();
            let base = theme.color(variant);

            let bg = match ctx.state() {
                ButtonState::Hovered => base.with_alpha(0.2),
                _ => base.with_alpha(0.15),
            };
            div().bg(bg)
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
notification_toast("File saved successfully", ColorToken::Success)
notification_toast("Network error occurred", ColorToken::Error)
notification_toast("New update available", ColorToken::Info)
```
