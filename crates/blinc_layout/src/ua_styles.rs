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
"#,
    "}\n"
);

/// Queue [`UA_STYLES`] for the runner to register, once per process. The
/// runners call this at start-up, ahead of the theme's own CSS.
pub fn install() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| blinc_core::context_state::queue_pending_stylesheet(UA_STYLES));
}
