//! The default look of blinc_layout's own widgets, as a stylesheet.
//!
//! Rules sit in the `components` layer, so an application's rules override
//! them whatever their specificity, and a fluent value on an element wins over
//! them. Colours come from theme variables, so they follow the theme.

/// Default rules for the layout widgets.
pub const UA_STYLES: &str = concat!(
    "@layer components {\n",
    r#"
.blinc-button {
    background: var(--primary);
    color: var(--text-inverse);
    transition:
        background-color var(--duration-fast) var(--ease-state),
        color var(--duration-fast) var(--ease-state),
        border-color var(--duration-fast) var(--ease-state);
}
.blinc-button:hover:not(:disabled) { background: var(--primary-hover); }
.blinc-button:active:not(:disabled) { background: var(--primary-active); }
.blinc-button:disabled {
    background: var(--input-bg-disabled);
    color: var(--text-tertiary);
}

/* A button that stays pressed: clear when off, a faint wash of the text
   colour when on; the outline variant keeps a hairline either way. Sizes and
   variants are attributes, so one set of rules serves a toggle, and the items
   of a group that sets them for its items. */
.blinc-toggle {
    flex-direction: row;
    align-items: center;
    justify-content: center;
    gap: 6px;
    height: 36px;
    min-width: 36px;
    padding: 0 12px;
    font-size: var(--text-sm);
    font-weight: 500;
    border: 1px solid transparent;
    border-radius: var(--radius-default);
    background: transparent;
    color: var(--text-primary);
    cursor: pointer;
    transition:
        background-color var(--duration-fast) var(--ease-state),
        border-color var(--duration-fast) var(--ease-state),
        transform var(--duration-faster) var(--ease-state);
}
.blinc-toggle:hover:not(:disabled) {
    background: color-mix(in srgb, var(--text-primary) 5%, transparent);
}
.blinc-toggle:active:not(:disabled) {
    background: color-mix(in srgb, var(--text-primary) 10%, transparent);
    transform: scale(0.97);
}
.blinc-toggle[data-state="on"] {
    background: color-mix(in srgb, var(--text-primary) 10%, var(--background));
}
.blinc-toggle[data-state="on"]:hover:not(:disabled) {
    background: color-mix(in srgb, var(--text-primary) 14%, var(--background));
}
.blinc-toggle:disabled { opacity: 0.5; cursor: not-allowed; }
.blinc-toggle[data-variant="outline"],
.blinc-toggle-group[data-variant="outline"] > .blinc-toggle {
    border-color: var(--border-secondary);
}
.blinc-toggle[data-size="sm"],
.blinc-toggle-group[data-size="sm"] > .blinc-toggle {
    height: 32px;
    min-width: 32px;
    padding: 0 8px;
    font-size: var(--text-xs);
    border-radius: var(--radius-sm);
}
.blinc-toggle[data-size="lg"],
.blinc-toggle-group[data-size="lg"] > .blinc-toggle {
    height: 40px;
    min-width: 40px;
    padding: 0 16px;
    font-size: var(--text-base);
}
.blinc-toggle > svg { flex-shrink: 0; }
.blinc-toggle-group {
    flex-direction: row;
    align-items: center;
    gap: 4px;
}
"#,
    "}\n"
);

/// Queue [`UA_STYLES`] for the runner to register, once per process. The
/// runners call this at start-up, ahead of the theme's own CSS.
pub fn install() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| blinc_core::context_state::queue_pending_stylesheet(UA_STYLES));
}
