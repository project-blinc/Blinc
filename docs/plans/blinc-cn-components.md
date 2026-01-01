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
| **Badge** | div, text | Planned |
| **Card** | div, text | Planned |
| **Separator** | div | Planned |
| **Skeleton** | div with animation | Planned |
| **Spinner** | div with animation | Planned |

### 2. Form Components (Priority: High)

Form inputs and controls.

| Component | Primitives Used | Status |
|-----------|-----------------|--------|
| **Input** | text_input | Planned |
| **Textarea** | text_area | Planned |
| **Checkbox** | Stateful, div, svg | Planned |
| **Radio Group** | Stateful, div | Planned |
| **Switch** | Stateful, motion | Planned |
| **Slider** | Stateful, div | Planned |
| **Select** | scroll, text, overlay | Planned |
| **Combobox** | text_input, scroll, overlay | Planned |
| **Label** | text | Planned |
| **Form** | div, validation | Planned |
| **Field** | div, label, input, error | Planned |

### 3. Data Display (Priority: Medium)

Components for displaying data.

| Component | Primitives Used | Status |
|-----------|-----------------|--------|
| **Avatar** | image, div, text | Planned |
| **Table** | div, scroll | Planned |
| **Data Table** | table, scroll, sorting | Planned |
| **Progress** | div, animation | Planned |
| **Calendar** | div, text, grid | Planned |
| **Chart** | canvas | Planned |

### 4. Feedback (Priority: Medium)

User feedback components.

| Component | Primitives Used | Status |
|-----------|-----------------|--------|
| **Alert** | div, text, icon | Planned |
| **Toast** | overlay, motion | Planned |
| **Tooltip** | overlay, motion | Planned |
| **Popover** | overlay | Planned |

### 5. Overlays (Priority: Medium)

Modal and overlay components.

| Component | Primitives Used | Status |
|-----------|-----------------|--------|
| **Dialog** | overlay, motion | Planned |
| **Sheet** | overlay, motion | Planned |
| **Drawer** | overlay, motion | Planned |
| **Dropdown Menu** | overlay, scroll | Planned |
| **Context Menu** | overlay, scroll | Planned |
| **Menubar** | div, overlay | Planned |
| **Hover Card** | overlay, motion | Planned |

### 6. Navigation (Priority: Medium)

Navigation components.

| Component | Primitives Used | Status |
|-----------|-----------------|--------|
| **Tabs** | Stateful, div, text | Planned |
| **Breadcrumb** | div, link | Planned |
| **Pagination** | div, button | Planned |
| **Navigation Menu** | div, overlay | Planned |
| **Sidebar** | div, scroll | Planned |

### 7. Layout (Priority: Low)

Layout helpers.

| Component | Primitives Used | Status |
|-----------|-----------------|--------|
| **Accordion** | Stateful, motion | Planned |
| **Collapsible** | Stateful, motion | Planned |
| **Resizable** | div, drag | Planned |
| **Scroll Area** | scroll | Planned |
| **Aspect Ratio** | div | Planned |

### 8. Typography (Priority: Low)

Text components.

| Component | Primitives Used | Status |
|-----------|-----------------|--------|
| **Typography** | text, rich_text | Planned |
| **Kbd** | div, text | Planned |

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

## Next Steps

1. Implement Badge component
2. Implement Card component
3. Implement Separator component
4. Create example demo showcasing components
