//! The stylesheet's rules that reach a text area's content: its text, caret,
//! selection and size. The box is styled by the ordinary cascade.

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
        if let Some(s) = state
            && let Some(state_style) = stylesheet.get_class_with_state(class, s)
        {
            apply_style_to_textarea_config(cfg, state_style, visual);
        }
        if matches!(visual, TextFieldState::FocusedHovered)
            && let Some(s) = stylesheet.get_class_with_state(class, ElementState::Focus)
        {
            apply_style_to_textarea_config(cfg, s, visual);
        }
    }

    // 2. Apply ID-based styles (higher priority — overrides class)
    if let Some(element_id) = element_id {
        if let Some(base) = stylesheet.get(element_id) {
            apply_style_to_textarea_config(cfg, base, visual);
        }
        if let Some(s) = state
            && let Some(state_style) = stylesheet.get_with_state(element_id, s)
        {
            apply_style_to_textarea_config(cfg, state_style, visual);
        }
        if matches!(visual, TextFieldState::FocusedHovered)
            && let Some(focus_style) = stylesheet.get_with_state(element_id, ElementState::Focus)
        {
            apply_style_to_textarea_config(cfg, focus_style, visual);
        }
    }

    // 3. `::placeholder` and `::selection`.
    let (placeholder, selection) =
        crate::widgets::text_input::pseudo_element_colours(stylesheet, element_id, css_classes);
    if let Some(color) = placeholder {
        cfg.placeholder_color = color;
    }
    if let Some(color) = selection {
        cfg.selection_color = color;
    }
}

pub(super) fn apply_style_to_textarea_config(
    cfg: &mut TextAreaConfig,
    style: &crate::element_style::ElementStyle,
    visual: &TextFieldState,
) {
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
