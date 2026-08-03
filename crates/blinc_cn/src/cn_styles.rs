//! Default CSS stylesheet for blinc_cn components.
//!
//! All visual properties use `var()` to reference theme tokens,
//! making everything overridable via CSS. Component-level variables
//! (e.g., `--cn-button-primary-bg`) provide targeted override points
//! that fall back to theme tokens.
//!
//! # Usage
//!
//! ```ignore
//! // Register default styles before user CSS
//! blinc_cn::register_cn_styles(ctx);
//!
//! // User CSS can then override:
//! ctx.add_css(r#"
//!     .cn-button--primary { background: #ff6600; }
//!     .cn-card { border-radius: 0; }
//! "#);
//! ```

/// Default CSS for all blinc_cn components.
///
/// Uses `var(--theme-token)` for all color references.
/// Each component defines `var(--cn-component-prop, var(--fallback))` for overridability.
pub const CN_STYLES: &str = r#"
/* ============================================================================
   Typography base — every cn widget inherits the theme's canonical
   sans, body line-height, and dense-HID letter-spacing. CN_STYLES
   was previously silent on these, leaving widgets to fall back to
   the platform default font and ignore the theme's deliberate
   "Noto Sans + 13px text_sm + tracking_tight" choice.
   ============================================================================ */

.cn-button, .cn-card, .cn-card-header, .cn-card-footer,
.cn-badge, .cn-alert, .cn-alert-box,
.cn-input, .cn-textarea, .cn-label,
.cn-checkbox, .cn-switch, .cn-radio, .cn-toggle,
.cn-tabs-list, .cn-tabs-trigger,
.cn-select-trigger, .cn-select-content, .cn-select-item,
.cn-combobox-trigger, .cn-combobox-content, .cn-combobox-item,
.cn-slider-track, .cn-progress,
.cn-avatar, .cn-tooltip,
.cn-dialog, .cn-drawer, .cn-sheet, .cn-toast,
.cn-accordion, .cn-accordion-trigger, .cn-accordion-content,
.cn-breadcrumb, .cn-breadcrumb-item,
.cn-pagination, .cn-pagination-btn,
.cn-nav-menu, .cn-nav-link,
.cn-sidebar, .cn-sidebar-item,
.cn-dropdown-menu, .cn-dropdown-item,
.cn-context-menu, .cn-context-menu-item,
.cn-menubar, .cn-menubar-trigger, .cn-menubar-content, .cn-menubar-item,
.cn-popover-content, .cn-hover-card-content,
.cn-tree-node, .cn-skeleton {
    font-family: var(--font-sans);
}

.cn-kbd {
    font-family: var(--font-mono);
}

/* Body-text widgets get the theme's line-height so multi-line
   content (alert descriptions, accordion content, tooltips) breathes
   correctly. */
.cn-alert, .cn-alert-box, .cn-accordion-content, .cn-tooltip {
    line-height: var(--leading-normal);
}

/* Dense-HID tracking on interactive labels. Picks up the variant's
   tracking_tight value (Universal HID = -0.025em). */
.cn-button, .cn-tabs-trigger,
.cn-input, .cn-textarea,
.cn-select-trigger, .cn-combobox-trigger {
    letter-spacing: var(--tracking-tight);
}

/* ============================================================================
   Button
   ============================================================================ */

/* Button: visual states (hover, active, disabled) handled by Stateful FSM.
   Geometry tokens flow from the active theme's RadiusTokens so each
   variant (Restrained / Hybrid / Expressive) gets its own corner reach.
   The Rust side already reads `RadiusToken` per size — these rules act
   as the cascade override surface users can hook into. */
.cn-button { border-radius: var(--radius-default); }
.cn-button--primary { }
.cn-button--secondary { }
.cn-button--destructive { }
.cn-button--outline { }
.cn-button--ghost { }
.cn-button--link { }
.cn-button--disabled { }
.cn-button--sm { border-radius: var(--radius-sm); }
.cn-button--md { border-radius: var(--radius-default); }
.cn-button--lg { border-radius: var(--radius-lg); }
.cn-button--icon { border-radius: var(--radius-default); }

/* ============================================================================
   Card
   ============================================================================ */

.cn-card {
    background: var(--cn-card-bg, var(--surface));
    border: 1px solid var(--cn-card-border, var(--border));
    border-radius: var(--cn-card-radius, var(--radius-xl));
    padding: var(--space-6);
    gap: var(--space-4);
}

.cn-card-header {
    gap: var(--space-1-5);
}

.cn-card-footer {
    gap: var(--space-2);
}

/* ============================================================================
   Badge
   ============================================================================ */

.cn-badge {
    border-radius: var(--radius-full);
    font-size: var(--text-xs);
    /* `2 10` doesn't have an exact spacing token (space-0-5 = 2,
       space-2-5 = 10) — use the closest matching pair. */
    padding: var(--space-0-5) var(--space-2-5);
}

/* Soft (default) — pale tinted bg + 1px same-hue border + same-hue
   text. The thin colored border gives the pill more definition
   against neutral surfaces; without it the soft fill alone reads
   as a weak smudge on light themes. Mirrors the Alert component's
   family so a `Success` badge sits next to a `Success` alert and
   reads as part of the same status system. */
.cn-badge--soft-default {
    background: var(--accent-subtle);
    border: 1px solid var(--primary);
    color: var(--primary);
}
.cn-badge--soft-secondary {
    background: var(--surface-elevated);
    border: 1px solid var(--border);
    color: var(--text-secondary);
}
.cn-badge--soft-success {
    background: var(--success-bg);
    border: 1px solid var(--success);
    color: var(--success);
}
.cn-badge--soft-warning {
    background: var(--warning-bg);
    border: 1px solid var(--warning);
    color: var(--warning);
}
.cn-badge--soft-destructive {
    background: var(--error-bg);
    border: 1px solid var(--error);
    color: var(--error);
}

/* Solid — legacy filled style. Useful when the badge sits on a
   page-tinted surface and a soft variant would disappear. */
.cn-badge--solid-default {
    background: var(--primary);
    color: var(--text-inverse);
}
.cn-badge--solid-secondary {
    background: var(--secondary);
    color: var(--text-inverse);
}
.cn-badge--solid-success {
    background: var(--success);
    color: var(--text-inverse);
}
.cn-badge--solid-warning {
    background: var(--warning);
    color: var(--text-inverse);
}
.cn-badge--solid-destructive {
    background: var(--error);
    color: var(--text-inverse);
}

/* Outline — transparent + variant-coloured border + variant-
   coloured text. Quietest of the three when used in a dense set. */
.cn-badge--outline-default {
    background: transparent;
    border: 1px solid var(--primary);
    color: var(--primary);
}
.cn-badge--outline-secondary {
    background: transparent;
    border: 1px solid var(--border);
    color: var(--text-primary);
}
.cn-badge--outline-success {
    background: transparent;
    border: 1px solid var(--success);
    color: var(--success);
}
.cn-badge--outline-warning {
    background: transparent;
    border: 1px solid var(--warning);
    color: var(--warning);
}
.cn-badge--outline-destructive {
    background: transparent;
    border: 1px solid var(--error);
    color: var(--error);
}

/* Icon tint — target the inner SVG shape elements (`path`,
   `circle`, …) directly instead of the `<svg>` wrapper. The
   `blinc_icons` / `blinc_tabler_icons` generators emit paths with
   only the `d` attribute and rely on `stroke="currentColor"` on
   the outer `<svg>` for the tint. Blinc's CSS engine doesn't
   propagate `stroke` from `<svg>` down to child paths, so a rule
   on `svg { stroke: … }` has no effect; the path needs its own
   matching selector. Listing each SVG shape tag covers both
   line-style icons (Lucide / Tabler outline — stroke-driven) and
   filled icons (Tabler filled — fill-driven). Setting both stroke
   and fill is safe because each shape carries `fill="none"` in
   the outline sets, which the per-shape CSS rule overrides only
   for shapes that actually fill.
   Inline `.color(...)` on the icon element wins via specificity
   for one-off overrides. */
.cn-badge--soft-default :is(path, circle, rect, ellipse, line, polygon, polyline, g),
.cn-badge--outline-default :is(path, circle, rect, ellipse, line, polygon, polyline, g) {
    stroke: var(--primary);
}
.cn-badge--soft-secondary :is(path, circle, rect, ellipse, line, polygon, polyline, g),
.cn-badge--outline-secondary :is(path, circle, rect, ellipse, line, polygon, polyline, g) {
    stroke: var(--text-secondary);
}
.cn-badge--soft-success :is(path, circle, rect, ellipse, line, polygon, polyline, g),
.cn-badge--outline-success :is(path, circle, rect, ellipse, line, polygon, polyline, g) {
    stroke: var(--success);
}
.cn-badge--soft-warning :is(path, circle, rect, ellipse, line, polygon, polyline, g),
.cn-badge--outline-warning :is(path, circle, rect, ellipse, line, polygon, polyline, g) {
    stroke: var(--warning);
}
.cn-badge--soft-destructive :is(path, circle, rect, ellipse, line, polygon, polyline, g),
.cn-badge--outline-destructive :is(path, circle, rect, ellipse, line, polygon, polyline, g) {
    stroke: var(--error);
}
.cn-badge--solid-default :is(path, circle, rect, ellipse, line, polygon, polyline, g),
.cn-badge--solid-secondary :is(path, circle, rect, ellipse, line, polygon, polyline, g),
.cn-badge--solid-success :is(path, circle, rect, ellipse, line, polygon, polyline, g),
.cn-badge--solid-warning :is(path, circle, rect, ellipse, line, polygon, polyline, g),
.cn-badge--solid-destructive :is(path, circle, rect, ellipse, line, polygon, polyline, g) {
    stroke: var(--text-inverse);
}

/* ============================================================================
   Alert
   ============================================================================ */

.cn-alert {
    background: var(--cn-alert-bg, var(--surface));
    border: 1px solid var(--cn-alert-border, var(--border));
    border-radius: var(--radius-default);
    color: var(--text-primary);
    font-size: var(--text-sm);
    padding: var(--space-4);
}
.cn-alert-box {
    background: var(--cn-alert-bg, var(--surface));
    border: 1px solid var(--cn-alert-border, var(--border));
    border-radius: var(--radius-default);
    color: var(--text-primary);
    font-size: var(--text-sm);
    padding: var(--space-4);
    gap: var(--space-3);
}
.cn-alert--success {
    background: var(--success-bg);
    border-color: var(--success);
    color: var(--success);
}
.cn-alert--warning {
    background: var(--warning-bg);
    border-color: var(--warning);
    color: var(--warning);
}
.cn-alert--error {
    background: var(--error-bg);
    border-color: var(--error);
    color: var(--error);
}
.cn-alert--info {
    background: var(--info-bg);
    border-color: var(--info);
    color: var(--info);
}

/* ============================================================================
   Separator
   ============================================================================ */

.cn-separator {
    background: var(--cn-separator-color, var(--border));
}

/* ============================================================================
   Skeleton
   ============================================================================ */

.cn-skeleton {
    background: var(--cn-skeleton-bg, var(--surface-elevated));
    border-radius: var(--radius-sm);
}

/* ============================================================================
   Input
   ============================================================================ */

.cn-input {
    /* No `border:` here — the layout TextInput already sets idle /
       hover / focused border colours via setters in cn::input
       (mirroring combobox's search input, which works). Adding a base
       `border:` rule here parses a `border-color` that
       apply_complex_selector_styles writes into render_props every
       frame, clobbering the focused colour the setter chose. The
       `:focus` rule below still applies the focus colour because
       state-rules are applied after base rules in the same pass. */
    background: var(--cn-input-bg, var(--input-bg));
    border-radius: var(--radius-default);
    color: var(--text-primary);
    /* Idle ring: same width, transparent, gap of 1 px so the focus
       transition "scales" the gap from 1 → 2 px while the colour fades
       in. Without a base outline the transition has no starting point
       and the ring pops in. */
    outline: 2px solid transparent;
    outline-offset: 1px;
    transition: outline-color 160ms ease, outline-offset 160ms ease;
}
.cn-input:hover {
    border-color: var(--border-hover);
    background: var(--input-bg-hover);
}
.cn-input:focus {
    /* HID focus affordance: brighten the border AND draw a 2px outer
       ring offset 2px out from the input edge. The outline uses a
       fainter `--focus-ring` (alpha-tinted variant token) so the
       crisp border edge stays distinguishable from the soft halo. */
    border-color: var(--border-focus);
    background: var(--input-bg-focus);
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
}
.cn-input--error {
    border-color: var(--border-error);
}
.cn-input--error:focus {
    border-color: var(--border-error);
    outline: 2px solid var(--focus-ring-error);
}
.cn-input--success {
    border-color: var(--success);
}
.cn-input--success:focus {
    border-color: var(--success);
    outline: 2px solid var(--focus-ring-success);
}

.cn-input--sm { font-size: var(--text-xs); }
.cn-input--md { font-size: var(--text-sm); }
.cn-input--lg { font-size: var(--text-lg); }

/* Input OTP. Border color is owned by Rust-side state setters. */

.cn-input-otp-slot {
    background: var(--cn-input-bg, var(--input-bg));
    border-radius: var(--radius-md);
    color: var(--text-primary);
    outline: 2px solid transparent;
    outline-offset: 1px;
    transition: outline-color 160ms ease, outline-offset 160ms ease;
}
.cn-input-otp-slot:hover {
    border-color: var(--border-hover);
    background: var(--input-bg-hover);
}
.cn-input-otp-slot:focus {
    /* HID focus ring — see `.cn-input:focus` for rationale. */
    border-color: var(--border-focus);
    background: var(--input-bg-focus);
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
}
.cn-input-otp-slot--error {
    border-color: var(--border-error);
}
.cn-input-otp-slot--error:focus {
    border-color: var(--border-error);
    outline: 2px solid var(--focus-ring-error);
}
.cn-input-otp--disabled {
    opacity: 0.5;
}

/* ============================================================================
   Textarea
   ============================================================================ */

.cn-textarea {
    /* See `.cn-input` for the rationale on omitting `border:` here. */
    background: var(--cn-textarea-bg, var(--input-bg));
    border-radius: var(--radius-default);
    color: var(--text-primary);
    outline: 2px solid transparent;
    outline-offset: 1px;
    transition: outline-color 160ms ease, outline-offset 160ms ease;
}
.cn-textarea:hover {
    border-color: var(--border-hover);
    background: var(--input-bg-hover);
}
.cn-textarea:focus {
    /* HID focus ring — see `.cn-input:focus` for rationale. */
    border-color: var(--border-focus);
    background: var(--input-bg-focus);
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
}
.cn-textarea--error {
    border-color: var(--border-error);
}
.cn-textarea--error:focus {
    border-color: var(--border-error);
    outline: 2px solid var(--focus-ring-error);
}
.cn-textarea--success {
    border-color: var(--success);
}
.cn-textarea--success:focus {
    border-color: var(--success);
    outline: 2px solid var(--focus-ring-success);
}

/* ============================================================================
   Label
   ============================================================================ */

.cn-label {
    color: var(--cn-label-color, var(--text-primary));
}
.cn-label--disabled {
    color: var(--text-tertiary);
}

/* ============================================================================
   Kbd
   ============================================================================ */

.cn-kbd {
    background: var(--cn-kbd-bg, var(--surface));
    border-color: var(--cn-kbd-border, var(--border));
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
}

/* ============================================================================
   Checkbox
   ============================================================================ */

.cn-checkbox {
    /* Border width is Rust-owned because it varies per size
       (1.5px / 2px / 2px). Border color + bg are state-driven by the
       Stateful builder, but we still set `transition:` here so any user
       cascade override animates rather than snaps. */
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition:
        background var(--duration-fast) var(--ease-state),
        border-color var(--duration-fast) var(--ease-state);
}
/* Hover scale is Rust-driven via the Stateful FSM — duplicating it here
   compounded with the Rust transform. State colors handled in Rust too. */
.cn-checkbox--checked {
    background: var(--cn-checkbox-checked-bg, var(--primary));
    border-color: var(--cn-checkbox-checked-border, var(--primary));
}
.cn-checkbox--disabled {
    opacity: 0.5;
    cursor: not-allowed;
}

/* ============================================================================
   Switch
   ============================================================================ */

.cn-switch {
    border-radius: var(--radius-full);
    cursor: pointer;
    transition: background var(--duration-normal) var(--ease-state);
}
.cn-switch-track {
    background: var(--cn-switch-off-bg, var(--border));
    border-radius: var(--radius-full);
}
.cn-switch-track--on {
    background: var(--cn-switch-on-bg, var(--primary));
}
.cn-switch-thumb {
    background: var(--cn-switch-thumb, var(--text-inverse));
    border-radius: var(--radius-full);
}
.cn-switch--disabled {
    opacity: 0.5;
    cursor: not-allowed;
}

/* ============================================================================
   Toggle (binary on/off button — shadcn `<Toggle>`)
   ============================================================================ */

/* Layout widget already paints bg / border / fg from theme tokens.
   The cn rules here just add the variant + size hooks and the hover /
   active state-rule wiring users can extend via `.cn-toggle:hover` /
   `.cn-toggle--pressed` overrides. Don't redeclare bg/border here —
   the layout widget's setters would get clobbered. */
.cn-toggle {
    border-radius: var(--radius-default);
    cursor: pointer;
    transition:
        background var(--duration-fast) var(--ease-state),
        border-color var(--duration-fast) var(--ease-state),
        color var(--duration-fast) var(--ease-state);
}
.cn-toggle--sm { border-radius: var(--radius-sm); }
.cn-toggle--md { border-radius: var(--radius-default); }
.cn-toggle--lg { border-radius: var(--radius-default); }

/* ============================================================================
   Radio
   ============================================================================ */

.cn-radio {
    border: 2px solid var(--cn-radio-border, var(--border-secondary));
    border-radius: var(--radius-full);
    cursor: pointer;
    transition:
        border-color var(--duration-fast) var(--ease-state),
        transform var(--duration-fastest) var(--ease-state);
}
/* `:not(--disabled)` matches what the widget itself does: it gates both
   the accent border and the scale on the option being reachable. Without
   it a disabled dial lit up and grew under the pointer, and the scale
   arrived as a state style on an already-built node rather than at build
   time, which smeared. */
.cn-radio:hover:not(.cn-radio--disabled) {
    border-color: var(--cn-radio-hover-border, var(--primary));
    transform: scale(1.05, 1.05);
}
.cn-radio--selected {
    border-color: var(--cn-radio-selected, var(--primary));
}
.cn-radio-dot {
    background: var(--cn-radio-dot, var(--primary));
    border-radius: var(--radius-full);
    transition: transform var(--duration-fastest) var(--ease-state);
}
/* Press feedback. The widget used to do this from an FSM, which meant a
   rebuild per pointer transition; here it costs nothing. */
.cn-radio:active:not(.cn-radio--disabled) .cn-radio-dot {
    transform: scale(0.8, 0.8);
}
.cn-radio--disabled {
    opacity: 0.5;
    cursor: not-allowed;
}

/* ============================================================================
   Tabs
   ============================================================================ */

.cn-tabs-list {
    /* Tonal tray for the trigger row — uses surface-overlay so the
       active trigger (raised to --surface) reads as elevated. */
    background: var(--cn-tabs-list-bg, var(--surface-overlay));
    border-radius: var(--radius-md);
    padding: var(--space-1);
    gap: var(--space-1);
}
.cn-tabs-trigger {
    border-radius: var(--radius-default);
    cursor: pointer;
    color: var(--text-secondary);
    transition: color var(--duration-fast) var(--ease-state);
}
.cn-tabs-trigger:hover:not(.cn-tabs-trigger--active) {
    color: var(--text-primary);
}
.cn-tabs-trigger--active {
    /* Active trigger lifts to --surface so it stands out from the
       --surface-overlay tray underneath. Previously --background,
       which made the active trigger MERGE with the canvas instead
       of reading as raised. */
    background: var(--cn-tabs-active-bg, var(--surface));
    color: var(--text-primary);
    box-shadow: theme(shadow-sm);
}
.cn-tabs-trigger--disabled {
    opacity: 0.5;
    cursor: not-allowed;
}

/* Tab trigger sizes — content + padding determines height (no fixed
   heights). Vertical padding kept tight to keep the tray compact. */
.cn-tabs-trigger--sm { padding: var(--space-1) var(--space-3); font-size: var(--text-sm); }
.cn-tabs-trigger--md { padding: var(--space-1-5) var(--space-4); font-size: var(--text-sm); }
.cn-tabs-trigger--lg { padding: var(--space-2) var(--space-5); font-size: var(--text-lg); }

/* ============================================================================
   Select
   ============================================================================ */

.cn-select-trigger {
    /* `background`, `border`, and `color` are Rust-owned (state-aware:
       open / disabled / placeholder). Leaving CSS values here would
       overwrite the disabled `InputBgDisabled` fill with `--surface`
       (white) — the exact regression that hid the disabled trigger bg.
       Keep only the radius + cursor + transition; users still cascade
       via more specific rules if they need to override. */
    border-radius: var(--radius-default);
    cursor: pointer;
    transition: border-color var(--duration-fast) var(--ease-state);
}

.cn-select-content {
    /* Floating dropdown panel → elevation 2.
       Shared overlay-menu chrome — keep `.cn-dropdown-menu`,
       `.cn-context-menu`, and `.cn-combobox-content` in sync.
       No outer padding: items extend edge-to-edge and the rounded
       overflow_clip on the panel (see `ClipShape::rounded_rect_shaped`)
       trims their hover bg to the panel's outer curve, so the
       highlight follows the rounded corner instead of leaving a
       gap between the item's small radius and the panel's larger
       one. */
    background: var(--surface-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-default);
    /* Reuse the dropdown keyframes — same top-anchored slide/fade. */
    animation: cn-dropdown-menu-enter var(--duration-fast) var(--ease-state);
    transform-origin: top center;
}

.cn-select-item {
    padding: var(--space-2) var(--space-3);
    cursor: pointer;
    color: var(--text-primary);
    /* No `border-radius` — item bg is a clean rectangle so the
       panel's rounded clip can shape the first/last items into the
       outer curve. A small radius here would leave a visible band
       between the item's curve and the panel's curve. */
    /* No CSS `transition` here — same rationale as cn-menubar-item:
       when the cursor slides across rows quickly the bg transition
       leaves multiple rows partially highlighted (each at a different
       point in the fade-out) and reads as a stuck-hover bug. Instant
       on/off matches the HID. */
}
/* Item hover uses `--accent-subtle` because the parent panel is
   already at `--surface-elevated`; hovering to the same colour
   would be invisible. `--accent-subtle` is a low-alpha accent
   tint specifically designed for this use. */
.cn-select-item:hover {
    background: var(--accent-subtle);
}
/* `--selection` (~24 % alpha accent on Hybrid, 20 % on macOS) vs hover's
   `--accent-subtle` (~10 % alpha) so the currently-chosen row is
   visibly distinct from hovered rows and from the panel itself. */
.cn-select-item--selected {
    background: var(--selection);
    color: var(--accent);
}

/* ============================================================================
   Slider
   ============================================================================ */

.cn-slider-track {
    /* Match `.cn-progress` and `.cn-switch-track` — `--border` reads as
       quiet chrome that delineates the track without competing with
       the primary fill. `--surface-elevated` previously vanished
       against the page on light themes (panel + page are both very
       near-white in Hybrid). */
    background: var(--cn-slider-track-bg, var(--border));
    border-radius: var(--radius-full);
}
.cn-slider-fill {
    background: var(--cn-slider-fill-bg, var(--primary));
    border-radius: var(--radius-full);
}
.cn-slider-thumb {
    /* No `background` here — bg is driven entirely by the Rust-side
       `.bg(...)` call so per-state interiors (TextInverse for idle,
       transparent for hover, input-bg-disabled for disabled via the
       `--disabled` class) aren't clobbered by CSS-class application
       order. The base CSS used to set `background: var(--surface)`,
       which was applied AFTER any inline `.bg(TRANSPARENT)` and made
       the hover halo invisible through the thumb's centre.
       No `border` either — same rationale as the bg: state-specific
       outlines (Border idle, Primary hover/drag, BorderSecondary
       disabled) come from Rust. CSS keeps just the always-true chrome
       (full rounded corners, pointer cursor). */
    border-radius: var(--radius-full);
    cursor: pointer;
}
/* Disabled thumb tone — matches the disabled-button / disabled-input
   surface family (--input-bg-disabled). Just changing the thumb bg
   isn't enough on its own — `cn::slider` also overrides the track to
   `--input-bg-disabled` and the fill to `--border-secondary` so the
   whole control reads as inert (same approach `cn::switch` takes:
   muted track + thumb chrome that doesn't change opacity). */
.cn-slider-thumb--disabled {
    background: var(--input-bg-disabled);
    border-color: var(--border-secondary);
    border-width: 1px;
    cursor: not-allowed;
}

/* ============================================================================
   Progress
   ============================================================================ */

.cn-progress {
    /* Subtle gray track — matches typical HID expectations (Material /
       Apple HIG / shadcn). `--secondary` was the dark slate Secondary-
       button tone, which competed visually with the primary fill and
       made the track read as a second-tier button rather than chrome.
       `--border` is the same token used for switch tracks and reads as
       light contained chrome. */
    background: var(--cn-progress-track, var(--border));
    border-radius: var(--radius-full);
    overflow: hidden;
}
.cn-progress-bar {
    background: var(--cn-progress-bar, var(--primary));
    border-radius: var(--radius-full);
    transition: width var(--duration-slow) var(--ease-nav);
}
.cn-progress--sm { height: var(--space-1); }
.cn-progress--md { height: var(--space-2); }
.cn-progress--lg { height: var(--space-3); }

/* ============================================================================
   Avatar
   ============================================================================ */

.cn-avatar {
    background: var(--cn-avatar-bg, var(--surface));
    border-radius: var(--radius-full);
    overflow: hidden;
}
.cn-avatar--square {
    border-radius: var(--radius-default);
}

/* ============================================================================
   Spinner
   ============================================================================ */

.cn-spinner {
    color: var(--cn-spinner-color, var(--primary));
}

/* ============================================================================
   Tooltip
   ============================================================================ */

.cn-tooltip {
    background: var(--cn-tooltip-bg, var(--tooltip-bg));
    color: var(--cn-tooltip-text, var(--tooltip-text));
    border-radius: var(--radius-sm);
    font-size: var(--text-xs);
    padding: var(--space-1-5) var(--space-3);
    /* Snap-in for now. An earlier CSS-driven fade-in (`@keyframes
       cn-tooltip-enter`) was wired up but never actually fired — class-
       only `@keyframes` animations didn't start until the class-anim
       fix in `start_all_css_animations`. Re-enabling the fade exposed
       a text-glyph opacity gap: the glyph pipeline doesn't pick up the
       container's animated opacity mid-animation, so the tooltip text
       disappears on desktop and looks laggy on web. Keep snap-in until
       the glyph path honours animated opacity. */
}

/* ============================================================================
   Dialog
   ============================================================================ */

.cn-dialog {
    /* Modal → elevation 2 (lifts above surrounding cards) */
    background: var(--cn-dialog-bg, var(--surface-elevated));
    border: 1px solid var(--cn-dialog-border, var(--border));
    border-radius: var(--radius-xl);
    padding: var(--space-6);
    gap: var(--space-4);
    /* Enter/exit motion is driven by the OverlayBuilder motion_enter /
       motion_exit; no CSS keyframe here. */
}

/* ============================================================================
   Drawer
   ============================================================================ */

.cn-drawer {
    /* Modal overlay → elevation 2. Enter/exit motion is driven by the
       DrawerBuilder motion_enter / motion_exit. */
    background: var(--cn-drawer-bg, var(--surface-elevated));
    border: 1px solid var(--cn-drawer-border, var(--border));
}
.cn-drawer-header {
    border-bottom: 1px solid var(--border);
    padding: var(--space-4);
}
.cn-drawer-footer {
    padding: var(--space-4);
}

/* ============================================================================
   Sheet
   ============================================================================ */

.cn-sheet {
    /* Modal overlay → elevation 2. Enter/exit motion is driven by the
       SheetBuilder motion_enter / motion_exit. */
    background: var(--cn-sheet-bg, var(--surface-elevated));
    border: 1px solid var(--cn-sheet-border, var(--border));
}

/* ============================================================================
   Toast
   ============================================================================ */

.cn-toast {
    /* Floating notification → elevation 2 */
    background: var(--cn-toast-bg, var(--surface-elevated));
    border: 1px solid var(--cn-toast-border, var(--border));
    border-radius: var(--radius-xl);
    color: var(--text-primary);
    /* Enter/exit motion is driven by the ToastBuilder motion_enter /
       motion_exit (slide from the tray corner by default). */
}
.cn-toast--success {
    border: 1px solid var(--success);
}
.cn-toast--warning {
    border: 1px solid var(--warning);
}
.cn-toast--error {
    border: 1px solid var(--error);
}
.cn-toast--info {
    border: 1px solid var(--info);
}

/* ============================================================================
   Accordion
   ============================================================================ */

.cn-accordion {
    /* Accordion outer is a card-tier container, not a modal —
       elevation 1 (--surface). */
    background: var(--cn-accordion-bg, var(--surface));
    border: 1.5px solid var(--cn-accordion-border, var(--border));
    border-radius: var(--radius-xl);
}
.cn-accordion-trigger {
    padding: var(--space-4) var(--space-3);
    cursor: pointer;
    color: var(--text-primary);
    font-size: var(--text-sm);
}
.cn-accordion-trigger:hover {
    background: var(--surface-overlay);
}
.cn-accordion-content {
    /* Expanded content recedes to the canvas tone inside the
       elevated container. */
    background: var(--cn-accordion-content-bg, var(--background));
    border-top: 1px solid var(--border);
    color: var(--text-secondary);
}

/* ============================================================================
   Breadcrumb
   ============================================================================ */

.cn-breadcrumb {
    gap: var(--space-2);
    color: var(--text-secondary);
}
.cn-breadcrumb-item {
    color: var(--text-secondary);
    cursor: pointer;
}
.cn-breadcrumb-item:hover {
    color: var(--text-primary);
}
.cn-breadcrumb-item--active {
    color: var(--text-primary);
}

/* ============================================================================
   Pagination
   ============================================================================ */

.cn-pagination {
    gap: var(--space-1);
}
.cn-pagination-btn {
    border: 1px solid var(--border);
    border-radius: var(--radius-default);
    cursor: pointer;
    color: var(--text-primary);
}
.cn-pagination-btn:hover {
    background: var(--surface-elevated);
}
.cn-pagination-btn--active {
    background: var(--primary);
    color: var(--text-inverse);
    border-color: var(--primary);
}
/* The current page button is non-interactive (cursor: default, no
   click handler), so its hover state should stay locked on the active
   styling. Without this, the generic `.cn-pagination-btn:hover` rule
   above wins (same specificity, state-selector applied after base) and
   the primary-blue fill flips to `--surface-elevated` mid-hover — the
   number disappears against the now-light bg and the row reads as if
   no page is selected. */
.cn-pagination-btn--active:hover {
    background: var(--primary);
    color: var(--text-inverse);
    border-color: var(--primary);
}
.cn-pagination-btn--disabled {
    opacity: 0.5;
    cursor: not-allowed;
}
/* Match the active rule — disabled buttons (chevrons at first/last
   page) shouldn't repaint to surface-elevated on hover; they keep
   their dimmed look so the interactive affordance reads as 'not
   available right now'. */
.cn-pagination-btn--disabled:hover {
    background: transparent;
}

/* ============================================================================
   Navigation Menu
   ============================================================================ */

.cn-nav-menu {
    gap: var(--space-1);
}
.cn-nav-link {
    padding: var(--space-2) var(--space-3);
    cursor: pointer;
    color: var(--text-secondary);
}
.cn-nav-link:hover {
    background: var(--surface-elevated);
    color: var(--text-primary);
}
.cn-nav-link--active {
    background: var(--surface-elevated);
    color: var(--text-primary);
}
/* The dropdown panel (`.cn-nav-menu-content`) is itself painted at
   `--surface-elevated`, so the generic `.cn-nav-link:hover` rule
   above (which sets the same fill) leaves items inside the panel
   with no visible hover affordance. Override with `--accent-subtle`
   for the descendant case — same convention combobox / select /
   dropdown-menu items use against their own surface-elevated panels. */
.cn-nav-menu-content .cn-nav-link:hover {
    background: var(--accent-subtle);
}

.cn-nav-menu-content {
    /* Floating overlay → elevation 2 */
    background: var(--surface-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    /* CSS-driven enter — slide down + fade. Motion FSM workaround. */
    animation: cn-nav-menu-enter var(--duration-fast) var(--ease-state);
    transform-origin: top center;
}

@keyframes cn-nav-menu-enter {
    from { opacity: 0; transform: scale(0.98) translateY(-4px); }
    to   { opacity: 1; transform: scale(1) translateY(0); }
}

/* ============================================================================
   Sidebar
   ============================================================================ */

.cn-sidebar {
    background: var(--cn-sidebar-bg, var(--surface));
    border-right: 1px solid var(--border);
}
.cn-sidebar-item {
    padding: var(--space-2) var(--space-3);
    cursor: pointer;
    background: transparent;
    color: var(--text-secondary);
}
.cn-sidebar-item:hover:not(.cn-sidebar-item--active) {
    /* Subtle accent feedback — matches the overlay-menu hover treatment
       (dropdown / select / context). Previously this used
       `--surface-elevated` (#FBFCFE in light), which is the same token
       as the active state, so hover and active looked identical. */
    background: var(--accent-subtle);
    color: var(--text-primary);
}
.cn-sidebar-item--active {
    background: var(--surface-elevated);
    color: var(--text-primary);
}

/* ============================================================================
   Scroll Area
   ============================================================================ */

.cn-scroll-area {
    overflow: hidden;
}

/* ============================================================================
   Dropdown Menu
   ============================================================================ */

.cn-dropdown-menu {
    /* Shared overlay-menu chrome with select / context / combobox.
       No outer padding so item hover bgs run flush with the panel's
       rounded edge — see `.cn-select-content` for the rationale. */
    background: var(--surface-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-default);
    /* CSS-driven enter — slight scale + fade. Motion FSM workaround. */
    animation: cn-dropdown-menu-enter var(--duration-fast) var(--ease-state);
    transform-origin: top center;
}

@keyframes cn-dropdown-menu-enter {
    from { opacity: 0; transform: scale(0.96) translateY(-4px); }
    to   { opacity: 1; transform: scale(1) translateY(0); }
}
.cn-dropdown-item {
    padding: var(--space-2) var(--space-3);
    cursor: pointer;
    color: var(--text-primary);
    font-size: var(--text-sm);
    /* No `border-radius` — see `.cn-select-item`. */
    /* No transition — see cn-menubar-item rationale. */
}
/* `--accent-subtle` — parent panel sits at `--surface-elevated`,
   so hovering to the same tier would be invisible. */
.cn-dropdown-item:hover {
    background: var(--accent-subtle);
}
.cn-dropdown-item--disabled {
    opacity: 0.5;
    cursor: not-allowed;
}
.cn-dropdown-item--destructive {
    color: var(--error);
}

/* ============================================================================
   Context Menu
   ============================================================================ */

.cn-context-menu {
    /* Shared overlay-menu chrome with select / dropdown / combobox.
       No outer padding so item hover bgs run flush with the panel's
       rounded edge — see `.cn-select-content` for the rationale. */
    background: var(--surface-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-default);
    /* CSS-driven enter — small scale + fade. */
    animation: cn-context-menu-enter var(--duration-fast) var(--ease-state);
    transform-origin: top left;
}

@keyframes cn-context-menu-enter {
    from { opacity: 0; transform: scale(0.96) translateY(-2px); }
    to   { opacity: 1; transform: scale(1) translateY(0); }
}

/* Belt-and-braces marker for "parent menu should not re-enter when a
   submenu is hovered". The real fix lives in
   `RenderTree::sweep_stale_css_animations` — the parent menu's
   ActiveCssAnimation now survives the OverlayStack-driven Structural
   rebuild because the wrapper's stable id is re-minted unchanged, so
   `start_all_css_animations`' contains_key gate suppresses the
   restart. This `:has()` rule does not actively stop a running
   animation (state-style passes don't reach into css_anim_store) but
   documents the intent and would no-op a fresh start if any future
   rebuild path bypasses the sweep. */
.cn-context-menu:has(.cn-context-menu-item--has-submenu:hover) {
    background: var(--surface-elevated);
    animation: none;
}
.cn-context-menu-item {
    /* `space-1-5 / space-3` (~6×12) gives a tighter row height that
       matches `.cn-menubar-trigger`. The earlier `space-2` (8px
       vertical) left items reading taller than the surrounding
       menubar / dropdown chrome — reducing to space-1-5 lines them
       up. Horizontal padding stays at space-3 so the row chrome's
       hit area extends comfortably past the label edge. */
    padding: var(--space-1-5) var(--space-3);
    /* No `border-radius` — see `.cn-select-item`. */
    cursor: pointer;
    color: var(--text-primary);
    font-size: var(--text-sm);
    /* No transition — see cn-menubar-item rationale. */
}
.cn-context-menu-item:hover {
    background: var(--accent-subtle);
}

/* ============================================================================
   Menubar
   ============================================================================ */

.cn-menubar {
    /* Elevated container → elevation 2 */
    background: var(--surface-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    padding: var(--space-1);
    gap: var(--space-1);
}
.cn-menubar-trigger {
    padding: var(--space-1-5) var(--space-3);
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-primary);
    font-size: var(--text-sm);
    background: transparent;
}
.cn-menubar-trigger:hover {
    background: var(--surface-elevated);
}
/* Dropdown panel that opens when a menubar trigger is activated.
   Shared chrome with .cn-dropdown-menu / .cn-context-menu so the
   File / Edit / View popups read as elevation-2 floating surfaces
   instead of the pure-white `Surface` the Rust-side `.bg(...)`
   fallback was painting. */
.cn-menubar-content {
    /* No outer padding — see `.cn-select-content`. */
    background: var(--surface-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-default);
    /* Reuse the dropdown keyframes — same top-anchored slide/fade. */
    animation: cn-dropdown-menu-enter var(--duration-fast) var(--ease-state);
    transform-origin: top center;
}
/* Belt-and-braces — same as `.cn-context-menu:has(...)`. Real
   suppression now lives in `RenderTree::sweep_stale_css_animations`,
   which keeps the parent panel's ActiveCssAnimation alive across the
   submenu's OverlayStack-driven Structural rebuild so the enter
   keyframe never restarts. */
.cn-menubar-content:has(.cn-menubar-item--has-submenu:hover) {
    animation: none;
}
.cn-menubar-item {
    /* Match `.cn-dropdown-item` / `.cn-context-menu-item` so all three
       menu primitives share the same row chrome. The Rust builder
       applies a smaller `.py / .px` to set a sensible fallback when
       cn_styles isn't loaded; this CSS rule overrides it when the
       stylesheet IS present. No `border-radius` — see
       `.cn-select-item`. */
    padding: var(--space-2) var(--space-3);
    background: transparent;
    color: var(--text-primary);
    font-size: var(--text-sm);
    /* No CSS `transition` here — when the cursor moves across menu rows
       quickly, the transition trail leaves multiple rows partially
       highlighted (each at a different point in the fade-out animation),
       which reads as a stuck-hover bug. Instant on/off matches the HID. */
}
/* `--accent-subtle` — parent dropdown panel sits at `--surface-elevated`,
   so hovering to the same tier is invisible. Match cn-dropdown-item /
   cn-context-menu-item for visual consistency across menu primitives. */
.cn-menubar-item:hover {
    background: var(--accent-subtle);
}

/* ============================================================================
   Popover
   ============================================================================ */

.cn-popover-content {
    /* Floating overlay → elevation 2 */
    background: var(--surface-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    padding: var(--space-4);
    /* CSS-driven enter — same approach as cn-tooltip while the motion FSM
       integration with the new OverlayStack is being fixed. */
    animation: cn-popover-enter var(--duration-normal) var(--ease-spring);
    transform-origin: top center;
}

@keyframes cn-popover-enter {
    from { opacity: 0; transform: scale(0.96) translateY(-4px); }
    to   { opacity: 1; transform: scale(1) translateY(0); }
}

/* ============================================================================
   Hover Card
   ============================================================================ */

.cn-hover-card-content {
    /* Floating overlay → elevation 2 */
    background: var(--surface-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    padding: var(--space-4);
    /* CSS-driven enter — same approach as cn-tooltip / cn-popover. */
    animation: cn-hover-card-enter var(--duration-normal) var(--ease-spring);
    transform-origin: top center;
}

@keyframes cn-hover-card-enter {
    from { opacity: 0; transform: scale(0.96) translateY(-4px); }
    to   { opacity: 1; transform: scale(1) translateY(0); }
}

/* ============================================================================
   Tree View
   ============================================================================ */

.cn-tree-node {
    /* No `padding` here — Rust owns per-side padding so the left side
       can encode tree-depth indent. CSS overriding `padding:` would
       collapse all rows to the same x-offset. */
    border-radius: var(--radius-sm);
    cursor: pointer;
    /* No CSS `transition` here — same rationale as cn-menubar-item /
       cn-select-item: the bg transition leaves multiple rows
       partially highlighted on a fast cursor sweep. */
}
.cn-tree-node:hover {
    background: var(--surface-elevated);
}
.cn-tree-node--selected {
    background: var(--primary);
    color: var(--text-inverse);
}

/* ============================================================================
   Resizable
   ============================================================================ */

/* `.cn-resizable-handle` is the wide HIT AREA wrapper (thickness +
   hit padding on each side). The actual visible thin handle line
   is the Rust-side inner div whose background is theme-driven —
   `--border` at rest, `--primary` while dragging. Painting the
   wrapper background would fill the entire hit zone with that
   colour, making the handle read 2-3× wider than its actual
   visual stripe. Keep the wrapper transparent. */
.cn-resizable-handle {
    background: transparent;
}

/* ============================================================================
   Collapsible
   ============================================================================ */

.cn-collapsible-trigger {
    cursor: pointer;
    color: var(--text-primary);
}

/* ============================================================================
   Combobox
   ============================================================================ */

.cn-combobox-trigger {
    /* Match select-trigger: bg / border / color are Rust-owned so the
       state-aware disabled fill isn't clobbered by CSS. */
    border-radius: var(--radius-default);
    cursor: pointer;
    transition: border-color var(--duration-fast) var(--ease-state);
}
.cn-combobox-content {
    /* Shared overlay-menu chrome with select / dropdown / context.
       No outer padding — see `.cn-select-content`. */
    background: var(--surface-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-default);
    /* Reuse the dropdown keyframes — same top-anchored slide/fade. */
    animation: cn-dropdown-menu-enter var(--duration-fast) var(--ease-state);
    transform-origin: top center;
}
.cn-combobox-item {
    padding: var(--space-2) var(--space-3);
    /* No `border-radius` — see `.cn-select-item`. */
    cursor: pointer;
    color: var(--text-primary);
    font-size: var(--text-sm);
    /* No transition — see cn-menubar-item rationale. */
}
/* Hover uses `--accent-subtle`. The selected state uses `--selection`,
   which carries roughly 2× the alpha (~20% vs ~10%) on most themes so
   the currently-chosen row is visibly distinct against the panel
   background even when Surface is pure white — `accent-subtle` alone
   on `Surface = #FFFFFF` (macOS light) renders as a ~5 % blue tint
   that's effectively invisible. */
.cn-combobox-item:hover {
    background: var(--accent-subtle);
}
.cn-combobox-item--selected {
    background: var(--selection);
    color: var(--accent);
}

/* ============================================================================
   Table (shadcn `<Table>` family)
   ============================================================================ */

/* Outer container — 1px border, rounded, clipped so the row borders
   don't bleed past the rounded corners. The layout `table()` widget
   already paints `overflow_clip`; the cn rule adds the surface chrome
   (border + radius + base font-size). bg stays unset so layout's
   per-section setters (thead / tfoot paint `--surface-overlay`) win. */
.cn-table {
    border: 1px solid var(--cn-table-border, var(--border));
    border-radius: var(--cn-table-radius, var(--radius-md));
    font-size: var(--text-sm);
}

/* thead / tfoot bg is owned by the layout widget (theme token
   `--surface-overlay` via Rust setters). Don't redeclare here — the
   CSS value would clobber the setter result. The cn footer only adds
   the top-border + medium font-weight. */
.cn-table-footer {
    font-weight: 500;
    border-top: 1px solid var(--cn-table-border, var(--border));
}

/* Rows — bottom border + hover bg. The row separator is what gives the
   table its grid feel; tbody's last row strips the border via the
   `:last-child` rule so the closing edge sits flush with the outer
   border-radius. */
.cn-table-row {
    border-bottom: 1px solid var(--cn-table-border, var(--border));
    transition: background var(--duration-fast) var(--ease-state);
}
.cn-table-body .cn-table-row:last-child {
    border-bottom: none;
}
.cn-table-row:hover {
    background: var(--cn-table-row-hover, var(--surface-hover));
}
.cn-table-row--selected,
.cn-table-row--selected:hover {
    background: var(--cn-table-row-selected, var(--selection));
}

/* Header cell — muted-foreground, medium-weight, left-aligned, compact
   40 px height to match shadcn. */
.cn-table-head {
    color: var(--text-secondary);
    font-weight: 500;
    font-size: var(--text-sm);
}

/* Data cell — no extra colour; inherits TextPrimary from the layout
   widget. cn just contributes the class hook for users to extend. */
.cn-table-cell { }

/* Caption — small muted label below the table. */
.cn-table-caption {
    color: var(--text-secondary);
    font-size: var(--text-sm);
}
"#;
