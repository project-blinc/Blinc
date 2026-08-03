//! `cn.HoverCard` — a rich preview that appears on hover.

use std::cell::OnceCell;
use std::sync::Mutex;

use blinc_dsl_core::extern_widget;
use blinc_layout::div::{ElementBuilder, div};

/// `cn.HoverCardTrigger { … }` — what is hovered.
///
/// Read by [`CnHoverCard`] through `as_any`; rendering it anywhere else
/// shows the body plainly, since there is no card for it to raise.
#[extern_widget(namespace = "cn", name = "HoverCardTrigger")]
pub struct CnHoverCardTrigger {
    #[children]
    pub children: Mutex<Vec<Box<dyn ElementBuilder>>>,
    /// Only built when the slot renders outside its owner.
    #[skip]
    fallback: std::cell::OnceCell<blinc_layout::div::Div>,
}

/// `cn.HoverCardContent { … }` — what the hover raises.
///
/// Read by [`CnHoverCard`] through `as_any`. Outside one it renders
/// inline, which is what a card with nothing to raise it amounts to.
#[extern_widget(namespace = "cn", name = "HoverCardContent")]
pub struct CnHoverCardContent {
    #[children]
    pub children: Mutex<Vec<Box<dyn ElementBuilder>>>,
    /// Only built when the slot renders outside its owner.
    #[skip]
    fallback: std::cell::OnceCell<blinc_layout::div::Div>,
}

macro_rules! slot_widget {
    ($ty:ident, $name:literal) => {
        impl $ty {
            /// Take the body, leaving the slot empty. The hover card
            /// calls this once while building.
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
                    tracing::warn!(concat!($name, " outside a cn.HoverCard — rendering inline"));
                    let mut d = div().flex_col();
                    for child in self.take_children() {
                        d = d.child_box(child);
                    }
                    d
                })
            }
        }

        impl ElementBuilder for $ty {
            /// Only reached outside a hover card, where the slot has
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

            /// What lets `cn.HoverCard` tell the two slots apart.
            fn as_any(&self) -> Option<&dyn std::any::Any> {
                Some(self)
            }
        }
    };
}

slot_widget!(CnHoverCardTrigger, "cn.HoverCardTrigger");
slot_widget!(CnHoverCardContent, "cn.HoverCardContent");

/// `cn.HoverCard { cn.HoverCardTrigger { … } cn.HoverCardContent { … } }`
/// — hover the trigger and the card appears; unlike a popover, nothing
/// is clicked.
///
/// ```dsl,ignore
/// cn.HoverCard(side = "bottom") {
///     cn.HoverCardTrigger { cn.Label("@username") }
///     cn.HoverCardContent {
///         cn.Label("Ada Lovelace")
///         cn.Label("wrote the first program")
///     }
/// }
/// ```
///
/// Two named slots rather than a body and a prop, same as `cn.Popover`:
/// both halves are elements, and which of two children is the card is
/// not something to guess at.
#[extern_widget(namespace = "cn", name = "HoverCard")]
pub struct CnHoverCard {
    /// `top` / `bottom` (default) / `left` / `right`.
    pub side: String,
    /// `start` (default) / `center` / `end`, along the chosen side.
    pub align: String,
    /// Gap between trigger and card. Omitted keeps the cn default.
    pub offset: f64,
    /// Milliseconds of hover before the card shows. Omitted keeps the
    /// cn default, which is long enough to not fire in passing.
    pub open_delay: f64,
    /// Milliseconds after the pointer leaves before the card goes,
    /// long enough to travel from trigger to card. Omitted keeps the
    /// cn default.
    pub close_delay: f64,
    /// Names this card when two of them would otherwise look identical.
    /// Only needed for a genuine duplicate: see [`Self::card_key`].
    pub key: String,
    #[children]
    pub children: Mutex<Vec<Box<dyn ElementBuilder>>>,
    /// Built once, consuming `children`.
    #[skip]
    shell: OnceCell<blinc_cn::HoverCardBuilder>,
}

impl CnHoverCard {
    fn get_or_build(&self) -> &blinc_cn::HoverCardBuilder {
        ::blinc_layout::build_once::build_once(&self.shell, || self.make())
    }

    /// What tells this card's hover state from another's.
    ///
    /// Named by what the author wrote rather than the call site, per
    /// the container-widget lesson: a DSL widget with children has no
    /// call-site id, and every card is built from the one line below
    /// regardless. Two cards alike in every one of these respects share
    /// their hover state; `key` is the way out.
    fn card_key(&self) -> String {
        if !self.key.is_empty() {
            return format!("cn-hover-card-{}", self.key);
        }
        format!(
            "cn-hover-card-{}-{}-{}-{}-{}",
            self.side, self.align, self.offset, self.open_delay, self.close_delay,
        )
    }

    fn make(&self) -> blinc_cn::HoverCardBuilder {
        let children = std::mem::take(&mut *self.children.lock().expect("children mutex"));

        let mut trigger = None;
        let mut content = None;
        for child in children {
            let Some(any) = child.as_any() else {
                warn_loose_child();
                continue;
            };
            if let Some(slot) = any.downcast_ref::<CnHoverCardTrigger>() {
                trigger = Some(slot.take_children());
            } else if let Some(slot) = any.downcast_ref::<CnHoverCardContent>() {
                content = Some(slot.take_children());
            } else {
                warn_loose_child();
            }
        }

        let trigger = trigger.unwrap_or_else(|| {
            tracing::warn!("cn.HoverCard: no cn.HoverCardTrigger — nothing can raise it");
            Vec::new()
        });
        let trigger = crate::shared_child::body_recipe(trigger);

        let key = blinc_layout::InstanceKey::explicit(self.card_key());
        let mut h = blinc_cn::HoverCardBuilder::with_key(move || trigger(), key);
        if let Some(content) = content {
            let content = crate::shared_child::body_recipe(content);
            h = h.content(move || content());
        } else {
            tracing::warn!("cn.HoverCard: no cn.HoverCardContent — it raises nothing");
        }
        if let Some(side) = self.side() {
            h = h.side(side);
        }
        if let Some(align) = self.align() {
            h = h.align(align);
        }
        if self.offset > 0.0 {
            h = h.offset(self.offset as f32);
        }
        if self.open_delay > 0.0 {
            h = h.open_delay_ms(self.open_delay as u32);
        }
        if self.close_delay > 0.0 {
            h = h.close_delay_ms(self.close_delay as u32);
        }
        h
    }

    fn side(&self) -> Option<blinc_cn::HoverCardSide> {
        use blinc_cn::HoverCardSide as S;
        match self.side.as_str() {
            "" => None,
            "top" => Some(S::Top),
            "bottom" => Some(S::Bottom),
            "left" => Some(S::Left),
            "right" => Some(S::Right),
            other => {
                tracing::warn!(side = %other, "cn.HoverCard: unknown side");
                None
            }
        }
    }

    fn align(&self) -> Option<blinc_cn::HoverCardAlign> {
        use blinc_cn::HoverCardAlign as A;
        match self.align.as_str() {
            "" => None,
            "start" => Some(A::Start),
            "center" => Some(A::Center),
            "end" => Some(A::End),
            other => {
                tracing::warn!(align = %other, "cn.HoverCard: unknown align");
                None
            }
        }
    }
}

fn warn_loose_child() {
    tracing::warn!(
        "cn.HoverCard: child is not a cn.HoverCardTrigger or cn.HoverCardContent — dropped",
    );
}

impl ElementBuilder for CnHoverCard {
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
