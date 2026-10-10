//! Children that come and go in place: a list from an iterator, or a branch
//! shown while a condition holds.
//!
//! A region is a run of rows among a `Div`'s children. A row is built from an
//! item when its key first appears, under its own [`Owner`], and torn down when
//! the key goes: what it created is disposed with it. A key that stays keeps
//! its node. The `Div` itself is the container, so the rows lay out as its own
//! children, with its direction, gap and alignment.
//!
//! A row can stay mounted for a while after its key goes (see [`Row::on_leave`]),
//! so it can animate out. It keeps its node, its place among the rows and its
//! scope until it says it is done.
//!
//! The definition lives on the thread that built it, keyed by the node that
//! holds the region, and is looked up when the source changes.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use blinc_core::owner::Owner;
use blinc_core::reactive::{Computed, DerivedId, ReactiveGraph, computed, dispose_derived};

use crate::div::ElementBuilder;
use crate::tree::LayoutNodeId;

/// What an item shows: an element, and optionally what happens when its key
/// goes.
///
/// Anything that is an element converts to a row that goes at once.
pub struct Row {
    element: Box<dyn ElementBuilder>,
    exit: Option<Exit>,
}

struct Exit {
    start: Box<dyn FnOnce()>,
    done: Box<dyn Fn() -> bool>,
}

impl Row {
    pub fn new(element: impl ElementBuilder + 'static) -> Self {
        Self {
            element: Box::new(element),
            exit: None,
        }
    }

    /// The element, for a row that is not going to leave.
    pub(crate) fn into_element(self) -> Box<dyn ElementBuilder> {
        self.element
    }

    /// Keep the row mounted after its key goes.
    ///
    /// `start` runs once, when the key goes. The row then stays where it is,
    /// with everything it owns, until `done` returns true; `done` is asked
    /// once per frame and must come true eventually, or the row stays for as
    /// long as the element does. If the key comes back meanwhile, the item is
    /// built afresh as a new row and the old one still finishes leaving.
    pub fn on_leave(
        mut self,
        start: impl FnOnce() + 'static,
        done: impl Fn() -> bool + 'static,
    ) -> Self {
        self.exit = Some(Exit {
            start: Box::new(start),
            done: Box::new(done),
        });
        self
    }
}

impl<E: ElementBuilder + 'static> From<E> for Row {
    fn from(element: E) -> Self {
        Row::new(element)
    }
}

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
    /// Rows whose key has gone but that stay mounted until they are done.
    /// They are not among `rows`.
    pub lingering: Vec<RowId>,
    /// Whether a row began to leave in this evaluation, so its `start` has
    /// run and may have queued writes.
    pub started: bool,
}

pub(crate) trait RegionLogic {
    /// Read the source now and plan the rows.
    fn evaluate(&mut self) -> Evaluation;
    /// A removed row's nodes are gone: dispose what it created.
    fn dispose_row(&mut self, id: RowId);
    /// Whether a row is still leaving, which has to be asked about again.
    fn is_leaving(&self) -> bool;
}

/// The row an item shows; `None` for nothing.
type Item<T> = Box<dyn Fn(T) -> Option<Row>>;

struct Mounted {
    id: RowId,
    owner: Owner,
    exit: Option<Exit>,
}

/// A row whose key has gone, waiting to be done.
struct Lingering {
    id: RowId,
    owner: Owner,
    done: Box<dyn Fn() -> bool>,
}

/// A list: the rows are the items, matched by key.
pub(crate) struct ForRegion<T, K> {
    each: Computed<Vec<T>>,
    /// `each`, when the list made it itself and so disposes it, rather than
    /// being handed one to read.
    owned_each: Option<DerivedId>,
    key: Box<dyn Fn(&T) -> K>,
    item: Item<T>,
    rows: HashMap<K, Mounted>,
    lingering: Vec<Lingering>,
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
        each: Computed<Vec<T>>,
        owns_each: bool,
        key: impl Fn(&T) -> K + 'static,
        item: impl Fn(T) -> Option<Row> + 'static,
    ) -> Self {
        Self {
            owned_each: owns_each.then(|| each.derived_id()),
            each,
            key: Box::new(key),
            item: Box::new(item),
            rows: HashMap::new(),
            lingering: Vec::new(),
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
            let (node, exit) = match built {
                Some(Row { element, exit }) => (RowNode::Build(element), exit),
                None => (RowNode::Empty, None),
            };
            self.rows.insert(key, Mounted { id, owner, exit });
            rows.push(RowPlan { id, node });
        }

        let gone: Vec<K> = self
            .rows
            .keys()
            .filter(|key| !seen.contains(*key))
            .cloned()
            .collect();
        let mut removed = Vec::with_capacity(gone.len());
        let mut started = false;
        // Rows already waiting are asked before the ones that start now, so
        // a row is never done in the pass that starts it.
        let (finished, waiting): (Vec<Lingering>, Vec<Lingering>) =
            std::mem::take(&mut self.lingering)
                .into_iter()
                .partition(|row| (row.done)());
        self.lingering = waiting;
        for row in finished {
            removed.push(row.id);
            self.leaving.insert(row.id, row.owner);
        }
        for key in gone {
            let Some(row) = self.rows.remove(&key) else {
                continue;
            };
            match row.exit {
                Some(Exit { start, done }) => {
                    start();
                    started = true;
                    self.lingering.push(Lingering {
                        id: row.id,
                        owner: row.owner,
                        done,
                    });
                }
                None => {
                    removed.push(row.id);
                    self.leaving.insert(row.id, row.owner);
                }
            }
        }
        let lingering = self.lingering.iter().map(|row| row.id).collect();
        Evaluation {
            rows,
            removed,
            lingering,
            started,
        }
    }

    fn dispose_row(&mut self, id: RowId) {
        if let Some(owner) = self.leaving.remove(&id) {
            owner.dispose();
        }
    }

    fn is_leaving(&self) -> bool {
        !self.lingering.is_empty()
    }
}

impl<T, K> Drop for ForRegion<T, K> {
    fn drop(&mut self) {
        if let Some(id) = self.owned_each {
            dispose_derived(id);
        }
        for row in self.rows.values() {
            row.owner.dispose();
        }
        for row in &self.lingering {
            row.owner.dispose();
        }
        for owner in self.leaving.values() {
            owner.dispose();
        }
    }
}

/// The computed a list reads its items from, and whether it was made here.
///
/// A computed is read as it is. A state or a plain list gets a computed of
/// its own, which the list disposes with itself.
pub(crate) fn source_of<T>(items: crate::binding::Reactive<Vec<T>>) -> (Computed<Vec<T>>, bool)
where
    T: Clone + Send + 'static,
{
    use crate::binding::Reactive;
    match items {
        Reactive::Computed(computed) => (computed, false),
        Reactive::Bound(state) => {
            let signal = state.signal();
            (
                computed(move |graph: &ReactiveGraph| graph.get(signal).unwrap_or_default()),
                true,
            )
        }
        Reactive::Const(list) => (computed(move |_: &ReactiveGraph| list.clone()), true),
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
