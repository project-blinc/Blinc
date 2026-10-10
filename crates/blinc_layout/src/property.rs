//! Property identifiers + side-effect metadata for the unified property
//! channel ([[project-reactive-architecture-v2]] Phase 1 foundation).
//!
//! Every source that mutates visual state (signals, CSS state, animations,
//! transitions, motion bindings, pointer events, direct user calls) emits
//! property updates tagged with a `PropertyId`. The id tells the drain step
//! what downstream work the update implies — does it require relayout?
//! does it invalidate text measurement? does it affect clip geometry? —
//! without the drain having to read the value or know the closure body.
//!
//! Phase 1 lands the foundation. `queue_prop_update_partial(node_id,
//! property, side_effects, closure)` is the canonical entry point.
//! Existing `queue_prop_update(node_id, full_RenderProps)` is preserved
//! as a backward-compat shim that routes through the same channel with
//! `PropertyId::Compound + SideEffects::VISUAL` so user code calling
//! `ElementHandle::mark_visual_dirty` and similar stays source-compatible.

/// Identifier for a single visual / layout / text property.
///
/// Carries enough metadata via [`Self::side_effects`] for the drain step
/// to decide whether the update requires relayout or text remeasurement,
/// without inspecting the value or closure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PropertyId {
    // ── visual-only (Tier 1 in the v2 plan) ──────────────────────────
    Background,
    BorderColor,
    BorderWidth,
    CornerRadius,
    Opacity,
    Transform,
    Shadow,
    Color,
    Filter,
    AccentColor,

    // ── layout-affecting (Tier 2) ────────────────────────────────────
    Width,
    Height,
    MinWidth,
    MaxWidth,
    MinHeight,
    MaxHeight,
    Padding,
    Margin,
    Gap,
    FlexDirection,
    AlignItems,
    JustifyContent,
    AlignSelf,
    FlexGrow,
    FlexShrink,
    FlexWrap,
    FlexBasis,
    Display,
    Overflow,
    Position,
    Top,
    Right,
    Bottom,
    Left,

    // ── text-measurement affecting (subset of Tier 2) ────────────────
    FontSize,
    FontFamily,
    FontWeight,
    FontStyle,
    LetterSpacing,
    LineHeight,
    TextAlign,
    TextContent,

    // ── membership of a class, which restyles from the stylesheet ────
    Class,

    // ── the rows of a list or branch, which come and go ──────────────
    Region,

    // ── catch-all for compound mutations ─────────────────────────────
    /// Used by full `RenderProps` replacements where the diff isn't
    /// property-granular. Drain conservatively assumes layout-affecting.
    Compound,
}

/// Downstream effects of applying a property update.
///
/// The drain step ORs effects across all updates in a frame, then decides
/// per-frame whether to invoke `compute_layout`, text remeasurement, or
/// clip recomputation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SideEffects {
    /// Update changes a taffy-relevant property; `compute_layout` must
    /// run before the next paint.
    pub needs_layout: bool,
    /// Update changes a font / text property; text measurement caches
    /// for the affected subtree must be invalidated.
    pub needs_text_remeasure: bool,
    /// Update changes clip-affecting geometry (corner_radius, overflow,
    /// clip-path); clip cascade for descendants must be recomputed.
    pub affects_clip: bool,
}

impl SideEffects {
    /// No downstream work — pure visual cell write.
    pub const VISUAL: Self = Self {
        needs_layout: false,
        needs_text_remeasure: false,
        affects_clip: false,
    };

    /// Triggers `compute_layout` next frame; doesn't touch text.
    pub const LAYOUT: Self = Self {
        needs_layout: true,
        needs_text_remeasure: false,
        affects_clip: false,
    };

    /// Triggers `compute_layout` + text remeasure (font / text changes).
    pub const TEXT: Self = Self {
        needs_layout: true,
        needs_text_remeasure: true,
        affects_clip: false,
    };

    /// Triggers clip-cascade invalidation; usually paired with layout.
    pub const CLIP: Self = Self {
        needs_layout: false,
        needs_text_remeasure: false,
        affects_clip: true,
    };

    /// Worst case — used by `PropertyId::Compound` and unknown updates.
    pub const ALL: Self = Self {
        needs_layout: true,
        needs_text_remeasure: true,
        affects_clip: true,
    };

    /// OR the effects of two updates (drain accumulator).
    pub fn or(self, other: Self) -> Self {
        Self {
            needs_layout: self.needs_layout || other.needs_layout,
            needs_text_remeasure: self.needs_text_remeasure || other.needs_text_remeasure,
            affects_clip: self.affects_clip || other.affects_clip,
        }
    }
}

impl PropertyId {
    /// Side-effects implied by mutating this property.
    pub fn side_effects(self) -> SideEffects {
        use PropertyId::*;
        match self {
            // visual cells — patched in RenderProps, no layout / text work
            Background | BorderColor | Opacity | Transform | Shadow | Color | Filter
            | AccentColor => SideEffects::VISUAL,

            // border width affects content rect → layout
            BorderWidth => SideEffects::LAYOUT,

            // corner radius / overflow affect clip
            CornerRadius => SideEffects::CLIP,
            Overflow => SideEffects {
                needs_layout: false,
                affects_clip: true,
                needs_text_remeasure: false,
            },

            // taffy-style fields — relayout
            Width | Height | MinWidth | MaxWidth | MinHeight | MaxHeight | Padding | Margin
            | Gap | FlexDirection | AlignItems | JustifyContent | AlignSelf | FlexGrow
            | FlexShrink | FlexWrap | FlexBasis | Display | Position | Top | Right | Bottom
            | Left => SideEffects::LAYOUT,

            // text / font — remeasure
            FontSize | FontFamily | FontWeight | FontStyle | LetterSpacing | LineHeight
            | TextAlign | TextContent => SideEffects::TEXT,

            // compound / unknown — conservative worst case. A class decides
            // which rules apply, and a rule can set anything.
            Class | Region | Compound => SideEffects::ALL,
        }
    }

    /// Whether this property can be animated by a transform-equivalent
    /// without entering the layout pass.
    ///
    /// Used by Phase 8's `.animated_*` helpers to detect when a `.w(signal)`
    /// could be rewritten as `.scale_x(signal)` for free, and by Phase 4's
    /// subtree-as-texture promotion to detect transform-only motion.
    pub fn transform_equivalent(self) -> Option<TransformEquivalent> {
        match self {
            PropertyId::Width => Some(TransformEquivalent::ScaleX),
            PropertyId::Height => Some(TransformEquivalent::ScaleY),
            _ => None,
        }
    }
}

/// Transform-pipeline equivalent of a layout property, for Phase 4 /
/// Phase 8 detection. Animating `Width` → `ScaleX` keeps the same visual
/// result without touching layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransformEquivalent {
    ScaleX,
    ScaleY,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visual_props_skip_layout() {
        assert!(!PropertyId::Background.side_effects().needs_layout);
        assert!(!PropertyId::Opacity.side_effects().needs_layout);
        assert!(!PropertyId::Transform.side_effects().needs_layout);
    }

    #[test]
    fn layout_props_trigger_relayout() {
        assert!(PropertyId::Width.side_effects().needs_layout);
        assert!(PropertyId::Padding.side_effects().needs_layout);
        assert!(PropertyId::Gap.side_effects().needs_layout);
    }

    #[test]
    fn text_props_trigger_remeasure() {
        let fx = PropertyId::FontSize.side_effects();
        assert!(fx.needs_text_remeasure);
        assert!(fx.needs_layout, "font change implies relayout");
    }

    #[test]
    fn compound_is_worst_case() {
        let fx = PropertyId::Compound.side_effects();
        assert!(fx.needs_layout && fx.needs_text_remeasure && fx.affects_clip);
    }

    #[test]
    fn side_effects_or() {
        let a = SideEffects::VISUAL;
        let b = SideEffects::LAYOUT;
        let c = a.or(b);
        assert!(c.needs_layout);
        assert!(!c.needs_text_remeasure);
    }

    #[test]
    fn transform_equivalent_for_size_props() {
        assert_eq!(
            PropertyId::Width.transform_equivalent(),
            Some(TransformEquivalent::ScaleX)
        );
        assert_eq!(
            PropertyId::Height.transform_equivalent(),
            Some(TransformEquivalent::ScaleY)
        );
        assert_eq!(PropertyId::Background.transform_equivalent(), None);
    }
}
