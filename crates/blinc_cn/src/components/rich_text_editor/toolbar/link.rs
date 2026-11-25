//! The link popover: a Text field for the linked text and a Link field for its URL, with Remove
//! and Apply. Enter in either field applies.

use std::sync::Arc;

use blinc_core::Signal;
use blinc_icons::icons;
use blinc_layout::binding::Reactive;
use blinc_layout::div::{Div, div};
use blinc_layout::widgets::overlay_stack::OverlayHandle;
use blinc_layout::widgets::text_input::{SharedTextInputData, focus_text_input_deferred};

use super::actions;
use super::controls::{self, Target};
use crate::components::button::{ButtonSize, ButtonVariant, button};
use crate::components::input::{InputSize, input};
use crate::components::label::label;
use crate::components::picker_popover::picker_popover;

/// The popover's size.
const WIDTH: f32 = 340.0;
const HEIGHT: f32 = 168.0;
/// The width of the field labels.
const LABEL: f32 = 40.0;

/// A field's text.
fn value(field: &SharedTextInputData) -> String {
    field.lock().map(|f| f.value.clone()).unwrap_or_default()
}

/// Put `text` in a field, with the cursor after it.
fn fill(field: &SharedTextInputData, text: String) {
    if let Ok(mut f) = field.lock() {
        f.cursor = text.chars().count();
        f.selection_start = None;
        f.value = text;
    }
}

/// Close the popover `active` holds.
fn close(active: Signal<Option<u64>>) {
    if let Some(raw) = active.get() {
        OverlayHandle::from_raw(raw).close();
    }
}

/// Apply the fields to the editor and close the popover.
fn apply(target: &Target, active: Signal<Option<u64>>) {
    let fields = target
        .state
        .lock()
        .ok()
        .map(|d| (value(&d.link_text), value(&d.link_url)));
    if let Some((text, url)) = fields {
        target.act(|d| actions::apply_link(d, &text, &url));
    }
    close(active);
}

/// Fill the fields from the selection and hand them the focus, when the popover opens.
fn open(target: &Target) {
    let Ok(mut data) = target.state.lock() else {
        return;
    };
    if data.link_open {
        // This press closes it.
        return;
    }
    data.link_open = true;
    let (text, url) = actions::link_fields(&data);
    let empty_text = text.is_empty();
    fill(&data.link_text, text);
    fill(&data.link_url, url);
    // The fields take the keys; the selection stays shown while the popover is open.
    data.set_focus(false);
    let first = if empty_text {
        Arc::clone(&data.link_text)
    } else {
        Arc::clone(&data.link_url)
    };
    drop(data);
    target.version.update(|n| n.wrapping_add(1));
    focus_text_input_deferred(&first);
}

/// A labelled field.
fn field(name: &'static str, input: impl blinc_layout::div::ElementBuilder + 'static) -> Div {
    div()
        .flex_row()
        .items_center()
        .gap_px(8.0)
        .child(div().w(LABEL).child(label(name)))
        .child(div().flex_grow().min_w(0.0).child(input))
}

/// The popover's form.
fn form(target: &Target, active: Signal<Option<u64>>) -> Div {
    let Ok(data) = target.state.lock() else {
        return div();
    };
    let (text, url) = (Arc::clone(&data.link_text), Arc::clone(&data.link_url));
    let linked = actions::existing_link(&data).is_some();
    drop(data);

    let submit = |target: &Target| {
        let target = target.clone();
        move |_: bool| apply(&target, active)
    };
    let mut buttons = div().flex_row().justify_end().gap_px(8.0);
    if linked {
        let target = target.clone();
        buttons = buttons.child(
            button("Remove")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Small)
                .on_click(move |_| {
                    target.act(|d| {
                        let text = actions::link_fields(d).0;
                        actions::apply_link(d, &text, "");
                    });
                    close(active);
                }),
        );
    }
    let on_apply = target.clone();
    buttons = buttons.child(
        button("Apply")
            .size(ButtonSize::Small)
            .on_click(move |_| apply(&on_apply, active)),
    );

    let state = Arc::clone(&target.state);
    div()
        .class("cn-rich-text-editor-link")
        .w(WIDTH)
        .flex_col()
        .gap_px(10.0)
        .child(field(
            "Text",
            input(&text)
                .size(InputSize::Small)
                .placeholder("Text")
                .w_full()
                .on_submit(submit(target)),
        ))
        .child(field(
            "Link",
            input(&url)
                .size(InputSize::Small)
                .placeholder("Paste a link")
                .w_full()
                .on_submit(submit(target)),
        ))
        .child(buttons)
        // Closed by any means, the popover no longer holds the selection.
        .on_unmount(move |_| {
            if let Ok(mut data) = state.lock() {
                data.link_open = false;
            }
        })
}

/// The link button, which opens the link popover. `linked` shows it pressed.
pub(super) fn link_button(target: &Target, linked: bool) -> Div {
    let id = format!("cn-rich-text-editor-link-{:p}", Arc::as_ptr(&target.state));
    let trigger = controls::icon_button(target, icons::LINK, "Link", linked, open);
    let target = target.clone();
    picker_popover(
        trigger,
        id,
        Reactive::Const(false),
        (WIDTH, HEIGHT),
        move |active| form(&target, active),
    )
}
