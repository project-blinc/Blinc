use super::IOSRenderContext;
use std::ffi::{CStr, CString, c_char};

/// Copy the focused native editing session. Free with `blinc_ime_free`.
///
/// # Safety
/// Access a live context on the main thread only.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blinc_ime_snapshot(ctx: *const IOSRenderContext) -> *mut c_char {
    let Some(ctx) = (unsafe { ctx.as_ref() }) else {
        return std::ptr::null_mut();
    };
    let value = ctx
        .render_tree
        .as_ref()
        .map(crate::ime::snapshot)
        .unwrap_or_default();
    CString::new(value.to_string()).unwrap().into_raw()
}

/// Release an editing snapshot.
///
/// # Safety
/// The pointer must be null or an unreleased snapshot returned above.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blinc_ime_free(snapshot: *mut c_char) {
    if !snapshot.is_null() {
        drop(unsafe { CString::from_raw(snapshot) });
    }
}

/// Apply text, marked range and selection from UIKit.
///
/// # Safety
/// Access a live context on the main thread; `json` must be a UTF-8 C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blinc_ime_edit(ctx: *mut IOSRenderContext, json: *const c_char) -> bool {
    let Some(ctx) = (unsafe { ctx.as_mut() }) else {
        return false;
    };
    crate::mobile_paint::request_full_paint();
    if json.is_null() {
        return false;
    }
    let Ok(text) = (unsafe { CStr::from_ptr(json) }).to_str() else {
        return false;
    };
    let Ok(edit) = serde_json::from_str::<crate::ime::NativeImeEdit>(text) else {
        return false;
    };
    let accepted = ctx
        .render_tree
        .as_ref()
        .is_some_and(|tree| crate::ime::apply(tree, &edit));
    if accepted && edit.submit {
        ctx.handle_key_down(13);
    }
    accepted
}
