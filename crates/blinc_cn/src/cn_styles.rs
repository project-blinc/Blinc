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

/* Button: each variant's fill, text colour, border and shadow, and how they
   change on hover, press and while disabled, are rules, eased by the
   transition. Geometry tokens flow from the active theme's RadiusTokens so
   each variant (Restrained / Hybrid / Expressive) gets its own corner reach. */
.cn-button {
    border-radius: var(--radius-default);
    transition:
        background-color var(--duration-fast) var(--ease-state),
        color var(--duration-fast) var(--ease-state),
        border-color var(--duration-fast) var(--ease-state),
        box-shadow var(--duration-fast) var(--ease-state),
        transform var(--duration-faster) var(--ease-state);
}
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
.cn-input.cn-input--error:not(:disabled),
.cn-input.cn-input--error:hover:not(:disabled),
.cn-input.cn-input--error:focus:not(:disabled) {
    border-color: var(--border-error);
}
.cn-input.cn-input--error:focus:not(:disabled) {
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
.cn-textarea.cn-textarea--error:not(:disabled),
.cn-textarea.cn-textarea--error:hover:not(:disabled),
.cn-textarea.cn-textarea--error:focus:not(:disabled) {
    border-color: var(--border-error);
}
.cn-textarea.cn-textarea--error:focus:not(:disabled) {
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

/* Number input, after ashui's: a number between a - and a +, one bordered
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

/* Number gauge, after ashui's: a number over a range as a fill tinted from
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
    border-color: var(--cn-kbd-border, var(--border));
    border-radius: var(--radius-sm);
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
   transition so it spins rather than flips (ashui's chevrons do the same). */
.cn-chevron {
    transition: transform var(--duration-normal) var(--ease-state);
}
.cn-select-trigger[data-state="open"] > .cn-chevron,
.cn-combobox-trigger[data-state="open"] > .cn-chevron,
.cn-dropdown-trigger[data-state="open"] > .cn-chevron,
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
    box-shadow: var(--shadow-sm);
    cursor: pointer;
}
/* Disabled thumb tone — matches the disabled-button / disabled-input
   surface family (--input-bg-disabled). Just changing the thumb bg
   isn't enough on its own — `cn::slider` also overrides the track to
   `--input-bg-disabled` and the fill to `--border-secondary` so the
   whole control reads as inert (same approach `cn::switch` takes:
   muted track + thumb chrome that doesn't change opacity). */
.cn-slider-thumb--disabled {
    box-shadow: none;
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
/* The close button, as ashui's sheet's: quiet until hovered. */
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
.cn-dropdown-item:hover,
.cn-dropdown-item--active {
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

/* The dropdown menu's default trigger: filled as the outline button is,
   tinted on hover and press. */
.cn-dropdown-trigger {
    background: transparent;
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
"#,
    "\n}\n"
);
