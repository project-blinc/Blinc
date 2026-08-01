//! `cn.Popover` — a panel anchored to whatever opens it.

use std::cell::OnceCell;
use std::sync::Mutex;

use blinc_dsl_core::extern_widget;
use blinc_layout::div::ElementBuilder;

use crate::popover_slots::{CnPopoverContent, CnPopoverTrigger};

/// `cn.Popover { cn.PopoverTrigger { … } cn.PopoverContent { … } }` — a
/// panel that opens against its trigger.
///
/// ```dsl,ignore
/// cn.Popover(side = "bottom", align = "start") {
///     cn.PopoverTrigger { cn.Button("Options") }
///     cn.PopoverContent {
///         cn.Label("Anything at all")
///         cn.Button("Do the thing")
///     }
/// }
/// ```
///
/// Two named slots rather than a body and a prop: both halves are
/// elements, and a `content` prop could only have taken a string.
///
/// Unlike `cn.Tooltip`, whose body is its trigger, a popover has two
/// distinct pieces and nothing to infer from position — which of two
/// children is the panel is not something to guess at.
#[extern_widget(namespace = "cn", name = "Popover")]
pub struct CnPopover {
    /// `top` / `bottom` (default) / `left` / `right`.
    pub side: String,
    /// `start` (default) / `center` / `end`, along the chosen side.
    pub align: String,
    /// Gap between trigger and panel. Omitted keeps the cn default.
    pub offset: f64,
    #[children]
    pub children: Mutex<Vec<Box<dyn ElementBuilder>>>,
    /// Built once, consuming `children`.
    #[skip]
    shell: OnceCell<blinc_cn::PopoverBuilder>,
}

impl CnPopover {
    fn get_or_build(&self) -> &blinc_cn::PopoverBuilder {
        // Built outside the cell: the widget runs its trigger recipe
        // during construction, which re-enters here.
        if let Some(built) = self.shell.get() {
            return built;
        }
        let built = self.make();
        let _ = self.shell.set(built);
        self.shell.get().expect("just set")
    }

    fn make(&self) -> blinc_cn::PopoverBuilder {
        let children = std::mem::take(&mut *self.children.lock().expect("children mutex"));

        let mut trigger = None;
        let mut content = None;
        for child in children {
            let Some(any) = child.as_any() else {
                warn_loose_child();
                continue;
            };
            if let Some(slot) = any.downcast_ref::<CnPopoverTrigger>() {
                trigger = Some(slot.take_children());
            } else if let Some(slot) = any.downcast_ref::<CnPopoverContent>() {
                content = Some(slot.take_children());
            } else {
                warn_loose_child();
            }
        }

        // A popover with no trigger cannot be opened, so it is worth
        // saying rather than rendering an invisible one.
        let trigger = trigger.unwrap_or_else(|| {
            tracing::warn!("cn.Popover: no cn.PopoverTrigger — nothing can open it");
            Vec::new()
        });
        let trigger = crate::shared_child::body_recipe(trigger);

        // The cn trigger takes the open state so it can show its own
        // state; a DSL trigger is a fixed block of children, so it
        // ignores the flag.
        let mut p = blinc_cn::popover(move |_open| trigger());
        if let Some(content) = content {
            p = p.content(crate::shared_child::body_recipe(content));
        } else {
            tracing::warn!("cn.Popover: no cn.PopoverContent — it opens onto nothing");
        }
        if let Some(side) = self.side() {
            p = p.side(side);
        }
        if let Some(align) = self.align() {
            p = p.align(align);
        }
        if self.offset > 0.0 {
            p = p.offset(self.offset as f32);
        }
        p
    }

    fn side(&self) -> Option<blinc_cn::PopoverSide> {
        use blinc_cn::PopoverSide as S;
        match self.side.as_str() {
            "" => None,
            "top" => Some(S::Top),
            "bottom" => Some(S::Bottom),
            "left" => Some(S::Left),
            "right" => Some(S::Right),
            other => {
                tracing::warn!(side = %other, "cn.Popover: unknown side");
                None
            }
        }
    }

    fn align(&self) -> Option<blinc_cn::PopoverAlign> {
        use blinc_cn::PopoverAlign as A;
        match self.align.as_str() {
            "" => None,
            "start" => Some(A::Start),
            "center" => Some(A::Center),
            "end" => Some(A::End),
            other => {
                tracing::warn!(align = %other, "cn.Popover: unknown align");
                None
            }
        }
    }
}

fn warn_loose_child() {
    tracing::warn!("cn.Popover: child is not a cn.PopoverTrigger or cn.PopoverContent — dropped",);
}

impl ElementBuilder for CnPopover {
    fn build(&self, tree: &mut blinc_layout::LayoutTree) -> blinc_layout::LayoutNodeId {
        self.get_or_build().build(tree)
    }

    fn render_props(&self) -> blinc_layout::RenderProps {
        self.get_or_build().render_props()
    }

    /// The shell's children, which now hold the trigger.
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
