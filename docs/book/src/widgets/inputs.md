# Buttons & Inputs

Blinc provides ready-to-use input widgets. Their hover, press and checked
looks are patched in place from signals; they don't rebuild when the
pointer moves.

## Buttons

### Basic Button

```rust
use blinc_layout::prelude::*;

button("Save").on_click(|_| println!("Saved!"))
```

A button keeps its pointer state under a key made from where it was
created. Buttons made in a loop from one line need a key each:

```rust
div().flex_row().gap(2.0).children(
    ["Cut", "Copy", "Paste"].map(|label| button(label).key(format!("edit-{label}"))),
)
```

### Styled Buttons

```rust
button("Primary")
    .bg_color(Color::rgba(0.3, 0.5, 0.9, 1.0))
    .hover_color(Color::rgba(0.4, 0.6, 1.0, 1.0))
    .pressed_color(Color::rgba(0.2, 0.4, 0.8, 1.0))
    .text_color(Color::WHITE)
    .rounded(8.0)
    .px(4.0)
```

The colours default to the theme's primary tokens. `.id(..)` and
`.class(..)` let a stylesheet style the button, including `:hover`,
`:active` and `:disabled`.

### A Label That Follows a Signal

The label takes a string or a signal, and `.disabled(..)` takes a bool or a
signal. Both are patched in place:

```rust
use blinc_core::reactive::{computed, signal};

let saving = signal(false);
let label = computed(move |g| {
    if g.get(saving).unwrap_or(false) { "Saving…".to_string() } else { "Save".to_string() }
});

button(label)
    .disabled(saving)
    .on_click(move |_| saving.set(true))
```

### Custom Content Buttons

`button_with` builds the content once and hands it a `ButtonLook`. Bind
what should follow the button's state to it:

```rust
use blinc_layout::widgets::button_with;

button_with(|look| {
    div()
        .flex_row()
        .gap(2.0)
        .items_center()
        .child(svg(SAVE_ICON).size(16.0, 16.0).color(look.text_color()))
        .child(text("Save").color(look.text_color()))
})
.on_click(|_| save_file())
```

`look.text_color()` is the text colour for the button's current state,
after stylesheet overrides. `look.interaction()` gives its hovered, pressed
and focused signals.

### Disabled Buttons

```rust
button("Cannot Click")
    .disabled(true)
    .disabled_color(Color::rgba(0.2, 0.2, 0.25, 0.5))
    .disabled_text_color(Color::rgba(1.0, 1.0, 1.0, 0.4))
    .flat_when_disabled(true)
```

A disabled button has its disabled fill and does not click.
`.disabled_border(width, color)` replaces its border while disabled.

---

## Checkboxes

### Basic Checkbox

```rust
use blinc_core::context_state::use_state_keyed;
use blinc_layout::widgets::checkbox::checkbox;

let remember = use_state_keyed("remember_me", || false);

checkbox(&remember).on_change(|checked| println!("Checkbox is now: {checked}"))
```

The checkbox reads and writes the `State<bool>` it is given, so the state
is the source of truth: set it from elsewhere and the box follows.

### Labeled Checkbox

```rust
checkbox(&remember)
    .label("Remember me")
    .label_color(Color::WHITE)
```

`checkbox_labeled(&remember, "Remember me")` is the same.

### Styled Checkbox

```rust
checkbox(&remember)
    .check_color(Color::WHITE)
    .checked_bg(Color::rgba(0.4, 0.6, 1.0, 1.0))
    .unchecked_bg(Color::rgba(0.2, 0.2, 0.25, 1.0))
    .rounded(4.0)
    .checkbox_size(20.0)
```

### Initially Checked

```rust
let remember = use_state_keyed("remember_me", || true); // Start checked
```

---

## Text Input

### Basic Text Input

```rust
use blinc_layout::widgets::text_input::{text_input, text_input_state_with_placeholder};

fn my_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    let state = text_input_state_with_placeholder("Enter your name...");

    text_input(&state)
        .w(300.0)
        .on_change(|text| {
            println!("Input: {}", text);
        })
}
```

### Styled Text Input

```rust
text_input(&state)
    .w(300.0)
    .rounded(8.0)
    .idle_bg_color(Color::rgba(0.15, 0.15, 0.2, 1.0))
    .text_color(Color::WHITE)
    .placeholder_color(Color::rgba(0.5, 0.5, 0.6, 1.0))
    .focused_border_color(Color::rgba(0.4, 0.6, 1.0, 1.0))
```

### Reading Input Value

```rust
let state = text_input_state();

// Later, read the current value
let current_text = state.lock().unwrap().value.clone();
```

### Setting the Value from Code

The field is built once. Typing, focus, hover and scrolling patch what it
shows in place. After changing its data from outside the widget, as a
stepper button does, call `refresh_text_input` so the field shows the
change:

```rust
use blinc_layout::widgets::text_input::refresh_text_input;

{
    let mut data = state.lock().unwrap();
    data.value = "42".to_string();
    data.cursor = data.value.chars().count();
}
refresh_text_input(&state);
```

---

## OTP Input

`input_otp` renders linked single-character text input slots for
verification codes and PINs. It keeps a joined `State<String>` in sync
while handling focus movement across slots.

```rust
use blinc_layout::widgets::input_otp::input_otp;

fn verification_code(ctx: &WindowedContext) -> impl ElementBuilder {
    let code = ctx.use_state_keyed("otp_code", || String::new());

    input_otp(&code, 6)
        .numeric_only(true)
        .on_complete(|code| println!("complete code: {code}"))
}
```

### Behavior

- Typing a character advances focus to the next slot.
- Backspace on an empty slot rewinds focus and clears the previous slot.
- Pasting distributes characters across the remaining slots.
- `numeric_only(true)` filters typed and pasted input to ASCII digits.

### Options

```rust
input_otp(&code, 6)
    .numeric_only(true)
    .masked(false)
    .gap(8.0)
    .class("verification-code")
```

---

## Text Area

### Basic Text Area

```rust
use blinc_layout::widgets::text_area::{text_area, text_area_state};

fn my_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    let state = text_area_state("Enter description...");

    text_area(&state)
        .w(400.0)
        .h(200.0)
        .on_change(|text| {
            println!("Content: {}", text);
        })
}
```

### Styled Text Area

```rust
text_area(&state)
    .w(400.0)
    .h(200.0)
    .rounded(8.0)
    .bg_color(Color::rgba(0.15, 0.15, 0.2, 1.0))
    .text_color(Color::WHITE)
    .font_size(14.0)
    .line_height(1.5)
```

---

## Code Editor

### Syntax Highlighted Code

```rust
use blinc_layout::widgets::code::code;

fn my_ui() -> impl ElementBuilder {
    let source = r#"
fn main() {
    println!("Hello, Blinc!");
}
"#;

    code(source)
        .lang("rust")
        .w_full()
        .h(300.0)
        .rounded(8.0)
        .font("Fira Code")
        .size(14.0)
}
```

### Supported Languages

- `rust`, `python`, `javascript`, `typescript`
- `html`, `css`, `json`, `yaml`, `xml`
- `sql`, `bash`, `go`, `java`, `c`, `cpp`
- And more...

---

## Form Example

```rust
fn login_form(ctx: &WindowedContext) -> impl ElementBuilder {
    let email_state = text_input_state_with_placeholder("Email address");
    let password_state = text_input_state_with_placeholder("Password");
    let remember_state = use_state_keyed("remember_me", || false);

    div()
        .w(400.0)
        .p(24.0)
        .rounded(16.0)
        .bg(Color::rgba(0.12, 0.12, 0.16, 1.0))
        .flex_col()
        .gap(16.0)
        // Title
        .child(
            text("Sign In")
                .size(24.0)
                .weight(FontWeight::Bold)
                .color(Color::WHITE)
        )
        // Email field
        .child(
            div()
                .flex_col()
                .gap(4.0)
                .child(label("Email").color(Color::WHITE))
                .child(
                    text_input(&email_state)
                        .w_full()
                        .rounded(8.0)
                )
        )
        // Password field
        .child(
            div()
                .flex_col()
                .gap(4.0)
                .child(label("Password").color(Color::WHITE))
                .child(
                    text_input(&password_state)
                        .w_full()
                        .rounded(8.0)
                        // Note: password masking would be a feature to add
                )
        )
        // Remember me
        .child(
            checkbox(&remember_state)
                .label("Remember me")
                .label_color(Color::WHITE)
        )
        // Submit button
        .child(
            button("Sign In")
                .w_full()
                .bg_color(Color::rgba(0.3, 0.5, 0.9, 1.0))
                .text_color(Color::WHITE)
                .rounded(8.0)
                .on_click(|_| {
                    println!("Form submitted!");
                })
        )
}
```

---

## Widget State

| Widget | State | Source |
|--------|-------|--------|
| Button | Hovered, pressed, focused, disabled | Signals from `Interaction`; `.disabled(..)` |
| Checkbox | Checked, hovered | The `State<bool>` it is given; pointer signals |
| TextInput | `TextFieldState`: Idle, Hovered, Focused, FocusedHovered, Disabled | Its own state handle |
| TextArea | `TextFieldState` | Same as TextInput |

---

## Best Practices

1. **Use unique keys for state** - Each widget needs its own state key.

2. **Handle validation in on_change** - Validate input as users type.

3. **Provide visual feedback** - Use colors to indicate focus and errors.

4. **Group related inputs** - Use flex containers to organize forms.

5. **Add labels** - Every input should have an associated label for accessibility.
