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
/// Wrapped in `@layer components`, so every rule sits below the
/// application's own rules whatever their specificity.
///
/// Uses `var(--theme-token)` for all color references.
/// Each component defines `var(--cn-component-prop, var(--fallback))` for overridability.
pub const CN_STYLES: &str = concat!(
    "@layer components {\n",
    r#"
/* The document's text colour, which text and icons that set none inherit,
   as a page's root colour is. */
:root {
    color: var(--text-primary);
    /* What a chart's canvas draws its grid, pointer cursor and dot rings in;
       series take --chart-1 to --chart-5 from the theme. */
    --cn-chart-grid: var(--border);
    --cn-chart-cursor: var(--text-secondary);
    --cn-chart-surface: var(--surface);
}

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
.cn-slider-field, .cn-progress,
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
.cn-tree, .cn-skeleton {
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

/* Button: each variant's fill, text colour, border and shadow, and how they
   change on hover, press and while disabled, are rules, eased by the
   transition. Geometry tokens flow from the active theme's RadiusTokens so
   each variant (Restrained / Hybrid / Expressive) gets its own corner reach. */
.cn-button {
    border-radius: var(--radius-default);
    cursor: pointer;
    transition:
        background-color var(--duration-fast) var(--ease-state),
        color var(--duration-fast) var(--ease-state),
        border-color var(--duration-fast) var(--ease-state),
        box-shadow var(--duration-fast) var(--ease-state),
        transform var(--duration-faster) var(--ease-state);
}
.cn-button:disabled { cursor: not-allowed; }
/* Press feedback: a pressed button shrinks a little and eases back. A link
   button is flat, so it dims instead. */
.cn-button:active:not(:disabled) {
    transform: scale(0.97, 0.97);
}
.cn-button--link:active:not(:disabled) {
    transform: none;
    opacity: 0.8;
}
.cn-button--primary {
    background: var(--primary);
    color: var(--text-inverse);
    box-shadow: var(--shadow-md);
}
.cn-button--primary:hover:not(:disabled) { background: var(--primary-hover); }
.cn-button--primary:active:not(:disabled) { background: var(--primary-active); }
.cn-button--secondary {
    background: var(--secondary);
    color: var(--text-inverse);
    box-shadow: var(--shadow-md);
}
.cn-button--secondary:hover:not(:disabled) { background: var(--secondary-hover); }
.cn-button--secondary:active:not(:disabled) { background: var(--secondary-active); }
.cn-button--destructive {
    background: var(--error);
    color: var(--text-inverse);
    box-shadow: var(--shadow-md);
}
.cn-button--destructive:hover:not(:disabled) { background: color-mix(in srgb, var(--error) 90%, black); }
.cn-button--destructive:active:not(:disabled) { background: color-mix(in srgb, var(--error) 85%, black); }
.cn-button--outline {
    background: transparent;
    color: var(--text-primary);
    border: 1px solid var(--border);
    box-shadow: var(--shadow-sm);
}
.cn-button--ghost {
    background: transparent;
    color: var(--text-primary);
}
.cn-button--outline:hover:not(:disabled), .cn-button--ghost:hover:not(:disabled) {
    background: color-mix(in srgb, var(--text-primary) 5%, transparent);
}
.cn-button--outline:active:not(:disabled), .cn-button--ghost:active:not(:disabled) {
    background: color-mix(in srgb, var(--text-primary) 10%, transparent);
}
.cn-button--link {
    background: transparent;
    color: var(--primary);
}
/* Disabled: a filled tonal surface with muted text, a thin outline and no
   shadow. Opacity dimming on a saturated fill washed out to a pale tint, so
   the fill is swapped instead. The hover and press rules leave a disabled
   button alone. */
.cn-button:disabled {
    background: var(--input-bg-disabled);
    color: var(--text-tertiary);
    border: 1px solid var(--border-secondary);
    box-shadow: none;
    transform: none;
}
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
.cn-card-title { color: var(--text-primary); }
.cn-card-description { color: var(--text-secondary); }

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

/* A placeholder that pulses while content loads. */
.cn-skeleton {
    background: var(--cn-skeleton-bg, var(--surface-elevated));
    border-radius: var(--radius-sm);
    animation: cn-skeleton-pulse 1500ms var(--ease-in-out) infinite;
}
.cn-skeleton--circle { border-radius: var(--radius-full); }
@keyframes cn-skeleton-pulse {
    from { opacity: 1; }
    50%  { opacity: 0.4; }
    to   { opacity: 1; }
}

/* ============================================================================
   Input
   ============================================================================ */

/* The message under a field: an error, or a description. */
.cn-field-error { color: var(--error); }
.cn-field-description { color: var(--text-tertiary); }

.cn-input {
    box-shadow: var(--shadow-sm);
    background: var(--cn-input-bg, var(--input-bg));
    border-color: var(--border);
    border-radius: var(--radius-default);
    color: var(--text-primary);
    /* Idle ring: same width, transparent, gap of 1 px so the focus
       transition "scales" the gap from 1 → 2 px while the colour fades
       in. Without a base outline the transition has no starting point
       and the ring pops in. */
    outline: 2px solid transparent;
    outline-offset: 1px;
    transition:
        background-color var(--duration-fast) var(--ease-state),
        border-color var(--duration-fast) var(--ease-state),
        outline-color var(--duration-fast) var(--ease-state),
        outline-offset var(--duration-fast) var(--ease-out);
}
/* The border and fill per state are the layout input's `.blinc-input`
   rules, which these sit beside at the same specificity. */
.cn-input:focus:not(:disabled) {
    /* HID focus affordance: the border brightens AND a 2px outer ring is
       drawn 2px out from the input edge. The ring uses the fainter
       `--focus-ring` (alpha-tinted variant token) so the crisp border edge
       stays distinguishable from the soft halo. */
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
}
.cn-input:disabled,
.cn-textarea:disabled,
.cn-input-otp-slot:disabled {
    box-shadow: none;
}
.cn-input[data-invalid]:not(:disabled),
.cn-input[data-invalid]:hover:not(:disabled),
.cn-input[data-invalid]:focus:not(:disabled) {
    border-color: var(--border-error);
}
.cn-input[data-invalid]:focus:not(:disabled) {
    outline: 2px solid var(--focus-ring-error);
}
.cn-input.cn-input--success:not(:disabled),
.cn-input.cn-input--success:hover:not(:disabled),
.cn-input.cn-input--success:focus:not(:disabled) {
    border-color: var(--success);
}
.cn-input.cn-input--success:focus:not(:disabled) {
    outline: 2px solid var(--focus-ring-success);
}

.cn-input--sm { font-size: var(--text-xs); }
.cn-input--md { font-size: var(--text-sm); }
.cn-input--lg { font-size: var(--text-lg); }

/* Input OTP. The slot is a layout input; the border and fill per state are
   its `.blinc-input` rules. */

.cn-input-otp-slot {
    box-shadow: var(--shadow-sm);
    background: var(--cn-input-bg, var(--input-bg));
    border-color: var(--border);
    border-radius: var(--radius-md);
    color: var(--text-primary);
    outline: 2px solid transparent;
    outline-offset: 1px;
    transition:
        background-color var(--duration-fast) var(--ease-state),
        border-color var(--duration-fast) var(--ease-state),
        outline-color var(--duration-fast) var(--ease-state),
        outline-offset var(--duration-fast) var(--ease-out);
}
.cn-input-otp-slot:focus:not(:disabled) {
    /* HID focus ring — see `.cn-input:focus` for rationale. */
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
}
.cn-input-otp-slot.cn-input-otp-slot--error:not(:disabled),
.cn-input-otp-slot.cn-input-otp-slot--error:hover:not(:disabled),
.cn-input-otp-slot.cn-input-otp-slot--error:focus:not(:disabled) {
    border-color: var(--border-error);
}
.cn-input-otp-slot.cn-input-otp-slot--error:focus:not(:disabled) {
    outline: 2px solid var(--focus-ring-error);
}
/* The combobox's search field sits on the dropdown's surface and shows the
   focus border while hovered as well as focused. */
.cn-combobox-search {
    background: var(--surface);
    border-color: var(--border);
}
.cn-combobox-search:hover:not(:disabled),
.cn-combobox-search:focus:not(:disabled) {
    background: var(--surface);
    border-color: var(--border-focus);
}
.cn-input-otp--disabled {
    opacity: 0.5;
}

/* ============================================================================
   Textarea
   ============================================================================ */

.cn-textarea {
    box-shadow: var(--shadow-sm);
    background: var(--cn-textarea-bg, var(--input-bg));
    border-color: var(--border);
    border-radius: var(--radius-default);
    color: var(--text-primary);
    outline: 2px solid transparent;
    outline-offset: 1px;
    transition:
        background-color var(--duration-fast) var(--ease-state),
        border-color var(--duration-fast) var(--ease-state),
        outline-color var(--duration-fast) var(--ease-state),
        outline-offset var(--duration-fast) var(--ease-out);
}
/* The border and fill per state are the layout input's `.blinc-input`
   rules, which these sit beside at the same specificity. */
.cn-textarea:focus:not(:disabled) {
    /* HID focus ring — see `.cn-input:focus` for rationale. */
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
}
.cn-textarea[data-invalid]:not(:disabled),
.cn-textarea[data-invalid]:hover:not(:disabled),
.cn-textarea[data-invalid]:focus:not(:disabled) {
    border-color: var(--border-error);
}
.cn-textarea[data-invalid]:focus:not(:disabled) {
    outline: 2px solid var(--focus-ring-error);
}
.cn-textarea.cn-textarea--success:not(:disabled),
.cn-textarea.cn-textarea--success:hover:not(:disabled),
.cn-textarea.cn-textarea--success:focus:not(:disabled) {
    border-color: var(--success);
}
.cn-textarea.cn-textarea--success:focus:not(:disabled) {
    outline: 2px solid var(--focus-ring-success);
}

/* Number input: a number between a - and a +, one bordered
   piece. The steps are transparent and tint on hover and press; the field is
   borderless and clear (set in Rust, so the input rules give way to it). */
.cn-number-input-group {
    box-shadow: var(--shadow-sm);
    background: var(--input-bg);
    border-color: var(--border);
}
.cn-number-input-group:disabled {
    background: var(--input-bg-disabled);
    box-shadow: none;
}
.cn-number-input-step {
    background: transparent;
    color: var(--text-secondary);
    transition:
        background-color var(--duration-fast) var(--ease-state),
        color var(--duration-fast) var(--ease-state);
}
.cn-number-input-step--dec { border-right: 1px solid var(--border); }
.cn-number-input-step--inc { border-left: 1px solid var(--border); }
.cn-number-input-step:hover:not(:disabled) {
    background: color-mix(in srgb, var(--text-primary) 6%, transparent);
    color: var(--text-primary);
}
.cn-number-input-step:active:not(:disabled) {
    background: color-mix(in srgb, var(--text-primary) 10%, transparent);
}
.cn-number-input-step:disabled { color: var(--text-tertiary); }

/* Number gauge: a number over a range as a fill tinted from
   the primary, the value centred on it and a six-dot grip at the fill's edge.
   The grip deepens while the gauge is hovered, pressed, dragged or keyed. */
.cn-number-gauge {
    border-color: var(--border);
    background: var(--input-bg);
    color: var(--text-primary);
    cursor: ew-resize;
    outline: 2px solid transparent;
    outline-offset: 2px;
    transition: border-color var(--duration-fast) var(--ease-state);
}
.cn-number-gauge--sm { border-radius: var(--radius-sm); }
.cn-number-gauge--md, .cn-number-gauge--lg { border-radius: var(--radius-default); }
.cn-number-gauge:hover:not(:disabled) { border-color: var(--border-hover); }
.cn-number-gauge[data-focus-visible] { outline-color: var(--focus-ring); }
.cn-number-gauge-fill { background: color-mix(in srgb, var(--primary) 22%, var(--input-bg)); }
.cn-number-gauge-rest { background: transparent; }
.cn-number-gauge-thumb {
    background: color-mix(in srgb, var(--primary) 30%, var(--input-bg));
    color: var(--primary);
    transition: background-color var(--duration-fast) var(--ease-state);
}
.cn-number-gauge:hover:not(:disabled) > .cn-number-gauge-thumb,
.cn-number-gauge:active:not(:disabled) > .cn-number-gauge-thumb,
.cn-number-gauge[data-state="dragging"] > .cn-number-gauge-thumb,
.cn-number-gauge[data-focus-visible] > .cn-number-gauge-thumb {
    background: color-mix(in srgb, var(--primary) 44%, var(--input-bg));
}
.cn-number-gauge:disabled { color: var(--text-tertiary); cursor: default; }
.cn-number-gauge:disabled > .cn-number-gauge-fill { background: var(--input-bg-disabled); }
.cn-number-gauge:disabled > .cn-number-gauge-thumb {
    background: var(--border-secondary);
    color: var(--text-tertiary);
}
/* A range gauge stacks two faces: the upper paints the interval, the lower
   covers what lies before its grip with the track. */
.cn-number-gauge-layer[data-endpoint="lower"] > .cn-number-gauge-fill { background: var(--input-bg); }
.cn-number-gauge:hover:not(:disabled) > .cn-number-gauge-layer > .cn-number-gauge-thumb,
.cn-number-gauge:active:not(:disabled) > .cn-number-gauge-layer > .cn-number-gauge-thumb,
.cn-number-gauge[data-state="dragging"] > .cn-number-gauge-layer > .cn-number-gauge-thumb,
.cn-number-gauge[data-focus-visible] > .cn-number-gauge-layer > .cn-number-gauge-thumb {
    background: color-mix(in srgb, var(--primary) 44%, var(--input-bg));
}
.cn-number-gauge:disabled > .cn-number-gauge-layer > .cn-number-gauge-fill { background: var(--input-bg-disabled); }
.cn-number-gauge:disabled > .cn-number-gauge-layer > .cn-number-gauge-thumb {
    background: var(--border-secondary);
    color: var(--text-tertiary);
}

/* ============================================================================
   Label
   ============================================================================ */

.cn-label {
    color: var(--cn-label-color, var(--text-primary));
}
.cn-label-text { color: var(--cn-label-color, var(--text-primary)); }
.cn-label:disabled > .cn-label-text { color: var(--text-tertiary); }
.cn-label-required { color: var(--error); }

/* ============================================================================
   Kbd
   ============================================================================ */

.cn-kbd {
    background: var(--cn-kbd-bg, var(--surface));
    border: 1px solid var(--cn-kbd-border, var(--border));
    border-bottom-width: 3px;
    border-radius: var(--radius-sm);
    box-shadow: var(--shadow-sm);
    color: var(--text-secondary);
}

/* ============================================================================
   Checkbox
   ============================================================================ */

/* The look is the layout checkbox's `.blinc-checkbox` rules. These are cn's
   hooks on top of them: the custom properties a theme can set, and the
   disabled class. */
.cn-checkbox:checked {
    background: var(--cn-checkbox-checked-bg, var(--primary));
    border-color: var(--cn-checkbox-checked-border, var(--primary));
}

/* ============================================================================
   Switch
   ============================================================================ */

/* The track is the element; its fill follows hover, `checked` and disabled,
   and the thumb slides by a transform per size (track - thumb - 4px). */
.cn-switch {
    background: var(--border-secondary);
    border-radius: var(--radius-full);
    cursor: pointer;
    transition: background-color var(--duration-normal) var(--ease-state);
}
/* A wash of the text colour over the idle track, so hovering moves it the same
   way in either scheme (`--border-hover` is translucent and reads darker than
   this solid track on a dark theme). */
.cn-switch:hover:not(:disabled) {
    background: color-mix(in srgb, var(--text-primary) 10%, var(--border-secondary));
}
.cn-switch:checked { background: var(--primary); }
.cn-switch:checked:hover:not(:disabled) { background: var(--primary-hover); }
/* A disabled track takes the disabled-button palette, with a thin outline so
   it keeps a silhouette. */
.cn-switch:disabled {
    background: var(--input-bg-disabled);
    border: 1px solid var(--border-secondary);
    opacity: 0.5;
    cursor: not-allowed;
}
.cn-switch-thumb {
    background: var(--text-inverse);
    border-radius: var(--radius-full);
    box-shadow: var(--shadow-sm);
    transition: transform var(--duration-normal) var(--ease-state);
}
.cn-switch:disabled > .cn-switch-thumb {
    box-shadow: none;
    border: 1px solid var(--border-secondary);
}
.cn-switch--sm:checked > .cn-switch-thumb { transform: translateX(14px); }
.cn-switch--md:checked > .cn-switch-thumb { transform: translateX(20px); }
.cn-switch--lg:checked > .cn-switch-thumb { transform: translateX(24px); }
.cn-switch:active:not(:disabled) > .cn-switch-thumb { transform: scale(0.9); }
.cn-switch--sm:checked:active:not(:disabled) > .cn-switch-thumb { transform: translateX(14px) scale(0.9); }
.cn-switch--md:checked:active:not(:disabled) > .cn-switch-thumb { transform: translateX(20px) scale(0.9); }
.cn-switch--lg:checked:active:not(:disabled) > .cn-switch-thumb { transform: translateX(24px) scale(0.9); }
.cn-switch-label { color: var(--text-primary); }
.cn-switch-row:disabled > .cn-switch-label { color: var(--text-tertiary); }

/* ============================================================================
   Toggle (binary on/off button — shadcn `<Toggle>`)
   ============================================================================ */

/* The look is the layout toggle's `.blinc-toggle` rules (fill by state, the
   outline variant, the size ladder), selected by `data-state`, `data-variant`
   and `data-size`. `.cn-toggle` and its variant and size classes are hooks for
   an app's own rules. */

/* ============================================================================
   Radio
   ============================================================================ */

/* The look is the layout radio's `.blinc-radio` rules. These are cn's hooks on
   top of them: the custom properties a theme can set. */
.cn-radio { border-color: var(--cn-radio-border, var(--border-secondary)); }
.cn-radio:hover:not(:disabled) { border-color: var(--cn-radio-hover-border, var(--primary)); }
.cn-radio:checked { border-color: var(--cn-radio-selected, var(--primary)); }
.cn-radio-dot { background: var(--cn-radio-dot, var(--primary)); }

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
    color: var(--text-primary);
    font-weight: 500;
}
/* The pill behind the active trigger, raised to --surface over the
   --surface-overlay tray. It slides to the next trigger on a switch. */
.cn-tabs-indicator {
    background: var(--cn-tabs-active-bg, var(--surface));
    border-radius: var(--radius-default);
    box-shadow: var(--shadow-sm);
}
.cn-tabs-trigger--disabled {
    opacity: 0.5;
    cursor: not-allowed;
}

/* A panel enters and leaves by its `data-transition`: it fades, or slides a
   little from a side as it fades. `slide` goes the way the active tab moved,
   by `data-direction`: to a tab further along, the new panel comes from the
   right and the old one leaves to the left. */
@keyframes cn-tab-from-left {
    from { opacity: 0; transform: translateX(-20px); }
    to { opacity: 1; transform: translateX(0); }
}
@keyframes cn-tab-to-left {
    from { opacity: 1; transform: translateX(0); }
    to { opacity: 0; transform: translateX(-20px); }
}
@keyframes cn-tab-from-right {
    from { opacity: 0; transform: translateX(20px); }
    to { opacity: 1; transform: translateX(0); }
}
@keyframes cn-tab-to-right {
    from { opacity: 1; transform: translateX(0); }
    to { opacity: 0; transform: translateX(20px); }
}
@keyframes cn-tab-from-top {
    from { opacity: 0; transform: translateY(-20px); }
    to { opacity: 1; transform: translateY(0); }
}
@keyframes cn-tab-to-top {
    from { opacity: 1; transform: translateY(0); }
    to { opacity: 0; transform: translateY(-20px); }
}
@keyframes cn-tab-from-bottom {
    from { opacity: 0; transform: translateY(20px); }
    to { opacity: 1; transform: translateY(0); }
}
@keyframes cn-tab-to-bottom {
    from { opacity: 1; transform: translateY(0); }
    to { opacity: 0; transform: translateY(20px); }
}
.cn-tabs-panel[data-transition="fade"][data-motion="enter"] { animation: cn-fade-in var(--duration-normal) var(--ease-out); }
.cn-tabs-panel[data-transition="fade"][data-motion="exit"] { animation: cn-fade-out var(--duration-fast) var(--ease-in) forwards; }
.cn-tabs-panel[data-transition="slide"][data-direction="next"][data-motion="enter"] { animation: cn-tab-from-right var(--duration-normal) var(--ease-out); }
.cn-tabs-panel[data-transition="slide"][data-direction="next"][data-motion="exit"] { animation: cn-tab-to-left var(--duration-fast) var(--ease-in) forwards; }
.cn-tabs-panel[data-transition="slide"][data-direction="prev"][data-motion="enter"] { animation: cn-tab-from-left var(--duration-normal) var(--ease-out); }
.cn-tabs-panel[data-transition="slide"][data-direction="prev"][data-motion="exit"] { animation: cn-tab-to-right var(--duration-fast) var(--ease-in) forwards; }
.cn-tabs-panel[data-transition="slide-left"][data-motion="enter"] { animation: cn-tab-from-left var(--duration-normal) var(--ease-out); }
.cn-tabs-panel[data-transition="slide-left"][data-motion="exit"] { animation: cn-tab-to-left var(--duration-fast) var(--ease-in) forwards; }
.cn-tabs-panel[data-transition="slide-right"][data-motion="enter"] { animation: cn-tab-from-right var(--duration-normal) var(--ease-out); }
.cn-tabs-panel[data-transition="slide-right"][data-motion="exit"] { animation: cn-tab-to-right var(--duration-fast) var(--ease-in) forwards; }
.cn-tabs-panel[data-transition="slide-up"][data-motion="enter"] { animation: cn-tab-from-top var(--duration-normal) var(--ease-out); }
.cn-tabs-panel[data-transition="slide-up"][data-motion="exit"] { animation: cn-tab-to-top var(--duration-fast) var(--ease-in) forwards; }
.cn-tabs-panel[data-transition="slide-down"][data-motion="enter"] { animation: cn-tab-from-bottom var(--duration-normal) var(--ease-out); }
.cn-tabs-panel[data-transition="slide-down"][data-motion="exit"] { animation: cn-tab-to-bottom var(--duration-fast) var(--ease-in) forwards; }

/* A panel leaving after a switch overlays the new one instead of sitting
   above it, so the content does not jump while it fades out. */
.cn-tabs-panel--leaving {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
}

/* Tab trigger sizes — content + padding determines height (no fixed
   heights). Vertical padding kept tight to keep the tray compact. */
.cn-tabs-trigger--sm { padding: var(--space-1) var(--space-3); font-size: var(--text-sm); }
.cn-tabs-trigger--md { padding: var(--space-1-5) var(--space-4); font-size: var(--text-sm); }
.cn-tabs-trigger--lg { padding: var(--space-2) var(--space-5); font-size: var(--text-lg); }

/* ============================================================================
   Select
   ============================================================================ */

/* The caret of a trigger turns half a turn while its list is open, eased by a
   transition so it spins rather than flips. */
.cn-chevron {
    transition: transform var(--duration-normal) var(--ease-state);
}
.cn-select-trigger[data-state="open"] > .cn-chevron,
.cn-combobox-trigger[data-state="open"] > .cn-chevron,
.cn-dropdown-trigger[data-state="open"] > .cn-chevron,
.cn-collapsible-trigger[data-state="open"] > .cn-chevron,
.cn-accordion-trigger[data-state="open"] > .cn-chevron {
    transform: rotate(180deg);
}

.cn-select-trigger {
    color: var(--text-primary);
    background: transparent;
    box-shadow: var(--shadow-sm);
    border-color: var(--border);
    border-radius: var(--radius-default);
    cursor: pointer;
    transition:
        background-color var(--duration-fast) var(--ease-state),
        border-color var(--duration-fast) var(--ease-state);
}
/* Hover and press tint as the outline button's do; the border darkens while
   the list is open. */
.cn-select-trigger:hover:not(:disabled) {
    background: color-mix(in srgb, var(--text-primary) 5%, transparent);
}
.cn-select-trigger:active:not(:disabled) {
    background: color-mix(in srgb, var(--text-primary) 10%, transparent);
}
.cn-select-trigger[data-state="open"]:not(:disabled) {
    border-color: var(--border-hover);
}
.cn-combobox-box:disabled { opacity: 0.5; }
/* No value chosen, or disabled: the shown text, which inherits, dims. */
.cn-select-trigger[data-placeholder] { color: var(--text-tertiary); }
.cn-select-trigger:disabled {
    color: var(--text-tertiary);
    box-shadow: none;
    background: var(--input-bg-disabled);
    border-color: var(--border-secondary);
    cursor: not-allowed;
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
    color: var(--text-primary);
    box-shadow: var(--shadow-md);
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
.cn-select-item:hover:not(:disabled) {
    background: var(--accent-subtle);
}
/* `--selection` (~24 % alpha accent on Hybrid, 20 % on macOS) vs hover's
   `--accent-subtle` (~10 % alpha) so the currently-chosen row is
   visibly distinct from hovered rows and from the panel itself. */
.cn-select-item--selected {
    background: var(--selection);
    color: var(--accent);
}
.cn-select-item:disabled {
    color: var(--text-tertiary);
    background: transparent;
    cursor: not-allowed;
}
.cn-select-trigger > .cn-chevron { color: var(--text-tertiary); }
.cn-dropdown-trigger > .cn-chevron { color: var(--text-secondary); }

/* ============================================================================
   Slider
   ============================================================================ */

/* A fill, the thumb and the rest of the track in a row. The thumb grows
   under the pointer and more while held; a disabled slider mutes every part
   instead of fading. */
.cn-slider {
    border-radius: var(--radius-full);
    cursor: pointer;
    outline: 2px solid transparent;
    outline-offset: 2px;
}
.cn-slider[data-focus-visible] { outline-color: var(--focus-ring); }
.cn-slider-fill {
    background: var(--cn-slider-fill-bg, var(--primary));
    border-radius: var(--radius-full) 0 0 var(--radius-full);
}
.cn-slider-rest {
    background: var(--cn-slider-track-bg, var(--border));
    border-radius: 0 var(--radius-full) var(--radius-full) 0;
}
.cn-slider-thumb {
    background: var(--cn-slider-thumb-bg, var(--text-inverse));
    border: 2px solid var(--primary);
    border-radius: var(--radius-full);
    box-shadow: var(--shadow-sm);
    transition: transform var(--duration-fast) var(--ease-state),
        border-color var(--duration-fast) var(--ease-state);
}
.cn-slider:hover:not(:disabled) > .cn-slider-thumb {
    transform: scale(1.1);
    box-shadow: var(--shadow-md);
    border-color: var(--primary-hover);
}
.cn-slider:active:not(:disabled) > .cn-slider-thumb,
.cn-slider[data-state="dragging"] > .cn-slider-thumb {
    transform: scale(1.2);
}
.cn-slider:disabled { cursor: not-allowed; }
.cn-slider:disabled > .cn-slider-rest { background: var(--input-bg-disabled); }
.cn-slider:disabled > .cn-slider-fill { background: var(--border-secondary); }
.cn-slider:disabled > .cn-slider-thumb {
    background: var(--input-bg-disabled);
    border: 1px solid var(--border-secondary);
    box-shadow: none;
}
.cn-slider-label { color: var(--text-primary); }
.cn-slider-value { color: var(--text-secondary); }
.cn-slider-field[data-disabled] .cn-slider-label,
.cn-slider-field[data-disabled] .cn-slider-value { color: var(--text-tertiary); }

/* ============================================================================
   Progress
   ============================================================================ */

/* A track with a bar grown to the value, easing to each new one. An
   indeterminate bar covers part of the track and pulses. */
.cn-progress {
    background: var(--cn-progress-track, var(--border));
    border-radius: var(--radius-full);
}
.cn-progress-bar {
    background: var(--cn-progress-bar, var(--primary));
    border-radius: var(--radius-full);
    transition: flex-grow var(--duration-slow) var(--ease-nav);
}
@keyframes cn-progress-pulse {
    from { opacity: 0.35; }
    to { opacity: 1; }
}
.cn-progress[data-indeterminate] > .cn-progress-bar {
    animation: cn-progress-pulse var(--duration-slowest) var(--ease-in-out) infinite alternate;
}

/* ============================================================================
   Chart
   ============================================================================ */

/* A plot drawn on a canvas, with its labels, tooltip and legend as parts. */
.cn-chart { gap: 12px; }
.cn-chart-axis { font-size: var(--text-xs); color: var(--text-tertiary); }
.cn-chart-axis[data-hidden] { opacity: 0; }
.cn-chart-legend {
    gap: 16px;
    font-size: var(--text-xs);
    color: var(--text-secondary);
}
.cn-chart-legend-item { gap: 6px; }
.cn-chart-swatch { border-radius: 3px; }
.cn-chart-c1 { background: var(--chart-1); }
.cn-chart-c2 { background: var(--chart-2); }
.cn-chart-c3 { background: var(--chart-3); }
.cn-chart-c4 { background: var(--chart-4); }
.cn-chart-c5 { background: var(--chart-5); }
.cn-chart-tooltip {
    gap: 6px;
    padding: 8px 10px;
    min-width: 128px;
    background: var(--surface-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-md);
    font-size: var(--text-xs);
}
.cn-chart-tooltip-title { font-weight: 500; color: var(--text-primary); }
.cn-chart-tooltip-row { gap: 8px; }
.cn-chart-tooltip-name { flex-grow: 1; color: var(--text-secondary); }
.cn-chart-tooltip-value { margin-left: 12px; font-weight: 500; color: var(--text-primary); }
.cn-chart-band-label { font-size: 10px; color: var(--text-secondary); }
.cn-chart-note { font-size: var(--text-xs); font-weight: 500; color: var(--text-secondary); }
.cn-chart-note[data-tone="bad"] { color: var(--error); }
.cn-chart-note[data-tone="good"] { color: var(--success); }
.cn-chart-pie-total { font-size: var(--text-2xl); font-weight: 700; color: var(--text-primary); }
.cn-chart-pie-caption { font-size: var(--text-xs); color: var(--text-secondary); }

/* ============================================================================
   Avatar
   ============================================================================ */

.cn-avatar {
    background: var(--cn-avatar-bg, var(--surface));
    color: var(--cn-avatar-fg, var(--text-primary));
    border-radius: var(--radius-full);
    overflow: hidden;
}
.cn-avatar-placeholder { color: var(--text-tertiary); }
.cn-avatar-status[data-status="online"] { background: var(--success); }
.cn-avatar-status[data-status="offline"] { background: var(--text-tertiary); }
.cn-avatar-status[data-status="away"] { background: var(--warning); }
.cn-avatar-status[data-status="busy"] { background: var(--error); }
/* Avatars in a group are ringed so they read apart where they overlap. */
.cn-avatar-group .cn-avatar { border-color: var(--border-secondary); }
.cn-avatar-group__more {
    background: var(--surface-elevated);
    border-color: var(--border-secondary);
    color: var(--text-tertiary);
}
.cn-avatar--square {
    border-radius: var(--radius-default);
}

/* ============================================================================
   Spinner
   ============================================================================ */

/* A faint track with a half-circle arc going round, a turn a second. */
.cn-spinner {
    color: var(--cn-spinner-color, var(--primary));
}
.cn-spinner__track { border-color: var(--cn-spinner-track, var(--border)); }
.cn-spinner__arc {
    border-color: var(--cn-spinner-color, var(--primary));
    animation: cn-spinner-turn 1000ms linear infinite;
}
@keyframes cn-spinner-turn {
    from { transform: rotate(0deg); }
    to   { transform: rotate(360deg); }
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
    box-shadow: var(--shadow-sm);
    /* Fades in, growing out of the edge that faces its trigger. */
    animation: cn-tooltip-enter var(--duration-fast) var(--ease-out);
    transform-origin: bottom center;
}

.cn-tooltip--bottom { transform-origin: top center; }
.cn-tooltip--left { transform-origin: right center; }
.cn-tooltip--right { transform-origin: left center; }

@keyframes cn-tooltip-enter {
    from { opacity: 0; transform: scale(0.96); }
    to   { opacity: 1; transform: scale(1); }
}

/* ============================================================================
   Dialog
   ============================================================================ */

.cn-dialog {
    /* Modal → elevation 2 (lifts above surrounding cards) */
    background: var(--cn-dialog-bg, var(--surface-elevated));
    border: 1px solid var(--cn-dialog-border, var(--border));
    border-radius: var(--radius-xl);
    color: var(--text-primary);
    box-shadow: var(--cn-dialog-shadow, var(--shadow-xl));
    padding: var(--space-6);
    gap: var(--space-4);
    /* Enter/exit motion is driven by the OverlayBuilder motion_enter /
       motion_exit; no CSS keyframe here. */
}
.cn-dialog-title { color: var(--text-primary); }
.cn-dialog-description { color: var(--text-secondary); }

/* ============================================================================
   Drawer
   ============================================================================ */

.cn-drawer {
    /* Modal overlay → elevation 2. Enter/exit motion is driven by the
       DrawerBuilder motion_enter / motion_exit. */
    background: var(--cn-drawer-bg, var(--surface-elevated));
    border: 1px solid var(--cn-drawer-border, var(--border));
}
.cn-drawer, .cn-sheet {
    color: var(--text-primary);
    box-shadow: var(--shadow-xl);
}
.cn-drawer-title, .cn-sheet-title { color: var(--text-primary); }
.cn-sheet-description { color: var(--text-secondary); }
.cn-drawer-separator, .cn-sheet-separator { background: var(--border); }
/* The close button: quiet until hovered. */
.cn-drawer-close, .cn-sheet-close {
    color: var(--text-secondary);
    border-radius: var(--radius-md);
    transition:
        background-color var(--duration-fast) var(--ease-state),
        color var(--duration-fast) var(--ease-state);
}
.cn-drawer-close:hover, .cn-sheet-close:hover {
    background: color-mix(in srgb, var(--text-primary) 8%, transparent);
    color: var(--text-primary);
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

/* A notification: success, warning and destructive toasts colour their
   border and icon by `data-variant`. */
.cn-toast {
    background: var(--cn-toast-bg, var(--surface));
    border: 1px solid var(--cn-toast-border, var(--border));
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-lg);
    color: var(--text-primary);
}
.cn-toast[data-variant="success"] { border-color: var(--success); }
.cn-toast[data-variant="warning"] { border-color: var(--warning); }
.cn-toast[data-variant="destructive"] { border-color: var(--error); }
.cn-toast[data-variant="success"] .cn-toast-icon { color: var(--success); }
.cn-toast[data-variant="warning"] .cn-toast-icon { color: var(--warning); }
.cn-toast[data-variant="destructive"] .cn-toast-icon { color: var(--error); }
.cn-toast-title { color: var(--text-primary); }
.cn-toast-description { color: var(--text-secondary); }
.cn-toast-close {
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
}
.cn-toast-close:hover { color: var(--text-primary); }

/* ============================================================================
   Accordion
   ============================================================================ */

.cn-accordion {
    /* Accordion outer is a card-tier container, not a modal —
       elevation 1 (--surface). */
    background: var(--cn-accordion-bg, var(--surface));
    border: 1.5px solid var(--cn-accordion-border, var(--border));
    border-radius: var(--radius-xl);
    box-shadow: var(--shadow-md);
}
.cn-accordion-separator { background: var(--cn-accordion-border, var(--border)); }
.cn-accordion-trigger > .cn-chevron { color: var(--text-secondary); }
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
    overflow: hidden;
}
/* Closed, laid out at no height and without its divider; opening and
   closing are a change of size, which the layout animation eases. */
.cn-accordion-content[data-state="closed"] { height: 0; border-top-width: 0; }

/* ============================================================================
   Breadcrumb
   ============================================================================ */

.cn-breadcrumb {
    gap: var(--space-2);
    color: var(--text-secondary);
}
.cn-breadcrumb-item { color: var(--text-secondary); }
.cn-breadcrumb-item[data-link]:hover { color: var(--text-primary); }
.cn-breadcrumb-item[aria-current="page"] {
    color: var(--text-primary);
    font-weight: 500;
}
.cn-breadcrumb-separator { color: var(--text-tertiary); }

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
    transition: background var(--duration-fast) var(--ease-state),
        border-color var(--duration-fast) var(--ease-state);
}
.cn-pagination-btn:hover { background: var(--surface-elevated); }
/* The current page keeps its fill under the pointer; a button with nowhere
   to go keeps its dimmed look. */
.cn-pagination-btn[aria-current="page"],
.cn-pagination-btn[aria-current="page"]:hover {
    background: var(--primary);
    border-color: var(--primary);
    color: var(--text-inverse);
    cursor: default;
}
.cn-pagination-btn:disabled,
.cn-pagination-btn:disabled:hover {
    background: transparent;
    opacity: 0.5;
    cursor: not-allowed;
}
.cn-pagination-ellipsis { color: var(--text-secondary); }

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
/* Inside the panel a link's title is primary and its description secondary. */
.cn-nav-menu-content .cn-nav-link { color: var(--text-primary); }
.cn-nav-link-description { color: var(--text-secondary); }

.cn-nav-menu-content {
    /* Floating overlay → elevation 2 */
    background: var(--surface-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    color: var(--text-primary);
    box-shadow: var(--shadow-md);
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

/* Collapsing eases the rail's width to its icons' and back; the content
   beside it is laid out again each frame and moves with it. What the rail
   holds stays laid out, clipped by the narrowing width: labels fade and
   group titles fold away. */
.cn-sidebar-layout { flex-direction: row; align-items: stretch; overflow: hidden; }
.cn-sidebar {
    flex-direction: column;
    flex-shrink: 0;
    gap: 2px;
    padding: 8px 0;
    overflow: hidden;
    width: var(--cn-sidebar-width, 200px);
    background: var(--cn-sidebar-bg, var(--surface));
    border-right: 1px solid var(--border);
    transition: width var(--duration-normal) var(--ease-state);
}
.cn-sidebar[data-state="collapsed"] { width: var(--cn-sidebar-collapsed-width, 41px); }
.cn-sidebar-toggle {
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-secondary);
    cursor: pointer;
    transition: background var(--duration-fast) var(--ease-state);
}
.cn-sidebar-toggle:hover { background: var(--accent-subtle); color: var(--text-primary); }
/* Points left while the rail is open and turns right while it is collapsed. */
.cn-sidebar-chevron { transition: transform var(--duration-normal) var(--ease-state); }
.cn-sidebar[data-state="collapsed"] .cn-sidebar-chevron { transform: rotate(180deg); }
.cn-sidebar-group { flex-direction: column; gap: 2px; }
.cn-sidebar-group-label {
    flex-shrink: 0;
    height: 26px;
    padding: 8px 12px 0;
    overflow: hidden;
    color: var(--text-tertiary);
    transition: height var(--duration-normal) var(--ease-state),
        padding-top var(--duration-normal) var(--ease-state),
        opacity var(--duration-fast) var(--ease-state);
}
.cn-sidebar[data-state="collapsed"] .cn-sidebar-group-label { height: 0; padding: 0 12px; opacity: 0; }
.cn-sidebar-item {
    padding: 8px 12px;
    color: var(--text-secondary);
    background: transparent;
    cursor: pointer;
    transition: background var(--duration-fast) var(--ease-state),
        color var(--duration-fast) var(--ease-state);
}
.cn-sidebar-item:hover { background: var(--accent-subtle); color: var(--text-primary); }
.cn-sidebar-item[data-active], .cn-sidebar-item[data-active]:hover {
    background: var(--surface-elevated);
    color: var(--text-primary);
}
.cn-sidebar-item[data-active] > .cn-sidebar-icon { color: var(--primary); }
.cn-sidebar-label { transition: opacity var(--duration-fast) var(--ease-state); }
.cn-sidebar[data-state="collapsed"] .cn-sidebar-label { opacity: 0; }
.cn-sidebar-inset { flex-grow: 1; flex-direction: column; min-width: 0; }

/* ============================================================================
   Scroll Area
   ============================================================================ */

.cn-scroll-area {
    overflow: hidden;
}

/* ============================================================================
   Menus: the panel, rows, shortcuts, submenu chevrons and separators every
   cn menu is built from. The dropdown, context and menubar menus add their
   own class for their enter animation and row padding.
   ============================================================================ */

/* No padding: rows run to the rounded edge, which clips their highlight. */
.cn-menu {
    background: var(--surface-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-default);
    color: var(--text-primary);
    box-shadow: var(--shadow-md);
}
/* No transition on rows: sweeping the pointer across them would leave several
   half-lit at once. */
.cn-menu-item {
    color: var(--text-primary);
    font-size: var(--text-sm);
    cursor: pointer;
}
/* `--accent-subtle`, since the panel already sits at `--surface-elevated`. */
.cn-menu-item:hover:not(:disabled),
.cn-dropdown-item--active {
    background: var(--accent-subtle);
}
.cn-menu-item:disabled {
    opacity: 0.5;
    cursor: not-allowed;
}
.cn-menu-shortcut { color: var(--text-secondary); letter-spacing: 0.05em; }
.cn-menu-chevron { color: var(--text-tertiary); }
.cn-menu-separator { background: var(--border); margin: var(--space-1) 0; }

/* ============================================================================
   Dropdown Menu
   ============================================================================ */

.cn-dropdown-menu {
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
}
.cn-dropdown-item--destructive {
    color: var(--error);
}

/* ============================================================================
   Context Menu
   ============================================================================ */

.cn-context-menu {
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
    padding: var(--space-2) var(--space-3);
}

/* ============================================================================
   Popover
   ============================================================================ */

.cn-popover-content {
    /* Floating overlay → elevation 2 */
    background: var(--surface-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    color: var(--text-primary);
    box-shadow: var(--shadow-md);
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
    color: var(--text-primary);
    box-shadow: var(--shadow-md);
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

/* A closed group is at no height, the change eased by its layout animation,
   and its item's chevron turns down while open. Guides show along groups
   while the tree has `data-guides`. */
.cn-tree { font-size: var(--text-sm); }
.cn-tree-row {
    border-radius: var(--radius-md);
    color: var(--text-secondary);
    background: transparent;
    cursor: pointer;
    outline: 0 solid var(--focus-ring);
    outline-offset: -2px;
    transition: background var(--duration-fast) var(--ease-state),
        color var(--duration-fast) var(--ease-state);
}
.cn-tree-row:hover { background: var(--accent-subtle); color: var(--text-primary); }
.cn-tree-row[data-selected], .cn-tree-row[data-selected]:hover {
    background: color-mix(in srgb, var(--primary) 24%, transparent);
    color: var(--text-primary);
}
.cn-tree-row[data-focus-visible] { outline-width: 2px; }
.cn-tree-row[data-diff="added"] { color: var(--success); }
.cn-tree-row[data-diff="removed"] { color: var(--error); }
.cn-tree-row[data-diff="modified"] { color: var(--warning); }
.cn-tree-chevron {
    color: var(--text-tertiary);
    transition: transform var(--duration-normal) var(--ease-state);
}
.cn-tree-item[data-state="open"] > .cn-tree-row > .cn-tree-chevron { transform: rotate(90deg); }
.cn-tree-icon { color: var(--text-tertiary); }
.cn-tree-row[data-selected] > .cn-tree-icon { color: var(--primary); }
.cn-tree-group { overflow: hidden; }
.cn-tree-group[data-state="closed"] { height: 0; }
.cn-tree-guide { background: transparent; }
.cn-tree[data-guides] .cn-tree-guide { background: var(--border); }

/* ============================================================================
   Overlay motion
   ============================================================================ */

/* Dialogs, sheets, drawers and toasts enter and leave by these rules. Each one's
   wrapper carries `data-motion="enter"`, then `"exit"` while it closes, and
   stays until its exit has played; the backdrop behind it, `.overlay-backdrop`,
   fades with it. A dialog rises into place; a sheet or drawer slides from
   beyond its edge by its own size. */
@keyframes cn-rise-in {
    from { opacity: 0; transform: translateY(-4px) scale(0.96); }
    to { opacity: 1; transform: translateY(0) scale(1); }
}
@keyframes cn-rise-out {
    from { opacity: 1; transform: translateY(0) scale(1); }
    to { opacity: 0; transform: translateY(-4px) scale(0.96); }
}
@keyframes cn-fade-in { from { opacity: 0; } to { opacity: 1; } }
@keyframes cn-fade-out { from { opacity: 1; } to { opacity: 0; } }
@keyframes cn-slide-from-right {
    from { transform: translateX(100%); }
    to { transform: translateX(0); }
}
@keyframes cn-slide-to-right {
    from { transform: translateX(0); }
    to { transform: translateX(100%); }
}
@keyframes cn-slide-from-left {
    from { transform: translateX(-100%); }
    to { transform: translateX(0); }
}
@keyframes cn-slide-to-left {
    from { transform: translateX(0); }
    to { transform: translateX(-100%); }
}
@keyframes cn-slide-from-bottom {
    from { transform: translateY(100%); }
    to { transform: translateY(0); }
}
@keyframes cn-slide-to-bottom {
    from { transform: translateY(0); }
    to { transform: translateY(100%); }
}
@keyframes cn-slide-from-top {
    from { transform: translateY(-100%); }
    to { transform: translateY(0); }
}
@keyframes cn-slide-to-top {
    from { transform: translateY(0); }
    to { transform: translateY(-100%); }
}
.cn-dialog-motion[data-motion="enter"] { animation: cn-rise-in var(--duration-normal) var(--ease-sheet); }
.cn-dialog-motion[data-motion="exit"] { animation: cn-rise-out var(--duration-fast) var(--ease-in) forwards; }
.cn-sheet-motion-right[data-motion="enter"] { animation: cn-slide-from-right var(--duration-slow) var(--ease-sheet); }
.cn-sheet-motion-right[data-motion="exit"] { animation: cn-slide-to-right var(--duration-normal) var(--ease-sheet) forwards; }
.cn-sheet-motion-left[data-motion="enter"] { animation: cn-slide-from-left var(--duration-slow) var(--ease-sheet); }
.cn-sheet-motion-left[data-motion="exit"] { animation: cn-slide-to-left var(--duration-normal) var(--ease-sheet) forwards; }
.cn-sheet-motion-bottom[data-motion="enter"] { animation: cn-slide-from-bottom var(--duration-slow) var(--ease-sheet); }
.cn-sheet-motion-bottom[data-motion="exit"] { animation: cn-slide-to-bottom var(--duration-normal) var(--ease-sheet) forwards; }
.cn-sheet-motion-top[data-motion="enter"] { animation: cn-slide-from-top var(--duration-slow) var(--ease-sheet); }
.cn-sheet-motion-top[data-motion="exit"] { animation: cn-slide-to-top var(--duration-normal) var(--ease-sheet) forwards; }
.cn-drawer-motion-right[data-motion="enter"] { animation: cn-slide-from-right var(--duration-slow) var(--ease-sheet); }
.cn-drawer-motion-right[data-motion="exit"] { animation: cn-slide-to-right var(--duration-normal) var(--ease-in) forwards; }
.cn-drawer-motion-left[data-motion="enter"] { animation: cn-slide-from-left var(--duration-slow) var(--ease-sheet); }
.cn-drawer-motion-left[data-motion="exit"] { animation: cn-slide-to-left var(--duration-normal) var(--ease-in) forwards; }
/* A toast slides in from beyond its side and back out, without fading:
   its shadow would show through its fill part-way. */
.cn-toast-motion-right[data-motion="enter"] { animation: cn-slide-from-right var(--duration-normal) var(--ease-sheet); }
.cn-toast-motion-right[data-motion="exit"] { animation: cn-slide-to-right var(--duration-fast) var(--ease-in) forwards; }
.cn-toast-motion-left[data-motion="enter"] { animation: cn-slide-from-left var(--duration-normal) var(--ease-sheet); }
.cn-toast-motion-left[data-motion="exit"] { animation: cn-slide-to-left var(--duration-fast) var(--ease-in) forwards; }
.overlay-backdrop[data-motion="enter"] { animation: cn-fade-in var(--duration-normal) var(--ease-out); }
.overlay-backdrop[data-motion="exit"] { animation: cn-fade-out var(--duration-fast) var(--ease-in) forwards; }

/* ============================================================================
   Calendar
   ============================================================================ */

/* A month and year to pick between two quiet buttons, the week's days, and
   six weeks of days in a grid of seven. */
.cn-calendar {
    flex-direction: column;
    gap: 12px;
    align-self: flex-start;
    padding: 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--surface);
    color: var(--text-primary);
    font-family: var(--font-sans);
}
.cn-calendar-header { flex-direction: row; align-items: center; justify-content: space-between; }
.cn-calendar-caption { flex-direction: row; align-items: center; gap: 4px; }
.cn-calendar-caption .cn-select-trigger { height: 28px; padding: 0 6px; }
.cn-calendar-caption .cn-select-item { justify-content: center; }
.cn-calendar-nav {
    width: 28px;
    height: 28px;
    align-items: center;
    justify-content: center;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-secondary);
    cursor: pointer;
    transition: background var(--duration-fast) var(--ease-state),
        color var(--duration-fast) var(--ease-state);
}
.cn-calendar-nav:hover {
    background: color-mix(in srgb, var(--text-primary) 6%, transparent);
    color: var(--text-primary);
}
.cn-calendar-nav:disabled { opacity: 0.4; cursor: default; }
.cn-calendar-body { flex-direction: column; gap: 2px; }
.cn-calendar-weekdays { display: grid; grid-template-columns: repeat(7, 36px); }
/* The days, clipped a little outside them so focus rings show: a month
   turning slides the new one in from its side, pushing the old out the
   other. */
.cn-calendar-months { overflow: hidden; margin: -4px; padding: 4px; }
.cn-calendar-grid { display: grid; grid-template-columns: repeat(7, 36px); row-gap: 2px; }
.cn-calendar-grid[data-enter="next"] { animation: cn-calendar-from-next var(--duration-normal) var(--ease-nav); }
.cn-calendar-grid[data-enter="prev"] { animation: cn-calendar-from-prev var(--duration-normal) var(--ease-nav); }
.cn-calendar-grid[data-leaving="next"] { animation: cn-calendar-to-prev var(--duration-normal) var(--ease-nav) forwards; }
.cn-calendar-grid[data-leaving="prev"] { animation: cn-calendar-to-next var(--duration-normal) var(--ease-nav) forwards; }
@keyframes cn-calendar-from-next {
    from { transform: translateX(260px); opacity: 0.4; }
    to { transform: translateX(0); opacity: 1; }
}
@keyframes cn-calendar-from-prev {
    from { transform: translateX(-260px); opacity: 0.4; }
    to { transform: translateX(0); opacity: 1; }
}
@keyframes cn-calendar-to-prev {
    from { transform: translateX(0); opacity: 1; }
    to { transform: translateX(-260px); opacity: 0.4; }
}
@keyframes cn-calendar-to-next {
    from { transform: translateX(0); opacity: 1; }
    to { transform: translateX(260px); opacity: 0.4; }
}
.cn-calendar-weekday { height: 28px; align-items: center; justify-content: center; color: var(--text-secondary); }
.cn-calendar-day {
    width: 36px;
    height: 36px;
    align-items: center;
    justify-content: center;
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--text-primary);
    cursor: pointer;
    outline: 0 solid var(--focus-ring);
    outline-offset: 1px;
    transition: background var(--duration-fast) var(--ease-state),
        color var(--duration-fast) var(--ease-state);
}
.cn-calendar-day:hover { background: color-mix(in srgb, var(--text-primary) 7%, transparent); }
.cn-calendar-day:active { transform: scale(0.94); }
.cn-calendar-day[data-outside] { color: var(--text-tertiary); }
.cn-calendar-day[data-today] { background: var(--accent-subtle); color: var(--accent); font-weight: 600; }
.cn-calendar-day[data-selected], .cn-calendar-day[data-selected]:hover {
    background: var(--primary);
    color: var(--text-inverse);
}
.cn-calendar-day[data-focus-visible] { outline-width: 2px; }
.cn-calendar-day:disabled { opacity: 0.4; cursor: default; }
.cn-calendar-event {
    width: 4px;
    height: 4px;
    border-radius: 9999px;
    background: var(--primary);
}
.cn-calendar-event--icon { width: 8px; height: 8px; border-radius: 0; background: transparent; color: var(--primary); }
.cn-calendar-day[data-selected] .cn-calendar-event { background: var(--text-inverse); }
.cn-calendar-day[data-selected] .cn-calendar-event--icon { background: transparent; color: var(--text-inverse); }

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
.cn-resizable-line {
    background: var(--border);
    transition: background var(--duration-fast) var(--ease-state);
}
.cn-resizable-handle:hover > .cn-resizable-line,
.cn-resizable-handle[data-state="dragging"] > .cn-resizable-line {
    background: var(--primary);
}

/* ============================================================================
   Collapsible
   ============================================================================ */

/* Closed, the content is laid out at no height; opening and closing are a
   change of size, which the content's layout animation eases. */
.cn-collapsible-content { overflow: hidden; }
.cn-collapsible-content[data-state="closed"] { height: 0; }
.cn-collapsible-section {
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
}
.cn-collapsible-trigger {
    cursor: pointer;
    color: var(--text-primary);
    border-radius: var(--radius-md);
    transition: background-color var(--duration-fast) var(--ease-state);
}
.cn-collapsible-trigger:hover,
.cn-collapsible-trigger:active {
    background: color-mix(in srgb, var(--surface-elevated) 50%, transparent);
}
.cn-collapsible-trigger > .cn-chevron { color: var(--text-secondary); }

/* ============================================================================
   Combobox
   ============================================================================ */

/* The dropdown menu's default trigger: filled as the outline button is,
   tinted on hover and press. */
.cn-dropdown-trigger {
    color: var(--text-primary);
    background: transparent;
    border-color: var(--border);
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-sm);
    transition: background-color var(--duration-fast) var(--ease-state);
}
.cn-dropdown-trigger:hover {
    background: color-mix(in srgb, var(--text-primary) 5%, transparent);
}
.cn-dropdown-trigger:active {
    background: color-mix(in srgb, var(--text-primary) 10%, transparent);
}

.cn-combobox-trigger {
    color: var(--text-primary);
    background: var(--surface);
    box-shadow: var(--shadow-sm);
    border-color: var(--border);
    border-radius: var(--radius-default);
    cursor: pointer;
    transition:
        background-color var(--duration-fast) var(--ease-state),
        border-color var(--duration-fast) var(--ease-state);
}
.cn-combobox-trigger:hover:not(:disabled) { border-color: var(--border-hover); }
/* Open wins over hover; the hover variant is listed because rules with a
   pointer pseudo-class apply after plain ones. */
.cn-combobox-trigger[data-state="open"]:not(:disabled),
.cn-combobox-trigger[data-state="open"]:hover:not(:disabled) { border-color: var(--border-focus); }
/* Nothing chosen, or disabled: the shown text, which inherits, dims. */
.cn-combobox-trigger[data-placeholder] { color: var(--text-tertiary); }
.cn-combobox-trigger:disabled {
    color: var(--text-tertiary);
    box-shadow: none;
    background: var(--input-bg-disabled);
    border-color: var(--border-secondary);
    cursor: not-allowed;
}
.cn-combobox-content {
    /* Shared overlay-menu chrome with select / dropdown / context.
       No outer padding — see `.cn-select-content`. */
    background: var(--surface-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-default);
    color: var(--text-primary);
    box-shadow: var(--shadow-md);
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
.cn-combobox-item:hover:not(:disabled) {
    background: var(--accent-subtle);
}
.cn-combobox-item--selected {
    background: var(--selection);
    color: var(--accent);
}
.cn-combobox-item:disabled {
    color: var(--text-tertiary);
    background: transparent;
    cursor: not-allowed;
}
.cn-combobox-empty { color: var(--text-tertiary); }
.cn-combobox-search-row { border-bottom: 1px solid var(--border); }
.cn-combobox-trigger > .cn-chevron { color: var(--text-tertiary); }

/* ============================================================================
   Table (shadcn `<Table>` family)
   ============================================================================ */

/* The table's text is the theme's, which every cell inherits; header and
   caption text is muted, header and footer sit on the overlay surface. */
.cn-table {
    color: var(--text-primary);
    border: 1px solid var(--cn-table-border, var(--border));
    border-radius: var(--cn-table-radius, var(--radius-md));
    font-size: var(--text-sm);
}
.cn-table-header { background: var(--surface-overlay); }
.cn-table-footer {
    font-weight: 500;
    background: var(--surface-overlay);
    border-top: 1px solid var(--cn-table-border, var(--border));
}

/* Rows are separated by a border; the body's last row has none, so the
   closing edge sits flush with the outer radius. */
.cn-table-row {
    border-bottom: 1px solid var(--cn-table-border, var(--border));
    transition: background var(--duration-fast) var(--ease-state);
}
.cn-table-body .cn-table-row:last-child {
    border-bottom: none;
}
.cn-table-body > .cn-table-row:hover {
    background: var(--cn-table-row-hover, color-mix(in srgb, var(--text-primary) 4%, transparent));
}
.cn-table-row[data-state="selected"],
.cn-table-row[data-state="selected"]:hover {
    background: var(--cn-table-row-selected, var(--selection));
}

.cn-table-head {
    color: var(--text-secondary);
    font-weight: 500;
    font-size: var(--text-sm);
}
.cn-table-caption {
    color: var(--text-secondary);
    font-size: var(--text-sm);
}

/* Time and colour pickers */
:root {
    --cn-color-picker-checker-light: #ffffff;
    --cn-color-picker-checker-dark: #b8b8b8;
}
.cn-time-picker, .cn-color-picker-trigger, .cn-color-picker-panel {
    font-family: var(--font-sans);
    color: var(--text-primary);
}
.cn-time-picker {
    gap: 2px;
    border: 1px solid var(--cn-time-picker-border, var(--border));
    border-radius: var(--cn-time-picker-radius, var(--radius-md));
    background: var(--cn-time-picker-bg, var(--input-bg));
    padding: 2px;
}
.cn-time-picker-segment {
    width: 40px; height: 36px; padding: 0 4px;
    border: 0; border-radius: var(--radius-sm);
    background: transparent; color: var(--text-primary);
    font-family: var(--font-mono); font-size: 14px;
    transition: background var(--duration-fast) var(--ease-state);
}
.cn-time-picker-segment:hover,
.cn-time-picker-segment:focus {
    background: var(--cn-time-picker-field-hover, var(--surface-overlay));
}
.cn-time-picker-segment:focus {
    outline: 2px solid var(--ring); outline-offset: -2px;
}
.cn-time-picker-segment--sm { width: 36px; height: 28px; font-size: 12px; }
.cn-time-picker-segment--lg { width: 48px; height: 44px; font-size: 16px; }
.cn-time-picker-separator { color: var(--text-secondary); }
.cn-time-picker-period {
    padding: 6px; border-radius: var(--radius-sm); cursor: pointer;
    font-size: 13px; color: var(--text-primary);
    transition: background var(--duration-fast) var(--ease-state);
}
.cn-time-picker-period:hover { background: var(--surface-overlay); }
.cn-time-picker-period:focus-visible { outline: 2px solid var(--ring); }
.cn-time-picker:disabled { opacity: 0.5; }
.cn-time-picker-segment:disabled,
.cn-time-picker-period:disabled { cursor: not-allowed; background: transparent; opacity: 1; }

.cn-color-picker-trigger {
    gap: 8px; padding: 6px 10px;
    border: 1px solid var(--cn-color-picker-border, var(--border));
    border-radius: var(--cn-color-picker-radius, var(--radius-md));
    background: var(--cn-color-picker-bg, var(--input-bg)); cursor: pointer;
    transition: background var(--duration-fast) var(--ease-state), border-color var(--duration-fast) var(--ease-state);
}
.cn-color-picker-trigger:hover { background: var(--surface-overlay); }
.cn-color-picker-trigger:focus-visible { outline: 2px solid var(--ring); outline-offset: 2px; }
.cn-color-picker-trigger:disabled { opacity: 0.5; cursor: not-allowed; background: var(--input-bg); }
.cn-color-picker-value { font-family: var(--font-mono); font-size: 13px; }
.cn-color-picker-swatch {
    width: 24px; height: 24px; flex-shrink: 0;
    border: 1px solid var(--border); border-radius: var(--radius-sm);
}
.cn-color-picker-panel {
    width: 256px; padding: 16px; gap: 8px;
    border: 1px solid var(--cn-color-picker-border, var(--border));
    border-radius: var(--cn-color-picker-radius, var(--radius-md));
    background: var(--cn-color-picker-panel-bg, var(--surface-elevated));
    box-shadow: var(--shadow-md);
}
.cn-color-picker-panel:disabled { opacity: 0.5; }
.cn-color-picker-sv { height: 160px; cursor: crosshair; border-radius: var(--radius-sm); }
.cn-color-picker-white { background: linear-gradient(to right, #ffffff, #ffffff00); border-radius: var(--radius-sm); }
.cn-color-picker-black { background: linear-gradient(to bottom, #00000000, #000000); border-radius: var(--radius-sm); }
.cn-color-picker-hue {
    height: 16px; cursor: pointer; border-radius: 999px;
}
.cn-color-picker-hue-segment[data-sector="0"] { background: linear-gradient(to right, #ff0000, #ffff00); border-radius: 999px 0 0 999px; }
.cn-color-picker-hue-segment[data-sector="1"] { background: linear-gradient(to right, #ffff00, #00ff00); }
.cn-color-picker-hue-segment[data-sector="2"] { background: linear-gradient(to right, #00ff00, #00ffff); }
.cn-color-picker-hue-segment[data-sector="3"] { background: linear-gradient(to right, #00ffff, #0000ff); }
.cn-color-picker-hue-segment[data-sector="4"] { background: linear-gradient(to right, #0000ff, #ff00ff); }
.cn-color-picker-hue-segment[data-sector="5"] { background: linear-gradient(to right, #ff00ff, #ff0000); border-radius: 0 999px 999px 0; }
.cn-color-picker-alpha { height: 16px; cursor: pointer; background: var(--surface-overlay); border-radius: 999px; }
.cn-color-picker-alpha > div { border-radius: 999px; }
.cn-color-picker-sv:focus-visible, .cn-color-picker-hue:focus-visible, .cn-color-picker-alpha:focus-visible,
.cn-color-picker-sv[data-focus-visible], .cn-color-picker-hue[data-focus-visible], .cn-color-picker-alpha[data-focus-visible] {
    outline: 2px solid var(--ring); outline-offset: 3px;
}
.cn-color-picker-sv:disabled, .cn-color-picker-hue:disabled, .cn-color-picker-alpha:disabled { cursor: not-allowed; }
.cn-color-picker-marker {
    width: 10px; height: 10px; border: 2px solid var(--cn-color-picker-marker, #ffffff);
    border-radius: 999px; box-shadow: 0 0 2px 1px #00000080;
    left: -5px; top: -5px;
}
.cn-color-picker-label { font-size: 12px; color: var(--text-secondary); }
.cn-color-picker-entry { gap: 8px; }
.cn-color-picker-hex {
    width: 100%; height: 32px; font-family: var(--font-mono); font-size: 13px;
    color: var(--text-primary); background: var(--input-bg);
    border: 1px solid var(--border); border-radius: var(--radius-sm);
}
.cn-color-picker-hex:focus { border-color: var(--ring); outline: 2px solid var(--ring); outline-offset: -2px; }
.cn-color-picker-hex--invalid { border-color: var(--error); }
.cn-color-picker-swatches { gap: 8px; }
.cn-color-picker-preset {
    width: 24px; height: 24px; border-radius: 999px; cursor: pointer;
    border: 1px solid var(--border);
}
.cn-color-picker-preset[data-state="selected"] { outline: 2px solid var(--ring); outline-offset: 2px; }
.cn-color-picker-preset:focus-visible { outline: 2px solid var(--ring); outline-offset: 2px; }
.cn-color-picker-preset:disabled { cursor: not-allowed; }
"#,
    "\n}\n"
);
