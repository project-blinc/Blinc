//! `cn.PopoverTrigger` / `cn.PopoverContent` — the two halves of a
//! popover, each naming which it is.

use std::sync::Mutex;

use blinc_dsl_core::extern_widget;
use blinc_layout::div::{ElementBuilder, div};

/// `cn.PopoverTrigger { … }` — what opens the popover.
///
/// Read by [`crate::popover::CnPopover`] through `as_any`; rendering it
/// anywhere else shows the body plainly, since there is no popover for
/// it to open.
#[extern_widget(namespace = "cn", name = "PopoverTrigger")]
pub struct CnPopoverTrigger {
    #[children]
    pub children: Mutex<Vec<Box<dyn ElementBuilder>>>,
    /// Only built when the slot renders outside its owner.
    #[skip]
    fallback: std::cell::OnceCell<blinc_layout::div::Div>,
}

/// `cn.PopoverContent { … }` — what the popover shows.
///
/// Read by [`crate::popover::CnPopover`] through `as_any`. Outside one
/// it renders inline, which is what a panel with nothing to open it
/// amounts to.
#[extern_widget(namespace = "cn", name = "PopoverContent")]
pub struct CnPopoverContent {
    #[children]
    pub children: Mutex<Vec<Box<dyn ElementBuilder>>>,
    /// Only built when the slot renders outside its owner.
    #[skip]
    fallback: std::cell::OnceCell<blinc_layout::div::Div>,
}

macro_rules! slot_widget {
    ($ty:ident, $name:literal) => {
        impl $ty {
            /// Take the body, leaving the slot empty. The popover calls
            /// this once while building.
            pub(crate) fn take_children(&self) -> Vec<Box<dyn ElementBuilder>> {
                std::mem::take(&mut *self.children.lock().expect("children mutex"))
            }
        }

        impl $ty {
            /// The inline fallback, built once. `RenderTree` recurses
            /// through `children_builders`, never `build`, so the body
            /// must come from a cached element the slot borrows from —
            /// reporting no children drops the subtree silently.
            fn get_or_build(&self) -> &blinc_layout::div::Div {
                ::blinc_layout::build_once::build_once(&self.fallback, || {
                    tracing::warn!(concat!($name, " outside a cn.Popover — rendering inline"));
                    let mut d = div().flex_col();
                    for child in self.take_children() {
                        d = d.child_box(child);
                    }
                    d
                })
            }
        }

        impl ElementBuilder for $ty {
            /// Only reached outside a popover, where the slot has
            /// nothing to belong to.
            fn build(&self, tree: &mut blinc_layout::LayoutTree) -> blinc_layout::LayoutNodeId {
                self.get_or_build().build(tree)
            }

            fn render_props(&self) -> blinc_layout::RenderProps {
                self.get_or_build().render_props()
            }

            fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
                self.get_or_build().children_builders()
            }

            /// What lets `cn.Popover` tell the two slots apart.
            fn as_any(&self) -> Option<&dyn std::any::Any> {
                Some(self)
            }
        }
    };
}

slot_widget!(CnPopoverTrigger, "cn.PopoverTrigger");
slot_widget!(CnPopoverContent, "cn.PopoverContent");
