//! Notch element for shapes with concave curves or sharp steps
//!
//! Provides a fluent API for creating shapes with concave (outward-bowing)
//! curves or sharp 90° step notches that `div()` cannot do.
//!
//! # The Notched Dropdown
//!
//! The primary use case - macOS-style menu bar dropdowns:
//!
//! ```text
//!     ╭──────────────╮  ← Menu bar
//! ╭───╯              ╰───╮
//! │                      │  ← Concave curves connect to bar
//! │    Content here      │
//! │                      │
//! ╰──────────────────────╯  ← Convex (standard) rounding
//! ```
//!
//! # Curved Example
//!
//! ```ignore
//! use blinc_layout::prelude::*;
//!
//! notch()
//!     .concave_top(24.0)    // Curved notch
//!     .rounded_bottom(16.0) // Standard rounding
//!     .bg(Color::BLACK)
//!     .child(text("Battery | 87% Charged"))
//! ```
//!
//! # Sharp Step Example
//!
//! ```ignore
//! notch()
//!     .step_top(24.0)       // Sharp 90° step notch
//!     .rounded_bottom(16.0)
//!     .bg(Color::BLACK)
//! ```
//!
//! # Animation with Signed Radius
//!
//! Use `.corner_*()` methods with signed values for smooth morphing:
//! - **Negative** = concave (curves outward)
//! - **Positive** = convex (standard rounding)  
//! - **Zero** = sharp corner (crossover point)
//!
//! ```ignore
//! stateful(|ctx| {
//!     let top_r = ctx.spring("top", if open { -24.0 } else { 16.0 });
//!     notch().corner_top(top_r).corner_bottom(16.0)
//! })
//! ```

use std::rc::Rc;

use blinc_core::{
    Brush, Color, CornerRadius, DrawContext, Gradient, Path, Rect, Shadow, Transform,
};
use taffy::{Overflow, prelude::*};

use crate::Div;
use crate::canvas::{CanvasBounds, CanvasRenderFn};
use crate::div::{ElementBuilder, ElementTypeId};
use crate::element::{Material, RenderLayer, RenderProps};
use crate::event_handler::EventHandlers;
use crate::tree::{LayoutNodeId, LayoutTree};

// =============================================================================
// Corner Configuration
// =============================================================================

/// Configuration for a single corner
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CornerConfig {
    /// Radius/depth of the corner effect
    pub radius: f32,
    /// The style of corner
    pub style: CornerStyle,
}

/// The type of corner curve
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum CornerStyle {
    /// No corner effect (sharp 90° corner)
    #[default]
    None,
    /// Standard rounded corner (curves inward)
    Convex,
    /// Concave corner (curves outward)
    Concave,
    /// Sharp right-angle step/notch (extends outward with 90° angles)
    Step,
}

impl CornerConfig {
    /// No corner rounding
    pub const NONE: Self = Self {
        radius: 0.0,
        style: CornerStyle::None,
    };

    /// Create a convex corner - standard rounded corner (curves inward)
    pub fn convex(radius: f32) -> Self {
        Self {
            radius,
            style: CornerStyle::Convex,
        }
    }

    /// Create a concave corner - curves outward from the shape
    pub fn concave(radius: f32) -> Self {
        Self {
            radius,
            style: CornerStyle::Concave,
        }
    }

    /// Create a step corner - sharp right-angle notch
    ///
    /// ```text
    /// ┌───┐
    /// │   │  ← step with depth
    /// │   └───
    /// ```
    pub fn step(depth: f32) -> Self {
        Self {
            radius: depth,
            style: CornerStyle::Step,
        }
    }

    /// Check if this is a concave corner
    pub fn is_concave(&self) -> bool {
        matches!(self.style, CornerStyle::Concave)
    }

    /// Check if this is a step corner
    pub fn is_step(&self) -> bool {
        matches!(self.style, CornerStyle::Step)
    }
}

/// Configuration for all four corners
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CornersConfig {
    pub top_left: CornerConfig,
    pub top_right: CornerConfig,
    pub bottom_right: CornerConfig,
    pub bottom_left: CornerConfig,
}

impl CornersConfig {
    /// All corners with no rounding
    pub const NONE: Self = Self {
        top_left: CornerConfig::NONE,
        top_right: CornerConfig::NONE,
        bottom_right: CornerConfig::NONE,
        bottom_left: CornerConfig::NONE,
    };

    /// Check if any corner has concave curves
    pub fn has_concave_curves(&self) -> bool {
        self.top_left.is_concave()
            || self.top_right.is_concave()
            || self.bottom_right.is_concave()
            || self.bottom_left.is_concave()
    }

    /// Check if any corner has step notches
    pub fn has_step_corners(&self) -> bool {
        self.top_left.is_step()
            || self.top_right.is_step()
            || self.bottom_right.is_step()
            || self.bottom_left.is_step()
    }

    /// Check if any corner requires custom path rendering (concave or step)
    pub fn needs_custom_rendering(&self) -> bool {
        self.has_concave_curves() || self.has_step_corners()
    }

    /// Convert to standard CornerRadius (ignoring concave/step)
    /// Used for simple shapes that don't need custom corners
    pub fn to_corner_radius(&self) -> CornerRadius {
        CornerRadius {
            top_left: if self.top_left.is_concave() || self.top_left.is_step() {
                0.0
            } else {
                self.top_left.radius
            },
            top_right: if self.top_right.is_concave() || self.top_right.is_step() {
                0.0
            } else {
                self.top_right.radius
            },
            bottom_right: if self.bottom_right.is_concave() || self.bottom_right.is_step() {
                0.0
            } else {
                self.bottom_right.radius
            },
            bottom_left: if self.bottom_left.is_concave() || self.bottom_left.is_step() {
                0.0
            } else {
                self.bottom_left.radius
            },
        }
    }
}

/// Configuration for a centered scoop/notch on an edge
///
/// This creates an inward curve in the center of an edge, like Apple's Dynamic Island.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CenterScoop {
    /// Width of the scoop
    pub width: f32,
    /// Depth of the scoop (how far it curves inward)
    pub depth: f32,
    /// Corner radius at scoop entry/exit points for smoother transitions
    pub corner_radius: f32,
}

/// Configuration for a centered bulge (outward protrusion) on an edge
///
/// This creates an outward curve in the center of an edge - the opposite of a scoop.
/// Useful for highlighting active items in a bottom navigation bar.
///
/// ```text
/// ─────────╲         ╱─────────
///           ╲───────╱           ← bulge protrudes outward
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CenterBulge {
    /// Width of the bulge
    pub width: f32,
    /// Height of the bulge (how far it protrudes outward)
    pub height: f32,
    /// Corner radius at bulge entry/exit points for smoother transitions
    pub corner_radius: f32,
}

/// Configuration for a centered V-cut (sharp inward notch) on an edge
///
/// This creates a sharp V-shaped cut in the center of an edge.
/// The angle is determined by the ratio of width to depth.
///
/// ```text
/// ─────────╲    ╱─────────
///           ╲  ╱
///            ╲╱   ← sharp point at depth
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CenterCut {
    /// Width of the cut at the edge
    pub width: f32,
    /// Depth of the cut (how far it goes inward)
    pub depth: f32,
}

/// Configuration for a centered V-peak (sharp outward point) on an edge
///
/// This creates a sharp V-shaped peak protruding from the center of an edge.
/// The angle is determined by the ratio of width to height.
///
/// ```text
///            ╱╲   ← sharp point at height
///           ╱  ╲
/// ─────────╱    ╲─────────
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CenterPeak {
    /// Width of the peak at the base
    pub width: f32,
    /// Height of the peak (how far it protrudes outward)
    pub height: f32,
}

// =============================================================================
// Notch Element
// =============================================================================

/// A notch element for shapes with concave curves
///
/// Unlike `div()` which only supports convex (inward) corner rounding,
/// `Notch` supports concave (outward) curves for patterns like:
///
/// - **Menu bar dropdowns**: Concave curves at top connect to the bar
/// - **Tabs**: Concave curves where they meet adjacent content
pub struct Notch {
    // Corner configuration
    pub(crate) corners: CornersConfig,

    // Center scoop configuration (for Dynamic Island-style centered notches)
    pub(crate) top_center_scoop: Option<CenterScoop>,
    pub(crate) bottom_center_scoop: Option<CenterScoop>,

    // Center bulge configuration (for active menu highlighting)
    pub(crate) top_center_bulge: Option<CenterBulge>,
    pub(crate) bottom_center_bulge: Option<CenterBulge>,

    // Center cut configuration (for sharp V-shaped inward cuts)
    pub(crate) top_center_cut: Option<CenterCut>,
    pub(crate) bottom_center_cut: Option<CenterCut>,

    // Center peak configuration (for sharp V-shaped outward peaks)
    pub(crate) top_center_peak: Option<CenterPeak>,
    pub(crate) bottom_center_peak: Option<CenterPeak>,

    // Layout
    pub(crate) style: Style,
    pub(crate) children: Vec<Box<dyn ElementBuilder>>,

    // Visual
    pub(crate) background: Option<Brush>,
    pub(crate) border_color: Option<Color>,
    pub(crate) border_width: f32,
    pub(crate) shadow: Vec<Shadow>,
    pub(crate) material: Option<Material>,
    pub(crate) opacity: f32,
    pub(crate) render_layer: RenderLayer,

    // Interaction
    pub(crate) event_handlers: EventHandlers,
    pub(crate) element_id: Option<String>,
    /// When true, the notch's hit box is transparent to pointer
    /// events — hit-test passes through to whatever sits beneath it
    /// in z-order. Use when the notch is a decorative overlay (e.g.
    /// a dropdown panel positioned over a menu bar) whose bbox would
    /// otherwise block hovers on the items below.
    pub(crate) pointer_events_none: bool,

    pub inner: Div,
}

impl Notch {
    /// Create a new notch element
    pub fn new() -> Self {
        Self {
            corners: CornersConfig::NONE,
            top_center_scoop: None,
            bottom_center_scoop: None,
            top_center_bulge: None,
            bottom_center_bulge: None,
            top_center_cut: None,
            bottom_center_cut: None,
            top_center_peak: None,
            bottom_center_peak: None,
            style: Style::default(),
            children: Vec::new(),
            background: None,
            border_color: None,
            border_width: 0.0,
            shadow: Vec::new(),
            material: None,
            opacity: 1.0,
            render_layer: RenderLayer::default(),
            event_handlers: EventHandlers::default(),
            element_id: None,
            pointer_events_none: false,
            inner: Div::new(),
        }
    }

    // =========================================================================
    // Signed Radius Corners (for animation)
    // =========================================================================
    // Convention: negative = concave, positive = convex, zero = sharp
    // This allows smooth animation through the crossover point.

    /// Set top corners with signed radius
    ///
    /// - Negative: concave (curves outward) - the notch effect
    /// - Positive: convex (standard rounding)
    /// - Zero: sharp corner
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Animate from convex to concave
    /// let r = ctx.spring("top", if open { -24.0 } else { 16.0 });
    /// notch().corner_top(r)
    /// ```
    pub fn corner_top(mut self, radius: f32) -> Self {
        if radius < 0.0 {
            self.corners.top_left = CornerConfig::concave(-radius);
            self.corners.top_right = CornerConfig::concave(-radius);
        } else {
            self.corners.top_left = CornerConfig::convex(radius);
            self.corners.top_right = CornerConfig::convex(radius);
        }
        self
    }

    /// Set bottom corners with signed radius
    pub fn corner_bottom(mut self, radius: f32) -> Self {
        if radius < 0.0 {
            self.corners.bottom_left = CornerConfig::concave(-radius);
            self.corners.bottom_right = CornerConfig::concave(-radius);
        } else {
            self.corners.bottom_left = CornerConfig::convex(radius);
            self.corners.bottom_right = CornerConfig::convex(radius);
        }
        self
    }

    /// Set left corners with signed radius
    pub fn corner_left(mut self, radius: f32) -> Self {
        if radius < 0.0 {
            self.corners.top_left = CornerConfig::concave(-radius);
            self.corners.bottom_left = CornerConfig::concave(-radius);
        } else {
            self.corners.top_left = CornerConfig::convex(radius);
            self.corners.bottom_left = CornerConfig::convex(radius);
        }
        self
    }

    /// Set right corners with signed radius
    pub fn corner_right(mut self, radius: f32) -> Self {
        if radius < 0.0 {
            self.corners.top_right = CornerConfig::concave(-radius);
            self.corners.bottom_right = CornerConfig::concave(-radius);
        } else {
            self.corners.top_right = CornerConfig::convex(radius);
            self.corners.bottom_right = CornerConfig::convex(radius);
        }
        self
    }

    /// Set top-left corner with signed radius
    pub fn corner_tl(mut self, radius: f32) -> Self {
        if radius < 0.0 {
            self.corners.top_left = CornerConfig::concave(-radius);
        } else {
            self.corners.top_left = CornerConfig::convex(radius);
        }
        self
    }

    /// Set top-right corner with signed radius
    pub fn corner_tr(mut self, radius: f32) -> Self {
        if radius < 0.0 {
            self.corners.top_right = CornerConfig::concave(-radius);
        } else {
            self.corners.top_right = CornerConfig::convex(radius);
        }
        self
    }

    /// Set bottom-right corner with signed radius
    pub fn corner_br(mut self, radius: f32) -> Self {
        if radius < 0.0 {
            self.corners.bottom_right = CornerConfig::concave(-radius);
        } else {
            self.corners.bottom_right = CornerConfig::convex(radius);
        }
        self
    }

    /// Set bottom-left corner with signed radius
    pub fn corner_bl(mut self, radius: f32) -> Self {
        if radius < 0.0 {
            self.corners.bottom_left = CornerConfig::concave(-radius);
        } else {
            self.corners.bottom_left = CornerConfig::convex(radius);
        }
        self
    }

    // =========================================================================
    // Concave Curves - The key differentiator from div()
    // =========================================================================

    /// Add concave curves on the left side (top-left and bottom-left corners bow left)
    ///
    /// ```text
    /// ╭───╯     
    /// │ ← curves bow left
    /// ╰───╮     
    /// ```
    pub fn concave_left(mut self, radius: f32) -> Self {
        self.corners.top_left = CornerConfig::concave(radius);
        self.corners.bottom_left = CornerConfig::concave(radius);
        self
    }

    /// Add concave curves on the right side (top-right and bottom-right corners bow right)
    ///
    /// ```text
    ///     ╰───╮
    ///         │ → curves bow right
    ///     ╭───╯
    /// ```
    pub fn concave_right(mut self, radius: f32) -> Self {
        self.corners.top_right = CornerConfig::concave(radius);
        self.corners.bottom_right = CornerConfig::concave(radius);
        self
    }

    /// Add concave curves on the top (top-left bows left, top-right bows right)
    ///
    /// This is the key method for creating notched dropdown shapes:
    /// ```text
    /// ╭───╯              ╰───╮
    /// │   ↑ bows left/right ↑│
    /// ```
    pub fn concave_top(mut self, radius: f32) -> Self {
        self.corners.top_left = CornerConfig::concave(radius);
        self.corners.top_right = CornerConfig::concave(radius);
        self
    }

    /// Add concave curves on the bottom (bottom-left bows left, bottom-right bows right)
    ///
    /// ```text
    /// │   ↓ bows left/right ↓│
    /// ╰───╮              ╭───╯
    /// ```
    pub fn concave_bottom(mut self, radius: f32) -> Self {
        self.corners.bottom_left = CornerConfig::concave(radius);
        self.corners.bottom_right = CornerConfig::concave(radius);
        self
    }

    /// Add concave curve to top-left corner (bows up-left)
    pub fn concave_tl(mut self, radius: f32) -> Self {
        self.corners.top_left = CornerConfig::concave(radius);
        self
    }

    /// Add concave curve to top-right corner (bows up-right)
    pub fn concave_tr(mut self, radius: f32) -> Self {
        self.corners.top_right = CornerConfig::concave(radius);
        self
    }

    /// Add concave curve to bottom-right corner (bows down-right)
    pub fn concave_br(mut self, radius: f32) -> Self {
        self.corners.bottom_right = CornerConfig::concave(radius);
        self
    }

    /// Add concave curve to bottom-left corner (bows down-left)
    pub fn concave_bl(mut self, radius: f32) -> Self {
        self.corners.bottom_left = CornerConfig::concave(radius);
        self
    }

    // =========================================================================
    // Step Corners (Sharp Right-Angle Notches)
    // =========================================================================

    /// Add sharp step notches to the top corners
    ///
    /// ```text
    ///     ┌──┐          ┌──┐
    ///     │  │          │  │
    /// ────┘  └──────────┘  └────
    /// ```
    pub fn step_top(mut self, depth: f32) -> Self {
        self.corners.top_left = CornerConfig::step(depth);
        self.corners.top_right = CornerConfig::step(depth);
        self
    }

    /// Add sharp step notches to the bottom corners
    pub fn step_bottom(mut self, depth: f32) -> Self {
        self.corners.bottom_left = CornerConfig::step(depth);
        self.corners.bottom_right = CornerConfig::step(depth);
        self
    }

    /// Add sharp step notches to the left corners
    pub fn step_left(mut self, depth: f32) -> Self {
        self.corners.top_left = CornerConfig::step(depth);
        self.corners.bottom_left = CornerConfig::step(depth);
        self
    }

    /// Add sharp step notches to the right corners
    pub fn step_right(mut self, depth: f32) -> Self {
        self.corners.top_right = CornerConfig::step(depth);
        self.corners.bottom_right = CornerConfig::step(depth);
        self
    }

    /// Add step notch to top-left corner
    pub fn step_tl(mut self, depth: f32) -> Self {
        self.corners.top_left = CornerConfig::step(depth);
        self
    }

    /// Add step notch to top-right corner
    pub fn step_tr(mut self, depth: f32) -> Self {
        self.corners.top_right = CornerConfig::step(depth);
        self
    }

    /// Add step notch to bottom-right corner
    pub fn step_br(mut self, depth: f32) -> Self {
        self.corners.bottom_right = CornerConfig::step(depth);
        self
    }

    /// Add step notch to bottom-left corner
    pub fn step_bl(mut self, depth: f32) -> Self {
        self.corners.bottom_left = CornerConfig::step(depth);
        self
    }

    // =========================================================================
    // Center Scoops (Dynamic Island-style)
    // =========================================================================

    /// Add a center scoop on the top edge
    ///
    /// Creates an inward curve in the center of the top edge, like Apple's Dynamic Island.
    /// The scoop curves INTO the shape (downward), not outward.
    ///
    /// ```text
    ///     ╭─────────╮
    ///     │ ╲_____╱ │  ← scoop cuts into top
    ///     │         │
    ///     ╰─────────╯
    /// ```
    pub fn center_scoop_top(mut self, width: f32, depth: f32) -> Self {
        self.top_center_scoop = Some(CenterScoop {
            width,
            depth,
            corner_radius: 0.0,
        });
        self
    }

    /// Add a center scoop on the top edge with rounded corners
    ///
    /// Like `center_scoop_top`, but with smooth corner transitions at scoop entry/exit.
    ///
    /// ```text
    ///     ╭─────────╮
    ///     │╭╲_____╱╮│  ← rounded corners at scoop edges
    ///     │         │
    ///     ╰─────────╯
    /// ```
    pub fn center_scoop_top_rounded(mut self, width: f32, depth: f32, corner_radius: f32) -> Self {
        self.top_center_scoop = Some(CenterScoop {
            width,
            depth,
            corner_radius,
        });
        self
    }

    /// Add a center scoop on the bottom edge
    ///
    /// Creates an inward curve in the center of the bottom edge.
    /// The scoop curves INTO the shape (upward), not outward.
    ///
    /// ```text
    ///     ╭─────────╮
    ///     │         │
    ///     │ ╱─────╲ │  ← scoop cuts into bottom
    ///     ╰─────────╯
    /// ```
    pub fn center_scoop_bottom(mut self, width: f32, depth: f32) -> Self {
        self.bottom_center_scoop = Some(CenterScoop {
            width,
            depth,
            corner_radius: 0.0,
        });
        self
    }

    /// Add a center scoop on the bottom edge with rounded corners
    ///
    /// Like `center_scoop_bottom`, but with smooth corner transitions at scoop entry/exit.
    pub fn center_scoop_bottom_rounded(
        mut self,
        width: f32,
        depth: f32,
        corner_radius: f32,
    ) -> Self {
        self.bottom_center_scoop = Some(CenterScoop {
            width,
            depth,
            corner_radius,
        });
        self
    }

    // =========================================================================
    // Center Bulges (Outward Protrusions)
    // =========================================================================

    /// Add a center bulge on the top edge
    ///
    /// Creates an outward curve in the center of the top edge - the opposite of a scoop.
    /// The bulge protrudes OUT of the shape (upward).
    ///
    /// ```text
    ///           ╱───────╲
    ///     ╭────╱         ╲────╮
    ///     │                   │  ← bulge protrudes upward
    ///     ╰───────────────────╯
    /// ```
    pub fn center_bulge_top(mut self, width: f32, height: f32) -> Self {
        self.top_center_bulge = Some(CenterBulge {
            width,
            height,
            corner_radius: 0.0,
        });
        self
    }

    /// Add a center bulge on the top edge with rounded corners
    ///
    /// Like `center_bulge_top`, but with smooth corner transitions at bulge entry/exit.
    pub fn center_bulge_top_rounded(mut self, width: f32, height: f32, corner_radius: f32) -> Self {
        self.top_center_bulge = Some(CenterBulge {
            width,
            height,
            corner_radius,
        });
        self
    }

    /// Add a center bulge on the bottom edge
    ///
    /// Creates an outward curve in the center of the bottom edge - the opposite of a scoop.
    /// The bulge protrudes OUT of the shape (downward).
    /// Perfect for highlighting active items in a bottom navigation bar.
    ///
    /// ```text
    ///     ╭───────────────────╮
    ///     │                   │
    ///     ╰────╲         ╱────╯
    ///           ╲───────╱       ← bulge protrudes downward (active indicator)
    /// ```
    pub fn center_bulge_bottom(mut self, width: f32, height: f32) -> Self {
        self.bottom_center_bulge = Some(CenterBulge {
            width,
            height,
            corner_radius: 0.0,
        });
        self
    }

    /// Add a center bulge on the bottom edge with rounded corners
    ///
    /// Like `center_bulge_bottom`, but with smooth corner transitions at bulge entry/exit.
    /// Great for smooth, polished active menu indicators.
    pub fn center_bulge_bottom_rounded(
        mut self,
        width: f32,
        height: f32,
        corner_radius: f32,
    ) -> Self {
        self.bottom_center_bulge = Some(CenterBulge {
            width,
            height,
            corner_radius,
        });
        self
    }

    // =========================================================================
    // Center Cuts (Sharp V-shaped Inward)
    // =========================================================================

    /// Add a center V-cut on the top edge
    ///
    /// Creates a sharp V-shaped cut in the center of the top edge.
    /// The cut goes INWARD (downward into the shape).
    ///
    /// ```text
    /// ─────────╲    ╱─────────
    ///           ╲  ╱
    ///            ╲╱   ← sharp point at depth
    /// ```
    ///
    /// The angle is determined by the width/depth ratio:
    /// - Narrow width + deep = steep angle
    /// - Wide width + shallow = gentle angle
    pub fn center_cut_top(mut self, width: f32, depth: f32) -> Self {
        self.top_center_cut = Some(CenterCut { width, depth });
        self
    }

    /// Add a center V-cut on the bottom edge
    ///
    /// Creates a sharp V-shaped cut in the center of the bottom edge.
    /// The cut goes INWARD (upward into the shape).
    ///
    /// ```text
    ///            ╱╲   ← sharp point at depth
    ///           ╱  ╲
    /// ─────────╱    ╲─────────
    /// ```
    pub fn center_cut_bottom(mut self, width: f32, depth: f32) -> Self {
        self.bottom_center_cut = Some(CenterCut { width, depth });
        self
    }

    // =========================================================================
    // Center Peaks (Sharp V-shaped Outward)
    // =========================================================================

    /// Add a center V-peak on the top edge
    ///
    /// Creates a sharp V-shaped peak protruding from the center of the top edge.
    /// The peak goes OUTWARD (upward out of the shape).
    ///
    /// ```text
    ///            ╱╲   ← sharp point at height
    ///           ╱  ╲
    /// ─────────╱    ╲─────────
    /// ```
    ///
    /// The angle is determined by the width/height ratio:
    /// - Narrow width + tall = steep angle
    /// - Wide width + short = gentle angle
    pub fn center_peak_top(mut self, width: f32, height: f32) -> Self {
        self.top_center_peak = Some(CenterPeak { width, height });
        self
    }

    /// Add a center V-peak on the bottom edge
    ///
    /// Creates a sharp V-shaped peak protruding from the center of the bottom edge.
    /// The peak goes OUTWARD (downward out of the shape).
    ///
    /// ```text
    /// ─────────╲    ╱─────────
    ///           ╲  ╱
    ///            ╲╱   ← sharp point at height
    /// ```
    pub fn center_peak_bottom(mut self, width: f32, height: f32) -> Self {
        self.bottom_center_peak = Some(CenterPeak { width, height });
        self
    }

    // =========================================================================
    // Inner Curves (Convex) - Standard rounded corners
    // =========================================================================

    /// Set uniform inner (convex) corner radius for all corners
    pub fn rounded(mut self, radius: f32) -> Self {
        self.corners.top_left = CornerConfig::convex(radius);
        self.corners.top_right = CornerConfig::convex(radius);
        self.corners.bottom_right = CornerConfig::convex(radius);
        self.corners.bottom_left = CornerConfig::convex(radius);
        self
    }

    /// Round only the bottom corners (inner/convex)
    pub fn rounded_bottom(mut self, radius: f32) -> Self {
        self.corners.bottom_left = CornerConfig::convex(radius);
        self.corners.bottom_right = CornerConfig::convex(radius);
        self
    }

    /// Round only the top corners (inner/convex)
    pub fn rounded_top(mut self, radius: f32) -> Self {
        self.corners.top_left = CornerConfig::convex(radius);
        self.corners.top_right = CornerConfig::convex(radius);
        self
    }

    /// Round only the left corners (inner/convex)
    pub fn rounded_left(mut self, radius: f32) -> Self {
        self.corners.top_left = CornerConfig::convex(radius);
        self.corners.bottom_left = CornerConfig::convex(radius);
        self
    }

    /// Round only the right corners (inner/convex)
    pub fn rounded_right(mut self, radius: f32) -> Self {
        self.corners.top_right = CornerConfig::convex(radius);
        self.corners.bottom_right = CornerConfig::convex(radius);
        self
    }

    /// Round top-left corner (inner/convex)
    pub fn rounded_tl(mut self, radius: f32) -> Self {
        self.corners.top_left = CornerConfig::convex(radius);
        self
    }

    /// Round top-right corner (inner/convex)
    pub fn rounded_tr(mut self, radius: f32) -> Self {
        self.corners.top_right = CornerConfig::convex(radius);
        self
    }

    /// Round bottom-right corner (inner/convex)
    pub fn rounded_br(mut self, radius: f32) -> Self {
        self.corners.bottom_right = CornerConfig::convex(radius);
        self
    }

    /// Round bottom-left corner (inner/convex)
    pub fn rounded_bl(mut self, radius: f32) -> Self {
        self.corners.bottom_left = CornerConfig::convex(radius);
        self
    }

    /// Make fully rounded (pill shape)
    pub fn rounded_full(mut self) -> Self {
        self.corners.top_left = CornerConfig::convex(9999.0);
        self.corners.top_right = CornerConfig::convex(9999.0);
        self.corners.bottom_right = CornerConfig::convex(9999.0);
        self.corners.bottom_left = CornerConfig::convex(9999.0);
        self
    }

    // =========================================================================
    // Size & Layout
    // =========================================================================

    /// Set fixed width
    pub fn w(mut self, width: f32) -> Self {
        self.style.size.width = Dimension::length(width);
        self
    }

    /// Set fixed height
    pub fn h(mut self, height: f32) -> Self {
        self.style.size.height = Dimension::length(height);
        self
    }

    /// Set both width and height to the same value
    pub fn size(mut self, size: f32) -> Self {
        self.style.size.width = Dimension::length(size);
        self.style.size.height = Dimension::length(size);
        self
    }

    /// Set width to 100%
    pub fn w_full(mut self) -> Self {
        self.style.size.width = Dimension::percent(1.0);
        self
    }

    /// Set height to 100%
    pub fn h_full(mut self) -> Self {
        self.style.size.height = Dimension::percent(1.0);
        self
    }

    /// Set width to fit content
    pub fn w_fit(mut self) -> Self {
        self.style.size.width = Dimension::auto();
        self
    }

    /// Set height to fit content
    pub fn h_fit(mut self) -> Self {
        self.style.size.height = Dimension::auto();
        self
    }

    /// Set minimum width
    pub fn min_w(mut self, width: f32) -> Self {
        self.style.min_size.width = LengthPercentageAuto::length(width);
        self
    }

    /// Set minimum height
    pub fn min_h(mut self, height: f32) -> Self {
        self.style.min_size.height = LengthPercentageAuto::length(height);
        self
    }

    /// Set maximum width
    pub fn max_w(mut self, width: f32) -> Self {
        self.style.max_size.width = LengthPercentageAuto::length(width);
        self
    }

    /// Set maximum height
    pub fn max_h(mut self, height: f32) -> Self {
        self.style.max_size.height = LengthPercentageAuto::length(height);
        self
    }

    // =========================================================================
    // Padding
    // =========================================================================

    /// Set uniform padding on all sides
    pub fn p(mut self, padding: f32) -> Self {
        self.style.padding = taffy::Rect {
            left: LengthPercentage::length(padding),
            right: LengthPercentage::length(padding),
            top: LengthPercentage::length(padding),
            bottom: LengthPercentage::length(padding),
        };
        self
    }

    /// Set horizontal padding (left and right)
    pub fn px(mut self, padding: f32) -> Self {
        self.style.padding.left = LengthPercentage::length(padding);
        self.style.padding.right = LengthPercentage::length(padding);
        self
    }

    /// Set vertical padding (top and bottom)
    pub fn py(mut self, padding: f32) -> Self {
        self.style.padding.top = LengthPercentage::length(padding);
        self.style.padding.bottom = LengthPercentage::length(padding);
        self
    }

    /// Set top padding
    pub fn pt(mut self, padding: f32) -> Self {
        self.style.padding.top = LengthPercentage::length(padding);
        self
    }

    /// Set bottom padding
    pub fn pb(mut self, padding: f32) -> Self {
        self.style.padding.bottom = LengthPercentage::length(padding);
        self
    }

    /// Set left padding
    pub fn pl(mut self, padding: f32) -> Self {
        self.style.padding.left = LengthPercentage::length(padding);
        self
    }

    /// Set right padding
    pub fn pr(mut self, padding: f32) -> Self {
        self.style.padding.right = LengthPercentage::length(padding);
        self
    }

    // =========================================================================
    // Margin
    // =========================================================================

    /// Set uniform margin on all sides
    pub fn m(mut self, margin: f32) -> Self {
        self.style.margin = taffy::Rect {
            left: LengthPercentageAuto::length(margin),
            right: LengthPercentageAuto::length(margin),
            top: LengthPercentageAuto::length(margin),
            bottom: LengthPercentageAuto::length(margin),
        };
        self
    }

    /// Set horizontal margin (left and right)
    pub fn mx(mut self, margin: f32) -> Self {
        self.style.margin.left = LengthPercentageAuto::length(margin);
        self.style.margin.right = LengthPercentageAuto::length(margin);
        self
    }

    /// Set vertical margin (top and bottom)
    pub fn my(mut self, margin: f32) -> Self {
        self.style.margin.top = LengthPercentageAuto::length(margin);
        self.style.margin.bottom = LengthPercentageAuto::length(margin);
        self
    }

    /// Center horizontally with auto margins
    pub fn mx_auto(mut self) -> Self {
        self.style.margin.left = LengthPercentageAuto::auto();
        self.style.margin.right = LengthPercentageAuto::auto();
        self
    }

    // =========================================================================
    // Flexbox
    // =========================================================================

    /// Set flex direction to column
    pub fn flex_col(mut self) -> Self {
        self.style.display = Display::Flex;
        self.style.flex_direction = FlexDirection::Column;
        self
    }

    /// Set flex direction to row
    pub fn flex_row(mut self) -> Self {
        self.style.display = Display::Flex;
        self.style.flex_direction = FlexDirection::Row;
        self
    }

    /// Set gap between children
    pub fn gap(mut self, gap: f32) -> Self {
        self.style.gap = taffy::Size {
            width: LengthPercentage::length(gap),
            height: LengthPercentage::length(gap),
        };
        self
    }

    /// Center children both horizontally and vertically
    pub fn flex_center(mut self) -> Self {
        self.style.display = Display::Flex;
        self.style.justify_content = Some(JustifyContent::CENTER);
        self.style.align_items = Some(AlignItems::CENTER);
        self
    }

    /// Justify content to start
    pub fn justify_start(mut self) -> Self {
        self.style.justify_content = Some(JustifyContent::START);
        self
    }

    /// Justify content to center
    pub fn justify_center(mut self) -> Self {
        self.style.justify_content = Some(JustifyContent::CENTER);
        self
    }

    /// Justify content to end
    pub fn justify_end(mut self) -> Self {
        self.style.justify_content = Some(JustifyContent::END);
        self
    }

    /// Justify content with space between
    pub fn justify_between(mut self) -> Self {
        self.style.justify_content = Some(JustifyContent::SPACE_BETWEEN);
        self
    }

    /// Align items to start
    pub fn items_start(mut self) -> Self {
        self.style.align_items = Some(AlignItems::START);
        self
    }

    /// Align items to center
    pub fn items_center(mut self) -> Self {
        self.style.align_items = Some(AlignItems::CENTER);
        self
    }

    /// Align items to end
    pub fn items_end(mut self) -> Self {
        self.style.align_items = Some(AlignItems::END);
        self
    }

    /// Flex grow
    pub fn flex_grow(mut self) -> Self {
        self.style.flex_grow = 1.0;
        self
    }

    /// Flex shrink
    pub fn flex_shrink(mut self) -> Self {
        self.style.flex_shrink = 1.0;
        self
    }

    /// Flex none (don't grow or shrink)
    pub fn flex_none(mut self) -> Self {
        self.style.flex_grow = 0.0;
        self.style.flex_shrink = 0.0;
        self
    }

    // =========================================================================
    // Positioning
    // =========================================================================

    /// Set position to absolute
    pub fn absolute(mut self) -> Self {
        self.style.position = Position::Absolute;
        self
    }

    /// Set position to relative
    pub fn relative(mut self) -> Self {
        self.style.position = Position::Relative;
        self
    }

    /// Set top offset
    pub fn top(mut self, offset: f32) -> Self {
        self.style.inset.top = LengthPercentageAuto::length(offset);
        self
    }

    /// Set bottom offset
    pub fn bottom(mut self, offset: f32) -> Self {
        self.style.inset.bottom = LengthPercentageAuto::length(offset);
        self
    }

    /// Set left offset
    pub fn left(mut self, offset: f32) -> Self {
        self.style.inset.left = LengthPercentageAuto::length(offset);
        self
    }

    /// Set right offset
    pub fn right(mut self, offset: f32) -> Self {
        self.style.inset.right = LengthPercentageAuto::length(offset);
        self
    }

    // =========================================================================
    // Visual Styling
    // =========================================================================

    /// Set background color
    pub fn bg(mut self, color: impl Into<Color>) -> Self {
        self.background = Some(Brush::Solid(color.into()));
        self
    }

    /// Set background brush (color, gradient, or glass)
    pub fn background(mut self, brush: impl Into<Brush>) -> Self {
        self.background = Some(brush.into());
        self
    }

    /// Set border color and width
    pub fn border(mut self, width: f32, color: impl Into<Color>) -> Self {
        self.border_width = width;
        self.border_color = Some(color.into());
        self
    }

    /// Set border width only
    pub fn border_width(mut self, width: f32) -> Self {
        self.border_width = width;
        self
    }

    /// Set border color only
    pub fn border_color(mut self, color: impl Into<Color>) -> Self {
        self.border_color = Some(color.into());
        self
    }

    /// Add a single drop shadow (replaces any existing stack).
    pub fn shadow(mut self, shadow: Shadow) -> Self {
        self.shadow = vec![shadow];
        self
    }

    /// Apply a compound drop shadow stack.
    pub fn shadow_stack(mut self, shadows: Vec<Shadow>) -> Self {
        self.shadow = shadows;
        self
    }

    /// Add a small shadow
    pub fn shadow_sm(mut self) -> Self {
        self.shadow = vec![Shadow::new(0.0, 1.0, 3.0, Color::rgba(0.0, 0.0, 0.0, 0.1))];
        self
    }

    /// Add a medium shadow
    pub fn shadow_md(mut self) -> Self {
        self.shadow = vec![Shadow::new(0.0, 4.0, 6.0, Color::rgba(0.0, 0.0, 0.0, 0.1))];
        self
    }

    /// Add a large shadow
    pub fn shadow_lg(mut self) -> Self {
        self.shadow = vec![Shadow::new(
            0.0,
            10.0,
            15.0,
            Color::rgba(0.0, 0.0, 0.0, 0.1),
        )];
        self
    }

    /// Set opacity
    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }

    /// Apply glass/frosted material effect
    pub fn glass(mut self) -> Self {
        self.material = Some(Material::Glass(Default::default()));
        self.render_layer = RenderLayer::Glass;
        self
    }

    /// Make the notch transparent to pointer events. Hit-test passes
    /// through to whatever sits beneath it in z-order. Useful when
    /// the notch is a decorative overlay (e.g. a dropdown panel over
    /// a menu bar) whose bbox would otherwise block hovers on the
    /// items underneath.
    pub fn pointer_events_none(mut self) -> Self {
        self.pointer_events_none = true;
        self
    }

    // =========================================================================
    // Overflow
    // =========================================================================

    /// Clip overflowing content
    pub fn overflow_clip(mut self) -> Self {
        self.style.overflow.x = Overflow::Hidden;
        self.style.overflow.y = Overflow::Hidden;
        self
    }

    /// Allow content to overflow
    pub fn overflow_visible(mut self) -> Self {
        self.style.overflow.x = Overflow::Visible;
        self.style.overflow.y = Overflow::Visible;
        self
    }

    // =========================================================================
    // Children
    // =========================================================================

    /// Add a child element
    pub fn child(mut self, child: impl ElementBuilder + 'static) -> Self {
        self.children.push(Box::new(child));
        self
    }

    /// Add a boxed child element.
    pub fn child_box(mut self, child: Box<dyn ElementBuilder>) -> Self {
        self.children.push(child);
        self
    }

    /// Add multiple children from an iterator
    pub fn children(
        mut self,
        children: impl IntoIterator<Item = impl ElementBuilder + 'static>,
    ) -> Self {
        for child in children {
            self.children.push(Box::new(child));
        }
        self
    }

    // =========================================================================
    // Conditional Builders
    // =========================================================================

    /// Conditionally apply a transformation to self.
    #[inline]
    pub fn when<F>(self, condition: bool, f: F) -> Self
    where
        F: FnOnce(Self) -> Self,
    {
        if condition { f(self) } else { self }
    }

    // =========================================================================
    // Events
    // =========================================================================

    // =========================================================================
    // Event Handlers
    // =========================================================================

    /// Register a click handler
    pub fn on_click<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = self.inner.on_click(handler);
        self
    }

    /// Register a mouse down handler
    pub fn on_mouse_down<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = self.inner.on_mouse_down(handler);
        self
    }

    /// Register a mouse up handler
    pub fn on_mouse_up<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = self.inner.on_mouse_up(handler);
        self
    }

    /// Register a mouse move handler
    pub fn on_mouse_move<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = self.inner.on_mouse_move(handler);
        self
    }

    /// Register a drag handler
    pub fn on_drag<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = self.inner.on_drag(handler);
        self
    }

    /// Register a scroll handler
    pub fn on_scroll<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = self.inner.on_scroll(handler);
        self
    }

    /// Register a hover enter handler
    pub fn on_hover_enter<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = self.inner.on_hover_enter(handler);
        self
    }

    /// Register a hover leave handler
    pub fn on_hover_leave<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = self.inner.on_hover_leave(handler);
        self
    }

    /// Register a focus handler
    pub fn on_focus<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = self.inner.on_focus(handler);
        self
    }

    /// Register a blur handler
    pub fn on_blur<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = self.inner.on_blur(handler);
        self
    }
    // =========================================================================
    // ID
    // =========================================================================

    /// Set element ID for selector API queries
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.element_id = Some(id.into());
        self
    }
}

impl Default for Notch {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Path Building for Complex Notchs
// =============================================================================

/// Build the path for a shape with configurable corners, scoops, bulges, cuts, and peaks
///
/// This handles convex (standard rounding), concave curves, step notches,
/// center scoops (Dynamic Island-style centered indentations),
/// center bulges (outward protrusions for active menu highlighting),
/// Apply a uniform opacity multiplier to a brush.
///
/// Solid brushes get their alpha scaled directly; gradient brushes get a
/// new `Gradient` with every stop's alpha scaled. Non-color brushes (glass,
/// blur, image) pass through unchanged — their opacity is handled
/// elsewhere in the pipeline.
fn apply_brush_opacity(brush: Brush, opacity: f32) -> Brush {
    if opacity >= 1.0 {
        return brush;
    }
    match brush {
        Brush::Solid(color) => Brush::Solid(color.with_alpha(color.a * opacity)),
        Brush::Gradient(g) => {
            let new_stops: Vec<_> = g
                .stops()
                .iter()
                .map(|stop| {
                    blinc_core::GradientStop::new(
                        stop.offset,
                        stop.color.with_alpha(stop.color.a * opacity),
                    )
                })
                .collect();
            let new_gradient = match g {
                Gradient::Linear {
                    start,
                    end,
                    space,
                    spread,
                    ..
                } => Gradient::Linear {
                    start,
                    end,
                    stops: new_stops,
                    space,
                    spread,
                },
                Gradient::Radial {
                    center,
                    radius,
                    focal,
                    space,
                    spread,
                    ..
                } => Gradient::Radial {
                    center,
                    radius,
                    focal,
                    stops: new_stops,
                    space,
                    spread,
                },
                Gradient::Conic {
                    center,
                    start_angle,
                    space,
                    ..
                } => Gradient::Conic {
                    center,
                    start_angle,
                    stops: new_stops,
                    space,
                },
            };
            Brush::Gradient(new_gradient)
        }
        other => other,
    }
}

/// center cuts (sharp V-shaped inward notches), and
/// center peaks (sharp V-shaped outward points).
/// The path is built clockwise starting from the top-left corner.
///
/// For convex corners: the curve stays inside the bounds
/// For concave corners: the curve bows outward, extending beyond the bounds
/// For center scoops: the curve bows inward, staying inside the bounds
/// For center bulges: the curve protrudes outward, extending beyond the bounds
/// For center cuts: sharp V going inward
/// For center peaks: sharp V going outward
#[allow(clippy::too_many_arguments)]
fn build_shape_path(
    bounds: Rect,
    corners: &CornersConfig,
    top_center_scoop: Option<&CenterScoop>,
    bottom_center_scoop: Option<&CenterScoop>,
    top_center_bulge: Option<&CenterBulge>,
    bottom_center_bulge: Option<&CenterBulge>,
    top_center_cut: Option<&CenterCut>,
    bottom_center_cut: Option<&CenterCut>,
    top_center_peak: Option<&CenterPeak>,
    bottom_center_peak: Option<&CenterPeak>,
) -> Path {
    let w = bounds.width();
    let h = bounds.height();
    let x = bounds.x();
    let y = bounds.y();

    // Clamp radii to half the smaller dimension
    let max_radius = (w.min(h)) / 2.0;
    let clamp = |r: f32| r.min(max_radius);

    let tl = &corners.top_left;
    let tr = &corners.top_right;
    let br = &corners.bottom_right;
    let bl = &corners.bottom_left;

    let tl_r = clamp(tl.radius);
    let tr_r = clamp(tr.radius);
    let br_r = clamp(br.radius);
    let bl_r = clamp(bl.radius);

    let mut path = Path::new();

    // Determine where the top edge starts (after top-left corner)
    let top_start_x = match tl.style {
        CornerStyle::Concave => x - tl_r,              // Extends left
        CornerStyle::Convex if tl_r > 0.0 => x + tl_r, // Inset
        _ => x,
    };

    // Start at the beginning of the top edge
    path = path.move_to(top_start_x, y);

    // Top edge - with optional center scoop
    let top_end_x = match tr.style {
        CornerStyle::Concave => x + w + tr_r, // Extends right
        CornerStyle::Convex if tr_r > 0.0 => x + w - tr_r, // Inset
        _ => x + w,
    };

    // Draw top center scoop if present
    // The scoop curves DOWN into the shape, creating a visible indent
    if let Some(scoop) = top_center_scoop {
        let center_x = x + w / 2.0;
        let scoop_start_x = center_x - scoop.width / 2.0;
        let scoop_end_x = center_x + scoop.width / 2.0;
        let scoop_bottom_y = y + scoop.depth;

        // Corner radius for smooth entry/exit transitions (clamped to 1/4 of scoop width)
        let cr = scoop.corner_radius.min(scoop.width / 4.0).max(0.0);

        // Cubic bezier control point factor for circular arc approximation
        // k = 4 * (sqrt(2) - 1) / 3 ≈ 0.5522847498
        const K: f32 = 0.552_284_8;

        if cr > 0.0 {
            // With corner radius: concave (outward-bowing) entry/exit transitions
            // Creates "ear" shapes like macOS Dynamic Island
            let effective_start_x = scoop_start_x + cr;
            let effective_end_x = scoop_end_x - cr;
            let effective_start_y = y + cr;
            let effective_rx = (effective_end_x - effective_start_x) / 2.0;
            let effective_ry = scoop.depth - cr;

            // Line to entry corner point
            path = path.line_to(scoop_start_x, y);

            // Entry corner: CONCAVE ear - control toward CENTER makes fill bulge outward
            path = path.quad_to(
                scoop_start_x + cr * 0.55,
                y, // control: toward center (right)
                effective_start_x,
                effective_start_y, // end at main scoop start
            );

            // Main scoop curve: from effective start to bottom center
            path = path.cubic_to(
                effective_start_x,
                effective_start_y + K * effective_ry, // control 1
                center_x - K * effective_rx,
                scoop_bottom_y, // control 2
                center_x,
                scoop_bottom_y, // end at bottom center
            );

            // Main scoop curve: from bottom center to effective end
            path = path.cubic_to(
                center_x + K * effective_rx,
                scoop_bottom_y, // control 1
                effective_end_x,
                effective_start_y + K * effective_ry, // control 2
                effective_end_x,
                effective_start_y, // end at effective end
            );

            // Exit corner: CONCAVE ear - control toward CENTER makes fill bulge outward
            path = path.quad_to(
                scoop_end_x - cr * 0.55,
                y, // control: toward center (left)
                scoop_end_x,
                y, // end at top edge
            );

            // Line to top edge end
            path = path.line_to(top_end_x, y);
        } else {
            // Without corner radius: sharp transitions (original behavior)
            let rx = scoop.width / 2.0;
            let ry = scoop.depth;

            // Line to scoop start
            path = path.line_to(scoop_start_x, y);

            // First quarter arc: from left edge to bottom center
            path = path.cubic_to(
                scoop_start_x,
                y + K * ry, // control 1
                center_x - K * rx,
                scoop_bottom_y, // control 2
                center_x,
                scoop_bottom_y, // end at bottom center
            );

            // Second quarter arc: from bottom center to right edge
            path = path.cubic_to(
                center_x + K * rx,
                scoop_bottom_y, // control 1
                scoop_end_x,
                y + K * ry, // control 2
                scoop_end_x,
                y, // end at right edge
            );

            // Line to top edge end
            path = path.line_to(top_end_x, y);
        }
    } else if let Some(bulge) = top_center_bulge {
        // Draw top center bulge - protrudes UPWARD out of the shape
        let center_x = x + w / 2.0;
        let bulge_start_x = center_x - bulge.width / 2.0;
        let bulge_end_x = center_x + bulge.width / 2.0;
        let bulge_top_y = y - bulge.height; // Protrudes upward (negative y)

        // Corner radius for smooth entry/exit transitions
        let cr = bulge.corner_radius.min(bulge.width / 4.0).max(0.0);

        const K: f32 = 0.552_284_8;

        if cr > 0.0 {
            // With corner radius: smooth convex entry/exit transitions
            // Control points placed at the "corner" for smooth quarter-circle curves
            let effective_start_x = bulge_start_x + cr;
            let effective_end_x = bulge_end_x - cr;
            let effective_start_y = y - cr; // Going upward
            let effective_rx = (effective_end_x - effective_start_x) / 2.0;
            let effective_ry = bulge.height - cr;

            // Line to entry corner point
            path = path.line_to(bulge_start_x, y);

            // Entry corner: smooth convex quarter-circle transition
            // Control at corner where horizontal tangent from start meets vertical tangent to end
            path = path.quad_to(
                effective_start_x, // control x: at the corner (right of start)
                y,                 // control y: at edge level (horizontal tangent)
                effective_start_x,
                effective_start_y, // end at main bulge start
            );

            // Main bulge curve: from effective start to top center
            path = path.cubic_to(
                effective_start_x,
                effective_start_y - K * effective_ry, // control 1
                center_x - K * effective_rx,
                bulge_top_y, // control 2
                center_x,
                bulge_top_y, // end at top center
            );

            // Main bulge curve: from top center to effective end
            path = path.cubic_to(
                center_x + K * effective_rx,
                bulge_top_y, // control 1
                effective_end_x,
                effective_start_y - K * effective_ry, // control 2
                effective_end_x,
                effective_start_y, // end at effective end
            );

            // Exit corner: smooth convex quarter-circle transition back to edge
            // Control at corner where vertical tangent from start meets horizontal tangent to end
            path = path.quad_to(
                effective_end_x, // control x: at the corner (left of end)
                y,               // control y: at edge level (horizontal tangent)
                bulge_end_x,
                y, // end at top edge
            );

            // Line to top edge end
            path = path.line_to(top_end_x, y);
        } else {
            // Without corner radius: smooth arc with horizontal tangents at edges
            // This creates a curve that smoothly wraps around circular content
            let rx = bulge.width / 2.0;

            // Line to bulge start
            path = path.line_to(bulge_start_x, y);

            // First half arc: from left edge to top center
            // Both control points positioned for horizontal tangents:
            // - At start: tangent is horizontal (going right toward center)
            // - At end: tangent is horizontal (at the peak of the curve)
            path = path.cubic_to(
                bulge_start_x + K * rx, // control 1 x: offset right (horizontal tangent at start)
                y,                      // control 1 y: same as start y
                center_x - K * rx,      // control 2 x: offset left from center
                bulge_top_y,            // control 2 y: same as end (horizontal tangent at peak)
                center_x,
                bulge_top_y, // end at top center
            );

            // Second half arc: from top center to right edge
            // Both control points positioned for horizontal tangents
            path = path.cubic_to(
                center_x + K * rx,    // control 1 x: offset right from center
                bulge_top_y,          // control 1 y: same as start (horizontal tangent at peak)
                bulge_end_x - K * rx, // control 2 x: offset left (horizontal tangent at end)
                y,                    // control 2 y: same as end y
                bulge_end_x,
                y, // end at right edge
            );

            // Line to top edge end
            path = path.line_to(top_end_x, y);
        }
    } else if let Some(cut) = top_center_cut {
        // Draw top center V-cut - sharp V going DOWNWARD into the shape
        let center_x = x + w / 2.0;
        let cut_start_x = center_x - cut.width / 2.0;
        let cut_end_x = center_x + cut.width / 2.0;
        let cut_point_y = y + cut.depth; // Point goes downward (into shape)

        // Line to cut start
        path = path.line_to(cut_start_x, y);

        // Line down to the V point
        path = path.line_to(center_x, cut_point_y);

        // Line up to cut end
        path = path.line_to(cut_end_x, y);

        // Line to top edge end
        path = path.line_to(top_end_x, y);
    } else if let Some(peak) = top_center_peak {
        // Draw top center V-peak - sharp V pointing UPWARD out of the shape
        let center_x = x + w / 2.0;
        let peak_start_x = center_x - peak.width / 2.0;
        let peak_end_x = center_x + peak.width / 2.0;
        let peak_point_y = y - peak.height; // Point goes upward (out of shape)

        // Line to peak start
        path = path.line_to(peak_start_x, y);

        // Line up to the V point
        path = path.line_to(center_x, peak_point_y);

        // Line down to peak end
        path = path.line_to(peak_end_x, y);

        // Line to top edge end
        path = path.line_to(top_end_x, y);
    } else {
        path = path.line_to(top_end_x, y);
    }

    // Top-right corner
    match tr.style {
        CornerStyle::Step => {
            path = path.line_to(x + w + tr_r, y);
            path = path.line_to(x + w + tr_r, y + tr_r);
            path = path.line_to(x + w, y + tr_r);
        }
        CornerStyle::Concave => {
            // Concave: smooth quarter-circle from extended top to right edge
            // Control at the outer corner for proper curve
            path = path.quad_to(x + w, y, x + w, y + tr_r);
        }
        CornerStyle::Convex if tr_r > 0.0 => {
            // Convex: standard rounded corner
            path = path.quad_to(x + w, y, x + w, y + tr_r);
        }
        _ => {}
    }

    // Right edge to bottom-right corner
    let right_end_y = match br.style {
        CornerStyle::Concave => y + h + br_r, // Extends down
        CornerStyle::Convex if br_r > 0.0 => y + h - br_r, // Inset
        _ => y + h,
    };
    path = path.line_to(x + w, right_end_y);

    // Bottom-right corner
    match br.style {
        CornerStyle::Step => {
            path = path.line_to(x + w, y + h + br_r);
            path = path.line_to(x + w - br_r, y + h + br_r);
            path = path.line_to(x + w - br_r, y + h);
        }
        CornerStyle::Concave => {
            // Concave: curve inward from extended position to bottom edge
            path = path.quad_to(x + w, y + h, x + w - br_r, y + h);
        }
        CornerStyle::Convex if br_r > 0.0 => {
            // Convex: standard rounded corner
            path = path.quad_to(x + w, y + h, x + w - br_r, y + h);
        }
        _ => {}
    }

    // Bottom edge - with optional center scoop
    let bottom_end_x = match bl.style {
        CornerStyle::Concave => x - bl_r,              // Extends left
        CornerStyle::Convex if bl_r > 0.0 => x + bl_r, // Inset
        _ => x,
    };

    // Draw bottom center scoop if present
    // Path is going from right to left on bottom edge
    // The scoop curves UP into the shape, creating a visible indent
    if let Some(scoop) = bottom_center_scoop {
        let center_x = x + w / 2.0;
        let scoop_start_x = center_x + scoop.width / 2.0; // Start from right side (path goes right to left)
        let scoop_end_x = center_x - scoop.width / 2.0; // End at left side
        let scoop_top_y = y + h - scoop.depth;

        // Corner radius for smooth entry/exit transitions (clamped to 1/4 of scoop width)
        let cr = scoop.corner_radius.min(scoop.width / 4.0).max(0.0);

        // Cubic bezier control point factor for circular arc approximation
        const K: f32 = 0.552_284_8;

        if cr > 0.0 {
            // With corner radius: concave (outward-bowing) entry/exit transitions
            // Note: path goes right to left on bottom edge
            let effective_start_x = scoop_start_x - cr; // Inset from right
            let effective_end_x = scoop_end_x + cr; // Inset from left
            let effective_start_y = y + h - cr;
            let effective_rx = (effective_start_x - effective_end_x) / 2.0;
            let effective_ry = scoop.depth - cr;

            // Line to entry corner point (from right)
            path = path.line_to(scoop_start_x, y + h);

            // Entry corner: CONCAVE ear - control toward CENTER makes fill bulge outward
            // (path goes right to left on bottom edge)
            path = path.quad_to(
                scoop_start_x - cr * 0.55,
                y + h, // control: toward center (left)
                effective_start_x,
                effective_start_y, // end at main scoop start
            );

            // Main scoop curve: from effective start to top center
            path = path.cubic_to(
                effective_start_x,
                effective_start_y - K * effective_ry, // control 1
                center_x + K * effective_rx,
                scoop_top_y, // control 2
                center_x,
                scoop_top_y, // end at top center
            );

            // Main scoop curve: from top center to effective end
            path = path.cubic_to(
                center_x - K * effective_rx,
                scoop_top_y, // control 1
                effective_end_x,
                effective_start_y - K * effective_ry, // control 2
                effective_end_x,
                effective_start_y, // end at effective end
            );

            // Exit corner: CONCAVE ear - control toward CENTER makes fill bulge outward
            path = path.quad_to(
                scoop_end_x + cr * 0.55,
                y + h, // control: toward center (right)
                scoop_end_x,
                y + h, // end at bottom edge
            );

            // Line to bottom edge end
            path = path.line_to(bottom_end_x, y + h);
        } else {
            // Without corner radius: sharp transitions (original behavior)
            let rx = scoop.width / 2.0;
            let ry = scoop.depth;

            // Line to scoop start (from right)
            path = path.line_to(scoop_start_x, y + h);

            // First quarter arc: from right edge to top center (going right to left)
            path = path.cubic_to(
                scoop_start_x,
                y + h - K * ry, // control 1
                center_x + K * rx,
                scoop_top_y, // control 2
                center_x,
                scoop_top_y, // end at top center
            );

            // Second quarter arc: from top center to left edge
            path = path.cubic_to(
                center_x - K * rx,
                scoop_top_y, // control 1
                scoop_end_x,
                y + h - K * ry, // control 2
                scoop_end_x,
                y + h, // end at left edge
            );

            // Line to bottom edge end
            path = path.line_to(bottom_end_x, y + h);
        }
    } else if let Some(bulge) = bottom_center_bulge {
        // Draw bottom center bulge - protrudes DOWNWARD out of the shape
        // Path is going from right to left on bottom edge
        let center_x = x + w / 2.0;
        let bulge_start_x = center_x + bulge.width / 2.0; // Start from right side
        let bulge_end_x = center_x - bulge.width / 2.0; // End at left side
        let bulge_bottom_y = y + h + bulge.height; // Protrudes downward (positive y)

        // Corner radius for smooth entry/exit transitions
        let cr = bulge.corner_radius.min(bulge.width / 4.0).max(0.0);

        const K: f32 = 0.552_284_8;

        if cr > 0.0 {
            // With corner radius: smooth convex entry/exit transitions
            // Control points placed at the "corner" for smooth quarter-circle curves
            let effective_start_x = bulge_start_x - cr; // Inset from right
            let effective_end_x = bulge_end_x + cr; // Inset from left
            let effective_start_y = y + h + cr; // Going downward
            let effective_rx = (effective_start_x - effective_end_x) / 2.0;
            let effective_ry = bulge.height - cr;

            // Line to entry corner point (from right)
            path = path.line_to(bulge_start_x, y + h);

            // Entry corner: smooth convex quarter-circle transition going downward
            // Control at corner where horizontal tangent from start meets vertical tangent to end
            path = path.quad_to(
                effective_start_x, // control x: at the corner (left of start, path goes right-to-left)
                y + h,             // control y: at edge level (horizontal tangent)
                effective_start_x,
                effective_start_y, // end at main bulge start
            );

            // Main bulge curve: from effective start to bottom center
            path = path.cubic_to(
                effective_start_x,
                effective_start_y + K * effective_ry, // control 1
                center_x + K * effective_rx,
                bulge_bottom_y, // control 2
                center_x,
                bulge_bottom_y, // end at bottom center
            );

            // Main bulge curve: from bottom center to effective end
            path = path.cubic_to(
                center_x - K * effective_rx,
                bulge_bottom_y, // control 1
                effective_end_x,
                effective_start_y + K * effective_ry, // control 2
                effective_end_x,
                effective_start_y, // end at effective end
            );

            // Exit corner: smooth convex quarter-circle transition back to edge
            // Control at corner where vertical tangent from start meets horizontal tangent to end
            path = path.quad_to(
                effective_end_x, // control x: at the corner (right of end)
                y + h,           // control y: at edge level (horizontal tangent)
                bulge_end_x,
                y + h, // end at bottom edge
            );

            // Line to bottom edge end
            path = path.line_to(bottom_end_x, y + h);
        } else {
            // Without corner radius: smooth arc with horizontal tangents at edges
            // Path goes right-to-left on bottom edge
            let rx = bulge.width / 2.0;

            // Line to bulge start (from right)
            path = path.line_to(bulge_start_x, y + h);

            // First half arc: from right edge to bottom center
            // Both control points positioned for horizontal tangents
            path = path.cubic_to(
                bulge_start_x - K * rx, // control 1 x: offset left (horizontal tangent at start)
                y + h,                  // control 1 y: same as start y
                center_x + K * rx,      // control 2 x: offset right from center
                bulge_bottom_y,         // control 2 y: same as end (horizontal tangent at peak)
                center_x,
                bulge_bottom_y, // end at bottom center
            );

            // Second half arc: from bottom center to left edge
            // Both control points positioned for horizontal tangents
            path = path.cubic_to(
                center_x - K * rx,    // control 1 x: offset left from center
                bulge_bottom_y,       // control 1 y: same as start (horizontal tangent at peak)
                bulge_end_x + K * rx, // control 2 x: offset right (horizontal tangent at end)
                y + h,                // control 2 y: same as end y
                bulge_end_x,
                y + h, // end at left edge
            );

            // Line to bottom edge end
            path = path.line_to(bottom_end_x, y + h);
        }
    } else if let Some(cut) = bottom_center_cut {
        // Draw bottom center V-cut - sharp V going UPWARD into the shape
        // Path goes right-to-left on bottom edge
        let center_x = x + w / 2.0;
        let cut_start_x = center_x + cut.width / 2.0; // Start from right
        let cut_end_x = center_x - cut.width / 2.0; // End at left
        let cut_point_y = y + h - cut.depth; // Point goes upward (into shape)

        // Line to cut start (from right)
        path = path.line_to(cut_start_x, y + h);

        // Line up to the V point
        path = path.line_to(center_x, cut_point_y);

        // Line down to cut end
        path = path.line_to(cut_end_x, y + h);

        // Line to bottom edge end
        path = path.line_to(bottom_end_x, y + h);
    } else if let Some(peak) = bottom_center_peak {
        // Draw bottom center V-peak - sharp V pointing DOWNWARD out of the shape
        // Path goes right-to-left on bottom edge
        let center_x = x + w / 2.0;
        let peak_start_x = center_x + peak.width / 2.0; // Start from right
        let peak_end_x = center_x - peak.width / 2.0; // End at left
        let peak_point_y = y + h + peak.height; // Point goes downward (out of shape)

        // Line to peak start (from right)
        path = path.line_to(peak_start_x, y + h);

        // Line down to the V point
        path = path.line_to(center_x, peak_point_y);

        // Line up to peak end
        path = path.line_to(peak_end_x, y + h);

        // Line to bottom edge end
        path = path.line_to(bottom_end_x, y + h);
    } else {
        path = path.line_to(bottom_end_x, y + h);
    }

    // Bottom-left corner
    match bl.style {
        CornerStyle::Step => {
            path = path.line_to(x - bl_r, y + h);
            path = path.line_to(x - bl_r, y + h - bl_r);
            path = path.line_to(x, y + h - bl_r);
        }
        CornerStyle::Concave => {
            // Concave: curve inward from extended position to left edge
            path = path.quad_to(x, y + h, x, y + h - bl_r);
        }
        CornerStyle::Convex if bl_r > 0.0 => {
            // Convex: standard rounded corner
            path = path.quad_to(x, y + h, x, y + h - bl_r);
        }
        _ => {}
    }

    // Left edge back to top-left corner
    let left_end_y = match tl.style {
        CornerStyle::Concave => y + tl_r, // Stop before top, curve will bow outward
        CornerStyle::Convex if tl_r > 0.0 => y + tl_r, // Inset
        _ => y,
    };
    path = path.line_to(x, left_end_y);

    // Top-left corner (completes the shape)
    match tl.style {
        CornerStyle::Step => {
            path = path.line_to(x, y - tl_r);
            path = path.line_to(x - tl_r, y - tl_r);
            path = path.line_to(x - tl_r, y);
        }
        CornerStyle::Concave => {
            // Concave: smooth quarter-circle from left edge to extended top
            // Control at the inner corner for proper curve
            path = path.quad_to(x, y, top_start_x, y);
        }
        CornerStyle::Convex if tl_r > 0.0 => {
            // Convex: standard rounded corner back to start
            path = path.quad_to(x, y, top_start_x, y);
        }
        _ => {}
    }

    path.close()
}

// =============================================================================
// Path Shadow Utilities
// =============================================================================

/// Offset a path outward by a given amount.
///
/// This creates an expanded version of the path for shadow rendering.
/// The offset is applied by scaling the path from its center.
fn offset_path_by(path: &Path, amount: f32, bounds: Rect) -> Path {
    use blinc_core::PathCommand;

    if amount <= 0.0 {
        return path.clone();
    }

    // Calculate the center of the bounds
    let cx = bounds.x() + bounds.width() / 2.0;
    let cy = bounds.y() + bounds.height() / 2.0;

    // Calculate scale factor based on offset amount
    let scale_x = if bounds.width() > 0.0 {
        (bounds.width() + amount * 2.0) / bounds.width()
    } else {
        1.0
    };
    let scale_y = if bounds.height() > 0.0 {
        (bounds.height() + amount * 2.0) / bounds.height()
    } else {
        1.0
    };

    // Transform each point by scaling from center
    let transform_point = |x: f32, y: f32| -> (f32, f32) {
        let new_x = cx + (x - cx) * scale_x;
        let new_y = cy + (y - cy) * scale_y;
        (new_x, new_y)
    };

    // Build new path with transformed commands
    let mut new_path = Path::new();
    for cmd in path.commands() {
        match cmd {
            PathCommand::MoveTo(p) => {
                let (x, y) = transform_point(p.x, p.y);
                new_path = new_path.move_to(x, y);
            }
            PathCommand::LineTo(p) => {
                let (x, y) = transform_point(p.x, p.y);
                new_path = new_path.line_to(x, y);
            }
            PathCommand::QuadTo { control, end } => {
                let (cx, cy) = transform_point(control.x, control.y);
                let (ex, ey) = transform_point(end.x, end.y);
                new_path = new_path.quad_to(cx, cy, ex, ey);
            }
            PathCommand::CubicTo {
                control1,
                control2,
                end,
            } => {
                let (c1x, c1y) = transform_point(control1.x, control1.y);
                let (c2x, c2y) = transform_point(control2.x, control2.y);
                let (ex, ey) = transform_point(end.x, end.y);
                new_path = new_path.cubic_to(c1x, c1y, c2x, c2y, ex, ey);
            }
            PathCommand::ArcTo {
                radii,
                rotation,
                large_arc,
                sweep,
                end,
            } => {
                let (ex, ey) = transform_point(end.x, end.y);
                // Scale radii proportionally
                let new_radii = blinc_core::Vec2::new(radii.x * scale_x, radii.y * scale_y);
                new_path = new_path.arc_to(new_radii, *rotation, *large_arc, *sweep, ex, ey);
            }
            PathCommand::Close => {
                new_path = new_path.close();
            }
        }
    }

    new_path
}

/// Draw a path-based shadow using multi-layer blur approximation.
///
/// This renders multiple offset versions of the path with decreasing opacity
/// to simulate a soft shadow effect.
fn draw_path_shadow(ctx: &mut dyn DrawContext, path: &Path, bounds: Rect, shadow: &Shadow) {
    let blur = shadow.blur;
    if blur <= 0.0 {
        return;
    }

    // More layers = smoother shadow (aim for ~2px per layer)
    let layers = ((blur / 1.5).ceil() as usize).clamp(8, 24);

    // Apply shadow offset by translating
    let offset_x = shadow.offset_x;
    let offset_y = shadow.offset_y;

    // Draw layers from outermost to innermost
    for i in (0..layers).rev() {
        let t = (i as f32 + 0.5) / layers as f32;
        let layer_offset = (blur + shadow.spread) * t;

        // Gaussian-like alpha falloff: exp(-t^2 * k)
        // This creates a smoother, more natural shadow gradient
        let gaussian = (-t * t * 3.0).exp();
        let alpha = shadow.color.a * gaussian * 0.15;
        if alpha < 0.005 {
            continue;
        }

        // Create offset path
        let offset_path = offset_path_by(path, layer_offset, bounds);

        // Apply shadow offset via transform
        ctx.push_transform(Transform::translate(offset_x, offset_y));

        // Draw the shadow layer
        let shadow_color = Color::rgba(shadow.color.r, shadow.color.g, shadow.color.b, alpha);
        ctx.fill_path(&offset_path, Brush::Solid(shadow_color));

        ctx.pop_transform();
    }
}

// =============================================================================
// Notch Render Data
// =============================================================================

/// Data for rendering a shape element
#[derive(Clone)]
pub struct NotchRenderData {
    pub corners: CornersConfig,
    pub background: Option<Brush>,
    pub border_color: Option<Color>,
    pub border_width: f32,
    /// Drop shadow stack
    pub shadow: Vec<Shadow>,
}

impl std::fmt::Debug for NotchRenderData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NotchRenderData")
            .field("has_concave_curves", &self.corners.has_concave_curves())
            .finish()
    }
}

// =============================================================================
// ElementBuilder Implementation
// =============================================================================

impl Notch {
    /// Apply the same style adjustments `build()` installs on taffy:
    /// reserve scoop_depth of padding on whichever edge has a center
    /// scoop, so child layout doesn't render under the carved-out
    /// region. Pure function — shared by [`Notch::build`] and
    /// [`<Notch as ElementBuilder>::effective_layout_style`] so the
    /// fast layout-prop rebuild path reflows with the same style
    /// taffy got at original build time.
    fn apply_scoop_padding(&self, style: &mut Style) {
        if let Some(scoop) = &self.top_center_scoop {
            let current_top = match style.padding.top.expand() {
                taffy::style::ExpandedLengthPercentage::Length(v) => v,
                _ => 0.0,
            };
            style.padding.top = LengthPercentage::length(current_top + scoop.depth);
        }
        if let Some(scoop) = &self.bottom_center_scoop {
            let current_bottom = match style.padding.bottom.expand() {
                taffy::style::ExpandedLengthPercentage::Length(v) => v,
                _ => 0.0,
            };
            style.padding.bottom = LengthPercentage::length(current_bottom + scoop.depth);
        }
        // NOTE: concave corner padding is intentionally NOT auto-added
        // here. Existing callers (e.g. the notch_demo dropdown) already
        // set explicit `.pt(top_radius + …)` padding on notches with
        // concave top corners, and auto-adding would double up the
        // inset. Callers that want the inner-body inset reserved for
        // children should set padding manually (or wrap their children
        // in a div sized to the inner body).
    }
}

impl ElementBuilder for Notch {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        let mut style = self.style.clone();
        self.apply_scoop_padding(&mut style);

        let node = tree.create_node(style);

        // Build and add children
        for child in &self.children {
            let child_node = child.build(tree);
            tree.add_child(node, child_node);
        }

        node
    }

    fn effective_layout_style(&self) -> Option<Style> {
        let mut style = self.style.clone();
        self.apply_scoop_padding(&mut style);
        Some(style)
    }

    fn render_props(&self) -> RenderProps {
        // If we have concave curves, step corners, center scoops, bulges, cuts, or peaks,
        // we need custom canvas rendering. Otherwise use standard div rendering.
        let has_center_scoop =
            self.top_center_scoop.is_some() || self.bottom_center_scoop.is_some();
        let has_center_bulge =
            self.top_center_bulge.is_some() || self.bottom_center_bulge.is_some();
        let has_center_cut = self.top_center_cut.is_some() || self.bottom_center_cut.is_some();
        let has_center_peak = self.top_center_peak.is_some() || self.bottom_center_peak.is_some();
        let needs_custom = self.corners.needs_custom_rendering()
            || has_center_scoop
            || has_center_bulge
            || has_center_cut
            || has_center_peak;

        if needs_custom {
            // Shape fill AND drop shadow are both rendered via the
            // canvas path (`fill_notch`). The shadow is carried on
            // the PRIM_NOTCH primitive itself so the shader can trace
            // the notch's actual outer outline via `sd_notch` — a
            // standard `draw_shadow` would use a rect SDF and miss
            // the concave arcs / modifiers.
            //
            // The renderer's canvas clip is opt-in (only when the
            // element sets `overflow_clip`), so the shadow's blur
            // expansion can render past the element's layout box.
            RenderProps {
                background: None, // Rendered via canvas
                border_radius: CornerRadius::ZERO,
                border_color: None,
                border_width: 0.0,
                border_sides: Default::default(),
                layer: self.render_layer,
                material: self.material.clone(),
                shadow: Vec::new(), // Rendered via canvas
                transform: None,
                opacity: self.opacity,
                pointer_events_none: self.pointer_events_none,
                ..Default::default()
            }
        } else {
            // Standard rendering path - use div-like rendering
            RenderProps {
                background: self.background.clone(),
                border_radius: self.corners.to_corner_radius(),
                border_color: self.border_color,
                border_width: self.border_width,
                border_sides: Default::default(),
                layer: self.render_layer,
                material: self.material.clone(),
                shadow: self.shadow.clone(),
                transform: None,
                opacity: self.opacity,
                pointer_events_none: self.pointer_events_none,
                ..Default::default()
            }
        }
    }

    fn is_static_canvas(&self) -> bool {
        // The notch's canvas closure captures all shape parameters
        // (corner radii, scoop / bulge / cut / peak configs, brush,
        // shadow, opacity) at construction. The only runtime input
        // is `bounds.width / height` from layout, and any layout
        // change forces a full repaint that refreshes the static
        // cache. So the canvas overlay's per-frame re-invocation of
        // `render_fn` produces the same output as the static-cache
        // entry — except it overdraws any children the walker
        // layered on top. Opting out of the overlay keeps the
        // children visible and lets the compositor fast-path engage
        // on frames where the only canvases are notches.
        true
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        &self.children
    }

    fn element_type_id(&self) -> ElementTypeId {
        // If we have custom corners, center scoops, bulges, cuts, or peaks, use Canvas type
        // Otherwise, use Div for standard rendering
        let has_center_scoop =
            self.top_center_scoop.is_some() || self.bottom_center_scoop.is_some();
        let has_center_bulge =
            self.top_center_bulge.is_some() || self.bottom_center_bulge.is_some();
        let has_center_cut = self.top_center_cut.is_some() || self.bottom_center_cut.is_some();
        let has_center_peak = self.top_center_peak.is_some() || self.bottom_center_peak.is_some();
        let needs_custom = self.corners.needs_custom_rendering()
            || has_center_scoop
            || has_center_bulge
            || has_center_cut
            || has_center_peak;
        if needs_custom {
            ElementTypeId::Canvas
        } else {
            ElementTypeId::Div
        }
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        Some(&self.style)
    }

    fn canvas_render_info(&self) -> Option<CanvasRenderFn> {
        // Only provide canvas render if we have custom corners, scoops, bulges, cuts, or peaks
        let has_center_scoop =
            self.top_center_scoop.is_some() || self.bottom_center_scoop.is_some();
        let has_center_bulge =
            self.top_center_bulge.is_some() || self.bottom_center_bulge.is_some();
        let has_center_cut = self.top_center_cut.is_some() || self.bottom_center_cut.is_some();
        let has_center_peak = self.top_center_peak.is_some() || self.bottom_center_peak.is_some();
        let needs_custom = self.corners.needs_custom_rendering()
            || has_center_scoop
            || has_center_bulge
            || has_center_cut
            || has_center_peak;
        if !needs_custom {
            return None;
        }

        let corners = self.corners;
        let top_center_scoop = self.top_center_scoop;
        let bottom_center_scoop = self.bottom_center_scoop;
        let top_center_bulge = self.top_center_bulge;
        let bottom_center_bulge = self.bottom_center_bulge;
        let top_center_cut = self.top_center_cut;
        let bottom_center_cut = self.bottom_center_cut;
        let top_center_peak = self.top_center_peak;
        let bottom_center_peak = self.bottom_center_peak;
        let background = self.background.clone();
        let border_color = self.border_color;
        let border_width = self.border_width;
        // The notch canvas path emits a single fill_notch call. Compound
        // shadow stacks degrade to the topmost layer here; full multi-layer
        // support would need fill_notch to grow a per-shadow iteration.
        let shadow = self.shadow.first().copied();
        let opacity = self.opacity;

        Some(Rc::new(
            move |ctx: &mut dyn DrawContext, bounds: CanvasBounds| {
                // Every notch variant — concave corners, bulge, peak, cut,
                // scoop, plus step corners (treated as sharp until the SDF
                // gains a dedicated step case) — runs entirely through the
                // SDF pipeline via `ctx.fill_notch`. No CPU tessellation,
                // no separate vertex buffer, no layer-composition detour:
                // the fragment shader's PRIM_NOTCH branch evaluates every
                // shape via unions and subtractions of existing 2D SDFs.
                let corner_types_arr = [
                    if corners.top_left.is_concave() {
                        1.0
                    } else {
                        0.0
                    },
                    if corners.top_right.is_concave() {
                        1.0
                    } else {
                        0.0
                    },
                    if corners.bottom_right.is_concave() {
                        1.0
                    } else {
                        0.0
                    },
                    if corners.bottom_left.is_concave() {
                        1.0
                    } else {
                        0.0
                    },
                ];
                let corner_radii_arr = [
                    corners.top_left.radius,
                    corners.top_right.radius,
                    corners.bottom_right.radius,
                    corners.bottom_left.radius,
                ];

                // Pack top/bottom edge modifiers. Each slot carries
                // (type, width, height_or_depth, corner_radius).
                //
                // Modifier precedence (top edge): scoop > bulge > cut > peak.
                // Only one modifier applies per edge — matches the `if ...
                // else if ...` chain in `build_shape_path` so mixed-config
                // notches behave identically to the legacy renderer.
                let top_mod = if let Some(s) = &top_center_scoop {
                    [1.0, s.width, s.depth, s.corner_radius]
                } else if let Some(b) = &top_center_bulge {
                    [2.0, b.width, b.height, b.corner_radius]
                } else if let Some(c) = &top_center_cut {
                    [3.0, c.width, c.depth, 0.0]
                } else if let Some(p) = &top_center_peak {
                    [4.0, p.width, p.height, 0.0]
                } else {
                    [0.0; 4]
                };
                let bottom_mod = if let Some(s) = &bottom_center_scoop {
                    [1.0, s.width, s.depth, s.corner_radius]
                } else if let Some(b) = &bottom_center_bulge {
                    [2.0, b.width, b.height, b.corner_radius]
                } else if let Some(c) = &bottom_center_cut {
                    [3.0, c.width, c.depth, 0.0]
                } else if let Some(p) = &bottom_center_peak {
                    [4.0, p.width, p.height, 0.0]
                } else {
                    [0.0; 4]
                };

                let outer_rect = Rect::new(0.0, 0.0, bounds.width, bounds.height);

                let fill_brush = if opacity < 1.0 {
                    background.clone().map(|b| apply_brush_opacity(b, opacity))
                } else {
                    background.clone()
                };

                let border_arg = if let (Some(c), w) = (border_color, border_width) {
                    if w > 0.0 {
                        let stroked_color = if opacity < 1.0 {
                            c.with_alpha(c.a * opacity)
                        } else {
                            c
                        };
                        Some((w, stroked_color))
                    } else {
                        None
                    }
                } else {
                    None
                };

                // Always emit via the SDF pipeline. Border-only notches go
                // out as a fully transparent fill so the fragment shader
                // still evaluates the shape for the stroke.
                let brush = fill_brush.unwrap_or(Brush::Solid(Color::rgba(0.0, 0.0, 0.0, 0.0)));
                ctx.fill_notch(
                    outer_rect,
                    corner_radii_arr,
                    corner_types_arr,
                    top_mod,
                    bottom_mod,
                    border_arg,
                    shadow,
                    brush,
                );
            },
        ))
    }

    fn event_handlers(&self) -> Option<&EventHandlers> {
        if !self.event_handlers.is_empty() {
            Some(&self.event_handlers)
        } else {
            None
        }
    }
}

// =============================================================================
// Factory Function
// =============================================================================

/// Create a new notch element
///
/// Use `notch()` when you need concave (outward-bowing) curves
/// that `div()` cannot do.
///
/// # Example
///
/// ```ignore
/// use blinc_layout::prelude::*;
///
/// // Menu bar dropdown with notched connection
/// notch()
///     .concave_top(24.0)    // The notch!
///     .rounded_bottom(16.0) // Standard rounding
///     .bg(Color::BLACK)
///     .p(16.0)
///     .child(text("Battery | 87% Charged"))
/// ```
///
/// # Animated Morphing
///
/// Use signed radius for smooth animation between concave and convex:
///
/// ```ignore
/// stateful(|ctx| {
///     // Negative = concave, positive = convex
///     let top_r = ctx.spring("top", if open { -24.0 } else { 16.0 });
///     
///     notch()
///         .corner_top(top_r)   // Animates through 0 (sharp)
///         .corner_bottom(16.0)
///         .bg(Color::BLACK)
/// })
/// ```
pub fn notch() -> Notch {
    Notch::new()
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notch_no_concave_curves() {
        let s = notch().rounded(8.0);
        assert!(!s.corners.has_concave_curves());
    }

    #[test]
    fn test_notch_with_concave_curves() {
        let s = notch().concave_top(24.0);
        assert!(s.corners.has_concave_curves());
        assert!(s.corners.top_left.is_concave());
        assert!(s.corners.top_right.is_concave());
        assert!(!s.corners.bottom_left.is_concave());
        assert!(!s.corners.bottom_right.is_concave());
    }

    #[test]
    fn test_notched_dropdown() {
        let s = notch().concave_top(24.0).rounded_bottom(16.0);

        assert!(s.corners.top_left.is_concave());
        assert_eq!(s.corners.top_left.radius, 24.0);
        assert!(s.corners.top_right.is_concave());
        assert_eq!(s.corners.top_right.radius, 24.0);

        assert!(!s.corners.bottom_left.is_concave());
        assert_eq!(s.corners.bottom_left.radius, 16.0);
        assert!(!s.corners.bottom_right.is_concave());
        assert_eq!(s.corners.bottom_right.radius, 16.0);
    }

    #[test]
    fn test_element_type_changes() {
        // Without custom corners -> Div
        let s1 = notch().rounded(8.0);
        assert_eq!(s1.element_type_id(), ElementTypeId::Div);

        // With concave curves -> Canvas
        let s2 = notch().concave_top(24.0);
        assert_eq!(s2.element_type_id(), ElementTypeId::Canvas);

        // With step corners -> Canvas
        let s3 = notch().step_top(20.0);
        assert_eq!(s3.element_type_id(), ElementTypeId::Canvas);
    }

    #[test]
    fn test_signed_radius_positive() {
        // Positive = convex (standard rounding)
        let s = notch().corner_top(16.0);
        assert!(!s.corners.top_left.is_concave());
        assert!(!s.corners.top_right.is_concave());
        assert_eq!(s.corners.top_left.radius, 16.0);
        assert_eq!(s.corners.top_right.radius, 16.0);
    }

    #[test]
    fn test_signed_radius_negative() {
        // Negative = concave
        let s = notch().corner_top(-24.0);
        assert!(s.corners.top_left.is_concave());
        assert!(s.corners.top_right.is_concave());
        assert_eq!(s.corners.top_left.radius, 24.0); // Stored as positive
        assert_eq!(s.corners.top_right.radius, 24.0);
    }

    #[test]
    fn test_signed_radius_zero() {
        // Zero = sharp corner
        let s = notch().corner_top(0.0);
        assert!(!s.corners.top_left.is_concave());
        assert_eq!(s.corners.top_left.radius, 0.0);
    }

    #[test]
    fn test_signed_radius_mixed() {
        // Typical dropdown: concave top, convex bottom
        let s = notch().corner_top(-24.0).corner_bottom(16.0);

        assert!(s.corners.top_left.is_concave());
        assert!(s.corners.top_right.is_concave());
        assert!(!s.corners.bottom_left.is_concave());
        assert!(!s.corners.bottom_right.is_concave());

        assert_eq!(s.corners.top_left.radius, 24.0);
        assert_eq!(s.corners.bottom_left.radius, 16.0);
    }

    #[test]
    fn test_signed_radius_individual_corners() {
        let s = notch()
            .corner_tl(-10.0)
            .corner_tr(20.0)
            .corner_br(-30.0)
            .corner_bl(40.0);

        assert!(s.corners.top_left.is_concave());
        assert_eq!(s.corners.top_left.radius, 10.0);

        assert!(!s.corners.top_right.is_concave());
        assert_eq!(s.corners.top_right.radius, 20.0);

        assert!(s.corners.bottom_right.is_concave());
        assert_eq!(s.corners.bottom_right.radius, 30.0);

        assert!(!s.corners.bottom_left.is_concave());
        assert_eq!(s.corners.bottom_left.radius, 40.0);
    }

    #[test]
    fn test_step_corners() {
        let s = notch().step_top(20.0);
        assert!(s.corners.top_left.is_step());
        assert!(s.corners.top_right.is_step());
        assert!(!s.corners.bottom_left.is_step());
        assert!(!s.corners.bottom_right.is_step());
        assert_eq!(s.corners.top_left.radius, 20.0);
    }

    #[test]
    fn test_step_with_rounded_bottom() {
        // Sharp step at top, rounded at bottom
        let s = notch().step_top(24.0).rounded_bottom(12.0);

        assert!(s.corners.top_left.is_step());
        assert!(s.corners.top_right.is_step());
        assert!(!s.corners.bottom_left.is_step());
        assert!(!s.corners.bottom_right.is_step());

        assert_eq!(s.corners.top_left.radius, 24.0);
        assert_eq!(s.corners.bottom_left.radius, 12.0);
    }

    #[test]
    fn test_needs_custom_rendering() {
        // Convex only -> no custom rendering
        let s1 = notch().rounded(8.0);
        assert!(!s1.corners.needs_custom_rendering());

        // Concave -> custom rendering
        let s2 = notch().concave_top(24.0);
        assert!(s2.corners.needs_custom_rendering());

        // Step -> custom rendering
        let s3 = notch().step_top(20.0);
        assert!(s3.corners.needs_custom_rendering());
    }
}
