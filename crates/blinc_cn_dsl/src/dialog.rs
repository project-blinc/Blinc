//! `cn.Dialog` — a modal, opened by a signal rather than by a call.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use blinc_dsl_core::{Reactive, extern_widget};

use crate::bridge::CallSiteId;
use blinc_layout::div::{Div, ElementBuilder, div};
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
    /// Call-site identity, captured while the FFI builds the struct.
    ///
    /// The struct itself is rebuilt every frame, so it cannot hold the
    /// open modal's handle — a field on it would be empty on the next
    /// build and the dialog would be shown again, once per frame.
    #[skip]
    call_site: CallSiteId,
}

/// Open modals, by the call site that opened them.
///
/// Outside the widget because the widget does not outlive a frame, and
/// a modal outlives many.
fn open_modals() -> &'static Mutex<HashMap<u64, OverlayHandle>> {
    static MODALS: OnceLock<Mutex<HashMap<u64, OverlayHandle>>> = OnceLock::new();
    MODALS.get_or_init(|| Mutex::new(HashMap::new()))
}

impl CnDialog {
    /// The watcher. Renders nothing; exists to follow the signal.
    fn to_element(&self) -> Div {
        let state = crate::bridge::bool_state(&self.open);
        let should_be_open = state.try_get().unwrap_or(false);
        self.sync(should_be_open);
        div()
    }

    /// Bring the modal into line with the signal.
    ///
    /// Called on every build, so it has to be idempotent: showing an
    /// already-open dialog would stack a second copy behind the first.
    fn sync(&self, should_be_open: bool) {
        let key = self.call_site.0;
        let mut modals = open_modals().lock().expect("open modals");
        let live = modals.get(&key).is_some_and(|h| h.is_live());
        match (should_be_open, live) {
            (true, false) => {
                modals.insert(key, self.show());
            }
            (false, true) => {
                // Removed before closing: `close` reaches back into the
                // overlay stack, and a handle left behind would read as
                // live on the next build.
                if let Some(h) = modals.remove(&key) {
                    h.close();
                }
            }
            _ => {}
        }
    }

    fn show(&self) -> OverlayHandle {
        let mut d = blinc_cn::dialog();
        if !self.title.is_empty() {
            d = d.title(self.title.clone());
        }
        if !self.description.is_empty() {
            d = d.description(self.description.clone());
        }
        if let Some(size) = self.size() {
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

        let children = std::mem::take(&mut *self.children.lock().expect("children mutex"));
        if !children.is_empty() {
            d = d.content(crate::shared_child::body_recipe(children));
        }

        // Either button clears the signal as well as running the
        // handler: the modal closes itself, and a signal left set would
        // reopen it on the next build.
        let state = crate::bridge::bool_state(&self.open);
        let confirm = closure(self.on_confirm);
        let confirm_state = state.clone();
        d = d.on_confirm(move || {
            confirm_state.set(false);
            confirm();
        });
        let cancel = closure(self.on_cancel);
        d = d.on_cancel(move || {
            state.set(false);
            cancel();
        });

        d.show()
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

/// A DSL closure pointer as something callable, or a no-op for zero.
fn closure(ptr: i64) -> impl Fn() + Send + Sync + 'static {
    move || {
        if ptr != 0 {
            type ClosureFn = extern "C" fn();
            let func: ClosureFn = unsafe { std::mem::transmute(ptr) };
            func();
        }
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
