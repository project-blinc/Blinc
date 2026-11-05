//! Optional host transport for DOM media when rendering in an OffscreenCanvas worker.
use crate::Frame;
use std::cell::RefCell;
use wasm_bindgen::{JsCast, JsValue};
thread_local! { static BRIDGE: RefCell<Option<js_sys::Function>> = const { RefCell::new(None) }; }

/// Install the browser host's synchronous media adapter in a dedicated worker.
///
/// Commands (`create`, `load-url`, `load-bytes`, `play`, `pause`, `seek`, `volume`)
/// are forwarded to the DOM host. Reads (`position`, `duration`, `buffered`,
/// `paused`, `ended`, `frame`) return its most recent snapshot. Times are in ms;
/// `frame` is `{ width, height, pixels: ArrayBuffer }` or null. Commands may be
/// asynchronous; the host reports actual playback state, including rejections.
pub fn set_web_media_bridge(callback: js_sys::Function) {
    BRIDGE.with(|slot| *slot.borrow_mut() = Some(callback));
}
pub(super) fn call(id: u64, operation: &str, value: JsValue) -> Option<JsValue> {
    BRIDGE.with(|slot| {
        slot.borrow().as_ref().map(|f| {
            f.call3(
                &JsValue::NULL,
                &JsValue::from_f64(id as f64),
                &JsValue::from_str(operation),
                &value,
            )
            .unwrap_or(JsValue::NULL)
        })
    })
}
pub(super) fn frame(value: JsValue) -> Option<Frame> {
    let get = |name| js_sys::Reflect::get(&value, &JsValue::from_str(name)).ok();
    let width = get("width")?.as_f64()? as u32;
    let height = get("height")?.as_f64()? as u32;
    let buffer = get("pixels")?;
    if !buffer.is_instance_of::<js_sys::ArrayBuffer>() {
        return None;
    }
    let pixels = js_sys::Uint8Array::new(&buffer);
    let expected = width.checked_mul(height)?.checked_mul(4)?;
    if width == 0 || height == 0 || pixels.length() != expected {
        return None;
    }
    Some(Frame::from_rgba(pixels.to_vec(), width, height))
}
