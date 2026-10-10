//! A per-frame record of what the frame loop decided and how far each
//! animation got, for finding where an animation stops advancing.
//!
//! `BLINC_MOTION_TRACE=<file>` appends one JSON object per frame to the
//! file: the clock step the animations took, whether the frame was drawn and
//! by which path, the terms that asked for another frame, and the progress of
//! every CSS animation, transition and motion still running. Unset, every
//! call returns at once.

use std::cell::RefCell;
use std::io::Write;
use std::sync::{Mutex, OnceLock};

use blinc_layout::render_state::RenderState;
use blinc_layout::renderer::RenderTree;
use serde_json::{Map, Value, json};

static WRITER: OnceLock<Option<Mutex<std::io::BufWriter<std::fs::File>>>> = OnceLock::new();

thread_local! {
    static FRAME: RefCell<Option<Map<String, Value>>> = const { RefCell::new(None) };
    static COUNT: RefCell<u64> = const { RefCell::new(0) };
}

fn writer() -> Option<&'static Mutex<std::io::BufWriter<std::fs::File>>> {
    WRITER
        .get_or_init(|| {
            let path = std::env::var_os("BLINC_MOTION_TRACE")?;
            let file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .map_err(|e| tracing::warn!("BLINC_MOTION_TRACE: {e}"))
                .ok()?;
            tracing::info!("writing a motion trace to {}", path.to_string_lossy());
            Some(Mutex::new(std::io::BufWriter::new(file)))
        })
        .as_ref()
}

/// Whether a trace is being written.
pub(crate) fn enabled() -> bool {
    writer().is_some()
}

/// Start the record of a frame at `now_ms`.
pub(crate) fn begin(now_ms: u64) {
    if !enabled() {
        return;
    }
    let frame = COUNT.with(|c| {
        *c.borrow_mut() += 1;
        *c.borrow()
    });
    FRAME.with(|f| {
        let mut map = Map::new();
        map.insert("frame".into(), json!(frame));
        map.insert("t".into(), json!(now_ms));
        *f.borrow_mut() = Some(map);
    });
}

/// Add `value` to the current frame's record under `key`.
pub(crate) fn note(key: &str, value: impl Into<Value>) {
    if !enabled() {
        return;
    }
    let value = value.into();
    FRAME.with(|f| {
        if let Some(map) = f.borrow_mut().as_mut() {
            map.insert(key.to_string(), value);
        }
    });
}

/// Add every running CSS animation, transition and motion to the record.
pub(crate) fn note_animations(tree: &RenderTree, rs: &RenderState) {
    if !enabled() {
        return;
    }
    let css: Vec<Value> = tree
        .css_samples()
        .into_iter()
        .map(|(stable, kind, progress, opacity, painted)| {
            json!({ "id": stable, "kind": kind, "p": progress, "opacity": opacity, "painted": painted })
        })
        .collect();
    let motions: Vec<Value> = rs
        .motion_samples()
        .into_iter()
        .map(|m| {
            json!({
                "key": m.key, "state": m.state, "p": m.progress, "opacity": m.opacity,
                "tx": m.translate.0, "ty": m.translate.1, "sx": m.scale.0, "sy": m.scale.1,
            })
        })
        .collect();
    note("css", css);
    note("motions", motions);
}

/// Write the current frame's record.
pub(crate) fn end() {
    let Some(writer) = writer() else {
        return;
    };
    let Some(map) = FRAME.with(|f| f.borrow_mut().take()) else {
        return;
    };
    if let Ok(mut w) = writer.lock() {
        let _ = serde_json::to_writer(&mut *w, &Value::Object(map));
        let _ = w.write_all(b"\n");
        let _ = w.flush();
    }
}
