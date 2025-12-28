# Layout & Diff System

Blinc separates layout computation from visual rendering, enabling incremental updates without full tree rebuilds.

## Layout System

### Taffy Integration

Blinc uses [Taffy](https://github.com/DioxusLabs/taffy) for flexbox layout computation. Taffy is a high-performance layout engine that implements the CSS Flexbox specification.

```rust
// LayoutTree wraps Taffy
struct LayoutTree {
    taffy: TaffyTree,
    node_map: HashMap<LayoutNodeId, TaffyNodeId>,
    reverse_map: HashMap<TaffyNodeId, LayoutNodeId>,
}
```

### Layout Properties vs Visual Properties

Layout and visual properties are handled separately:

| Layout Properties | Visual Properties |
|-------------------|-------------------|
| `width`, `height` | `background` |
| `padding`, `margin` | `border_color` |
| `flex_direction` | `shadow` |
| `justify_content` | `opacity` |
| `align_items` | `transform` |
| `gap` | `rounded` |

This separation allows visual-only updates without layout recomputation.

### RenderTree Structure

```rust
struct RenderTree {
    layout_tree: LayoutTree,           // Taffy wrapper
    render_props: HashMap<NodeId, RenderProps>,  // Visual properties
    dirty_nodes: HashSet<NodeId>,      // Nodes needing update
    scroll_state: HashMap<NodeId, ScrollState>,
    motion_bindings: HashMap<NodeId, MotionBinding>,
}
```

### Layout Computation

Layout is computed on-demand when the tree structure changes:

```rust
// Compute layout for the tree
render_tree.compute_layout(root_id, AvailableSpace {
    width: window_width,
    height: window_height,
});

// Get computed bounds for a node
let layout = render_tree.get_layout(node_id);
// Returns: Layout { x, y, width, height }
```

---

## Diff System

The diff system determines the minimum changes needed when UI structure updates.

### DivHash - Content-Based Identity

Every element has a hash computed from its properties:

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct DivHash(u64);

impl Div {
    // Hash of this element only (excludes children)
    fn compute_hash(&self) -> DivHash {
        let mut hasher = DefaultHasher::new();
        self.width.hash(&mut hasher);
        self.height.hash(&mut hasher);
        self.background.hash(&mut hasher);
        // ... all properties
        DivHash(hasher.finish())
    }

    // Hash including entire subtree
    fn compute_tree_hash(&self) -> DivHash {
        let mut hasher = DefaultHasher::new();
        self.compute_hash().hash(&mut hasher);
        for child in &self.children {
            child.compute_tree_hash().hash(&mut hasher);
        }
        DivHash(hasher.finish())
    }
}
```

### Change Categories

Changes are classified to determine the update strategy:

```rust
struct ChangeCategory {
    layout: bool,      // Requires layout recomputation
    visual: bool,      // Only visual properties changed
    children: bool,    // Children added/removed/reordered
    handlers: bool,    // Event handlers changed
}
```

| Category | Example Changes | Action |
|----------|-----------------|--------|
| Visual only | `bg`, `opacity`, `shadow` | Update RenderProps |
| Layout | `width`, `padding`, `gap` | Recompute layout |
| Children | Add/remove child | Rebuild subtree |
| Handlers | Event callback changed | Update handlers |

### Child Diffing Algorithm

When children change, the diff algorithm matches old and new children:

```rust
enum ChildDiff {
    Unchanged { index: usize },           // Same content, same position
    Moved { old_idx: usize, new_idx: usize },  // Same content, moved
    Modified { old_idx: usize, new_idx: usize, diff: Box<DiffResult> },
    Added { index: usize },               // New child
    Removed { index: usize },             // Old child gone
}
```

### Matching Strategy

1. **Compute hashes** for all old and new children
2. **Build hash map** of old children by hash
3. **Match new children** to old by hash lookup
4. **Detect moves** when hash matches at different position
5. **Classify remaining** as added or removed
6. **Merge same-position changes** as modifications

```rust
fn diff_children(old: &[Div], new: &[Div]) -> Vec<ChildDiff> {
    let old_hashes: Vec<_> = old.iter().map(|c| c.compute_tree_hash()).collect();
    let new_hashes: Vec<_> = new.iter().map(|c| c.compute_tree_hash()).collect();

    let mut old_by_hash: HashMap<DivHash, usize> = old_hashes
        .iter()
        .enumerate()
        .map(|(i, h)| (*h, i))
        .collect();

    let mut diffs = Vec::new();

    for (new_idx, new_hash) in new_hashes.iter().enumerate() {
        if let Some(old_idx) = old_by_hash.remove(new_hash) {
            if old_idx == new_idx {
                diffs.push(ChildDiff::Unchanged { index: new_idx });
            } else {
                diffs.push(ChildDiff::Moved { old_idx, new_idx });
            }
        } else {
            diffs.push(ChildDiff::Added { index: new_idx });
        }
    }

    // Remaining old children were removed
    for (old_idx, _) in old_by_hash {
        diffs.push(ChildDiff::Removed { index: old_idx });
    }

    diffs
}
```

---

## Incremental Updates

### Update Result

The incremental update process returns what changed:

```rust
enum UpdateResult {
    NoChanges,                    // Nothing to do
    VisualOnly(Vec<PropUpdate>),  // Apply prop updates only
    LayoutChanged,                // Props + recompute layout
    ChildrenChanged,              // Rebuild subtrees + layout
}
```

### Reconciliation

The `reconcile` function determines actions from diffs:

```rust
struct ReconcileActions {
    prop_updates: Vec<(NodeId, RenderProps)>,  // Visual updates
    subtree_rebuild_ids: Vec<NodeId>,          // Nodes to rebuild
    needs_layout: bool,                        // Layout recomputation needed
}

fn reconcile(old: &Div, new: &Div) -> ReconcileActions {
    let changes = categorize_changes(old, new);

    let mut actions = ReconcileActions::default();

    if changes.visual && !changes.layout && !changes.children {
        // Visual-only: just update props
        actions.prop_updates.push((node_id, new.to_render_props()));
    }

    if changes.layout {
        actions.needs_layout = true;
    }

    if changes.children {
        actions.subtree_rebuild_ids.push(node_id);
        actions.needs_layout = true;
    }

    actions
}
```

### Update Flow

```
incremental_update(root, new_element)
    │
    ├── Compare hashes: old_tree_hash vs new_tree_hash
    │   └── Same? Return NoChanges
    │
    ├── Compare element hashes: old_hash vs new_hash
    │   └── Same? Children might have changed
    │
    ├── Categorize changes
    │   ├── Visual only? Queue prop updates
    │   ├── Layout changed? Mark layout dirty
    │   └── Children changed? Diff children recursively
    │
    └── Return UpdateResult
```

---

## Pending Update Queues

The system uses global queues for deferred updates:

```rust
// Global pending updates (thread-local)
static PENDING_PROP_UPDATES: RefCell<Vec<(NodeId, RenderProps)>>;
static PENDING_SUBTREE_REBUILDS: RefCell<Vec<NodeId>>;
static NEEDS_REDRAW: AtomicBool;
```

### Queue Processing

The windowed app processes queues each frame:

```rust
fn process_pending_updates(&mut self) {
    // Apply prop updates (no layout needed)
    for (node_id, props) in drain_prop_updates() {
        self.render_tree.update_props(node_id, props);
    }

    // Rebuild dirty subtrees
    for node_id in drain_subtree_rebuilds() {
        self.rebuild_subtree(node_id);
    }

    // Recompute layout if needed
    if self.layout_dirty {
        self.render_tree.compute_layout(self.root_id, self.available_space);
        self.layout_dirty = false;
    }

    // Trigger redraw if visual changes occurred
    if NEEDS_REDRAW.swap(false, Ordering::SeqCst) {
        self.request_redraw();
    }
}
```

---

## Performance Benefits

### Hash-Based Comparison

- O(1) per element to compute hash
- O(1) equality check via hash comparison
- No deep property-by-property comparison needed

### Child Matching

- O(n) to build hash map
- O(n) to match children
- Detects moves without position-based assumptions

### Minimal Recomputation

| Scenario | What Runs |
|----------|-----------|
| Hover color change | Update 1 RenderProps |
| Text content change | Rebuild 1 text node |
| Add item to list | Insert node, layout affected subtree |
| Reorder list | Move nodes, minimal layout |

### Layout Caching

- Layout only recomputes when structure/dimensions change
- Visual-only changes skip layout entirely
- Taffy caches intermediate results
