//! The link field: a cn input for the URL, with Apply and Remove buttons. Enter applies.

use std::sync::Arc;

use blinc_layout::div::{Div, div};
use blinc_layout::widgets::text_input::SharedTextInputData;

use super::actions;
use super::controls::Target;
use crate::components::button::{ButtonSize, ButtonVariant, button};
use crate::components::input::{InputSize, input};
use crate::components::rich_text_editor::state::PickerState;

/// The width of the link row.
pub(super) const WIDTH: f32 = 340.0;
/// The height of the link row.
pub(super) const HEIGHT: f32 = 40.0;

/// Link the selection to the field's URL, or remove its link when the field is empty, and close
/// the field.
fn apply(target: &Target, field: &SharedTextInputData) {
    let url = field
        .lock()
        .map(|f| f.value.trim().to_string())
        .unwrap_or_default();
    target.act(|d| {
        actions::set_link(d, (!url.is_empty()).then_some(url));
        d.picker = PickerState::None;
    });
}

/// The link row for the editor of `target`.
pub(super) fn field(target: &Target) -> Div {
    let Some(field) = target.state.lock().ok().map(|d| Arc::clone(&d.link_input)) else {
        return div();
    };
    let on_submit = (target.clone(), Arc::clone(&field));
    let on_apply = (target.clone(), Arc::clone(&field));
    let on_remove = target.clone();
    div()
        .class("blinc-rte-link")
        .flex_row()
        .items_center()
        .gap_px(6.0)
        .h(HEIGHT)
        .child(
            input(&field)
                .size(InputSize::Small)
                .placeholder("https://")
                .w(200.0)
                .on_submit(move |_| apply(&on_submit.0, &on_submit.1)),
        )
        .child(
            button("Apply")
                .size(ButtonSize::Small)
                .on_click(move |_| apply(&on_apply.0, &on_apply.1)),
        )
        .child(
            button("Remove")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Small)
                .on_click(move |_| {
                    on_remove.act(|d| {
                        actions::set_link(d, None);
                        d.picker = PickerState::None;
                    })
                }),
        )
}
