//! Adaptive animation frame-rate cap: the ladder and the adapter that steps it.

/// Pick an initial animation-FPS cap for `AnimationFps::Adaptive`
/// based on coarse system characteristics.
///
/// Currently looks at logical-core count via
/// `std::thread::available_parallelism()`. Future revisions can add
/// GPU class (via wgpu adapter info), total RAM, and battery /
/// power-mode signals.
///
/// Thresholds are conservative: the dynamic adaptation step (when it
/// lands) will raise the cap if frames are coming in well under
/// budget, so we err on the side of starting lower and letting the
/// adapter climb. The animation_fps_cap field clamping further
/// enforces sane bounds.
///
/// Returns:
/// * `<= 4` cores  → 30 fps  (typical low-end laptops, older
///   Chromebooks, single-board computers)
/// * `<= 8` cores  → 60 fps  (mainstream desktops, recent laptops)
/// * `>  8` cores  → 60 fps  (high-end; could be 120 but conservative)
///
/// Apps that want vsync regardless can set
/// `WindowConfig::animation_fps = AnimationFps::Refresh`.
pub(crate) fn detect_initial_fps_cap() -> u32 {
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2);
    if cores <= 4 { 30 } else { 60 }
}

/// Allowed cap values for dynamic adaptation, ascending. The adapter
/// walks one step at a time.
pub(super) const FPS_LADDER: &[u32] = &[15, 20, 30, 45, 60, 90, 120];
/// Multiple of the current per-frame budget (1000/cap ms) above
/// which the window's median frame time counts as overshooting.
pub(super) const FPS_OVERSHOOT_MULT: f32 = 1.2;
/// Multiple of the budget below which the median counts as headroom.
pub(super) const FPS_HEADROOM_MULT: f32 = 0.45;
/// Consecutive evaluation windows of consistent signal needed before
/// the cap moves (hysteresis to dampen transient spikes).
pub(super) const FPS_WINDOWS_TO_DROP: u32 = 2;
pub(super) const FPS_WINDOWS_TO_RAISE: u32 = 6;
/// Frames per evaluation window (≈ 1 second at 60 fps).
pub(super) const FPS_WINDOW_FRAMES: usize = 60;

/// Adapter that adjusts the animation FPS cap based on observed
/// frame timings. Drops when the window-median frame time
/// consistently overshoots the budget; raises when frames have
/// substantial headroom. Only consulted under
/// [`AnimationFps::Adaptive`]; `Fixed` / `Refresh` skip it.
pub(crate) struct FpsAdapter {
    pub(super) current_cap: u32,
    /// Upper bound the cap may raise to — set to the display refresh
    /// rate. Raising above the panel refresh wastes renders and, when
    /// the cap isn't an integer multiple of vsync (90 on a 60 Hz
    /// panel), beats against the compositor's presentation cadence →
    /// visible judder in continuous animations (spinner, scroll decel).
    pub(super) ceiling: u32,
    pub(super) window: Vec<u64>,
    pub(super) consecutive_overshoot_windows: u32,
    pub(super) consecutive_headroom_windows: u32,
}

impl FpsAdapter {
    pub(crate) fn new(initial_cap: u32) -> Self {
        Self {
            current_cap: clamp_to_ladder(initial_cap),
            // Until the real refresh rate is known, allow the full
            // ladder; `set_ceiling` narrows it at window resume.
            ceiling: *FPS_LADDER.last().unwrap(),
            window: Vec::with_capacity(FPS_WINDOW_FRAMES),
            consecutive_overshoot_windows: 0,
            consecutive_headroom_windows: 0,
        }
    }

    pub(crate) fn current_cap(&self) -> u32 {
        self.current_cap
    }

    /// Clamp the adaptive cap to the display refresh rate. The adapter
    /// never raises past this, and an already-raised cap is pulled back
    /// down immediately. Called at window resume once the panel's
    /// refresh is known.
    pub(crate) fn set_ceiling(&mut self, ceiling: u32) {
        self.ceiling = ceiling.max(FPS_LADDER[0]);
        if self.current_cap > self.ceiling {
            self.current_cap = self.ceiling;
        }
    }

    /// Record a frame's total wall-clock time. Returns the cap to
    /// use going forward (only changes after a hysteresis tally
    /// crosses its threshold).
    pub(crate) fn record(&mut self, total_us: u64) -> u32 {
        self.window.push(total_us);
        if self.window.len() >= FPS_WINDOW_FRAMES {
            self.evaluate();
            self.window.clear();
        }
        self.current_cap
    }

    fn evaluate(&mut self) {
        let median = window_median(&mut self.window);
        let budget_us = (1_000_000.0 / self.current_cap as f32) as u64;
        let drop_thresh = (budget_us as f32 * FPS_OVERSHOOT_MULT) as u64;
        let raise_thresh = (budget_us as f32 * FPS_HEADROOM_MULT) as u64;
        if median > drop_thresh {
            self.consecutive_overshoot_windows =
                self.consecutive_overshoot_windows.saturating_add(1);
            self.consecutive_headroom_windows = 0;
            if self.consecutive_overshoot_windows >= FPS_WINDOWS_TO_DROP {
                let new_cap = ladder_step_down(self.current_cap);
                if new_cap != self.current_cap {
                    tracing::info!(
                        "fps_adapter: dropping cap {} -> {} (median={}us budget={}us)",
                        self.current_cap,
                        new_cap,
                        median,
                        budget_us
                    );
                    self.current_cap = new_cap;
                    self.consecutive_overshoot_windows = 0;
                }
            }
        } else if median < raise_thresh {
            self.consecutive_headroom_windows = self.consecutive_headroom_windows.saturating_add(1);
            self.consecutive_overshoot_windows = 0;
            if self.consecutive_headroom_windows >= FPS_WINDOWS_TO_RAISE {
                let new_cap = ladder_step_up(self.current_cap).min(self.ceiling);
                if new_cap != self.current_cap {
                    tracing::info!(
                        "fps_adapter: raising cap {} -> {} (median={}us budget={}us)",
                        self.current_cap,
                        new_cap,
                        median,
                        budget_us
                    );
                    self.current_cap = new_cap;
                    self.consecutive_headroom_windows = 0;
                }
            }
        } else {
            self.consecutive_overshoot_windows = 0;
            self.consecutive_headroom_windows = 0;
        }
    }
}

pub(super) fn clamp_to_ladder(target: u32) -> u32 {
    let mut best = FPS_LADDER[0];
    let mut best_diff = (FPS_LADDER[0] as i64 - target as i64).abs();
    for &rung in FPS_LADDER.iter().skip(1) {
        let diff = (rung as i64 - target as i64).abs();
        if diff < best_diff {
            best = rung;
            best_diff = diff;
        }
    }
    best
}

pub(super) fn ladder_step_down(cap: u32) -> u32 {
    let mut prev = cap;
    for &rung in FPS_LADDER {
        if rung >= cap {
            return prev;
        }
        prev = rung;
    }
    prev
}

pub(super) fn ladder_step_up(cap: u32) -> u32 {
    for &rung in FPS_LADDER {
        if rung > cap {
            return rung;
        }
    }
    cap
}

pub(super) fn window_median(window: &mut [u64]) -> u64 {
    if window.is_empty() {
        return 0;
    }
    window.sort_unstable();
    window[window.len() / 2]
}
