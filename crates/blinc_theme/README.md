# blinc_theme

> **Part of the [Blinc UI Framework](https://project-blinc.github.io/Blinc)**
>
> This crate is a component of Blinc, a GPU-accelerated UI framework for Rust.
> For full documentation and guides, visit the [Blinc documentation](https://project-blinc.github.io/Blinc).

Theme system for Blinc UI with design tokens and platform-native themes.

## Overview

`blinc_theme` provides a comprehensive theming system with design tokens for colors, typography, spacing, and more. It supports automatic light/dark mode detection and platform-specific themes.

## Features

- **Design Tokens**: Colors, typography, spacing, radii, shadows, animations
- **Platform Themes**: Native look for macOS, Windows, Linux, iOS, Android
- **Color Schemes**: Automatic light/dark mode detection
- **Dynamic Updates**: Runtime theme changes without layout rebuild
- **Token Categories**: Visual tokens (repaint only) vs layout tokens

## Quick Start

```rust
use blinc_theme::{ThemeState, ColorScheme};

// Get current theme
let theme = ThemeState::current();

// Use theme tokens
let bg_color = theme.colors.background;
let text_color = theme.colors.foreground;
let radius = theme.radius.md;
let spacing = theme.spacing.lg;
```

## Color Tokens

```rust
use blinc_theme::ColorToken;

// Semantic colors
ColorToken::Background
ColorToken::Foreground
ColorToken::Primary
ColorToken::Secondary
ColorToken::Accent
ColorToken::Muted
ColorToken::MutedForeground
ColorToken::Destructive
ColorToken::Border
ColorToken::Ring
ColorToken::Card
ColorToken::CardForeground
ColorToken::Popover
ColorToken::PopoverForeground
```

## Typography Tokens

```rust
use blinc_theme::TypographyToken;

// Font sizes
TypographyToken::Xs    // 12px
TypographyToken::Sm    // 14px
TypographyToken::Base  // 16px
TypographyToken::Lg    // 18px
TypographyToken::Xl    // 20px
TypographyToken::Xl2   // 24px
TypographyToken::Xl3   // 30px
TypographyToken::Xl4   // 36px
```

## Spacing Tokens

```rust
use blinc_theme::SpacingToken;

SpacingToken::Xs    // 4px
SpacingToken::Sm    // 8px
SpacingToken::Md    // 12px
SpacingToken::Lg    // 16px
SpacingToken::Xl    // 24px
SpacingToken::Xl2   // 32px
SpacingToken::Xl3   // 48px
```

## Radius Tokens

```rust
use blinc_theme::RadiusToken;

RadiusToken::None   // 0px
RadiusToken::Sm     // 2px
RadiusToken::Md     // 6px
RadiusToken::Lg     // 8px
RadiusToken::Xl     // 12px
RadiusToken::Full   // 9999px (pill shape)
```

## Color Scheme

```rust
use blinc_theme::{ThemeState, ColorScheme, detect_system_color_scheme};

// Get system preference
let scheme = detect_system_color_scheme();

// Set color scheme
ThemeState::set_color_scheme(ColorScheme::Dark);
ThemeState::set_color_scheme(ColorScheme::Light);
ThemeState::set_color_scheme(ColorScheme::System); // Follow system

// Check current scheme
if ThemeState::is_dark_mode() {
    // Dark mode specific logic
}
```

## Custom Themes

```rust
use blinc_theme::{ThemeState, ThemeOverrides};

// Override specific tokens
ThemeState::set_overrides(ThemeOverrides {
    colors: ColorOverrides {
        primary: Some(Color::rgb(0.2, 0.5, 1.0)),
        accent: Some(Color::rgb(1.0, 0.5, 0.0)),
        ..Default::default()
    },
    ..Default::default()
});
```

## Platform Detection

```rust
use blinc_theme::Platform;

match Platform::current() {
    Platform::MacOS => { /* macOS specific */ }
    Platform::Windows => { /* Windows specific */ }
    Platform::Linux => { /* Linux specific */ }
    Platform::IOS => { /* iOS specific */ }
    Platform::Android => { /* Android specific */ }
    Platform::Web => { /* Web specific */ }
}
```

## Architecture

```
blinc_theme
├── tokens/
│   ├── colors.rs      # Color tokens
│   ├── typography.rs  # Typography tokens
│   ├── spacing.rs     # Spacing tokens
│   ├── radius.rs      # Border radius tokens
│   └── shadows.rs     # Shadow tokens
├── schemes/
│   ├── light.rs       # Light theme
│   └── dark.rs        # Dark theme
├── platforms/
│   ├── macos.rs       # macOS theme
│   ├── windows.rs     # Windows theme
│   └── ...
└── state.rs           # Global theme state
```

## License

MIT OR Apache-2.0
