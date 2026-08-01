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

        impl ElementBuilder for $ty {
            /// Only reached outside a popover, where the slot has
            /// nothing to belong to.
            fn build(&self, tree: &mut blinc_layout::LayoutTree) -> blinc_layout::LayoutNodeId {
                tracing::warn!(concat!($name, " outside a cn.Popover — rendering inline"));
                let mut d = div().flex_col();
                for child in self.take_children() {
                    d = d.child_box(child);
                }
                d.build(tree)
            }

            fn render_props(&self) -> blinc_layout::RenderProps {
                blinc_layout::RenderProps::default()
            }

            fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
                &[]
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
