# blinc_icons

> **Part of the [Blinc UI Framework](https://project-blinc.github.io/Blinc)**
>
> This crate is a component of Blinc, a GPU-accelerated UI framework for Rust.
> For full documentation and guides, visit the [Blinc documentation](https://project-blinc.github.io/Blinc).

[Lucide](https://lucide.dev/) icon library for Blinc UI.

## Overview

`blinc_icons` provides 1000+ icons from the Lucide icon set as compile-time constants. Icons are stored as SVG path strings for zero runtime cost.

## Features

- **1000+ Icons**: Complete Lucide icon set
- **Zero Cost**: Dead code elimination removes unused icons
- **SVG Output**: Generate complete SVG strings
- **Customizable**: Custom stroke width and colors

## Quick Start

```rust
use blinc_icons::icons;
use blinc_layout::prelude::*;

// Use in layout
svg(icons::CHECK)
    .size(24.0, 24.0)
    .color(Color::GREEN)

svg(icons::ARROW_RIGHT)
    .size(16.0, 16.0)
    .color(Color::BLUE)
```

## Available Icons

Icons are organized by category. Here are some examples:

### Navigation
```rust
icons::ARROW_LEFT
icons::ARROW_RIGHT
icons::ARROW_UP
icons::ARROW_DOWN
icons::CHEVRON_LEFT
icons::CHEVRON_RIGHT
icons::MENU
icons::X
```

### Actions
```rust
icons::CHECK
icons::PLUS
icons::MINUS
icons::EDIT
icons::TRASH
icons::COPY
icons::DOWNLOAD
icons::UPLOAD
icons::SEARCH
icons::SETTINGS
```

### Media
```rust
icons::PLAY
icons::PAUSE
icons::STOP
icons::VOLUME
icons::VOLUME_OFF
icons::IMAGE
icons::VIDEO
icons::MUSIC
```

### Communication
```rust
icons::MAIL
icons::MESSAGE_SQUARE
icons::PHONE
icons::SEND
icons::BELL
icons::AT_SIGN
```

### Files
```rust
icons::FILE
icons::FOLDER
icons::FOLDER_OPEN
icons::FILE_TEXT
icons::FILE_CODE
icons::SAVE
```

### User
```rust
icons::USER
icons::USERS
icons::USER_PLUS
icons::USER_MINUS
icons::LOG_IN
icons::LOG_OUT
```

## Generate SVG String

```rust
use blinc_icons::{icons, to_svg, to_svg_colored};

// Basic SVG (24x24, stroke-width 2)
let svg_string = to_svg(icons::CHECK);
// <svg xmlns="..." viewBox="0 0 24 24" ...>...</svg>

// Custom stroke width
let svg_string = to_svg_with_stroke(icons::CHECK, 1.5);

// With custom color
let svg_string = to_svg_colored(icons::CHECK, "#00ff00");
```

## Icon Constants

All icons are `&'static str` constants containing SVG path data:

```rust
// Example icon definition
pub const CHECK: &str = r#"<path d="M20 6 9 17l-5-5"/>"#;
```

## Full Icon List

See the [Lucide Icons](https://lucide.dev/icons/) website for the complete list of available icons. All icon names are converted to SCREAMING_SNAKE_CASE:

- `arrow-right` → `ARROW_RIGHT`
- `chevron-down` → `CHEVRON_DOWN`
- `file-text` → `FILE_TEXT`

## License

MIT OR Apache-2.0

Icons are from [Lucide](https://lucide.dev/) under ISC License.
