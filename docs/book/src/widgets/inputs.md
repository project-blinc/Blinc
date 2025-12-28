# Buttons & Inputs

Blinc provides ready-to-use input widgets with built-in state management.

## Buttons

### Basic Button

```rust
use blinc_layout::widgets::button::{button, Button};

fn my_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    let btn_state = ctx.use_state_for("save_btn", ButtonState::Idle);

    button(btn_state, "Save")
        .on_click(|_| {
            println!("Saved!");
        })
}
```

### Styled Buttons

```rust
button(state, "Primary")
    .bg_color(Color::rgba(0.3, 0.5, 0.9, 1.0))
    .hover_color(Color::rgba(0.4, 0.6, 1.0, 1.0))
    .pressed_color(Color::rgba(0.2, 0.4, 0.8, 1.0))
    .text_color(Color::WHITE)
    .rounded(8.0)
    .p(2.0)
```

### Custom Content Buttons

```rust
Button::with_content(state, |s| {
    div()
        .flex_row()
        .gap(8.0)
        .items_center()
        .child(svg("icons/save.svg").w(16.0).h(16.0).tint(Color::WHITE))
        .child(text("Save").color(Color::WHITE))
})
.on_click(|_| save_file())
```

### Disabled Buttons

```rust
let state = ctx.use_state_for("btn", ButtonState::Disabled);

button(state, "Cannot Click")
    .disabled_color(Color::rgba(0.2, 0.2, 0.25, 0.5))
```

---

## Checkboxes

### Basic Checkbox

```rust
use blinc_layout::widgets::checkbox::{checkbox, checkbox_state};

fn my_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    let state = checkbox_state(false);  // Initially unchecked

    checkbox(&state)
        .on_change(|checked| {
            println!("Checkbox is now: {}", checked);
        })
}
```

### Labeled Checkbox

```rust
checkbox(&state)
    .label("Remember me")
    .label_color(Color::WHITE)
```

### Styled Checkbox

```rust
checkbox(&state)
    .check_color(Color::rgba(0.4, 0.6, 1.0, 1.0))
    .bg_color(Color::rgba(0.2, 0.2, 0.25, 1.0))
    .rounded(4.0)
    .size(20.0)
```

### Initially Checked

```rust
let state = checkbox_state(true);  // Start checked
```

---

## Text Input

### Basic Text Input

```rust
use blinc_layout::widgets::text_input::{text_input, text_input_state};

fn my_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    let state = text_input_state("Enter your name...");

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
    .bg_color(Color::rgba(0.15, 0.15, 0.2, 1.0))
    .text_color(Color::WHITE)
    .placeholder_color(Color::rgba(0.5, 0.5, 0.6, 1.0))
    .focus_border_color(Color::rgba(0.4, 0.6, 1.0, 1.0))
```

### Reading Input Value

```rust
let state = text_input_state("");

// Later, read the current value
let current_text = state.text();
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
    let email_state = text_input_state("Email address");
    let password_state = text_input_state("Password");
    let remember_state = checkbox_state(false);
    let submit_state = ctx.use_state_for("submit", ButtonState::Idle);

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
            button(submit_state, "Sign In")
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

## Widget State Types

Each widget uses a specific state type:

| Widget | State Type | States |
|--------|-----------|--------|
| Button | `ButtonState` | Idle, Hovered, Pressed, Disabled |
| Checkbox | `CheckboxState` | UncheckedIdle, UncheckedHovered, CheckedIdle, CheckedHovered |
| TextInput | `TextFieldState` | Idle, Hovered, Focused, FocusedHovered, Disabled |
| TextArea | `TextFieldState` | Same as TextInput |

---

## Best Practices

1. **Use unique keys for state** - Each widget needs its own state key.

2. **Handle validation in on_change** - Validate input as users type.

3. **Provide visual feedback** - Use colors to indicate focus and errors.

4. **Group related inputs** - Use flex containers to organize forms.

5. **Add labels** - Every input should have an associated label for accessibility.
