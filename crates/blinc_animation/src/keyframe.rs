//! Keyframe animations
//!
//! This module provides both single-value keyframe animations and multi-property
//! keyframe animations for animating multiple properties simultaneously.

use crate::easing::Easing;
use std::sync::atomic::{AtomicU64, Ordering};

/// A single keyframe in a single-value animation
#[derive(Clone, Debug)]
pub struct Keyframe {
    /// Time position (0.0 to 1.0)
    pub time: f32,
    /// Target value at this keyframe
    pub value: f32,
    /// Easing function to use when transitioning TO this keyframe
    pub easing: Easing,
}

/// A keyframe-based animation (single value)
#[derive(Clone, Debug)]
pub struct KeyframeAnimation {
    duration_ms: u32,
    keyframes: Vec<Keyframe>,
    current_time: f32,
    playing: bool,
}

impl KeyframeAnimation {
    pub fn new(duration_ms: u32, keyframes: Vec<Keyframe>) -> Self {
        Self {
            duration_ms,
            keyframes,
            current_time: 0.0,
            playing: false,
        }
    }

    pub fn start(&mut self) {
        self.current_time = 0.0;
        self.playing = true;
    }

    pub fn stop(&mut self) {
        self.playing = false;
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    pub fn progress(&self) -> f32 {
        self.current_time / self.duration_ms as f32
    }

    /// Get the current interpolated value
    pub fn value(&self) -> f32 {
        if self.keyframes.is_empty() {
            return 0.0;
        }

        let progress = self.progress().clamp(0.0, 1.0);

        // Find surrounding keyframes
        let mut prev_kf = &self.keyframes[0];
        let mut next_kf = &self.keyframes[0];

        for kf in &self.keyframes {
            if kf.time <= progress {
                prev_kf = kf;
            }
            if kf.time >= progress {
                next_kf = kf;
                break;
            }
        }

        if (prev_kf.time - next_kf.time).abs() < f32::EPSILON {
            return prev_kf.value;
        }

        // Interpolate between keyframes
        let local_progress = (progress - prev_kf.time) / (next_kf.time - prev_kf.time);
        let eased = next_kf.easing.apply(local_progress);

        prev_kf.value + (next_kf.value - prev_kf.value) * eased
    }

    /// Advance the animation by delta time (in milliseconds)
    pub fn tick(&mut self, dt_ms: f32) {
        if !self.playing {
            return;
        }

        self.current_time += dt_ms;

        if self.current_time >= self.duration_ms as f32 {
            self.current_time = self.duration_ms as f32;
            self.playing = false;
        }
    }

    /// Get the keyframes
    pub fn keyframes(&self) -> &[Keyframe] {
        &self.keyframes
    }
}

// ============================================================================
// Multi-Property Keyframe Animation
// ============================================================================

/// Properties that can be animated in a multi-property keyframe
#[derive(Clone, Debug, Default, PartialEq)]
pub struct KeyframeProperties {
    /// Opacity (0.0 to 1.0)
    pub opacity: Option<f32>,
    /// Scale X factor
    pub scale_x: Option<f32>,
    /// Scale Y factor
    pub scale_y: Option<f32>,
    /// Translation X in pixels
    pub translate_x: Option<f32>,
    /// Translation Y in pixels
    pub translate_y: Option<f32>,
    /// Rotation in degrees (Z-axis)
    pub rotate: Option<f32>,
    /// Rotation X in degrees (3D tilt)
    pub rotate_x: Option<f32>,
    /// Rotation Y in degrees (3D turn)
    pub rotate_y: Option<f32>,
    /// Perspective distance in pixels
    pub perspective: Option<f32>,
    /// 3D extrusion depth in pixels
    pub depth: Option<f32>,
    /// Z-axis translation in pixels (positive = toward viewer)
    pub translate_z: Option<f32>,
    /// Blend radius for smooth boolean operations (in pixels)
    pub blend_3d: Option<f32>,
    /// Clip-path inset [top%, right%, bottom%, left%]
    pub clip_inset: Option<[f32; 4]>,

    // --- Color properties ---
    /// Background color RGBA (solid brush)
    pub background_color: Option<[f32; 4]>,
    /// Gradient start color RGBA (first stop)
    pub gradient_start_color: Option<[f32; 4]>,
    /// Gradient end color RGBA (last stop)
    pub gradient_end_color: Option<[f32; 4]>,
    /// Gradient angle in degrees (linear gradients)
    pub gradient_angle: Option<f32>,
    /// Border color RGBA
    pub border_color: Option<[f32; 4]>,
    /// Text foreground color RGBA
    pub text_color: Option<[f32; 4]>,

    // --- Geometric properties ---
    /// Corner radius [top_left, top_right, bottom_right, bottom_left]
    pub corner_radius: Option<[f32; 4]>,
    /// Corner shape (superellipse n parameter) [top_left, top_right, bottom_right, bottom_left]
    pub corner_shape: Option<[f32; 4]>,
    /// Border width in pixels
    pub border_width: Option<f32>,
    /// Outline width in pixels
    pub outline_width: Option<f32>,
    /// Outline color RGBA
    pub outline_color: Option<[f32; 4]>,
    /// Outline offset in pixels
    pub outline_offset: Option<f32>,
    /// Clip-path circle radius (percent)
    pub clip_circle_radius: Option<f32>,
    /// Clip-path ellipse radii [rx, ry] (percent)
    pub clip_ellipse_radii: Option<[f32; 2]>,
    /// Overflow fade distances [top, right, bottom, left]
    pub overflow_fade: Option<[f32; 4]>,

    // --- Shadow properties ---
    /// Shadow [offset_x, offset_y, blur, spread] for the first layer.
    /// Kept alongside [`Self::shadow_stack`] for the single-layer fast paths
    /// (e.g. composite-promoted `CssAnimPaintMeta` in motion.rs) that
    /// only sample one layer.
    pub shadow_params: Option<[f32; 4]>,
    /// Shadow color RGBA for the first layer (mirrors `shadow_params`).
    pub shadow_color: Option<[f32; 4]>,
    /// Full compound shadow stack — each entry is
    /// `[offset_x, offset_y, blur, spread, r, g, b, a]`. When set this
    /// is the canonical storage; the consumer applies the whole stack
    /// to `RenderProps.shadow` rather than just the first layer. Empty
    /// `Vec` means "no shadow" (so transitioning from a shadow to none
    /// fades the stack out instead of popping).
    pub shadow_stack: Option<Vec<[f32; 8]>>,
    /// Text shadow [offset_x, offset_y, blur, spread]
    pub text_shadow_params: Option<[f32; 4]>,
    /// Text shadow color RGBA
    pub text_shadow_color: Option<[f32; 4]>,

    // --- 3D lighting ---
    /// Light intensity (0.0-1.0+)
    pub light_intensity: Option<f32>,
    /// Ambient light level (0.0-1.0)
    pub ambient: Option<f32>,
    /// Specular power (higher = sharper highlights)
    pub specular: Option<f32>,
    /// Light direction [x, y, z]
    pub light_direction: Option<[f32; 3]>,

    // --- CSS filter properties ---
    /// filter: grayscale(0..1)
    pub filter_grayscale: Option<f32>,
    /// filter: invert(0..1)
    pub filter_invert: Option<f32>,
    /// filter: sepia(0..1)
    pub filter_sepia: Option<f32>,
    /// filter: brightness(multiplier, 1.0 = normal)
    pub filter_brightness: Option<f32>,
    /// filter: contrast(multiplier, 1.0 = normal)
    pub filter_contrast: Option<f32>,
    /// filter: saturate(multiplier, 1.0 = normal)
    pub filter_saturate: Option<f32>,
    /// filter: hue-rotate(degrees)
    pub filter_hue_rotate: Option<f32>,
    /// filter: blur(px)
    pub filter_blur: Option<f32>,

    // --- Backdrop filter (glass material) ---
    /// backdrop-filter: blur(px)
    pub backdrop_blur: Option<f32>,
    /// backdrop-filter saturation multiplier (1.0 = normal)
    pub backdrop_saturation: Option<f32>,
    /// backdrop-filter brightness multiplier (1.0 = normal)
    pub backdrop_brightness: Option<f32>,

    // --- Layout properties (require layout recomputation) ---
    /// Width in pixels
    pub width: Option<f32>,
    /// Height in pixels
    pub height: Option<f32>,
    /// Min-width in pixels
    pub min_width: Option<f32>,
    /// Max-width in pixels
    pub max_width: Option<f32>,
    /// Min-height in pixels
    pub min_height: Option<f32>,
    /// Max-height in pixels
    pub max_height: Option<f32>,
    /// Padding [top, right, bottom, left] in pixels
    pub padding: Option<[f32; 4]>,
    /// Margin [top, right, bottom, left] in pixels
    pub margin: Option<[f32; 4]>,
    /// Gap between flex items in pixels
    pub gap: Option<f32>,
    /// Flex grow factor
    pub flex_grow: Option<f32>,
    /// Flex shrink factor
    pub flex_shrink: Option<f32>,

    // --- Positioning (require layout recomputation) ---
    /// top inset in pixels
    pub inset_top: Option<f32>,
    /// right inset in pixels
    pub inset_right: Option<f32>,
    /// bottom inset in pixels
    pub inset_bottom: Option<f32>,
    /// left inset in pixels
    pub inset_left: Option<f32>,

    // --- Text properties ---
    /// Font size in pixels
    pub font_size: Option<f32>,

    // --- Skew ---
    /// Skew X in degrees
    pub skew_x: Option<f32>,
    /// Skew Y in degrees
    pub skew_y: Option<f32>,

    // --- Transform origin ---
    /// Transform origin [x%, y%] (default 50%, 50% = center)
    pub transform_origin: Option<[f32; 2]>,

    // --- Stacking ---
    /// z-index (f32 for smooth interpolation, rounded on apply)
    pub z_index: Option<f32>,

    // --- Mask gradient ---
    /// Combined mask gradient: [mask_type, start_alpha, end_alpha, 0, p0, p1, p2, p3]
    /// where p0..p3 = linear(x1,y1,x2,y2) or radial(cx,cy,r,0) in OBB space
    pub mask_gradient: Option<[f32; 8]>,

    // --- SVG properties ---
    /// SVG fill color RGBA
    pub svg_fill: Option<[f32; 4]>,
    /// SVG stroke color RGBA
    pub svg_stroke: Option<[f32; 4]>,
    /// SVG stroke width in pixels
    pub svg_stroke_width: Option<f32>,
    /// SVG stroke-dashoffset in pixels (for line-drawing animations)
    pub svg_stroke_dashoffset: Option<f32>,
    /// SVG path `d` attribute data (for path morphing animations)
    pub svg_path_data: Option<String>,
}

impl KeyframeProperties {
    /// Whether any property tracked here demands native-vsync smoothness
    /// to look right.
    ///
    /// "Yes" properties cause visible stair-stepping or layout-shift
    /// jank if rendered below the display refresh rate — transforms
    /// (translate / scale / rotate / skew), 3D rotation, layout
    /// sizing (width / height / padding / margin / gap / inset),
    /// font-size, and clip-path animations all fall here.
    ///
    /// "No" properties (the default for fields not listed below) are
    /// gentle visual changes — opacity, colors, shadows, filters,
    /// border/outline thickness, corner radius/shape, light shading.
    /// They look indistinguishable at 30 fps vs 60 fps for slow
    /// animation durations and tolerate `WindowConfig::animation_fps_cap`
    /// without visible degradation.
    ///
    /// Used by the windowed app to decide, per frame, whether the
    /// configured animation FPS cap should apply: if any visible
    /// animation is touching a vsync-class property, the cap is
    /// bypassed and that frame ships at native vsync. Capped frames
    /// only happen when every visible animation is gentle.
    pub fn needs_vsync_for_smoothness(&self) -> bool {
        // Transforms (motion / orientation / size).
        self.translate_x.is_some()
            || self.translate_y.is_some()
            || self.translate_z.is_some()
            || self.scale_x.is_some()
            || self.scale_y.is_some()
            || self.rotate.is_some()
            || self.rotate_x.is_some()
            || self.rotate_y.is_some()
            || self.skew_x.is_some()
            || self.skew_y.is_some()
            || self.perspective.is_some()
            || self.depth.is_some()
            // Layout sizing & positioning (any change is a layout shift).
            || self.width.is_some()
            || self.height.is_some()
            || self.min_width.is_some()
            || self.max_width.is_some()
            || self.min_height.is_some()
            || self.max_height.is_some()
            || self.padding.is_some()
            || self.margin.is_some()
            || self.gap.is_some()
            || self.flex_grow.is_some()
            || self.flex_shrink.is_some()
            || self.inset_top.is_some()
            || self.inset_right.is_some()
            || self.inset_bottom.is_some()
            || self.inset_left.is_some()
            // Font size — text reflow is layout-shift jank.
            || self.font_size.is_some()
            // Clip-path reveals — geometry change, jank visible.
            || self.clip_inset.is_some()
            || self.clip_circle_radius.is_some()
            || self.clip_ellipse_radii.is_some()
    }

    /// Whether every property tracked here can be implemented by
    /// transforming + alpha-blending a pre-rasterized texture, with
    /// no per-frame re-rasterization required.
    ///
    /// First-cut composite-promotable set:
    ///
    /// - `opacity` (alpha multiplier at composite time)
    /// - `translate_x` / `translate_y` (dest-rect translation)
    /// - `scale_x` / `scale_y` (dest-rect size)
    ///
    /// 2D `rotate` is technically composite-friendly but
    /// `blit_tight_texture_to_target` doesn't have a `rotate_z`
    /// path yet — promoting rotation animations today would
    /// render them without the rotation applied. Tracked as a
    /// follow-up; until then we keep rotation animations on the
    /// slow path.
    ///
    /// Used by the compositor's composited-layer path: when a CSS-
    /// animated node's `current_properties` is composite-promotable,
    /// the walker rasterizes its subtree once into a `LayerTexture`
    /// and each animation frame just blits the texture with the
    /// current transform / opacity applied — bypassing the full
    /// walker re-emission cost.
    ///
    /// Returns `false` if any property outside the set above is set
    /// (3D rotation, layout sizing, colour, filter, clip, shadow,
    /// 2D rotate, …). Those properties require re-rasterizing the
    /// subtree on every animation tick, so they take the slow path.
    pub fn is_composite_promotable(&self) -> bool {
        // Out-of-scope: any field that changes shading or geometry
        // beyond a 2D affine + alpha. Inverse of the promotable set
        // — we list the disqualifiers explicitly to keep this
        // predicate locked in when new fields are added (better to
        // omit a new field and force the slow path than silently
        // promote and produce wrong pixels).
        let touches_non_compositable = self.translate_z.is_some()
            || self.rotate_x.is_some()
            || self.rotate_y.is_some()
            || self.skew_x.is_some()
            || self.skew_y.is_some()
            || self.perspective.is_some()
            || self.depth.is_some()
            || self.transform_origin.is_some()
            // Layout — needs taffy recompute + walker re-emit.
            || self.width.is_some()
            || self.height.is_some()
            || self.min_width.is_some()
            || self.max_width.is_some()
            || self.min_height.is_some()
            || self.max_height.is_some()
            || self.padding.is_some()
            || self.margin.is_some()
            || self.gap.is_some()
            || self.flex_grow.is_some()
            || self.flex_shrink.is_some()
            || self.inset_top.is_some()
            || self.inset_right.is_some()
            || self.inset_bottom.is_some()
            || self.inset_left.is_some()
            || self.font_size.is_some()
            || self.z_index.is_some()
            // Visual / shading — would change the rasterized pixels.
            || self.background_color.is_some()
            || self.gradient_start_color.is_some()
            || self.gradient_end_color.is_some()
            || self.gradient_angle.is_some()
            || self.border_color.is_some()
            || self.border_width.is_some()
            || self.outline_width.is_some()
            || self.outline_color.is_some()
            || self.outline_offset.is_some()
            || self.text_color.is_some()
            || self.corner_radius.is_some()
            || self.corner_shape.is_some()
            || self.shadow_params.is_some()
            || self.shadow_color.is_some()
            || self.shadow_stack.is_some()
            || self.text_shadow_params.is_some()
            || self.text_shadow_color.is_some()
            || self.overflow_fade.is_some()
            // Lighting / 3D shading.
            || self.light_intensity.is_some()
            || self.ambient.is_some()
            || self.specular.is_some()
            || self.light_direction.is_some()
            // CSS filters.
            || self.filter_grayscale.is_some()
            || self.filter_invert.is_some()
            || self.filter_sepia.is_some()
            || self.filter_brightness.is_some()
            || self.filter_contrast.is_some()
            || self.filter_saturate.is_some()
            || self.filter_hue_rotate.is_some()
            || self.filter_blur.is_some()
            // Backdrop filters.
            || self.backdrop_blur.is_some()
            || self.backdrop_saturation.is_some()
            || self.backdrop_brightness.is_some()
            // Clip-path geometry.
            || self.clip_inset.is_some()
            || self.clip_circle_radius.is_some()
            || self.clip_ellipse_radii.is_some()
            // Mask + SVG content.
            || self.mask_gradient.is_some()
            || self.svg_fill.is_some()
            || self.svg_stroke.is_some()
            || self.svg_stroke_width.is_some()
            || self.svg_stroke_dashoffset.is_some()
            || self.svg_path_data.is_some();
        if touches_non_compositable {
            return false;
        }
        // 2D rotation is provisionally OUT of the promotable set
        // until the blit shader supports a `rotate_z` path. Without
        // it, promoting `rotate` would render the texture without
        // the rotation applied — visually broken. Track this in
        // touches_non_compositable so a rotate-bearing keyframe
        // takes the slow path even if every other property is
        // promotable.
        //
        // Identity rotation (`Some(0.0)` or near-zero) is BENIGN:
        // the CSS @keyframes parser at `css_parser.rs::style_to_keyframe_properties`
        // fills `rotate = Some(0.0)` whenever ANY transform component
        // is present so that `lerp_opt` interpolates rotate consistently
        // across keyframes (mixing `None` + `Some` would cause a jump).
        // That parser invariant means a translate-or-scale-only @keyframes
        // (no rotation in the CSS at all) still has `rotate=Some(0.0)`
        // in every sampled `current_properties`. Rejecting it here would
        // misroute every such animation through the slow path — observed
        // as cn::context_menu / cn::popover / cn::dropdown surface bg
        // stuck at opacity 0 on canvas-bearing pages (the slow path's
        // `apply_keyframe_props_to_render` writes the interpolated
        // opacity into `RenderProps.opacity`, the SDF emits at color.a
        // ≈ 0, glyphs bypass that propagation and stay visible).
        //
        // The 1e-3 epsilon covers floating-point drift in the lerp
        // between two `Some(0.0)` endpoints. A keyframe with actual
        // non-trivial rotation lands a `current_properties.rotate`
        // well above this threshold and correctly takes the slow path.
        // At the instant a real rotate animation crosses zero, the
        // texture briefly promotes; the rendered output matches the
        // slow-path output at that point (zero rotation = identity),
        // so there's no visible discontinuity.
        // promotable.
        if let Some(r) = self.rotate {
            if r.abs() > 1e-3 {
                return false;
            }
        }
        // At least one promotable property must be Some — an empty
        // properties block means "no animation tick", not "promote
        // an idle subtree".
        self.opacity.is_some()
            || self.translate_x.is_some()
            || self.translate_y.is_some()
            || self.scale_x.is_some()
            || self.scale_y.is_some()
    }

    /// Create properties with only opacity set
    pub fn opacity(value: f32) -> Self {
        Self {
            opacity: Some(value),
            ..Default::default()
        }
    }

    /// Create properties with uniform scale
    pub fn scale(value: f32) -> Self {
        Self {
            scale_x: Some(value),
            scale_y: Some(value),
            ..Default::default()
        }
    }

    /// Create properties with translation
    pub fn translate(x: f32, y: f32) -> Self {
        Self {
            translate_x: Some(x),
            translate_y: Some(y),
            ..Default::default()
        }
    }

    /// Create properties with rotation
    pub fn rotation(degrees: f32) -> Self {
        Self {
            rotate: Some(degrees),
            ..Default::default()
        }
    }

    /// Builder: set opacity
    pub fn with_opacity(mut self, value: f32) -> Self {
        self.opacity = Some(value);
        self
    }

    /// Builder: set uniform scale
    pub fn with_scale(mut self, value: f32) -> Self {
        self.scale_x = Some(value);
        self.scale_y = Some(value);
        self
    }

    /// Builder: set scale x and y separately
    pub fn with_scale_xy(mut self, x: f32, y: f32) -> Self {
        self.scale_x = Some(x);
        self.scale_y = Some(y);
        self
    }

    /// Builder: set translation
    pub fn with_translate(mut self, x: f32, y: f32) -> Self {
        self.translate_x = Some(x);
        self.translate_y = Some(y);
        self
    }

    /// Builder: set rotation
    pub fn with_rotate(mut self, degrees: f32) -> Self {
        self.rotate = Some(degrees);
        self
    }

    /// Builder: set X rotation (3D tilt)
    pub fn with_rotate_x(mut self, degrees: f32) -> Self {
        self.rotate_x = Some(degrees);
        self
    }

    /// Builder: set Y rotation (3D turn)
    pub fn with_rotate_y(mut self, degrees: f32) -> Self {
        self.rotate_y = Some(degrees);
        self
    }

    /// Builder: set perspective distance
    pub fn with_perspective(mut self, px: f32) -> Self {
        self.perspective = Some(px);
        self
    }

    /// Builder: set 3D depth
    pub fn with_depth(mut self, px: f32) -> Self {
        self.depth = Some(px);
        self
    }

    /// Builder: set translate-z
    pub fn with_translate_z(mut self, px: f32) -> Self {
        self.translate_z = Some(px);
        self
    }

    /// Builder: set 3D blend radius
    pub fn with_blend_3d(mut self, px: f32) -> Self {
        self.blend_3d = Some(px);
        self
    }

    /// Interpolate between two property sets
    pub fn lerp(&self, other: &Self, t: f32) -> Self {
        Self {
            opacity: lerp_opt(self.opacity, other.opacity, t),
            scale_x: lerp_opt(self.scale_x, other.scale_x, t),
            scale_y: lerp_opt(self.scale_y, other.scale_y, t),
            translate_x: lerp_opt(self.translate_x, other.translate_x, t),
            translate_y: lerp_opt(self.translate_y, other.translate_y, t),
            rotate: lerp_opt(self.rotate, other.rotate, t),
            rotate_x: lerp_opt(self.rotate_x, other.rotate_x, t),
            rotate_y: lerp_opt(self.rotate_y, other.rotate_y, t),
            perspective: lerp_opt(self.perspective, other.perspective, t),
            depth: lerp_opt(self.depth, other.depth, t),
            translate_z: lerp_opt(self.translate_z, other.translate_z, t),
            blend_3d: lerp_opt(self.blend_3d, other.blend_3d, t),
            clip_inset: lerp_opt_array4(self.clip_inset, other.clip_inset, t),
            // Color
            background_color: lerp_opt_array4(self.background_color, other.background_color, t),
            gradient_start_color: lerp_opt_array4(
                self.gradient_start_color,
                other.gradient_start_color,
                t,
            ),
            gradient_end_color: lerp_opt_array4(
                self.gradient_end_color,
                other.gradient_end_color,
                t,
            ),
            gradient_angle: lerp_opt(self.gradient_angle, other.gradient_angle, t),
            border_color: lerp_opt_array4(self.border_color, other.border_color, t),
            text_color: lerp_opt_array4(self.text_color, other.text_color, t),
            // Geometric
            corner_radius: lerp_opt_array4(self.corner_radius, other.corner_radius, t),
            corner_shape: lerp_opt_array4(self.corner_shape, other.corner_shape, t),
            border_width: lerp_opt(self.border_width, other.border_width, t),
            outline_width: lerp_opt(self.outline_width, other.outline_width, t),
            outline_color: lerp_opt_array4(self.outline_color, other.outline_color, t),
            outline_offset: lerp_opt(self.outline_offset, other.outline_offset, t),
            clip_circle_radius: lerp_opt(self.clip_circle_radius, other.clip_circle_radius, t),
            clip_ellipse_radii: lerp_opt_array2(
                self.clip_ellipse_radii,
                other.clip_ellipse_radii,
                t,
            ),
            overflow_fade: lerp_opt_array4(self.overflow_fade, other.overflow_fade, t),
            // Shadow
            shadow_params: lerp_opt_array4(self.shadow_params, other.shadow_params, t),
            shadow_color: lerp_opt_array4(self.shadow_color, other.shadow_color, t),
            shadow_stack: lerp_opt_shadow_stack(
                self.shadow_stack.as_deref(),
                other.shadow_stack.as_deref(),
                t,
            ),
            text_shadow_params: lerp_opt_array4(
                self.text_shadow_params,
                other.text_shadow_params,
                t,
            ),
            text_shadow_color: lerp_opt_array4(self.text_shadow_color, other.text_shadow_color, t),
            // 3D lighting
            light_intensity: lerp_opt(self.light_intensity, other.light_intensity, t),
            ambient: lerp_opt(self.ambient, other.ambient, t),
            specular: lerp_opt(self.specular, other.specular, t),
            light_direction: lerp_opt_array3(self.light_direction, other.light_direction, t),
            // Filters
            filter_grayscale: lerp_opt(self.filter_grayscale, other.filter_grayscale, t),
            filter_invert: lerp_opt(self.filter_invert, other.filter_invert, t),
            filter_sepia: lerp_opt(self.filter_sepia, other.filter_sepia, t),
            filter_brightness: lerp_opt(self.filter_brightness, other.filter_brightness, t),
            filter_contrast: lerp_opt(self.filter_contrast, other.filter_contrast, t),
            filter_saturate: lerp_opt(self.filter_saturate, other.filter_saturate, t),
            filter_hue_rotate: lerp_opt(self.filter_hue_rotate, other.filter_hue_rotate, t),
            filter_blur: lerp_opt(self.filter_blur, other.filter_blur, t),
            // Backdrop filter
            backdrop_blur: lerp_opt(self.backdrop_blur, other.backdrop_blur, t),
            backdrop_saturation: lerp_opt(self.backdrop_saturation, other.backdrop_saturation, t),
            backdrop_brightness: lerp_opt(self.backdrop_brightness, other.backdrop_brightness, t),
            // Layout
            width: lerp_opt(self.width, other.width, t),
            height: lerp_opt(self.height, other.height, t),
            min_width: lerp_opt(self.min_width, other.min_width, t),
            max_width: lerp_opt(self.max_width, other.max_width, t),
            min_height: lerp_opt(self.min_height, other.min_height, t),
            max_height: lerp_opt(self.max_height, other.max_height, t),
            padding: lerp_opt_array4(self.padding, other.padding, t),
            margin: lerp_opt_array4(self.margin, other.margin, t),
            gap: lerp_opt(self.gap, other.gap, t),
            flex_grow: lerp_opt(self.flex_grow, other.flex_grow, t),
            flex_shrink: lerp_opt(self.flex_shrink, other.flex_shrink, t),
            // Positioning
            inset_top: lerp_opt(self.inset_top, other.inset_top, t),
            inset_right: lerp_opt(self.inset_right, other.inset_right, t),
            inset_bottom: lerp_opt(self.inset_bottom, other.inset_bottom, t),
            inset_left: lerp_opt(self.inset_left, other.inset_left, t),
            // Text properties
            font_size: lerp_opt(self.font_size, other.font_size, t),
            // Skew
            skew_x: lerp_opt(self.skew_x, other.skew_x, t),
            skew_y: lerp_opt(self.skew_y, other.skew_y, t),
            // Transform origin
            transform_origin: lerp_opt_array2(self.transform_origin, other.transform_origin, t),
            // Stacking
            z_index: lerp_opt(self.z_index, other.z_index, t),
            // Mask gradient
            mask_gradient: lerp_opt_array8(self.mask_gradient, other.mask_gradient, t),
            // SVG
            svg_fill: lerp_opt_array4(self.svg_fill, other.svg_fill, t),
            svg_stroke: lerp_opt_array4(self.svg_stroke, other.svg_stroke, t),
            svg_stroke_width: lerp_opt(self.svg_stroke_width, other.svg_stroke_width, t),
            svg_stroke_dashoffset: lerp_opt(
                self.svg_stroke_dashoffset,
                other.svg_stroke_dashoffset,
                t,
            ),
            svg_path_data: lerp_path_data(&self.svg_path_data, &other.svg_path_data, t),
        }
    }

    /// Get the resolved opacity (defaults to 1.0 if not set)
    pub fn resolved_opacity(&self) -> f32 {
        self.opacity.unwrap_or(1.0)
    }

    /// Get the resolved scale (defaults to 1.0 if not set)
    pub fn resolved_scale(&self) -> (f32, f32) {
        (self.scale_x.unwrap_or(1.0), self.scale_y.unwrap_or(1.0))
    }

    /// Get the resolved translation (defaults to 0.0 if not set)
    pub fn resolved_translate(&self) -> (f32, f32) {
        (
            self.translate_x.unwrap_or(0.0),
            self.translate_y.unwrap_or(0.0),
        )
    }

    /// Get the resolved rotation (defaults to 0.0 if not set)
    pub fn resolved_rotate(&self) -> f32 {
        self.rotate.unwrap_or(0.0)
    }

    /// Get the resolved X rotation in degrees (defaults to 0.0)
    pub fn resolved_rotate_x(&self) -> f32 {
        self.rotate_x.unwrap_or(0.0)
    }

    /// Get the resolved Y rotation in degrees (defaults to 0.0)
    pub fn resolved_rotate_y(&self) -> f32 {
        self.rotate_y.unwrap_or(0.0)
    }

    /// Get the resolved perspective distance (defaults to 0.0 = no perspective)
    pub fn resolved_perspective(&self) -> f32 {
        self.perspective.unwrap_or(0.0)
    }

    /// Get the resolved 3D depth (defaults to 0.0 = flat)
    pub fn resolved_depth(&self) -> f32 {
        self.depth.unwrap_or(0.0)
    }
}

/// Helper to interpolate optional values
fn lerp_opt(a: Option<f32>, b: Option<f32>, t: f32) -> Option<f32> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a + (b - a) * t),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

/// Helper to interpolate optional [f32; 2] arrays
fn lerp_opt_array2(a: Option<[f32; 2]>, b: Option<[f32; 2]>, t: f32) -> Option<[f32; 2]> {
    match (a, b) {
        (Some(a), Some(b)) => Some([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

/// Helper to interpolate optional [f32; 3] arrays
fn lerp_opt_array3(a: Option<[f32; 3]>, b: Option<[f32; 3]>, t: f32) -> Option<[f32; 3]> {
    match (a, b) {
        (Some(a), Some(b)) => Some([
            a[0] + (b[0] - a[0]) * t,
            a[1] + (b[1] - a[1]) * t,
            a[2] + (b[2] - a[2]) * t,
        ]),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

/// Helper to interpolate optional SVG path data strings via morph engine
fn lerp_path_data(a: &Option<String>, b: &Option<String>, t: f32) -> Option<String> {
    match (a, b) {
        (Some(from), Some(to)) => {
            crate::morph::interpolate_paths(from, to, t).or_else(|| Some(to.clone()))
        }
        (Some(s), None) => Some(s.clone()),
        (None, Some(s)) => Some(s.clone()),
        (None, None) => None,
    }
}

/// Helper to interpolate optional [f32; 4] arrays
fn lerp_opt_array4(a: Option<[f32; 4]>, b: Option<[f32; 4]>, t: f32) -> Option<[f32; 4]> {
    match (a, b) {
        (Some(a), Some(b)) => Some([
            a[0] + (b[0] - a[0]) * t,
            a[1] + (b[1] - a[1]) * t,
            a[2] + (b[2] - a[2]) * t,
            a[3] + (b[3] - a[3]) * t,
        ]),
        // Fade towards zero when one side is None (e.g. color → transparent,
        // padding → removed). This is correct for CSS transitions where None
        // means "property not set in this state" rather than "keep previous value".
        (Some(a), None) => Some([
            a[0] * (1.0 - t),
            a[1] * (1.0 - t),
            a[2] * (1.0 - t),
            a[3] * (1.0 - t),
        ]),
        (None, Some(b)) => Some([b[0] * t, b[1] * t, b[2] * t, b[3] * t]),
        (None, None) => None,
    }
}

fn lerp_opt_array8(a: Option<[f32; 8]>, b: Option<[f32; 8]>, t: f32) -> Option<[f32; 8]> {
    match (a, b) {
        (Some(a), Some(b)) => Some([
            a[0] + (b[0] - a[0]) * t,
            a[1] + (b[1] - a[1]) * t,
            a[2] + (b[2] - a[2]) * t,
            a[3] + (b[3] - a[3]) * t,
            a[4] + (b[4] - a[4]) * t,
            a[5] + (b[5] - a[5]) * t,
            a[6] + (b[6] - a[6]) * t,
            a[7] + (b[7] - a[7]) * t,
        ]),
        (Some(a), None) => {
            let s = 1.0 - t;
            Some([
                a[0] * s,
                a[1] * s,
                a[2] * s,
                a[3] * s,
                a[4] * s,
                a[5] * s,
                a[6] * s,
                a[7] * s,
            ])
        }
        (None, Some(b)) => Some([
            b[0] * t,
            b[1] * t,
            b[2] * t,
            b[3] * t,
            b[4] * t,
            b[5] * t,
            b[6] * t,
            b[7] * t,
        ]),
        (None, None) => None,
    }
}

/// Element-wise interpolation of a compound shadow stack. When the two
/// stacks differ in length, the shorter side is padded with a fully
/// transparent layer (zeros) so the extra layers fade in / out during
/// the transition instead of popping. Mirrors `box-shadow:` lerp
/// behaviour in CSS Level 4. Returns `None` only when both inputs are
/// `None`.
fn lerp_opt_shadow_stack(
    a: Option<&[[f32; 8]]>,
    b: Option<&[[f32; 8]]>,
    t: f32,
) -> Option<Vec<[f32; 8]>> {
    if a.is_none() && b.is_none() {
        return None;
    }
    let a = a.unwrap_or(&[]);
    let b = b.unwrap_or(&[]);
    let n = a.len().max(b.len());
    let zero = [0.0f32; 8];
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let from = a.get(i).copied().unwrap_or(zero);
        let to = b.get(i).copied().unwrap_or(zero);
        let mut layer = [0.0f32; 8];
        for j in 0..8 {
            layer[j] = from[j] + (to[j] - from[j]) * t;
        }
        out.push(layer);
    }
    Some(out)
}

/// A keyframe with multiple animated properties
#[derive(Clone, Debug)]
pub struct MultiKeyframe {
    /// Time position (0.0 to 1.0)
    pub time: f32,
    /// Properties at this keyframe
    pub properties: KeyframeProperties,
    /// Easing function to use when transitioning TO this keyframe
    pub easing: Easing,
}

impl MultiKeyframe {
    /// Create a new multi-property keyframe
    pub fn new(time: f32, properties: KeyframeProperties, easing: Easing) -> Self {
        Self {
            time,
            properties,
            easing,
        }
    }
}

/// Playback direction for animations
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlayDirection {
    /// Play forward (0.0 -> 1.0)
    #[default]
    Forward,
    /// Play in reverse (1.0 -> 0.0)
    Reverse,
    /// Alternate between forward and reverse each iteration
    Alternate,
}

/// Fill mode determines the animation state before/after playback
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FillMode {
    /// No fill - reset to initial state after animation
    #[default]
    None,
    /// Hold the final keyframe value after animation completes
    Forwards,
    /// Apply the first keyframe value before animation starts
    Backwards,
    /// Apply both forwards and backwards fill
    Both,
}

/// Multi-property keyframe animation
#[derive(Clone, Debug)]
pub struct MultiKeyframeAnimation {
    /// Cache identity shared by snapshots of the same playback.
    playback_id: u64,
    /// Duration in milliseconds
    duration_ms: u32,
    /// Keyframes sorted by time
    keyframes: Vec<MultiKeyframe>,
    /// Current time in milliseconds
    current_time: f32,
    /// Whether animation is playing
    playing: bool,
    /// Playback direction
    direction: PlayDirection,
    /// Fill mode
    fill_mode: FillMode,
    /// Number of iterations (-1 for infinite)
    iterations: i32,
    /// Current iteration count
    current_iteration: i32,
    /// Whether currently playing in reverse (for Alternate)
    reversed: bool,
    /// Delay before animation starts (ms)
    delay_ms: u32,
}

static NEXT_MULTI_KEYFRAME_PLAYBACK_ID: AtomicU64 = AtomicU64::new(1);

impl MultiKeyframeAnimation {
    /// Create a new multi-property animation with given duration
    pub fn new(duration_ms: u32) -> Self {
        Self {
            playback_id: NEXT_MULTI_KEYFRAME_PLAYBACK_ID.fetch_add(1, Ordering::Relaxed),
            duration_ms,
            keyframes: Vec::new(),
            current_time: 0.0,
            playing: false,
            direction: PlayDirection::Forward,
            fill_mode: FillMode::Forwards,
            iterations: 1,
            current_iteration: 0,
            reversed: false,
            delay_ms: 0,
        }
    }

    /// Add a keyframe to the animation (builder pattern)
    pub fn keyframe(mut self, time: f32, properties: KeyframeProperties, easing: Easing) -> Self {
        self.keyframes
            .push(MultiKeyframe::new(time, properties, easing));
        // Keep keyframes sorted by time
        self.keyframes
            .sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());
        self
    }

    /// Set playback direction
    pub fn direction(mut self, direction: PlayDirection) -> Self {
        self.direction = direction;
        self
    }

    /// Set fill mode
    pub fn fill_mode(mut self, fill_mode: FillMode) -> Self {
        self.fill_mode = fill_mode;
        self
    }

    /// Set number of iterations (-1 for infinite)
    pub fn iterations(mut self, count: i32) -> Self {
        self.iterations = count;
        self
    }

    /// Set delay before animation starts
    pub fn delay(mut self, delay_ms: u32) -> Self {
        self.delay_ms = delay_ms;
        self
    }

    // =========================================================================
    // Mutating setters (for post-construction configuration)
    // =========================================================================

    /// Set playback direction (mutating)
    pub fn set_direction(&mut self, direction: PlayDirection) {
        self.direction = direction;
    }

    /// Set fill mode (mutating)
    pub fn set_fill_mode(&mut self, fill_mode: FillMode) {
        self.fill_mode = fill_mode;
    }

    /// Set number of iterations (mutating, -1 for infinite)
    pub fn set_iterations(&mut self, count: i32) {
        self.iterations = count;
    }

    /// Set delay before animation starts (mutating)
    pub fn set_delay(&mut self, delay_ms: u32) {
        self.delay_ms = delay_ms;
    }

    /// Set reversed state (mutating)
    ///
    /// When true, animation plays in reverse direction for the current iteration.
    /// Used internally for Alternate direction and for AlternateReverse start.
    pub fn set_reversed(&mut self, reversed: bool) {
        self.reversed = reversed;
    }

    /// Start the animation
    pub fn start(&mut self) {
        self.playback_id = NEXT_MULTI_KEYFRAME_PLAYBACK_ID.fetch_add(1, Ordering::Relaxed);
        self.current_time = -(self.delay_ms as f32);
        self.current_iteration = 0;
        self.reversed = self.direction == PlayDirection::Reverse;
        self.playing = true;
    }

    /// Stop the animation
    pub fn stop(&mut self) {
        self.playing = false;
    }

    /// Process-local identity for cache ownership of this playback.
    ///
    /// Construction and explicit starts assign fresh identities. Ticks, loops,
    /// completion and snapshot clones retain it. Start a clone for independent playback.
    pub fn playback_id(&self) -> u64 {
        self.playback_id
    }

    /// Check if the animation is currently playing
    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// Get the current progress (0.0 to 1.0)
    pub fn progress(&self) -> f32 {
        if self.current_time < 0.0 {
            return 0.0;
        }
        (self.current_time / self.duration_ms as f32).clamp(0.0, 1.0)
    }

    /// Get the current interpolated properties
    pub fn current_properties(&self) -> KeyframeProperties {
        if self.keyframes.is_empty() {
            return KeyframeProperties::default();
        }

        // Handle delay period
        if self.current_time < 0.0 {
            return match self.fill_mode {
                FillMode::Backwards | FillMode::Both => self.keyframes[0].properties.clone(),
                _ => KeyframeProperties::default(),
            };
        }

        let mut progress = self.progress();

        // Apply reverse if needed
        if self.reversed {
            progress = 1.0 - progress;
        }

        // Find surrounding keyframes
        let mut prev_kf = &self.keyframes[0];
        let mut next_kf = &self.keyframes[0];

        for kf in &self.keyframes {
            if kf.time <= progress {
                prev_kf = kf;
            }
            if kf.time >= progress {
                next_kf = kf;
                break;
            }
        }

        if (prev_kf.time - next_kf.time).abs() < f32::EPSILON {
            return prev_kf.properties.clone();
        }

        // Interpolate between keyframes
        let local_progress = (progress - prev_kf.time) / (next_kf.time - prev_kf.time);
        let eased = next_kf.easing.apply(local_progress);

        prev_kf.properties.lerp(&next_kf.properties, eased)
    }

    /// Advance the animation by delta time (in milliseconds)
    pub fn tick(&mut self, dt_ms: f32) {
        if !self.playing {
            return;
        }

        self.current_time += dt_ms;

        // Check if we've completed an iteration
        if self.current_time >= self.duration_ms as f32 {
            self.current_iteration += 1;

            // Check if we should continue (infinite or more iterations remaining)
            if self.iterations < 0 || self.current_iteration < self.iterations {
                // Reset for next iteration
                self.current_time = 0.0;

                // Handle alternate direction
                if self.direction == PlayDirection::Alternate {
                    self.reversed = !self.reversed;
                }
            } else {
                // Animation complete
                self.current_time = self.duration_ms as f32;
                self.playing = false;
            }
        }
    }

    /// Get the duration in milliseconds
    pub fn duration_ms(&self) -> u32 {
        self.duration_ms
    }

    /// Get the delay in milliseconds
    pub fn delay_ms(&self) -> u32 {
        self.delay_ms
    }

    /// Get total duration including delay
    pub fn total_duration_ms(&self) -> u32 {
        self.delay_ms + self.duration_ms
    }

    /// Get the first keyframe (if any)
    pub fn first_keyframe(&self) -> Option<&MultiKeyframe> {
        self.keyframes.first()
    }

    /// Get the last keyframe (if any)
    pub fn last_keyframe(&self) -> Option<&MultiKeyframe> {
        self.keyframes.last()
    }

    /// Get all keyframes
    pub fn keyframes(&self) -> &[MultiKeyframe] {
        &self.keyframes
    }

    /// Sample the animation at a specific progress (0.0 to 1.0)
    ///
    /// This is useful for externally-timed animations where you manage
    /// the progress yourself rather than using tick().
    pub fn sample_at(&self, progress: f32) -> KeyframeProperties {
        if self.keyframes.is_empty() {
            return KeyframeProperties::default();
        }

        let progress = progress.clamp(0.0, 1.0);

        // Find surrounding keyframes
        let mut prev_kf = &self.keyframes[0];
        let mut next_kf = &self.keyframes[0];

        for kf in &self.keyframes {
            if kf.time <= progress {
                prev_kf = kf;
            }
            if kf.time >= progress {
                next_kf = kf;
                break;
            }
        }

        if (prev_kf.time - next_kf.time).abs() < f32::EPSILON {
            return prev_kf.properties.clone();
        }

        // Interpolate between keyframes
        let local_progress = (progress - prev_kf.time) / (next_kf.time - prev_kf.time);
        let eased = next_kf.easing.apply(local_progress);

        prev_kf.properties.lerp(&next_kf.properties, eased)
    }
}

impl Default for MultiKeyframeAnimation {
    fn default() -> Self {
        Self::new(300)
    }
}

// ============================================================================
// Keyframe Track Builder - Fluent API for single-value keyframe animations
// ============================================================================

/// A point in time with a value for keyframe animation
#[derive(Clone, Debug)]
pub struct KeyframePoint {
    /// Time in milliseconds
    pub time_ms: u32,
    /// Value at this time
    pub value: f32,
    /// Easing to use when transitioning TO this point
    pub easing: Easing,
}

/// Builder for creating keyframe animations with a fluent API
///
/// # Example
///
/// ```ignore
/// let track = KeyframeTrackBuilder::new()
///     .at(0, 0.8)
///     .at(800, 1.2)
///     .ease(Easing::EaseInOut)
///     .ping_pong()
///     .loop_infinite()
///     .build();
/// ```
#[derive(Clone, Debug)]
pub struct KeyframeTrackBuilder {
    /// Keyframe points (time_ms, value)
    points: Vec<KeyframePoint>,
    /// Default easing for all keyframes (can be overridden per-keyframe)
    default_easing: Easing,
    /// Play direction
    direction: PlayDirection,
    /// Number of iterations (-1 for infinite)
    iterations: i32,
    /// Delay before starting (ms)
    delay_ms: u32,
    /// Fill mode
    fill_mode: FillMode,
    /// Whether to auto-start
    auto_start: bool,
}

impl Default for KeyframeTrackBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyframeTrackBuilder {
    /// Create a new keyframe track builder
    pub fn new() -> Self {
        Self {
            points: Vec::new(),
            default_easing: Easing::Linear,
            direction: PlayDirection::Forward,
            iterations: 1,
            delay_ms: 0,
            fill_mode: FillMode::Forwards,
            auto_start: false,
        }
    }

    /// Add a keyframe at a specific time (in milliseconds)
    pub fn at(mut self, time_ms: u32, value: f32) -> Self {
        self.points.push(KeyframePoint {
            time_ms,
            value,
            easing: self.default_easing,
        });
        self
    }

    /// Add a keyframe with specific easing
    pub fn at_with_ease(mut self, time_ms: u32, value: f32, easing: Easing) -> Self {
        self.points.push(KeyframePoint {
            time_ms,
            value,
            easing,
        });
        self
    }

    /// Set the default easing for all keyframes
    ///
    /// This applies to keyframes added after this call, and updates
    /// existing keyframes that haven't had explicit easing set.
    pub fn ease(mut self, easing: Easing) -> Self {
        self.default_easing = easing;
        // Update existing points that use the default
        for point in &mut self.points {
            point.easing = easing;
        }
        self
    }

    /// Enable ping-pong mode (alternate between forward and reverse)
    ///
    /// The animation plays forward, then backward, creating a smooth
    /// back-and-forth effect without needing duplicate keyframes.
    pub fn ping_pong(mut self) -> Self {
        self.direction = PlayDirection::Alternate;
        self
    }

    /// Alias for ping_pong()
    pub fn alternate(self) -> Self {
        self.ping_pong()
    }

    /// Play in reverse direction
    pub fn reverse(mut self) -> Self {
        self.direction = PlayDirection::Reverse;
        self
    }

    /// Set the number of loop iterations
    pub fn loop_count(mut self, count: i32) -> Self {
        self.iterations = count;
        self
    }

    /// Loop infinitely
    pub fn loop_infinite(mut self) -> Self {
        self.iterations = -1;
        self
    }

    /// Set delay before animation starts (in milliseconds)
    pub fn delay(mut self, delay_ms: u32) -> Self {
        self.delay_ms = delay_ms;
        self
    }

    /// Set fill mode (what happens before/after animation)
    pub fn fill(mut self, fill_mode: FillMode) -> Self {
        self.fill_mode = fill_mode;
        self
    }

    /// Auto-start the animation when built
    pub fn start(mut self) -> Self {
        self.auto_start = true;
        self
    }

    /// Get the total duration based on keyframe times
    pub fn duration_ms(&self) -> u32 {
        self.points.iter().map(|p| p.time_ms).max().unwrap_or(0)
    }

    /// Build into a KeyframeTrack
    pub fn build(mut self) -> KeyframeTrack {
        // Sort points by time
        self.points.sort_by_key(|p| p.time_ms);

        let duration = self.duration_ms();

        // Convert to normalized keyframes (time as 0.0-1.0)
        let keyframes: Vec<Keyframe> = self
            .points
            .iter()
            .map(|p| {
                let time = if duration > 0 {
                    p.time_ms as f32 / duration as f32
                } else {
                    0.0
                };
                Keyframe {
                    time,
                    value: p.value,
                    easing: p.easing,
                }
            })
            .collect();

        let mut animation = KeyframeAnimation::new(duration, keyframes);

        if self.auto_start {
            animation.start();
        }

        KeyframeTrack {
            animation,
            direction: self.direction,
            iterations: self.iterations,
            current_iteration: 0,
            reversed: self.direction == PlayDirection::Reverse,
            delay_ms: self.delay_ms,
            fill_mode: self.fill_mode,
            duration_ms: duration,
        }
    }
}

/// A complete keyframe track with playback controls
///
/// Wraps KeyframeAnimation with additional features like ping-pong,
/// looping, and fill modes.
#[derive(Clone, Debug)]
pub struct KeyframeTrack {
    animation: KeyframeAnimation,
    direction: PlayDirection,
    iterations: i32,
    current_iteration: i32,
    reversed: bool,
    #[allow(dead_code)] // Reserved for future delay support
    delay_ms: u32,
    #[allow(dead_code)] // Reserved for future fill mode support
    fill_mode: FillMode,
    duration_ms: u32,
}

impl KeyframeTrack {
    /// Create a new builder
    pub fn builder() -> KeyframeTrackBuilder {
        KeyframeTrackBuilder::new()
    }

    /// Start the animation
    pub fn start(&mut self) {
        self.current_iteration = 0;
        self.reversed = self.direction == PlayDirection::Reverse;
        self.animation.start();
    }

    /// Stop the animation
    pub fn stop(&mut self) {
        self.animation.stop();
    }

    /// Restart the animation from the beginning
    pub fn restart(&mut self) {
        self.start();
    }

    /// Check if the animation is playing
    pub fn is_playing(&self) -> bool {
        self.animation.is_playing() || self.should_continue()
    }

    /// Check if we should continue to next iteration
    fn should_continue(&self) -> bool {
        self.iterations < 0 || self.current_iteration < self.iterations
    }

    /// Get the current value with ping-pong and iteration support
    pub fn value(&self) -> f32 {
        let keyframes = self.animation.keyframes();
        if keyframes.is_empty() {
            return 0.0;
        }

        let progress = self.animation.progress().clamp(0.0, 1.0);

        // Apply reverse if in ping-pong mode and on reverse phase
        let effective_progress = if self.reversed {
            1.0 - progress
        } else {
            progress
        };

        // Find surrounding keyframes
        let mut prev_kf = &keyframes[0];
        let mut next_kf = &keyframes[0];

        for kf in keyframes {
            if kf.time <= effective_progress {
                prev_kf = kf;
            }
            if kf.time >= effective_progress {
                next_kf = kf;
                break;
            }
        }

        if (prev_kf.time - next_kf.time).abs() < f32::EPSILON {
            return prev_kf.value;
        }

        // Interpolate between keyframes
        let local_progress = (effective_progress - prev_kf.time) / (next_kf.time - prev_kf.time);
        let eased = next_kf.easing.apply(local_progress);

        prev_kf.value + (next_kf.value - prev_kf.value) * eased
    }

    /// Get the current progress (0.0 to 1.0)
    pub fn progress(&self) -> f32 {
        let progress = self.animation.progress();
        if self.reversed {
            1.0 - progress
        } else {
            progress
        }
    }

    /// Advance the animation by delta time (in milliseconds)
    pub fn tick(&mut self, dt_ms: f32) {
        self.animation.tick(dt_ms);

        // Check if we've completed an iteration
        if self.animation.progress() >= 1.0 && !self.animation.is_playing() {
            self.current_iteration += 1;

            // Check if we should continue
            if self.iterations < 0 || self.current_iteration < self.iterations {
                // Reset for next iteration
                self.animation.start();

                // Handle alternate direction
                if self.direction == PlayDirection::Alternate {
                    self.reversed = !self.reversed;
                }
            }
        }
    }

    /// Get the duration in milliseconds
    pub fn duration_ms(&self) -> u32 {
        self.duration_ms
    }
}

#[cfg(test)]
mod composite_promotable_tests {
    use super::*;

    #[test]
    fn opacity_only_is_promotable() {
        let p = KeyframeProperties {
            opacity: Some(0.5),
            ..Default::default()
        };
        assert!(p.is_composite_promotable());
    }

    #[test]
    fn translate_only_is_promotable() {
        let p = KeyframeProperties {
            translate_x: Some(10.0),
            translate_y: Some(20.0),
            ..Default::default()
        };
        assert!(p.is_composite_promotable());
    }

    #[test]
    fn scale_opacity_combination_is_promotable() {
        let p = KeyframeProperties {
            scale_x: Some(1.5),
            scale_y: Some(1.5),
            opacity: Some(0.7),
            ..Default::default()
        };
        assert!(p.is_composite_promotable());
    }

    #[test]
    fn rotate_disqualifies_pending_shader_support() {
        // 2D rotation is provisionally NOT promotable until the
        // blit shader gains a `rotate_z` path. Promoting today
        // would render the texture un-rotated.
        let p = KeyframeProperties {
            rotate: Some(45.0),
            ..Default::default()
        };
        assert!(!p.is_composite_promotable());
    }

    #[test]
    fn identity_rotate_is_benign() {
        // The CSS @keyframes parser fills rotate=Some(0.0) whenever
        // any transform component is present, even if no keyframe
        // mentions rotation, so lerp_opt stays Some↔Some across all
        // sample times. Treating Some(0.0) as a disqualifier would
        // misroute every translate-or-scale-only @keyframes through
        // the slow path — the regression that hid cn::context_menu /
        // cn::popover / cn::dropdown surface BG on canvas pages until
        // a mouse-move invalidated the cache.
        let p = KeyframeProperties {
            opacity: Some(0.5),
            scale_x: Some(0.96),
            scale_y: Some(0.96),
            translate_x: Some(0.0),
            translate_y: Some(-2.0),
            rotate: Some(0.0),
            ..Default::default()
        };
        assert!(p.is_composite_promotable());
    }

    #[test]
    fn near_zero_rotate_within_epsilon_is_benign() {
        // Float drift in the lerp between two Some(0.0) endpoints
        // can produce values like 1e-7. Stay promotable.
        let p = KeyframeProperties {
            opacity: Some(0.5),
            rotate: Some(1e-7),
            ..Default::default()
        };
        assert!(p.is_composite_promotable());
    }

    #[test]
    fn width_disqualifies() {
        let p = KeyframeProperties {
            opacity: Some(0.5),
            width: Some(100.0),
            ..Default::default()
        };
        assert!(!p.is_composite_promotable());
    }

    #[test]
    fn background_color_disqualifies() {
        let p = KeyframeProperties {
            opacity: Some(0.5),
            background_color: Some([1.0, 0.0, 0.0, 1.0]),
            ..Default::default()
        };
        assert!(!p.is_composite_promotable());
    }

    #[test]
    fn rotate_x_disqualifies() {
        // 3D rotation is out of scope for first cut (uses 3D
        // perspective pipeline, requires different math).
        let p = KeyframeProperties {
            rotate_x: Some(30.0),
            ..Default::default()
        };
        assert!(!p.is_composite_promotable());
    }

    #[test]
    fn filter_blur_disqualifies() {
        let p = KeyframeProperties {
            filter_blur: Some(2.0),
            ..Default::default()
        };
        assert!(!p.is_composite_promotable());
    }

    #[test]
    fn empty_properties_not_promotable() {
        // An empty properties block means "no animation tick happened",
        // not "promote this static subtree". Predicate must return
        // false so we don't allocate layer textures for non-animating
        // nodes.
        let p = KeyframeProperties::default();
        assert!(!p.is_composite_promotable());
    }
}
