use std::ffi::{CStr, CString, c_char};

use super::IOSRenderContext;
use blinc_layout::accessibility::{AccessibilityAction, AccessibilityRole};
use blinc_layout::tree::StableNodeId;

/// Copy the current semantic snapshot. Release it with `blinc_accessibility_free`.
///
/// # Safety
/// `ctx` must be a live iOS context, accessed on the main thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blinc_accessibility_snapshot(ctx: *const IOSRenderContext) -> *mut c_char {
    let Some(ctx) = (unsafe { ctx.as_ref() }) else {
        return std::ptr::null_mut();
    };
    // Walking and serialising the whole semantic tree is the costliest step of an idle frame.
    // The snapshot is reused until something raises a paint request, and at most this long.
    const MAX_AGE_MS: u64 = 500;
    static CACHE: std::sync::Mutex<Option<(usize, u64, u64, CString)>> =
        std::sync::Mutex::new(None);
    let key = ctx as *const IOSRenderContext as usize;
    let generation = crate::mobile_paint::paint_generation();
    let now = blinc_layout::prelude::elapsed_ms();
    if let Ok(cache) = CACHE.lock()
        && let Some((k, g, at, json)) = cache.as_ref()
        && *k == key
        && *g == generation
        && now.saturating_sub(*at) < MAX_AGE_MS
    {
        return json.clone().into_raw();
    }
    let nodes = ctx
        .render_tree
        .as_ref()
        .map(|tree| tree.accessibility_nodes())
        .unwrap_or_default();
    let nodes: Vec<_> = nodes.into_iter().map(|n| {
        let role = match n.role {
            AccessibilityRole::Text => "text", AccessibilityRole::Heading => "heading",
            AccessibilityRole::Button => "button", AccessibilityRole::Link => "link",
            AccessibilityRole::TextField => "textfield", AccessibilityRole::Checkbox => "checkbox",
            AccessibilityRole::Switch => "switch", AccessibilityRole::Radio => "radio",
            AccessibilityRole::Slider => "slider", AccessibilityRole::Tab => "tab",
            AccessibilityRole::MenuItem => "menuitem", AccessibilityRole::Image => "image",
        };
        let paint = if n.role == AccessibilityRole::TextField {
            let tree = ctx.render_tree.as_ref().unwrap();
            let mut pending: Vec<_> = tree.layout_id(n.id).into_iter().collect();
            let mut color = None;
            while let Some(node) = pending.pop() {
                if let Some(render) = tree.get_render_node(node)
                    && let blinc_layout::renderer::ElementType::Text(text) = &render.element_type
                    && !text.content.is_empty() {
                    color = Some((text.color, text.font_size)); break;
                }
                pending.extend(tree.layout().children(node).into_iter().rev());
            }
            color
        } else { None };
        serde_json::json!({
            "id": n.id.to_raw().to_string(), "identifier": n.identifier, "role": role,
            "label": n.label, "value": n.value, "hint": n.hint, "placeholder": n.placeholder,
            "x": n.bounds.x, "y": n.bounds.y, "width": n.bounds.width, "height": n.bounds.height,
            "disabled": n.disabled, "selected": n.selected, "checked": n.checked,
            "expanded": n.expanded, "secure": n.secure, "focused": n.focused,
            "selection": n.selection, "foreground": paint.map(|p| p.0), "fontSize": paint.map(|p| p.1),
        })
    }).collect();
    let json = CString::new(serde_json::to_string(&nodes).unwrap()).unwrap();
    if let Ok(mut cache) = CACHE.lock() {
        *cache = Some((key, generation, now, json.clone()));
    }
    json.into_raw()
}

/// Release a semantic snapshot allocated by Blinc.
///
/// # Safety
/// `snapshot` must be null or an unreleased pointer returned by `blinc_accessibility_snapshot`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blinc_accessibility_free(snapshot: *mut c_char) {
    if !snapshot.is_null() {
        drop(unsafe { CString::from_raw(snapshot) });
    }
}

/// Dispatch an accessibility action: activate, focus, replace value, increment or decrement.
///
/// # Safety
/// Access `ctx` on the main thread. A non-null `value` must point to a UTF-8 C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blinc_accessibility_action(
    ctx: *mut IOSRenderContext,
    id: u64,
    action: u32,
    value: *const c_char,
) -> bool {
    let Some(ctx) = (unsafe { ctx.as_mut() }) else {
        return false;
    };
    crate::mobile_paint::request_full_paint();
    let Some(tree) = ctx.render_tree.as_mut() else {
        return false;
    };
    let action = match action {
        0 => AccessibilityAction::Activate,
        1 => AccessibilityAction::Focus,
        2 if !value.is_null() => {
            let Ok(value) = (unsafe { CStr::from_ptr(value) }).to_str() else {
                return false;
            };
            AccessibilityAction::SetValue(value.to_owned())
        }
        3 => AccessibilityAction::Increment,
        4 => AccessibilityAction::Decrement,
        5 if !value.is_null() => {
            let Ok(value) = (unsafe { CStr::from_ptr(value) }).to_str() else {
                return false;
            };
            let Ok(edit) = serde_json::from_str::<serde_json::Value>(value) else {
                return false;
            };
            let (Some(start), Some(end), Some(text)) = (
                edit["start"].as_u64(),
                edit["end"].as_u64(),
                edit["text"].as_str(),
            ) else {
                return false;
            };
            AccessibilityAction::ReplaceText {
                start: start as usize,
                end: end as usize,
                text: text.to_owned(),
            }
        }
        _ => return false,
    };
    let handled = tree.perform_accessibility_action(StableNodeId::from_raw(id), action);
    if handled {
        blinc_layout::request_redraw();
    }
    handled
}
