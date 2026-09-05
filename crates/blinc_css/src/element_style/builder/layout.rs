//! Layout: sizing, flex, alignment, spacing, overflow, and position.

use blinc_core::OverflowFade;

use crate::element_style::*;

impl ElementStyle {
    // =========================================================================
    // Layout: Sizing
    // =========================================================================

    /// Set width in pixels
    pub fn w(mut self, px: f32) -> Self {
        self.width = Some(StyleDimension::Length(px));
        self
    }

    /// Set height in pixels
    pub fn h(mut self, px: f32) -> Self {
        self.height = Some(StyleDimension::Length(px));
        self
    }

    /// Set minimum width in pixels
    pub fn min_w(mut self, px: f32) -> Self {
        self.min_width = Some(px);
        self
    }

    /// Set minimum height in pixels
    pub fn min_h(mut self, px: f32) -> Self {
        self.min_height = Some(px);
        self
    }

    /// Set maximum width in pixels
    pub fn max_w(mut self, px: f32) -> Self {
        self.max_width = Some(px);
        self
    }

    /// Set maximum height in pixels
    pub fn max_h(mut self, px: f32) -> Self {
        self.max_height = Some(px);
        self
    }

    // =========================================================================
    // Layout: Flex Direction & Display
    // =========================================================================

    /// Set display to flex with row direction
    pub fn flex_row(mut self) -> Self {
        self.display = Some(StyleDisplay::Flex);
        self.flex_direction = Some(StyleFlexDirection::Row);
        self
    }

    /// Set display to flex with column direction
    pub fn flex_col(mut self) -> Self {
        self.display = Some(StyleDisplay::Flex);
        self.flex_direction = Some(StyleFlexDirection::Column);
        self
    }

    /// Set display to flex with row-reverse direction
    pub fn flex_row_reverse(mut self) -> Self {
        self.display = Some(StyleDisplay::Flex);
        self.flex_direction = Some(StyleFlexDirection::RowReverse);
        self
    }

    /// Set display to flex with column-reverse direction
    pub fn flex_col_reverse(mut self) -> Self {
        self.display = Some(StyleDisplay::Flex);
        self.flex_direction = Some(StyleFlexDirection::ColumnReverse);
        self
    }

    /// Enable flex wrapping
    pub fn flex_wrap(mut self) -> Self {
        self.flex_wrap = Some(true);
        self
    }

    /// Set display to none (hidden)
    pub fn display_none(mut self) -> Self {
        self.display = Some(StyleDisplay::None);
        self
    }

    // =========================================================================
    // Layout: Flex Properties
    // =========================================================================

    /// Set flex-grow to 1
    pub fn flex_grow(mut self) -> Self {
        self.flex_grow = Some(1.0);
        self
    }

    /// Set flex-grow to a specific value
    pub fn flex_grow_value(mut self, value: f32) -> Self {
        self.flex_grow = Some(value);
        self
    }

    /// Set flex-shrink to 0 (prevent shrinking)
    pub fn flex_shrink_0(mut self) -> Self {
        self.flex_shrink = Some(0.0);
        self
    }

    // =========================================================================
    // Layout: Alignment
    // =========================================================================

    /// Align items to center on cross axis
    pub fn items_center(mut self) -> Self {
        self.align_items = Some(StyleAlign::Center);
        self
    }

    /// Align items to start on cross axis
    pub fn items_start(mut self) -> Self {
        self.align_items = Some(StyleAlign::Start);
        self
    }

    /// Align items to end on cross axis
    pub fn items_end(mut self) -> Self {
        self.align_items = Some(StyleAlign::End);
        self
    }

    /// Stretch items on cross axis
    pub fn items_stretch(mut self) -> Self {
        self.align_items = Some(StyleAlign::Stretch);
        self
    }

    /// Justify content to center on main axis
    pub fn justify_center(mut self) -> Self {
        self.justify_content = Some(StyleJustify::Center);
        self
    }

    /// Justify content to start on main axis
    pub fn justify_start(mut self) -> Self {
        self.justify_content = Some(StyleJustify::Start);
        self
    }

    /// Justify content to end on main axis
    pub fn justify_end(mut self) -> Self {
        self.justify_content = Some(StyleJustify::End);
        self
    }

    /// Space between items on main axis
    pub fn justify_between(mut self) -> Self {
        self.justify_content = Some(StyleJustify::SpaceBetween);
        self
    }

    /// Space around items on main axis
    pub fn justify_around(mut self) -> Self {
        self.justify_content = Some(StyleJustify::SpaceAround);
        self
    }

    /// Space evenly on main axis
    pub fn justify_evenly(mut self) -> Self {
        self.justify_content = Some(StyleJustify::SpaceEvenly);
        self
    }

    /// Align self to center (override parent's align-items)
    pub fn self_center(mut self) -> Self {
        self.align_self = Some(StyleAlign::Center);
        self
    }

    /// Align self to start (override parent's align-items)
    pub fn self_start(mut self) -> Self {
        self.align_self = Some(StyleAlign::Start);
        self
    }

    /// Align self to end (override parent's align-items)
    pub fn self_end(mut self) -> Self {
        self.align_self = Some(StyleAlign::End);
        self
    }

    // =========================================================================
    // Layout: Spacing
    // =========================================================================

    /// Set uniform padding in pixels
    pub fn p(mut self, px: f32) -> Self {
        self.padding = Some(SpacingRect::uniform(px));
        self
    }

    /// Set horizontal and vertical padding in pixels
    pub fn p_xy(mut self, x: f32, y: f32) -> Self {
        self.padding = Some(SpacingRect::xy(x, y));
        self
    }

    /// Set per-side padding in pixels (top, right, bottom, left)
    pub fn p_trbl(mut self, top: f32, right: f32, bottom: f32, left: f32) -> Self {
        self.padding = Some(SpacingRect::new(top, right, bottom, left));
        self
    }

    /// Set uniform margin in pixels
    pub fn m(mut self, px: f32) -> Self {
        self.margin = Some(SpacingRect::uniform(px));
        self
    }

    /// Set horizontal and vertical margin in pixels
    pub fn m_xy(mut self, x: f32, y: f32) -> Self {
        self.margin = Some(SpacingRect::xy(x, y));
        self
    }

    /// Set per-side margin in pixels (top, right, bottom, left)
    pub fn m_trbl(mut self, top: f32, right: f32, bottom: f32, left: f32) -> Self {
        self.margin = Some(SpacingRect::new(top, right, bottom, left));
        self
    }

    /// Set uniform gap between children in pixels
    pub fn gap(mut self, px: f32) -> Self {
        self.gap = Some(px);
        self
    }

    // =========================================================================
    // Layout: Overflow
    // =========================================================================

    /// Clip overflow
    pub fn overflow_clip(mut self) -> Self {
        self.overflow = Some(StyleOverflow::Clip);
        self
    }

    /// Allow visible overflow
    pub fn overflow_visible(mut self) -> Self {
        self.overflow = Some(StyleOverflow::Visible);
        self
    }

    /// Enable scroll overflow
    pub fn overflow_scroll(mut self) -> Self {
        self.overflow = Some(StyleOverflow::Scroll);
        self
    }

    // =========================================================================
    // Overflow Fade
    // =========================================================================

    /// Set uniform overflow fade distance (in pixels)
    pub fn overflow_fade(mut self, distance: f32) -> Self {
        self.overflow_fade = Some(OverflowFade::uniform(distance));
        self
    }

    /// Set per-edge overflow fade (top, right, bottom, left)
    pub fn overflow_fade_edges(mut self, top: f32, right: f32, bottom: f32, left: f32) -> Self {
        self.overflow_fade = Some(OverflowFade::new(top, right, bottom, left));
        self
    }

    /// Set vertical overflow fade only (top + bottom)
    pub fn overflow_fade_y(mut self, distance: f32) -> Self {
        self.overflow_fade = Some(OverflowFade::vertical(distance));
        self
    }

    /// Set horizontal overflow fade only (left + right)
    pub fn overflow_fade_x(mut self, distance: f32) -> Self {
        self.overflow_fade = Some(OverflowFade::horizontal(distance));
        self
    }

    // =========================================================================
    // Overflow per-axis
    // =========================================================================

    /// Set overflow-x behavior
    pub fn overflow_x(mut self, o: StyleOverflow) -> Self {
        self.overflow_x = Some(o);
        self
    }

    /// Set overflow-y behavior
    pub fn overflow_y(mut self, o: StyleOverflow) -> Self {
        self.overflow_y = Some(o);
        self
    }

    // =========================================================================
    // Position & Inset
    // =========================================================================

    /// Set CSS position
    pub fn position(mut self, pos: StylePosition) -> Self {
        self.position = Some(pos);
        self
    }

    /// Set top inset in pixels
    pub fn top(mut self, px: f32) -> Self {
        self.top = Some(px);
        self
    }

    /// Set right inset in pixels
    pub fn right(mut self, px: f32) -> Self {
        self.right = Some(px);
        self
    }

    /// Set bottom inset in pixels
    pub fn bottom(mut self, px: f32) -> Self {
        self.bottom = Some(px);
        self
    }

    /// Set left inset in pixels
    pub fn left(mut self, px: f32) -> Self {
        self.left = Some(px);
        self
    }

    /// Set inset for all sides
    pub fn inset(mut self, px: f32) -> Self {
        self.top = Some(px);
        self.right = Some(px);
        self.bottom = Some(px);
        self.left = Some(px);
        self
    }

    /// Set z-index
    pub fn z_index(mut self, z: i32) -> Self {
        self.z_index = Some(z);
        self
    }

    /// Set visibility
    pub fn visibility(mut self, vis: StyleVisibility) -> Self {
        self.visibility = Some(vis);
        self
    }

    // =========================================================================
    // Flex shrink with value
    // =========================================================================

    /// Set flex-shrink to a specific value
    pub fn flex_shrink(mut self, value: f32) -> Self {
        self.flex_shrink = Some(value);
        self
    }
}
