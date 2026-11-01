//! Timeline Panel - Event timeline with scrubber

use std::cell::OnceCell;

use blinc_cn::components::button::{ButtonSize, ButtonVariant, button};
use blinc_cn::components::select::{SelectSize, select};
use blinc_cn::components::separator::separator;
use blinc_cn::components::slider::{SliderSize, slider};
use blinc_core::Color;
use blinc_core::context_state::BlincContextState;
use blinc_icons::icons;
use blinc_layout::div::{Div, ElementBuilder};
use blinc_layout::element::RenderProps;
use blinc_layout::event_handler::EventHandlers;
use blinc_layout::prelude::*;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_recorder::replay::ReplayState;
use blinc_recorder::{RecordedEvent, Timestamp, TimestampedEvent};
use blinc_theme::{ColorToken, ThemeState};

use crate::theme::DebuggerTokens;

#[derive(Clone)]
pub struct TimelinePanelState {
    pub position: Timestamp,
    pub duration: Timestamp,
    pub playback_state: ReplayState,
    pub speed: f64,
}

impl Default for TimelinePanelState {
    fn default() -> Self {
        Self {
            position: Timestamp::zero(),
            duration: Timestamp::zero(),
            playback_state: ReplayState::Idle,
            speed: 1.0,
        }
    }
}

struct TimelinePanelConfig {
    position: Timestamp,
    duration: Timestamp,
    playback_state: ReplayState,
    speed: f64,
}

struct BuiltTimelinePanel {
    inner: Div,
}

impl BuiltTimelinePanel {
    fn from_config(config: &TimelinePanelConfig) -> Self {
        let theme = ThemeState::get();

        let inner = div()
            .w_full()
            .py(2.0)
            .h(DebuggerTokens::TIMELINE_HEIGHT)
            .bg(theme.color(ColorToken::SurfaceElevated))
            .flex_col()
            .child(separator())
            .child(Self::controls(config))
            .child(
                div()
                    .w_full()
                    .flex_row()
                    .items_center()
                    .child(Self::timeline_track(config)),
            );

        BuiltTimelinePanel { inner }
    }

    fn controls(config: &TimelinePanelConfig) -> Div {
        let theme = ThemeState::get();
        let is_playing = config.playback_state == ReplayState::Playing;

        // Get speed state from context
        let speed_str = format!("{:.1}", config.speed);
        let speed_state =
            BlincContextState::get().use_state_keyed("timeline_speed", || speed_str.clone());

        div()
            .w_full()
            .h(44.0)
            .px(12.0)
            .py(8.0)
            .flex_row()
            .items_center()
            .justify_between()
            .child(
                // Playback controls
                div()
                    .flex_row()
                    .items_center()
                    .gap(2.0)
                    .child(
                        button("")
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Icon)
                            .icon(icons::SKIP_BACK),
                    )
                    .child(
                        button("")
                            .variant(ButtonVariant::Primary)
                            .size(ButtonSize::Icon)
                            .icon(if is_playing {
                                icons::PAUSE
                            } else {
                                icons::PLAY
                            }),
                    )
                    .child(
                        button("")
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Icon)
                            .icon(icons::SKIP_FORWARD),
                    ),
            )
            .child(
                // Time display
                div()
                    .flex_row()
                    .items_center()
                    .gap(4.0)
                    .child(
                        text(Self::format_time(config.position))
                            .size(12.0)
                            .color(theme.color(ColorToken::TextPrimary)),
                    )
                    .child(
                        text("/")
                            .size(12.0)
                            .color(theme.color(ColorToken::TextTertiary)),
                    )
                    .child(
                        text(Self::format_time(config.duration))
                            .size(12.0)
                            .color(theme.color(ColorToken::TextSecondary)),
                    ),
            )
            .child(
                // Speed selector
                select(speed_state.signal())
                    .size(SelectSize::Small)
                    .w(80.0)
                    .option("0.5", "0.5x")
                    .option("1.0", "1.0x")
                    .option("2.0", "2.0x"),
            )
    }

    fn timeline_track(config: &TimelinePanelConfig) -> Div {
        // Get position state from context (normalized 0.0-1.0)
        let position_norm = if config.duration.as_micros() > 0 {
            config.position.as_micros() as f32 / config.duration.as_micros() as f32
        } else {
            0.0
        };
        let position_state =
            BlincContextState::get().use_state_keyed("timeline_position", || position_norm);

        div()
            .w_full()
            .padding_x_px(24.0)
            .py(4.0)
            .flex_col()
            .bg_background()
            .gap_px(8.0)
            .items_center()
            .justify_center()
            .child(Self::event_markers())
            .child(
                slider(position_state.signal())
                    .min(0.0)
                    .max(1.0)
                    .size(SliderSize::Small)
                    .w(740.0),
            )
            .child(Self::time_labels(config.duration))
    }

    fn event_markers() -> Div {
        let theme = ThemeState::get();

        let colors = [
            theme.color(ColorToken::Primary),
            theme.color(ColorToken::Info),
            theme.color(ColorToken::Secondary),
            theme.color(ColorToken::Accent),
            theme.color(ColorToken::Warning),
        ];

        // Use flexbox to distribute markers evenly - match slider width
        let mut track = div()
            .id("timeline_track_id")
            .w(740.0) // Match slider width
            .h(16.0)
            .flex_row()
            .items_center()
            .justify_between()
            .px(4.0); // Small padding at edges

        for i in 0..10 {
            track = track.child(div().w(3.0).h(12.0).rounded(1.5).bg(colors[i % 5]));
        }
        track
    }

    fn time_labels(duration: Timestamp) -> Div {
        let theme = ThemeState::get();
        div()
            .w(740.0) // Match slider width
            .h(14.0)
            .flex_row()
            .justify_between()
            .child(
                text("0:00")
                    .size(10.0)
                    .color(theme.color(ColorToken::TextTertiary)),
            )
            .child(
                text(Self::format_time(duration))
                    .size(10.0)
                    .color(theme.color(ColorToken::TextTertiary)),
            )
    }

    fn format_time(ts: Timestamp) -> String {
        let total_secs = ts.as_micros() / 1_000_000;
        format!("{}:{:02}", total_secs / 60, total_secs % 60)
    }
}

pub struct TimelinePanel {
    config: TimelinePanelConfig,
    built: OnceCell<BuiltTimelinePanel>,
}

impl TimelinePanel {
    pub fn new(_events: &[TimestampedEvent], state: &TimelinePanelState) -> Self {
        Self {
            config: TimelinePanelConfig {
                position: state.position,
                duration: state.duration,
                playback_state: state.playback_state,
                speed: state.speed,
            },
            built: OnceCell::new(),
        }
    }

    fn get_or_build(&self) -> &BuiltTimelinePanel {
        self.built
            .get_or_init(|| BuiltTimelinePanel::from_config(&self.config))
    }

    pub fn build(self) -> Div {
        BuiltTimelinePanel::from_config(&self.config).inner
    }
}

impl ElementBuilder for TimelinePanel {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.get_or_build().inner.build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.get_or_build().inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().inner.children_builders()
    }

    fn event_handlers(&self) -> Option<&EventHandlers> {
        let handlers = self.get_or_build().inner.event_handlers();
        if handlers.is_empty() {
            None
        } else {
            Some(handlers)
        }
    }
}

#[allow(dead_code)]
fn event_color(event: &RecordedEvent) -> Color {
    let theme = ThemeState::get();
    match event {
        RecordedEvent::Click(_)
        | RecordedEvent::DoubleClick(_)
        | RecordedEvent::MouseDown(_)
        | RecordedEvent::MouseUp(_)
        | RecordedEvent::MouseMove(_) => theme.color(ColorToken::Primary),
        RecordedEvent::KeyDown(_) | RecordedEvent::KeyUp(_) | RecordedEvent::TextInput(_) => {
            theme.color(ColorToken::Info)
        }
        RecordedEvent::Scroll(_) => theme.color(ColorToken::Secondary),
        RecordedEvent::FocusChange(_) => theme.color(ColorToken::Accent),
        RecordedEvent::HoverEnter(_) | RecordedEvent::HoverLeave(_) => {
            theme.color(ColorToken::Warning)
        }
        _ => theme.color(ColorToken::TextTertiary),
    }
}
