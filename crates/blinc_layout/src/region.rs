//! Children that come and go in place: a list from an iterator, or a branch
//! shown while a condition holds.
//!
//! A region is a run of rows among a `Div`'s children. A row is built from an
//! item when its key first appears, under its own [`Owner`], and torn down when
//! the key goes: what it created is disposed with it. A key that stays keeps
//! its node. The `Div` itself is the container, so the rows lay out as its own
//! children, with its direction, gap and alignment.
//!
//! The definition lives on the thread that built it, keyed by the node that
//! holds the region, and is looked up when the source changes.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use blinc_core::owner::Owner;
use blinc_core::reactive::{Computed, ReactiveGraph, computed};

use crate::div::ElementBuilder;
use crate::tree::LayoutNodeId;

pub(crate) type RowId = u64;

/// What a row is on this evaluation.
pub(crate) enum RowNode {
    /// Built before and still shown: keep its node.
    Keep,
    /// New: build this.
    Build(Box<dyn ElementBuilder>),
    /// A row that has no element (a branch with nothing to show).
    Empty,
}

pub(crate) struct RowPlan {
    pub id: RowId,
    pub node: RowNode,
}

pub(crate) struct Evaluation {
    /// The rows now, in order.
    pub rows: Vec<RowPlan>,
    /// Rows that were there and no longer are. Their scopes are still alive:
    /// the tree tears the nodes down, then calls `dispose_row`.
    pub removed: Vec<RowId>,
}

pub(crate) trait RegionLogic {
    /// Read the source now and plan the rows.
    fn evaluate(&mut self) -> Evaluation;
    /// A removed row's nodes are gone: dispose what it created.
    fn dispose_row(&mut self, id: RowId);
}

/// The element a row shows for an item; `None` for nothing.
type Item<T> = Box<dyn Fn(T) -> Option<Box<dyn ElementBuilder>>>;

struct Row {
    id: RowId,
    owner: Owner,
}

/// A list: the rows are the items, matched by key.
pub(crate) struct ForRegion<T, K> {
    each: Computed<Vec<T>>,
    key: Box<dyn Fn(&T) -> K>,
    item: Item<T>,
    rows: HashMap<K, Row>,
    /// Rows taken out of the list, waiting for their nodes to be torn down.
    leaving: HashMap<RowId, Owner>,
    next_id: RowId,
}

impl<T, K> ForRegion<T, K>
where
    T: Clone + Send + 'static,
    K: Hash + Eq + Clone + 'static,
{
    pub(crate) fn new(
        each: impl Fn(&ReactiveGraph) -> Vec<T> + Send + 'static,
        key: impl Fn(&T) -> K + 'static,
        item: impl Fn(T) -> Option<Box<dyn ElementBuilder>> + 'static,
    ) -> Self {
        Self {
            each: computed(each),
            key: Box::new(key),
            item: Box::new(item),
            rows: HashMap::new(),
            leaving: HashMap::new(),
            next_id: 0,
        }
    }

    /// What the source reads, to subscribe the node to it.
    pub(crate) fn source(&self) -> Computed<Vec<T>> {
        self.each.clone()
    }
}

impl<T, K> RegionLogic for ForRegion<T, K>
where
    T: Clone + Send + 'static,
    K: Hash + Eq + Clone + 'static,
{
    fn evaluate(&mut self) -> Evaluation {
        let list = self.each.try_get().unwrap_or_default();
        let mut seen: HashSet<K> = HashSet::new();
        let mut rows = Vec::with_capacity(list.len());
        for value in list {
            let key = (self.key)(&value);
            if !seen.insert(key.clone()) {
                tracing::warn!("a list has two items with one key; the later is skipped");
                continue;
            }
            if let Some(row) = self.rows.get(&key) {
                rows.push(RowPlan {
                    id: row.id,
                    node: RowNode::Keep,
                });
                continue;
            }
            let owner = Owner::new();
            let built = owner.run(|| (self.item)(value));
            let id = self.next_id;
            self.next_id += 1;
            self.rows.insert(key, Row { id, owner });
            rows.push(RowPlan {
                id,
                node: match built {
                    Some(builder) => RowNode::Build(builder),
                    None => RowNode::Empty,
                },
            });
        }

        let gone: Vec<K> = self
            .rows
            .keys()
            .filter(|key| !seen.contains(*key))
            .cloned()
            .collect();
        let mut removed = Vec::with_capacity(gone.len());
        for key in gone {
            if let Some(row) = self.rows.remove(&key) {
                removed.push(row.id);
                self.leaving.insert(row.id, row.owner);
            }
        }
        Evaluation { rows, removed }
    }

    fn dispose_row(&mut self, id: RowId) {
        if let Some(owner) = self.leaving.remove(&id) {
            owner.dispose();
        }
    }
}

impl<T, K> Drop for ForRegion<T, K> {
    fn drop(&mut self) {
        for row in self.rows.values() {
            row.owner.dispose();
        }
        for owner in self.leaving.values() {
            owner.dispose();
        }
    }
}

// ---------------------------------------------------------------------------
// Where a built region lives
// ---------------------------------------------------------------------------

static NEXT_REGION: AtomicU64 = AtomicU64::new(1);

pub(crate) fn next_region_id() -> u64 {
    NEXT_REGION.fetch_add(1, Ordering::Relaxed)
}

/// A region as built into a node: its logic, and which node each row is.
pub(crate) struct Region {
    pub id: u64,
    pub logic: RefCell<Box<dyn RegionLogic>>,
    /// How many of the node's children come before the first row.
    pub start: usize,
    /// The rows in order, with the node each one has.
    pub live: RefCell<Vec<(RowId, Option<LayoutNodeId>)>>,
}

thread_local! {
    static REGIONS: RefCell<HashMap<LayoutNodeId, Rc<Region>>> = RefCell::new(HashMap::new());
    static ROW_KEYS: RefCell<HashMap<LayoutNodeId, String>> = RefCell::new(HashMap::new());
}

/// Record the region a node holds.
pub(crate) fn register(node: LayoutNodeId, region: Rc<Region>) {
    REGIONS.with(|regions| regions.borrow_mut().insert(node, region));
}

/// The region a node holds, if it is the one with this id.
pub(crate) fn region_of(node: LayoutNodeId, id: u64) -> Option<Rc<Region>> {
    REGIONS.with(|regions| {
        regions
            .borrow()
            .get(&node)
            .filter(|region| region.id == id)
            .cloned()
    })
}

/// The node is gone: forget its region and its row key.
pub(crate) fn forget(node: LayoutNodeId) {
    REGIONS.with(|regions| regions.borrow_mut().remove(&node));
    ROW_KEYS.with(|keys| keys.borrow_mut().remove(&node));
}

/// Name a row's node, so its stable id follows the row, not its position.
pub(crate) fn set_row_key(node: LayoutNodeId, region: u64, row: RowId) {
    ROW_KEYS.with(|keys| {
        keys.borrow_mut()
            .insert(node, format!("row:{region}:{row}"));
    });
}

pub(crate) fn row_key(node: LayoutNodeId) -> Option<String> {
    ROW_KEYS.with(|keys| keys.borrow().get(&node).cloned())
}
