# Date, Time and Colour Pickers

The pickers read and write a `Signal` that you supply. External writes update
the existing fields. `on_change` runs only for user edits that change the value.
Before you use the pickers, install `CN_STYLES` through `cn_bundle()` or your
theme bundle.

## Date Picker

`cn::calendar` is the inline component for date selection. `cn::date_picker`
puts that calendar in a popover. It shows the selected date in a compact trigger
(the control that opens the popover). It uses the same `CalendarDay` and
`Signal<Option<CalendarDay>>` model.

The calendar selection uses `--cn-calendar-selected-bg` (default `--primary`)
and `--cn-calendar-selected-color` (default `--text-inverse`). While a day is
selected, its event dots and icons use the selected foreground color. After
that, they return to the application colors. These colors follow the
[protected selection rules](../core/css-styling.md#selected-component-colours).

```rust
use blinc::cn::prelude::*;
use blinc::core::reactive::signal;

let selected = signal(None::<CalendarDay>);
let disabled = signal(false);
let placeholder = signal("Choose a date".to_owned());

let picker = cn::date_picker(selected)
    .placeholder(placeholder)
    .disabled(disabled)
    .week_start(1)
    .format(|day| format!("{:02}/{:02}/{}", day.day, day.month, day.year))
    .on_change(|date| tracing::info!(?date, "Date selected"));
```

The default format is `YYYY-MM-DD`. When you pick a day, the picker writes
`Some(day)` and closes the popup. The clear action writes `None`.
`.clearable(false)` omits that action. `.clear_text(...)` accepts a string, a
Signal, or a Computed. External writes update the trigger and an open calendar,
and they do not call `on_change`. If you choose the same date, the popup closes
and emits no new change. If you disable an open picker, it closes and rejects
edits.

Tab moves to the trigger, and Enter/Space opens it. The calendar keeps its own
month controls and keyboard navigation. A click outside dismisses the popup.
`size(InputSize)`, `week_start`, `min_year`, `max_year`, `today`, and `events`
configure its presentation through the existing calendar API.

| Class | Part |
|---|---|
| `.cn-date-picker-trigger` | Trigger. `data-size="sm\|md\|lg"`, `data-empty`, `data-state="open"`, `disabled`, `:focus-visible` |
| `.cn-date-picker-icon`, `.cn-date-picker-value` | Calendar icon, and the formatted date or placeholder |
| `.cn-date-picker-content` | Detached popup. It also has `.cn-popover-content` |
| `.cn-date-picker-footer` | Row for the clear action |
| `.cn-date-picker-clear` | Clear button. `disabled`, `:hover`, `:focus-visible` |

The calendar keeps its documented `.cn-calendar-*` part classes. To restyle the
picker, override `--cn-date-picker-bg`, `--cn-date-picker-border`, and
`--cn-date-picker-radius`, or target the part classes. `.class(...)` and
`.id(...)` apply to the trigger. The popup is in the overlay layer, outside the
descendants of the trigger.

```css
.booking-date { min-width: 220px; }
.cn-date-picker-content { padding: 16px; }
.cn-date-picker-clear { color: var(--primary); }
```

## Time Picker

```rust
use blinc::cn::prelude::*;
use blinc::core::reactive::{computed, signal};

let time = signal(TimeOfDay::new(14, 30, 0).unwrap());
let disabled = signal(false);
let picker = cn::time_picker(time)
    .format(TimeFormat::TwelveHour)
    .seconds(true)
    .minute_step(5)
    .size(InputSize::Medium)
    .disabled(disabled)
    .on_change(|time| tracing::info!(%time, "Time selected"));

let caption = computed(move |g| {
    g.get(time).unwrap_or_default().format(TimeFormat::TwelveHour, true)
});
```

`TimeOfDay::new(hour, minute, second)` accepts only a valid 24-hour time. For an
out-of-range part, it returns `None`. The value has the accessors `hour()`,
`minute()`, and `second()`. It implements `Display` as `HH:MM:SS`, and it has no
date or time zone.

The default picker shows hours and minutes in 24-hour form. If you hide the
seconds, their stored value stays the same. Midnight is `12 AM`, and noon is
`12 PM`. A click on the period toggles AM/PM. Tab moves between the fields and
the period, and Shift+Tab moves back. Enter/Space activates the focused period.

The clock button opens a compact wheel popup over the control. The popup stays
inside the window edges. It shows five visible rows for each numeric field. The
middle row marks the draft selection (the choice that is not saved yet). The
values near it fade out toward the edges.

To select a value, scroll a column with the mouse wheel or trackpad. You can
also drag the column vertically, or click a nearby value. In 12-hour mode,
separate **AM** and **PM** buttons are above the wheels. These buttons change
the period and keep the selected hour. If you enable seconds, they add a third
numeric column.

**Save** writes the draft to the bound signal. If the value changed, it also
calls `on_change` once. Each of these actions discards the draft: **Cancel**,
Escape, a click outside, and disabling the picker. External writes update the
open draft and do not call `on_change`. The compact fields continue to commit
edits immediately.

Tab moves to each column and to the footer actions, and Shift+Tab goes back.
Up/Down step the focused column. Page Up/Down step five times. Home/End select
the first or last value of the column. `minute_step` and `second_step` apply to
keyboard stepping and to the wheel input of the compact fields.

The popup columns are native scroll containers that hold every valid value. The
columns stop at their ends, but keyboard stepping wraps. The other fields and
the hidden seconds keep their values. When you leave a column, it centers its
nearest value. For the compact editor without a clock button, use
`.popup(false)`.

In each field, type up to two digits, or use Up/Down and the mouse wheel to
step. Stepping wraps within that field and does not carry into the neighboring
fields. `minute_step` and `second_step` affect stepping. Typing accepts any
valid value. Enter restores the two-digit presentation of the field. Invalid or
partial text does not change the selected time.

### Time styling

`.class(...)` and `.id(...)` apply to the outer `.cn-time-picker`. That element
exposes `data-size="sm|md|lg"`, `data-format="12|24"`, and `disabled`. The
control heights are 32, 36, and 40px. The numeric fields share the outer focus
ring (`data-focus-within`). The local highlight of a field shows which part you
edit. When a field loses focus, its selection highlight clears.

| Class | Part |
|---|---|
| `.cn-time-picker-part` | Field wrapper |
| `.cn-time-picker-segment` | Editable field. `:hover`, `:focus`, `:disabled` |
| `.cn-time-picker-segment--sm`, `--md`, `--lg` | Field size classes |
| `.cn-time-picker-hour`, `.cn-time-picker-minute`, `.cn-time-picker-second` | Individual fields |
| `.cn-time-picker-separator` | Colon |
| `.cn-time-picker-period` | AM/PM button. `:focus-visible`, `:disabled` |
| `.cn-time-picker-clock` | Trigger for the wheel popup. `data-state="open"`, `:focus-visible`, `:disabled` |
| `.cn-time-picker-wheel` | Detached popup. `data-format="12\|24"`, `data-seconds` |
| `.cn-time-picker-wheel-header`, `.cn-time-picker-wheel-title` | Heading |
| `.cn-time-picker-wheels` | Five-row numeric viewport |
| `.cn-time-picker-wheel-selection` | Shared border and fill of the selected row |
| `.cn-time-picker-wheel-column` | Target for native scroll, drag, and keyboard input. `:focus-visible`, `overflow-fade`, `scrollbar-width: none` |
| `.cn-time-picker-wheel-hour`, `.cn-time-picker-wheel-minute`, `.cn-time-picker-wheel-second` | Individual columns |
| `.cn-time-picker-wheel-list`, `.cn-time-picker-wheel-option` | Scrollable number list and static value rows |
| `.cn-time-picker-wheel-separator` | Colon between columns |
| `.cn-time-picker-wheel-period`, `.cn-time-picker-wheel-period-option` | Separate period control. Its options expose `data-selected`, `:focus-visible` |
| `.cn-time-picker-wheel-am`, `.cn-time-picker-wheel-pm` | Individual period choices |
| `.cn-time-picker-wheel-footer`, `.cn-time-picker-wheel-cancel`, `.cn-time-picker-wheel-save` | Footer and actions |

Override `--cn-time-picker-bg`, `--cn-time-picker-border`,
`--cn-time-picker-radius`, `--cn-time-picker-field-hover`, and
`--cn-time-picker-field-focus`, or write rules:

```css
.appointment .cn-time-picker-segment { width: 48px; }
.appointment[data-focus-within] { outline-color: #6366f1; }
```

The popup is in the overlay layer, outside the descendants of the trigger. Style
its classes directly. Before you load `CN_STYLES`, define the size and color
variables in `:root`.

`--cn-time-picker-wheel-row-height` (32px) scales all five rows together.
`--cn-time-picker-wheel-font-size` (22px) sets the size of the digits.
`--cn-time-picker-wheel-border` and `--cn-time-picker-wheel-selection-bg` style
the selected row. Layout and typography are CSS rules. The columns use the
`overflow-fade` property of Blinc for their edge fade.
`--cn-time-picker-wheel-fade` defaults to 64px. The default popup is 232px wide
(288px with seconds), with 32px rows and small footer actions.

```css
:root { --cn-time-picker-wheel-row-height: 36px; }
.cn-time-picker-wheel-selection { border-color: var(--primary); }
.cn-time-picker-wheel-period-option[data-selected] { color: var(--primary); }
```

## Colour Picker

```rust
use blinc::cn::prelude::*;
use blinc::core::{Color, reactive::signal};

let color = signal(Color::from_hex(0x6366f1));
let disabled = signal(false);
let favorites = signal(vec![Color::RED, Color::GREEN, Color::BLUE]);

let picker = cn::color_picker(color)
    .alpha(true)
    .swatches(favorites)
    .disabled(disabled)
    .on_change(|color| tracing::info!(hex = %color.to_hex_string(true), "Colour selected"));

// The same editor embedded directly into a settings panel.
let inline = cn::color_picker(color).alpha(true).inline();
```

The default trigger shows a swatch (a small color sample) and a hex value. It
opens a popover with a saturation/brightness field, a hue strip, and a hex
editor. `.inline()` shows the panel directly. `.alpha(true)` adds an opacity
strip. With alpha hidden, an RGB edit or a swatch selection keeps the bound
opacity.

To select a value, drag the selection ring or press anywhere on a field. Arrow
keys adjust saturation/brightness by 1%, hue by 1 degree, or opacity by 1%.
Shift increases the step to 10. Home/End select the endpoints of a strip. In the
two-dimensional field, they select the saturation endpoints. A move through gray
or black keeps the hue and saturation selection.

Tab moves between the coordinate fields, the hex editor, and the presets. Arrow
keys act only on the current field. Thus, cursor movement inside the hex editor
does not change the selected color. Enter/Space opens the focused swatch trigger
or selects a focused preset.

A hex entry stays a draft until you press Enter. The editor accepts `#RGB`,
`#RGBA`, `#RRGGBB`, and `#RRGGBBAA`. The `#` is optional. After invalid input,
the picker keeps the selected color and shows an error border. RGB-only input
keeps the opacity. If `.alpha(true)` is enabled, a form with an alpha value
changes the opacity. An external color write replaces the draft with the new hex
value in its standard form.

`swatches` takes a `Vec<Color>`, a Signal, or a Computed. It supplies presets
that the user can select. Your application is responsible for persistence
(saving the presets). By default, the picker imposes no palette. Disabled
pickers reject pointer, keyboard, and hex edits. If you disable an open picker,
its popover closes. A click outside dismisses the panel.

### Colour styling

The selected color and the coordinate gradients show the value. CSS rules and
theme tokens control focus, disabled appearance, motion, and the widget chrome
(the frame around the content).

| Class | Part |
|---|---|
| `.cn-color-picker-trigger` | Swatch trigger. `data-state="open"`, `:focus-visible`, `:disabled` |
| `.cn-color-picker-panel` | Inline panel or detached popup panel. `disabled` |
| `.cn-color-picker-swatch`, `.cn-color-picker-value` | Preview swatch, and the hex label of the trigger |
| `.cn-color-picker-sv` | Saturation/brightness field |
| `.cn-color-picker-white`, `.cn-color-picker-black` | Gradient layers of the field |
| `.cn-color-picker-hue`, `.cn-color-picker-alpha` | Hue and opacity strips |
| `.cn-color-picker-hue-segment` | Hue spectrum sectors, `data-sector="0"` through `"5"` |
| `.cn-color-picker-marker` | Selection rings |
| `.cn-color-picker-label` | Strip labels |
| `.cn-color-picker-entry`, `.cn-color-picker-hex-wrap` | Hex row and field wrapper |
| `.cn-color-picker-hex`, `.cn-color-picker-hex--invalid` | Hex editor and invalid draft |
| `.cn-color-picker-swatches`, `.cn-color-picker-preset` | Palette and swatches. `data-state="selected"` |

The coordinate fields expose `disabled`, `tabindex="0"`, and
`data-focus-visible`. Presets also expose `disabled` and `tabindex="0"`. During
value updates, the fields and the hex entry keep their node identities.

`.class(...)` and `.id(...)` apply to the trigger. In inline mode, they apply to
the panel. Blinc mounts a popup in the overlay layer. For that reason, style the
popup with its own part classes, not with a descendant selector from the
trigger.

```css
.cn-color-picker-panel {
    --cn-color-picker-border: var(--border-secondary);
    border-radius: 16px;
    width: 280px;
}
.cn-color-picker-sv { height: 180px; }
.cn-color-picker-marker { border-color: white; }
```

The variables are `--cn-color-picker-bg`, `--cn-color-picker-panel-bg`,
`--cn-color-picker-border`, `--cn-color-picker-radius`, and
`--cn-color-picker-marker`. The checker canvas reads the global colors
`--cn-color-picker-checker-light` and `--cn-color-picker-checker-dark`. To
change the interaction transitions, use `--duration-fast` and `--ease-state`.

To run the native example, use
`cargo run -p blinc_app_examples --features cn --example cn_pickers_demo`. The
**Colour Picker** section in `cn_demo` also shows linked inline and popover
editors, a bound disabled switch, and a reactive palette with a save button. Its
**Time Picker** section shows linked 12/24-hour editors and sizes. Its **Date
Picker** section shows calendar popovers with default and custom formats. For
the inline calendar, see [Form Components](form.md#calendar).
