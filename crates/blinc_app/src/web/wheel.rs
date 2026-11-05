//! Dispatch queued wheel input before a pointer move can change its target.

use super::WebApp;
use blinc_layout::renderer::ScrollGesturePhase;

impl WebApp {
    pub(super) fn drain_wheel_input(&mut self, now: u64) {
        if self.wheel_input.is_empty() {
            return;
        }
        // Preserve trackpad-sized deltas; compress larger wheel impulses.
        let damp = |delta: f32| {
            let magnitude = delta.abs();
            if magnitude <= 40.0 {
                delta
            } else {
                delta.signum() * (40.0 + (magnitude - 40.0).powf(0.85))
            }
        };
        while let Some(delta) = self.wheel_input.pop() {
            if delta.starts_gesture {
                if self.last_wheel_time_ms.is_some() {
                    if let Some(tree) = self.current_tree.as_mut() {
                        tree.on_scroll_end();
                        tree.on_scroll_gesture(ScrollGesturePhase::MomentumEnd);
                    }
                }
                self.last_wheel_time_ms = None;
            }
            Self::dispatch_scroll(self, damp(delta.x), damp(delta.y));
        }
        // Cached layers must follow scroll positions without rebaking every tick.
        if self
            .last_scroll_invalidate_ms
            .is_none_or(|last| now.saturating_sub(last) > 100)
        {
            self.blinc_app
                .invalidate_render_cache_tagged("wheel_burst_start");
            self.blinc_app.drain_all_layer_textures("wheel_burst_start");
            self.last_scroll_invalidate_ms = Some(now);
        }
    }
}
