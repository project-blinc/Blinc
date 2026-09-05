//! `cn.ResizableGroup` — panels split by draggable handles.

use std::cell::OnceCell;
use std::sync::Mutex;

use blinc_dsl_core::extern_widget;
use blinc_layout::div::{Div, ElementBuilder, div};

/// `cn.ResizablePanel(default_size = 200.0) { … }` — one pane inside a
/// [`CnResizableGroup`].
///
/// Sizes are pixels along the group's axis. A panel with no
/// `default_size` flexes to fill what the fixed ones leave.
#[extern_widget(namespace = "cn", name = "ResizablePanel")]
pub struct CnResizablePanel {
    /// Names the panel, so its dragged size can persist. Defaults to
    /// its position in the group.
    pub id: String,
    /// Starting size in pixels. Omitted makes the panel flex.
    pub default_size: f64,
    /// The drag stops here. Omitted keeps the cn default.
    pub min_size: f64,
    /// And here. Omitted leaves it unbounded.
    pub max_size: f64,
    /// Grow factor for a flexing panel. Omitted reads as one.
    pub flex: f64,
    #[children]
    pub children: Mutex<Vec<Box<dyn ElementBuilder>>>,
    /// Only built when the panel renders outside a group.
    #[skip]
    fallback: OnceCell<Div>,
}

impl CnResizablePanel {
    /// Take the body as one element, leaving the panel empty. The
    /// group calls this once while building.
    pub(crate) fn take_body(&self) -> Div {
        let children = std::mem::take(&mut *self.children.lock().expect("children mutex"));
        let mut body = div().w_full().h_full().flex_col();
        for child in children {
            body = body.child_box(child);
        }
        body
    }

    pub(crate) fn to_cn_panel(&self, index: usize) -> blinc_cn::ResizablePanelBuilder {
        let mut p = blinc_cn::resizable_panel();
        p = if self.id.is_empty() {
            p.id(format!("panel-{index}"))
        } else {
            p.id(self.id.clone())
        };
        if self.default_size > 0.0 {
            p = p.default_size(self.default_size as f32);
        } else {
            p = p.flex(if self.flex > 0.0 {
                self.flex as f32
            } else {
                1.0
            });
        }
        if self.min_size > 0.0 {
            p = p.min_size(self.min_size as f32);
        }
        if self.max_size > 0.0 {
            p = p.max_size(self.max_size as f32);
        }
        p.child(self.take_body())
    }

    /// The bare body. A panel outside a group has no handle to drag,
    /// so showing the content beats showing nothing.
    fn get_or_build(&self) -> &Div {
        ::blinc_layout::build_once::build_once(&self.fallback, || {
            tracing::warn!("cn.ResizablePanel outside a cn.ResizableGroup — rendering inline");
            self.take_body()
        })
    }
}

impl ElementBuilder for CnResizablePanel {
    fn build(&self, tree: &mut blinc_layout::LayoutTree) -> blinc_layout::LayoutNodeId {
        self.get_or_build().build(tree)
    }

    fn render_props(&self) -> blinc_layout::RenderProps {
        self.get_or_build().render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().children_builders()
    }

    /// What lets `cn.ResizableGroup` read this panel's sizes and body.
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }
}

/// `cn.ResizableGroup { cn.ResizablePanel { … } … }` — drag the handle
/// between panels to trade space.
///
/// ```dsl,ignore
/// cn.ResizableGroup(direction = "horizontal", h = 220.0) {
///     cn.ResizablePanel(default_size = 180.0, min_size = 120.0) {
///         cn.Label("sidebar")
///     }
///     cn.ResizablePanel {
///         cn.Label("content fills the rest")
///     }
/// }
/// ```
///
/// Handles are drawn by the group, one between each pair of panels —
/// nothing to declare.
#[extern_widget(namespace = "cn", name = "ResizableGroup")]
pub struct CnResizableGroup {
    /// `horizontal` (default) — panels side by side — or `vertical`.
    pub direction: String,
    /// Grab area in pixels. Omitted keeps the cn default.
    pub handle_thickness: f64,
    /// Group height in pixels. A horizontal split usually wants one,
    /// since its panels fill whatever cross-axis they are given.
    pub h: f64,
    /// Names this group when two of them would otherwise look
    /// identical. Only needed for a genuine duplicate: see
    /// `group_key`.
    pub key: String,
    #[children]
    pub children: Mutex<Vec<Box<dyn ElementBuilder>>>,
    /// Built once, consuming `children`.
    #[skip]
    shell: OnceCell<Div>,
}

impl CnResizableGroup {
    fn get_or_build(&self) -> &Div {
        ::blinc_layout::build_once::build_once(&self.shell, || self.make())
    }

    /// What tells this group's dragged sizes from another's.
    ///
    /// Named by what the author wrote rather than the call site, per
    /// the container-widget lesson. Two groups alike in every one of
    /// these respects share their drag state; `key` is the way out.
    fn group_key(&self, panel_ids: &[String]) -> String {
        if !self.key.is_empty() {
            return format!("cn-resizable-{}", self.key);
        }
        format!(
            "cn-resizable-{}-{}-{}",
            self.direction,
            self.handle_thickness,
            panel_ids.join(",")
        )
    }

    fn make(&self) -> Div {
        let children = std::mem::take(&mut *self.children.lock().expect("children mutex"));

        let mut g = blinc_cn::resizable_group().direction(self.direction());
        if self.handle_thickness > 0.0 {
            g = g.handle_thickness(self.handle_thickness as f32);
        }

        let mut panel_ids = Vec::new();
        let mut index = 0usize;
        for child in children {
            let Some(panel) = child
                .as_any()
                .and_then(|any| any.downcast_ref::<CnResizablePanel>())
            else {
                tracing::warn!(
                    "cn.ResizableGroup: child is not a cn.ResizablePanel — dropped; \
                     wrap it in cn.ResizablePanel to give it a pane",
                );
                continue;
            };
            panel_ids.push(if panel.id.is_empty() {
                index.to_string()
            } else {
                panel.id.clone()
            });
            g = g.panel(panel.to_cn_panel(index));
            index += 1;
        }

        g = g.key(self.group_key(&panel_ids));

        // The wrapper owns the cross-axis size: the group fills its
        // container, and a DSL page is a column of h-fit rows.
        let mut host = div().w_full().child(g.build());
        if self.h > 0.0 {
            host = host.h(self.h as f32);
        }
        host
    }

    fn direction(&self) -> blinc_cn::ResizeDirection {
        match self.direction.as_str() {
            "vertical" | "column" => blinc_cn::ResizeDirection::Vertical,
            "" | "horizontal" | "row" => blinc_cn::ResizeDirection::Horizontal,
            other => {
                tracing::warn!(direction = %other, "cn.ResizableGroup: unknown direction");
                blinc_cn::ResizeDirection::Horizontal
            }
        }
    }
}

impl ElementBuilder for CnResizableGroup {
    fn build(&self, tree: &mut blinc_layout::LayoutTree) -> blinc_layout::LayoutNodeId {
        self.get_or_build().build(tree)
    }

    fn render_props(&self) -> blinc_layout::RenderProps {
        self.get_or_build().render_props()
    }

    /// The shell's children, which now hold the panels and handles.
    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().children_builders()
    }

    // MUST forward — see `gotcha_element_builder_trait_forwarding`.
    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        Some(self.get_or_build().event_handlers())
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
