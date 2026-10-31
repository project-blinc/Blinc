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

A button is an element: `:hover`, `:active` and `:disabled` match it, so how
it looks in each state, and how it eases between them, is a stylesheet.
`button()` starts from the theme's primary look (the `.blinc-button` rules of
`blinc_layout::ua_styles::UA_STYLES`); give it a class and write your own
rules, which win over the default look:

```rust
button("Delete").class("danger").rounded(8.0).px(4.0)
```

```css
.danger { background: #c0392b; transition: background-color 150ms ease; }
.danger:hover:not(:disabled) { background: #e74c3c; }
.danger:active:not(:disabled) { background: #a93226; }
.danger:disabled { background: #555; color: #999; }
```

`.id(..)` works the same way. A fluent value such as `.bg_color(..)` fixes
that property in every state, as an inline style does on the web.

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

`button_with` builds the content once. Text and icons inside take the
button's `color`, so a rule that changes it on `:hover` changes them too:

```rust
use blinc_layout::widgets::button_with;

button_with(|_| {
    div()
        .flex_row()
        .gap(2.0)
        .items_center()
        .child(svg(SAVE_ICON).size(16.0, 16.0))
        .child(text("Save"))
})
.on_click(|_| save_file())
```

The closure is also given a `ButtonLook`; `look.interaction()` has the
button's hovered, pressed and focused signals for content that needs them.

### Disabled Buttons

```rust
button("Cannot Click").disabled(true)
```

A disabled button matches `:disabled` and does not click. The default look
gives it the theme's disabled fill and muted text; restyle it with a
`:disabled` rule.

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
checkbox(&remember).label("Remember me")
```

`checkbox_labeled(&remember, "Remember me")` is the same.

### Styled Checkbox

A checkbox is an element with a `checked` attribute while it is checked and
`disabled` while it cannot be used. Its fill, border colour, hover, the
disabled palette and the mark's fade-in are the `.blinc-checkbox` rules of
`blinc_layout::ua_styles::UA_STYLES`, with a `transition`. Restyle it with
rules of your own; they win over the defaults:

```css
.my-checkbox:checked { background: #16a34a; border-color: #16a34a; }
.my-checkbox:hover:not(:disabled):not(:checked) { border-color: #16a34a; }
```

```rust
checkbox(&remember)
    .class("my-checkbox")   // on the box
    .checkbox_size(20.0)    // sizes are layout, so they stay in Rust
    .border_width(2.0)
    .rounded(4.0)
```

`.mark_size(..)`, `.gap(..)` and `.label_font_size(..)` size the parts; colours
come from rules, so there are no colour setters.

### Initially Checked

```rust
let remember = use_state_keyed("remember_me", || true); // Start checked
```

---

## Radio Groups

```rust
use blinc_layout::widgets::radio_group;

let theme = use_state_keyed("theme", || "light".to_string());

radio_group(&theme)
    .label("Theme")
    .horizontal()
    .option("light", "Light")
    .option("dark", "Dark")
    .option_disabled("auto", "Auto")
    .on_change(|value| println!("Selected: {value}"))
```

The group reads and writes a `State<String>` holding the selected value.

### Styled Radio Group

Each ring has a `checked` attribute while its option is selected and
`disabled` while it cannot be chosen. The ring's border, the dot's fade-in,
hover (which also highlights an unselected option) and the disabled fill are
the `.blinc-radio` and `.blinc-radio-dot` rules of
`blinc_layout::ua_styles::UA_STYLES`, with a `transition`. Restyle them with
rules of your own:

```css
.blinc-radio:checked { border-color: #16a34a; }
.blinc-radio:checked > .blinc-radio-dot { background: #16a34a; }
```

Sizes are layout, so they stay in Rust (`.gap(..)` and the group's label);
colours come from rules, so there are no colour setters.

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
    .text_color(Color::WHITE)
    .placeholder_color(Color::rgba(0.5, 0.5, 0.6, 1.0))
    .class("my-field")
```

```css
.my-field { background: #262633; }
.my-field:focus { border-color: #6699ff; }
```

A text input is one element with the class `blinc-input`. It has `disabled`
while it cannot be edited and `data-invalid` while its value fails validation,
so its look in each state, and how it eases between them, is the
`.blinc-input` rules of the default stylesheet. `idle_bg_color` and
`idle_border_color` set the idle values as inline style, which the default
rules do not replace.

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
use blinc_layout::widgets::text_area::{text_area, text_area_state_with_placeholder};

fn my_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    let state = text_area_state_with_placeholder("Enter description...");

    text_area(&state)
        .w(400.0)
        .rows(6)
        .on_change(|text| {
            println!("Content: {}", text);
        })
}
```

### Styled Text Area

Size and shape are set on the builder. Colours come from the stylesheet, by
class or id, per state:

```rust
text_area(&state)
    .w(400.0)
    .rows(6)
    .rounded(8.0)
    .font_size(14.0)
    .class("notes")
```

```css
.notes { background: #1e1e2a; color: #ffffff; caret-color: #6699ff; }
.notes:hover { background: #24243a; }
.notes:focus { border-color: #6699ff; }
```

### Setting the Text from Code

Like the text input, the text area is built once and patches what it shows
in place. After changing its state from outside the widget, call
`refresh_text_area` so the field shows the change:

```rust
use blinc_layout::widgets::text_area::refresh_text_area;

state.lock().unwrap().set_value("First line\nSecond line");
refresh_text_area(&state);
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
| Button | Hovered, pressed, disabled | `:hover`, `:active` and `:disabled` rules; `.disabled(..)` |
| Checkbox | Checked, hovered, disabled | `checked` and `disabled` attributes from the `State<bool>` it is given; `:hover` rules |
| TextInput | `TextFieldState`: Idle, Hovered, Focused, FocusedHovered, Disabled | Its own state handle |
| TextArea | `TextFieldState` | Same as TextInput |

---

## Best Practices

1. **Use unique keys for state** - Each widget needs its own state key.

2. **Handle validation in on_change** - Validate input as users type.

3. **Provide visual feedback** - Use colors to indicate focus and errors.

4. **Group related inputs** - Use flex containers to organize forms.

5. **Add labels** - Every input should have an associated label for accessibility.
