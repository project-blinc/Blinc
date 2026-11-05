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

use crate::{key::InstanceKey, selector::ScrollRef};
use blinc_core::reactive::{computed, signal};
use std::sync::{Arc, Mutex};
mod heights;
use heights::Heights;

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
    key: InstanceKey,
    scroll_ref: Option<ScrollRef>,
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
#[track_caller]
pub fn virtual_list<F>(item_count: usize, builder: F) -> VirtualList
where
    F: Fn(usize) -> Div + Send + Sync + 'static,
{
    VirtualList {
        item_count,
        key: InstanceKey::new("virtual-list"),
        scroll_ref: None,
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
        self.estimated_item_height = if height.is_finite() && height > 0.0 {
            height
        } else {
            40.0
        };
        self
    }

    /// Target mounted row count, including overscan (default: 50).
    /// The visible viewport is always included, even when it needs more rows.
    pub fn window_size(mut self, n: usize) -> Self {
        self.window_size = n.max(1);
        self
    }

    /// Bind a reference for programmatic scrolling and observing the offset.
    pub fn bind(mut self, reference: &ScrollRef) -> Self {
        self.scroll_ref = Some(reference.clone());
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

    /// Build a measured, reactive window with spacers for unmounted rows.
    pub fn into_div(self) -> Div {
        use crate::div::ElementBuilder as _;
        let initial_height = self
            .inner
            .layout_style()
            .and_then(|s| match s.size.height.expand() {
                taffy::style::ExpandedDimension::Length(h) => Some(h),
                _ => None,
            })
            .unwrap_or(400.0);
        let key = self.key.derive("window");
        let count = self.item_count;
        let estimate = self.estimated_item_height;
        let create = || {
            let reference = self.scroll_ref.unwrap_or_default();
            let offset = reference.watch_y(0.0);
            (
                reference,
                offset,
                signal(initial_height),
                signal(0u64),
                Arc::new(Mutex::new(Heights::new(count, estimate))),
            )
        };
        let (reference, offset, viewport, revision, heights) =
            if let Some(ctx) = blinc_core::BlincContextState::try_get() {
                ctx.use_signal_keyed(&key, create)
                    .try_get()
                    .expect("virtual list state")
            } else {
                create()
            };
        if heights.lock().unwrap().reconfigure(count, estimate) {
            revision.set(revision.get().wrapping_add(1));
        }
        // Resizing or changing the dataset updates the window without resetting scroll.
        let budget = self.window_size;
        let measured = heights.clone();
        let window = computed(move |g| {
            let _ = g.get(revision);
            measured.lock().unwrap().window(
                g.get(offset).unwrap_or(0.0),
                g.get(viewport).unwrap_or(initial_height),
                budget,
            )
        });
        let top_window = window.clone();
        let bottom_window = window.clone();
        let top = computed(move |_| top_window.try_get().map(|w| w.2).unwrap_or(0.0));
        let bottom = computed(move |_| bottom_window.try_get().map(|w| w.3).unwrap_or(0.0));
        let indices = computed(move |_| {
            window
                .try_get()
                .map(|w| (w.0..w.1).collect::<Vec<_>>())
                .unwrap_or_default()
        });
        let builder = self.item_builder;
        let item_class = self.item_class;
        let rows = div().flex_col().w_full().h_fit().for_each(
            indices,
            |i| *i,
            move |i| {
                let measured = heights.clone();
                let mut row = builder(i).flex_shrink_0().on_resize(move |event| {
                    let changed = measured.lock().unwrap().measure(i, event.bounds_height);
                    if changed {
                        revision.set(revision.get().wrapping_add(1));
                    }
                });
                if let Some(class) = &item_class {
                    row = row.class(class);
                }
                row
            },
        );
        let mut content = div()
            .flex_col()
            .w_full()
            .h_fit()
            .child(div().w_full().h(top).flex_shrink_0())
            .child(rows)
            .child(div().w_full().h(bottom).flex_shrink_0());
        if let Some(class) = self.content_class {
            content = content.class(class);
        }
        let scroll = crate::widgets::scroll::scroll()
            .direction(ScrollDirection::Vertical)
            .bind(&reference)
            .w_full()
            .h_full()
            .child(content);
        self.inner
            .on_resize(move |event| {
                if (viewport.get() - event.bounds_height).abs() > 0.5 && event.bounds_height > 0.0 {
                    viewport.set(event.bounds_height);
                }
            })
            .child(scroll)
    }
}

impl From<VirtualList> for Div {
    fn from(vl: VirtualList) -> Div {
        vl.into_div()
    }
}
