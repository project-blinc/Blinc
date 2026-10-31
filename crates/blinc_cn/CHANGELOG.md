# Changelog

All notable changes to `blinc_cn` will be documented in this file.

## [Unreleased]

### Changed
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
