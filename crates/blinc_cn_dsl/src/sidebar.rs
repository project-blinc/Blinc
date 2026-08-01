//! `cn.Sidebar` — collapsible navigation with an optional content area.

use std::cell::OnceCell;
use std::sync::Mutex;

use blinc_dsl_core::{Reactive, extern_widget};
use blinc_layout::div::ElementBuilder;

use crate::sidebar_content::CnSidebarContent;
use crate::sidebar_item::{CnSidebarItem, ICON_SIZE};
use crate::sidebar_section::CnSidebarSection;

/// `cn.Sidebar(collapsed = shut) { …items, sections, content… }` — a
/// navigation rail that animates between icon-only and full width.
///
/// ```dsl,ignore
/// cn.Sidebar(collapsed = shut) {
///     cn.SidebarSection(title = "Widgets") {
///         cn.SidebarItem(label = "Forms", icon = "square-pen", active = true,
///                        on_click = || page.set(0))
///         cn.SidebarItem(label = "Feedback", icon = "bell",
///                        on_click = || page.set(1))
///     }
///     cn.SidebarContent {
///         with {
///             if page.get() == 0 { FormWidgets() } else { FeedbackWidgets() }
///         }
///     }
/// }
/// ```
///
/// Three kinds of child, each read through `as_any`: an item is a row, a
/// section is a titled group of rows, and content fills the area beside
/// them. Anything else is dropped with a warning, since the widget draws
/// its own rows and has nowhere to put a loose element.
///
/// Items outside any section land in an untitled group above the first
/// one, so a short sidebar needs no sections at all.
///
/// The widget tracks which row is selected itself and highlights it, so
/// an item's `on_click` only has to record where the app now is. That is
/// also why `active` is an initial value rather than a binding.
#[extern_widget(namespace = "cn", name = "Sidebar")]
pub struct CnSidebar {
    /// Whether the rail is collapsed to icons.
    ///
    /// Bind a signal to drive it from elsewhere and to observe the
    /// toggle; the widget writes back through the same state. A literal
    /// is an initial value, kept per instance across rebuilds.
    pub collapsed: Reactive<bool>,
    /// Width when expanded. Omitted keeps the cn default.
    pub expanded_w: f64,
    /// Width when collapsed. Omitted keeps the cn default.
    pub collapsed_w: f64,
    /// Hide the collapse toggle, for when something else drives
    /// `collapsed`. Named for what it turns off because an omitted bool
    /// prop reads as `false`, so a `toggle` that defaulted to shown
    /// could never be switched off.
    pub hide_toggle: bool,
    #[children]
    pub children: Mutex<Vec<Box<dyn ElementBuilder>>>,
    /// Built once, consuming `children`.
    #[skip]
    shell: OnceCell<blinc_cn::SidebarBuilder>,
}

impl CnSidebar {
    fn get_or_build(&self) -> &blinc_cn::SidebarBuilder {
        // Built outside the cell: the widget runs its stateful body
        // during construction, which re-enters here.
        if let Some(built) = self.shell.get() {
            return built;
        }
        let built = self.make();
        let _ = self.shell.set(built);
        self.shell.get().expect("just set")
    }

    fn make(&self) -> blinc_cn::SidebarBuilder {
        let children = std::mem::take(&mut *self.children.lock().expect("children mutex"));

        let collapsed = crate::bridge::bool_state(&self.collapsed);
        let mut b = blinc_cn::sidebar(&collapsed);
        if self.expanded_w > 0.0 {
            b = b.expanded_width(self.expanded_w as f32);
        }
        if self.collapsed_w > 0.0 {
            b = b.collapsed_width(self.collapsed_w as f32);
        }
        if self.hide_toggle {
            b = b.show_toggle(false);
        }

        for child in children {
            let Some(any) = child.as_any() else {
                warn_loose_child();
                continue;
            };
            if let Some(item) = any.downcast_ref::<CnSidebarItem>() {
                b = add_item(b, item);
            } else if let Some(section) = any.downcast_ref::<CnSidebarSection>() {
                // An untitled section still separates the group from
                // what came before, which is what `section_untitled` is
                // for.
                b = match section.title.is_empty() {
                    true => b.section_untitled(),
                    false => b.section(section.title.clone()),
                };
                for row in section.take_children() {
                    match row
                        .as_any()
                        .and_then(|any| any.downcast_ref::<CnSidebarItem>())
                    {
                        Some(item) => b = add_item(b, item),
                        None => tracing::warn!(
                            "cn.SidebarSection: child is not a cn.SidebarItem — dropped",
                        ),
                    }
                }
            } else if let Some(content) = any.downcast_ref::<CnSidebarContent>() {
                b = b.content(crate::shared_child::filling_body_recipe(
                    content.take_children(),
                ));
            } else {
                warn_loose_child();
            }
        }
        b
    }
}

fn warn_loose_child() {
    tracing::warn!(
        "cn.Sidebar: child is not a cn.SidebarItem, cn.SidebarSection or \
         cn.SidebarContent — dropped",
    );
}

fn add_item(b: blinc_cn::SidebarBuilder, item: &CnSidebarItem) -> blinc_cn::SidebarBuilder {
    let icon = crate::icon::resolve("cn.SidebarItem", &item.icon, ICON_SIZE).unwrap_or_default();
    let on_click = item.click_handler();
    match item.active {
        true => b.item_active(item.label.clone(), icon, on_click),
        false => b.item(item.label.clone(), icon, on_click),
    }
}

impl ElementBuilder for CnSidebar {
    fn build(&self, tree: &mut blinc_layout::LayoutTree) -> blinc_layout::LayoutNodeId {
        self.get_or_build().build(tree)
    }

    fn render_props(&self) -> blinc_layout::RenderProps {
        self.get_or_build().render_props()
    }

    /// The shell's children, which now hold the rows and the content.
    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().children_builders()
    }

    // MUST forward — see `gotcha_element_builder_trait_forwarding`.
    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        self.get_or_build().event_handlers()
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.get_or_build().layout_style()
    }

    fn visual_animation_config(
        &self,
    ) -> Option<blinc_layout::visual_animation::VisualAnimationConfig> {
        self.get_or_build().visual_animation_config()
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
