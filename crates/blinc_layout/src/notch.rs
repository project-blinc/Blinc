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

use blinc_core::{Brush, Color, CornerRadius, DrawContext, Gradient, Path, Rect, Shadow};
use taffy::{prelude::*, Overflow};

use crate::canvas::{CanvasBounds, CanvasRenderFn};
use crate::div::{ElementBuilder, ElementTypeId};
use crate::element::{Material, RenderLayer, RenderProps};
use crate::event_handler::EventHandlers;
use crate::tree::{LayoutNodeId, LayoutTree};
use crate::Div;

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

    // Layout
    pub(crate) style: Style,
    pub(crate) children: Vec<Box<dyn ElementBuilder>>,

    // Visual
    pub(crate) background: Option<Brush>,
    pub(crate) border_color: Option<Color>,
    pub(crate) border_width: f32,
    pub(crate) shadow: Option<Shadow>,
    pub(crate) material: Option<Material>,
    pub(crate) opacity: f32,
    pub(crate) render_layer: RenderLayer,

    // Interaction
    pub(crate) event_handlers: EventHandlers,
    pub(crate) element_id: Option<String>,

    pub inner: Div,
}

impl Notch {
    /// Create a new notch element
    pub fn new() -> Self {
        Self {
            corners: CornersConfig::NONE,
            top_center_scoop: None,
            bottom_center_scoop: None,
            style: Style::default(),
            children: Vec::new(),
            background: None,
            border_color: None,
            border_width: 0.0,
            shadow: None,
            material: None,
            opacity: 1.0,
            render_layer: RenderLayer::default(),
            event_handlers: EventHandlers::default(),
            element_id: None,
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
        self.top_center_scoop = Some(CenterScoop { width, depth });
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
        self.bottom_center_scoop = Some(CenterScoop { width, depth });
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
        self.style.size.width = Dimension::Length(width);
        self
    }

    /// Set fixed height
    pub fn h(mut self, height: f32) -> Self {
        self.style.size.height = Dimension::Length(height);
        self
    }

    /// Set both width and height to the same value
    pub fn size(mut self, size: f32) -> Self {
        self.style.size.width = Dimension::Length(size);
        self.style.size.height = Dimension::Length(size);
        self
    }

    /// Set width to 100%
    pub fn w_full(mut self) -> Self {
        self.style.size.width = Dimension::Percent(1.0);
        self
    }

    /// Set height to 100%
    pub fn h_full(mut self) -> Self {
        self.style.size.height = Dimension::Percent(1.0);
        self
    }

    /// Set width to fit content
    pub fn w_fit(mut self) -> Self {
        self.style.size.width = Dimension::Auto;
        self
    }

    /// Set height to fit content
    pub fn h_fit(mut self) -> Self {
        self.style.size.height = Dimension::Auto;
        self
    }

    /// Set minimum width
    pub fn min_w(mut self, width: f32) -> Self {
        self.style.min_size.width = Dimension::Length(width);
        self
    }

    /// Set minimum height
    pub fn min_h(mut self, height: f32) -> Self {
        self.style.min_size.height = Dimension::Length(height);
        self
    }

    /// Set maximum width
    pub fn max_w(mut self, width: f32) -> Self {
        self.style.max_size.width = Dimension::Length(width);
        self
    }

    /// Set maximum height
    pub fn max_h(mut self, height: f32) -> Self {
        self.style.max_size.height = Dimension::Length(height);
        self
    }

    // =========================================================================
    // Padding
    // =========================================================================

    /// Set uniform padding on all sides
    pub fn p(mut self, padding: f32) -> Self {
        self.style.padding = taffy::Rect {
            left: LengthPercentage::Length(padding),
            right: LengthPercentage::Length(padding),
            top: LengthPercentage::Length(padding),
            bottom: LengthPercentage::Length(padding),
        };
        self
    }

    /// Set horizontal padding (left and right)
    pub fn px(mut self, padding: f32) -> Self {
        self.style.padding.left = LengthPercentage::Length(padding);
        self.style.padding.right = LengthPercentage::Length(padding);
        self
    }

    /// Set vertical padding (top and bottom)
    pub fn py(mut self, padding: f32) -> Self {
        self.style.padding.top = LengthPercentage::Length(padding);
        self.style.padding.bottom = LengthPercentage::Length(padding);
        self
    }

    /// Set top padding
    pub fn pt(mut self, padding: f32) -> Self {
        self.style.padding.top = LengthPercentage::Length(padding);
        self
    }

    /// Set bottom padding
    pub fn pb(mut self, padding: f32) -> Self {
        self.style.padding.bottom = LengthPercentage::Length(padding);
        self
    }

    /// Set left padding
    pub fn pl(mut self, padding: f32) -> Self {
        self.style.padding.left = LengthPercentage::Length(padding);
        self
    }

    /// Set right padding
    pub fn pr(mut self, padding: f32) -> Self {
        self.style.padding.right = LengthPercentage::Length(padding);
        self
    }

    // =========================================================================
    // Margin
    // =========================================================================

    /// Set uniform margin on all sides
    pub fn m(mut self, margin: f32) -> Self {
        self.style.margin = taffy::Rect {
            left: LengthPercentageAuto::Length(margin),
            right: LengthPercentageAuto::Length(margin),
            top: LengthPercentageAuto::Length(margin),
            bottom: LengthPercentageAuto::Length(margin),
        };
        self
    }

    /// Set horizontal margin (left and right)
    pub fn mx(mut self, margin: f32) -> Self {
        self.style.margin.left = LengthPercentageAuto::Length(margin);
        self.style.margin.right = LengthPercentageAuto::Length(margin);
        self
    }

    /// Set vertical margin (top and bottom)
    pub fn my(mut self, margin: f32) -> Self {
        self.style.margin.top = LengthPercentageAuto::Length(margin);
        self.style.margin.bottom = LengthPercentageAuto::Length(margin);
        self
    }

    /// Center horizontally with auto margins
    pub fn mx_auto(mut self) -> Self {
        self.style.margin.left = LengthPercentageAuto::Auto;
        self.style.margin.right = LengthPercentageAuto::Auto;
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
            width: LengthPercentage::Length(gap),
            height: LengthPercentage::Length(gap),
        };
        self
    }

    /// Center children both horizontally and vertically
    pub fn flex_center(mut self) -> Self {
        self.style.display = Display::Flex;
        self.style.justify_content = Some(JustifyContent::Center);
        self.style.align_items = Some(AlignItems::Center);
        self
    }

    /// Justify content to start
    pub fn justify_start(mut self) -> Self {
        self.style.justify_content = Some(JustifyContent::Start);
        self
    }

    /// Justify content to center
    pub fn justify_center(mut self) -> Self {
        self.style.justify_content = Some(JustifyContent::Center);
        self
    }

    /// Justify content to end
    pub fn justify_end(mut self) -> Self {
        self.style.justify_content = Some(JustifyContent::End);
        self
    }

    /// Justify content with space between
    pub fn justify_between(mut self) -> Self {
        self.style.justify_content = Some(JustifyContent::SpaceBetween);
        self
    }

    /// Align items to start
    pub fn items_start(mut self) -> Self {
        self.style.align_items = Some(AlignItems::Start);
        self
    }

    /// Align items to center
    pub fn items_center(mut self) -> Self {
        self.style.align_items = Some(AlignItems::Center);
        self
    }

    /// Align items to end
    pub fn items_end(mut self) -> Self {
        self.style.align_items = Some(AlignItems::End);
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
        self.style.inset.top = LengthPercentageAuto::Length(offset);
        self
    }

    /// Set bottom offset
    pub fn bottom(mut self, offset: f32) -> Self {
        self.style.inset.bottom = LengthPercentageAuto::Length(offset);
        self
    }

    /// Set left offset
    pub fn left(mut self, offset: f32) -> Self {
        self.style.inset.left = LengthPercentageAuto::Length(offset);
        self
    }

    /// Set right offset
    pub fn right(mut self, offset: f32) -> Self {
        self.style.inset.right = LengthPercentageAuto::Length(offset);
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

    /// Add drop shadow
    pub fn shadow(mut self, shadow: Shadow) -> Self {
        self.shadow = Some(shadow);
        self
    }

    /// Add a small shadow
    pub fn shadow_sm(mut self) -> Self {
        self.shadow = Some(Shadow::new(0.0, 1.0, 3.0, Color::rgba(0.0, 0.0, 0.0, 0.1)));
        self
    }

    /// Add a medium shadow
    pub fn shadow_md(mut self) -> Self {
        self.shadow = Some(Shadow::new(0.0, 4.0, 6.0, Color::rgba(0.0, 0.0, 0.0, 0.1)));
        self
    }

    /// Add a large shadow
    pub fn shadow_lg(mut self) -> Self {
        self.shadow = Some(Shadow::new(
            0.0,
            10.0,
            15.0,
            Color::rgba(0.0, 0.0, 0.0, 0.1),
        ));
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
        if condition {
            f(self)
        } else {
            self
        }
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

/// Build the path for a shape with configurable corners and center scoops
///
/// This handles convex (standard rounding), concave curves, step notches,
/// and center scoops (Dynamic Island-style centered indentations).
/// The path is built clockwise starting from the top-left corner.
///
/// For convex corners: the curve stays inside the bounds
/// For concave corners: the curve bows outward, extending beyond the bounds
/// For center scoops: the curve bows inward, staying inside the bounds
fn build_shape_path(
    bounds: Rect,
    corners: &CornersConfig,
    top_center_scoop: Option<&CenterScoop>,
    bottom_center_scoop: Option<&CenterScoop>,
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

        // Cubic bezier control point factor for circular arc approximation
        // k = 4 * (sqrt(2) - 1) / 3 ≈ 0.5522847498
        const K: f32 = 0.5522847498;
        let rx = scoop.width / 2.0; // horizontal radius
        let ry = scoop.depth; // vertical radius

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
        let scoop_start_x = center_x + scoop.width / 2.0; // Start from right side
        let scoop_end_x = center_x - scoop.width / 2.0; // End at left side
        let scoop_top_y = y + h - scoop.depth;

        // Cubic bezier control point factor for circular arc approximation
        const K: f32 = 0.5522847498;
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
// Notch Render Data
// =============================================================================

/// Data for rendering a shape element
#[derive(Clone)]
pub struct NotchRenderData {
    pub corners: CornersConfig,
    pub background: Option<Brush>,
    pub border_color: Option<Color>,
    pub border_width: f32,
    pub shadow: Option<Shadow>,
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

impl ElementBuilder for Notch {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        // Clone style and adjust padding for center scoops
        // Scoops curve INWARD, so we need to reserve that space for children
        let mut style = self.style.clone();

        // Add top padding for top center scoop
        if let Some(scoop) = &self.top_center_scoop {
            let current_top = match style.padding.top {
                LengthPercentage::Length(v) => v,
                LengthPercentage::Percent(_) => 0.0, // Can't add to percentage
            };
            style.padding.top = LengthPercentage::Length(current_top + scoop.depth);
        }

        // Add bottom padding for bottom center scoop
        if let Some(scoop) = &self.bottom_center_scoop {
            let current_bottom = match style.padding.bottom {
                LengthPercentage::Length(v) => v,
                LengthPercentage::Percent(_) => 0.0, // Can't add to percentage
            };
            style.padding.bottom = LengthPercentage::Length(current_bottom + scoop.depth);
        }

        // Create the layout node with adjusted style
        let node = tree.create_node(style);

        // Build and add children
        for child in &self.children {
            let child_node = child.build(tree);
            tree.add_child(node, child_node);
        }

        node
    }

    fn render_props(&self) -> RenderProps {
        // If we have concave curves, step corners, OR center scoops, we need custom canvas rendering
        // Otherwise, we can use standard div rendering with CornerRadius
        let has_center_scoop =
            self.top_center_scoop.is_some() || self.bottom_center_scoop.is_some();
        let needs_custom = self.corners.needs_custom_rendering() || has_center_scoop;

        if needs_custom {
            // For shapes with custom corners/scoops, we'll set background to None here
            // and render via canvas. The canvas data is passed separately.
            RenderProps {
                background: None, // Rendered via canvas
                border_radius: CornerRadius::ZERO,
                border_color: None,
                border_width: 0.0,
                border_sides: Default::default(),
                layer: self.render_layer,
                material: self.material.clone(),
                shadow: None, // Rendered via canvas
                transform: None,
                opacity: self.opacity,
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
                shadow: self.shadow,
                transform: None,
                opacity: self.opacity,
                ..Default::default()
            }
        }
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        &self.children
    }

    fn element_type_id(&self) -> ElementTypeId {
        // If we have custom corners OR center scoops, use Canvas type for custom rendering
        // Otherwise, use Div for standard rendering
        let has_center_scoop =
            self.top_center_scoop.is_some() || self.bottom_center_scoop.is_some();
        let needs_custom = self.corners.needs_custom_rendering() || has_center_scoop;
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
        // Only provide canvas render if we have custom corners or center scoops
        let has_center_scoop =
            self.top_center_scoop.is_some() || self.bottom_center_scoop.is_some();
        let needs_custom = self.corners.needs_custom_rendering() || has_center_scoop;
        if !needs_custom {
            return None;
        }

        let corners = self.corners;
        let top_center_scoop = self.top_center_scoop;
        let bottom_center_scoop = self.bottom_center_scoop;
        let background = self.background.clone();
        let border_color = self.border_color;
        let border_width = self.border_width;
        let shadow = self.shadow;
        let opacity = self.opacity;

        Some(Rc::new(
            move |ctx: &mut dyn DrawContext, bounds: CanvasBounds| {
                // For concave corners, we need to offset the rect inward so the concave
                // portions (which extend outward) stay within the canvas bounds
                let tl_r = corners.top_left.radius;
                let tr_r = corners.top_right.radius;
                let bl_r = corners.bottom_left.radius;
                let br_r = corners.bottom_right.radius;

                // Calculate offsets for concave corners
                let left_offset =
                    if corners.top_left.is_concave() || corners.bottom_left.is_concave() {
                        tl_r.max(bl_r)
                    } else {
                        0.0
                    };
                let right_offset =
                    if corners.top_right.is_concave() || corners.bottom_right.is_concave() {
                        tr_r.max(br_r)
                    } else {
                        0.0
                    };
                let top_offset =
                    if corners.top_left.is_concave() || corners.top_right.is_concave() {
                        tl_r.max(tr_r)
                    } else {
                        0.0
                    };
                let bottom_offset =
                    if corners.bottom_left.is_concave() || corners.bottom_right.is_concave() {
                        bl_r.max(br_r)
                    } else {
                        0.0
                    };
                // NOTE: Center scoops do NOT add to the rect offset like concave corners.
                // The scoop curves INTO the shape (not outward), so the rect stays at bounds origin.

                // Create the rect with offsets so concave curves stay within bounds
                let rect = Rect::new(
                    left_offset,
                    top_offset,
                    bounds.width - left_offset - right_offset,
                    bounds.height - top_offset - bottom_offset,
                );

                // Build the path for any combination of corner types and center scoops
                let path = build_shape_path(
                    rect,
                    &corners,
                    top_center_scoop.as_ref(),
                    bottom_center_scoop.as_ref(),
                );

                // Find a reasonable radius for shadow approximation
                let shadow_radius = [
                    corners.bottom_left.radius,
                    corners.bottom_right.radius,
                    corners.top_left.radius,
                    corners.top_right.radius,
                ]
                .iter()
                .filter(|&&r| r > 0.0)
                .copied()
                .next()
                .unwrap_or(0.0);

                // Draw shadow first (behind the fill)
                if let Some(shadow) = shadow {
                    // For now, use bounding box shadow
                    // TODO: Path-based shadow
                    ctx.draw_shadow(rect, shadow_radius.into(), shadow);
                }

                // Fill the path (with opacity applied)
                if let Some(ref brush) = background {
                    let brush_with_opacity = if opacity < 1.0 {
                        match brush.clone() {
                            Brush::Solid(color) => {
                                Brush::Solid(color.with_alpha(color.a * opacity))
                            }
                            Brush::Gradient(g) => {
                                // Apply opacity to all gradient stops
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
                                // Recreate gradient with modified stops
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
                    } else {
                        brush.clone()
                    };
                    ctx.fill_path(&path, brush_with_opacity);
                }

                // Stroke the path (with opacity applied)
                if let (Some(color), width) = (border_color, border_width) {
                    if width > 0.0 {
                        let stroke_color = if opacity < 1.0 {
                            color.with_alpha(color.a * opacity)
                        } else {
                            color
                        };
                        ctx.stroke_path(
                            &path,
                            &blinc_core::Stroke::new(width),
                            Brush::Solid(stroke_color),
                        );
                    }
                }
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
