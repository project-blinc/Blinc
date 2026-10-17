//! The code editor's default stylesheet.

/// The look of the editor's find and replace bar, in the theme's tokens.
///
/// The bar is built from classed elements and falls back to a plain look
/// without this. Add it to the application's CSS to get the designed one,
/// and override any rule after it:
///
/// ```ignore
/// ctx.add_css(blinc_layout::widgets::CODE_EDITOR_STYLES);
/// ```
///
/// The cn theme bundle (`cn_bundle` in blinc_cn) includes it.
pub const CODE_EDITOR_STYLES: &str = r#"
/* The code editor's find and replace bar. */
.blinc-find {
    background: var(--surface-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: 8px;
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.5);
}
.blinc-find-row { gap: 6px; }

/* A solid 1px border: opaque colours, and a whole-pixel width so it stays
   crisp (the plain input default is 1.5px). */
.blinc-find-input {
    height: 28px;
    background: var(--surface-overlay);
    border-width: 1px;
    border-color: var(--border-secondary);
    border-radius: var(--radius-md);
    color: var(--text-primary);
}
.blinc-find-input:hover { border-color: var(--text-secondary); }
.blinc-find-input:focus {
    background: var(--surface-overlay);
    border-color: var(--border-focus);
}

.blinc-find-button {
    border-radius: var(--radius-md);
    transition: background 120ms ease;
}
.blinc-find-button:hover { background: var(--border); }
.blinc-find-button:active { background: var(--border-hover); }

.blinc-find-toggle {
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    transition: background 120ms ease;
}
.blinc-find-toggle:hover { background: var(--border); color: var(--text-primary); }
.blinc-find-toggle--on,
.blinc-find-toggle--on:hover { background: var(--accent-subtle); color: var(--accent); }

.blinc-find-count { color: var(--text-secondary); padding: 0 6px; }
.blinc-find-count--none { color: var(--error); }
"#;
