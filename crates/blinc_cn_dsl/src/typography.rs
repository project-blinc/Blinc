//! `cn.H1`…`cn.H6`, `cn.P`, `cn.Muted`, `cn.Caption` — the themed type
//! ramp, one constructor per rung, mirroring the Rust surface
//! (`cn::h1(..)` … `cn::caption(..)`).

use std::cell::OnceCell;

use blinc_dsl_core::{Reactive, extern_widget};
use blinc_layout::div::ElementBuilder;

macro_rules! typography_widget {
    ($ty:ident, $name:literal, $ctor:ident) => {
        #[doc = concat!(
                                    "`cn.", $name, "(text)` — one rung of the theme's type ramp.\n\
             \n\
             `text` binds: a signal here re-renders just this text node.",
                                )]
        #[extern_widget(namespace = "cn", name = $name)]
        pub struct $ty {
            /// What it says.
            pub text: Reactive<String>,
            /// Built once. Boxed because a bound text is a subscribed
            /// wrapper while a literal is a bare text node.
            #[skip]
            built: OnceCell<Box<dyn ElementBuilder>>,
        }

        impl $ty {
            fn get_or_build(&self) -> &Box<dyn ElementBuilder> {
                ::blinc_layout::build_once::build_once(&self.built, || {
                    let src = blinc_layout::binding::IntoReactive::into_reactive(self.text.clone());
                    blinc_cn::reactive_props::reactive_node(&src, |s| Box::new(blinc_cn::$ctor(s)))
                })
            }
        }

        impl ElementBuilder for $ty {
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
            // `text_render_info` most of all: it is what tells the
            // renderer this element IS text. A literal heading built
            // through a wrapper that leaves it at the `None` default
            // gets a correctly-sized layout node and no glyphs.
            fn text_render_info(&self) -> Option<blinc_layout::div::TextRenderInfo> {
                self.get_or_build().text_render_info()
            }

            fn styled_text_render_info(&self) -> Option<blinc_layout::div::StyledTextRenderInfo> {
                self.get_or_build().styled_text_render_info()
            }

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
    };
}

typography_widget!(CnH1, "H1", h1);
typography_widget!(CnH2, "H2", h2);
typography_widget!(CnH3, "H3", h3);
typography_widget!(CnH4, "H4", h4);
typography_widget!(CnH5, "H5", h5);
typography_widget!(CnH6, "H6", h6);
typography_widget!(CnP, "P", p);
typography_widget!(CnMuted, "Muted", muted);
typography_widget!(CnCaption, "Caption", caption);
typography_widget!(CnSpan, "Span", span);
typography_widget!(CnB, "B", b);
typography_widget!(CnStrong, "Strong", strong);
typography_widget!(CnSmall, "Small", small);
typography_widget!(CnInlineCode, "InlineCode", inline_code);

/// `cn.ChainedText { cn.Span("This is ") cn.B("bold") }` — inline runs
/// on one baseline, mirroring `cn::chained_text([...])`.
///
/// The children are the inline constructors above; anything else still
/// renders, it just sits on the same baseline row.
#[extern_widget(namespace = "cn", name = "ChainedText")]
pub struct CnChainedText {
    #[children]
    pub children: std::sync::Mutex<Vec<Box<dyn ElementBuilder>>>,
    /// Built once, consuming `children`.
    #[skip]
    built: OnceCell<blinc_layout::div::Div>,
}

impl CnChainedText {
    fn get_or_build(&self) -> &blinc_layout::div::Div {
        ::blinc_layout::build_once::build_once(&self.built, || {
            let children = std::mem::take(&mut *self.children.lock().expect("children mutex"));
            // The same shape `cn::chained_text` builds — a baseline
            // flex row — composed here because the DSL's children
            // arrive as boxed wrappers rather than bare `Text`s.
            let mut row = blinc_layout::div::div()
                .flex_row()
                .items_start()
                .items_baseline();
            for child in children {
                row = row.child_box(child);
            }
            row
        })
    }
}

impl ElementBuilder for CnChainedText {
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
