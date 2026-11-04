//! Popover lifecycle shared by value pickers.

use blinc_core::reactive::{Signal, computed, effect, signal};
use blinc_layout::widgets::{
    overlay::AnchorDirection,
    overlay_stack::{OverlayBuilder, OverlayHandle},
};
use blinc_layout::{binding::Reactive, div::Div};
use std::sync::Arc;

pub(super) fn picker_popover(
    trigger: Div,
    id: String,
    disabled: Reactive<bool>,
    size: (f32, f32),
    content: impl Fn(Signal<Option<u64>>) -> Div + Send + Sync + 'static,
) -> Div {
    picker_popup(trigger, id, disabled, size, false, content)
}

pub(super) fn picker_popup(
    trigger: Div,
    id: String,
    disabled: Reactive<bool>,
    size: (f32, f32),
    over_control: bool,
    content: impl Fn(Signal<Option<u64>>) -> Div + Send + Sync + 'static,
) -> Div {
    let active = signal(None::<u64>);
    let observe_disabled = disabled.clone();
    effect(move |g| {
        if observe_disabled.read(g).unwrap_or(false) {
            if let Some(raw) = g.get(active).flatten() {
                OverlayHandle::from_raw(raw).close();
            }
        }
    });
    let content = Arc::new(content);
    trigger
        .id(&id)
        .attr("tabindex", "0")
        .attr("role", "button")
        .attr("aria-haspopup", "dialog")
        .attr_when("disabled", "", disabled.clone())
        .attr_when(
            "data-state",
            "open",
            computed(move |g| g.get(active).flatten().is_some()),
        )
        .on_click(move |event| {
            if disabled.now().unwrap_or(false) {
                return;
            }
            if let Some(raw) = active.get() {
                OverlayHandle::from_raw(raw).close();
                return;
            }
            let panel_id = format!("{id}-panel");
            let outside_key = format!("{id}-outside");
            let cleanup = outside_key.clone();
            let content_id = panel_id.clone();
            let content = content.clone();
            let (x, y) = if over_control {
                let (width, height) =
                    blinc_core::context_state::BlincContextState::get().viewport_size();
                (
                    (event.bounds_x + event.bounds_width / 2.0 - size.0 / 2.0)
                        .clamp(8.0, (width - size.0 - 8.0).max(8.0)),
                    (event.bounds_y + event.bounds_height / 2.0 - size.1 / 2.0)
                        .clamp(8.0, (height - size.1 - 8.0).max(8.0)),
                )
            } else {
                (event.bounds_x, event.bounds_y + event.bounds_height + 4.0)
            };
            let handle = OverlayBuilder::popover()
                .at(x, y)
                .size(size.0, size.1)
                .anchor_direction(AnchorDirection::Bottom)
                .on_close(move |_| {
                    active.set(None);
                    blinc_layout::click_outside::unregister_click_outside(&cleanup);
                })
                .content(move || {
                    content(active)
                        .class("cn-popover-content")
                        .attr("role", "dialog")
                        .id(&content_id)
                })
                .show();
            active.set(Some(handle.raw()));
            blinc_layout::click_outside::register_click_outside_multi(
                &outside_key,
                &[id.clone(), panel_id],
                move || {
                    handle.close();
                },
            );
        })
        .on_unmount(move |_| {
            if let Some(raw) = active.get() {
                OverlayHandle::from_raw(raw).close();
            }
        })
}
