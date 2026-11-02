//! Waveform visualization via canvas

use std::sync::Arc;

use blinc_core::Color;

use crate::canvas::canvas;
use crate::div::{Div, div};

/// Waveform visualization builder
pub struct Waveform {
    /// Pre-computed amplitude buckets (0.0 to 1.0)
    buckets: Arc<Vec<f32>>,
    /// Progress ratio (0.0 to 1.0) — portion played
    progress: f32,
    /// Over `--blinc-waveform-played`, when set
    played_color: Option<Color>,
    /// Over `--blinc-waveform-unplayed`, when set
    unplayed_color: Option<Color>,
    inner: Div,
}

/// Create a waveform visualization from pre-computed amplitude buckets
pub fn waveform(buckets: Arc<Vec<f32>>) -> Waveform {
    Waveform {
        buckets,
        progress: 0.0,
        played_color: None,
        unplayed_color: None,
        inner: div().class("blinc-waveform"),
    }
}

impl Waveform {
    /// Set the played progress (0.0 to 1.0)
    pub fn progress(mut self, ratio: f32) -> Self {
        self.progress = ratio.clamp(0.0, 1.0);
        self
    }
    pub fn played_color(mut self, color: Color) -> Self {
        self.played_color = Some(color);
        self
    }
    pub fn unplayed_color(mut self, color: Color) -> Self {
        self.unplayed_color = Some(color);
        self
    }
    pub fn w(mut self, v: f32) -> Self {
        self.inner = self.inner.w(v);
        self
    }
    pub fn h(mut self, v: f32) -> Self {
        self.inner = self.inner.h(v);
        self
    }
    pub fn w_full(mut self) -> Self {
        self.inner = self.inner.w_full();
        self
    }
    pub fn class(mut self, class: &str) -> Self {
        self.inner = self.inner.class(class);
        self
    }

    pub fn into_div(self) -> Div {
        let buckets = self.buckets;
        let progress = self.progress;
        let played_color = self.played_color;
        let unplayed_color = self.unplayed_color;

        let wave_canvas = canvas(
            move |ctx: &mut dyn blinc_core::DrawContext, bounds: crate::canvas::CanvasBounds| {
                let count = buckets.len();
                if count == 0 {
                    return;
                }

                let bar_total_w = bounds.width / count as f32;
                let bar_w = (bar_total_w * 0.7).max(1.0);
                let gap = bar_total_w - bar_w;
                let center_y = bounds.height / 2.0;
                let played_count = (progress * count as f32) as usize;
                // Resolved each paint, so a theme or stylesheet change shows.
                let played = played_color.unwrap_or_else(|| {
                    crate::ua_styles::stylesheet_color(
                        "var(--blinc-waveform-played, var(--primary))",
                        blinc_theme::ColorToken::Primary,
                    )
                });
                let unplayed = unplayed_color.unwrap_or_else(|| {
                    crate::ua_styles::stylesheet_color(
                        "var(--blinc-waveform-unplayed, var(--border-secondary))",
                        blinc_theme::ColorToken::BorderSecondary,
                    )
                });

                for (i, &amplitude) in buckets.iter().enumerate() {
                    let x = i as f32 * bar_total_w + gap / 2.0;
                    let bar_h = (amplitude * bounds.height * 0.9).max(2.0);
                    let y = center_y - bar_h / 2.0;

                    let color = if i < played_count { played } else { unplayed };

                    ctx.fill_rect(
                        blinc_core::Rect::new(x, y, bar_w, bar_h),
                        blinc_core::CornerRadius::uniform(1.0),
                        blinc_core::Brush::Solid(color),
                    );
                }
            },
        )
        .w_full()
        .h_full();

        self.inner.child(wave_canvas)
    }
}
