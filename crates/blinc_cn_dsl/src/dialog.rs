//! `cn.Dialog` — a modal, opened by a signal rather than by a call.

use std::sync::Mutex;

use blinc_dsl_core::{Reactive, extern_widget};

use blinc_layout::div::{Div, ElementBuilder};
use blinc_layout::widgets::overlay_stack::OverlayHandle;

/// `cn.Dialog(open = signal, title = "…") { …body… }` — a modal that
/// follows a signal.
///
/// ```dsl,ignore
/// signal confirm: bool = false
///
/// cn.Button("Delete", on_click = || confirm.set(true))
/// cn.Dialog(open = confirm, title = "Delete this row?",
///           description = "This cannot be undone.") {
///     cn.Label("The row and its history go with it.")
/// }
/// ```
///
/// The Rust dialog is imperative — `dialog().show()` pushes onto the
/// overlay stack and hands back a handle to close later. A DSL source
/// has nowhere to keep that handle, so the signal is the handle: set it
/// and the modal appears, clear it and the modal goes.
///
/// The element itself draws nothing. It sits in the tree only to watch
/// the signal, which is why it can be written anywhere in a view.
#[extern_widget(namespace = "cn", name = "Dialog")]
pub struct CnDialog {
    /// Whether the modal is up. Writing it opens and closes.
    pub open: Reactive<bool>,
    /// Heading.
    pub title: String,
    /// Line under the heading.
    pub description: String,
    /// `small` / `medium` (default) / `large` / `full`.
    pub size: String,
    /// Confirm button text. Omitted keeps the cn default.
    pub confirm_text: String,
    /// Cancel button text. Omitted keeps the cn default.
    pub cancel_text: String,
    /// Style the confirm button as destructive.
    pub destructive: bool,
    /// Drop the cancel button, for a dialog with only an acknowledgement.
    pub hide_cancel: bool,
    /// Fired when confirm is pressed. Zero when omitted.
    pub on_confirm: i64,
    /// Fired when cancel is pressed, or the modal is dismissed. Zero
    /// when omitted.
    pub on_cancel: i64,
    #[children]
    pub children: Mutex<Vec<Box<dyn ElementBuilder>>>,
}

/// Everything the watcher needs to raise the modal, owned rather than
/// borrowed: the callback outlives the widget struct that made it.
#[derive(Clone)]
struct DialogProps {
    open: blinc_core::reactive::State<bool>,
    title: String,
    description: String,
    size: Option<blinc_cn::DialogSize>,
    confirm_text: String,
    cancel_text: String,
    destructive: bool,
    hide_cancel: bool,
    on_confirm: i64,
    on_cancel: i64,
    content: Option<std::sync::Arc<dyn Fn() -> Div + Send + Sync>>,
}

impl CnDialog {
    fn to_element(&self) -> blinc_layout::stateful::Stateful<()> {
        let open = crate::bridge::bool_state(&self.open);
        let props = self.props();
        crate::modal::watcher(open, move || props.show())
    }

    fn props(&self) -> DialogProps {
        let children = std::mem::take(&mut *self.children.lock().expect("children mutex"));
        DialogProps {
            open: crate::bridge::bool_state(&self.open),
            title: self.title.clone(),
            description: self.description.clone(),
            size: self.size(),
            confirm_text: self.confirm_text.clone(),
            cancel_text: self.cancel_text.clone(),
            destructive: self.destructive,
            hide_cancel: self.hide_cancel,
            on_confirm: self.on_confirm,
            on_cancel: self.on_cancel,
            content: crate::modal::content_recipe(children),
        }
    }

    fn size(&self) -> Option<blinc_cn::DialogSize> {
        use blinc_cn::DialogSize as S;
        match self.size.as_str() {
            "" => None,
            "small" | "sm" => Some(S::Small),
            "medium" | "md" => Some(S::Medium),
            "large" | "lg" => Some(S::Large),
            "full" => Some(S::Full),
            other => {
                tracing::warn!(size = %other, "cn.Dialog: unknown size");
                None
            }
        }
    }
}

impl DialogProps {
    fn show(&self) -> OverlayHandle {
        let mut d = blinc_cn::dialog();
        if !self.title.is_empty() {
            d = d.title(self.title.clone());
        }
        if !self.description.is_empty() {
            d = d.description(self.description.clone());
        }
        if let Some(size) = self.size {
            d = d.size(size);
        }
        if !self.confirm_text.is_empty() {
            d = d.confirm_text(self.confirm_text.clone());
        }
        if !self.cancel_text.is_empty() {
            d = d.cancel_text(self.cancel_text.clone());
        }
        if self.destructive {
            d = d.confirm_destructive(true);
        }
        if self.hide_cancel {
            d = d.hide_cancel();
        }
        if let Some(content) = self.content.clone() {
            d = d.content(move || content());
        }
        // Either button clears the signal as well as running its
        // handler, so the modal that closed itself stays closed.
        d = d.on_confirm(crate::modal::closing_handler(
            self.open.clone(),
            self.on_confirm,
        ));
        d = d.on_cancel(crate::modal::closing_handler(
            self.open.clone(),
            self.on_cancel,
        ));
        // And every other way out: a backdrop click or an Escape closes
        // the dialog without either button firing.
        d = d.on_close(crate::modal::closing_handler(self.open.clone(), 0));
        d.show()
    }
}

impl ElementBuilder for CnDialog {
    fn build(&self, tree: &mut blinc_layout::LayoutTree) -> blinc_layout::LayoutNodeId {
        self.to_element().build(tree)
    }

    fn render_props(&self) -> blinc_layout::RenderProps {
        blinc_layout::RenderProps::default()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        &[]
    }
}
