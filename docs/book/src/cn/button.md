# Button

Buttons trigger actions.

## Basic Usage

```rust
use blinc::cn::prelude::*;

button("Click me")
    .on_click(|_| println!("Clicked!"))
```

The label takes a string or a signal. When the label changes, it is patched in
place.

## Variants

```rust
// Primary (default) - main actions
button("Save").variant(ButtonVariant::Primary)

// Secondary - alternative actions
button("Cancel").variant(ButtonVariant::Secondary)

// Destructive - dangerous actions
button("Delete").variant(ButtonVariant::Destructive)

// Outline - bordered
button("Edit").variant(ButtonVariant::Outline)

// Ghost - minimal
button("More").variant(ButtonVariant::Ghost)

// Link - looks like a link
button("Learn more").variant(ButtonVariant::Link)
```

## Sizes

```rust
button("Small").size(ButtonSize::Small)
button("Default").size(ButtonSize::Medium)
button("Large").size(ButtonSize::Large)

// Icon only (square)
button("").size(ButtonSize::Icon).icon(icons::SETTINGS)

// A size of your own: width, height
button("Wide").size(ButtonSize::Custom(160.0, 48.0))
```

## With Icons

```rust
use blinc::icons::icons;

button("Settings").icon(icons::SETTINGS)

// After the label
button("Next")
    .icon(icons::ARROW_RIGHT)
    .icon_position(IconPosition::End)
```

## Disabled

`disabled` takes a bool or a signal, so a button can follow a state:

```rust
button("Disabled").disabled(true)
button("Save").disabled(&is_saving)
```

A disabled button does not respond to clicks, and it has the disabled look.

## Styling

A stylesheet defines the look of a button. The `.cn-button` rules of
`CN_STYLES` set its fill, text color, border and shadow. They also set how these
change on hover, on press and while the button is disabled. The rules include a
`transition`, so a button changes smoothly between states. Each variant and size
has a class that your own rules can use:

| Class | Matches |
|-------|---------|
| `.cn-button` | every cn button |
| `.cn-button--primary`, `--secondary`, `--destructive`, `--outline`, `--ghost`, `--link` | the variant |
| `.cn-button--sm`, `--md`, `--lg`, `--icon`, `--custom` | the size |

```css
/* A brand colour for primary buttons, eased on hover. Your rules win over
   cn's whatever their specificity. */
.cn-button--primary { background: #6d28d9; }
.cn-button--primary:hover:not(:disabled) { background: #7c3aed; }
.cn-button--primary:active:not(:disabled) { background: #5b21b6; }

/* One button of your own */
.danger { background: #c0392b; transition: background-color 150ms ease; }
.danger:disabled { background: #555; color: #999; }
```

```rust
button("Delete").build_component().class("danger")
```

The `:hover:not(:disabled)` selector keeps a rule off a disabled button.
`:active` matches while the pointer is down on the button or on anything inside
it, as on the web. The text and icons inside the button take its `color`. A rule
that changes this `color` also changes them.

`.color(..)` on the builder fixes the text color in every state. A fluent value
(a value that you set with a builder method) overrides the cn rules, as an
inline style does on the web.

## Button Groups

```rust
div()
    .flex_row()
    .gap(2.0)
    .child(button("Save").variant(ButtonVariant::Primary))
    .child(button("Cancel").variant(ButtonVariant::Outline))
```

`gap` takes units of 4px, so `gap(2.0)` is 8px.

## Examples

### Icon Button

```rust
button("")
    .size(ButtonSize::Icon)
    .variant(ButtonVariant::Ghost)
    .icon(icons::X)
    .on_click(|_| close_dialog())
```

### A Button That Follows a Signal

```rust
let saving = ctx.use_signal(false);

button("Save")
    .disabled(saving)
    .on_click(move |_| saving.set(true))
```

## API Reference

| Method | Type | Default | Description |
|--------|------|---------|-------------|
| `variant` | `ButtonVariant` | `Primary` | Visual style |
| `size` | `ButtonSize` | `Medium` | `Small`, `Medium`, `Large`, `Icon` or `Custom(w, h)` |
| `disabled` | `bool` or signal | `false` | Disables interaction |
| `icon` | `&str` | none | An icon from `blinc::icons::icons` |
| `icon_position` | `IconPosition` | `Start` | `Start` or `End` |
| `icon_size` | `f32` | label size + 2 | Icon size in px |
| `color` | `Color` | by variant | Text color in every state |
| `on_click` | `Fn(&EventContext)` | none | Called on a click |
| `class`, `id` | `&str` | none | Selectors for your own rules |
