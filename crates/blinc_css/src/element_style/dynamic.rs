//! Properties whose value is a `calc()` expression containing `env()`.
//!
//! These cannot be resolved at parse time because they read live pointer
//! state, so the style keeps the expression and the renderer evaluates it
//! per frame. The write side lives in the layout crate, which owns the
//! render props such a property writes into.

use crate::calc::CalcExpr;

/// A CSS property whose value is a dynamic `calc()` expression containing `env()` references.
/// These are evaluated per-frame with the current pointer query state.
#[derive(Clone, Debug)]
pub enum DynamicProperty {
    Opacity(CalcExpr),
    RotateX(CalcExpr),
    RotateY(CalcExpr),
    Perspective(CalcExpr),
    CornerRadius(CalcExpr),
    TranslateZ(CalcExpr),
    Depth(CalcExpr),
    BorderWidth(CalcExpr),
    /// 2D skew-x (in degrees) — composited into props.transform (Affine2D)
    SkewX(CalcExpr),
    /// 2D skew-y (in degrees) — composited into props.transform (Affine2D)
    SkewY(CalcExpr),
    /// 2D rotate (in degrees) — composited into props.transform (Affine2D)
    Rotate(CalcExpr),
    /// 2D scale-x — composited into props.transform (Affine2D)
    ScaleX(CalcExpr),
    /// 2D scale-y — composited into props.transform (Affine2D)
    ScaleY(CalcExpr),
}

impl DynamicProperty {
    /// Returns true if this dynamic property modifies `props.transform` (Affine2D).
    pub fn is_transform(&self) -> bool {
        matches!(
            self,
            DynamicProperty::SkewX(_)
                | DynamicProperty::SkewY(_)
                | DynamicProperty::Rotate(_)
                | DynamicProperty::ScaleX(_)
                | DynamicProperty::ScaleY(_)
        )
    }

    /// Get the CalcExpr from this dynamic property.
    pub fn expr(&self) -> &CalcExpr {
        match self {
            DynamicProperty::Opacity(e)
            | DynamicProperty::RotateX(e)
            | DynamicProperty::RotateY(e)
            | DynamicProperty::Perspective(e)
            | DynamicProperty::CornerRadius(e)
            | DynamicProperty::TranslateZ(e)
            | DynamicProperty::Depth(e)
            | DynamicProperty::BorderWidth(e)
            | DynamicProperty::SkewX(e)
            | DynamicProperty::SkewY(e)
            | DynamicProperty::Rotate(e)
            | DynamicProperty::ScaleX(e)
            | DynamicProperty::ScaleY(e) => e,
        }
    }
}
