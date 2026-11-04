use super::overlay::OverlayKind;
use crate::div::Div;

pub(super) fn scope(div: Div, key: String, kind: OverlayKind) -> Div {
    if kind == OverlayKind::Toast {
        return div;
    }
    let div = div.attr("data-focus-scope", key);
    let div = if matches!(kind, OverlayKind::Modal | OverlayKind::Dialog) {
        div.attr("data-focus-modal", "true").attr("tabindex", "-1")
    } else {
        div
    };
    if kind == OverlayKind::Tooltip {
        div.attr("data-focus-passive", "true")
    } else {
        div
    }
}

pub(crate) fn is_active(key: &str) -> bool {
    if let Some(id) = key.strip_prefix("stack-").and_then(|id| id.parse().ok()) {
        return super::overlay_stack::OverlayHandle::from_raw(id).is_live();
    }
    if let Some(id) = key.strip_prefix("legacy-").and_then(|id| id.parse().ok()) {
        return crate::overlay_state::OverlayContext::try_get().is_some_and(|ctx| {
            let manager = ctx.overlay_manager();
            let inner = manager.lock().unwrap();
            inner.overlays_sorted().iter().any(|overlay| {
                overlay.handle.id() == id
                    && overlay.state != super::overlay::OverlayState::Closing
                    && overlay.is_visible()
            })
        });
    }
    true
}
