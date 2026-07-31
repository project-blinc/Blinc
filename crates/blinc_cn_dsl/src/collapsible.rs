//! `cn.Collapsible` — a section that opens and closes from a bound
//! signal.

use std::cell::{OnceCell, RefCell};

use blinc_dsl_core::{Reactive, extern_widget};
use blinc_layout::div::{ElementBuilder, div};

/// `cn.Collapsible(open = Panel.expanded) { … }` — content that folds
/// away, driven by a signal rather than by an internal toggle.
///
/// ```dsl,ignore
/// signal shown: bool = false
///
/// cn.Switch(checked = shown, label = "details")
/// cn.Collapsible(open = shown) {
///     cn.Label("only visible when shown is true")
/// }
/// ```
///
/// `open` binds two ways, the same as `cn.Switch`'s `checked`: writing
/// the signal folds the section, and the section reflects whatever the
/// signal holds. A literal `true` / `false` pins it open or shut.
///
/// The body stays mounted either way — closing animates scale and
/// opacity rather than unmounting — so anything mid-flight inside
/// survives a fold. That also means the body is built once, which is
/// what lets it be handed to the cn builder here.
///
/// The trigger is deliberately not part of this: pair it with whatever
/// control owns the signal (a `cn.Switch`, a `cn.Button` writing it in
/// an `on_click`), rather than baking one shape in.
#[extern_widget(namespace = "cn", name = "Collapsible")]
pub struct CnCollapsible {
    /// Open when true. Bind a `signal` for a section that folds.
    pub open: Reactive<bool>,
    #[children]
    pub children: RefCell<Vec<Box<dyn ElementBuilder>>>,
    /// Built once, consuming `children`.
    #[skip]
    shell: OnceCell<blinc_cn::Collapsible>,
}

impl CnCollapsible {
    fn get_or_build(&self) -> &blinc_cn::Collapsible {
        self.shell.get_or_init(|| {
            let state = crate::bridge::bool_state(&self.open);

            // One content element holding the whole body, same as the
            // other containers.
            let mut content = div().flex_col().w_full();
            for child in self.children.borrow_mut().drain(..) {
                content = content.child_box(child);
            }

            // `content` takes a closure so a caller can rebuild per
            // render, but `build_collapsible` calls it exactly once —
            // open and closed are a scale/opacity animation over
            // mounted content, not a remount. So the body is handed
            // over through a cell it can be taken from.
            //
            // A second call yields an empty div rather than panicking:
            // if that invariant ever changes, a collapsible should lose
            // its content, not take the process down.
            let content = RefCell::new(Some(content));
            blinc_cn::collapsible(&state)
                .content(move || content.borrow_mut().take().unwrap_or_else(div))
                .build_collapsible()
        })
    }
}

impl ElementBuilder for CnCollapsible {
    fn build(&self, tree: &mut blinc_layout::LayoutTree) -> blinc_layout::LayoutNodeId {
        self.get_or_build().build(tree)
    }

    fn render_props(&self) -> blinc_layout::RenderProps {
        self.get_or_build().render_props()
    }

    /// The shell's children, which now hold the body.
    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().children_builders()
    }

    // MUST forward — see `gotcha_element_builder_trait_forwarding`.
    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        self.get_or_build().event_handlers()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.get_or_build().element_classes()
    }

    fn element_id(&self) -> Option<&str> {
        self.get_or_build().element_id()
    }

    fn element_type_id(&self) -> blinc_layout::div::ElementTypeId {
        self.get_or_build().element_type_id()
    }
}
