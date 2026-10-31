//! The stylesheet's rules that reach a text input's content: its text, caret,
//! selection and size. The box is styled by the ordinary cascade.

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
    }

    // 3. `::placeholder` and `::selection`.
    let (placeholder, selection) = pseudo_element_colours(stylesheet, element_id, css_classes);
    if let Some(color) = placeholder {
        cfg.placeholder_color = color;
    }
    if let Some(color) = selection {
        cfg.selection_color = color;
    }
}

/// The placeholder and selection colours the stylesheet's `::placeholder` and
/// `::selection` rules give a field: `.class::placeholder { color }` and
/// `.class::selection { background }`, then `#id::...`, which wins.
pub(crate) fn pseudo_element_colours(
    stylesheet: &Stylesheet,
    element_id: Option<&str>,
    css_classes: &[std::sync::Arc<str>],
) -> (Option<Color>, Option<Color>) {
    let (mut placeholder, mut selection) = (None, None);
    let mut take_placeholder = |style: &crate::element_style::ElementStyle| {
        if let Some(color) = style.placeholder_color.or(style.text_color) {
            placeholder = Some(color);
        }
    };
    for class in css_classes {
        if let Some(style) = stylesheet.get_class_pseudo_element(class, "placeholder") {
            take_placeholder(style);
        }
    }
    if let Some(style) = element_id.and_then(|id| stylesheet.get_placeholder_style(id)) {
        take_placeholder(style);
    }
    let mut take_selection = |style: &crate::element_style::ElementStyle| {
        if let Some(color) = style.selection_color {
            selection = Some(color);
        } else if let Some(blinc_core::Brush::Solid(color)) = style.background {
            selection = Some(color);
        }
    };
    for class in css_classes {
        if let Some(style) = stylesheet.get_class_pseudo_element(class, "selection") {
            take_selection(style);
        }
    }
    if let Some(style) = element_id.and_then(|id| stylesheet.get(&format!("{id}::selection"))) {
        take_selection(style);
    }
    (placeholder, selection)
}

/// Apply an ElementStyle to a TextInputConfig (CSS properties override config values)
pub(super) fn apply_style_to_config(
    cfg: &mut TextInputConfig,
    style: &crate::element_style::ElementStyle,
    visual: &TextFieldState,
) {
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

// =============================================================================
// TextInputConfig - visual configuration
// =============================================================================

/// The config with the stylesheet's rules for the field's state applied.
pub(super) fn resolved_config(cfg: &TextInputConfig, d: &TextInputData) -> TextInputConfig {
    let mut cfg = cfg.clone();
    if d.css_element_id.is_some() || !d.css_classes.is_empty() {
        if let Some(stylesheet) = active_stylesheet() {
            apply_css_overrides(
                &mut cfg,
                &stylesheet,
                d.css_element_id.as_deref(),
                &d.css_classes,
                &d.visual,
            );
        }
    }
    cfg
}
