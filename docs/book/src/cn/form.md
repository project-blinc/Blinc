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

// Text that follows signals: an empty error clears it
let placeholder = signal("you@example.com".to_string());
let error = signal(String::new());
cn::input(&email)
    .placeholder(placeholder)
    .description("We never share it")
    .error(error)
```

The placeholder, description and error each take a string or a signal and
change in place. While the error is non-empty it shows in place of the
description.

### Styling

The field is one element with the class `cn-input`. Its border and fill while
hovered, focused, invalid or disabled are rules, eased by a CSS `transition`:
`:hover`, `:focus`, `[data-invalid]` (set while the error is non-empty, and
while a value fails validation) and `:disabled`.
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

let code = ctx.use_signal(String::new());

cn::input_otp(code, 6)
    .numeric_only(true)
    .on_complete(|code| println!("complete code: {code}"))
```

### OTP Options

```rust
cn::input_otp(code, 6)
    .numeric_only(true)
    .masked(false)
    .group_every(3)
    .error(has_error)        // a bool, Signal or Computed
```

## Textarea

Multi-line text input, bound to a `SharedTextAreaState`:

```rust
use blinc_layout::widgets::text_area::text_area_state;

let description = text_area_state();

cn::textarea(&description)
    .label("Description")
    .placeholder("Enter description...")
    .rows(4)
    .on_change(|value| println!("description: {value}"))
```

`.error("msg")` shows the message and turns the border red; `.disabled(true)`
dims the field. The message under a field is `.cn-field-error`, or
`.cn-field-description` for `.description(..)`, coloured by those rules. It is styled like [Input](#input): one element with the class
`cn-textarea`, whose border and fill per state are `:hover`, `:focus`,
`[data-invalid]` and `:disabled` rules, eased by a `transition`. Its
placeholder, description and error follow signals as an input's do.

## Number Input

A number between a − and a +, bound to a `Signal<f64>`:

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

`.size(..)` picks a scale of its own, a step smaller than a text input's, so
the control sits in a row beside others. The group carries the size as
`data-size` for rules to key on:

| Size | `data-size` | Row | Steps | Field (at least) | Text |
|------|-------------|-----|-------|------------------|------|
| `InputSize::Small` | `sm` | 24px | 24px | 40px | 12px |
| `InputSize::Medium` (default) | `md` | 32px | 32px | 48px | 13px |
| `InputSize::Large` | `lg` | 40px | 36px | 52px | 14px |

Typing, the arrow keys, the buttons and the mouse wheel all clamp to `min` and `max`. One wheel line is one step and wheel up increases the value; trackpad movement accumulates into steps, and a disabled input ignores the wheel. The
width follows the bounds, not the value, so typing never resizes it.

The group is one bordered element (`.cn-number-input-group`) with a borderless
field and two steps (`.cn-number-input-step--dec` and `--inc`). A step tints on
`:hover` and `:active`, and is `disabled`, so dimmed and inert, where a step
would change nothing. These are rules with a `transition`; restyle them with
rules of your own.

## Checkbox

A checkbox reads and writes the `Signal<bool>` it is given:

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

The label takes a string, Signal or Computed too.

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

The switch is the track, with a thumb inside it. The track has a `checked`
attribute while on; the thumb slides by a transform and shrinks slightly while
pressed. All of it is the `.cn-switch` rules of `CN_STYLES`, per size
(`.cn-switch--sm`, `--md`, `--lg`):

```css
.my-switch:checked { background: #16a34a; }
.my-switch:checked:hover:not(:disabled) { background: #15803d; }
```

```rust
switch(dark).class("my-switch")
```

## Radio Group

A group reads and writes a `Signal<String>` holding the selected value:

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

A toggle is one element carrying `data-state="on"` or `"off"` (and a
`checked` attribute while on), `data-variant`, `data-size` and `disabled`.
Its look is the `.blinc-toggle` rules of the layout stylesheet: transparent
when off, a faint wash of the text colour when on, a
lighter wash on hover, and a slight shrink while pressed, all eased by a
`transition`. Sizes and variants are attribute selectors, so your own rules
can use them the same way:

```css
.my-toggle[data-state="on"] { background: #16a34a; color: white; }
.my-toggle[data-size="lg"] { min-width: 48px; }
```

```rust
toggle(bold).class("my-toggle")
```

## Toggle Group

A row of toggles where one value is selected, such as text alignment. The
group reads and writes a `Signal<String>`:

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

Clicking the selected item leaves it selected; to clear the selection, set the
signal to `""` yourself. `.size(..)`, `.disabled(..)` for the group (a bool, Signal or Computed) and
`toggle_item(..).disabled(true)` for one item work as on a toggle.

Each item is a toggle, so the rules above apply to it. The group sets
`data-variant` and `data-size` for its items, and its rules (`.blinc-toggle-group`)
give a 4px gap; `.gap(..)` overrides it.

## Select

Dropdown selection, bound to a `Signal<String>` holding the selected value:

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

`.disabled(..)` makes the whole select inert; it takes a bool, Signal or Computed.

The open list grows to 300px and then scrolls, and it opens with the chosen
option in the middle. For a long list, `.virtual_rows(row_height)` builds only
the rows in view, each `row_height` pixels tall, so hundreds of options open
at once:

```rust
let year = ctx.use_signal("2026".to_string());

cn::select(year)
    .options((1900..=2100).map(|y| SelectOption::new(y.to_string(), y.to_string())))
    .virtual_rows(32.0)
```

### Styling

The trigger is one element with the class `cn-select-trigger`. It has a
`disabled` attribute while disabled, `data-placeholder` while nothing is
chosen and `data-state="open"` while its list is showing, so its look is
rules, eased by a `transition`: a faint tint on `:hover`, a stronger one on
`:active`, a darker border while open, and the disabled palette. The shown
text inherits the trigger's `color`, which the placeholder and disabled rules
dim. Restyle it with rules of your own:

```css
.cn-select-trigger[data-state="open"] { border-color: #16a34a; }
```

## Calendar

A month of days to pick a date from, bound to a `Signal<Option<CalendarDay>>`:

```rust
let picked = ctx.use_signal(None::<CalendarDay>);

cn::calendar(picked)
    .week_start(1)          // Monday first; 0 (Sunday) is the default
    .min_year(2020)
    .max_year(2030)
    .on_change(|day| println!("{}-{}-{}", day.year, day.month, day.day))
```

`CalendarDay` holds a year, a month from 1 to 12 and a day. `CalendarDay::today()`
is the local date, and `add_days`, `add_months` and `weekday` do the date
arithmetic. The month and year selects, or the buttons either side of them,
turn the month. Dragging or a two-finger swipe across the days moves the
month with it; let go far enough and it turns (leftward for the next), or it
springs back. Moving the pointer off the days cancels it. Scrolling up and down still moves the page;
choosing a day of the month before or after turns to it.
Tab to the day grid, or click a day: the arrows move by a day or a week, Page Up and
Page Down by a month, Home and End to the ends of the week, and Enter or Space
choose.
For a compact trigger that opens this calendar in a popover, use
[Date Picker](pickers.md#date-picker).

### Events

`.events(..)` marks days with what is on them: meetings, the weather,
deadlines. It takes a list, or a Signal or Computed of one, and the marks
follow it in place. Up to three show along the bottom of a day, each a dot or,
with `.icon(..)`, the event's icon:

```rust
let events = ctx.use_signal(vec![
    CalendarEvent::new(CalendarDay::new(2026, 11, 3), "Standup").kind("meeting"),
    CalendarEvent::new(CalendarDay::new(2026, 11, 4), "Rain")
        .kind("rain")
        .icon(to_svg(icons::CLOUD_RAIN, 8.0)),
]);

cn::calendar(picked).events(events)
```

`events_on(&events, day)` lists what is on a day, for showing beside the
calendar.

### Styling

The parts are `.cn-calendar`, `-header`, `-caption`, `-nav` (with
`data-step="prev"` or `"next"`), `-body`, `-weekdays`, `-weekday`, `-months`,
`-page`, `-grid` and `-day`. A day carries `data-outside`, `data-today`,
`data-selected`, `data-events`, `data-focus-visible` while the keys are on it,
and `disabled` outside the allowed years. A turning month's grid carries
`data-enter` while it slides in and `data-leaving` while it slides out, each
`"next"` or `"prev"`, and slides by the `cn-calendar-*` keyframes. An event's
mark is `.cn-calendar-event` with its `data-kind`, and `.cn-calendar-event--icon`
when it is an icon; a dot takes its `background`, an icon its `color`:

```css
.cn-calendar-event[data-kind="rain"] { color: #38bdf8; }
.cn-calendar-event[data-kind="deadline"] { background: var(--error); }
.cn-calendar-day[data-today] { background: transparent; }
```

## Combobox

Searchable select, bound to a `Signal<String>` holding the selected value:

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

Typing in the list's search field filters the options.

### Styling

The trigger is one element with the class `cn-combobox-trigger`. It has a
`disabled` attribute while disabled and `data-state="open"` while its list is
showing, so its border is rules, eased by a `transition`: darker on `:hover`,
the focus border while open, and the disabled palette. The search field is a
text input with the class `cn-combobox-search`. Restyle either with rules of
your own.

## Slider

A number picked along a track, bound to a `Signal<f32>` or `Signal<f64>`:

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

Click where you want it or drag the thumb; the wheel steps it, and after a
click the arrow keys, Page Up/Down and Home/End move it too. Without a step,
an arrow or wheel step moves a hundredth of the range. `.size(SliderSize::Small)`
sets the size, `.w(px)` the track's length (300px unless set), and
`.disabled(..)` takes a bool, Signal or Computed and follows it in place.

The slider is a `.cn-slider` row of `.cn-slider-fill`, `.cn-slider-thumb` and
`.cn-slider-rest`, inside a `.cn-slider-field` with a `.cn-slider-header`
(`.cn-slider-label`, `.cn-slider-value`) when it has a label or shows its value.
Colours are those rules; `--cn-slider-fill-bg`, `--cn-slider-track-bg` and
`--cn-slider-thumb-bg` recolour one. The thumb grows under the pointer and more
while held, and a disabled slider carries `disabled`:

```css
.cn-slider:disabled > .cn-slider-fill { background: var(--border); }
```

For two ends on one control, use a [Number Range Gauge](#number-range-gauge).

## Number Gauge

A number shown as a fill, with the value written over it:

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

Drag it, click where you want it, or scroll over it. After a click the arrow
keys, Page Up/Down and Home/End move it too. Values snap to the step.

The gauge is as wide as its longest value needs, so it doesn't change width
as you drag; `.w(px)` sets a width instead.

### Number Range Gauge

Two ends on one gauge, held in one `Signal<RangeInclusive<f64>>`:

```rust
let band = ctx.use_signal(18.0..=24.0);

cn::number_range_gauge(band)
    .range(10.0..=30.0)
    .step(0.5)
    .format(|r| format!("{}–{} °C", r.start(), r.end()))
```

Clicking or scrolling moves the nearer end. The ends never cross.

### Styling

Both are a `.cn-number-gauge` (the range one also `.cn-number-range-gauge`)
holding `.cn-number-gauge-fill`, `.cn-number-gauge-thumb` and
`.cn-number-gauge-rest`, with the value in `.cn-number-gauge-value`. The range
gauge puts each face in a `.cn-number-gauge-layer` with
`data-endpoint="lower"` or `"upper"`. The gauge has `data-state="dragging"`
during a drag, `data-focus-visible` once a key has moved it, and `disabled`.
Colours, the border, the radius per size (`.cn-number-gauge--sm`, `--md`,
`--lg`) and the cursor are rules of `CN_STYLES`:

```css
.my-gauge > .cn-number-gauge-fill { background: color-mix(in srgb, #16a34a 25%, var(--input-bg)); }
.my-gauge > .cn-number-gauge-thumb { color: #16a34a; }
```

```rust
cn::number_gauge(opacity).class("my-gauge")
```

## Label

Inputs, selects and checkboxes take a `.label(..)` of their own. A standalone
label's text is a string, a `Signal<String>` or a `Computed<String>`, and
follows a signal in place:

```rust
cn::label("Email").required()      // marks the field required
cn::label("Notes").size(LabelSize::Small)
cn::label(status)                  // a Signal<String>
```

## Form Layout Example

```rust
use blinc_core::reactive::computed;
use blinc_layout::widgets::text_input::{InputType, text_input_data};

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

An `Email` or `Number` input checks its own value and sets `data-invalid`
while it fails, which the `.cn-input` rules show as a red border. For a
message of your own, write it to a signal and show it in a label:

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
