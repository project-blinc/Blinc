# Form Components

Use these components to build forms. They include inputs, checkboxes, selects
and more.

## Input

The input is a text field that is bound to a `SharedTextInputData`:

```rust
use blinc::cn::prelude::*;
use blinc::layout::widgets::text_input::text_input_data;

let name = text_input_data();

cn::input(&name)
    .label("Name")
    .placeholder("Enter your name...")
    .on_change(|value| println!("name: {value}"))
```

### Input Types

```rust
use blinc::layout::widgets::text_input::InputType;

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

// Text that follows signals: an empty error clears it
let placeholder = signal("you@example.com".to_string());
let error = signal(String::new());
cn::input(&email)
    .placeholder(placeholder)
    .description("We never share it")
    .error(error)
```

The placeholder, description and error each take a string or a signal, and they
change in place. While the error is not empty, it shows instead of the
description.

### Styling

The field is one element with the class `cn-input`. While it is hovered,
focused, invalid or disabled, CSS rules set its border and fill. A CSS
`transition` makes these changes smooth. The rules use `:hover`, `:focus`,
`[data-invalid]` and `:disabled`. While the error is not empty, or while a value
fails validation, the field has `data-invalid`. To restyle the field, write your
own rules:

```css
.my-field:focus { border-color: #16a34a; outline-color: #16a34a; }
```

`idle_border_color(..)` and `idle_bg_color(..)` set the idle values as inline
style. The component rules do not replace these values. The `hover_*`,
`focused_*`, `error_*` and grouped color setters are removed. Use rules instead.

## OTP Input

The OTP input has separate segments for a one-time code:

```rust
use blinc::cn::prelude::*;

let code = ctx.use_signal(String::new());

cn::input_otp(code, 6)
    .numeric_only(true)
    .on_complete(|code| println!("complete code: {code}"))
```

### OTP Options

Tab and Shift+Tab reach each enabled slot, then continue to the controls before
or after the OTP input. When you type, the focus still advances automatically.
Backspace in an empty slot moves back. When you paste a code, the input spreads
it across the slots. Disabled slots are skipped.

```rust
cn::input_otp(code, 6)
    .numeric_only(true)
    .masked(false)
    .group_every(3)
    .error(has_error)        // a bool, Signal or Computed
```

## Textarea

The textarea is a text input with multiple lines. It is bound to a
`SharedTextAreaState`:

```rust
use blinc::layout::widgets::text_area::text_area_state;

let description = text_area_state();

cn::textarea(&description)
    .label("Description")
    .placeholder("Enter description...")
    .rows(4)
    .on_change(|value| println!("description: {value}"))
```

`.error("msg")` shows the message and turns the border red. `.disabled(true)`
dims the field. The message under a field has the class `.cn-field-error`. For
`.description(..)`, the class is `.cn-field-description`. The rules for these
classes color the message.

The textarea is styled like [Input](#input). It is one element with the class
`cn-textarea`. The `:hover`, `:focus`, `[data-invalid]` and `:disabled` rules
set its border and fill for each state, and a `transition` makes the changes
smooth. Its placeholder, description and error follow signals, as they do on an
input.

## Number Input

The number input shows a number between a − button and a + button. It is bound
to a `Signal<f64>`:

```rust
let qty = ctx.use_signal(1.0);

cn::number_input(qty)
    .min(0.0)
    .max(99.0)
    .step(1.0)
    .precision(0)
    .size(InputSize::Medium)
    .on_change(|value| println!("qty: {value}"))
```

`.size(..)` selects from a size scale that belongs to the number input. This
scale is one step smaller than the scale of a text input, so the control fits in
a row beside other controls. The group stores the size as `data-size`, and your
rules can select on it:

| Size | `data-size` | Row | Steps | Field (at least) | Text |
|------|-------------|-----|-------|------------------|------|
| `InputSize::Small` | `sm` | 24px | 24px | 40px | 12px |
| `InputSize::Medium` (default) | `md` | 32px | 32px | 48px | 13px |
| `InputSize::Large` | `lg` | 40px | 36px | 52px | 14px |

Typing, the arrow keys, the buttons and the mouse wheel all keep the value
between `min` and `max`. One wheel line is one step, and wheel up increases the
value. Trackpad movement adds up into steps. A disabled input ignores the wheel.
The width of the control depends on the bounds, not on the value. Thus, typing
never changes the width.

The group is one element with a border (`.cn-number-input-group`). It contains
a field with no border and two step buttons (`.cn-number-input-step--dec` and
`--inc`). On `:hover` and `:active`, a step button gets a tint. If a step cannot
change the value, its button is `disabled`, so it is dimmed and does not
respond. Rules with a `transition` define these looks. To restyle them, write
your own rules.

## Checkbox

A checkbox reads and writes the `Signal<bool>` that you give to it:

```rust
let accepted = ctx.use_signal(false);

checkbox(accepted)
    .label("Accept terms and conditions")
    .on_change(|checked| println!("checked: {checked}"))
```

```rust
checkbox(accepted).size(CheckboxSize::Large)
checkbox(accepted).disabled(locked)   // a bool, Signal or Computed
```

The label also takes a string, Signal or Computed.

### Styling

While a checkbox is checked, its element has a `checked` attribute. While it
cannot be used, it has `disabled`. The `.cn-checkbox` rules of `CN_STYLES`,
with a `transition`, set its fill, border color and hover look. They also set
the disabled palette and the fade-in of the check mark. To restyle the
checkbox, write your own rules:

```css
@layer components {
    .my-checkbox:checked {
        background: #16a34a !important;
        border-color: #16a34a !important;
    }
}
.my-checkbox:hover:not(:disabled):not(:checked) { border-color: #16a34a; }
```

```rust
checkbox(accepted).class("my-checkbox")
```

## Switch

```rust
let dark = ctx.use_signal(false);

switch(dark)
    .label("Dark mode")
    .on_change(|on| println!("dark: {on}"))
```

```rust
switch(dark).size(SwitchSize::Small)   // Small, Medium, Large
switch(dark).disabled(locked)          // a bool, Signal or Computed
```

### Styling

The switch element is the track, and a thumb is inside it. While the switch is
on, the track has a `checked` attribute. The thumb slides by a transform. While
pressed, the thumb shrinks slightly. The `.cn-switch` rules of `CN_STYLES`
define all of this for each size (`.cn-switch--sm`, `--md`, `--lg`):

```css
@layer components {
    .my-switch:checked {
        background: #16a34a !important;
    }
    .my-switch:checked:hover:not(:disabled) {
        background: #15803d !important;
    }
}
```

```rust
switch(dark).class("my-switch")
```

## Radio Group

A group reads and writes a `Signal<String>` that holds the selected value:

```rust
let size = ctx.use_signal("md".to_string());

radio_group(size)
    .label("Size")
    .option("sm", "Small")
    .option("md", "Medium")
    .option("lg", "Large")
    .on_change(|value| println!("size: {value}"))
```

```rust
radio_group(size).horizontal()
radio_group(size).size(RadioSize::Large)
radio_group(size).option_disabled("lg", "Large")   // one option
radio_group(size).disabled(locked)                  // all of them; a bool, Signal or Computed
```

### Styling

While its option is selected, each ring has a `checked` attribute. While the
option cannot be chosen, the ring has `disabled`. Hover also highlights an
unselected option. The `.cn-radio` and `.cn-radio-dot` rules of `CN_STYLES` set
the ring border, the fade-in of the dot, hover and the disabled fill:

```css
@layer components {
    .my-radio:checked {
        border-color: #16a34a !important;
    }
    .my-radio:checked > .cn-radio-dot {
        background: #16a34a !important;
    }
}
```

## Toggle

A toggle is a button that stays pressed. Use it, for example, as a formatting
toggle in a toolbar or as a filter.

```rust
let bold = ctx.use_signal(false);

toggle(bold)
    .label("B")
    .on_change(|on| println!("bold: {on}"))
```

```rust
// An icon, with or without a label
toggle(bold).icon(to_svg(icons::BOLD, 16.0))

// Outline variant: a hairline in both states
toggle(bold).label("B").variant(ToggleVariant::Outline)

// Sizes: Small (32px), Medium (36px), Large (40px)
toggle(bold).label("B").size(ToggleSize::Small)

toggle(bold).label("B").disabled(locked)   // a bool, Signal or Computed
```

### Styling

A toggle is one element that carries `data-state="on"` or `"off"`,
`data-variant`, `data-size` and `disabled`. While on, it also has a `checked`
attribute.

The `.blinc-toggle` rules of the layout stylesheet define its look. When off,
the toggle is transparent. When on, it has a faint tint of the text color. On
hover, it has a lighter tint. While pressed, it shrinks slightly. A
`transition` makes all of these changes smooth.

The rules select sizes and variants with attribute selectors, so your own rules
can do the same:

```css
@layer components {
    .my-toggle[data-state="on"] {
        background: #16a34a !important;
        color: white !important;
    }
}
.my-toggle[data-size="lg"] { min-width: 48px; }
```

```rust
toggle(bold).class("my-toggle")
```

## Toggle Group

A toggle group is a row of toggles in which one value is selected, such as a
text alignment. The group reads and writes a `Signal<String>`:

```rust
let align = ctx.use_signal("left".to_string());

toggle_group(align)
    .variant(ToggleVariant::Outline)
    .item(toggle_item("left").label("Left"))
    .item(toggle_item("center").label("Centre"))
    .item(toggle_item("right").label("Right"))
    .on_change(|value| println!("align: {value}"))

// Items take an icon too, with or without a label
toggle_item("add").icon(to_svg(icons::PLUS, 16.0)).label("Add")
```

If you click the selected item, it stays selected. To clear the selection, set
the signal to `""` yourself. `.size(..)` and `.disabled(..)` (a bool, Signal or
Computed) on the group work as on a toggle. `toggle_item(..).disabled(true)`
does the same for one item.

Each item is a toggle, so the rules above apply to it. The group sets
`data-variant` and `data-size` for its items. The rules of the group
(`.blinc-toggle-group`) give a 4px gap, and `.gap(..)` overrides this gap.

## Select

With a select, you choose a value from a dropdown list. It is bound to a
`Signal<String>` that holds the selected value:

```rust
let fruit = ctx.use_signal(String::new());

cn::select(fruit)
    .label("Fruit")
    .option("apple", "Apple")
    .option("banana", "Banana")
    .option_disabled("durian", "Durian")
    .placeholder("Select a fruit...")
    .size(SelectSize::Medium)
    .w(240.0)
    .on_change(|value| println!("selected: {value}"))
```

`.disabled(..)` makes the whole select inactive. It takes a bool, Signal or
Computed.

The open list grows to 300px and then scrolls. It opens with the chosen option
in the middle. For a long list, use `.virtual_rows(row_height)`. It builds only
the rows in view, and each row is `row_height` pixels tall. Thus, a list with
hundreds of options opens immediately:

```rust
let year = ctx.use_signal("2026".to_string());

cn::select(year)
    .options((1900..=2100).map(|y| SelectOption::new(y.to_string(), y.to_string())))
    .virtual_rows(32.0)
```

### Styling

The trigger is one element with the class `cn-select-trigger`. While disabled,
it has a `disabled` attribute. While nothing is chosen, it has
`data-placeholder`. While its list is open, it has `data-state="open"`.

Thus, rules set its look, and a `transition` makes the changes smooth. The rules
give a faint tint on `:hover` and a stronger tint on `:active`. While the list
is open, they give a darker border. A disabled trigger uses the disabled
palette.

The shown text inherits the `color` of the trigger, and the placeholder and
disabled rules dim this color. To restyle the trigger, write your own rules:

```css
.cn-select-trigger[data-state="open"] { border-color: #16a34a; }
```

## Calendar

The calendar shows the days of a month, and you pick a date from them. It is
bound to a `Signal<Option<CalendarDay>>`:

```rust
let picked = ctx.use_signal(None::<CalendarDay>);

cn::calendar(picked)
    .week_start(1)          // Monday first; 0 (Sunday) is the default
    .min_year(2020)
    .max_year(2030)
    .on_change(|day| println!("{}-{}-{}", day.year, day.month, day.day))
```

`CalendarDay` holds a year, a month from 1 to 12 and a day.
`CalendarDay::today()` is the local date. `add_days`, `add_months` and
`weekday` do the date arithmetic.

To change the month, use the month and year selects, or the buttons on either
side of them. If you drag or swipe with two fingers across the days, the month
moves with your movement. If you let go after you move far enough, the month
changes (a leftward movement shows the next month). Otherwise, the month springs
back. If the pointer moves off the days, the drag or swipe is canceled.

Vertical scrolling still moves the page. If you choose a day of the previous or
next month, the calendar changes to that month.

To use the keyboard, Tab to the day grid, or click a day. Then use these keys:

- The arrow keys move by a day or a week.
- Page Up and Page Down move by a month.
- Home and End move to the ends of the week.
- Enter or Space chooses the day that the keys are on.

If you need a compact trigger that opens this calendar in a popover, use
[Date Picker](pickers.md#date-picker).

### Events

`.events(..)` marks days with their events, such as meetings, the weather or
deadlines. It takes a list, or a Signal or Computed of a list, and the marks
follow it in place. A day shows up to three marks along its bottom. Each mark is
a dot or, with `.icon(..)`, the icon of the event:

```rust
let events = ctx.use_signal(vec![
    CalendarEvent::new(CalendarDay::new(2026, 11, 3), "Standup").kind("meeting"),
    CalendarEvent::new(CalendarDay::new(2026, 11, 4), "Rain")
        .kind("rain")
        .icon(to_svg(icons::CLOUD_RAIN, 8.0)),
]);

cn::calendar(picked).events(events)
```

To show the events of a day beside the calendar, get the list with
`events_on(&events, day)`.

### Styling

The parts are `.cn-calendar`, `-header`, `-caption` and `-nav` (with
`data-step="prev"` or `"next"`). The other parts are `-body`, `-weekdays`,
`-weekday`, `-months`, `-page`, `-grid` and `-day`.

A day carries `data-outside`, `data-today`, `data-selected` and `data-events`.
While the keys are on it, it also carries `data-focus-visible`. Outside the
allowed years, it carries `disabled`.

When the month changes, two grids move. While the new grid slides in, it
carries `data-enter`. While the old grid slides out, it carries `data-leaving`.
Each value is `"next"` or `"prev"`. The grids slide by the `cn-calendar-*`
keyframes.

The mark of an event is `.cn-calendar-event` with its `data-kind`. If the mark
is an icon, it also has `.cn-calendar-event--icon`. To color a dot, set its
`background`. To color an icon, set its `color`:

```css
.cn-calendar-event[data-kind="rain"] { color: #38bdf8; }
.cn-calendar-event[data-kind="deadline"] { background: var(--error); }
.cn-calendar-day[data-today] { background: transparent; }
```

## Combobox

The combobox is a select that you can search. It is bound to a `Signal<String>`
that holds the selected value:

```rust
let framework = ctx.use_signal(String::new());

cn::combobox(framework)
    .option("react", "React")
    .option("vue", "Vue")
    .option_disabled("angular", "Angular")
    .placeholder("Select a framework...")
    .size(ComboboxSize::Medium)
    .w(240.0)
    .allow_custom(false)
    .on_change(|value| println!("selected: {value}"))
```

The text that you type in the search field of the list filters the options.

### Styling

The trigger is one element with the class `cn-combobox-trigger`. While disabled,
it has a `disabled` attribute. While its list is open, it has
`data-state="open"`.

Thus, rules set its border, and a `transition` makes the changes smooth. On
`:hover`, the border is darker. While the list is open, the trigger uses the
focus border. A disabled trigger uses the disabled palette.

The search field is a text input with the class `cn-combobox-search`. To restyle
the trigger or the search field, write your own rules.

## Slider

With a slider, you pick a number along a track. It is bound to a `Signal<f32>`
or `Signal<f64>`:

```rust
let volume = ctx.use_signal(50.0);

cn::slider(volume)
    .min(0.0)
    .max(100.0)
    .step(1.0)
    .label("Volume")
    .show_value()
    .on_change(|value| println!("volume: {value}"))
```

To set the value, click the position that you want on the track, or drag the
thumb. The wheel also steps the value. After a click, the arrow keys, Page
Up/Down and Home/End move the value too. If you do not set a step, one arrow or
wheel step moves one hundredth of the range.

`.size(SliderSize::Small)` sets the size. `.w(px)` sets the length of the track.
If you do not set it, the length is 300px. `.disabled(..)` takes a bool, Signal
or Computed and follows it in place.

The label and live value form one adjustable element in the
[accessibility snapshot](../core/accessibility.md). Assistive adjustment uses
the same step, bounds and change callback. Disabled sliders reject it.

The slider is a `.cn-slider` row of `.cn-slider-fill`, `.cn-slider-thumb` and
`.cn-slider-rest`. If the slider has a label or shows its value, this row is
inside a `.cn-slider-field`. That field has a `.cn-slider-header`
(`.cn-slider-label`, `.cn-slider-value`).

The rules for these classes set the colors. To recolor one slider, use
`--cn-slider-fill-bg`, `--cn-slider-track-bg` and `--cn-slider-thumb-bg`. Under
the pointer, the thumb grows. While held, it grows more. A disabled slider
carries `disabled`:

```css
.cn-slider:disabled > .cn-slider-fill { background: var(--border); }
```

If you need two ends on one control, use a
[Number Range Gauge](#number-range-gauge).

## Number Gauge

The number gauge shows a number as a fill, with the value written over it:

```rust
let opacity = ctx.use_signal(64.0);

cn::number_gauge(opacity)
    .range(0.0..=100.0)              // the default
    .step(1.0)                       // the default
    .size(GaugeSize::Medium)         // Small, Medium, Large
    .format(|n| format!("{n} %"))
    .disabled(locked)                // a bool, Signal or Computed
    .on_change(|n| println!("opacity: {n}"))
```

To change the value, drag the gauge, click the position that you want, or scroll
over it. After a click, the arrow keys, Page Up/Down and Home/End move it too.
Values snap to the step.

The gauge is as wide as its longest value needs, so a drag does not change its
width. `.w(px)` sets a width instead.

### Number Range Gauge

This gauge has two ends. One `Signal<RangeInclusive<f64>>` holds them:

```rust
let band = ctx.use_signal(18.0..=24.0);

cn::number_range_gauge(band)
    .range(10.0..=30.0)
    .step(0.5)
    .format(|r| format!("{}–{} °C", r.start(), r.end()))
```

A click or a scroll moves the nearer end. The ends never cross.

### Styling

Each gauge is a `.cn-number-gauge`, and the range gauge is also a
`.cn-number-range-gauge`. Each one holds `.cn-number-gauge-fill`,
`.cn-number-gauge-thumb` and `.cn-number-gauge-rest`. The value is in
`.cn-number-gauge-value`. In the range gauge, the fill, thumb and rest of each
end are in a `.cn-number-gauge-layer` with `data-endpoint="lower"` or `"upper"`.

During a drag, the gauge has `data-state="dragging"`. After a key moves the
gauge, it has `data-focus-visible`. It can also have `disabled`. The rules of
`CN_STYLES` set the colors, the border, the radius for each size
(`.cn-number-gauge--sm`, `--md`, `--lg`) and the cursor:

```css
.my-gauge > .cn-number-gauge-fill { background: color-mix(in srgb, #16a34a 25%, var(--input-bg)); }
.my-gauge > .cn-number-gauge-thumb { color: #16a34a; }
```

```rust
cn::number_gauge(opacity).class("my-gauge")
```

## Label

Inputs, selects and checkboxes have their own `.label(..)`. The text of a
standalone label is a string, a `Signal<String>` or a `Computed<String>`. The
label follows a signal in place:

```rust
cn::label("Email").required()      // marks the field required
cn::label("Notes").size(LabelSize::Small)
cn::label(status)                  // a Signal<String>
```

## Form Layout Example

```rust
use blinc::core::reactive::computed;
use blinc::layout::widgets::text_input::{InputType, text_input_data};

let name = text_input_data();
let email = text_input_data();
let country = ctx.use_signal(String::new());
let accepted = ctx.use_signal(false);

// Submit stays disabled until the terms are accepted.
let blocked = computed(move |g| !g.get(accepted).unwrap_or(false));

div()
    .flex_col()
    .gap(6.0) // 6 × 4px
    .max_w(400.0)
    .child(cn::input(&name).label("Name").placeholder("Ada Lovelace"))
    .child(
        cn::input(&email)
            .label("Email")
            .input_type(InputType::Email)
            .placeholder("ada@example.com"),
    )
    .child(
        cn::select(country)
            .label("Country")
            .placeholder("Select a country...")
            .option("us", "United States")
            .option("uk", "United Kingdom")
            .option("ca", "Canada"),
    )
    .child(cn::checkbox(accepted).label("I accept the terms and conditions"))
    .child(cn::button("Submit").disabled(blocked).on_click(|_| submit()))
```

## Validation

An `Email` or `Number` input tests its own value. While the value is not valid,
the input sets `data-invalid`, and the `.cn-input` rules show a red border. To
show your own message, write it to a signal, and show the signal in a label:

```rust
let email = text_input_data();
let problem = ctx.use_signal(String::new());

div()
    .flex_col()
    .gap(1.0)
    .child(
        cn::input(&email)
            .label("Email")
            .input_type(InputType::Email)
            .on_change(move |value| {
                problem.set(if value.is_empty() || value.contains('@') {
                    String::new()
                } else {
                    "Enter an email address".into()
                })
            }),
    )
    .child(cn::label(problem).size(LabelSize::Small))
```
