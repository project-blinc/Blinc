//! Browser-native video playback via `<video>` element.
//!
//! Frame pixels are captured each tick via a hidden `<canvas>` + `getImageData`.

use crate::frame::Frame;
mod bridge;
pub use bridge::set_web_media_bridge;
use std::cell::RefCell;
use std::collections::HashMap;
use wasm_bindgen::JsValue;

use wasm_bindgen::JsCast;
use web_sys::{
    Blob, BlobPropertyBag, CanvasRenderingContext2d, HtmlCanvasElement, HtmlVideoElement, Url,
};

struct WebVideoState {
    video: HtmlVideoElement,
    canvas: HtmlCanvasElement,
    ctx2d: CanvasRenderingContext2d,
    width: u32,
    height: u32,
    ready: bool,
}

thread_local! {
    static WEB_VIDEOS: RefCell<HashMap<u64, WebVideoState>> = RefCell::new(HashMap::new());
}

fn document() -> web_sys::Document {
    web_sys::window().unwrap().document().unwrap()
}

pub(crate) fn create(player_id: u64) {
    if let Some(_value) = bridge::call(player_id, "create", JsValue::UNDEFINED) {
        return;
    }

    let video: HtmlVideoElement = document()
        .create_element("video")
        .unwrap()
        .dyn_into()
        .unwrap();
    video.set_attribute("playsinline", "").ok();
    video.set_attribute("preload", "auto").ok();
    video.style().set_property("display", "none").ok();
    document().body().unwrap().append_child(&video).ok();

    let canvas: HtmlCanvasElement = document()
        .create_element("canvas")
        .unwrap()
        .dyn_into()
        .unwrap();
    canvas.style().set_property("display", "none").ok();
    document().body().unwrap().append_child(&canvas).ok();

    // `willReadFrequently: true` hints the browser to back the 2D
    // canvas with a software buffer instead of the default
    // GPU-resident one. The video player calls `getImageData` on every
    // tick to pull decoded frames into CPU memory for `blinc_media`
    // consumers — on a GPU-backed canvas each `getImageData` stalls
    // the GPU while it copies pixels back, and Chrome flags it with
    // the console warning "Canvas2D: Multiple readback operations".
    // With the hint set, reads are effectively free but GPU draws
    // to this canvas (we don't do any) would slow down.
    let ctx_options = js_sys::Object::new();
    js_sys::Reflect::set(
        &ctx_options,
        &wasm_bindgen::JsValue::from_str("willReadFrequently"),
        &wasm_bindgen::JsValue::from_bool(true),
    )
    .ok();
    let ctx2d: CanvasRenderingContext2d = canvas
        .get_context_with_context_options("2d", &ctx_options)
        .unwrap()
        .unwrap()
        .dyn_into()
        .unwrap();

    WEB_VIDEOS.with(|map| {
        map.borrow_mut().insert(
            player_id,
            WebVideoState {
                video,
                canvas,
                ctx2d,
                width: 0,
                height: 0,
                ready: false,
            },
        );
    });
}

/// Hand a URL directly to the `<video>` element without
/// materialising the bytes in memory first. The browser's media
/// pipeline handles HTTP range requests, progressive buffering,
/// seek, and media-source decoding — every property that makes
/// `<video>` worth using over a `<canvas>` + custom decoder.
///
/// Compare to [`load_bytes`], which fetches the entire file via
/// the preload cache, copies it into a JS `Uint8Array`, wraps in
/// a `Blob`, and passes the object URL — the video then has to
/// fully-buffer before playing, with a second memory copy held
/// alive for the blob URL's lifetime. Use that path only when
/// the caller already owns the bytes (e.g. a test harness or a
/// procedurally generated clip); URL-based loading is the right
/// default for shipped content.
pub(crate) fn load_url(player_id: u64, url: &str) {
    if let Some(_value) = bridge::call(player_id, "load-url", JsValue::from_str(url)) {
        return;
    }

    WEB_VIDEOS.with(|map| {
        let map = map.borrow();
        if let Some(state) = map.get(&player_id) {
            state.video.set_src(url);
            state.video.load();
        }
    });
}

pub(crate) fn load_bytes(player_id: u64, bytes: &[u8]) {
    if let Some(_value) = bridge::call(
        player_id,
        "load-bytes",
        js_sys::Uint8Array::from(bytes).into(),
    ) {
        return;
    }

    WEB_VIDEOS.with(|map| {
        let map = map.borrow();
        let Some(state) = map.get(&player_id) else {
            return;
        };

        let array = js_sys::Uint8Array::new_with_length(bytes.len() as u32);
        array.copy_from(bytes);
        let parts = js_sys::Array::new();
        parts.push(&array.buffer());

        let opts = BlobPropertyBag::new();
        opts.set_type("video/mp4");
        let blob = Blob::new_with_buffer_source_sequence_and_options(&parts, &opts).unwrap();
        let url = Url::create_object_url_with_blob(&blob).unwrap();

        state.video.set_src(&url);
        state.video.load();
    });
}

pub(crate) fn play(player_id: u64) {
    if let Some(_value) = bridge::call(player_id, "play", JsValue::UNDEFINED) {
        return;
    }

    WEB_VIDEOS.with(|map| {
        let map = map.borrow();
        if let Some(state) = map.get(&player_id) {
            let _ = state.video.play();
        }
    });
}

pub(crate) fn pause(player_id: u64) {
    if let Some(_value) = bridge::call(player_id, "pause", JsValue::UNDEFINED) {
        return;
    }

    WEB_VIDEOS.with(|map| {
        let map = map.borrow();
        if let Some(state) = map.get(&player_id) {
            state.video.pause().ok();
        }
    });
}

pub(crate) fn seek(player_id: u64, position_ms: u64) {
    if let Some(_value) = bridge::call(player_id, "seek", JsValue::from_f64(position_ms as f64)) {
        return;
    }

    WEB_VIDEOS.with(|map| {
        let map = map.borrow();
        if let Some(state) = map.get(&player_id) {
            state.video.set_current_time(position_ms as f64 / 1000.0);
        }
    });
}

pub(crate) fn set_volume(player_id: u64, volume: f32) {
    if let Some(_value) = bridge::call(player_id, "volume", JsValue::from_f64(volume as f64)) {
        return;
    }

    WEB_VIDEOS.with(|map| {
        let map = map.borrow();
        if let Some(state) = map.get(&player_id) {
            state.video.set_volume(volume as f64);
        }
    });
}

pub(crate) fn duration_ms(player_id: u64) -> u64 {
    if let Some(value) = bridge::call(player_id, "duration", JsValue::UNDEFINED) {
        return value.as_f64().unwrap_or(0.0) as u64;
    }

    WEB_VIDEOS.with(|map| {
        let map = map.borrow();
        map.get(&player_id)
            .map(|s| (s.video.duration() * 1000.0) as u64)
            .unwrap_or(0)
    })
}

pub(crate) fn position_ms(player_id: u64) -> u64 {
    if let Some(value) = bridge::call(player_id, "position", JsValue::UNDEFINED) {
        return value.as_f64().unwrap_or(0.0) as u64;
    }

    WEB_VIDEOS.with(|map| {
        let map = map.borrow();
        map.get(&player_id)
            .map(|s| (s.video.current_time() * 1000.0) as u64)
            .unwrap_or(0)
    })
}

/// Report the tip of the progressive-download buffer in milliseconds.
///
/// `<video>.buffered` is a `TimeRanges` list — one range per contiguous
/// chunk the browser has downloaded. Sites that seek around pile up
/// multiple ranges. We return the end of the range that covers the
/// current playback position (what the seek bar cares about: "how far
/// can I scrub forward without waiting?"). If no range covers the
/// current position we fall back to the last range's end, so the UI
/// still shows the most-recently-downloaded chunk rather than zero.
pub(crate) fn buffered_end_ms(player_id: u64) -> u64 {
    if let Some(value) = bridge::call(player_id, "buffered", JsValue::UNDEFINED) {
        return value.as_f64().unwrap_or(0.0) as u64;
    }

    WEB_VIDEOS.with(|map| {
        let map = map.borrow();
        let Some(state) = map.get(&player_id) else {
            return 0;
        };
        let ranges = state.video.buffered();
        let len = ranges.length();
        if len == 0 {
            return 0;
        }
        let current = state.video.current_time();
        for i in 0..len {
            let (Ok(start), Ok(end)) = (ranges.start(i), ranges.end(i)) else {
                continue;
            };
            if current >= start && current <= end {
                return (end * 1000.0) as u64;
            }
        }
        ranges
            .end(len - 1)
            .map(|e| (e * 1000.0) as u64)
            .unwrap_or(0)
    })
}

#[allow(dead_code)]
pub(crate) fn is_paused(player_id: u64) -> bool {
    if let Some(value) = bridge::call(player_id, "paused", JsValue::UNDEFINED) {
        return value.as_bool().unwrap_or(true);
    }

    WEB_VIDEOS.with(|map| {
        let map = map.borrow();
        map.get(&player_id)
            .map(|s| s.video.paused())
            .unwrap_or(true)
    })
}

pub(crate) fn is_ended(player_id: u64) -> bool {
    if let Some(value) = bridge::call(player_id, "ended", JsValue::UNDEFINED) {
        return value.as_bool().unwrap_or(false);
    }

    WEB_VIDEOS.with(|map| {
        let map = map.borrow();
        map.get(&player_id)
            .map(|s| s.video.ended())
            .unwrap_or(false)
    })
}

/// Capture the current video frame as RGBA pixels.
/// Returns None if the video isn't ready or has no dimensions.
pub(crate) fn capture_frame(player_id: u64) -> Option<Frame> {
    if let Some(value) = bridge::call(player_id, "frame", JsValue::UNDEFINED) {
        return bridge::frame(value);
    }

    WEB_VIDEOS.with(|map| {
        let mut map = map.borrow_mut();
        let state = map.get_mut(&player_id)?;

        let vw = state.video.video_width();
        let vh = state.video.video_height();
        if vw == 0 || vh == 0 {
            return None;
        }

        if !state.ready || state.width != vw || state.height != vh {
            state.canvas.set_width(vw);
            state.canvas.set_height(vh);
            state.width = vw;
            state.height = vh;
            state.ready = true;
        }

        state
            .ctx2d
            .draw_image_with_html_video_element(&state.video, 0.0, 0.0)
            .ok()?;

        let image_data = state
            .ctx2d
            .get_image_data(0.0, 0.0, vw as f64, vh as f64)
            .ok()?;

        let rgba = image_data.data().0;
        Some(Frame::from_rgba(rgba, vw, vh))
    })
}
