# Date, Time and Colour Pickers

The pickers read and write a supplied `Signal`. External writes update the
existing fields; `on_change` runs only for changed user edits. Install
`CN_STYLES` through `cn_bundle()` or your theme bundle before using them.

## Date Picker

`cn::calendar` is the inline date-selection component. `cn::date_picker` wraps
that calendar in a popover and shows its selected date in a compact trigger.
It uses the same `CalendarDay` and `Signal<Option<CalendarDay>>` model.

Calendar selection uses `--cn-calendar-selected-bg` (default `--primary`) and
`--cn-calendar-selected-color` (default `--text-inverse`). Event dots and icons
use the selected foreground colour while their day is selected, then return to
application colours. These colours follow the
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

The default format is `YYYY-MM-DD`. Picking a day writes `Some(day)` and closes
the popup. The clear action writes `None`; `.clearable(false)` omits it, and
`.clear_text(...)` accepts a string, Signal or Computed. External writes update
the trigger and an open calendar without invoking `on_change`. Choosing the
same date closes the popup without emitting another change. Disabling an open
picker closes it and rejects edits.

Tab reaches the trigger; Enter/Space opens it. The calendar retains its own
month controls and keyboard navigation. Clicking outside dismisses the popup.
`size(InputSize)`, `week_start`, `min_year`, `max_year`, `today` and `events`
configure its presentation through the existing calendar API.

| Class | Part |
|---|---|
| `.cn-date-picker-trigger` | Trigger; `data-size="sm|md|lg"`, `data-empty`, `data-state="open"`, `disabled`, `:focus-visible` |
| `.cn-date-picker-icon`, `.cn-date-picker-value` | Calendar icon and formatted date/placeholder |
| `.cn-date-picker-content` | Detached popup; also has `.cn-popover-content` |
| `.cn-date-picker-footer` | Clear-action row |
| `.cn-date-picker-clear` | Clear button; `disabled`, `:hover`, `:focus-visible` |

The calendar keeps its documented `.cn-calendar-*` part classes. Override
`--cn-date-picker-bg`, `--cn-date-picker-border` and `--cn-date-picker-radius`
or target the part classes. `.class(...)` and `.id(...)` apply to the trigger;
the popup lives in the overlay layer, outside the trigger's descendants.

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

`TimeOfDay::new(hour, minute, second)` validates a 24-hour time and returns
`None` for out-of-range parts. The value has `hour()`, `minute()` and
`second()` accessors, implements `Display` as `HH:MM:SS`, and carries no
date or time zone. The default picker shows hours and minutes in 24-hour form.
Hiding seconds preserves their stored value. Midnight is `12 AM`; noon is
`12 PM`. Clicking the period toggles AM/PM. Tab moves between the fields and
the period; Shift+Tab moves back. Enter/Space activates the focused period.

The clock button opens a compact wheel popup over the control, kept within
the window edges, with five visible rows for each numeric
field. The middle row marks the draft selection and neighbouring values fade
out toward the edges. Scroll a column with the mouse wheel or trackpad, drag
it vertically, or click a nearby value. In 12-hour mode, separate **AM** and
**PM** buttons sit above the wheels. They preserve the selected hour while
switching periods. Seconds add a third numeric column when enabled.

**Save** writes the draft to the bound signal and calls `on_change` once if
it changed. **Cancel**, Escape, clicking outside, or disabling the picker
discards the draft. External writes update the open draft without calling
`on_change`. The compact fields continue to commit edits immediately.

Tab reaches each column and the footer actions; Shift+Tab goes back.
Up/Down step the focused column, Page Up/Down step five times, and Home/End
select its first/last value. `minute_step` and `second_step` apply to keyboard
stepping and the compact fields’ wheel input. Popup columns are native scroll
containers with every valid value and stop at their ends; keyboard stepping
wraps. Other fields and hidden seconds are preserved. Leaving a column
centres its nearest value.
Use `.popup(false)` for the compact editor without a clock button.

Type up to two digits per field, or use Up/Down and the mouse wheel to step.
Stepping wraps within that field without carrying into neighbouring fields.
`minute_step` and `second_step` affect stepping; typing accepts any valid
value. Enter restores the field's two-digit presentation. Invalid or partial
text leaves the selected time unchanged.

### Time styling

`.class(...)` and `.id(...)` apply to the outer `.cn-time-picker`, which
exposes `data-size="sm|md|lg"`, `data-format="12|24"` and `disabled`.
The control heights are 32, 36 and 40px. Numeric fields share the outer
focus ring (`data-focus-within`); their local highlight identifies the part
being edited. Selection highlighting clears when a field loses focus.

| Class | Part |
|---|---|
| `.cn-time-picker-part` | Field wrapper |
| `.cn-time-picker-segment` | Editable field; `:hover`, `:focus`, `:disabled` |
| `.cn-time-picker-segment--sm`, `--md`, `--lg` | Field size classes |
| `.cn-time-picker-hour`, `.cn-time-picker-minute`, `.cn-time-picker-second` | Individual fields |
| `.cn-time-picker-separator` | Colon |
| `.cn-time-picker-period` | AM/PM button; `:focus-visible`, `:disabled` |
| `.cn-time-picker-clock` | Wheel popup trigger; `data-state="open"`, `:focus-visible`, `:disabled` |
| `.cn-time-picker-wheel` | Detached popup; `data-format="12|24"`, `data-seconds` |
| `.cn-time-picker-wheel-header`, `.cn-time-picker-wheel-title` | Heading |
| `.cn-time-picker-wheels` | Five-row numeric viewport |
| `.cn-time-picker-wheel-selection` | Shared selected-row border and fill |
| `.cn-time-picker-wheel-column` | Native scroll, drag and keyboard target; `:focus-visible`, `overflow-fade`, `scrollbar-width: none` |
| `.cn-time-picker-wheel-hour`, `.cn-time-picker-wheel-minute`, `.cn-time-picker-wheel-second` | Individual columns |
| `.cn-time-picker-wheel-list`, `.cn-time-picker-wheel-option` | Scrollable number list and static value rows |
| `.cn-time-picker-wheel-separator` | Colon between columns |
| `.cn-time-picker-wheel-period`, `.cn-time-picker-wheel-period-option` | Separate period control; options expose `data-selected`, `:focus-visible` |
| `.cn-time-picker-wheel-am`, `.cn-time-picker-wheel-pm` | Individual period choices |
| `.cn-time-picker-wheel-footer`, `.cn-time-picker-wheel-cancel`, `.cn-time-picker-wheel-save` | Footer and actions |

Override `--cn-time-picker-bg`, `--cn-time-picker-border`,
`--cn-time-picker-radius`, `--cn-time-picker-field-hover` and
`--cn-time-picker-field-focus`, or write rules:

```css
.appointment .cn-time-picker-segment { width: 48px; }
.appointment[data-focus-within] { outline-color: #6366f1; }
```

The popup lives in the overlay layer, outside the trigger's descendants.
Style its classes directly. Define size/colour variables in `:root` before
loading `CN_STYLES`.
`--cn-time-picker-wheel-row-height` (32px) scales
all five rows together, and `--cn-time-picker-wheel-font-size` (22px) sets
the digits. `--cn-time-picker-wheel-border` and
`--cn-time-picker-wheel-selection-bg` style the selected row. Layout and
typography are CSS rules. Columns use Blinc’s `overflow-fade` for their edge
fade; `--cn-time-picker-wheel-fade` defaults to 64px. The default popup is 232px wide (288px with seconds), with 32px
rows and small footer actions.

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

The default trigger shows a swatch and hex value. It opens a popover with a
saturation/brightness field, hue strip and hex editor. `.inline()` shows the
panel directly. `.alpha(true)` adds an opacity strip. With alpha hidden,
editing RGB or selecting a swatch preserves the bound opacity.

Drag the selection ring or press anywhere on a field to select a value.
Arrow keys adjust saturation/brightness by 1%, hue by 1 degree, or opacity
by 1%; Shift increases the step to 10. Home/End select the endpoints of a
strip, or the saturation endpoints in the two-dimensional field.
The hue and saturation selection survives moving through grey or black.
Tab moves between the coordinate fields, hex editor and presets. Arrow keys
act only on the current field; navigating inside the hex editor leaves the
selected colour unchanged. Enter/Space opens the focused swatch trigger or
selects a focused preset.

Hex entry is a draft until **Enter**. It accepts `#RGB`, `#RGBA`, `#RRGGBB`
and `#RRGGBBAA`, with an optional `#`. Invalid input keeps the selected colour
and shows an error border. RGB-only input preserves opacity; an alpha-bearing
form changes opacity when `.alpha(true)` is enabled. An external colour write
replaces the draft with the new canonical hex value.

`swatches` takes a `Vec<Color>`, Signal or Computed. It supplies selectable
presets; the application owns persistence. No palette is imposed by default.
Disabled pickers reject pointer, keyboard and hex edits. Disabling an open
picker closes its popover. Clicking outside dismisses the panel.

### Colour styling

The selected colour and coordinate gradients visualize the value. Widget
chrome, focus, disabled appearance and motion follow CSS rules and theme tokens.

| Class | Part |
|---|---|
| `.cn-color-picker-trigger` | Swatch trigger; `data-state="open"`, `:focus-visible`, `:disabled` |
| `.cn-color-picker-panel` | Inline or detached popup panel; `disabled` |
| `.cn-color-picker-swatch`, `.cn-color-picker-value` | Preview swatch and trigger hex label |
| `.cn-color-picker-sv` | Saturation/brightness field |
| `.cn-color-picker-white`, `.cn-color-picker-black` | Field gradient layers |
| `.cn-color-picker-hue`, `.cn-color-picker-alpha` | Hue and opacity strips |
| `.cn-color-picker-hue-segment` | Hue spectrum sectors, `data-sector="0"` through `"5"` |
| `.cn-color-picker-marker` | Selection rings |
| `.cn-color-picker-label` | Strip labels |
| `.cn-color-picker-entry`, `.cn-color-picker-hex-wrap` | Hex row and field wrapper |
| `.cn-color-picker-hex`, `.cn-color-picker-hex--invalid` | Hex editor and invalid draft |
| `.cn-color-picker-swatches`, `.cn-color-picker-preset` | Palette and swatches; `data-state="selected"` |

The coordinate fields expose `disabled`, `tabindex="0"` and
`data-focus-visible`. Presets expose `disabled` and `tabindex="0"` too.
The fields and hex entry retain their node identities during value updates.

`.class(...)` and `.id(...)` apply to the trigger, or to the panel in inline
mode. A popup is mounted in the overlay layer, so style it using its own part
classes rather than a descendant selector from the trigger.

```css
.cn-color-picker-panel {
    --cn-color-picker-border: var(--border-secondary);
    border-radius: 16px;
    width: 280px;
}
.cn-color-picker-sv { height: 180px; }
.cn-color-picker-marker { border-color: white; }
```

Variables are `--cn-color-picker-bg`, `--cn-color-picker-panel-bg`,
`--cn-color-picker-border`, `--cn-color-picker-radius` and
`--cn-color-picker-marker`. The checker canvas reads the global
`--cn-color-picker-checker-light` and `--cn-color-picker-checker-dark` colours.
Use `--duration-fast` and `--ease-state` to change interaction transitions.

Run the native example with
`cargo run -p blinc_app_examples --features cn --example cn_pickers_demo`.
The **Colour Picker** section in `cn_demo` also shows linked inline and popover
editors, a bound disabled switch and a reactive palette with a save button.
Its **Time Picker** section shows linked 12/24-hour editors and sizes, and its
**Date Picker** section shows default and custom-formatted calendar popovers.
The inline calendar is documented in [Form Components](form.md#calendar).
