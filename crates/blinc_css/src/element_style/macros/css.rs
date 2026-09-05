//! The `css!` macro: an `ElementStyle` written in CSS property names.

/// CSS-like macro for creating ElementStyle with CSS property names
///
/// Uses CSS property naming conventions (with hyphens parsed as separate tokens).
/// Provides a familiar syntax for developers coming from CSS/web development.
///
/// # Examples
///
/// ```ignore
/// use blinc_layout::prelude::*;
/// use blinc_core::Color;
///
/// // CSS-style properties (note: use spaces around hyphens)
/// let card = css! {
///     background: Color::WHITE;
///     border-radius: 8.0;
///     box-shadow: Shadow::new(0.0, 4.0, 8.0, Color::BLACK.with_alpha(0.2));
///     opacity: 0.9;
/// };
///
/// // Transform properties
/// let hover = css! {
///     transform: Transform::scale(1.05, 1.05);
///     opacity: 1.0;
/// };
///
/// // Material effects (Blinc extensions)
/// let glass_panel = css! {
///     background: Color::WHITE.with_alpha(0.1);
///     border-radius: 16.0;
///     backdrop-filter: glass;
/// };
///
/// // Animation
/// let animated = css! {
///     animation-name: "fade-in";
///     animation-duration: 300;
/// };
/// ```
///
/// # Supported Properties
///
/// ## Visual
/// - `background`: Color or Brush
/// - `border-radius`: f32 or CornerRadius
/// - `box-shadow`: sm | md | lg | xl | none | Shadow
/// - `opacity`: f32 (0.0-1.0)
/// - `transform`: Transform | `scale(f)` | `scale(x,y)` | `translate(x,y)` | `rotate(deg)` | `skewX(deg)` | `skewY(deg)`
/// - `transform-origin`: (x%, y%)
/// - `clip-path`: ClipPath
/// - `filter`: CssFilter
/// - `mask-image`: MaskImage
/// - `mask-mode`: MaskMode
/// - `mix-blend-mode`: BlendMode
///
/// ## Text
/// - `color`: Color (text color)
/// - `font-size`: f32 (pixels)
/// - `font-weight`: FontWeight
/// - `text-decoration`: TextDecoration
/// - `text-decoration-color`: Color
/// - `text-decoration-thickness`: f32
/// - `line-height`: f32
/// - `text-align`: left | center | right | TextAlign
/// - `letter-spacing`: f32
/// - `text-shadow`: Shadow
/// - `text-overflow`: clip | ellipsis | TextOverflow
/// - `white-space`: normal | nowrap | pre | WhiteSpace
///
/// ## Layout
/// - `width`, `height`, `min-width`, `min-height`, `max-width`, `max-height`: f32
/// - `display`: flex | block | none
/// - `flex-direction`: row | column | row-reverse | column-reverse
/// - `flex-wrap`: wrap | nowrap
/// - `flex-grow`, `flex-shrink`: f32
/// - `align-items`: center | start | end | stretch | baseline
/// - `justify-content`: center | start | end | space-between | space-around | space-evenly
/// - `align-self`: center | start | end | stretch | baseline
/// - `padding`, `margin`: f32 (uniform)
/// - `gap`: f32
/// - `overflow`: clip | hidden | visible | scroll
/// - `overflow-x`, `overflow-y`: clip | hidden | visible | scroll
/// - `position`: static | relative | absolute | fixed | sticky
/// - `top`, `right`, `bottom`, `left`: f32
/// - `inset`: f32 (all sides)
/// - `z-index`: i32
/// - `visibility`: visible | hidden
///
/// ## Border & Outline
/// - `border`: (width, color)
/// - `border-width`: f32
/// - `border-color`: Color
/// - `outline`: (width, color)
/// - `outline-width`, `outline-color`, `outline-offset`: f32 / Color
///
/// ## 3D
/// - `rotate-x`, `rotate-y`: f32 (degrees)
/// - `perspective`: f32, `translate-z`: f32
/// - `shape-3d`: "box" | "sphere" | "cylinder" | "torus" | "capsule" | "group"
/// - `depth`: f32, `light-direction`: (x,y,z), `light-intensity`, `ambient`, `specular`: f32
/// - `3d-op`: "union" | "subtract" | "intersect" | smooth variants
/// - `3d-blend`: f32
///
/// ## Materials
/// - `backdrop-filter`: glass | metallic | chrome | gold | wood | Material
/// - `render-layer`: foreground | background | RenderLayer
///
/// ## Animation & Transition
/// - `animation`: CssAnimation
/// - `animation-name`: String, `animation-duration`: u32 (ms)
/// - `animation-delay`: u32, `animation-timing-function`, `animation-iteration-count`: u32
/// - `animation-direction`, `animation-fill-mode`
/// - `transition`: CssTransitionSet
///
/// ## SVG
/// - `fill`, `stroke`: Color
/// - `stroke-width`: f32, `stroke-dasharray`: `Vec<f32>`, `stroke-dashoffset`: f32
///
/// ## Form Controls
/// - `caret-color`, `selection-color`, `placeholder-color`, `accent-color`: Color
/// - `scrollbar-color`: (thumb, track), `scrollbar-width`: auto | thin | none
///
/// ## Interaction
/// - `pointer-events`: auto | none
/// - `cursor`: CursorStyle
///
/// ## Image
/// - `object-fit`: u8, `object-position`: (x, y)
#[macro_export]
macro_rules! css {
    // Empty style
    () => {
        $crate::element_style::ElementStyle::new()
    };

    // Main entry point - parse CSS properties (semicolon separated)
    ($($tokens:tt)*) => {{
        let mut __style = $crate::element_style::ElementStyle::new();
        $crate::css_impl!(__style; $($tokens)*);
        __style
    }};
}

/// Internal macro for parsing CSS properties
#[macro_export]
#[doc(hidden)]
macro_rules! css_impl {
    // Base case - no more tokens
    ($style:ident;) => {};

    // =========================================================================
    // Background (CSS: background)
    // =========================================================================
    ($style:ident; background: $value:expr; $($rest:tt)*) => {
        $style = $style.bg($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; background: $value:expr) => {
        $style = $style.bg($value);
    };

    // =========================================================================
    // Border Radius (CSS: border-radius)
    // =========================================================================
    ($style:ident; border-radius: $value:expr; $($rest:tt)*) => {
        $style = $style.rounded($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; border-radius: $value:expr) => {
        $style = $style.rounded($value);
    };

    // =========================================================================
    // Corner Shape (CSS: corner-shape)
    // =========================================================================
    // corner-shape keyword values
    ($style:ident; corner-shape: round; $($rest:tt)*) => {
        $style = $style.corner_shape(1.0);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; corner-shape: round) => {
        $style = $style.corner_shape(1.0);
    };
    ($style:ident; corner-shape: bevel; $($rest:tt)*) => {
        $style = $style.corner_shape(0.0);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; corner-shape: bevel) => {
        $style = $style.corner_shape(0.0);
    };
    ($style:ident; corner-shape: squircle; $($rest:tt)*) => {
        $style = $style.corner_shape(2.0);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; corner-shape: squircle) => {
        $style = $style.corner_shape(2.0);
    };
    ($style:ident; corner-shape: scoop; $($rest:tt)*) => {
        $style = $style.corner_shape(-1.0);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; corner-shape: scoop) => {
        $style = $style.corner_shape(-1.0);
    };
    ($style:ident; corner-shape: notch; $($rest:tt)*) => {
        $style = $style.corner_shape(-100.0);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; corner-shape: notch) => {
        $style = $style.corner_shape(-100.0);
    };
    ($style:ident; corner-shape: square; $($rest:tt)*) => {
        $style = $style.corner_shape(100.0);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; corner-shape: square) => {
        $style = $style.corner_shape(100.0);
    };
    ($style:ident; corner-shape: $value:expr; $($rest:tt)*) => {
        $style = $style.corner_shape($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; corner-shape: $value:expr) => {
        $style = $style.corner_shape($value);
    };

    // =========================================================================
    // Overflow Fade (CSS: overflow-fade)
    // =========================================================================
    ($style:ident; overflow-fade: $value:expr; $($rest:tt)*) => {
        $style = $style.overflow_fade($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; overflow-fade: $value:expr) => {
        $style = $style.overflow_fade($value);
    };

    // =========================================================================
    // Box Shadow (CSS: box-shadow)
    // Shadow presets must come BEFORE generic expr to match correctly
    // =========================================================================
    ($style:ident; box-shadow: sm; $($rest:tt)*) => {
        $style = $style.shadow_sm();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; box-shadow: sm) => {
        $style = $style.shadow_sm();
    };
    ($style:ident; box-shadow: md; $($rest:tt)*) => {
        $style = $style.shadow_md();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; box-shadow: md) => {
        $style = $style.shadow_md();
    };
    ($style:ident; box-shadow: lg; $($rest:tt)*) => {
        $style = $style.shadow_lg();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; box-shadow: lg) => {
        $style = $style.shadow_lg();
    };
    ($style:ident; box-shadow: xl; $($rest:tt)*) => {
        $style = $style.shadow_xl();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; box-shadow: xl) => {
        $style = $style.shadow_xl();
    };
    ($style:ident; box-shadow: none; $($rest:tt)*) => {
        $style = $style.shadow_none();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; box-shadow: none) => {
        $style = $style.shadow_none();
    };
    // Generic expression (must come after presets)
    ($style:ident; box-shadow: $value:expr; $($rest:tt)*) => {
        $style = $style.shadow($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; box-shadow: $value:expr) => {
        $style = $style.shadow($value);
    };

    // =========================================================================
    // Opacity (CSS: opacity)
    // =========================================================================
    ($style:ident; opacity: $value:expr; $($rest:tt)*) => {
        $style = $style.opacity($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; opacity: $value:expr) => {
        $style = $style.opacity($value);
    };

    // =========================================================================
    // Flow (CSS: flow)
    // =========================================================================
    ($style:ident; flow: $value:expr; $($rest:tt)*) => {
        $style = $style.flow($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; flow: $value:expr) => {
        $style = $style.flow($value);
    };

    // =========================================================================
    // Transform (CSS: transform)
    // =========================================================================
    ($style:ident; transform: $value:expr; $($rest:tt)*) => {
        $style = $style.transform($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; transform: $value:expr) => {
        $style = $style.transform($value);
    };
    // Scale shorthand
    ($style:ident; transform: scale($value:expr); $($rest:tt)*) => {
        $style = $style.scale($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; transform: scale($sx:expr, $sy:expr); $($rest:tt)*) => {
        $style = $style.scale_xy($sx, $sy);
        $crate::css_impl!($style; $($rest)*);
    };
    // Translate shorthand
    ($style:ident; transform: translate($x:expr, $y:expr); $($rest:tt)*) => {
        $style = $style.translate($x, $y);
        $crate::css_impl!($style; $($rest)*);
    };
    // Rotate shorthand (degrees)
    ($style:ident; transform: rotate($deg:expr); $($rest:tt)*) => {
        $style = $style.rotate_deg($deg);
        $crate::css_impl!($style; $($rest)*);
    };

    // =========================================================================
    // 3D Transform Properties
    // =========================================================================
    ($style:ident; rotate-x: $value:expr; $($rest:tt)*) => {
        $style = $style.rotate_x_deg($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; rotate-x: $value:expr) => {
        $style = $style.rotate_x_deg($value);
    };
    ($style:ident; rotate-y: $value:expr; $($rest:tt)*) => {
        $style = $style.rotate_y_deg($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; rotate-y: $value:expr) => {
        $style = $style.rotate_y_deg($value);
    };
    ($style:ident; perspective: $value:expr; $($rest:tt)*) => {
        $style = $style.perspective_px($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; perspective: $value:expr) => {
        $style = $style.perspective_px($value);
    };
    ($style:ident; translate-z: $value:expr; $($rest:tt)*) => {
        $style = $style.translate_z_px($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; translate-z: $value:expr) => {
        $style = $style.translate_z_px($value);
    };

    // =========================================================================
    // 3D SDF Shape Properties
    // =========================================================================
    ($style:ident; shape-3d: $value:expr; $($rest:tt)*) => {
        $style = $style.shape_3d($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; shape-3d: $value:expr) => {
        $style = $style.shape_3d($value);
    };
    ($style:ident; depth: $value:expr; $($rest:tt)*) => {
        $style = $style.depth_px($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; depth: $value:expr) => {
        $style = $style.depth_px($value);
    };

    // =========================================================================
    // 3D Lighting Properties
    // =========================================================================
    ($style:ident; light-direction: ($x:expr, $y:expr, $z:expr); $($rest:tt)*) => {
        $style = $style.light_direction($x, $y, $z);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; light-direction: ($x:expr, $y:expr, $z:expr)) => {
        $style = $style.light_direction($x, $y, $z);
    };
    ($style:ident; light-intensity: $value:expr; $($rest:tt)*) => {
        $style = $style.light_intensity($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; light-intensity: $value:expr) => {
        $style = $style.light_intensity($value);
    };
    ($style:ident; ambient: $value:expr; $($rest:tt)*) => {
        $style = $style.ambient_light($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; ambient: $value:expr) => {
        $style = $style.ambient_light($value);
    };
    ($style:ident; specular: $value:expr; $($rest:tt)*) => {
        $style = $style.specular_power($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; specular: $value:expr) => {
        $style = $style.specular_power($value);
    };

    // =========================================================================
    // 3D Boolean Operation Properties
    // =========================================================================
    ($style:ident; 3d-op: $value:expr; $($rest:tt)*) => {
        $style = $style.op_3d_type($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; 3d-op: $value:expr) => {
        $style = $style.op_3d_type($value);
    };
    ($style:ident; 3d-blend: $value:expr; $($rest:tt)*) => {
        $style = $style.blend_3d_px($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; 3d-blend: $value:expr) => {
        $style = $style.blend_3d_px($value);
    };

    // =========================================================================
    // Clip-Path
    // =========================================================================
    ($style:ident; clip-path: $value:expr; $($rest:tt)*) => {
        $style = $style.clip_path($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; clip-path: $value:expr) => {
        $style = $style.clip_path($value);
    };

    // =========================================================================
    // Backdrop Filter (Blinc extension for materials)
    // =========================================================================
    ($style:ident; backdrop-filter: glass; $($rest:tt)*) => {
        $style = $style.glass();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; backdrop-filter: glass) => {
        $style = $style.glass();
    };
    ($style:ident; backdrop-filter: metallic; $($rest:tt)*) => {
        $style = $style.metallic();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; backdrop-filter: chrome; $($rest:tt)*) => {
        $style = $style.chrome();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; backdrop-filter: gold; $($rest:tt)*) => {
        $style = $style.gold();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; backdrop-filter: wood; $($rest:tt)*) => {
        $style = $style.wood();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; backdrop-filter: $value:expr; $($rest:tt)*) => {
        $style = $style.material($value);
        $crate::css_impl!($style; $($rest)*);
    };

    // =========================================================================
    // Render Layer (Blinc extension)
    // =========================================================================
    ($style:ident; render-layer: foreground; $($rest:tt)*) => {
        $style = $style.foreground();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; render-layer: background; $($rest:tt)*) => {
        $style = $style.layer_background();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; render-layer: $value:expr; $($rest:tt)*) => {
        $style = $style.layer($value);
        $crate::css_impl!($style; $($rest)*);
    };

    // =========================================================================
    // Animation Properties
    // =========================================================================
    ($style:ident; animation: $value:expr; $($rest:tt)*) => {
        $style = $style.animation($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; animation: $value:expr) => {
        $style = $style.animation($value);
    };
    ($style:ident; animation-name: $value:expr; $($rest:tt)*) => {
        $style = $style.animation_name($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; animation-name: $value:expr) => {
        $style = $style.animation_name($value);
    };
    ($style:ident; animation-duration: $value:expr; $($rest:tt)*) => {
        $style = $style.animation_duration($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; animation-duration: $value:expr) => {
        $style = $style.animation_duration($value);
    };

    // =========================================================================
    // Layout: Sizing (CSS: width, height, min-width, etc.)
    // =========================================================================
    ($style:ident; width: $value:expr; $($rest:tt)*) => {
        $style = $style.w($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; width: $value:expr) => {
        $style = $style.w($value);
    };
    ($style:ident; height: $value:expr; $($rest:tt)*) => {
        $style = $style.h($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; height: $value:expr) => {
        $style = $style.h($value);
    };
    ($style:ident; min-width: $value:expr; $($rest:tt)*) => {
        $style = $style.min_w($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; min-width: $value:expr) => {
        $style = $style.min_w($value);
    };
    ($style:ident; min-height: $value:expr; $($rest:tt)*) => {
        $style = $style.min_h($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; min-height: $value:expr) => {
        $style = $style.min_h($value);
    };
    ($style:ident; max-width: $value:expr; $($rest:tt)*) => {
        $style = $style.max_w($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; max-width: $value:expr) => {
        $style = $style.max_w($value);
    };
    ($style:ident; max-height: $value:expr; $($rest:tt)*) => {
        $style = $style.max_h($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; max-height: $value:expr) => {
        $style = $style.max_h($value);
    };

    // =========================================================================
    // Layout: Flex Direction (CSS: display, flex-direction, flex-wrap)
    // =========================================================================
    ($style:ident; display: flex; $($rest:tt)*) => {
        $style.display = Some($crate::element_style::StyleDisplay::Flex);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; display: none; $($rest:tt)*) => {
        $style = $style.display_none();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; flex-direction: row; $($rest:tt)*) => {
        $style = $style.flex_row();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; flex-direction: column; $($rest:tt)*) => {
        $style = $style.flex_col();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; flex-direction: row-reverse; $($rest:tt)*) => {
        $style = $style.flex_row_reverse();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; flex-direction: column-reverse; $($rest:tt)*) => {
        $style = $style.flex_col_reverse();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; flex-wrap: wrap; $($rest:tt)*) => {
        $style = $style.flex_wrap();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; flex-grow: $value:expr; $($rest:tt)*) => {
        $style = $style.flex_grow_value($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; flex-grow: $value:expr) => {
        $style = $style.flex_grow_value($value);
    };
    ($style:ident; flex-shrink: $value:expr; $($rest:tt)*) => {
        $style.flex_shrink = Some($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; flex-shrink: $value:expr) => {
        $style.flex_shrink = Some($value);
    };

    // =========================================================================
    // Layout: Alignment (CSS: align-items, justify-content, align-self)
    // =========================================================================
    ($style:ident; align-items: center; $($rest:tt)*) => {
        $style = $style.items_center();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; align-items: start; $($rest:tt)*) => {
        $style = $style.items_start();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; align-items: end; $($rest:tt)*) => {
        $style = $style.items_end();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; align-items: stretch; $($rest:tt)*) => {
        $style = $style.items_stretch();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; justify-content: center; $($rest:tt)*) => {
        $style = $style.justify_center();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; justify-content: start; $($rest:tt)*) => {
        $style = $style.justify_start();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; justify-content: end; $($rest:tt)*) => {
        $style = $style.justify_end();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; justify-content: space-between; $($rest:tt)*) => {
        $style = $style.justify_between();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; justify-content: space-around; $($rest:tt)*) => {
        $style = $style.justify_around();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; justify-content: space-evenly; $($rest:tt)*) => {
        $style = $style.justify_evenly();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; align-self: center; $($rest:tt)*) => {
        $style = $style.self_center();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; align-self: start; $($rest:tt)*) => {
        $style = $style.self_start();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; align-self: end; $($rest:tt)*) => {
        $style = $style.self_end();
        $crate::css_impl!($style; $($rest)*);
    };

    // =========================================================================
    // Layout: Spacing (CSS: padding, margin, gap)
    // =========================================================================
    ($style:ident; padding: $value:expr; $($rest:tt)*) => {
        $style = $style.p($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; padding: $value:expr) => {
        $style = $style.p($value);
    };
    ($style:ident; margin: $value:expr; $($rest:tt)*) => {
        $style = $style.m($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; margin: $value:expr) => {
        $style = $style.m($value);
    };
    ($style:ident; gap: $value:expr; $($rest:tt)*) => {
        $style = $style.gap($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; gap: $value:expr) => {
        $style = $style.gap($value);
    };

    // =========================================================================
    // Layout: Overflow (CSS: overflow)
    // =========================================================================
    ($style:ident; overflow: clip; $($rest:tt)*) => {
        $style = $style.overflow_clip();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; overflow: visible; $($rest:tt)*) => {
        $style = $style.overflow_visible();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; overflow: scroll; $($rest:tt)*) => {
        $style = $style.overflow_scroll();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; overflow: hidden; $($rest:tt)*) => {
        $style = $style.overflow_clip();
        $crate::css_impl!($style; $($rest)*);
    };

    // =========================================================================
    // Layout: Border (CSS: border-width, border-color)
    // =========================================================================
    ($style:ident; border-width: $value:expr; $($rest:tt)*) => {
        $style = $style.border_w($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; border-width: $value:expr) => {
        $style = $style.border_w($value);
    };
    ($style:ident; border-color: $value:expr; $($rest:tt)*) => {
        $style.border_color = Some($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; border-color: $value:expr) => {
        $style.border_color = Some($value);
    };

    // =========================================================================
    // Text Properties (CSS: color, font-size, font-weight, etc.)
    // =========================================================================
    ($style:ident; color: $value:expr; $($rest:tt)*) => {
        $style = $style.text_color($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; color: $value:expr) => {
        $style = $style.text_color($value);
    };
    ($style:ident; font-size: $value:expr; $($rest:tt)*) => {
        $style = $style.font_size($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; font-size: $value:expr) => {
        $style = $style.font_size($value);
    };
    ($style:ident; font-weight: $value:expr; $($rest:tt)*) => {
        $style = $style.font_weight($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; font-weight: $value:expr) => {
        $style = $style.font_weight($value);
    };
    ($style:ident; font-style: $value:expr; $($rest:tt)*) => {
        $style = $style.font_style($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; font-style: $value:expr) => {
        $style = $style.font_style($value);
    };
    ($style:ident; text-decoration: $value:expr; $($rest:tt)*) => {
        $style = $style.text_decoration($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; text-decoration: $value:expr) => {
        $style = $style.text_decoration($value);
    };
    ($style:ident; text-decoration-color: $value:expr; $($rest:tt)*) => {
        $style = $style.text_decoration_color($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; text-decoration-color: $value:expr) => {
        $style = $style.text_decoration_color($value);
    };
    ($style:ident; text-decoration-thickness: $value:expr; $($rest:tt)*) => {
        $style = $style.text_decoration_thickness($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; text-decoration-thickness: $value:expr) => {
        $style = $style.text_decoration_thickness($value);
    };
    ($style:ident; line-height: $value:expr; $($rest:tt)*) => {
        $style = $style.line_height($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; line-height: $value:expr) => {
        $style = $style.line_height($value);
    };
    ($style:ident; text-align: center; $($rest:tt)*) => {
        $style = $style.text_align($crate::text::TextAlign::Center);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; text-align: right; $($rest:tt)*) => {
        $style = $style.text_align($crate::text::TextAlign::Right);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; text-align: left; $($rest:tt)*) => {
        $style = $style.text_align($crate::text::TextAlign::Left);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; text-align: $value:expr; $($rest:tt)*) => {
        $style = $style.text_align($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; text-align: $value:expr) => {
        $style = $style.text_align($value);
    };
    ($style:ident; letter-spacing: $value:expr; $($rest:tt)*) => {
        $style = $style.letter_spacing($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; letter-spacing: $value:expr) => {
        $style = $style.letter_spacing($value);
    };
    ($style:ident; text-shadow: $value:expr; $($rest:tt)*) => {
        $style = $style.text_shadow($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; text-shadow: $value:expr) => {
        $style = $style.text_shadow($value);
    };
    ($style:ident; text-overflow: clip; $($rest:tt)*) => {
        $style = $style.text_overflow($crate::element_style::TextOverflow::Clip);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; text-overflow: ellipsis; $($rest:tt)*) => {
        $style = $style.text_overflow($crate::element_style::TextOverflow::Ellipsis);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; text-overflow: $value:expr; $($rest:tt)*) => {
        $style = $style.text_overflow($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; text-overflow: $value:expr) => {
        $style = $style.text_overflow($value);
    };
    ($style:ident; white-space: normal; $($rest:tt)*) => {
        $style = $style.white_space($crate::element_style::WhiteSpace::Normal);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; white-space: nowrap; $($rest:tt)*) => {
        $style = $style.white_space($crate::element_style::WhiteSpace::Nowrap);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; white-space: pre; $($rest:tt)*) => {
        $style = $style.white_space($crate::element_style::WhiteSpace::Pre);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; white-space: $value:expr; $($rest:tt)*) => {
        $style = $style.white_space($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; white-space: $value:expr) => {
        $style = $style.white_space($value);
    };

    // =========================================================================
    // Transform Extras (CSS: transform-origin, skew)
    // =========================================================================
    ($style:ident; transform-origin: ($x:expr, $y:expr); $($rest:tt)*) => {
        $style = $style.transform_origin($x, $y);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; transform-origin: ($x:expr, $y:expr)) => {
        $style = $style.transform_origin($x, $y);
    };
    ($style:ident; transform: skewX($deg:expr); $($rest:tt)*) => {
        $style = $style.skew_x($deg);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; transform: skewY($deg:expr); $($rest:tt)*) => {
        $style = $style.skew_y($deg);
        $crate::css_impl!($style; $($rest)*);
    };

    // =========================================================================
    // Transition (CSS: transition)
    // =========================================================================
    ($style:ident; transition: $value:expr; $($rest:tt)*) => {
        $style = $style.transition($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; transition: $value:expr) => {
        $style = $style.transition($value);
    };

    // =========================================================================
    // Filter (CSS: filter)
    // =========================================================================
    ($style:ident; filter: $value:expr; $($rest:tt)*) => {
        $style = $style.filter($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; filter: $value:expr) => {
        $style = $style.filter($value);
    };

    // =========================================================================
    // Mask Properties (CSS: mask-image, mask-mode)
    // =========================================================================
    ($style:ident; mask-image: $value:expr; $($rest:tt)*) => {
        $style.mask_image = Some($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; mask-image: $value:expr) => {
        $style.mask_image = Some($value);
    };
    ($style:ident; mask-mode: $value:expr; $($rest:tt)*) => {
        $style = $style.mask_mode($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; mask-mode: $value:expr) => {
        $style = $style.mask_mode($value);
    };

    // =========================================================================
    // Mix Blend Mode (CSS: mix-blend-mode)
    // =========================================================================
    ($style:ident; mix-blend-mode: $value:expr; $($rest:tt)*) => {
        $style = $style.mix_blend_mode($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; mix-blend-mode: $value:expr) => {
        $style = $style.mix_blend_mode($value);
    };

    // =========================================================================
    // Outline (CSS: outline, outline-width, outline-color, outline-offset)
    // =========================================================================
    ($style:ident; outline: ($width:expr, $color:expr); $($rest:tt)*) => {
        $style = $style.outline($width, $color);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; outline: ($width:expr, $color:expr)) => {
        $style = $style.outline($width, $color);
    };
    ($style:ident; outline-width: $value:expr; $($rest:tt)*) => {
        $style = $style.outline_w($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; outline-width: $value:expr) => {
        $style = $style.outline_w($value);
    };
    ($style:ident; outline-color: $value:expr; $($rest:tt)*) => {
        $style.outline_color = Some($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; outline-color: $value:expr) => {
        $style.outline_color = Some($value);
    };
    ($style:ident; outline-offset: $value:expr; $($rest:tt)*) => {
        $style = $style.outline_offset($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; outline-offset: $value:expr) => {
        $style = $style.outline_offset($value);
    };

    // =========================================================================
    // Border shorthand (CSS: border)
    // =========================================================================
    ($style:ident; border: ($width:expr, $color:expr); $($rest:tt)*) => {
        $style = $style.border($width, $color);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; border: ($width:expr, $color:expr)) => {
        $style = $style.border($width, $color);
    };

    // =========================================================================
    // Overflow per-axis (CSS: overflow-x, overflow-y)
    // =========================================================================
    ($style:ident; overflow-x: clip; $($rest:tt)*) => {
        $style = $style.overflow_x($crate::element_style::StyleOverflow::Clip);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; overflow-x: hidden; $($rest:tt)*) => {
        $style = $style.overflow_x($crate::element_style::StyleOverflow::Clip);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; overflow-x: visible; $($rest:tt)*) => {
        $style = $style.overflow_x($crate::element_style::StyleOverflow::Visible);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; overflow-x: scroll; $($rest:tt)*) => {
        $style = $style.overflow_x($crate::element_style::StyleOverflow::Scroll);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; overflow-y: clip; $($rest:tt)*) => {
        $style = $style.overflow_y($crate::element_style::StyleOverflow::Clip);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; overflow-y: hidden; $($rest:tt)*) => {
        $style = $style.overflow_y($crate::element_style::StyleOverflow::Clip);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; overflow-y: visible; $($rest:tt)*) => {
        $style = $style.overflow_y($crate::element_style::StyleOverflow::Visible);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; overflow-y: scroll; $($rest:tt)*) => {
        $style = $style.overflow_y($crate::element_style::StyleOverflow::Scroll);
        $crate::css_impl!($style; $($rest)*);
    };

    // =========================================================================
    // Position & Inset (CSS: position, top, right, bottom, left, inset)
    // =========================================================================
    ($style:ident; position: static; $($rest:tt)*) => {
        $style = $style.position($crate::element_style::StylePosition::Static);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; position: relative; $($rest:tt)*) => {
        $style = $style.position($crate::element_style::StylePosition::Relative);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; position: absolute; $($rest:tt)*) => {
        $style = $style.position($crate::element_style::StylePosition::Absolute);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; position: fixed; $($rest:tt)*) => {
        $style = $style.position($crate::element_style::StylePosition::Fixed);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; position: sticky; $($rest:tt)*) => {
        $style = $style.position($crate::element_style::StylePosition::Sticky);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; position: $value:expr; $($rest:tt)*) => {
        $style = $style.position($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; position: $value:expr) => {
        $style = $style.position($value);
    };
    ($style:ident; top: $value:expr; $($rest:tt)*) => {
        $style = $style.top($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; top: $value:expr) => {
        $style = $style.top($value);
    };
    ($style:ident; right: $value:expr; $($rest:tt)*) => {
        $style = $style.right($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; right: $value:expr) => {
        $style = $style.right($value);
    };
    ($style:ident; bottom: $value:expr; $($rest:tt)*) => {
        $style = $style.bottom($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; bottom: $value:expr) => {
        $style = $style.bottom($value);
    };
    ($style:ident; left: $value:expr; $($rest:tt)*) => {
        $style = $style.left($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; left: $value:expr) => {
        $style = $style.left($value);
    };
    ($style:ident; inset: $value:expr; $($rest:tt)*) => {
        $style = $style.inset($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; inset: $value:expr) => {
        $style = $style.inset($value);
    };
    ($style:ident; z-index: $value:expr; $($rest:tt)*) => {
        $style = $style.z_index($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; z-index: $value:expr) => {
        $style = $style.z_index($value);
    };
    ($style:ident; visibility: visible; $($rest:tt)*) => {
        $style = $style.visibility($crate::element_style::StyleVisibility::Visible);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; visibility: hidden; $($rest:tt)*) => {
        $style = $style.visibility($crate::element_style::StyleVisibility::Hidden);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; visibility: $value:expr; $($rest:tt)*) => {
        $style = $style.visibility($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; visibility: $value:expr) => {
        $style = $style.visibility($value);
    };

    // =========================================================================
    // Form Element Colors
    // =========================================================================
    ($style:ident; caret-color: $value:expr; $($rest:tt)*) => {
        $style = $style.caret_color($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; caret-color: $value:expr) => {
        $style = $style.caret_color($value);
    };
    ($style:ident; selection-color: $value:expr; $($rest:tt)*) => {
        $style = $style.selection_color($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; selection-color: $value:expr) => {
        $style = $style.selection_color($value);
    };
    ($style:ident; placeholder-color: $value:expr; $($rest:tt)*) => {
        $style = $style.placeholder_color($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; placeholder-color: $value:expr) => {
        $style = $style.placeholder_color($value);
    };
    ($style:ident; accent-color: $value:expr; $($rest:tt)*) => {
        $style = $style.accent_color($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; accent-color: $value:expr) => {
        $style = $style.accent_color($value);
    };

    // =========================================================================
    // Scrollbar Properties
    // =========================================================================
    ($style:ident; scrollbar-color: ($thumb:expr, $track:expr); $($rest:tt)*) => {
        $style = $style.scrollbar_color($thumb, $track);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; scrollbar-color: ($thumb:expr, $track:expr)) => {
        $style = $style.scrollbar_color($thumb, $track);
    };
    ($style:ident; scrollbar-width: auto; $($rest:tt)*) => {
        $style = $style.scrollbar_width($crate::element_style::ScrollbarWidth::Auto);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; scrollbar-width: thin; $($rest:tt)*) => {
        $style = $style.scrollbar_width($crate::element_style::ScrollbarWidth::Thin);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; scrollbar-width: none; $($rest:tt)*) => {
        $style = $style.scrollbar_width($crate::element_style::ScrollbarWidth::None);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; scrollbar-width: $value:expr; $($rest:tt)*) => {
        $style = $style.scrollbar_width($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; scrollbar-width: $value:expr) => {
        $style = $style.scrollbar_width($value);
    };

    // =========================================================================
    // SVG Properties (CSS: fill, stroke, etc.)
    // =========================================================================
    ($style:ident; fill: $value:expr; $($rest:tt)*) => {
        $style = $style.fill($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; fill: $value:expr) => {
        $style = $style.fill($value);
    };
    ($style:ident; stroke: $value:expr; $($rest:tt)*) => {
        $style = $style.stroke($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; stroke: $value:expr) => {
        $style = $style.stroke($value);
    };
    ($style:ident; stroke-width: $value:expr; $($rest:tt)*) => {
        $style = $style.stroke_width($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; stroke-width: $value:expr) => {
        $style = $style.stroke_width($value);
    };
    ($style:ident; stroke-dasharray: $value:expr; $($rest:tt)*) => {
        $style = $style.stroke_dasharray($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; stroke-dasharray: $value:expr) => {
        $style = $style.stroke_dasharray($value);
    };
    ($style:ident; stroke-dashoffset: $value:expr; $($rest:tt)*) => {
        $style = $style.stroke_dashoffset($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; stroke-dashoffset: $value:expr) => {
        $style = $style.stroke_dashoffset($value);
    };

    // =========================================================================
    // Image Properties (CSS: object-fit, object-position)
    // =========================================================================
    ($style:ident; object-fit: $value:expr; $($rest:tt)*) => {
        $style = $style.object_fit($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; object-fit: $value:expr) => {
        $style = $style.object_fit($value);
    };
    ($style:ident; object-position: ($x:expr, $y:expr); $($rest:tt)*) => {
        $style = $style.object_position($x, $y);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; object-position: ($x:expr, $y:expr)) => {
        $style = $style.object_position($x, $y);
    };

    // =========================================================================
    // Interaction Properties (CSS: pointer-events, cursor)
    // =========================================================================
    ($style:ident; pointer-events: none; $($rest:tt)*) => {
        $style = $style.pointer_events_none();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; pointer-events: auto; $($rest:tt)*) => {
        $style = $style.pointer_events(blinc_core::PointerEvents::Auto);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; pointer-events: $value:expr; $($rest:tt)*) => {
        $style = $style.pointer_events($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; pointer-events: $value:expr) => {
        $style = $style.pointer_events($value);
    };
    ($style:ident; cursor: $value:expr; $($rest:tt)*) => {
        $style = $style.cursor($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; cursor: $value:expr) => {
        $style = $style.cursor($value);
    };

    // =========================================================================
    // Animation sub-properties
    // =========================================================================
    ($style:ident; animation-delay: $value:expr; $($rest:tt)*) => {
        {
            let mut anim = $style.animation.clone().unwrap_or_default();
            anim.delay_ms = $value;
            $style.animation = Some(anim);
        }
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; animation-delay: $value:expr) => {
        {
            let mut anim = $style.animation.clone().unwrap_or_default();
            anim.delay_ms = $value;
            $style.animation = Some(anim);
        }
    };
    ($style:ident; animation-timing-function: $value:expr; $($rest:tt)*) => {
        {
            let mut anim = $style.animation.clone().unwrap_or_default();
            anim.timing = $value;
            $style.animation = Some(anim);
        }
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; animation-timing-function: $value:expr) => {
        {
            let mut anim = $style.animation.clone().unwrap_or_default();
            anim.timing = $value;
            $style.animation = Some(anim);
        }
    };
    ($style:ident; animation-iteration-count: $value:expr; $($rest:tt)*) => {
        {
            let mut anim = $style.animation.clone().unwrap_or_default();
            anim.iteration_count = $value;
            $style.animation = Some(anim);
        }
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; animation-iteration-count: $value:expr) => {
        {
            let mut anim = $style.animation.clone().unwrap_or_default();
            anim.iteration_count = $value;
            $style.animation = Some(anim);
        }
    };
    ($style:ident; animation-direction: $value:expr; $($rest:tt)*) => {
        {
            let mut anim = $style.animation.clone().unwrap_or_default();
            anim.direction = $value;
            $style.animation = Some(anim);
        }
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; animation-direction: $value:expr) => {
        {
            let mut anim = $style.animation.clone().unwrap_or_default();
            anim.direction = $value;
            $style.animation = Some(anim);
        }
    };
    ($style:ident; animation-fill-mode: $value:expr; $($rest:tt)*) => {
        {
            let mut anim = $style.animation.clone().unwrap_or_default();
            anim.fill_mode = $value;
            $style.animation = Some(anim);
        }
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; animation-fill-mode: $value:expr) => {
        {
            let mut anim = $style.animation.clone().unwrap_or_default();
            anim.fill_mode = $value;
            $style.animation = Some(anim);
        }
    };

    // =========================================================================
    // Display: block
    // =========================================================================
    ($style:ident; display: block; $($rest:tt)*) => {
        $style.display = Some($crate::element_style::StyleDisplay::Block);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; flex-wrap: nowrap; $($rest:tt)*) => {
        $style.flex_wrap = Some(false);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; flex-shrink: $value:expr; $($rest:tt)*) => {
        $style = $style.flex_shrink($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; flex-shrink: $value:expr) => {
        $style = $style.flex_shrink($value);
    };
    ($style:ident; align-items: baseline; $($rest:tt)*) => {
        $style.align_items = Some($crate::element_style::StyleAlign::Baseline);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; align-self: stretch; $($rest:tt)*) => {
        $style.align_self = Some($crate::element_style::StyleAlign::Stretch);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; align-self: baseline; $($rest:tt)*) => {
        $style.align_self = Some($crate::element_style::StyleAlign::Baseline);
        $crate::css_impl!($style; $($rest)*);
    };
}
