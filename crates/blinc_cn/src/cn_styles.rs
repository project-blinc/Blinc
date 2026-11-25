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
pub const CN_STYLES: &str = include_str!(concat!(env!("OUT_DIR"), "/cn.css"));
