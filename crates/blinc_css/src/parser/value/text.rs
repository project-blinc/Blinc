//! Text-related keyword values.

pub(crate) fn parse_font_weight(value: &str) -> Option<crate::text::FontWeight> {
    use crate::text::FontWeight;
    match value.trim().to_lowercase().as_str() {
        "100" | "thin" => Some(FontWeight::Thin),
        "200" | "extra-light" | "extralight" => Some(FontWeight::ExtraLight),
        "300" | "light" => Some(FontWeight::Light),
        "400" | "normal" => Some(FontWeight::Normal),
        "500" | "medium" => Some(FontWeight::Medium),
        "600" | "semi-bold" | "semibold" => Some(FontWeight::SemiBold),
        "700" | "bold" => Some(FontWeight::Bold),
        "800" | "extra-bold" | "extrabold" => Some(FontWeight::ExtraBold),
        "900" | "black" => Some(FontWeight::Black),
        _ => None,
    }
}

pub(crate) fn parse_text_decoration(value: &str) -> Option<crate::element_style::TextDecoration> {
    use crate::element_style::TextDecoration;
    match value.trim().to_lowercase().as_str() {
        "none" => Some(TextDecoration::None),
        "underline" => Some(TextDecoration::Underline),
        "line-through" => Some(TextDecoration::LineThrough),
        _ => None,
    }
}

/// `font-style: normal | italic | oblique`
///
/// `oblique` maps to italic: the renderer selects a face, and there is no
/// synthetic slant to distinguish the two.
pub(crate) fn parse_font_style(value: &str) -> Option<crate::element_style::FontStyle> {
    use crate::element_style::FontStyle;
    match value.trim().to_lowercase().as_str() {
        "normal" => Some(FontStyle::Normal),
        "italic" | "oblique" => Some(FontStyle::Italic),
        _ => None,
    }
}

pub(crate) fn parse_text_align(value: &str) -> Option<crate::text::TextAlign> {
    use crate::text::TextAlign;
    match value.trim().to_lowercase().as_str() {
        "left" | "start" => Some(TextAlign::Left),
        "center" => Some(TextAlign::Center),
        "right" | "end" => Some(TextAlign::Right),
        _ => None,
    }
}

pub(crate) fn parse_cursor(value: &str) -> Option<crate::material::CursorStyle> {
    use crate::material::CursorStyle;
    match value.trim().to_lowercase().as_str() {
        "default" | "auto" => Some(CursorStyle::Default),
        "pointer" => Some(CursorStyle::Pointer),
        "text" => Some(CursorStyle::Text),
        "crosshair" => Some(CursorStyle::Crosshair),
        "move" => Some(CursorStyle::Move),
        "not-allowed" => Some(CursorStyle::NotAllowed),
        "ns-resize" | "n-resize" | "s-resize" | "row-resize" => Some(CursorStyle::ResizeNS),
        "ew-resize" | "e-resize" | "w-resize" | "col-resize" => Some(CursorStyle::ResizeEW),
        "nesw-resize" | "ne-resize" | "sw-resize" => Some(CursorStyle::ResizeNESW),
        "nwse-resize" | "nw-resize" | "se-resize" => Some(CursorStyle::ResizeNWSE),
        "grab" => Some(CursorStyle::Grab),
        "grabbing" => Some(CursorStyle::Grabbing),
        "wait" => Some(CursorStyle::Wait),
        "progress" => Some(CursorStyle::Progress),
        "none" => Some(CursorStyle::None),
        "help" => Some(CursorStyle::Default), // map to default for now
        _ => None,
    }
}
