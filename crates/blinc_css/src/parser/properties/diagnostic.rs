//! Property application with diagnostics.
//!
//! Mirrors [`super::apply_property`] but pushes a [`ParseError`] for an
//! unknown property name or a value the grammar rejects, so a stylesheet
//! load can report what it ignored.

use blinc_core::{Color, CornerRadius, OverflowFade};

use crate::element_style::{
    ElementStyle, SpacingRect, StyleAlign, StyleDisplay, StyleFlexDirection, StyleJustify,
    StyleOverflow, StylePosition,
};
use crate::material::{GlassMaterial, Material, MetallicMaterial, RenderLayer, WoodMaterial};
use crate::parser::*;

/// Apply a property with error collection
pub(crate) fn apply_property_with_errors(
    style: &mut ElementStyle,
    name: &str,
    value: &str,
    original_css: &str,
    current_input: &str,
    errors: &mut Vec<ParseError>,
) {
    let (line, column, _) = calculate_position(original_css, current_input);

    match name {
        "background" | "background-color" => {
            if let Some(brush) = parse_brush(value) {
                style.background = Some(brush);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "color" => {
            if let Some(c) = parse_color(value) {
                style.text_color = Some(c);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "font-size" => {
            if let Some(px) = parse_length_value(value) {
                style.font_size = Some(px);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "font-weight" => {
            if let Some(fw) = parse_font_weight(value) {
                style.font_weight = Some(fw);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "font-style" => {
            if let Some(fs) = parse_font_style(value) {
                style.font_style = Some(fs);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "text-decoration" | "text-decoration-line" => {
            if let Some(td) = parse_text_decoration(value) {
                style.text_decoration = Some(td);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "line-height" => {
            if let Some(val) = parse_length_value(value) {
                style.line_height = Some(val);
            } else if let Ok(val) = value.trim().parse::<f32>() {
                style.line_height = Some(val);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "text-align" => {
            if let Some(ta) = parse_text_align(value) {
                style.text_align = Some(ta);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "letter-spacing" => {
            if let Some(px) = parse_length_value(value) {
                style.letter_spacing = Some(px);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "fill" => {
            if value.trim().eq_ignore_ascii_case("none") {
                style.fill = Some(Color::TRANSPARENT);
            } else if let Some(color) = parse_color(value) {
                style.fill = Some(color);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "stroke" => {
            if let Some(color) = parse_color(value) {
                style.stroke = Some(color);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "stroke-width" => {
            if let Some(px) = parse_length_value(value) {
                style.stroke_width = Some(px);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
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
                } else {
                    errors.push(ParseError::invalid_value(name, value, line, column));
                }
            }
        }
        "stroke-dashoffset" => {
            if let Some(px) = parse_length_value(value) {
                style.stroke_dashoffset = Some(px);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "d" => {
            let trimmed = value.trim();
            if let Some(inner) = trimmed
                .strip_prefix("path(")
                .and_then(|s| s.strip_suffix(')'))
            {
                let inner = inner.trim();
                let path_data = if (inner.starts_with('"') && inner.ends_with('"'))
                    || (inner.starts_with('\'') && inner.ends_with('\''))
                {
                    &inner[1..inner.len() - 1]
                } else {
                    inner
                };
                style.svg_path_data = Some(path_data.to_string());
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "scrollbar-color" => {
            let parts: Vec<&str> = value.split_whitespace().collect();
            if parts.len() == 2 {
                if let (Some(thumb), Some(track)) = (parse_color(parts[0]), parse_color(parts[1])) {
                    style.scrollbar_color = Some((thumb, track));
                } else {
                    errors.push(ParseError::invalid_value(name, value, line, column));
                }
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "scrollbar-width" => match value.trim() {
            "auto" => style.scrollbar_width = Some(crate::element_style::ScrollbarWidth::Auto),
            "thin" => style.scrollbar_width = Some(crate::element_style::ScrollbarWidth::Thin),
            "none" => style.scrollbar_width = Some(crate::element_style::ScrollbarWidth::None),
            _ => errors.push(ParseError::invalid_value(name, value, line, column)),
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
                } else {
                    errors.push(ParseError::invalid_value(name, value, line, column));
                }
            }
        },
        "corner-shape" => {
            let value = value.trim();
            if let Some(cs) = parse_corner_shape_value(value) {
                style.corner_shape = Some(cs);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
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
                    } else {
                        errors.push(ParseError::invalid_value(name, value, line, column));
                    }
                }
                2 => {
                    if let (Some(v), Some(h)) = (parse_fade_val(parts[0]), parse_fade_val(parts[1]))
                    {
                        style.overflow_fade = Some(OverflowFade::new(v, h, v, h));
                    } else {
                        errors.push(ParseError::invalid_value(name, value, line, column));
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
                    } else {
                        errors.push(ParseError::invalid_value(name, value, line, column));
                    }
                }
                _ => {
                    errors.push(ParseError::invalid_value(name, value, line, column));
                }
            }
        }
        "box-shadow" => {
            if let Some(stack) = parse_shadow_stack(value) {
                style.shadow = stack;
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "text-shadow" => {
            if let Some(shadow) = parse_shadow(value) {
                style.text_shadow = Some(shadow);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "transform" => {
            if !parse_transform_with_3d(value, style) {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "transform-origin" => {
            if let Some(origin) = parse_transform_origin(value) {
                style.transform_origin = Some(origin);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
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
                } else {
                    errors.push(ParseError::invalid_value(name, value, line, column));
                }
            }
        },
        "render-layer" => {
            if let Ok((_, layer)) = parse_render_layer::<nom::error::Error<&str>>(value) {
                style.render_layer = Some(layer);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "z-index" => {
            if let Ok(z) = value.trim().parse::<i32>() {
                style.z_index = Some(z);
            } else if let Ok((_, layer)) = parse_render_layer::<nom::error::Error<&str>>(value) {
                style.render_layer = Some(layer);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
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
                } else {
                    errors.push(ParseError::invalid_value(name, value, line, column));
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
                } else {
                    errors.push(ParseError::invalid_value(name, value, line, column));
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
                } else {
                    errors.push(ParseError::invalid_value(name, value, line, column));
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
                } else {
                    errors.push(ParseError::invalid_value(name, value, line, column));
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
                } else {
                    errors.push(ParseError::invalid_value(name, value, line, column));
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
                } else {
                    errors.push(ParseError::invalid_value(name, value, line, column));
                }
            }
        },
        "shape-3d" | "shape" => {
            if is_valid_shape_3d(value) {
                style.shape_3d = Some(value.trim().to_lowercase());
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
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
                } else {
                    errors.push(ParseError::invalid_value(name, value, line, column));
                }
            }
        },
        "light-direction" | "light" => {
            if let Some(dir) = parse_vec3_value(value) {
                style.light_direction = Some(dir);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "light-intensity" => {
            if let Ok(v) = value.trim().parse::<f32>() {
                style.light_intensity = Some(v);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "light-color" => {
            // Stub: light color modulation (Phase 5)
        }
        "ambient" => {
            if let Ok(v) = value.trim().parse::<f32>() {
                style.ambient = Some(v);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "specular" => {
            if let Ok(v) = value.trim().parse::<f32>() {
                style.specular = Some(v);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
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
                } else {
                    errors.push(ParseError::invalid_value(name, value, line, column));
                }
            }
        },
        "3d-op" | "shape-combine" => {
            if is_valid_op_3d(value) {
                style.op_3d = Some(value.trim().to_lowercase());
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "3d-blend" | "shape-blend" => {
            if let Some(px) = parse_css_px(value) {
                style.blend_3d = Some(px);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "surface" => match value.trim() {
            "flat" | "solid" | "none" => {}
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
            _ => {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        },
        "surface-roughness" | "surface-fresnel" | "surface-color" | "surface-normal" => {
            // Stubs for Phase 5 surface model extensions
        }
        "animation" => {
            if let Some(animation) = parse_animation(value) {
                style.animation = Some(animation);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
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
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "animation-delay" => {
            if let Some(ms) = parse_time_value(value) {
                let mut anim = style.animation.take().unwrap_or_default();
                anim.delay_ms = ms;
                style.animation = Some(anim);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "animation-timing-function" => {
            if let Some(timing) = AnimationTiming::from_str(value.trim()) {
                let mut anim = style.animation.take().unwrap_or_default();
                anim.timing = timing;
                style.animation = Some(anim);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "animation-iteration-count" => {
            let mut anim = style.animation.take().unwrap_or_default();
            if value.trim().eq_ignore_ascii_case("infinite") {
                anim.iteration_count = 0;
                style.animation = Some(anim);
            } else if let Ok(count) = value.trim().parse::<u32>() {
                anim.iteration_count = count;
                style.animation = Some(anim);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "animation-direction" => {
            if let Some(direction) = parse_animation_direction(value.trim()) {
                let mut anim = style.animation.take().unwrap_or_default();
                anim.direction = direction;
                style.animation = Some(anim);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "animation-fill-mode" => {
            if let Some(fill_mode) = parse_animation_fill_mode(value.trim()) {
                let mut anim = style.animation.take().unwrap_or_default();
                anim.fill_mode = fill_mode;
                style.animation = Some(anim);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "transition" => {
            if let Some(transitions) = parse_transition(value) {
                style.transition = Some(transitions);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
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
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
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
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
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
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "filter" => {
            if let Some(filter) = parse_css_filter(value) {
                style.filter = Some(filter);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
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
                    if let Some(glass) = parse_liquid_glass_functions(&trimmed) {
                        style.material = Some(Material::Glass(glass));
                        style.render_layer = Some(RenderLayer::Glass);
                    } else if let Some(glass) = parse_backdrop_filter_functions(&trimmed) {
                        style.material = Some(Material::Glass(glass));
                        style.render_layer = Some(RenderLayer::Glass);
                    } else {
                        errors.push(ParseError::invalid_value(name, value, line, column));
                    }
                }
            }
        }
        "clip-path" => {
            if let Some(cp) = parse_clip_path(value) {
                style.clip_path = Some(cp);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        // =====================================================================
        // Layout Properties
        // =====================================================================
        "width" => {
            if let Some(dim) = parse_css_dimension(value) {
                style.width = Some(dim);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "height" => {
            if let Some(dim) = parse_css_dimension(value) {
                style.height = Some(dim);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "min-width" => {
            if let Some(px) = parse_css_px(value) {
                style.min_width = Some(px);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "min-height" => {
            if let Some(px) = parse_css_px(value) {
                style.min_height = Some(px);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "max-width" => {
            if let Some(px) = parse_css_px(value) {
                style.max_width = Some(px);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "max-height" => {
            if let Some(px) = parse_css_px(value) {
                style.max_height = Some(px);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "display" => match value.trim() {
            "flex" => style.display = Some(StyleDisplay::Flex),
            "block" => style.display = Some(StyleDisplay::Block),
            "none" => style.display = Some(StyleDisplay::None),
            _ => errors.push(ParseError::invalid_value(name, value, line, column)),
        },
        "visibility" => match value.trim() {
            "hidden" | "collapse" => {
                style.visibility = Some(crate::element_style::StyleVisibility::Hidden)
            }
            "visible" | "normal" => {
                style.visibility = Some(crate::element_style::StyleVisibility::Visible)
            }
            _ => errors.push(ParseError::invalid_value(name, value, line, column)),
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
            _ => errors.push(ParseError::invalid_value(name, value, line, column)),
        },
        "flex-wrap" => match value.trim() {
            "wrap" => style.flex_wrap = Some(true),
            "nowrap" => style.flex_wrap = Some(false),
            _ => errors.push(ParseError::invalid_value(name, value, line, column)),
        },
        "flex-grow" => {
            if let Ok(v) = value.trim().parse::<f32>() {
                style.flex_grow = Some(v);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "flex-shrink" => {
            if let Ok(v) = value.trim().parse::<f32>() {
                style.flex_shrink = Some(v);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "align-items" => match value.trim() {
            "center" => style.align_items = Some(StyleAlign::Center),
            "start" | "flex-start" => style.align_items = Some(StyleAlign::Start),
            "end" | "flex-end" => style.align_items = Some(StyleAlign::End),
            "stretch" => style.align_items = Some(StyleAlign::Stretch),
            "baseline" => style.align_items = Some(StyleAlign::Baseline),
            _ => errors.push(ParseError::invalid_value(name, value, line, column)),
        },
        "justify-content" => match value.trim() {
            "center" => style.justify_content = Some(StyleJustify::Center),
            "start" | "flex-start" => style.justify_content = Some(StyleJustify::Start),
            "end" | "flex-end" => style.justify_content = Some(StyleJustify::End),
            "space-between" => style.justify_content = Some(StyleJustify::SpaceBetween),
            "space-around" => style.justify_content = Some(StyleJustify::SpaceAround),
            "space-evenly" => style.justify_content = Some(StyleJustify::SpaceEvenly),
            _ => errors.push(ParseError::invalid_value(name, value, line, column)),
        },
        "align-self" => match value.trim() {
            "center" => style.align_self = Some(StyleAlign::Center),
            "start" | "flex-start" => style.align_self = Some(StyleAlign::Start),
            "end" | "flex-end" => style.align_self = Some(StyleAlign::End),
            "stretch" => style.align_self = Some(StyleAlign::Stretch),
            "baseline" => style.align_self = Some(StyleAlign::Baseline),
            _ => errors.push(ParseError::invalid_value(name, value, line, column)),
        },
        "padding" => {
            if let Some(rect) = parse_css_spacing(value) {
                style.padding = Some(rect);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
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
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
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
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
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
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
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
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "margin" => {
            if let Some(rect) = parse_css_spacing(value) {
                style.margin = Some(rect);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
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
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
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
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
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
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
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
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "gap" => {
            if let Some(px) = parse_css_px(value) {
                style.gap = Some(px);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "overflow" => match value.trim() {
            "hidden" | "clip" => style.overflow = Some(StyleOverflow::Clip),
            "visible" => style.overflow = Some(StyleOverflow::Visible),
            "scroll" | "auto" => style.overflow = Some(StyleOverflow::Scroll),
            _ => errors.push(ParseError::invalid_value(name, value, line, column)),
        },
        "overflow-x" => match value.trim() {
            "hidden" | "clip" => style.overflow_x = Some(StyleOverflow::Clip),
            "visible" => style.overflow_x = Some(StyleOverflow::Visible),
            "scroll" | "auto" => style.overflow_x = Some(StyleOverflow::Scroll),
            _ => errors.push(ParseError::invalid_value(name, value, line, column)),
        },
        "overflow-y" => match value.trim() {
            "hidden" | "clip" => style.overflow_y = Some(StyleOverflow::Clip),
            "visible" => style.overflow_y = Some(StyleOverflow::Visible),
            "scroll" | "auto" => style.overflow_y = Some(StyleOverflow::Scroll),
            _ => errors.push(ParseError::invalid_value(name, value, line, column)),
        },
        "border" => {
            // Shorthand: border: [width] [style] [color]
            for part in value.split_whitespace() {
                let p = part.trim();
                if p == "solid" || p == "dashed" || p == "dotted" || p == "none" || p == "hidden" {
                    continue;
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
                } else {
                    errors.push(ParseError::invalid_value(name, value, line, column));
                }
            }
        },
        "border-color" => {
            if let Some(color) = parse_color(value) {
                style.border_color = Some(color);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "border-style" => {
            // Blinc borders are always solid; accept and ignore
        }
        "outline-width" => {
            if let Some(px) = parse_css_px(value) {
                style.outline_width = Some(px);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "outline-color" => {
            if let Some(color) = parse_color(value) {
                style.outline_color = Some(color);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "outline-offset" => {
            if let Some(px) = parse_css_px(value) {
                style.outline_offset = Some(px);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "outline" => {
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
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "selection-color" => {
            if let Some(color) = parse_color(value) {
                style.selection_color = Some(color);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "accent-color" => {
            if let Some(color) = parse_color(value) {
                style.accent_color = Some(color);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "placeholder-color" => {
            if let Some(color) = parse_color(value) {
                style.placeholder_color = Some(color);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "position" => match value.trim() {
            "static" => style.position = Some(StylePosition::Static),
            "relative" => style.position = Some(StylePosition::Relative),
            "absolute" => style.position = Some(StylePosition::Absolute),
            "fixed" => style.position = Some(StylePosition::Fixed),
            "sticky" => style.position = Some(StylePosition::Sticky),
            _ => errors.push(ParseError::invalid_value(name, value, line, column)),
        },
        "top" => {
            if let Some(px) = parse_css_px(value) {
                style.top = Some(px);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "right" => {
            if let Some(px) = parse_css_px(value) {
                style.right = Some(px);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "bottom" => {
            if let Some(px) = parse_css_px(value) {
                style.bottom = Some(px);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "left" => {
            if let Some(px) = parse_css_px(value) {
                style.left = Some(px);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "inset" => {
            if let Some(px) = parse_css_px(value) {
                style.top = Some(px);
                style.right = Some(px);
                style.bottom = Some(px);
                style.left = Some(px);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "object-fit" => match value.trim() {
            "cover" => style.object_fit = Some(0),
            "contain" => style.object_fit = Some(1),
            "fill" => style.object_fit = Some(2),
            "scale-down" => style.object_fit = Some(3),
            "none" => style.object_fit = Some(4),
            _ => errors.push(ParseError::invalid_value(name, value, line, column)),
        },
        "loading" => match value.trim() {
            "lazy" => style.loading_strategy = Some(1),
            "eager" => style.loading_strategy = Some(0),
            _ => errors.push(ParseError::invalid_value(name, value, line, column)),
        },
        "image-placeholder-color" => {
            if let Some(color) = parse_color(value) {
                style.image_placeholder_color = Some([color.r, color.g, color.b, color.a]);
                style.image_placeholder_type = Some(1);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
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
            _ => errors.push(ParseError::invalid_value(name, value, line, column)),
        },
        "fade-duration" => {
            if let Some(ms) = parse_time_value(value) {
                style.fade_duration_ms = Some(ms);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "object-position" => {
            if let Some(pos) = parse_object_position(value) {
                style.object_position = Some(pos);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "pointer-events" => match value.trim() {
            "auto" => style.pointer_events = Some(blinc_core::PointerEvents::Auto),
            "none" => style.pointer_events = Some(blinc_core::PointerEvents::None),
            _ => errors.push(ParseError::invalid_value(name, value, line, column)),
        },
        "cursor" => {
            if let Some(cursor) = parse_cursor(value) {
                style.cursor = Some(cursor);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "mix-blend-mode" => {
            if let Some(mode) = parse_blend_mode(value) {
                style.mix_blend_mode = Some(mode);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "text-decoration-color" => {
            if let Some(c) = parse_color(value) {
                style.text_decoration_color = Some(c);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "text-decoration-thickness" => {
            if let Some(px) = parse_length_value(value) {
                style.text_decoration_thickness = Some(px);
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "text-overflow" => match value.trim() {
            "clip" => style.text_overflow = Some(crate::element_style::TextOverflow::Clip),
            "ellipsis" => style.text_overflow = Some(crate::element_style::TextOverflow::Ellipsis),
            _ => errors.push(ParseError::invalid_value(name, value, line, column)),
        },
        "white-space" => match value.trim() {
            "normal" => style.white_space = Some(crate::element_style::WhiteSpace::Normal),
            "nowrap" => style.white_space = Some(crate::element_style::WhiteSpace::Nowrap),
            "pre" => style.white_space = Some(crate::element_style::WhiteSpace::Pre),
            "pre-wrap" => style.white_space = Some(crate::element_style::WhiteSpace::PreWrap),
            _ => errors.push(ParseError::invalid_value(name, value, line, column)),
        },
        "mask-image" => {
            let v = value.trim();
            if v == "none" {
                style.mask_image = None;
            } else if v.starts_with("linear-gradient(") {
                if let Some(g) = parse_linear_gradient(v) {
                    style.mask_image = Some(blinc_core::MaskImage::Gradient(g));
                } else {
                    errors.push(ParseError::invalid_value(name, value, line, column));
                }
            } else if v.starts_with("radial-gradient(") {
                if let Some(g) = parse_radial_gradient(v) {
                    style.mask_image = Some(blinc_core::MaskImage::Gradient(g));
                } else {
                    errors.push(ParseError::invalid_value(name, value, line, column));
                }
            } else if let Some(url) = parse_url_value(v) {
                style.mask_image = Some(blinc_core::MaskImage::Url(url));
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "mask-mode" => match value.trim() {
            "alpha" => style.mask_mode = Some(blinc_core::MaskMode::Alpha),
            "luminance" => style.mask_mode = Some(blinc_core::MaskMode::Luminance),
            _ => errors.push(ParseError::invalid_value(name, value, line, column)),
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
                _ => {
                    errors.push(ParseError::invalid_value(name, value, line, column));
                    return;
                }
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
                _ => {
                    errors.push(ParseError::invalid_value(name, value, line, column));
                    return;
                }
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
                } else {
                    errors.push(ParseError::invalid_value(name, value, line, column));
                }
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        "pointer-smoothing" => {
            use crate::pointer::PointerSpaceConfig;
            let v = value.trim();
            let v = v.strip_suffix('s').unwrap_or(v);
            if let Ok(dur) = v.parse::<f32>() {
                let config = style
                    .pointer_space
                    .get_or_insert(PointerSpaceConfig::default());
                config.smoothing = dur;
            } else {
                errors.push(ParseError::invalid_value(name, value, line, column));
            }
        }
        _ => {
            // Unknown property - collect as warning
            errors.push(ParseError::unknown_property(name, line, column));
        }
    }
}

// ============================================================================
// Value Parsers
// These use generic error types so they work with both simple and VerboseError
// ============================================================================
