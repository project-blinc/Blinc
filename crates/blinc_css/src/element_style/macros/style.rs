//! The `style!` macro: an `ElementStyle` written in builder names.

/// Rust-friendly macro for creating ElementStyle with builder-like syntax
///
/// Uses Rust naming conventions (underscores instead of hyphens).
/// Comma-separated properties with colon syntax.
///
/// # Examples
///
/// ```ignore
/// use blinc_layout::prelude::*;
/// use blinc_core::Color;
///
/// // Basic usage with property: value syntax
/// let s = style! {
///     bg: Color::BLUE,
///     rounded: 8.0,
///     opacity: 0.9,
/// };
///
/// // Preset methods (no value needed)
/// let card = style! {
///     bg: Color::WHITE,
///     rounded_lg,
///     shadow_md,
/// };
///
/// // Transform shortcuts
/// let hover = style! {
///     scale: 1.05,
///     rotate_deg: 15.0,
///     translate: (10.0, 5.0),
/// };
///
/// // Material effects
/// let glass_panel = style! {
///     glass,
///     rounded: 16.0,
/// };
/// ```
#[macro_export]
macro_rules! style {
    // Empty style
    () => {
        $crate::element_style::ElementStyle::new()
    };

    // Main entry point - parse properties
    ($($tokens:tt)*) => {{
        let mut __style = $crate::element_style::ElementStyle::new();
        $crate::style_impl!(__style; $($tokens)*);
        __style
    }};
}

/// Internal macro for parsing style properties (Rust-style)
#[macro_export]
#[doc(hidden)]
macro_rules! style_impl {
    // Base case - no more tokens
    ($style:ident;) => {};

    // =========================================================================
    // Background properties
    // =========================================================================
    ($style:ident; bg: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.bg($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; background: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.background($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Corner radius properties
    // =========================================================================
    ($style:ident; rounded: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.rounded($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; rounded_corners: ($tl:expr, $tr:expr, $br:expr, $bl:expr) $(, $($rest:tt)*)?) => {
        $style = $style.rounded_corners($tl, $tr, $br, $bl);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    // Preset corner radii
    ($style:ident; rounded_sm $(, $($rest:tt)*)?) => {
        $style = $style.rounded_sm();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; rounded_md $(, $($rest:tt)*)?) => {
        $style = $style.rounded_md();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; rounded_lg $(, $($rest:tt)*)?) => {
        $style = $style.rounded_lg();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; rounded_xl $(, $($rest:tt)*)?) => {
        $style = $style.rounded_xl();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; rounded_2xl $(, $($rest:tt)*)?) => {
        $style = $style.rounded_2xl();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; rounded_none $(, $($rest:tt)*)?) => {
        $style = $style.rounded_none();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; rounded_full $(, $($rest:tt)*)?) => {
        $style = $style.rounded_full();
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Corner Shape properties
    // =========================================================================
    ($style:ident; corner_shape: round $(, $($rest:tt)*)?) => {
        $style = $style.corner_shape(1.0);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; corner_shape: bevel $(, $($rest:tt)*)?) => {
        $style = $style.corner_shape(0.0);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; corner_shape: squircle $(, $($rest:tt)*)?) => {
        $style = $style.corner_shape(2.0);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; corner_shape: scoop $(, $($rest:tt)*)?) => {
        $style = $style.corner_shape(-1.0);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; corner_shape: notch $(, $($rest:tt)*)?) => {
        $style = $style.corner_shape(-100.0);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; corner_shape: square $(, $($rest:tt)*)?) => {
        $style = $style.corner_shape(100.0);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; corner_shape: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.corner_shape($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Overflow Fade properties
    // =========================================================================
    ($style:ident; overflow_fade: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.overflow_fade($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Shadow properties
    // =========================================================================
    ($style:ident; shadow: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.shadow($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; shadow_sm $(, $($rest:tt)*)?) => {
        $style = $style.shadow_sm();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; shadow_md $(, $($rest:tt)*)?) => {
        $style = $style.shadow_md();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; shadow_lg $(, $($rest:tt)*)?) => {
        $style = $style.shadow_lg();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; shadow_xl $(, $($rest:tt)*)?) => {
        $style = $style.shadow_xl();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; shadow_none $(, $($rest:tt)*)?) => {
        $style = $style.shadow_none();
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Transform properties
    // =========================================================================
    ($style:ident; transform: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.transform($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; scale: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.scale($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; scale_xy: ($sx:expr, $sy:expr) $(, $($rest:tt)*)?) => {
        $style = $style.scale_xy($sx, $sy);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; translate: ($x:expr, $y:expr) $(, $($rest:tt)*)?) => {
        $style = $style.translate($x, $y);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; rotate: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.rotate($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; rotate_deg: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.rotate_deg($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // 3D Transform properties
    // =========================================================================
    ($style:ident; rotate_x: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.rotate_x_deg($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; rotate_y: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.rotate_y_deg($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; perspective: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.perspective_px($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; translate_z: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.translate_z_px($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // 3D SDF Shape properties
    // =========================================================================
    ($style:ident; shape_3d: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.shape_3d($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; depth: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.depth_px($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // 3D Lighting properties
    // =========================================================================
    ($style:ident; light_direction: ($x:expr, $y:expr, $z:expr) $(, $($rest:tt)*)?) => {
        $style = $style.light_direction($x, $y, $z);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; light_intensity: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.light_intensity($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; ambient: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.ambient_light($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; specular: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.specular_power($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // 3D Boolean Operation properties
    // =========================================================================
    ($style:ident; op_3d: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.op_3d_type($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; blend_3d: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.blend_3d_px($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Clip-Path
    // =========================================================================
    ($style:ident; clip_path: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.clip_path($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Opacity properties
    // =========================================================================
    ($style:ident; opacity: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.opacity($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Flow shader
    // =========================================================================
    ($style:ident; flow: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.flow($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; opaque $(, $($rest:tt)*)?) => {
        $style = $style.opaque();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; translucent $(, $($rest:tt)*)?) => {
        $style = $style.translucent();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; transparent $(, $($rest:tt)*)?) => {
        $style = $style.transparent();
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Material properties
    // =========================================================================
    ($style:ident; material: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.material($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; glass $(, $($rest:tt)*)?) => {
        $style = $style.glass();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; metallic $(, $($rest:tt)*)?) => {
        $style = $style.metallic();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; chrome $(, $($rest:tt)*)?) => {
        $style = $style.chrome();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; gold $(, $($rest:tt)*)?) => {
        $style = $style.gold();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; wood $(, $($rest:tt)*)?) => {
        $style = $style.wood();
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Layer properties
    // =========================================================================
    ($style:ident; layer: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.layer($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; foreground $(, $($rest:tt)*)?) => {
        $style = $style.foreground();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; layer_background $(, $($rest:tt)*)?) => {
        $style = $style.layer_background();
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Animation properties
    // =========================================================================
    ($style:ident; animation: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.animation($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; animation_name: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.animation_name($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; animation_duration: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.animation_duration($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Layout: Sizing
    // =========================================================================
    ($style:ident; w: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.w($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; h: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.h($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; min_w: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.min_w($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; min_h: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.min_h($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; max_w: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.max_w($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; max_h: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.max_h($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Layout: Flex Direction & Display
    // =========================================================================
    ($style:ident; flex_row $(, $($rest:tt)*)?) => {
        $style = $style.flex_row();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; flex_col $(, $($rest:tt)*)?) => {
        $style = $style.flex_col();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; flex_row_reverse $(, $($rest:tt)*)?) => {
        $style = $style.flex_row_reverse();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; flex_col_reverse $(, $($rest:tt)*)?) => {
        $style = $style.flex_col_reverse();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; flex_wrap $(, $($rest:tt)*)?) => {
        $style = $style.flex_wrap();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; display_none $(, $($rest:tt)*)?) => {
        $style = $style.display_none();
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Layout: Flex Properties
    // =========================================================================
    ($style:ident; flex_grow $(, $($rest:tt)*)?) => {
        $style = $style.flex_grow();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; flex_grow_value: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.flex_grow_value($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; flex_shrink_0 $(, $($rest:tt)*)?) => {
        $style = $style.flex_shrink_0();
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Layout: Alignment
    // =========================================================================
    ($style:ident; items_center $(, $($rest:tt)*)?) => {
        $style = $style.items_center();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; items_start $(, $($rest:tt)*)?) => {
        $style = $style.items_start();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; items_end $(, $($rest:tt)*)?) => {
        $style = $style.items_end();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; items_stretch $(, $($rest:tt)*)?) => {
        $style = $style.items_stretch();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; justify_center $(, $($rest:tt)*)?) => {
        $style = $style.justify_center();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; justify_start $(, $($rest:tt)*)?) => {
        $style = $style.justify_start();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; justify_end $(, $($rest:tt)*)?) => {
        $style = $style.justify_end();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; justify_between $(, $($rest:tt)*)?) => {
        $style = $style.justify_between();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; justify_around $(, $($rest:tt)*)?) => {
        $style = $style.justify_around();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; justify_evenly $(, $($rest:tt)*)?) => {
        $style = $style.justify_evenly();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; self_center $(, $($rest:tt)*)?) => {
        $style = $style.self_center();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; self_start $(, $($rest:tt)*)?) => {
        $style = $style.self_start();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; self_end $(, $($rest:tt)*)?) => {
        $style = $style.self_end();
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Layout: Spacing
    // =========================================================================
    ($style:ident; p: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.p($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; p_xy: ($x:expr, $y:expr) $(, $($rest:tt)*)?) => {
        $style = $style.p_xy($x, $y);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; m: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.m($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; m_xy: ($x:expr, $y:expr) $(, $($rest:tt)*)?) => {
        $style = $style.m_xy($x, $y);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; gap: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.gap($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Layout: Overflow
    // =========================================================================
    ($style:ident; overflow_clip $(, $($rest:tt)*)?) => {
        $style = $style.overflow_clip();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; overflow_visible $(, $($rest:tt)*)?) => {
        $style = $style.overflow_visible();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; overflow_scroll $(, $($rest:tt)*)?) => {
        $style = $style.overflow_scroll();
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Layout: Border
    // =========================================================================
    ($style:ident; border: ($width:expr, $color:expr) $(, $($rest:tt)*)?) => {
        $style = $style.border($width, $color);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; border_width: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.border_w($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; border_color: $value:expr $(, $($rest:tt)*)?) => {
        $style.border_color = Some($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Text Properties
    // =========================================================================
    ($style:ident; text_color: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.text_color($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; color: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.text_color($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; font_size: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.font_size($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; font_weight: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.font_weight($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; text_decoration: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.text_decoration($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; text_decoration_color: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.text_decoration_color($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; text_decoration_thickness: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.text_decoration_thickness($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; line_height: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.line_height($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; text_align: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.text_align($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; letter_spacing: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.letter_spacing($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; text_shadow: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.text_shadow($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; text_overflow: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.text_overflow($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; white_space: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.white_space($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Transform Extras
    // =========================================================================
    ($style:ident; skew_x: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.skew_x($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; skew_y: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.skew_y($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; transform_origin: ($x:expr, $y:expr) $(, $($rest:tt)*)?) => {
        $style = $style.transform_origin($x, $y);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Transition
    // =========================================================================
    ($style:ident; transition: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.transition($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Filter
    // =========================================================================
    ($style:ident; filter: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.filter($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Mask Properties
    // =========================================================================
    ($style:ident; mask_image: $value:expr $(, $($rest:tt)*)?) => {
        $style.mask_image = Some($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; mask_gradient: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.mask_gradient($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; mask_mode: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.mask_mode($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Mix Blend Mode
    // =========================================================================
    ($style:ident; mix_blend_mode: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.mix_blend_mode($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Outline
    // =========================================================================
    ($style:ident; outline: ($width:expr, $color:expr) $(, $($rest:tt)*)?) => {
        $style = $style.outline($width, $color);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; outline_width: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.outline_w($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; outline_color: $value:expr $(, $($rest:tt)*)?) => {
        $style.outline_color = Some($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; outline_offset: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.outline_offset($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Overflow per-axis
    // =========================================================================
    ($style:ident; overflow_x: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.overflow_x($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; overflow_y: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.overflow_y($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Position & Inset
    // =========================================================================
    ($style:ident; position: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.position($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; top: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.top($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; right: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.right($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; bottom: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.bottom($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; left: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.left($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; inset: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.inset($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; z_index: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.z_index($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; visibility: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.visibility($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Form Element Colors
    // =========================================================================
    ($style:ident; caret_color: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.caret_color($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; selection_color: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.selection_color($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; placeholder_color: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.placeholder_color($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; accent_color: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.accent_color($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Scrollbar Properties
    // =========================================================================
    ($style:ident; scrollbar_color: ($thumb:expr, $track:expr) $(, $($rest:tt)*)?) => {
        $style = $style.scrollbar_color($thumb, $track);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; scrollbar_width: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.scrollbar_width($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // SVG Properties
    // =========================================================================
    ($style:ident; fill: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.fill($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; stroke: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.stroke($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; stroke_width: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.stroke_width($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; stroke_dasharray: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.stroke_dasharray($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; stroke_dashoffset: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.stroke_dashoffset($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; svg_path_data: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.svg_path_data($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Image Properties
    // =========================================================================
    ($style:ident; object_fit: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.object_fit($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; object_position: ($x:expr, $y:expr) $(, $($rest:tt)*)?) => {
        $style = $style.object_position($x, $y);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Interaction Properties
    // =========================================================================
    ($style:ident; pointer_events: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.pointer_events($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; pointer_events_none $(, $($rest:tt)*)?) => {
        $style = $style.pointer_events_none();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; cursor: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.cursor($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Flex shrink with value
    // =========================================================================
    ($style:ident; flex_shrink: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.flex_shrink($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Display block
    // =========================================================================
    ($style:ident; display_block $(, $($rest:tt)*)?) => {
        $style.display = Some($crate::element_style::StyleDisplay::Block);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Width/Height aliases for CSS-style naming
    // =========================================================================
    ($style:ident; width: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.w($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; height: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.h($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
}

// =============================================================================
// flow! macro — define @flow shaders using Rust identifiers and primitives
// =============================================================================
