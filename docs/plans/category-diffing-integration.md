# Category-Based Diffing Integration Plan

## Overview

Integrate the existing `diff.rs` category-based change detection system with `mark_dirty()` to enable targeted updates instead of full UI rebuilds.

## Current State

### What Exists

1. **diff.rs** - Sophisticated change detection system:
   - `ChangeCategory { layout, visual, children, handlers }` - Categorizes what changed
   - `DivHash` - Content-based identity via hashing
   - `diff()` / `diff_elements()` - Compare two elements and produce detailed diff
   - `reconcile()` - Produces `ReconcileActions` with prop updates vs subtree rebuilds

2. **stateful.rs** - Incremental update queues:
   - `queue_prop_update(node_id, props)` - Visual-only updates
   - `queue_subtree_rebuild(parent_id, new_child)` - Structural changes
   - `take_pending_prop_updates()` / `take_pending_subtree_rebuilds()` - Processing

3. **renderer.rs** - Partial integration:
   - `incremental_update()` - Uses hash comparison and change analysis
   - `UpdateResult { NoChanges, VisualOnly, LayoutChanged, ChildrenChanged }`
   - `update_render_props_in_place()` - Apply visual changes without rebuild

4. **windowed.rs** - Event loop integration:
   - Processes `take_pending_prop_updates()` for event-driven changes
   - Calls `tree.process_pending_subtree_rebuilds()` for structural changes
   - Conditionally skips layout when only visual properties change

### What's Missing

1. **mark_dirty() uses full rebuild** - Just sets dirty flag, no diffing
2. **No element reconstruction** - Can't diff without original element stored
3. **No public diff API for arbitrary nodes** - Only works during full tree rebuild
4. **Node ID ↔ Element mapping** - Need to map between IDs and element positions

---

## Architecture Decision

### Option A: Store Original Elements (Recommended)

Store the original `Div` or `ElementBuilder` in `RenderNode` to enable diffing.

**Pros:**
- Direct diffing without reconstruction
- Works with existing diff.rs infrastructure
- Clean integration path

**Cons:**
- Memory overhead (storing Div per node)
- Need to update stored element on changes

### Option B: Reconstruct from RenderProps

Build a minimal `Div` from `RenderProps` for comparison.

**Pros:**
- No additional storage
- Works with current architecture

**Cons:**
- Lossy reconstruction (RenderProps doesn't capture everything)
- Complex reconstruction logic
- May miss some change categories

### Option C: Hash-Only Comparison

Store only hashes, use them to detect changes, but rebuild entire subtree on any change.

**Pros:**
- Minimal storage (just u64 hash)
- Simple implementation

**Cons:**
- No category-based optimization
- All changes trigger full subtree rebuild

**Decision: Option A** - Store original elements for accurate diffing.

---

## Implementation Plan

### Phase 1: Store Original Elements in RenderNode

**File: `crates/blinc_layout/src/renderer.rs`**

```rust
pub struct RenderNode {
    // Existing fields...
    pub props: RenderProps,
    pub parent: Option<LayoutNodeId>,
    pub children: Vec<LayoutNodeId>,

    // NEW: Store original element for diffing
    pub original_div: Option<Div>,  // Only for nodes with .id()
}
```

**Changes:**
1. Add `original_div: Option<Div>` field to `RenderNode`
2. Populate during tree build for nodes with string IDs
3. Update when props change via incremental update

### Phase 2: Add Targeted Update API to RenderTree

**File: `crates/blinc_layout/src/renderer.rs`**

```rust
impl RenderTree {
    /// Update a specific node using category-based diffing
    ///
    /// Returns what kind of update was performed, allowing caller
    /// to skip layout if only visual properties changed.
    pub fn update_node_diffed(
        &mut self,
        node_id: LayoutNodeId,
        new_element: &Div,
    ) -> UpdateResult {
        // Get stored original element
        let Some(original) = self.get_original_div(node_id) else {
            // No original stored, fall back to full update
            return self.rebuild_subtree(node_id, new_element);
        };

        // Diff old vs new
        let diff_result = diff(&original, new_element);

        // Apply based on category
        match diff_result.changes {
            c if c.is_none() => UpdateResult::NoChanges,

            c if c.visual_only() => {
                // Just update render props
                self.update_render_props(node_id, |p| {
                    *p = new_element.render_props();
                });
                self.store_original_div(node_id, new_element.clone());
                UpdateResult::VisualOnly
            }

            c if c.layout && !c.children => {
                // Update props, mark for relayout
                self.update_render_props(node_id, |p| {
                    *p = new_element.render_props();
                });
                self.store_original_div(node_id, new_element.clone());
                UpdateResult::LayoutChanged
            }

            _ => {
                // Children changed - rebuild subtree
                self.rebuild_subtree(node_id, new_element);
                UpdateResult::ChildrenChanged
            }
        }
    }
}
```

### Phase 3: Wire mark_dirty() to Use Diffing

**File: `crates/blinc_layout/src/selector/handle.rs`**

Current (problematic):
```rust
pub fn mark_dirty(&self) {
    if let Some(ctx) = BlincContextState::try_get() {
        ctx.request_rebuild();  // Full rebuild!
    }
}
```

New approach - requires element to compare against:
```rust
/// Mark this element as dirty with a new element for diffing
///
/// Uses category-based diffing to determine minimal update:
/// - Visual-only changes: Update props, skip layout
/// - Layout changes: Update props, recompute layout
/// - Children changes: Rebuild subtree, recompute layout
pub fn mark_dirty_with(&self, new_element: Div) {
    if let Some(node_id) = self.registry.get(&self.string_id) {
        // Queue a diffed update
        crate::stateful::queue_diffed_update(node_id, new_element);
    }
}

/// Mark dirty for visual-only changes (skip diffing)
///
/// Use when you know only visual properties changed (bg, opacity, etc.)
pub fn mark_visual_dirty(&self, new_props: RenderProps) {
    if let Some(node_id) = self.registry.get(&self.string_id) {
        crate::stateful::queue_prop_update(node_id, new_props);
    }
}
```

### Phase 4: Add Diffed Update Queue to stateful.rs

**File: `crates/blinc_layout/src/stateful.rs`**

```rust
/// Pending diffed update - element update with category detection
pub struct PendingDiffedUpdate {
    pub node_id: LayoutNodeId,
    pub new_element: Div,
}

static PENDING_DIFFED_UPDATES: LazyLock<Mutex<Vec<PendingDiffedUpdate>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

/// Queue a diffed update for a node
pub fn queue_diffed_update(node_id: LayoutNodeId, new_element: Div) {
    PENDING_DIFFED_UPDATES.lock().unwrap().push(
        PendingDiffedUpdate { node_id, new_element }
    );
    request_redraw();
}

/// Take all pending diffed updates
pub fn take_pending_diffed_updates() -> Vec<PendingDiffedUpdate> {
    std::mem::take(&mut *PENDING_DIFFED_UPDATES.lock().unwrap())
}

/// Check if there are pending diffed updates
pub fn has_pending_diffed_updates() -> bool {
    PENDING_DIFFED_UPDATES.lock().map(|v| !v.is_empty()).unwrap_or(false)
}
```

### Phase 5: Process Diffed Updates in Event Loop

**File: `crates/blinc_app/src/windowed.rs`**

Add to the `take_needs_redraw()` block:

```rust
if blinc_layout::take_needs_redraw() {
    // Existing: Process prop updates
    let prop_updates = blinc_layout::take_pending_prop_updates();
    // ...apply prop updates...

    // NEW: Process diffed updates with category detection
    let diffed_updates = blinc_layout::take_pending_diffed_updates();
    let mut needs_layout_from_diff = false;

    for update in diffed_updates {
        let result = tree.update_node_diffed(update.node_id, &update.new_element);
        match result {
            UpdateResult::LayoutChanged | UpdateResult::ChildrenChanged => {
                needs_layout_from_diff = true;
            }
            _ => {}
        }
    }

    // Existing: Process subtree rebuilds
    let had_subtree_rebuilds = blinc_layout::has_pending_subtree_rebuilds();
    tree.process_pending_subtree_rebuilds();

    // Recompute layout if needed
    if had_subtree_rebuilds || needs_layout_from_diff {
        tree.compute_layout(ctx.width, ctx.height);
    }

    window.request_redraw();
}
```

---

## API Design

### ElementHandle Methods

```rust
impl ElementHandle {
    /// Mark dirty - triggers full rebuild (fallback)
    ///
    /// Use mark_dirty_with() or mark_visual_dirty() for better performance.
    pub fn mark_dirty(&self) { ... }

    /// Mark dirty with new element - uses category diffing
    ///
    /// Best for structural or layout changes where you have the new element.
    pub fn mark_dirty_with(&self, new_element: Div) { ... }

    /// Mark dirty for visual-only changes - skips layout
    ///
    /// Best for color, opacity, shadow changes.
    pub fn mark_visual_dirty(&self, new_props: RenderProps) { ... }

    /// Mark subtree dirty with new children
    ///
    /// Best when you know children structure changed.
    pub fn mark_dirty_subtree(&self, new_children: Div) { ... }
}
```

### Usage Examples

```rust
// Visual-only change (fastest)
ctx.query("my-button").mark_visual_dirty(
    RenderProps::default().bg(Color::RED)
);

// Layout change with diffing
ctx.query("my-card").mark_dirty_with(
    div().w(200.0).h(150.0).bg(Color::BLUE)  // Width changed
);

// Children structure change
ctx.query("my-list").mark_dirty_subtree(
    div().children(new_list_items)
);

// Fallback - full rebuild
ctx.query("complex-widget").mark_dirty();
```

---

## Testing Strategy

### Unit Tests

1. **diff.rs tests** - Already exist, verify change detection
2. **RenderTree::update_node_diffed()** - Test each UpdateResult path
3. **Queue processing** - Test diffed update queue integration

### Integration Tests

1. **Visual-only update** - Verify layout not recomputed
2. **Layout change** - Verify layout recomputed
3. **Children change** - Verify subtree rebuilt
4. **Mixed updates** - Multiple nodes with different change types

### Performance Benchmarks

1. Compare full rebuild vs diffed update for:
   - Single visual property change
   - Single layout property change
   - Subtree structure change
   - Large tree with localized change

---

## Migration Path

### Phase 1: Non-Breaking Addition
- Add new APIs alongside existing `mark_dirty()`
- Existing code continues to work
- New code can opt into diffed updates

### Phase 2: Deprecation Warning
- Add warning to `mark_dirty()` recommending alternatives
- Document migration in changelog

### Phase 3: Integration with Stateful Elements
- Consider making stateful elements use diffed updates internally
- Could improve performance for complex state callbacks

---

## Files to Modify

1. **`crates/blinc_layout/src/renderer.rs`**
   - Add `original_div` field to `RenderNode`
   - Add `update_node_diffed()` method
   - Add `store_original_div()` / `get_original_div()` helpers

2. **`crates/blinc_layout/src/stateful.rs`**
   - Add `PendingDiffedUpdate` struct
   - Add `queue_diffed_update()` function
   - Add `take_pending_diffed_updates()` function

3. **`crates/blinc_layout/src/selector/handle.rs`**
   - Add `mark_dirty_with()` method
   - Add `mark_visual_dirty()` method
   - Keep existing `mark_dirty()` as fallback

4. **`crates/blinc_app/src/windowed.rs`**
   - Process diffed updates in event loop
   - Track layout needs from diffed updates

5. **`crates/blinc_layout/src/lib.rs`**
   - Export new public APIs

---

## Open Questions

1. **Memory overhead** - Is storing `Div` per node acceptable? Could store only for nodes with `.id()`.

2. **Div cloning** - `Div` contains `Arc` for children, so cloning is cheap. Verify.

3. **Handler comparison** - Should handler changes trigger rebuild or just update?

4. **Batching** - Should multiple diffed updates to same node be coalesced?

5. **Async considerations** - Are there race conditions with queued updates?

---

## Success Criteria

1. Visual-only changes via `mark_visual_dirty()` skip layout computation
2. Layout changes via `mark_dirty_with()` correctly detect category
3. Children changes trigger subtree rebuild only
4. No performance regression for existing code paths
5. Memory overhead is acceptable (< 10% increase for typical apps)
