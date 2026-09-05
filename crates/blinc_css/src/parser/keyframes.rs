//! `@keyframes` rules.
//!
//! A [`CssKeyframes`] is the parsed body of one `@keyframes` block: a name
//! and its stops, each stop a percentage plus the style declared there.
//! Resolving an animation at a given progress interpolates between the two
//! bracketing stops.

use crate::element_style::ElementStyle;
use crate::material::Material;
use crate::parser::*;

/// A CSS keyframe animation definition
///
/// Represents a parsed `@keyframes` rule with multiple stops.
#[derive(Clone, Debug)]
pub struct CssKeyframes {
    /// Animation name
    pub name: String,
    /// Keyframe stops (position 0.0-1.0 -> style properties)
    pub keyframes: Vec<CssKeyframe>,
}

/// A single keyframe stop in an animation
#[derive(Clone, Debug)]
pub struct CssKeyframe {
    /// Position in the animation (0.0 = start, 1.0 = end)
    pub position: f32,
    /// Style properties at this keyframe
    pub style: ElementStyle,
}

impl CssKeyframes {
    /// Create a new keyframes definition
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            keyframes: Vec::new(),
        }
    }

    /// Add a keyframe at a specific position
    pub fn add_keyframe(&mut self, position: f32, style: ElementStyle) {
        self.keyframes.push(CssKeyframe { position, style });
        // Keep keyframes sorted by position
        self.keyframes
            .sort_by(|a, b| a.position.partial_cmp(&b.position).unwrap());
    }

    /// Get the keyframe at or before a given position
    pub fn keyframe_at(&self, position: f32) -> Option<&CssKeyframe> {
        self.keyframes
            .iter()
            .rev()
            .find(|kf| kf.position <= position)
    }

    /// Convert to Blinc MotionAnimation for enter animations
    ///
    /// Uses the first keyframe (0% or from) as enter_from and animates to the final state.
    pub fn to_enter_animation(&self, duration_ms: u32) -> crate::motion::MotionAnimation {
        let enter_from = self
            .keyframes
            .first()
            .map(|kf| Self::style_to_motion_keyframe(&kf.style));

        crate::motion::MotionAnimation {
            enter_from,
            enter_duration_ms: duration_ms,
            enter_delay_ms: 0,
            exit_to: None,
            exit_duration_ms: 0,
        }
    }

    /// Convert to Blinc MotionAnimation for exit animations
    ///
    /// Uses the last keyframe (100% or to) as exit_to.
    pub fn to_exit_animation(&self, duration_ms: u32) -> crate::motion::MotionAnimation {
        let exit_to = self
            .keyframes
            .last()
            .map(|kf| Self::style_to_motion_keyframe(&kf.style));

        crate::motion::MotionAnimation {
            enter_from: None,
            enter_duration_ms: 0,
            enter_delay_ms: 0,
            exit_to,
            exit_duration_ms: duration_ms,
        }
    }

    /// Convert to a full enter/exit MotionAnimation
    ///
    /// First keyframe becomes enter_from, last keyframe becomes exit_to.
    pub fn to_motion_animation(
        &self,
        enter_duration_ms: u32,
        exit_duration_ms: u32,
    ) -> crate::motion::MotionAnimation {
        let enter_from = self
            .keyframes
            .first()
            .map(|kf| Self::style_to_motion_keyframe(&kf.style));
        let exit_to = self
            .keyframes
            .last()
            .map(|kf| Self::style_to_motion_keyframe(&kf.style));

        crate::motion::MotionAnimation {
            enter_from,
            enter_duration_ms,
            enter_delay_ms: 0,
            exit_to,
            exit_duration_ms,
        }
    }

    /// Convert to a MultiKeyframeAnimation for more complex, multi-step animations
    ///
    /// This is the preferred method for animations with multiple keyframes (more than
    /// just from/to). It creates a proper multi-keyframe animation that can be played,
    /// paused, and controlled.
    ///
    /// # Arguments
    ///
    /// * `duration_ms` - Total animation duration in milliseconds
    /// * `easing` - Default easing function for transitions between keyframes
    ///
    /// # Example
    ///
    /// ```ignore
    /// let css = r#"
    ///     @keyframes pulse {
    ///         0%, 100% { opacity: 1; transform: scale(1); }
    ///         50% { opacity: 0.8; transform: scale(1.05); }
    ///     }
    /// "#;
    /// let stylesheet = Stylesheet::parse_with_errors(css).stylesheet;
    /// if let Some(keyframes) = stylesheet.get_keyframes("pulse") {
    ///     let mut animation = keyframes.to_multi_keyframe_animation(1000, Easing::EaseInOut);
    ///     animation.set_iterations(-1); // Infinite loop
    ///     animation.play();
    /// }
    /// ```
    pub fn to_multi_keyframe_animation(
        &self,
        duration_ms: u32,
        easing: blinc_animation::Easing,
    ) -> blinc_animation::MultiKeyframeAnimation {
        use blinc_animation::MultiKeyframeAnimation;

        let mut animation = MultiKeyframeAnimation::new(duration_ms);

        for kf in &self.keyframes {
            let props = Self::style_to_keyframe_properties(&kf.style);
            animation = animation.keyframe(kf.position, props, easing);
        }

        animation
    }

    /// Convert ElementStyle to KeyframeProperties for MultiKeyframeAnimation
    fn style_to_keyframe_properties(style: &ElementStyle) -> blinc_animation::KeyframeProperties {
        use blinc_animation::KeyframeProperties;
        use blinc_core::Transform;

        let mut props = KeyframeProperties::default();

        if let Some(opacity) = style.opacity {
            props.opacity = Some(opacity);
        }

        // Extract transform components for animation.
        // IMPORTANT: When a transform IS explicitly set, always include all components
        // (even identity values like scale=1.0, rotate=0.0) so that lerp between
        // keyframes works correctly (lerp_opt(Some(1.0), Some(1.3), t) interpolates,
        // while lerp_opt(None, Some(1.3), t) jumps).
        //
        // Prefer the original decomposed values (rotate, scale_x, scale_y) stored
        // during CSS parsing. These preserve the exact angle/factor from the CSS source.
        // Falling back to Affine2D decomposition is lossy — atan2 maps 359° to -1°.
        if style.transform.is_some() || style.rotate.is_some() || style.scale_x.is_some() {
            // Use original decomposed values when available
            props.rotate = Some(style.rotate.unwrap_or(0.0));
            props.scale_x = Some(style.scale_x.unwrap_or(1.0));
            props.scale_y = Some(style.scale_y.unwrap_or(1.0));

            // Extract translation from the matrix (translation doesn't suffer from wrapping)
            if let Some(Transform::Affine2D(affine)) = &style.transform {
                let [_a, _b, _c, _d, tx, ty] = affine.elements;
                props.translate_x = Some(tx);
                props.translate_y = Some(ty);
            } else {
                props.translate_x = Some(0.0);
                props.translate_y = Some(0.0);
            }
        }

        // 3D properties
        if let Some(rx) = style.rotate_x {
            props.rotate_x = Some(rx);
        }
        if let Some(ry) = style.rotate_y {
            props.rotate_y = Some(ry);
        }
        if let Some(p) = style.perspective {
            props.perspective = Some(p);
        }
        if let Some(d) = style.depth {
            props.depth = Some(d);
        }
        if let Some(tz) = style.translate_z {
            props.translate_z = Some(tz);
        }
        if let Some(b) = style.blend_3d {
            props.blend_3d = Some(b);
        }

        // Clip-path
        match &style.clip_path {
            Some(blinc_core::ClipPath::Inset {
                top,
                right,
                bottom,
                left,
                ..
            }) => {
                props.clip_inset = Some([
                    clip_length_to_percent(top),
                    clip_length_to_percent(right),
                    clip_length_to_percent(bottom),
                    clip_length_to_percent(left),
                ]);
            }
            Some(blinc_core::ClipPath::Circle {
                radius: Some(r), ..
            }) => {
                props.clip_circle_radius = Some(clip_length_to_percent(r));
            }
            Some(blinc_core::ClipPath::Ellipse {
                rx: Some(rx),
                ry: Some(ry),
                ..
            }) => {
                props.clip_ellipse_radii =
                    Some([clip_length_to_percent(rx), clip_length_to_percent(ry)]);
            }
            _ => {}
        }

        // Background color (solid or gradient)
        match &style.background {
            Some(blinc_core::Brush::Solid(c)) => {
                props.background_color = Some([c.r, c.g, c.b, c.a]);
            }
            Some(blinc_core::Brush::Gradient(gradient)) => {
                let stops = gradient.stops();
                if let Some(first) = stops.first() {
                    props.gradient_start_color =
                        Some([first.color.r, first.color.g, first.color.b, first.color.a]);
                }
                if let Some(last) = stops.last() {
                    props.gradient_end_color =
                        Some([last.color.r, last.color.g, last.color.b, last.color.a]);
                }
                if let blinc_core::Gradient::Linear { start, end, .. } = gradient {
                    props.gradient_angle = Some(gradient_points_to_angle(*start, *end));
                }
            }
            _ => {}
        }

        // Text color
        if let Some(c) = &style.text_color {
            props.text_color = Some([c.r, c.g, c.b, c.a]);
        }

        // Text shadow
        if let Some(ts) = &style.text_shadow {
            props.text_shadow_params = Some([ts.offset_x, ts.offset_y, ts.blur, ts.spread]);
            props.text_shadow_color = Some([ts.color.r, ts.color.g, ts.color.b, ts.color.a]);
        }

        // Font size
        if let Some(fs) = style.font_size {
            props.font_size = Some(fs);
        }

        // Corner radius
        if let Some(cr) = &style.corner_radius {
            props.corner_radius =
                Some([cr.top_left, cr.top_right, cr.bottom_right, cr.bottom_left]);
        }

        // Corner shape (superellipse)
        if let Some(cs) = &style.corner_shape {
            props.corner_shape = Some(cs.to_array());
        }

        // Overflow fade
        if let Some(fade) = &style.overflow_fade {
            props.overflow_fade = Some(fade.to_array());
        }

        // Border
        if let Some(bw) = style.border_width {
            props.border_width = Some(bw);
        }
        if let Some(bc) = &style.border_color {
            props.border_color = Some([bc.r, bc.g, bc.b, bc.a]);
        }

        // Outline
        if let Some(ow) = style.outline_width {
            props.outline_width = Some(ow);
        }
        if let Some(oc) = &style.outline_color {
            props.outline_color = Some([oc.r, oc.g, oc.b, oc.a]);
        }
        if let Some(offset) = style.outline_offset {
            props.outline_offset = Some(offset);
        }

        // Shadow — keyframe interpolates the FIRST layer only. Multi-layer
        // animations replace the stack on each tick; sub-layer lerping
        // would need a Vec<Shadow> in KeyframeProperties (follow-up).
        if let Some(shadow) = style.shadow.first() {
            props.shadow_params =
                Some([shadow.offset_x, shadow.offset_y, shadow.blur, shadow.spread]);
            props.shadow_color = Some([
                shadow.color.r,
                shadow.color.g,
                shadow.color.b,
                shadow.color.a,
            ]);
        }

        // 3D lighting
        if let Some(li) = style.light_intensity {
            props.light_intensity = Some(li);
        }
        if let Some(a) = style.ambient {
            props.ambient = Some(a);
        }
        if let Some(s) = style.specular {
            props.specular = Some(s);
        }
        if let Some(ld) = &style.light_direction {
            props.light_direction = Some(*ld);
        }

        // CSS filter properties
        if let Some(f) = &style.filter {
            props.filter_grayscale = Some(f.grayscale);
            props.filter_invert = Some(f.invert);
            props.filter_sepia = Some(f.sepia);
            props.filter_brightness = Some(f.brightness);
            props.filter_contrast = Some(f.contrast);
            props.filter_saturate = Some(f.saturate);
            props.filter_hue_rotate = Some(f.hue_rotate);
            props.filter_blur = Some(f.blur);
        }

        // Backdrop filter (glass material)
        if let Some(Material::Glass(glass)) = &style.material {
            props.backdrop_blur = Some(glass.blur);
            props.backdrop_saturation = Some(glass.saturation);
            props.backdrop_brightness = Some(glass.brightness);
        }

        // Layout properties (only Length values are animatable)
        if let Some(crate::element_style::StyleDimension::Length(w)) = style.width {
            props.width = Some(w);
        }
        if let Some(crate::element_style::StyleDimension::Length(h)) = style.height {
            props.height = Some(h);
        }
        if let Some(ref p) = style.padding {
            props.padding = Some([p.top, p.right, p.bottom, p.left]);
        }
        if let Some(ref m) = style.margin {
            props.margin = Some([m.top, m.right, m.bottom, m.left]);
        }
        if let Some(g) = style.gap {
            props.gap = Some(g);
        }
        if let Some(v) = style.min_width {
            props.min_width = Some(v);
        }
        if let Some(v) = style.max_width {
            props.max_width = Some(v);
        }
        if let Some(v) = style.min_height {
            props.min_height = Some(v);
        }
        if let Some(v) = style.max_height {
            props.max_height = Some(v);
        }
        if let Some(v) = style.flex_grow {
            props.flex_grow = Some(v);
        }
        if let Some(v) = style.flex_shrink {
            props.flex_shrink = Some(v);
        }
        if let Some(v) = style.top {
            props.inset_top = Some(v);
        }
        if let Some(v) = style.right {
            props.inset_right = Some(v);
        }
        if let Some(v) = style.bottom {
            props.inset_bottom = Some(v);
        }
        if let Some(v) = style.left {
            props.inset_left = Some(v);
        }
        if let Some(z) = style.z_index {
            props.z_index = Some(z as f32);
        }

        // Skew
        if let Some(sx) = style.skew_x {
            props.skew_x = Some(sx);
        }
        if let Some(sy) = style.skew_y {
            props.skew_y = Some(sy);
        }

        // Transform origin
        if let Some(to) = style.transform_origin {
            props.transform_origin = Some(to);
        }

        // SVG properties
        if let Some(fill) = &style.fill {
            props.svg_fill = Some([fill.r, fill.g, fill.b, fill.a]);
        }
        if let Some(stroke) = &style.stroke {
            props.svg_stroke = Some([stroke.r, stroke.g, stroke.b, stroke.a]);
        }
        if let Some(sw) = style.stroke_width {
            props.svg_stroke_width = Some(sw);
        }
        if let Some(offset) = style.stroke_dashoffset {
            props.svg_stroke_dashoffset = Some(offset);
        }
        if let Some(ref path_data) = style.svg_path_data {
            props.svg_path_data = Some(path_data.clone());
        }

        props
    }

    /// Convert ElementStyle to MotionKeyframe
    ///
    /// Extracts animatable properties from an ElementStyle for use in motion animations.
    /// Note: Transform decomposition is limited - for complex CSS transforms, only
    /// simple scale/translate/rotate can be reliably extracted.
    fn style_to_motion_keyframe(style: &ElementStyle) -> crate::motion::MotionKeyframe {
        use blinc_core::Transform;

        let mut kf = crate::motion::MotionKeyframe::new();

        if let Some(opacity) = style.opacity {
            kf.opacity = Some(opacity);
        }

        // Try to extract transform components from Affine2D
        // Note: Complex combined transforms may not decompose cleanly
        if let Some(Transform::Affine2D(affine)) = &style.transform {
            let [a, b, c, d, tx, ty] = affine.elements;

            // Always extract translation for keyframe animations
            // (including zero values which are meaningful end states)
            kf.translate_x = Some(tx);
            kf.translate_y = Some(ty);

            // Try to extract scale (valid when no rotation/skew: b=0, c=0)
            if b.abs() < 0.0001 && c.abs() < 0.0001 {
                // Always include scale values for keyframe animations
                // (including 1.0 which is a meaningful end state)
                kf.scale_x = Some(a);
                kf.scale_y = Some(d);
            } else {
                // Has rotation - try to extract rotation angle
                // For pure rotation: a=cos(θ), b=sin(θ), c=-sin(θ), d=cos(θ)
                let rotation = b.atan2(a);
                if rotation.abs() > 0.0001 {
                    kf.rotate = Some(rotation.to_degrees());
                }
            }
        }
        // Mat4 transforms are more complex, skip for now

        kf
    }
}
