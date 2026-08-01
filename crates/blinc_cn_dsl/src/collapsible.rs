//! `cn.Collapsible` — a section that folds, driven by a bound signal.

use std::cell::OnceCell;
use std::sync::Mutex;

use blinc_dsl_core::{Reactive, extern_widget};
use blinc_layout::div::ElementBuilder;

/// `cn.Collapsible(open = shown) { … }` — content that folds away.
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
/// `open` binds two ways, the same as `cn.Switch`'s `checked`. The
/// widget rebuilds itself, so nothing is asked of the caller.
///
/// The body reaches the widget through [`crate::shared_child::body_recipe`]:
/// `cn::collapsible` rebuilds its subtree and so takes content as a
/// recipe, which a DSL body cannot be as it arrives.
#[extern_widget(namespace = "cn", name = "Collapsible")]
pub struct CnCollapsible {
    /// Open when true. Bind a `signal` for a section that folds.
    pub open: Reactive<bool>,
    #[children]
    pub children: Mutex<Vec<Box<dyn ElementBuilder>>>,
    /// Built once, consuming `children`.
    #[skip]
    shell: OnceCell<blinc_cn::CollapsibleBuilder>,
}

impl CnCollapsible {
    fn get_or_build(&self) -> &blinc_cn::CollapsibleBuilder {
        self.shell.get_or_init(|| {
            let state = crate::bridge::bool_state(&self.open);
            let children = std::mem::take(&mut *self.children.lock().expect("children mutex"));
            blinc_cn::collapsible(&state).content(crate::shared_child::body_recipe(children))
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
