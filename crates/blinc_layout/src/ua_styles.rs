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

/* The box is the element; fill, border colour, hover and the mark follow its
   `checked` and `disabled` attributes. Border width is set in Rust because it
   varies with the size. */
.blinc-checkbox {
    background: var(--input-bg);
    border-color: var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-inverse);
    cursor: pointer;
    transition:
        background-color var(--duration-fast) var(--ease-state),
        border-color var(--duration-fast) var(--ease-state),
        transform var(--duration-fast) var(--ease-state);
}
.blinc-checkbox:hover:not(:disabled):not(:checked) { border-color: var(--border-hover); }
.blinc-checkbox:hover:not(:disabled) { transform: scale(1.05); }
.blinc-checkbox:checked {
    background: var(--primary);
    border-color: var(--primary);
}
/* A disabled box takes the disabled palette shared with the button, select
   and switch. */
.blinc-checkbox:disabled {
    background: var(--input-bg-disabled);
    border-color: var(--border-secondary);
    opacity: 0.5;
    cursor: not-allowed;
}
.blinc-checkbox-mark {
    opacity: 0;
    transition: opacity var(--duration-fast) var(--ease-state);
}
.blinc-checkbox:checked > .blinc-checkbox-mark { opacity: 1; }
.blinc-checkbox-label { color: var(--text-primary); }
.blinc-checkbox-row:disabled > .blinc-checkbox-label { color: var(--text-tertiary); }

/* The ring is the element; its border colour, the dot and the disabled fill
   follow its `checked` and `disabled` attributes. Border width is set in Rust
   because it varies with the size. */
.blinc-radio {
    background: transparent;
    border-color: var(--border-secondary);
    border-radius: var(--radius-full);
    cursor: pointer;
    transition:
        border-color var(--duration-fast) var(--ease-state),
        transform var(--duration-fastest) var(--ease-state);
}
.blinc-radio:hover:not(:disabled) {
    border-color: var(--primary);
    transform: scale(1.05, 1.05);
}
.blinc-radio:checked { border-color: var(--primary); }
.blinc-radio:disabled {
    background: var(--input-bg-disabled);
    border-color: var(--border-secondary);
    opacity: 0.5;
    cursor: not-allowed;
}
.blinc-radio-dot {
    background: var(--primary);
    border-radius: var(--radius-full);
    opacity: 0;
    transition:
        opacity var(--duration-fast) var(--ease-state),
        transform var(--duration-fastest) var(--ease-state);
}
.blinc-radio:checked > .blinc-radio-dot { opacity: 1; }
.blinc-radio-label { color: var(--text-primary); }
.blinc-radio-row:disabled > .blinc-radio-label { color: var(--text-tertiary); }
.blinc-radio-group-label { color: var(--text-secondary); }
.blinc-radio-group:disabled > .blinc-radio-group-label { color: var(--text-tertiary); }
.blinc-radio:active:not(:disabled) .blinc-radio-dot { transform: scale(0.8, 0.8); }

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

/* The field is the element: hover and focus come from the pointer and the
   router's focus, `disabled` and `data-invalid` are attributes the widget
   keeps in step with its data. An invalid value keeps its error border while
   hovered or focused. */
.blinc-input {
    background: var(--input-bg);
    border-color: var(--border-secondary);
    cursor: text;
    transition:
        background-color var(--duration-fast) var(--ease-state),
        border-color var(--duration-fast) var(--ease-state);
}
.blinc-input:hover:not(:disabled) {
    background: var(--input-bg-hover);
    border-color: var(--border-hover);
}
.blinc-input:focus:not(:disabled) {
    background: var(--input-bg-focus);
    border-color: var(--border-focus);
}
.blinc-input[data-invalid]:not(:disabled) { border-color: var(--border-error); }
.blinc-input:disabled {
    background: var(--input-bg-disabled);
    border-color: var(--border-secondary);
    cursor: not-allowed;
}

/* Text and tables. */
.blinc-link { color: var(--text-link); }
.blinc-link:hover { color: var(--primary-hover); }
.blinc-blockquote {
    background: var(--surface-overlay);
    border-left: 4px solid var(--border);
}
.blinc-hr { background: var(--border); }
.blinc-list-marker { color: var(--text-tertiary); }
.blinc-task-box { border-color: var(--text-tertiary); }
.blinc-task-box[checked] { background: var(--text-tertiary); color: #ffffff; }
.blinc-thead, .blinc-tfoot { background: var(--surface-overlay); }
.blinc-th { color: var(--text-primary); }
.blinc-td { color: var(--text-secondary); }
.blinc-tr-alt { background: var(--accent-subtle); }
.blinc-table-separator { background: var(--border); }
"#,
    "}\n"
);

/// Queue [`UA_STYLES`] for the runner to register, once per process. The
/// runners call this at start-up, ahead of the theme's own CSS.
pub fn install() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| blinc_core::context_state::queue_pending_stylesheet(UA_STYLES));
}
