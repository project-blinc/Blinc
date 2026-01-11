# blinc_cn Component Library Plan

A comprehensive shadcn-inspired component library built on `blinc_layout` primitives.

## Architecture

```
blinc_theme      - Design tokens (colors, spacing, radii, shadows, typography)
     ↓
blinc_layout     - Primitives (div, text, scroll, motion, stateful, etc.)
     ↓
blinc_cn         - Styled components (Button, Card, Dialog, etc.)
```

## Component Categories

Based on shadcn/ui's component library, organized by category.

### 1. Core (Priority: High)

These are fundamental components used throughout any application.

| Component | Primitives Used | Status |
|-----------|-----------------|--------|
| **Button** | Stateful<ButtonState>, text | ✅ Done |
| **Badge** | div, text | ✅ Done |
| **Card** | div, text | ✅ Done |
| **Separator** | div | ✅ Done |
| **Skeleton** | div with animation | ✅ Done |
| **Spinner** | canvas, AnimatedTimeline | ✅ Done |
| **Alert** | div, text, icon | ✅ Done |

### 2. Form Components (Priority: High)

Form inputs and controls.

| Component | Primitives Used | Status |
|-----------|-----------------|--------|
| **Input** | text_input | ✅ Done |
| **Label** | text | ✅ Done |
| **Textarea** | text_area | ✅ Done |
| **Checkbox** | Stateful, div, svg | ✅ Done |
| **Radio Group** | Stateful, div | ✅ Done |
| **Switch** | Stateful, motion | ✅ Done |
| **Slider** | Stateful, div | ✅ Done |
| **Select** | Stateful, div, scroll | ✅ Done |
| **Combobox** | text_input, scroll, overlay | ✅ Done |
| **Form** | div, validation | Planned |
| **Field** | div, label, input, error | Planned |

### 3. Data Display (Priority: Medium)

Components for displaying data.

| Component | Primitives Used | Status |
|-----------|-----------------|--------|
| **Avatar** | image, div, text | Planned |
| **Table** | div, scroll | Planned |
| **Data Table** | table, scroll, sorting | Planned |
| **Progress** | div | ✅ Done |
| **Calendar** | div, text, grid | Planned |
| **Chart** | svg, div | ✅ Done |
| **Tree View** | Stateful, div, motion | ✅ Done |

### 4. Feedback (Priority: Medium)

User feedback components.

| Component | Primitives Used | Status |
|-----------|-----------------|--------|
| **Alert** | div, text, icon | ✅ Done |
| **Toast** | overlay, motion | ✅ Done |
| **Tooltip** | overlay, motion | ✅ Done |
| **Popover** | overlay | ✅ Done |

### 5. Overlays (Priority: Medium)

Modal and overlay components.

| Component | Primitives Used | Status |
|-----------|-----------------|--------|
| **Dialog** | overlay, motion | ✅ Done |
| **Sheet** | overlay, motion | ✅ Done |
| **Drawer** | overlay, motion | ✅ Done |
| **Dropdown Menu** | overlay, scroll | ✅ Done |
| **Context Menu** | overlay, scroll | ✅ Done |
| **Menubar** | div, overlay | ✅ Done |
| **Hover Card** | overlay, motion | ✅ Done |

### 6. Navigation (Priority: Medium)

Navigation components.

| Component | Primitives Used | Status |
|-----------|-----------------|--------|
| **Tabs** | Stateful, div, text | ✅ Done |
| **Breadcrumb** | div, link | ✅ Done |
| **Pagination** | div, button | ✅ Done |
| **Navigation Menu** | div, overlay | ✅ Done |
| **Sidebar** | div, scroll, SharedAnimatedValue | ✅ Done |

### 7. Layout (Priority: Low)

Layout helpers.

| Component | Primitives Used | Status |
|-----------|-----------------|--------|
| **Accordion** | motion, SharedAnimatedValue | ✅ Done |
| **Collapsible** | motion, SharedAnimatedValue | ✅ Done |
| **Resizable** | div, drag | Planned |
| **Scroll Area** | scroll | Planned |
| **Aspect Ratio** | div | Planned |

### 8. Typography (Priority: Low)

Text components.

| Component | Primitives Used | Status |
|-----------|-----------------|--------|
| **Typography** | text, rich_text | Planned |
| **Kbd** | div, text | ✅ Done |

### 9. Icons (Priority: High)

Icon system with Lucide icons. See [blinc-icons-implementation.md](./blinc-icons-implementation.md) for detailed plan.

| Component | Primitives Used | Status  |
|-----------|-----------------|---------|
| **Icon**  | svg             | Planned |

## Icon System

### Icon Overview

The icon system provides a unified API for rendering icons from multiple icon libraries.
Icons are rendered using the `svg()` element primitive and support theming, sizing, and animations.

### Default Icon Library: Lucide

[Lucide](https://lucide.dev/) is the default icon library, forked and maintained as SVG assets.
Lucide provides 1500+ icons with consistent stroke width and sizing.

### Icon Crate Structure

```
blinc_icons/              # Icon library crate
├── Cargo.toml
├── src/
│   ├── lib.rs            # Main exports, Icon trait
│   ├── registry.rs       # Icon registry for lookups
│   └── libraries/
│       ├── mod.rs
│       ├── lucide.rs     # Lucide icons (default)
│       ├── heroicons.rs  # Heroicons (optional)
│       └── custom.rs     # User-defined icons
└── assets/
    └── lucide/           # Forked Lucide SVG files
        ├── arrow-right.svg
        ├── check.svg
        └── ...
```

### Icon Trait

```rust
/// Trait for icon providers
pub trait IconProvider {
    /// Get SVG path data for an icon by name
    fn get_icon(&self, name: &str) -> Option<IconData>;

    /// List all available icon names
    fn list_icons(&self) -> &[&str];
}

/// Icon data returned by providers
pub struct IconData {
    /// SVG path data (d attribute)
    pub path: &'static str,
    /// Default viewBox dimensions
    pub view_box: (f32, f32, f32, f32),
    /// Stroke width (for stroke-based icons like Lucide)
    pub stroke_width: Option<f32>,
    /// Fill rule
    pub fill_rule: FillRule,
}

pub enum FillRule {
    Stroke,     // Icons drawn with strokes (Lucide)
    Fill,       // Icons drawn with fills (some Heroicons)
    EvenOdd,    // Complex paths
}
```

### Icon Component API

```rust
// Basic usage with Lucide (default)
cn::icon("arrow-right")
    .size(IconSize::Medium)

// With explicit size in pixels
cn::icon("check")
    .size_px(24.0)

// With color from theme
cn::icon("alert-circle")
    .color(ColorToken::Destructive)

// With custom color
cn::icon("heart")
    .color_value(Color::RED)

// Animated icon
cn::icon("loader")
    .spin()  // Continuous rotation

cn::icon("chevron-down")
    .rotate(90.0)  // Static rotation

// From different library
cn::icon("arrow-right")
    .library(IconLibrary::Heroicons)

// Custom SVG path
cn::icon_custom("M12 2L2 7l10 5 10-5-10-5z")
    .view_box(0.0, 0.0, 24.0, 24.0)
```

### Icon Sizes

```rust
pub enum IconSize {
    ExtraSmall,  // 12px
    Small,       // 16px
    Medium,      // 20px (default)
    Large,       // 24px
    ExtraLarge,  // 32px
}
```

### Integration with Components

Icons integrate seamlessly with other components:

```rust
// Button with icon
cn::button("Submit")
    .icon_left(cn::icon("arrow-right"))

// Button with only icon
cn::button_icon(cn::icon("settings"))

// Alert with icon
cn::alert()
    .icon(cn::icon("alert-triangle"))
    .title("Warning")
    .description("Something went wrong")

// Input with icon
cn::input()
    .icon_left(cn::icon("search"))
    .placeholder("Search...")

// Badge with icon
cn::badge("New")
    .icon(cn::icon("sparkles"))
```

### Configuration

```rust
// Global icon configuration
IconConfig::set_default_library(IconLibrary::Lucide);
IconConfig::set_default_size(IconSize::Medium);
IconConfig::set_default_stroke_width(2.0);

// Register custom icon library
IconConfig::register_library("my-icons", MyIconProvider::new());
```

### Build-time Icon Inclusion

To minimize bundle size, icons can be included at build time:

```rust
// In build.rs or via feature flags
// Only include icons that are actually used

// Feature flags in Cargo.toml
[features]
lucide-full = []           # All 1500+ icons
lucide-common = []         # ~100 common icons (default)
lucide-arrows = []         # Arrow icons only
heroicons = []             # Include Heroicons
```

### Icon Generation (Build Script)

```rust
// build.rs generates Rust code from SVG files
fn main() {
    let icons_dir = "assets/lucide";
    let output = "src/libraries/lucide_generated.rs";

    // Parse SVGs and generate static icon data
    blinc_icons_codegen::generate(icons_dir, output);
}
```

### Example: Complete Icon Usage

```rust
use blinc_cn::prelude::*;
use blinc_icons::{icon, IconSize};

fn toolbar(ctx: &impl BlincContext) -> impl ElementBuilder {
    div().flex_row().gap(2.0).children([
        cn::button_icon(icon("bold")).on_click(|_| {}),
        cn::button_icon(icon("italic")).on_click(|_| {}),
        cn::button_icon(icon("underline")).on_click(|_| {}),
        cn::separator().vertical(),
        cn::button_icon(icon("align-left")).on_click(|_| {}),
        cn::button_icon(icon("align-center")).on_click(|_| {}),
        cn::button_icon(icon("align-right")).on_click(|_| {}),
    ])
}
```

## Component API Patterns

### Variant Pattern

Every component that has visual variations should use enums:

```rust
// Variant enum for visual style
pub enum ButtonVariant {
    Primary,    // Main action
    Secondary,  // Secondary action
    Destructive,// Dangerous action
    Outline,    // Border only
    Ghost,      // Minimal
    Link,       // Text only
}

// Size enum for dimensions
pub enum ButtonSize {
    Small,
    Medium,     // Default
    Large,
    Icon,
}

// Usage
cn::button("Submit")
    .variant(ButtonVariant::Primary)
    .size(ButtonSize::Medium)
```

### Builder Pattern

All components use fluent builders:

```rust
cn::card()
    .title("Card Title")
    .description("Description")
    .footer(cn::button("Action"))
```

### State Pattern

Interactive components use `Stateful<T>`:

```rust
// Component defines its states
pub enum CheckboxState {
    Unchecked,
    Checked,
    Indeterminate,
}

// Stateful wraps for automatic state management
cn::checkbox()
    .checked(true)
    .on_change(|checked| {...})
```

### Theme Integration

Components use theme tokens directly:

```rust
// Use theme tokens for consistent styling
let theme = ThemeState::get();
let radius = theme.radius(RadiusToken::Md);
let spacing = theme.spacing_value(SpacingToken::Space4);
let color = theme.color(ColorToken::Primary);
```

## Implementation Order

### Phase 1: Core Components
1. Badge
2. Card
3. Separator
4. Alert

### Phase 2: Form Components
1. Input (wraps text_input)
2. Checkbox
3. Switch
4. Label
5. Field (input + label + error)

### Phase 3: Feedback
1. Toast (uses existing overlay system)
2. Tooltip
3. Progress

### Phase 4: Overlays
1. Dialog
2. Dropdown Menu
3. Popover

### Phase 5: Navigation
1. Tabs
2. Breadcrumb
3. Pagination

### Phase 6: Advanced
1. Select
2. Combobox
3. Data Table
4. Accordion
5. Calendar

## File Structure

```
crates/blinc_cn/
├── Cargo.toml
├── src/
│   ├── lib.rs           # Main exports
│   └── components/
│       ├── mod.rs       # Component exports
│       ├── button.rs    # ✅ Done
│       ├── badge.rs
│       ├── card.rs
│       ├── separator.rs
│       ├── skeleton.rs
│       ├── spinner.rs
│       ├── input.rs
│       ├── checkbox.rs
│       ├── switch.rs
│       ├── select.rs
│       ├── alert.rs
│       ├── toast.rs
│       ├── tooltip.rs
│       ├── dialog.rs
│       ├── dropdown.rs
│       ├── tabs.rs
│       ├── breadcrumb.rs
│       ├── pagination.rs
│       ├── avatar.rs
│       ├── progress.rs
│       ├── accordion.rs
│       └── ...
```

## Design Principles

1. **Use Theme Tokens**: Never hardcode colors, spacing, or radii
2. **Leverage Primitives**: Build on `blinc_layout` primitives, don't recreate
3. **Composable**: Components should compose well together
4. **Accessible**: Follow accessibility best practices
5. **Consistent API**: Similar components should have similar APIs
6. **Animated**: Use spring animations for interactions
7. **Themed**: Respect system dark/light mode

## Testing Strategy

1. Unit tests for each component
2. Visual regression tests (future)
3. Accessibility tests (future)
4. Example demos for each component

## Progress Summary

### Completed

- **Phase 1 (Core)**: All 7 components done (Button, Badge, Card, Separator, Skeleton, Spinner, Alert)
- **Phase 2 (Form)**: Input, Label, Checkbox, Switch, and Textarea done with full theme integration

### Current: Phase 2 - Form Components

Next components to implement:

1. **Select** - Dropdown select input
2. **Combobox** - Searchable dropdown

### Notes

- Spinner uses canvas-based animation with AnimatedTimeline for smooth 60fps rotation
- Input supports full color customization (border, bg, text, placeholder, cursor, selection)
- Checkbox uses State<bool> from context with signal-based reactivity and SVG checkmark
- Switch uses State<bool> with dual animation system:
  - Spring physics via `motion().translate_x(SharedAnimatedValue)` for thumb movement
  - Opacity animation via `SharedAnimatedValue` for background color transition
  - Thumb wrapper uses absolute positioning (left=0, top=0) with motion translation for visual movement
- Slider uses drag-based interaction with animated fill:
  - `SharedAnimatedValue` for smooth thumb position updates
  - Fill track reveals from left using `overflow_clip()` with animated width
  - Thumb wrapper absolutely positioned at origin with `motion().translate_x()` for GPU-accelerated movement
- Textarea uses builder pattern with lazy initialization:
  - `TextareaConfig` struct holds configuration
  - `Textarea` struct contains `inner: Div` with fully-built element tree
  - `TextareaBuilder` uses `OnceCell` for lazy init ensuring `children_builders()` returns actual children
  - Size presets (Small/Medium/Large) with rows/cols support
- Radio Group uses State<String> for selected value with smooth hover/press feedback
- Progress is a simple static div-based progress bar:
  - Size presets (Small/Medium/Large) control bar height (4px/8px/12px)
  - Uses theme tokens for indicator and track colors
  - For animated progress, wrap the indicator with `motion().scale_x(SharedAnimatedValue)` with `transform_origin_left()`
- All components use theme tokens from blinc_theme

### Builder Pattern for Complex Components

Components that wrap other elements must use the builder pattern with lazy initialization to ensure the incremental diff system works correctly:

```rust
pub struct MyComponent {
    inner: Div,  // Contains fully-built element tree
}

pub struct MyComponentBuilder {
    config: MyComponentConfig,
    built: std::cell::OnceCell<MyComponent>,  // Lazy init
}

impl ElementBuilder for MyComponentBuilder {
    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().inner.children_builders()  // Delegate to inner
    }
}
```

This ensures `children_builders()` returns the actual children, which the layout tree diff algorithm requires.

### Collapsible/Accordion Animation Pattern

The Collapsible and Accordion components use a `scale_y` animation pattern for smooth expand/collapse:

```rust
// Animation approach:
// 1. Use scale_y(0.0 → 1.0) for vertical collapse/expand
// 2. Use opacity(0.0 → 1.0) for fade effect
// 3. Wrap content in overflow_clip() to hide scaled content
// 4. Spring physics via SharedAnimatedValue for natural feel

let scale_anim = SharedAnimatedValue::new(scheduler, initial_scale, SpringConfig::snappy());
let opacity_anim = SharedAnimatedValue::new(scheduler, initial_opacity, SpringConfig::snappy());

// Content wrapper with animation
let animated_content = motion()
    .scale_y(scale_anim.clone())
    .opacity(opacity_anim.clone())
    .child(content);

// Clip overflow during animation
let collapsible = div().w_full().overflow_clip().child(animated_content);

// Toggle open/close by setting targets
scale_anim.lock().unwrap().set_target(if open { 1.0 } else { 0.0 });
opacity_anim.lock().unwrap().set_target(if open { 1.0 } else { 0.0 });
```

Accordion manages multiple Collapsible sections with optional single-open mode:
- `AccordionMode::Single` - Only one section open at a time
- `AccordionMode::Multi` - Multiple sections can be open simultaneously
