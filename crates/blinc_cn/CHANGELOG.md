# Changelog

All notable changes to `blinc_cn` will be documented in this file.

## [Unreleased]

### Fixed
- Turning a `cn::switch` off no longer snaps: the thumb keeps its place while the switch is held and eases back on release, and the track fades from its on colour to its off colour instead of jumping. Hovering an off track lightens it (a wash of the text colour over the idle fill), where it used to darken.
- The combobox's option rows clip to the popup, so a long list no longer paints below it or over the search field while scrolled.
- Clicking the trigger of an open `cn::dropdown_menu` or `cn::popover` closes it. The press counted as a click outside, which closed the menu, and the click that followed reopened it; the trigger is now inside the click-outside set.

### Added
- `cn::dropdown_menu` is keyboard-driven, as ashui's: Down and Up move through the enabled rows (wrapping, skipping separators and disabled rows), Home and End jump to the first and last, Enter and Space activate the active row. The active row is the `.cn-dropdown-item--active` class and takes the hover fill. An open submenu receives the keys instead.
- The carets of `cn::select`, `cn::combobox`, the default `cn::dropdown_menu` trigger and `cn::accordion` turn half a turn when opened, eased by a `transition` on `.cn-chevron` keyed on the trigger's `data-state="open"`, as ashui's chevrons do.
- `cn::number_input` steps with the wheel, as ashui's does: one wheel line (40 units) is one step, wheel up increasing; trackpad deltas accumulate to a whole step; reversing direction or leaving the control clears the remainder; horizontal gestures and a disabled control are ignored.

### Changed
- `cn::input`, `cn::textarea`, the OTP slots and `cn::number_input` carry `box-shadow: var(--shadow-sm)`, as the select, combobox and outline button already did, and none while disabled; a disabled number input also takes the disabled fill.
- `cn::number_input` follows ashui's number input: one bordered group (`.cn-number-input-group`) with a borderless field and two steps whose fill, divider and icon colour are `CN_STYLES` rules. A step tints on hover and press, and is `disabled` (dimmed, inert) where a step would change nothing. The pointer-signal colour code is gone.
- Shadows, motion and shape come from theme tokens in the rules: the select, combobox and dropdown triggers take `box-shadow: var(--shadow-sm)` (and `none` while disabled) from `CN_STYLES` instead of `.shadow_sm()` in Rust; the input, textarea and OTP focus-ring transitions use `--duration-fast` and `--ease-state` instead of literal times. `.cn-slider-thumb` now has `var(--shadow-sm)` (none while disabled), as ashui's slider thumb does; it was flat.
- `cn::combobox`: the trigger's border on hover, while open and while disabled is `.cn-combobox-trigger` rules (`data-state="open"`, `disabled`) with a `transition`, instead of a colour computed from pointer signals in Rust; a disabled combobox now shows the disabled palette. The search field takes its fill and border from `.cn-combobox-search` rules; the `.hover_*` and `.focused_*` colour setters it called are gone.
- `cn::select` and the default `cn::dropdown_menu` trigger: fill and border on hover, press, open and disabled are `CN_STYLES` rules (`.cn-select-trigger` with `data-state="open"` and `disabled` attributes, `.cn-dropdown-trigger`) with a `transition`, instead of colours computed from pointer signals in Rust.
- `cn::textarea`: border and fill per state are `CN_STYLES` rules (`.cn-textarea`, `.cn-textarea--error`) on the layout rules, eased by a `transition`; `.error(..)` now adds `cn-textarea--error`, so the border is red before anything is typed.
- `cn::input`: border and fill while hovered, focused, in error and disabled are `CN_STYLES` rules (`.cn-input`, `.cn-input--error`) on top of the layout input's `.blinc-input` rules, eased by a `transition`; the Rust colour code is gone. `hover_border_color`, `focused_border_color`, `error_border_color`, `border_colors`, `hover_bg_color`, `focused_bg_color`, `bg_colors`, `InputBorderColors` and `InputBgColors` are removed; use rules. `idle_border_color` and `idle_bg_color` set inline style. `.error(..)` adds `cn-input--error`, so the border is red before anything is typed. `cn::input_otp` slots and the combobox search field take their look from rules the same way.
- `cn::badge`: a bound label inherits its `color` from the `.cn-badge--{style}-{variant}` rule, like a static one, instead of setting it in Rust from a table that mirrored the stylesheet. `cn::radio_group` takes its look from the layout radio's `.blinc-radio` rules; `CN_STYLES` keeps only its custom-property hooks.
- `cn::toggle` and `cn::toggle_group` items are one element each with `data-state`, `data-variant`, `data-size` and `disabled` attributes; their look, the variant and size ladder, and hover, press and on states are the `.blinc-toggle` rules of the layout stylesheet (ported from ashui's), with a `transition`. The Rust colour code, size tables passed per item and the copied `mix` helper are gone. A group sets `data-variant` and `data-size` for its items, and its default gap is 4px, not 16px.
- `cn::radio_group`: each ring is an element with `checked` and `disabled` attributes; border colour, the dot's fade-in, the disabled fill and hover are `CN_STYLES` rules, and the stylesheet-by-id override path is gone. `selected_color`, `border_color` and `hover_border_color` are removed; restyle with rules such as `.my-radio:checked { border-color: #16a34a }`. An unselected option now highlights on hover.
- `cn::checkbox` is an element with `checked` and `disabled` attributes: fill, border colour, hover, the disabled palette and the mark's fade-in are `CN_STYLES` rules with a `transition`. `checked_color`, `unchecked_bg`, `border_color`, `hover_border_color` and `check_color` are removed; restyle with rules such as `.my-checkbox:checked { background: #16a34a }`. A disabled checked box now shows its mark. It builds on `blinc_layout::widgets::checkbox`, so `.class(..)` and `.id(..)` apply to the box. The check mark is bolder and larger (11, 14 and 18px for the three sizes), after ashui's.
- `cn::switch` is an element with a `checked` attribute: its track fill (off, on, hover, disabled), the thumb's slide and its press shrink are `CN_STYLES` rules with a `transition`, per size (`.cn-switch--sm|md|lg`). The spring and the Rust colour setters (`on_color`, `off_color`, `thumb_color`, `spring`) are removed; restyle with rules, such as `.my-switch:checked { background: #16a34a }`. The thumb stays inside the track while it slides.
- `cn::button`: each variant's fill, text colour, border and shadow, and how they change on hover, press and while disabled, are `CN_STYLES` rules with a CSS `transition`, instead of colours computed in Rust. Rules for a hovered or pressed button leave a disabled one alone (`:not(:disabled)`). A pressed button that is still hovered keeps its hover colour, as on the web.

### Added
- **`cn::input_otp`** — themed segmented one-time-code input backed by linked single-character `text_input` slots. Supports auto-advance, empty-slot Backspace rewind, paste distribution across remaining slots, optional numeric-only filtering, masking, disabled state, error styling, and grouped separators. The shared slot behavior lives in `blinc_layout::widgets::input_otp`; cn contributes the themed border/background/focus surface.
- **`cn::table`** — themed wrapper around `blinc_layout::widgets::table` exposing the shadcn surface: `cn::table()` / `cn::table_header()` / `cn::table_body()` / `cn::table_footer()` / `cn::table_row()` / `cn::table_head(label)` / `cn::table_cell()` / `cn::table_caption(label)`. The layout widget already paints section backgrounds + cell padding + flex-1 cell distribution from theme tokens; cn only adds the surface CSS (1 px outer border at `--border`, `--radius-md`, `:hover` row tint at `--surface-hover`, `:last-child` border-strip so the bottom row sits flush with the rounded edge), the medium-weight muted-foreground header text, and the footer's top-border + medium font-weight. `cn::table_row().selected(true)` toggles the `.cn-table-row--selected` class which paints `--selection`. No sort / filter — that's the eventual `cn::data_grid`.
- **`cn::number_input`** — themed wrapper around the underlying `text_input` with a `+` / `−` SVG-icon stepper pair flanking the field. Reuses `InputSize` from `cn::input` for height / font alignment so number inputs sit flush with other form inputs of the matching size. Behaviour:
  - **Bound to a `State<f64>`** — stepper buttons clamp against `min` / `max` and call `state.set`. An outer `Stateful` watches `state.signal_id()` and re-renders the field on every value change so steppers / external code / keyboard stepping all push their updates through the visible field automatically (no `refresh_text_input` boilerplate at the caller).
  - **Centred value + tight cell** — `text_input::text_align(Center)` + `padding_x(4)` for a fixed-width cell that hugs the centred digit count. Default cell is 64 px; callers can override via `.w(px)` for a roomy six-digit field or a single-digit one.
  - **Keyboard stepping** — ↑ / ↓ / `+` / `−` step the value (via `text_input::on_step`).
  - **Click-to-select-all** — single-click on the centred field selects the whole value so the next keystroke replaces it (canonical `<input type=number>` UX).
  - **No mid-edit reformat** — while the field is focused, the outer Stateful skips the "format state into visible text" sync, so typing `30.0` doesn't get auto-canonicalised to `3.00.0` mid-stroke. Reformatting happens on blur or via stepper.
- **`cn::toggle_group`** — single-select toggle bar (shadcn's `<ToggleGroup type="single">`). Takes a `State<String>` for the active value; items are `cn::toggle_item(value).label(…).icon(…).disabled(…)`. Each item is a `Stateful<ButtonState>` watching the group's signal id, so clicks anywhere in the row re-render all items in lockstep. Inherits the same theme-token defaults + variant / size ladder as `cn::toggle` (the visual rules are re-resolved per item inside the callback). Multi-select (`type="multiple"`) is a follow-up that wants its own `State<Vec<String>>` overload.
- **`cn::toggle`** — shadcn-style binary toggle button. Themed wrapper around `blinc_layout::widgets::toggle`, contributing `.cn-toggle` + `.cn-toggle--default` / `.cn-toggle--outline` variant classes and `.cn-toggle--sm` / `--md` / `--lg` size classes. All parse / state / token-default work lives in the layout widget — cn only supplies the surface CSS and the variant / size selection. `ToggleVariant::Default` (no border off) is the toolbar-friendly default; `ToggleVariant::Outline` borders the off state (pairs with future `cn::toggle_group`). `ToggleSize::{Small, Medium, Large}` heights line up with `cn::button` and `cn::input` so toggle rows sit flush.
- **HID focus ring on `.cn-input` / `.cn-textarea`**. On focus the border brightens to `--border-focus` and a 2 px outer ring at 2 px offset draws around the input edge using a 35 %-alpha tint (`--focus-ring`) so the ring reads as a soft halo distinct from the crisp border. Error and success variants get their own `--focus-ring-error` / `--focus-ring-success` colours. Outline scales from a transparent 1 px-offset baseline to the focus 2 px offset with a 160 ms ease for the first focus interaction (see Known Issues).
- **Semantic easings wired through dialog / sheet / drawer / toast and the CSS surface**. Enter / exit animations now use `--ease-default` (or the variant-specific role) instead of the previous fixed `EaseInOut`. The cn stylesheet picks up the same vars so user-written CSS transitions match the framework's built-in motion.
- **`ButtonSize::Custom(width, height)`**. Escape hatch for cases where the Small / Medium / Large / Icon ladder doesn't fit — wide auth-flow CTAs, tight inline actions, settings rows aligned to a specific column.

### Changed
- `cn::number_input` now derives its field width from configured bounds and precision rather than the current value, so typing more digits does not resize the control. Keyboard step changes force one focused-field sync so the visible value tracks ↑ / ↓ / `+` / `−` immediately without reformatting ordinary typed input mid-edit.
- Dropdown menu / context menu / select / combobox / menubar / nav-menu hover highlights now clip to the panel's outer radius, so the highlight follows the panel's rounded edge instead of leaving a visible strip of the panel's surface bg at the corners.
- Combobox search input bumps `radius_sm` → `radius_md` to match the dropdown's corner reach, plus added horizontal padding so the input doesn't butt against the panel edge.
- Breadcrumb item labels + separators (slash / text) now `.no_wrap()` so long path segments don't fold across two lines mid-trail.
- `cn::label` collapses to `w_fit()` and the inner text is `.no_wrap()` so a long label doesn't take the full row width.
- `.cn-input` / `.cn-textarea` no longer redeclare `border:` at the base level — the layout `TextInput` setters supply idle / hover / focused border colours; the redundant base rule was being rewritten by `apply_complex_selector_styles` every frame and clobbering the setter-chosen focused colour.
- `apply_css_overrides` on text input / text area now applies `:focus` AFTER `:hover` for the `FocusedHovered` state so focus colour wins while the user is typing in a hovered input. Same ordering applied to the outline-extraction path.
- All component `.class()` builders take `impl AsRef<str>` (was `impl Into<String>`), and per-component `classes` / `css_classes` storage is now `Vec<Arc<str>>` interned through `blinc_core::intern`. A class repeated across hundreds of nodes now allocates exactly once.
- `element_classes()` overrides return `&[Arc<str>]` to match the trait change in `blinc_layout`.

### Fixed
- Popover / context-menu / dropdown / select / combobox / menubar / dialog / sheet / drawer / toast animations now play on the first interaction (class-only `@keyframes` rules were previously skipped on the very first build).
- Dropdown menu's top/bottom Rust-side `.py(1.0)` padding removed (was double-padding with the CSS `padding:` declaration).

### Known Issues
- `.cn-input` / `.cn-textarea` focus ring transition plays smoothly on the first focus but snaps in on every subsequent focus. Border-color transition is unaffected; only the outline transition is affected. See `gotcha_focus_ring_transition_no_replay.md` in dev memory. Deferred.

## [0.4.0] - 2026-04-05

### Changed
- Version bump to align with workspace 0.4.0 release

## [0.1.15] - 2026-03-22

### Fixed

- Removed CSS transition declarations from nav-link, sidebar-item, menubar-trigger, and menubar-item that caused hover-leave visual artifacts
- Sidebar item background set to transparent to prevent stale background on hover-leave
- Clippy warnings in menubar overlay functions (let-binding return)
- Toast slide distance adjusted to 200px for clear right-edge entry animation
- Toast enter/exit animations now use proper off-screen distance for all corner positions
