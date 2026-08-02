//! Making an already-built DSL body deliverable to a widget that
//! rebuilds.
//!
//! A `Stateful` rebuilds its subtree, so a widget that owns one takes
//! its content as a recipe: `Fn() -> Div + Send + Sync`, called again on
//! every change. `cn::accordion` and `cn::collapsible` both work that
//! way.
//!
//! A DSL body cannot be that recipe as it arrives. `#[children]` hands
//! over `Box<dyn ElementBuilder>` values — already constructed, not
//! cloneable, and holding `Rc` event handlers, so they can neither be
//! rebuilt from nor captured by a `Send + Sync` closure.
//!
//! [`SharedChild`] closes that gap: an `Arc` around the built element
//! that clones cheaply, so each call of the recipe assembles a fresh
//! `Div` from the same children.

use std::sync::Arc;

use blinc_layout::div::{Div, ElementBuilder, div};

/// A built element that can be handed out more than once.
///
/// Cloning shares the element rather than copying it, which is what
/// lets a rebuild callback produce a new container around the same
/// body.
#[derive(Clone)]
pub struct SharedChild(Arc<dyn ElementBuilder>);

// SAFETY: the element holds `Rc` handlers, so it is neither `Send` nor
// `Sync` on its own. It never leaves the UI thread: the DSL runtime is
// single-threaded (the JIT is driven from one thread), and a `Stateful`
// callback runs during that thread's build. The bound exists because
// `Stateful` is generic over callbacks that COULD be shared, not
// because these are. Same argument as `JitGuardDispatcher` /
// `JitViewRenderer` in `blinc_dsl_core::runtime_bridge`.
unsafe impl Send for SharedChild {}
unsafe impl Sync for SharedChild {}

impl SharedChild {
    pub fn new(child: Box<dyn ElementBuilder>) -> Self {
        Self(Arc::from(child))
    }
}

impl ElementBuilder for SharedChild {
    fn build(&self, tree: &mut blinc_layout::LayoutTree) -> blinc_layout::LayoutNodeId {
        self.0.build(tree)
    }
    fn render_props(&self) -> blinc_layout::RenderProps {
        self.0.render_props()
    }
    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.0.children_builders()
    }
    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        self.0.event_handlers()
    }
    fn element_classes(&self) -> &[Arc<str>] {
        self.0.element_classes()
    }
    fn element_id(&self) -> Option<&str> {
        self.0.element_id()
    }
    fn element_type_id(&self) -> blinc_layout::div::ElementTypeId {
        self.0.element_type_id()
    }
}

/// Turn a DSL body into a recipe a rebuilding widget can call.
///
/// Each call assembles a fresh column around the same shared children,
/// which is what `Fn() -> Div` asks for.
pub fn body_recipe(children: Vec<Box<dyn ElementBuilder>>) -> impl Fn() -> Div + Send + Sync {
    let shared: Vec<SharedChild> = children.into_iter().map(SharedChild::new).collect();
    move || {
        // `items_start`: a column stretches its children across the
        // cross axis by default, which widened a badge in a sheet to
        // the whole panel. A content slot holds whatever the source put
        // in it, and a chip is not a banner — anything that wants the
        // full width says so, as the page classes do with `width: 100%`.
        let mut d = div().w_full().flex_col().items_start();
        for child in &shared {
            d = d.child(child.clone());
        }
        d
    }
}

/// Like [`body_recipe`], but the wrapper fills its parent and scrolls.
///
/// For a body that IS the area rather than sitting inside one. The
/// sidebar's content area is the only node in that chain with a definite
/// height — every wrapper between it and the page is auto-sized, so a
/// percentage height further down has nothing to resolve against and a
/// scroll container declared there can never be bounded.
///
/// Built here rather than left to a CSS class on the DSL side for the
/// same reason it has to be this node: the caller cannot reach it.
/// `scroll_ref` is the handle a `ref = …` prop carried in, or zero.
pub fn filling_body_recipe(
    children: Vec<Box<dyn ElementBuilder>>,
    scroll_ref: i64,
) -> impl Fn() -> Div + Send + Sync {
    let shared: Vec<SharedChild> = children.into_iter().map(SharedChild::new).collect();
    let handle = blinc_dsl_core::refs::scroll_ref_by_id(scroll_ref);
    move || {
        let mut d = div().w_full().h_full().flex_col().overflow_scroll();
        if let Some(handle) = &handle {
            d = d.bind_scroll(handle);
        }
        for child in &shared {
            d = d.child(child.clone());
        }
        d
    }
}
