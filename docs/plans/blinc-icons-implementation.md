# blinc_icons Implementation Plan

## Overview

Implement a Lucide-based icon system for Blinc applications. This plan covers forking Lucide, generating Rust code from SVG files, and creating the `blinc_icons` crate with `cn::icon()` component integration.

## Source: Lucide Icons

- **Repository**: https://github.com/lucide-icons/lucide
- **License**: ISC (free for commercial and personal use)
- **Icon count**: 1000+ SVG icons
- **SVG location**: `/icons` directory in repository
- **Structure**: Each icon is a single SVG file with consistent 24x24 viewBox and stroke-based paths

### Why Lucide?

1. **Consistency**: All icons follow same design language (24x24, stroke width 2, rounded caps/joins)
2. **Quality**: Community-maintained, actively updated
3. **License**: ISC allows embedding without attribution requirements
4. **Shadcn alignment**: shadcn/ui uses Lucide, maintaining design consistency

---

## Architecture

```
blinc_icons/                    # New crate
├── Cargo.toml
├── build.rs                    # SVG → Rust codegen
├── assets/
│   └── lucide/                 # Forked SVG files (~1000 icons)
│       ├── arrow-right.svg
│       ├── check.svg
│       └── ...
└── src/
    ├── lib.rs                  # Public API
    ├── icon_data.rs            # Generated: static icon data
    ├── registry.rs             # Icon lookup by name
    └── provider.rs             # IconProvider trait

blinc_cn/
└── src/components/
    └── icon.rs                 # cn::icon() component
```

---

## Phase 1: Fork Lucide Icons

### 1.1 Create Fork

```bash
# Fork https://github.com/lucide-icons/lucide to organization
# Clone only the /icons directory (sparse checkout)
git clone --filter=blob:none --sparse https://github.com/anthropics/lucide-fork
cd lucide-fork
git sparse-checkout set icons
```

### 1.2 Icon Selection Strategy

To minimize binary size, we'll use a tiered approach:

| Tier | Icon Count | Use Case |
|------|------------|----------|
| **Core** | ~50 | Always included (arrows, check, x, plus, minus, etc.) |
| **Common** | ~150 | Default feature flag, covers most UI needs |
| **Full** | ~1000+ | Optional feature flag for complete set |

### 1.3 Core Icons List

Essential icons that should always be available:

```
Navigation: arrow-up, arrow-down, arrow-left, arrow-right, chevron-up, chevron-down, chevron-left, chevron-right
Actions: check, x, plus, minus, search, settings, edit, trash, copy, save
UI: menu, more-horizontal, more-vertical, external-link, link, eye, eye-off
Status: alert-circle, alert-triangle, info, check-circle, x-circle
Media: play, pause, stop, volume-2, volume-x, maximize, minimize
Files: file, folder, folder-open, download, upload
User: user, users, log-in, log-out
Misc: loader, refresh-cw, clock, calendar, star, heart
```

---

## Phase 2: Create blinc_icons Crate

### 2.1 Cargo.toml

```toml
[package]
name = "blinc_icons"
version = "0.1.0"
edition = "2021"
description = "Icon library for Blinc UI framework"
build = "build.rs"

[features]
default = ["common"]
core = []           # ~50 essential icons
common = ["core"]   # ~150 common icons (default)
full = ["common"]   # All ~1000+ icons

[build-dependencies]
walkdir = "2"
roxmltree = "0.20"  # SVG parsing

[dependencies]
# No runtime dependencies - pure data
```

### 2.2 Icon Data Structure

```rust
// src/lib.rs

/// Static icon data extracted from SVG
#[derive(Debug, Clone, Copy)]
pub struct IconData {
    /// Icon name (e.g., "arrow-right")
    pub name: &'static str,
    /// SVG path data (d attribute)
    pub path: &'static str,
    /// ViewBox: (min_x, min_y, width, height)
    pub view_box: (f32, f32, f32, f32),
    /// Default stroke width
    pub stroke_width: f32,
    /// Fill rule
    pub fill: IconFill,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IconFill {
    /// Stroke-based (Lucide default)
    Stroke,
    /// Fill-based
    Fill,
    /// Both stroke and fill
    Both,
}

impl IconData {
    /// Generate SVG string for this icon
    pub fn to_svg(&self, size: f32, color: &str) -> String {
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="{} {} {} {}" fill="none" stroke="{}" stroke-width="{}" stroke-linecap="round" stroke-linejoin="round">{}</svg>"#,
            size, size,
            self.view_box.0, self.view_box.1, self.view_box.2, self.view_box.3,
            color,
            self.stroke_width,
            self.path_elements()
        )
    }

    fn path_elements(&self) -> String {
        // Handle multiple paths if present
        self.path.split("|||")
            .map(|p| format!(r#"<path d="{}"/>"#, p))
            .collect::<Vec<_>>()
            .join("")
    }
}
```

### 2.3 Build Script (build.rs)

```rust
// build.rs
use roxmltree::Document;
use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=assets/lucide");

    let icons_dir = Path::new("assets/lucide");
    let out_path = Path::new("src/icon_data.rs");

    let mut output = String::from(
        "// Auto-generated from Lucide SVG files - DO NOT EDIT\n\n"
    );
    output.push_str("use crate::{IconData, IconFill};\n\n");

    // Collect all icons
    let mut icons = Vec::new();

    for entry in walkdir::WalkDir::new(icons_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "svg"))
    {
        let name = entry.path().file_stem().unwrap().to_str().unwrap();
        let content = fs::read_to_string(entry.path()).unwrap();

        if let Some(icon_data) = parse_svg(&content, name) {
            icons.push(icon_data);
        }
    }

    // Sort alphabetically
    icons.sort_by(|a, b| a.0.cmp(&b.0));

    // Generate static array
    output.push_str(&format!(
        "pub static ICONS: &[IconData] = &[\n"
    ));

    for (name, path, view_box, stroke_width) in &icons {
        output.push_str(&format!(
            "    IconData {{ name: \"{}\", path: r#\"{}\"#, view_box: ({:.1}, {:.1}, {:.1}, {:.1}), stroke_width: {:.1}, fill: IconFill::Stroke }},\n",
            name, path, view_box.0, view_box.1, view_box.2, view_box.3, stroke_width
        ));
    }

    output.push_str("];\n");

    fs::write(out_path, output).unwrap();
}

fn parse_svg(content: &str, _name: &str) -> Option<(String, String, (f32, f32, f32, f32), f32)> {
    let doc = Document::parse(content).ok()?;
    let svg = doc.root_element();

    // Parse viewBox
    let view_box = svg.attribute("viewBox")
        .map(|vb| {
            let parts: Vec<f32> = vb.split_whitespace()
                .filter_map(|s| s.parse().ok())
                .collect();
            if parts.len() == 4 {
                (parts[0], parts[1], parts[2], parts[3])
            } else {
                (0.0, 0.0, 24.0, 24.0)
            }
        })
        .unwrap_or((0.0, 0.0, 24.0, 24.0));

    // Parse stroke-width
    let stroke_width = svg.attribute("stroke-width")
        .and_then(|sw| sw.parse().ok())
        .unwrap_or(2.0);

    // Collect all path data
    let mut paths = Vec::new();
    for node in svg.descendants() {
        if node.tag_name().name() == "path" {
            if let Some(d) = node.attribute("d") {
                paths.push(d.to_string());
            }
        }
        // Handle other elements (line, circle, rect, polyline, polygon)
        // Convert them to path data
    }

    let name = svg.attribute("data-lucide")
        .or_else(|| Some(_name))
        .unwrap()
        .to_string();

    Some((name, paths.join("|||"), view_box, stroke_width))
}
```

### 2.4 Registry

```rust
// src/registry.rs

use crate::icon_data::ICONS;
use crate::IconData;
use std::collections::HashMap;
use std::sync::OnceLock;

static ICON_MAP: OnceLock<HashMap<&'static str, &'static IconData>> = OnceLock::new();

fn get_icon_map() -> &'static HashMap<&'static str, &'static IconData> {
    ICON_MAP.get_or_init(|| {
        ICONS.iter().map(|icon| (icon.name, icon)).collect()
    })
}

/// Look up an icon by name
pub fn get_icon(name: &str) -> Option<&'static IconData> {
    get_icon_map().get(name).copied()
}

/// List all available icon names
pub fn list_icons() -> impl Iterator<Item = &'static str> {
    ICONS.iter().map(|icon| icon.name)
}

/// Check if an icon exists
pub fn has_icon(name: &str) -> bool {
    get_icon_map().contains_key(name)
}
```

---

## Phase 3: cn::icon() Component

### 3.1 Icon Component API

```rust
// blinc_cn/src/components/icon.rs

use blinc_core::Color;
use blinc_icons::{get_icon, IconData};
use blinc_layout::prelude::*;
use blinc_theme::{ColorToken, ThemeState};

/// Icon size presets
#[derive(Debug, Clone, Copy, Default)]
pub enum IconSize {
    /// 12px
    ExtraSmall,
    /// 16px
    Small,
    /// 20px
    #[default]
    Medium,
    /// 24px
    Large,
    /// 32px
    ExtraLarge,
}

impl IconSize {
    pub fn pixels(&self) -> f32 {
        match self {
            IconSize::ExtraSmall => 12.0,
            IconSize::Small => 16.0,
            IconSize::Medium => 20.0,
            IconSize::Large => 24.0,
            IconSize::ExtraLarge => 32.0,
        }
    }
}

/// Icon component
pub struct Icon {
    inner: Div,
}

impl ElementBuilder for Icon {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.inner.build(tree)
    }
    // ... other trait methods
}

/// Builder for Icon
pub struct IconBuilder {
    name: String,
    size: IconSize,
    size_px: Option<f32>,
    color: Option<Color>,
    color_token: Option<ColorToken>,
    stroke_width: Option<f32>,
    rotation: f32,
    spin: bool,
}

impl IconBuilder {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            size: IconSize::default(),
            size_px: None,
            color: None,
            color_token: None,
            stroke_width: None,
            rotation: 0.0,
            spin: false,
        }
    }

    /// Set icon size preset
    pub fn size(mut self, size: IconSize) -> Self {
        self.size = size;
        self
    }

    /// Set icon size in pixels
    pub fn size_px(mut self, px: f32) -> Self {
        self.size_px = Some(px);
        self
    }

    /// Set color from theme token
    pub fn color(mut self, token: ColorToken) -> Self {
        self.color_token = Some(token);
        self
    }

    /// Set color directly
    pub fn color_value(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    /// Set stroke width (overrides default)
    pub fn stroke_width(mut self, width: f32) -> Self {
        self.stroke_width = Some(width);
        self
    }

    /// Rotate icon by degrees
    pub fn rotate(mut self, degrees: f32) -> Self {
        self.rotation = degrees;
        self
    }

    /// Enable continuous spin animation
    pub fn spin(mut self) -> Self {
        self.spin = true;
        self
    }

    /// Build the icon
    pub fn build(self) -> Icon {
        let theme = ThemeState::get();

        let size = self.size_px.unwrap_or_else(|| self.size.pixels());

        let color = self.color
            .or_else(|| self.color_token.map(|t| theme.color(t)))
            .unwrap_or_else(|| theme.color(ColorToken::TextPrimary));

        // Get icon data
        let icon_data = get_icon(&self.name);

        let inner = if let Some(data) = icon_data {
            let stroke_width = self.stroke_width.unwrap_or(data.stroke_width);
            let svg_str = data.to_svg_with_stroke(size, stroke_width);

            let mut container = div()
                .w(size)
                .h(size)
                .items_center()
                .justify_center();

            let svg_element = svg(&svg_str)
                .size(size, size)
                .color(color);

            // Apply rotation if set
            if self.rotation != 0.0 {
                // TODO: Apply rotation transform
            }

            // Apply spin animation if enabled
            if self.spin {
                // TODO: Apply continuous rotation animation
            }

            container = container.child(svg_element);
            container
        } else {
            // Fallback: empty placeholder or error indicator
            div()
                .w(size)
                .h(size)
                .bg(Color::from_hex(0xFF0000).with_alpha(0.2))
                .rounded(2.0)
        };

        Icon { inner }
    }
}

/// Create an icon by name
pub fn icon(name: impl Into<String>) -> IconBuilder {
    IconBuilder::new(name)
}

/// Create an icon from custom SVG path data
pub fn icon_custom(path: &str) -> CustomIconBuilder {
    CustomIconBuilder::new(path)
}
```

### 3.2 Integration with Other Components

```rust
// Button with icon
cn::button("Submit")
    .icon_left(cn::icon("arrow-right"))

// Icon-only button
cn::button_icon(cn::icon("settings"))

// Input with icon
cn::input()
    .icon_left(cn::icon("search"))
    .placeholder("Search...")

// Alert with icon
cn::alert("Warning")
    .icon(cn::icon("alert-triangle"))
    .variant(AlertVariant::Warning)

// Menu item with icon
cn::dropdown_menu()
    .item(cn::icon("edit"), "Edit", |_| {})
    .item(cn::icon("copy"), "Copy", |_| {})
    .separator()
    .item(cn::icon("trash"), "Delete", |_| {})
```

---

## Phase 4: Exports & Integration

### 4.1 blinc_cn Updates

```rust
// blinc_cn/src/lib.rs

pub mod cn {
    // ... existing exports
    pub use crate::components::icon::{icon, icon_custom, IconSize};
}

pub mod prelude {
    // ... existing exports
    pub use crate::components::icon::{icon, icon_custom, Icon, IconBuilder, IconSize};
}
```

### 4.2 Re-export from blinc_icons

```rust
// blinc_cn/Cargo.toml
[dependencies]
blinc_icons = { path = "../blinc_icons", features = ["common"] }

// blinc_cn/src/lib.rs
pub use blinc_icons::{get_icon, has_icon, list_icons, IconData};
```

---

## Implementation Steps

### Step 1: Setup blinc_icons crate
1. Create `crates/blinc_icons/` directory structure
2. Add Cargo.toml with build dependencies
3. Create stub lib.rs with IconData types

### Step 2: Fork and import Lucide
1. Clone Lucide icons directory locally
2. Copy to `assets/lucide/`
3. Add to .gitignore any temporary files

### Step 3: Implement build script
1. Write build.rs SVG parser
2. Generate icon_data.rs with static data
3. Test with a few icons first

### Step 4: Implement registry
1. Create HashMap-based lookup
2. Add list_icons() and has_icon()
3. Add tests

### Step 5: Create cn::icon() component
1. Implement IconBuilder in blinc_cn
2. Add size and color support
3. Test rendering

### Step 6: Add component integration
1. Update Button to accept icon prop
2. Update Input to accept icon prop
3. Update Alert, Menu items, etc.

### Step 7: Add to cn_demo
1. Create icon gallery section
2. Show all size variants
3. Show color variants
4. Show component integration examples

---

## Testing Plan

1. **Unit tests**: Icon lookup, SVG generation
2. **Visual tests**: cn_demo icon gallery
3. **Integration tests**: Icons in buttons, inputs, alerts
4. **Size tests**: Verify binary size with different feature flags

---

## Future Enhancements

1. **Icon search**: Fuzzy search by name/tags
2. **Custom icon packs**: Allow users to add their own icons
3. **Icon animations**: Entrance/exit animations
4. **Icon variants**: Filled vs outlined versions
5. **Tree-shaking**: Only include used icons (requires build-time analysis)

---

## References

- [Lucide Icons](https://lucide.dev/)
- [Lucide GitHub](https://github.com/lucide-icons/lucide)
- [shadcn/ui Icons](https://ui.shadcn.com/docs/components/icons)
- [blinc-cn-components.md](./blinc-cn-components.md) - Icon System section
