//! Element-builder → `ElementType` projection.
//!
//! Two flavours, both `pub(crate)`:
//!
//! - `determine_element_type<E: ElementBuilder>` for the generic call
//!   sites (build path, change-analysis path).
//! - `determine_element_type_boxed(&dyn ElementBuilder)` for sites
//!   that hold a trait-object builder (subtree rebuilds, fragment
//!   children that may carry mixed concrete types).
//!
//! Each one peeks the builder's `ElementTypeId` and pulls the
//! matching `*_render_info()` to populate `ElementType::Text(...)`,
//! `Svg(...)`, `Image(...)`, `Canvas(...)`, `StyledText(...)`, or
//! falls back to `ElementType::Div`. Text variants delegate to
//! `Self::build_text_data` so CSS overrides folded into the parent
//! `RenderProps` carry through to the materialised `TextData`.

use crate::div::{ElementBuilder, ElementTypeId};
use crate::element::RenderProps;

use super::super::{
    CanvasData, ElementType, ImageData, RenderTree, StyledTextData, StyledTextSpan, SvgData,
};

impl RenderTree {
    /// Determine element type from an element builder
    pub(crate) fn determine_element_type<E: ElementBuilder>(element: &E) -> ElementType {
        let type_id = element.element_type_id();
        if matches!(type_id, ElementTypeId::Canvas) {
            tracing::trace!("determine_element_type: ElementTypeId::Canvas detected!");
        }
        let default_props = RenderProps::default();
        match type_id {
            ElementTypeId::Text => {
                if let Some(info) = element.text_render_info() {
                    ElementType::Text(Self::build_text_data(info, &default_props))
                } else {
                    ElementType::Div
                }
            }
            ElementTypeId::Svg => {
                if let Some(info) = element.svg_render_info() {
                    ElementType::Svg(SvgData {
                        source: info.source,
                        tint: info.tint,
                        fill: info.fill,
                        stroke: info.stroke,
                        stroke_width: info.stroke_width,
                    })
                } else {
                    ElementType::Div
                }
            }
            ElementTypeId::Image => {
                if let Some(info) = element.image_render_info() {
                    ElementType::Image(ImageData {
                        source: info.source,
                        object_fit: info.object_fit,
                        object_position: info.object_position,
                        opacity: info.opacity,
                        border_radius: info.border_radius,
                        tint: info.tint,
                        filter: info.filter,
                        loading_strategy: info.loading_strategy,
                        placeholder_type: info.placeholder_type,
                        placeholder_color: info.placeholder_color,
                        placeholder_image: info.placeholder_image,
                        fade_duration_ms: info.fade_duration_ms,
                    })
                } else {
                    ElementType::Div
                }
            }
            ElementTypeId::Canvas => ElementType::Canvas(CanvasData {
                render_fn: element.canvas_render_info(),
                is_static: element.is_static_canvas(),
            }),
            ElementTypeId::StyledText => {
                if let Some(info) = element.styled_text_render_info() {
                    ElementType::StyledText(StyledTextData {
                        content: info.content,
                        spans: info
                            .spans
                            .into_iter()
                            .map(|s| StyledTextSpan {
                                start: s.start,
                                end: s.end,
                                color: s.color,
                                bold: s.bold,
                                italic: s.italic,
                                underline: s.underline,
                                strikethrough: s.strikethrough,
                                link_url: s.link_url,
                            })
                            .collect(),
                        default_color: info.default_color,
                        font_size: info.font_size,
                        align: info.align,
                        v_align: info.v_align,
                        font_family: info.font_family,
                        line_height: info.line_height,
                        weight: info.weight,
                        italic: info.italic,
                        ascender: info.ascender,
                        half_leading: info.half_leading,
                    })
                } else {
                    ElementType::Div
                }
            }
            ElementTypeId::Div => ElementType::Div,
            ElementTypeId::Motion => ElementType::Div, // Motion is a transparent container
        }
    }

    /// Determine element type from a boxed element builder
    pub(crate) fn determine_element_type_boxed(element: &dyn ElementBuilder) -> ElementType {
        let type_id = element.element_type_id();
        if matches!(type_id, ElementTypeId::Canvas) {
            tracing::trace!("determine_element_type_boxed: ElementTypeId::Canvas detected!");
        }
        let default_props = RenderProps::default();
        match type_id {
            ElementTypeId::Text => {
                if let Some(info) = element.text_render_info() {
                    ElementType::Text(Self::build_text_data(info, &default_props))
                } else {
                    ElementType::Div
                }
            }
            ElementTypeId::StyledText => {
                if let Some(info) = element.styled_text_render_info() {
                    ElementType::StyledText(StyledTextData {
                        content: info.content,
                        spans: info
                            .spans
                            .into_iter()
                            .map(|s| StyledTextSpan {
                                start: s.start,
                                end: s.end,
                                color: s.color,
                                bold: s.bold,
                                italic: s.italic,
                                underline: s.underline,
                                strikethrough: s.strikethrough,
                                link_url: s.link_url,
                            })
                            .collect(),
                        default_color: info.default_color,
                        font_size: info.font_size,
                        align: info.align,
                        v_align: info.v_align,
                        font_family: info.font_family,
                        line_height: info.line_height,
                        weight: info.weight,
                        italic: info.italic,
                        ascender: info.ascender,
                        half_leading: info.half_leading,
                    })
                } else {
                    ElementType::Div
                }
            }
            ElementTypeId::Svg => {
                if let Some(info) = element.svg_render_info() {
                    ElementType::Svg(SvgData {
                        source: info.source,
                        tint: info.tint,
                        fill: info.fill,
                        stroke: info.stroke,
                        stroke_width: info.stroke_width,
                    })
                } else {
                    ElementType::Div
                }
            }
            ElementTypeId::Image => {
                if let Some(info) = element.image_render_info() {
                    ElementType::Image(ImageData {
                        source: info.source,
                        object_fit: info.object_fit,
                        object_position: info.object_position,
                        opacity: info.opacity,
                        border_radius: info.border_radius,
                        tint: info.tint,
                        filter: info.filter,
                        loading_strategy: info.loading_strategy,
                        placeholder_type: info.placeholder_type,
                        placeholder_color: info.placeholder_color,
                        placeholder_image: info.placeholder_image,
                        fade_duration_ms: info.fade_duration_ms,
                    })
                } else {
                    ElementType::Div
                }
            }
            ElementTypeId::Canvas => ElementType::Canvas(CanvasData {
                render_fn: element.canvas_render_info(),
                is_static: element.is_static_canvas(),
            }),
            ElementTypeId::Div => ElementType::Div,
            ElementTypeId::Motion => ElementType::Div,
        }
    }
}
