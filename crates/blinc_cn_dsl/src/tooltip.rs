//! `cn.Tooltip` — text on hover, hanging off whatever it wraps.

use std::cell::OnceCell;
use std::sync::Mutex;

use blinc_dsl_core::extern_widget;
use blinc_layout::div::ElementBuilder;

/// `cn.Tooltip(text = "…") { …trigger… }` — a label that appears when
/// the pointer rests on the body.
///
/// ```dsl,ignore
/// cn.Tooltip(text = "Delete this row", side = "bottom") {
///     cn.Button("Delete", variant = "destructive")
/// }
/// ```
///
/// The body IS the trigger: whatever is inside is what the pointer has
/// to reach, and the tooltip hangs off it. That is why there is no
/// separate trigger slot — a tooltip with two triggers has no meaning.
///
/// The body is handed over as a recipe rather than as built elements,
/// because the widget rebuilds its trigger when the tooltip opens and
/// closes. See `crate::shared_child` for what that costs.
#[extern_widget(namespace = "cn", name = "Tooltip")]
pub struct CnTooltip {
    /// What the tooltip says. Empty renders the trigger alone, since a
    /// tooltip with nothing to say is just a delay.
    pub text: String,
    /// `top` (default) / `bottom` / `left` / `right`.
    pub side: String,
    /// `start` / `center` (default) / `end`, along the chosen side.
    pub align: String,
    /// How long the pointer has to rest before it opens, in
    /// milliseconds. Omitted keeps the cn default.
    pub open_delay: f64,
    /// How long it lingers after the pointer leaves. Omitted keeps the
    /// cn default, which is to close at once.
    pub close_delay: f64,
    /// Gap between trigger and tooltip. Omitted keeps the cn default.
    pub offset: f64,
    #[children]
    pub children: Mutex<Vec<Box<dyn ElementBuilder>>>,
    /// Built once, consuming `children`.
    #[skip]
    shell: OnceCell<blinc_cn::TooltipBuilder>,
}

impl CnTooltip {
    fn get_or_build(&self) -> &blinc_cn::TooltipBuilder {
        // Built outside the cell: the widget runs its trigger recipe
        // during construction, which re-enters here.
        if let Some(built) = self.shell.get() {
            return built;
        }
        let built = self.make();
        let _ = self.shell.set(built);
        self.shell.get().expect("just set")
    }

    fn make(&self) -> blinc_cn::TooltipBuilder {
        let children = std::mem::take(&mut *self.children.lock().expect("children mutex"));
        let mut t = blinc_cn::tooltip(crate::shared_child::body_recipe(children));
        if !self.text.is_empty() {
            t = t.text(self.text.clone());
        }
        if let Some(side) = self.side() {
            t = t.side(side);
        }
        if let Some(align) = self.align() {
            t = t.align(align);
        }
        // Zero is what an omitted number prop reads as, and a zero
        // offset or delay is a meaningful setting — so only a positive
        // one overrides. An instant tooltip wants `open_delay` gone
        // from the source, not set to zero.
        if self.open_delay > 0.0 {
            t = t.open_delay_ms(self.open_delay as u32);
        }
        if self.close_delay > 0.0 {
            t = t.close_delay_ms(self.close_delay as u32);
        }
        if self.offset > 0.0 {
            t = t.offset(self.offset as f32);
        }
        t
    }

    fn side(&self) -> Option<blinc_cn::TooltipSide> {
        use blinc_cn::TooltipSide as S;
        match self.side.as_str() {
            "" => None,
            "top" => Some(S::Top),
            "bottom" => Some(S::Bottom),
            "left" => Some(S::Left),
            "right" => Some(S::Right),
            other => {
                tracing::warn!(side = %other, "cn.Tooltip: unknown side");
                None
            }
        }
    }

    fn align(&self) -> Option<blinc_cn::TooltipAlign> {
        use blinc_cn::TooltipAlign as A;
        match self.align.as_str() {
            "" => None,
            "start" => Some(A::Start),
            "center" => Some(A::Center),
            "end" => Some(A::End),
            other => {
                tracing::warn!(align = %other, "cn.Tooltip: unknown align");
                None
            }
        }
    }
}

impl ElementBuilder for CnTooltip {
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
