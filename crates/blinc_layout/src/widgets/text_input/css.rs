//! The stylesheet's rules for a text input, per visual state.

use super::*;

/// Apply CSS stylesheet overrides to a TextInputConfig.
///
/// Resolves base styles, state-specific styles (:hover/:focus/:disabled),
/// and ::placeholder styles from the active stylesheet, then mutates the
/// config with any CSS-specified values.
pub(super) fn apply_css_overrides(
    cfg: &mut TextInputConfig,
    stylesheet: &Stylesheet,
    element_id: Option<&str>,
    css_classes: &[std::sync::Arc<str>],
    visual: &TextFieldState,
) {
    // Determine the element state for state-specific lookups
    let state = match visual {
        TextFieldState::Hovered | TextFieldState::FocusedHovered => Some(ElementState::Hover),
        TextFieldState::Focused => Some(ElementState::Focus),
        TextFieldState::Disabled => Some(ElementState::Disabled),
        TextFieldState::Idle => None,
    };

    // 1. Apply class-based styles (lowest priority — overridden by ID).
    //
    // Order for FocusedHovered: base → :hover → :focus. `:focus` must run
    // LAST so its `border-color` wins over `:hover`'s — without this, a
    // hovered focused input falls back to the hover (grey) border while
    // the user is still typing in it. The user's intent on hover-while-
    // focused is "I'm interacting with this input" → focus colour wins.
    for class in css_classes {
        if let Some(base) = stylesheet.get_class(class) {
            apply_style_to_config(cfg, base, visual);
        }
        if let Some(s) = state {
            if let Some(state_style) = stylesheet.get_class_with_state(class, s) {
                apply_style_to_config(cfg, state_style, visual);
            }
        }
        if matches!(visual, TextFieldState::FocusedHovered) {
            if let Some(s) = stylesheet.get_class_with_state(class, ElementState::Focus) {
                apply_style_to_config(cfg, s, visual);
            }
        }
    }

    // 2. Apply ID-based styles (higher priority — overrides class).
    // Same hover-then-focus ordering as the class branch above.
    if let Some(element_id) = element_id {
        if let Some(base) = stylesheet.get(element_id) {
            apply_style_to_config(cfg, base, visual);
        }
        if let Some(s) = state {
            if let Some(state_style) = stylesheet.get_with_state(element_id, s) {
                apply_style_to_config(cfg, state_style, visual);
            }
        }
        if matches!(visual, TextFieldState::FocusedHovered) {
            if let Some(focus_style) = stylesheet.get_with_state(element_id, ElementState::Focus) {
                apply_style_to_config(cfg, focus_style, visual);
            }
        }

        // 3. Apply ::placeholder style (ID-only)
        if let Some(placeholder_style) = stylesheet.get_placeholder_style(element_id) {
            if let Some(color) = placeholder_style.text_color {
                cfg.placeholder_color = color;
            }
            if let Some(color) = placeholder_style.placeholder_color {
                cfg.placeholder_color = color;
            }
        }
    }
}

/// Apply an ElementStyle to a TextInputConfig (CSS properties override config values)
pub(super) fn apply_style_to_config(
    cfg: &mut TextInputConfig,
    style: &crate::element_style::ElementStyle,
    visual: &TextFieldState,
) {
    // Background → applies to current state's bg color
    if let Some(ref bg) = style.background {
        let color = match bg {
            blinc_core::Brush::Solid(c) => *c,
            _ => return, // Gradients not supported for input bg
        };
        match visual {
            TextFieldState::Idle => cfg.bg_color = color,
            TextFieldState::Hovered => cfg.hover_bg_color = color,
            TextFieldState::Focused | TextFieldState::FocusedHovered => {
                cfg.focused_bg_color = color;
            }
            TextFieldState::Disabled => {} // Disabled bg is hardcoded
        }
    }

    // Border color
    if let Some(color) = style.border_color {
        match visual {
            TextFieldState::Idle => cfg.border_color = color,
            TextFieldState::Hovered => cfg.hover_border_color = color,
            TextFieldState::Focused | TextFieldState::FocusedHovered => {
                cfg.focused_border_color = color;
            }
            TextFieldState::Disabled => {}
        }
    }

    // Border width
    if let Some(w) = style.border_width {
        cfg.border_width = w;
    }

    // Corner radius
    if let Some(cr) = style.corner_radius {
        cfg.corner_radius = cr.top_left; // Use uniform radius
    }

    // Text color
    if let Some(color) = style.text_color {
        cfg.text_color = color;
    }

    // Font size
    if let Some(size) = style.font_size {
        cfg.font_size = size;
    }

    // Caret (cursor) color
    if let Some(color) = style.caret_color {
        cfg.cursor_color = color;
    }

    // Selection color
    if let Some(color) = style.selection_color {
        cfg.selection_color = color;
    }

    // Placeholder color
    if let Some(color) = style.placeholder_color {
        cfg.placeholder_color = color;
    }
}

/// Extract outline properties from stylesheet for the current state.
/// Returns (width, color, offset) if any outline is specified.
///
/// Walks class selectors first (lowest priority) and then `#id`
/// selectors (overrides), matching the precedence rules
/// `apply_css_overrides` uses for the rest of the input's style.
/// Without the class branch, rules like `.cn-input:focus { outline:
/// 2px solid var(--border-focus); }` from the cn stylesheet would
/// silently no-op because cn::input attaches via class, not id.
pub(super) fn extract_outline_from_stylesheet(
    stylesheet: &Stylesheet,
    element_id: Option<&str>,
    css_classes: &[std::sync::Arc<str>],
    visual: &TextFieldState,
) -> Option<(f32, Color, f32)> {
    let mut width = None;
    let mut color = None;
    let mut offset = None;

    let state = match visual {
        TextFieldState::Hovered | TextFieldState::FocusedHovered => Some(ElementState::Hover),
        TextFieldState::Focused => Some(ElementState::Focus),
        TextFieldState::Disabled => Some(ElementState::Disabled),
        TextFieldState::Idle => None,
    };

    let mut absorb = |s: &crate::element_style::ElementStyle| {
        if let Some(w) = s.outline_width {
            width = Some(w);
        }
        if let Some(c) = s.outline_color {
            color = Some(c);
        }
        if let Some(o) = s.outline_offset {
            offset = Some(o);
        }
    };

    // 1. Class-based styles (lowest priority — overridden by ID).
    // FocusedHovered order: base → :hover → :focus, so the focus ring
    // wins over hover (same rationale as apply_css_overrides above).
    for class in css_classes {
        if let Some(base) = stylesheet.get_class(class) {
            absorb(base);
        }
        if let Some(s) = state {
            if let Some(state_style) = stylesheet.get_class_with_state(class, s) {
                absorb(state_style);
            }
        }
        if matches!(visual, TextFieldState::FocusedHovered) {
            if let Some(s) = stylesheet.get_class_with_state(class, ElementState::Focus) {
                absorb(s);
            }
        }
    }

    // 2. ID-based styles (overrides class)
    if let Some(element_id) = element_id {
        if let Some(base) = stylesheet.get(element_id) {
            absorb(base);
        }
        if let Some(s) = state {
            if let Some(state_style) = stylesheet.get_with_state(element_id, s) {
                absorb(state_style);
            }
        }
        if matches!(visual, TextFieldState::FocusedHovered) {
            if let Some(focus_style) = stylesheet.get_with_state(element_id, ElementState::Focus) {
                absorb(focus_style);
            }
        }
    }

    // Only return if at least width is specified
    width.map(|w| {
        (
            w,
            color.unwrap_or(Color::rgba(0.23, 0.51, 0.97, 0.5)),
            offset.unwrap_or(0.0),
        )
    })
}

// =============================================================================
// TextInputConfig - visual configuration
// =============================================================================

/// The config with the stylesheet's rules for the field's state applied, and
/// the outline those rules give it.
pub(super) fn resolved_config(
    cfg: &TextInputConfig,
    d: &TextInputData,
) -> (TextInputConfig, Option<(f32, Color, f32)>) {
    let mut cfg = cfg.clone();
    let has_css_target = d.css_element_id.is_some() || !d.css_classes.is_empty();
    let outline = if has_css_target {
        active_stylesheet().and_then(|stylesheet| {
            apply_css_overrides(
                &mut cfg,
                &stylesheet,
                d.css_element_id.as_deref(),
                &d.css_classes,
                &d.visual,
            );
            extract_outline_from_stylesheet(
                &stylesheet,
                d.css_element_id.as_deref(),
                &d.css_classes,
                &d.visual,
            )
        })
    } else {
        None
    };
    (cfg, outline)
}
