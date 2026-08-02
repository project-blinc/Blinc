//! `cn.Tabs` — a strip of labelled panels, one showing at a time.

use std::cell::OnceCell;
use std::sync::Mutex;

use blinc_dsl_core::{Reactive, extern_widget};
use blinc_layout::div::ElementBuilder;

use crate::tab::CnTab;

/// `cn.Tabs(value = signal) { cn.Tab(label = "…") { … } }` — panels that
/// swap, with the strip drawn for you.
///
/// ```dsl,ignore
/// signal section: string = "account"
///
/// cn.Tabs(value = section, size = "small") {
///     cn.Tab(value = "account", label = "Account", icon = "user") {
///         Div { Text("who you are") }
///     }
///     cn.Tab(value = "alerts", label = "Alerts", badge = "3") {
///         Div { Text("what we sent you") }
///     }
/// }
/// ```
///
/// `value` binds both ways: clicking a tab writes the signal, and
/// writing the signal moves the strip. Each tab names itself, so the
/// widget reads its children rather than being told about them
/// separately. A child that is not a `cn.Tab` is dropped with a warning:
/// tabs draw their own strip and have nowhere to put a loose element.
///
/// Panels are handed over as recipes rather than as built elements,
/// because the widget rebuilds one whenever it comes to the front. See
/// `crate::shared_child` for what that costs and why it is sound.
#[extern_widget(namespace = "cn", name = "Tabs")]
pub struct CnTabs {
    /// Which tab is showing. Bind a signal to drive it from elsewhere.
    pub value: Reactive<String>,
    /// `small` / `medium` (default) / `large`.
    pub size: String,
    /// How a panel arrives: `none` / `fade` (default) / `slide_left` /
    /// `slide_right` / `slide_up` / `slide_down`.
    pub transition: String,
    #[children]
    pub children: Mutex<Vec<Box<dyn ElementBuilder>>>,
    /// Built once, consuming `children`.
    #[skip]
    shell: OnceCell<blinc_cn::TabsBuilder>,
}

impl CnTabs {
    fn get_or_build(&self) -> &blinc_cn::TabsBuilder {
        ::blinc_layout::build_once::build_once(&self.shell, || self.make())
    }

    fn make(&self) -> blinc_cn::TabsBuilder {
        let children = std::mem::take(&mut *self.children.lock().expect("children mutex"));
        let state = crate::bridge::string_state(&self.value);

        let mut b = blinc_cn::tabs(&state)
            .size(self.size())
            .transition(self.transition());
        // The first tab is what shows when the bound value names none of
        // them, which includes the common case of an unset signal.
        let mut first = None;

        for child in children {
            let Some(tab) = child.as_any().and_then(|any| any.downcast_ref::<CnTab>()) else {
                tracing::warn!(
                    "cn.Tabs: child is not a cn.Tab — dropped; wrap it in \
                     cn.Tab(label = \"…\") to give it a place in the strip",
                );
                continue;
            };
            first.get_or_insert_with(|| tab.tab_value());
            let panel = crate::shared_child::body_recipe(tab.take_children());
            b = b.tab_item(tab.menu_item(), panel);
        }

        if let Some(value) = first {
            b = b.default_value(value);
        }
        b
    }

    fn size(&self) -> blinc_cn::TabsSize {
        match self.size.as_str() {
            "small" | "sm" => blinc_cn::TabsSize::Small,
            "large" | "lg" => blinc_cn::TabsSize::Large,
            _ => blinc_cn::TabsSize::Medium,
        }
    }

    fn transition(&self) -> blinc_cn::TabsTransition {
        use blinc_cn::TabsTransition as T;
        match self.transition.as_str() {
            "" | "fade" => T::Fade,
            "none" => T::None,
            "slide_left" => T::SlideLeft,
            "slide_right" => T::SlideRight,
            "slide_up" => T::SlideUp,
            "slide_down" => T::SlideDown,
            other => {
                tracing::warn!(transition = %other, "cn.Tabs: unknown transition");
                T::Fade
            }
        }
    }
}

impl ElementBuilder for CnTabs {
    fn build(&self, tree: &mut blinc_layout::LayoutTree) -> blinc_layout::LayoutNodeId {
        self.get_or_build().build(tree)
    }

    fn render_props(&self) -> blinc_layout::RenderProps {
        self.get_or_build().render_props()
    }

    /// The shell's children, which now hold the strip and the panel.
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
