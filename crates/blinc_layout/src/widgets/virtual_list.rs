//! Virtualized list widget
//!
//! Renders only a window of items in a scrollable list, enabling efficient
//! display of large datasets without creating elements for every item.
//! Items can have variable heights — flexbox layout determines their size.
//!
//! # Example
//!
//! ```ignore
//! use blinc_layout::prelude::*;
//! use blinc_layout::widgets::virtual_list::virtual_list;
//!
//! let items: Vec<String> = (0..10_000).map(|i| format!("Item {}", i)).collect();
//!
//! virtual_list(items.len(), move |index| {
//!     div()
//!         .w_full()
//!         .p_px(8.0)
//!         .child(text(&items[index]).size(14.0).color(Color::WHITE))
//! })
//! .w_full()
//! .h(400.0)
//! ```

use std::sync::Arc;

use crate::div::{Div, div};
use crate::widgets::scroll::ScrollDirection;

/// Builder function that creates a Div for a given item index.
/// Items can be any height — flexbox layout determines their size.
pub type ItemBuilder = Arc<dyn Fn(usize) -> Div + Send + Sync>;

/// A virtualized list that only renders a window of items.
///
/// Items can have variable heights. The list uses an estimated item
/// height for scroll calculations and refines as items are rendered.
pub struct VirtualList {
    /// Total number of items
    item_count: usize,
    /// Function to build a Div for a given index
    item_builder: ItemBuilder,
    /// Inner div for layout props (width, height, bg, etc.)
    inner: Div,
    /// Estimated average item height (for scroll spacer calculation)
    estimated_item_height: f32,
    /// Number of items to render in the visible window
    /// (more items = smoother scroll, more elements)
    window_size: usize,
    /// CSS class applied to each item wrapper
    item_class: Option<String>,
    /// CSS class applied to the scroll content container
    content_class: Option<String>,
}

/// Create a virtualized list with variable-height items.
///
/// - `item_count`: total number of items
/// - `builder`: closure that creates a Div for each visible item index
///
/// Items can be any height. The layout system handles sizing via flexbox.
/// Use `.estimated_item_height()` to improve scroll spacer accuracy.
pub fn virtual_list<F>(item_count: usize, builder: F) -> VirtualList
where
    F: Fn(usize) -> Div + Send + Sync + 'static,
{
    VirtualList {
        item_count,
        item_builder: Arc::new(builder),
        inner: div().overflow_clip(),
        estimated_item_height: 40.0,
        window_size: 50,
        item_class: None,
        content_class: None,
    }
}

impl VirtualList {
    /// Set the estimated average item height (default: 40px).
    ///
    /// This is used to calculate the total scroll content height
    /// for items that haven't been rendered yet. It doesn't constrain
    /// actual item heights — items use flexbox sizing.
    pub fn estimated_item_height(mut self, height: f32) -> Self {
        self.estimated_item_height = height;
        self
    }

    /// Set the number of items to render in the visible window (default: 50).
    ///
    /// Larger values = smoother scrolling but more elements.
    /// Smaller values = better performance but may show blank areas during fast scroll.
    pub fn window_size(mut self, n: usize) -> Self {
        self.window_size = n;
        self
    }

    // Forward common Div methods
    pub fn w(mut self, v: f32) -> Self {
        self.inner = self.inner.w(v);
        self
    }
    pub fn h(mut self, v: f32) -> Self {
        self.inner = self.inner.h(v);
        self
    }
    pub fn w_full(mut self) -> Self {
        self.inner = self.inner.w_full();
        self
    }
    pub fn h_full(mut self) -> Self {
        self.inner = self.inner.h_full();
        self
    }
    pub fn bg(mut self, color: blinc_core::Color) -> Self {
        self.inner = self.inner.bg(color);
        self
    }
    pub fn rounded(mut self, r: f32) -> Self {
        self.inner = self.inner.rounded(r);
        self
    }
    pub fn border(mut self, width: f32, color: blinc_core::Color) -> Self {
        self.inner = self.inner.border(width, color);
        self
    }
    pub fn p(mut self, v: f32) -> Self {
        self.inner = self.inner.p(v);
        self
    }
    pub fn gap_px(mut self, v: f32) -> Self {
        self.inner = self.inner.gap_px(v);
        self
    }
    pub fn id(mut self, id: &str) -> Self {
        self.inner = self.inner.id(id);
        self
    }
    pub fn class(mut self, class: &str) -> Self {
        self.inner = self.inner.class(class);
        self
    }

    /// Set a CSS class applied to each item wrapper div
    pub fn item_class(mut self, class: impl Into<String>) -> Self {
        self.item_class = Some(class.into());
        self
    }

    /// Set a CSS class applied to the scroll content container
    pub fn content_class(mut self, class: impl Into<String>) -> Self {
        self.content_class = Some(class.into());
        self
    }

    /// Build the virtual list into a Div.
    ///
    /// Renders the initial window of items with spacers for
    /// items above and below the visible range.
    pub fn into_div(self) -> Div {
        let viewport_height = {
            use crate::div::ElementBuilder as _;
            self.inner
                .layout_style()
                .and_then(|s| match s.size.height.expand() {
                    taffy::style::ExpandedDimension::Length(h) => Some(h),
                    _ => None,
                })
                .unwrap_or(400.0)
        };

        // Render the first `window_size` items (or all if fewer)
        let render_count = self.window_size.min(self.item_count);

        // Build content column with flex layout (items size themselves)
        let mut content = div().flex_col().w_full();
        if let Some(ref cls) = self.content_class {
            content = content.class(cls);
        }
        for i in 0..render_count {
            let mut item = (self.item_builder)(i);
            if let Some(ref cls) = self.item_class {
                item = item.class(cls);
            }
            content = content.child(item);
        }

        // Estimated spacer for remaining items below the window
        if render_count < self.item_count {
            let remaining = self.item_count - render_count;
            let spacer_height = remaining as f32 * self.estimated_item_height;
            content = content.child(div().h(spacer_height).w_full());
        }

        // Wrap in scroll container
        let scroll = crate::widgets::scroll::scroll()
            .direction(ScrollDirection::Vertical)
            .w_full()
            .h(viewport_height)
            .child(content);

        self.inner.child(scroll)
    }
}

impl From<VirtualList> for Div {
    fn from(vl: VirtualList) -> Div {
        vl.into_div()
    }
}
