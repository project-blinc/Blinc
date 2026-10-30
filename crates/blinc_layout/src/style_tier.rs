//! Which writer last claimed a visual property on a node.
//!
//! A property resolves from three tiers, lowest first: the component's own
//! default, the application stylesheet, and the call-site fluent method.
//! `PropTiers` records the tiers that have written each property; nothing
//! reads it yet, so recording changes no rendering.

/// A visual property that both a fluent method and a stylesheet rule can write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum StyleProp {
    Background,
    BorderRadius,
    CornerShape,
    OverflowFade,
    InnerShadow,
    Shadow,
    Transform,
    Opacity,
    Layer,
    Material,
    ClipPath,
    Filter,
    MaskImage,
    TextColor,
    TextShadow,
    FontSize,
    FontWeight,
    FontStyle,
    TextDecoration,
    LineHeight,
    TextAlign,
    LetterSpacing,
    Fill,
    Stroke,
    StrokeWidth,
    StrokeDasharray,
    StrokeDashoffset,
    SvgPathData,
    TransformOrigin,
    BorderWidth,
    BorderColor,
    OutlineWidth,
    OutlineColor,
    OutlineOffset,
    Rotate3d,
    Perspective,
    Shape3d,
    Depth,
    Light3d,
    Translate3d,
    Op3d,
    Blend3d,
}

/// Who wrote a property.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StyleTier {
    /// The element's own fluent call.
    Owned,
    /// A matched stylesheet rule.
    Stylesheet,
}

/// Per-property writer record, one bit per property per tier.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PropTiers {
    owned: u64,
    stylesheet: u64,
}

impl PropTiers {
    fn bits(&mut self, tier: StyleTier) -> &mut u64 {
        match tier {
            StyleTier::Owned => &mut self.owned,
            StyleTier::Stylesheet => &mut self.stylesheet,
        }
    }

    pub fn mark(&mut self, prop: StyleProp, tier: StyleTier) {
        *self.bits(tier) |= 1 << prop as u8;
    }

    pub fn clear(&mut self, prop: StyleProp, tier: StyleTier) {
        *self.bits(tier) &= !(1 << prop as u8);
    }

    pub fn has(&self, prop: StyleProp, tier: StyleTier) -> bool {
        let bits = match tier {
            StyleTier::Owned => self.owned,
            StyleTier::Stylesheet => self.stylesheet,
        };
        bits & (1 << prop as u8) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiers_are_recorded_independently() {
        let mut t = PropTiers::default();
        t.mark(StyleProp::Background, StyleTier::Owned);
        t.mark(StyleProp::Blend3d, StyleTier::Stylesheet);
        assert!(t.has(StyleProp::Background, StyleTier::Owned));
        assert!(!t.has(StyleProp::Background, StyleTier::Stylesheet));
        assert!(t.has(StyleProp::Blend3d, StyleTier::Stylesheet));
        t.clear(StyleProp::Background, StyleTier::Owned);
        assert!(!t.has(StyleProp::Background, StyleTier::Owned));
    }
}
