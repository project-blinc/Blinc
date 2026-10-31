# Form Components

Components for building forms: inputs, checkboxes, selects, and more.

## Input

Text input field, bound to a `SharedTextInputData`:

```rust
use blinc_cn::prelude::*;
use blinc_layout::widgets::text_input::text_input_data;

let name = text_input_data();

cn::input(&name)
    .label("Name")
    .placeholder("Enter your name...")
    .on_change(|value| println!("name: {value}"))
```

### Input Types

```rust
use blinc_layout::widgets::text_input::InputType;

cn::input(&email).input_type(InputType::Email).placeholder("Email")
cn::input(&password).password().placeholder("Password")
cn::input(&age).input_type(InputType::Number).placeholder("Age")
cn::input(&query).input_type(InputType::Search).placeholder("Search...")
```

### Input States

```rust
// Disabled
cn::input(&name).disabled(true)

// With an error message; the border turns red, also while hovered or focused
cn::input(&name).error("Name is required")
```

### Styling

The field is one element with the class `cn-input`. Its border and fill while
hovered, focused, invalid or disabled are rules, eased by a CSS `transition`:
`:hover`, `:focus`, `.cn-input--error` (added by `.error(..)`; a value that
fails validation also sets a `data-invalid` attribute) and `:disabled`.
Restyle it with rules of your own:

```css
.my-field:focus { border-color: #16a34a; outline-color: #16a34a; }
```

`idle_border_color(..)` and `idle_bg_color(..)` set the idle values as inline
style, which the component rules do not replace. The `hover_*`, `focused_*`,
`error_*` and grouped colour setters are removed; use rules.

## OTP Input

Segmented one-time-code input:

```rust
use blinc_cn::prelude::*;

let code = ctx.use_state_keyed("verification_code", || String::new());

cn::input_otp(&code, 6)
    .numeric_only(true)
    .on_complete(|code| println!("complete code: {code}"))
```

### OTP Options

```rust
cn::input_otp(&code, 6)
    .numeric_only(true)
    .masked(false)
    .group_every(3)
    .error(has_error)
```

## Textarea

Multi-line text input:

```rust
textarea()
    .placeholder("Enter description...")
    .rows(4)
    .value(description)
    .on_change(|value| set_description(value))
```

## Checkbox

A checkbox reads and writes the `State<bool>` it is given:

```rust
let accepted = ctx.use_state_keyed("accepted", || false);

checkbox(&accepted)
    .label("Accept terms and conditions")
    .on_change(|checked| println!("checked: {checked}"))
```

```rust
checkbox(&accepted).size(CheckboxSize::Large)
checkbox(&accepted).disabled(true)
```

### Styling

A checkbox is an element with a `checked` attribute while it is checked and
`disabled` while it cannot be used. Its fill, border colour, hover, the
disabled palette and the mark's fade-in are the `.cn-checkbox` rules of
`CN_STYLES`, with a `transition`. Restyle it with rules of your own:

```css
.my-checkbox:checked { background: #16a34a; border-color: #16a34a; }
.my-checkbox:hover:not(:disabled):not(:checked) { border-color: #16a34a; }
```

```rust
checkbox(&accepted).class("my-checkbox")
```

## Switch

```rust
let dark = ctx.use_state_keyed("dark", || false);

switch(&dark)
    .label("Dark mode")
    .on_change(|on| println!("dark: {on}"))
```

```rust
switch(&dark).size(SwitchSize::Small)   // Small, Medium, Large
switch(&dark).disabled(true)
```

### Styling

The switch is the track, with a thumb inside it. The track has a `checked`
attribute while on; the thumb slides by a transform and shrinks slightly while
pressed. All of it is the `.cn-switch` rules of `CN_STYLES`, per size
(`.cn-switch--sm`, `--md`, `--lg`):

```css
.my-switch:checked { background: #16a34a; }
.my-switch:checked:hover:not(:disabled) { background: #15803d; }
```

```rust
switch(&dark).class("my-switch")
```

## Radio Group

A group reads and writes a `State<String>` holding the selected value:

```rust
let size = ctx.use_state_keyed("size", || "md".to_string());

radio_group(&size)
    .label("Size")
    .option("sm", "Small")
    .option("md", "Medium")
    .option("lg", "Large")
    .on_change(|value| println!("size: {value}"))
```

```rust
radio_group(&size).horizontal()
radio_group(&size).size(RadioSize::Large)
radio_group(&size).option_disabled("lg", "Large")   // one option
radio_group(&size).disabled(true)                    // all of them
```

### Styling

Each ring has a `checked` attribute while its option is selected and
`disabled` while it cannot be chosen. The ring's border, the dot's fade-in,
hover (which also highlights an unselected option) and the disabled fill are
the `.cn-radio` and `.cn-radio-dot` rules of `CN_STYLES`:

```css
.my-radio:checked { border-color: #16a34a; }
.my-radio:checked > .cn-radio-dot { background: #16a34a; }
```

## Toggle

A button that stays pressed: a formatting toggle in a toolbar, or a filter.

```rust
let bold = ctx.use_state_keyed("bold", || false);

toggle(&bold)
    .label("B")
    .on_change(|on| println!("bold: {on}"))
```

```rust
// An icon, with or without a label
toggle(&bold).icon(to_svg(icons::BOLD, 16.0))

// Outline variant: a hairline in both states
toggle(&bold).label("B").variant(ToggleVariant::Outline)

// Sizes: Small (32px), Medium (36px), Large (40px)
toggle(&bold).label("B").size(ToggleSize::Small)

toggle(&bold).label("B").disabled(true)
```

### Styling

A toggle is one element carrying `data-state="on"` or `"off"` (and a
`checked` attribute while on), `data-variant`, `data-size` and `disabled`.
Its look is the `.blinc-toggle` rules of the layout stylesheet, ported from
ashui's: transparent when off, a faint wash of the text colour when on, a
lighter wash on hover, and a slight shrink while pressed, all eased by a
`transition`. Sizes and variants are attribute selectors, so your own rules
can use them the same way:

```css
.my-toggle[data-state="on"] { background: #16a34a; color: white; }
.my-toggle[data-size="lg"] { min-width: 48px; }
```

```rust
toggle(&bold).class("my-toggle")
```

## Toggle Group

A row of toggles where one value is selected, such as text alignment. The
group reads and writes a `State<String>`:

```rust
let align = ctx.use_state_keyed("align", || "left".to_string());

toggle_group(&align)
    .variant(ToggleVariant::Outline)
    .item(toggle_item("left").label("Left"))
    .item(toggle_item("center").label("Centre"))
    .item(toggle_item("right").label("Right"))
    .on_change(|value| println!("align: {value}"))

// Items take an icon too, with or without a label
toggle_item("add").icon(to_svg(icons::PLUS, 16.0)).label("Add")
```

Clicking the selected item leaves it selected; to clear the selection, set the
state to `""` yourself. `.size(..)`, `.disabled(true)` for the group and
`toggle_item(..).disabled(true)` for one item work as on a toggle.

Each item is a toggle, so the rules above apply to it. The group sets
`data-variant` and `data-size` for its items, and its rules (`.blinc-toggle-group`)
give a 4px gap; `.gap(..)` overrides it.

## Select

Dropdown selection:

```rust
select()
    .value(selected)
    .on_change(|value| set_selected(value))
    .child(select_trigger()
        .child(select_value().placeholder("Select option...")))
    .child(select_content()
        .child(select_item("opt1").child(text("Option 1")))
        .child(select_item("opt2").child(text("Option 2")))
        .child(select_item("opt3").child(text("Option 3"))))
```

### Grouped Options

```rust
select()
    .child(select_trigger().child(select_value()))
    .child(select_content()
        .child(select_group()
            .child(select_label("Fruits"))
            .child(select_item("apple").child(text("Apple")))
            .child(select_item("banana").child(text("Banana"))))
        .child(select_separator())
        .child(select_group()
            .child(select_label("Vegetables"))
            .child(select_item("carrot").child(text("Carrot")))
            .child(select_item("broccoli").child(text("Broccoli")))))
```

## Combobox

Searchable select with autocomplete:

```rust
combobox()
    .value(selected)
    .on_change(|value| set_selected(value))
    .child(combobox_trigger()
        .child(combobox_input().placeholder("Search...")))
    .child(combobox_content()
        .child(combobox_empty().child(text("No results found")))
        .child(combobox_item("react").child(text("React")))
        .child(combobox_item("vue").child(text("Vue")))
        .child(combobox_item("svelte").child(text("Svelte"))))
```

## Slider

Range slider:

```rust
slider()
    .value(volume)
    .min(0.0)
    .max(100.0)
    .step(1.0)
    .on_change(|value| set_volume(value))
```

### Range Slider

```rust
slider()
    .value_range(min_price, max_price)
    .min(0.0)
    .max(1000.0)
    .on_change_range(|min, max| {
        set_min_price(min);
        set_max_price(max);
    })
```

## Label

```rust
// Associated with input via for
label("Email").for_id("email-input")

// Direct child of input
checkbox()
    .child(label("Remember me"))
```

## Form Layout Example

```rust
div()
    .flex_col()
    .gap(24.0)
    .max_w(400.0)
    // Name field
    .child(
        div().flex_col().gap(4.0)
            .child(label("Name"))
            .child(input()
                .placeholder("John Doe")
                .value(&name)
                .on_change(|v| set_name(v)))
    )
    // Email field
    .child(
        div().flex_col().gap(4.0)
            .child(label("Email"))
            .child(input()
                .input_type("email")
                .placeholder("john@example.com")
                .value(&email)
                .on_change(|v| set_email(v)))
    )
    // Country select
    .child(
        div().flex_col().gap(4.0)
            .child(label("Country"))
            .child(select()
                .value(&country)
                .on_change(|v| set_country(v))
                .child(select_trigger().child(select_value()))
                .child(select_content()
                    .child(select_item("us").child(text("United States")))
                    .child(select_item("uk").child(text("United Kingdom")))
                    .child(select_item("ca").child(text("Canada")))))
    )
    // Terms checkbox
    .child(
        checkbox()
            .checked(accepted_terms)
            .on_change(|v| set_accepted_terms(v))
            .child(label("I accept the terms and conditions"))
    )
    // Submit button
    .child(
        button("Submit")
            .full_width(true)
            .disabled(!accepted_terms)
            .on_click(|| submit_form())
    )
```

## Validation

```rust
let email = use_state_keyed("form_email", || String::new());
let email_error = use_derived(|| {
    let value = email.get();
    if value.is_empty() {
        None
    } else if !value.contains('@') {
        Some("Invalid email address")
    } else {
        None
    }
});

div().flex_col().gap(4.0)
    .child(label("Email"))
    .child(input()
        .value(&email)
        .error(email_error.is_some())
        .on_change({
            let email = email.clone();
            move |v| email.set(v)
        }))
    .child(
        email_error.map(|err|
            text(err).size(12.0).color(Color::RED)
        )
    )
```
