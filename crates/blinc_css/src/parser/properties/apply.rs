//! Silent property application.
//!
//! Dispatches a declaration to the matching value grammar and writes the
//! result into the style. Anything unrecognized or unparsable is dropped
//! without comment. Use [`super::apply_property_with_errors`] when the
//! caller wants diagnostics.

use blinc_core::{Color, CornerRadius, OverflowFade};
use tracing::debug;

use crate::element_style::{
    ElementStyle, SpacingRect, StyleAlign, StyleDisplay, StyleFlexDirection, StyleJustify,
    StyleOverflow, StylePosition,
};
use crate::material::{GlassMaterial, Material, MetallicMaterial, RenderLayer, WoodMaterial};
use crate::parser::*;

pub(crate) fn apply_property(style: &mut ElementStyle, name: &str, value: &str) {
    match name {
        "background" | "background-color" => {
            if let Some(brush) = parse_brush(value) {
                style.background = Some(brush);
            }
        }
        "color" => {
            if let Some(c) = parse_color(value) {
                style.text_color = Some(c);
            }
        }
        "font-size" => {
            if let Some(px) = parse_length_value(value) {
                style.font_size = Some(px);
            }
        }
        "font-weight" => {
            style.font_weight = parse_font_weight(value);
        }
        "font-style" => {
            style.font_style = parse_font_style(value);
        }
        "text-decoration" | "text-decoration-line" => {
            style.text_decoration = parse_text_decoration(value);
        }
        "line-height" => {
            if let Some(val) = parse_length_value(value) {
                style.line_height = Some(val);
            } else if let Ok(val) = value.trim().parse::<f32>() {
                style.line_height = Some(val);
            }
        }
        "text-align" => {
            style.text_align = parse_text_align(value);
        }
        "letter-spacing" => {
            if let Some(px) = parse_length_value(value) {
                style.letter_spacing = Some(px);
            }
        }
        "fill" => {
            if value.trim().eq_ignore_ascii_case("none") {
                style.fill = Some(Color::TRANSPARENT);
            } else if let Some(color) = parse_color(value) {
                style.fill = Some(color);
            }
        }
        "stroke" => {
            if let Some(color) = parse_color(value) {
                style.stroke = Some(color);
            }
        }
        "stroke-width" => {
            if let Some(px) = parse_length_value(value) {
                style.stroke_width = Some(px);
            }
        }
        "stroke-dasharray" => {
            let trimmed = value.trim();
            if trimmed.eq_ignore_ascii_case("none") {
                style.stroke_dasharray = Some(vec![]);
            } else {
                let dashes: Vec<f32> = trimmed
                    .split([',', ' '])
                    .filter_map(|s| {
                        let s = s.trim();
                        if s.is_empty() {
                            None
                        } else {
                            parse_length_value(s)
                        }
                    })
                    .collect();
                if !dashes.is_empty() {
                    style.stroke_dasharray = Some(dashes);
                }
            }
        }
        "stroke-dashoffset" => {
            if let Some(px) = parse_length_value(value) {
                style.stroke_dashoffset = Some(px);
            }
        }
        "d" => {
            // CSS d: path("...") for SVG path morphing
            let trimmed = value.trim();
            if let Some(inner) = trimmed
                .strip_prefix("path(")
                .and_then(|s| s.strip_suffix(')'))
            {
                let inner = inner.trim();
                // Remove surrounding quotes if present
                let path_data = if (inner.starts_with('"') && inner.ends_with('"'))
                    || (inner.starts_with('\'') && inner.ends_with('\''))
                {
                    &inner[1..inner.len() - 1]
                } else {
                    inner
                };
                style.svg_path_data = Some(path_data.to_string());
            }
        }
        "scrollbar-color" => {
            let parts: Vec<&str> = value.split_whitespace().collect();
            if parts.len() == 2 {
                if let (Some(thumb), Some(track)) = (parse_color(parts[0]), parse_color(parts[1])) {
                    style.scrollbar_color = Some((thumb, track));
                }
            }
        }
        "scrollbar-width" => match value.trim() {
            "auto" => style.scrollbar_width = Some(crate::element_style::ScrollbarWidth::Auto),
            "thin" => style.scrollbar_width = Some(crate::element_style::ScrollbarWidth::Thin),
            "none" => style.scrollbar_width = Some(crate::element_style::ScrollbarWidth::None),
            _ => {}
        },
        "border-radius" => match try_parse_calc(value) {
            CalcParseResult::Dynamic(expr) => {
                style
                    .dynamic_properties
                    .get_or_insert_with(Vec::new)
                    .push(crate::element_style::DynamicProperty::CornerRadius(expr));
            }
            CalcParseResult::Static(val) => {
                style.corner_radius = Some(CornerRadius::uniform(val.max(0.0)));
            }
            CalcParseResult::NotCalc => {
                if let Some(radius) = parse_radius(value) {
                    style.corner_radius = Some(radius);
                }
            }
        },
        "corner-shape" => {
            let value = value.trim();
            if let Some(cs) = parse_corner_shape_value(value) {
                style.corner_shape = Some(cs);
            }
        }
        "overflow-fade" => {
            let trimmed = value.trim();
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            let parse_fade_val =
                |s: &str| -> Option<f32> { parse_length_value(s).map(|v| v.max(0.0)) };
            match parts.len() {
                1 => {
                    if let Some(v) = parse_fade_val(parts[0]) {
                        style.overflow_fade = Some(OverflowFade::uniform(v));
                    }
                }
                2 => {
                    if let (Some(v), Some(h)) = (parse_fade_val(parts[0]), parse_fade_val(parts[1]))
                    {
                        style.overflow_fade = Some(OverflowFade::new(v, h, v, h));
                    }
                }
                4 => {
                    if let (Some(t), Some(r), Some(b), Some(l)) = (
                        parse_fade_val(parts[0]),
                        parse_fade_val(parts[1]),
                        parse_fade_val(parts[2]),
                        parse_fade_val(parts[3]),
                    ) {
                        style.overflow_fade = Some(OverflowFade::new(t, r, b, l));
                    }
                }
                _ => {}
            }
        }
        "box-shadow" => {
            if let Some(shadow_stack) = parse_shadow_stack(value) {
                style.shadow = shadow_stack;
            }
        }
        "text-shadow" => {
            if let Some(shadow) = parse_shadow(value) {
                style.text_shadow = Some(shadow);
            }
        }
        "transform" => {
            parse_transform_with_3d(value, style);
        }
        "transform-origin" => {
            if let Some(origin) = parse_transform_origin(value) {
                style.transform_origin = Some(origin);
            }
        }
        "opacity" => match try_parse_calc(value) {
            CalcParseResult::Dynamic(expr) => {
                style
                    .dynamic_properties
                    .get_or_insert_with(Vec::new)
                    .push(crate::element_style::DynamicProperty::Opacity(expr));
            }
            CalcParseResult::Static(val) => {
                style.opacity = Some(val.clamp(0.0, 1.0));
            }
            CalcParseResult::NotCalc => {
                if let Ok((_, opacity)) = parse_opacity::<nom::error::Error<&str>>(value) {
                    style.opacity = Some(opacity.clamp(0.0, 1.0));
                }
            }
        },
        "render-layer" => {
            if let Ok((_, layer)) = parse_render_layer::<nom::error::Error<&str>>(value) {
                style.render_layer = Some(layer);
            }
        }
        "z-index" => {
            if let Ok(z) = value.trim().parse::<i32>() {
                style.z_index = Some(z);
            } else if let Ok((_, layer)) = parse_render_layer::<nom::error::Error<&str>>(value) {
                style.render_layer = Some(layer);
            }
        }
        // 3D transform properties
        "rotate-x" => match try_parse_calc(value) {
            CalcParseResult::Dynamic(expr) => {
                style
                    .dynamic_properties
                    .get_or_insert_with(Vec::new)
                    .push(crate::element_style::DynamicProperty::RotateX(expr));
            }
            CalcParseResult::Static(val) => {
                style.rotate_x = Some(val);
            }
            CalcParseResult::NotCalc => {
                if let Some(deg) = parse_angle_value(value) {
                    style.rotate_x = Some(deg);
                }
            }
        },
        "rotate-y" => match try_parse_calc(value) {
            CalcParseResult::Dynamic(expr) => {
                style
                    .dynamic_properties
                    .get_or_insert_with(Vec::new)
                    .push(crate::element_style::DynamicProperty::RotateY(expr));
            }
            CalcParseResult::Static(val) => {
                style.rotate_y = Some(val);
            }
            CalcParseResult::NotCalc => {
                if let Some(deg) = parse_angle_value(value) {
                    style.rotate_y = Some(deg);
                }
            }
        },
        "perspective" => match try_parse_calc(value) {
            CalcParseResult::Dynamic(expr) => {
                style
                    .dynamic_properties
                    .get_or_insert_with(Vec::new)
                    .push(crate::element_style::DynamicProperty::Perspective(expr));
            }
            CalcParseResult::Static(val) => {
                style.perspective = Some(val);
            }
            CalcParseResult::NotCalc => {
                if let Some(px) = parse_css_px(value) {
                    style.perspective = Some(px);
                }
            }
        },
        // 2D transform properties (standalone, work with text inheritance)
        "rotate" => match try_parse_calc(value) {
            CalcParseResult::Dynamic(expr) => {
                style
                    .dynamic_properties
                    .get_or_insert_with(Vec::new)
                    .push(crate::element_style::DynamicProperty::Rotate(expr));
            }
            CalcParseResult::Static(val) => {
                style.rotate = Some(val);
            }
            CalcParseResult::NotCalc => {
                if let Some(deg) = parse_angle_value(value) {
                    style.rotate = Some(deg);
                }
            }
        },
        "skew-x" => match try_parse_calc(value) {
            CalcParseResult::Dynamic(expr) => {
                style
                    .dynamic_properties
                    .get_or_insert_with(Vec::new)
                    .push(crate::element_style::DynamicProperty::SkewX(expr));
            }
            CalcParseResult::Static(val) => {
                style.skew_x = Some(val);
            }
            CalcParseResult::NotCalc => {
                if let Some(deg) = parse_angle_value(value) {
                    style.skew_x = Some(deg);
                }
            }
        },
        "skew-y" => match try_parse_calc(value) {
            CalcParseResult::Dynamic(expr) => {
                style
                    .dynamic_properties
                    .get_or_insert_with(Vec::new)
                    .push(crate::element_style::DynamicProperty::SkewY(expr));
            }
            CalcParseResult::Static(val) => {
                style.skew_y = Some(val);
            }
            CalcParseResult::NotCalc => {
                if let Some(deg) = parse_angle_value(value) {
                    style.skew_y = Some(deg);
                }
            }
        },
        "shape-3d" | "shape" => {
            if is_valid_shape_3d(value) {
                style.shape_3d = Some(value.trim().to_lowercase());
            }
        }
        "depth" => match try_parse_calc(value) {
            CalcParseResult::Dynamic(expr) => {
                style
                    .dynamic_properties
                    .get_or_insert_with(Vec::new)
                    .push(crate::element_style::DynamicProperty::Depth(expr));
            }
            CalcParseResult::Static(val) => {
                style.depth = Some(val);
            }
            CalcParseResult::NotCalc => {
                if let Some(px) = parse_css_px(value) {
                    style.depth = Some(px);
                }
            }
        },
        "light-direction" | "light" => {
            if let Some(dir) = parse_vec3_value(value) {
                style.light_direction = Some(dir);
            }
        }
        "light-intensity" => {
            if let Ok(v) = value.trim().parse::<f32>() {
                style.light_intensity = Some(v);
            }
        }
        "light-color" => {
            // Stub: light color modulation (Phase 5)
            // Currently ignored — light color is always white
        }
        "ambient" => {
            if let Ok(v) = value.trim().parse::<f32>() {
                style.ambient = Some(v);
            }
        }
        "specular" => {
            if let Ok(v) = value.trim().parse::<f32>() {
                style.specular = Some(v);
            }
        }
        "translate-z" => match try_parse_calc(value) {
            CalcParseResult::Dynamic(expr) => {
                style
                    .dynamic_properties
                    .get_or_insert_with(Vec::new)
                    .push(crate::element_style::DynamicProperty::TranslateZ(expr));
            }
            CalcParseResult::Static(val) => {
                style.translate_z = Some(val);
            }
            CalcParseResult::NotCalc => {
                if let Some(px) = parse_css_px(value) {
                    style.translate_z = Some(px);
                }
            }
        },
        "3d-op" | "shape-combine" => {
            if is_valid_op_3d(value) {
                style.op_3d = Some(value.trim().to_lowercase());
            }
        }
        "3d-blend" | "shape-blend" => {
            if let Some(px) = parse_css_px(value) {
                style.blend_3d = Some(px);
            }
        }
        "surface" => {
            // Map surface names to existing material system
            match value.trim() {
                "flat" | "solid" | "none" => {
                    // No material (default solid rendering)
                }
                "glossy" | "glass" => {
                    style.material = Some(Material::Glass(GlassMaterial::default()));
                }
                "metallic" | "chrome" => {
                    style.material = Some(Material::Metallic(MetallicMaterial::new()));
                }
                "gold" => {
                    style.material = Some(Material::Metallic(MetallicMaterial::gold()));
                }
                "wood" => {
                    style.material = Some(Material::Wood(WoodMaterial::default()));
                }
                _ => {}
            }
        }
        "surface-roughness" | "surface-fresnel" | "surface-color" | "surface-normal" => {
            // Stubs for Phase 5 surface model extensions
        }
        "animation" => {
            if let Some(animation) = parse_animation(value) {
                style.animation = Some(animation);
            }
        }
        "animation-name" => {
            let mut anim = style.animation.take().unwrap_or_default();
            anim.name = value.trim().to_string();
            style.animation = Some(anim);
        }
        "animation-duration" => {
            if let Some(ms) = parse_time_value(value) {
                let mut anim = style.animation.take().unwrap_or_default();
                anim.duration_ms = ms;
                style.animation = Some(anim);
            }
        }
        "animation-delay" => {
            if let Some(ms) = parse_time_value(value) {
                let mut anim = style.animation.take().unwrap_or_default();
                anim.delay_ms = ms;
                style.animation = Some(anim);
            }
        }
        "animation-timing-function" => {
            if let Some(timing) = AnimationTiming::from_str(value.trim()) {
                let mut anim = style.animation.take().unwrap_or_default();
                anim.timing = timing;
                style.animation = Some(anim);
            }
        }
        "animation-iteration-count" => {
            let mut anim = style.animation.take().unwrap_or_default();
            if value.trim().eq_ignore_ascii_case("infinite") {
                anim.iteration_count = 0;
            } else if let Ok(count) = value.trim().parse::<u32>() {
                anim.iteration_count = count;
            }
            style.animation = Some(anim);
        }
        "animation-direction" => {
            if let Some(direction) = parse_animation_direction(value.trim()) {
                let mut anim = style.animation.take().unwrap_or_default();
                anim.direction = direction;
                style.animation = Some(anim);
            }
        }
        "animation-fill-mode" => {
            if let Some(fill_mode) = parse_animation_fill_mode(value.trim()) {
                let mut anim = style.animation.take().unwrap_or_default();
                anim.fill_mode = fill_mode;
                style.animation = Some(anim);
            }
        }
        "transition" => {
            if let Some(transitions) = parse_transition(value) {
                style.transition = Some(transitions);
            }
        }
        "transition-property" => {
            let mut ts = style.transition.take().unwrap_or_else(|| CssTransitionSet {
                transitions: vec![CssTransition {
                    property: String::new(),
                    duration_ms: 0,
                    timing: AnimationTiming::Ease,
                    delay_ms: 0,
                }],
            });
            if let Some(t) = ts.transitions.first_mut() {
                t.property = value.trim().to_string();
            }
            style.transition = Some(ts);
        }
        "transition-duration" => {
            if let Some(ms) = parse_time_value(value) {
                let mut ts = style.transition.take().unwrap_or_else(|| CssTransitionSet {
                    transitions: vec![CssTransition {
                        property: "all".to_string(),
                        duration_ms: 0,
                        timing: AnimationTiming::Ease,
                        delay_ms: 0,
                    }],
                });
                if let Some(t) = ts.transitions.first_mut() {
                    t.duration_ms = ms;
                }
                style.transition = Some(ts);
            }
        }
        "transition-timing-function" => {
            if let Some(timing) = AnimationTiming::from_str(value.trim()) {
                let mut ts = style.transition.take().unwrap_or_else(|| CssTransitionSet {
                    transitions: vec![CssTransition {
                        property: "all".to_string(),
                        duration_ms: 0,
                        timing: AnimationTiming::Ease,
                        delay_ms: 0,
                    }],
                });
                if let Some(t) = ts.transitions.first_mut() {
                    t.timing = timing;
                }
                style.transition = Some(ts);
            }
        }
        "transition-delay" => {
            if let Some(ms) = parse_time_value(value) {
                let mut ts = style.transition.take().unwrap_or_else(|| CssTransitionSet {
                    transitions: vec![CssTransition {
                        property: "all".to_string(),
                        duration_ms: 0,
                        timing: AnimationTiming::Ease,
                        delay_ms: 0,
                    }],
                });
                if let Some(t) = ts.transitions.first_mut() {
                    t.delay_ms = ms;
                }
                style.transition = Some(ts);
            }
        }
        "filter" => {
            if let Some(filter) = parse_css_filter(value) {
                style.filter = Some(filter);
            }
        }
        "backdrop-filter" => {
            let trimmed = value.trim().to_lowercase();
            match trimmed.as_str() {
                "glass" => {
                    style.material = Some(Material::Glass(GlassMaterial::new()));
                    style.render_layer = Some(RenderLayer::Glass);
                }
                "liquid-glass" => {
                    style.material = Some(Material::Glass(GlassMaterial::new()));
                    style.render_layer = Some(RenderLayer::Glass);
                }
                "metallic" => {
                    style.material = Some(Material::Metallic(MetallicMaterial::new()));
                }
                "chrome" => {
                    style.material = Some(Material::Metallic(MetallicMaterial::chrome()));
                }
                "gold" => {
                    style.material = Some(Material::Metallic(MetallicMaterial::gold()));
                }
                "wood" => {
                    style.material = Some(Material::Wood(WoodMaterial::new()));
                }
                _ => {
                    // Parse liquid-glass(...) or blur()/saturate()/brightness() functions
                    if let Some(glass) = parse_liquid_glass_functions(&trimmed) {
                        style.material = Some(Material::Glass(glass));
                        style.render_layer = Some(RenderLayer::Glass);
                    } else if let Some(glass) = parse_backdrop_filter_functions(&trimmed) {
                        style.material = Some(Material::Glass(glass));
                        style.render_layer = Some(RenderLayer::Glass);
                    }
                }
            }
        }
        "clip-path" => {
            if let Some(cp) = parse_clip_path(value) {
                style.clip_path = Some(cp);
            }
        }
        // =====================================================================
        // Layout Properties
        // =====================================================================
        "width" => {
            if let Some(dim) = parse_css_dimension(value) {
                style.width = Some(dim);
            }
        }
        "height" => {
            if let Some(dim) = parse_css_dimension(value) {
                style.height = Some(dim);
            }
        }
        "min-width" => {
            if let Some(px) = parse_css_px(value) {
                style.min_width = Some(px);
            }
        }
        "min-height" => {
            if let Some(px) = parse_css_px(value) {
                style.min_height = Some(px);
            }
        }
        "max-width" => {
            if let Some(px) = parse_css_px(value) {
                style.max_width = Some(px);
            }
        }
        "max-height" => {
            if let Some(px) = parse_css_px(value) {
                style.max_height = Some(px);
            }
        }
        "display" => match value.trim() {
            "flex" => style.display = Some(StyleDisplay::Flex),
            "block" => style.display = Some(StyleDisplay::Block),
            "none" => style.display = Some(StyleDisplay::None),
            _ => {}
        },
        "visibility" => match value.trim() {
            "hidden" | "collapse" => {
                style.visibility = Some(crate::element_style::StyleVisibility::Hidden)
            }
            "visible" | "normal" => {
                style.visibility = Some(crate::element_style::StyleVisibility::Visible)
            }
            _ => {}
        },
        "flex-direction" => match value.trim() {
            "row" => {
                style.display = Some(StyleDisplay::Flex);
                style.flex_direction = Some(StyleFlexDirection::Row);
            }
            "column" => {
                style.display = Some(StyleDisplay::Flex);
                style.flex_direction = Some(StyleFlexDirection::Column);
            }
            "row-reverse" => {
                style.display = Some(StyleDisplay::Flex);
                style.flex_direction = Some(StyleFlexDirection::RowReverse);
            }
            "column-reverse" => {
                style.display = Some(StyleDisplay::Flex);
                style.flex_direction = Some(StyleFlexDirection::ColumnReverse);
            }
            _ => {}
        },
        "flex-wrap" => match value.trim() {
            "wrap" => style.flex_wrap = Some(true),
            "nowrap" => style.flex_wrap = Some(false),
            _ => {}
        },
        "flex-grow" => {
            if let Ok(v) = value.trim().parse::<f32>() {
                style.flex_grow = Some(v);
            }
        }
        "flex-shrink" => {
            if let Ok(v) = value.trim().parse::<f32>() {
                style.flex_shrink = Some(v);
            }
        }
        "align-items" => match value.trim() {
            "center" => style.align_items = Some(StyleAlign::Center),
            "start" | "flex-start" => style.align_items = Some(StyleAlign::Start),
            "end" | "flex-end" => style.align_items = Some(StyleAlign::End),
            "stretch" => style.align_items = Some(StyleAlign::Stretch),
            "baseline" => style.align_items = Some(StyleAlign::Baseline),
            _ => {}
        },
        "justify-content" => match value.trim() {
            "center" => style.justify_content = Some(StyleJustify::Center),
            "start" | "flex-start" => style.justify_content = Some(StyleJustify::Start),
            "end" | "flex-end" => style.justify_content = Some(StyleJustify::End),
            "space-between" => style.justify_content = Some(StyleJustify::SpaceBetween),
            "space-around" => style.justify_content = Some(StyleJustify::SpaceAround),
            "space-evenly" => style.justify_content = Some(StyleJustify::SpaceEvenly),
            _ => {}
        },
        "align-self" => match value.trim() {
            "center" => style.align_self = Some(StyleAlign::Center),
            "start" | "flex-start" => style.align_self = Some(StyleAlign::Start),
            "end" | "flex-end" => style.align_self = Some(StyleAlign::End),
            "stretch" => style.align_self = Some(StyleAlign::Stretch),
            "baseline" => style.align_self = Some(StyleAlign::Baseline),
            _ => {}
        },
        "padding" => {
            if let Some(rect) = parse_css_spacing(value) {
                style.padding = Some(rect);
            }
        }
        "padding-top" => {
            if let Some(px) = parse_css_px(value) {
                let mut p = style.padding.unwrap_or(SpacingRect {
                    top: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 0.0,
                });
                p.top = px;
                style.padding = Some(p);
            }
        }
        "padding-right" => {
            if let Some(px) = parse_css_px(value) {
                let mut p = style.padding.unwrap_or(SpacingRect {
                    top: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 0.0,
                });
                p.right = px;
                style.padding = Some(p);
            }
        }
        "padding-bottom" => {
            if let Some(px) = parse_css_px(value) {
                let mut p = style.padding.unwrap_or(SpacingRect {
                    top: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 0.0,
                });
                p.bottom = px;
                style.padding = Some(p);
            }
        }
        "padding-left" => {
            if let Some(px) = parse_css_px(value) {
                let mut p = style.padding.unwrap_or(SpacingRect {
                    top: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 0.0,
                });
                p.left = px;
                style.padding = Some(p);
            }
        }
        "margin" => {
            if let Some(rect) = parse_css_spacing(value) {
                style.margin = Some(rect);
            }
        }
        "margin-top" => {
            if let Some(px) = parse_css_px(value) {
                let mut m = style.margin.unwrap_or(SpacingRect {
                    top: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 0.0,
                });
                m.top = px;
                style.margin = Some(m);
            }
        }
        "margin-right" => {
            if let Some(px) = parse_css_px(value) {
                let mut m = style.margin.unwrap_or(SpacingRect {
                    top: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 0.0,
                });
                m.right = px;
                style.margin = Some(m);
            }
        }
        "margin-bottom" => {
            if let Some(px) = parse_css_px(value) {
                let mut m = style.margin.unwrap_or(SpacingRect {
                    top: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 0.0,
                });
                m.bottom = px;
                style.margin = Some(m);
            }
        }
        "margin-left" => {
            if let Some(px) = parse_css_px(value) {
                let mut m = style.margin.unwrap_or(SpacingRect {
                    top: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 0.0,
                });
                m.left = px;
                style.margin = Some(m);
            }
        }
        "gap" => {
            if let Some(px) = parse_css_px(value) {
                style.gap = Some(px);
            }
        }
        "overflow" => match value.trim() {
            "hidden" | "clip" => style.overflow = Some(StyleOverflow::Clip),
            "visible" => style.overflow = Some(StyleOverflow::Visible),
            "scroll" | "auto" => style.overflow = Some(StyleOverflow::Scroll),
            _ => {}
        },
        "overflow-x" => match value.trim() {
            "hidden" | "clip" => style.overflow_x = Some(StyleOverflow::Clip),
            "visible" => style.overflow_x = Some(StyleOverflow::Visible),
            "scroll" | "auto" => style.overflow_x = Some(StyleOverflow::Scroll),
            _ => {}
        },
        "overflow-y" => match value.trim() {
            "hidden" | "clip" => style.overflow_y = Some(StyleOverflow::Clip),
            "visible" => style.overflow_y = Some(StyleOverflow::Visible),
            "scroll" | "auto" => style.overflow_y = Some(StyleOverflow::Scroll),
            _ => {}
        },
        "border" => {
            // Shorthand: border: [width] [style] [color]
            // Parts can be in any order. Style is ignored (always solid).
            for part in value.split_whitespace() {
                let p = part.trim();
                if p == "solid" || p == "dashed" || p == "dotted" || p == "none" || p == "hidden" {
                    continue; // skip style keyword
                } else if let Some(px) = parse_css_px(p) {
                    style.border_width = Some(px);
                } else if let Some(color) = parse_color(p) {
                    style.border_color = Some(color);
                }
            }
        }
        "border-width" => match try_parse_calc(value) {
            CalcParseResult::Dynamic(expr) => {
                style
                    .dynamic_properties
                    .get_or_insert_with(Vec::new)
                    .push(crate::element_style::DynamicProperty::BorderWidth(expr));
            }
            CalcParseResult::Static(val) => {
                style.border_width = Some(val.max(0.0));
            }
            CalcParseResult::NotCalc => {
                if let Some(px) = parse_css_px(value) {
                    style.border_width = Some(px);
                }
            }
        },
        "border-color" => {
            if let Some(color) = parse_color(value) {
                style.border_color = Some(color);
            }
        }
        "border-style" => {
            // Blinc borders are always solid; accept and ignore
        }
        "outline-width" => {
            if let Some(px) = parse_css_px(value) {
                style.outline_width = Some(px);
            }
        }
        "outline-color" => {
            if let Some(color) = parse_color(value) {
                style.outline_color = Some(color);
            }
        }
        "outline-offset" => {
            if let Some(px) = parse_css_px(value) {
                style.outline_offset = Some(px);
            }
        }
        "outline" => {
            // Shorthand: outline: <width> solid <color>
            // We ignore the style (always solid) and parse width + color
            let parts = split_whitespace_respecting_parens(value);
            for part in &parts {
                if let Some(px) = parse_css_px(part) {
                    style.outline_width = Some(px);
                } else if part != "solid" && part != "none" && part != "dotted" && part != "dashed"
                {
                    if let Some(color) = parse_color(part) {
                        style.outline_color = Some(color);
                    }
                }
            }
            if value.trim() == "none" {
                style.outline_width = Some(0.0);
            }
        }
        "caret-color" => {
            if let Some(color) = parse_color(value) {
                style.caret_color = Some(color);
            }
        }
        "selection-color" => {
            if let Some(color) = parse_color(value) {
                style.selection_color = Some(color);
            }
        }
        "accent-color" => {
            if let Some(color) = parse_color(value) {
                style.accent_color = Some(color);
            }
        }
        "placeholder-color" => {
            if let Some(color) = parse_color(value) {
                style.placeholder_color = Some(color);
            }
        }
        "position" => match value.trim() {
            "static" => style.position = Some(StylePosition::Static),
            "relative" => style.position = Some(StylePosition::Relative),
            "absolute" => style.position = Some(StylePosition::Absolute),
            "fixed" => style.position = Some(StylePosition::Fixed),
            "sticky" => style.position = Some(StylePosition::Sticky),
            _ => {}
        },
        "top" => {
            if let Some(px) = parse_css_px(value) {
                style.top = Some(px);
            }
        }
        "right" => {
            if let Some(px) = parse_css_px(value) {
                style.right = Some(px);
            }
        }
        "bottom" => {
            if let Some(px) = parse_css_px(value) {
                style.bottom = Some(px);
            }
        }
        "left" => {
            if let Some(px) = parse_css_px(value) {
                style.left = Some(px);
            }
        }
        "inset" => {
            if let Some(px) = parse_css_px(value) {
                style.top = Some(px);
                style.right = Some(px);
                style.bottom = Some(px);
                style.left = Some(px);
            }
        }
        "object-fit" => match value.trim() {
            "cover" => style.object_fit = Some(0),
            "contain" => style.object_fit = Some(1),
            "fill" => style.object_fit = Some(2),
            "scale-down" => style.object_fit = Some(3),
            "none" => style.object_fit = Some(4),
            _ => {}
        },
        "object-position" => {
            if let Some(pos) = parse_object_position(value) {
                style.object_position = Some(pos);
            }
        }
        "loading" => match value.trim() {
            "lazy" => style.loading_strategy = Some(1),
            "eager" => style.loading_strategy = Some(0),
            _ => {}
        },
        "image-placeholder-color" => {
            if let Some(color) = parse_color(value) {
                style.image_placeholder_color = Some([color.r, color.g, color.b, color.a]);
                style.image_placeholder_type = Some(1);
            }
        }
        "image-placeholder-image" | "image-placeholder" => {
            let trimmed = value.trim().trim_matches(|c| c == '"' || c == '\'');
            if !trimmed.is_empty() {
                style.image_placeholder_image = Some(trimmed.to_string());
                style.image_placeholder_type = Some(2);
            }
        }
        "image-placeholder-type" => match value.trim() {
            "skeleton" => style.image_placeholder_type = Some(3),
            "none" => style.image_placeholder_type = Some(0),
            "color" => style.image_placeholder_type = Some(1),
            _ => {}
        },
        "fade-duration" => {
            if let Some(ms) = parse_time_value(value) {
                style.fade_duration_ms = Some(ms);
            }
        }
        "pointer-events" => match value.trim() {
            "auto" => style.pointer_events = Some(blinc_core::PointerEvents::Auto),
            "none" => style.pointer_events = Some(blinc_core::PointerEvents::None),
            _ => {}
        },
        "cursor" => {
            if let Some(cursor) = parse_cursor(value) {
                style.cursor = Some(cursor);
            }
        }
        "mix-blend-mode" => {
            if let Some(mode) = parse_blend_mode(value) {
                style.mix_blend_mode = Some(mode);
            }
        }
        "text-decoration-color" => {
            if let Some(c) = parse_color(value) {
                style.text_decoration_color = Some(c);
            }
        }
        "text-decoration-thickness" => {
            if let Some(px) = parse_length_value(value) {
                style.text_decoration_thickness = Some(px);
            }
        }
        "text-overflow" => match value.trim() {
            "clip" => style.text_overflow = Some(crate::element_style::TextOverflow::Clip),
            "ellipsis" => style.text_overflow = Some(crate::element_style::TextOverflow::Ellipsis),
            _ => {}
        },
        "white-space" => match value.trim() {
            "normal" => style.white_space = Some(crate::element_style::WhiteSpace::Normal),
            "nowrap" => style.white_space = Some(crate::element_style::WhiteSpace::Nowrap),
            "pre" => style.white_space = Some(crate::element_style::WhiteSpace::Pre),
            "pre-wrap" => style.white_space = Some(crate::element_style::WhiteSpace::PreWrap),
            _ => {}
        },
        "mask-image" => {
            let v = value.trim();
            if v == "none" {
                style.mask_image = None;
            } else if v.starts_with("linear-gradient(") {
                if let Some(g) = parse_linear_gradient(v) {
                    style.mask_image = Some(blinc_core::MaskImage::Gradient(g));
                }
            } else if v.starts_with("radial-gradient(") {
                if let Some(g) = parse_radial_gradient(v) {
                    style.mask_image = Some(blinc_core::MaskImage::Gradient(g));
                }
            } else if let Some(url) = parse_url_value(v) {
                style.mask_image = Some(blinc_core::MaskImage::Url(url));
            }
        }
        "mask-mode" => match value.trim() {
            "alpha" => style.mask_mode = Some(blinc_core::MaskMode::Alpha),
            "luminance" => style.mask_mode = Some(blinc_core::MaskMode::Luminance),
            _ => {}
        },
        "flow" => {
            let v = value.trim();
            if v == "none" {
                style.flow = None;
            } else {
                style.flow = Some(v.to_string());
            }
        }
        "pointer-space" => {
            use crate::pointer::{PointerSpace, PointerSpaceConfig};
            let v = value.trim();
            let space = match v {
                "self" => PointerSpace::SelfSpace,
                "parent" => PointerSpace::Parent,
                "viewport" => PointerSpace::Viewport,
                "none" => {
                    style.pointer_space = None;
                    return;
                }
                _ => PointerSpace::SelfSpace,
            };
            let config = style
                .pointer_space
                .get_or_insert(PointerSpaceConfig::default());
            config.space = space;
        }
        "pointer-origin" => {
            use crate::pointer::{PointerOrigin, PointerSpaceConfig};
            let v = value.trim();
            let origin = match v {
                "center" => PointerOrigin::Center,
                "top-left" => PointerOrigin::TopLeft,
                "bottom-left" => PointerOrigin::BottomLeft,
                _ => return,
            };
            let config = style
                .pointer_space
                .get_or_insert(PointerSpaceConfig::default());
            config.origin = origin;
        }
        "pointer-range" => {
            use crate::pointer::PointerSpaceConfig;
            let v = value.trim();
            let parts: Vec<&str> = v.split_whitespace().collect();
            if parts.len() == 2 {
                if let (Ok(min), Ok(max)) = (parts[0].parse::<f32>(), parts[1].parse::<f32>()) {
                    let config = style
                        .pointer_space
                        .get_or_insert(PointerSpaceConfig::default());
                    config.range = (min, max);
                }
            }
        }
        "pointer-smoothing" => {
            use crate::pointer::PointerSpaceConfig;
            let v = value.trim();
            let v = v.strip_suffix('s').unwrap_or(v); // strip optional 's' suffix
            if let Ok(dur) = v.parse::<f32>() {
                let config = style
                    .pointer_space
                    .get_or_insert(PointerSpaceConfig::default());
                config.smoothing = dur;
            }
        }
        _ => {
            // Unknown property - log at debug level for forward compatibility
            debug!(
                property = name,
                value = value,
                "Unknown CSS property (ignored)"
            );
        }
    }
}
