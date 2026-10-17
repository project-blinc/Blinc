//! The stylesheet's rules for a text area, per visual state.

use super::*;

/// Apply CSS stylesheet overrides to a TextAreaConfig.
pub(super) fn apply_css_overrides_textarea(
    cfg: &mut TextAreaConfig,
    stylesheet: &Stylesheet,
    element_id: Option<&str>,
    css_classes: &[std::sync::Arc<str>],
    visual: &TextFieldState,
) {
    let state = match visual {
        TextFieldState::Hovered | TextFieldState::FocusedHovered => Some(ElementState::Hover),
        TextFieldState::Focused => Some(ElementState::Focus),
        TextFieldState::Disabled => Some(ElementState::Disabled),
        TextFieldState::Idle => None,
    };

    // 1. Apply class-based styles (lowest priority — overridden by ID).
    // FocusedHovered order: base → :hover → :focus, so focus colour wins
    // over hover while the user is typing. See text_input::apply_css_overrides
    // for the full rationale.
    for class in css_classes {
        if let Some(base) = stylesheet.get_class(class) {
            apply_style_to_textarea_config(cfg, base, visual);
        }
        if let Some(s) = state {
            if let Some(state_style) = stylesheet.get_class_with_state(class, s) {
                apply_style_to_textarea_config(cfg, state_style, visual);
            }
        }
        if matches!(visual, TextFieldState::FocusedHovered) {
            if let Some(s) = stylesheet.get_class_with_state(class, ElementState::Focus) {
                apply_style_to_textarea_config(cfg, s, visual);
            }
        }
    }

    // 2. Apply ID-based styles (higher priority — overrides class)
    if let Some(element_id) = element_id {
        if let Some(base) = stylesheet.get(element_id) {
            apply_style_to_textarea_config(cfg, base, visual);
        }
        if let Some(s) = state {
            if let Some(state_style) = stylesheet.get_with_state(element_id, s) {
                apply_style_to_textarea_config(cfg, state_style, visual);
            }
        }
        if matches!(visual, TextFieldState::FocusedHovered) {
            if let Some(focus_style) = stylesheet.get_with_state(element_id, ElementState::Focus) {
                apply_style_to_textarea_config(cfg, focus_style, visual);
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

pub(super) fn apply_style_to_textarea_config(
    cfg: &mut TextAreaConfig,
    style: &crate::element_style::ElementStyle,
    visual: &TextFieldState,
) {
    if let Some(blinc_core::Brush::Solid(color)) = style.background.as_ref() {
        match visual {
            TextFieldState::Idle => cfg.bg_color = *color,
            TextFieldState::Hovered => cfg.hover_bg_color = *color,
            TextFieldState::Focused | TextFieldState::FocusedHovered => {
                cfg.focused_bg_color = *color;
            }
            TextFieldState::Disabled => {}
        }
    }
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
    if let Some(w) = style.border_width {
        cfg.border_width = w;
    }
    if let Some(cr) = style.corner_radius {
        cfg.corner_radius = cr.top_left;
    }
    if let Some(color) = style.text_color {
        cfg.text_color = color;
    }
    if let Some(size) = style.font_size {
        cfg.font_size = size;
    }
    if let Some(color) = style.caret_color {
        cfg.cursor_color = color;
    }
    if let Some(color) = style.selection_color {
        cfg.selection_color = color;
    }
    if let Some(color) = style.placeholder_color {
        cfg.placeholder_color = color;
    }
}

/// Extract outline properties from stylesheet for the current state.
///
/// Walks class selectors first (lowest priority) and then `#id`
/// selectors (overrides). Without the class branch, focus-ring rules
/// like `.cn-textarea:focus { outline: 2px solid var(--border-focus); }`
/// from the cn stylesheet silently no-op because cn::textarea
/// attaches via class, not id. Mirrors the same fix on
/// `text_input::extract_outline_from_stylesheet`.
pub(super) fn extract_outline_from_textarea_stylesheet(
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
    // wins over hover.
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

    width.map(|w| {
        (
            w,
            color.unwrap_or(Color::rgba(0.23, 0.51, 0.97, 0.5)),
            offset.unwrap_or(0.0),
        )
    })
}
