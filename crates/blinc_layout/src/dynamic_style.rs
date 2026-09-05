//! Applying `calc(env(...))` properties to resolved render props.
//!
//! A [`DynamicProperty`] is parsed and stored by `blinc_css`, but evaluating
//! one writes into [`RenderProps`], which belongs to this crate. The write
//! side therefore lives here as an extension trait rather than as an
//! inherent method on the property.

use crate::calc::CalcContext;
use crate::element::RenderProps;
use crate::element_style::DynamicProperty;

/// Evaluates a dynamic property against a calc context and writes the result
/// into render props.
pub trait DynamicPropertyExt {
    /// Evaluate this dynamic property and apply the result to `props`.
    fn apply(&self, props: &mut RenderProps, ctx: &CalcContext);
}

impl DynamicPropertyExt for DynamicProperty {
    fn apply(&self, props: &mut RenderProps, ctx: &CalcContext) {
        match self {
            DynamicProperty::Opacity(expr) => {
                let v = expr.eval(ctx).clamp(0.0, 1.0);
                props.opacity = v;
            }
            DynamicProperty::RotateX(expr) => {
                let v = expr.eval(ctx);
                props.rotate_x = Some(v);
            }
            DynamicProperty::RotateY(expr) => {
                let v = expr.eval(ctx);
                props.rotate_y = Some(v);
            }
            DynamicProperty::Perspective(expr) => {
                props.perspective = Some(expr.eval(ctx));
            }
            DynamicProperty::CornerRadius(expr) => {
                let r = expr.eval(ctx).max(0.0);
                props.border_radius = blinc_core::CornerRadius::uniform(r);
            }
            DynamicProperty::TranslateZ(expr) => {
                props.translate_z = Some(expr.eval(ctx));
            }
            DynamicProperty::Depth(expr) => {
                props.depth = Some(expr.eval(ctx));
            }
            DynamicProperty::BorderWidth(expr) => {
                let v = expr.eval(ctx).max(0.0);
                props.border_width = v;
            }
            DynamicProperty::SkewX(expr) => {
                let deg = expr.eval(ctx);
                let skew = blinc_core::Affine2D::skew_x(deg.to_radians());
                compose_affine(props, skew);
            }
            DynamicProperty::SkewY(expr) => {
                let deg = expr.eval(ctx);
                let skew = blinc_core::Affine2D::skew_y(deg.to_radians());
                compose_affine(props, skew);
            }
            DynamicProperty::Rotate(expr) => {
                let deg = expr.eval(ctx);
                let rot = blinc_core::Affine2D::rotation(deg.to_radians());
                compose_affine(props, rot);
            }
            DynamicProperty::ScaleX(expr) => {
                let sx = expr.eval(ctx);
                let s = blinc_core::Affine2D::scale(sx, 1.0);
                compose_affine(props, s);
            }
            DynamicProperty::ScaleY(expr) => {
                let sy = expr.eval(ctx);
                let s = blinc_core::Affine2D::scale(1.0, sy);
                compose_affine(props, s);
            }
        }
    }
}

/// Compose a new 2D affine transform onto the existing `props.transform`.
/// If no transform exists, sets it directly. Otherwise multiplies.
fn compose_affine(props: &mut RenderProps, new_affine: blinc_core::Affine2D) {
    use blinc_core::Transform;
    let composed = match &props.transform {
        Some(Transform::Affine2D(existing)) => existing.then(&new_affine),
        _ => new_affine,
    };
    props.transform = Some(Transform::Affine2D(composed));
}
