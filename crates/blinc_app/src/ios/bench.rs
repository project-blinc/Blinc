//! Measurement hooks for a phone, off unless `BLINC_BENCH_STATS` (frame statistics) or
//! `BLINC_BENCH_SCROLL` (a scripted drag and fling, fed as touches) is set in the environment.
//! Statistics are gathered in memory and written once per window of frames, so logging does not
//! disturb the frames it measures.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Frames per statistics line; `BLINC_BENCH_WINDOW` overrides it.
fn window() -> u32 {
    static N: OnceLock<u32> = OnceLock::new();
    *N.get_or_init(|| {
        std::env::var("BLINC_BENCH_WINDOW")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(120)
    })
}

unsafe extern "C" {
    fn mach_task_self() -> u32;
    fn task_info(task: u32, flavor: i32, info: *mut u32, count: *mut u32) -> i32;
}

/// The process's physical footprint in MB, which is what the system's memory limit counts.
fn footprint_mb() -> f64 {
    const TASK_VM_INFO: i32 = 22;
    let mut info = [0u64; 64];
    let mut count = 128u32;
    let status = unsafe {
        task_info(
            mach_task_self(),
            TASK_VM_INFO,
            info.as_mut_ptr() as *mut u32,
            &mut count,
        )
    };
    if status == 0 {
        info[18] as f64 / (1024.0 * 1024.0)
    } else {
        -1.0
    }
}

pub fn stats_on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("BLINC_BENCH_STATS").is_some())
}

pub fn scroll_on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("BLINC_BENCH_SCROLL").is_some())
}

#[derive(Default)]
struct Window {
    build: Vec<f64>,
    acquire: Vec<f64>,
    render: Vec<f64>,
    since: Option<Instant>,
}

static WINDOW_STATS: Mutex<Window> = Mutex::new(Window {
    build: Vec::new(),
    acquire: Vec::new(),
    render: Vec::new(),
    since: None,
});

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

pub fn note_build(d: Duration) {
    WINDOW_STATS.lock().unwrap().build.push(ms(d));
}

pub fn note_acquire(d: Duration) {
    WINDOW_STATS.lock().unwrap().acquire.push(ms(d));
}

pub fn note_render(d: Duration) {
    WINDOW_STATS.lock().unwrap().render.push(ms(d));
}

fn pick(values: &mut [f64], percentile: f64) -> f64 {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    if values.is_empty() {
        return 0.0;
    }
    values[((values.len() - 1) as f64 * percentile).round() as usize]
}

/// One frame has been presented.
pub fn frame_done() {
    let mut w = WINDOW_STATS.lock().unwrap();
    let now = Instant::now();
    let since = *w.since.get_or_insert(now);
    if (w.render.len() as u32) < window() {
        return;
    }
    let frames = w.render.len();
    let secs = (now - since).as_secs_f64();
    let line =
        |name: &str, v: &mut Vec<f64>| format!("{name} {:.2}/{:.2}", pick(v, 0.5), pick(v, 0.95));
    let (mut b, mut a, mut r) = (
        std::mem::take(&mut w.build),
        std::mem::take(&mut w.acquire),
        std::mem::take(&mut w.render),
    );
    eprintln!(
        "[bench] fps {:.1} frames {frames} mem {:.0} MB | ms median/p95: {} | {} | {}",
        frames as f64 / secs,
        footprint_mb(),
        line("build", &mut b),
        line("acquire", &mut a),
        line("render", &mut r),
    );
    w.since = Some(now);
}

/// The touch a script feeds on frame `n`, as `(x, y, phase)` for a screen `w` by `h` points:
/// a drag up, a lift at speed so the content glides, then the same downward, in a loop of 400
/// frames. One touch event per frame, as the display link delivers them.
pub fn scripted_touch(n: u64, w: f32, h: f32) -> Option<(f32, f32, i32)> {
    const STEP: f32 = 12.0;
    let f = (n % 400) as f32;
    let x = w * 0.5;
    match f as u32 {
        0 => Some((x, h * 0.75, 0)),
        1..=49 => Some((x, h * 0.75 - STEP * f, 1)),
        50 => Some((x, h * 0.75 - STEP * 50.0, 2)),
        200 => Some((x, h * 0.2, 0)),
        201..=249 => Some((x, h * 0.2 + STEP * (f - 200.0), 1)),
        250 => Some((x, h * 0.2 + STEP * 50.0, 2)),
        _ => None,
    }
}

pub static FRAME: AtomicU64 = AtomicU64::new(0);

pub fn next_frame() -> u64 {
    FRAME.fetch_add(1, Ordering::Relaxed)
}

/// Times its scope into the build statistics when `BLINC_BENCH_STATS` is set.
pub(super) struct BuildTimer(Option<std::time::Instant>);

impl BuildTimer {
    pub(super) fn start() -> Self {
        Self(stats_on().then(std::time::Instant::now))
    }
}

impl Drop for BuildTimer {
    fn drop(&mut self) {
        if let Some(start) = self.0 {
            note_build(start.elapsed());
        }
    }
}
