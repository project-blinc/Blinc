//! `cn.DropdownMenu` — a menu hanging off a trigger.

use blinc_dsl_core::extern_widget;
use blinc_layout::div::ElementBuilder;
use std::cell::OnceCell;
use std::sync::Mutex;

use crate::menu_item::{CnMenuItem, CnMenuSeparator};

/// `cn.DropdownMenu(label = "File") { cn.MenuItem(…) }` — commands
/// under a button.
///
/// ```dsl,ignore
/// cn.DropdownMenu(label = "File", align = "start") {
///     cn.MenuItem(label = "New", shortcut = "⌘N", on_click = || make())
///     cn.MenuSeparator()
///     cn.MenuItem(label = "Quit", disabled = true)
/// }
/// ```
///
/// The rows are children, the shape `cn.ToggleGroup` and `cn.Select`
/// already use. What differs is the child type: a menu row is a COMMAND
/// rather than a value, so it is `cn.MenuItem` rather than `cn.Option`.
///
/// Submenus are not exposed yet. The cn builder takes one through a
/// nested closure, and the DSL equivalent is a `cn.Submenu` container
/// whose own children are rows — worth doing once the flat form has
/// been used in anger.
#[extern_widget(namespace = "cn", name = "DropdownMenu")]
pub struct CnDropdownMenu {
    /// Text on the trigger button.
    pub label: String,
    /// `bottom` (default) / `top` / `left` / `right`.
    pub position: String,
    /// `start` (default) / `center` / `end`, along the trigger's edge.
    pub align: String,
    /// Gap between trigger and panel, in pixels.
    pub offset: f64,
    /// Panel width floor, in pixels.
    pub min_width: f64,
    #[children]
    pub children: Mutex<Vec<Box<dyn ElementBuilder>>>,
    /// Built once, consuming `children`.
    #[skip]
    shell: OnceCell<blinc_cn::DropdownMenuBuilder>,
}

impl CnDropdownMenu {
    fn get_or_build(&self) -> &blinc_cn::DropdownMenuBuilder {
        ::blinc_layout::build_once::build_once(&self.shell, || self.make())
    }

    fn make(&self) -> blinc_cn::DropdownMenuBuilder {
        let children = std::mem::take(&mut *self.children.lock().expect("children mutex"));
        let mut b = blinc_cn::dropdown_menu(self.label.clone())
            .position(self.position())
            .align(self.align());
        if self.offset > 0.0 {
            b = b.offset(self.offset as f32);
        }
        if self.min_width > 0.0 {
            b = b.min_width(self.min_width as f32);
        }

        for child in children {
            let Some(any) = child.as_any() else {
                warn_loose_child();
                continue;
            };
            if any.downcast_ref::<CnMenuSeparator>().is_some() {
                b = b.separator();
            } else if let Some(item) = any.downcast_ref::<CnMenuItem>() {
                b = self.add_item(b, item);
            } else {
                warn_loose_child();
            }
        }
        b
    }

    /// One row, routed by what the author gave it.
    fn add_item(
        &self,
        b: blinc_cn::DropdownMenuBuilder,
        item: &CnMenuItem,
    ) -> blinc_cn::DropdownMenuBuilder {
        let label = item.item_label();
        // A disabled row runs nothing, so its handler is never wired.
        if item.disabled {
            return b.item_disabled(label);
        }
        let Some(handler) = item.handler() else {
            // No handler is still a row: it draws, and choosing it just
            // dismisses. Dropping it would silently lose a line the
            // author wrote.
            return b.item(label, || {});
        };
        if !item.icon.is_empty()
            && let Some(svg) = crate::icon::resolve("cn.MenuItem", &item.icon, ICON_SIZE)
        {
            return b.item_with_icon(label, svg, handler);
        }
        if !item.shortcut.is_empty() {
            return b.item_with_shortcut(label, item.shortcut.clone(), handler);
        }
        b.item(label, handler)
    }

    fn position(&self) -> blinc_cn::DropdownPosition {
        match self.position.as_str() {
            "top" => blinc_cn::DropdownPosition::Top,
            "left" => blinc_cn::DropdownPosition::Left,
            "right" => blinc_cn::DropdownPosition::Right,
            "" | "bottom" => blinc_cn::DropdownPosition::Bottom,
            other => {
                tracing::warn!(position = %other, "cn.DropdownMenu: unknown position");
                blinc_cn::DropdownPosition::Bottom
            }
        }
    }

    fn align(&self) -> blinc_cn::DropdownAlign {
        match self.align.as_str() {
            "center" => blinc_cn::DropdownAlign::Center,
            "end" => blinc_cn::DropdownAlign::End,
            "" | "start" => blinc_cn::DropdownAlign::Start,
            other => {
                tracing::warn!(align = %other, "cn.DropdownMenu: unknown align");
                blinc_cn::DropdownAlign::Start
            }
        }
    }
}

/// Icon size inside a menu row.
const ICON_SIZE: f32 = 16.0;

fn warn_loose_child() {
    tracing::warn!(
        "cn.DropdownMenu: child is not a cn.MenuItem or cn.MenuSeparator — dropped; \
         the menu draws its own rows and has nowhere to put a loose element",
    );
}

impl ElementBuilder for CnDropdownMenu {
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

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.get_or_build().layout_style()
    }
}
