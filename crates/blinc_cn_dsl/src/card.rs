//! `cn.Card` — surface container with `cn-card` CSS class + shadow.

use std::cell::OnceCell;

use blinc_dsl_core::extern_widget;
use blinc_layout::div::ElementBuilder;

/// `cn.Card { children… }` — container surface.
///
/// Body block ⇨ children. The DSL form
///
/// ```dsl,ignore
/// cn.Card {
///     cn.Label("Email")
///     cn.Button("Save")
///     Text("subtle hint")
/// }
/// ```
///
/// is wrapped in a `cn-card`-classed `Div` with the standard cn shadow,
/// flex-column layout, and left-aligned items.
///
/// `blinc_cn::Card` exposes layout-shape builders (`.w()`, `.h()`,
/// `.p{,x,y}()`, `.m()`, `.shadow_{sm,lg}()` etc.) that overlap with
/// the DSL's universal `Div`-style overlay surface and should ride
/// through that path rather than as per-widget props.
///
/// Children-block plumbing reuses the macro's existing `#[children]`
/// support — the `cn.` namespace works the same as bare-name widgets
/// for body blocks. No new grammar work; the dotted call shape
/// composes with the existing `Name(args) { body }` rule.
#[extern_widget(namespace = "cn", name = "Card")]
pub struct CnCard {
    #[children]
    pub children: Vec<Box<dyn ElementBuilder>>,
    /// Cached card shell. Built once so `build()` and `render_props()`
    /// describe the same instance, and so the identity methods can
    /// return references -- they borrow from the shell, which a fresh
    /// `Card::new()` temporary cannot outlive. Same rationale as
    /// `CnButton::built`.
    #[skip]
    shell: OnceCell<blinc_cn::Card>,
}

impl CnCard {
    fn get_or_build(&self) -> &blinc_cn::Card {
        ::blinc_layout::build_once::build_once(&self.shell, blinc_cn::Card::new)
    }
}

impl ElementBuilder for CnCard {
    fn build(&self, tree: &mut blinc_layout::LayoutTree) -> blinc_layout::LayoutNodeId {
        // Build the empty cn::Card shell first — gives us the
        // `cn-card` class + shadow + flex-col layout. Then attach
        // the DSL-body children directly to the card's layout node.
        //
        // We can't feed `self.children` into `cn::Card::child()`
        // because that API consumes owned values and we only hold
        // shared refs. Manual tree.add_child has the same observable
        // result — every child's `build()` runs once and its node
        // ends up parented to the card.
        let card_node = self.get_or_build().build(tree);
        for child in &self.children {
            let child_node = child.build(tree);
            tree.add_child(card_node, child_node);
        }
        card_node
    }

    fn render_props(&self) -> blinc_layout::RenderProps {
        // RenderProps is element-local — children don't contribute,
        // so the card shell carries exactly the visual state this
        // wrapper renders.
        self.get_or_build().render_props()
    }

    // The DSL body children, not the shell's: `build()` parents these
    // to the card node directly, so these are the builders that
    // correspond to its layout children.
    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        &self.children
    }

    // MUST forward — see `gotcha_element_builder_trait_forwarding`.
    // Without `element_classes` the `cn-card` selector never matches
    // and the card renders with no surface, border or shadow.
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
