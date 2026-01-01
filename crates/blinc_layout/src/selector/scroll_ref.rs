//! ScrollRef - Reference for programmatic scroll control

use std::sync::{Arc, Mutex, Weak};

use blinc_core::reactive::SignalId;

use crate::tree::LayoutNodeId;

use super::registry::ElementRegistry;
use super::{ScrollBehavior, ScrollOptions};

/// Callback type for triggering reactive updates
pub type TriggerCallback = Arc<dyn Fn() + Send + Sync>;

/// Shared inner state for ScrollRef (public for persistence across rebuilds)
pub type SharedScrollRefInner = Arc<Mutex<ScrollRefInner>>;

/// Inner state for ScrollRef
#[derive(Debug, Default)]
pub struct ScrollRefInner {
    /// The layout node ID of the scroll container (set after build)
    node_id: Option<LayoutNodeId>,
    /// Registry for looking up child elements by ID
    registry: Option<Weak<ElementRegistry>>,
    /// Current scroll offset (x, y)
    offset: (f32, f32),
    /// Content size (for scroll limits)
    content_size: Option<(f32, f32)>,
    /// Viewport size
    viewport_size: Option<(f32, f32)>,
    /// Pending scroll command (consumed by renderer)
    pending_scroll: Option<PendingScroll>,
    /// Whether the scroll state has been modified
    dirty: bool,
}

/// A pending scroll operation to be executed by the renderer
#[derive(Debug, Clone)]
pub enum PendingScroll {
    /// Scroll to absolute offset
    ToOffset { x: f32, y: f32, smooth: bool },
    /// Scroll by relative amount
    ByAmount { dx: f32, dy: f32, smooth: bool },
    /// Scroll to make an element visible
    ToElement {
        element_id: String,
        options: ScrollOptions,
    },
    /// Scroll to top
    ToTop { smooth: bool },
    /// Scroll to bottom
    ToBottom { smooth: bool },
}

/// Reference for programmatic control of a scroll container
///
/// Create with `ctx.use_scroll_ref("key")` and bind to a scroll widget with `.bind(&scroll_ref)`.
///
/// # Example
///
/// ```rust,ignore
/// let scroll_ref = ctx.use_scroll_ref("my_scroll");
///
/// scroll()
///     .bind(&scroll_ref)
///     .child(items.iter().map(|i| div().id(format!("item-{}", i.id))))
///
/// // Later:
/// scroll_ref.scroll_to("item-42");
/// scroll_ref.scroll_to_bottom();
/// ```
#[derive(Clone)]
pub struct ScrollRef {
    inner: Arc<Mutex<ScrollRefInner>>,
    /// Signal ID for reactive updates
    signal_id: SignalId,
    /// Callback to trigger the signal (set by WindowedContext)
    trigger: TriggerCallback,
}

impl Default for ScrollRef {
    fn default() -> Self {
        Self::new()
    }
}

impl ScrollRef {
    /// Create a ScrollRef with a trigger callback (used by WindowedContext)
    pub fn with_trigger(signal_id: SignalId, trigger: TriggerCallback) -> Self {
        Self {
            inner: Arc::new(Mutex::new(ScrollRefInner::default())),
            signal_id,
            trigger,
        }
    }

    /// Create a ScrollRef with an existing inner state (for persistence across rebuilds)
    pub fn with_inner(
        inner: SharedScrollRefInner,
        signal_id: SignalId,
        trigger: TriggerCallback,
    ) -> Self {
        Self {
            inner,
            signal_id,
            trigger,
        }
    }

    /// Get a clone of the inner state for persistence
    pub fn inner(&self) -> SharedScrollRefInner {
        Arc::clone(&self.inner)
    }

    /// Create a new SharedScrollRefInner for storage
    pub fn new_inner() -> SharedScrollRefInner {
        Arc::new(Mutex::new(ScrollRefInner::default()))
    }
}

impl std::fmt::Debug for ScrollRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScrollRef")
            .field("bound", &self.is_bound())
            .finish()
    }
}

impl ScrollRef {
    /// Create a new scroll reference (standalone, without reactive integration)
    ///
    /// Prefer using `ctx.use_scroll_ref()` which properly integrates with the
    /// reactive system for automatic UI updates.
    pub fn new() -> Self {
        // Create a no-op trigger for standalone use
        let noop_trigger: TriggerCallback = Arc::new(|| {});
        Self {
            inner: Arc::new(Mutex::new(ScrollRefInner::default())),
            signal_id: SignalId::from_raw(0), // Placeholder
            trigger: noop_trigger,
        }
    }

    /// Get the signal ID for this scroll ref (for dependency tracking)
    pub fn signal_id(&self) -> SignalId {
        self.signal_id
    }

    /// Trigger reactive update
    fn trigger(&self) {
        (self.trigger)();
    }

    /// Check if this ref is bound to a scroll container
    pub fn is_bound(&self) -> bool {
        self.inner
            .lock()
            .ok()
            .is_some_and(|inner| inner.node_id.is_some())
    }

    // =========================================================================
    // Internal methods (called by scroll widget and renderer)
    // =========================================================================

    /// Bind this ref to a scroll container node (called during build)
    pub(crate) fn bind_to_node(&self, node_id: LayoutNodeId, registry: Weak<ElementRegistry>) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.node_id = Some(node_id);
            inner.registry = Some(registry);
        }
    }

    /// Get the bound node ID
    pub(crate) fn node_id(&self) -> Option<LayoutNodeId> {
        self.inner.lock().ok()?.node_id
    }

    /// Update scroll state from renderer
    pub(crate) fn update_state(
        &self,
        offset: (f32, f32),
        content_size: (f32, f32),
        viewport_size: (f32, f32),
    ) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.offset = offset;
            inner.content_size = Some(content_size);
            inner.viewport_size = Some(viewport_size);
        }
    }

    /// Take pending scroll command (called by renderer each frame)
    pub(crate) fn take_pending_scroll(&self) -> Option<PendingScroll> {
        self.inner.lock().ok()?.pending_scroll.take()
    }

    /// Check and clear dirty flag
    pub(crate) fn take_dirty(&self) -> bool {
        if let Ok(mut inner) = self.inner.lock() {
            let dirty = inner.dirty;
            inner.dirty = false;
            dirty
        } else {
            false
        }
    }

    // =========================================================================
    // Public scroll operations
    // =========================================================================

    /// Scroll to an element by ID within this container
    pub fn scroll_to(&self, element_id: &str) {
        self.scroll_to_with_options(element_id, ScrollOptions::default());
    }

    /// Scroll to an element with custom options
    pub fn scroll_to_with_options(&self, element_id: &str, options: ScrollOptions) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.pending_scroll = Some(PendingScroll::ToElement {
                element_id: element_id.to_string(),
                options,
            });
            inner.dirty = true;
        }
        self.trigger();
    }

    /// Scroll to the top of the content
    pub fn scroll_to_top(&self) {
        self.scroll_to_top_with_behavior(ScrollBehavior::Auto);
    }

    /// Scroll to the top with specified behavior
    pub fn scroll_to_top_with_behavior(&self, behavior: ScrollBehavior) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.pending_scroll = Some(PendingScroll::ToTop {
                smooth: behavior == ScrollBehavior::Smooth,
            });
            inner.dirty = true;
        }
        self.trigger();
    }

    /// Scroll to the bottom of the content
    pub fn scroll_to_bottom(&self) {
        self.scroll_to_bottom_with_behavior(ScrollBehavior::Auto);
    }

    /// Scroll to the bottom with specified behavior
    pub fn scroll_to_bottom_with_behavior(&self, behavior: ScrollBehavior) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.pending_scroll = Some(PendingScroll::ToBottom {
                smooth: behavior == ScrollBehavior::Smooth,
            });
            inner.dirty = true;
        }
        self.trigger();
    }

    /// Scroll by a relative amount
    pub fn scroll_by(&self, dx: f32, dy: f32) {
        self.scroll_by_with_behavior(dx, dy, ScrollBehavior::Auto);
    }

    /// Scroll by a relative amount with specified behavior
    pub fn scroll_by_with_behavior(&self, dx: f32, dy: f32, behavior: ScrollBehavior) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.pending_scroll = Some(PendingScroll::ByAmount {
                dx,
                dy,
                smooth: behavior == ScrollBehavior::Smooth,
            });
            inner.dirty = true;
        }
        self.trigger();
    }

    /// Set absolute scroll offset
    pub fn set_scroll_offset(&self, x: f32, y: f32) {
        self.set_scroll_offset_with_behavior(x, y, ScrollBehavior::Auto);
    }

    /// Set absolute scroll offset with specified behavior
    pub fn set_scroll_offset_with_behavior(&self, x: f32, y: f32, behavior: ScrollBehavior) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.pending_scroll = Some(PendingScroll::ToOffset {
                x,
                y,
                smooth: behavior == ScrollBehavior::Smooth,
            });
            inner.dirty = true;
        }
        self.trigger();
    }

    // =========================================================================
    // Query current state
    // =========================================================================

    /// Get current scroll offset
    pub fn offset(&self) -> (f32, f32) {
        self.inner.lock().ok().map(|i| i.offset).unwrap_or_default()
    }

    /// Get current horizontal scroll offset
    pub fn scroll_x(&self) -> f32 {
        self.offset().0
    }

    /// Get current vertical scroll offset
    pub fn scroll_y(&self) -> f32 {
        self.offset().1
    }

    /// Get content size
    pub fn content_size(&self) -> Option<(f32, f32)> {
        self.inner.lock().ok()?.content_size
    }

    /// Get viewport size
    pub fn viewport_size(&self) -> Option<(f32, f32)> {
        self.inner.lock().ok()?.viewport_size
    }

    /// Get maximum scroll offset
    pub fn max_scroll(&self) -> Option<(f32, f32)> {
        let inner = self.inner.lock().ok()?;
        let content = inner.content_size?;
        let viewport = inner.viewport_size?;
        Some((
            (content.0 - viewport.0).max(0.0),
            (content.1 - viewport.1).max(0.0),
        ))
    }

    /// Check if scrolled to top
    pub fn is_at_top(&self) -> bool {
        self.scroll_y() <= 0.0
    }

    /// Check if scrolled to bottom
    pub fn is_at_bottom(&self) -> bool {
        if let Some((_, max_y)) = self.max_scroll() {
            self.scroll_y() >= max_y - 1.0 // Small tolerance
        } else {
            true
        }
    }

    /// Get scroll progress (0.0 = top, 1.0 = bottom)
    pub fn scroll_progress(&self) -> f32 {
        if let Some((_, max_y)) = self.max_scroll() {
            if max_y > 0.0 {
                (self.scroll_y() / max_y).clamp(0.0, 1.0)
            } else {
                0.0
            }
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::selector::{ScrollBlock, ScrollInline};

    #[test]
    fn test_scroll_ref_new() {
        let scroll_ref = ScrollRef::new();
        assert!(!scroll_ref.is_bound());
    }

    #[test]
    fn test_scroll_to_bottom() {
        let scroll_ref = ScrollRef::new();
        scroll_ref.scroll_to_bottom();

        let pending = scroll_ref.take_pending_scroll();
        assert!(matches!(
            pending,
            Some(PendingScroll::ToBottom { smooth: false })
        ));
    }

    #[test]
    fn test_scroll_to_element() {
        let scroll_ref = ScrollRef::new();
        scroll_ref.scroll_to_with_options(
            "my-item",
            ScrollOptions {
                behavior: ScrollBehavior::Smooth,
                block: ScrollBlock::Center,
                inline: ScrollInline::Nearest,
            },
        );

        let pending = scroll_ref.take_pending_scroll();
        assert!(matches!(
            pending,
            Some(PendingScroll::ToElement {
                element_id,
                options,
            }) if element_id == "my-item" && options.behavior == ScrollBehavior::Smooth
        ));
    }

    #[test]
    fn test_scroll_offset_query() {
        let scroll_ref = ScrollRef::new();
        scroll_ref.update_state((100.0, 200.0), (500.0, 1000.0), (300.0, 400.0));

        assert_eq!(scroll_ref.offset(), (100.0, 200.0));
        assert_eq!(scroll_ref.scroll_x(), 100.0);
        assert_eq!(scroll_ref.scroll_y(), 200.0);
        assert_eq!(scroll_ref.content_size(), Some((500.0, 1000.0)));
        assert_eq!(scroll_ref.viewport_size(), Some((300.0, 400.0)));
        assert_eq!(scroll_ref.max_scroll(), Some((200.0, 600.0)));
    }

    #[test]
    fn test_scroll_progress() {
        let scroll_ref = ScrollRef::new();
        scroll_ref.update_state((0.0, 300.0), (500.0, 1000.0), (300.0, 400.0));

        // max_y = 1000 - 400 = 600, progress = 300/600 = 0.5
        assert!((scroll_ref.scroll_progress() - 0.5).abs() < 0.01);
    }
}
