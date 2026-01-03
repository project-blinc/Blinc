# CSS Styling

Blinc includes a powerful CSS parser that allows you to define styles using familiar CSS syntax. This enables separation of concerns between layout code and visual styling.

## Overview

The CSS system supports:

- **ID-based selectors** (`#element-id`)
- **State modifiers** (`:hover`, `:active`, `:focus`, `:disabled`)
- **CSS custom properties** (`:root` and `var()`)
- **Keyframe animations** (`@keyframes`)
- **Automatic animation application** via the `animation:` property
- **Theme integration** (`theme()` function)
- **Length units** (`px`, `sp`, `%`)
- **Gradients** (`linear-gradient`, `radial-gradient`, `conic-gradient`)

---

## Basic Usage

### Parsing CSS

```rust
use blinc_layout::prelude::*;

let css = r#"
    #card {
        background: #3498db;
        border-radius: 12px;
        box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
    }
"#;

let result = Stylesheet::parse_with_errors(css);

// Check for errors
if result.has_errors() {
    result.print_colored_diagnostics();
}

let stylesheet = result.stylesheet;
```

### Applying Styles to Elements

Attach the stylesheet to the RenderTree:

```rust
use std::sync::Arc;

// In your render tree setup
render_tree.set_stylesheet(Some(Arc::new(stylesheet)));

// Then use IDs on elements
div()
    .id("card")
    .child(text("Styled with CSS!"))
```

---

## Supported Properties

### Background

```css
#element {
    background: #ff5733;                    /* Hex color */
    background: rgb(255, 87, 51);           /* RGB */
    background: rgba(255, 87, 51, 0.8);     /* RGBA */
    background: theme(primary);             /* Theme token */
}
```

### Gradients

CSS gradients are fully supported for the `background` property:

#### Linear Gradients

```css
#element {
    /* Angle-based (0deg = up, 90deg = right, 180deg = down) */
    background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);

    /* Direction keywords */
    background: linear-gradient(to right, red, blue);
    background: linear-gradient(to bottom right, #fff, #000);

    /* Multiple color stops */
    background: linear-gradient(90deg, red 0%, yellow 50%, green 100%);

    /* Implied positions (evenly distributed) */
    background: linear-gradient(to bottom, red, yellow, green);

    /* Different angle units */
    background: linear-gradient(0.25turn, red, blue);  /* 90deg */
    background: linear-gradient(1.5708rad, red, blue); /* ~90deg */
}
```

#### Radial Gradients

```css
#element {
    /* Simple circle from center */
    background: radial-gradient(circle, red, blue);

    /* With position */
    background: radial-gradient(circle at center, red, blue);
    background: radial-gradient(circle at 25% 75%, red, blue);

    /* Ellipse shape */
    background: radial-gradient(ellipse at center, red, blue);

    /* Multiple color stops */
    background: radial-gradient(circle, red 0%, yellow 50%, green 100%);
}
```

#### Conic Gradients

```css
#element {
    /* Simple color wheel */
    background: conic-gradient(red, yellow, green, blue, red);

    /* With starting angle */
    background: conic-gradient(from 45deg, red, blue);

    /* With position */
    background: conic-gradient(at 25% 75%, red, blue);

    /* Combined angle and position */
    background: conic-gradient(from 90deg at center, red, blue);
}
```

#### Gradient Color Stops

Color stops can use any supported color format:

```css
#element {
    /* Hex colors with positions */
    background: linear-gradient(to right, #667eea 0%, #764ba2 100%);

    /* RGBA colors */
    background: linear-gradient(45deg, rgba(255, 0, 0, 0.5), rgba(0, 0, 255, 0.8));

    /* Named colors */
    background: linear-gradient(to right, red, orange, yellow, green, blue);

    /* Mixed formats */
    background: linear-gradient(135deg, #ff0000, rgba(0, 255, 0, 0.5) 50%, blue);
}
```

### Border Radius

```css
#element {
    border-radius: 8px;                     /* Uniform */
    border-radius: theme(radius-lg);        /* Theme token */
}
```

### Box Shadow

```css
#element {
    box-shadow: 2px 4px 12px rgba(0, 0, 0, 0.3);  /* x y blur color */
    box-shadow: theme(shadow-md);                 /* Theme token */
    box-shadow: none;                             /* Remove shadow */
}
```

### Transform

```css
#element {
    transform: scale(1.02);                 /* Uniform scale */
    transform: scale(1.5, 0.8);             /* Non-uniform */
    transform: translate(10px, 20px);       /* Translation */
    transform: translateX(10px);            /* X only */
    transform: translateY(20px);            /* Y only */
    transform: rotate(45deg);               /* Rotation */
}
```

### Opacity

```css
#element {
    opacity: 0.8;
}
```

### Render Layer

```css
#element {
    render-layer: foreground;               /* On top */
    render-layer: background;               /* Behind */
    render-layer: glass;                    /* Glass layer */
}
```

---

## Length Units

Blinc CSS supports three types of length units:

### Pixels (`px`)

Raw pixel values. These are the default when no unit is specified.

```css
#element {
    border-radius: 8px;
    box-shadow: 2px 4px 12px rgba(0, 0, 0, 0.3);
    transform: translate(10px, 20px);
}
```

### Spacing Units (`sp`)

Spacing units follow a 4px grid system, where `1sp = 4px`. This helps maintain consistent spacing throughout your application.

```css
#card {
    border-radius: 2sp;                    /* 2 * 4 = 8px */
    box-shadow: 1sp 2sp 4sp rgba(0,0,0,0.2); /* 4px 8px 16px */
    transform: translate(4sp, 2sp);         /* 16px, 8px */
}
```

Common `sp` values:

- `1sp` = 4px
- `2sp` = 8px
- `4sp` = 16px
- `6sp` = 24px
- `8sp` = 32px

### Percentages (`%`)

Percentages are supported in gradient color stops and position values.

```css
#element {
    /* Gradient color stops use percentages */
    background: linear-gradient(to right, red 0%, blue 100%);

    /* Radial/conic gradient positions */
    background: radial-gradient(circle at 25% 75%, red, blue);
}
```

---

## State Modifiers

Define different styles for interactive states:

```css
#button {
    background: theme(primary);
    transform: scale(1.0);
}

#button:hover {
    background: theme(primary-hover);
    transform: scale(1.02);
}

#button:active {
    transform: scale(0.98);
}

#button:focus {
    box-shadow: 0 0 0 3px theme(primary);
}

#button:disabled {
    opacity: 0.5;
}
```

### Querying State Styles

```rust
// Get base style
let base = stylesheet.get("button");

// Get state-specific style
let hover = stylesheet.get_with_state("button", CssElementState::Hover);
let active = stylesheet.get_with_state("button", CssElementState::Active);

// Get all states at once
let (base, states) = stylesheet.get_all_states("button");
for (state, style) in states {
    println!(":{} => {:?}", state, style.opacity);
}
```

---

## CSS Variables

Define reusable values with custom properties:

```css
:root {
    --brand-color: #3498db;
    --hover-opacity: 0.85;
    --card-radius: 12px;
    --spacing-md: 16px;
}

#card {
    background: var(--brand-color);
    border-radius: var(--card-radius);
    opacity: 1.0;
}

#card:hover {
    opacity: var(--hover-opacity);
}
```

### Fallback Values

```css
#element {
    background: var(--undefined-color, #333);  /* Uses fallback */
}
```

### Accessing Variables Programmatically

```rust
// Get a variable value
if let Some(value) = stylesheet.get_variable("brand-color") {
    println!("Brand color: {}", value);
}

// Iterate all variables
for name in stylesheet.variable_names() {
    let value = stylesheet.get_variable(name).unwrap();
    println!("--{}: {}", name, value);
}
```

---

## Theme Integration

Use the `theme()` function to reference theme tokens:

```css
#card {
    background: theme(surface);
    border-radius: theme(radius-lg);
    box-shadow: theme(shadow-md);
}

#button {
    background: theme(primary);
}

#button:hover {
    background: theme(primary-hover);
}
```

### Available Theme Tokens

**Colors:**
- `primary`, `primary-hover`, `primary-active`
- `secondary`, `secondary-hover`, `secondary-active`
- `success`, `success-bg`
- `warning`, `warning-bg`
- `error`, `error-bg`
- `info`, `info-bg`
- `foreground`, `foreground-muted`
- `background`, `surface`, `surface-hover`
- `border`, `border-muted`

**Radii:**
- `radius-sm`, `radius-default`, `radius-md`
- `radius-lg`, `radius-xl`, `radius-2xl`

**Shadows:**
- `shadow-sm`, `shadow-default`, `shadow-md`
- `shadow-lg`, `shadow-xl`

---

## Keyframe Animations

Define complex animations with `@keyframes`:

```css
@keyframes fade-in {
    from {
        opacity: 0;
        transform: translateY(20px);
    }
    to {
        opacity: 1;
        transform: translateY(0);
    }
}

@keyframes pulse {
    0%, 100% {
        opacity: 1;
        transform: scale(1);
    }
    50% {
        opacity: 0.8;
        transform: scale(1.05);
    }
}
```

### Percentage Positions

```css
@keyframes complex-animation {
    0% { opacity: 0; }
    25% { opacity: 0.5; transform: scale(1.1); }
    50% { opacity: 1; }
    75% { opacity: 0.5; transform: scale(0.9); }
    100% { opacity: 1; transform: scale(1); }
}
```

### Accessing Keyframes

```rust
// Get keyframes by name
if let Some(keyframes) = stylesheet.get_keyframes("fade-in") {
    println!("Animation has {} stops", keyframes.keyframes.len());

    for kf in &keyframes.keyframes {
        println!("  {}%: opacity={:?}",
            (kf.position * 100.0) as i32,
            kf.style.opacity
        );
    }
}
```

### Converting to Motion Animation

```rust
// Convert to MotionAnimation (for simple from/to animations)
let motion = keyframes.to_motion_animation(300, 200);  // enter_ms, exit_ms

// Convert to MultiKeyframeAnimation (for complex multi-step animations)
let animation = keyframes.to_multi_keyframe_animation(1000, Easing::EaseInOut);
```

---

## Animation Property

Apply animations to elements with the `animation:` property:

```css
@keyframes slide-in {
    from { opacity: 0; transform: translateY(20px); }
    to { opacity: 1; transform: translateY(0); }
}

#modal {
    animation: slide-in 300ms ease-out;
}
```

### Animation Shorthand

```css
#element {
    /* animation: name duration timing-function delay iteration-count direction fill-mode */
    animation: pulse 2s ease-in-out 100ms infinite alternate forwards;
}
```

### Individual Properties

```css
#element {
    animation-name: pulse;
    animation-duration: 2s;
    animation-timing-function: ease-in-out;
    animation-delay: 100ms;
    animation-iteration-count: infinite;  /* or a number */
    animation-direction: alternate;        /* normal | reverse | alternate | alternate-reverse */
    animation-fill-mode: forwards;         /* none | forwards | backwards | both */
}
```

### Automatic Application

When a stylesheet is attached to the RenderTree, elements with IDs automatically receive animations:

```rust
let css = r#"
    @keyframes card-enter {
        from { opacity: 0; transform: scale(0.95); }
        to { opacity: 1; transform: scale(1); }
    }

    #card {
        animation: card-enter 300ms ease-out;
    }
"#;

let stylesheet = Stylesheet::parse_with_errors(css).stylesheet;
render_tree.set_stylesheet(Some(Arc::new(stylesheet)));

// This element will automatically animate on render!
div()
    .id("card")
    .child(content())
```

---

## Error Handling

The CSS parser collects errors without failing:

```rust
let css = r#"
    #card {
        background: red;
        opacity: invalid;        /* Error: invalid value */
        unknown-prop: foo;       /* Warning: unknown property */
    }
"#;

let result = Stylesheet::parse_with_errors(css);

// Check for issues
if result.has_errors() {
    println!("Has {} error(s)", result.errors_only().count());
}
if result.has_warnings() {
    println!("Has {} warning(s)", result.warnings_only().count());
}

// Print colored diagnostics to console
result.print_colored_diagnostics();
result.print_summary();

// The valid properties are still parsed!
let style = result.stylesheet.get("card").unwrap();
assert!(style.background.is_some());  // "red" was parsed
```

### Error Information

```rust
for error in &result.errors {
    println!("Line {}, Column {}: {}",
        error.line,
        error.column,
        error.message
    );

    if let Some(ref prop) = error.property {
        println!("  Property: {}", prop);
    }
    if let Some(ref val) = error.value {
        println!("  Value: {}", val);
    }
}
```

---

## Motion Container Integration

Use CSS keyframes with the `Motion` container:

```rust
let css = r#"
    @keyframes modal-enter {
        from { opacity: 0; transform: scale(0.9) translateY(20px); }
        to { opacity: 1; transform: scale(1) translateY(0); }
    }
"#;

let stylesheet = Stylesheet::parse_with_errors(css).stylesheet;

// Method 1: Using from_stylesheet
motion()
    .from_stylesheet(&stylesheet, "modal-enter", 300, 200)
    .child(modal_content())

// Method 2: Using keyframes_from_stylesheet for multi-step animations
motion()
    .keyframes_from_stylesheet(&stylesheet, "pulse", 1000, Easing::EaseInOut)
    .child(pulsing_element())
```

---

## Complete Example

```rust
use blinc_layout::prelude::*;
use std::sync::Arc;

fn styled_app() -> impl ElementBuilder {
    // Define styles
    let css = r#"
        :root {
            --card-bg: theme(surface);
            --card-radius: theme(radius-lg);
            --brand-gradient: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
        }

        @keyframes fade-in {
            from { opacity: 0; transform: translateY(10px); }
            to { opacity: 1; transform: translateY(0); }
        }

        #app-container {
            background: theme(background);
        }

        #card {
            background: var(--card-bg);
            border-radius: var(--card-radius);
            box-shadow: theme(shadow-md);
            animation: fade-in 300ms ease-out;
        }

        #card:hover {
            box-shadow: theme(shadow-lg);
            transform: translateY(-2px);
        }

        #gradient-card {
            background: var(--brand-gradient);
            border-radius: theme(radius-lg);
            box-shadow: theme(shadow-md);
        }

        #gradient-card:hover {
            background: linear-gradient(135deg, #7c8ff0 0%, #8b5cb8 100%);
            transform: translateY(-2px);
        }

        #primary-button {
            background: theme(primary);
            border-radius: theme(radius-default);
        }

        #primary-button:hover {
            background: theme(primary-hover);
            transform: scale(1.02);
        }

        #primary-button:active {
            transform: scale(0.98);
        }
    "#;

    let result = Stylesheet::parse_with_errors(css);
    if result.has_errors() {
        result.print_colored_diagnostics();
    }

    // In real usage, attach to render_tree
    // render_tree.set_stylesheet(Some(Arc::new(result.stylesheet)));

    div()
        .id("app-container")
        .flex_col()
        .p(24.0)
        .gap(16.0)
        .child(
            div()
                .id("card")
                .p(16.0)
                .child(text("Styled with CSS!"))
        )
        .child(
            div()
                .id("gradient-card")
                .p(16.0)
                .child(text("Gradient background!"))
        )
        .child(
            button("Click me")
                .id("primary-button")
        )
}
```

---

## Best Practices

1. **Use CSS variables** for values you want to reuse or override
2. **Use theme tokens** for colors that should respect the app's theme
3. **Check for errors** after parsing to catch typos and invalid values
4. **Keep animations short** for UI transitions (150-400ms)
5. **Use state modifiers** for hover/active effects instead of manual callbacks
6. **Prefer ID selectors** (`#id`) for precise targeting

---

## Comparison with Builder API

| CSS | Builder API |
|-----|-------------|
| `background: #3498db;` | `.bg(Color::hex("#3498db"))` |
| `border-radius: 8px;` | `.rounded(8.0)` |
| `transform: scale(1.02);` | `.scale(1.02)` |
| `opacity: 0.8;` | `.opacity(0.8)` |
| `box-shadow: theme(shadow-md);` | `.shadow_md()` |

Both approaches can be combined - use CSS for base styles and the builder API for dynamic values.
